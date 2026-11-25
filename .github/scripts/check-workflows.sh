#!/usr/bin/env bash
# Statically validate maintained GitHub workflow definitions.
#
# Enumerates .github/workflows/*.yml deterministically and runs actionlint
# against them. actionlint rejects invalid expression contexts, unsupported
# workflow keys and unknown action inputs before GitHub ever executes the
# workflow. It also validates embedded shell, but only as a supplementary
# check; ShellCheck remains the authoritative shell policy (see
# scripts-check.yml) so this does not duplicate or weaken it.
#
# Usage:
#   .github/scripts/check-workflows.sh
#
# Requires actionlint on PATH. CI installs the pinned version; locally install
# the same release (see ACTIONLINT_VERSION in .github/workflows/test.yml).
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
workflows_dir="$repo_root/.github/workflows"

if ! command -v actionlint >/dev/null 2>&1; then
  echo "error: actionlint not found on PATH" >&2
  echo "install the pinned release referenced by ACTIONLINT_VERSION in .github/workflows/test.yml" >&2
  exit 1
fi

# Deterministic enumeration: sorted, only maintained .yml workflow files.
mapfile -t workflows < <(find "$workflows_dir" -maxdepth 1 -type f -name '*.yml' | LC_ALL=C sort)

if [ "${#workflows[@]}" -eq 0 ]; then
  echo "error: no workflow files found under $workflows_dir" >&2
  exit 1
fi

echo "Validating ${#workflows[@]} workflow file(s):"
for wf in "${workflows[@]}"; do
  echo "  ${wf#"$repo_root"/}"
done

# -no-color keeps output stable for logs; actionlint exits non-zero on any
# invalid expression context, unsupported key or bad action input.
actionlint -no-color "${workflows[@]}"

echo "workflow validation passed"
