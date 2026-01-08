use crate::solver::SolverError;
use thiserror::Error;

///errors related to the client itself
#[derive(Error, Debug)]
pub enum ClientError {
    ///error communicationg with the p2pool server
    #[error("p2pool server error")]
    P2poolError(#[from] PoolError),

    ///Error creating the client
    #[error("wrong initialization parameters")]
    InitializationError(String),

    ///stream connection lost/corrupted
    #[error("error reading stream from server")]
    ReadError(#[from] tokio::io::Error),

    ///unknown/unparseable p2pool message received
    #[error("error parsing server message")]
    ParseError(#[from] serde_json::Error),

    ///Conversion failure from p2pool job to daturaPow
    #[error("error converting job to DaturaPow")]
    ConversionError(#[from] SolverError),
}

///Errors related with the p2pool server/proxy instance
#[derive(Error, Debug)]
pub enum PoolError {
    #[error("p2pool server disconnected")]
    ServerDisconnected,
    #[error("p2pool server sent unknown reply")]
    UnknownServerReply(String),
    #[error("share was refused by the server")]
    ShareError(String),
    #[error("invalid job Id")]
    InvalidJobId,
    #[error("invalid share")]
    InvalidShare,
    #[error("low difficulty share")]
    LowDifficultyShare,
}
