#![cfg(test)]

//! Regression test for issue #282: `renounce_beneficiary` removes the caller
//! from `will.beneficiaries` and their reverse index entry, but nothing
//! tested that a second call to `renounce_beneficiary` by the same
//! (now-removed) address correctly fails with `WillError::BeneficiaryNotFound`
//! rather than silently succeeding as a no-op or double-redistributing shares
//! that no longer exist.

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
    vec, Address, Env, IntoVal, Symbol,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient, WillError};

#[test]
fn renouncing_twice_from_the_same_address_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address, 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: a.clone(),
                allocation: Allocation::Percentage(5_000),
            },
            Beneficiary {
                address: b,
                allocation: Allocation::Percentage(5_000),
            },
        ],
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    // First renunciation succeeds and removes `a` from the beneficiary list.
    client.renounce_beneficiary(&will_id, &a);

    // A second renunciation from the same, now-removed address must be
    // rejected, not treated as a no-op or re-processed against a stale entry.
    assert_eq!(
        client.try_renounce_beneficiary(&will_id, &a),
        Err(Ok(WillError::BeneficiaryNotFound.into()))
    );
}

#[test]
fn all_none_settings_update_is_a_noop() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address, 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: beneficiary,
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

    // Advance the ledger so a rewrite would be observable via TTL changes.
    env.ledger().with_mut(|li| {
        li.sequence_number += 10;
    });

    let before = client.get_will(&will_id);
    let before_ttl = env.storage().instance().get_ttl();

    // An all-None request must be a no-op: no rewrite, no event.
    client.update_will_settings(&will_id, &None, &None, &None, &None, &None);

    let after = client.get_will(&will_id);
    assert_eq!(before, after, "all-None update must not rewrite the will");
    assert_eq!(
        env.storage().instance().get_ttl(),
        before_ttl,
        "all-None update must not extend the TTL"
    );

    let settings_events = env
        .events()
        .all()
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() == 2
                && topics.get(0).unwrap() == Symbol::new(&env, "will").into_val(&env)
                && topics.get(1).unwrap() == Symbol::new(&env, "settings_updated").into_val(&env)
        })
        .count();
    assert_eq!(
        settings_events, 0,
        "all-None update must not emit will_settings_updated"
    );

    // A single-field update must still persist and emit the correct field set.
    client.update_will_settings(&will_id, &Some(120u32), &None, &None, &None, &None);

    let updated = client.get_will(&will_id);
    assert_eq!(updated.period_duration, 120);
    assert_eq!(updated.grace_period, before.grace_period);

    let emitted = env
        .events()
        .all()
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() == 2
                && topics.get(0).unwrap() == Symbol::new(&env, "will").into_val(&env)
                && topics.get(1).unwrap() == Symbol::new(&env, "settings_updated").into_val(&env)
        })
        .count();
    assert_eq!(
        emitted, 1,
        "single-field update must emit exactly one will_settings_updated"
    );
}

#[test]
fn unauthorized_settings_update_is_rejected() {
    let env = Env::default();

    let owner = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    env.mock_all_auths();
    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address, 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: beneficiary,
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

    // Without mocked auths the owner's authorization cannot be satisfied, so
    // an empty request must be rejected rather than silently accepted.
    env.set_auths(&[]);
    assert!(
        client
            .try_update_will_settings(&will_id, &None, &None, &None, &None, &None)
            .is_err(),
        "unauthorized caller must not be silently accepted"
    );
}
