----- MODULE AdversarialNodes -----

EXTENDS Naturals, FiniteSets, Daturanet

CONSTANTS BadNodes
ASSUME BadNodes \subset Nodes /\ BadNodes /= {}

VARIABLES bad_nodes, now

\* Refines Daturanet.now
advVars == <<bad_nodes, now>>

\* At least one honest node always exists
InvAdversaryBound == bad_nodes \subset Nodes

\* Bad nodes gossip fake/nonexistent nodes to pollute memory
PoisonGossip(n) ==
  /\ n \in bad_nodes
  /\ \E fake \in (Nodes \times {1..10})  \* Generate fake node identifiers
  /\ LET new_knowledge == known_nodes[n] \cup {fake}
         trimmed == IF Cardinality(new_knowledge) <= MaxNodeMemory
                    THEN new_knowledge
                    ELSE new_knowledge
     IN known_nodes' = [known_nodes EXCEPT ![n] = trimmed]
        /\ UNCHANGED bad_nodes

\* Bad nodes repeatedly connect/disconnect to exhaust connection slots
SlotExhaustion(n) ==
  /\ n \in bad_nodes
  /\ \E peer \in known_nodes[n] :
     /\ LET new_conn == [dest |-> peer, timestamp |-> now, status |-> "connected"]
        IN node_connections' = [node_connections EXCEPT ![n] = @ \cup {new_conn}]

=========================================
