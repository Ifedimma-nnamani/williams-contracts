# Fix guardian freshness module compilation (#183)

## Summary

`guardian_vote_freshness.rs` used an undeclared `expiry_days`, mixed `u32` and `u64` for the vote-expiry window, and
built test wills with a non-existent `Will::default()`. This change normalises the module on a single `u64` day type and
replaces the invalid fixtures with Env-backed wills created through the contract client.

## What changed

- **`MIN_GUARDIAN_VOTE_EXPIRY_DAYS`** is now `u64`, matching `Will::grace_period_days` and the `expiry_days` parameter of
  `storage::has_guardian_voted`.
- **`guardian_vote_expiry_days`** now returns `u64`; the comparison and both branches use the same type, so a large
  `u64` grace period is preserved rather than truncated.
- **`live_guardian_vote_weight`** binds `let expiry_days = guardian_vote_expiry_days(will)` once and passes it to
  `has_guardian_voted`, replacing the undeclared identifier.
- **Tests** build a real will with `create_will` via an Env-backed fixture instead of `Will::default()`, and a new case
  verifies that `u64::MAX` grace days survive the conversion unchanged.

## Acceptance criteria addressed

- **Module type-checks in native and WASM builds** — the three diagnostics in this module are gone (`cargo check` now
  reports only the independent blockers from #174, #175 and #184).
- **Tests use real fixtures** — no invalid default addresses/structs remain.
- **No undeclared expiry variable** — the effective window is computed once from the will.

## Validation note

Other compile errors in this checkout are owned by separate open issues (#174, #175, #184) and are deliberately left to
their own PRs.

closes #183
