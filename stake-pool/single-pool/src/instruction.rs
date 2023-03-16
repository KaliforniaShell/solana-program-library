//! Instruction types

#![allow(clippy::too_many_arguments)]

use {
    crate::{find_pool_authority_address, find_pool_mint_address, find_pool_stake_address},
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
    ///   Initialize the mint for a new single-validator pool.
    ///   Step one of the permissionless three-stage initialization flow.
    ///
    ///   0. `[]` Validator vote account
    ///   1. `[]` Pool authority
    ///   2. `[w]` Pool token mint
    ///   3. `[]` Rent sysvar
    ///   4. `[]` System program
    ///   5. `[]` Token program
    InitializeMint,

    ///   Initialize the stake account for a new single-validator pool.
    ///   Step two of the permissionless three-stage initialization flow.
    ///
    ///   0. `[]` Validator vote account
    ///   1. `[w]` Pool stake account
    ///   2. `[]` Pool authority
    ///   3. `[w]` Pool token mint
    ///   4. `[w]` User associated token account to receive pool tokens
    ///   5. `[]` Rent sysvar
    ///   6. `[]` Clock sysvar
    ///   7. `[]` Stake history sysvar
    ///   8. `[]` Stake config sysvar
    ///   9. `[]` System program
    ///  10. `[]` Token program
    ///  11. `[]` Stake program
    InitializeStake,

    ///   Deposit stake into the pool.  The output is a "pool" token representing fractional
    ///   ownership of the pool stake. Inputs are converted to the current ratio.
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
    ///   Step three of the permissionless three-stage initialization flow.
    ///   Note this instruction is not necessary for the pool to operate, to ensure we cannot
    ///   be broken by upstream.
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

/// Creates all necessary instructions to initialize the pool.
pub fn initialize(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    payer: &Pubkey,
    rent: &Rent,
    minimum_delegation: u64,
) -> Vec<Instruction> {
    let (stake_address, _) = find_pool_stake_address(program_id, vote_account);
    let stake_space = std::mem::size_of::<stake::state::StakeState>();
    let stake_rent_plus_one = rent
        .minimum_balance(stake_space)
        .saturating_add(minimum_delegation);

    let (mint_address, _) = find_pool_mint_address(program_id, vote_account);
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);
    let user_token_address =
        spl_associated_token_account::get_associated_token_address(payer, &mint_address);

    vec![
        system_instruction::transfer(payer, &stake_address, stake_rent_plus_one),
        system_instruction::transfer(payer, &mint_address, mint_rent),
        initialize_mint(program_id, vote_account),
        spl_associated_token_account::instruction::create_associated_token_account(
            payer,
            payer,
            &mint_address,
            &spl_token::id(),
        ),
        initialize_stake(program_id, vote_account, &user_token_address),
        create_token_metadata(program_id, vote_account, payer),
    ]
}

/// Creates an `InitializeMint` instruction.
pub fn initialize_mint(program_id: &Pubkey, vote_account: &Pubkey) -> Instruction {
    let data = SinglePoolInstruction::InitializeMint.try_to_vec().unwrap();
    let accounts = vec![
        AccountMeta::new_readonly(*vote_account, false),
        AccountMeta::new_readonly(
            find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(find_pool_mint_address(program_id, vote_account).0, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Creates an `InitializeStake` instruction.
pub fn initialize_stake(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_token_address: &Pubkey,
) -> Instruction {
    let (mint_address, _) = find_pool_mint_address(program_id, vote_account);

    let data = SinglePoolInstruction::InitializeStake.try_to_vec().unwrap();
    let accounts = vec![
        AccountMeta::new_readonly(*vote_account, false),
        AccountMeta::new(find_pool_stake_address(program_id, vote_account).0, false),
        AccountMeta::new_readonly(
            find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(mint_address, false),
        AccountMeta::new(*user_token_address, false),
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

/// Creates all necessary instructions to deposit stake.
pub fn deposit(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_token_account: &Pubkey,
    user_lamport_account: &Pubkey,
    user_withdraw_authority: &Pubkey,
) -> Vec<Instruction> {
    let (pool_authority, _) = find_pool_authority_address(program_id, vote_account);

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
        deposit_stake(
            program_id,
            vote_account,
            user_stake_account,
            user_token_account,
            user_lamport_account,
        ),
    ]
}

/// Creates a `DepositStake` instruction.
pub fn deposit_stake(
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
        AccountMeta::new(find_pool_stake_address(program_id, vote_account).0, false),
        AccountMeta::new_readonly(
            find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(find_pool_mint_address(program_id, vote_account).0, false),
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

/// Creates all necessary instructions to withdraw stake into a given stake account.
/// If a new stake account is required, the user should first include `system_instruction::create_account`
/// with account size `std::mem::size_of::<stake::state::StakeState>()` and owner `stake::program::id()`.
pub fn withdraw(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_stake_authority: &Pubkey,
    user_token_account: &Pubkey,
    user_token_authority: &Pubkey,
    token_amount: u64,
) -> Vec<Instruction> {
    let (pool_authority, _) = find_pool_authority_address(program_id, vote_account);

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
        withdraw_stake(
            program_id,
            vote_account,
            user_stake_account,
            user_stake_authority,
            user_token_account,
            token_amount,
        ),
    ]
}

/// Creates a `WithdrawStake` instruction.
pub fn withdraw_stake(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    user_stake_account: &Pubkey,
    user_stake_authority: &Pubkey,
    user_token_account: &Pubkey,
    token_amount: u64,
) -> Instruction {
    let data = SinglePoolInstruction::WithdrawStake {
        vote_account_address: *vote_account,
        user_stake_authority: *user_stake_authority,
        token_amount,
    }
    .try_to_vec()
    .unwrap();

    let accounts = vec![
        AccountMeta::new(find_pool_stake_address(program_id, vote_account).0, false),
        AccountMeta::new_readonly(
            find_pool_authority_address(program_id, vote_account).0,
            false,
        ),
        AccountMeta::new(find_pool_mint_address(program_id, vote_account).0, false),
        AccountMeta::new(*user_stake_account, false),
        AccountMeta::new(*user_token_account, false),
        AccountMeta::new_readonly(sysvar::clock::id(), false),
        AccountMeta::new_readonly(spl_token::id(), false),
        AccountMeta::new_readonly(stake::program::id(), false),
    ];

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

// TODO maybe have a helper function here that stakes for the user?
// eg, creates instructions like create_independent_stake_account and delegate_stake_account
// i havent checked if the stake program itself has helpers for this, but it might be nice

/// Creates a `CreateTokenMetadata` instruction.
pub fn create_token_metadata(
    program_id: &Pubkey,
    vote_account: &Pubkey,
    payer: &Pubkey,
) -> Instruction {
    let (pool_authority, _) = find_pool_authority_address(program_id, vote_account);
    let (pool_mint, _) = find_pool_mint_address(program_id, vote_account);
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
    let (pool_authority, _) = find_pool_authority_address(program_id, vote_account);
    let (pool_mint, _) = find_pool_mint_address(program_id, vote_account);
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
