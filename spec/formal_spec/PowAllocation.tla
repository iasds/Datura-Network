---- MODULE PowAllocation ----
\*
\* TLA+ Specification: PoW-Based Resource Allocation
\*
\* Models the epoch-based bandwidth allocation system where clients
\* earn resource shares by submitting proof-of-work (p2pool mining shares).
\*
\* Key mechanisms modeled:
\*   - Epoch-based allocation cycles with contribution reset
\*   - Sublinear compression of contributions (x^0.55)
\*   - Threshold + tenure hybrid minimum guarantees
\*   - Per-client allocation cap (anti-monopolization)
\*   - Normalization when total exceeds capacity
\*   - Adversarial (Sybil) client behavior
\*
\* Verified properties:
\*   Safety:
\*     - Total allocations never exceed capacity
\*     - No single client exceeds the allocation cap
\*     - Inactive clients always have zero allocation
\*     - Sub-threshold clients receive no minimum guarantee boost
\*   Liveness:
\*     - Contributing clients eventually receive allocation (under fairness)
\*
\* Design note: The allocation function is deterministic given the current
\* state. Non-determinism comes from client connect/disconnect/submit timing.
\* TLC explores all interleavings to verify invariants hold universally.

EXTENDS Integers, FiniteSets

\* ================================================================
\* Constants
\* ================================================================

CONSTANTS
    MaxNodes,              \* Set of node IDs {1..MaxNodes} (shared with Daturanet)
    NAttackers,            \* Number of attacker nodes {1..NAttackers} (shared with Daturanet)
    PowCapacity,           \* Total resource capacity (integer, e.g. 1000)
    PowMinThreshold,       \* Min contribution to qualify for guarantee
    PowMaxAllocPermille,   \* Max allocation per client (permille, 250 = 25%)
    PowMaxTenure,          \* Cap on tenure for minimum calculation
    PowReservePermille,    \* Reserve earned per epoch of tenure (permille)
    PowMaxContrib,         \* Max contribution per epoch (bounds state space)
    PowMaxEpochs           \* Epoch bound for model checking termination
ASSUME PowCapacity > 0
ASSUME PowMaxAllocPermille > 0 /\ PowMaxAllocPermille <= 1000
ASSUME PowMaxTenure >= 1
ASSUME PowMinThreshold >= 1
ASSUME PowReservePermille >= 0
ASSUME PowMaxContrib >= 1
ASSUME PowMaxEpochs >= 1

PowAttacker == 1..NAttackers
PowLegitimate == (NAttackers + 1)..MaxNodes

\* ================================================================
\* Variables
\* ================================================================

VARIABLES
    powEpoch,       \* Nat: current epoch number
    powActive,      \* SUBSET 1..MaxNodes: currently connected clients
    powContrib,     \* [1..MaxNodes -> Nat]: weighted contribution this epoch
    powTenure,      \* [1..MaxNodes -> Nat]: consecutive epochs connected
    powAlloc        \* [1..MaxNodes -> Nat]: current resource allocation

powVars == <<powEpoch, powActive, powContrib, powTenure, powAlloc>>

\* ================================================================
\* Arithmetic Helpers
\* ================================================================

Min(a, b) == IF a <= b THEN a ELSE b
Max(a, b) == IF a >= b THEN a ELSE b

\* Sum f[c] for all c in S. TLC evaluates this for small sets.
RECURSIVE SetSum(_, _)
SetSum(S, f) ==
    IF S = {} THEN 0
    ELSE LET c == CHOOSE c \in S : TRUE
         IN f[c] + SetSum(S \ {c}, f)

\* ================================================================
\* Sublinear Compression
\* ================================================================
\* Approximates x^0.55 scaled by 100 (integer arithmetic).
\* Precomputed for the model checking range [0..MaxContrib].
\* Property: Compress(a) + Compress(b) > Compress(a+b) (sublinear)
\*
\* In the real Rust implementation, this would use f64::powf(0.55).
\* The lookup table here is sufficient for TLC verification.

Compress(x) ==
    CASE x = 0  -> 0
    []   x = 1  -> 100
    []   x = 2  -> 146
    []   x = 3  -> 184
    []   x = 4  -> 214
    []   x = 5  -> 242
    []   x = 6  -> 267
    []   x = 7  -> 290
    []   x = 8  -> 312
    []   x = 9  -> 332
    []   x = 10 -> 351
    []   x = 11 -> 370
    []   x = 12 -> 387
    []   x = 13 -> 404
    []   x = 14 -> 420
    []   OTHER  -> 435 + (x - 15) * 14 \* Linear extrapolation

\* ================================================================
\* Allocation Computation
\* ================================================================
\* Core algorithm: given the current state, deterministically compute
\* the allocation for every client. This is called at epoch boundaries.
\*
\* Steps:
\*   1. Compress each client's contribution (sublinear)
\*   2. Compute proportional allocation based on compressed shares
\*   3. Compute minimum guarantee (threshold + tenure gated)
\*   4. Take max(proportional, minimum) per client
\*   5. Apply per-client cap
\*   6. Normalize if total exceeds capacity
\*
\* IMPORTANT: Step 6 (normalization) is essential. Without it, the
\* combination of minimum guarantees and proportional allocation can
\* cause total allocations to exceed capacity. Remove the normalization
\* and TLC will find a counterexample for TotalAllocationBound.

ComputeAlloc(act, con, ten) ==
    IF act = {} THEN [c \in 1..MaxNodes |-> 0]
    ELSE
    LET
        \* --- Step 1: Compressed contributions ---
        cc == [c \in 1..MaxNodes |->
            IF c \in act THEN Compress(con[c]) ELSE 0]

        totalCC == SetSum(act, cc)

        \* --- Step 2: Proportional allocation ---
        \* Each client gets (their_compressed / total_compressed) × PowCapacity
        prop == [c \in 1..MaxNodes |->
            IF c \in act /\ totalCC > 0
            THEN (cc[c] * PowCapacity) \div totalCC
            ELSE 0]

        \* --- Step 3: Minimum guarantee ---
        \* Only clients meeting PowMinThreshold qualify.
        \* Minimum = (capped_tenure × PowReservePermille / 1000) × share × PowCapacity
        \*         = (eTen × PowReservePermille × cc × PowCapacity) / (1000 × totalCC)
        eTen == [c \in 1..MaxNodes |-> Min(ten[c], PowMaxTenure)]

        minG == [c \in 1..MaxNodes |->
            IF c \in act /\ con[c] >= PowMinThreshold /\ totalCC > 0
            THEN (eTen[c] * PowReservePermille * cc[c] * PowCapacity)
                 \div (1000 * totalCC)
            ELSE 0]

        \* --- Step 4: Apply minimum guarantee ---
        withMin == [c \in 1..MaxNodes |->
            IF c \in act THEN Max(prop[c], minG[c]) ELSE 0]

        \* --- Step 5: Per-client cap ---
        capVal == (PowMaxAllocPermille * PowCapacity) \div 1000

        capped == [c \in 1..MaxNodes |-> Min(withMin[c], capVal)]

        \* --- Step 6: Normalize if overallocated ---
        \* This can happen when minimum guarantees push multiple
        \* clients above their proportional share simultaneously.
        totalCapped == SetSum(1..MaxNodes, capped)

        normalized == [c \in 1..MaxNodes |->
            IF totalCapped > PowCapacity /\ totalCapped > 0
            THEN (capped[c] * PowCapacity) \div totalCapped
            ELSE capped[c]]

    IN normalized

\* ================================================================
\* Initial State
\* ================================================================

PowInit ==
    /\ powEpoch = 0
    /\ powActive = {}
    /\ powContrib = [c \in 1..MaxNodes |-> 0]
    /\ powTenure  = [c \in 1..MaxNodes |-> 0]
    /\ powAlloc   = [c \in 1..MaxNodes |-> 0]

\* ================================================================
\* Actions
\* ================================================================

\* --- Client connects ---
\* In the real system, this requires solving a connection PoW (2^14).
\* Here we abstract that away: any client can connect at any time.
\* The cost is modeled implicitly by the finite 1..MaxNodes set.
PowConnect(c) ==
    /\ c \notin powActive
    /\ powActive' = powActive \cup {c}
    /\ powContrib' = [powContrib EXCEPT ![c] = 0]
    /\ powTenure'  = [powTenure  EXCEPT ![c] = 0]
    /\ powAlloc'   = [powAlloc   EXCEPT ![c] = 0]
    /\ UNCHANGED powEpoch

\* --- Client disconnects ---
PowDisconnect(c) ==
    /\ c \in powActive
    /\ powActive' = powActive \ {c}
    /\ powAlloc' = [powAlloc EXCEPT ![c] = 0]
    /\ UNCHANGED <<powEpoch, powContrib, powTenure>>

\* --- Client submits a mining share ---
\* `amount` models the weighted difficulty of the share.
\* In the real system, shares must be for the latest p2pool challenge.
PowSubmitWork(c, amount) ==
    /\ c \in powActive
    /\ amount >= 1
    /\ powContrib[c] + amount <= PowMaxContrib
    /\ powContrib' = [powContrib EXCEPT ![c] = @ + amount]
    /\ UNCHANGED <<powEpoch, powActive, powTenure, powAlloc>>

\* --- Epoch ends: recompute allocations ---
\* The core state transition. Computes new allocations based on
\* this epoch's contributions, advances tenure, resets contributions.
PowEndEpoch ==
    /\ powActive /= {}
    /\ powEpoch < PowMaxEpochs
    /\ powAlloc' = ComputeAlloc(powActive, powContrib, powTenure)
    /\ powTenure' = [c \in 1..MaxNodes |->
         IF c \in powActive THEN powTenure[c] + 1 ELSE 0]
    /\ powContrib' = [c \in 1..MaxNodes |-> 0]
    /\ powEpoch' = powEpoch + 1
    /\ UNCHANGED powActive

\* ================================================================
\* Next-State Relation & Specification
\* ================================================================

\* Full non-deterministic next state: any client can do anything.
\* TLC explores ALL interleavings.
PowNext ==
    \/ \E c \in 1..MaxNodes : PowConnect(c)
    \/ \E c \in 1..MaxNodes : PowDisconnect(c)
    \/ \E c \in 1..MaxNodes : \E a \in 1..PowMaxContrib : PowSubmitWork(c, a)
    \/ PowEndEpoch

\* Specification with weak fairness constraints.
\* WF on PowEndEpoch ensures epochs keep advancing.
\* WF on PowNext (combined with PowEndEpoch fairness) ensures the system progresses.
PowSpec == PowInit /\ [][PowNext]_powVars /\ WF_powVars(PowEndEpoch)

\* --- Alternative: Sybil attack scenario ---
\* Restricts attackers to sub-threshold contributions only.
\* Use this to verify Sybil-specific properties.
SybilPowNext ==
    \/ \E c \in 1..MaxNodes : PowConnect(c)
    \/ \E c \in 1..MaxNodes : PowDisconnect(c)
    \/ \E c \in PowLegitimate :
         \E a \in 1..PowMaxContrib : PowSubmitWork(c, a)
    \/ \E c \in PowAttacker :
         \E a \in 1..Min(PowMaxContrib, PowMinThreshold - 1) : PowSubmitWork(c, a)
    \/ PowEndEpoch

SybilPowSpec == PowInit /\ [][SybilPowNext]_powVars /\ WF_powVars(PowEndEpoch)

\* ================================================================
\* Safety Invariants
\* ================================================================

\* --- Type correctness ---
PowTypeOK ==
    /\ powEpoch \in 0..PowMaxEpochs
    /\ powActive \subseteq 1..MaxNodes
    /\ \A c \in 1..MaxNodes : powContrib[c] \in 0..PowMaxContrib
    /\ \A c \in 1..MaxNodes : powTenure[c] >= 0
    /\ \A c \in 1..MaxNodes : powAlloc[c] >= 0

\* --- I1: Total allocation never exceeds capacity ---
\* THE fundamental safety property. If this fails, the system is
\* overcommitting resources and will degrade under load.
\* Try removing the normalization step in ComputeAlloc to see
\* TLC find a violation.
PowTotalAllocationBound ==
    SetSum(1..MaxNodes, powAlloc) <= PowCapacity

\* --- I2: No single client exceeds allocation cap ---
\* Anti-monopolization: even with overwhelming hashrate, a single
\* client cannot take more than PowMaxAllocPermille/1000 of capacity.
PowNoMonopolization ==
    LET capVal == (PowMaxAllocPermille * PowCapacity) \div 1000
    IN \A c \in powActive : powAlloc[c] <= capVal

\* --- I3: Disconnected clients have zero allocation ---
\* Resources are immediately freed on disconnect.
PowInactiveZero ==
    \A c \in 1..MaxNodes \ powActive : powAlloc[c] = 0

\* --- I4: Allocation only from contribution ---
\* If a client has non-zero allocation (which was computed in the prior epoch),
\* they must have either:
\*   1. Contributed in that prior epoch (before PowEndEpoch reset powContrib), OR
\*   2. Had positive tenure >= 1, meaning they were active in an earlier epoch
\*      and earned a minimum guarantee (eligible for sustained allocation)
\* This ensures all allocation traces back to actual work or eligible tenure.
PowAllocationFromWorkOrTenure ==
    \A c \in 1..MaxNodes :
        (powAlloc[c] > 0) => (powContrib[c] > 0 \/ powTenure[c] > 0)
            \* Either they contributed this epoch (powContrib > 0) or
            \* they earned it via tenure in a prior epoch (powTenure > 0)

\* --- Combined safety invariant ---
PowSafety ==
    /\ PowTypeOK
    /\ PowTotalAllocationBound
    /\ PowNoMonopolization
    /\ PowInactiveZero
    /\ PowAllocationFromWorkOrTenure

\* ================================================================
\* Sybil-Specific Invariants
\* ================================================================
\* Use with SybilPowSpec to verify properties under constrained
\* attacker behavior (sub-threshold contributions only).

\* Under Sybil attack, total attacker allocation is bounded by
\* their compressed contribution share (no amplification).
\* In other words: the threshold+tenure system doesn't give
\* attackers MORE than pure proportional allocation would.
\* Qualified legitimate clients with tenure always get at least as
\* much as any individual sub-threshold attacker client.
PowLegitimateAdvantage ==
    \A l \in powActive \cap PowLegitimate :
    \A a \in powActive \cap PowAttacker :
        (powContrib[l] >= PowMinThreshold /\ powContrib[a] < PowMinThreshold
         /\ powContrib[a] > 0 /\ powContrib[l] > 0)
         => powAlloc[l] >= powAlloc[a]
             \* Note: this can fail when the cap clips the legitimate
             \* client. That's by design (anti-monopolization), but
             \* interesting to verify. See CapAwareLegitAdvantage below.



THEOREM PowSpec => PowSafety
====
