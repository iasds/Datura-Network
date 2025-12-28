use thiserror::Error;
use crate::solver::SolverError;

#[derive(Error,Debug)]
pub enum ClientError {
    #[error("p2pool server disconnected")]
    ServerDisconnected,
    #[error("p2pool server sent unknown reply")]
    UnknownServerReply(String),
    #[error("error reading stream from server")]
    ReadError(String),
    #[error("error parsing server message")]
    ParseError(String),
    #[error("error converting job to DaturaPow")]
    ConversionError(SolverError),
    #[error("share was refused by the server")]
    ShareError(String),
}

impl From<SolverError> for ClientError {
    fn from (err: SolverError) -> Self {
        ClientError::ConversionError(err)
    }
}

impl From<tokio::io::Error> for ClientError {
    fn from(err: tokio::io::Error) -> Self {
        ClientError::ReadError(err.to_string())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> Self {
        ClientError::ParseError(err.to_string())
    }
}
