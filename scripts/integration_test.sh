#!/usr/bin/env bash
#
# Integration test for the compiled WillContract .wasm artifact.
#
# Unlike `cargo test`, which runs the contract logic in-process against
# soroban_sdk's mock `Env`, this script builds the real wasm binary, deploys
# it to a local Soroban network, and drives a handful of core lifecycle calls
# through `stellar contract invoke` — the same code path a real dApp uses.
# This catches build- or host-environment-specific issues (wasm size limits,
# host function availability, XDR encoding mismatches) that the fast unit
# test suite cannot, since it never touches the compiled artifact or a real
# host.
#
# Requirements: `stellar` CLI (>= 22.0.0), `docker` (for the local network),
# and `jq`.
#
# Usage:
#   ./scripts/integration_test.sh
#
# See CONTRIBUTING.md for how to run this locally and what it checks.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

NETWORK="local"
WASM_PATH="target/wasm32v1-none/release/will.wasm"

# Isolate all stellar-cli state (keys, network config) for this run so we never
# touch the user's existing identities or network aliases, and so parallel runs
# do not interfere with each other.
CLI_CONFIG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/will-integration.XXXXXX")"
export STELLAR_CONFIG_DIR="$CLI_CONFIG_DIR"
export HOME="$CLI_CONFIG_DIR"

# Unique per-run aliases so we never collide with (or overwrite) preexisting
# identities or network aliases.
RUN_SUFFIX="$$"
IDENTITY="integration-test-$RUN_SUFFIX"
BENEFICIARY="beneficiary-tmp-$RUN_SUFFIX"

# Ownership tracking: only clean up resources this run actually created.
network_started=0
identity_created=0
beneficiary_created=0

log() {
  printf '\n\033[1;34m[integration]\033[0m %s\n' "$1"
}

# Log a lifecycle stage boundary so a failing create/check-in/cancel step is
# identifiable in CI logs.
stage() {
  printf '\n\033[1;36m[integration][stage]\033[0m %s\n' "$1"
}

# Run a lifecycle invocation, reporting the stage name and exit status on
# failure. The script runs under `set -e`, so a nonzero status here aborts the
# whole run and propagates to the caller (e.g. the CI workflow job).
run_stage() {
  local name="$1"
  shift
  stage "BEGIN $name"
  if ! "$@"; then
    local status=$?
    printf '\033[1;31m[integration][stage]\033[0m FAILED %s (exit %d)\n' "$name" "$status" >&2
    exit "$status"
  fi
  stage "OK $name"
}
=======

cleanup() {
  local status=$?
  if [ "$beneficiary_created" -eq 1 ]; then
    log "Removing test beneficiary identity $BENEFICIARY"
    stellar keys remove "$BENEFICIARY" >/dev/null 2>&1 || true
  fi
  if [ "$identity_created" -eq 1 ]; then
    log "Removing test identity $IDENTITY"
    stellar keys remove "$IDENTITY" >/dev/null 2>&1 || true
  fi
  if [ "$network_started" -eq 1 ]; then
    log "Stopping local Soroban network"
    stellar network stop "$NETWORK" >/dev/null 2>&1 || true
  fi
  if [ -n "${CLI_CONFIG_DIR:-}" ] && [ -d "$CLI_CONFIG_DIR" ]; then
    rm -rf "$CLI_CONFIG_DIR"
  fi
  exit "$status"
}
trap cleanup EXIT

log "Building contract for wasm32v1-none (release)"
cargo build --package will --release --target wasm32v1-none

if [ ! -f "$WASM_PATH" ]; then
  echo "Expected wasm artifact not found at $WASM_PATH" >&2
  exit 1
fi

log "Starting local Soroban standalone network"
stellar network start "$NETWORK" --limits testnet >/dev/null 2>&1 || \
  stellar network start "$NETWORK"
network_started=1

log "Generating a funded identity for the test run"
stellar keys generate "$IDENTITY" --network "$NETWORK" --fund
identity_created=1

OWNER_ADDR="$(stellar keys address "$IDENTITY")"

log "Deploying will.wasm to $NETWORK"
CONTRACT_ID="$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$IDENTITY" \
  --network "$NETWORK" | tail -n 1)"
echo "Deployed contract id: $CONTRACT_ID"

log "Deploying a test SEP-41 token (native XLM stand-in) to fund the will"
TOKEN_ID="$(stellar contract asset deploy \
  --asset native \
  --source "$IDENTITY" \
  --network "$NETWORK" 2>/dev/null | tail -n 1 || echo "")"

if [ -z "$TOKEN_ID" ]; then
  echo "Could not resolve a native asset contract id; aborting" >&2
  exit 1
fi

stellar keys generate "$BENEFICIARY" --network "$NETWORK" --fund
beneficiary_created=1
BENEFICIARY_ADDR="$(stellar keys address "$BENEFICIARY")"

log "create_will: locking 1000 stroops for one beneficiary, 1-day check-in / 1-day grace"
stage "BEGIN create_will"
if ! WILL_ID="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- create_will \
  --owner "$OWNER_ADDR" \
  --tokens "[[\"$TOKEN_ID\",\"1000\"]]" \
  --beneficiaries "[{\"address\":\"$BENEFICIARY_ADDR\",\"basis_points\":10000}]" \
  --checkin_period_days 1 \
  --grace_period_days 1 \
  --guardians "[]" \
  --guardian_threshold 0 \
  --keeper_bounty_bps "null" \
  --confirmation_delay_seconds 0)"; then
  status=$?
  printf '\033[1;31m[integration][stage]\033[0m FAILED create_will (exit %d)\n' "$status" >&2
  exit "$status"
fi
stage "OK create_will"
echo "Created will id: $WILL_ID"

log "get_will: reading back the will we just created"
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- get_will --will_id "$WILL_ID"

log "get_will_status: expect Active"
STATUS="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- get_will_status --will_id "$WILL_ID")"
echo "Status: $STATUS"
if [[ "$STATUS" != *"Active"* ]]; then
  echo "Expected Active status after create_will, got: $STATUS" >&2
  exit 1
fi

log "check_in: resetting the deadline"
run_stage check_in \
  stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- check_in --will_id "$WILL_ID" --owner "$OWNER_ADDR"

log "cancel_will: withdrawing the balance and closing the will"
run_stage cancel_will \
  stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- cancel_will --will_id "$WILL_ID" --owner "$OWNER_ADDR"

log "get_will_status: expect Cancelled"
FINAL_STATUS="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$IDENTITY" \
  --network "$NETWORK" \
  -- get_will_status --will_id "$WILL_ID")"
echo "Status: $FINAL_STATUS"
if [[ "$FINAL_STATUS" != *"Cancelled"* ]]; then
  echo "Expected Cancelled status after cancel_will, got: $FINAL_STATUS" >&2
  exit 1
fi

log "All lifecycle invocations against the real wasm artifact succeeded."
