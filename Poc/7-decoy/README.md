# poc7: proof of concept for decoy destinations

Global passive adversary can count bytes. If only one node on the network ever receives the traffic meant for a hidden service, that node is the hidden service. So info isn't sent to one node: send the exact same bytes to 8, and only the real one can actually decrypt them. The other 7 get noise they can't open and throw it away. From the outside all 8 look identical, so the anon set is 1/8.

the 8 decoys belong to the hidden service, not the client, and are the same 8 for everyone who connects. if each client picked its own decoys, the only node common to every client's fanout would be the real hs, and a gpa running lots of clients would discover the real addr. one fixed hs-owned set means all 8 co-receive on every connection, so a gpa still sees 8, not 1. picked once and reused long-term for the same reason.

### what the hidden service actually holds

The decoy set is just 8 public node addrs (`hs_decoy_set.txt`). real hs plus 7 independent decoy nodes it selected. Nothing in that file is secret. The hs has one secret: its own key (`hs_identity.txt`). In prod: client resolves .dn addr through its descriptor to get public key; poc just hands the client the pubkey. It recognises its own slot only because it's the id it can reproduce from hs' key. The 7 decoy nodes are separate machines; the hs never holds their keys. it only knows their public ids and sends them cover traffic they can't decrypt. (demo makes throwaway keys to act as those 7 nodes; keys would live on those nodes)

## topology

18 nodes on localhost: 6 routers + 8 destinations run as their own threads (14 total), the 4 clients run in main's thread:

```
4 clients to 6 routers to 8 destinations (1 real + 7 decoy)
```

- clients each seal their own packet to the hs public key and push the same bytes into the router layer, one client after another in main. a client only knows the hs pubkey, it never sees the decoy set.
- routers are blind relays. They just copy the packet onward.
- dests each try to open the packet. only the real one succeeds.
- multiple clients so you can see they all fan out to the same fixed 8.

## crypto

- X-Wing (x25519 + ML-KEM-768) to wrap per-packet key for the real dest
- ChaCha20-Poly1305 over the payload. Wrong key means auth fails, so it's decoy
- every packet is padded to a fixed `PACKET_SIZE`, so all 8 are byte-identical in length. the real payload length is inside the encrypted region, so it never leaks either.

## run

```
cargo run
```

First run generates the hidden service's own key (`hs_identity.txt`) and its 8-node decoy set (`hs_decoy_set.txt`), picking the real slot at random. Every run after reuses both (delete to reroll). Both are anchored to the directory fyi. (in testing, running from other directories would recreate, so i locked them)

```
cargo test
```

## output

per client: which of the 8 slots saw its packet, opened only by the real slot. per slot: got one packet from every client, only the real slot ever opened any. so every client fanned out to the same 8

## not done in poc

- no PoW on who gets to be a decoy
- single machine, no real network / NAT stuff
- 1/8 anon is theoretical: if a gpa runs some of the 7 decoys it knows those are fake and subtracts them (runs all 7 -> 1/1). so the real anon set = how many decoys the hs actually controls/trusts. basically, for security you should run your own decoy nodes, or at least like 2-4. (funnily though, multiple competing gpa's in network would help security, and fight against eachother)

---

Written by hannahmoose (July 2026)
XMR: 86iWxZqVjvhGVqPTD8Rh36LHa5zCn4ZgkQBsqfUUFod73mpUHVxxS1VYFMv9PscniaWA2aSLKrpPeEFJrcVjaTomR42679J