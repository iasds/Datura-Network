## PoC 11.1: ZK certificates gating routing rules

PoC 11 installs a routing rule for whoever pays a PoW. PoC 10 has
two missing portions, a certificate proving ownership of a hash without revealing the key, and a private routing instruction naming where that hash's traffic goes. This PoC combines them, so a rule can only be installed by the hidden service that owns the hash it routes for, and only for the window it paid for.

### Usage
```
$ cargo run --release -- node-a [port]                  # RDV / routing node, default 9130
$ cargo run --release -- node-c <node-a-addr> [port]    # destination node, default 9131
$ cargo run --release -- register <node-a-addr> <node-a-hash> <target-hash> [expires]
$ cargo run --release -- send <node-a-addr> <dest-hash> <message>
$ cargo run --release -- test                           # full flow, in-process
```

Use `--release`. Debug builds are slower for anything touching the ZK circuit.

### Demo
```
$ cargo run --release -- test

step 1: Node C announces itself to Node A
[node-a] <peer>: node e8ec5e02f59dd771 announced at 127.0.0.1:9131

step 2: Node B proves ownership of its hash and buys an RDV grant
[node-a] <peer>: RDV request until <expires>, sent challenge (difficulty 800)
[register] hidden service hash:    a245c2430efa752c172d980426a32c13b457eabd357c925d2ecdfcbd04495603
[node-a] <peer>: certificate accepted, agreed to route for hs_hash a245c2430efa752c...
[register] certificate accepted, endorsement valid
[node-a] <peer>: routing instruction accepted for hs_hash a245c2430efa752c... (target kept private)

step 3: a packet for the hidden service's hash is forwarded to Node C
[node-a] <peer>: forwarding to 127.0.0.1:9131 re-addressed as e8ec5e02f59dd771
[node-c] packet received:
  dest_hash : e8ec5e02f59dd771d0fc8885bca4f8328a92ac9451640b126cbcf344b37ce10a
  for me    : yes
  payload   : "hello, hidden service!"

step 4: a packet for an unrouted hash is dropped
[node-a] <peer>: no live route for 8909795a969f995b, dropping

step 5: the same hidden service renews its grant
step 6: a different hidden service cannot take over that hash
  its grant landed on its own hash 89bb0e099a30cb9a..., not a245c2430efa752c...

step 7: the original route works
```

Step 6, the unrelated hidden service is not refused; it buys its own grant, which lands on its own hash.

### Implementation

New code is `announce.rs` and `main.rs`. `routing.rs` is PoC 10's plus the binding work, and
`circuit.rs` is PoC 10's plus one off-circuit helper (`route_binding`). The other six modules
(`dlog.rs`, `schnorr.rs`, `certificate.rs`, `identity.rs`, `address.rs`, `equix_pow.rs`) are copies of
PoC 10's. `CertificateCircuit` itself is untouched, so the proving key is still PoC 10's.

**PoC 10's gap** Its routing instruction names a target **node hash**; PoC 11's forwarder
needs a **`SocketAddr`**. Nothing resolves one to the other yet: PoC 8.1's DHT is
keyed by `SHA256(ed25519_pk)` and PoC 8's hashring by random ASCII labels, neither of which is
`Poseidon(pallas_pk.x, pallas_pk.y)`.

Node A builds its own map from nodes that announce themselves to it and prove ownership of the hash they
claim. This is not an alternative for a directory: a routing node only needs addresses for nodes it will
actually forward to, and those nodes have every reason to announce themselves to it. After PoC 8.2
puts the hashring on the canonical hash, `node_list.get(hash)` becomes a network lookup.

**Node self-announcement**
```
C --> A : [VER][MSG_ANNOUNCE]
A --> C : [VER][MSG_CHALLENGE][challenge: 16]
C --> A : [VER][pallas_pk: 32][port: 2 LE][pow_solution: 24][sig_r: 32][sig_s: 32]
A --> C : [VER][MSG_ACK] | [VER][MSG_REJECT]
```
No ZK proof needed. A node's identity hash `Poseidon(pk)` is public, so the announcer reveals its pallas public key and signs.

- **The announcer never states its own IP** Node A takes the IP from the TCP source address and
  only the *port* from the message.
- **The signature covers the challenge Node A issued on this connection.** Without it, anyone who
  observed an announcement could resend it from their own IP and rebind that hash to themselves.

The digest is `Poseidon(node_hash, port, challenge, ANNOUNCE_DOMAIN)`. Its 4-ary arity
separates it from every other Poseidon use in this lineage (2-ary `hs_hash`, 5-ary
`endorsement_digest`); `ANNOUNCE_DOMAIN` mirrors `ROUTE_DOMAIN`. Re-announcing is how a node moves
address, only the key holder can do it.

`route_binding` is now 3-ary, same as `envelope_message`. `envelope_message` produces the *signed message*, which never enters an instance vector, and `route_binding`'s output is truncated into `[2^192, 2^193)` before it becomes slot 3. The certificate/instruction separation is on that range (slot 3 is a `u128` for a certificate), not on arity.

**Register flow.**
```
B --> A : [VER][MSG_REQUEST_RDV][requested_expires: 8 LE]
A --> B : [VER][MSG_CHALLENGE][challenge: 16]              (priced by required_effort)
B --> A : [VER][MSG_CERTIFICATE][len: 4 LE][certificate JSON]
A --> B : [VER][MSG_ACK][rdv_pk: 32][sig_r: 32][sig_s: 32] | [VER][MSG_REJECT]
B --> A : [VER][MSG_ROUTE][len: 4 LE][instruction JSON]
A --> B : [VER][MSG_ACK] | [VER][MSG_REJECT]
```
PoC 11's PoW-only registration is gone, and its separate register-time PoW. The certificate
carries a challenge Node A issued in this same session, priced by the requested duration
(`MIN_CHALLENGE_DIFFICULTY` per started day), a second PoW would charge twice for one admission.
The announce path keeps its own PoW because it carries no certificate, making it the only unpaid
write to Node A's state.

The instruction JSON carries a 16-byte `client_nonce` alongside the three hashes and the proof; Node A
needs it to rebuild slot 3.

`accept_certificate` runs cheap checks (hash match, expiry, `issued_at` skew, PoW) before the ZK
proof, and `accept_instruction` resolves the target through the node list before verifying its proof.
The nonce is claimed last, after the proof verifies, so a garbage submission cannot burn a nonce. The instruction's target goes into `RdvEntry.target_addr`.

**Packet flow.** Node A forwards only if the entry has a live grant *and* an attached route. A grant
whose registrant closed the session before sending an instruction routes nothing.
An expired grant stays reachable for up to `REVOCATION_SWEEP_INTERVAL`, so the forwarder re-checks
expiry.

**Automatic revocation.** Unchanged from PoC 10: a grant authorizes a window paid for
up front, so it ends by expiring. Nothing is signed to revoke and no party has to be reachable.
`prune_expired` drops the entry on a timer, which also drops the route, so an expired grant leaves
no record of where it pointed.

**Resource limits.** `MAX_RULES` 1024 granted entries, `MAX_NODES` 1024 distinct nodes in the map,
`MAX_SEEN_NONCES` 65,536 admitted client nonces, `MAX_CONNECTIONS` 256 live handler threads,
`MAX_CERT_LEN` 1 MiB, `IO_TIMEOUT` 30 s. The register
path is promoted to `REGISTER_IO_TIMEOUT` 400 s only once a peer identifies itself as a
registration, so a peer that connects and sends nothing holds a thread for 30 seconds rather than 400.

### Design notes

- **The re-addressing hash collapses, and so does rule chaining** PoC 11 forwarded
  `match_hash --> (target_addr, target_hash)` where `target_hash` was an independent,
  registrant-chosen value. Here it is necessarily `instr.target_node_hash`, the field the proof
  binds. That is stronger, since the outgoing hash is proven, but a
  forwarded packet arrives addressed to the target node's own hash and the next hop treats it as
  terminal. Restoring chaining needs a fifth instance slot in PoC 10's circuit and a new proving
  key, a PoC 10 change. **Feedback wanted on whether chaining is required.**
- **A routing instruction is bound to its grant** An instruction commits
  to `[hs_hash, rdv_hash, target_hash, slot3]`, and slot 3 had a constant domain separator. But, two instructions for the same route were identical to the verifier, so an old one could be replayed against a later grant. Because the instruction is not encrypted and `accept_instruction` authenticates the hash
  rather than the sender, anyone who observed one could replay it and re-pin a hs to a target, inside a window that hs paid for. Slot 3 now
  holds `route_binding(pow_challenge, client_nonce) = Poseidon(route_domain, challenge, nonce)`,
  truncated so the value stays in `[2^192, 2^193)`. The challenge is fresh per connection and already
  checked for freshness when the certificate is accepted, so each instruction is usable only in its
  own ceremony; keeping the value above `2^192` keeps  structural separation from
  certificates, whose slot 3 is a `u128`. No circuit or proving-key change was needed, so instance arity is still four.
- **The nonce stops a malicious RDV node, not the challenge.** Node A picks the challenge and
  keeps no record of ones it issued, so it can reissue a previous challenge instead of
  calling `create_challenge`. Node B solves what it is sent, so under a challenge-only binding
  the old instruction's slot-3 value was rebuilt and the stale target re-pinned for the newly paid
  window. The instruction was bound to *a* challenge, not *fresh*, and the adversary chose
  the value being bound, so no binding over Node A's inputs could close this.
  `client_nonce` is Node B's own 16 bytes, generated per connection by `new_client_nonce` and mixed
  into slot 3 alongside the challenge. Freshness comes from *generating* a value, not recognizing an old one, so it needs no state on Node B and survives restart. 
  The nonce is not a secret: its needs to be
  unpredictable to Node A before the instruction exists, and to be a value Node A must present
  after. Node A rejects a nonce it has already admitted (`claim_nonce`), which is the check the
  *enforcing* party can make: a client-side freshness check would be unverifiable at the
  node and would have to persist across restarts to catch a replay from an earlier grant at all.
  Tampering with the nonce breaks the proof.`a_reissued_challenge_does_not_admit_the_old_instruction` covers the attack end to end.
- **The seen-nonce set is bounded, and wraps** `MAX_SEEN_NONCES` 65,536 entries, cleared at the limit, so a node won't run out of memory. Wrapping loses replay detection for what was cleared. A real node would key the set per
  grant and drop it with the grant, which bounds it by `MAX_RULES` instead and needs no wrap; that is
  a change to `RdvEntry`, left out here.
- **First-come-wins replaced by proven ownership.** PoC 11 refused re-registration because
  it could not distinguish renewals from hijacks. A renewal now succeeds and inherits the existing
  route, while a older session cannot displace a grant committed after it.
- **The registration oracle is gone.** PoC 11 noted that acceptance was distinguishable from
  refusal, so a registrant could pay one PoW per hash to learn whether it was routed. Acceptance now
  additionally requires the hidden service's key, so a prober learns nothing.
- **`MAX_NODES` bounds distinct nodes, not writes.** A node replacing its own entry does not grow
  the map, so filling it requires many distinct pallas keys, each paying a PoW.
- **The node list is not authenticated as a *set*.** Each entry is authenticated individually, but
  Node A cannot know whether the nodes announcing to it are a good sample of the network
  or an adversary.
- **Expiry is unit-tested** A live demonstration would need a 60-second
  wait for the sweeper, or a grant so short it expires during its own handshake. The eviction path,
  the between-sweeps path and the forwarder's expiry filter are covered by tests instead.

### Notes for future reference and implementation

PoC 11 issues this PoC **closes**: no ownership check on the hash being routed for; first-come-wins;
no expiry or revocation; ASCII-padded placeholder hashes (every hash is now `Poseidon(pallas_pk.x, pallas_pk.y)`, validated canonical at the input boundary); no wire version byte; structural-only target validation; the registration probe oracle.

PoC 11 issues that **remain**:

- Routing state is in memory only and vanishes on restart. Production nodes need persistence to disk, encrypted at rest.
- No decoy fan-out. PoC 7 requires one incoming hash to map to 8 outgoing targets simultaneously; here one hash maps to one target.
- Connection and rule limits are fixed constants rather than scaling with a node's advertised capacity and phase.
- Packet forwarding is not itself PoW-gated. That belongs to PoC 6.
- **`hops_left` is sender-supplied and not clamped to `MAX_HOPS` on receipt.** `MAX_HOPS` is applied
  by `do_send` only, so a hostile sender can set 255 and raise the ceiling on a multi-node rule cycle
  accordingly. The counter still decrements monotonically, so a cycle terminates rather than looping
  forever, but the bound is the sender's number rather than the node's. Clamping on receipt is a
  one-line fix and should land alongside whatever PoW-gates forwarding.

New to this PoC:

- **Trusted setup / parameter distribution**, from PoC 10: this crate regenerates halo2
  params and the proving key on first use. Real deployment generates them once at network creation
  and distributes them.
- **Key persistence**, every run generates a fresh identity in memory.
- **The node list has no eviction.** An entry persists until the node re-announces or Node A
  restarts. A liveness check, or a TTL on announcements, is worth defining alongside whatever replaces the map in PoC 8.2.
- **A grant can be bought that is shorter than the time needed to use it.** Building the routing
  instruction's proof can outlast a very short grant, in which case the instruction is refused as
  `Expired`. A network-wide `MIN_CERT_LIFETIME` would close this;

### References
- [Issue #157: ZK certificates gating routing rules](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/157)
- [Issue #67: HS routing-delegation certificates](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/67)
- [Issue #71: PoC 11 routing rules](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/71)
- [PoC 10: Hidden service destination certificates](../10-hs-certificates/)
- [PoC 11: Hash-based routing rules](../11-routing-rules/)
- [PoC 8.1: Kademlia DHT](../8.1-dht-implementation/)

`README.md` generated from custom text editor