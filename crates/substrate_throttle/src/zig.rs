//! FFI bridge to the Zig `spawn_core` static library. Mirrors
//! sharecli `crates/spawn-core-sys/src/lib.rs` 1:1 — see
//! `KooshaPari/sharecli@a3e308e`.
//!
//! Only compiled when the build script successfully produced
//! `libspawn_core.a`. The `has_zig_spawn_core` cfg is emitted by the
//! `substrate-throttle` build script.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uchar};

/// Opaque semaphore pointer allocated by Zig. Never construct directly.
#[repr(transparent)]
pub struct SemaphoreHandle(*mut c_void);

// SAFETY: Zig semaphore is mutex+condvar protected; Send/Sync are safe.
unsafe impl Send for SemaphoreHandle {}
unsafe impl Sync for SemaphoreHandle {}

#[repr(C)]
pub struct SpawnParams {
    pub program: *const c_char,
    pub argv: *const *const c_char,
    pub envp: *const *const c_char,
    pub cwd: *const c_char,
    pub nice_delta: c_int,
    pub background_qos: c_uchar,
}

extern "C" {
    fn spc_semaphore_new(max: usize) -> *mut c_void;
    fn spc_semaphore_acquire(ptr: *mut c_void) -> c_int;
    fn spc_semaphore_try_acquire(ptr: *mut c_void) -> c_int;
    fn spc_semaphore_release(ptr: *mut c_void) -> c_int;
    fn spc_semaphore_available(ptr: *mut c_void) -> usize;
    fn spc_semaphore_destroy(ptr: *mut c_void);
    fn spc_spawn(params: *const SpawnParams) -> c_int;
    fn spc_waitpid(pid: c_int) -> c_int;
}

/// Safe RAII wrapper around the Zig semaphore.
pub struct ZigSemaphore {
    ptr: *mut c_void,
}

// SAFETY: Semaphore is thread-safe (POSIX mutex+condvar).
unsafe impl Send for ZigSemaphore {}
unsafe impl Sync for ZigSemaphore {}

// FFI surface is broader than the Rust wrapper currently uses; the extras
// (`acquire`/`waitpid`/`spawn`) are exercised by future engine-forge
// integration and downstream posix_spawn-based dispatch paths. Suppress
// dead_code so the crate compiles clean across the full FFI surface.
#[allow(dead_code)]
impl ZigSemaphore {
    pub fn new(max: usize) -> Self {
        // SAFETY: Zig allocator returns non-null or panics inside.
        let ptr = unsafe { spc_semaphore_new(max.max(1)) };
        assert!(!ptr.is_null(), "spc_semaphore_new: allocation failed");
        Self { ptr }
    }

    pub fn acquire(&self) -> Result<(), std::io::Error> {
        // SAFETY: ptr is valid; spc_semaphore_acquire is thread-safe.
        let rc = unsafe { spc_semaphore_acquire(self.ptr) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(-rc))
        }
    }

    pub fn try_acquire(&self) -> Result<bool, std::io::Error> {
        let rc = unsafe { spc_semaphore_try_acquire(self.ptr) };
        match rc {
            // Zig convention: 0 = acquired, 1 = no slot (see
            // crates/spawn-core/src/spawn_core.zig::spc_semaphore_try_acquire).
            0 => Ok(true),
            1 => Ok(false),
            e => Err(std::io::Error::from_raw_os_error(-e)),
        }
    }

    pub fn release(&self) -> Result<(), std::io::Error> {
        let rc = unsafe { spc_semaphore_release(self.ptr) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(-rc))
        }
    }

    pub fn available(&self) -> usize {
        unsafe { spc_semaphore_available(self.ptr) }
    }

    /// `spc_spawn` FFI wrapper — exposed for future posix_spawn-based
    /// integration paths.
    pub fn spawn(params: &SpawnParams) -> Result<i32, std::io::Error> {
        // SAFETY: caller guarantees SpawnParams pointer fields are valid
        // null-terminated strings for the duration of the call.
        let rc = unsafe { spc_spawn(params as *const SpawnParams) };
        if rc > 0 {
            Ok(rc)
        } else if rc == 0 {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "spc_spawn returned 0 (invalid pid)",
            ))
        } else {
            Err(std::io::Error::from_raw_os_error(-rc))
        }
    }

    /// `spc_waitpid` FFI wrapper.
    pub fn waitpid(pid: i32) -> i32 {
        // SAFETY: simple waitpid(2) wrapper.
        unsafe { spc_waitpid(pid) }
    }
}

impl Drop for ZigSemaphore {
    fn drop(&mut self) {
        // SAFETY: ptr was allocated by Zig; spc_semaphore_destroy is idempotent.
        unsafe { spc_semaphore_destroy(self.ptr) }
    }
}