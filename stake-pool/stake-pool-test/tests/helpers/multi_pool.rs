#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    crate::{create_independent_stake_account, create_mint, create_token_account, create_vote},
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
        instruction,
        state::{self, FeeType},
        MINIMUM_RESERVE_LAMPORTS,
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
    pub epoch_fee: state::Fee,
    pub withdrawal_fee: state::Fee,
    pub deposit_fee: state::Fee,
    pub referral_fee: u8,
    pub sol_deposit_fee: state::Fee,
    pub sol_referral_fee: u8,
    pub max_validators: u32,
    pub compute_unit_limit: Option<u32>,
    pub reserve_lamports: u64,
}
impl MultiPoolAccounts {
    pub async fn initialize(&self, context: &mut ProgramTestContext) -> Result<(), TransportError> {
        create_mint(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.token_program_id,
            &self.pool_mint,
            &self.withdraw_authority,
            self.pool_decimals,
            &[],
        )
        .await?;

        create_token_account(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.token_program_id,
            &self.pool_fee_account,
            &self.pool_mint.pubkey(),
            &self.manager,
            &[],
        )
        .await?;

        create_independent_stake_account(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.reserve_stake,
            &stake::state::Authorized {
                staker: self.withdraw_authority,
                withdrawer: self.withdraw_authority,
            },
            &stake::state::Lockup::default(),
            self.reserve_lamports,
        )
        .await;

        create_stake_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.stake_pool,
            &self.validator_list,
            &self.reserve_stake.pubkey(),
            &self.token_program_id,
            &self.pool_mint.pubkey(),
            &self.pool_fee_account.pubkey(),
            &self.manager,
            &self.staker.pubkey(),
            &self.withdraw_authority,
            &self.stake_deposit_authority_keypair,
            &self.epoch_fee,
            &self.withdrawal_fee,
            &self.deposit_fee,
            self.referral_fee,
            &self.sol_deposit_fee,
            self.sol_referral_fee,
            self.max_validators,
        )
        .await?;

        Ok(())
    }
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
            epoch_fee: state::Fee {
                numerator: 1,
                denominator: 100,
            },
            withdrawal_fee: state::Fee {
                numerator: 3,
                denominator: 1000,
            },
            deposit_fee: state::Fee {
                numerator: 1,
                denominator: 1000,
            },
            referral_fee: 25,
            sol_deposit_fee: state::Fee {
                numerator: 3,
                denominator: 100,
            },
            sol_referral_fee: 50,
            max_validators: MAX_TEST_VALIDATORS,
            compute_unit_limit: None,
            reserve_lamports: MINIMUM_RESERVE_LAMPORTS,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create_stake_pool(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake_pool: &Keypair,
    validator_list: &Keypair,
    reserve_stake: &Pubkey,
    token_program_id: &Pubkey,
    pool_mint: &Pubkey,
    pool_token_account: &Pubkey,
    manager: &Keypair,
    staker: &Pubkey,
    withdraw_authority: &Pubkey,
    stake_deposit_authority: &Option<Keypair>,
    epoch_fee: &state::Fee,
    withdrawal_fee: &state::Fee,
    deposit_fee: &state::Fee,
    referral_fee: u8,
    sol_deposit_fee: &state::Fee,
    sol_referral_fee: u8,
    max_validators: u32,
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_stake_pool = rent.minimum_balance(get_packed_len::<state::StakePool>());
    let validator_list_size =
        get_instance_packed_len(&state::ValidatorList::new(max_validators)).unwrap();
    let rent_validator_list = rent.minimum_balance(validator_list_size);

    let mut transaction = Transaction::new_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &stake_pool.pubkey(),
                rent_stake_pool,
                get_packed_len::<state::StakePool>() as u64,
                &id(),
            ),
            system_instruction::create_account(
                &payer.pubkey(),
                &validator_list.pubkey(),
                rent_validator_list,
                validator_list_size as u64,
                &id(),
            ),
            instruction::initialize(
                &id(),
                &stake_pool.pubkey(),
                &manager.pubkey(),
                staker,
                withdraw_authority,
                &validator_list.pubkey(),
                reserve_stake,
                pool_mint,
                pool_token_account,
                token_program_id,
                stake_deposit_authority.as_ref().map(|k| k.pubkey()),
                *epoch_fee,
                *withdrawal_fee,
                *deposit_fee,
                referral_fee,
                max_validators,
            ),
            instruction::set_fee(
                &id(),
                &stake_pool.pubkey(),
                &manager.pubkey(),
                FeeType::SolDeposit(*sol_deposit_fee),
            ),
            instruction::set_fee(
                &id(),
                &stake_pool.pubkey(),
                &manager.pubkey(),
                FeeType::SolReferral(sol_referral_fee),
            ),
        ],
        Some(&payer.pubkey()),
    );
    let mut signers = vec![payer, stake_pool, validator_list, manager];
    if let Some(stake_deposit_authority) = stake_deposit_authority.as_ref() {
        signers.push(stake_deposit_authority);
    }
    transaction.sign(&signers, *recent_blockhash);
    banks_client
        .process_transaction(transaction)
        .await
        .map_err(|e| e.into())
}
