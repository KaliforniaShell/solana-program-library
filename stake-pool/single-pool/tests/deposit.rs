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

    let wallet_lamports_before = get_account(&mut context.banks_client, &accounts.alice.pubkey())
        .await
        .lamports;

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

    let wallet_lamports_after_stake =
        get_account(&mut context.banks_client, &accounts.alice.pubkey())
            .await
            .lamports;

    let (_, alice_stake_before_deposit, stake_lamports) =
        get_stake_account(&mut context.banks_client, &alice_stake.pubkey()).await;
    let alice_stake_before_deposit = alice_stake_before_deposit.unwrap().delegation.stake;

    let (_, pool_stake_before, pool_lamports_before) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;
    let pool_stake_before = pool_stake_before.unwrap().delegation.stake;

    let mut fees = wallet_lamports_before - wallet_lamports_after_stake - stake_lamports;

    let instructions = instruction::deposit(
        &id(),
        &accounts.vote_account.pubkey(),
        &alice_stake.pubkey(),
        &accounts.alice_token,
        &accounts.alice.pubkey(),
        &accounts.alice.pubkey(),
    );
    let message = Message::new(&instructions, Some(&accounts.alice.pubkey()));
    fees += get_fee_for_message(&mut context.banks_client, &message).await;
    let transaction = Transaction::new(&[&accounts.alice], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let wallet_lamports_after_deposit =
        get_account(&mut context.banks_client, &accounts.alice.pubkey())
            .await
            .lamports;

    let (pool_meta_after, pool_stake_after, pool_lamports_after) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;
    let pool_stake_after = pool_stake_after.unwrap().delegation.stake;

    // deposit stake account is closed
    assert!(context
        .banks_client
        .get_account(alice_stake.pubkey())
        .await
        .expect("get_account")
        .is_none());

    // entire alice stake has moved to pool
    assert_eq!(
        pool_stake_before + alice_stake_before_deposit,
        pool_stake_after
    );

    // pool only gained stake
    assert_eq!(
        pool_lamports_after,
        pool_lamports_before + TEST_STAKE_AMOUNT
    );
    assert_eq!(
        pool_lamports_after,
        pool_stake_before + TEST_STAKE_AMOUNT + pool_meta_after.rent_exempt_reserve
    );

    // alice got her rent back
    assert_eq!(
        wallet_lamports_after_deposit,
        wallet_lamports_before - TEST_STAKE_AMOUNT - fees
    );

    // alice got tokens. no rewards have been paid so tokens correspond to stake 1:1
    assert_eq!(
        get_token_balance(&mut context.banks_client, &accounts.alice_token).await,
        TEST_STAKE_AMOUNT
    );
}

// TODO deposit via seed, deposit during activation, deposit with extra lamports mints them
// cannot deposit zero, cannot deposit from the deposit account
