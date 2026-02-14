----- MODULE CircuitBuild -----

EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS TotalNodes, CircuitLen, Empty, MaxCircuits, HiddenServiceNodes, MaxIntroPoints

VARIABLES hs_intro_points, circuits, bridges, hs_to_intro_circuits, next_br_cookie

vars == << hs_intro_points, circuits, bridges, hs_to_intro_circuits, next_br_cookie>>

ASSUME MaxCircuits >= 1
ASSUME HiddenServiceNodes \subseteq 1..TotalNodes
ASSUME MaxIntroPoints >= 1

Nodes == 1..TotalNodes


\* hs_intro_points: maps each hidden service node to its set of introduction points
HSipTypeOK == \/ hs_intro_points = Empty
              \/ /\ DOMAIN hs_intro_points \subseteq HiddenServiceNodes
                 /\ \A hs \in DOMAIN hs_intro_points:
                    /\ hs_intro_points[hs] \subseteq Nodes
                    /\ hs \notin hs_intro_points[hs]  \* HS cannot be its own intro point
                    /\ Cardinality(hs_intro_points[hs]) <= MaxIntroPoints

\* hs_to_intro_circuits: circuits from hidden service to each of its introduction points
\* Structure: set of records [hs: Node, intro: Node, circuit: Seq(Nodes)]
HSToIntroCircuitsTypeOK ==
    \/ hs_to_intro_circuits = Empty
    \/ \A c \in hs_to_intro_circuits:
        /\ c.hs \in HiddenServiceNodes
        /\ c.intro \in Nodes
        /\ c.hs # c.intro
        /\ c.circuit \in Seq(Nodes)
        /\ Len(c.circuit) = CircuitLen
        /\ c.circuit[1] = c.hs           \* Circuit starts at HS
        /\ c.circuit[CircuitLen] = c.intro  \* Circuit ends at intro point
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
        /\ hs_to_intro_circuits = Empty
        /\ next_br_cookie = 0

\* Helper to convert set to sorted sequence
SetToSortedSeq(S) ==
  LET F[s \in SUBSET S] ==
    IF s = {} THEN <<>>
    ELSE LET x == CHOOSE x \in s: \A y \in s: x <= y
         IN <<x>> \o F[s \ {x}]
  IN F[S]

\* Build circuit with deterministic ordering: src -> sorted intermediaries -> dst
\* br_cookie parameter is for tracking purposes but not stored in circuit
BuildCircuit(src, dst, br_cookie) ==
  /\ src # dst
  /\ src \in Nodes
  /\ dst \in Nodes
  /\ \E intermediary \in Nodes \ {src, dst}:
     LET circuit == <<src, intermediary, dst>>
     IN /\ circuits' = IF circuits = Empty
                       THEN {circuit}
                       ELSE circuits \cup {circuit}
  /\ UNCHANGED << hs_intro_points, bridges, hs_to_intro_circuits>>

\* ============================================================================
\* HIDDEN SERVICE ACTIONS
\* ============================================================================

\* Get current intro points for a hidden service (handles Empty case)
GetHSIntroPoints(hs) ==
    IF hs_intro_points = Empty THEN {}
    ELSE IF hs \in DOMAIN hs_intro_points THEN hs_intro_points[hs]
    ELSE {}

\* Check if a hidden service can add more introduction points
CanAddIntroPoint(hs) ==
    /\ hs \in HiddenServiceNodes
    /\ Cardinality(GetHSIntroPoints(hs)) < MaxIntroPoints

\* Hidden service selects an introduction point and creates a circuit to it
\* This models I2P-style hidden service setup where HS creates circuits to intro points
SelectIntroPoint(hs, intro_point) ==
    /\ CanAddIntroPoint(hs)
    /\ intro_point \in Nodes
    /\ intro_point # hs
    /\ intro_point \notin GetHSIntroPoints(hs)  \* Not already an intro point
    \* Create the circuit from HS to intro point through an intermediary
    /\ \E intermediary \in Nodes \ {hs, intro_point}:
        LET new_circuit == <<hs, intermediary, intro_point>>
            new_circuit_record == [hs |-> hs, intro |-> intro_point, circuit |-> new_circuit]
        IN /\ hs_to_intro_circuits' = IF hs_to_intro_circuits = Empty
                                       THEN {new_circuit_record}
                                       ELSE hs_to_intro_circuits \cup {new_circuit_record}
           /\ hs_intro_points' = IF hs_intro_points = Empty
                                  THEN [h \in {hs} |-> {intro_point}]
                                  ELSE IF hs \in DOMAIN hs_intro_points
                                       THEN [hs_intro_points EXCEPT ![hs] = @ \cup {intro_point}]
                                       ELSE hs_intro_points @@ (hs :> {intro_point})
    /\ UNCHANGED <<circuits, bridges, next_br_cookie>>

\* Action: any hidden service node can select a new introduction point
HSSelectIntroPoint ==
    \E hs \in HiddenServiceNodes:
    \E intro \in Nodes \ {hs}:
        SelectIntroPoint(hs, intro)

CanAddCircuit == IF circuits = Empty
                 THEN TRUE
                 ELSE Cardinality(circuits) < MaxCircuits

AddCircuit == /\ CanAddCircuit
              /\ \E src, dst \in Nodes:
                 /\ src # dst
                 /\ IF circuits # Empty
                    THEN ~\E c \in circuits: {c[1], c[CircuitLen]} = {src, dst}
                    ELSE TRUE
                 /\ BuildCircuit(src, dst, next_br_cookie)
                 /\ next_br_cookie' = next_br_cookie + 1

\* Add a bridge linking two circuits at a rendezvous node
\* This connects client circuit to HS circuit through the rendezvous node
AddBridge(c1, c2) ==
    /\ c1 \in circuits
    /\ c2 \in circuits
    /\ c1 # c2
    /\ bridges' = IF bridges = Empty
                  THEN c1 :> c2
                  ELSE bridges @@ (c1 :> c2)
    /\ UNCHANGED <<circuits, hs_intro_points, hs_to_intro_circuits, next_br_cookie>>

ConnectHiddenService == \E n \in Nodes:
                         \E hs \in HiddenServiceNodes:
                          \E i \in GetHSIntroPoints(hs):
                           \E intermediary1 \in Nodes \ {n, i}:
                            \E rv \in Nodes:
                             \E intermediary2 \in Nodes \ {hs, rv}:
                              \E intermediary3 \in Nodes \ {n, rv}:
                               LET client_to_intro == <<n, intermediary1, i>>
                                   hs_to_rv == <<hs, intermediary2, rv>>
                                   client_to_rv == <<n, intermediary3, rv>>
                               IN /\ rv # n
                                  /\ rv # hs
                                  /\ circuits' = IF circuits = Empty
                                                 THEN {client_to_intro, hs_to_rv, client_to_rv}
                                                 ELSE circuits \cup {client_to_intro, hs_to_rv, client_to_rv}
                                  /\ bridges' = IF bridges = Empty
                                                THEN client_to_rv :> hs_to_rv
                                                ELSE bridges @@ (client_to_rv :> hs_to_rv)
                                  /\ next_br_cookie' = next_br_cookie + 3
                                  /\ UNCHANGED <<hs_intro_points, hs_to_intro_circuits>>

\* Check if all hidden services have reached max intro points
AllHSHaveMaxIntroPoints ==
    \A hs \in HiddenServiceNodes:
        Cardinality(GetHSIntroPoints(hs)) = MaxIntroPoints

\* Allow termination when max circuits reached AND all HS have their intro points
Terminated == /\ circuits # Empty
              /\ Cardinality(circuits) = MaxCircuits
              /\ AllHSHaveMaxIntroPoints
              /\ UNCHANGED vars

Next == AddCircuit \/ HSSelectIntroPoint \/ Terminated

Spec == Init /\ [][Next]_vars

\* ============================================================================
\* HIDDEN SERVICE INVARIANTS
\* ============================================================================

\* Every intro point must have a corresponding circuit from its HS
IntroPointHasCircuit ==
    hs_intro_points # Empty =>
    \A hs \in DOMAIN hs_intro_points:
        \A intro \in hs_intro_points[hs]:
            hs_to_intro_circuits # Empty /\
            \E c \in hs_to_intro_circuits:
                c.hs = hs /\ c.intro = intro

\* Circuit must exist for each registered intro point
CircuitIntroConsistency ==
    hs_to_intro_circuits # Empty =>
    \A c \in hs_to_intro_circuits:
        hs_intro_points # Empty /\
        c.hs \in DOMAIN hs_intro_points /\
        c.intro \in hs_intro_points[c.hs]

\* Hidden service nodes must be valid
HSNodesValid ==
    \A hs \in HiddenServiceNodes: hs \in Nodes

===============================
