use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::StellarAssetClient,
    vec,
    xdr::{ContractEventBody, ScVal},
    Address, Env, IntoVal, TryFromVal, Val,
};
use will::{Allocation, Beneficiary, WillContract, WillContractClient, WillError};

#[test]
fn batch_check_in_validates_every_will_and_summarizes_only_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let start = 1_700_000_000;
    env.ledger().set_timestamp(start);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &asset).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let beneficiary = Address::generate(&env);
    let create = || {
        client.create_will(
            &owner,
            &vec![&env, (asset.clone(), 100_000_i128)],
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

    let single = create();
    let batch = create();
    client.set_delegate(&single, &owner, &Some(delegate.clone()));
    client.set_delegate(&batch, &owner, &Some(delegate.clone()));

    for elapsed in [0, 59, 60] {
        env.ledger().set_timestamp(start + elapsed);
        client.check_in(&single, &delegate);
        let single_events = env.events().all().events().len();
        client.batch_check_in(&vec![&env, batch], &delegate);
        let batch_events = env.events().all().events().len();
        let eligible = elapsed == 60;
        assert_eq!(single_events, usize::from(eligible));
        assert_eq!(batch_events, 2 * usize::from(eligible));
        for id in [single, batch] {
            let expected = if eligible { start + 60 } else { start };
            assert_eq!(client.get_will(&id).last_checkin, expected);
            assert_eq!(client.get_will_history(&id).len(), 1 + u32::from(eligible));
        }
    }

    let fresh = create();
    let unauthorized = create();
    client.set_delegate(&fresh, &owner, &Some(delegate.clone()));
    env.ledger().set_timestamp(start + 61);
    assert_eq!(
        client.try_batch_check_in(&vec![&env, batch, unauthorized], &delegate),
        Err(Ok(WillError::NotOwner.into())),
    );

    env.ledger().set_timestamp(start + 120);
    let newest = create();
    client.set_delegate(&newest, &owner, &Some(delegate.clone()));
    let newest_history = client.get_will_history(&newest);
    let before = client.get_will_history(&batch);
    assert_eq!(
        client.try_batch_check_in(&vec![&env, batch, unauthorized], &delegate),
        Err(Ok(WillError::NotOwner.into())),
    );
    assert!(env.events().all().events().is_empty());
    assert_eq!(client.get_will_history(&batch), before);
    assert_eq!(client.get_will(&batch).last_checkin, start + 60);

    client.batch_check_in(&vec![&env, batch, newest, fresh], &delegate);
    let events = env.events().all();
    assert_eq!(events.events().len(), 3);
    let ContractEventBody::V0(summary) = &events.events().last().unwrap().body;
    let expected_topic: ScVal = symbol_short!("batchchk").into_val(&env);
    assert_eq!(summary.topics[0], expected_topic);
    let expected_data: Val = (vec![&env, batch, fresh], 2_u32).into_val(&env);
    assert_eq!(
        summary.data,
        ScVal::try_from_val(&env, &expected_data).unwrap()
    );
    assert_eq!(client.get_will_history(&newest), newest_history);
    assert_eq!(client.get_will(&newest).last_checkin, start + 120);
}
