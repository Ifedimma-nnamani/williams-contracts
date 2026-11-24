# Centralize schema-compatible will decoding in storage (#177)

## Summary

`storage::load_will` decoded raw entries through `migration::decode_will`, but five other reads of `DataKey::Will`
decoded directly as the current `Will`. A legacy record could therefore load successfully and then fail inside
`save_will` (before the migrated value was persisted) or be silently skipped by statistics scans. All will-entry reads
now share one decoder.

## What changed

- **`contracts/will/src/storage.rs`** — added `decode_will_entry(env, id)` which reads the raw `Val`, returns `None` for
  a missing key, decodes through `migration::decode_will` for a present key, and panics with `UnsupportedSchemaVersion`
  when the layout is unknown. Replaced all five direct `get::<_, Will>` calls with it:
  `recompute_protocol_stats_chunk`, `recompute_protocol_stats`, `compare_protocol_stats`, the previous-value read in
  `save_will`, and `trim_stale_beneficiary_index`.
- **`contracts/will/src/issue_177_test.rs`** — new regression test that writes a raw `WillV1` under `DataKey::Will(id)`
  and verifies a stats scan succeeds, a subsequent `check_in` (which goes through `save_will`) preserves the legacy
  schema version, and a malformed entry produces an explicit `UnsupportedSchemaVersion` error.
- **`contracts/will/src/lib.rs`** — registered the new test module.

## Acceptance criteria addressed

- **No production read bypasses compatible decoding** — every `DataKey::Will` read in `storage.rs` goes through the
  shared helper (verified by grep: zero remaining direct `get::<_, Will>` calls on that key).
- **Raw `WillV1` can be loaded, checked in and scanned** — covered by the new test.
- **Malformed layouts are distinguishable from missing entries** — `None` stays missing; unknown layouts panic with the
  dedicated error code rather than being treated as absent.

## Validation note

`cargo check --tests` reports no new diagnostics from this change; remaining errors belong to independent issues
(#174, #175, #183, #184) handled separately.

closes #177
