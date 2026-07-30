# Datura DHT

PoC 8.1: Here is a basic DHT implementation for Datura Network. 
It's a Kademlia setup based on XOR distance, with each node identity being derived from their encryption key through SHA256 hash function.

## Overview

- There is one hardcoded node as the first nerwork node.
- You can create more nodes and make them join the network.
- You can publish and look up HS and NAT info.
- DHT is based on Kademlia with 256 buckets.


## Run/Test

### 1- Create the first node

```
cargo run init
```

You must see this output exactly, because the first node is hardcoded (as specs stated):

```
Signing Key: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
Public Key: [3b, 6a, 27, bc, ce, b6, a4, 2d, 62, a3, a8, d0, 2a, 6f, d, 73, 65, 32, 15, 77, 1d, e2, 43, a6, 3a, c0, 48, a1, 8b, 59, da, 29]
Node ID: [13, 9e, 39, 40, e6, 4b, 54, 91, 72, 20, 88, d9, a0, d7, 41, 62, 8f, c8, 26, e0, 94, 75, d3, 41, a7, 80, ac, de, 3c, 4b, 80, 70]
Node ID (hex string): 139e3940e64b5491722088d9a0d741628fc826e09475d341a780acde3c4b8070
Address: hnvcppgow2sc2yvdvdicu3ynonsteflxdxrehjr2ybekdc2z3iu6fkqd.dn
Starting server with ID [13, 9e, 39, 40, e6, 4b, 54, 91, 72, 20, 88, d9, a0, d7, 41, 62, 8f, c8, 26, e0, 94, 75, d3, 41, a7, 80, ac, de, 3c, 4b, 80, 70]
Listening on 127.0.0.1:9000
```

This node is the default bootstrap node, if no bootstrap is specified.
Of course this can be a list of nodes, but for simplicity it's just one node.
When more nodes join the network, you can manually use them as bootstrap nodes.

### 2- Create more nodes

```
cargo run node 127.0.0.1:9001
```

The output for each node is something similar to this:

```
Signing Key: [1e, a6, 45, 8d, 65, b6, 56, c1, 30, 94, cc, 37, 20, c0, 8, 37, 21, 5f, d8, cc, 69, dc, bb, 11, bd, 7d, 1b, 45, 75, 92, ad, a3]
Public Key: [40, 58, 1e, 52, 5e, 9a, e9, 7f, 4a, 30, 48, d4, 25, 34, b0, f7, a5, 14, fe, d9, 6b, 80, 7c, fe, eb, 46, e0, 3c, 51, de, 55, 8f]
Node ID: [b8, d4, 79, d3, 9f, 42, c1, ee, 83, b1, 3a, ee, d2, 49, a5, 74, 3b, 4d, 35, 73, 54, 9a, 12, db, 9e, 33, 77, bf, c, 1f, 56, d2]
Node ID (hex string): b8d479d39f42c1ee83b13aeed249a5743b4d3573549a12db9e3377bf0c1f56d2
Address: ibmb4us6tlux6srqjdkcknfq66srj7wznoahz7xli3qdyuo6kwhqaead.dn
Joining network via [13, 9e, 39, 40, e6, 4b, 54, 91, 72, 20, 88, d9, a0, d7, 41, 62, 8f, c8, 26, e0, 94, 75, d3, 41, a7, 80, ac, de, 3c, 4b, 80, 70] at 127.0.0.1:9000
Connected to [13, 9e, 39, 40, e6, 4b, 54, 91, 72, 20, 88, d9, a0, d7, 41, 62, 8f, c8, 26, e0, 94, 75, d3, 41, a7, 80, ac, de, 3c, 4b, 80, 70] at 127.0.0.1:9000
Starting server with ID [b8, d4, 79, d3, 9f, 42, c1, ee, 83, b1, 3a, ee, d2, 49, a5, 74, 3b, 4d, 35, 73, 54, 9a, 12, db, 9e, 33, 77, bf, c, 1f, 56, d2]
Listening on 127.0.0.1:9001
```

You can manually use any running node as bootstrap for a new node like this:

```
cargo run node 127.0.0.1:9002 b8d479d39f42c1ee83b13aeed249a5743b4d3573549a12db9e3377bf0c1f56d2 127.0.0.1:9001
```

The above command will use 127.0.0.1:9001 with Node ID b8d479d39f42c1ee83b13aeed249a5743b4d3573549a12db9e3377bf0c1f56d2
as bootstrap instead of the default node.

It's a good idea to run more than three nodes for a better demonstration.

### 3- Publish HS or NAT

```
cargo run publish-hs
```

or if you don't want to use the default node:

```
cargo run publish-hs <bootstrap-node-id> <bootstrap-node-ip-port>
```

The above commands publish a hidden service to K closest nodes.

Similary you can publish NAT info:

```
cargo run publish-nat <gateway-node-id> <gateway-node-ip-port>
```

or if you don't want to use the default node:

```
cargo run publish-nat <gateway-node-id> <gateway-node-ip-port> <bootstrap-node-id> <bootstrap-node-ip>
```

The above commands publish NAT info to K closest nodes.

### 4- Resolve HS or NAT

```
cargo run resolve-hs <hs-id>
```

or if you don't want to use the default node:

```
cargo run resolve-hs <hs-id> <bootstrap-node-id> <bootstrap-node-ip-port>
```

The above commands get access info for a hidden service by asking K closest nodes for it.
<hs-id> is the hash of hidden service's address (previously published) a.k.a Node ID.

Similary you can get NAT info:

```
cargo run resolve-nat <hs-id>
```

or if you don't want to use the default node:

```
cargo run resolve-nat <hs-id> <bootstrap-node-id> <bootstrap-node-ip-port>
```

The above commands get access info for a node behind NAT by asking K closest nodes for it.
<hs-id> is the hash of hidden service's address (previously published) a.k.a Node ID.

## Assumptions / Simplifications

- I assumed nodes trust each other so they don't sign RPC messages (for simplicity).
- I assumed the situation where "all and every neighbor node who had stored node X info are down" never happens (for simplicity)