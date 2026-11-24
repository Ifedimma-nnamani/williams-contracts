# Correct merged percentages across all entries (#209)

## Summary

`merge_beneficiaries` promotes every tiny positive share to at least 1 bp, then reduces only the *last* percentage entry
to correct an excess. When that last entry is smaller than the excess, the merged allocations still total above 10,000
bps and later release fails with `DistributionMismatch` or leaves negative remainder shares. The correction is now
spread across all entries with headroom, and merged allocations are validated before saving.

## What changed

- **`contracts/will/src/lib.rs` — `merge_beneficiaries`**
  - When `total_bp > 10_000`, reduces each percentage entry from the end by `min(bp - 1, excess)`, preserving the
    documented 1-bp promised minimum for every positive share.
  - Rejects with `InvalidPercentages` if the excess cannot be absorbed (an unrepresentable policy) instead of
    persisting an invalid total.
- **`contracts/will/src/lib.rs` — `merge_wills`**
  - Calls `assert_valid_allocations` on the merged beneficiaries against the merged primary balance before any state
    write.
- **`contracts/will/src/merge_rounding_test.rs`** — added the documented 10^12 + 90,000 / 9-beneficiary scenario:
  merged bps total exactly 10,000 and release distributes the full combined balance.

## Acceptance criteria addressed

- **Merged total cannot exceed 10,000 bps** — enforced by redistribution plus the final validation.
- **Positive-share preservation remains explicit** — no entry drops below 1 bp; impossible cases are typed errors.
- **Distributions conserve funds for varied ordering** — the new end-to-end release test asserts exact conservation.

## Validation note

The checkout's independent build blockers (#174, #175, #183, #184) and a pre-existing unclosed delimiter in
`archive_will_test.rs` remain; this change adds no new diagnostic.

closes #209
