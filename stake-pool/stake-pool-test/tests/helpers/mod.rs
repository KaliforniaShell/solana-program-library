#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    crate::{multi_pool::MultiPoolAccounts, single_pool::SinglePoolAccounts},
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
    spl_associated_token_account as atoken, spl_stake_pool as mpool,
    spl_stake_single_pool as spool,
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

pub mod multi_pool;
pub mod single_pool;

pub mod vote_legacy;
pub use vote_legacy::*;

pub mod token;
pub use token::*;

// XXX TODO FIXME need a build.rs to ensure all my program bins exist

pub const FIRST_NORMAL_EPOCH: u64 = 15;
pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EnvBuilder {
    SinglePool,
    SinglePoolLegacyVote,
    MultiPool,
    MultiPoolToken22,
}
impl EnvBuilder {
    pub fn env(self) -> Env {
        match self {
            EnvBuilder::SinglePool => Env::SinglePool(SinglePoolAccounts::default()),
            EnvBuilder::MultiPool => Env::MultiPool(MultiPoolAccounts::default()),
            EnvBuilder::SinglePoolLegacyVote => Env::SinglePool(SinglePoolAccounts {
                legacy_vote: true,
                ..SinglePoolAccounts::default()
            }),
            EnvBuilder::MultiPoolToken22 => Env::MultiPool(MultiPoolAccounts {
                token_program_id: spl_token_2022::id(),
                ..Default::default()
            }),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Env {
    SinglePool(SinglePoolAccounts),
    MultiPool(MultiPoolAccounts),
}
impl Env {
    pub fn program_test(&self) -> ProgramTest {
        let mut program_test = ProgramTest::default();
        program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);

        match self {
            Env::SinglePool(_) => {
                program_test.add_program(
                    "spl_stake_single_pool",
                    spool::id(),
                    processor!(spool::processor::Processor::process),
                );
                program_test.deactivate_feature(stake_raise_minimum_delegation_to_1_sol::id());
            }
            Env::MultiPool(_) => {
                program_test.add_program(
                    "spl_stake_pool",
                    mpool::id(),
                    processor!(mpool::processor::Processor::process),
                );
                program_test.add_program(
                    "spl_token_2022",
                    spl_token_2022::id(),
                    processor!(spl_token_2022::processor::Processor::process),
                );
            }
        }

        program_test.prefer_bpf(false);
        program_test
    }

    pub async fn initialize(&self, context: &mut ProgramTestContext) -> Result<(), TransportError> {
        match self {
            Env::SinglePool(accounts) => accounts.initialize(context).await,
            Env::MultiPool(accounts) => accounts.initialize(context).await,
        }
    }

    pub fn unwrap_single(self) -> SinglePoolAccounts {
        match self {
            Env::SinglePool(accounts) => accounts,
            Env::MultiPool(_) => panic!("cannot unwrap_single a MultiPool"),
        }
    }

    pub fn unwrap_multi(self) -> MultiPoolAccounts {
        match self {
            Env::MultiPool(accounts) => accounts,
            Env::SinglePool(_) => panic!("cannot unwrap_multi a SinglePool"),
        }
    }

    pub fn mint_address(&self) -> Pubkey {
        match self {
            Env::SinglePool(accounts) => accounts.mint,
            Env::MultiPool(accounts) => accounts.pool_mint.pubkey(),
        }
    }

    pub fn set_deposit_authority(&mut self, stake_deposit_authority: Keypair) {
        match self {
            Env::SinglePool(_) => panic!("dont do that"),
            // TODO FIXME check that this actually works, clippy said i dont need to borrow...
            Env::MultiPool(accounts) => {
                accounts.stake_deposit_authority = stake_deposit_authority.pubkey();
                accounts.stake_deposit_authority_keypair = Some(stake_deposit_authority);
            }
        }
    }

    pub fn set_reserve_lamports(&mut self, reserve_lamports: u64) {
        match self {
            Env::SinglePool(_) => panic!("dont do that"),
            // TODO FIXME check that this actually works, clippy said i dont need to borrow...
            Env::MultiPool(accounts) => {
                accounts.reserve_lamports = reserve_lamports;
            }
        }
    }
}

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> SolanaAccount {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("client error")
        .expect("account not found")
}

pub async fn create_vote(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    validator: &Keypair,
    vote: &Keypair,
) {
    let rent = banks_client.get_rent().await.unwrap();
    let rent_voter = rent.minimum_balance(VoteState::size_of());

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &validator.pubkey(),
        rent.minimum_balance(0),
        0,
        &system_program::id(),
    )];
    instructions.append(&mut vote_instruction::create_account(
        &payer.pubkey(),
        &vote.pubkey(),
        &VoteInit {
            node_pubkey: validator.pubkey(),
            authorized_voter: validator.pubkey(),
            authorized_withdrawer: validator.pubkey(),
            ..VoteInit::default()
        },
        rent_voter,
    ));

    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[validator, vote, payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn create_independent_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
    authorized: &stake::state::Authorized,
    lockup: &stake::state::Lockup,
    stake_amount: u64,
) -> u64 {
    let rent = banks_client.get_rent().await.unwrap();
    let lamports =
        rent.minimum_balance(std::mem::size_of::<stake::state::StakeState>()) + stake_amount;

    let transaction = Transaction::new_signed_with_payer(
        &stake::instruction::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            authorized,
            lockup,
            lamports,
        ),
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn create_blank_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Keypair,
) -> u64 {
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(std::mem::size_of::<stake::state::StakeState>()) + 1;

    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::create_account(
            &payer.pubkey(),
            &stake.pubkey(),
            lamports,
            std::mem::size_of::<stake::state::StakeState>() as u64,
            &stake::program::id(),
        )],
        Some(&payer.pubkey()),
        &[payer, stake],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    lamports
}

pub async fn delegate_stake_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    stake: &Pubkey,
    authorized: &Keypair,
    vote: &Pubkey,
) {
    let mut transaction = Transaction::new_with_payer(
        &[stake::instruction::delegate_stake(
            stake,
            &authorized.pubkey(),
            vote,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[payer, authorized], *recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}

pub async fn transfer(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    recipient: &Pubkey,
    amount: u64,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            recipient,
            amount,
        )],
        Some(&payer.pubkey()),
        &[payer],
        *recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();
}
