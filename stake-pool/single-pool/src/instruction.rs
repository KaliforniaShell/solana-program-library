//! Instruction types

#![allow(clippy::too_many_arguments)]

use {
    borsh::{BorshDeserialize, BorshSerialize},
    mpl_token_metadata::pda::find_metadata_account,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        stake, system_program, sysvar,
    },
};

/// Instructions supported by the StakePool program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum StakePoolInstruction {
    ///   Initialize a new [bikeshed name].
    ///
    ///   0. `[]` Validator vote account
    ///   1. `[s, w]` Fee-payer
    ///   2. `[w]` Pool stake account
    ///   3. `[w]` Pool authority
    ///   4. `[w]` Pool token mint
    ///   5. `[]` Rent sysvar
    ///   6. `[]` Clock sysvar
    ///   7. `[]` Stake history sysvar
    ///   8. `[]` Stake config sysvar
    ///   9. `[]` System program
    ///  10. `[]` Token program
    ///  11. `[]` Stake program
    Initialize,

    ///   Deposit some stake into the pool.  The output is a "pool" token representing ownership
    ///   into the pool. Inputs are converted to the current ratio.
    ///
    ///   0. `[w]` Pool stake account
    ///   1. `[]` Pool authority
    ///   2. `[w]` Pool token mint
    ///   3. `[w]` User stake account to join to the pool
    ///   4. `[w]` User account to receive pool tokens
    ///   5. `[w]` User account to receive lamports
    ///   6. `[]` Clock sysvar
    ///   7. `[]` Stake history sysvar
    ///   8. `[]` Token program
    ///   9. `[]` Stake program
    DepositStake {
        /// Validator vote account address
        vote_account_address: Pubkey,
    },

    ///   Redeem tokens issued by this pool for stake at the current ratio.
    ///
    ///   0. `[w]` Pool stake account
    ///   1. `[]` Pool authority
    ///   2. `[w]` Pool token mint
    ///   3. `[w]` User stake account to receive stake at
    ///   4. `[w]` User account to take pool tokens from
    ///   5. `[]` Clock sysvar
    ///   6. `[]` Token program
    ///   7. `[]` Stake program
    WithdrawStake {
        /// Validator vote account address
        vote_account_address: Pubkey,
        /// User authority for the new stake account
        user_stake_authority: Pubkey,
        /// Amount of tokens to redeem for stake
        token_amount: u64,
    },

    ///   Create token metadata for the stake-pool token in the metaplex-token program.
    ///   This permissionless instruction is called as part of pool initialization.
    ///
    ///   0. `[]` Pool authority
    ///   1. `[]` Pool token mint
    ///   2. `[s, w]` Payer for creation of token metadata account
    ///   3. `[w]` Token metadata account
    ///   4. `[]` Metadata program id
    ///   5. `[]` System program id
    ///   6. `[]` Rent sysvar
    CreateTokenMetadata {
        /// Validator vote account address
        vote_account_address: Pubkey,
    },

    // TODO
    /// Update token metadata for the stake-pool token in the
    /// metaplex-token program
    ///
    /// 0. `[]` Stake pool
    /// 1. `[s]` Manager
    /// 2. `[]` Stake pool withdraw authority
    /// 3. `[w]` Token metadata account
    /// 4. `[]` Metadata program id
    UpdateTokenMetadata {
        /// Token name
        name: String,
        /// Token symbol e.g. stkSOL
        symbol: String,
        /// URI of the uploaded metadata of the spl-token
        uri: String,
    },
}

/// Creates an `Initialize` instruction, plus helper instruction(s).
pub fn initialize(program_id: &Pubkey, vote_account: &Pubkey, payer: &Pubkey) -> Vec<Instruction> {
    let data = StakePoolInstruction::Initialize.try_to_vec().unwrap();
    let accounts = vec![
        AccountMeta::new_readonly(*vote_account, false),
        AccountMeta::new(*payer, true),
        AccountMeta::new(
            crate::find_pool_stake_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(
            crate::find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(
            crate::find_pool_mint_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(stake::config::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(stake::program::id(), false),
    ];

    vec![
        Instruction {
            program_id: *program_id,
            accounts,
            data,
        },
        create_token_metadata(program_id, vote_account, payer),
    ]
}

// TODO wrapper function that replaces the last 3 params with just wallet, calls this with atat/wallet/wallet?
/// Creates a `DepositStake` instruction, plus helper instruction(s).
pub fn deposit_stake(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_token_account: &Pubkey,
    user_lamport_account: &Pubkey,
    user_withdraw_authority: &Pubkey,
) -> Vec<Instruction> {
    let (pool_authority, _) = crate::find_pool_authority_address(program_id, vote_account);
    let data = StakePoolInstruction::DepositStake {
        vote_account_address: *vote_account,
    }
    .try_to_vec()
    .unwrap();

    let accounts = vec![
        AccountMeta::new(
            crate::find_pool_stake_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new_readonly(pool_authority, false),
        AccountMeta::new(
            crate::find_pool_mint_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(*user_stake_account, false),
        AccountMeta::new(*user_token_account, false),
        AccountMeta::new(*user_lamport_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(sysvar::stake_history::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(stake::program::id(), false),
    ];

    vec![
        stake::instruction::authorize(
            user_stake_account,
            user_withdraw_authority,
            &pool_authority,
            stake::state::StakeAuthorize::Staker,
            None,
        ),
        stake::instruction::authorize(
            user_stake_account,
            user_withdraw_authority,
            &pool_authority,
            stake::state::StakeAuthorize::Withdrawer,
            None,
        ),
        Instruction {
            program_id: *program_id,
            accounts,
            data,
        },
    ]
}

// TODO wrapper which creates the system account ala create_blank_stake_account?
// ergonomics are tricky because it needs to get the rent amount from somewhere
/// Creates a `WithdrawStake` instruction, plus helper instruction(s).
pub fn withdraw_stake(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_stake_authority: &Pubkey,
    user_token_account: &Pubkey,
    user_token_authority: &Pubkey,
    token_amount: u64,
) -> Vec<Instruction> {
    let (pool_authority, _) = crate::find_pool_authority_address(program_id, vote_account);
    let data = StakePoolInstruction::WithdrawStake {
        vote_account_address: *vote_account,
        user_stake_authority: *user_stake_authority,
        token_amount,
    }
    .try_to_vec()
    .unwrap();

    let accounts = vec![
        AccountMeta::new(
            crate::find_pool_stake_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new_readonly(pool_authority, false),
        AccountMeta::new(
            crate::find_pool_mint_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(*user_stake_account, false),
        AccountMeta::new(*user_token_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(stake::program::id(), false),
    ];

    vec![
        spl_token::instruction::approve(
            &spl_token::id(),
            user_token_account,
            &pool_authority,
            user_token_authority,
            &[],
            token_amount,
        )
        .unwrap(),
        Instruction {
            program_id: *program_id,
            accounts,
            data,
        },
    ]
}

// TODO maybe have a helper function here that stakes for the user?
// eg, creates instructions like create_independent_stake_account and delegate_stake_account

/// Creates a `CreateTokenMetadata` instruction.
pub fn create_token_metadata(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    payer: &Pubkey,
) -> Instruction {
    let (pool_authority, _) = crate::find_pool_authority_address(program_id, vote_account);
    let (pool_mint, _) = crate::find_pool_mint_address(program_id, vote_account);
    let (token_metadata, _) = find_metadata_account(&pool_mint);
    let data = StakePoolInstruction::CreateTokenMetadata {
        vote_account_address: *vote_account,
    }
    .try_to_vec()
    .unwrap();

    let accounts = vec![
        AccountMeta::new_readonly(pool_authority, false),
        AccountMeta::new_readonly(pool_mint, false),
        AccountMeta::new(*payer, true),
        AccountMeta::new(token_metadata, false),
        AccountMeta::new_readonly(mpl_token_metadata::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// FIXME unchanged from original
pub fn update_token_metadata(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    pool_mint: &Pubkey,
    name: String,
    symbol: String,
    uri: String,
) -> Instruction {
    let (stake_pool_withdraw_authority, _) = (Pubkey::default(), ()); //FIXME find_withdraw_authority_program_address(program_id, stake_pool);
    let (token_metadata, _) = find_metadata_account(pool_mint);

    let accounts = vec![
        AccountMeta::new_readonly(*stake_pool, false),
        AccountMeta::new_readonly(*manager, true),
        AccountMeta::new_readonly(stake_pool_withdraw_authority, false),
        AccountMeta::new(token_metadata, false),
        AccountMeta::new_readonly(mpl_token_metadata::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::UpdateTokenMetadata { name, symbol, uri }
            .try_to_vec()
            .unwrap(),
    }
}
