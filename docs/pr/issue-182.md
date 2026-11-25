# Establish a positive guardian threshold when enabling a committee (#182)

## Summary

`create_will` normalises the threshold to 0 for an empty guardian list, but `update_guardians` and the guardian branch
of `update_will_settings` only rejected thresholds *above* the new list length. Adding guardians to a previously
guardian-free will therefore left `guardian_threshold = 0`, so the first guardian's vote satisfied quorum alone.

## What changed

- **`contracts/will/src/lib.rs`**
  - `update_guardians`: when installing a non-empty list over `guardian_threshold == 0`, the threshold is set to a
    simple majority (`len / 2 + 1`); thresholds above the new length are still rejected.
  - `update_will_settings`: identical policy for its `guardians` branch.
  - Disabling guardians (empty list) still stores threshold 0 and continues to disable the mechanism.
- **`contracts/will/src/update_guardians_threshold_test.rs`** — new tests for both entry points: enabling a 3- or
  2-guardian committee yields threshold 2, a single vote does not trigger the will, and the second vote reaches quorum.

## Acceptance criteria addressed

- **No non-empty guardian list is persisted with threshold zero** — both enablement paths now assign a positive value.
- **Both unweighted update routes have a clear enablement policy** — documented simple-majority default.
- **Disabling and re-enabling preserves the chosen safety rule** — re-enabling re-applies the same majority rule; existing
  positive thresholds are preserved (shrink validation unchanged).

## Validation note

No new diagnostics in `cargo check --tests`; remaining errors belong to independent issues.

closes #182
