# Restore the trigger cooldown type/constant integration (#174)

## Summary

`trigger_will` read `will.trigger_cooldown_until` and `emergency_checkin` wrote it using `TRIGGER_COOLDOWN_SECONDS`, but
neither the field nor the constant existed, so every build failed. This change finishes the integration with explicit
defaults for all creation and migration paths, keeps the cooldown deadline distinct from the check-in deadline, and
updates the schema and regression coverage.

## What changed

- **`contracts/will/src/types.rs`** — added `trigger_cooldown_until: u64` to `Will`, documented as the re-trigger
  cooldown deadline (distinct from `last_checkin + checkin_period_days`).
- **`contracts/will/src/lib.rs`**
  - Added `TRIGGER_COOLDOWN_SECONDS = SECONDS_PER_DAY` (24 h, matching issue #104).
  - `emergency_checkin` now writes the deadline through `checked_add_timestamp`, so extreme ledger timestamps cannot
    wrap.
  - All four creation paths (`create_will`, `clone_will`, batch create, `split_will`) initialise the field to `0`, i.e.
    no pending cooldown.
- **`contracts/will/src/migration.rs`** — `WillV1::into_current` defaults the field to `0`, keeping legacy records
  triggerable (the cooldown is not retroactively applied).
- **`spec/will-v1.3.0.json`** — added the field to the current `Will` struct listing; the historical v0.1.0 spec is
  intentionally unchanged.
- **`contracts/will/src/issue_104_test.rs`** — the cooldown test now asserts the stored `trigger_cooldown_until` equals
  `checkin_at + 1 day`, that `last_checkin + 90 days` is the *separate* check-in deadline, and that the cooldown expires
  before it.
- **`CHANGELOG.md`** — recorded the fix under Unreleased / Fixed.

## Acceptance criteria addressed

- **No unresolved cooldown names** — the four `cargo check` diagnostics for this issue are gone.
- **Every creation/migration path supplies a default** — 0 on create/clone/batch/split and legacy migration.
- **Cooldown tests distinguish the two deadlines** — explicit assertions added.

## Validation note

`cargo check` on this checkout still reports independent blockers owned by #175, #183 and #184, each handled in its own
PR. No unrelated source was changed.

closes #174
