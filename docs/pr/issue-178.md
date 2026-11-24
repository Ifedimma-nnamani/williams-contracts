# Guard close_will against unclaimed hashed reserves (#178)

## Summary

`close_will` moved a `Released` will to `Settled` unconditionally. Because `reveal_and_claim` only accepts `Released`,
closing with an unclaimed hashed reserve permanently trapped those funds; the following `archive_will` also skipped its
unclaimed-share guard for `Settled` wills. Closing now requires all hashed obligations to be discharged.

## What changed

- **`contracts/will/src/lib.rs`**
  - `close_will` rejects with `UnclaimedHashedBeneficiary` while `unclaimed_hashed_bps` is non-zero, before any status
    transition, event or history record is written.
  - `archive_will`'s unclaimed-share guard now covers both `Released` and `Settled` wills as defence in depth for legacy
    settled entries; its documentation was updated.
- **`contracts/will/src/issue_1_archive_hashed_test.rs`** — added `close_will_blocked_while_hashed_beneficiary_unclaimed`:
  a released will with an unclaimed hashed slot cannot be closed or archived; after `reveal_and_claim`, close and archive
  succeed.

## Acceptance criteria addressed

- **Closing cannot remove access to an unclaimed reserve** — explicit guard and regression test.
- **Archival cannot bypass the guard via Settled** — archival now checks both statuses.
- **Fully discharged wills still close and archive** — covered by the same test after the claim.

## Validation note

`cargo check --tests` shows no new diagnostics from this change; the remaining build errors are independent issues with
their own PRs.

closes #178
