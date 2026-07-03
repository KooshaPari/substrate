#!/usr/bin/env bash
# scripts/install.sh — native install for the substrate CLI + TUI daemon.
#
# Usage:
#   ./scripts/install.sh            # install substrate + substrate-tui to ~/.cargo/bin
#   INSTALL_PREFIX=/usr/local ./scripts/install.sh   # install to a custom prefix

set -euo pipefail

# --- configuration -----------------------------------------------------------
INSTALL_PREFIX="${INSTALL_PREFIX:-$HOME/.cargo/bin}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# --- helpers -----------------------------------------------------------------
info()  { printf '\033[0;32m[install]\033[0m %s\n' "$*"; }
warn()  { printf '\033[0;33m[install]\033[0m %s\n' "$*" >&2; }
fatal() { printf '\033[0;31m[install]\033[0m %s\n' "$*" >&2; exit 1; }

# --- preflight ---------------------------------------------------------------
command -v cargo >/dev/null 2>&1 || fatal "cargo not found — install Rust from https://rustup.rs"
cargo --version | grep -q "^cargo" || fatal "cargo --version failed"

info "Building substrate (driver-cli) and substrate-tui in release mode..."
cd "$REPO_ROOT"

cargo build --release -p driver-cli -p substrate-tui 2>&1

TARGET_DIR="$REPO_ROOT/target/release"

# --- install binaries --------------------------------------------------------
mkdir -p "$INSTALL_PREFIX"

for bin in substrate substrate-tui; do
    src="$TARGET_DIR/$bin"
    dst="$INSTALL_PREFIX/$bin"
    if [[ ! -f "$src" ]]; then
        fatal "Expected binary not found: $src (did the build succeed?)"
    fi
    cp -f "$src" "$dst"
    chmod +x "$dst"
    info "Installed $bin → $dst"
done

# --- verify ------------------------------------------------------------------
if command -v substrate >/dev/null 2>&1; then
    info "substrate --help:"
    substrate --help | head -5
fi

info ""
info "Installation complete."
info ""
info "Quick start:"
info "  substrate --help"
info "  substrate dash --gateway http://127.0.0.1:8010"
info ""
info "Run as a daemon with proc-compose:"
info "  process-compose up substrate-gateway"
info "  # in another terminal:"
info "  substrate dash"
