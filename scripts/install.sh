#!/usr/bin/env bash
# scripts/install.sh — install `substrate` CLI binary (macOS / Linux)
# Wraps: cargo build --release -p driver-cli
# Default: /usr/local/bin/substrate (override with INSTALL_DIR)
set -euo pipefail

REPO_ROOT="$([ -d .git ] && git rev-parse --show-toplevel || pwd)"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="substrate"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"

cd "$REPO_ROOT"

echo "==> building release binary (jobs=$JOBS)"
cargo build --release -p driver-cli --jobs "$JOBS"

BIN_PATH="target/release/$BINARY_NAME"
if [[ ! -f "$BIN_PATH" ]]; then
  echo "error: build did not produce $BIN_PATH" >&2
  exit 1
fi

echo "==> stripping debug symbols"
strip "$BIN_PATH" 2>/dev/null || true

SIZE=$(du -h "$BIN_PATH" | cut -f1)
echo "==> built $BIN_PATH ($SIZE)"

if [[ ! -d "$INSTALL_DIR" ]]; then
  echo "==> creating $INSTALL_DIR (sudo may prompt)"
  sudo mkdir -p "$INSTALL_DIR"
fi

echo "==> installing to $INSTALL_DIR/$BINARY_NAME"
if [[ -w "$INSTALL_DIR" ]]; then
  install -m 0755 "$BIN_PATH" "$INSTALL_DIR/$BINARY_NAME"
else
  sudo install -m 0755 "$BIN_PATH" "$INSTALL_DIR/$BINARY_NAME"
fi

echo "==> verifying install"
"$INSTALL_DIR/$BINARY_NAME" --version || true

echo
echo "substrate installed to $INSTALL_DIR/$BINARY_NAME"
echo "ensure $INSTALL_DIR is on your PATH"