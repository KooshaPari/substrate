#!/usr/bin/env bash
# package.sh — build the substrate CLI and create a distributable archive.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Detect version, architecture, OS. The version lives under [workspace.package],
# so do not accidentally read a dependency or a nested package manifest.
VERSION=$(awk '
  /^\[workspace\.package\]/ { in_workspace_package = 1; next }
  /^\[/ { in_workspace_package = 0 }
  in_workspace_package && /^version[[:space:]]*=/ {
    sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit
  }
' Cargo.toml)
if [ -z "$VERSION" ]; then
    echo "Error: could not determine workspace package version" >&2
    exit 1
fi
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

# Build the release CLI. The package is named driver-cli; its [[bin]] target is
# deliberately named substrate.
echo "Building substrate v$VERSION for $ARCH-$OS..."
cargo build --locked --release -p driver-cli

BINARY="target/release/substrate"
if [ ! -x "$BINARY" ]; then
    echo "Error: expected release binary was not produced: $BINARY" >&2
    exit 1
fi

# Check size before packaging
BINARY_SIZE=$(ls -lh "$BINARY" | awk '{print $5}')
echo "Binary size: $BINARY_SIZE"

ARCHIVE_NAME="substrate-${VERSION}-${ARCH}-${OS}.tar.gz"
echo "Creating archive: $ARCHIVE_NAME"

STAGING_DIR=$(mktemp -d "${TMPDIR:-/tmp}/substrate-package.XXXXXX")
trap 'rm -rf "$STAGING_DIR"' EXIT
PACKAGE_DIR="$STAGING_DIR/substrate-${VERSION}"
mkdir -p "$PACKAGE_DIR"
cp "$BINARY" "$PACKAGE_DIR/substrate"
for metadata in LICENSE LICENSE-APACHE LICENSE-MIT; do
    if [ -f "$metadata" ]; then
        cp "$metadata" "$PACKAGE_DIR/"
    fi
done
if [ -f docs/INSTALL.md ]; then
    cp docs/INSTALL.md "$PACKAGE_DIR/"
fi
tar -czf "$ARCHIVE_NAME" -C "$STAGING_DIR" "substrate-${VERSION}/"

ARCHIVE_SIZE=$(ls -lh "$ARCHIVE_NAME" | awk '{print $5}')
echo "Archive size: $ARCHIVE_SIZE"
echo "Packaged: $ARCHIVE_NAME"

echo "Done. Binary: $BINARY_SIZE → Archive: $ARCHIVE_SIZE"
