mod common;
use common::*;

/// Simulates a server environment with mixed legitimate users and attackers
/// Tests the system's resilience against:
/// 1. One very powerful attacker (high hashing power monopoly attack)
/// 2. Many weak attackers (coordinated min-allocation attack - below qualification threshold)
/// 3. Normal users trying to maintain fair access
#[test]
fn test_server_with_mixed_users_and_attackers() {
    use variable_pricing::resource_manager::{ResourceManager, Consumer};
    use variable_pricing::consts::MIN_CONTRIBUTION_THRESHOLD;

    let mut rng = rand::thread_rng();
    let difficulty = 2u32; // 2+ leading zero bits
    let capacity = 1000.0;

    // ========================================================================
    // SETUP PHASE: Create different types of clients
    // ========================================================================

    // Legitimate normal users - 5 typical clients with moderate work (well above threshold)
    let mut normal_users = Vec::new();
    for i in 1..=5 {
        let mut client = PowClient::new(i as u64);
        client.generate_solutions(500000, difficulty, &mut rng);
        normal_users.push(client);
    }

    // Very powerful attacker - extremely high hashing power (multiple times normal users)
    let mut whale_attacker = PowClient::new(9999);
    whale_attacker.generate_solutions(5000000, difficulty, &mut rng); // Massive work

    // Coordinated weak attackers - 100 botnet clients BELOW qualification threshold
    // Each does minimal work (below MIN_CONTRIBUTION_THRESHOLD) trying to multiply through quantity
    let threshold = MIN_CONTRIBUTION_THRESHOLD;
    let mut weak_attackers = Vec::new();
    for i in 0..100 {
        let mut client = PowClient::new(1000 + i as u64);
        // Generate extremely minimal work - just 2 attempts ensures each stays well below 50k threshold
        client.generate_solutions(2, difficulty, &mut rng); // Minimal per bot
        weak_attackers.push(client);
    }

    // ========================================================================
    // ANALYSIS PHASE: Print work distribution
    // ========================================================================

    let normal_total: u64 = normal_users.iter().map(|c| c.total_work).sum();
    let weak_total: u64 = weak_attackers.iter().map(|c| c.total_work).sum();

    println!("\n=== Server Attack Simulation ===");
    println!("\nNormal Users (5 clients):");
    println!("  Total work: {}", normal_total);
    println!("  Average per user: {}", normal_total / 5);
    println!("  All qualify? {}", normal_total / 5 > threshold);

    println!("\nPowerful Attacker (whale):");
    println!("  Work: {}", whale_attacker.total_work);
    println!("  Qualifies: {}", whale_attacker.total_work > threshold);
    println!("  Ratio to normal user avg: {:.2}x",
        whale_attacker.total_work as f64 / ((normal_total / 5) as f64)
    );

    println!("\nWeak Attackers Botnet (100 clients):");
    println!("  Total work: {}", weak_total);
    println!("  Average per bot: {}", weak_total / 100);
    println!("  Each qualifies? {}", (weak_total / 100) > threshold);
    println!("  Threshold: {}", threshold);

    // ========================================================================
    // ALLOCATION PHASE: Allocate resources
    // ========================================================================

    let mut rm = ResourceManager::new(capacity);
    let mut all_clients = Vec::new();

    // Add normal users with tenure (simulating established users)
    for user in &normal_users {
        all_clients.push(Consumer::new(user.id, user.total_work, 5));
    }

    // Add powerful attacker with no tenure
    all_clients.push(Consumer::new(
        whale_attacker.id,
        whale_attacker.total_work,
        0,
    ));

    // Add weak attackers with no tenure (all unqualified)
    for attacker in &weak_attackers {
        all_clients.push(Consumer::new(attacker.id, attacker.total_work, 0));
    }

    let allocated = rm.allocate(all_clients);

    // ========================================================================
    // ANALYSIS PHASE: Results
    // ========================================================================

    // Extract allocations by category
    let normal_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id >= 1 && c.id <= 5)
        .map(|c| c.allocation)
        .collect();

    let whale_alloc = allocated
        .iter()
        .find(|c| c.id == 9999)
        .unwrap()
        .allocation;

    let weak_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id >= 1000 && c.id < 1100)
        .map(|c| c.allocation)
        .collect();

    let normal_total_alloc: f64 = normal_allocs.iter().sum();
    let weak_total_alloc: f64 = weak_allocs.iter().sum();
    let total_alloc: f64 = allocated.iter().map(|c| c.allocation).sum();

    println!("\n=== Allocation Results ===");
    println!("\nNormal Users:");
    println!("  Total allocation: {:.2} / {}", normal_total_alloc, capacity);
    println!("  Percentage: {:.1}%", (normal_total_alloc / capacity) * 100.0);
    println!("  Average per user: {:.2}", normal_total_alloc / 5.0);

    println!("\nPowerful Attacker (whale):");
    println!("  Allocation: {:.2}", whale_alloc);
    println!("  Percentage: {:.1}%", (whale_alloc / capacity) * 100.0);

    println!("\nWeak Attackers (botnet, unqualified):");
    println!("  Total allocation: {:.2}", weak_total_alloc);
    println!("  Percentage: {:.1}%", (weak_total_alloc / capacity) * 100.0);
    println!("  Average per bot: {:.6}", weak_total_alloc / 100.0);

    println!("\nTotal Allocated: {:.2} / {}", total_alloc, capacity);

    // ========================================================================
    // ASSERTIONS: Verify system resilience
    // ========================================================================

    // 1. Capacity should not be exceeded
    assert!(
        total_alloc <= capacity * 1.001,
        "Total allocation should not exceed capacity"
    );

    // 2. Normal users with tenure should get substantial allocation
    // They qualify and have tenure, so they benefit from both reserve and earned shares
    assert!(
        normal_total_alloc > capacity * 0.30,
        "Normal users with tenure should get 30%+ of capacity, got {:.1}%",
        (normal_total_alloc / capacity) * 100.0
    );

    // 3. Powerful attacker should NOT monopolize
    // Even with 10x+ the work, compression and lack of tenure limits them
    assert!(
        whale_alloc < capacity * 0.50,
        "Whale attacker should not exceed 50% of capacity, got {:.1}%",
        (whale_alloc / capacity) * 100.0
    );

    // 4. Weak attackers (unqualified) should get minimal allocation
    // They don't qualify for minimum guarantee, only earn from their tiny contribution
    assert!(
        weak_total_alloc < capacity * 0.15,
        "Unqualified weak attackers should get <15% of capacity, got {:.1}%",
        (weak_total_alloc / capacity) * 100.0
    );

    // 5. Each weak attacker gets effectively nothing
    let avg_weak = weak_total_alloc / 100.0;
    assert!(
        avg_weak < 0.5,
        "Average weak attacker should get <0.5 allocation, got {:.6}",
        avg_weak
    );

    // 6. Normal users should fare dramatically better per capita than weak attackers
    let avg_normal = normal_total_alloc / 5.0;
    assert!(
        avg_normal > avg_weak * 50.0,
        "Normal users should get 50x+ more than weak attackers"
    );

    println!("\n=== Test Passed ===");
    println!("System resilience verified:");
    println!("  ✓ Capacity limit maintained");
    println!("  ✓ Qualified users with tenure protected (30%+)");
    println!("  ✓ Powerful attacker limited (<50%)");
    println!("  ✓ Unqualified weak attackers severely limited (<15%)");
    println!("  ✓ Per-user allocation heavily favors legitimate users (50x+)");
}

/// Tests the effectiveness of tenure protection specifically
/// Verifies that long-tenure users resist displacement by attackers
#[test]
fn test_tenure_protection_against_attackers() {
    use variable_pricing::resource_manager::{ResourceManager, Consumer};

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;
    let capacity = 1000.0;

    // Established user with moderate work but high tenure
    let mut established = PowClient::new(1);
    established.generate_solutions(200000, difficulty, &mut rng);

    // New attacker with extreme work but no tenure
    let mut attacker = PowClient::new(2);
    attacker.generate_solutions(2000000, difficulty, &mut rng); // 10x the work

    // Allocate with established user having tenure advantage
    let mut rm = ResourceManager::new(capacity);
    let clients = vec![
        Consumer::new(1, established.total_work, 10), // 10 epochs tenure
        Consumer::new(2, attacker.total_work, 0),    // No tenure
    ];

    let allocated = rm.allocate(clients);

    let established_alloc = allocated.iter().find(|c| c.id == 1).unwrap().allocation;
    let attacker_alloc = allocated.iter().find(|c| c.id == 2).unwrap().allocation;

    println!("\nTenure Protection Test:");
    println!("Established user (200k work, 10 epochs): {:.2}", established_alloc);
    println!("Attacker (2M work, 0 epochs): {:.2}", attacker_alloc);
    println!("Ratio: {:.2}%", (established_alloc / attacker_alloc) * 100.0);

    // Despite 10x less work, established user should maintain decent allocation
    assert!(
        established_alloc > capacity * 0.1,
        "Tenure should protect established user"
    );

    // Attacker gets more due to work, but tenure prevents monopoly
    assert!(
        attacker_alloc > established_alloc,
        "More work should yield more allocation"
    );
    assert!(
        attacker_alloc < capacity * 0.7,
        "Even 10x work shouldn't dominate with tenure protection"
    );
}

/// Tests how the system scales with increasing number of weak attackers
/// Verifies that weak unqualified attackers have minimal impact on legitimate users
#[test]
fn test_botnet_scaling_resistance() {
    use variable_pricing::resource_manager::{ResourceManager, Consumer};

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;
    let capacity = 1000.0;

    // Normal user with tenure and good contribution
    let mut normal = PowClient::new(9999);
    normal.generate_solutions(500000, difficulty, &mut rng);

    // Test with different botnet sizes (all unqualified)
    let botnet_sizes = vec![10, 50, 100, 200];
    let mut results = Vec::new();

    for botnet_size in botnet_sizes {
        let mut weak_attackers = Vec::new();
        for i in 0..botnet_size {
            let mut client = PowClient::new(i as u64);
            client.generate_solutions(2, difficulty, &mut rng); // Minimal work, unqualified
            weak_attackers.push(client);
        }

        let mut rm = ResourceManager::new(capacity);
        let mut clients = vec![Consumer::new(9999, normal.total_work, 5)];

        for attacker in &weak_attackers {
            clients.push(Consumer::new(attacker.id, attacker.total_work, 0));
        }

        let allocated = rm.allocate(clients);
        let normal_alloc = allocated
            .iter()
            .find(|c| c.id == 9999)
            .unwrap()
            .allocation;

        results.push((botnet_size, normal_alloc));
    }

    println!("\nBotnet Scaling Resistance (unqualified attackers):");
    for (size, alloc) in &results {
        println!("  Botnet size {}: normal user gets {:.2}", size, alloc);
    }

    // Normal user should maintain strong allocation regardless of unqualified botnet size
    for (_, alloc) in &results {
        assert!(
            *alloc > capacity * 0.6,
            "Normal user with tenure should maintain >60% allocation against unqualified botnets"
        );
    }

    // Allocation should be relatively stable - unqualified attackers have minimal impact
    let allocations: Vec<f64> = results.iter().map(|(_, a)| *a).collect();
    let avg = allocations.iter().sum::<f64>() / allocations.len() as f64;
    let max_deviation = allocations
        .iter()
        .map(|a| (a - avg).abs())
        .fold(0.0, f64::max);

    assert!(
        max_deviation < avg * 0.15,
        "Allocation should be stable - unqualified botnet size shouldn't matter much. Max deviation: {:.2}, avg: {:.2}",
        max_deviation, avg
    );
}

/// Tests the combined scenario of powerful monopoly attacker + coordinated weak attackers
#[test]
fn test_combined_whale_plus_botnet_attack() {
    use variable_pricing::resource_manager::{ResourceManager, Consumer};

    let mut rng = rand::thread_rng();
    let difficulty = 2u32;
    let capacity = 1000.0;

    // Normal users with tenure
    let mut users = Vec::new();
    for i in 1..=3 {
        let mut client = PowClient::new(i as u64);
        client.generate_solutions(400000, difficulty, &mut rng);
        users.push(client);
    }

    // Whale attacker - extreme work but no tenure
    let mut whale = PowClient::new(9999);
    whale.generate_solutions(5000000, difficulty, &mut rng);

    // Botnet - 100 weak UNQUALIFIED clients (each below threshold)
    let mut botnet = Vec::new();
    for i in 0..100 {
        let mut client = PowClient::new(1000 + i as u64);
        client.generate_solutions(2, difficulty, &mut rng); // Minimal work, unqualified
        botnet.push(client);
    }

    // Allocate
    let mut rm = ResourceManager::new(capacity);
    let mut clients = Vec::new();

    for user in &users {
        clients.push(Consumer::new(user.id, user.total_work, 3));
    }

    clients.push(Consumer::new(9999, whale.total_work, 0));

    for attacker in &botnet {
        clients.push(Consumer::new(attacker.id, attacker.total_work, 0));
    }

    let allocated = rm.allocate(clients);

    // Analyze results
    let user_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id >= 1 && c.id <= 3)
        .map(|c| c.allocation)
        .collect();

    let whale_alloc = allocated.iter().find(|c| c.id == 9999).unwrap().allocation;

    let botnet_allocs: Vec<f64> = allocated
        .iter()
        .filter(|c| c.id >= 1000 && c.id < 1100)
        .map(|c| c.allocation)
        .collect();

    let user_total: f64 = user_allocs.iter().sum();
    let botnet_total: f64 = botnet_allocs.iter().sum();

    println!("\nCombined Whale + Unqualified Botnet Attack:");
    println!("Normal users (qualified, tenure): {:.2} ({:.1}%)", user_total, (user_total / capacity) * 100.0);
    println!("Whale (qualified, no tenure): {:.2} ({:.1}%)", whale_alloc, (whale_alloc / capacity) * 100.0);
    println!("Botnet (unqualified): {:.2} ({:.1}%)", botnet_total, (botnet_total / capacity) * 100.0);

    // Verify system resilience
    let total: f64 = allocated.iter().map(|c| c.allocation).sum();
    assert!(total <= capacity * 1.001, "Capacity exceeded");

    // Users with tenure and qualification should maintain good allocation
    assert!(
        user_total > capacity * 0.25,
        "Qualified users with tenure should get >25% against combined attacks, got {:.1}%",
        (user_total / capacity) * 100.0
    );

    // Whale limited by compression and lack of tenure
    assert!(
        whale_alloc < capacity * 0.55,
        "Whale should be limited to <55%, got {:.1}%",
        (whale_alloc / capacity) * 100.0
    );

    // Unqualified botnet severely limited
    assert!(
        botnet_total < capacity * 0.15,
        "Unqualified botnet should get <15%, got {:.1}%",
        (botnet_total / capacity) * 100.0
    );

    println!("\n✓ System resists combined attack vectors:");
    println!("  Legitimate users (tenure + qualification): {:.1}%", (user_total / capacity) * 100.0);
    println!("  Powerful attacker (quality over quantity): {:.1}%", (whale_alloc / capacity) * 100.0);
    println!("  Weak attackers (unqualified): {:.1}%", (botnet_total / capacity) * 100.0);
}
