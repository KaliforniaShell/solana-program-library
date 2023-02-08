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
    spl_stake_pool::{
        self, find_deposit_authority_program_address, find_withdraw_authority_program_address, id,
        state::Fee,
    },
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

pub const MAX_TEST_VALIDATORS: u32 = 10_000;

#[derive(Debug, PartialEq)]
pub struct MultiPoolAccounts {
    pub stake_pool: Keypair,
    pub validator_list: Keypair,
    pub reserve_stake: Keypair,
    pub token_program_id: Pubkey,
    pub pool_mint: Keypair,
    pub pool_fee_account: Keypair,
    pub pool_decimals: u8,
    pub manager: Keypair,
    pub staker: Keypair,
    pub withdraw_authority: Pubkey,
    pub stake_deposit_authority: Pubkey,
    pub stake_deposit_authority_keypair: Option<Keypair>,
    pub epoch_fee: Fee,
    pub withdrawal_fee: Fee,
    pub deposit_fee: Fee,
    pub referral_fee: u8,
    pub sol_deposit_fee: Fee,
    pub sol_referral_fee: u8,
    pub max_validators: u32,
    pub compute_unit_limit: Option<u32>,
}
impl Default for MultiPoolAccounts {
    fn default() -> Self {
        let stake_pool = Keypair::new();
        let validator_list = Keypair::new();
        let stake_pool_address = &stake_pool.pubkey();
        let (stake_deposit_authority, _) =
            find_deposit_authority_program_address(&id(), stake_pool_address);
        let (withdraw_authority, _) =
            find_withdraw_authority_program_address(&id(), stake_pool_address);
        let reserve_stake = Keypair::new();
        let pool_mint = Keypair::new();
        let pool_fee_account = Keypair::new();
        let manager = Keypair::new();
        let staker = Keypair::new();

        Self {
            stake_pool,
            validator_list,
            reserve_stake,
            token_program_id: spl_token::id(),
            pool_mint,
            pool_fee_account,
            pool_decimals: 0,
            manager,
            staker,
            withdraw_authority,
            stake_deposit_authority,
            stake_deposit_authority_keypair: None,
            epoch_fee: Fee {
                numerator: 1,
                denominator: 100,
            },
            withdrawal_fee: Fee {
                numerator: 3,
                denominator: 1000,
            },
            deposit_fee: Fee {
                numerator: 1,
                denominator: 1000,
            },
            referral_fee: 25,
            sol_deposit_fee: Fee {
                numerator: 3,
                denominator: 100,
            },
            sol_referral_fee: 50,
            max_validators: MAX_TEST_VALIDATORS,
            compute_unit_limit: None,
        }
    }
}
