#![cfg(test)]

//! Regression tests for issue #101: `check_in` rate-limits state updates so
//! consecutive check-ins within `MIN_CHECKIN_INTERVAL` do not write redundant
//! history entries.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{Allocation, Beneficiary, WillContract, WillContractClient, MIN_CHECKIN_INTERVAL};

fn setup(env: &Env) -> (WillContractClient<'_>, Address, Address) {
    let owner = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_address = sac.address();
    StellarAssetClient::new(env, &token_address).mint(&owner, &10_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(env, &contract_id);
    (client, owner, token_address)
}

fn create_will(
    env: &Env,
    client: &WillContractClient<'_>,
    owner: &Address,
    token_address: &Address,
    beneficiary: &Address,
) -> u64 {
    client.create_will(
        owner,
        &vec![env, (token_address.clone(), 1_000_000_i128)],
        &vec![
            env,
            Beneficiary {
                address: beneficiary.clone(),
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

#[test]
fn check_in_rate_limits_redundant_updates() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let heir = Address::generate(&env);
    let (client, owner, token_address) = setup(&env);
    let will_id = create_will(&env, &client, &owner, &token_address, &heir);

    env.ledger()
        .with_mut(|ledger| ledger.timestamp += MIN_CHECKIN_INTERVAL);
    let first_checkin = env.ledger().timestamp();
    client.check_in(&will_id, &owner);
    assert_eq!(client.get_will(&will_id).last_checkin, first_checkin);

    env.ledger()
        .with_mut(|ledger| ledger.timestamp += MIN_CHECKIN_INTERVAL - 1);
    client.check_in(&will_id, &owner);
    assert_eq!(client.get_will(&will_id).last_checkin, first_checkin);

    env.ledger().with_mut(|ledger| ledger.timestamp += 1);
    client.check_in(&will_id, &owner);
    assert_eq!(
        client.get_will(&will_id).last_checkin,
        first_checkin + MIN_CHECKIN_INTERVAL
    );
}
