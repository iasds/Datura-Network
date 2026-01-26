This module describes an initial abstract specification of the network.
It is made of several nodes, each knowing a limited amount of other nodes
We have a set of bootstrap nodes and want to show that after connecting
and sharing information about themselves every node can know every other


---------- MODULE Daturanet -----------

EXTENDS Naturals, FiniteSets

CONSTANTS Nodes, ConnStatus, BootstrapNodes, NodeStatus, MaxNodeMemory, MaxConnections
ASSUME ConnStatus == {connected, unresponsive, disconnected}
ASSUME NodeStatus == {running, rebooting, stopped}
ASSUME MaxNodeMemory >= 2 \* we need to know at least a bootstrap node and another node
ASSUME MaxConnections > 0 /\ MaxConnections < Cardinality(Nodes) \*we must be able to handle at least one connection per nodes and less than all nodes at the same time

\* Nodes can't know every other node in the system
ASSUME MaxNodeMemory < Cardinality(Nodes)

\* There can be many bootstrap nodes but at least one normal node
ASSUME BootstrapNodes \subset Nodes

VARIABLES known_nodes, bootstrap_nodes, node_status, node_connections, now, bad_nodes

vars == <<known_nodes, bootstrap_nodes, node_status, now, node_connections, bad_nodes>>

InvTypeOK == /\ /\ known_nodes \in [ Nodes -> SUBSET Nodes ]
                /\ \A n \in known_nodes: Cardinality(known_nodes[n]) <= MaxNodeMemory
             /\ /\ node_connections \in [ Nodes -> SUBSET {[dest |-> d, timestamp |-> t, status |-> s] : d \in Nodes, t \in Nat, s \in ConnStatus } ]
                /\ \A c \in node_connections: 
                    /\ Cardinality(node_connections[c]) <= MaxConnections 
                    /\ \A conn \in node_connections[c]: c /= conn.dest
             /\ node_status \in SUBSET { [node |-> n1, status |-> s]: n1 \in Nodes, s \in NodeStatus }
             /\ now \in Nat 

InvAdversaryBehavior == bad_nodes \subset Nodes \* not all nodes turn bad at once

GlobalInvariants == InvTypeOK /\ InvAdversaryBehavior

=========================================
