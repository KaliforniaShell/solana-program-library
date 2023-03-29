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
        signature::{Keypair, Signer},
        transaction::{Transaction, TransactionError},
        transport::TransportError,
    },
    spl_token::state::{Mint, Account},
    test_case::test_case,
};

#[tokio::test]
async fn success() {
    let mut context = program_test().start_with_context().await;
    let accounts = SinglePoolAccounts::default();
    accounts.initialize(&mut context).await.unwrap();

    // mint exists
    let mint_account = get_account(&mut context.banks_client, &accounts.mint).await;
    Mint::unpack_from_slice(&mint_account.data).unwrap();

    // stake account exists
    let stake_account =
        get_account(&mut context.banks_client, &accounts.stake_account).await;
    assert_eq!(stake_account.owner, stake::program::id());
}

// TODO port over the fails... some are multi-only but def want to make sure double init fails
