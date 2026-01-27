---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets

CONSTANTS Nodes, BootstrapNodes
ASSUME BNinNodes == BootstrapNodes \subseteq Nodes

VARIABLES known_nodes

vars == <<known_nodes>>

DefaultNodeCardinality == 1..5
DefaultBootstrapNodes == 1..2

InvTypeOK == /\ known_nodes \in [Nodes -> SUBSET Nodes]
             /\ \A n \in Nodes: n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in Nodes: \E bn \in BootstrapNodes: bn /= n /\ bn \in known_nodes[n]

Invariants == InvTypeOK /\ InvAlwaysKnowBootstrap

Init == /\ known_nodes = [n \in Nodes |-> {CHOOSE bn \in BootstrapNodes: bn /= n}]

Next == \E n \in Nodes:
          \E peer \in Nodes:
            /\ n /= peer
            /\ known_nodes' = [
                   known_nodes EXCEPT 
                   ![n] = known_nodes[n] \cup {peer} \cup (known_nodes[peer] \ {n})
               ]

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

EventuallyLearnNewNodes == <>\A n \in Nodes: 
                                Cardinality(known_nodes[n]) > 1
EventuallyLearnNonBootStrap == <>\A n \in Nodes:
                                \E nb \in Nodes \ BootstrapNodes: 
                                    nb \in known_nodes[n]

Properties == EventuallyLearnNewNodes /\ EventuallyLearnNonBootStrap

----

THEOREM Spec => Invariants

<1>1. Init => Invariants
<1>2. Invariants /\ [Next]_vars => Invariants'
<1>3. QED BY <1>1, <1>2 DEF Spec

=========================================
