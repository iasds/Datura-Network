# Building
```
cargo build --release
```

# Usage
#### Start server
```
./target/release/vanity-hs-e2ee server [port]
```
Generates a fresh Ed25519 keypair, derives a `.dn` hidden service address, and prints it. Default port: 9009.

#### Start client
```
./target/release/vanity-hs-e2ee client <address.dn> <port> [message]
```
Extracts the Ed25519 public key from the address (no network lookup), encrypts a message, and sends it. Message defaults to `"hello datura network!"` if omitted.

# Output
#### Server output
```
hidden service address: i6d5omux3j2cu3ryv56t2j3dlqp7rey7pxdwpjrnezpfo3r7udcqd3ad.dn
listening on port 9009
[127.0.0.1:57927] connected
[127.0.0.1:57927] decrypted: "hello datura network!"
[127.0.0.1:57928] connected
[127.0.0.1:57928] decrypted: "test message"
```

#### Client output
```
extracted pubkey from address; no network lookup needed
message: "hello datura network!"
encrypted 21 bytes  -->  kem: 32 bytes, ciphertext: 37 bytes
sent encrypted message to server
```

# Implementation

Hidden service addresses embed the Ed25519 public key directly in the address string (Tor v3 style), so no DHT or directory lookup is needed for encryption, the address alone is sufficient.

#### Address format
35 bytes encoded as base32-lower + `.dn` (59 characters total):
```
base32_lower( pubkey[32] || sha3_256(".dn checksum" || pubkey || version)[:2] || version[1] ) + ".dn"
```

#### Key conversion
Ed25519 and X25519 share the same underlying Curve25519 group.

- **Public key**: `VerifyingKey::to_montgomery()` converts the Edwards point to its Montgomery (u-coordinate) representation, which is the X25519 public key format.
- **Private key**: `SHA-512(seed)[0..32]` with RFC 7748 §5 clamping applied. Same expansion Ed25519 uses internally for signing, and a valid X25519 scalar.

Note: `ed25519-dalek`'s `to_scalar()` returns the scalar reduced mod l (the group order), which is not the same as the clamped SHA-512 output and produces a public key mismatch. The direct SHA-512 path is required.

#### Encryption
`DhKem25519` + `HkdfSha256` + `ChaCha20Poly1305` via HPKE (RFC 9180) in Base mode.

**Post-quantum note:** This PoC uses classical X25519 (DhKem25519). Other PoCs (2, 12) use X-Wing (X25519 + ML-KEM-768). A PQ-safe HS encryption scheme requires a separate ML-KEM keypair as there is no Ed25519->ML-KEM conversion. That extension belongs in PoC 10.1 (encrypted HS descriptor), where a PQ public key can be embedded alongside the Ed25519 identity key.

**Key reuse note:** The same Ed25519 keypair is used for both signing (HS identity) and X25519 encryption (via `to_montgomery()`). This is the same design as Tor v3 onion services and is considered safe with this specific conversion, but it should be noted the HS key serves both roles.

#### Wire format
```
[ kem_output: 32 bytes fixed ]
[ ciphertext_len: 4 bytes big-endian ]
[ ciphertext: ciphertext_len bytes ]
```
Ciphertext is plaintext length + 16 bytes (ChaCha20Poly1305 authentication tag).

#### Sizes
```
kem_output: 32 bytes (ephemeral X25519 public key)
ciphertext overhead: 16 bytes (AEAD tag)
address: 59 characters
```

# References
- [Issue #63: Vanity V3 hidden services](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/issues/63)
- [RFC 9180: HPKE](https://www.rfc-editor.org/rfc/rfc9180)
- [RFC 7748 s5: X25519 clamping](https://www.rfc-editor.org/rfc/rfc7748#section-5)
- [Tor v3 address spec](https://spec.torproject.org/rend-spec/overview.html)

# Notes for future reference and implementation:
- Keys need to be zeroized to not be accessable through process or heap
- Quantum security through a separate ML-KEM keypair, paired with the Ed25519 identity key
- Per-session nonce in wire format, bound into HPKE info string in order to prevent replay attacks
- HPKE Auth mode or Ed25519 signature over the plaintext so the server can verify who sent the message
- Key persistence: server currently generates a fresh keypair on every startup; keys need to be saved to disk (encrypted at rest) so the address stays stable across restarts
- Key rotation: mechanism to migrate to a new address with a transition period, or sign a new key with the old one
- Tighten the HPKE INFO binding: currently just "datura-network-poc9". It should include the recipient's address and message direction (client-to-server) to prevent the same keypair being confused across different protocol contexts.
- Wire protocol versioning: no version byte in the current wire format means any future change silently breaks compatibility with older clients.

Note: These future implementations will likely make the adresses longer, but will be more secure. (expected to be about 91–111 chars if the ML-KEM key reference is embedded, otherwise should mainly be server improvements)


`README.md` generated from custom text editor