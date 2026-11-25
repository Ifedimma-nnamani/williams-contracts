use soroban_sdk::{
    testutils::{Address as _, Events, IssuerFlags, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    vec,
    xdr::ToXdr,
    Address, Bytes, Env, Vec,
};
use will::{
    Allocation, Beneficiary, WillContract, WillContractClient, WillError, MAX_HASHED_BENEFICIARIES,
};

const DAY: u64 = 86_400;
const MAX_TOKENS: u32 = 10;

fn profile(wasm: Option<&[u8]>) {
    let env = Env::default();
    // Meter without aborting at the SDK's test budgets, then assert the
    // relevant transaction resource limits for release and each claim.
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let contract_id = match wasm {
        Some(wasm) => env.register(wasm, ()),
        None => env.register(WillContract, ()),
    };
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let mut tokens = Vec::new(&env);
    for _ in 0..MAX_TOKENS {
        let asset = env.register_stellar_asset_contract_v2(owner.clone());
        asset.issuer().set_flag(IssuerFlags::RevocableFlag);
        let token = asset.address();
        StellarAssetClient::new(&env, &token).mint(&owner, &1_000_000);
        tokens.push_back((token, 1_000_000_i128));
    }
    let client = WillContractClient::new(&env, &contract_id);
    let mut visible = Vec::new(&env);
    for _ in 0..3 {
        visible.push_back(Beneficiary {
            address: Address::generate(&env),
            allocation: Allocation::FixedAmount(100_000),
        });
    }
    let id = client.create_will(
        &owner,
        &tokens,
        &visible,
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    let mut claims = std::vec::Vec::new();
    for slot in 0..=MAX_HASHED_BENEFICIARIES {
        let claimant = Address::generate(&env);
        let mut preimage: Bytes = env
            .crypto()
            .sha256(&claimant.clone().to_xdr(&env))
            .to_bytes()
            .into();
        preimage.append(&Bytes::from_array(&env, &[slot as u8; 32]));
        let commitment: Bytes = env.crypto().sha256(&preimage).to_bytes().into();
        if slot == MAX_HASHED_BENEFICIARIES {
            let before = client.get_will(&id);
            assert_eq!(
                client.try_add_hashed_beneficiary(&id, &owner, &commitment, &1),
                Err(Ok(WillError::TooManyBeneficiaries.into()))
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(client.get_will(&id), before);
        } else {
            client.add_hashed_beneficiary(&id, &owner, &commitment, &100);
            assert_eq!(client.get_will(&id).hashed_beneficiaries.len(), slot + 1);
            claims.push((claimant, preimage));
        }
    }

    let primary = tokens.get(0).unwrap().0;
    // Fail one visible transfer during release and one hashed transfer during
    // the first claim. Other tokens and beneficiaries should still be paid.
    let blocked_visible = visible.get(0).unwrap().address;
    let blocked_claimant = &claims.last().unwrap().0;
    let admin = StellarAssetClient::new(&env, &primary);
    admin.set_authorized(&blocked_visible, &false);
    admin.set_authorized(blocked_claimant, &false);
    env.ledger().with_mut(|ledger| ledger.timestamp += 91 * DAY);
    client.trigger_will(&id);
    env.ledger().with_mut(|ledger| ledger.timestamp += 8 * DAY);
    client.release_inheritance(&id, &None);
    assert_resources(&env, "release");
    let primary_client = TokenClient::new(&env, &primary);
    assert_eq!(primary_client.balance(&blocked_visible), 0);
    assert_eq!(
        primary_client.balance(&visible.get(1).unwrap().address),
        100_000
    );

    // Claim in reverse order, exercising the longest initial lookup.
    for (claimant, preimage) in claims.iter().rev() {
        client.reveal_and_claim(&id, claimant, preimage);
        assert_resources(&env, "claim");
        for (token, _) in tokens.iter() {
            let balance = TokenClient::new(&env, &token).balance(claimant);
            if claimant == blocked_claimant && token == primary {
                assert_eq!(balance, 0);
            } else {
                assert!(balance > 0);
            }
        }
    }
    let will = client.get_will(&id);
    assert!(will.hashed_beneficiaries.iter().all(|slot| slot.claimed));
    assert!(will.balances.iter().all(|(_, balance)| balance == 0));

    admin.set_authorized(&blocked_visible, &true);
    admin.set_authorized(blocked_claimant, &true);
    client.retry_failed_payout(&id, &primary, &blocked_visible);
    client.retry_failed_payout(&id, &primary, blocked_claimant);
    assert_eq!(primary_client.balance(&blocked_visible), 100_000);
    assert!(primary_client.balance(blocked_claimant) > 0);
}

fn assert_resources(env: &Env, call: &str) {
    let cost = env.cost_estimate().resources();
    println!("{call}: {cost:?}");
    assert!(cost.instructions <= 100_000_000);
    assert!(cost.mem_bytes <= 40 * 1024 * 1024);
    assert!(cost.write_entries <= 200);
    assert!(cost.disk_read_entries <= 200);
    assert!(cost.memory_read_entries + cost.disk_read_entries <= 400);
    assert!(cost.disk_read_bytes <= 200_000);
    assert!(cost.write_bytes <= 132_096);
    assert!(cost.contract_events_size_bytes <= 16_384);
}

#[test]
fn hashed_slot_limit_and_native_resources() {
    profile(None);
}

#[test]
#[ignore = "requires the release WASM artifact; set WILL_WASM to its path"]
fn hashed_slot_limit_and_wasm_resources() {
    let path = std::env::var_os("WILL_WASM").expect("set WILL_WASM to the compiled contract");
    let wasm = std::fs::read(path).expect("read compiled contract");
    profile(Some(&wasm));
}
