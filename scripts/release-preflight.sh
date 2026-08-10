#!/usr/bin/env bash
# Ensure a tagged binary can resolve solely from the committed lockfile.
set -euo pipefail

cargo metadata --locked --offline --no-deps --format-version 1 >/dev/null
cargo build --locked --offline --release -p driver-cli
