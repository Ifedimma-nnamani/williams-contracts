# Normalize visible percentages over the distributable pool (#179)

## Summary

`distribute` withheld the hashed reserve and then applied visible basis points with a fixed 10,000 denominator to the
reduced pool. With two 4,000-bps visible beneficiaries and a 2,000-bps hashed reserve on a 1,000-unit will, the visible
shares became 320/480 instead of 400/400 — the second beneficiary silently absorbed 160 units as "rounding dust".
Visible basis points are now normalized over the visible pool before splitting.

## What changed

- **`contracts/will/src/lib.rs` — `distribute`**
  - Sums all visible percentage basis points once up front (`total_percentage_bps`).
  - For each non-final percentage beneficiary, renormalises its bps to `bp * 10_000 / total_percentage_bps` before
    computing the share from the residual pool (after bounty, hashed reserve and fixed deductions). The final
    percentage beneficiary still absorbs only genuine integer remainder dust.
  - When visible bps already total 10,000 (no hashed reserve) the normalisation is a no-op, preserving existing payouts.
- **`contracts/will/src/issue_57_58_59_60_test.rs`** — added `issue_179_mixed_allocations_conserve_material_shares`:
  the 1,000/4,000/4,000/2,000 example now pays 400 + 400 and leaves a 200-unit hashed reserve.

## Acceptance criteria addressed

- **Equal visible allocations receive equal shares** — 400/400 in the documented example.
- **Permutation only changes permitted rounding units** — symmetric normalisation; the remainder still goes to the
  final percentage beneficiary.
- **Mixed fixed/visible/hashed cases conserve every token** — the conservation check remains unchanged and the new test
  asserts the reserve plus payouts equal the balance.

## Validation note

No new diagnostics in `cargo check --tests`; unrelated compile blockers are handled by their own PRs.

closes #179
