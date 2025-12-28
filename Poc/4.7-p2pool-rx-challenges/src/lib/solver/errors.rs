use thiserror::Error;
use std::num::ParseIntError;
use std::array::TryFromSliceError;
use crate::client::ServerReply;

#[derive(Error,Debug)]
pub enum SolverError{
    #[error("couldn't create daturapow from this server reply")]
    JobCreationError(ServerReply),
    #[error("error converting job hex to datura pow input")]
    HexError(String),
    #[error("failed converting the hex to an u8 slice")]
    HexToSliceError(String),
    #[error("error creating daturaPoW challenge")]
    DaturaPowCreationError(String),
    #[error("challenge response is invalid")]
    DaturaPowInvalidResponse,
    #[error("ran out of possible nonces trying to solve")]
    DaturaPowExhaustedSearchSpace,
    #[error("error with the underlying randomX process")]
    SolverRandomXError(String),

}

impl From<ParseIntError> for SolverError {
    fn from(err:ParseIntError) -> Self {
        SolverError::DaturaPowCreationError(err.to_string())
    }
}

impl From<hex::FromHexError> for SolverError {
    fn from(err: hex::FromHexError) -> Self {
        SolverError::HexError(err.to_string())
    }
}

impl From<TryFromSliceError> for SolverError {
    fn from(err: TryFromSliceError) -> Self {
        SolverError::HexToSliceError(err.to_string())
    }
}

impl From<randomx_rs::RandomXError> for SolverError {
    fn from(err: randomx_rs::RandomXError) -> Self {
        SolverError::SolverRandomXError(err.to_string())
    }
}
