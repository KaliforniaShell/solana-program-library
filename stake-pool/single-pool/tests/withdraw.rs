#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]

mod helpers;

use {
    helpers::*,
    solana_program::stake,
    solana_program_test::*,
    solana_sdk::{
        message::Message,
        signature::{Keypair, Signer},
        transaction::Transaction,
    },
    spl_single_validator_pool::{id, instruction},
};

#[tokio::test]
async fn success() {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await.unwrap();
    let alice_stake = Keypair::new();

    create_independent_stake_account(
        &mut context.banks_client,
        &accounts.alice,
        &context.last_blockhash,
        &alice_stake,
        &stake::state::Authorized {
            staker: accounts.alice.pubkey(),
            withdrawer: accounts.alice.pubkey(),
        },
        &stake::state::Lockup::default(),
        TEST_STAKE_AMOUNT,
    )
    .await;

    delegate_stake_account(
        &mut context.banks_client,
        &accounts.alice,
        &context.last_blockhash,
        &alice_stake.pubkey(),
        &accounts.alice,
        &accounts.vote_account.pubkey(),
    )
    .await;

    advance_epoch(&mut context).await;

    let instructions = instruction::deposit(
        &id(),
        &accounts.vote_account.pubkey(),
        &alice_stake.pubkey(),
        &accounts.alice_token,
        &accounts.alice.pubkey(),
        &accounts.alice.pubkey(),
    );
    let message = Message::new(&instructions, Some(&accounts.alice.pubkey()));
    let transaction = Transaction::new(&[&accounts.alice], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let (_, _, pool_lamports_before) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;

    // it doesnt matter if we reuse the address or not
    create_blank_stake_account(
        &mut context.banks_client,
        &accounts.alice,
        &context.last_blockhash,
        &alice_stake,
    )
    .await;

    let wallet_lamports_before = get_account(&mut context.banks_client, &accounts.alice.pubkey())
        .await
        .lamports;

    let instructions = instruction::withdraw(
        &id(),
        &accounts.vote_account.pubkey(),
        &alice_stake.pubkey(),
        &accounts.alice.pubkey(),
        &accounts.alice_token,
        &accounts.alice.pubkey(),
        TEST_STAKE_AMOUNT,
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
        get_stake_account(&mut context.banks_client, &alice_stake.pubkey()).await;
    let alice_stake_after = alice_stake_after.unwrap().delegation.stake;

    let (_, pool_stake_after, pool_lamports_after) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;
    let pool_stake_after = pool_stake_after.unwrap().delegation.stake;

    // alice received her stake back
    assert_eq!(alice_stake_after, TEST_STAKE_AMOUNT);

    // alice paid chain fee for withdraw and nothing else
    assert_eq!(wallet_lamports_after, wallet_lamports_before - fees);

    // pool retains minstake
    assert_eq!(pool_stake_after, MINIMUM_STAKE_AMOUNT);

    // pool lamports otherwise unchanged
    assert_eq!(
        pool_lamports_after,
        pool_lamports_before - TEST_STAKE_AMOUNT
    );

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

// TODO withdraw after rewards, withdraw while activating
