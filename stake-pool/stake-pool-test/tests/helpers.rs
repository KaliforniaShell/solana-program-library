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
    spl_associated_token_account as atoken, spl_stake_birdbath as spool, spl_stake_pool as mpool,
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

// XXX TODO FIXME i need to ask jon about how to build shit for this shit
// rn i am just running cargo build-sbf on the toplevel and hoping that fixes it locally
// but that doesnt work for ci. i might have to write a script like in token-program-test
// but where the hell does mpl metadata come from?
// thread 'success::single_pool' panicked at 'Program file data not available for mpl_token_metadata (metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s)', /home/hana/.cargo/registry/src/github.com-1ecc6299db9ec823/solana-program-test-1.14.10/src/lib.rs:680:17
// actually come to think of it, why do i even need this? arent we just using the processor functions?
// and if i dont have this, why do the existing tests work??

// XXX copy-paste from initialize.rs
// two structs, trait with initialize, deposit, withdraw, and maybe some "is everything chill" validation method
// and then... do i imple stuff like create stake account on it?
// hmm actually what if instead of a trait i just... impled everything on Env
// change it to Env maybe. so env.initialize_pool() and so on
// and it can carry all the logic, impled once or twice as needed. actually this is perfect yea
// if we need to get any addresses out we have functions for those too. perfect

pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;
pub const MAX_TEST_VALIDATORS: u32 = 10_000;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Env {
    SinglePool,
    MultiPoolTokenkeg,
    MultiPoolToken22,
}
impl Env {
    pub fn token_program_id(&self) -> Pubkey {
        match self {
            Env::MultiPoolToken22 => spl_token_2022::id(),
            _ => spl_token::id(),
        }
    }

    pub fn is_multi(&self) -> bool {
        match self {
            Env::SinglePool => false,
            _ => true,
        }
    }

    pub fn program_test(&self) -> ProgramTest {
        let mut program_test = ProgramTest::default();
        // FIXME figure out how to build this
        // program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);

        match self {
            Env::SinglePool => {
                program_test.add_program(
                    "spl_stake_birdbath",
                    spool::id(),
                    processor!(spool::processor::Processor::process),
                );
                program_test.deactivate_feature(stake_raise_minimum_delegation_to_1_sol::id());
            }
            _ => {
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

    // a new() for single-pool is unnecessary because the Default impl is sufficient in all cases
    pub fn new_multi_pool(&self, maybe_deposit_authority: Option<Keypair>) -> MultiPoolAccounts {
        match self {
            Env::SinglePool => panic!("dont do that"),
            _ => {
                if let Some(stake_deposit_authority) = maybe_deposit_authority {
                    MultiPoolAccounts {
                        stake_deposit_authority: stake_deposit_authority.pubkey(),
                        stake_deposit_authority_keypair: Some(stake_deposit_authority),
                        token_program_id: self.token_program_id(),
                        ..Default::default()
                    }
                } else {
                    MultiPoolAccounts {
                        token_program_id: self.token_program_id(),
                        ..Default::default()
                    }
                }
            }
        }
    }
}

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
            stake_account: spool::find_pool_stake_address(&spool::id(), &vote_account.pubkey()).0,
            authority: spool::find_pool_authority_address(&spool::id(), &vote_account.pubkey()).0,
            mint: spool::find_pool_mint_address(&spool::id(), &vote_account.pubkey()).0,
            vote_account,
            token_program_id: spl_token::id(),
        }
    }
}

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
    pub epoch_fee: mpool::state::Fee,
    pub withdrawal_fee: mpool::state::Fee,
    pub deposit_fee: mpool::state::Fee,
    pub referral_fee: u8,
    pub sol_deposit_fee: mpool::state::Fee,
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
            mpool::find_deposit_authority_program_address(&mpool::id(), stake_pool_address);
        let (withdraw_authority, _) =
            mpool::find_withdraw_authority_program_address(&mpool::id(), stake_pool_address);
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
            epoch_fee: mpool::state::Fee {
                numerator: 1,
                denominator: 100,
            },
            withdrawal_fee: mpool::state::Fee {
                numerator: 3,
                denominator: 1000,
            },
            deposit_fee: mpool::state::Fee {
                numerator: 1,
                denominator: 1000,
            },
            referral_fee: 25,
            sol_deposit_fee: mpool::state::Fee {
                numerator: 3,
                denominator: 100,
            },
            sol_referral_fee: 50,
            max_validators: MAX_TEST_VALIDATORS,
            compute_unit_limit: None,
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

pub trait TestAccounts {}
