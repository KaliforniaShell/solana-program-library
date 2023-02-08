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
    spl_stake_birdbath as spool, spl_stake_pool as mpool,
    spl_stake_pool::{error, id, instruction, state, MINIMUM_RESERVE_LAMPORTS},
    spl_token_2022::extension::ExtensionType,
    test_case::test_case,
};

#[test_case(Env::SinglePool ; "single-pool")]
#[test_case(Env::MultiPoolTokenkeg ; "multi-pool tokenkeg")]
#[test_case(Env::MultiPoolToken22 ; "multi-pool token22")]
#[tokio::test]
async fn success(env: Env) {
    let (mut banks_client, payer, recent_blockhash) = env.program_test().start().await;

    // XXX ok now how am i managine the accounts lol...
    // either a wrapper enum or a trait. getting this to work with the type system will be a pain
    // i cant have direct field access but
    // i guess i can have a trait over both accounts objects, that has all fields of both
    // and just have it panic on the ones that dont match
    // alternatively uh... well, traits are dynamic dispatch remember, generics are static
    // wait but the... gah this is confusing
    // i cant directly access struct fields regardless of trait vs generic because
    // so i could wrap in

    // ok umm hmm lets see
    // * if i make this generic over PoolAccounts types, i cant access field
    //   i would need to have all my methods i call generic too
    // * if i have a trait over both then i can impl functions that get the individual fields...
    //   i could also have two methods that return the struct as a concrete type
    //   and... just return default for one of the other...?
    //

    // ok fundamentally what actually needs to be generic...
    // i think i just need to encapsulate init, deposit, withdraw...?
    // and all the other

    // ok. cool. plan when i get back
    // two structs, trait with initialize, deposit, withdraw, and maybe some "is everything chill" validation method
    // and then... do i imple stuff like create stake account on it?
    // hmm actually what if instead of a trait i just... impled everything on Env
    // change it to Env maybe. so env.initialize_pool() and so on
    // and it can carry all the logic, impled once or twice as needed. actually this is perfect yea
    // if we need to get any addresses out we have functions for those too. perfect

    /*
        let stake_pool_accounts = StakePoolAccounts::new_with_token_program(token_program_id);
        stake_pool_accounts
            .initialize_stake_pool(
                &mut banks_client,
                &payer,
                &recent_blockhash,
                mpool::MINIMUM_RESERVE_LAMPORTS,
            )
            .await
            .unwrap();

        // Stake pool now exists
        let stake_pool = get_account(&mut banks_client, &stake_pool_accounts.stake_pool.pubkey()).await;
        assert_eq!(stake_pool.data.len(), get_packed_len::<state::StakePool>());
        assert_eq!(stake_pool.owner, id());

        // Validator stake list storage initialized
        let validator_list = get_account(
            &mut banks_client,
            &stake_pool_accounts.validator_list.pubkey(),
        )
        .await;
        let validator_list =
            try_from_slice_unchecked::<state::ValidatorList>(validator_list.data.as_slice()).unwrap();
        assert!(validator_list.header.is_valid());
    */
}
