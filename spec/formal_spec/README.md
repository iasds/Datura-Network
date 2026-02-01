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
