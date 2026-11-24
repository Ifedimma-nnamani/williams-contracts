# Enforce MAX_TOKENS on the merged token union (#180)

## Summary

`merge_wills` combined both balance maps without checking the resulting number of distinct tokens. Two individually
valid 10-token wills with one shared primary token but disjoint secondaries produced a 19-token survivor, which
`assert_release_preconditions` rejects — trapping the merged estate. The union is now validated before any mutation.

## What changed

- **`contracts/will/src/lib.rs` — `merge_wills`**
  - Computes the distinct address union of `will_a.balances` and `will_b.balances` before merging balances/guardians.
  - Rejects with `MergeWouldExceedLimits` and a diagnostic log when the union exceeds `MAX_TOKENS`.
  - Counts actual distinct addresses, so overlapping secondary sets still merge when the union fits.
- **`contracts/will/src/issue_358_test.rs`** — added two regression tests: a 19-token union is rejected atomically and
  the survivor keeps its original 10 tokens; fully overlapping sets (union of 10) still merge successfully.

## Acceptance criteria addressed

- **19-token union rejected atomically** — check runs before any state write.
- **Overlapping sets with a union of exactly 10 remain allowed** — covered.
- **Successful merges remain eligible for normal release size checks** — the union bound equals `MAX_TOKENS`, the same
  cap release enforces.

## Validation note

No new diagnostics in `cargo check --tests`; remaining errors belong to independent issues with their own PRs.

closes #180
