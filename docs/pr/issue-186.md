# Use the effective grace deadline everywhere (#186)

## Summary

After `request_grace_extension`, `release_inheritance` and `emergency_checkin` used the extended deadline, but
`get_time_until_deadline` and `guardian_cancel_trigger` still used only the base `grace_period_days`. At day 8 of a
7+7-day grace the query reported overdue and guardian cancellation was rejected with `GracePeriodExpired`, even though
recovery remained valid and release remained blocked. All consumers now share one effective deadline calculation.

## What changed

- **`contracts/will/src/lib.rs`**
  - `grace_deadline` now computes `grace_period_days + grace_extension_days` with checked addition (overflow surfaces as
    `ArithmeticOverflow`).
  - `get_time_until_deadline` (Triggered arm) uses `grace_deadline` converted through a new `timestamp_as_i64` helper,
    preserving the checked `i64` conversion for queries.
  - `guardian_cancel_trigger` uses `grace_deadline` instead of recomputing the base window.
  - Added `timestamp_as_i64` so both deadline-to-`i64` conversions share the same overflow policy.
- **New test module `issue_186_effective_deadline_test.rs`** — asserts that after a 7-day extension:
  - at `trigger_time + 8 days` the deadline query reports the remaining 6 days and guardian cancellation succeeds;
  - at `trigger_time + 15 days` cancellation is rejected with `GracePeriodExpired`.

## Acceptance criteria addressed

- **Triggered query includes granted extension days** — covered directly.
- **Guardian cancellation remains available throughout the effective grace window** — boundary and post-boundary tests.
- **All consumers agree at the boundary and adjacent seconds** — single `grace_deadline` helper; boundary policy
  (`now > deadline`) unchanged.

## Validation note

`cargo check --tests` reports no new diagnostics from this change; unrelated build blockers are handled in their own
PRs.

closes #186
