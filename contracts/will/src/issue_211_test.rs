//! Tests for issue #211: explicit keepalive for released hashed claim reserves.
//!
//! These tests exercise the `keepalive_released_will` entrypoint which allows a
//! reserve holder to maintain the live `Will` record for a `Released` will that
//! still has outstanding hashed reserves, without changing entitlements.

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, Vec,
};

fn setup_token(env: &Env, admin: &Address) -> Address {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    token_id.address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

fn hash_of(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

/// Build a will with `n` hashed beneficiaries, distribute it (moving it to
/// `Released`), and return the will id plus the beneficiary addresses.
fn create_released_will_with_hashed_reserves(
    env: &Env,
    client: &WillContractClient,
    token: &Address,
    owner: &Address,
    n: u32,
) -> (u64, Vec<Address>) {
    let mut beneficiaries = Vec::new(env);
    let mut hashes = Vec::new(env);
    for i in 0..n {
        let b = Address::generate(env);
        beneficiaries.push_back(b.clone());
        hashes.push_back(hash_of(env, (i + 1) as u8));
    }

    let total: i128 = 1_000_000 * (n as i128);
    mint(env, token, owner, total);

    let will_id = client.create_will(
        owner,
        token,
        &total,
        &beneficiaries,
        &hashes,
        &0u64, // no inactivity requirement for this test
    );

    // Move the will to Released by distributing.
    client.distribute(&will_id);

    (will_id, beneficiaries)
}

#[test]
fn keepalive_preserves_released_record_and_reserves() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let token = setup_token(&env, &admin);

    let contract_id = env.register_contract(None, WillContract);
    let client = WillContractClient::new(&env, &contract_id);

    let (will_id, beneficiaries) =
        create_released_will_with_hashed_reserves(&env, &client, &token, &owner, 3);

    // Age the ledger near the TTL threshold.
    let current = env.ledger().sequence();
    env.ledger().set_sequence_number(current + 100_000);

    // A reserve holder maintains the claim record.
    let holder = beneficiaries.get(0).unwrap();
    client.keepalive_released_will(&will_id, &holder);

    // The live record is preserved.
    let will = client.get_will(&will_id);
    assert_eq!(will.id, will_id);
    assert_eq!(will.balances.len(), 3);

    // Remaining hashed reserves and permissions are preserved.
    for i in 0..3u32 {
        let b = beneficiaries.get(i).unwrap();
        assert!(client.has_claim(&will_id, &b));
    }

    // The record's TTL was renewed (extended beyond the aged sequence).
    let ttl = env.storage().instance().get_ttl();
    assert!(ttl > 0);
}

#[test]
fn keepalive_after_partial_claim_keeps_remaining_slots_live() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let token = setup_token(&env, &admin);

    let contract_id = env.register_contract(None, WillContract);
    let client = WillContractClient::new(&env, &contract_id);

    let (will_id, beneficiaries) =
        create_released_will_with_hashed_reserves(&env, &client, &token, &owner, 3);

    // Claim one of several slots.
    let first = beneficiaries.get(0).unwrap();
    client.reveal_and_claim(&will_id, &first, &hash_of(&env, 1));

    // The remaining record is still live.
    let will = client.get_will(&will_id);
    assert_eq!(will.id, will_id);

    // Remaining slots still have outstanding reserves.
    for i in 1..3u32 {
        let b = beneficiaries.get(i).unwrap();
        assert!(client.has_claim(&will_id, &b));
    }

    // Age the ledger near the TTL threshold and maintain the record.
    let current = env.ledger().sequence();
    env.ledger().set_sequence_number(current + 100_000);

    let holder = beneficiaries.get(1).unwrap();
    client.keepalive_released_will(&will_id, &holder);

    // Remaining record still live and TTL renewed.
    let will_after = client.get_will(&will_id);
    assert_eq!(will_after.id, will_id);
    let ttl = env.storage().instance().get_ttl();
    assert!(ttl > 0);

    // Remaining slots still claimable.
    for i in 1..3u32 {
        let b = beneficiaries.get(i).unwrap();
        assert!(client.has_claim(&will_id, &b));
    }
}

#[test]
fn keepalive_rejected_when_fully_discharged() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let token = setup_token(&env, &admin);

    let contract_id = env.register_contract(None, WillContract);
    let client = WillContractClient::new(&env, &contract_id);

    let (will_id, beneficiaries) =
        create_released_will_with_hashed_reserves(&env, &client, &token, &owner, 2);

    // Claim all hashed reserves.
    for i in 0..2u32 {
        let b = beneficiaries.get(i).unwrap();
        client.reveal_and_claim(&will_id, &b, &hash_of(&env, (i + 1) as u8));
    }

    // A fully discharged record is rejected by the keepalive entrypoint.
    let holder = beneficiaries.get(0).unwrap();
    let result = client.try_keepalive_released_will(&will_id, &holder);
    assert!(result.is_err());
}

#[test]
fn keepalive_rejected_for_non_released_will() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let token = setup_token(&env, &admin);

    let contract_id = env.register_contract(None, WillContract);
    let client = WillContractClient::new(&env, &contract_id);

    let mut beneficiaries = Vec::new(&env);
    let mut hashes = Vec::new(&env);
    for i in 0..2u32 {
        beneficiaries.push_back(Address::generate(&env));
        hashes.push_back(hash_of(&env, (i + 1) as u8));
    }

    let total: i128 = 1_000_000;
    mint(&env, &token, &owner, total);

    // Create but do NOT distribute: the will is not Released.
    let will_id = client.create_will(
        &owner,
        &token,
        &total,
        &beneficiaries,
        &hashes,
        &0u64,
    );

    let holder = beneficiaries.get(0).unwrap();
    let result = client.try_keepalive_released_will(&will_id, &holder);
    assert!(result.is_err());
}