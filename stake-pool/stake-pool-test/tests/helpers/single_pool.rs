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
        signature::{Keypair, Signer},
        transaction::Transaction,
        transport::TransportError,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState, VoteStateVersions},
    },
    spl_associated_token_account as atoken,
    spl_stake_birdbath::{
        self, find_pool_authority_address, find_pool_mint_address, find_pool_stake_address, id,
    },
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

#[derive(Debug, PartialEq)]
pub struct SinglePoolAccounts {
    pub validator: Keypair,
    pub vote_account: Keypair,
    pub stake_account: Pubkey,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub token_program_id: Pubkey,
}
impl Default for SinglePoolAccounts {
    fn default() -> Self {
        let vote_account = Keypair::new();

        Self {
            validator: Keypair::new(),
            authority: find_pool_authority_address(&id(), &vote_account.pubkey()).0,
            stake_account: find_pool_stake_address(&id(), &vote_account.pubkey()).0,
            mint: find_pool_mint_address(&id(), &vote_account.pubkey()).0,
            vote_account,
            token_program_id: spl_token::id(),
        }
    }
}
