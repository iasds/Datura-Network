//! copied from Pow-4.
use randomx_rs::RandomXVM;

const CHALLENGE_DIFFICULTY: u8 = 4;

pub(crate) fn create_challenge() -> [u8; 16] {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).unwrap();
    buf[0] = CHALLENGE_DIFFICULTY & 0b111111;

    buf
}
