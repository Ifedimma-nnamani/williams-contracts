# Reject overflowing fixed-allocation totals (#208)

## Summary

`assert_valid_allocations` accumulated fixed amounts with `saturating_add`. Two positive amounts whose mathematical sum
overflowed `i128` became `i128::MAX`, which could then pass the check against a primary balance of `i128::MAX`. The
merged-allocation helper had the same saturation problem. Both now use checked arithmetic with a typed error.

## What changed

- **`contracts/will/src/lib.rs`**
  - `assert_valid_allocations` accumulates fixed amounts with `checked_add` and panics with `ArithmeticOverflow` on
    overflow instead of saturating.
  - `merge_allocation` now takes `&Env`, sums two fixed amounts with `checked_add`, and reports the same typed error on
    overflow; its single caller was updated.
- **`contracts/will/src/issue_208_test.rs`** — added an overflowing pair (`i128::MAX` + `1`) that is rejected and an
  exactly-funded pair (`i128::MAX - 1` + `1`) that is still accepted.

## Acceptance criteria addressed

- **Overflowing fixed totals are rejected atomically** — validation fails before any state change.
- **Exactly funded totals remain accepted** — covered by the second test.
- **No saturating sum disguises overcommitment** — both the validator and the merge helper use checked arithmetic.

## Validation note

`cargo check --tests` in this environment hit a linker crash while building a `soroban-env-common` build script (a toolchain/environment fault, not a source diagnostic); earlier checks of the modified code produced no Rust errors.

closes #208
