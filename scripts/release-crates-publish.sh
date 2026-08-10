#!/usr/bin/env bash
# Publish a bounded, deterministic batch of public workspace crates.
set -euo pipefail

dry_run="${DRY_RUN:-false}"
github_ref="${GITHUB_REF:-}"
batch_start="${PUBLISH_BATCH_START:-0}"
batch_size="${PUBLISH_BATCH_SIZE:-10}"
(( batch_start >= 0 && batch_size > 0 && batch_size <= 10 )) || {
  echo "PUBLISH_BATCH_START must be >= 0 and PUBLISH_BATCH_SIZE must be 1..10" >&2
  exit 2
}

if [[ "$dry_run" == "true" ]] || [[ ! "$github_ref" =~ ^refs/tags/v ]]; then
  cargo workspaces publish --dry-run --allow-dirty
  exit 0
fi

: "${CARGO_TOKEN:?CARGO_TOKEN must be set for a release publish}"
export CARGO_REGISTRY_TOKEN="$CARGO_TOKEN"

if [[ -n "${PUBLISH_ORDER:-}" ]]; then
  IFS=',' read -r -a packages <<< "$PUBLISH_ORDER"
  versions=()
  for _ in "${packages[@]}"; do versions+=("${PUBLISH_VERSION:-0.0.0}"); done
else
  metadata="$(cargo metadata --locked --no-deps --format-version 1)"
  mapfile -t packages < <(jq -r '.packages[] | select(.publish == null or (.publish | length) > 0) | .name' <<< "$metadata")
  mapfile -t versions < <(jq -r '.packages[] | select(.publish == null or (.publish | length) > 0) | .version' <<< "$metadata")
  # Keep foundational crates ahead of dependents; append the remaining public
  # members in metadata order. This is the stable dependency spine for batches.
  preferred=(substrate-core engine-spec substrate-a2a substrate-app substrate-serve-lock
    transport-file store-file cloud-dispatch-conformance cloud-codex engine-conformance
    engine-agentapi engine-claude engine-codex)
  ordered=(); ordered_versions=()
  for wanted in "${preferred[@]}"; do
    for j in "${!packages[@]}"; do
      if [[ "${packages[$j]}" == "$wanted" ]]; then
        ordered+=("${packages[$j]}"); ordered_versions+=("${versions[$j]}"); break
      fi
    done
  done
  for j in "${!packages[@]}"; do
    case " ${ordered[*]} " in *" ${packages[$j]} "*) ;; *) ordered+=("${packages[$j]}"); ordered_versions+=("${versions[$j]}");; esac
  done
  packages=("${ordered[@]}"); versions=("${ordered_versions[@]}")
fi

end=$((batch_start + batch_size))
for ((i=batch_start; i<end && i<${#packages[@]}; i++)); do
  package="${packages[$i]}"
  version="${versions[$i]}"
  if [[ "${SKIP_REGISTRY_CHECK:-false}" != "true" ]] && curl -fsS "https://crates.io/api/v1/crates/${package}/${version}" >/dev/null 2>&1; then
    echo "skip already-published ${package} ${version}"
    continue
  fi
  cargo publish --locked --package "$package"
done
