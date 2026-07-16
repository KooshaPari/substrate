#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/release-crates-publish.sh"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cat > "$tmp_dir/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$CARGO_LOG"
EOF
chmod +x "$tmp_dir/cargo"

assert_log() {
  local expected="$1"
  if ! grep -Fxq -- "$expected" "$CARGO_LOG"; then
    echo "expected cargo invocation not found: $expected" >&2
    cat "$CARGO_LOG" >&2
    exit 1
  fi
}

export PATH="$tmp_dir:$PATH"
export CARGO_LOG="$tmp_dir/cargo.log"

DRY_RUN=true EVENT_NAME=workflow_dispatch bash "$script"
assert_log "workspaces publish --dry-run --allow-dirty"

: > "$CARGO_LOG"
DRY_RUN=false EVENT_NAME=workflow_dispatch GITHUB_REF=refs/heads/main CARGO_TOKEN=release-token bash "$script"
assert_log "workspaces publish --dry-run --allow-dirty"

: > "$CARGO_LOG"
DRY_RUN=false EVENT_NAME=push GITHUB_REF=refs/tags/v0.3.1 CARGO_TOKEN=release-token bash "$script"
assert_log "workspaces publish --from-git"

if grep -q -- '--no-verify' "$CARGO_LOG"; then
  echo "release publishing must not bypass cargo verification" >&2
  exit 1
fi

if DRY_RUN=false EVENT_NAME=push GITHUB_REF=refs/tags/v0.3.1 bash "$script" 2>/dev/null; then
  echo "tag publishing must reject a missing CARGO_TOKEN" >&2
  exit 1
fi
