#![deny(missing_docs)]

//! A program for liquid staking with a single validator

pub mod error;
pub mod instruction;
pub mod processor;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

// export current sdk types for downstream users building with a different sdk version
pub use solana_program;
use solana_program::pubkey::Pubkey;

// XXX TODO FIXME change this
// (XXX ask how do we as a company handle privkeys for our onchain programs?)
solana_program::declare_id!("3cqnsMsT6LE96pxv7GR4di5rLqHDZZbR3FbeSUeRLFqY");

const POOL_STAKE_PREFIX: &[u8] = b"stake";
const POOL_AUTHORITY_PREFIX: &[u8] = b"authority";
const POOL_MINT_PREFIX: &[u8] = b"mint";

const MINT_DECIMALS: u8 = 9;
const INITIAL_LAMPORTS: u64 = 1;

// authorized withdrawer starts immediately after the enum tag
const VOTE_STATE_START: usize = 4;
const VOTE_STATE_END: usize = 36;

// authorized withdrawer starts at:
//    4 (enum tag)
// + 32 (node_pubkey)
// + 32 (authorized_voter)
// +  8 (authorized_voter_epoch)
// + (32 + 8 * 3) * 32 + 8 (prior_voters)
// = 1876
const LEGACY_VOTE_STATE_START: usize = 1876;
const LEGACY_VOTE_STATE_END: usize = 1908;

fn find_address(program_id: &Pubkey, vote_account_address: &Pubkey, prefix: &[u8]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[prefix, vote_account_address.as_ref()], program_id)
}

/// Find the canonical stake account address for a given vote account.
pub fn find_pool_stake_address(program_id: &Pubkey, vote_account_address: &Pubkey) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_STAKE_PREFIX)
}

/// Find the canonical authority address for a given vote account.
pub fn find_pool_authority_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_AUTHORITY_PREFIX)
}

/// Find the canonical token mint address for a given vote account.
pub fn find_pool_mint_address(program_id: &Pubkey, vote_account_address: &Pubkey) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_MINT_PREFIX)
}

#[allow(missing_docs)]
/// Internal constants confined to a suggestively named submodule for use in tests.
pub mod test_variable {
    pub const LEGACY_VOTE_STATE_START: usize = super::LEGACY_VOTE_STATE_START;
    pub const LEGACY_VOTE_STATE_END: usize = super::LEGACY_VOTE_STATE_END;
}
