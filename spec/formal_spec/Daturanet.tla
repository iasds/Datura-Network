---------- MODULE Daturanet -----------

EXTENDS Naturals,FiniteSets

CONSTANTS Nodes, BootstrapNodes
ASSUME BootstrapNodes \subset Nodes

VARIABLES known_nodes

vars == <<known_nodes>>

\* Each node knows a set of other nodes (including bootstrap nodes)
InvTypeOK == /\ known_nodes \in [Nodes -> SUBSET Nodes]
             /\ \A n \in Nodes: n \notin known_nodes[n]

InvAlwaysKnowBootstrap == \A n \in Nodes: \E bn \in BootstrapNodes: bn /= n /\ bn \in known_nodes[n]

Invariants == InvTypeOK /\ InvAlwaysKnowBootstrap

\* Initially, every node knows bootstrap nodes (except itself)
Init == /\ known_nodes = [n \in Nodes |-> {CHOOSE bn \in BootstrapNodes: bn /= n }]

\* Nodes gossip: pick a node and a peer, learn about peer's known nodes
Next == \E n \in Nodes :
          \E peer \in known_nodes[n] :
          known_nodes' = [known_nodes EXCEPT ![n] = known_nodes[n] \cup known_nodes[peer]]

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* eventually each node will have a list of Nodes containing more than the bootstrap
EventuallyLearnNewNodes == <>[Cardinality(known_nodes[n]) > 1]_vars

THEOREM Spec => Invariants /\ EventuallyLearnNewNodes

=========================================
