# Building
```
cargo build --release
```

# Usage
#### Run Node A (routing node)
```
./target/release/routing-rules node-a [port]
```
Listens for incoming connections. Accepts PoW-gated routing rule registrations and forwards packets according to its local routing table. Default port: 9110.

#### Run Node C (destination node)
```
./target/release/routing-rules node-c [port]
```
Listens for forwarded packets and prints the received hash and payload. Default port: 9111.

#### Register a routing rule (Node B)
```
./target/release/routing-rules register <node-a-addr> <match-hash> <node-c-addr> <target-hash>
```
Fetches a PoW challenge from Node A, solves it, and submits a routing rule: packets arriving at Node A for `match-hash` will be forwarded to `node-c-addr` re-addressed as `target-hash`.

#### Send a packet
```
./target/release/routing-rules send <node-a-addr> <dest-hash> <message>
```
Sends a packet to Node A destined for `dest-hash`. If a routing rule exists, Node A forwards it; otherwise it is silently dropped.

#### Self-contained test
```
./target/release/routing-rules test
```
Spawns Node A and Node C in-process, registers a routing rule, delivers a matching packet, delivers an unmatched packet (dropped), and then attempts two rules that Node A refuses: a second rule for an already-routed hash, and a rule that points back at Node A. Demonstrates the full flow without external processes.

# Output
#### test
```
[node-c:9111] listening
[node-a:9110] listening

step 1: Node B registers routing rule on Node A
[register] connecting to node-a at 127.0.0.1:9110
[node-a] 127.0.0.1:62106: register request, sent challenge (difficulty 800)
[register] challenge received (difficulty 800), solving...
[register] solved, submitting rule: '44AWD' -> 127.0.0.1:9111 @ '88QWD'
[node-a] 127.0.0.1:62106: rule stored: '44AWD' -> 127.0.0.1:9111 @ '88QWD'
[register] routing rule accepted

step 2: sender delivers packet for hash '44AWD' to Node A
[send] --> node-a at 127.0.0.1:9110, hash '44AWD', 22 bytes
[node-a] 127.0.0.1:62107: packet arrived for hash '44AWD' (8 hops left)
[node-a] 127.0.0.1:62107: forwarding to 127.0.0.1:9111 re-addressed as '88QWD'
[node-a] forwarded 22 payload bytes
[node-c] packet received from 127.0.0.1:62108:
  dest_hash : '88QWD'
  payload   : "hello, hidden service!"

step 3: packet for unknown hash is dropped by Node A
[send] --> node-a at 127.0.0.1:9110, hash 'UNKNOWN', 31 bytes
[node-a] 127.0.0.1:62109: packet arrived for hash 'UNKNOWN' (8 hops left)
[node-a] 127.0.0.1:62109: no routing rule for 'UNKNOWN', dropping

step 4: a second rule for '44AWD' is refused (hijack attempt)
[register] connecting to node-a at 127.0.0.1:9110
[node-a] 127.0.0.1:62110: register request, sent challenge (difficulty 800)
[register] challenge received (difficulty 800), solving...
[register] solved, submitting rule: '44AWD' -> 127.0.0.1:9199 @ '88QWD'
[node-a] 127.0.0.1:62110: a rule for that hash already exists, rejecting
[register] routing rule rejected

step 5: a rule pointing back at Node A is refused (routing loop)
[register] connecting to node-a at 127.0.0.1:9110
[node-a] 127.0.0.1:62111: register request, sent challenge (difficulty 800)
[register] challenge received (difficulty 800), solving...
[register] solved, submitting rule: 'LOOPHASH' -> 127.0.0.1:9110 @ 'LOOPHASH'
[node-a] 127.0.0.1:62111: target address is this node, rejecting
[register] routing rule rejected

test complete
```

# Implementation

Node A maintains a private in-memory routing table (`HashMap<[u8;32], RoutingRule>`). Entries are never advertised over the network. Any node can ask Node A to add a rule by paying PoW; any sender can deliver a packet, which Node A either forwards or drops based on whether the destination hash matches a stored rule.

#### REGISTER flow

Node B pays to install a routing rule on Node A:

```
Node B --> Node A : [MSG_REGISTER: 1 byte]
Node A --> Node B : [challenge: 16 bytes]           (Equi-X PoW, difficulty 800)
Node B --> Node A : [solution: 24 bytes]
                    [match_hash: 32 bytes]
                    [target_addr_len: 2 bytes LE]
                    [target_addr: N bytes]
                    [target_hash: 32 bytes]
Node A --> Node B : [MSG_ACK: 1 byte] | [MSG_REJECT: 1 byte]
```

Node A verifies the solution before storing the rule, then checks that the rule itself is acceptable. It is stored only if all of the following hold:

- the PoW solution verifies against the challenge Node A issued on this connection
- `target_addr` parses as a literal `IP:port`. Node A never resolves a hostname supplied by a registrant, so a rule cannot make it perform a DNS lookup
- the target is a unicast host that is not `0.0.0.0`/`::`, not multicast, not the IPv4 broadcast address, not port 0, and not Node A's own listener
- `target_addr_len` is at most 64 bytes
- no rule already exists for `match_hash` (**first-come-wins**, see below)
- the table holds fewer than 1024 rules

Anything else receives `MSG_REJECT`. Every rejection is identical: Node A never reports which check failed. A reasons would let anyone spend a PoW to learn whether a given hash is already routed here.

The routing table entry maps `match_hash --> (target_addr, target_hash)`.

#### PACKET flow

```
Sender --> Node A : [MSG_PACKET: 1 byte]
                    [hops_left: 1 byte]
                    [dest_hash: 32 bytes]
                    [payload_len: 2 bytes LE]
                    [payload: N bytes]
```

Node A looks up `dest_hash` in its routing table:
- **`hops_left == 0`**: drops the packet.
- **Hit**: opens a new TCP connection to `target_addr` and writes `MSG_PACKET + (hops_left - 1) + target_hash + payload_len + payload`. The payload is forwarded unchanged; only the destination hash is rewritten and the hop counter decremented.
- **Miss**: drops the connection silently. The sender receives no response in either case.

Senders start a packet at 8 hops. Without the counter, a set of rules that forms a cycle forwards the same packet forever, so a single packet becomes unbounded traffic. Refusing a rule that points at Node A itself catches the one-node case; the hop counter is what bounds a cycle spread across several nodes, which no single node can detect locally.

#### Resource limits

Node A is intended to survive the spec's (DDoS) adversary, so every unbounded resource a peer can consume is capped:

| Limit | Value | What it bounds |
| --- | --- | --- |
| `MAX_HOPS` | 8 | forwards per packet, so a rule cycle cannot amplify |
| `MAX_RULES` | 1024 | routing table entries; a rule costs one PoW to install but nothing to keep |
| `MAX_CONNECTIONS` | 256 | live handler threads; each is parked for up to `IO_TIMEOUT` |
| `MAX_ADDR_LEN` | 64 | bytes of registrant-supplied target address |
| `IO_TIMEOUT` | 30 s | how long a stalled peer holds a thread |

A failed `accept()` is logged and skipped rather than terminating the listener, so exhausting file descriptors cannot take the node down.

#### PoW (Equi-X)

Reused from PoC 4.5 / 4.9. Challenge format: `[effort: 4 bytes LE][random: 12 bytes]`. Solution format: `[salt: 8 bytes][equix_hash: 16 bytes]`. Verification: recompute BLAKE2b-32 over `challenge || solution`, check `hash_u32 * effort < U32_MAX`, then verify the Equi-X solution. Difficulty 800 solves in tens of milliseconds on a single thread.

#### Hash format

Hashes are fixed 32-byte arrays. In this PoC they are populated by zero-padding the ASCII bytes of a string (`"44AWD"` --> `[0x34,0x34,0x41,0x57,0x44,0x00,...]`). In production these would be the node's hashring position derived from its default hidden service address (see PoC 8).

#### Routing table privacy

The table is intentionally not queryable. There is no protocol message to list or inspect entries. A node can only confirm a rule exists implicitly, by observing that a packet was forwarded. This is the design in the spec: routing rules enable rendezvous-point indirection without revealing to intermediate nodes which hidden service the traffic is ultimately for.

# References
- [Issue #71: PoC 11 routing rules](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/71)
- [PoC 4.5: Equi-X multi-threaded PoW](../4.5-pow-equix-threaded/)
- [PoC 4.9: PoW-gated node list query](../4.9-pow-query/)
- [PoC 8: Hashring + closest neighbor lookup](../8-hashring-neighbors/)

# Notes for future reference and implementation
- No hidden service ownership check: any node that pays PoW can install a routing rule. PoC 10 is still incomplete: it would produce a ZKP-based certificate signed by the hidden service (using Schnorr on Ristretto255 + Poseidon hash) that proves the requester owns the HS whose hash is `44AWD` without revealing the actual address. Until PoC 10, a malicious node can install routing rules for hashes it does not own.
- **First come first served** Because Node A cannot tell a legitimate re-registration from a hijack, it refuses both. PoC 10's work.
- A registrant can still probe the table indirectly: `MSG_ACK` means the hash was free, `MSG_REJECT` means it was not (or the rule was otherwise invalid). Rejections are indistinguishable from each other, but acceptance is inherently distinguishable from refusal. The cost is one PoW per hash tested against a 2^256 space, so this only confirms a hash the adversary already suspects. An ownership certificate removes the oracle, since a prober would then also need the HS key.
- Target validation is structural only. Node A checks that a target is a routable unicast `IP:port` and not itself, but has no way to confirm it is a real Datura node, so a rule can still aim traffic at an arbitrary third party. A production node should check the target against the node list before accepting the rule, and should refuse targets in ranges it should never route to. The loopback targets used throughout this PoC are why no private-range filter is applied here.
- No expiry on rules: the spec describes timestamps after which a rule must be evicted. PoC 10's certificate embeds the `until_timestamp` that Node A would verify before storing the rule; without it there is no authenticated expiry. `MAX_RULES` bounds the memory an adversary can buy but is not a substitute — once the table is full, Node A refuses every new rule until restart, so expiry is what actually keeps the table usable
- Routing rules are currently held in memory only; they vanish on restart. Production nodes need persistence to disk (encrypted at rest) so rules survive across restarts
- Currently one rule per `match_hash`. The decoy destinations feature (PoC 7) requires fan-out: one incoming hash maps to 8 outgoing targets simultaneously, all receiving the same payload
- Packet forwarding is not PoW-gated. Only rule registration costs PoW. Per-packet PoW is a separate concern handled by the bandwidth throttling layer (PoC 6)
- The 32-byte hash is currently ASCII-padded strings. It should be replaced with the hashring position hash from PoC 8 (Blake3 or SHA-256 of the node's default `.dn` address)
- No rule revocation: a registered rule persists until restart. A revocation message (also PoW-gated) is needed so nodes can remove stale rules without restarting. First-come-wins makes this more pressing than it was: a hash occupied by mistake or by a squatter now cannot be reclaimed at all until Node A restarts
- Connection and rule limits are fixed constants. They should scale with the node's advertised capacity and its phase (see the phase-based onboarding in the spec) rather than being the same on every node
- Wire protocol has no version byte; any future change could break compatibility with older nodes. The packet format already changed once, when `hops_left` was added — a version byte would have made that a negotiation instead of a flag day

`README.md` generated from custom text editor
