#!/usr/bin/env bash
#
# deploy-testnet.sh — build and deploy the `will` contract to Stellar
# Testnet, then record the result in deployments/testnet.json.
#
# Prerequisites:
#   - stellar-cli >= 22.0.0 (`cargo install --locked stellar-cli`)
#   - rustup target wasm32v1-none (`rustup target add wasm32v1-none`)
#   - A funded testnet identity already configured in stellar-cli, e.g.:
#       stellar keys generate deployer --network testnet --fund
#     or, to import an existing key:
#       stellar keys add deployer --secret-key
#
# Configuration (environment variables):
#   DEPLOY_IDENTITY   Required. stellar-cli identity name used to sign and
#                      pay for the deployment. Must already exist and be
#                      funded on testnet.
#   NETWORK            Optional. Soroban network alias. Default: testnet
#                      Must be one of: testnet, futurenet, mainnet, sandbox.
#   RPC_URL             Optional. Soroban RPC endpoint. Must be consistent
#                      with NETWORK (see NETWORK_RPC_DEFAULTS below).
#                        Default: https://soroban-testnet.stellar.org
#   OUTPUT_FILE         Optional. Deployment artifact path. Defaults to
#                      deployments/<NETWORK>.json, so a non-testnet run can
#                      never overwrite deployments/testnet.json.
#
# Usage:
#   DEPLOY_IDENTITY=deployer ./scripts/deploy-testnet.sh
#
# After it finishes, review and commit the updated deployment artifact
# (deployments/testnet.json for the default network)
# — see CONTRIBUTING.md#updating-deploymentstestnetjson-after-a-redeploy.
#
# Version consistency (#501): CONTRACT_VERSION being correct in source is not
# the same thing as the *deployed* contract actually running that source. A
# stale local build, a wrong --wasm path, or deploying from the wrong git
# worktree would all still "succeed" as far as `stellar contract deploy` is
# concerned. So after deploying, this script calls the freshly deployed
# contract's own `get_contract_version` over RPC and fails loudly, before
# writing deployments/testnet.json, if it disagrees with the source
# CONTRACT_VERSION that was supposedly just built and deployed.
#
# Deployment artifact (#87): deployments/testnet.json records the concrete
# contract ID, the network, an ISO-8601 deployment timestamp, and the SHA-256
# hash of the deployed WASM bytecode so the artifact can be verified against
# the current release build.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

: "${DEPLOY_IDENTITY:?Set DEPLOY_IDENTITY to a funded stellar-cli identity name, e.g. DEPLOY_IDENTITY=deployer ./scripts/deploy-testnet.sh}"
NETWORK="${NETWORK:-testnet}"

# Known networks and their canonical RPC endpoints. Keeping this mapping in
# one place lets us reject unknown networks and catch a NETWORK/RPC_URL
# mismatch before any transaction is submitted.
declare -A NETWORK_RPC_DEFAULTS=(
  [testnet]="https://soroban-testnet.stellar.org"
  [futurenet]="https://rpc-futurenet.stellar.org"
  [mainnet]="https://soroban-rpc.mainnet.stellar.gateway.fm"
  [sandbox]="http://localhost:8000/soroban/rpc"
)

if [[ -z "${NETWORK_RPC_DEFAULTS[$NETWORK]+set}" ]]; then
  echo "error: unsupported NETWORK '$NETWORK'. Supported networks: ${!NETWORK_RPC_DEFAULTS[*]}" >&2
  exit 1
fi

RPC_URL="${RPC_URL:-${NETWORK_RPC_DEFAULTS[$NETWORK]}}"
WASM_PATH="target/wasm32v1-none/release/will.wasm"
# Network-specific artifact path: a non-testnet run writes its own file and
# can never silently overwrite deployments/testnet.json.
OUTPUT_FILE="${OUTPUT_FILE:-deployments/${NETWORK}.json}"
LIB_RS="contracts/will/src/lib.rs"

# Guard against an explicit OUTPUT_FILE that would clobber the testnet record
# from a non-testnet run.
if [[ "$NETWORK" != "testnet" && "$OUTPUT_FILE" == "deployments/testnet.json" ]]; then
  echo "error: refusing to write a '$NETWORK' deployment to $OUTPUT_FILE (the testnet record)." >&2
  echo "error: unset OUTPUT_FILE to use the default deployments/${NETWORK}.json." >&2
  exit 1
fi

command -v stellar >/dev/null 2>&1 || {
  echo "error: stellar-cli not found on PATH. Install it with:" >&2
  echo "  cargo install --locked stellar-cli" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || {
  echo "error: jq not found on PATH. It is required to serialize the deployment manifest safely." >&2
  echo "  Install it with your package manager, e.g. 'apt-get install jq' or 'brew install jq'." >&2
  exit 1
}
=======

echo "==> Checking CONTRACT_VERSION is consistent with Cargo.toml before building"
"$ROOT_DIR/.github/scripts/check-contract-version.sh"

echo "==> Building contract for wasm32v1-none (release)"
cargo build --package will --release --target wasm32v1-none

if [[ ! -f "$WASM_PATH" ]]; then
  echo "error: expected wasm artifact at $WASM_PATH, but it was not produced" >&2
  exit 1
fi

echo "==> Computing SHA-256 hash of deployed WASM bytecode"
if command -v sha256sum >/dev/null 2>&1; then
  WASM_HASH="$(sha256sum "$WASM_PATH" | awk '{print $1}')"
else
  WASM_HASH="$(shasum -a 256 "$WASM_PATH" | awk '{print $1}')"
fi
if [[ -z "$WASM_HASH" ]]; then
  echo "error: failed to compute SHA-256 hash of $WASM_PATH" >&2
  exit 1
fi

echo "==> Deploying to '$NETWORK' as identity '$DEPLOY_IDENTITY'"
CONTRACT_ID="$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$DEPLOY_IDENTITY" \
  --network "$NETWORK" \
  --rpc-url "$RPC_URL")"

if [[ -z "$CONTRACT_ID" ]]; then
  echo "error: stellar contract deploy did not return a contract id" >&2
  exit 1
fi

DEPLOYED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

echo "==> Deployed WillContract: $CONTRACT_ID at $DEPLOYED_AT (wasm sha256: $WASM_HASH)"

echo "==> Verifying deployed contract's get_contract_version matches source CONTRACT_VERSION"
EXPECTED_VERSION_RAW="$(grep -E '^pub const CONTRACT_VERSION: u32 = ' "$LIB_RS" | sed -E 's/^pub const CONTRACT_VERSION: u32 = ([0-9_]+);.*/\1/')"
EXPECTED_VERSION="${EXPECTED_VERSION_RAW//_/}"

ONCHAIN_VERSION_RAW="$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$DEPLOY_IDENTITY" \
  --network "$NETWORK" \
  --rpc-url "$RPC_URL" \
  -- get_contract_version)"
# stellar-cli prints scalar results as bare JSON (a plain number here); strip
# any surrounding quotes/whitespace defensively before comparing.
ONCHAIN_VERSION="$(echo "$ONCHAIN_VERSION_RAW" | tr -d '"[:space:]')"

if [[ "$ONCHAIN_VERSION" != "$EXPECTED_VERSION" ]]; then
  echo "error: deployed contract $CONTRACT_ID reports get_contract_version() = $ONCHAIN_VERSION, but source CONTRACT_VERSION is $EXPECTED_VERSION" >&2
  echo "error: the deployment is inconsistent with the source tree -- do NOT record this in $OUTPUT_FILE. Check for a stale build or wrong --wasm artifact." >&2
  exit 1
fi
echo "==> OK: on-chain version $ONCHAIN_VERSION matches source CONTRACT_VERSION"

echo "==> Verifying deployed bytecode hash matches local WASM SHA-256"
# `stellar contract fetch` retrieves the deployed WASM bytecode for the
# contract id; hashing it lets us assert the on-chain instance matches the
# audited local build artifact before recording the deployment.
FETCHED_WASM="$(mktemp)"
trap 'rm -f "$FETCHED_WASM"' EXIT
stellar contract fetch \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --rpc-url "$RPC_URL" \
  --out-file "$FETCHED_WASM"
if command -v sha256sum >/dev/null 2>&1; then
  DEPLOYED_WASM_HASH="$(sha256sum "$FETCHED_WASM" | awk '{print $1}')"
else
  DEPLOYED_WASM_HASH="$(shasum -a 256 "$FETCHED_WASM" | awk '{print $1}')"
fi
echo "==> Deployed WASM SHA-256: $DEPLOYED_WASM_HASH"

if [[ "$DEPLOYED_WASM_HASH" != "$WASM_HASH" ]]; then
  echo "error: deployed bytecode hash $DEPLOYED_WASM_HASH does not match local WASM hash $WASM_HASH" >&2
  echo "error: the deployed contract does not match the audited local build -- do NOT record this in $OUTPUT_FILE." >&2
  exit 1
fi
echo "==> OK: deployed bytecode hash matches local WASM SHA-256"

# Serialize the manifest with jq so every value is properly JSON-escaped, then
# validate it and atomically replace the destination. A failure at any point
# (serialization, validation, interrupted write) leaves the existing
# OUTPUT_FILE bytes untouched.
OUTPUT_DIR="$(dirname "$OUTPUT_FILE")"
mkdir -p "$OUTPUT_DIR"
TMP_MANIFEST="$(mktemp "${OUTPUT_DIR}/.deploy-manifest.XXXXXX")"
trap 'rm -f "$FETCHED_WASM" "$TMP_MANIFEST"' EXIT

if ! jq -n \
  --arg contractId "$CONTRACT_ID" \
  --arg network "$NETWORK" \
  --arg rpcUrl "$RPC_URL" \
  --arg identity "$DEPLOY_IDENTITY" \
  --arg deployedAt "$DEPLOYED_AT" \
  --arg wasmHash "$WASM_HASH" \
  '{
    WillContract: $contractId,
    network: $network,
    rpcUrl: $rpcUrl,
    identity: $identity,
    deployedAt: $deployedAt,
    wasmHash: $wasmHash
  }' > "$TMP_MANIFEST"; then
  echo "error: failed to serialize deployment manifest; $OUTPUT_FILE left unchanged." >&2
  exit 1
fi

# Validate required fields are present, string-typed and non-empty before the
# manifest is allowed to replace the prior record.
if ! jq -e '
  (type == "object")
  and ([.WillContract, .network, .rpcUrl, .identity, .deployedAt, .wasmHash]
       | all(type == "string" and length > 0))
' "$TMP_MANIFEST" >/dev/null; then
  echo "error: generated deployment manifest failed validation; $OUTPUT_FILE left unchanged." >&2
  exit 1
fi

# Atomic replace: mv within the same directory is a rename, so readers see
# either the old complete file or the new complete file, never a partial one.
if ! mv -f "$TMP_MANIFEST" "$OUTPUT_FILE"; then
  echo "error: failed to atomically replace $OUTPUT_FILE; prior record left unchanged." >&2
  exit 1
fi
# The temp file no longer exists after a successful mv; disarm the cleanup.
trap 'rm -f "$FETCHED_WASM"' EXIT

echo "==> Wrote $OUTPUT_FILE"
echo
echo "Next steps:"
echo "  1. Review the diff: git diff -- $OUTPUT_FILE"
echo "  2. Commit it on its own: git add $OUTPUT_FILE && git commit -m 'chore: record $NETWORK deployment $CONTRACT_ID'"
echo "  3. See CONTRIBUTING.md#updating-deploymentstestnetjson-after-a-redeploy for the full checklist."
