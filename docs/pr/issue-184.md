# Restore `InvalidDelegate` with a non-colliding error code (#184)

## Summary

`set_delegate` referenced `WillError::InvalidDelegate`, but the enum did not define it, so the crate could not compile.
Published metadata assigned code 58 to `InvalidDelegate`, while `errors.rs` already assigns 58 to
`UnclaimedHashedBeneficiary`. This change restores the variant with the next free code (59), keeps the archival error's
established meaning intact, and reconciles the SDK-facing metadata.

## What changed

- **`contracts/will/src/errors.rs`** — added `InvalidDelegate = 59` with documentation explaining the historical 58
  collision and why the existing meaning is preserved.
- **`spec/will-v1.3.0.json`** — corrected the error table: 58 is now `UnclaimedHashedBeneficiary` (matching the compiled
  enum) and `InvalidDelegate` is 59.
- **`README.md`** — updated the delegate-role note to quote the new code and state the 58 ownership explicitly.
- **`contracts/will/src/issue_184_test.rs`** — added a test asserting both codes are stable and distinct.

## Acceptance criteria addressed

- **Every referenced variant exists** — the `InvalidDelegate` compile error is gone.
- **Delegate and archival errors have distinct stable codes** — 59 vs 58, with a regression assertion.
- **SDK-facing tables match the compiled enum** — spec JSON and README reconciled.

## Validation note

`cargo check` still reports the independent blockers from #174, #175 and #183, each handled in its own PR. No unrelated
source was changed.

closes #184
