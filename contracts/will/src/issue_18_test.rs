#![cfg(test)]

//! Regression tests for issue #18: `reveal_and_claim` executed token
//! transfers directly, so a single failing transfer panicked the whole
//! claim — no failed-payout record was created and a multi-token hashed
//! beneficiary could not receive *any* of their assets. Failures now flow
//! through the same `pay_or_record_failure` ledger `distribute` uses (#459),
//! so the claim commits and each token's share is retried individually.

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    vec,
    xdr::ToXdr,
    Address, Bytes, Env, Vec as SorobanVec,
};

use crate::{storage, Allocation, Beneficiary, WillContract, WillContractClient, PREIMAGE_LENGTH};

const DAY: u64 = 86_400;

#[contracttype]
#[derive(Clone)]
enum TokenDataKey {
    Balance(Address),
    FailAll,
}

/// A minimal SEP-41-shaped token whose transfers can be switched to fail
/// wholesale, simulating a paused token contract or a destination the token
/// refuses to pay.
#[contract]
pub struct FailableToken;

#[contractimpl]
impl FailableToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = TokenDataKey::Balance(to.clone());
        let balance: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(balance + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&TokenDataKey::Balance(id))
            .unwrap_or(0)
    }

    /// `create_will` and `batch_create_wills` probe every token with a
    /// read-only `decimals()` call, so this mock must answer it like a real
    /// SEP-41 token to be usable as a will's locked asset.
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    /// Transfer tokens while optionally rejecting impossible movements.
    ///
    /// * Rejects negative amounts.
    /// * Rejects transfers that would cause the sender's balance to go below zero.
    /// * Honors the `FailAll` switch used in other tests to simulate a fully
    ///   paused token contract.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), soroban_sdk::Error> {
        // Disallow nonsensical transfer amounts.
        if amount <= 0 {
            return Err(soroban_sdk::Error::from_contract_error(1)); // Invalid amount
        }

        // If the token is globally set to fail, abort.
        if env.storage().instance().has(&TokenDataKey::FailAll) {
            return Err(soroban_sdk::Error::from_contract_error(2)); // Forced failure
        }

        // Ensure the sender has sufficient balance.
        let sender_key = TokenDataKey::Balance(from.clone());
        let sender_balance: i128 = env.storage().instance().get(&sender_key).unwrap_or(0);
        if sender_balance < amount {
            return Err(soroban_sdk::Error::from_contract_error(3)); // Insufficient balance
        }

        // Perform the transfer.
        env.storage().instance().set(&sender_key, &(sender_balance - amount));
        let receiver_key = TokenDataKey::Balance(to.clone());
        let receiver_balance: i128 = env.storage().instance().get(&receiver_key).unwrap_or(0);
        env.storage().instance().set(&receiver_key, &(receiver_balance + amount));
        Ok(())
    }
}

/// Issue #18: on a multi-token will, one token failing must not block the
/// claimant's other token shares — each failing token is recorded
/// individually while the healthy token pays out in the same call.
#[test]
fn one_failing_token_does_not_block_the_other_tokens_share() {
    let (env, client, token_client, owner, token_address, contract_id) = setup();
    let claimant = Address::generate(&env);

    // A second failable token: the will's non-primary token.
    let token_b_id = env.register(FailableToken, ());
    let token_b_client = FailableTokenClient::new(&env, &token_b_id);
    token_b_client.mint(&owner, &2_000_000);

    let (preimage, commitment) = preimage_and_commitment(&env, &claimant);
    let tokens: SorobanVec<(Address, i128)> = vec![
        &env,
        (token_address.clone(), 1_000_000_i128),
        (token_b_id.clone(), 2_000_000_i128),
    ];
    let will_id = create_will_with_hashed(&env, &client, &owner, &tokens, &commitment);

    advance(&env, 91);
    client.trigger_will(&will_id);
    advance(&env, 8);
    client.release_inheritance(&will_id, &None);

    // Only the secondary token starts failing; the primary stays healthy.
    token_b_client.set_fail_all(&true);

    let result = client.try_reveal_and_claim(&will_id, &claimant, &preimage);
    assert!(
        result.is_ok(),
        "one failing token must not abort the multi-token claim, got {result:?}"
    );

    // The healthy token's share was delivered in the same call.
    assert_eq!(
        token_client.balance(&claimant),
        500_000,
        "the primary token's share must be paid despite the secondary failing"
    );
    // The failing token's share is recorded for retry instead.
    assert_eq!(token_b_client.balance(&claimant), 0);
    assert_eq!(
        env.as_contract(&contract_id, || storage::get_failed_payout(
            &env,
            will_id,
            &token_b_id,
            &claimant
        )),
        Some(1_000_000),
        "the secondary token's share must be recorded for retry_failed_payout"
    );

    token_b_client.set_fail_all(&false);
    client.retry_failed_payout(&will_id, &token_b_id, &claimant);
    assert_eq!(token_b_client.balance(&claimant), 1_000_000);
}

/// Issue #213: `cancel_will` refunds the owner token by token. One token
/// refusing its refund must not roll back the other tokens' refunds; the
/// undelivered amount is recorded for exactly-once recovery, and the
/// cancellation statistics / index cleanup still complete.
#[test]
fn cancel_will_isolates_a_failing_owner_refund_and_allows_recovery() {
    let (env, client, token_client, owner, token_address, contract_id) = setup();

    // A second failable token: the will's non-primary token.
    let token_b_id = env.register(FailableToken, ());
    let token_b_client = FailableTokenClient::new(&env, &token_b_id);
    token_b_client.mint(&owner, &2_000_000);

    let tokens: SorobanVec<(Address, i128)> = vec![
        &env,
        (token_address.clone(), 1_000_000_i128),
        (token_b_id.clone(), 2_000_000_i128),
    ];
    let visible = Address::generate(&env);
    let will_id = client.create_will(
        &owner,
        &tokens,
        &vec![
            &env,
            Beneficiary {
                address: visible,
                allocation: Allocation::FixedAmount(500_000),
            },
        ],
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    // Only the secondary token starts refusing transfers.
    token_b_client.set_fail_all(&true);

    let result = client.try_cancel_will(&will_id, &owner);
    assert!(
        result.is_ok(),
        "one failing token must not abort the cancellation, got {result:?}"
    );

    // The healthy token's refund was delivered in the same call.
    assert_eq!(
        token_client.balance(&owner),
        1_000_000,
        "the primary token's refund must be paid despite the secondary failing"
    );
    // The failing token's refund is recorded for retry instead.
    assert_eq!(token_b_client.balance(&owner), 2_000_000);
    assert_eq!(
        env.as_contract(&contract_id, || storage::get_failed_payout(
            &env,
            will_id,
            &token_b_id,
            &owner
        )),
        Some(2_000_000),
        "the secondary token's refund must be recorded for retry_failed_payout"
    );

    // Cancellation state and statistics are consistent despite the deferral.
    let will = client.get_will(&will_id);
    assert!(will.cancelled);
    assert_eq!(client.get_cancelled_count(), 1);
    assert_eq!(client.get_active_will_count(), 0);

    // Once transfers work again the owner recovers the deferred refund.
    token_b_client.set_fail_all(&false);
    client.retry_failed_payout(&will_id, &token_b_id, &owner);
    assert_eq!(token_b_client.balance(&owner), 4_000_000);
    assert_eq!(
        env.as_contract(&contract_id, || storage::get_failed_payout(
            &env,
            will_id,
            &token_b_id,
            &owner
        )),
        None,
        "a delivered retry must clear the record"
    );

    // Exactly-once: a second retry must not pay the owner again.
    let second = client.try_retry_failed_payout(&will_id, &token_b_id, &owner);
    assert!(
        second.is_err(),
        "a delivered refund must not be recoverable twice, got {second:?}"
    );
    assert_eq!(token_b_client.balance(&owner), 4_000_000);
}
