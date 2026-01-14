/// Message types for resource allocation communication
#[derive(Debug, Clone)]
pub enum ResourceMessage {
    /// Request for allocation
    AllocationRequest,
    /// Response with allocation results
    AllocationResponse,
}
