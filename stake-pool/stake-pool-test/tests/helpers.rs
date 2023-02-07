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

pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TestCase {
    SinglePool,
    MultiPoolTokenkeg,
    MultiPoolToken22,
}
impl TestCase {
    pub fn token_program_id(&self) -> Pubkey {
        match self {
            TestCase::MultiPoolToken22 => spl_token_2022::id(),
            _ => spl_token::id(),
        }
    }

    pub fn is_multi(&self) -> bool {
        match self {
            TestCase::SinglePool => false,
            _ => true,
        }
    }

    pub fn program_test(&self) -> ProgramTest {
        let mut program_test = ProgramTest::default();
        // FIXME figure out how to build this
        // program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);

        match self {
            TestCase::SinglePool => {
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
}

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> SolanaAccount {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("client error")
        .expect("account not found")
}

pub trait TestAccounts {}
