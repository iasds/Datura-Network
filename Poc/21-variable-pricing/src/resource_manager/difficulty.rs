use std::convert::TryFrom;
use thiserror::Error;
use std::default::Default;
use crate::consts;

struct Difficulty(u64);

impl Difficulty {
    pub fn increase(&mut self) -> Result<(),DifficultyError) {

    }
}

///Create a difficulty from a u32 rung number
impl TryFrom<u32> for Difficulty {
    type Error = DifficultyError;
}

///Create a difficulty from a compact target string (eg p2pool share diff
impl TryFrom<String> for Difficulty {
    type Error = DifficultyError;
}

impl Default for Difficulty {
    fn default() -> Self {
        let target = 2u64.pow(64);
    }
}

#[derive(Error)]
pub enum DifficultyError {
    
}
