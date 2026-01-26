This module describes the core peer discovery protocol via gossip.
Nodes share knowledge of other nodes they know about, starting from bootstrap nodes.
The system converges when nodes discover each other through information propagation.

---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets

CONSTANTS Nodes, BootstrapNodes, MaxNodeMemory
ASSUME BootstrapNodes \subset Nodes
ASSUME MaxNodeMemory >= 2
ASSUME MaxNodeMemory < Cardinality(Nodes)

VARIABLES known_nodes, now

vars == <<known_nodes, now>>

\* Each node knows a set of other nodes (including bootstrap nodes)
\* Cardinality constrained by memory
InvTypeOK == /\ known_nodes \in [Nodes -> SUBSET Nodes]
             /\ \A n \in Nodes : Cardinality(known_nodes[n]) <= MaxNodeMemory
             /\ now \in Nat

\* At least one node is always honest (bootstrap doesn't disappear)
InvBootstrapPreserved == \A n \in Nodes : BootstrapNodes \ {n} \subseteq known_nodes[n]

GlobalInvariants == InvTypeOK /\ InvBootstrapPreserved

\* Initially, every node knows bootstrap nodes (except itself)
Init == /\ known_nodes = [n \in Nodes |-> BootstrapNodes \ {n}]
        /\ now = 0

\* Nodes gossip: pick a node and a peer, learn about peer's known nodes
Gossip == \E n \in Nodes :
          \E peer \in known_nodes[n] :
          LET new_knowledge == known_nodes[n] \cup known_nodes[peer]
              trimmed == IF Cardinality(new_knowledge) <= MaxNodeMemory
                         THEN new_knowledge
                         ELSE new_knowledge  \* TODO: eviction policy
          IN known_nodes' = [known_nodes EXCEPT ![n] = trimmed]
             /\ now' = now + 1

\* Time advancement
Tick == /\ now' = now + 1
        /\ UNCHANGED known_nodes

Next == Gossip \/ Tick

Spec == Init /\ [][Next]_vars

\* Liveness: every node eventually learns about at least one non-bootstrap node
\* (through gossip, not just initial bootstrap knowledge)
EventuallyLearnBeyondBootstrap == 
  \A n \in Nodes :
  \F (Cardinality(known_nodes[n]) > Cardinality(BootstrapNodes \ {n}))

=========================================
