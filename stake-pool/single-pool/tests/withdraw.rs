#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]

mod helpers;

use {
    helpers::*,
    solana_program_test::*,
    solana_sdk::{message::Message, signature::Signer, transaction::Transaction},
    spl_single_validator_pool::{id, instruction},
    test_case::test_case,
};

#[test_case(true; "activated")]
#[test_case(false; "activating")]
#[tokio::test]
async fn success(activate: bool) {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    let minimum_delegation = accounts
        .initialize_for_withdraw(&mut context, TEST_STAKE_AMOUNT, None, activate)
        .await;

    let (_, _, pool_lamports_before) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;

    let wallet_lamports_before = get_account(&mut context.banks_client, &accounts.alice.pubkey())
        .await
        .lamports;

    let instructions = instruction::withdraw(
        &id(),
        &accounts.vote_account.pubkey(),
        &accounts.alice_stake.pubkey(),
        &accounts.alice.pubkey(),
        &accounts.alice_token,
        &accounts.alice.pubkey(),
        get_token_balance(&mut context.banks_client, &accounts.alice_token).await,
    );
    let message = Message::new(&instructions, Some(&accounts.alice.pubkey()));
    let fees = get_fee_for_message(&mut context.banks_client, &message).await;
    let transaction = Transaction::new(&[&accounts.alice], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let wallet_lamports_after = get_account(&mut context.banks_client, &accounts.alice.pubkey())
        .await
        .lamports;

    let (_, alice_stake_after, _) =
        get_stake_account(&mut context.banks_client, &accounts.alice_stake.pubkey()).await;
    let alice_stake_after = alice_stake_after.unwrap().delegation.stake;

    let (_, pool_stake_after, pool_lamports_after) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;
    let pool_stake_after = pool_stake_after.unwrap().delegation.stake;

    // when active, the depositor gets their rent back, but when activating, its just added to stake
    let expected_deposit = if activate {
        TEST_STAKE_AMOUNT
    } else {
        get_stake_account_rent(&mut context.banks_client).await + TEST_STAKE_AMOUNT
    };

    // alice received her stake back
    assert_eq!(alice_stake_after, expected_deposit);

    // alice paid chain fee for withdraw and nothing else
    // (we create the blank account before getting wallet_lamports_before)
    assert_eq!(wallet_lamports_after, wallet_lamports_before - fees);

    // pool retains minstake
    assert_eq!(pool_stake_after, minimum_delegation);

    // pool lamports otherwise unchanged
    assert_eq!(pool_lamports_after, pool_lamports_before - expected_deposit,);

    // alice has no tokens
    assert_eq!(
        get_token_balance(&mut context.banks_client, &accounts.alice_token).await,
        0,
    );

    // tokens were burned
    assert_eq!(
        get_token_supply(&mut context.banks_client, &accounts.mint).await,
        0,
    );
}

// TODO withdraw after rewards, withdraw after another deposit, fail withdraw from pool account
// fail with fake mint, fail with fake token value, fail fake token program, fail fake stake program
