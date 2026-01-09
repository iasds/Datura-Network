use proptest::prelude::*;
use crate::consts;
use std::cmp::max;
mod work_report;

const TOTAL_UNITS: u64 = 2u64.pow(consts::MAX_RUNG + 1) - 1;

#[derive(Debug)]
pub struct ResourceManager {
    total_available: u64,
    total_allocated: u64,
    total_units: u64,
    unit_size: u64,
}

impl ResourceManager {
    pub fn new(total_available: u64, minimum_block_size: u64) -> Self {
        let unit_size = max(total_available / TOTAL_UNITS, minimum_block_size);
        ResourceManager {
            total_available,
            total_allocated: 0,
            total_units : total_available/unit_size,
            unit_size,
        }
    }
    pub fn allocate(work_done: &Vec<WorkReport>) -> Vec<Allocation> {

    }
}

prop_compose! {
    pub fn new_rm()(total_available in 1..u64::MAX)(min_block_size in 1..=total_available, total_available in Just(total_available)) -> (ResourceManager, u64) {
        (ResourceManager::new(total_available, min_block_size),min_block_size)
    }
}

proptest! {
    #[test]
    fn test_units_not_0((rm, min_size) in new_rm()){
        assert!(rm.total_units != 0);
        assert!(rm.unit_size >= min_size);
    }
}
