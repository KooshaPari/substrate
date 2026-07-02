#!/usr/bin/env bash
# package.sh — build slim substrate binary and create distributable archive.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Detect version, architecture, OS
VERSION=$(grep '^version' Cargo.toml | grep -v workspace | head -1 | cut -d'"' -f2 || echo "0.1.0")
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Build release binary with slim profile
echo "Building substrate v$VERSION for $ARCH-$OS (slim profile)..."
cargo build --release -p substrate 2>&1 | tail -5

# Try substrate-gateway, substrate-driver-http, or substrate-driver-cli
for BIN in substrate-gateway substrate-driver-http substrate-driver-cli gateway driver-http driver-cli; do
    if [ -f "target/release/$BIN" ]; then
        BINARY="target/release/$BIN"
        BIN_NAME=$(basename "$BIN")
        break
    fi
done

if [ -z "${BINARY:-}" ]; then
    echo "Error: No binary found"
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
