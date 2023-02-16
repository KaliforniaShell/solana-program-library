//! program state processor

use {
    crate::{
        error::StakePoolError, instruction::StakePoolInstruction, MINT_DECIMALS,
        POOL_AUTHORITY_PREFIX, POOL_MINT_PREFIX, POOL_STAKE_PREFIX,
    },
    borsh::BorshDeserialize,
    mpl_token_metadata::{
        instruction::{create_metadata_accounts_v3, update_metadata_accounts_v2},
        pda::find_metadata_account,
        state::DataV2,
    },
    num_traits::FromPrimitive,
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        borsh::try_from_slice_unchecked,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        msg,
        program::invoke_signed,
        program_error::{PrintProgramError, ProgramError},
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        stake, system_instruction, system_program,
        sysvar::Sysvar,
        //vote::state::vote_state_versions::VoteStateVersions,
    },
    spl_token_2022::{extension::StateWithExtensions, state::Mint},
};

/// Deserialize the stake state from AccountInfo
fn get_stake_state(
    stake_account_info: &AccountInfo,
) -> Result<(stake::state::Meta, stake::state::Stake), ProgramError> {
    let stake_state =
        try_from_slice_unchecked::<stake::state::StakeState>(&stake_account_info.data.borrow())?;
    match stake_state {
        stake::state::StakeState::Stake(meta, stake) => Ok((meta, stake)),
        _ => Err(StakePoolError::WrongStakeState.into()),
    }
}

// XXX HANA hana zone

fn check_pool_stake_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
    address: &Pubkey,
) -> Result<u8, ProgramError> {
    let (pool_stake_address, bump_seed) =
        crate::find_pool_stake_address(program_id, vote_account_address);
    if *address != pool_stake_address {
        msg!(
            "Incorrect pool stake address for vote {}, expected {}, received {}",
            vote_account_address,
            pool_stake_address,
            address
        );
        panic!("return error here");
    } else {
        Ok(bump_seed)
    }
}

fn check_pool_authority_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
    address: &Pubkey,
) -> Result<u8, ProgramError> {
    let (pool_authority_address, bump_seed) =
        crate::find_pool_authority_address(program_id, vote_account_address);
    if *address != pool_authority_address {
        msg!(
            "Incorrect pool authority address for vote {}, expected {}, received {}",
            vote_account_address,
            pool_authority_address,
            address
        );
        panic!("return error here");
    } else {
        Ok(bump_seed)
    }
}

fn check_pool_mint_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
    address: &Pubkey,
) -> Result<u8, ProgramError> {
    let (pool_mint_address, bump_seed) =
        crate::find_pool_mint_address(program_id, vote_account_address);
    if *address != pool_mint_address {
        msg!(
            "Incorrect pool mint address for vote {}, expected {}, received {}",
            vote_account_address,
            pool_mint_address,
            address
        );
        panic!("return error here");
    } else {
        Ok(bump_seed)
    }
}

fn check_token_program(address: &Pubkey) -> Result<(), ProgramError> {
    let token_program_address = spl_token::id();
    if *address != token_program_address {
        msg!(
            "Incorrect token program, expected {}, received {}",
            token_program_address,
            address
        );
        panic!("return error here");
    } else {
        Ok(())
    }
}

fn calculate_deposit_amount(
    token_supply: u64,
    validator_lamports: u64,
    deposit_lamports: u64,
) -> Option<u64> {
    if validator_lamports == 0 || token_supply == 0 {
        Some(deposit_lamports)
    } else {
        u64::try_from(
            (deposit_lamports as u128)
                .checked_mul(token_supply as u128)?
                .checked_div(validator_lamports as u128)?,
        )
        .ok()
    }
}

fn calculate_withdraw_amount(
    token_supply: u64,
    validator_lamports: u64,
    burn_tokens: u64,
) -> Option<u64> {
    let numerator = (burn_tokens as u128).checked_mul(validator_lamports as u128)?;
    let denominator = token_supply as u128;
    if numerator < denominator || denominator == 0 {
        Some(0)
    } else {
        u64::try_from(numerator.checked_div(denominator)?).ok()
    }
}

// XXX hana zone over

/// Check mpl metadata account address for the pool mint
fn check_mpl_metadata_account_address(
    metadata_address: &Pubkey,
    pool_mint: &Pubkey,
) -> Result<(), ProgramError> {
    let (metadata_account_pubkey, _) = find_metadata_account(pool_mint);
    if metadata_account_pubkey != *metadata_address {
        Err(StakePoolError::InvalidMetadataAccount.into())
    } else {
        Ok(())
    }
}

/// Check system program address
fn check_system_program(program_id: &Pubkey) -> Result<(), ProgramError> {
    if *program_id != system_program::id() {
        msg!(
            "Expected system program {}, received {}",
            system_program::id(),
            program_id
        );
        Err(ProgramError::IncorrectProgramId)
    } else {
        Ok(())
    }
}

/// Check stake program address
fn check_stake_program(program_id: &Pubkey) -> Result<(), ProgramError> {
    if *program_id != stake::program::id() {
        msg!(
            "Expected stake program {}, received {}",
            stake::program::id(),
            program_id
        );
        Err(ProgramError::IncorrectProgramId)
    } else {
        Ok(())
    }
}

/// Check mpl metadata program
fn check_mpl_metadata_program(program_id: &Pubkey) -> Result<(), ProgramError> {
    if *program_id != mpl_token_metadata::id() {
        msg!(
            "Expected mpl metadata program {}, received {}",
            mpl_token_metadata::id(),
            program_id
        );
        Err(ProgramError::IncorrectProgramId)
    } else {
        Ok(())
    }
}

/// Check rent sysvar correctness
fn check_rent_sysvar(sysvar_key: &Pubkey) -> Result<(), ProgramError> {
    if *sysvar_key != solana_program::sysvar::rent::id() {
        msg!(
            "Expected rent sysvar {}, received {}",
            solana_program::sysvar::rent::id(),
            sysvar_key
        );
        Err(ProgramError::InvalidArgument)
    } else {
        Ok(())
    }
}

/// Check account owner is the given program
fn check_account_owner(
    account_info: &AccountInfo,
    program_id: &Pubkey,
) -> Result<(), ProgramError> {
    if *program_id != *account_info.owner {
        msg!(
            "Expected account to be owned by program {}, received {}",
            program_id,
            account_info.owner
        );
        Err(ProgramError::IncorrectProgramId)
    } else {
        Ok(())
    }
}

/// Program state handler.
pub struct Processor {}
impl Processor {
    #[allow(clippy::too_many_arguments)]
    fn stake_merge<'a>(
        validator_vote_key: &Pubkey,
        source_account: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        destination_account: AccountInfo<'a>,
        clock: AccountInfo<'a>,
        stake_history: AccountInfo<'a>,
        stake_program_info: AccountInfo<'a>,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        invoke_signed(
            &stake::instruction::merge(destination_account.key, source_account.key, authority.key)
                [0],
            &[
                destination_account,
                source_account,
                clock,
                stake_history,
                authority,
                stake_program_info,
            ],
            signers,
        )
    }

    fn stake_split<'a>(
        validator_vote_key: &Pubkey,
        stake_account: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
        split_stake: AccountInfo<'a>,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        let split_instruction =
            stake::instruction::split(stake_account.key, authority.key, amount, split_stake.key);

        invoke_signed(
            split_instruction.last().unwrap(),
            &[stake_account, split_stake, authority],
            signers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn stake_authorize<'a>(
        validator_vote_key: &Pubkey,
        stake_account: AccountInfo<'a>,
        stake_authority: AccountInfo<'a>,
        bump_seed: u8,
        new_stake_authority: &Pubkey,
        clock: AccountInfo<'a>,
        stake_program_info: AccountInfo<'a>,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        let authorize_instruction = stake::instruction::authorize(
            stake_account.key,
            stake_authority.key,
            new_stake_authority,
            stake::state::StakeAuthorize::Staker,
            None,
        );

        invoke_signed(
            &authorize_instruction,
            &[
                stake_account.clone(),
                clock.clone(),
                stake_authority.clone(),
                stake_program_info.clone(),
            ],
            signers,
        )?;

        let authorize_instruction = stake::instruction::authorize(
            stake_account.key,
            stake_authority.key,
            new_stake_authority,
            stake::state::StakeAuthorize::Withdrawer,
            None,
        );
        invoke_signed(
            &authorize_instruction,
            &[stake_account, clock, stake_authority, stake_program_info],
            signers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn stake_withdraw<'a>(
        validator_vote_key: &Pubkey,
        stake_account: AccountInfo<'a>,
        stake_authority: AccountInfo<'a>,
        bump_seed: u8,
        destination_account: AccountInfo<'a>,
        clock: AccountInfo<'a>,
        stake_history: AccountInfo<'a>,
        stake_program_info: AccountInfo<'a>,
        lamports: u64,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        let withdraw_instruction = stake::instruction::withdraw(
            stake_account.key,
            stake_authority.key,
            destination_account.key,
            lamports,
            None,
        );

        invoke_signed(
            &withdraw_instruction,
            &[
                stake_account,
                destination_account,
                clock,
                stake_history,
                stake_authority,
                stake_program_info,
            ],
            signers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn token_mint_to<'a>(
        validator_vote_key: &Pubkey,
        token_program: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        destination: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        let ix = spl_token::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;

        invoke_signed(&ix, &[mint, destination, authority, token_program], signers)
    }

    #[allow(clippy::too_many_arguments)]
    fn token_burn<'a>(
        validator_vote_key: &Pubkey,
        token_program: AccountInfo<'a>,
        burn_account: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_key.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        let ix = spl_token::instruction::burn(
            token_program.key,
            burn_account.key,
            mint.key,
            authority.key,
            &[],
            amount,
        )?;

        invoke_signed(
            &ix,
            &[burn_account, mint, authority, token_program],
            signers,
        )
    }

    #[inline(never)] // needed due to stack size violation
    fn process_initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let validator_vote_info = next_account_info(account_info_iter)?;
        let payer_info = next_account_info(account_info_iter)?;
        let pool_stake_info = next_account_info(account_info_iter)?;
        let pool_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;
        let clock_info = next_account_info(account_info_iter)?;
        let stake_history_info = next_account_info(account_info_iter)?;
        let stake_config_info = next_account_info(account_info_iter)?;
        let system_program_info = next_account_info(account_info_iter)?;
        let token_program_info = next_account_info(account_info_iter)?;
        let stake_program_info = next_account_info(account_info_iter)?;

        let stake_bump_seed =
            check_pool_stake_address(program_id, validator_vote_info.key, pool_stake_info.key)?;
        let authority_bump_seed = check_pool_authority_address(
            program_id,
            validator_vote_info.key,
            pool_authority_info.key,
        )?;
        let mint_bump_seed =
            check_pool_mint_address(program_id, validator_vote_info.key, pool_mint_info.key)?;
        check_system_program(system_program_info.key)?;
        check_token_program(token_program_info.key)?;
        check_stake_program(stake_program_info.key)?;

        let stake_seeds = &[
            POOL_STAKE_PREFIX,
            validator_vote_info.key.as_ref(),
            &[stake_bump_seed],
        ];
        let stake_signers = &[&stake_seeds[..]];

        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            validator_vote_info.key.as_ref(),
            &[authority_bump_seed],
        ];
        let authority_signers = &[&authority_seeds[..]];

        let mint_seeds = &[
            POOL_MINT_PREFIX,
            validator_vote_info.key.as_ref(),
            &[mint_bump_seed],
        ];
        let mint_signers = &[&mint_seeds[..]];

        // change to Rent::get() if i get rid of the invokes that require the AccountInfo
        let rent = &Rent::from_account_info(rent_info)?;

        // we can create the mint and stake in separate instructions
        // i just like it this way because no account validation required lol

        // create the pool mint
        let mint_space = spl_token::state::Mint::LEN;
        let mint_rent = rent.minimum_balance(mint_space);

        invoke_signed(
            &system_instruction::create_account(
                payer_info.key,
                pool_mint_info.key,
                mint_rent,
                mint_space as u64,
                token_program_info.key,
            ),
            &[
                payer_info.clone(),
                pool_mint_info.clone(),
                system_program_info.clone(),
            ],
            mint_signers,
        )?;

        invoke_signed(
            &spl_token::instruction::initialize_mint(
                token_program_info.key,
                pool_mint_info.key,
                pool_authority_info.key,
                None,
                MINT_DECIMALS,
            )?,
            &[
                pool_mint_info.clone(),
                rent_info.clone(),
                system_program_info.clone(),
            ],
            authority_signers,
        )?;

        // create the pool stake account
        let stake_space = std::mem::size_of::<stake::state::StakeState>();
        let required_lamports = rent.minimum_balance(stake_space).saturating_add(1);
        let authorized = stake::state::Authorized::auto(pool_authority_info.key);

        invoke_signed(
            &system_instruction::create_account(
                payer_info.key,
                pool_stake_info.key,
                required_lamports,
                stake_space as u64,
                stake_program_info.key,
            ),
            &[
                payer_info.clone(),
                pool_stake_info.clone(),
                stake_program_info.clone(),
            ],
            stake_signers,
        )?;

        invoke_signed(
            &stake::instruction::initialize_checked(pool_stake_info.key, &authorized),
            &[
                pool_stake_info.clone(),
                rent_info.clone(),
                pool_authority_info.clone(),
                pool_authority_info.clone(),
            ],
            authority_signers,
        )?;

        // delegate the stake so it activates
        invoke_signed(
            &stake::instruction::delegate_stake(
                pool_stake_info.key,
                pool_authority_info.key,
                validator_vote_info.key,
            ),
            &[
                pool_stake_info.clone(),
                validator_vote_info.clone(),
                clock_info.clone(),
                stake_history_info.clone(),
                stake_config_info.clone(),
                pool_authority_info.clone(),
            ],
            authority_signers,
        )?;

        // could mint the token here if we wanted, either to user or incinerator

        Ok(())
    }

    // XXX ok cool next up ummm
    // the other two functions are extremely simplified version of their namesakes
    // for deposit we literally only need to call stake_merge (or a hana version), not authorize
    // because the user can et both authorities to ours rather than going through deposit authority
    //
    // and then the token calculation is just...
    // stake added * total tokens / total stake ?
    //
    // "total deposit" is simply, post lamps minus pre lamps
    // "stake deposit" is post stake minus pre stake
    // "sol deposit" is total deposit minus stake deposit
    // so it calcs "new pool" and "new pool from stake" from quantities 1 and 2
    // then "new pool from sol" as "new pool" minus "new pool from stake"
    // it calcs stake and sol deposit fees... and the total fee is the sum of them
    // "pool tokens user" then is "new pool" minus "total fee"
    // and finally it mints this. so...
    // im not sure why "sol deposit" should ever be nonzero? unless is this the rent?
    // assuming its rent (actually this makes sense, its not active stake!), we can just kick it back to the user
    // this means all the calculation goes basically goes away
    // can user withdraw their own rent? the account wouldnt get zeroed until the end of the txn
    // if so lol just. check lamps minus stake is zero and mint tokens commesurate to the stake
    // if its not possible tho just take a wallet do the merge and send back the extra lamps

    #[inline(never)] // needed to avoid stack size violation
    fn process_deposit_stake(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        vote_account_address: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let pool_stake_info = next_account_info(account_info_iter)?;
        let pool_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let user_stake_info = next_account_info(account_info_iter)?;
        let user_token_account_info = next_account_info(account_info_iter)?;
        let user_lamport_account_info = next_account_info(account_info_iter)?;
        let clock_info = next_account_info(account_info_iter)?;
        let stake_history_info = next_account_info(account_info_iter)?;
        let token_program_info = next_account_info(account_info_iter)?;
        let stake_program_info = next_account_info(account_info_iter)?;

        check_pool_stake_address(program_id, vote_account_address, pool_stake_info.key)?;
        let bump_seed = check_pool_authority_address(
            program_id,
            vote_account_address,
            pool_authority_info.key,
        )?;
        check_pool_mint_address(program_id, vote_account_address, pool_mint_info.key)?;
        check_token_program(token_program_info.key)?;
        check_stake_program(stake_program_info.key)?;

        // TODO assert pool stake state is active

        let (_, pre_validator_stake) = get_stake_state(pool_stake_info)?;
        let pre_validator_lamports = pool_stake_info.lamports();
        msg!("Stake pre merge {}", pre_validator_stake.delegation.stake);

        let (_, pre_user_stake) = get_stake_state(user_stake_info)?;
        let user_unstaked_lamports = user_stake_info
            .lamports()
            .checked_sub(pre_user_stake.delegation.stake)
            .ok_or(StakePoolError::CalculationFailure)?;

        // we have no deposit authority, so we dont need to call stake_authorize
        // user should set both authorities to pool_authority_info
        // the merge succeeding implicitly validates all properties of the user stake account

        // merge the user stake account, which is preauthed to us, into the pool stake account
        Self::stake_merge(
            vote_account_address,
            user_stake_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            pool_stake_info.clone(),
            clock_info.clone(),
            stake_history_info.clone(),
            stake_program_info.clone(),
        )?;

        let (_, post_validator_stake) = get_stake_state(pool_stake_info)?;
        let post_validator_lamports = pool_stake_info.lamports();
        msg!("Stake post merge {}", post_validator_stake.delegation.stake);

        let lamports_added = post_validator_lamports
            .checked_sub(pre_validator_lamports)
            .ok_or(StakePoolError::CalculationFailure)?;

        let stake_added = post_validator_stake
            .delegation
            .stake
            .checked_sub(pre_validator_stake.delegation.stake)
            .ok_or(StakePoolError::CalculationFailure)?;

        let leftover_rent = lamports_added
            .checked_sub(stake_added)
            .ok_or(StakePoolError::CalculationFailure)?;

        if stake_added != pre_user_stake.delegation.stake {
            panic!("sanity check failed");
        }

        if leftover_rent != user_unstaked_lamports {
            panic!("sanity check failed");
        }

        if user_stake_info.lamports() != 0 {
            panic!("sanity check failed");
        }

        let token_supply = {
            let pool_mint_data = pool_mint_info.try_borrow_data()?;
            let pool_mint = StateWithExtensions::<Mint>::unpack(&pool_mint_data)?;
            pool_mint.base.supply
        };

        let new_pool_tokens =
            calculate_deposit_amount(token_supply, pre_validator_lamports, lamports_added)
                .ok_or(StakePoolError::CalculationFailure)?;

        if new_pool_tokens == 0 {
            return Err(StakePoolError::DepositTooSmall.into());
        }

        // mint tokens to the user corresponding to their deposit
        Self::token_mint_to(
            vote_account_address,
            token_program_info.clone(),
            pool_mint_info.clone(),
            user_token_account_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            new_pool_tokens,
        )?;

        // return the lamports their stake account used to contain for rent-exemption
        Self::stake_withdraw(
            vote_account_address,
            pool_stake_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            user_lamport_account_info.clone(),
            clock_info.clone(),
            stake_history_info.clone(),
            stake_program_info.clone(),
            leftover_rent,
        )?;

        Ok(())
    }

    #[inline(never)] // needed to avoid stack size violation
    fn process_withdraw_stake(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        vote_account_address: &Pubkey,
        user_stake_authority: &Pubkey,
        token_amount: u64,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let pool_stake_info = next_account_info(account_info_iter)?;
        let pool_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let user_stake_info = next_account_info(account_info_iter)?;
        let user_token_account_info = next_account_info(account_info_iter)?;
        let clock_info = next_account_info(account_info_iter)?;
        let token_program_info = next_account_info(account_info_iter)?;
        let stake_program_info = next_account_info(account_info_iter)?;

        check_pool_stake_address(program_id, vote_account_address, pool_stake_info.key)?;
        let bump_seed = check_pool_authority_address(
            program_id,
            vote_account_address,
            pool_authority_info.key,
        )?;
        check_pool_mint_address(program_id, vote_account_address, pool_mint_info.key)?;
        check_token_program(token_program_info.key)?;
        check_stake_program(stake_program_info.key)?;

        let (_, pre_validator_stake) = get_stake_state(pool_stake_info)?;
        let pre_all_validator_lamports = pool_stake_info.lamports();
        msg!("Stake pre split {}", pre_validator_stake.delegation.stake);

        let token_supply = {
            let pool_mint_data = pool_mint_info.try_borrow_data()?;
            let pool_mint = StateWithExtensions::<Mint>::unpack(&pool_mint_data)?;
            pool_mint.base.supply
        };

        let withdraw_lamports =
            calculate_withdraw_amount(token_supply, pre_all_validator_lamports, token_amount)
                .ok_or(StakePoolError::CalculationFailure)?;

        if withdraw_lamports == 0 {
            return Err(StakePoolError::WithdrawalTooSmall.into());
        }

        // theres a *ton* of housekeeping in process_withdraw_stake that i havent read line by line fully carefully
        // but its all basically "we have a reserve and n validators and m transient accounts, whence stake?"
        // here in stupidland we have no need of any of that

        // burn user tokens corresponding to the amount of stake they wish to withdraw
        Self::token_burn(
            vote_account_address,
            token_program_info.clone(),
            user_token_account_info.clone(),
            pool_mint_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            token_amount,
        )?;

        // split stake into a blank stake account the user has created for this purpose
        Self::stake_split(
            vote_account_address,
            pool_stake_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            withdraw_lamports,
            user_stake_info.clone(),
        )?;

        // assign both authorities on the new stake account to the user
        Self::stake_authorize(
            vote_account_address,
            user_stake_info.clone(),
            pool_authority_info.clone(),
            bump_seed,
            user_stake_authority,
            clock_info.clone(),
            stake_program_info.clone(),
        )?;

        Ok(())
    }

    // this is an optional, but typical, part of initialization
    // we separate it from the initialize instruction so that upstream cannot break us
    #[inline(never)]
    fn process_create_pool_token_metadata(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        vote_account_address: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let pool_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let payer_info = next_account_info(account_info_iter)?;
        let metadata_info = next_account_info(account_info_iter)?;
        let mpl_token_metadata_program_info = next_account_info(account_info_iter)?;
        let system_program_info = next_account_info(account_info_iter)?;
        let rent_sysvar_info = next_account_info(account_info_iter)?;

        let bump_seed = check_pool_authority_address(
            program_id,
            vote_account_address,
            pool_authority_info.key,
        )?;
        check_pool_mint_address(program_id, vote_account_address, pool_mint_info.key)?;
        check_system_program(system_program_info.key)?;
        check_rent_sysvar(rent_sysvar_info.key)?;
        check_account_owner(payer_info, &system_program::id())?;
        check_mpl_metadata_program(mpl_token_metadata_program_info.key)?;
        check_mpl_metadata_account_address(metadata_info.key, pool_mint_info.key)?;

        if !payer_info.is_signer {
            msg!("Payer did not sign metadata creation");
            return Err(StakePoolError::SignatureMissing.into());
        }

        // checking the mint exists confirms pool is initialized
        // TODO put this in a utility function? do something smarter/simpler?
        {
            let pool_mint_data = pool_mint_info.try_borrow_data()?;
            let _ = StateWithExtensions::<Mint>::unpack(&pool_mint_data)?;
        }

        let new_metadata_instruction = create_metadata_accounts_v3(
            *mpl_token_metadata_program_info.key,
            *metadata_info.key,
            *pool_mint_info.key,
            *pool_authority_info.key,
            *payer_info.key,
            *pool_authority_info.key,
            // XXX TODO FIXME figure out good defaults
            // symbol and uri maybe are supposed to be ""? i dunno whats ideomatic
            // name im leaning toward "SPL Single-Pool Token (1234...abcd)"
            // where the parens surround an abbreviation of the vote account address
            "SOMETHING EXCITING TO DEBATE".to_string(),
            "".to_string(),
            "".to_string(),
            None,
            0,
            true,
            true,
            None,
            None,
            None,
        );

        let authority_seeds = &[
            POOL_AUTHORITY_PREFIX,
            vote_account_address.as_ref(),
            &[bump_seed],
        ];
        let signers = &[&authority_seeds[..]];

        invoke_signed(
            &new_metadata_instruction,
            &[
                metadata_info.clone(),
                pool_mint_info.clone(),
                pool_authority_info.clone(),
                payer_info.clone(),
                pool_authority_info.clone(),
                system_program_info.clone(),
                rent_sysvar_info.clone(),
                mpl_token_metadata_program_info.clone(),
            ],
            signers,
        )?;

        Ok(())
    }

    // XXX NEXT UP this shit
    // take authorized_withdrawer as a signer. take the vote account and get authorized_withdrawer off it
    // check that it matches. if so we change shit to whatever the person wants
    // and we use pool authority as the authority on mpl, we only handle this signature internally
    // i asked edgar if this flow is reasonable or not, maybe they cold wallet the withdrawer?
    #[inline(never)]
    fn process_update_pool_token_metadata(
        _program_id: &Pubkey,
        accounts: &[AccountInfo],
        name: String,
        symbol: String,
        uri: String,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let stake_pool_info = next_account_info(account_info_iter)?;
        let _manager_info = next_account_info(account_info_iter)?;
        let withdraw_authority_info = next_account_info(account_info_iter)?;
        let metadata_info = next_account_info(account_info_iter)?;
        let mpl_token_metadata_program_info = next_account_info(account_info_iter)?;
        let vote_account_info = next_account_info(account_info_iter)?;

    // XXX WRONG
    //let vote_state = vote_account_info.try_borrow_data().and_then(VoteState::deserialize)?;
    // XXX RIGHT????
    //let vote_state: VoteStateVersions = vote_account_info.state().unwrap();
    //let authorized_withdrawer = vote_state.convert_to_current().authorized_withdrawer;

    // XXX ok argh think think. this is how the cli does it: 
    //     let vote_state: VoteStateVersions = vote_account.state().unwrap();
    //     let authorized_withdrawer = vote_state.convert_to_current().authorized_withdrawer;
    // state() comes from State which is a convenience for converting bincode errors to instruction errors
    // and is impled for Account which is a struct (not the same-named trait) returned by rpc et al
    // this is an *inferior* type to AccountInfo. but downcasting to Account doesnt appear to be a thing
    // but... im slavishly following existing code because idk if the version byte will be on the data...?
    // state() is literally just deserialize_data().map_err(). i think i can just deserialize normally?
    // deserialize_data() is a transparent wrapper on bincode::deserialize()
    // and VoteStateVersions derives Deserialize. so i thiiiink i can just call deserialize on the data?
    // im not seeing how this actually works tho. where does it handle the leading byte
    // oh well. the code paths ive read simply would not work unless it was baked into the types

    // FIXME custom error here
    // XXX ask jon to look at this, i feel like there has to be an sdk trait that wraps deserialie or something
    //let vote_data = vote_account_info.try_borrow_data()?;
    //let vote_state: VoteStateVersions = bincode::deserialize(&vote_data).unwrap();
    //let authorized_withdrawer = vote_state.convert_to_current().authorized_withdrawer;

    // XXX TODO FIXME ok i am officially out of my depth
    //Error: Function _ZN14solana_program4vote5state9VoteState11deserialize17he97c00397d39ebd1E Stack offset of 6344 exceeded max offset of 4096 by 2248 bytes, please minimize large stack variables
    //Error: Function _ZN229_$LT$solana_program..vote..state..vote_state_0_23_5.._..$LT$impl$u20$serde..de..Deserialize$u20$for$u20$solana_program..vote..state..vote_state_0_23_5..VoteState0_23_5$GT$..deserialize..__Visitor$u20$as$u20$serde..de..Visitor$GT$9visit_seq17hffe462f16e947850E Stack offset of 5752 exceeded max offset of 4096 by 1656 bytes, please minimize large stack variables
    // i get this from build-sbf. tried boxing the results of the above, no luck
    // then i *commented out the code and the import* and *still* got these errors from build-sbf
    // is this coming from the monorepo?? wtf is going on. gonna try to upgrade in a new branch and see what happens ig...
    // uhhhhhHHHHHHH i get the same errors when building multi-stake????
    // ok. whatever. new branch for the 1.15 update, cargo clean, see what happens then

    

        check_mpl_metadata_program(mpl_token_metadata_program_info.key)?;

        /* XXX HANA as noted in create metadata
        check_account_owner(stake_pool_info, program_id)?;
        let stake_pool = try_from_slice_unchecked::<StakePool>(&stake_pool_info.data.borrow())?;
        if !stake_pool.is_valid() {
            return Err(StakePoolError::InvalidState.into());
        }

        stake_pool.check_manager(manager_info)?;
        stake_pool.check_authority_withdraw(
            withdraw_authority_info.key,
            program_id,
            stake_pool_info.key,
        )?;
        check_mpl_metadata_account_address(metadata_info.key, &stake_pool.pool_mint)?;
        */

        // Token mint authority for stake-pool token is withdraw authority only
        let token_mint_authority = withdraw_authority_info;

        let update_metadata_accounts_instruction = update_metadata_accounts_v2(
            *mpl_token_metadata_program_info.key,
            *metadata_info.key,
            *token_mint_authority.key,
            None,
            Some(DataV2 {
                name,
                symbol,
                uri,
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            }),
            None,
            Some(true),
        );

        let (_, stake_withdraw_bump_seed) = ((), 0); //FIXME crate::find_withdraw_authority_program_address(program_id, stake_pool_info.key);

        let token_mint_authority_signer_seeds: &[&[_]] = &[
            &stake_pool_info.key.to_bytes()[..32],
            &[], //FIXME AUTHORITY_WITHDRAW,
            &[stake_withdraw_bump_seed],
        ];

        invoke_signed(
            &update_metadata_accounts_instruction,
            &[
                metadata_info.clone(),
                withdraw_authority_info.clone(),
                mpl_token_metadata_program_info.clone(),
            ],
            &[token_mint_authority_signer_seeds],
        )?;

        Ok(())
    }

    /// Processes [Instruction](enum.Instruction.html).
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = StakePoolInstruction::try_from_slice(input)?;
        match instruction {
            StakePoolInstruction::Initialize => {
                msg!("Instruction: Initialize");
                Self::process_initialize(program_id, accounts)
            }
            StakePoolInstruction::DepositStake {
                vote_account_address,
            } => {
                msg!("Instruction: DepositStake");
                Self::process_deposit_stake(program_id, accounts, &vote_account_address)
            }
            StakePoolInstruction::WithdrawStake {
                vote_account_address,
                user_stake_authority,
                token_amount,
            } => {
                msg!("Instruction: WithdrawStake");
                Self::process_withdraw_stake(
                    program_id,
                    accounts,
                    &vote_account_address,
                    &user_stake_authority,
                    token_amount,
                )
            }
            StakePoolInstruction::CreateTokenMetadata {
                vote_account_address,
            } => {
                msg!("Instruction: CreateTokenMetadata");
                Self::process_create_pool_token_metadata(
                    program_id,
                    accounts,
                    &vote_account_address,
                )
            }
            StakePoolInstruction::UpdateTokenMetadata { name, symbol, uri } => {
                msg!("Instruction: UpdateTokenMetadata");
                Self::process_update_pool_token_metadata(program_id, accounts, name, symbol, uri)
            }
        }
    }
}

impl PrintProgramError for StakePoolError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            StakePoolError::AlreadyInUse => msg!("Error: The account cannot be initialized because it is already being used"),
            StakePoolError::InvalidProgramAddress => msg!("Error: The program address provided doesn't match the value generated by the program"),
            StakePoolError::InvalidState => msg!("Error: The stake pool state is invalid"),
            StakePoolError::CalculationFailure => msg!("Error: The calculation failed"),
            StakePoolError::FeeTooHigh => msg!("Error: Stake pool fee > 1"),
            StakePoolError::WrongAccountMint => msg!("Error: Token account is associated with the wrong mint"),
            StakePoolError::WrongManager => msg!("Error: Wrong pool manager account"),
            StakePoolError::SignatureMissing => msg!("Error: Required signature is missing"),
            StakePoolError::InvalidValidatorStakeList => msg!("Error: Invalid validator stake list account"),
            StakePoolError::InvalidFeeAccount => msg!("Error: Invalid manager fee account"),
            StakePoolError::WrongPoolMint => msg!("Error: Specified pool mint account is wrong"),
            StakePoolError::WrongStakeState => msg!("Error: Stake account is not in the state expected by the program"),
            StakePoolError::UserStakeNotActive => msg!("Error: User stake is not active"),
            StakePoolError::ValidatorAlreadyAdded => msg!("Error: Stake account voting for this validator already exists in the pool"),
            StakePoolError::ValidatorNotFound => msg!("Error: Stake account for this validator not found in the pool"),
            StakePoolError::InvalidStakeAccountAddress => msg!("Error: Stake account address not properly derived from the validator address"),
            StakePoolError::StakeListOutOfDate => msg!("Error: Identify validator stake accounts with old balances and update them"),
            StakePoolError::StakeListAndPoolOutOfDate => msg!("Error: First update old validator stake account balances and then pool stake balance"),
            StakePoolError::UnknownValidatorStakeAccount => {
                msg!("Error: Validator stake account is not found in the list storage")
            }
            StakePoolError::WrongMintingAuthority => msg!("Error: Wrong minting authority set for mint pool account"),
            StakePoolError::UnexpectedValidatorListAccountSize=> msg!("Error: The size of the given validator stake list does match the expected amount"),
            StakePoolError::WrongStaker=> msg!("Error: Wrong pool staker account"),
            StakePoolError::NonZeroPoolTokenSupply => msg!("Error: Pool token supply is not zero on initialization"),
            StakePoolError::StakeLamportsNotEqualToMinimum => msg!("Error: The lamports in the validator stake account is not equal to the minimum"),
            StakePoolError::IncorrectDepositVoteAddress => msg!("Error: The provided deposit stake account is not delegated to the preferred deposit vote account"),
            StakePoolError::IncorrectWithdrawVoteAddress => msg!("Error: The provided withdraw stake account is not the preferred deposit vote account"),
            StakePoolError::InvalidMintFreezeAuthority => msg!("Error: The mint has an invalid freeze authority"),
            StakePoolError::FeeIncreaseTooHigh => msg!("Error: The fee cannot increase by a factor exceeding the stipulated ratio"),
            StakePoolError::WithdrawalTooSmall => msg!("Error: Not enough pool tokens provided to withdraw 1-lamport stake"),
            StakePoolError::DepositTooSmall => msg!("Error: Not enough lamports provided for deposit to result in one pool token"),
            StakePoolError::InvalidStakeDepositAuthority => msg!("Error: Provided stake deposit authority does not match the program's"),
            StakePoolError::InvalidSolDepositAuthority => msg!("Error: Provided sol deposit authority does not match the program's"),
            StakePoolError::InvalidPreferredValidator => msg!("Error: Provided preferred validator is invalid"),
            StakePoolError::TransientAccountInUse => msg!("Error: Provided validator stake account already has a transient stake account in use"),
            StakePoolError::InvalidSolWithdrawAuthority => msg!("Error: Provided sol withdraw authority does not match the program's"),
            StakePoolError::SolWithdrawalTooLarge => msg!("Error: Too much SOL withdrawn from the stake pool's reserve account"),
            StakePoolError::InvalidMetadataAccount => msg!("Error: Metadata account derived from pool mint account does not match the one passed to program"),
            StakePoolError::UnsupportedMintExtension => msg!("Error: mint has an unsupported extension"),
            StakePoolError::UnsupportedFeeAccountExtension => msg!("Error: fee account has an unsupported extension"),
        }
    }
}
