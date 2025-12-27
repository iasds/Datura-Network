use thiserror:Error;
use crate::DaturaPow

#[derive(Error,Debug)]
pub enum ClientError {
    #[error("p2pool server disconnected")]
    ServerDisconnected,
    #[error("p2pool server sent unknown reply")]
    UnknownServerReply(String),
    #[error("error reading stream from server")]
    ReadError(String)
}
