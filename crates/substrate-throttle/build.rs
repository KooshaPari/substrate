//! build.rs — compile the Zig substrate-throttle static library and link it.
//!
//! Same protocol as sharecli/spawn-core-sys (which this mirrors):
//!   1. `zig build` in `crates/substrate-throttle/` → zig-out/lib/libspawn_core.a
//!   2. Cargo links `libspawn_core.a` via `cargo:rustc-link-lib=static=spawn_core`
//!   3. macOS links `libSystem`, Linux links `libc` + `libpthread`.

use std::path::PathBuf;
use std::process::Command;

fn main() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib_out = manifest_dir.join("zig-out").join("lib");

    println!("cargo:rerun-if-changed=src/spawn_core.zig");
    println!("cargo:rerun-if-changed=build.zig");

    let status = Command::new("zig")
        .args(["build", "-Doptimize=ReleaseSafe"])
        .current_dir(&manifest_dir)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run `zig build`: {e}\nIs zig installed and on PATH?"))?;
    if !status.success() {
        anyhow::bail!("`zig build` exited with status {status}");
    }

    println!("cargo:rustc-link-search=native={}", lib_out.display());
    println!("cargo:rustc-link-lib=static=spawn_core");

    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=dylib=System");

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=dylib=c");
        println!("cargo:rustc-link-lib=dylib=pthread");
    }

    Ok(())
}