//! Error types

use {
    num_derive::FromPrimitive,
    solana_program::{decode_error::DecodeError, program_error::ProgramError},
    thiserror::Error,
};

/// Errors that may be returned by the StakePool program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum SinglePoolError {
    // 0.
    /// Provided pool stake account does not match stake account derived for validator vote account.
    #[error("InvalidPoolStakeAccount")]
    InvalidPoolStakeAccount,
    /// Provided pool authority does not match authority derived for validator vote account.
    #[error("InvalidPoolAuthority")]
    InvalidPoolAuthority,
    /// Provided pool mint does not match mint derived for validator vote account.
    #[error("InvalidPoolMint")]
    InvalidPoolMint,
    /// Provided metadata account does not match metadata account derived for pool mint.
    #[error("InvalidMetadataAccount")]
    InvalidMetadataAccount,
    /// Authorized withdrawer provided for metadata update does not match the vote account.
    #[error("InvalidMetadataSigner")]
    InvalidMetadataSigner,

    // 5.
    /// Not enough lamports provided for deposit to result in one pool token.
    #[error("DepositTooSmall")]
    DepositTooSmall,
    /// Not enough pool tokens provided to withdraw stake worth one lamport.
    #[error("WithdrawalTooSmall")]
    WithdrawalTooSmall,
    /// Required signature is missing.
    #[error("SignatureMissing")]
    SignatureMissing,
    /// Stake account is not in the state expected by the program.
    #[error("WrongStakeState")]
    WrongStakeState,
    /// Unsigned subtraction crossed the zero.
    #[error("ArithmeticUnderflow")]
    ArithmeticUnderflow,

    // 10.
    /// A calculation failed unexpectedly.
    /// (This error should never be surfaced; it stands in for failure conditions that should never be reached.)
    #[error("UnexpectedMathError")]
    UnexpectedMathError,
    /// Failed to parse vote account.
    #[error("UnparseableVoteAccount")]
    UnparseableVoteAccount,
}
impl From<SinglePoolError> for ProgramError {
    fn from(e: SinglePoolError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
impl<T> DecodeError<T> for SinglePoolError {
    fn type_of() -> &'static str {
        "Single Pool Error"
    }
}
