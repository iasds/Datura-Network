----- MODULE CircuitBuild -----

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS TotalNodes, CircuitLen, Empty

VARIABLES hs_intro_points, circuits, bridges

vars == << hs_intro_points, circuits, bridges>>

Nodes == 1..TotalNodes
RoutingKeys == {"prev","next","id"}
RoutingValues == {Nodes, Nat}


HSipTypeOK == \/ hs_intro_points = Empty
              \/ hs_intro_points \in SUBSET [ Nodes -> Nodes]
               /\ \A n \in DOMAIN hs_intro_points: hs_intro_points[n] # n
CircuitsTypeOK == \/ circuits = Empty
                  \/ circuits \in SUBSET Seq(Nodes)
                    /\ \A c \in circuits:
                      Cardinality({c[i]: i \in DOMAIN c}) = CircuitLen
BridgesTypeOK == \/ bridges = Empty
                 \/ bridges \in [ circuits -> circuits ]
                   /\ \A b \in DOMAIN bridges: bridges[b] # b


Init == /\ circuits = Empty
        /\ bridges = Empty
        /\ hs_intro_points = Empty

BuildCircuit(src,dst) == /\ src # dst
                         /\ {src, dst} \in SUBSET Nodes
                         /\ CHOOSE intermediaries \in SUBSET Nodes:
                            /\ Cardinality(intermediaries) + 2 = CircuitLen
                            /\ CHOOSE circuit \in Seq({src, dst} \cup intermediaries):
                              /\ circuit[1] = src
                              /\ circuit[CircuitLen] = dst
                              /\ circuits' = circuits \cup {circuit}
                         /\ UNCHANGED << hs_intro_points, circuits, bridges>>

AddCircuit == CHOOSE endpoints \in SUBSET Nodes:
              /\ Cardinality(endpoints) = 2
              /\ IF circuits # Empty THEN
                /\ ~\E c \in circuits: c \in Seq(endpoints)
                /\ CHOOSE d \in Seq(endpoints):
                   BuildCircuit(d[1],d[2])
                ELSE
                UNCHANGED vars

Next == AddCircuit

Spec == Init /\ [][Next]_vars

===============================
