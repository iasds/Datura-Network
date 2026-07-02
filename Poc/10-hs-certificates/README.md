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
./target/release/hs-certificates request-rdv <node-a-addr> <node-a-hash-hex> [expires-unix-ts]
```
Connects to Node A, solves its Equi-X PoW challenge, builds a certificate (a fresh random HS identity is generated per run), and submits it. `expires` defaults to 24h from now if omitted.

#### Verify a certificate standalone
```
./target/release/hs-certificates verify <cert-file>
```
Loads a certificate from a JSON file and independently re-checks everything a third party can check from the certificate alone: the ZK proof, the Equi-X PoW solution against the embedded challenge (proof of payment), current-time expiry (automatic revocation), and the RDV node's endorsement signature. No connection to Node A, no knowledge of the original handshake required. This is the property PoC 10.1 (#117) needs: the proof must stay attached and stay checkable by anyone, not be verified once and discarded.

#### Self-contained test
```
./target/release/hs-certificates test
```
Runs Node A and Node B in-process: RDV request, challenge, PoW solve, certificate build, submit, accept, then saves the certificate to a temp file and re-verifies it standalone, demonstrating the full flow without separate terminals.

# Output
#### test
```
[node-a] identity hash: 0ba8fcd3d42bb9a1324b22aecdf7af12e4cbc5e7f0d8e29e8156b232a9786c35
[node-a:9120] listening

step 1: Node B requests RDV status and submits a certificate
[request-rdv] connecting to node-a at 127.0.0.1:9120
[node-a] 127.0.0.1:52051: RDV request, sent challenge (difficulty 800)
[request-rdv] challenge received (difficulty 800), solving...
[request-rdv] solved, building certificate...
[request-rdv] hidden service hash: efdb8d3a0e07b055d94e9a284bf8b58b749ce2869c1e29ffdf3234b4e5888b14
[node-a] 127.0.0.1:52051: certificate accepted, agreed to route for hs_hash efdb8d3a... until 1783028733
[request-rdv] certificate accepted

step 2: save the certificate and re-verify it completely standalone
hs_hash:       efdb8d3a0e07b055d94e9a284bf8b58b749ce2869c1e29ffdf3234b4e5888b14
rdv_node_hash: 0ba8fcd3d42bb9a1324b22aecdf7af12e4cbc5e7f0d8e29e8156b232a9786c35
expires:       1783028733
proof valid:   true
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
| `certificate.rs` | `Certificate` struct, `build_certificate` (prove), `verify_certificate` (pure, stateless, repeatable verify) |
| `main.rs` | Node A / Node B CLI plus the full acceptance check (`accept_certificate`: PoW freshness + hash match + expiry + proof, on top of `verify_certificate`'s proof-only check) |

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

#### RDV endorsement

Issue thread mentioned: "the certificate also needs to include that the RDV agreed to become a RDV node in the first place", an accepted certificate carries the RDV node's own counter-signature, not just a session ACK. After `accept_certificate` passes, Node A signs `Poseidon(hs_hash, rdv_node_hash, expires, pow_challenge)` (a 4-ary Poseidon, domain-separated by arity from the 2-ary identity hash and the 3-ary envelope message) with its identity key and returns `(rdv_pk, R, s)` alongside the ACK. Unlike the hidden service's side this needs no ZK: an RDV node's identity hash `Poseidon(rdv_pk)` is public by design, so it can reveal `rdv_pk` and sign plainly. `verify_endorsement` checks that the revealed `rdv_pk` actually hashes to the certificate's `rdv_node_hash` before checking the signature, so only the named node can endorse. Node B verifies the endorsement on receipt and discards it if invalid.

The circuit proves, over public inputs `(hs_hash, rdv_node_hash, expires, pow_challenge)`: (1) knowledge of `sk` such that `pk = [sk]G`, (2) `hs_hash == Poseidon(pk.x, pk.y)`, and (3) a Schnorr signature over `m = Poseidon(rdv_node_hash, expires, pow_challenge)` verifies under `pk`. Binding `rdv_node_hash`, `expires`, and `pow_challenge` into the signed message is what makes tampering with any of them after issuance break the proof.

#### PoW (Equi-X)

Reused from what I did for PoC 11 (which reused it from PoC 4.5 / 4.9), with one fix: the salt arithmetic in `solve_challenge` is now wrapping (the original overflows u64 about half the time with 2+ threads, panicking in debug builds; PoC 11 got the same fix). Difficulty 800 solves in tens of milliseconds on a single thread. Node A checks that the certificate's `pow_challenge` is exactly the one it issued this session and that `pow_solution` actually solves it, so a solution for some other challenge cannot be replayed.

Because the PoW price is flat, Node A caps how far out it will endorse: `accept_certificate` rejects `expires` more than 30 days (`MAX_CERT_LIFETIME_SECS`) in the future, so one solve cannot buy RDV status for decades. See future-reference notes for alternative.

# Design notes / tradeoffs

Read these before extending this PoC.

- (!) New identity scheme, scoped to this PoC only. The rest of the network uses SHA-family hashes over Ed25519 keys (`.dn` addresses). Neither Ed25519 nor SHA-256 have practical ZK-circuit support, so this PoC uses its own scheme: a keypair on halo2's native pallas curve, with the public hash `H = Poseidon(pk.x, pk.y)`. This mirrors what was already agreed in the issue thread (Ristretto255 + Poseidon), just swapped onto halo2-native primitives so the same keys work inside the circuit. Reconciling this with the network's real `.dn` scheme is an open follow-up.
- halo2 over Ristretto255/bulletproofs or arkworks/Groth16. halo2 is published, actively maintained, and needs no trusted-setup ceremony. The R1CS gadget API needed to do this with `bulletproofs` + Ristretto255 is experimental and unpublished (`yoloproofs`, off crates.io). arkworks/Groth16 is published but needs a per-circuit trusted-setup ceremony, not helping here in aa basically "no trust" network.
- In-circuit Schnorr verification has no Orchard precedent. Orchard's own spend authorization is checked outside it's circuit (a plain signature at the transaction level) because it doesn't need to hide the signing key. This PoC does, so the verification equation is checked entirely in-circuit. (see `schnorr.rs`'s module doc for the specific gadget sequence).
- Proof size is NOT optimized. halo2's no-trusted-setup property (IPA-based) is a trade off with Groth16's constant ~200-byte proofs; halo2 proofs here run several KB. Although we want certificates as small as possible, since potentially millions could be stored network-wide, this is intentionally deferred to a follow-up PoC.
- `verify_certificate` only checks the proof. PoW freshness, hash match, and expiry are Node A's own business logic (`accept_certificate` in `main.rs`), kept separate so `verify_certificate` stays a pure function anyone can call later, independent of the original handshake (a PoC 10.1 / #117 requirement). `accept_certificate` runs (in essence) 'cheap checks' (hash, expiry, PoW) before the expensive ZK proof verification, so a peer that never solved the PoW can't burn Node A's CPU on proof verification with faked certificates.
- Standalone verifiers must enforce a minimum PoW effort themselves. The effort is embedded in `pow_challenge`'s low 32 bits and is whatever the issuing node chose; a colluding HS + RDV pair could mint a certificate against a trivial-effort challenge. `verify_cert_file` therefore rejects certificates whose embedded effort is below `CHALLENGE_DIFFICULTY`; this would be a network-wide constant.
- The two 3-ary Poseidon uses (envelope message `m = Poseidon(rdv_hash, expires, challenge)` and Schnorr challenge `e = Poseidon(R.x, pk.x, m)`) share a domain (`ConstantLength<3>` encodes only arity, not purpose).

# References
- [Issue #67: HS routing-delegation certificates](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/67)
- [Issue #117: PoC 10.1 encrypted HS descriptor](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/117) (included and guided some descisions to allow easier future development)
- [PoC 11: Hash-based routing rules](../11-routing-rules/)
- [PoC 4.5: Equi-X multi-threaded PoW](../4.5-pow-equix-threaded/)
- [PoC 8: Hashring + closest neighbor lookup](../8-hashring-neighbors/)
- [halo2](https://zcash.github.io/halo2/) [CLEARNET LINK]

# Notes for future reference and implementation
- PoW price vs. grant duration: currently a flat difficulty plus a hard 30-day cap on `expires` (`MAX_CERT_LIFETIME_SECS` in `main.rs`). Goal is to scale challenge difficulty with the requested duration, but the challenge is issued before Node A learns the requested expiry, so that needs a protocol change: Node B would send its desired `expires` in the RDV request and Node A would price the challenge from it. Standalone verifiers can't enforce the cap (a certificate doesn't record when it was issued), so it remains the issuing node's policy; if the network wants third parties to check it too, an `issued_at` field bound into the circuit's public instance would be needed. Feedback for solutions here is welcome
- Not concerning myself with low dependency count currently, just working towards a functional version.
- (!) One major dependency (`halo2_gadgets`) is relatively new and needs security research and ongoing verification. A critical vulnerability was disclosed in mid-2026. Nothing here should be treated as fully secure cryptography without proper review of the circuit.
- Identity scheme mismatch: `pow_challenge` / `expires` / hash fields are encoded as raw pallas field elements (`Fp`). An externally-supplied 32-byte hash must already be a canonical `Fp` encoding, which is true for hashes produced by this PoC's own scheme but not guaranteed for arbitrary SHA-family hashes potentially elsewhere in the network. This needs connecting with PoC 8's `.dn` hashring positions.
- Proof size optimization (see design notes) is not focused on. A follow-up should measure actual certificate size and decide whether the DHT (#65) is needed to store them.
- Trusted setup / parameter distribution: this PoC regenerates halo2 params and the proving key on first use. A real deployment would generate them once at network creation and distribute them, so nodes do not each pay that cost. (Potentially, could leave them to generate each for a node, which would discourage node creation, but make botting slightly more time and resource consuming)
- "Node A tells Node B what to route to Node C" step (actual private routing instructions) is out of scope here. This PoC covers the RDV-agreement handshake only. Packet routing itself is PoC 11 (#71). (Should mostly be seamless connecting with PoC 11 (#71) however!)

`README.md` generated from custom text editor