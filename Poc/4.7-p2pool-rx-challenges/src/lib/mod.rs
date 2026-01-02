mod client;
pub mod consts;
mod solver;
pub use client::{Client, ClientError};
pub use Solver::{Solver, SolverJob, SolverMode,SolverResult};
