use std::collections::HashMap;
use uuid::Uuid;
use tracing::{event,instrument};
use opentelemetry::{global,KeyValue};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::fmt::Debug;

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
pub struct Resource {
    resource_type: ResourceType,
    total_available: u64,
    total_allocated: u64,
    unit_size: u64,
}

impl Resource {
    pub fn new(resource_type: ResourceType, total_available: u64) -> Self {
        Resource {
            resource_type,
            total_available,
            total_allocated: 0
            unit_size: total_available / TOTAL_UNITS;
        }
    }
}

#[derive(Debug,Hash)]
pub enum ResourceType {
    Bandwidth,
    Memory,
}

impl ResourceType {
    pub fn to_string(&self) -> String {
        match self {
            ResourceType::Bandwidth => "Bandwidth".to_string(),
            ResourceType::Memory => "Memory".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Allocation {
    current_allocation: u64,
    projected_allocation: u64,
}

#[derive(Debug)]
pub struct Consumer {
    allocations: HashMap<ResourceType,Allocation>,
    rung: u64,
}

impl Consumer {
    pub fn new() -> Self {
        Consumer {
            allocations: HashMap::new(),
            rung: 0,
        }
    }
}

#[derive(Debug)]
pub struct ResourceManager {
    allocations: RwLock<HashMap<Uuid,Consumer>>,
    on_ramp: RwLock<HashMap<Uuid,Consumer>>,
    resources: RwLock<Vec<Resource>>,
}

impl ResourceManager {
    pub fn new(service_name: &'static str) -> Arc<Self> {
        let result = Arc::new(ResourceManager {
            on_ramp: RwLock::new(HashMap::new()),
            allocations: RwLock::new(HashMap::new()),
            resources: RwLock::new(Vec::new()),
        });

        let obs_rm = result.clone();
        let meter = global::meter(&service_name);

        
        let obs_rm = result.clone();
        let _usage_gauge = meter.f64_observable_gauge("resource_manager_usage").with_callback(|observer|{
            let res_guard = obs_rm.resources.blocking_read();
            for r in res_guard.iter() {
                observer.observe(
                    r.total_allocated as f64 / r.total_available as f64,
                    &[
                        KeyValue::new("metric","percent_used"),
                        KeyValue::new("resource_type",r.resource_type.to_string()),
                    ]
                );
            }
}).build();

        let obs_rm = result.clone();
        let _clients = meter.u64_observable_gauge("resource_manager_clients").with_callback(|observer|{
            let ramp_guard = result.on_ramp.blocking_read();
            let max_onramp = ramp_guard.values().fold(0u64,|acc,v| {
                if v.rung  > acc {
                    v.rung
                }
                else {
                    acc
                }
            });
            observer.observe(
                max_onramp,
                &[
                    KeyValue::new("metric","max_rung_reached"),
                    KeyValue::new("status","onboarding"),
                ]
            );
            observer.observe(
                ramp_guard.len().try_into().unwrap(),
                &[
                KeyValue::new("metric","clients"),
                KeyValue::new("status","onboarding"),
                ]
            );
            drop(ramp_guard);


            let alloc_guard = result.allocations.blocking_read();
            let max_overall = ramp_guard.values().fold(max_onramp,|acc,v|{
                if v.rung  > acc {
                    v.rung
                }
                else {
                    acc
                }
            });
            observer.observe(
                alloc_guard.len().try_into().unwrap(),
                &[
                    KeyValue::new("metric","clients")
                ]
            );
            observer.observe(
                max_overall,
                &[
                    KeyValue::new("metric","max_rung_reached")
                ]
            );
        }).build();
        result
    }

    /*
    ///Add a new consumer, intially will be inside the onRamp and get resource from the available
    ///pool
    pub async fn onboard(&mut self, consumer: Uuid) {
        let guard = self.on_ramp.write().await;
        guard.insert(consumer, HashMap::new());
    }


    ///update the rung status for an existing consumer. if they are in the onRamp this whill
    ///immediately update their current allocation, else it will just update the allocation state
    ///for the next epoch
    #[instrument]
    pub fn update_rung(&mut self, consumer:Consumer, new_rung: u8) -> Result<(),ManagerError> {

    }
    */


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
    use std::sync::Arc;

}
