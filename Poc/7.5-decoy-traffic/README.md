# PoC 7.5: Telling decoy source nodes what traffic they must send

The core idea of the decoy source: a client (Node A) instructs a controllable node (Node B) to emit a precise amount of cover traffic at a specific rate to a destination (Node C). The instruction is sealed to B's key, so only B can read it, and once B knows what to send, it generates and transmits that traffic at a set bitrate. To C, and to any GPA watching, this looks like ordinary traffic it cannot open.

In this PoC, Node A sends encrypted instructions directly to B over TCP. B opens them with its own key, then generates `packet_count` garbage packets and transmits them to C at `bitrate_bps`. To C these look like ordinary traffic it cannot open.

## Decoy source topology

```
Node A (client)  --->  Node B (decoy source)  --->  Node C (RDV destination)
     |                      (controllable)               (cannot open noise)
     |                         |       |                         |
   seal                      gen      transmit                discard
   instruction               noise    at bitrate              as decoy
```

- A dials B directly over TCP and sends a sealed `DecoySourceInstruction` (exactly PACKET_SIZE bytes, indistinguishable from normal traffic).
- B accepts one connection, opens the instruction, then loops `packet_count` times generating random bytes (of length `packet_size` which must be less than or equal to `MAX_PAYLOAD`), sealing the result to a random ephemeral key (producing a fixed `PACKET_SIZE` wire packet), and sending it to C.
- C reads each packet, attempts to open it, and discards it as decoy noise (`opened == false`).

## Control packet schema

```
version u8 (1)
destination_addr u16 le
packet_count u32 le
packet_size u16 le         # inner random-byte count (not wire size; wire packet is always PACKET_SIZE)
bitrate_bps u64 le
```

Serialized as 17 bytes, then sealed with `envelope::seal` to B's public key.

## PoC limitations

- Single machine, no real network / NAT stuff

## Sample output

```
--- decoy source (B) self-reports ---
sent  seq=0  tag=bca8628d
sent  seq=1  tag=263e2534
sent  seq=2  tag=d2af8cd3
sent  seq=3  tag=415751b3
sent  seq=4  tag=0ccb656a
sent  seq=5  tag=b1e7137e
sent  seq=6  tag=f55a8eb8
sent  seq=7  tag=66318819
sent  seq=8  tag=91fb3399
sent  seq=9  tag=25fa0b11
--- rdv destination (C) reports ---
received  seq=0  tag=bca8628d  opened=false
received  seq=1  tag=263e2534  opened=false
received  seq=2  tag=d2af8cd3  opened=false
received  seq=3  tag=415751b3  opened=false
received  seq=4  tag=0ccb656a  opened=false
received  seq=5  tag=b1e7137e  opened=false
received  seq=6  tag=f55a8eb8  opened=false
received  seq=7  tag=66318819  opened=false
received  seq=8  tag=91fb3399  opened=false
received  seq=9  tag=25fa0b11  opened=false
decoy source control tag: 12eedda2
noise packets sent by B: 10
noise packets received by C: 10
noise packets opened by C: 0
elapsed: 1.596689597s

Everything is OK: Decoy source sent cover traffic.
```

---

Written by ava (July 2026)
XMR: `86No9mVp1M2GtB88eJHtSAZ5deZVk6FQFGifGHMhr1eC5cdGCXLTFopiA8Q3EDYt2R5oFHkHZDP9n7RmctfoJHrVV9uTJrs`