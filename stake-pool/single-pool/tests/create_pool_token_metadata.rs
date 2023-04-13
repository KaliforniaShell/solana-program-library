#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]
mod helpers;

use {
    helpers::*,
    mpl_token_metadata::{
        state::Metadata,
        state::{MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH, MAX_URI_LENGTH},
        utils::puffed_out_string,
    },
    solana_program::instruction::InstructionError,
    solana_program_test::*,
    solana_sdk::{
        message::Message,
        signature::{Keypair, Signer},
        transaction::{Transaction, TransactionError},
    },
    spl_single_validator_pool::{id, instruction},
    test_case::test_case,
};

fn assert_metadata(metadata: &Metadata) {
    // TODO match the actual strings once we decide on them
    assert!(!metadata.data.name.is_empty());
    assert!(!metadata.data.symbol.is_empty());
}

#[tokio::test]
async fn success() {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await.unwrap();

    let metadata = get_metadata_account(&mut context.banks_client, &accounts.mint).await;
    assert_metadata(&metadata);
}

#[tokio::test]
async fn fail_double_init() {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await.unwrap();
    refresh_blockhash(&mut context).await;

    let instruction = instruction::create_token_metadata(
        &id(),
        &accounts.vote_account.pubkey(),
        &context.payer.pubkey(),
    );
    let message = Message::new(&[instruction], Some(&context.payer.pubkey()));
    let transaction = Transaction::new(&[&context.payer], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap_err();
}
