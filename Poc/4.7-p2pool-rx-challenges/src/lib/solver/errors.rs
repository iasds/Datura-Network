use thiserror::Error;
use crate::client::models::ServerReply;

#[derive(Error,Debug)]
pub enum SolverError{
    JobCreationError(ServerReply)
}
