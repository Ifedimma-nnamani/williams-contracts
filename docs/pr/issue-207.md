# Keep migrated portfolio discovery coherent (#207)

## Summary

`migrate_owner_portfolio` moves `OwnerWills` and per-status index entries to a new discovery address but deliberately
leaves `Will.owner` unchanged (it remains the authorization/refund owner). Later lifecycle calls still arrive with the
old owner, so `cancel_will`, `trigger_will` and `distribute` recreated stale entries under the old address while the
destination portfolio went stale. A persisted discovery alias now redirects index writes without touching authorization.

## What changed

- **`contracts/will/src/storage.rs`**
  - New `DataKey::OwnerDiscoveryAlias(Address)` storing the migration destination for an authorization owner.
  - `discovery_owner()` follows the alias chain (bounded to 8 hops, cycle-safe), so repeated A→B→C migrations work.
  - `index_by_owner`, `remove_owner_index`, `index_owner_status` and `remove_owner_status_index` resolve through
    `discovery_owner`; `transition_owner_status_index` inherits the redirect through those helpers.
  - `migrate_owner_portfolio` persists the alias after copying the index entries.
  - Read helpers (`get_owner_wills`, `get_owner_status_wills`) deliberately keep using the literal address: after
    migration the source portfolio is empty and the destination holds the entries.
- **`contracts/will/src/issue_110_test.rs`** — regressions for a cancel after A→B migration and after A→B→C: the
  destination no longer lists the will as Active, lists it under Cancelled, and `Will.owner` remains A.

## Acceptance criteria addressed

- **Migrated discovery reflects later trigger/cancel/release/archive** — all index writes redirect; cancel covered.
- **Signing authority remains explicitly defined** — `Will.owner` is never modified; alias is discovery-only.
- **Repeated migration and destination portfolios work coherently** — alias chains followed; second regression test.

## Validation note

`cargo check --tests` adds no new diagnostics from this change; the checkout's independent blockers and a pre-existing
unclosed delimiter in `archive_will_test.rs` remain.

closes #207
