# Decode the prior v2 will layout (#176)

## Summary

`grace_extension_days` was added to `Will` without changing `CURRENT_SCHEMA_VERSION`, leaving wills written by the
previous v2 layout (which had `keeper_bounty_token` but not the new field) undecodable. This change freezes that layout,
bumps the schema version, adds an explicit migration step and tests both active and archived records.

## What changed

- **`contracts/will/src/migration.rs`**
  - Added `legacy::WillV2` with the exact prior-v2 field set and `into_current()`, which defaults
    `grace_extension_days` to 0 while preserving every other field, the status and the original `schema_version`.
  - `decode_will` now tries `WillV2` before `WillV1`, so a raw prior-v2 XDR map no longer falls through to
    `UnsupportedSchemaVersion`.
  - `CURRENT_SCHEMA_VERSION` bumped 2 → 3; added `migrate_v2_to_v3` and wired it into `upgrade`.
  - Tests cover a prior-v2 active record (all fields preserved), an archived `Settled` record, and upgrading to v3.
- **`contracts/will/src/storage.rs`** — its mirrored `CURRENT_SCHEMA_VERSION` bumped to 3 (the compile-time assertion in
  `migrate_will_test.rs` still holds).
- **`CONTRIBUTING.md`** — version-history table updated (v2 description corrected, v3 added).
- **`CHANGELOG.md`** — recorded under Unreleased / Fixed.

## Acceptance criteria addressed

- **Prior v2 active and archived records decode** — dedicated tests for both statuses.
- **Migration preserves balances, commitments, guardians and payout settings** — the conversion copies each field
  verbatim and only defaults the newly added field; the test asserts the full set.
- **Schema/version documentation reflects the field addition** — CONTRIBUTING table, spec version constant and
  CHANGELOG updated.

## Validation note

`cargo check` and `cargo check --tests` report no new diagnostics from this change; the remaining errors are the
independent blockers owned by #174, #175, #183 and #184.

closes #176
