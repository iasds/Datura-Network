use std::collections::HashMap;
use tracing::{event,instrument};
use opentelemetry::{global,KeyValue};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::fmt::Debug;
use crate::consts;
use tokio::time::{Instant,Duration,sleep};

mod resource;
use resource::*;

mod consumer;
use consumer::*;

mod messages;
pub use messages::ResourceMessage;

//total unit is calculated based on the maximum number of reachable rungs
const TOTAL_UNITS: u64 = 2u64.pow(consts::MAX_RUNG + 1) - 1;
const ALLOC_PROJECTION_TIME: Duration = Duration::from_secs(1);
const ALLOCATION_TIME: Duration = Duration::from_secs(5);

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

    pub async fn start(this: Arc<Self>) {
        let mut last_allocation = Instant::now();
        let mut last_projection = Instant::now();
        loop {
            let allocating = Instant::now() - last_allocation >= ALLOCATION_TIME;

            //map per resource type of availability
            let mut available_resources= HashMap::new();
            let res_guard = self.resources.read().await;
            for r in res_guard.iter() {
                available_resources.insert(r.resource_type,(r.total_available,r.total_available / TOTAL_UNITS));
            }
            drop(res_guard);

            //for the on_ramp we update immediately to allocate a minimum based on the
            //current availability
            let mut ramp_guard = self.on_ramp.write().await;

            if allocating {
                let mut alloc_guard = self.allocations.write().await;
                //move the on_ramp population to the main allocation pool
                for (id, consumer) in ramp_guard.drain() {
                    for (rtype, allocation) in consumer.iter_mut() {
                        if let Some(mut res) = self.available_resources.get_mut(rtype) {
                            res.total_allocated -= allocation.current_allocation;
                        }
                        else {
                            panic!("allocated on_ramp resource doesn't exist");
                        }
                    }
                    alloc_guard.insert(id,consumer);
                }

                let total_to_allocate = alloc_guard.iter().fold(0,|acc,(_,v)|{
                    
                });
            }
            else {
                for (_,mut consumer) in ramp_guard.iter_mut() {
                    for (rtype,mut allocation) in consumer.allocations.iter_mut() {
                        let new_total = 2.pow(consumer.rung) * available_resources_unit_sizes.get(rtype).unwrap();
                        allocation.current_allocation = new_total;
                    }
                }
            }
            

            sleep(ALLOC_PROJECTION_TIME).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ResourceManager;
    use proptest::prelude::*;
    use std::sync::Arc;

}
