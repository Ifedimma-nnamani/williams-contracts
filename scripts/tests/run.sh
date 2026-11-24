#!/usr/bin/env bash
# Offline command-stub test suite for shell tools.
#
# Runs each case in an isolated temporary directory with a minimal PATH that
# contains only stub executables. No network, real identity, Docker daemon or
# deployment is required. Stub state and temporary artifacts are removed after
# every case, including on failure.
#
# Usage: scripts/tests/run.sh
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)

PASS=0
FAIL=0
FAILED_CASES=()

log() { printf '%s\n' "$*" >&2; }

# --- per-case isolation -----------------------------------------------------

CASE_DIR=""

begin_case() {
  CASE_DIR=$(mktemp -d "${TMPDIR:-/tmp}/shell-stub-case.XXXXXX")
  mkdir -p "${CASE_DIR}/bin" "${CASE_DIR}/work" "${CASE_DIR}/calls"
}

end_case() {
  if [ -n "${CASE_DIR}" ] && [ -d "${CASE_DIR}" ]; then
    rm -rf "${CASE_DIR}"
  fi
  CASE_DIR=""
}

# install_stub <name> <exit-code> [stdout-text]
# Creates a stub executable that records its invocation and exits with the
# given code. Invocations are appended to ${CASE_DIR}/calls/<name>.log.
install_stub() {
  local name=$1 code=$2 out=${3:-}
  local path="${CASE_DIR}/bin/${name}"
  {
    printf '#!/usr/bin/env bash\n'
    printf 'printf "%%s\\n" "$*" >> "%s/calls/%s.log"\n' "${CASE_DIR}" "${name}"
    if [ -n "${out}" ]; then
      printf 'printf "%%s\\n" %q\n' "${out}"
    fi
    printf 'exit %s\n' "${code}"
  } > "${path}"
  chmod +x "${path}"
}

# run_in_case <command...> — runs with minimal PATH and isolated HOME.
run_in_case() {
  ( cd "${CASE_DIR}/work" && \
    PATH="${CASE_DIR}/bin:/usr/bin:/bin" \
    HOME="${CASE_DIR}/work" \
    TMPDIR="${CASE_DIR}/work" \
    "$@" )
}

# assert_exit <expected> <actual> <label>
assert_exit() {
  local expected=$1 actual=$2 label=$3
  if [ "${expected}" -ne "${actual}" ]; then
    log "  FAIL: ${label}: expected exit ${expected}, got ${actual}"
    return 1
  fi
}

# assert_calls <stub> <expected-lines...> — verifies recorded invocation order.
assert_calls() {
  local stub=$1; shift
  local log_file="${CASE_DIR}/calls/${stub}.log"
  local actual expected
  actual=$(cat "${log_file}" 2>/dev/null || true)
  expected=$(printf '%s\n' "$@")
  if [ "${actual}" != "${expected}" ]; then
    log "  FAIL: unexpected ${stub} invocations"
    log "    expected: ${expected//$'\n'/ | }"
    log "    actual:   ${actual//$'\n'/ | }"
    return 1
  fi
}

# assert_no_calls <stub>
assert_no_calls() {
  local stub=$1
  if [ -s "${CASE_DIR}/calls/${stub}.log" ]; then
    log "  FAIL: expected no ${stub} invocations"
    return 1
  fi
}

# assert_clean <path> — verifies a temporary artifact was removed.
assert_clean() {
  local path=$1
  if [ -e "${path}" ]; then
    log "  FAIL: temporary artifact not cleaned: ${path}"
    return 1
  fi
}

# --- case runner ------------------------------------------------------------

run_case() {
  local name=$1 fn=$2
  begin_case
  local rc=0
  ( set -e; "${fn}" ) || rc=$?
  end_case
  if [ "${rc}" -eq 0 ]; then
    PASS=$((PASS + 1))
    log "ok   ${name}"
  else
    FAIL=$((FAIL + 1))
    FAILED_CASES+=("${name}")
    log "FAIL ${name}"
  fi
}

# --- export-spec.sh cases ---------------------------------------------------

export_spec_success() {
  install_stub stellar 0 'exported-spec'
  local rc=0
  run_in_case env STELLAR_BIN="${CASE_DIR}/bin/stellar" \
    bash "${REPO_ROOT}/scripts/export-spec.sh" >/dev/null 2>&1 || rc=$?
  assert_exit 0 "${rc}" "export-spec success exit"
  if [ ! -s "${CASE_DIR}/calls/stellar.log" ]; then
    log "  FAIL: stellar stub was not invoked"
    return 1
  fi
}

export_spec_failure() {
  install_stub stellar 7
  local rc=0
  run_in_case env STELLAR_BIN="${CASE_DIR}/bin/stellar" \
    bash "${REPO_ROOT}/scripts/export-spec.sh" >/dev/null 2>&1 || rc=$?
  if [ "${rc}" -eq 0 ]; then
    log "  FAIL: export-spec ignored stellar failure"
    return 1
  fi
}

# --- check_resource_costs.sh cases ------------------------------------------

resource_costs_success() {
  install_stub cargo 0
  install_stub stellar 0 'cpu: 1\nmem: 1'
  local rc=0
  run_in_case env STELLAR_BIN="${CASE_DIR}/bin/stellar" \
    bash "${REPO_ROOT}/scripts/check_resource_costs.sh" >/dev/null 2>&1 || rc=$?
  assert_exit 0 "${rc}" "resource-costs success exit"
}

resource_costs_failure() {
  install_stub cargo 0
  install_stub stellar 9
  local rc=0
  run_in_case env STELLAR_BIN="${CASE_DIR}/bin/stellar" \
    bash "${REPO_ROOT}/scripts/check_resource_costs.sh" >/dev/null 2>&1 || rc=$?
  if [ "${rc}" -eq 0 ]; then
    log "  FAIL: check_resource_costs ignored stellar failure"
    return 1
  fi
}

# --- harness self-tests -----------------------------------------------------

# Proves that unexpected command order fails the suite.
self_test_order_detection() {
  install_stub alpha 0
  install_stub beta 0
  run_in_case bash -c '"${0}/bin/beta"; "${0}/bin/alpha"' "${CASE_DIR}" >/dev/null 2>&1 || true
  if assert_calls alpha beta >/dev/null 2>&1; then
    log "  FAIL: order self-test did not detect wrong order"
    return 1
  fi
}

# Proves that an ignored exit code fails the suite.
self_test_exit_detection() {
  install_stub gamma 3
  local rc=0
  run_in_case bash -c '"${0}/bin/gamma" || true' "${CASE_DIR}" >/dev/null 2>&1 || rc=$?
  if assert_exit 3 "${rc}" "self-test exit" >/dev/null 2>&1; then
    log "  FAIL: exit self-test did not detect ignored failure"
    return 1
  fi
}

# Proves that leftover artifacts are detected.
self_test_cleanup_detection() {
  touch "${CASE_DIR}/work/leftover"
  if assert_clean "${CASE_DIR}/work/leftover" >/dev/null 2>&1; then
    log "  FAIL: cleanup self-test did not detect leftover artifact"
    return 1
  fi
}

# --- main -------------------------------------------------------------------

main() {
  run_case "self-test: order detection" self_test_order_detection
  run_case "self-test: exit detection" self_test_exit_detection
  run_case "self-test: cleanup detection" self_test_cleanup_detection
  run_case "export-spec: success" export_spec_success
  run_case "export-spec: stellar failure" export_spec_failure
  run_case "check_resource_costs: success" resource_costs_success
  run_case "check_resource_costs: stellar failure" resource_costs_failure

  log ""
  log "passed: ${PASS}, failed: ${FAIL}"
  if [ "${FAIL}" -ne 0 ]; then
    log "failed cases: ${FAILED_CASES[*]}"
    return 1
  fi
}

main "$@"
