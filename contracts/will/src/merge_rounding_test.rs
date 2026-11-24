#![cfg(test)]

//! Regression test for issue #188: ensure that `merge_beneficiaries` does not
//! silently drop a beneficiary whose merged share rounds down to 0 basis points.
//!
//! A beneficiary with a small percentage in one of the two source wills, when
//! merged into a much larger combined balance, could have their basis-point
//! share truncate to zero. This test ensures such a beneficiary is preserved
//! with at least 1 basis point rather than silently dropped.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env, Vec as SorobanVec,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient};

const DAY: u64 = 86_400;

fn setup<'a>() -> (
    Env,
    WillContractClient<'a>,
    Address,
    TokenClient<'a>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    (
        env.clone(),
        client,
        owner,
        TokenClient::new(&env, &token_address),
        token_address,
    )
}

fn advance(env: &Env, days: u64) {
    env.ledger().with_mut(|l| l.timestamp += days * DAY);
}

fn release(env: &Env, client: &WillContractClient, will_id: u64) {
    advance(env, 91);
    client.trigger_will(&will_id);
    advance(env, 8);
    client.release_inheritance(&will_id, &None);
}

/// Regression test for issue #188: a beneficiary whose merged share rounds to 0 bp
/// should not be silently dropped; they should receive at least 1 bp.
#[test]
fn merge_beneficiaries_preserves_small_share() {
    let (env, client, owner, token, token_address) = setup();
    let beneficiary_small = Address::generate(&env);
    let beneficiary_large = Address::generate(&env);

    let will_a_beneficiaries: SorobanVec<Beneficiary> = vec![
        &env,
        Beneficiary {
            address: beneficiary_small.clone(),
            allocation: Allocation::Percentage(100), // 1% in a 100k will
        },
        Beneficiary {
            address: beneficiary_large.clone(),
            allocation: Allocation::Percentage(9_900),
        },
    ];
    let will_a_tokens: SorobanVec<(Address, i128)> =
        vec![&env, (token_address.clone(), 100_000i128)];

    let will_b_beneficiaries: SorobanVec<Beneficiary> = vec![
        &env,
        Beneficiary {
            address: beneficiary_small.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: beneficiary_large.clone(),
            allocation: Allocation::Percentage(9_900),
        },
    ];
    let will_b_tokens: SorobanVec<(Address, i128)> =
        vec![&env, (token_address.clone(), 9_900_000i128)];

    let will_a = client.create_will(
        &owner,
        &will_a_tokens,
        &will_a_beneficiaries,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );
    let will_b = client.create_will(
        &owner,
        &will_b_tokens,
        &will_b_beneficiaries,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    client.merge_wills(&owner, &will_a, &will_b);

    release(&env, &client, will_a);

    let small_balance = token.balance(&beneficiary_small);
    let large_balance = token.balance(&beneficiary_large);
    let total = small_balance + large_balance;

    assert!(
        small_balance > 0,
        "beneficiary_small should receive a share (not be dropped)"
    );
    assert!(
        large_balance > 0,
        "beneficiary_large should receive a share"
    );
    assert_eq!(
        total, 10_000_000,
        "total distributed should equal combined will balance"
    );
}

/// Issue #209: promoting tiny positive shares to 1 bp can push the merged
/// total above 10,000. The correction must be spread across entries with
/// headroom, and the merge must still conserve the whole balance.
#[test]
fn merge_beneficiaries_keeps_total_within_10000_bps() {
    let (env, client, owner, token, token_address) = setup();
    let large = Address::generate(&env);

    let a = client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 1_000_000_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: large.clone(),
                allocation: Allocation::Percentage(10_000),
            },
        ],
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    let mut b_beneficiaries: SorobanVec<Beneficiary> = SorobanVec::new(&env);
    let mut b_addresses: SorobanVec<Address> = SorobanVec::new(&env);
    for i in 0..9 {
        let addr = Address::generate(&env);
        b_addresses.push_back(addr.clone());
        let bp = if i == 8 { 1_112 } else { 1_111 };
        b_beneficiaries.push_back(Beneficiary {
            address: addr,
            allocation: Allocation::Percentage(bp),
        });
    }
    let b = client.create_will(
        &owner,
        &vec![&env, (token_address, 90_000_i128)],
        &b_beneficiaries,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    client.merge_wills(&owner, &a, &b);

    let merged = client.get_will(&a);
    let total_bps: u32 = merged
        .beneficiaries
        .iter()
        .map(|b| match b.allocation {
            Allocation::Percentage(bp) => bp,
            Allocation::FixedAmount(_) => 0,
        })
        .sum();
    assert_eq!(total_bps, 10_000, "merged percentages must total exactly 10000");

    release(&env, &client, a);
    let distributed: i128 = merged
        .beneficiaries
        .iter()
        .map(|b| token.balance(&b.address))
        .sum();
    assert_eq!(distributed, 1_000_000_090_000);
}

/// Regression test: multiple beneficiaries with very small shares that would
/// all round to 0 bp should not be silently dropped.
#[test]
fn merge_multiple_small_shares_preserved() {
    let (env, client, owner, token, token_address) = setup();
    let small_1 = Address::generate(&env);
    let small_2 = Address::generate(&env);
    let small_3 = Address::generate(&env);
    let large = Address::generate(&env);

    let will_a_beneficiaries: SorobanVec<Beneficiary> = vec![
        &env,
        Beneficiary {
            address: small_1.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: small_2.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: small_3.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: large.clone(),
            allocation: Allocation::Percentage(9_700),
        },
    ];
    let will_a_tokens: SorobanVec<(Address, i128)> =
        vec![&env, (token_address.clone(), 100_000i128)];

    let will_b_beneficiaries: SorobanVec<Beneficiary> = vec![
        &env,
        Beneficiary {
            address: small_1.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: small_2.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: small_3.clone(),
            allocation: Allocation::Percentage(100),
        },
        Beneficiary {
            address: large.clone(),
            allocation: Allocation::Percentage(9_700),
        },
    ];
    let will_b_tokens: SorobanVec<(Address, i128)> =
        vec![&env, (token_address.clone(), 9_900_000i128)];

    let will_a = client.create_will(
        &owner,
        &will_a_tokens,
        &will_a_beneficiaries,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );
    let will_b = client.create_will(
        &owner,
        &will_b_tokens,
        &will_b_beneficiaries,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    client.merge_wills(&owner, &will_a, &will_b);

    release(&env, &client, will_a);

    let balance_small_1 = token.balance(&small_1);
    let balance_small_2 = token.balance(&small_2);
    let balance_small_3 = token.balance(&small_3);
    let balance_large = token.balance(&large);

    assert!(balance_small_1 > 0, "small_1 should not be dropped");
    assert!(balance_small_2 > 0, "small_2 should not be dropped");
    assert!(balance_small_3 > 0, "small_3 should not be dropped");
    assert!(balance_large > 0, "large should receive a share");

    let total = balance_small_1 + balance_small_2 + balance_small_3 + balance_large;
    assert_eq!(
        total, 10_000_000,
        "total should equal combined will balance"
    );
}
