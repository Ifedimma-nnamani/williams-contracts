# Fix batch retry event arity (#175)

## Summary

`events::payout_retried` takes six arguments including `fully_resolved: bool`, but `batch_retry_failed_payouts` still
called it with five. This statically blocks every library build, including deployments that never use batch retry.
The batch path now passes the same resolution context as the single retry path.

## What changed

- **`contracts/will/src/lib.rs` — `batch_retry_failed_payouts`**
  - Passes `true` as `fully_resolved` when emitting `payout_retried`.
  - Rationale is documented inline: the exact `(will_id, token, recipient)` tuple is removed from failed-payout storage
    immediately before the event, so the retry fully resolves it — identical semantics to `retry_failed_payout`, which
    already passes `true`.
- **`contracts/will/src/issue_108_test.rs`**
  - Added `batch_retry_emits_resolution_context`, which creates a failed payout, retries it in a batch and decodes the
    emitted `payretry` event payload, asserting that the trailing `fully_resolved` flag is `true`.

## Acceptance criteria addressed

- **All call sites match the definition** — the only remaining five-argument call is gone.
- **Single and batched retries emit the same tuple resolution semantics** — both pass `true`.
- **Payload decoding tests cover both paths** — the existing issue #107 test covers the single path; the new test covers
  the batch path and decodes the boolean field.

## Validation note

`cargo check` on this checkout reports four independent pre-existing compile blockers owned by other open issues
(#174 cooldown field/constant, #183 guardian freshness module, #184 `InvalidDelegate`). This PR removes the
`payout_retried` arity error; the remaining errors are addressed by their own issues/PRs. No unrelated source was
changed.

closes #175
