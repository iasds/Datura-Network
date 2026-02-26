# Formal Specifications

TLA+ specifications for verifying core protocol properties.

## Specifications

### PowAllocation.tla

Models the epoch-based bandwidth allocation system where clients earn resource shares by submitting proof-of-work (p2pool mining shares).

**Key mechanisms:**
- Epoch-based allocation cycles with contribution reset
- Sublinear compression of contributions (x^0.55) - discourages Sybil attacks
- Threshold + tenure hybrid minimum guarantees
- Per-client allocation cap (anti-monopolization)
- Normalization when total exceeds capacity

**Verified invariants:**
- `TypeOK` - Type correctness for all variables
- `TotalAllocationBound` - Total allocations never exceed capacity
- `NoMonopolization` - No client exceeds 25% of capacity (configurable via `MaxAllocPermille`)
- `InactiveZero` - Disconnected clients have zero allocation

**Specifications:**
- `Spec` - Full non-deterministic behavior with weak fairness on epoch transitions
- `SybilSpec` - Constrains attackers to sub-threshold contributions only

### Daturanet.tla

Models the gossip protocol for node discovery.

**Verified invariants:**
- `InvTypeOK` - Type correctness
- `InvAlwaysKnowBootstrap` - Nodes always know bootstrap nodes

**Verified properties:**
- `EventuallyLearnNewNodes` - Nodes eventually discover new peers
- `EventuallyLearnNonBootStrap` - Nodes eventually learn about non-bootstrap nodes

## Running the Model Checker

Requires TLC (part of the TLA+ toolbox). On NixOS: `nix-shell -p tlaplus`

### Basic usage

```bash
tlc PowAllocation.tla -config PowAllocation.cfg -workers auto
```

### Options

- `-workers auto` - Use all available CPU cores
- `-deadlock` - Don't treat deadlock as an error
- `-depth N` - Limit search depth

### Configuration

The `.cfg` files define model parameters. Key settings in `PowAllocation.cfg`:

| Constant | Description | Impact on state space |
|----------|-------------|----------------------|
| `Client` | Set of client IDs | Exponential |
| `MaxContrib` | Max contribution per epoch | High |
| `MaxTenure` | Max tenure epochs | Moderate |
| `MaxEpochs` | Epoch bound | Moderate |

Reduce these values if model checking takes too long. The default configuration checks ~75k states in ~4 seconds.

## Notes

**Sublinear compression:** The `Compress` function uses a lookup table scaled by 100 to approximate x^0.55 using integer arithmetic (TLA+ has no floats).

**Temporal invariants:** Some properties (`NoFreeRide`, `SubThresholdNoMinBoost`) are enforced by construction in `ComputeAlloc` rather than as runtime invariants. They would fail as invariants due to temporal mismatch - `contrib` resets at epoch end while `alloc` persists.


### Output

~~~
TLC2 Version 2.19 of 08 August 2024 (rev: 5a47802)
Running breadth-first search Model-Checking with fp 106 and seed -3292612459317719981 with 12 workers on 12 cores with 7099MB heap and 64MB offheap memory (Linux 6.12.67 amd64, Oracle Corporation 1.8.0_472 x86_64, MSBDiskFPSet, DiskStateQueue).
Parsing file /home/urist/Documents/datura_net/spec/formal_spec/PowAllocation.tla
Parsing file /tmp/Integers.tla
Parsing file /tmp/FiniteSets.tla
Parsing file /tmp/Naturals.tla
Parsing file /tmp/Sequences.tla
Semantic processing of module Naturals
Semantic processing of module Integers
Semantic processing of module Sequences
Semantic processing of module FiniteSets
Semantic processing of module PowAllocation
Starting... (2026-02-03 10:29:30)
Computing initial states...
Finished computing initial states: 1 distinct state generated at 2026-02-03 10:29:30.
Progress(13) at 2026-02-03 10:29:33: 310,293 states generated (310,293 s/min), 71,049 distinct states found (71,049 ds/min), 36,553 states left on queue.
Model checking completed. No error has been found.
  Estimates of the probability that TLC did not check all reachable states
  because two distinct states had the same fingerprint:
  calculated (optimistic):  val = 2.8E-7
  based on the actual fingerprints:  val = 5.8E-7
6545251 states generated, 901000 distinct states found, 0 states left on queue.
The depth of the complete state graph search is 19.
The average outdegree of the complete state graph is 1 (minimum is 0, the maximum 16 and the 95th percentile is 5).
Finished in 16s at (2026-02-03 10:29:46)
~~~


### Conclusion

Based on this analysis the current pow allocation algorithm as specified fulfills the requirements. The hashing algorithm itself could be swapped for another as long as general difficulty is scaled appropriately.
