----- MODULE CircuitBuild -----

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS TotalNodes, CircuitLen, Empty, MaxCircuits

VARIABLES hs_intro_points, circuits, bridges

vars == << hs_intro_points, circuits, bridges>>

ASSUME MaxCircuits >= 1

Nodes == 1..TotalNodes


HSipTypeOK == \/ hs_intro_points = Empty
              \/ /\ hs_intro_points \in [Nodes -> Nodes]
                 /\ \A n \in DOMAIN hs_intro_points: hs_intro_points[n] # n
CircuitsTypeOK == \/ circuits = Empty
                  \/ /\ Cardinality(circuits) <= MaxCircuits
                     /\ \A c \in circuits:
                        /\ c \in Seq(Nodes)
                        /\ Len(c) = CircuitLen
                        /\ Cardinality({c[i]: i \in DOMAIN c}) = CircuitLen
BridgesTypeOK == \/ bridges = Empty
                 \/ bridges \in [ circuits -> circuits ]
                   /\ \A b \in DOMAIN bridges: bridges[b] # b


Init == /\ circuits = Empty
        /\ bridges = Empty
        /\ hs_intro_points = Empty

\* Helper to convert set to sorted sequence
SetToSortedSeq(S) ==
  LET F[s \in SUBSET S] ==
    IF s = {} THEN <<>>
    ELSE LET x == CHOOSE x \in s: \A y \in s: x <= y
         IN <<x>> \o F[s \ {x}]
  IN F[S]

\* Build circuit with deterministic ordering: src -> sorted intermediaries -> dst
BuildCircuit(src, dst) ==
  /\ src # dst
  /\ src \in Nodes
  /\ dst \in Nodes
  /\ \E intermediary \in Nodes \ {src, dst}:
     LET circuit == <<src, intermediary, dst>>
     IN /\ circuits' = IF circuits = Empty
                       THEN {circuit}
                       ELSE circuits \cup {circuit}
  /\ UNCHANGED << hs_intro_points, bridges>>

CanAddCircuit == IF circuits = Empty
                 THEN TRUE
                 ELSE Cardinality(circuits) < MaxCircuits

AddCircuit == /\ CanAddCircuit
              /\ \E src, dst \in Nodes:
                 /\ src # dst
                 /\ IF circuits # Empty
                    THEN ~\E c \in circuits: {c[1], c[CircuitLen]} = {src, dst}
                    ELSE TRUE
                 /\ BuildCircuit(src, dst)

\* Allow termination when max circuits reached
Terminated == /\ circuits # Empty
              /\ Cardinality(circuits) = MaxCircuits
              /\ UNCHANGED vars

Next == AddCircuit \/ Terminated

Spec == Init /\ [][Next]_vars

===============================
