#!/usr/bin/env bash
# scripts/release-crates-publish.sh — driver for the Release Crates workflow.
#
# Inputs (env):
#   DRY_RUN    — "true" to do a dry-run, anything else publishes for real.
#   EVENT_NAME — the triggering GitHub event (push tag, workflow_dispatch, ...).
#                We publish for real on tag-push or explicit dispatch.
#   CARGO_TOKEN — crates.io API token. Required when DRY_RUN != "true".
#
# Strategy:
#   - When DRY_RUN=true we dry-run every member crate in topological order.
#   - Otherwise we use `cargo workspaces publish --from-git` which handles
#     dependency order for the whole workspace and respects the matched tag.

set -euo pipefail

DRY_RUN="${DRY_RUN:-false}"
EVENT_NAME="${EVENT_NAME:-workflow_dispatch}"

# Only publish for real on tag-push or workflow_dispatch.
publish_for_real() {
  case "$EVENT_NAME" in
    push) return 0 ;;
    workflow_dispatch) return 0 ;;
    *) return 1 ;;
  esac
}

if [[ "$DRY_RUN" == "true" ]] || ! publish_for_real; then
  echo "==> Dry-run: packaging every workspace member with --dry-run"
  cargo workspaces publish --dry-run --from-git --allow-dirty
else
  : "${CARGO_TOKEN:?CARGO_TOKEN must be set for non-dry-run release}"
  echo "==> Publishing every workspace member in topological order"
  cargo workspaces publish --from-git --token "$CARGO_TOKEN" --no-verify
fi
