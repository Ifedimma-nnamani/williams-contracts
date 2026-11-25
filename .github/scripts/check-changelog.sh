#!/bin/bash
set -euo pipefail

BASE_REF="${CHANGELOG_BASE:-origin/${GITHUB_BASE_REF:-main}}"
LIB_RS=contracts/will/src/lib.rs

if ! git rev-parse --verify --quiet "$BASE_REF^{commit}" >/dev/null; then
    echo "::error::Cannot resolve base commit $BASE_REF; checkout must fetch full history (fetch-depth: 0)"
    exit 1
fi

# Compare the declaration itself. Diff context or a changed reference to the
# constant does not constitute a version bump.
base_version="$(git show "$BASE_REF:$LIB_RS" | sed -nE 's/^pub const CONTRACT_VERSION: u32 = ([0-9_]+);.*/\1/p')"
head_version="$(sed -nE 's/^pub const CONTRACT_VERSION: u32 = ([0-9_]+);.*/\1/p' "$LIB_RS")"

if [[ "${base_version//_/}" != "${head_version//_/}" ]] &&
    git diff --quiet "$BASE_REF" -- CHANGELOG.md; then
    echo "::error::CHANGELOG.md must be updated when CONTRACT_VERSION is changed in $LIB_RS"
    exit 1
fi
