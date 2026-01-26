----- MODULE NodeStatus -----

EXTENDS Naturals, FiniteSets, Daturanet

CONSTANTS NodeStates
ASSUME NodeStates == {"running", "rebooting", "stopped"}

VARIABLES node_status, now

\* Refines Daturanet.now
statusVars == <<node_status, now>>

InvStatusTypeOK ==
  node_status \in [Nodes -> NodeStates]

\* Environment triggers arbitrary reboots
NodeReboot(n) ==
  /\ node_status[n] = "running"
  /\ node_status' = [node_status EXCEPT ![n] = "rebooting"]
  /\ UNCHANGED known_nodes

\* Node recovers after reboot (kept bootstrap knowledge)
NodeRecover(n) ==
  /\ node_status[n] = "rebooting"
  /\ node_status' = [node_status EXCEPT ![n] = "running"]
  /\ UNCHANGED known_nodes

=========================================
