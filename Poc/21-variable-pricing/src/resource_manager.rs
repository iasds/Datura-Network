use std::collections::HashMap;
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

mod messages;
pub use messages::ResourceMessage;

//total unit is calculated based on the maximum number of reachable rungs
const TOTAL_UNITS: u64 = 2u64.pow(consts::MAX_RUNG + 1) - 1;

#[derive(Debug)]
pub struct ResourceManager {
    allocations: RwLock<HashMap<u64,Consumer>>,
    on_ramp: RwLock<HashMap<u64,Consumer>>,
    resources: RwLock<Vec<Resource>>,
    work_reports_receiver: mpsc::Receiver<WorkReport>,
    pub work_reports_sender: mpsc::Sender<WorkReport>,
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
}

#[cfg(test)]
mod tests {
    use super::ResourceManager;
    use proptest::prelude::*;
    use std::sync::Arc;

}
