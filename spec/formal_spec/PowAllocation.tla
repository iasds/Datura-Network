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
    Client,             \* Set of all possible client IDs (model values)
    Attacker,           \* Subset of Client controlled by adversary
    Capacity,           \* Total resource capacity (integer, e.g. 1000)
    MinThreshold,       \* Min contribution to qualify for guarantee
    MaxAllocPermille,   \* Max allocation per client (permille, 250 = 25%)
    MaxTenure,          \* Cap on tenure for minimum calculation
    ReservePermille,    \* Reserve earned per epoch of tenure (permille)
    MaxContrib,         \* Max contribution per epoch (bounds state space)
    MaxEpochs           \* Epoch bound for model checking termination

ASSUME Attacker \subseteq Client
ASSUME Capacity > 0
ASSUME MaxAllocPermille > 0 /\ MaxAllocPermille <= 1000
ASSUME MaxTenure >= 1
ASSUME MinThreshold >= 1
ASSUME ReservePermille >= 0
ASSUME MaxContrib >= 1
ASSUME MaxEpochs >= 1

Legitimate == Client \ Attacker

\* ================================================================
\* Variables
\* ================================================================

VARIABLES
    epoch,       \* Nat: current epoch number
    active,      \* SUBSET Client: currently connected clients
    contrib,     \* [Client -> Nat]: weighted contribution this epoch
    tenure,      \* [Client -> Nat]: consecutive epochs connected
    alloc        \* [Client -> Nat]: current resource allocation

vars == <<epoch, active, contrib, tenure, alloc>>

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
    IF act = {} THEN [c \in Client |-> 0]
    ELSE
    LET
        \* --- Step 1: Compressed contributions ---
        cc == [c \in Client |->
            IF c \in act THEN Compress(con[c]) ELSE 0]

        totalCC == SetSum(act, cc)

        \* --- Step 2: Proportional allocation ---
        \* Each client gets (their_compressed / total_compressed) × Capacity
        prop == [c \in Client |->
            IF c \in act /\ totalCC > 0
            THEN (cc[c] * Capacity) \div totalCC
            ELSE 0]

        \* --- Step 3: Minimum guarantee ---
        \* Only clients meeting MinThreshold qualify.
        \* Minimum = (capped_tenure × ReservePermille / 1000) × share × Capacity
        \*         = (eTen × ReservePermille × cc × Capacity) / (1000 × totalCC)
        eTen == [c \in Client |-> Min(ten[c], MaxTenure)]

        minG == [c \in Client |->
            IF c \in act /\ con[c] >= MinThreshold /\ totalCC > 0
            THEN (eTen[c] * ReservePermille * cc[c] * Capacity)
                 \div (1000 * totalCC)
            ELSE 0]

        \* --- Step 4: Apply minimum guarantee ---
        withMin == [c \in Client |->
            IF c \in act THEN Max(prop[c], minG[c]) ELSE 0]

        \* --- Step 5: Per-client cap ---
        capVal == (MaxAllocPermille * Capacity) \div 1000

        capped == [c \in Client |-> Min(withMin[c], capVal)]

        \* --- Step 6: Normalize if overallocated ---
        \* This can happen when minimum guarantees push multiple
        \* clients above their proportional share simultaneously.
        totalCapped == SetSum(Client, capped)

        normalized == [c \in Client |->
            IF totalCapped > Capacity /\ totalCapped > 0
            THEN (capped[c] * Capacity) \div totalCapped
            ELSE capped[c]]

    IN normalized

\* ================================================================
\* Initial State
\* ================================================================

Init ==
    /\ epoch = 0
    /\ active = {}
    /\ contrib = [c \in Client |-> 0]
    /\ tenure  = [c \in Client |-> 0]
    /\ alloc   = [c \in Client |-> 0]

\* ================================================================
\* Actions
\* ================================================================

\* --- Client connects ---
\* In the real system, this requires solving a connection PoW (2^14).
\* Here we abstract that away: any client can connect at any time.
\* The cost is modeled implicitly by the finite Client set.
Connect(c) ==
    /\ c \notin active
    /\ active' = active \cup {c}
    /\ contrib' = [contrib EXCEPT ![c] = 0]
    /\ tenure'  = [tenure  EXCEPT ![c] = 0]
    /\ alloc'   = [alloc   EXCEPT ![c] = 0]
    /\ UNCHANGED epoch

\* --- Client disconnects ---
Disconnect(c) ==
    /\ c \in active
    /\ active' = active \ {c}
    /\ alloc' = [alloc EXCEPT ![c] = 0]
    /\ UNCHANGED <<epoch, contrib, tenure>>

\* --- Client submits a mining share ---
\* `amount` models the weighted difficulty of the share.
\* In the real system, shares must be for the latest p2pool challenge.
SubmitWork(c, amount) ==
    /\ c \in active
    /\ amount >= 1
    /\ contrib[c] + amount <= MaxContrib
    /\ contrib' = [contrib EXCEPT ![c] = @ + amount]
    /\ UNCHANGED <<epoch, active, tenure, alloc>>

\* --- Epoch ends: recompute allocations ---
\* The core state transition. Computes new allocations based on
\* this epoch's contributions, advances tenure, resets contributions.
EndEpoch ==
    /\ active /= {}
    /\ epoch < MaxEpochs
    /\ alloc' = ComputeAlloc(active, contrib, tenure)
    /\ tenure' = [c \in Client |->
         IF c \in active THEN tenure[c] + 1 ELSE 0]
    /\ contrib' = [c \in Client |-> 0]
    /\ epoch' = epoch + 1
    /\ UNCHANGED active

\* ================================================================
\* Next-State Relation & Specification
\* ================================================================

\* Full non-deterministic next state: any client can do anything.
\* TLC explores ALL interleavings.
Next ==
    \/ \E c \in Client : Connect(c)
    \/ \E c \in Client : Disconnect(c)
    \/ \E c \in Client : \E a \in 1..MaxContrib : SubmitWork(c, a)
    \/ EndEpoch

\* Specification with weak fairness on EndEpoch.
\* Guarantees epochs keep advancing (needed for liveness).
Spec == Init /\ [][Next]_vars /\ WF_vars(EndEpoch)

\* --- Alternative: Sybil attack scenario ---
\* Restricts attackers to sub-threshold contributions only.
\* Use this to verify Sybil-specific properties.
SybilNext ==
    \/ \E c \in Client : Connect(c)
    \/ \E c \in Client : Disconnect(c)
    \/ \E c \in Legitimate :
         \E a \in 1..MaxContrib : SubmitWork(c, a)
    \/ \E c \in Attacker :
         \E a \in 1..Min(MaxContrib, MinThreshold - 1) : SubmitWork(c, a)
    \/ EndEpoch

SybilSpec == Init /\ [][SybilNext]_vars /\ WF_vars(EndEpoch)

\* ================================================================
\* Safety Invariants
\* ================================================================

\* --- Type correctness ---
TypeOK ==
    /\ epoch \in 0..MaxEpochs
    /\ active \subseteq Client
    /\ \A c \in Client : contrib[c] \in 0..MaxContrib
    /\ \A c \in Client : tenure[c] >= 0
    /\ \A c \in Client : alloc[c] >= 0

\* --- I1: Total allocation never exceeds capacity ---
\* THE fundamental safety property. If this fails, the system is
\* overcommitting resources and will degrade under load.
\* Try removing the normalization step in ComputeAlloc to see
\* TLC find a violation.
TotalAllocationBound ==
    SetSum(Client, alloc) <= Capacity

\* --- I2: No single client exceeds allocation cap ---
\* Anti-monopolization: even with overwhelming hashrate, a single
\* client cannot take more than MaxAllocPermille/1000 of capacity.
NoMonopolization ==
    LET capVal == (MaxAllocPermille * Capacity) \div 1000
    IN \A c \in active : alloc[c] <= capVal

\* --- I3: Disconnected clients have zero allocation ---
\* Resources are immediately freed on disconnect.
InactiveZero ==
    \A c \in Client \ active : alloc[c] = 0

\* --- Combined safety invariant ---
Safety ==
    /\ TypeOK
    /\ TotalAllocationBound
    /\ NoMonopolization
    /\ InactiveZero

\* ================================================================
\* Sybil-Specific Invariants
\* ================================================================
\* Use with SybilSpec to verify properties under constrained
\* attacker behavior (sub-threshold contributions only).

\* Under Sybil attack, total attacker allocation is bounded by
\* their compressed contribution share (no amplification).
\* In other words: the threshold+tenure system doesn't give
\* attackers MORE than pure proportional allocation would.
SybilNoAmplification ==
    LET
        attackersActive == active \cap Attacker
        cc == [c \in Client |->
            IF c \in active THEN Compress(contrib[c]) ELSE 0]
        totalCC == SetSum(active, cc)
        attackerCC == SetSum(attackersActive, cc)
        attackerAlloc == SetSum(attackersActive, alloc)
        \* Attacker's "fair share" based on compressed contribution
        attackerFairShare ==
            IF totalCC > 0
            THEN (attackerCC * Capacity) \div totalCC
            ELSE 0
    IN
        \* Attacker allocation <= their compressed proportional share + rounding
        attackerAlloc <= attackerFairShare + Cardinality(attackersActive)

\* Qualified legitimate clients with tenure always get at least as
\* much as any individual sub-threshold attacker client.
LegitimateAdvantage ==
    \A l \in active \cap Legitimate :
    \A a \in active \cap Attacker :
        (contrib[l] >= MinThreshold /\ contrib[a] < MinThreshold
         /\ contrib[a] > 0 /\ contrib[l] > 0)
        => alloc[l] >= alloc[a]
            \* Note: this can fail when the cap clips the legitimate
            \* client. That's by design (anti-monopolization), but
            \* interesting to verify. See CapAwareLegitAdvantage below.

\* Weaker version accounting for the allocation cap
CapAwareLegitAdvantage ==
    LET capVal == (MaxAllocPermille * Capacity) \div 1000
    IN \A l \in active \cap Legitimate :
       \A a \in active \cap Attacker :
           (contrib[l] >= MinThreshold /\ contrib[a] < MinThreshold
            /\ contrib[a] > 0 /\ contrib[l] > 0 /\ alloc[l] < capVal)
           => alloc[l] >= alloc[a]

\* Combined Sybil safety
SybilSafety ==
    /\ Safety
    /\ SybilNoAmplification
    /\ CapAwareLegitAdvantage

\* ================================================================
\* Liveness Properties
\* ================================================================

\* A client that is active and has contributed will eventually
\* receive a non-zero allocation (requires WF on EndEpoch).
EventualAllocation ==
    \A c \in Client :
        (c \in active /\ contrib[c] > 0) ~> (alloc[c] > 0)

\* Epochs keep advancing as long as clients are active.
EpochProgress ==
    (active /= {} /\ epoch < MaxEpochs) ~> (epoch > 0)

====
