//! Instruction types

#![allow(clippy::too_many_arguments)]

use {
    borsh::{BorshDeserialize, BorshSerialize},
    mpl_token_metadata::pda::find_metadata_account,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        stake, system_instruction, system_program, sysvar,
    },
};

/// Instructions supported by the SinglePool program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum SinglePoolInstruction {
    ///   Initialize a new single-validator pool.
    ///
    ///   0. `[]` Validator vote account
    ///   1. `[w]` Pool stake account
    ///   2. `[w]` Pool authority
    ///   3. `[w]` Pool token mint
    ///   4. `[]` Rent sysvar
    ///   5. `[]` Clock sysvar
    ///   6. `[]` Stake history sysvar
    ///   7. `[]` Stake config sysvar
    ///   8. `[]` System program
    ///   9. `[]` Token program
    ///  10. `[]` Stake program
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
    CreateTokenMetadata {
        /// Validator vote account address
        vote_account_address: Pubkey,
    },

    ///   Update token metadata for the stake-pool token in the metaplex-token program.
    ///
    ///   0. `[]` Validator vote account
    ///   1. `[]` Pool authority
    ///   2. `[s]` Vote account authorized withdrawer
    ///   3. `[w]` Token metadata account
    ///   4. `[]` Metadata program id
    UpdateTokenMetadata {
        /// Token name
        name: String,
        /// Token symbol e.g. stkSOL
        symbol: String,
        /// URI of the uploaded metadata of the spl-token
        uri: String,
    },
}

// XXX we can bikeshed names of single-instruction vs "batteries included" helper functions
// but i like making the latter the implicit default... instruction::initialize_instruction looks stupid tho idk
// maybe that could be instruction::initialize but this could be instructions::initialize?
// i dont want to just arbitrarily define a new convention tho
/// Creates all necessary instructions to initialize the pool.
pub fn initialize(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    payer: &Pubkey,
    rent: &Rent,
) -> Vec<Instruction> {
    let (stake_address, _) = crate::find_pool_stake_address(program_id, vote_account);
    let stake_space = std::mem::size_of::<stake::state::StakeState>();
    let stake_rent_plus_one = rent.minimum_balance(stake_space).saturating_add(1);

    let (mint_address, _) = crate::find_pool_mint_address(program_id, vote_account);
    let mint_space = spl_token::state::Mint::LEN;
    let mint_rent = rent.minimum_balance(mint_space);

    vec![
        system_instruction::transfer(payer, &stake_address, stake_rent_plus_one),
        system_instruction::transfer(payer, &mint_address, mint_rent),
        initialize_instruction(program_id, vote_account),
        create_token_metadata(program_id, vote_account, payer),
    ]
}

/// Creates an `Initialize` instruction.
pub fn initialize_instruction(program_id: &Pubkey, vote_account: &Pubkey) -> Instruction {
    let data = SinglePoolInstruction::Initialize.try_to_vec().unwrap();
    let accounts = vec![
        AccountMeta::new_readonly(*vote_account, false),
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
        deposit_stake_instruction(
            program_id,
            vote_account,
            user_stake_account,
            user_token_account,
            user_lamport_account,
        ),
    ]
}

/// Creates a `DepositStake` instruction.
pub fn deposit_stake_instruction(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_token_account: &Pubkey,
    user_lamport_account: &Pubkey,
) -> Instruction {
    let data = SinglePoolInstruction::DepositStake {
        vote_account_address: *vote_account,
    }
    .try_to_vec()
    .unwrap();

    let accounts = vec![
        AccountMeta::new(
            crate::find_pool_stake_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new_readonly(
            crate::find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
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

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
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
    let data = SinglePoolInstruction::WithdrawStake {
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
    let data = SinglePoolInstruction::CreateTokenMetadata {
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
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Creates an `UpdateTokenMetadata` instruction.
pub fn update_token_metadata(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    authorized_withdrawer: &Pubkey,
    name: String,
    symbol: String,
    uri: String,
) -> Instruction {
    let (pool_authority, _) = crate::find_pool_authority_address(program_id, vote_account);
    let (pool_mint, _) = crate::find_pool_mint_address(program_id, vote_account);
    let (token_metadata, _) = find_metadata_account(&pool_mint);
    let data = SinglePoolInstruction::UpdateTokenMetadata { name, symbol, uri }
        .try_to_vec()
        .unwrap();

    let accounts = vec![
        AccountMeta::new_readonly(*vote_account, false),
        AccountMeta::new_readonly(pool_authority, false),
        AccountMeta::new_readonly(*authorized_withdrawer, true),
        AccountMeta::new(token_metadata, false),
        AccountMeta::new_readonly(mpl_token_metadata::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}
