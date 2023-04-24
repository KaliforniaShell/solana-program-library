#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]

mod helpers;

use {
    helpers::*,
    solana_program_test::*,
    solana_sdk::{
        message::Message, native_token::LAMPORTS_PER_SOL, signature::Signer,
        transaction::Transaction,
    },
    spl_single_validator_pool::{id, instruction},
    test_case::test_case,
};

#[test_case(true; "success-activated")]
#[test_case(false; "success-activating")]
#[tokio::test]
async fn success(activate: bool) {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts
        .initialize_for_deposit(&mut context, TEST_STAKE_AMOUNT, None)
        .await;

    if activate {
        advance_epoch(&mut context).await;
    }

    let wallet_lamports_after_stake =
        get_account(&mut context.banks_client, &accounts.alice.pubkey())
            .await
            .lamports;

    let (_, alice_stake_before_deposit, stake_lamports) =
        get_stake_account(&mut context.banks_client, &accounts.alice_stake.pubkey()).await;
    let alice_stake_before_deposit = alice_stake_before_deposit.unwrap().delegation.stake;

    let (_, pool_stake_before, pool_lamports_before) =
        get_stake_account(&mut context.banks_client, &accounts.stake_account).await;
    let pool_stake_before = pool_stake_before.unwrap().delegation.stake;

    let mut fees =
        USER_STARTING_SOL * LAMPORTS_PER_SOL - wallet_lamports_after_stake - stake_lamports;

    let instructions = instruction::deposit(
        &id(),
        &accounts.vote_account.pubkey(),
        &accounts.alice_stake.pubkey(),
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
        .get_account(accounts.alice_stake.pubkey())
        .await
        .expect("get_account")
        .is_none());

    // when active, the depositor gets their rent back, but when activating, its just added to stake
    let expected_deposit = if activate {
        alice_stake_before_deposit
    } else {
        stake_lamports
    };

    // entire stake has moved to pool
    assert_eq!(pool_stake_before + expected_deposit, pool_stake_after);

    // pool only gained stake
    assert_eq!(pool_lamports_after, pool_lamports_before + expected_deposit,);
    assert_eq!(
        pool_lamports_after,
        pool_stake_before + expected_deposit + pool_meta_after.rent_exempt_reserve
    );

    // alice got her rent back if active, or only paid fees otherwise
    assert_eq!(
        wallet_lamports_after_deposit,
        USER_STARTING_SOL * LAMPORTS_PER_SOL - expected_deposit - fees
    );

    // alice got tokens. no rewards have been paid so tokens correspond to stake 1:1
    assert_eq!(
        get_token_balance(&mut context.banks_client, &accounts.alice_token).await,
        expected_deposit,
    );
}

#[test_case(true; "fail-autodeposit-activated")]
#[test_case(false; "fail-autodeposit-activating")]
#[tokio::test]
async fn fail_autodeposit(activate: bool) {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await;

    if activate {
        advance_epoch(&mut context).await;
    }

    let instruction = instruction::deposit_stake(
        &id(),
        &accounts.vote_account.pubkey(),
        &accounts.stake_account,
        &accounts.alice_token,
        &accounts.alice.pubkey(),
    );
    let message = Message::new(&[instruction], Some(&accounts.alice.pubkey()));
    let transaction = Transaction::new(&[&accounts.alice], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap_err();
}

// TODO deposit via seed, deposit with extra lamports mints them
// cannot deposit activated into activating, cannot deposit activating into activated

// XXX TODO ok next i want to...
// * test create_and_delegate_user_stake
// * negative cases listed above and in withdraw
// * test the token math stochastically
