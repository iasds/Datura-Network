use super::worker::WorkerError;
use randomx_rs::*;
use std::array::TryFromSliceError;
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SolverError {
    #[error("error converting job hex to datura pow input")]
    HexError(#[from] hex::FromHexError),
    #[error("failed converting the hex to an u8 slice")]
    HexToSliceError(#[from] TryFromSliceError),
    #[error("error creating daturaPoW challenge")]
    DaturaPowCreationError(#[from] ParseIntError),
    #[error("challenge response is invalid")]
    DaturaPowInvalidResponse,
    #[error("error with the underlying randomX process")]
    RandomXError(#[from] RandomXError),
    #[error("job timedout")]
    TimeoutError(String),
    #[error("invalid configuration error")]
    ConfigurationError(String),
    #[error("worker thread error")]
    WorkerError(#[from] WorkerError),
}
