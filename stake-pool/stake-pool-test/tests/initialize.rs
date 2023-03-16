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
    spl_single_validator_pool as spool, spl_stake_pool as mpool,
    spl_stake_pool::{error, id, instruction, state, MINIMUM_RESERVE_LAMPORTS},
    spl_token_2022::{
        extension::StateWithExtensionsOwned,
        state::{Account, Mint},
    },
    test_case::test_case,
};

#[test_case(EnvBuilder::SinglePool.env() ; "single-pool")]
#[test_case(EnvBuilder::MultiPool.env() ; "multi-pool")]
#[test_case(EnvBuilder::MultiPoolToken22.env() ; "multi-pool token22")]
#[tokio::test]
async fn success(env: Env) {
    let mut context = env.program_test().start_with_context().await;
    env.initialize(&mut context).await.unwrap();

    match env {
        Env::SinglePool(accounts) => {
            // mint exists
            let mint_account = get_account(&mut context.banks_client, &accounts.mint).await;
            StateWithExtensionsOwned::<Mint>::unpack(mint_account.data).unwrap();

            // stake account exists
            let stake_account =
                get_account(&mut context.banks_client, &accounts.stake_account).await;
            assert_eq!(stake_account.owner, stake::program::id());
        }
        Env::MultiPool(accounts) => {
            // Stake pool now exists
            let stake_pool =
                get_account(&mut context.banks_client, &accounts.stake_pool.pubkey()).await;
            assert_eq!(stake_pool.data.len(), get_packed_len::<state::StakePool>());
            assert_eq!(stake_pool.owner, id());

            // Validator stake list storage initialized
            let validator_list =
                get_account(&mut context.banks_client, &accounts.validator_list.pubkey()).await;
            let validator_list =
                try_from_slice_unchecked::<state::ValidatorList>(validator_list.data.as_slice())
                    .unwrap();
            assert!(validator_list.header.is_valid());
        }
    }
}

// TODO port over the fails... some are multi-only but def want to make sure double init fails
