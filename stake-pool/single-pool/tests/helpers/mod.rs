#![allow(dead_code)] // needed because cargo doesnt understand test usage

use {
    solana_program::{hash::Hash, pubkey::Pubkey, system_instruction, system_program},
    solana_program_test::{
        processor, BanksClient, ProgramTest, ProgramTestBanksClientExt, ProgramTestContext,
    },
    solana_sdk::{
        account::Account as SolanaAccount,
        feature_set::stake_allow_zero_undelegated_amount,
        message::Message,
        signature::{Keypair, Signer},
        stake::state::{Authorized, Lockup},
        transaction::Transaction,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState},
    },
    spl_associated_token_account as atoken,
    spl_single_validator_pool::{
        find_pool_authority_address, find_pool_mint_address, find_pool_stake_address, id,
        instruction, processor::Processor,
    },
};

pub mod token;
pub use token::*;

pub mod stake;
pub use stake::*;

pub const FIRST_NORMAL_EPOCH: u64 = 15;
pub const USER_STARTING_LAMPORTS: u64 = 10_000_000_000_000; // 10k sol

pub fn program_test() -> ProgramTest {
    let mut program_test = ProgramTest::default();
    program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);

    program_test.add_program(
        "spl_single_validator_pool",
        id(),
        processor!(Processor::process),
    );
    program_test.deactivate_feature(stake_allow_zero_undelegated_amount::id());

    program_test.prefer_bpf(false);
    program_test
}

#[derive(Debug, PartialEq)]
pub struct SinglePoolAccounts {
    pub validator: Keypair,
    pub vote_account: Keypair,
    pub stake_account: Pubkey,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub alice: Keypair,
    pub bob: Keypair,
    pub alice_stake: Keypair,
    pub bob_stake: Keypair,
    pub alice_token: Pubkey,
    pub bob_token: Pubkey,
    pub token_program_id: Pubkey,
}
impl SinglePoolAccounts {
    // does everything in initialize plus creates/delegates one or both stake accounts for our users
    // note this does not advance time, so everything is in an activating state
    pub async fn initialize_for_deposit(
        &self,
        context: &mut ProgramTestContext,
        alice_amount: u64,
        maybe_bob_amount: Option<u64>,
    ) -> u64 {
        let minimum_delegation = self.initialize(context).await;

        create_independent_stake_account(
            &mut context.banks_client,
            &self.alice,
            &context.last_blockhash,
            &self.alice_stake,
            &Authorized {
                staker: self.alice.pubkey(),
                withdrawer: self.alice.pubkey(),
            },
            &Lockup::default(),
            alice_amount,
        )
        .await;

        delegate_stake_account(
            &mut context.banks_client,
            &self.alice,
            &context.last_blockhash,
            &self.alice_stake.pubkey(),
            &self.alice,
            &self.vote_account.pubkey(),
        )
        .await;

        if let Some(bob_amount) = maybe_bob_amount {
            create_independent_stake_account(
                &mut context.banks_client,
                &self.bob,
                &context.last_blockhash,
                &self.bob_stake,
                &Authorized {
                    staker: self.bob.pubkey(),
                    withdrawer: self.bob.pubkey(),
                },
                &Lockup::default(),
                bob_amount,
            )
            .await;

            delegate_stake_account(
                &mut context.banks_client,
                &self.bob,
                &context.last_blockhash,
                &self.bob_stake.pubkey(),
                &self.bob,
                &self.vote_account.pubkey(),
            )
            .await;
        };

        minimum_delegation
    }

    // creates a vote account and stake pool for it. also sets up two users with sol and token accounts
    // note this leaves the pool in an activating state. caller can advance to next epoch if they please
    pub async fn initialize(&self, context: &mut ProgramTestContext) -> u64 {
        let first_normal_slot = context.genesis_config().epoch_schedule.first_normal_slot;
        context.warp_to_slot(first_normal_slot).unwrap();

        create_vote(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.validator,
            &self.vote_account,
        )
        .await;

        let rent = context.banks_client.get_rent().await.unwrap();
        let minimum_delegation = get_minimum_delegation(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await;

        let instructions = instruction::initialize(
            &id(),
            &self.vote_account.pubkey(),
            &context.payer.pubkey(),
            &rent,
            minimum_delegation,
        );
        let message = Message::new(&instructions, Some(&context.payer.pubkey()));
        let transaction = Transaction::new(&[&context.payer], message, context.last_blockhash);

        context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();

        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.alice.pubkey(),
            USER_STARTING_LAMPORTS,
        )
        .await;

        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.bob.pubkey(),
            USER_STARTING_LAMPORTS,
        )
        .await;

        create_ata(
            &mut context.banks_client,
            &context.payer,
            &self.alice.pubkey(),
            &context.last_blockhash,
            &self.mint,
        )
        .await;

        create_ata(
            &mut context.banks_client,
            &context.payer,
            &self.bob.pubkey(),
            &context.last_blockhash,
            &self.mint,
        )
        .await;

        minimum_delegation
    }
}
impl Default for SinglePoolAccounts {
    fn default() -> Self {
        let vote_account = Keypair::new();
        let alice = Keypair::new();
        let bob = Keypair::new();
        let mint = find_pool_mint_address(&id(), &vote_account.pubkey());

        Self {
            validator: Keypair::new(),
            authority: find_pool_authority_address(&id(), &vote_account.pubkey()),
            stake_account: find_pool_stake_address(&id(), &vote_account.pubkey()),
            mint,
            vote_account,
            alice_stake: Keypair::new(),
            bob_stake: Keypair::new(),
            alice_token: atoken::get_associated_token_address(&alice.pubkey(), &mint),
            bob_token: atoken::get_associated_token_address(&bob.pubkey(), &mint),
            alice,
            bob,
            token_program_id: spl_token::id(),
        }
    }
}

pub async fn refresh_blockhash(context: &mut ProgramTestContext) {
    context.last_blockhash = context
        .banks_client
        .get_new_latest_blockhash(&context.last_blockhash)
        .await
        .unwrap();
}

pub async fn advance_epoch(context: &mut ProgramTestContext) {
    let root_slot = context.banks_client.get_root_slot().await.unwrap();
    let slots_per_epoch = context.genesis_config().epoch_schedule.slots_per_epoch;
    context.warp_to_slot(root_slot + slots_per_epoch).unwrap();
}

pub async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> SolanaAccount {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("client error")
        .expect("account not found")
}

// XXX using this unless i figure out how tf to get tarpc::context::Context
#[allow(deprecated)]
pub async fn get_fee_for_message(banks_client: &mut BanksClient, message: &Message) -> u64 {
    let (fee_calculator, _, _) = banks_client.get_fees().await.unwrap();
    fee_calculator.calculate_fee(message)
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
