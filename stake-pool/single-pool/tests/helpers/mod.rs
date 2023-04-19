#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    bincode::deserialize,
    borsh::BorshSerialize,
    mpl_token_metadata::{pda::find_metadata_account, state::Metadata},
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        hash::Hash,
        instruction::Instruction,
        program_option::COption,
        program_pack::Pack,
        pubkey::Pubkey,
        stake::{self, state::StakeState},
        system_instruction, system_program,
    },
    solana_program_test::{
        processor, BanksClient, ProgramTest, ProgramTestBanksClientExt, ProgramTestContext,
    },
    solana_sdk::{
        account::{Account as SolanaAccount, WritableAccount},
        clock::{Clock, Epoch},
        compute_budget::ComputeBudgetInstruction,
        feature_set::stake_allow_zero_undelegated_amount,
        message::Message,
        native_token::LAMPORTS_PER_SOL,
        signature::{Keypair, Signer},
        transaction::Transaction,
        transport::TransportError,
    },
    solana_vote_program::{
        self, vote_instruction,
        vote_state::{VoteInit, VoteState, VoteStateVersions},
    },
    spl_associated_token_account as atoken,
    spl_single_validator_pool::{
        find_pool_authority_address, find_pool_mint_address, find_pool_stake_address, id,
        instruction, processor::Processor,
    },
    std::{convert::TryInto, num::NonZeroU32},
};

pub mod token;
pub use token::*;

pub const FIRST_NORMAL_EPOCH: u64 = 15;
pub const USER_STARTING_SOL: u64 = 100_000;

pub async fn refresh_blockhash(context: &mut ProgramTestContext) {
    context.last_blockhash = context
        .banks_client
        .get_new_latest_blockhash(&context.last_blockhash)
        .await
        .unwrap();
}

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
    pub alice_token: Pubkey,
    pub bob_token: Pubkey,
    pub token_program_id: Pubkey,
}
impl SinglePoolAccounts {
    pub async fn initialize(&self, context: &mut ProgramTestContext) -> Result<(), TransportError> {
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
        let instructions = instruction::initialize(
            &id(),
            &self.vote_account.pubkey(),
            &context.payer.pubkey(),
            &rent,
            stake_get_minimum_delegation(
                &mut context.banks_client,
                &context.payer,
                &context.last_blockhash,
            )
            .await,
        );
        let message = Message::new(&instructions, Some(&context.payer.pubkey()));
        let transaction = Transaction::new(&[&context.payer], message, context.last_blockhash);

        context
            .banks_client
            .process_transaction(transaction)
            .await?;

        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.alice.pubkey(),
            USER_STARTING_SOL * LAMPORTS_PER_SOL,
        )
        .await;

        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &self.bob.pubkey(),
            USER_STARTING_SOL * LAMPORTS_PER_SOL,
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

        Ok(())
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
            alice_token: atoken::get_associated_token_address(&alice.pubkey(), &mint),
            bob_token: atoken::get_associated_token_address(&bob.pubkey(), &mint),
            alice,
            bob,
            token_program_id: spl_token::id(),
        }
    }
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

pub async fn get_stake_account(
    banks_client: &mut BanksClient,
    pubkey: &Pubkey,
) -> (StakeState, u64) {
    let stake_account = get_account(banks_client, pubkey).await;
    let lamports = stake_account.lamports;
    let stake = deserialize::<StakeState>(&stake_account.data).unwrap();

    (stake, lamports)
}

pub async fn stake_get_minimum_delegation(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
) -> u64 {
    let transaction = Transaction::new_signed_with_payer(
        &[stake::instruction::get_minimum_delegation()],
        Some(&payer.pubkey()),
        &[payer],
        *recent_blockhash,
    );
    let mut data = banks_client
        .simulate_transaction(transaction)
        .await
        .unwrap()
        .simulation_details
        .unwrap()
        .return_data
        .unwrap()
        .data;
    data.resize(8, 0);
    data.try_into().map(u64::from_le_bytes).unwrap()
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
