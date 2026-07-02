#!/usr/bin/env bash
# package.sh — build slim substrate binary and create distributable archive.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Detect version, architecture, OS
VERSION=$(grep '^version' crates/substrate/Cargo.toml | head -1 | cut -d'"' -f2)
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Build release binary with slim profile
echo "Building substrate v$VERSION for $ARCH-$OS (slim profile)..."
cargo build --release -p substrate 2>&1 | tail -5

BINARY="target/release/substrate"
if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    exit 1
fi

# Check size before packaging
BINARY_SIZE=$(ls -lh "$BINARY" | awk '{print $5}')
echo "Binary size: $BINARY_SIZE"

# Create archive
ARCHIVE_NAME="substrate-${VERSION}-${ARCH}-${OS}.tar.gz"
echo "Creating archive: $ARCHIVE_NAME"

# Extract binary and strip/compress
mkdir -p build/substrate-${VERSION}
cp "$BINARY" build/substrate-${VERSION}/substrate
tar -czf "$ARCHIVE_NAME" -C build substrate-${VERSION}/

ARCHIVE_SIZE=$(ls -lh "$ARCHIVE_NAME" | awk '{print $5}')
echo "Archive size: $ARCHIVE_SIZE"
echo "Packaged: $ARCHIVE_NAME"

# Cleanup
rm -rf build

echo "Done. Binary: $BINARY_SIZE → Archive: $ARCHIVE_SIZE"
