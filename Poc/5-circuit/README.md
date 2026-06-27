# PoC 5 — 3-Hop Circuit Building with Hidden Service Rendezvous

Implements the circuit construction protocol specified in
`spec/formal_spec/CircuitBuild.tla`.

## What this demonstrates

- **AddCircuit**: client builds a 3-hop path using CREATE/EXTEND cells.
  Each relay holds only its own DH keys and its upstream/downstream connections.
- **SelectIntroPoint**: a hidden service registers itself with an introduction point
  relay by establishing a shared circuit leg.
- **ConnectHiddenService**: full rendezvous flow — client contacts the intro point,
  HS connects back to a rendezvous relay, the RV bridges both legs.

## Wire format

Fixed 512-byte cells:

```
[type: 1 byte][circuit_id: 4 bytes LE][payload: 507 bytes]
```

| Type | Hex | Direction | Purpose |
|------|-----|-----------|---------|
| CREATE | 0x01 | client → hop | begin DH; payload = client X25519 pubkey |
| CREATED | 0x02 | hop → client | complete DH; payload = relay X25519 pubkey |
| EXTEND | 0x03 | client → hop | ask hop to extend; `[addr_len:1][addr:N][pubkey:32]` |
| EXTENDED | 0x04 | hop → client | extension done; payload = next-hop X25519 pubkey |
| RELAY | 0x05 | any direction | onion-encrypted 507-byte payload (stream cipher) |
| DATA | 0x06 | inner payload | `[len:2][data:N]` — visible after all layers peeled |
| BRIDGE | 0x07 | RV relay | link two circuit legs |
| INTRO | 0x08 | HS → intro | register rendezvous address |
| RENDEZVOUS | 0x09 | HS → RV | HS arrives; bridge to waiting client |

## Cryptography

Each hop negotiates keys via an ephemeral X25519 DH exchange:

```
shared = X25519(our_ephemeral_secret, their_ephemeral_pubkey)
okm    = HKDF-SHA256(shared, info="datura-stream-v0", len=64)
k_fwd  = okm[0:32]   // client→hop keystream key
k_bwd  = okm[32:64]  // hop→client keystream key
```

Relay cells use **ChaCha20 counter mode** (no AEAD tag) so the 507-byte payload
stays exactly 507 bytes through every hop. Each hop XORs one keystream layer
in-place; the client applies layers in reverse order before sending.

## Circuit building flow

```
Client → hop1:  CREATE [pub1]
hop1  → client: CREATED [hop1_pub]          (keys1 derived)

Client → hop1:  EXTEND [hop2_addr][pub2]
  hop1 → hop2:  CREATE [pub2]
  hop2 → hop1:  CREATED [hop2_pub]
hop1  → client: EXTENDED [hop2_pub]         (keys2 derived)

Client → hop1:  EXTEND [dest_addr][pub3]
  hop1 → hop2:  EXTEND [dest_addr][pub3]    (forwarded)
  hop2 → dest:  CREATE [pub3]
  dest → hop2:  CREATED [dest_pub]
  hop2 → hop1:  EXTENDED [dest_pub]
hop1  → client: EXTENDED [dest_pub]         (keys3 derived)

Client: encrypt payload with keys3, keys2, keys1 (innermost first)
Client → hop1 → hop2 → dest: RELAY [triple-encrypted]
  hop1 peels keys1 layer, forwards
  hop2 peels keys2 layer, forwards
  dest peels keys3 layer → plaintext DATA
```

## Running

```sh
# Start relay nodes (each in its own terminal)
circuit relay 9001
circuit relay 9002
circuit relay 9003

# Build a 3-hop circuit and send a message
circuit client 127.0.0.1:9001 127.0.0.1:9002 127.0.0.1:9003 "hello"

# Run a hidden service (registers with an intro relay)
circuit hs 9004 127.0.0.1:9001

# Run unit tests
circuit test
```

Build with `cargo build --release`.

## Tests

```
cargo test
```

Three tests:
- `dh_stream` — X25519 DH + ChaCha20 fwd/bwd stream roundtrip
- `onion` — 3-layer onion wrap and peel across independent key sets
- `relay_circuit` — live relay nodes on loopback, full 3-hop EXTEND+DATA flow

## Relation to the TLA+ spec

| TLA+ action | Code location |
|-------------|---------------|
| `AddCircuit` | `run_client()` — CREATE + 2× EXTEND |
| `SelectIntroPoint` | `run_hidden_service()` — INTRO cell exchange |
| `ConnectHiddenService` | `run_hidden_service()` — RENDEZVOUS + BRIDGE |

## Dependencies

- `x25519-dalek 2` — ephemeral DH
- `chacha20 0.9` — stream cipher for relay cells
- `chacha20poly1305 0.10` — AEAD (available for future use)
- `hkdf 0.12` + `sha2 0.10` — key derivation
- `rand 0.8` — RNG
