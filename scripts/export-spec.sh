#!/usr/bin/env bash
# Export the compiled `will` contract spec to spec/will-v<version>.json.
#
# Usage:
#   scripts/export-spec.sh           # write/refresh the snapshot
#   scripts/export-spec.sh --check   # fail if the snapshot is stale
#
# The Stellar CLI binary can be overridden with STELLAR_BIN.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

STELLAR_BIN="${STELLAR_BIN:-stellar}"
WASM_PATH="target/wasm32v1-none/release/will.wasm"

MODE="write"
if [[ "${1:-}" == "--check" ]]; then
  MODE="check"
elif [[ $# -gt 0 ]]; then
  echo "error: unknown argument: $1" >&2
  echo "usage: $0 [--check]" >&2
  exit 2
fi

stellar_bin="${STELLAR_BIN:-stellar}"

# ── Preflight ────────────────────────────────────────────────────────────────
# Fail before any build or output work when a required executable is missing,
# so contributors can tell an unmet local prerequisite from a repo regression.
missing_tools=()
for tool in cargo jq "${stellar_bin}"; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    missing_tools+=("${tool}")
  fi
done
if (( ${#missing_tools[@]} > 0 )); then
  for tool in "${missing_tools[@]}"; do
    echo "ERROR: required executable '${tool}' was not found on PATH." >&2
  done
  echo "Install the missing tool(s) and re-run scripts/export-spec.sh." >&2
  echo "  cargo:   https://rustup.rs" >&2
  echo "  jq:      https://jqlang.github.io/jq/download/" >&2
  echo "  stellar: cargo install --locked stellar-cli" >&2
  exit 1
fi

crate_version() {
  cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[] | select(.name=="will") | .version'
}

# Export the embedded SCSpecEntry stream from the compiled wasm as JSON.
# Writes to $1. Propagates any exporter failure to the caller.
export_spec() {
  local out="$1"
  "$STELLAR_BIN" contract bindings json \
    --wasm "$WASM_PATH" \
    --output "$out"
}

main() {
  echo "Building will contract (canonical locked release build)..."
  "${ROOT_DIR}/scripts/build-wasm.sh" will

  if [[ ! -f "$WASM_PATH" ]]; then
    echo "error: expected wasm artifact not found at $WASM_PATH" >&2
    exit 1
  fi

  local version spec_file tmp_file
  version="$(crate_version)"
  if [[ -z "$version" || "$version" == "null" ]]; then
    echo "error: could not determine will crate version" >&2
    exit 1
  fi
  spec_file="spec/will-v${version}.json"

  mkdir -p spec
  tmp_file="$(mktemp "spec/.will-v${version}.XXXXXX.json")"
  # shellcheck disable=SC2064
  trap "rm -f '$tmp_file'" EXIT

  echo "Exporting spec to $spec_file..."
  if ! export_spec "$tmp_file"; then
    echo "error: spec export failed; snapshot left untouched" >&2
    exit 1
  fi

  if [[ ! -s "$tmp_file" ]]; then
    echo "error: exporter produced no output; snapshot left untouched" >&2
    exit 1
  fi

  if [[ "$MODE" == "check" ]]; then
    if [[ ! -f "$spec_file" ]]; then
      echo "error: missing spec snapshot $spec_file" >&2
      exit 1
    fi
    if ! diff -u "$spec_file" "$tmp_file"; then
      echo "error: $spec_file is stale; run scripts/export-spec.sh" >&2
      exit 1
    fi
    echo "Spec snapshot $spec_file is up to date."
    return 0
  fi

  mv "$tmp_file" "$spec_file"
  echo "Wrote $spec_file"
}

main "$@"
