# Building
```
cargo build --release
```

Debug builds work but are dramatically slower for anything that touches the ZK circuit (MockProver runs on the full certificate circuit take 15+ minutes in debug vs. ~1-2 minutes in release). Use `--release` for everything below.

# Usage
#### Run Node A (candidate rendezvous node)
```
./target/release/hs-certificates node-a [port]
```
Generates a random dual-keypair identity, prints its `.dn` address and its canonical hash, and listens for RDV requests. Default port: 9120.

#### Request RDV status (Node B, the hidden service destination)
```
./target/release/hs-certificates request-rdv <node-a-addr> <node-a-hash-hex> [expires-unix-ts] [target-node-hash-hex]
```
Connects to Node A declaring the desired `expires` (so the challenge is priced for it), solves the Equi-X PoW challenge, builds a certificate (a fresh random HS identity is generated per run), and submits it. After Node A's endorsement comes back, sends the private routing instruction naming the target node (Node C). `expires` defaults to 24h; the target defaults to a random demo hash if omitted. Writes the resulting certificate and the HS's address binding to the temp directory, and prints the `verify` command line for them.

#### Verify a certificate standalone
```
./target/release/hs-certificates verify <cert-file> [dn-address] [binding-file]
```
Loads a certificate from a JSON file and independently re-checks everything a third party can check from the certificate alone: the ZK proof, the Equi-X PoW solution against the embedded challenge (proof of payment), current-time expiry (see [Automatic revocation](#automatic-revocation)), and the RDV node's endorsement signature. No connection to Node A, no knowledge of the original handshake required. This is the property PoC 10.1 (#117) needs: the proof must stay attached and stay checkable by anyone, not be verified once and discarded.

Supplying a `.dn` address and its address binding adds a check the third party can't make: that the certificate belongs to that hidden service. Both arguments are required.

#### Self-contained test
```
./target/release/hs-certificates test
```
Runs Node A and Node B in-process: RDV request (with declared expiry), duration-priced challenge, PoW solve, certificate build, submit, accept, endorsement, private routing instruction, then saves the certificate to a temp file and re-verifies it standalone. Two further steps re-run that verification with the hidden service's `.dn` address (accepted) and with an unrelated address (rejected), demonstrating the full flow.

# Output
#### test
```
[node-a] address:       ettjdyqaigqxoxhzbziii23zmtyylxm3hxzg4zicurwsg6jr2lzsgkyd.dn
[node-a] identity hash: 3fd6a4eb9afac858dfaa212fff52478d263408d2b420e14893c8466cc08db90f
[node-a:9120] listening

step 1: Node B requests RDV status, submits a certificate, then the private routing instruction
[request-rdv] connecting to node-a at 127.0.0.1:9120
[node-a] 127.0.0.1:60500: RDV request until 1786566113, sent challenge (difficulty 800)
[request-rdv] challenge received (difficulty 800), solving...
[request-rdv] solved, building certificate...
[request-rdv] hidden service address: s5je6kpdlje34ua5wlxekpjk3f72wxpblba53fvkpe42vj4khi3dbuqd.dn
[request-rdv] hidden service hash:    c6a899013e795fa70b8f68493bea6b850ff19ded3c4bb73bf8d55c05da9d0413
[node-a] 127.0.0.1:60500: certificate accepted, agreed to route for hs_hash c6a899013e... until 1786566113
[request-rdv] certificate accepted, endorsement valid
[request-rdv] sending private routing instruction (target: 932a8a984f3df4d9...)
[node-a] 127.0.0.1:60500: routing instruction accepted for hs_hash c6a899013e... until 1786566113 (target kept private)
[request-rdv] routing instruction accepted

step 2: save the certificate and re-verify it completely standalone
hs_hash:        c6a899013e795fa70b8f68493bea6b850ff19ded3c4bb73bf8d55c05da9d0413
rdv_node_hash:  3fd6a4eb9afac858dfaa212fff52478d263408d2b420e14893c8466cc08db90f
issued_at:      1786479714
expires:        1786566113 (not expired)
pow effort:     800 (required 800 for a 86399s window)
pow valid:      true
proof valid:    true
endorsement:    valid
certificate:    VALID

step 3: a client that knows the .dn address checks the certificate belongs to them
...
address:        s5je6kpdlje34ua5wlxekpjk3f72wxpblba53fvkpe42vj4khi3dbuqd.dn
binding:        valid, resolves to this certificate's hs_hash
certificate:    VALID
for address:    yes

step 4: the same certificate checked against an unrelated address
...
address:        3ohofoffcamqfi7arrjxkelfwcldt35mpgos3owpuaq5jbxqyy63daad.dn
binding:        INVALID (cross-signatures invalid: binding does not belong to this address)
certificate:    VALID
for address:    no (authentic certificate, but not this address's)
```

The two verdict lines are deliberately separate. `certificate:` is about authenticity, and an `INVALID` there means whoever served it produced or relayed a forgery, which is what a client would blacklist a node over. `for address:` is about relevance: step 4's certificate is perfectly authentic, it just belongs to somebody else, which says nothing about the node that served it. Collapsing the two into one verdict would make those indistinguishable.

Note the asymmetry between what Node A learns and what the client learns. Node A is told only `c6a899013e...` and cannot work back from it to `s5je6kpd....dn`. The client, starting from the address, can prove the certificate belongs to it. See the censorship note below for what that asymmetry does and does not buy.

# Implementation

A certificate (issue #67) lets a hidden service (HS) destination authorize a node to act as its rendezvous (RDV) node, up until an expiration timestamp and proved to have paid via a solved PoW challenge, without ever revealing the HS's actual identity to that node, only the public hash of it. The certificate carries a halo2 zero-knowledge proof of "the party that signed this message knows the private key behind hash H", so the RDV node (and later, any client) can verify the grant is authentic without learning who H actually belongs to.

#### Module layout

| File | What it proves / does |
|---|---|
| `address.rs` | `.dn` address encoding/decoding: `pubkey_to_address` / `address_to_pubkey`, a 56-character base32 label carrying the Ed25519 key, a checksum and a version byte. Copied verbatim from PoC 9 so the two stay diffable |
| `identity.rs` | The dual-keypair identity: `generate_identity`, the two-way cross-signature (`cross_sign` / `verify_cross_signatures`), the publishable `AddressBinding`, and `resolve_hs_hash` (`.dn` address --> canonical network hash) |
| `dlog.rs` | Constraint 1: knowledge of `sk` such that `pk = [sk]G`, using a from-scratch `FixedPoints` impl on the pallas curve generator |
| `schnorr.rs` | Constraint 3: a Schnorr signature `(R, s)` over a message verifies under `pk`, entirely inside the circuit, without ever revealing `pk` |
| `circuit.rs` | Combines constraint 1, Poseidon(pk) == hs_hash (constraint 2), and the Schnorr check into one circuit, over the public instance `[hs_hash, rdv_node_hash, expires, pow_challenge]` |
| `equix_pow.rs` | PoW challenge/solve/verify, copied from PoC 11 for simplicity, same scheme (Equi-X + BLAKE2b outer difficulty check) |
| `certificate.rs` | `Certificate` struct, `build_certificate` (prove), `verify_certificate` (pure, stateless, repeatable verify), plus the shared prove/verify plumbing both artifact types go through |
| `routing.rs` | The private routing instruction (option B): `build_routing_instruction` / `verify_routing_instruction`, reusing the certificate circuit with a domain-separated instance |
| `main.rs` | Node A / Node B CLI plus the full acceptance checks (`accept_certificate`: PoW freshness + hash match + expiry + proof; `accept_instruction`: hash matches + proof) and duration-based challenge pricing (`required_effort`) |

#### Dual-keypair identity

Every node and hidden service is one principal holding two keypairs:

| Keypair | Used for |
|---|---|
| Ed25519 | the `.dn` address, 9's address-only E2EE, plain signatures |
| pallas | this PoC's ZK certificates; its Poseidon hash is the canonical network identifier and hashring position |

The two are fused by **two-way cross-signatures**: the Ed25519 key signs the pallas public key, and the pallas key signs (a uniform-mapped digest of) the Ed25519 public key. Each direction is domain-separated by its context string. Verifying both proves the keys belong to the same principal, and neither direction alone suffices: a single direction lets whoever holds the other key claim a binding they cannot produce the counterpart for.

The canonical identifier is `Poseidon(pallas_pk.x, pallas_pk.y)`, not a hash of the `.dn` address. This makes every network identifier a valid `Fp` element by construction (~75% of SHA-family hashes are not), so certificates, PoC 8's hashring positions and PoC 11's routing rules all use the same 32-byte hash. Addresses stay Ed25519-based, so PoC 9's address-only E2EE works.

#### The address binding (not in the certificate)

`resolve_hs_hash(dn_address, binding)` is the `.dn` --> canonical-hash resolution step: it recovers the Ed25519 key from the address, checks both cross-signature directions against the supplied pallas key, and returns `Poseidon(pallas_pk)`. A directory serving a binding cannot substitute its own pallas key for an address; it'd require forging an Ed25519 signature under a key it does not have.

```
AddressBinding
  pallas_pk        32 bytes   the principal's pallas public key
  cross_sigs       96 bytes   Ed25519 sig over the pallas key (64) + Schnorr (R, s) over the Ed25519 key (32+32)
```

The binding is not a certificate field. The Ed25519 public key is the `.dn` address, shipping it inside the artifact the rendezvous node receives would hand Node A the identity the ZK proof hides. The binding should travel only to those that already know the address; in PoC 10.1 (#117) it rides inside the encrypted descriptor, where the HSdir cannot read it. A test shows the Ed25519 public key appears nowhere in a serialized certificate.

#### (!) This does not block targeted censorship (!)

The goal is that a RDV node "must not be able to know that they are being requested to become a RDV node for THIS hidden service (because if they can, then that means that they can censor hidden services)". Only half of that works here.

`H` is a deterministic function of the identity, so address --> `H` is computable by anyone holding the binding, and a binding is by obtainable by anyone who knows the address (in 10.1, the descriptor's decryption key is derived from the address itself, so "can decrypt" and "knows the address" are the same). A rendezvous node cannot work backwards from a certificate to an address it did not know, but it can work forwards: take an address it wants to blocklist, compute that service's `H` once, and refuse certificates with it.

So what the ZK proof gets is that a RDV node cannot enumerate or profile who it relays for, and cannot censor a service it has never heard of. It does not stop a censor who already has a target list of addrs. Fixing it needs an identifier that varies per epoch or per RDV node (Tor's HSDir blinding is closest) so that no single long-lived `H` exists to blocklist. That is a design change of the entire datura network spec.

#### Certificate contents

```
hs_hash        32 bytes   H = Poseidon(pk.x, pk.y), the public HS identifier
rdv_node_hash  32 bytes   which node this grant authorizes
issued_at       8 bytes   unix timestamp the grant was issued; with expires it fixes the paid-for window
expires         8 bytes   unix timestamp, public and unencrypted
pow_challenge  16 bytes   the Equi-X challenge Node A issued
pow_solution   24 bytes   the Equi-X solution Node B found
proof          variable   the halo2 ZK proof
endorsement    96 bytes   the RDV node's counter-signature (rdv_pk, sig R, sig s), attached after acceptance; binds issued_at
```

#### Routing instruction contents (private, never published)

```
hs_hash           32 bytes   same H as the certificate's
rdv_node_hash     32 bytes   the RDV node being instructed (Node A)
target_node_hash  32 bytes   where to route H's traffic (Node C); the secret this artifact exists to protect
proof             variable   halo2 proof over [hs_hash, rdv_node_hash, target_node_hash, ROUTE_DOMAIN]
```

#### RDV endorsement

An accepted certificate carries the RDV node's own counter-signature, not just a session ACK. After `accept_certificate` passes, Node A signs `Poseidon(hs_hash, rdv_node_hash, issued_at, expires, pow_challenge)` (a 5-ary Poseidon, domain-separated by arity from the 2-ary identity hash and the 3-ary envelope message) with its identity key and returns `(rdv_pk, R, s)` alongside the ACK. Including `issued_at` here is what lets a standalone verifier trust it when recomputing the duration price. Unlike the hidden service's side this needs no ZK: an RDV node's identity hash `Poseidon(rdv_pk)` is public by design, so it can reveal `rdv_pk` and sign plainly. `verify_endorsement` checks that the revealed `rdv_pk` actually hashes to the certificate's `rdv_node_hash` before checking the signature, so only the named node can endorse. Node B verifies the endorsement on receipt and discards it if invalid.

The circuit proves, over public inputs `(hs_hash, rdv_node_hash, expires, pow_challenge)`: (1) knowledge of `sk` such that `pk = [sk]G`, (2) `hs_hash == Poseidon(pk.x, pk.y)`, and (3) a Schnorr signature over `m = Poseidon(rdv_node_hash, expires, pow_challenge)` verifies under `pk`. Binding `rdv_node_hash`, `expires`, and `pow_challenge` into the signed message is what makes tampering with any of them after issuance break the proof.

#### Private routing instruction

A certificate deliberately does NOT say where traffic should actually be routed. Binding the target into the shareable certificate would leak Node C to any third party who later reads it from a directory/DHT; So here we implement a second, private, ZK-signed artifact.

After Node A's endorsement comes back, Node B sends a `RoutingInstruction` on the same session: "route traffic for `hs_hash` to `target_node_hash`". It reuses the certificate circuit and proving key verbatim, with the public instance `[hs_hash, rdv_node_hash, target_node_hash, ROUTE_DOMAIN]` in place of the certificate's `[hs_hash, rdv_node_hash, expires, pow_challenge]`. `ROUTE_DOMAIN` is a fixed constant equal to `2^192 + ascii("route_1")`: a certificate's fourth slot is a u128 `pow_challenge` and can never reach that value, so neither proof type can be replayed as the other even though they share a circuit (tested both directions in `routing.rs`).

Node A accepts an instruction only if its `hs_hash` equals the hash of the certificate it just endorsed, its `rdv_node_hash` names Node A itself, and the proof verifies. (only the holder of the key behind the granted hash could have produced it) The target then goes into Node A's local routing table (`RdvEntry.route_target`) and nowhere else: it is never logged in full, never part of the certificate, and never shared. Verifying the instruction reveals nothing about the HS beyond what the certificate already exposed.

#### PoW (Equi-X), priced by requested duration

Reused from what I did for PoC 11 (which reused it from PoC 4.5 / 4.9), with one fix: the salt arithmetic in `solve_challenge` is now wrapping (the original overflows u64 about half the time with 2+ threads, panicking in debug builds; PoC 11 got the same fix).

The challenge difficulty now scales with the grant being bought: Node B declares its desired `expires` inside the RDV request, and Node A prices the challenge at `MIN_CHALLENGE_DIFFICULTY` (800) per started day of requested lifetime (`required_effort`), so a 30-day grant costs 30× the work of a 1-day grant. `accept_certificate` then requires `cert.expires` to equal exactly the expiry that was priced: a certificate claiming any other lifetime wasn't what was paid for. Difficulty 800 solves in tens of milliseconds on a single thread; 24000 (30 days) in the low seconds.

The 30-day cap (`MAX_CERT_LIFETIME_SECS`) stays even with pricing: Equi-X solve time scales linearly, so a years-long grant would need one absurd challenge, and renewal-by-expiry is the intended mechanism anyway.

#### Automatic revocation

A grant is authorization for a window that was paid for up front, so it ends by expiring. Nothing is signed to revoke, nothing says it ended, and no party has to be reachable.

**Any verifier, from the certificate alone.** `verify_cert_file` treats `expires <= now` as expired and reports `certificate: INVALID` regardless of how well everything else checks out.

**On the RDV node, on a timer.** `prune_expired` drops every entry whose window has closed, run by a sweeper thread every `REVOCATION_SWEEP_INTERVAL`. (set at 1 minute here) The target is the field this node is trusted to keep private, and it doesn't outlive the grant that authorized it. `attach_route` also refuses to install a target on an already-expired grant, which is reachable between sweeps. Nothing stops Node B buying a grant lasting seconds, and building the routing instruction's proof takes longer than that. A `MIN_CERT_LIFETIME` could be added network-wide in addition to `MAX_CERT_LIFETIME_SECS`.

# Design notes / tradeoffs

Read these before extending this PoC.

- Identity: dual keypair rather than one curve for everything. The pallas keypair with `H = Poseidon(pk.x, pk.y)` is what the circuit can work with (neither Ed25519 nor SHA-256 have practical ZK-circuit support); the Ed25519 keypair is what the rest of the network uses. Rather than remove one, a principal holds both and cross-signs them. The alternative was to replace PoC 9's HPKE with a circuit-friendly KEM over pallas, which trades two well-understood prims for nonstandard crypto. The cost of the dual keypair is that a principal now has two secrets to keep and that any consumer needs the binding to move between them; the benefit is that neither existing scheme has to change.
- halo2 over Ristretto255/bulletproofs or arkworks/Groth16. halo2 is published, actively maintained, and needs no trusted-setup ceremony. The R1CS gadget API needed to do this with `bulletproofs` + Ristretto255 is experimental and unpublished (`yoloproofs`, off crates.io). arkworks/Groth16 is published but needs a per-circuit trusted-setup ceremony, not helping here in aa basically "no trust" network.
- In-circuit Schnorr verification has no Orchard precedent. Orchard's own spend authorization is checked outside it's circuit (a plain signature at the transaction level) because it doesn't need to hide the signing key. This PoC does, so the verification equation is checked entirely in-circuit. (see `schnorr.rs`'s module doc for the specific gadget sequence).
- Proof size is NOT optimized. halo2's no-trusted-setup property (IPA-based) is a trade off with Groth16's constant ~200-byte proofs; halo2 proofs here run several KB. Although we want certificates as small as possible, since potentially millions could be stored network-wide, this is intentionally deferred to a follow-up PoC.
- `verify_certificate` only checks the proof. PoW freshness, hash match, and expiry are Node A's own business logic (`accept_certificate` in `main.rs`), kept separate so `verify_certificate` stays a pure function anyone can call later, independent of the original handshake. `accept_certificate` runs (in essence) 'cheap checks' (hash, expiry, PoW) before the expensive ZK proof verification, so a peer that never solved the PoW can't burn Node A's CPU on proof verification with faked certificates.
- Standalone verifiers enforce the PoW policy themselves. The effort is embedded in `pow_challenge`'s low 32 bits and is whatever the issuing node chose; a colluding HS + RDV pair could otherwise mint a long-lived certificate against a trivial-effort challenge. The certificate now records `issued_at` (bound into the endorsement digest), so `verify_cert_file` recomputes the exact duration price `required_effort(expires - issued_at)` and rejects any certificate paying less, keeping `MIN_CHALLENGE_DIFFICULTY` only as a floor. Because `issued_at` must not be in the future, a far-future `expires` forces a proportionally large window, so the pair can't understate what they owe.
- The 3-ary Poseidon uses (certificate envelope `m = Poseidon(rdv_hash, expires, challenge)`, routing envelope `m = Poseidon(rdv_hash, target, ROUTE_DOMAIN)`, and Schnorr challenge `e = Poseidon(R.x, pk.x, m)`) share a hash domain (`ConstantLength<3>` encodes only arity, not purpose). The certificate/routing envelopes are separated from *each other* by the `ROUTE_DOMAIN` constant in slot 3, which no u128 `pow_challenge` can equal. Tested in both directions in `routing.rs`.
- The routing instruction carries no expiry of its own: the route lives and dies with the certificate that authorized the RDV grant, and Node A ties the two together at acceptance time (`accept_instruction` requires the instruction's `hs_hash` to match the endorsed certificate's). Since the instruction is private to the B→A session, there is no third-party replay surface to bind an expiry against.
- Table writes are keyed on a session number, not just `hs_hash`. One hidden service can have two handshakes in flight at once (a renewal opened before the previous session finished, or a retry after a timeout), and both key the routing table on the same hash. Since the grant is stored when the certificate is endorsed but the target only arrives a message later, an unguarded table would let the second session's certificate replace the first's in between, leaving the entry pairing one session's certificate with the other's target and Node A would still ACK the first session as successful. Each connection is numbered in accept order, and both writes are conditioned on it under a single lock acquisition: `commit_grant` refuses to let a stalled older session displace a newer grant, and `attach_route` only writes a target onto the grant from its own session, rejecting otherwise so Node B learns the instruction did not take and can resubmit. A grant that supersedes another inherits its `route_target`, since the hidden service just re-proved ownership of the same hash and the protocol lets Node B close without resending an instruction; clearing it would silently blackhole a route already carrying traffic.

# References
- [Issue #67: HS routing-delegation certificates](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/67)
- [Issue #117: PoC 10.1 encrypted HS descriptor](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/117) (included and guided some descisions to allow easier future development)
- [PoC 11: Hash-based routing rules](../11-routing-rules/)
- [PoC 4.5: Equi-X multi-threaded PoW](../4.5-pow-equix-threaded/)
- [PoC 8: Hashring + closest neighbor lookup](../8-hashring-neighbors/)
- [halo2](https://zcash.github.io/halo2/) [CLEARNET LINK]

# Notes for future reference and implementation
- PoW price vs. grant duration: Node B declares its desired `expires` in the RDV request and Node A prices the challenge from it (`required_effort`, linear per started day). The certificate records `issued_at` and binds it into the RDV node's endorsement digest, so a standalone third party can now recompute the full price `required_effort(expires - issued_at)` rather than only the one-day floor. `issued_at` is bound via the endorsement (which a complete certificate must carry) rather than the ZK proof's public instance, so this needed no circuit/proving-key change; folding it into the instance instead would be a stronger but heavier alternative. Feedback on that tradeoff is welcome.
- Not concerning myself with low dependency count currently, just working towards a functional version.
- (!) One major dependency (`halo2_gadgets`) is relatively new and needs security research and ongoing verification. A critical vulnerability was disclosed in mid-2026. Nothing here should be treated as fully secure cryptography without proper review of the circuit.
- Field-element encoding: `pow_challenge` / `expires` / hash fields are encoded as raw pallas field elements (`Fp`), so an externally-supplied 32-byte hash must already be a canonical `Fp` encoding. Making `Poseidon(pallas_pk)` the canonical identifier is what guarantees this network-wide, since every such hash is an `Fp` element by construction. A non-canonical hash from anywhere else is still rejected cleanly at the input boundary (`parse_hex32`), and `hash_bytes_to_fp` returns `Option` rather than panicking. Aligning PoC 8's hashring positions onto the same identifier is tracked separately (#158).
- Proof size optimization (see design notes) is not focused on. A follow-up should measure actual certificate size and decide whether the DHT (#65) is needed to store them.
- Trusted setup / parameter distribution: this PoC regenerates halo2 params and the proving key on first use. A real deployment would generate them once at network creation and distribute them, so nodes do not each pay that cost. (Potentially, could leave them to generate each for a node, which would discourage node creation, but make botting slightly more time and resource consuming)
- The private routing instruction ("route H's traffic to Node C") is now implemented (`routing.rs` + the `MSG_ROUTE` phase). Actual packet forwarding along the installed route remains PoC 11 (#71) / 11.1 (#157) scope: PoC 11.1's `MSG_REGISTER` becomes "submit certificate + instruction" and the rule payload comes from `RdvEntry.route_target`.
- `address.rs` and `identity.rs` are written to be lifted wholesale into PoC 10.1 (#117), per nihilist's "reuse it for poc10.1 afterward". They depend only on `circuit::hs_hash`, `dlog::derive_pk` and `schnorr`, and `digest_to_fp` lives in `identity.rs` rather than being borrowed from a certificate-specific module, so nothing certificate-shaped has to come with them. The descriptor there should carry the `AddressBinding` inside its encrypted payload and reuse `resolve_hs_hash` for the client-side lookup.
- Key persistence is out of scope: every run generates a fresh identity in memory, so an operator cannot yet keep a `.dn` address across restarts. A real node needs on-disk storage for both secrets, which is worth defining once rather than per-PoC.

`README.md` generated from custom text editor