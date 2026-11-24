#![cfg(test)]

//! Issue #186: grace-extension days must be part of the effective deadline
//! used by the deadline query and guardian cancellation, not only by release
//! and emergency recovery.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient, WillError, WillStatus};

const DAY: u64 = 86_400;

fn setup<'a>() -> (Env, WillContractClient<'a>, Address, Address, u64) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let guardian = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token = sac.address();
    StellarAssetClient::new(&env, &token).mint(&owner, 10_000_000);

    let id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &id);
    let will_id = client.create_will(
        &owner,
        &vec![&env, (token, 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: Address::generate(&env),
                allocation: Allocation::Percentage(10_000),
            },
        ],
        &90,
        &7,
        &vec![&env, guardian.clone()],
        &1,
        &None,
        &0,
    );
    client.accept_guardian_role(&will_id, &guardian);
    (env, client, owner, guardian, will_id)
}

#[test]
fn extension_is_visible_to_query_and_guardian_cancellation() {
    let (env, client, owner, guardian, will_id) = setup();

    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);
    let trigger_at = env.ledger().timestamp();
    client.request_grace_extension(&will_id, &owner, &7);

    assert_eq!(client.get_will(&will_id).grace_extension_days, 7);

    // Base grace is 7 days, but the effective window is 14 days. At day 8
    // the query must still show time remaining and a guardian must still be
    // allowed to cancel the trigger.
    env.ledger().with_mut(|l| l.timestamp += 8 * DAY);
    let remaining = client.get_time_until_deadline(&will_id).unwrap();
    assert_eq!(remaining, (trigger_at + 14 * DAY) as i64 - env.ledger().timestamp() as i64);
    assert!(remaining > 0);

    client.guardian_cancel_trigger(&will_id, &guardian);
    assert_eq!(client.get_will(&will_id).status, WillStatus::Active);
}

#[test]
fn cancellation_is_rejected_past_the_effective_deadline() {
    let (env, client, owner, guardian, will_id) = setup();
    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);
    client.request_grace_extension(&will_id, &owner, &7);

    env.ledger().with_mut(|l| l.timestamp += 15 * DAY);
    let err = client
        .try_guardian_cancel_trigger(&will_id, &guardian)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, WillError::GracePeriodExpired.into());
}
