# Validate the copied bounty token in clone_will (#210)

## Summary

`clone_will` copies `source.keeper_bounty_token` verbatim, but the clone's balances come from the caller-supplied `tokens`
argument. A clone funded only with a different token therefore advertised a bounty target it did not hold; `distribute`
silently fell back, leaving the stored configuration inconsistent with the actual incentive. The selection is now kept
only when the clone holds that token.

## What changed

- **`contracts/will/src/lib.rs` — `clone_will`**
  - Computes the clone's `keeper_bounty_token` before constructing `Will`: `Some(target)` only when the clone's balances
    contain it, otherwise `None`.
  - This matches `set_keeper_bounty_token`'s rule that a named token must be held by the will, and preserves the
    proportional fallback documented for `None`.
- **`contracts/will/src/clone_will_test.rs`** — added tests for both cases: the selection is cleared when the clone is
  funded with a different token and retained when the clone still holds the targeted token.

## Acceptance criteria addressed

- **Every stored nonempty bounty target is held by the clone** — enforced at clone creation.
- **Omitted source target has a documented outcome** — cleared to `None`, with the proportional fallback documented in
  the field and in code.
- **Clones retaining the token preserve valid selection** — covered by the second test.

## Validation note

`cargo check --tests` reports an unrelated pre-existing syntax error (`archive_will_test.rs` unclosed delimiter) plus the
independent build blockers owned by other issues; no new diagnostics come from this change.

closes #210
