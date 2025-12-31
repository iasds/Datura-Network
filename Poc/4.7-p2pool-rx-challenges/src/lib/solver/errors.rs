use thiserror::Error;
use randomx_rs::*;
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
    #[error("error with the underlying randomX process")]
    RandomXError(#[from]RandomXError),
    #[error("job timedout")]
    TimeoutError(String),
    #[error("invalid configuration error")]
    ConfigurationError(String),
    #[error("worker thread error")]
    WorkerError(#[from]WorkerError),

}
