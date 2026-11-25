#![cfg(test)]

//! Issue #181: a guardian imported by a merge must not be able to use the
//! survivor's already-expired guardian-list cooldown.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{
    Allocation, Beneficiary, GuardianVoteReason, WillContract, WillContractClient, WillError,
    WillStatus,
};

const DAY: u64 = 86_400;

fn create<'a>(
    env: &Env,
    client: &WillContractClient<'a>,
    owner: &Address,
    guardians: &soroban_sdk::Vec<Address>,
    threshold: u32,
) -> u64 {
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    StellarAssetClient::new(env, &token).mint(owner, 10_000_000);
    client.create_will(
        owner,
        &vec![env, (token, 1_000_000_i128)],
        &vec![
            env,
            Beneficiary {
                address: Address::generate(env),
                allocation: Allocation::Percentage(10_000),
            },
        ],
        &90,
        &7,
        guardians,
        &threshold,
        &None,
        &0,
    )
}

#[test]
fn imported_guardian_gets_a_fresh_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let owner = Address::generate(&env);
    let id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &id);

    // A is an old guardian-free will: its cooldown has long since elapsed.
    let a = create(&env, &client, &owner, &vec![&env], 0);
    env.ledger().with_mut(|l| l.timestamp += 20 * DAY);

    // B is fresh and names one guardian with threshold 1.
    let guardian = Address::generate(&env);
    let b = create(&env, &client, &owner, &vec![&env, guardian.clone()], 1);
    client.accept_guardian_role(&b, &guardian);

    let merged_at = env.ledger().timestamp();
    client.merge_wills(&owner, &a, &b);
    let survivor = client.get_will(&a);
    assert_eq!(survivor.guardians.len(), 1);
    assert_eq!(survivor.guardian_threshold, 1);
    assert_eq!(survivor.guardian_list_updated_at, merged_at);

    // The imported guardian cannot vote during the fresh 7-day cooldown.
    let err = client
        .try_guardian_trigger(&a, &guardian, &GuardianVoteReason::Deceased)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, WillError::GuardianCooldownActive.into());

    // After the cooldown the vote is accepted and reaches the threshold.
    env.ledger().with_mut(|l| l.timestamp += 7 * DAY + 1);
    client.guardian_trigger(&a, &guardian, &GuardianVoteReason::Deceased);
    assert_eq!(client.get_will(&a).status, WillStatus::Triggered);
}

#[test]
fn unchanged_guardian_list_keeps_its_elapsed_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let owner = Address::generate(&env);
    let id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &id);

    let guardian = Address::generate(&env);
    let a = create(&env, &client, &owner, &vec![&env, guardian.clone()], 1);
    client.accept_guardian_role(&a, &guardian);
    let b = create(&env, &client, &owner, &vec![&env, guardian.clone()], 1);
    client.accept_guardian_role(&b, &guardian);

    env.ledger().with_mut(|l| l.timestamp += 20 * DAY);
    let before = client.get_will(&a).guardian_list_updated_at;
    client.merge_wills(&owner, &a, &b);
    assert_eq!(client.get_will(&a).guardian_list_updated_at, before);

    client.guardian_trigger(&a, &guardian, &GuardianVoteReason::Deceased);
    assert_eq!(client.get_will(&a).status, WillStatus::Triggered);
}
