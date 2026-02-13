----- MODULE CircuitBuild -----

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS TotalNodes, CircuitLen, Empty

VARIABLES hs_intro_points, circuits, bridges

vars == << hs_intro_points, circuits, bridges>>

Nodes == 1..TotalNodes


HSipTypeOK == \/ hs_intro_points = Empty
              \/ /\ hs_intro_points \in [Nodes -> Nodes]
                 /\ \A n \in DOMAIN hs_intro_points: hs_intro_points[n] # n
CircuitsTypeOK == \/ circuits = Empty
                  \/ /\ \A c \in circuits:
                        /\ c \in Seq(Nodes)
                        /\ Len(c) = CircuitLen
                        /\ Cardinality({c[i]: i \in DOMAIN c}) = CircuitLen
BridgesTypeOK == \/ bridges = Empty
                 \/ bridges \in [ circuits -> circuits ]
                   /\ \A b \in DOMAIN bridges: bridges[b] # b


Init == /\ circuits = Empty
        /\ bridges = Empty
        /\ hs_intro_points = Empty

SeqFromSet(S, n) ==
  {f \in [1..n -> S]: Cardinality({f[i]: i \in DOMAIN f}) = n}

BuildCircuit(src,dst) == /\ src # dst
                         /\ src \in Nodes
                         /\ dst \in Nodes
                         /\ \E intermediaries \in SUBSET Nodes:
                            /\ Cardinality(intermediaries) = CircuitLen - 2
                            /\ src \notin intermediaries
                            /\ dst \notin intermediaries
                            /\ \E circuit \in SeqFromSet({src, dst} \cup intermediaries, CircuitLen):
                              /\ circuit[1] = src
                              /\ circuit[CircuitLen] = dst
                              /\ circuits' = IF circuits = Empty THEN {circuit} ELSE circuits \cup {circuit}
                         /\ UNCHANGED << hs_intro_points, bridges>>

AddCircuit == \E src, dst \in Nodes:
              /\ src # dst
              /\ IF circuits # Empty
                 THEN ~\E c \in circuits: {c[1], c[CircuitLen]} = {src, dst}
                 ELSE TRUE
              /\ BuildCircuit(src, dst)

Next == AddCircuit

Spec == Init /\ [][Next]_vars

===============================
