# PoC 10, explained

A walkthrough of *how* this PoC works, for if you can read code and know
the basics (public/private keys, hashes, signatures) but haven't worked with
zero-knowledge proofs or in-circuit elliptic-curve arithmetic before.

For build/run instructions, the exact wire encodings, and the design tradeoffs, see
`README.md`. This file is the conceptual model behind those.

---

## 1. The problem

A hidden service (HS) never talks to clients directly; a **rendezvous (RDV) node** relays
on its behalf. We need a certificate authorizing that relay; the RDV node can present it,
and anyone can check it. The certificate asserts:

> Hidden service `H` authorizes node `N` to relay its traffic until `T`, having paid for
> the slot with a solution to proof-of-work challenge `C`.

`H`, `N`, `T`, `C` are all public. If that were it, an Ed25519 signature over
`(N, T, C)` by the HS key would work, and you'd verify it against the HS's public key.

**`H` is a hash of the HS's key, and the key itself must never appear.** 
Publishing the key would deanonymize the servic. The whole point is that not even `N` 
learns which HS it relays for, only the opaque hash `H`.
But signature verification *requires* the public key, and you can't recover it from `H`.

So our issue is:
- verifying a signature needs the public key;
- anonymity means you can only publish `Poseidon(key)`.

A zero-knowledge proof resolves it: prove *"I hold the key behind `H` and signed `(N, T, C)` 
with it"* while revealing nothing but `H` itself.

One asymmetry shapes the whole design: only the **HS** needs hiding. The RDV node `N` is
public by construction (`N = Poseidon(N's key)`), so `N`'s own consent to the arrangement
is a plain signature, not a proof. Section 5 covers that half.

---

## 2. Zero-knowledge proofs

A ZKP proves a statement without revealing the witness that makes it true. Here the
statement is:

> "There exists `sk` with `pk = [sk]G`, `H = Poseidon(pk.x, pk.y)`, and a Schnorr
> signature over `Poseidon(N, T, C)` that verifies under `pk`."

`sk` and `pk` are the private witness; `H`, `N`, `T`, `C` are the public instance. The
verifier learns only that *some* valid witness exists, but nothing about `sk` or `pk`.

### The circuit

A proof system encodes the statement as an arithmetic circuit: constraints (adds and
multiplies over a finite field) that are simultaneously satisfiable iff the statement
holds. Cells are either **advice** (private witness the prover fills) or **instance**
(public values the verifier also holds). The prover assigns every cell and emits a proof;
the verifier, holding only the instance and the proof, checks a satisfying assignment
existed. Soundness: you can't produce a valid proof without actually knowing such a
witness.

We build on **halo2** (`halo2_proofs` + `halo2_gadgets`, from Zcash). We don't implement
the proof system, we implement the *circuit*: the specific constraints for our statement.
Two halo2 properties that drove decisions here:

- **No trusted setup.** halo2 uses an IPA-based commitment scheme, so there's no
  ceremony whose leakage would let someone forge proofs. Making it appropriate for a
  trustless network. Groth16 would be smaller but needs a per-circuit trusted setup.
- **Larger proofs as the tradeoff.** These proofs run several KB versus Groth16's ~200-byte
  constant. With potentially millions of certificates stored network-wide, proof-size
  optimization is important, but not addressed here.

---

## 3. The primitives

Everything is on **Pallas**, because halo2's in-circuit arithmetic is native to it.
Ed25519 (what the rest of the network uses for addresses) is prohibitively expensive to
express as constraints, and that forces the primitive choices below.

**Keypair: `pk = [sk]G`.** `sk` a scalar, `pk` a curve point, `G` the fixed generator;
one-way as usual (`sk → pk` easy, `pk → sk` infeasible). Proving knowledge of `sk` for a
given `pk` (constraint 1) is a discrete-log relation. `dlog.rs` implements it with a
from-scratch `FixedPoints` config on the Pallas generator.

**Hash: Poseidon.** `H = Poseidon(pk.x, pk.y)` (a point is two coordinates; both are
absorbed). Poseidon over SHA-256 because SHA's bitwise structure blows up into tens of
thousands of constraints, while Poseidon is arithmetic-native and cheap in-circuit. Has the same
one-way property, but is a fraction of the cost.

A caveat that surfaces in the code: Poseidon consumes **field elements**, not arbitrary
bytes. Every value fed to it (`H`, `N`, `T`, `C`) must already be a canonical field
encoding. That holds automatically for values this scheme produces (they're Poseidon
outputs), but not for an arbitrary 32-byte SHA hash from elsewhere: a chunk of random
32-byte strings aren't valid Pallas field elements. This is why the
network adopted `Poseidon(pk)` as the canonical identifier (section 9): every identifier
is a valid field element by construction.

**Signature: Schnorr.** Verification is `[s]G = R + [e]pk`, pure point arithmetic, which
is what makes checking it *inside* the circuit feasible. Signature is `(R, s)`. Doing the
check in-circuit is the unusual part: you'd normally verify a signature in the clear, but
here the verifying key `pk` is exactly what we're hiding, so the check has to sit in the
circuit alongside the dlog and hash constraints. (Zcash's Orchard verifies spend
authorizations *outside* its circuit for the opposite reason: it doesn't hide the key.
`schnorr.rs`'s module doc gives the gadget sequence.)

### The other keypair

Pallas solves the circuit problem and creates an addressing issue. A user is given a `.dn`
address, which is an **Ed25519** key in base32; the certificate names the HS by `H`, a
Pallas hash. Nothing so far connects the two, so a client holding an address cannot tell
which certificates are its own.

But a principal holds **both** keypairs and signs each with the other:

```
Ed25519 key ──signs──> Pallas public key
Pallas key  ──signs──> Ed25519 public key
```

Check both directions and you know one party holds both secrets (the two keys are
one principal). One direction doesn't work: an Ed25519 signature over someone else's
Pallas key is something anyone holding the address key can produce for a Pallas key they
do not control. The two together are only producible by a holder of both.

That pair of signatures plus the Pallas public key is the **address binding**
(`identity.rs`), and it is what turns an address into a hash: recover the Ed25519 key
from the address, verify both directions against the claimed Pallas key, and compute
`Poseidon(pallas_pk)`. A directory that hands you a binding cannot swap in its own Pallas
key, because it would have to forge an Ed25519 signature under a key it does not hold.

**The binding is not part of the certificate.** The Ed25519 public key *is* the address,
so putting the binding in the artifact the RDV node receives would hand it the identity
the whole proof exists to hide. So the two artifacts have different audiences: the
certificate is public and says only `H`; the binding goes only to parties already told
the address. That split is what lets a client verify what an RDV node cannot.

Be careful about how far that goes. `H` is a fixed function of the identity, so the
address --> `H` direction is open to anyone with the binding, and anyone who knows an
address can get its binding. A RDV node therefore cannot go *backwards* from a certificate
to an address it never knew, but it can go *forwards*: compute `H` for an address it
wants to block and refuse that hash forever. Hiding the key stops profiling, not a censor
working from a list. The README's "What this does not block" section covers why fixing
it needs a per-epoch identifier rather than a fixed one.

---

## 4. What the proof binds together

Public certificate fields:

```
hs_hash        H, the HS's public identifier (Poseidon(pk.x, pk.y))
rdv_node_hash  N, the authorized relay node
issued_at      I, when the grant was issued (unix seconds)
expires        T, authorization deadline (unix seconds)
pow_challenge  C, the Equi-X challenge Node A issued
pow_solution   the solution to C
proof          the halo2 proof over instance (H, N, T, C)
endorsement    N's counter-signature over (H, N, I, T, C), attached on acceptance (section 5)
```

The circuit enforces three constraints over the public instance `(H, N, T, C)`:

1. **Key knowledge** `∃ sk. pk = [sk]G`. (`dlog.rs`)
2. **Hash binding** `H = Poseidon(pk.x, pk.y)`, tying the opaque `H` to that specific
   hidden `pk` so nobody can claim an `H` they don't control. (`circuit.rs`)
3. **Grant signature** a Schnorr signature over `m = Poseidon(N, T, C)` verifies under
   `pk`. (`schnorr.rs`)

All three are proven jointly while only `(H, N, T, C)` is exposed, so a verifier concludes
"the holder of `H` authorized `N` until `T`, having paid `C`" and learns nothing else.
Folding `N`, `T`, `C` into `m` is what makes the grant tamper-evident: altering any of them
invalidates the signature, hence the proof.

### The proof-of-work fields

`C`/`pow_solution` prove the HS actually paid for relay requests. The
scheme is **Equi-X** (shared with PoCs 4.5 / 4.9 / 11, including the wrapping-salt fix for
the multi-threaded overflow); the certificate just carries challenge and solution so anyone can re-check the work.

Two checks the proof itself doesn't cover, which a standalone verifier must enforce:

- **Minimum effort.** Difficulty is embedded in `C`. A colluding HS + RDV pair could mint a
  certificate against a trivial challenge, so verifiers reject any embedded effort below a
  network-wide floor.
- **Duration pricing.** The HS declares its desired `T` inside the RDV request, and the
  challenge is priced from it: one day's difficulty per started day of requested lifetime,
  linearly (a 30-day grant costs 30× a 1-day grant's work). Acceptance then requires the
  certificate's `T` to equal exactly what was priced. The certificate also records `issued_at`
  (`I`), bound into `N`'s endorsement, so a third party can re-derive the full price
  `required_effort(T - I)` and reject an under-paid grant; the one-day floor remains only for
  degenerate windows. Since `I` must not lie in the future, a far-off `T` forces a proportionally
  large window, so a colluding HS+RDV pair can't buy a long grant at a short grant's price.
- **Lifetime cap.** Even priced, the issuing node refuses `expires` more than 30 days out;
  renewal-by-expiry is the intended long-term mechanism, not one enormous challenge.

---

## 5. The relay's endorsement

The proof covers the HS's authorization. It says nothing about whether `N` *agreed* to
relay, and without that you could forge a certificate naming a node that never consented.

On acceptance, Node A counter-signs. The `endorsement` field is a **plain** Schnorr
signature, not a proof, because `N`'s identity is public: `N = Poseidon(A's pk)`, so A can
reveal its key and sign in the clear. (`endorse_certificate` / `verify_endorsement`.)

- Node A signs `Poseidon(H, N, T, C)` with its identity key and attaches `(rdv_pk, R, s)`.
- Verification checks `Poseidon(rdv_pk) == N` **before** checking the signature. Only the node 
  named by `N` holds a key hashing to `N`, so only it can endorse.

Domain-separation detail: the HS's in-circuit message is a 3-input `Poseidon(N, T, C)`; the
endorsement digest is a 4-input `Poseidon(H, N, T, C)`. Poseidon distinguishes by arity, so
the two signatures can't be cross-replayed. (`README.md` flags where arity separation is and
isn't sufficient. Two unrelated 3-input uses share a domain.)

So a complete certificate carries two authorizations: a ZK one from the anonymous HS, and a
plain one from the public relay.

---

## 6. The private routing instruction

The certificate says "N may relay for H until T". It deliberately does **not** say where N
should actually send H's traffic. Here, we choose to send the target as a second,
private, equally-authenticated message. This PoC implements it (`routing.rs`).

After B receives A's endorsement, it sends a `RoutingInstruction` on the same session:

```
hs_hash           the same H as the certificate
rdv_node_hash     N, the node being instructed
target_node_hash  C, where H's traffic should actually go (the secret)
proof             halo2 proof over (H, N, C, ROUTE_DOMAIN)
```

The authenticity requirement is identical to the certificate's: only the holder of the key
behind `H` may issue it, without revealing that key, so it reuses the *same circuit and
proving key*, just with a different public instance: `(H, N, C, ROUTE_DOMAIN)` instead of
`(H, N, T, C_pow)`. `ROUTE_DOMAIN` is a fixed constant above 2^192; a certificate's fourth
slot holds a 128-bit PoW challenge and can never reach it, so a certificate proof can't be
replayed as an instruction or vice versa (both directions are tested).

A accepts the instruction only if it names A itself, its `H` matches the certificate A just
endorsed (so an instruction can't attach to someone else's grant. A forger's own `H`
can never match, because the circuit binds `H` to the signing key), and the proof verifies.
The target then lives solely in A's local routing table, expiring with the certificate. It
is never published or logged in full.

---

## 7. halo2

The only non-obvious things in the code are halo2-specific:

- **`Value<T>`** wraps a cell whose value may be unknown in a given pass. During proving it
  holds the real witness; during keygen it's `unknown()`, since key generation needs only
  the circuit *shape*, not concrete secrets.
- **`into_option()` on the verify path**, decoding bytes back to a curve point or field
  element can fail (not every 32-byte string is a valid point/element), so decode returns an
  `Option` and a `None` is treated as "reject certificate" rather than a panic.
- **`LazyLock`** memoizes the expensive one-time setup (params + proving/verifying keys)
  so it's computed once per process.

---

## 8. End-to-end flow

`test` runs this in one process; the CLI splits Node A and Node B across machines.

**Setup (cached).** `certificate.rs` generates halo2 params and proving/verifying keys for
the circuit. (slow, done once per process via `LazyLock`)

**1. Challenge.** Node A (candidate relay) receives an RDV request from Node B (the HS)
declaring the desired `T`, prices the challenge from the requested lifetime
(`required_effort`), and replies with a fresh Equi-X challenge `C`. (`handle_node_a`.)

**2. Solve.** Node B solves `C`. (`solve_challenge`, `equix_pow.rs`.)

**3. Build.** Node B, holding `sk`: derives `H = Poseidon(pk.x, pk.y)`, signs `m =
Poseidon(N, T, C)`, runs the halo2 prover to produce `proof`, and sends the `Certificate`.
`sk`/`pk` never leave Node B. Only `H` and the proof go on the wire. (`build_certificate`.)

**4. Accept.** Node A runs `accept_certificate`, cheap checks first so an unpaid peer can't
force an expensive proof verification:
- `T` equals exactly the expiry the challenge was priced for;
- `I` (issued_at) sits at the present within a small skew, so the certificate's own window
  (`T - I`) matches what A priced and a later standalone verifier will accept;
- PoW solution matches the challenge issued this session (replay freshness) at the priced
  effort;
- `N` equals A's own hash;
- not expired, and within the 30-day cap;
- then, and only then, `verify_certificate` (the ZK check).

**5. Endorse.** On success A counter-signs (section 5) and returns the endorsement with its
ACK; B verifies it and drops it if bad. The certificate is now complete.

**6. Instruct.** Holding confirmation that A agreed, B sends the private routing
instruction (section 6) on the same session; A checks it against the just-endorsed
certificate (`accept_instruction`) and stores the target in its local table only.

**7. Re-verify later.** Proof and endorsement stay attached. Any client fetching the
certificate from a directory later can run `verify_certificate` + `verify_endorsement`
statelessly. With no contact with A or B, and no session context. That stateless re-verifiability
is what PoC 10.1 and 11.1 build on. (The routing instruction is *not* part of this: it
stays private between B and A by design.)

**7a. Expire.** Nothing ends the grant; it runs out. Verifiers read `expires` off the certificate it holds and calls it invalid past that point. No revocation message to distribute and nothing to fetch before trusting one. Expired entries are cleared out on a timer (`prune_expired`), which also discards the private routing target.

**8. Resolve, if you know the address.** A client that was given B's `.dn` address does one
more step A can't: `resolve_hs_hash(address, binding)` produces `H` from the address, and
matching it against the certificate's `hs_hash` shows that this certificate is the
one for the service. The `test` command runs this twice, once with the
right address and once with an unrelated one.

---

## 9. Scope and consumers

This PoC is certificate production and verification. Its neighbors:

- **PoC 11.1: routing rules gated by certificates (#157).** The direct consumer. PoC 11
  currently lets any PoW-paying node install a routing rule for any hash; 11.1 requires a
  valid certificate whose `H` matches the registered hash and whose `N` is the registering
  node. 11.1's rule payload comes from the instruction's `target_node_hash`.
- **PoC 10.1: encrypted descriptor (#117).** Takes the binding from here and solves the
  distribution problem this PoC leaves open: how a client obtains a binding for an
  address without the directory serving it learning which hidden service is being looked
  up. `address.rs` and `identity.rs` are meant to be lifted into it as-is.

On identity: the Pallas key here isn't PoC-local anymore. Every principal has two
cross-signed keypairs (section 3): Ed25519 for `.dn` addresses / E2EE / plain
signatures, and Pallas for these certificates. The canonical hashring position is
`Poseidon(pallas_pk.x, pallas_pk.y)`, i.e. the `H` this PoC computes is the network
identifier used by the hashring (PoC 8), routing tables (PoC 11), and certificate fields
alike.

Two gaps remain. The binding has to reach the client
somehow, and handing it out in the clear reintroduces the correlation the encryption in
10.1 removes. Keys are also generated fresh per run and held only in memory, so a `.dn`
address does not survive a restart.

Explicitly out of scope: actual packet relaying (PoC 11), the descriptor (PoC 10.1), and
any proof-size/proving-time optimization. Also note `halo2_gadgets` is relativley new
and requires ongoing review.

AI was used very minimally to aid me with technical language rewriting in this file. More specifically, for halo2 bullet points and editing on the certificate contents section.

`EXPLAINER.md` generated from custom text editor