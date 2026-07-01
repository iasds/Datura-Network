# Datura DHT

PoC 8.1: Here is a basic DHT implementation for Datura Network. 
It's a simplified Kademlia setup based on XOR distance, with each node identity being derived from their encryption key through SHA256 hash function.

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
Node ID: [13, 9e, 39, 40, e6, 4b, 54, 91, 72, 20, 88, d9, a0, d7, 41, 62, 8f, c8, 26, e0, 94, 75, d3, 41, a7, 80, ac, de, 3c, 4b, 80, 70]
Node ID (hex string): 139e3940e64b5491722088d9a0d741628fc826e09475d341a780acde3c4b8070
Public Key: [3b, 6a, 27, bc, ce, b6, a4, 2d, 62, a3, a8, d0, 2a, 6f, d, 73, 65, 32, 15, 77, 1d, e2, 43, a6, 3a, c0, 48, a1, 8b, 59, da, 29]
Address: hnvcppgow2sc2yvdvdicu3ynonsteflxdxrehjr2ybekdc2z3iu6fkqd.dn
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
Node ID: [34, 6d, 1, e6, 90, 34, fc, ab, b0, b2, 1d, 90, 5d, 25, 77, 95, 33, dd, 94, ea, c0, e1, bf, b4, 9f, b0, c0, d8, e9, dd, 47, 71]
Node ID (hex string): 346d01e69034fcabb0b21d905d25779533dd94eac0e1bfb49fb0c0d8e9dd4771
Public Key: [75, 59, 77, 59, b7, d9, 32, a3, ee, b6, 58, 2b, e8, c7, 57, 34, 12, 49, 43, b8, c2, 7b, 91, 77, e6, 97, 51, aa, 14, 10, f0, 11]
Address: ovmxownx3ezkh3vwlav6rr2xgqjesq5yyj5zc57gs5i2ufaq6ai6vryd.dn
Connected to 127.0.0.1:9000
Listening on 127.0.0.1:9001
```

You can manually use any running node as bootstrap for a new node like this:

```
cargo run node 127.0.0.1:9002 346d01e69034fcabb0b21d905d25779533dd94eac0e1bfb49fb0c0d8e9dd4771 127.0.0.1:9001
```

The above command will use 127.0.0.1:9001 with Node ID 346d01e69034fcabb0b21d905d25779533dd94eac0e1bfb49fb0c0d8e9dd4771
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
- Recursive fallback is not automated (again for simplicity). You can manually run "resolve-hs" or "resolve-nat" again connecting to another node of your choice.