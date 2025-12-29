use thiserror::Error;
use std::num::ParseIntError;
use std::array::TryFromSliceError;
use crate::client::ServerReply;
use super::worker::WorkerError;

#[derive(Error,Debug)]
pub enum SolverError{
    #[error("couldn't create daturapow from this server reply")]
    JobCreationError(ServerReply),
    #[error("error converting job hex to datura pow input")]
    HexError(#[from]hex::FromHexError),
    #[error("failed converting the hex to an u8 slice")]
    HexToSliceError(#[from]TryFromSliceError),
    #[error("error creating daturaPoW challenge")]
    DaturaPowCreationError(#[from]ParseIntError),
    #[error("challenge response is invalid")]
    DaturaPowInvalidResponse,
    #[error("ran out of possible nonces trying to solve")]
    DaturaPowExhaustedSearchSpace,
    #[error("error with the underlying randomX process")]
    SolverRandomXError(#[from]randomx_rs::RandomXError),
    #[error("job timedout")]
    SolverTimeoutError(String),
    #[error("invalid configuration error")]
    SolverConfigurationError(String),
    #[error("worker thread error")]
    WorkerError(#[from]WorkerError),

}
