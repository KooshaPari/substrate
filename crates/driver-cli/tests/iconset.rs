//! Integration test for the substrate golden icon set (vision-pillar L96).
//!
//! CI-safe: file presence + palette/dimension invariants only.
//!
//! Run: cargo test -p driver-cli --test iconset

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR for the integration test points at crates/driver-cli/,
    // but assets/ lives at the workspace root. Resolve up two levels.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn iconset_dir() -> PathBuf {
    workspace_root().join("assets/icons/substrate.iconset")
}

fn brand_dir() -> PathBuf {
    workspace_root().join("assets/brand")
}

fn root_assets() -> PathBuf {
    workspace_root().join("assets/icons")
}

const REQUIRED_SIZES: &[u32] = &[16, 32, 64, 128, 256, 512, 1024];

#[test]
fn brand_svg_exists_and_is_valid_xml() {
    let svg = brand_dir().join("substrate-icon.svg");
    assert!(svg.exists(), "missing brand svg: {}", svg.display());
    let content = std::fs::read_to_string(&svg).expect("read svg");
    assert!(content.starts_with("<?xml"), "missing XML declaration");
    assert!(content.contains("<svg"));
    assert!(content.contains("</svg>"));
}

#[test]
fn brand_svg_uses_backbone2_palette() {
    let content =
        std::fs::read_to_string(brand_dir().join("substrate-icon.svg")).expect("read svg");
    let palette = [
        ("#0a0d12", "graphite-black background"),
        ("#161b22", "panel base"),
        ("#a371f7", "sync-violet accent"),
        ("#3fb950", "pulse-green mesh glow"),
        ("#d29922", "warm-amber dispatch node"),
    ];
    for (hex, label) in palette {
        assert!(
            content.to_lowercase().contains(hex),
            "brand svg missing {label} ({hex})"
        );
    }
}

#[test]
fn brand_svg_viewbox_is_1024() {
    let content =
        std::fs::read_to_string(brand_dir().join("substrate-icon.svg")).expect("read svg");
    assert!(
        content.contains("viewBox=\"0 0 1024 1024\""),
        "viewBox must be 0 0 1024 1024"
    );
    assert!(content.contains("width=\"1024\""));
    assert!(content.contains("height=\"1024\""));
}

#[test]
fn iconset_has_all_required_apple_sizes() {
    let dir = iconset_dir();
    assert!(dir.is_dir(), "iconset dir missing: {}", dir.display());
    for sz in REQUIRED_SIZES {
        let p = dir.join(format!("icon_{sz}x{sz}.png"));
        assert!(p.exists(), "missing apple icon size: {}", p.display());
    }
}

#[test]
fn iconset_has_required_at2x_variants() {
    let dir = iconset_dir();
    for sz in [16u32, 32, 128, 256] {
        let p = dir.join(format!("icon_{sz}x{sz}@2x.png"));
        assert!(p.exists(), "missing @2x variant: {}", p.display());
    }
}

#[test]
fn iconset_has_windows_ico() {
    let ico = root_assets().join("substrate.ico");
    assert!(ico.exists(), "missing windows .ico: {}", ico.display());
    let bytes = std::fs::read(&ico).expect("read .ico");
    assert!(bytes.len() >= 6, "ico too small: {} bytes", bytes.len());
    assert_eq!(&bytes[0..2], &[0, 0]);
    assert_eq!(&bytes[2..4], &[1, 0], "ico type must be 1 (icon)");
}

#[test]
fn iconset_has_linux_256() {
    let png = root_assets().join("substrate-256x256.png");
    assert!(png.exists(), "missing linux png: {}", png.display());
}

#[test]
fn brand_readme_documents_palette_and_regen() {
    let readme = brand_dir().join("README.md");
    assert!(
        readme.exists(),
        "missing brand README: {}",
        readme.display()
    );
    let content = std::fs::read_to_string(&readme).expect("read readme");
    for hex in ["#0a0d12", "#161b22", "#a371f7", "#3fb950", "#d29922"] {
        assert!(
            content.contains(hex),
            "brand README missing palette hex {hex}"
        );
    }
    assert!(
        content.contains("rsvg-convert"),
        "regen snippet missing rsvg-convert"
    );
    assert!(content.contains("convert"), "regen snippet missing convert");
}

#[test]
fn driver_cli_cargo_toml_has_bundle_metadata_block() {
    let toml = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read Cargo.toml");
    assert!(
        toml.contains("[package.metadata.bundle]"),
        "missing [package.metadata.bundle]"
    );
    assert!(
        toml.contains("substrate.iconset"),
        "bundle.icon must reference substrate.iconset"
    );
    assert!(toml.contains("substrate"), "bundle.name must be substrate");
}

#[test]
fn iconset_pngs_are_nonempty() {
    let dir = iconset_dir();
    for entry in std::fs::read_dir(&dir).expect("read iconset dir") {
        let entry = entry.expect("dir entry");
        if entry.path().extension().and_then(|s| s.to_str()) == Some("png") {
            let bytes = std::fs::read(entry.path()).expect("read png");
            assert!(
                bytes.len() > 200,
                "{}: {} bytes (too small)",
                entry.path().display(),
                bytes.len()
            );
        }
    }
}

#[test]
fn palette_distinct_from_sharecli_and_tracera() {
    // Defensive: ensure we didn't accidentally sharecli-swap the dominant accent.
    let content =
        std::fs::read_to_string(brand_dir().join("substrate-icon.svg")).expect("read svg");
    // substrate dominant = sync-violet (#a371f7) — must appear more prominently than pulse-green (#3fb950).
    // Cheap heuristic: count occurrences.
    let violet_count = content.matches("#a371f7").count();
    let green_count = content.matches("#3fb950").count();
    assert!(
        violet_count > green_count,
        "substrate should be sync-violet dominant: violet={violet_count} green={green_count}"
    );
}
