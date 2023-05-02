#![cfg(feature = "test-sbf")]
#![cfg(feature = "dontbuild")]
#![allow(unused_imports)] // FIXME remove
#![allow(dead_code)] // FIXME

use {
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        hash::Hash,
        instruction::Instruction,
        program_option::COption,
        program_pack::Pack,
        pubkey::Pubkey,
        stake, system_instruction, system_program,
    },
    solana_program_test::*,
    solana_sdk::{
        account::{Account as SolanaAccount, WritableAccount},
        clock::{Clock, Epoch},
        compute_budget::ComputeBudgetInstruction,
        feature_set::stake_raise_minimum_delegation_to_1_sol,
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
        processor::Processor,
    },
    spl_token_2022::{
        extension::StateWithExtensionsOwned,
        state::{Account, Mint},
    },
};

// XXX HANA this is just scratch space for me to make sure my interfaces work
// ideally for real tests we would reuse as much existing code as possible
// ie all tests that make sense for both pool programs should exist in one place and be generic over both

pub const TEST_STAKE_AMOUNT: u64 = 1_500_000_000;

struct PoolAccounts {
    validator: Keypair,
    vote_account: Keypair,
    stake_account: Pubkey,
    authority: Pubkey,
    mint: Pubkey,
}
impl PoolAccounts {
    pub fn new() -> Self {
        let vote_account = Keypair::new();

        Self {
            validator: Keypair::new(),
            stake_account: find_pool_stake_address(&id(), &vote_account.pubkey()),
            authority: find_pool_authority_address(&id(), &vote_account.pubkey()),
            mint: find_pool_mint_address(&id(), &vote_account.pubkey()),
            vote_account,
        }
    }

    pub async fn initialize_pool(
        &self,
        banks_client: &mut BanksClient,
        payer: &Keypair,
        recent_blockhash: &Hash,
    ) -> Result<(), TransportError> {
        create_vote(
            banks_client,
            payer,
            recent_blockhash,
            &self.validator,
            &self.vote_account,
        )
        .await;

        let rent = banks_client.get_rent().await.unwrap();
        let instructions = spl_single_validator_pool::instruction::initialize(
            &id(),
            &self.vote_account.pubkey(),
            &payer.pubkey(),
            &rent,
            LAMPORTS_PER_SOL,
        );
        let message = Message::new(&instructions, Some(&payer.pubkey()));
        let transaction = Transaction::new(&[payer], message, *recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .map_err(|e| e.into())
    }
}

fn program_test() -> ProgramTest {
    let mut program_test = ProgramTest::new(
        "spl_single_validator_pool",
        id(),
        processor!(Processor::process),
    );
    program_test.add_program("mpl_token_metadata", mpl_token_metadata::id(), None);
    program_test.prefer_bpf(false);
    program_test.deactivate_feature(stake_raise_minimum_delegation_to_1_sol::id());
    program_test
}

async fn get_account(banks_client: &mut BanksClient, pubkey: &Pubkey) -> SolanaAccount {
    banks_client
        .get_account(*pubkey)
        .await
        .expect("client error")
        .expect("account not found")
}

async fn create_ata(
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

async fn create_vote(
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

async fn create_independent_stake_account(
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

async fn create_blank_stake_account(
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

async fn delegate_stake_account(
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

async fn transfer(
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

#[tokio::test]
async fn initialize_success() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let pool_accounts = PoolAccounts::new();
    pool_accounts
        .initialize_pool(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    // mint exists
    let mint_account = get_account(&mut banks_client, &pool_accounts.mint).await;
    StateWithExtensionsOwned::<Mint>::unpack(mint_account.data).unwrap();

    // stake account exists
    let stake_account = get_account(&mut banks_client, &pool_accounts.stake_account).await;
    assert_eq!(stake_account.owner, stake::program::id());
}

#[tokio::test]
async fn deposit_withdraw_success() {
    let mut context = program_test().start_with_context().await;
    let pool_accounts = PoolAccounts::new();
    pool_accounts
        .initialize_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await
        .unwrap();

    let user = Keypair::new();
    transfer(
        &mut context.banks_client,
        &context.payer,
        &context.last_blockhash,
        &user.pubkey(),
        LAMPORTS_PER_SOL * 1000,
    )
    .await;

    create_ata(
        &mut context.banks_client,
        &user,
        &user.pubkey(),
        &context.last_blockhash,
        &pool_accounts.mint,
    )
    .await
    .unwrap();
    let user_token = atoken::get_associated_token_address(&user.pubkey(), &pool_accounts.mint);

    let lamps_before = context
        .banks_client
        .get_account(user.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    let user_stake = Keypair::new();
    let lockup = stake::state::Lockup::default();

    let authorized = stake::state::Authorized {
        staker: user.pubkey(),
        withdrawer: user.pubkey(),
    };

    create_independent_stake_account(
        &mut context.banks_client,
        &user,
        &context.last_blockhash,
        &user_stake,
        &authorized,
        &lockup,
        TEST_STAKE_AMOUNT,
    )
    .await;

    delegate_stake_account(
        &mut context.banks_client,
        &user,
        &context.last_blockhash,
        &user_stake.pubkey(),
        &user,
        &pool_accounts.vote_account.pubkey(),
    )
    .await;

    let lamps_after_stake = context
        .banks_client
        .get_account(user.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;

    // XXX ok what needs to happen here
    // * create_independent_stake_account, for the user stake
    // * delegate_stake_account, to activate it
    // X use warp_to_slot to advance to an activated stake
    // * create user token account
    // * deposit! which consists of:
    //   - withdraw rent (if possible)
    //   - change both authorities
    //   - deposit
    //   if not possible just provide a return address
    // reading the withdraw code is a little confusing
    // im not exactly clear on whether i can take the rent out or not
    //
    // ok nm i understand now
    // withdraw fails if youre staked and trying to take out more than excess lamps
    // ie you can withdraw neither stake nor rent
    // if youre not staked then you can withdraw excess or you can withdraw all
    // but cannot leave it below the rent-exempt reserve
    // so the flow here has to be merge then withdraw

    let first_normal_slot = context.genesis_config().epoch_schedule.first_normal_slot;
    context.warp_to_slot(first_normal_slot).unwrap();

    let instructions = spl_single_validator_pool::instruction::deposit(
        &id(),
        &pool_accounts.vote_account.pubkey(),
        &user_stake.pubkey(),
        &user_token,
        &user.pubkey(),
        &user.pubkey(),
    );
    let message = Message::new(&instructions, Some(&user.pubkey()));
    let transaction = Transaction::new(&[&user], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    assert!(context
        .banks_client
        .get_account(user_stake.pubkey())
        .await
        .expect("get_account")
        .is_none());

    let lamps_after_deposit = context
        .banks_client
        .get_account(user.pubkey())
        .await
        .unwrap()
        .unwrap()
        .lamports;
    println!("HANA lamps before staking: {}\n     lamps after staking: {} ({} less than before, {} excluding stake)\n     lamps after deposit: {} ({} more than before)", lamps_before, lamps_after_stake, lamps_before - lamps_after_stake, lamps_before - lamps_after_stake - TEST_STAKE_AMOUNT, lamps_after_deposit, lamps_after_deposit - lamps_after_stake);

    // TODO check stake balance, check user got their lamports, check user tokens...

    let recipient_stake = Keypair::new();
    create_blank_stake_account(
        &mut context.banks_client,
        &user,
        &context.last_blockhash,
        &recipient_stake,
    )
    .await;

    let instructions = spl_single_validator_pool::instruction::withdraw(
        &id(),
        &pool_accounts.vote_account.pubkey(),
        &recipient_stake.pubkey(),
        &user.pubkey(),
        &user_token,
        &user.pubkey(),
        TEST_STAKE_AMOUNT,
    );
    let message = Message::new(&instructions, Some(&user.pubkey()));
    let transaction = Transaction::new(&[&user], message, context.last_blockhash);

    context
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    assert!(
        context
            .banks_client
            .get_account(recipient_stake.pubkey())
            .await
            .unwrap()
            .unwrap()
            .lamports
            > TEST_STAKE_AMOUNT
    );
}
