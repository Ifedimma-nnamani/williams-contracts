#![cfg(test)]

//! Issue #208: fixed-allocation totals must be checked, not saturated.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{assert_valid_allocations, Allocation, Beneficiary};

#[test]
#[should_panic]
fn overflowing_fixed_total_is_rejected() {
    let env = Env::default();
    let beneficiaries = vec![
        &env,
        Beneficiary {
            address: Address::generate(&env),
            allocation: Allocation::FixedAmount(i128::MAX),
        },
        Beneficiary {
            address: Address::generate(&env),
            allocation: Allocation::FixedAmount(1),
        },
    ];
    // A saturating sum would equal i128::MAX and pass against this balance,
    // even though the real commitment is larger than the estate.
    assert_valid_allocations(&env, &beneficiaries, &vec![&env], i128::MAX);
}

#[test]
fn exactly_funded_fixed_total_is_accepted() {
    let env = Env::default();
    let beneficiaries = vec![
        &env,
        Beneficiary {
            address: Address::generate(&env),
            allocation: Allocation::FixedAmount(i128::MAX - 1),
        },
        Beneficiary {
            address: Address::generate(&env),
            allocation: Allocation::FixedAmount(1),
        },
    ];
    assert_valid_allocations(&env, &beneficiaries, &vec![&env], i128::MAX);
}
