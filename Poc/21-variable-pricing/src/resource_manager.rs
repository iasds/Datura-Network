use std::collections::HashMap;
use uuid::Uuid;
use tracing::{event,instrument};
use opentelemetry::{global,KeyValue};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::fmt::Debug;
use crate::consts;

mod resource;
use resource::*;

mod consumer;
use consumer::*;

//total unit is calculated based on the maximum number of reachable rungs
const TOTAL_UNITS: u64 = 2u64.pow(consts::MAX_RUNG + 1) - 1;

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

        let meter = global::meter(&service_name);

        
        let obs_rm = result.clone();
        let _usage_gauge = meter.f64_observable_gauge("resource_manager_usage").with_callback(move |observer|{
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
        let _clients = meter.u64_observable_gauge("resource_manager_clients").with_callback(move |observer|{
            let ramp_guard = obs_rm.on_ramp.blocking_read();
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


            let alloc_guard = obs_rm.allocations.blocking_read();
            let max_overall = alloc_guard.values().fold(max_onramp,|acc,v|{
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
