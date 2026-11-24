# Refresh the guardian cooldown after a merge changes the committee (#181)

## Summary

`merge_wills` combined the two guardian lists and carried consent across but left the survivor's
`guardian_list_updated_at` untouched. A guardian newly introduced by the merge could therefore vote immediately under
the survivor's long-expired cooldown. Changed committees now get a fresh cooldown; unchanged ones keep their elapsed
cooldown.

## What changed

- **`contracts/will/src/lib.rs` — `merge_wills`**
  - Captures the survivor's original guardian list before merging and compares it (length, address, weight and consent)
    with the merged result.
  - Sets `guardian_list_updated_at = now` when the committee differs, so the standard 7-day `GUARDIAN_COOLDOWN_DAYS`
    applies to the newly introduced guardians.
  - Leaves the timestamp unchanged when the committee is identical, preserving the documented behaviour for merges
    that only move balances.
- **`contracts/will/src/issue_181_test.rs`** — two regression tests: an imported guardian is blocked by
  `GuardianCooldownActive` until 7 days after the merge and can then reach threshold; an unchanged committee retains its
  elapsed cooldown and can vote immediately.

## Acceptance criteria addressed

- **Newly introduced guardians cannot use an old survivor timestamp** — explicit fresh-cooldown assignment and test.
- **Unchanged-list merges have an explicitly tested cooldown policy** — second test.
- **Consent merging and weighted threshold validation remain intact** — the existing merge logic is untouched; only the
  timestamp is conditional.

## Validation note

No new diagnostics in `cargo check --tests`; remaining errors belong to independent issues.

closes #181
