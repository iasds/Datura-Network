mod work_report;
mod consts;

#[derive(Debug)]
pub struct ResourceManager {
    total_available: u64,
    total_allocated: u64,
    unit_size: u64,
}

impl ResourceManager {
    pub fn new(total_available: u64) -> Self {
        let unit_size = total_available / consts::TOTAL_UNITS;
        ResourceManager {
            total_available,
            total_allocated: 0,
            unit_size,
        }
    }

    pub fn allocate(work_done: &Vec<WorkReport>) -> Vec<Allocation> {

    }
}

#[cfg(test)]
mod tests {
    use super::ResourceManager;
    use proptest::prelude::*;

    prop_compose! {
        pub fn new_rm()(total_available in 1..u64::MAX) -> ResourceManager {
            ResourceManager::new(total_available)
        }
    }

    proptest! {
        #[test]
        fn test_units_not_0(rm in new_rm()){
            assert!(rm.unit_size > 0);
        }
    }
}
