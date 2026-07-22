# PoC 7.5: Telling decoy source nodes what traffic they must send

The core idea of the decoy source: a client (Node A) instructs a controllable node (Node B) to emit a precise amount of cover traffic to a destination (Node C). The instruction is sealed to B's key, so only B can read it, and once B knows what to send it generates and transmits that traffic at a set bitrate. To C, and to any GPA watching, this looks like ordinary traffic it cannot open.

In this PoC, Node A sends encrypted instructions directly to B over TCP. B opens them with its own key, then generates `packet_count` garbage packets of `packet_size` bytes and transmits them to C at `bitrate_bps`. To C these look like ordinary traffic it cannot open.

### Decoy source topology

```
Node A (client)  --->  Node B (decoy source)  --->  Node C (RDV destination)
     |                      (controllable)               (cannot open noise)
     |                         |       |                         |
   seal                      gen      transmit                discard
   instruction               noise    at bitrate              as decoy
```

- A dials B directly over TCP and sends a sealed `DecoySourceInstruction` (exactly PACKET_SIZE bytes, indistinguishable from normal traffic).
- B accepts one connection, opens the instruction, then loops `packet_count` times generating random bytes of `packet_size` and sending them to C.
- C reads each packet, attempts to open it, and discards it as decoy noise (`opened == false`).

### Control packet schema

```
version u8 (1)
destination_addr u16 le
packet_count u32 le
packet_size u16 le
bitrate_bps u64 le
```

Serialized as 17 bytes, then sealed with `envelope::seal` to B's public key.

## Crypto

- X-Wing (x25519 + ML-KEM-768) to wrap per-packet key for the real dest
- ChaCha20-Poly1305 over the payload. Wrong key means auth fails, so it's decoy
- Every packet is padded to a fixed `PACKET_SIZE`, so all 8 are byte-identical in length. the real payload length is inside the encrypted region, so it never leaks either.

## PoC Limitations

- Single machine, no real network / NAT stuff

---

Written by ava (July 2026)
XMR: `86No9mVp1M2GtB88eJHtSAZ5deZVk6FQFGifGHMhr1eC5cdGCXLTFopiA8Q3EDYt2R5oFHkHZDP9n7RmctfoJHrVV9uTJrs`