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
Generates a random identity, prints its hash, and listens for RDV requests. Default port: 9120.

#### Request RDV status (Node B, the hidden service destination)
```
./target/release/hs-certificates request-rdv <node-a-addr> <node-a-hash-hex> [expires-unix-ts] [target-node-hash-hex]
```
Connects to Node A declaring the desired `expires` (so the challenge is priced for it), solves the Equi-X PoW challenge, builds a certificate (a fresh random HS identity is generated per run), and submits it. After Node A's endorsement comes back, sends the private routing instruction naming the target node (Node C). `expires` defaults to 24h from now; the target defaults to a random demo hash if omitted.

#### Verify a certificate standalone
```
./target/release/hs-certificates verify <cert-file>
```
Loads a certificate from a JSON file and independently re-checks everything a third party can check from the certificate alone: the ZK proof, the Equi-X PoW solution against the embedded challenge (proof of payment), current-time expiry (automatic revocation), and the RDV node's endorsement signature. No connection to Node A, no knowledge of the original handshake required. This is the property PoC 10.1 (#117) needs: the proof must stay attached and stay checkable by anyone, not be verified once and discarded.

#### Self-contained test
```
./target/release/hs-certificates test
```
Runs Node A and Node B in-process: RDV request (with declared expiry), duration-priced challenge, PoW solve, certificate build, submit, accept, endorsement, private routing instruction, then saves the certificate to a temp file and re-verifies it standalone, demonstrating the full flow without separate terminals.

# Output
#### test
```
[node-a] identity hash: 5c328142b1d0c7ca87fe8627e1c533da41e08a52a6c09dcd70ca2d7bfa2bd53d
[node-a:9120] listening

step 1: Node B requests RDV status, submits a certificate, then the private routing instruction
[request-rdv] connecting to node-a at 127.0.0.1:9120
[node-a] 127.0.0.1:55630: RDV request until 1783118323, sent challenge (difficulty 800)
[request-rdv] challenge received (difficulty 800), solving...
[request-rdv] solved, building certificate...
[request-rdv] hidden service hash: 36c8680cf47c3a852dac0b0764aa8d1daa9c4a55ff5799af26faf94d3a24531f
[node-a] 127.0.0.1:55630: certificate accepted, agreed to route for hs_hash 36c8680c... until 1783118323
[request-rdv] certificate accepted, endorsement valid
[request-rdv] sending private routing instruction (target: 2288fdd0f44a27c7...)
[node-a] 127.0.0.1:55630: routing instruction accepted for hs_hash 36c8680c... until 1783118323 (target kept private)
[request-rdv] routing instruction accepted

step 2: save the certificate and re-verify it completely standalone
hs_hash:        36c8680cf47c3a852dac0b0764aa8d1daa9c4a55ff5799af26faf94d3a24531f
rdv_node_hash:  5c328142b1d0c7ca87fe8627e1c533da41e08a52a6c09dcd70ca2d7bfa2bd53d
expires:        1783118323 (not expired)
pow effort:     800
pow valid:      true
proof valid:    true
endorsement:    valid
certificate:    VALID
```

# Implementation

A certificate (issue #67) lets a hidden service (HS) destination authorize a node to act as its rendezvous (RDV) node, up until an expiration timestamp and proved to have paid via a solved PoW challenge, without ever revealing the HS's actual identity to that node, only the public hash of it. The certificate carries a halo2 zero-knowledge proof of "the party that signed this message knows the private key behind hash H", so the RDV node (and later, any client) can verify the grant is authentic without learning who H actually belongs to.

#### Module layout

| File | What it proves / does |
|---|---|
| `dlog.rs` | Constraint 1: knowledge of `sk` such that `pk = [sk]G`, using a from-scratch `FixedPoints` impl on the pallas curve generator |
| `schnorr.rs` | Constraint 3: a Schnorr signature `(R, s)` over a message verifies under `pk`, entirely inside the circuit, without ever revealing `pk` |
| `circuit.rs` | Combines constraint 1, Poseidon(pk) == hs_hash (constraint 2), and the Schnorr check into one circuit, over the public instance `[hs_hash, rdv_node_hash, expires, pow_challenge]` |
| `equix_pow.rs` | PoW challenge/solve/verify, copied from PoC 11 for simplicity, same scheme (Equi-X + BLAKE2b outer difficulty check) |
| `certificate.rs` | `Certificate` struct, `build_certificate` (prove), `verify_certificate` (pure, stateless, repeatable verify), plus the shared prove/verify plumbing both artifact types go through |
| `routing.rs` | The private routing instruction (option B): `build_routing_instruction` / `verify_routing_instruction`, reusing the certificate circuit with a domain-separated instance |
| `main.rs` | Node A / Node B CLI plus the full acceptance checks (`accept_certificate`: PoW freshness + hash match + expiry + proof; `accept_instruction`: hash matches + proof) and duration-based challenge pricing (`required_effort`) |

#### Certificate contents

```
hs_hash        32 bytes   H = Poseidon(pk.x, pk.y), the public HS identifier
rdv_node_hash  32 bytes   which node this grant authorizes
expires         8 bytes   unix timestamp, public and unencrypted
pow_challenge  16 bytes   the Equi-X challenge Node A issued
pow_solution   24 bytes   the Equi-X solution Node B found
proof          variable   the halo2 ZK proof
endorsement    96 bytes   the RDV node's counter-signature (rdv_pk, sig R, sig s), attached after acceptance
```

#### Routing instruction contents (private, never published)

```
hs_hash           32 bytes   same H as the certificate's
rdv_node_hash     32 bytes   the RDV node being instructed (Node A)
target_node_hash  32 bytes   where to route H's traffic (Node C); the secret this artifact exists to protect
proof             variable   halo2 proof over [hs_hash, rdv_node_hash, target_node_hash, ROUTE_DOMAIN]
```

#### RDV endorsement

Issue thread mentioned: "the certificate also needs to include that the RDV agreed to become a RDV node in the first place", an accepted certificate carries the RDV node's own counter-signature, not just a session ACK. After `accept_certificate` passes, Node A signs `Poseidon(hs_hash, rdv_node_hash, expires, pow_challenge)` (a 4-ary Poseidon, domain-separated by arity from the 2-ary identity hash and the 3-ary envelope message) with its identity key and returns `(rdv_pk, R, s)` alongside the ACK. Unlike the hidden service's side this needs no ZK: an RDV node's identity hash `Poseidon(rdv_pk)` is public by design, so it can reveal `rdv_pk` and sign plainly. `verify_endorsement` checks that the revealed `rdv_pk` actually hashes to the certificate's `rdv_node_hash` before checking the signature, so only the named node can endorse. Node B verifies the endorsement on receipt and discards it if invalid.

The circuit proves, over public inputs `(hs_hash, rdv_node_hash, expires, pow_challenge)`: (1) knowledge of `sk` such that `pk = [sk]G`, (2) `hs_hash == Poseidon(pk.x, pk.y)`, and (3) a Schnorr signature over `m = Poseidon(rdv_node_hash, expires, pow_challenge)` verifies under `pk`. Binding `rdv_node_hash`, `expires`, and `pow_challenge` into the signed message is what makes tampering with any of them after issuance break the proof.

#### Private routing instruction

A certificate deliberately does NOT say where traffic should actually be routed. Binding the target into the shareable certificate would leak Node C to any third party who later reads it from a directory/DHT; So here we implement a second, private, ZK-signed artifact.

After Node A's endorsement comes back, Node B sends a `RoutingInstruction` on the same session: "route traffic for `hs_hash` to `target_node_hash`". It reuses the certificate circuit and proving key verbatim, with the public instance `[hs_hash, rdv_node_hash, target_node_hash, ROUTE_DOMAIN]` in place of the certificate's `[hs_hash, rdv_node_hash, expires, pow_challenge]`. `ROUTE_DOMAIN` is a fixed constant equal to `2^192 + ascii("route_1")`: a certificate's fourth slot is a u128 `pow_challenge` and can never reach that value, so neither proof type can be replayed as the other even though they share a circuit (tested both directions in `routing.rs`).

Node A accepts an instruction only if its `hs_hash` equals the hash of the certificate it just endorsed, its `rdv_node_hash` names Node A itself, and the proof verifies. (only the holder of the key behind the granted hash could have produced it) The target then goes into Node A's local routing table (`RdvEntry.route_target`) and nowhere else: it is never logged in full, never part of the certificate, and never shared. Verifying the instruction reveals nothing about the HS beyond what the certificate already exposed.

#### PoW (Equi-X), priced by requested duration

Reused from what I did for PoC 11 (which reused it from PoC 4.5 / 4.9), with one fix: the salt arithmetic in `solve_challenge` is now wrapping (the original overflows u64 about half the time with 2+ threads, panicking in debug builds; PoC 11 got the same fix).

The challenge difficulty now scales with the grant being bought: Node B declares its desired `expires` inside the RDV request, and Node A prices the challenge at `MIN_CHALLENGE_DIFFICULTY` (800) per started day of requested lifetime (`required_effort`), so a 30-day grant costs 30× the work of a 1-day grant. `accept_certificate` then requires `cert.expires` to equal exactly the expiry that was priced: a certificate claiming any other lifetime wasn't what was paid for. Difficulty 800 solves in tens of milliseconds on a single thread; 24000 (30 days) in the low seconds.

The 30-day cap (`MAX_CERT_LIFETIME_SECS`) stays even with pricing: Equi-X solve time scales linearly, so a years-long grant would need one absurd challenge, and renewal-by-expiry is the intended mechanism anyway.

# Design notes / tradeoffs

Read these before extending this PoC.

- (!) New identity scheme, scoped to this PoC only. The rest of the network uses SHA-family hashes over Ed25519 keys (`.dn` addresses). Neither Ed25519 nor SHA-256 have practical ZK-circuit support, so this PoC uses its own scheme: a keypair on halo2's native pallas curve, with the public hash `H = Poseidon(pk.x, pk.y)`. This mirrors what was already agreed in the issue thread (Ristretto255 + Poseidon), just swapped onto halo2-native primitives so the same keys work inside the circuit. Reconciling this with the network's real `.dn` scheme is an open follow-up.
- halo2 over Ristretto255/bulletproofs or arkworks/Groth16. halo2 is published, actively maintained, and needs no trusted-setup ceremony. The R1CS gadget API needed to do this with `bulletproofs` + Ristretto255 is experimental and unpublished (`yoloproofs`, off crates.io). arkworks/Groth16 is published but needs a per-circuit trusted-setup ceremony, not helping here in aa basically "no trust" network.
- In-circuit Schnorr verification has no Orchard precedent. Orchard's own spend authorization is checked outside it's circuit (a plain signature at the transaction level) because it doesn't need to hide the signing key. This PoC does, so the verification equation is checked entirely in-circuit. (see `schnorr.rs`'s module doc for the specific gadget sequence).
- Proof size is NOT optimized. halo2's no-trusted-setup property (IPA-based) is a trade off with Groth16's constant ~200-byte proofs; halo2 proofs here run several KB. Although we want certificates as small as possible, since potentially millions could be stored network-wide, this is intentionally deferred to a follow-up PoC.
- `verify_certificate` only checks the proof. PoW freshness, hash match, and expiry are Node A's own business logic (`accept_certificate` in `main.rs`), kept separate so `verify_certificate` stays a pure function anyone can call later, independent of the original handshake. `accept_certificate` runs (in essence) 'cheap checks' (hash, expiry, PoW) before the expensive ZK proof verification, so a peer that never solved the PoW can't burn Node A's CPU on proof verification with faked certificates.
- Standalone verifiers must enforce a minimum PoW effort themselves. The effort is embedded in `pow_challenge`'s low 32 bits and is whatever the issuing node chose; a colluding HS + RDV pair could mint a certificate against a trivial-effort challenge. `verify_cert_file` therefore rejects certificates whose embedded effort is below `CHALLENGE_DIFFICULTY`; this would be a network-wide constant.
- The 3-ary Poseidon uses (certificate envelope `m = Poseidon(rdv_hash, expires, challenge)`, routing envelope `m = Poseidon(rdv_hash, target, ROUTE_DOMAIN)`, and Schnorr challenge `e = Poseidon(R.x, pk.x, m)`) share a hash domain (`ConstantLength<3>` encodes only arity, not purpose). The certificate/routing envelopes are separated from *each other* by the `ROUTE_DOMAIN` constant in slot 3, which no u128 `pow_challenge` can equal. Tested in both directions in `routing.rs`.
- The routing instruction carries no expiry of its own: the route lives and dies with the certificate that authorized the RDV grant, and Node A ties the two together at acceptance time (`accept_instruction` requires the instruction's `hs_hash` to match the endorsed certificate's). Since the instruction is private to the B→A session, there is no third-party replay surface to bind an expiry against.

# References
- [Issue #67: HS routing-delegation certificates](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/67)
- [Issue #117: PoC 10.1 encrypted HS descriptor](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/117) (included and guided some descisions to allow easier future development)
- [PoC 11: Hash-based routing rules](../11-routing-rules/)
- [PoC 4.5: Equi-X multi-threaded PoW](../4.5-pow-equix-threaded/)
- [PoC 8: Hashring + closest neighbor lookup](../8-hashring-neighbors/)
- [halo2](https://zcash.github.io/halo2/) [CLEARNET LINK]

# Notes for future reference and implementation
- PoW price vs. grant duration: Node B declares its desired `expires` in the RDV request and Node A prices the challenge from it (`required_effort`, linear per started day). Standalone verifiers can't re-derive the exact price (a certificate doesn't record when it was issued), so they can only enforce the one-day floor (`MIN_CHALLENGE_DIFFICULTY`); if the network wants third parties to check full duration pricing too, an `issued_at` field bound into the circuit's public instance would be needed. Feedback for solutions here is welcome
- Not concerning myself with low dependency count currently, just working towards a functional version.
- (!) One major dependency (`halo2_gadgets`) is relatively new and needs security research and ongoing verification. A critical vulnerability was disclosed in mid-2026. Nothing here should be treated as fully secure cryptography without proper review of the circuit.
- Identity scheme mismatch: `pow_challenge` / `expires` / hash fields are encoded as raw pallas field elements (`Fp`). An externally-supplied 32-byte hash must already be a canonical `Fp` encoding, which is true for hashes produced by this PoC's own scheme but not guaranteed for arbitrary SHA-family hashes potentially elsewhere in the network. This needs connecting with PoC 8's `.dn` hashring positions.
- Proof size optimization (see design notes) is not focused on. A follow-up should measure actual certificate size and decide whether the DHT (#65) is needed to store them.
- Trusted setup / parameter distribution: this PoC regenerates halo2 params and the proving key on first use. A real deployment would generate them once at network creation and distribute them, so nodes do not each pay that cost. (Potentially, could leave them to generate each for a node, which would discourage node creation, but make botting slightly more time and resource consuming)
- The private routing instruction ("route H's traffic to Node C") is now implemented (`routing.rs` + the `MSG_ROUTE` phase). Actual packet forwarding along the installed route remains PoC 11 (#71) / 11.1 (#157) scope: PoC 11.1's `MSG_REGISTER` becomes "submit certificate + instruction" and the rule payload comes from `RdvEntry.route_target`.

`README.md` generated from custom text editor