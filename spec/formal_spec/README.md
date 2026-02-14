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

### CircuitBuild.tla

Models the circuit building protocol for anonymous communication, including hidden service connections via rendezvous nodes.

**Key mechanisms:**
- **3-hop circuits** - Fixed-length paths: `<<source, intermediary, destination>>`
- **Hidden service introduction points** - HSes select and maintain circuits to publicly-advertised entry nodes
- **Rendezvous-based connections** - Client and HS both connect to a common rendezvous node
- **Circuit bridging** - Links two circuits at rendezvous for bidirectional communication

**State variables:**
- `circuits` - All network circuits (sequences of nodes)
- `bridges` - Maps circuit → circuit at rendezvous nodes
- `hs_intro_points` - Each hidden service's introduction points
- `hs_to_intro_circuits` - Circuits from HSes to their intro points

**Actions:**
- `AddCircuit` - Generic circuit creation between any two nodes
- `SelectIntroPoint` - Hidden service selects intro point and creates circuit to it
- `ConnectHiddenService` - Full client-to-HS connection via rendezvous protocol
  1. Client connects to HS's introduction point
  2. Creates three circuits: client→intro, hs→rv, client→rv
  3. Bridges client_to_rv → hs_to_rv at rendezvous node
- `AddBridge` - Links two circuits at a rendezvous node

**Verified invariants:**
- `HSipTypeOK` - Hidden service intro points well-formed (≤ MaxIntroPoints, HS not its own intro)
- `HSToIntroCircuitsTypeOK` - HS-to-intro circuits valid (start at HS, end at intro, correct length)
- `CircuitsTypeOK` - All circuits valid (≤ MaxCircuits, unique nodes, length = CircuitLen)
- `BridgesTypeOK` - Bridge mappings valid (no self-bridges)
- `IntroPointHasCircuit` - Every intro point has a corresponding HS circuit
- `CircuitIntroConsistency` - Every HS-to-intro circuit has a registered intro point
- `HSNodesValid` - All hidden service nodes are valid network nodes

**Protocol modeled from specification:**
- Hidden Services (spec lines 165-287)
- Rendezvous Nodes (spec lines 229-241)
- Bidirectional Communication (spec lines 288-299)

**Abstractions:**
- Encryption assumed (not modeled)
- PoW challenges not included (separate concern)
- DHT routing simplified to direct node selection
- Decoy destinations not yet modeled

**Configuration** (`CircuitBuild.cfg`):
```
TotalNodes = 4          # Total network nodes
CircuitLen = 3          # Fixed at 3 hops
MaxCircuits = 2         # Circuit limit
HiddenServiceNodes = {1}# Which nodes are HSes
MaxIntroPoints = 2      # Max intros per HS
```

**State space:** ~20k states in ~2s with default config

## Running the Model Checker

Requires TLC (part of the TLA+ toolbox). On NixOS: `nix-shell -p tlaplus`

### Basic usage

```bash
# Check the PoW allocation spec
tlc PowAllocation.tla -config PowAllocation.cfg -workers auto

# Check the circuit building spec
tlc CircuitBuild.tla -config CircuitBuild.cfg -workers auto

# Check the gossip protocol spec
tlc Daturanet.tla -config Daturanet.cfg -workers auto
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
