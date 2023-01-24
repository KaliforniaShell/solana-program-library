#![cfg(feature = "test-sbf")]

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
            stake_account: find_pool_stake_address(&id(), &vote_account.pubkey()).0,
            authority: find_pool_authority_address(&id(), &vote_account.pubkey()).0,
            mint: find_pool_mint_address(&id(), &vote_account.pubkey()).0,
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

        let instruction = spl_stake_birdbath::instruction::initialize(
            &id(),
            &self.vote_account.pubkey(),
            &payer.pubkey(),
        );
        let message = Message::new(&[instruction], Some(&payer.pubkey()));
        let transaction = Transaction::new(&[payer], message, *recent_blockhash);

        banks_client
            .process_transaction(transaction)
            .await
            .map_err(|e| e.into())
    }
}

fn program_test() -> ProgramTest {
    let mut program_test =
        ProgramTest::new("spl_stake_birdbath", id(), processor!(Processor::process));
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
    authority: &Keypair,
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
