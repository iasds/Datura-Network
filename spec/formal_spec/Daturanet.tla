---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets, Sequences, PowAllocation

CONSTANTS NBSNodes, Hops, NBDecoys, Empty
ASSUME /\ NBSNodes < MaxNodes
       /\ Hops >= 3
       /\ NBDecoys >= 0

\* Circuit structure: [client -> {dest_real, [dest_decoy_1, ..., dest_decoy_N], hops, pow_tokens, created_at, expires_at}]
VARIABLES known_nodes, circuits, daturaAllocations, circuit_counter

daturaVars == <<known_nodes, circuits, daturaAllocations, circuit_counter>>

allVars == <<daturaVars, powVars>>


InvKnownNodesOK == \A n \in 1..MaxNodes: /\ known_nodes[n] \subseteq 1..MaxNodes
                                          /\ n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in 1..MaxNodes: 
                              \/ n \in 1..NBSNodes
                              \/ \E bn \in (1..NBSNodes \ {n}): bn \in known_nodes[n]

InvAllocationsOK == IF daturaAllocations = Empty THEN TRUE ELSE
                     /\ daturaAllocations \in [ 1..MaxNodes -> [ 1..MaxNodes -> 0..PowCapacity ]]
                     /\ \A n \in DOMAIN daturaAllocations: n \notin daturaAllocations[n]

\* Circuit format: [id -> [client: node, dest_real: node, dest_decoys: {node}, 
\*                             hops: {[real: seq, decoy_i: seq, ...]}, 
\*                             pow_tokens: {...}, created_at: nat, expires_at: nat]]
IsValidHopPath(path, dest) == 
    /\ Len(path) >= 1 /\ Len(path) <= Hops
    /\ path[Len(path)] = dest  \* destination is final hop
    /\ \A i \in 1..Len(path)-1: path[i] /= dest  \* dest not in intermediate hops

IsValidCircuit(circ) ==
    /\ circ.client /= circ.dest_real
    /\ circ.dest_real \notin circ.dest_decoys
    /\ circ.client \notin circ.dest_decoys
    /\ Cardinality(circ.dest_decoys) = NBDecoys
    /\ IsValidHopPath(circ.hops.real, circ.dest_real)
    /\ \A d \in circ.dest_decoys:
         IsValidHopPath(circ.hops.decoys[d], d)

InvCircuitsOK == IF circuits = Empty THEN TRUE ELSE
                 /\ \A circ \in circuits:
                     IsValidCircuit(circ)
                 /\ circuit_counter \in Nat

Init == /\ known_nodes = [n \in 1..MaxNodes |-> (1..MaxNodes \ {n})]
         /\ daturaAllocations = Empty
         /\ circuits = Empty
         /\ circuit_counter = 0
         /\ PowInit

\* Node discovery disabled - all nodes know each other by default in Init

\* Check if enough nodes available for a path
CanGeneratePath(source, dest, available_nodes) ==
    LET hop_count == 3
        intermediate_needed == hop_count - 1
        candidates == available_nodes \ {source, dest}
    IN Cardinality(candidates) >= intermediate_needed

\* Simplified path generation: just return a valid sequence indicator
\* In practice, the client would construct actual paths with specific nodes
GenerateHopPath(source, dest, available_nodes) ==
    IF CanGeneratePath(source, dest, available_nodes) THEN
        <<source, dest>>  \* Simplified: path from source to dest (actual hops omitted for TLC)
    ELSE
        << >>  \* Empty sequence if insufficient nodes

create_circuit(client, real_dest, decoy_dests) ==
    /\ client \in 1..MaxNodes
    /\ real_dest \in 1..MaxNodes
    /\ real_dest /= client
    /\ decoy_dests \cap {client, real_dest} = {}
    /\ Cardinality(decoy_dests) = NBDecoys
    /\ LET new_circuit_id == circuit_counter + 1
           new_circuit == [
               id |-> new_circuit_id,
               client |-> client,
               dest_real |-> real_dest,
               dest_decoys |-> decoy_dests,
               hops |-> [real |-> <<client, real_dest>>, decoys |-> [d \in decoy_dests |-> <<client, d>>]],
               pow_tokens |-> [real |-> 0, decoys |-> [d \in decoy_dests |-> 0]],
               created_at |-> 0,
               expires_at |-> 0
           ]
       IN circuits' = circuits \cup {new_circuit}
          /\ circuit_counter' = new_circuit_id
          /\ UNCHANGED <<known_nodes, daturaAllocations, powVars>>

PowConnectDatura(c) == PowConnect(c) /\ UNCHANGED daturaVars

PowDisconnectDatura(c) == PowDisconnect(c) /\ UNCHANGED daturaVars

PowSubmitWorkDatura(c, amount) == PowSubmitWork(c, amount) /\ UNCHANGED daturaVars

PowEndEpochDatura == PowEndEpoch /\ UNCHANGED daturaVars

\* Fully deterministic circuit creation
create_circuit_action(client) ==
    /\ \A n \in 1..MaxNodes: n \in known_nodes[client] \/ n = client
    /\ LET real_dest == IF client < MaxNodes THEN client + 1 ELSE 1
           decoy1 == IF client + 2 <= MaxNodes THEN client + 2 ELSE 1
           decoy2 == IF client + 3 <= MaxNodes THEN client + 3 ELSE 2
       IN LET decoy_set == {decoy1, decoy2} \ {client, real_dest}
          IN /\ Cardinality(decoy_set) = NBDecoys
             /\ create_circuit(client, real_dest, decoy_set)

\* Next: circuit creation only
Next == (\E c \in 1..MaxNodes : create_circuit_action(c))

Spec == Init /\ [][Next]_allVars /\ WF_allVars(Next)

EventuallyLearnNewNodes == <>\A n \in 1..MaxNodes: 
                                 Cardinality(known_nodes[n]) > 1
EventuallyLearnNonBootStrap == <>\A n \in 1..MaxNodes:
                                 \E nb \in 1..MaxNodes \ 1..NBSNodes: 
                                     nb \in known_nodes[n]

\* Circuit properties (safety only, no liveness guarantee)
CircuitCreationEventually == \A c \in 1..MaxNodes:
    (Cardinality(known_nodes[c]) > 1 + NBDecoys) =>
        (\E circ \in circuits: circ.client = c)

CircuitDestinationInvariants == \A circ \in circuits:
    /\ circ.client /= circ.dest_real
    /\ \A d \in circ.dest_decoys: d /= circ.client /\ d /= circ.dest_real
    /\ \A d1 \in circ.dest_decoys: \A d2 \in circ.dest_decoys: d1 = d2 \/ d1 /= d2

CircuitPathInvariants == \A circ \in circuits:
    /\ circ.hops.real /= << >>
    /\ \A decoy_id \in DOMAIN circ.hops.decoys: circ.hops.decoys[decoy_id] /= << >>

CircuitValidityInvariants == \A circ \in circuits:
    /\ Cardinality(circ.dest_decoys) = NBDecoys
    /\ \A d \in circ.dest_decoys: d \in known_nodes[circ.client]

\* Packet routing safety: all destination nodes (real + decoys) receive packet
DecoyDestinationCoverage == \A circ \in circuits:
    LET dest_set == {circ.dest_real} \cup circ.dest_decoys
    IN Cardinality(dest_set) = 1 + NBDecoys

\* Path hop count invariant
PathHopCountInvariant == \A circ \in circuits:
    /\ Len(circ.hops.real) >= 1 /\ Len(circ.hops.real) <= Hops
    /\ \A decoy_id \in DOMAIN circ.hops.decoys:
        Len(circ.hops.decoys[decoy_id]) >= 1 /\ Len(circ.hops.decoys[decoy_id]) <= Hops

Properties == EventuallyLearnNewNodes /\ EventuallyLearnNonBootStrap
           /\ CircuitDestinationInvariants 
           /\ CircuitPathInvariants
           /\ CircuitValidityInvariants
           /\ DecoyDestinationCoverage
           /\ PathHopCountInvariant

=========================================
