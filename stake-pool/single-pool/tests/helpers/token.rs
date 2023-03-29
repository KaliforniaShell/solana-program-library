#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    borsh::BorshSerialize,
    mpl_token_metadata::{pda::find_metadata_account, state::Metadata},
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        hash::Hash,
        instruction::Instruction,
        program_option::COption,
        program_pack::Pack,
        pubkey::Pubkey,
        stake, system_instruction, system_program,
    },
    solana_program_test::{processor, BanksClient, ProgramTest, ProgramTestContext},
    solana_sdk::{
        account::{Account as SolanaAccount, WritableAccount},
        clock::{Clock, Epoch},
        compute_budget::ComputeBudgetInstruction,
        feature_set::stake_raise_minimum_delegation_to_1_sol,
        message::Message,
        signature::{Keypair, Signer},
        transaction::Transaction,
        transport::TransportError,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState, VoteStateVersions},
    },
    spl_associated_token_account as atoken, spl_single_validator_pool as spool,
    spl_token::state::{Mint, Account},
    std::{convert::TryInto, num::NonZeroU32},
};

pub async fn create_ata(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    owner: &Pubkey,
    recent_blockhash: &Hash,
    pool_mint: &Pubkey,
) -> Result<(), TransportError> {
    #[allow(deprecated)]
    let instruction = atoken::create_associated_token_account(&payer.pubkey(), owner, pool_mint);
    let message = Message::new(&[instruction], Some(&payer.pubkey()));
    let transaction = Transaction::new(&[payer], message, *recent_blockhash);

    banks_client
        .process_transaction(transaction)
        .await
        .map_err(|e| e.into())
}

pub async fn get_token_balance(banks_client: &mut BanksClient, token: &Pubkey) -> u64 {
    let token_account = banks_client.get_account(*token).await.unwrap().unwrap();
    let account_info = Account::unpack_from_slice(&token_account.data).unwrap();
    account_info.amount
}

pub async fn get_token_supply(banks_client: &mut BanksClient, mint: &Pubkey) -> u64 {
    let mint_account = banks_client.get_account(*mint).await.unwrap().unwrap();
    let account_info = Mint::unpack_from_slice(&mint_account.data).unwrap();
    account_info.supply
}

pub async fn get_metadata_account(banks_client: &mut BanksClient, token_mint: &Pubkey) -> Metadata {
    let (token_metadata, _) = find_metadata_account(token_mint);
    let token_metadata_account = banks_client
        .get_account(token_metadata)
        .await
        .unwrap()
        .unwrap();
    try_from_slice_unchecked(token_metadata_account.data.as_slice()).unwrap()
}
