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
    HanaInitialize,

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
    HanaDepositStake {
        /// Validator vote account address
        vote_account_address: Pubkey,
    },

    ///   Redeem tokens issued by this pool for stake at the current ratio.
    ///
    ///   0. `[w]` Pool stake account
    ///   1. `[]` Pool authority
    ///   2. `[w]` Pool token mint
    ///   3. `[w]` User stake account to receive stake at
    // XXX FIXME this could be an argument
    ///   4. `[]` User authority on stake account
    ///   5. `[w]` User account to take pool tokens from
    // XXX FIXME assign delegation to pool authority and drop this?
    ///   6. `[]` User authority on token account
    ///   7. `[]` Clock sysvar
    ///   8. `[]` Token program
    ///   9. `[]` Stake program
    HanaWithdrawStake {
        /// Validator vote account address
        vote_account_address: Pubkey,
        /// Amount of tokens to redeem for stake
        amount: u64,
    },

    // XXX as noted in processor, this actually can go away
    // set up a sensible default in initialize. we only need update
    /// Create token metadata for the stake-pool token in the
    /// metaplex-token program
    /// 0. `[]` Stake pool
    /// 1. `[s]` Manager
    /// 2. `[]` Stake pool withdraw authority
    /// 3. `[]` Pool token mint account
    /// 4. `[s, w]` Payer for creation of token metadata account
    /// 5. `[w]` Token metadata account
    /// 6. `[]` Metadata program id
    /// 7. `[]` System program id
    /// 8. `[]` Rent sysvar
    CreateTokenMetadata {
        /// Token name
        name: String,
        /// Token symbol e.g. stkSOL
        symbol: String,
        /// URI of the uploaded metadata of the spl-token
        uri: String,
    },
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

/// Creates an `Initialize` instruction.
pub fn initialize(program_id: &Pubkey, vote_account: &Pubkey, payer: &Pubkey) -> Instruction {
    let data = StakePoolInstruction::HanaInitialize.try_to_vec().unwrap();
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

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Creates a `DepositStake` instruction.
pub fn deposit_stake(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_token_account: &Pubkey,
    user_lamport_account: &Pubkey,
    user_withdraw_authority: &Pubkey,
) -> Vec<Instruction> {
    let (pool_authority, _) = crate::find_pool_authority_address(program_id, vote_account);
    let data = StakePoolInstruction::HanaDepositStake {
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

/// FIXME unchanged from original
pub fn withdraw_stake(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    validator_list_storage: &Pubkey,
    stake_pool_withdraw: &Pubkey,
    stake_to_split: &Pubkey,
    stake_to_receive: &Pubkey,
    user_stake_authority: &Pubkey,
    user_transfer_authority: &Pubkey,
    user_pool_token_account: &Pubkey,
    manager_fee_account: &Pubkey,
    pool_mint: &Pubkey,
    token_program_id: &Pubkey,
    amount: u64,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*stake_pool, false),
        AccountMeta::new(*validator_list_storage, false),
        AccountMeta::new_readonly(*stake_pool_withdraw, false),
        AccountMeta::new(*stake_to_split, false),
        AccountMeta::new(*stake_to_receive, false),
        AccountMeta::new_readonly(*user_stake_authority, false),
        AccountMeta::new_readonly(*user_transfer_authority, true),
        AccountMeta::new(*user_pool_token_account, false),
        AccountMeta::new(*manager_fee_account, false),
        AccountMeta::new(*pool_mint, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(*token_program_id, false),
        AccountMeta::new_readonly(stake::program::id(), false),
    ];
    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::HanaWithdrawStake {
            vote_account_address: Pubkey::default(),
            amount,
        }
        .try_to_vec()
        .unwrap(),
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

/// FIXME unchanged from original
pub fn create_token_metadata(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    manager: &Pubkey,
    pool_mint: &Pubkey,
    payer: &Pubkey,
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
        AccountMeta::new_readonly(*pool_mint, false),
        AccountMeta::new(*payer, true),
        AccountMeta::new(token_metadata, false),
        AccountMeta::new_readonly(mpl_token_metadata::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data: StakePoolInstruction::CreateTokenMetadata { name, symbol, uri }
            .try_to_vec()
            .unwrap(),
    }
}
