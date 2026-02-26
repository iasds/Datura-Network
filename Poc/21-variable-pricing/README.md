This crate proposes an approach to pow-based resource management (issue 111, poc21).

# Main Idea

When allocating resources (e.g., bandwidth, memory), one opens oneself to starvation-based attacks
where malicious peers would seek to grab as much as possible in order to deny those resources
to legitimate users.

## Fundamental Barrier

To tackle this behaviour, the datura net project opted for a PoW mechanism. This has the added
benefit of creating economic incentives to run nodes. All interactions are gated behind PoW,
and the higher the difficulty, the more resources a client can obtain.

## Attack Vectors

The system must resist three types of attacks:

1. **Weak Device Disadvantage**: Legitimate users with low-hashpower devices would be unfairly
   disadvantaged if only raw hashing power is considered.
2. **Monopoly Attack**: A determined attacker with extreme hashrate could dominate resource allocation.
3. **Botnet/Sybil Attack**: Many weak devices coordinating can collectively try to starve legitimate
   users through quantity rather than individual power.

## Defense Mechanisms

This crate implements a multi-layered approach to counter all three attack vectors:

### 1. Minimum Contribution Threshold (Primary Botnet Defense)

Only clients contributing at least `MIN_CONTRIBUTION_THRESHOLD` (50,000 units) qualify for
resource allocation guarantees. This is the **primary defense against Sybil/botnet attacks**.

- **Unqualified clients** (below threshold):
  - Receive zero minimum guarantee
  - Can only compete for leftover capacity via earned allocation
  - Results in negligible allocation despite coordinating in large numbers
  - Example: 100 unqualified botnet clients allocate only 1-2% of capacity

- **Qualified clients** (at or above threshold):
  - Become eligible for tenure-based minimum guarantees
  - Compete for both minimum reserves AND earned allocation

### 2. Tenure-Based Minimum Allocation

Clients with longer connection history receive protection against displacement, solving the
weak device problem while still requiring a baseline contribution level.

**Minimum allocation formula:**
```text
minimum = (tenure_epochs * 0.015) * sqrt(contribution) * reserve_scale
```

Where:
- `tenure_epochs`: Connection duration (capped at 10 epochs)
- `contribution`: Proof-of-work supplied by the client
- `sqrt()`: Sublinear compression (exponent 0.5)
- `reserve_scale`: Scales all minimums to fit 15% reserve capacity

**Effect**:
- New client with 50,000 contribution (no tenure): minimal guarantee
- Established client with 50,000 contribution (10 epochs): 15% tenure bonus baseline
- Weak devices can maintain fair allocation over time despite low hashpower, **provided** they
  meet the qualification threshold
- Prevents high-power newcomers from immediately starving long-term users

### 3. Sublinear Compression (Monopoly Prevention)

All contributions are normalized with an exponent < 1 (default 0.5) before proportional distribution,
creating diminishing returns on additional hashpower.

**Effect of sqrt(contribution)**:
- 1x hashpower → 1x allocation (baseline)
- 10x hashpower → ~3.16x allocation (not 10x)
- 100x hashpower → ~10x allocation (not 100x)

This applies to both:
- **Earned allocation**: `(sqrt(contribution) / total_compressed) * distributable_capacity`
- **Minimum allocation**: `tenure_rate * sqrt(contribution)`

Combined with the lack of tenure, a new high-power attacker is severely limited.

### 4. Reserve Capacity Capping

Total minimum guarantees cannot exceed 15% of capacity. This ensures:
- Qualified clients get meaningful baseline protection
- Earned allocations still determine most distribution
- No single group (e.g., many established clients) can monopolize minimums

## Allocation Algorithm Summary

**Two-tier distribution**:

1. **Reserve Phase** (applies to qualified clients only):
   - Calculate tenure-weighted reserves: `tenure_epochs * 0.015 * sqrt(contribution)`
   - Sum all reserves, then scale so total <= 15% of capacity
   - Assign each qualified client their scaled minimum

2. **Earned Phase** (applies to all clients):
   - Calculate remaining capacity (capacity - reserves)
   - Distribute proportionally by compressed contribution: `sqrt(contribution)`
   - All clients (qualified and unqualified) share earned allocation

3. **Final Allocation**:
   - Each client gets: `minimum + earned`
   - Qualified clients benefit from both; unqualified get only earned share

## Attack Resilience

The layered defense ensures:

- **Botnet (100 unqualified weak clients)**: <2% of capacity (no minimum reserve, tiny contribution)
- **Monopoly (10x hashpower newcomer)**: ~30% of capacity (no tenure, compression limits dominance)
- **Combined attack (whale + botnet)**: Legitimate users maintain >50% (qualification + tenure protection)
- **Scaling resistance**: Adding more unqualified clients has minimal impact on qualified users

## Implementation

The `SecureAllocationSystem` struct in the [`resource_manager`] module implements this algorithm via
the `calculate_allocation` method.

# Usage

## building the doc

```
cargo doc
```

## running the PoC

```
cargo test
```
