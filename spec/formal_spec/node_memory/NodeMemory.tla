----- MODULE NodeMemory -----

EXTENDS Naturals, FiniteSets, Daturanet

CONSTANTS MaxConnectionSlots, ExpirationTime

ASSUME MaxConnectionSlots > 0
ASSUME MaxConnectionSlots < Cardinality(Nodes)
ASSUME ExpirationTime > 0

VARIABLES node_connections, now

\* Refines Daturanet.now
nodeMemVars == <<node_connections, now>>

\* Connections: node -> set of {dest, timestamp, status}
\* Each connection expires after ExpirationTime
InvConnectionTypeOK ==
  /\ node_connections \in [Nodes -> SUBSET {[dest |-> d, timestamp |-> t, status |-> s] : 
                                              d \in Nodes, t \in Nat, s \in {"connected", "unresponsive", "disconnected"}}]
  /\ \A n \in Nodes :
     /\ Cardinality(node_connections[n]) <= MaxConnectionSlots
     /\ \A conn \in node_connections[n] : conn.dest /= n

\* Remove expired connections
GarbageCollect == 
  node_connections' = [n \in Nodes |-> 
                       {conn \in node_connections[n] : now - conn.timestamp < ExpirationTime}]

\* Establish connection to a known peer
EstablishConnection(n, peer) ==
  /\ peer \in known_nodes[n]
  /\ Cardinality(node_connections[n]) < MaxConnectionSlots
  /\ node_connections' = [node_connections EXCEPT 
                          ![n] = @ \cup {[dest |-> peer, timestamp |-> now, status |-> "connected"]}]

=========================================
