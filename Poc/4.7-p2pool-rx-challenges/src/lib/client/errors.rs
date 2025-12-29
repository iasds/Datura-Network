use thiserror::Error;
use crate::solver::SolverError;

#[derive(Error,Debug)]
pub enum ClientError {
    #[error("p2pool server error")]
    P2poolError(#[from] PoolError),
    #[error("error reading stream from server")]
    ReadError(#[from]tokio::io::Error),
    #[error("error parsing server message")]
    ParseError(#[from]serde_json::Error),
    #[error("error converting job to DaturaPow")]
    ConversionError(#[from]SolverError),
    #[error("unknown job Id")]
    UnknownJob,
}

#[derive(Error,Debug)]
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
