#![cfg(test)]

//! Verification test for the contract's public interface.
//! This test ensures the contract compiles and the main entry points
//! are accessible, as part of documentation improvements (issue #198).

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient};

/// Minimal two-token fixture where one token can be temporarily paused
/// (refusing transfers) while the other keeps working.
mod refusing_token {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    #[contracttype]
    #[derive(Clone)]
    pub enum DataKey {
        Admin,
        Balance(Address),
        Paused,
    }

    #[contract]
    pub struct RefusingToken;

    #[contractimpl]
    impl RefusingToken {
        pub fn initialize(env: Env, admin: Address) {
            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Paused, &false);
        }

        pub fn set_paused(env: Env, paused: bool) {
            env.storage().instance().set(&DataKey::Paused, &paused);
        }

        pub fn mint(env: Env, to: Address, amount: i128) {
            let key = DataKey::Balance(to.clone());
            let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
            env.storage().persistent().set(&key, &(current + amount));
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::Balance(id))
                .unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
            if paused {
                panic!("token temporarily paused");
            }
            let from_key = DataKey::Balance(from.clone());
            let from_balance: i128 = env.storage().persistent().get(&from_key).unwrap_or(0);
            assert!(from_balance >= amount, "insufficient balance");
            env.storage()
                .persistent()
                .set(&from_key, &(from_balance - amount));
            let to_key = DataKey::Balance(to.clone());
            let to_balance: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&to_key, &(to_balance + amount));
        }
    }
}

#[test]
fn contract_interface_is_available() {
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

    // Test that main entry points are accessible
    // create_will
    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 100_000_i128)],
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

    // get_will
    let will = client.get_will(&will_id);
    assert_eq!(will.id, will_id);

    // get_will_status
    let status = client.get_will_status(&will_id);
    assert_eq!(status, will.status);

    // get_time_until_deadline
    let deadline = client.get_time_until_deadline(&will_id);
    assert!(deadline.is_some());

    // get_wills_by_owner
    let owner_wills = client.get_wills_by_owner(&owner, &None, &10);
    assert!(!owner_wills.is_empty());

    // get_wills_by_beneficiary
    let beneficiary_wills = client.get_wills_by_beneficiary(&beneficiary, &None, &10);
    assert!(!beneficiary_wills.is_empty());

    // get_protocol_stats
    let stats = client.get_protocol_stats();
    assert!(!stats.total_locked_by_token.is_empty());

    // get_contract_version
    let version = client.get_contract_version();
    assert!(version > 0);

    // get_failed_payout_amount (issue #212): no record yet -> documented empty result
    let failed_amount = client.get_failed_payout_amount(&will_id, &token_address, &beneficiary);
    assert!(failed_amount.is_none());

    // get_guardian_cancel_vote_status (issue #214): no vote -> documented empty result
    let cancel_vote = client.get_guardian_cancel_vote_status(&will_id, &owner);
    assert!(cancel_vote.is_none());
}

#[test]
fn owner_refund_isolates_unavailable_token() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);

    // Token A: conforming Stellar asset.
    let sac_a = env.register_stellar_asset_contract_v2(owner.clone());
    let token_a = sac_a.address();
    StellarAssetClient::new(&env, &token_a).mint(&owner, &1_000_000);

    // Token B: fixture that can temporarily refuse transfers.
    let token_b = env.register(refusing_token::RefusingToken, ());
    let token_b_client = refusing_token::RefusingTokenClient::new(&env, &token_b);
    token_b_client.initialize(&owner);
    token_b_client.mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let beneficiary = Address::generate(&env);

    let will_id = client.create_will(
        &owner,
        &vec![
            &env,
            (token_a.clone(), 100_000_i128),
            (token_b.clone(), 100_000_i128),
        ],
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

    // Token B refuses transfers while the owner cancels.
    token_b_client.set_paused(&true);

    client.cancel_will(&will_id, &owner);

    // The available asset refund completes despite the refusing token.
    let owner_a = soroban_sdk::token::TokenClient::new(&env, &token_a).balance(&owner);
    assert_eq!(owner_a, 1_000_000);

    // The undelivered owner amount is recoverable exactly once.
    let deferred = client.get_failed_payout_amount(&will_id, &token_b, &owner);
    assert_eq!(deferred, Some(100_000_i128));

    // Cancellation statistics and indexes stay consistent.
    let stats = client.get_protocol_stats();
    assert!(!stats.total_locked_by_token.is_empty());
    let owner_wills = client.get_wills_by_owner(&owner, &None, &10);
    assert!(owner_wills.iter().all(|w| w.id != will_id));

    // Unpause and recover the deferred asset.
    token_b_client.set_paused(&false);
    client.retry_failed_payout(&will_id, &token_b, &owner);

    assert_eq!(token_b_client.balance(&owner), 1_000_000);
    assert!(client
        .get_failed_payout_amount(&will_id, &token_b, &owner)
        .is_none());

    // Exactly-once: a second retry must not double-pay.
    let second = client.try_retry_failed_payout(&will_id, &token_b, &owner);
    assert!(second.is_err());
    assert_eq!(token_b_client.balance(&owner), 1_000_000);
}
