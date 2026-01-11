use std::collections::HashMap;
use tracing::{event,instrument};
use std::default::Default;
use opentelemetry::{global,KeyValue};
use tokio::sync::RwLock;

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
    onRamp: RwLock<HashMap<Consumer, Allocation>>,
    allocations: RwLock<HashMap<Consumer,Allocation>>,
}

impl ResourceManager<Consumer> {
    pub fn new(total_available: u64, service_name: &'static str, resource_name: &'static str) -> Arc<Self> {
        let unit_size = total_available / TOTAL_UNITS;
        let result = Arc::new(ResourceManager {
            total_available,
            total_allocated: 0,
            unit_size,
            onRamp: RwLock::new(HashMap::new()),
            allocations: RwLock::new(HashMap::new()),
        });

        let obs_rm = result.clone();
        let meter = global::get_meter(&service_name);
        let _usage_gauge = meter.f64_observable_gauge(resource_name).with_callback(|observer|{
            observer.observe(
                obs_rm.total_allocated as f64 / obs_rm.total_available as f64,
                &[
                    KeyValue::new("metric","percent_used")
                ]
            )}).build();
        let obs_rm = result.clone();
        let _onboard_queue = meter.u64_observable_gauge(resource_name).with_callback(|observer|{
            observer.observe(
                obs_rm.onRamp.len(),
                &[
                    KeyValue::new("metric","queue_size")
                ]
            )}).build();

        let obs_rm = result.clone();
        let _total_clients = meter.u64_observable_gauge(resource_name).with_callback(|observer|{
            observer.observe(
                obs_rm.allocations.len(),
                &[
                    KeyValue::new("metric","total_clients")
                ]
            )}).build();

        let obs_rm = result.clone();
        let _max_rung_reached = meter.u64_observable_gauge(resource_name).with_callback(|observer|{
            let read_guard = obs_rm.onRamp.blocking_read().unwrap();
            let max_onramp = *read_guard.values().fold(0u64,|acc,v| {
                if v.rung_reached as u64 > acc {
                    rung_reached as u64
                }
                else {
                    acc
                }
            });

            drop(read_guard);
            let read_guard = obs_rm.allocations.blocking_read().unwrap();
            let max_overall = *read_guard.values().fold(obs_rm.allocations,|acc,v|{
                if v.rung_reached as u64 > acc {
                    rung_reached as u64
                }
                else {
                    acc
                }
            });
            drop(read_guard);
            observer.observe(
                max_overall,
                &[
                    KeyValue::new("metric","max_rung_reached")
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
