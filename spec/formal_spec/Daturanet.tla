---------- MODULE Daturanet -----------

EXTENDS Naturals,FiniteSets

CONSTANTS Nodes, BootstrapNodes \* both are a set of model values
ASSUME BNinNodes == BootstrapNodes \subseteq Nodes

VARIABLES known_nodes

vars == <<known_nodes>>

DefaultNodeCardinality == 1..5
DefaultBootstrapNodes == 1..2

\* Each node knows a set of other nodes (including bootstrap nodes)
InvTypeOK == /\ known_nodes \in [Nodes -> SUBSET Nodes]
             /\ \A n \in Nodes: n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in Nodes: \E bn \in BootstrapNodes: bn /= n /\ bn \in known_nodes[n]

Invariants == InvTypeOK /\ InvAlwaysKnowBootstrap

\* Initially, every node knows bootstrap nodes (except itself)
Init == /\ known_nodes = [n \in Nodes |-> {CHOOSE bn \in BootstrapNodes: bn /= n }]

\* Nodes gossip: pick a node and a peer, learn about peer's known nodes
Next == \E n \in Nodes :
          \E peer \in Nodes :
            /\ n /= peer \* we choose a peer that is not ourselves in the node set
            /\ known_nodes' = [known_nodes EXCEPT ![n] = known_nodes[n] \cup {peer} \cup (known_nodes[peer] \ {n}) ] \* we learn about that peer and we learn its own known nodes

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* eventually each node will have a list of Nodes containing more than the bootstrap
EventuallyLearnNewNodes == <>\A n \in Nodes: Cardinality(known_nodes[n]) > 1
EventuallyLearnNonBootStrap == <>\A n \in Nodes: \E nonBootstrap \in Nodes \ BootstrapNodes: nonBootstrap \in known_nodes[n]

Properties == EventuallyLearnNewNodes /\ EventuallyLearnNonBootStrap

THEOREM Spec => Invariants /\ Properties

=========================================
