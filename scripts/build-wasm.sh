#!/usr/bin/env bash
# Canonical locked WASM build for all artifact-producing workflows.
#
# This is the single source of truth for how the deployable WASM artifact is
# built. Size checks, spec export, testnet deployment, sandbox deployment and
# the justfile all invoke this script so that the bytecode measured/validated
# is built under the same declared inputs as the artifact that is deployed.
#
# Canonical build settings:
#   - package:      will (explicit)
#   - target:       wasm32-unknown-unknown (explicit)
#   - profile:      release (explicit)
#   - lockfile:     enforced via --locked (a required lockfile update fails)
#   - SDK flags:    SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1
#
# Usage:
#   scripts/build-wasm.sh            # build the canonical artifact
#   scripts/build-wasm.sh --print    # print the canonical artifact path only
#
# Environment overrides (kept minimal and explicit):
#   WASM_PACKAGE   package name to build (default: will)
#   WASM_TARGET    build target (default: wasm32-unknown-unknown)
#   WASM_PROFILE   build profile (default: release)
#   CARGO          cargo binary (default: cargo)
#
# The artifact is written to the standard path:
#   target/<target>/<profile>/<package>.wasm

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# --- Canonical build settings -------------------------------------------------

WASM_PACKAGE="${WASM_PACKAGE:-will}"
WASM_TARGET="${WASM_TARGET:-wasm32-unknown-unknown}"
WASM_PROFILE="${WASM_PROFILE:-release}"
CARGO="${CARGO:-cargo}"

# Shared SDK build flags. These must be identical across every entrypoint so
# that spec shaking is applied consistently to the measured/deployed artifact.
export SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1

# Standard artifact path derived from the canonical settings.
WASM_ARTIFACT="${REPO_ROOT}/target/${WASM_TARGET}/${WASM_PROFILE}/${WASM_PACKAGE}.wasm"

# --- Helpers ------------------------------------------------------------------

usage() {
  cat <<'EOF'
Usage: scripts/build-wasm.sh [--print] [--help]

Builds the canonical locked WASM artifact used by all workflows.

Options:
  --print   Print the canonical artifact path and exit (no build).
  --help    Show this help text.

Environment:
  WASM_PACKAGE   Package to build (default: will)
  WASM_TARGET    Build target (default: wasm32-unknown-unknown)
  WASM_PROFILE   Build profile (default: release)
  CARGO          Cargo binary (default: cargo)
EOF
}

log() {
  printf '[build-wasm] %s\n' "$*" >&2
}

# --- Argument parsing ---------------------------------------------------------

PRINT_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --print)
      PRINT_ONLY=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      log "unknown argument: $1"
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "${PRINT_ONLY}" -eq 1 ]]; then
  printf '%s\n' "${WASM_ARTIFACT}"
  exit 0
fi

# --- Build --------------------------------------------------------------------

cd "${REPO_ROOT}"

log "package=${WASM_PACKAGE} target=${WASM_TARGET} profile=${WASM_PROFILE}"
log "SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=${SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2}"
log "enforcing --locked (a required lockfile update will fail this build)"

# --locked ensures the committed Cargo.lock is authoritative: if the manifest
# requires a lockfile update, cargo errors out instead of silently resolving.
"${CARGO}" build \
  --locked \
  --package "${WASM_PACKAGE}" \
  --target "${WASM_TARGET}" \
  --profile "${WASM_PROFILE}"

if [[ ! -f "${WASM_ARTIFACT}" ]]; then
  log "expected artifact not found: ${WASM_ARTIFACT}"
  exit 1
fi

log "artifact: ${WASM_ARTIFACT}"
printf '%s\n' "${WASM_ARTIFACT}"