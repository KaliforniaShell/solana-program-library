#![allow(dead_code)]
#![allow(unused_imports)] // FIXME remove

use {
    crate::multi_pool::*,
    crate::single_pool::*,
    borsh::BorshSerialize,
    mpl_token_metadata::{pda::find_metadata_account, state::Metadata},
    solana_program::{
        borsh::{get_instance_packed_len, get_packed_len, try_from_slice_unchecked},
        hash::Hash,
        instruction::Instruction,
        program_option::COption,
        program_pack::Pack,
        pubkey::Pubkey,
        stake, system_instruction, system_program,
    },
    solana_program_test::{processor, BanksClient, ProgramTest, ProgramTestContext},
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
    spl_associated_token_account as atoken, spl_stake_pool as mpool,
    spl_stake_single_pool as spool,
    spl_token_2022::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account, Mint},
    },
    std::{convert::TryInto, num::NonZeroU32},
};

#[allow(clippy::too_many_arguments)]
pub async fn create_mint(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    program_id: &Pubkey,
    pool_mint: &Keypair,
    manager: &Pubkey,
    decimals: u8,
    extension_types: &[ExtensionType],
) -> Result<(), TransportError> {
    assert!(extension_types.is_empty() || program_id != &spl_token::id());
    let rent = banks_client.get_rent().await.unwrap();
    let space = ExtensionType::get_account_len::<Mint>(extension_types);
    let mint_rent = rent.minimum_balance(space);
    let mint_pubkey = pool_mint.pubkey();

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &mint_pubkey,
        mint_rent,
        space as u64,
        program_id,
    )];
    for extension_type in extension_types {
        let instruction = match extension_type {
            ExtensionType::MintCloseAuthority =>
                spl_token_2022::instruction::initialize_mint_close_authority(
                    program_id,
                    &mint_pubkey,
                    Some(manager),
                ),
            ExtensionType::DefaultAccountState =>
                spl_token_2022::extension::default_account_state::instruction::initialize_default_account_state(
                    program_id,
                    &mint_pubkey,
                    &spl_token_2022::state::AccountState::Initialized,
                ),
            ExtensionType::TransferFeeConfig => spl_token_2022::extension::transfer_fee::instruction::initialize_transfer_fee_config(
                program_id,
                &mint_pubkey,
                Some(manager),
                Some(manager),
                100,
                1_000_000,
            ),
            ExtensionType::InterestBearingConfig => spl_token_2022::extension::interest_bearing_mint::instruction::initialize(
                program_id,
                &mint_pubkey,
                Some(*manager),
                600,
            ),
            ExtensionType::NonTransferable =>
                spl_token_2022::instruction::initialize_non_transferable_mint(program_id, &mint_pubkey),
            _ => unimplemented!(),
        };
        instructions.push(instruction.unwrap());
    }
    instructions.push(
        spl_token_2022::instruction::initialize_mint(
            program_id,
            &pool_mint.pubkey(),
            manager,
            None,
            decimals,
        )
        .unwrap(),
    );
    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &[payer, pool_mint],
        *recent_blockhash,
    );
    banks_client
        .process_transaction(transaction)
        .await
        .map_err(|e| e.into())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_token_account(
    banks_client: &mut BanksClient,
    payer: &Keypair,
    recent_blockhash: &Hash,
    program_id: &Pubkey,
    account: &Keypair,
    pool_mint: &Pubkey,
    authority: &Keypair,
    extensions: &[ExtensionType],
) -> Result<(), TransportError> {
    let rent = banks_client.get_rent().await.unwrap();
    let space = ExtensionType::get_account_len::<Account>(extensions);
    let account_rent = rent.minimum_balance(space);

    let mut instructions = vec![system_instruction::create_account(
        &payer.pubkey(),
        &account.pubkey(),
        account_rent,
        space as u64,
        program_id,
    )];

    for extension in extensions {
        match extension {
            ExtensionType::ImmutableOwner => instructions.push(
                spl_token_2022::instruction::initialize_immutable_owner(
                    program_id,
                    &account.pubkey(),
                )
                .unwrap(),
            ),
            ExtensionType::TransferFeeAmount
            | ExtensionType::MemoTransfer
            | ExtensionType::CpiGuard => (),
            _ => unimplemented!(),
        };
    }

    instructions.push(
        spl_token_2022::instruction::initialize_account(
            program_id,
            &account.pubkey(),
            pool_mint,
            &authority.pubkey(),
        )
        .unwrap(),
    );

    let mut signers = vec![payer, account];
    for extension in extensions {
        match extension {
            ExtensionType::MemoTransfer => {
                signers.push(authority);
                instructions.push(
                spl_token_2022::extension::memo_transfer::instruction::enable_required_transfer_memos(
                    program_id,
                    &account.pubkey(),
                    &authority.pubkey(),
                    &[],
                )
                .unwrap()
                )
            }
            ExtensionType::CpiGuard => {
                signers.push(authority);
                instructions.push(
                    spl_token_2022::extension::cpi_guard::instruction::enable_cpi_guard(
                        program_id,
                        &account.pubkey(),
                        &authority.pubkey(),
                        &[],
                    )
                    .unwrap(),
                )
            }
            ExtensionType::ImmutableOwner | ExtensionType::TransferFeeAmount => (),
            _ => unimplemented!(),
        }
    }

    let transaction = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        *recent_blockhash,
    );
    banks_client
        .process_transaction(transaction)
        .await
        .map_err(|e| e.into())
}

pub async fn create_ata(
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
