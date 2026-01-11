use std::collections::HashMap;
use tracing::{event,instrument};
use std::default::Default;
use opentelemetry::{global,KeyValue};

/**
every rung equals one order magnitude more of difficulty, here we are effectively capping max
at 54 bits targets, the higher the more units will be created but the hardest it will be to reach
**/
pub const MAX_RUNG: u32 = 10;

//total unit is calculated based on the maximum number of reachable rungs
pub const TOTAL_UNITS: u64 = 2u64.pow(MAX_RUNG + 1) - 1;

///Minimum diff is high enough to incur some work
pub const MIN_DIFF: u32 = 256;

#[derive(Debug)]
pub struct Allocation {
    rung_reached: u8,
    current_allocation: u64,
    projected_allocation: u64,
}

impl Default for Allocation {
    pub fn default() -> Self {
        Allocation {
            rung_reached: 0,
            current_allocation: 0,
            projected_allocation: 0,
        }
    }
}


#[derive(Debug)]
pub struct ResourceManager<Consumer> {
    total_available: u64,
    total_allocated: u64,
    unit_size: u64,
    onRamp: HashMap<Consumer, Allocation>,
    allocations: HashMap<Consumer,Allocation>,
}

impl ResourceManager<Consumer> {
    pub fn new(total_available: u64, service_name: &'static str, resource_name: &'static str) -> Arc<Self> {
        let unit_size = total_available / TOTAL_UNITS;
        let result = Arc::new(ResourceManager {
            total_available,
            total_allocated: 0,
            unit_size,
            onRamp: HashMap::new(),
            allocations: HashMap::new(),
        });

        let obs_rm = result.clone();
        let meter = global::get_meter(&service_name);
        let usage_gauge = meter.f64_observable_gauge(resource_name).with_callback(|observer|{
            observer.observe(
                obs_rm.total_allocated as f64 / obs_rm.total_available as f64,
                &[
                    KeyValue::new("percent_allocated",resource_name)
                ]
            )}).build();
        result
    }

    ///Add a new consumer, intially will be inside the onRamp and get resource from the available
    ///pool
    pub fn onboard(&mut self, consumer: Consumer) {
        self.onRamp.insert(consumer, Allocation::default());
    }

    ///update the rung status for an existing consumer. if they are in the onRamp this whill
    ///immediately update their current allocation, else it will just update the allocation state
    ///for the next epoch
    #[instrument]
    pub fn update_rung(&mut self, consumer:Consumer, new_rung: u8) -> Result<(),ManagerError> {

    }


    ///do a global allocation and bring inside the main pool anyone being onboarded based on their
    ///current acomplished work
    #[instrument]
    pub fn global_allocate(&mut self) {

    }

}

#[cfg(test)]
mod tests {
    use super::ResourceManager;
    use proptest::prelude::*;

    prop_compose! {
        pub fn new_rm()(total_available in 1..u64::MAX) -> ResourceManager<()> {
            ResourceManager::new(total_available, "rm_test","dummy_resource")
        }
    }

    proptest! {
        #[test]
        fn test_units_not_0(rm in new_rm()){
            assert!(rm.unit_size > 0);
        }
    }
}
