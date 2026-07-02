//! substrate-throttle — vendored mirror of sharecli/spawn-core (Zig 0.16).
//!
//! # Purpose
//!
//! `engine-forge::run_simple` (F5 path) is gated by `FORGE_DAEMON=1` and
//! currently uses `forge_daemon::DaemonDispatch::dispatch` which calls
//! `posix_spawn` directly. When M concurrent dispatches target cargo build
//! paths, the parallel processes thrash the same `target/` directory and
//! CPU caches, dropping throughput.
//!
//! `substrate-throttle` exposes a counting semaphore plus throttled spawn
//! (nice_delta + macOS background_qos) so that callers can cap concurrent
//! substrate-driven builds to N without round-tripping through sharecli's
//! IPC core.
//!
//! # Vendoring boundary
//!
//! Source: github.com/KooshaPari/sharecli (MIT) — crates/spawn-core/src/spawn_core.zig
//! Vendored at SHA a3e308e (#16). Upstream path semantics are intentionally
//! preserved; renames go into a separate commit if sharecli ever changes.
//!
//! # FFI contract
//!
//! Mirrors sharecli/spawn-core-sys verbatim:
//!   * All `spc_*` functions return `i32` (0 ok, negative errno, +1 special)
//!   * `SemaphoreHandle` is an opaque heap pointer (Zig c_allocator)
//!   * `SpawnParams` is `#[repr(C)]` extern struct
//!
//! This crate wraps both with safe RAII / builder types so callers don't
//! touch raw FFI.

const std = @import("std");
const builtin = @import("builtin");
const is_macos = builtin.os.tag == .macos;
const is_linux = builtin.os.tag == .linux;

// ---------------------------------------------------------------------------
// Semaphore — POSIX mutex + condvar counting semaphore
// ---------------------------------------------------------------------------

const Semaphore = struct {
    mutex: std.c.pthread_mutex_t,
    cond: std.c.pthread_cond_t,
    count: usize,
    max: usize,
};

export fn spc_semaphore_new(max: usize) callconv(.c) ?*anyopaque {
    const sem = std.heap.c_allocator.create(Semaphore) catch return null;
    sem.* = .{
        .mutex = std.c.PTHREAD_MUTEX_INITIALIZER,
        .cond = std.c.PTHREAD_COND_INITIALIZER,
        .count = max,
        .max = max,
    };
    return @ptrCast(sem);
}

export fn spc_semaphore_acquire(ptr: *anyopaque) callconv(.c) i32 {
    const sem: *Semaphore = @ptrCast(@alignCast(ptr));
    const rc = std.c.pthread_mutex_lock(&sem.mutex);
    if (rc != .SUCCESS) return -@as(i32, @intCast(@intFromEnum(rc)));
    while (sem.count == 0) {
        const wrc = std.c.pthread_cond_wait(&sem.cond, &sem.mutex);
        if (wrc != .SUCCESS) {
            _ = std.c.pthread_mutex_unlock(&sem.mutex);
            return -@as(i32, @intCast(@intFromEnum(wrc)));
        }
    }
    sem.count -= 1;
    _ = std.c.pthread_mutex_unlock(&sem.mutex);
    return 0;
}

export fn spc_semaphore_try_acquire(ptr: *anyopaque) callconv(.c) i32 {
    const sem: *Semaphore = @ptrCast(@alignCast(ptr));
    const rc = std.c.pthread_mutex_lock(&sem.mutex);
    if (rc != .SUCCESS) return -@as(i32, @intCast(@intFromEnum(rc)));
    const got: i32 = if (sem.count > 0) blk: {
        sem.count -= 1;
        break :blk 0;
    } else 1;
    _ = std.c.pthread_mutex_unlock(&sem.mutex);
    return got;
}

export fn spc_semaphore_release(ptr: *anyopaque) callconv(.c) i32 {
    const sem: *Semaphore = @ptrCast(@alignCast(ptr));
    const rc = std.c.pthread_mutex_lock(&sem.mutex);
    if (rc != .SUCCESS) return -@as(i32, @intCast(@intFromEnum(rc)));
    if (sem.count < sem.max) sem.count += 1;
    _ = std.c.pthread_cond_signal(&sem.cond);
    _ = std.c.pthread_mutex_unlock(&sem.mutex);
    return 0;
}

export fn spc_semaphore_available(ptr: *anyopaque) callconv(.c) usize {
    const sem: *Semaphore = @ptrCast(@alignCast(ptr));
    _ = std.c.pthread_mutex_lock(&sem.mutex);
    const n = sem.count;
    _ = std.c.pthread_mutex_unlock(&sem.mutex);
    return n;
}

export fn spc_semaphore_destroy(ptr: *anyopaque) callconv(.c) void {
    const sem: *Semaphore = @ptrCast(@alignCast(ptr));
    _ = std.c.pthread_mutex_destroy(&sem.mutex);
    _ = std.c.pthread_cond_destroy(&sem.cond);
    std.heap.c_allocator.destroy(sem);
}

// ---------------------------------------------------------------------------
// Process spawn with scheduling policy
// ---------------------------------------------------------------------------

/// Parameters for a throttled build-harness spawn.
/// Layout MUST match Rust `substrate_throttle::SpawnParams` (#[repr(C)]).
pub const SpawnParams = extern struct {
    program: [*:0]const u8,
    argv: [*:null]const ?[*:0]const u8,
    envp: ?[*:null]const ?[*:0]const u8,
    cwd: ?[*:0]const u8,
    nice_delta: i32,
    background_qos: u8,
};

extern "c" fn setpriority(which: c_int, who: c_uint, prio: c_int) c_int;
extern "c" fn waitpid(pid: std.c.pid_t, status: ?*c_int, options: c_int) std.c.pid_t;

const posix_spawn_fn = if (is_macos)
    *const fn (
        pid: *std.c.pid_t,
        path: [*:0]const u8,
        file_actions: ?*anyopaque,
        attrp: ?*anyopaque,
        argv: [*:null]const ?[*:0]const u8,
        envp: [*:null]const ?[*:0]const u8,
    ) callconv(.c) c_int
else
    void;

export fn spc_spawn(params: *const SpawnParams) callconv(.c) i32 {
    const pid = if (is_macos)
        spawn_macos(params)
    else
        spawn_posix(params);

    if (pid <= 0) return pid;

    if (params.nice_delta != 0) {
        const PRIO_PROCESS: c_int = 0;
        _ = setpriority(PRIO_PROCESS, @intCast(pid), params.nice_delta);
    }

    if (is_macos and params.background_qos != 0) {
        const PRIO_DARWIN_BG: c_int = 0x1000;
        _ = setpriority(PRIO_DARWIN_BG, @intCast(pid), 0);
    }

    return pid;
}

export fn spc_waitpid(pid: i32) callconv(.c) i32 {
    var status: c_int = 0;
    const ret = waitpid(@intCast(pid), &status, 0);
    if (ret < 0) return -1;
    return (status >> 8) & 0xff;
}

// ---------------------------------------------------------------------------
// macOS spawn path — posix_spawn + file_actions for chdir
// ---------------------------------------------------------------------------

fn spawn_macos(params: *const SpawnParams) i32 {
    if (!is_macos) unreachable;

    var pid: std.c.pid_t = 0;

    const envp: [*:null]const ?[*:0]const u8 = params.envp orelse
        @ptrCast(std.c.environ);

    var rc: c_int = 0;

    if (params.cwd != null) {
        var fa: [sizeof_posix_spawn_file_actions_t]u8 = undefined;
        var attr: [sizeof_posix_spawnattr_t]u8 = undefined;

        rc = posix_spawn_file_actions_init(@ptrCast(&fa));
        if (rc != 0) return -rc;
        defer _ = posix_spawn_file_actions_destroy(@ptrCast(&fa));

        rc = posix_spawnattr_init(@ptrCast(&attr));
        if (rc != 0) return -rc;
        defer _ = posix_spawnattr_destroy(@ptrCast(&attr));

        rc = posix_spawn_file_actions_addchdir_np(@ptrCast(&fa), params.cwd.?);
        if (rc != 0) return -rc;

        rc = posix_spawn(&pid, params.program, @ptrCast(&fa), @ptrCast(&attr),
            params.argv, envp);
    } else {
        rc = posix_spawn(&pid, params.program, null, null, params.argv, envp);
    }

    if (rc != 0) return -rc;
    return @intCast(pid);
}

const sizeof_posix_spawn_file_actions_t: usize = if (is_macos) 152 else 0;
const sizeof_posix_spawnattr_t: usize = if (is_macos) 376 else 0;

extern "c" fn posix_spawn(
    pid: *std.c.pid_t,
    path: [*:0]const u8,
    file_actions: ?*anyopaque,
    attrp: ?*anyopaque,
    argv: [*:null]const ?[*:0]const u8,
    envp: [*:null]const ?[*:0]const u8,
) c_int;
extern "c" fn posix_spawn_file_actions_init(fa: *anyopaque) c_int;
extern "c" fn posix_spawn_file_actions_destroy(fa: *anyopaque) c_int;
extern "c" fn posix_spawn_file_actions_addchdir_np(fa: *anyopaque, path: [*:0]const u8) c_int;
extern "c" fn posix_spawnattr_init(attr: *anyopaque) c_int;
extern "c" fn posix_spawnattr_destroy(attr: *anyopaque) c_int;

// ---------------------------------------------------------------------------
// POSIX (Linux) spawn path — fork + execve
// ---------------------------------------------------------------------------

fn spawn_posix(params: *const SpawnParams) i32 {
    if (is_macos) unreachable;

    const pid = std.posix.fork() catch return -1;
    if (pid == 0) {
        if (params.cwd) |cwd| {
            std.posix.chdir(std.mem.sliceTo(cwd, 0)) catch std.posix.exit(127);
        }
        const err = std.posix.execveZ(
            params.program,
            params.argv,
            params.envp orelse @ptrCast(std.c.environ),
        );
        _ = err;
        std.posix.exit(127);
    }
    return @intCast(pid);
}