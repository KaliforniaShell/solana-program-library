#![allow(clippy::integer_arithmetic)]
#![cfg(feature = "test-sbf")]
#![allow(unused_imports)] // FIXME remove

mod helpers;

use {
    borsh::BorshSerialize,
    helpers::*,
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        pubkey::Pubkey,
        stake, system_instruction, sysvar,
    },
    solana_program_test::*,
    solana_sdk::{
        instruction::InstructionError,
        message::Message,
        native_token::LAMPORTS_PER_SOL,
        signature::{Keypair, Signer},
        transaction::{Transaction, TransactionError},
        transport::TransportError,
    },
    spl_single_validator_pool::{id, instruction},
    spl_token::state::{Account, Mint},
    test_case::test_case,
};

#[tokio::test]
async fn success() {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await.unwrap();

    let lamps_before = context
        .banks_client
        .get_account(accounts.alice.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    let alice_stake = Keypair::new();
    let lockup = stake::state::Lockup::default();

    let authorized = stake::state::Authorized {
        staker: accounts.alice.pubkey(),
        withdrawer: accounts.alice.pubkey(),
    };

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
        1 * LAMPORTS_PER_SOL,
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

    let lamps_after_stake = context
        .banks_client
        .get_account(accounts.alice.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    advance_epoch(&mut context).await;

    let instructions = spl_single_validator_pool::instruction::deposit(
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

    assert!(context
        .banks_client
        .get_account(alice_stake.pubkey())
        .await
        .expect("get_account")
        .is_none());

    let lamps_after_deposit = context
        .banks_client
        .get_account(accounts.alice.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    // XXX note that below, you gain lamports from deposit. in pre-activate test, we lose lamports (because all are activated)
    println!("HANA lamps before staking: {}\n     lamps after staking: {} ({} less than before, {} excluding stake)\n     lamps after deposit: {} ({} more than before)", lamps_before, lamps_after_stake, lamps_before - lamps_after_stake, lamps_before - lamps_after_stake - LAMPORTS_PER_SOL, lamps_after_deposit, lamps_after_deposit - lamps_after_stake);

    // TODO check balances (also remember to do the fuzzy thing with bals)
}

// TODO deposit via seed, deposit during activation
