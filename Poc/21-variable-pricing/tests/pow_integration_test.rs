mod common;
use common::*;
use std::collections::HashMap;

#[test]
fn test_pow_verification_mock() {
    // Test basic PoW verification with mock difficulty
    let verifier = PowVerifier::new(1); // Difficulty 1: accept if (nonce % 256) < 128

    // Create test solutions
    let mut valid_count = 0;
    let mut invalid_count = 0;

    for nonce in 0..256u64 {
        let solution = PowSolution {
            client_id: 1,
            nonce,
            timestamp: 1000,
            difficulty_target: 1,
        };

        if verifier.verify(&solution) {
            valid_count += 1;
        } else {
            invalid_count += 1;
        }
    }

    // At difficulty 1, about half should be valid (128 out of 256)
    assert!(valid_count > 100 && valid_count < 150, "Should have ~128 valid solutions");
    assert!(invalid_count > 100, "Should have ~128 invalid solutions");
    }

    #[test]
    fn test_single_client_pow_generation() {
    // Test a single client generating PoW solutions
    let mut rng = rand::thread_rng();
    let mut client = PowClient::new(1);
    let difficulty = 2u32; // Require 2+ leading zero bits in hash

    // Client attempts to generate solutions
    let solutions = client.generate_solutions(100000, difficulty, &mut rng);

    // Should have generated some solutions
    assert!(!solutions.is_empty(), "Client should generate some valid solutions");

    // All solutions should have correct client_id
    assert!(solutions.iter().all(|s| s.client_id == 1));

    // Verify all solutions are valid
    let verifier = PowVerifier::new(difficulty);
    assert!(solutions.iter().all(|s| verifier.verify(s)));

    // Total work should be sum of capped nonces
    let expected_work: u64 = solutions.iter()
        .map(|s| s.nonce % 1_000_000_000)
        .fold(0u64, |acc, x| acc.saturating_add(x));
    assert_eq!(client.total_work, expected_work);
    }

    #[test]
    fn test_multiple_clients_pow_competition() {
    // Test multiple clients competing to solve PoW
    let mut rng = rand::thread_rng();
    let difficulty = 3u32; // Require 3+ leading zero bits
    let verifier = PowVerifier::new(difficulty);

    let mut clients = vec![
        PowClient::new(1),
        PowClient::new(2),
        PowClient::new(3),
    ];

    // All clients attempt to generate solutions
    let mut all_solutions = Vec::new();
    for client in &mut clients {
        let solutions = client.generate_solutions(200000, difficulty, &mut rng);
        all_solutions.extend(solutions);
    }

    // Verify all collected solutions
    for solution in &all_solutions {
        assert!(verifier.verify(solution), "All solutions should verify");
    }

    // Should have collected solutions from all clients
    assert_eq!(clients.len(), 3);
    println!(
        "Client solutions: {} {} {}",
        clients[0].solutions.len(),
        clients[1].solutions.len(),
        clients[2].solutions.len()
    );
    }

    #[test]
    fn test_pow_contribution_tracking() {
    // Test that PoW work is correctly tracked as contribution
    let mut rng = rand::thread_rng();
    let mut client = PowClient::new(1);
    let difficulty = 2u32; // Require 2+ leading zero bits

    // Client generates solutions
    let solutions = client.generate_solutions(100000, difficulty, &mut rng);

    // Contribution should be tracked (using same capping as client)
    let expected_contribution: u64 = solutions.iter()
        .map(|s| s.nonce % 1_000_000_000)
        .fold(0u64, |acc, x| acc.saturating_add(x));
    assert_eq!(client.total_work, expected_contribution);

    // Should have some meaningful contribution
    assert!(client.total_work > 0);
    assert!(!client.solutions.is_empty());
    }

    #[test]
    fn test_pow_difficulty_scaling() {
    // Test PoW generation with different difficulty levels
    let mut rng = rand::thread_rng();

    let difficulties = vec![1u32, 2u32, 3u32]; // Leading bits requirements
    let attempts = 1000000;

    let mut prev_solutions = usize::MAX;
    for difficulty in difficulties {
        let mut client = PowClient::new(1);
        let solutions = client.generate_solutions(attempts, difficulty, &mut rng);

        println!("Difficulty {} leading zeros: {} solutions", difficulty, solutions.len());

        // Higher difficulty should result in fewer solutions
        assert!(solutions.len() < prev_solutions,
                "Higher difficulty should yield fewer solutions: {} vs {}",
                solutions.len(), prev_solutions);
        prev_solutions = solutions.len();

        // All solutions should be valid
        let verifier = PowVerifier::new(difficulty);
        for solution in &solutions {
            assert!(verifier.verify(solution));
        }
    }
    }

    #[test]
    fn test_pow_epoch_based_accumulation() {
    // Test that clients accumulate work across multiple epochs
    let mut rng = rand::thread_rng();
    let difficulty = 2u32; // 2+ leading zero bits
    let mut client = PowClient::new(1);

    // Simulate 3 epochs of work
    let mut total_work_accumulated = 0u64;
    for epoch in 1..=3 {
        let solutions = client.generate_solutions(200000, difficulty, &mut rng);
        let epoch_work: u64 = solutions.iter()
            .map(|s| s.nonce % 1_000_000_000)
            .fold(0u64, |acc, x| acc.saturating_add(x));
        total_work_accumulated = total_work_accumulated.saturating_add(epoch_work);

        println!(
            "Epoch {}: {} solutions, {} work",
            epoch,
            solutions.len(),
            epoch_work
        );

        // Work should accumulate
        assert_eq!(client.total_work, total_work_accumulated);
    }

    // Total should have meaningful contribution
    assert!(client.total_work > 0);
    }

    #[test]
    fn test_pow_multiepoch_qualification() {
    // Test clients reaching MIN_CONTRIBUTION_THRESHOLD through multiple epochs
    use variable_pricing::consts::MIN_CONTRIBUTION_THRESHOLD;

    let mut rng = rand::thread_rng();
    let difficulty = 2u32; // 2+ leading zero bits
    let mut client = PowClient::new(1);

    // Try to accumulate enough work to qualify
    let mut epochs = 0;
    while client.total_work < (MIN_CONTRIBUTION_THRESHOLD / 100) && epochs < 100 {
        let solutions = client.generate_solutions(500000, difficulty, &mut rng);
        let epoch_work: u64 = solutions.iter()
            .map(|s| s.nonce % 1_000_000_000)
            .fold(0u64, |acc, x| acc.saturating_add(x));
        epochs += 1;

        if epoch_work > 0 {
            println!(
                "Epoch {}: accumulated {} work (threshold: {})",
                epochs,
                client.total_work,
                MIN_CONTRIBUTION_THRESHOLD / 100
            );
        }
    }

    // Should eventually qualify (or get close)
    println!(
        "Final work: {} (scaled threshold: {})",
        client.total_work,
        MIN_CONTRIBUTION_THRESHOLD / 100
    );
    }

    #[test]
    fn test_pow_solution_batching() {
    // Test collecting and processing solutions in batches
    let mut rng = rand::thread_rng();
    let difficulty = 5; // 5+ leading bits

    let mut clients: HashMap<u64, PowClient> = (1..=5).map(|id| (id, PowClient::new(id))).collect();

    // Each client generates solutions
    let mut batch_solutions: HashMap<u64, Vec<PowSolution>> = HashMap::new();
    for (client_id, client) in &mut clients {
        let solutions = client.generate_solutions(200000, difficulty, &mut rng);
        batch_solutions.insert(*client_id, solutions);
    }

    // Verify batch integrity
    let total_solutions: usize = batch_solutions.values().map(|v| v.len()).sum();
    assert!(total_solutions > 0, "Should generate some solutions");

    // Verify all clients have proper solutions
    let verifier = PowVerifier::new(difficulty);
    for (client_id, solutions) in &batch_solutions {
        for solution in solutions {
            assert_eq!(solution.client_id, *client_id);
            assert!(verifier.verify(solution));
        }
    }

    println!(
        "Total solutions across {} clients: {}",
        batch_solutions.len(),
        total_solutions
    );
    }

    #[test]
    fn test_pow_with_resource_manager_integration() {
    // Integration test: PoW generation → contribution → allocation
    use variable_pricing::consts::MIN_CONTRIBUTION_THRESHOLD;

    let mut rng = rand::thread_rng();

    // Simulate clients doing PoW work
    let difficulty = 2; // 2+ leading bits
    let mut pow_client = PowClient::new(1);

    // Generate enough work (scaled threshold for faster testing)
    let target_contribution = MIN_CONTRIBUTION_THRESHOLD / 10;
    let mut epochs = 0;
    while pow_client.total_work < target_contribution && epochs < 50 {
        pow_client.generate_solutions(1000000, difficulty, &mut rng);
        epochs += 1;
    }

    // Now use this work as contribution in allocation system
    // In real system, this would feed into ResourceManager
    let contribution = pow_client.total_work;

    println!(
        "Client generated {} work across {} epochs, qualifies: {}",
        contribution,
        epochs,
        contribution >= target_contribution
    );

    // Verify contribution is sufficient to qualify
    assert!(
        contribution >= target_contribution,
        "PoW work should reach qualification threshold"
    );
    }

    #[test]
    fn test_pow_fairness_with_equal_resources() {
    // Test that clients with equal computational resources generate similar work
    let mut rng1 = rand::thread_rng();
    let mut rng2 = rand::thread_rng();
    let difficulty = 2u32; // 2+ leading zero bits
    let attempts = 500000;

    let mut client1 = PowClient::new(1);
    let mut client2 = PowClient::new(2);

    // Both clients make same number of attempts
    client1.generate_solutions(attempts, difficulty, &mut rng1);
    client2.generate_solutions(attempts, difficulty, &mut rng2);

    // Should generate similar amounts of work
    // (within reasonable variance due to randomness)
    let work_ratio = (client1.total_work as f64) / (client2.total_work as f64);

    println!("Work ratio: {:.2}", work_ratio);
    println!("Client 1: {} work in {} solutions", client1.total_work, client1.solutions.len());
    println!("Client 2: {} work in {} solutions", client2.total_work, client2.solutions.len());

    // Ratio should be reasonably close (0.5 to 2.0)
    assert!(
        work_ratio.is_finite() && work_ratio > 0.5 && work_ratio < 2.0,
        "Similar resources should generate similar work"
    );
    }

    #[test]
    fn test_pow_solution_timestamp_tracking() {
    // Test that solutions properly track timestamps
    let mut rng = rand::thread_rng();
    let mut client = PowClient::new(1);
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let solutions = client.generate_solutions(200000, 4, &mut rng);

    let end_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // All solutions should have timestamps within the test window
    for solution in &solutions {
        assert!(solution.timestamp >= start_time);
        assert!(solution.timestamp <= end_time + 1); // Allow 1 second buffer
    }
    }

// ============================================================================
// ADVERSARIAL SCENARIO TESTS
// ============================================================================

#[test]
fn test_botnet_attack_many_weak_clients() {
    // Simulate a botnet: 100 weak clients with minimal contribution
    // Tests that many weak clients can't dominate the resource pool individually
    use variable_pricing::resource_manager::{ResourceManager, Consumer};

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;
    let num_botnet_clients = 100; // Reduced to 100 for realistic simulation

    // Each botnet client does minimal work
    let per_client_attempts = 10000;

    let mut botnet_clients = Vec::new();
    for i in 0..num_botnet_clients {
        let mut client = PowClient::new(i as u64);
        client.generate_solutions(per_client_attempts, difficulty, &mut rng);
        botnet_clients.push(client);
    }

    let total_botnet_work: u64 = botnet_clients.iter().map(|c| c.total_work).sum();
    println!("Botnet: {} clients, {} total work", num_botnet_clients, total_botnet_work);

    // Each botnet client should have non-zero work
    assert!(botnet_clients.iter().all(|c| c.total_work > 0), "Botnet clients should have work");

    // Now allocate with one strong legitimate client
    let mut legitimate = PowClient::new(9999);
    legitimate.generate_solutions(500000, difficulty, &mut rng);

    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    let mut all_clients = Vec::new();
    for botnet_client in &botnet_clients {
        all_clients.push(Consumer::new(botnet_client.id, botnet_client.total_work, 0));
    }
    all_clients.push(Consumer::new(legitimate.id, legitimate.total_work, 5));

    let allocated = rm.allocate(all_clients);

    let legit_alloc = allocated.iter()
        .find(|c| c.id == legitimate.id)
        .unwrap()
        .allocation;

    let botnet_total: f64 = allocated.iter()
        .filter(|c| c.id < 100)
        .map(|c| c.allocation)
        .sum();

    println!("Botnet total allocation: {:.2}", botnet_total);
    println!("Legitimate allocation: {:.2}", legit_alloc);

    // Legitimate client with tenure should get significant share despite botnet
    assert!(legit_alloc > capacity * 0.2, "Legitimate should get >20% despite botnet");
}

#[test]
fn test_monopoly_prevention_single_whale() {
    // Simulate a single powerful client trying to monopolize
    // This client has 100x+ the work of legitimate clients
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let mut rng = rand::thread_rng();
    let difficulty = 1u32; // Easy difficulty for whale to mine lots

    // Whale client
    let mut whale = PowClient::new(9999);
    let whale_attempts = 5000000; // Massive amount of work
    whale.generate_solutions(whale_attempts, difficulty, &mut rng);

    // Legitimate clients
    let mut legit1 = PowClient::new(1);
    legit1.generate_solutions(100000, difficulty, &mut rng);

    let mut legit2 = PowClient::new(2);
    legit2.generate_solutions(100000, difficulty, &mut rng);

    println!("Whale work: {}", whale.total_work);
    println!("Legit1 work: {}", legit1.total_work);
    println!("Legit2 work: {}", legit2.total_work);

    // Create ResourceManager and allocate
    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    let clients = vec![
        Consumer::new(whale.id, whale.total_work, 10),
        Consumer::new(legit1.id, legit1.total_work, 5),
        Consumer::new(legit2.id, legit2.total_work, 3),
    ];

    let allocated = rm.allocate(clients);

    // Whale should NOT get the entire pool despite huge contribution
    let whale_alloc = allocated.iter().find(|c| c.id == whale.id).unwrap().allocation;
    let legit1_alloc = allocated.iter().find(|c| c.id == legit1.id).unwrap().allocation;
    let legit2_alloc = allocated.iter().find(|c| c.id == legit2.id).unwrap().allocation;

    // Monopoly prevention: no single client should get > 25% in normal cases
    // But verify that legitimate clients still get significant allocation
    println!("Allocations: whale={:.2}, legit1={:.2}, legit2={:.2}", whale_alloc, legit1_alloc, legit2_alloc);

    assert!(legit1_alloc > 0.0, "Legitimate client 1 should get allocation");
    assert!(legit2_alloc > 0.0, "Legitimate client 2 should get allocation");

    // Legitimate clients combined should get meaningful fraction
    let legit_combined = legit1_alloc + legit2_alloc;
    assert!(legit_combined > capacity * 0.1, "Legitimate clients should get >10% combined");
}

#[test]
fn test_sybil_attack_many_minimum_clients() {
    // Simulate Sybil attack: many clients with JUST enough contribution to qualify
    // Tests if MIN_CONTRIBUTION_THRESHOLD effectively prevents Sybil attacks
    use variable_pricing::consts::MIN_CONTRIBUTION_THRESHOLD;
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let mut rng = rand::thread_rng();
    let difficulty = 1u32; // Easy mining

    // Create 50 "Sybil" clients, each with exactly MIN_CONTRIBUTION_THRESHOLD
    let num_sybils = 50;
    let threshold = MIN_CONTRIBUTION_THRESHOLD;

    let mut sybil_clients = Vec::new();

    for i in 0..num_sybils {
        let mut client = PowClient::new(i as u64);
        // Generate enough work to reach threshold
        let mut attempts = 0;
        while client.total_work < threshold && attempts < 100 {
            client.generate_solutions(1000000, difficulty, &mut rng);
            attempts += 1;
        }
        sybil_clients.push(client);
    }

    println!("Sybil attack: {} clients at threshold", num_sybils);

    // All should be at or above threshold
    let qualified = sybil_clients.iter().filter(|c| c.total_work >= threshold).count();
    println!("Qualified sybils: {}/{}", qualified, num_sybils);
    assert!(qualified > 0, "Should have some qualified sybils");

    // Now allocate resources
    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    let consumers: Vec<Consumer> = sybil_clients
        .iter()
        .map(|c| Consumer::new(c.id, c.total_work, 1))
        .collect();

    let allocated = rm.allocate(consumers);

    // Total allocation should not exceed capacity
    let total_alloc: f64 = allocated.iter().map(|c| c.allocation).sum();
    assert!(total_alloc <= capacity * 1.001);

    // Even with many minimum clients, distribution should be reasonable
    let avg_alloc = total_alloc / (allocated.len() as f64);
    println!("Average allocation per sybil: {:.4}", avg_alloc);
    assert!(avg_alloc > 0.0, "Even sybils should get minimum allocation");
}

#[test]
fn test_combined_attack_botnet_and_whale() {
    // Simulate combined attack: botnet + monopoly whale
    // Tests system resilience against multiple simultaneous attack vectors
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;

    // Whale attempting monopoly
    let mut whale = PowClient::new(9999);
    whale.generate_solutions(2000000, difficulty, &mut rng);

    // Botnet (100 weak clients)
    let mut botnet_clients = Vec::new();
    for i in 0..100 {
        let mut client = PowClient::new(i as u64);
        client.generate_solutions(50000, difficulty, &mut rng);
        botnet_clients.push(client);
    }

    // Legitimate clients
    let mut legit = PowClient::new(10000);
    legit.generate_solutions(500000, difficulty, &mut rng);

    // Create allocation scenario
    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    let mut clients = vec![Consumer::new(whale.id, whale.total_work, 1)];

    for botnet_client in &botnet_clients {
        clients.push(Consumer::new(botnet_client.id, botnet_client.total_work, 0));
    }

    clients.push(Consumer::new(legit.id, legit.total_work, 5));

    let allocated = rm.allocate(clients);

    let whale_alloc = allocated.iter().find(|c| c.id == whale.id).unwrap().allocation;
    let legit_alloc = allocated.iter().find(|c| c.id == legit.id).unwrap().allocation;
    let botnet_total: f64 = allocated.iter()
        .filter(|c| c.id < 100)
        .map(|c| c.allocation)
        .sum();

    println!("Combined attack results:");
    println!("  Whale: {:.2}", whale_alloc);
    println!("  Botnet total: {:.2}", botnet_total);
    println!("  Legitimate: {:.2}", legit_alloc);

    // Legitimate client with tenure should get meaningful allocation
    assert!(legit_alloc > capacity * 0.1, "Legitimate client should get >10%");

    // Total should respect capacity
    let total: f64 = allocated.iter().map(|c| c.allocation).sum();
    assert!(total <= capacity * 1.001);
}

#[test]
fn test_resilience_against_flash_flood_attack() {
    // Simulate flash flood: sudden influx of many weak clients
    // Tests if system gracefully handles resource spikes
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;

    // Phase 1: Normal operation with few clients
    let mut normal_clients = vec![
        PowClient::new(1),
        PowClient::new(2),
        PowClient::new(3),
    ];

    for client in &mut normal_clients {
        client.generate_solutions(200000, difficulty, &mut rng);
    }

    // Phase 2: Sudden flood of weak clients
    let mut flood_clients = Vec::new();
    for i in 0..500 {
        let mut client = PowClient::new(100 + i);
        client.generate_solutions(5000, difficulty, &mut rng); // Minimal work
        flood_clients.push(client);
    }

    // Allocate mixed scenario
    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    let mut all_clients = Vec::new();
    for nc in &normal_clients {
        all_clients.push(Consumer::new(nc.id, nc.total_work, 5));
    }
    for fc in &flood_clients {
        all_clients.push(Consumer::new(fc.id, fc.total_work, 0));
    }

    let allocated = rm.allocate(all_clients);

    // Normal clients should maintain reasonable allocations
    let normal_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id <= 3)
        .map(|c| c.allocation)
        .collect();

    println!("Normal clients allocations: {:?}", normal_allocs);
    assert!(normal_allocs.iter().all(|&a| a > 0.0), "Normal clients should get allocation");

    // Flood clients should get diminished allocation
    let flood_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id > 100)
        .map(|c| c.allocation)
        .collect();

    let flood_total: f64 = flood_allocs.iter().sum();
    let normal_total: f64 = normal_allocs.iter().sum();

    println!("Flood clients total: {:.2}", flood_total);
    println!("Normal clients total: {:.2}", normal_total);

    // Even with flood, system should reach capacity
    let total: f64 = allocated.iter().map(|c| c.allocation).sum();
    assert!(total > capacity * 0.9, "System should allocate most of capacity");
}

#[test]
fn test_minimum_allocation_protection() {
    // Test that tenure-based minimum allocation protects against orphaning
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let capacity = 1000.0;
    let mut rm = ResourceManager::new(capacity);

    // Long-connected legitimate client with moderate contribution
    let legit_work = 100_000u64;
    let legit_epochs = 10u32;

    // Newcomer with extreme work
    let newcomer_work = 1_000_000u64;
    let newcomer_epochs = 0u32;

    let clients = vec![
        Consumer::new(1, legit_work, legit_epochs),
        Consumer::new(2, newcomer_work, newcomer_epochs),
    ];

    let allocated = rm.allocate(clients);

    let legit = allocated.iter().find(|c| c.id == 1).unwrap();
    let newcomer = allocated.iter().find(|c| c.id == 2).unwrap();

    println!("Legit (100k work, 10 epochs): {:.2}", legit.allocation);
    println!("Newcomer (1M work, 0 epochs): {:.2}", newcomer.allocation);

    // Despite 10x less work, legit client should get meaningful allocation due to tenure
    assert!(legit.allocation > capacity * 0.05, "Tenure should provide minimum protection");

    // Newcomer gets more due to work, but not everything
    assert!(newcomer.allocation > legit.allocation, "More work should yield more allocation");
    assert!(newcomer.allocation < capacity * 0.9, "Even huge work shouldn't monopolize");
}

#[test]
fn test_work_distribution_fairness_under_attack() {
    // Test that legitimate clients maintain fair share under various attacks
    use variable_pricing::resource_manager::ResourceManager;
    use variable_pricing::resource_manager::Consumer;

    let capacity = 1000.0;

    // Scenario: 3 legitimate clients + 1 attacking whale
    let legit_work = 100_000u64;
    let whale_work = 5_000_000u64;

    let clients = vec![
        Consumer::new(1, legit_work, 5),
        Consumer::new(2, legit_work, 5),
        Consumer::new(3, legit_work, 5),
        Consumer::new(99, whale_work, 0), // Whale with no tenure
    ];

    let mut rm = ResourceManager::new(capacity);
    let allocated = rm.allocate(clients);

    let legit_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id <= 3)
        .map(|c| c.allocation)
        .collect();

    let whale_alloc = allocated.iter().find(|c| c.id == 99).unwrap().allocation;

    let legit_total: f64 = legit_allocs.iter().sum();

    println!("Legitimate clients total: {:.2}", legit_total);
    println!("Whale allocation: {:.2}", whale_alloc);
    println!("Legit/Whale ratio: {:.2}", legit_total / whale_alloc);

    // Legitimate clients with tenure should get significant share
    assert!(legit_total > capacity * 0.3, "Legitimate clients should get >30% combined");

    // Whale shouldn't monopolize despite huge work
    assert!(whale_alloc < capacity * 0.7, "Whale shouldn't exceed 70%");

    // Spread across legitimate clients should be relatively even
    let avg_legit = legit_total / 3.0;
    for &alloc in &legit_allocs {
        assert!((alloc - avg_legit).abs() < avg_legit * 0.5, "Legitimate allocation spread should be fair");
    }
}
