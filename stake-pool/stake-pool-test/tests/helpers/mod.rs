#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    crate::multi_pool::*,
    crate::single_pool::*,
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
    spl_associated_token_account as atoken, spl_stake_birdbath as spool, spl_stake_pool as mpool,
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

pub mod multi_pool;
pub mod single_pool;
pub mod token;
pub use token::*;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EnvBuilder {
    SinglePool,
    MultiPool,
    MultiPoolToken22,
}
impl EnvBuilder {
    pub fn env(self) -> Env {
        match self {
            EnvBuilder::SinglePool => Env::SinglePool(SinglePoolAccounts::default()),
            _ => Env::MultiPool(MultiPoolAccounts {
                token_program_id: self.token_program_id(),
                ..Default::default()
            }),
        }
    }

    pub fn token_program_id(&self) -> Pubkey {
        match self {
            EnvBuilder::MultiPoolToken22 => spl_token_2022::id(),
            _ => spl_token::id(),
        }
    }
}

// XXX should i store ProgramTest and stuff on this?
#[derive(Debug, PartialEq)]
pub enum Env {
    SinglePool(SinglePoolAccounts),
    MultiPool(MultiPoolAccounts),
}
impl Env {
    pub fn program_test(&self) -> ProgramTest {
        let mut program_test = ProgramTest::default();
        // FIXME figure out how to build this
        // program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);

        match self {
            Env::SinglePool(_) => {
                program_test.add_program(
                    "spl_stake_birdbath",
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

    /* XXX dunno if i need this
        pub fn is_multi(&self) -> bool {
            match self {
                Env::SinglePool => false,
                _ => true,
            }
        }
    */

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

    pub async fn initialize(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        match self {
            Env::SinglePool(accounts) => {
                accounts
                    .initialize(banks_client, payer, recent_blockhash)
                    .await
            }
            Env::MultiPool(accounts) => {
                accounts
                    .initialize(banks_client, payer, recent_blockhash)
                    .await
            }
        }
    }
}

// XXX ok im confused about parametrization again
// * if i do an Accounts trait, and non-generic Env, i impl initialize on the trait
//   env only exists to produce the accounts
//   but because i dont return a concrete type i need to work in separate branch arms in tests?
// * if i do an Accounts trait and generic Env, i can have a generic new()
//   but then uhh. everything has to be a function on the trait, cant access struct fields
// * if i make Env a struct that stores the Accounts struct and operates on it internally...
//   i still need a trait? no, because the Env enum uniquely determines the Accounts type
//   well, i can use a trait to have Accounts functionality outside Env
//   but theres no way to return the Accounts directly... unless i have two partial getters i guess?
//   annoyingly, Keypair doesnt impl Clone tho, so id need to return an Arc or something
//   oh also theres the question of how to initialize it... can i have function calls in test_case?
pub trait PoolAccounts {}

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
