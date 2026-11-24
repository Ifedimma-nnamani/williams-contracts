#![cfg(test)]

//! Issue #177: every read of a `DataKey::Will` entry must use the schema-
//! compatible decoder, including the previous-value read in `save_will` and
//! the protocol-statistics scans.

use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, vec, Address, Env,
};

use crate::{
    migration::legacy::WillV1,
    storage::{self, DataKey},
    Allocation, Beneficiary, Will, WillContract, WillContractClient, WillError, WillStatus,
};

fn setup<'a>(env: &Env) -> (WillContractClient<'a>, Address, Address) {
    let owner = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    StellarAssetClient::new(env, &token).mint(&owner, 10_000_000);
    let id = env.register(WillContract, ());
    (WillContractClient::new(env, &id), owner, token)
}

fn create<'a>(
    env: &Env,
    client: &WillContractClient<'a>,
    owner: &Address,
    token: &Address,
) -> u64 {
    client.create_will(
        owner,
        &vec![env, (token.clone(), 1_000_000_i128)],
        &vec![
            env,
            Beneficiary {
                address: Address::generate(env),
                allocation: Allocation::Percentage(10_000),
            },
        ],
        &90,
        &7,
        &vec![env],
        &0,
        &None,
        &0,
    )
}

fn as_legacy(env: &Env, current: &Will) -> WillV1 {
    WillV1 {
        id: current.id,
        owner: current.owner.clone(),
        balances: current.balances.clone(),
        token: current.token.clone(),
        is_native: current.is_native,
        balance: current.balance,
        beneficiaries: current.beneficiaries.clone(),
        hashed_beneficiaries: current.hashed_beneficiaries.clone(),
        checkin_period_days: current.checkin_period_days,
        grace_period_days: current.grace_period_days,
        last_checkin: current.last_checkin,
        trigger_time: current.trigger_time,
        confirmation_deadline: current.confirmation_deadline,
        status: current.status,
        guardians: current.guardians.clone(),
        guardian_vote_weight: current.guardian_vote_weight,
        guardian_votes: current.guardian_votes,
        guardian_cancel_vote_weight: current.guardian_cancel_vote_weight,
        guardian_cancel_votes: current.guardian_cancel_votes,
        guardian_threshold: current.guardian_threshold,
        guardian_list_updated_at: current.guardian_list_updated_at,
        schema_version: 1,
        keeper_bounty_bps: current.keeper_bounty_bps,
        delegate: current.delegate.clone(),
    }
}

#[test]
fn legacy_will_survives_stats_scan_and_save_will() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let (client, owner, token) = setup(&env);
    let contract_id = client.address.clone();
    let id = create(&env, &client, &owner, &token);

    let current = client.get_will(&id);
    let legacy = as_legacy(&env, &current);
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Will(id), &legacy);
    });

    // Stats scans decode the raw legacy entry instead of failing on the
    // current-type read.
    let stats = env.as_contract(&contract_id, || storage::recompute_protocol_stats(&env));
    assert!(stats.active_will_count >= 1);

    // `check_in` writes through `save_will`, whose previous-value read must
    // also use the compatible decoder.
    client.check_in(&id, &owner);
    let reloaded = client.get_will(&id);
    assert_eq!(reloaded.status, WillStatus::Active);
    assert_eq!(reloaded.schema_version, 1);

    // A present entry that matches no known layout is reported explicitly.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Will(id), &42_u64);
    });
    let err = client.try_get_will(&id).unwrap_err().unwrap();
    assert_eq!(err, WillError::UnsupportedSchemaVersion.into());
}
