#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]
mod helpers;

use {
    helpers::*,
    mpl_token_metadata::{
        state::{MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH, MAX_URI_LENGTH},
        utils::puffed_out_string,
    },
    solana_program::instruction::InstructionError,
    solana_program_test::*,
    solana_sdk::{
        signature::{Keypair, Signer},
        transaction::{Transaction, TransactionError},
    },
    spl_stake_pool::{
        self as mpool,
        error::StakePoolError::{SignatureMissing, WrongManager},
    },
    spl_stake_single_pool as spool,
    test_case::test_case,
};

const MULTI_NAME: &str = "test_name";
const MULTI_SYMBOL: &str = "SYM";
const MULTI_URI: &str = "test_uri";

async fn setup(env: &Env) -> ProgramTestContext {
    let mut context = env.program_test().start_with_context().await;
    env.initialize(&mut context).await.unwrap();

    if let Env::MultiPool(ref stake_pool_accounts) = env {
        let ix = mpool::instruction::create_token_metadata(
            &spl_stake_pool::id(),
            &stake_pool_accounts.stake_pool.pubkey(),
            &stake_pool_accounts.manager.pubkey(),
            &stake_pool_accounts.pool_mint.pubkey(),
            &context.payer.pubkey(),
            MULTI_NAME.to_string(),
            MULTI_SYMBOL.to_string(),
            MULTI_URI.to_string(),
        );

        let transaction = Transaction::new_signed_with_payer(
            &[ix],
            Some(&context.payer.pubkey()),
            &[&context.payer, &stake_pool_accounts.manager],
            context.last_blockhash,
        );

        context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();
    }

    context
}

#[test_case(EnvBuilder::SinglePool.env() ; "single-pool")]
// XXX this fails now #[test_case(EnvBuilder::SinglePoolLegacyVote.env() ; "single-pool-legacy-vote")]
#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
//#[test_case(EnvBuilder::MultiPoolToken22.env() ; "multi-pool token22")] enable once metaplex supports token-2022
#[tokio::test]
async fn success_update_pool_token_metadata(env: Env) {
    let mut context = setup(&env).await;

    let updated_name = "updated_name";
    let updated_symbol = "USYM";
    let updated_uri = "updated_uri";

    let puffed_name = puffed_out_string(updated_name, MAX_NAME_LENGTH);
    let puffed_symbol = puffed_out_string(updated_symbol, MAX_SYMBOL_LENGTH);
    let puffed_uri = puffed_out_string(updated_uri, MAX_URI_LENGTH);

    let (instruction, authorized_withdrawer) = match env {
        Env::SinglePool(ref stake_pool_accounts) => {
            let instruction = spool::instruction::update_token_metadata(
                &spool::id(),
                &stake_pool_accounts.vote_account.pubkey(),
                &stake_pool_accounts.validator.pubkey(),
                updated_name.to_string(),
                updated_symbol.to_string(),
                updated_uri.to_string(),
            );

            (instruction, &stake_pool_accounts.validator)
        }
        Env::MultiPool(ref stake_pool_accounts) => {
            let instruction = mpool::instruction::update_token_metadata(
                &mpool::id(),
                &stake_pool_accounts.stake_pool.pubkey(),
                &stake_pool_accounts.manager.pubkey(),
                &stake_pool_accounts.pool_mint.pubkey(),
                updated_name.to_string(),
                updated_symbol.to_string(),
                updated_uri.to_string(),
            );

            (instruction, &stake_pool_accounts.manager)
        }
    };

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&context.payer.pubkey()),
        &[&context.payer, authorized_withdrawer],
        context.last_blockhash,
    );

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let metadata = get_metadata_account(&mut context.banks_client, &env.mint_address()).await;

    assert_eq!(metadata.data.name, puffed_name);
    assert_eq!(metadata.data.symbol, puffed_symbol);
    assert_eq!(metadata.data.uri, puffed_uri);
}

// TODO test bad withdrawer, test bad vote account (edit the ixn by hand to have the correct authority)

/*
#[tokio::test]
async fn fail_manager_did_not_sign() {
    let (mut context, stake_pool_accounts) = setup().await;

    let updated_name = "updated_name";
    let updated_symbol = "USYM";
    let updated_uri = "updated_uri";

    let mut ix = instruction::update_token_metadata(
        &spl_stake_pool::id(),
        &stake_pool_accounts.stake_pool.pubkey(),
        &stake_pool_accounts.manager.pubkey(),
        &stake_pool_accounts.pool_mint.pubkey(),
        updated_name.to_string(),
        updated_symbol.to_string(),
        updated_uri.to_string(),
    );
    ix.accounts[1].is_signer = false;

    let transaction = Transaction::new_signed_with_payer(
        &[ix],
        Some(&context.payer.pubkey()),
        &[&context.payer],
        context.last_blockhash,
    );

    let error = context
        .banks_client
        .process_transaction(transaction)
        .await
        .err()
        .unwrap()
        .unwrap();

    match error {
        TransactionError::InstructionError(_, InstructionError::Custom(error_index)) => {
            let program_error = SignatureMissing as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while manager signature missing"),
    }
}

#[tokio::test]
async fn fail_wrong_manager_signed() {
    let (mut context, stake_pool_accounts) = setup().await;

    let updated_name = "updated_name";
    let updated_symbol = "USYM";
    let updated_uri = "updated_uri";

    let random_keypair = Keypair::new();
    let ix = instruction::update_token_metadata(
        &spl_stake_pool::id(),
        &stake_pool_accounts.stake_pool.pubkey(),
        &random_keypair.pubkey(),
        &stake_pool_accounts.pool_mint.pubkey(),
        updated_name.to_string(),
        updated_symbol.to_string(),
        updated_uri.to_string(),
    );

    let transaction = Transaction::new_signed_with_payer(
        &[ix],
        Some(&context.payer.pubkey()),
        &[&context.payer, &random_keypair],
        context.last_blockhash,
    );

    let error = context
        .banks_client
        .process_transaction(transaction)
        .await
        .err()
        .unwrap()
        .unwrap();

    match error {
        TransactionError::InstructionError(_, InstructionError::Custom(error_index)) => {
            let program_error = WrongManager as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while signing with the wrong manager"),
    }
}
*/
