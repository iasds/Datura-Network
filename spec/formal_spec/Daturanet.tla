---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets,Sequences

CONSTANTS MaxNodes, NBSNodes, Hops, NDecoys, Empty,MaxAllocPermille
ASSUME MaxNodes > 0
ASSUME NBSNodes < MaxNodes

VARIABLES known_nodes, circuits, allocations

vars == <<known_nodes, circuits, allocations>>


InvKnownNodesOK == /\ known_nodes \in [1..MaxNodes -> SUBSET 1..MaxNodes]
                   /\ \A n \in 1..MaxNodes: n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in 1..MaxNodes: \E bn \in 1..NBSNodes: bn /= n /\ bn \in known_nodes[n]

InvAllocationsOK == /\ allocations \in [ 1..MaxNodes -> [ 1..MaxNodes -> 1..MaxAllocPermille ]]
                    /\ \A n \in DOMAIN allocations: n \notin allocations[n]

InvCircuitsOK == /\ circuits \in [1..MaxNodes -> SUBSET Seq(1..MaxNodes)]
                /\ \A n \in DOMAIN circuits:
                    \A c \in circuits[n]:
                     /\ Len(c) = Hops
                     /\ n \notin c

Init == /\ known_nodes = [n \in 1..MaxNodes |-> {CHOOSE bn \in 1..NBSNodes: bn /= n}]
        /\ allocations = [n \in 1..MaxNodes |-> {}]
        /\ circuits = [n \in 1..MaxNodes |-> {}]

learn_nodes(n, peer) == /\ n/= peer
            /\ known_nodes' = [
                   known_nodes EXCEPT 
                   ![n] = known_nodes[n] \cup {peer} \cup (known_nodes[peer] \ {n})
               ]

Next == \E n \in 1..MaxNodes:
          \E peer \in 1..MaxNodes:
            /\ n /= peer
            /\ \/ learn_nodes(n,peer)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

EventuallyLearnNewNodes == <>\A n \in 1..MaxNodes: 
                                Cardinality(known_nodes[n]) > 1
EventuallyLearnNonBootStrap == <>\A n \in 1..MaxNodes:
                                \E nb \in 1..MaxNodes \ 1..NBSNodes: 
                                    nb \in known_nodes[n]

Properties == EventuallyLearnNewNodes /\ EventuallyLearnNonBootStrap

=========================================
