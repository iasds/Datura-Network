extern crate stratum_v1;
use stratum_v1::Client;

pub enum JobType {
    XMR,
    Random,
}

pub struct Job {
    difficulty: u8,
}
