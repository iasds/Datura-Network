use super::TOTAL_UNITS;

#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
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
pub struct Resource {
    pub resource_type: ResourceType,
    pub total_available: u64,
    pub total_allocated: u64,
    pub unit_size: u64,
}

impl Resource {
    pub fn new(resource_type: ResourceType, total_available: u64) -> Self {
        Resource {
            resource_type,
            total_available,
            total_allocated: 0,
            unit_size: total_available / TOTAL_UNITS,
        }
    }
}


