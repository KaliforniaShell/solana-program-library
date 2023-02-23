#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    crate::multi_pool::*,
    crate::single_pool::*,
    borsh::BorshSerialize,
    mpl_token_metadata::{pda::find_metadata_account, state::Metadata},
    serde_derive::{Deserialize, Serialize},
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        clock::Slot,
        hash::Hash,
        instruction::Instruction,
        program_option::COption,
        program_pack::Pack,
        pubkey::Pubkey,
        stake, system_instruction, system_program,
    },
    solana_program_test::{processor, BanksClient, ProgramTest, ProgramTestContext},
    solana_sdk::{
        account::{Account as SolanaAccount, AccountSharedData, WritableAccount},
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
        vote_state::{
            BlockTimestamp, Lockout, VoteInit, VoteState, MAX_EPOCH_CREDITS_HISTORY,
            MAX_LOCKOUT_HISTORY,
        },
    },
    spl_associated_token_account as atoken, spl_stake_pool as mpool,
    spl_stake_single_pool as spool,
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{collections::VecDeque, convert::TryInto, num::NonZeroU32},
};

// structs are mostly copy-pasted from vote_state_0_23_5.rs, a private module

const MAX_ITEMS: usize = 32;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum VoteStateVersions {
    V0_23_5(Box<VoteState0_23_5>),
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct VoteState0_23_5 {
    pub node_pubkey: Pubkey,
    pub authorized_voter: Pubkey,
    pub authorized_voter_epoch: Epoch,
    pub prior_voters: CircBuf<(Pubkey, Epoch, Epoch, Slot)>,
    pub authorized_withdrawer: Pubkey,
    pub commission: u8,
    pub votes: VecDeque<Lockout>,
    pub root_slot: Option<u64>,
    pub epoch_credits: Vec<(Epoch, u64, u64)>,
    pub last_timestamp: BlockTimestamp,
}
impl VoteState0_23_5 {
    pub fn size_of() -> usize {
        let vote_state = VoteState0_23_5 {
            votes: VecDeque::from(vec![Lockout::default(); MAX_LOCKOUT_HISTORY]),
            root_slot: Some(std::u64::MAX),
            epoch_credits: vec![(0, 0, 0); MAX_EPOCH_CREDITS_HISTORY],
            ..Self::default()
        };
        let vote_state = VoteStateVersions::V0_23_5(Box::new(vote_state));
        let size = bincode::serialized_size(&vote_state).unwrap();

        size as usize
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct CircBuf<I> {
    pub buf: [I; MAX_ITEMS],
    pub idx: usize,
}

impl<I: Default + Copy> Default for CircBuf<I> {
    fn default() -> Self {
        Self {
            buf: [I::default(); MAX_ITEMS],
            idx: MAX_ITEMS - 1,
        }
    }
}

pub async fn create_vote_legacy(
    context: &mut ProgramTestContext,
    validator: &Keypair,
    vote: &Keypair,
) {
    let rent = context.banks_client.get_rent().await.unwrap();
    let state_size = VoteState0_23_5::size_of();
    let lamports = rent.minimum_balance(state_size);

    let vote_state = VoteState0_23_5 {
        node_pubkey: validator.pubkey(),
        authorized_voter: validator.pubkey(),
        authorized_withdrawer: validator.pubkey(),
        ..VoteState0_23_5::default()
    };
    let vote_state = VoteStateVersions::V0_23_5(Box::new(vote_state));
    let buf = bincode::serialize(&vote_state).unwrap();

    let account = SolanaAccount {
        lamports,
        data: buf.to_vec(),
        owner: solana_vote_program::id(),
        executable: false,
        rent_epoch: 0,
    };

    context.set_account(&vote.pubkey(), &AccountSharedData::from(account));
}
