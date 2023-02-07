#![deny(missing_docs)]

//! A program for creating and managing pools of stake

pub mod error;
pub mod instruction;
pub mod processor;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

// Export current sdk types for downstream users building with a different sdk version
pub use solana_program;
use solana_program::pubkey::Pubkey;

// XXX change this
solana_program::declare_id!("3cqnsMsT6LE96pxv7GR4di5rLqHDZZbR3FbeSUeRLFqY");

const POOL_STAKE_PREFIX: &[u8] = b"stake";
const POOL_AUTHORITY_PREFIX: &[u8] = b"authority";
const POOL_MINT_PREFIX: &[u8] = b"mint";

const MINT_DECIMALS: u8 = 9;

fn find_address(program_id: &Pubkey, vote_account_address: &Pubkey, prefix: &[u8]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[prefix, vote_account_address.as_ref()], program_id)
}

/// doc
pub fn find_pool_stake_address(program_id: &Pubkey, vote_account_address: &Pubkey) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_STAKE_PREFIX)
}

/// doc
pub fn find_pool_authority_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_AUTHORITY_PREFIX)
}

/// doc
pub fn find_pool_mint_address(program_id: &Pubkey, vote_account_address: &Pubkey) -> (Pubkey, u8) {
    find_address(program_id, vote_account_address, POOL_MINT_PREFIX)
}
