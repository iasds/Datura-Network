pub struct ResourceManager {
    total_available: u64,
    total_allocated: u64,
}

impl ResourceManager {
    pub fn new(total_available: u64) -> Self {
        ResourceManager {
            total_available,
            total_allocated: 0,
        }
    }
}
