use std::collections::HashMap;
use std::fmt::Debug;
use crate::consts;

mod consumer;
pub use consumer::Consumer;

#[derive(Debug)]
pub struct SecureAllocationSystem {
    pub capacity: f64,
    compression_exponent: f64,
    min_contribution_threshold: u64,
    max_reserve_percentage: f64,
    reserve_per_epoch: f64,
    max_tenure_epochs: u32,
}

impl SecureAllocationSystem {
    pub fn new(capacity: f64) -> Self {
        SecureAllocationSystem {
            capacity,
            compression_exponent: consts::COMPRESSION_EXPONENT,
            min_contribution_threshold: consts::MIN_CONTRIBUTION_THRESHOLD,
            max_reserve_percentage: consts::MAX_RESERVE_PERCENTAGE,
            reserve_per_epoch: consts::RESERVE_PER_EPOCH_TENURE,
            max_tenure_epochs: consts::MAX_TENURE_EPOCHS,
        }
    }

    pub fn calculate_allocation(&self, clients: &mut [Consumer]) {
        if clients.is_empty() {
            return;
        }

        // Separate qualified clients
        let qualified_ids: Vec<u64> = clients
            .iter()
            .filter(|c| c.contribution >= self.min_contribution_threshold)
            .map(|c| c.id)
            .collect();

        // Calculate individual reserve rates (tenure-based)
        let mut reserve_per_client: HashMap<u64, f64> = HashMap::new();
        let mut reserve_total = 0.0;

        for client in clients.iter() {
            if !qualified_ids.contains(&client.id) {
                continue;
            }

            let epochs = client.epochs_connected.min(self.max_tenure_epochs);
            let client_reserve_rate = (epochs as f64) * self.reserve_per_epoch;

            // Contribution-weighted reserve
            let compressed = client.contribution as f64;
            let compressed_pow = compressed.powf(self.compression_exponent);
            let reserve = client_reserve_rate * compressed_pow;

            reserve_per_client.insert(client.id, reserve);
            reserve_total += reserve;
        }

        // Cap total reserve
        let (reserve_capacity, reserve_scale) = if reserve_total > 0.0 {
            let capacity_reserve = self.capacity * self.max_reserve_percentage;
            let scale = capacity_reserve / reserve_total;
            (capacity_reserve, scale)
        } else {
            (0.0, 0.0)
        };

        // Distribute remaining by contribution
        let distributable = self.capacity - reserve_capacity;

        // Calculate compressed contributions for all clients
        let mut compressed: HashMap<u64, f64> = HashMap::new();
        let mut total_compressed = 0.0;

        for client in clients.iter() {
            let comp = client.contribution as f64;
            let comp_pow = comp.powf(self.compression_exponent);
            compressed.insert(client.id, comp_pow);
            total_compressed += comp_pow;
        }

        // Calculate allocations
        for client in clients.iter_mut() {
            // Minimum (if qualified)
            let minimum = if let Some(&reserve) = reserve_per_client.get(&client.id) {
                reserve * reserve_scale
            } else {
                0.0
            };

            // Earned
            let earned = if total_compressed > 0.0 {
                let share = compressed.get(&client.id).unwrap_or(&0.0) / total_compressed;
                share * distributable
            } else {
                0.0
            };

            client.allocation = minimum + earned;
        }
    }
}

#[derive(Debug)]
pub struct ResourceManager {
    allocations: HashMap<u64, Consumer>,
    on_ramp: HashMap<u64, Consumer>,
    allocation_system: SecureAllocationSystem,
}

impl ResourceManager {
    pub fn new(capacity: f64) -> Self {
        ResourceManager {
            on_ramp: HashMap::new(),
            allocations: HashMap::new(),
            allocation_system: SecureAllocationSystem::new(capacity),
        }
    }

    pub fn allocate(&mut self, mut clients: Vec<Consumer>) -> Vec<Consumer> {
        self.allocation_system.calculate_allocation(&mut clients);
        clients
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_qualified_clients() {
        let mut rm = ResourceManager::new(1000.0);

        let clients = vec![
            Consumer::new(1, 50_000, 5),
            Consumer::new(2, 100_000, 10),
        ];

        let allocated = rm.allocate(clients);

        assert!(allocated[0].allocation > 0.0);
        assert!(allocated[1].allocation > allocated[0].allocation);
    }

    #[test]
    fn test_allocation_unqualified_clients() {
        let mut rm = ResourceManager::new(1000.0);

        let clients = vec![
            Consumer::new(1, 10_000, 5), // Below threshold - gets no reserve but gets earned share
            Consumer::new(2, 100_000, 10),
        ];

        let allocated = rm.allocate(clients);

        // Unqualified clients don't get minimum reserve but still get allocation from earned share
        assert!(allocated[0].allocation > 0.0);
        assert!(allocated[1].allocation > allocated[0].allocation);
    }

    #[test]
    fn test_allocation_comprehensive() {
        // Test the full allocation algorithm with various scenarios
        let capacity = 1000.0;
        let mut rm = ResourceManager::new(capacity);

        // Scenario: Mixed qualified and unqualified clients
        let clients = vec![
            Consumer::new(1, 50_000, 0),    // Qualified, no tenure
            Consumer::new(2, 100_000, 10),  // Qualified, max tenure
            Consumer::new(3, 200_000, 5),   // Qualified, high contribution
            Consumer::new(4, 25_000, 10),   // Unqualified despite tenure
            Consumer::new(5, 60_000, 0),    // Qualified, no tenure
        ];

        let allocated = rm.allocate(clients);

        // Verify total allocation doesn't exceed capacity
        let total_allocated: f64 = allocated.iter().map(|c| c.allocation).sum();
        assert!(total_allocated <= capacity * 1.001, // Small floating point tolerance
                "Total allocation {} exceeds capacity {}", total_allocated, capacity);

        // Verify qualified clients get allocation
        assert!(allocated[0].allocation > 0.0, "Client 1 (qualified) should get allocation");
        assert!(allocated[1].allocation > 0.0, "Client 2 (qualified) should get allocation");
        assert!(allocated[2].allocation > 0.0, "Client 3 (qualified) should get allocation");
        assert!(allocated[4].allocation > 0.0, "Client 5 (qualified) should get allocation");

        // Verify unqualified client still gets allocation (from earned share, not reserve)
        assert!(allocated[3].allocation > 0.0, "Client 4 (unqualified) should get earned allocation");

        // Verify higher contribution gives higher allocation
        assert!(allocated[2].allocation > allocated[0].allocation,
                "Higher contribution (200k > 50k) should yield higher allocation");
        assert!(allocated[2].allocation > allocated[4].allocation,
                "Higher contribution (200k > 60k) should yield higher allocation");

        // Verify tenure bonus: Client 2 (100k contribution, 10 epochs) should beat Client 5 (60k, 0 epochs)
        // Even though Client 5 has less contribution, Client 2's tenure gives guaranteed minimum
        assert!(allocated[1].allocation > allocated[4].allocation,
                "Higher tenure should provide meaningful allocation boost");
    }

    #[test]
    fn test_allocation_reserve_calculation() {
        // Test that reserve (minimum guarantee) is correctly calculated
        let capacity = 1000.0;
        let mut rm = ResourceManager::new(capacity);

        // Two clients with different tenure
        let clients = vec![
            Consumer::new(1, 100_000, 1),   // 1 epoch of tenure
            Consumer::new(2, 100_000, 5),   // 5 epochs of tenure
        ];

        let allocated = rm.allocate(clients);

        // Client with more tenure should get higher allocation (due to reserve minimum)
        assert!(allocated[1].allocation > allocated[0].allocation,
                "Client with 5 epochs should get more than client with 1 epoch (same contribution)");

        // The difference should be meaningful but not extreme
        let difference_ratio = allocated[1].allocation / allocated[0].allocation;
        assert!(difference_ratio < 2.0, "Tenure shouldn't cause >2x difference at same contribution");
        assert!(difference_ratio > 1.0, "Tenure should increase allocation");
    }

    #[test]
    fn test_allocation_compression_effect() {
        // Test that compression exponent works correctly
        let capacity = 1000.0;
        let mut rm = ResourceManager::new(capacity);

        // Clients with 10x contribution difference
        let clients = vec![
            Consumer::new(1, 50_000, 5),    // Base contribution
            Consumer::new(2, 500_000, 5),   // 10x contribution
        ];

        let allocated = rm.allocate(clients);

        // With compression exponent 0.5, 10x contribution should yield ~sqrt(10) ≈ 3.16x allocation
        // But with tenure reserves, the ratio might be slightly different
        let ratio = allocated[1].allocation / allocated[0].allocation;

        // Should be somewhere between sqrt(10) ≈ 3.16 and 10 (no compression)
        assert!(ratio > 2.5, "10x contribution should yield significant allocation increase");
        assert!(ratio < 8.0, "Compression should prevent linear allocation scaling");
    }

    #[test]
    fn test_allocation_empty_clients() {
        // Test edge case: no clients
        let mut rm = ResourceManager::new(1000.0);
        let clients = vec![];

        let result = rm.allocate(clients);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_allocation_single_client() {
        // Test edge case: single qualified client
        let mut rm = ResourceManager::new(1000.0);
        let clients = vec![Consumer::new(1, 100_000, 5)];

        let allocated = rm.allocate(clients);

        // Single qualified client should get nearly all capacity
        assert!(allocated[0].allocation > 950.0,
                "Single qualified client should get nearly full capacity");
        assert!(allocated[0].allocation <= 1000.0);
    }

    #[test]
    fn test_allocation_all_unqualified() {
        // Test edge case: all clients below contribution threshold
        let mut rm = ResourceManager::new(1000.0);
        let clients = vec![
            Consumer::new(1, 10_000, 5),
            Consumer::new(2, 20_000, 10),
            Consumer::new(3, 30_000, 3),
        ];

        let allocated = rm.allocate(clients);

        // Unqualified clients still get allocation, just no minimum guarantee
        // They split capacity proportionally by compressed contribution
        let total: f64 = allocated.iter().map(|c| c.allocation).sum();
        assert!(total > 0.0, "Unqualified clients should still share capacity");
        assert!(total <= 1000.0, "Total should not exceed capacity");

        // Higher contribution should still get more
        assert!(allocated[2].allocation > allocated[0].allocation,
                "Higher contribution should get more even if unqualified");
    }

    #[test]
    fn test_allocation_fairness_proportional() {
        // Test that allocation is roughly proportional to compressed contribution
        let capacity = 1000.0;
        let mut rm = ResourceManager::new(capacity);

        // Three clients with 1:2:4 contribution ratio, all qualified, no tenure
        let clients = vec![
            Consumer::new(1, 100_000, 0),
            Consumer::new(2, 400_000, 0),
            Consumer::new(3, 1_600_000, 0),
        ];

        let allocated = rm.allocate(clients);

        // Compressed ratio: sqrt(100k) : sqrt(400k) : sqrt(1.6M) = 316.2 : 632.5 : 1264.9 ≈ 1 : 2 : 4
        // Expected allocation ratio should follow compressed contribution
        let ratio_1_2 = allocated[1].allocation / allocated[0].allocation;
        let ratio_1_3 = allocated[2].allocation / allocated[0].allocation;

        // Allow 10% variance for floating point math
        assert!(ratio_1_2 > 1.8 && ratio_1_2 < 2.2,
                "Allocation ratio 1:2 should hold (got {})", ratio_1_2);
        assert!(ratio_1_3 > 3.6 && ratio_1_3 < 4.4,
                "Allocation ratio 1:4 should hold (got {})", ratio_1_3);
    }
}
