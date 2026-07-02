//! substrate-throttle — safe Rust wrapper over the vendored Zig hot core.
//!
//! Two halves:
//!   1. `ZigSemaphore` — RAII wrapper around `spc_semaphore_*`.
//!      Used to cap concurrent substrate-driven builds.
//!   2. `zig_spawn` — thin wrapper around `spc_spawn` with optional
//!      `nice_delta` + macOS background_qos.
//!
//! # Activation
//!
//! The throttle is **opt-in** via the env var `SUBSTRATE_THROTTLE_MAX=N`.
//! When unset, `run_simple` calls `spc_semaphore_acquire` on a max=N=∞
//! permit (effectively a no-op). When set, it caps at N concurrent
//! dispatches across the calling process.
//!
//! Vendoring source: github.com/KooshaPari/sharecli @ a3e308e (#16).

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_void};
use std::ptr;

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

/// RAII wrapper around a Zig-allocated counting semaphore.
///
/// The semaphore is created lazily on first use by [`process_throttle`],
/// keyed on the value of `SUBSTRATE_THROTTLE_MAX` (or unbounded if unset).
pub struct ZigSemaphore {
    ptr: *mut c_void,
}

// SAFETY: the Zig semaphore is internally protected by a pthread mutex.
unsafe impl Send for ZigSemaphore {}
unsafe impl Sync for ZigSemaphore {}

impl ZigSemaphore {
    pub fn new(max: usize) -> Self {
        // SAFETY: spc_semaphore_new returns a valid heap pointer on success.
        let ptr = unsafe { spc_semaphore_new(max.max(1)) };
        assert!(!ptr.is_null(), "spc_semaphore_new: allocation failed");
        Self { ptr }
    }

    pub fn acquire(&self) -> Result<(), std::io::Error> {
        let rc = unsafe { spc_semaphore_acquire(self.ptr) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(-rc))
        }
    }

    /// Returns `Ok(true)` if acquired, `Ok(false)` if no permits available.
    pub fn try_acquire(&self) -> Result<bool, std::io::Error> {
        let rc = unsafe { spc_semaphore_try_acquire(self.ptr) };
        match rc {
            0 => Ok(true),
            1 => Ok(false),
            n => Err(std::io::Error::from_raw_os_error(-n)),
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
}

impl Drop for ZigSemaphore {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { spc_semaphore_destroy(self.ptr) };
            self.ptr = ptr::null_mut();
        }
    }
}

/// Spawn a throttled child process.
///
/// `args` must include argv[0] as the first element and end with `null`.
/// `envp` may be `None` to inherit the parent environment.
///
/// Returns the child PID on success.
pub fn zig_spawn(
    program: &CStr,
    args: &[*const c_char],
    envp: Option<&[*const c_char]>,
    cwd: Option<&CStr>,
    nice_delta: i32,
    background_qos: bool,
) -> Result<i32, std::io::Error> {
    let params = SpawnParams {
        program: program.as_ptr(),
        argv: args.as_ptr(),
        envp: envp.map(|e| e.as_ptr()).unwrap_or(ptr::null()),
        cwd: cwd.map(|c| c.as_ptr()).unwrap_or(ptr::null()),
        nice_delta,
        background_qos: if background_qos { 1 } else { 0 },
    };
    let pid = unsafe { spc_spawn(&params) };
    if pid > 0 {
        Ok(pid)
    } else {
        Err(std::io::Error::from_raw_os_error(-pid))
    }
}

/// Wait for child `pid` to exit. Returns the exit status (0..=255).
pub fn zig_waitpid(pid: i32) -> Result<i32, std::io::Error> {
    let rc = unsafe { spc_waitpid(pid) };
    if rc >= 0 {
        Ok(rc)
    } else {
        Err(std::io::Error::from_raw_os_error(-rc))
    }
}

// ---------------------------------------------------------------------------
// Process-scoped throttle
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

/// Maximum concurrent substrate dispatches allowed per process.
/// Default = `usize::MAX` (effectively unbounded, no overhead).
fn max_concurrent() -> usize {
    static MAX: OnceLock<usize> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("SUBSTRATE_THROTTLE_MAX")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(usize::MAX)
    })
}

/// Process-wide semaphore instance.
pub fn process_throttle() -> &'static ZigSemaphore {
    static THROTTLE: OnceLock<ZigSemaphore> = OnceLock::new();
    THROTTLE.get_or_init(|| ZigSemaphore::new(max_concurrent()))
}

/// RAII guard: holds one permit for the lifetime of the guard.
/// Released automatically on drop.
pub struct ThrottleGuard {
    acquired: bool,
}

impl ThrottleGuard {
    /// Acquire one permit, blocking until a slot is free.
    pub fn acquire() -> Self {
        let sem = process_throttle();
        sem.acquire().expect("semaphore acquire failed");
        Self { acquired: true }
    }
}

impl Drop for ThrottleGuard {
    fn drop(&mut self) {
        if self.acquired {
            let sem = process_throttle();
            let _ = sem.release();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semaphore_acquire_release() {
        let sem = ZigSemaphore::new(2);
        assert_eq!(sem.available(), 2);
        sem.acquire().unwrap();
        assert_eq!(sem.available(), 1);
        sem.acquire().unwrap();
        assert_eq!(sem.available(), 0);
        assert!(!sem.try_acquire().unwrap());
        sem.release().unwrap();
        assert_eq!(sem.available(), 1);
    }

    #[test]
    fn spawn_true() {
        use std::ffi::CString;
        let prog = CString::new("/usr/bin/true").unwrap();
        let argv0 = CString::new("true").unwrap();
        let args: Vec<*const c_char> = vec![argv0.as_ptr(), ptr::null()];
        let pid = zig_spawn(&prog, &args, None, None, 0, false).expect("spawn");
        assert!(pid > 0);
        assert_eq!(zig_waitpid(pid).unwrap(), 0);
    }
}