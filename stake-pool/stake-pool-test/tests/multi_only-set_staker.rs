#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]

mod helpers;

use {
    borsh::BorshSerialize,
    helpers::*,
    solana_program::{
        borsh::try_from_slice_unchecked,
        hash::Hash,
        instruction::{AccountMeta, Instruction},
    },
    solana_program_test::*,
    solana_sdk::{
        instruction::InstructionError,
        signature::{Keypair, Signer},
        transaction::{Transaction, TransactionError},
        transport::TransportError,
    },
    spl_stake_pool::{error, id, instruction, state},
    test_case::test_case,
};

async fn setup(env: &Env) -> (BanksClient, Keypair, Hash, Keypair) {
    let mut context = env.program_test().start_with_context().await;
    env.initialize(&mut context).await.unwrap();

    let new_staker = Keypair::new();

    (
        context.banks_client,
        context.payer,
        context.last_blockhash,
        new_staker,
    )
}

#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
#[tokio::test]
async fn success_set_staker_as_manager(env: Env) {
    let (mut banks_client, payer, recent_blockhash, new_staker) = setup(&env).await;
    let stake_pool_accounts = env.unwrap_multi();

    let mut transaction = Transaction::new_with_payer(
        &[instruction::set_staker(
            &id(),
            &stake_pool_accounts.stake_pool.pubkey(),
            &stake_pool_accounts.manager.pubkey(),
            &new_staker.pubkey(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &stake_pool_accounts.manager], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
    let stake_pool =
        try_from_slice_unchecked::<state::StakePool>(stake_pool.data.as_slice()).unwrap();

    assert_eq!(stake_pool.staker, new_staker.pubkey());
}

#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
#[tokio::test]
async fn success_set_staker_as_staker(env: Env) {
    let (mut banks_client, payer, recent_blockhash, new_staker) = setup(&env).await;
    let stake_pool_accounts = env.unwrap_multi();

    let mut transaction = Transaction::new_with_payer(
        &[instruction::set_staker(
            &id(),
            &stake_pool_accounts.stake_pool.pubkey(),
            &stake_pool_accounts.staker.pubkey(),
            &new_staker.pubkey(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &stake_pool_accounts.staker], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
    let stake_pool =
        try_from_slice_unchecked::<state::StakePool>(stake_pool.data.as_slice()).unwrap();

    assert_eq!(stake_pool.staker, new_staker.pubkey());

    let mut transaction = Transaction::new_with_payer(
        &[instruction::set_staker(
            &id(),
            &stake_pool_accounts.stake_pool.pubkey(),
            &new_staker.pubkey(),
            &stake_pool_accounts.staker.pubkey(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &new_staker], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
    let stake_pool =
        try_from_slice_unchecked::<state::StakePool>(stake_pool.data.as_slice()).unwrap();

    assert_eq!(stake_pool.staker, stake_pool_accounts.staker.pubkey());
}

#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
#[tokio::test]
async fn fail_wrong_manager(env: Env) {
    let (mut banks_client, payer, recent_blockhash, new_staker) = setup(&env).await;
    let stake_pool_accounts = env.unwrap_multi();

    let mut transaction = Transaction::new_with_payer(
        &[instruction::set_staker(
            &id(),
            &stake_pool_accounts.stake_pool.pubkey(),
            &new_staker.pubkey(),
            &new_staker.pubkey(),
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &new_staker], recent_blockhash);
    let transaction_error = banks_client
        .process_transaction(transaction)
        .await
        .err()
        .unwrap()
        .into();

    match transaction_error {
        TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        )) => {
            let program_error = error::StakePoolError::SignatureMissing as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while malicious try to set manager"),
    }
}

#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
#[tokio::test]
async fn fail_set_staker_without_signature(env: Env) {
    let (mut banks_client, payer, recent_blockhash, new_staker) = setup(&env).await;
    let stake_pool_accounts = env.unwrap_multi();

    let data = instruction::StakePoolInstruction::SetStaker
        .try_to_vec()
        .unwrap();
    let accounts = vec![
        AccountMeta::new(stake_pool_accounts.stake_pool.pubkey(), false),
        AccountMeta::new_readonly(stake_pool_accounts.manager.pubkey(), false),
        AccountMeta::new_readonly(new_staker.pubkey(), false),
    ];
    let instruction = Instruction {
        program_id: id(),
        accounts,
        data,
    };

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], recent_blockhash);
    let transaction_error = banks_client
        .process_transaction(transaction)
        .await
        .err()
        .unwrap()
        .into();

    match transaction_error {
        TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        )) => {
            let program_error = error::StakePoolError::SignatureMissing as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while try to set new manager without signature"),
    }
}
