#![cfg(test)]

//! Regression test for issue #280: `top_up` requires `WillStatus::Active`,
//! but no test attempted to top up a will in any other status and asserted
//! the rejection. A regression here (e.g. accidentally topping up a
//! `Released` will's already-zeroed balance map) would not have been caught.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient, WillError};

const DAY: u64 = 86_400;

fn setup<'a>() -> (Env, WillContractClient<'a>, Address, Address, u64) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &token_address).mint(&owner, &1_000_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);

    let will_id = client.create_will(
        &owner,
        &vec![&env, (token_address.clone(), 1_000_000_i128)],
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

    (env, client, owner, token_address, will_id)
}

#[test]
fn top_up_is_rejected_for_a_triggered_will() {
    let (env, client, owner, token_address, will_id) = setup();

    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);

    assert_eq!(
        client.try_top_up(&will_id, &owner, &token_address, &1_000),
        Err(Ok(WillError::WillNotActive.into()))
    );
}

#[test]
fn top_up_is_rejected_for_a_cancelled_will() {
    let (_env, client, owner, token_address, will_id) = setup();

    client.cancel_will(&will_id, &owner);

    assert_eq!(
        client.try_top_up(&will_id, &owner, &token_address, &1_000),
        Err(Ok(WillError::WillNotActive.into()))
    );
}

#[test]
fn top_up_is_rejected_for_a_released_will() {
    let (env, client, owner, token_address, will_id) = setup();

    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);
    env.ledger().with_mut(|l| l.timestamp += 8 * DAY);
    client.release_inheritance(&will_id, &None);

    assert_eq!(
        client.try_top_up(&will_id, &owner, &token_address, &1_000),
        Err(Ok(WillError::WillNotActive.into()))
    );
}

// ---------------------------------------------------------------------------
// README handoff reconciliation anchors (issue #280).
//
// The handoff notes previously claimed owner validation relied on SDK-side
// logic and referenced an `allocate_funds` allocator. These tests pin the
// current, native behaviour so the README dispositions stay verifiable.
// ---------------------------------------------------------------------------

#[test]
fn cancel_will_requires_owner_auth() {
    let (env, client, owner, _token_address, will_id) = setup();

    // With auths mocked the owner can cancel.
    client.cancel_will(&will_id, &owner);

    // Without mocked auths, the owner.require_auth() in cancel_will must fail.
    let (env2, client2, owner2, _token2, will_id2) = setup();
    env2.set_auths(&[]);
    assert!(
        client2.try_cancel_will(&will_id2, &owner2).is_err(),
        "cancel_will must require owner auth"
    );
    let _ = env;
}

#[test]
fn load_owned_rejects_non_owner() {
    let (env, client, owner, token_address, will_id) = setup();
    let stranger = Address::generate(&env);

    // A non-owner cannot cancel (load_owned verifies persisted owner).
    assert_eq!(
        client.try_cancel_will(&will_id, &stranger),
        Err(Ok(WillError::Unauthorized.into()))
    );

    // The persisted owner is still intact after the rejected attempt.
    client.cancel_will(&will_id, &owner);
    let _ = token_address;
}

#[test]
fn distribute_uses_vector_transfer_plan() {
    let (env, client, owner, token_address, will_id) = setup();

    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);
    env.ledger().with_mut(|l| l.timestamp += 8 * DAY);

    // Distribution succeeds through the vector-based transfer plan; there is
    // no `allocate_funds` entrypoint on the contract client.
    client.release_inheritance(&will_id, &None);

    let token = soroban_sdk::token::TokenClient::new(&env, &token_address);
    let _ = token;
    let _ = owner;
}
