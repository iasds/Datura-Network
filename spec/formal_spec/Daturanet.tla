---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets, Sequences, PowAllocation

CONSTANTS NBSNodes, Hops, NDecoys, Empty
ASSUME NBSNodes < MaxNodes

VARIABLES known_nodes, circuits, daturaAllocations

daturaVars == <<known_nodes, circuits, daturaAllocations>>

allVars == <<daturaVars, powVars>>


InvKnownNodesOK == \A n \in 1..MaxNodes: /\ known_nodes[n] \subseteq 1..MaxNodes
                                          /\ n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in 1..MaxNodes: 
                              \/ n \in 1..NBSNodes
                              \/ \E bn \in (1..NBSNodes \ {n}): bn \in known_nodes[n]

InvAllocationsOK == IF daturaAllocations = Empty THEN TRUE ELSE
                     /\ daturaAllocations \in [ 1..MaxNodes -> [ 1..MaxNodes -> 0..PowCapacity ]]
                     /\ \A n \in DOMAIN daturaAllocations: n \notin daturaAllocations[n]

InvCircuitsOK == IF circuits = Empty THEN TRUE ELSE
                /\ circuits \in [1..MaxNodes -> SUBSET Seq(1..MaxNodes)]
                /\ \A n \in DOMAIN circuits:
                    \A c \in circuits[n]:
                     /\ Len(c) = Hops
                     /\ n \notin c

Init == /\ known_nodes = [n \in 1..MaxNodes |-> {CHOOSE bn \in 1..NBSNodes: bn /= n}]
        /\ daturaAllocations = Empty
        /\ circuits = Empty
        /\ PowInit

learn_nodes(n, peer) == /\ n/= peer
             /\ known_nodes' = [
                    known_nodes EXCEPT 
                    ![n] = known_nodes[n] \cup {peer} \cup (known_nodes[peer] \ {n})
                ]
             /\ UNCHANGED <<circuits, daturaAllocations, powVars>>

PowConnectDatura(c) == PowConnect(c) /\ UNCHANGED daturaVars

PowDisconnectDatura(c) == PowDisconnect(c) /\ UNCHANGED daturaVars

PowSubmitWorkDatura(c, amount) == PowSubmitWork(c, amount) /\ UNCHANGED daturaVars

PowEndEpochDatura == PowEndEpoch /\ UNCHANGED daturaVars

Next == \/ (\E n \in 1..MaxNodes:
             \E peer \in 1..MaxNodes:
               /\ n /= peer
               /\ learn_nodes(n,peer))
        \/ (\E c \in 1..MaxNodes : PowConnectDatura(c))
        \/ (\E c \in 1..MaxNodes : PowDisconnectDatura(c))
        \/ (\E c \in 1..MaxNodes : \E a \in 1..PowMaxContrib : PowSubmitWorkDatura(c, a))
        \/ PowEndEpochDatura

Spec == Init /\ [][Next]_allVars /\ WF_allVars(Next)

EventuallyLearnNewNodes == <>\A n \in 1..MaxNodes: 
                                Cardinality(known_nodes[n]) > 1
EventuallyLearnNonBootStrap == <>\A n \in 1..MaxNodes:
                                \E nb \in 1..MaxNodes \ 1..NBSNodes: 
                                    nb \in known_nodes[n]

Properties == EventuallyLearnNewNodes /\ EventuallyLearnNonBootStrap

=========================================
