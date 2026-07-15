//! Build script for `substrate-throttle`.
//!
//! Invokes `zig build` on the sibling `crates/spawn-core/` subproject to
//! produce `libspawn_core.a` (C-ABI static library) and links it into the
//! resulting rlib/dylib. Mirrors `crates/forge_daemon/build.rs`.
//!
//! ## Why call `zig` from Cargo
//!
//! The spawn-core is a hot syscall-path (semaphore acquire, posix_spawn,
//! setpriority, waitpid). Keeping it in Zig gives us a deterministic C ABI
//! with no Rust trait/monomorphisation cost on the hot path — exactly the
//! pattern sharecli landed in PR #16.
//!
//! If `zig` is missing we `cargo:warning=` and skip the link — the Rust
//! types still compile, callers fall back to in-process throttling (e.g.
//! a `tokio::Semaphore`). This keeps `cargo check` usable on machines
//! without the Zig toolchain.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Tell rustc to expect the `has_zig_spawn_core` cfg so check-cfg
    // doesn't warn every build.
    println!("cargo:rustc-check-cfg=cfg(has_zig_spawn_core)");

    // Re-run when Zig sources change.
    // Re-run when Zig sources change.
    println!("cargo:rerun-if-changed=../spawn-core/build.zig");
    println!("cargo:rerun-if-changed=../spawn-core/src/spawn_core.zig");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let spawn_core = manifest_dir.parent().unwrap().join("spawn-core");
    if !spawn_core.join("build.zig").exists() {
        println!(
            "cargo:warning=substrate-throttle: spawn-core/build.zig missing at {}; \
             skipping Zig build (fallback to in-process throttle).",
            spawn_core.display()
        );
        return;
    }

    let zig = match which::which("zig") {
        Ok(p) => p,
        Err(_) => {
            println!(
                "cargo:warning=substrate-throttle: `zig` not on PATH; \
                 skipping Zig build (fallback to in-process throttle)."
            );
            return;
        }
    };

    // Out-of-tree build to avoid contaminating the spawn-core source dir.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("spawn-core-zig");
    std::fs::create_dir_all(&out_dir).expect("create zig out dir");

    let profile = if std::env::var("PROFILE").as_deref() == Ok("release") {
        "ReleaseFast"
    } else {
        "Debug"
    };

    let status = Command::new(&zig)
        .current_dir(&spawn_core)
        .arg("build")
        .arg(format!("-Doptimize={profile}"))
        .env("ZIG_OUT_DIR", &out_dir)
        .status()
        .expect("spawn zig build");
    if !status.success() {
        // Do not panic — emit a warning and let the Rust types compile so
        // `cargo check` continues to work. The runtime will fall back to the
        // tokio::Semaphore path (see lib.rs).
        println!(
            "cargo:warning=substrate-throttle: zig build failed (exit {status:?}); \
             using in-process fallback."
        );
        return;
    }

    // zig build writes libspawn_core.a into zig-out/lib (default layout).
    // Some zig versions honour $ZIG_OUT_DIR for the cache but still emit
    // artefacts under <source>/zig-out/lib; check both.
    let candidates = [
        out_dir.join("lib").join("libspawn_core.a"),
        spawn_core.join("zig-out").join("lib").join("libspawn_core.a"),
    ];
    let lib = candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());

    println!("cargo:rustc-link-search=native={}", lib.parent().unwrap().display());
    println!("cargo:rustc-link-lib=static=spawn_core");
    // Tell rustc which path to take so `mod zig` is only compiled when the
    // Zig static lib was actually produced.
    println!("cargo:rustc-cfg=has_zig_spawn_core");
}