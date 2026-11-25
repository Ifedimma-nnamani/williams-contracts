#![cfg(test)]

//! Regression coverage for `get_wills` (#214): asserts the batch-by-id fetch
//! preserves input order, silently skips nonexistent ids, and panics with
//! `TooManyIds` when given more than `MAX_GET_WILLS_IDS` entries.
//!
//! Also covers #126: reading the beneficiary reverse index must extend the
//! TTL of the `BeneficiaryWills` entry so dormant indices are not archived.
//!
//! Also covers #192: `get_owner_status_page` with `limit == 0` must return an
//! empty page with `next_cursor == None` instead of underflowing on
//! `wills.len() - 1`.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient, WillError, WillStatus};

#[test]
fn preserves_input_order_and_skips_missing_ids() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &10_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);

    let make_will = || {
        client.create_will(
            &owner,
            &vec![&env, (token_address.clone(), 1_000_000_i128)],
            &vec![
                &env,
                Beneficiary {
                    address: beneficiary.clone(),
                    allocation: Allocation::Percentage(10_000),
                },
            ],
            &90,
            &7,
            &vec![&env],
            &0,
            &None,
            &0,
        )
    };

    let will_a = make_will();
    let will_b = make_will();
    let will_c = make_will();

    // Request in a deliberately reversed order with a nonexistent id
    // interleaved; the result must preserve the requested order and skip
    // the nonexistent id, not filter-then-reorder.
    let nonexistent_id = will_c + 1000;
    let result = client.get_wills(&vec![&env, will_c, nonexistent_id, will_a, will_b]);

    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap().id, will_c);
    assert_eq!(result.get(1).unwrap().id, will_a);
    assert_eq!(result.get(2).unwrap().id, will_b);
}

#[test]
fn returns_empty_for_all_nonexistent_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let result = client.get_wills(&vec![&env, 999_u64, 1000_u64]);
    assert!(result.is_empty());
}

#[test]
fn duplicate_ids_produce_duplicate_result_entries() {
    // Passing the same id twice must yield two copies of the same Will, not
    // one — the function is documented to be order-preserving with no
    // deduplication. Any client-side aggregation (e.g. summing balances) is
    // the caller's responsibility to guard against double-counting.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &2_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);

    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: beneficiary.clone(),
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

    // Pass the same id twice.
    let result = client.get_wills(&vec![&env, will_id, will_id]);

    // Both occurrences must appear in the output, in order.
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap().id, will_id);
    assert_eq!(result.get(1).unwrap().id, will_id);
}

#[test]
fn panics_when_ids_exceed_max_get_wills_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let mut ids = soroban_sdk::Vec::new(&env);
    for i in 0..51u64 {
        ids.push_back(i);
    }

    assert_eq!(
        client.try_get_wills(&ids),
        Err(Ok(WillError::TooManyIds.into()))
    );
}

#[test]
fn get_wills_by_beneficiary_extends_index_ttl() {
    // #126: reading the beneficiary reverse index must bump the TTL of the
    // `BeneficiaryWills` entry, mirroring `get_wills_by_owner`, so dormant
    // beneficiary indices are not archived.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);

    client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: beneficiary.clone(),
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

    // Advance the ledger so the index entry's TTL is measurably below the
    // configured bump target before the read.
    env.ledger().with_mut(|li| {
        li.sequence_number += 1_000;
    });

    let before = env.storage().persistent().get_ttl(&crate::DataKey::BeneficiaryWills(beneficiary.clone()));

    let wills = client.get_wills_by_beneficiary(&beneficiary);
    assert_eq!(wills.len(), 1);

    let after = env.storage().persistent().get_ttl(&crate::DataKey::BeneficiaryWills(beneficiary.clone()));

    assert!(
        after > before,
        "beneficiary index TTL must be extended on read (before={before}, after={after})"
    );
}

#[test]
fn owner_status_page_zero_limit_returns_empty_page() {
    // #192: limit == 0 must not underflow on `wills.len() - 1`. Even for an
    // owner with no wills, the empty vector satisfies `len() >= page_size`
    // (0 >= 0), so the subtraction is evaluated before `Vec::get` can return
    // None. The page must be empty with `next_cursor == None`.
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let page = client.get_owner_status_page(&owner, &WillStatus::Active, &None, &0);
    assert!(page.wills.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[test]
fn owner_status_page_zero_limit_with_populated_owner_returns_empty_page() {
    // #192: a populated owner with limit == 0 must also return an empty page
    // with `next_cursor == None`, not an arithmetic abort.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);

    client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: beneficiary.clone(),
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

    let page = client.get_owner_status_page(&owner, &WillStatus::Active, &None, &0);
    assert!(page.wills.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[test]
fn owner_status_page_positive_limit_retains_total_count_and_cursor() {
    // #192: positive limits must retain total_count and continuation behavior.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &3_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);

    for _ in 0..3 {
        client.create_will(
            &owner,
            &vec![&env, (token_address.clone(), 1_000_000_i128)],
            &vec![
                &env,
                Beneficiary {
                    address: beneficiary.clone(),
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
    }

    let page = client.get_owner_status_page(&owner, &WillStatus::Active, &None, &2);
    assert_eq!(page.wills.len(), 2);
    assert_eq!(page.total_count, 3);
    assert!(page.next_cursor.is_some());

    let next = client.get_owner_status_page(&owner, &WillStatus::Active, &page.next_cursor, &2);
    assert_eq!(next.wills.len(), 1);
    assert_eq!(next.total_count, 3);
    assert_eq!(next.next_cursor, None);
}
