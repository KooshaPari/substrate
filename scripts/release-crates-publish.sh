#!/usr/bin/env bash
# Publish the whole workspace in dependency order for a release tag.
set -euo pipefail

dry_run="${DRY_RUN:-false}"
github_ref="${GITHUB_REF:-}"

if [[ "$dry_run" == "true" ]] || [[ ! "$github_ref" =~ ^refs/tags/v ]]; then
  cargo workspaces publish --dry-run --allow-dirty
  exit 0
fi

: "${CARGO_TOKEN:?CARGO_TOKEN must be set for a release publish}"
export CARGO_REGISTRY_TOKEN="$CARGO_TOKEN"
cargo workspaces publish --from-git
