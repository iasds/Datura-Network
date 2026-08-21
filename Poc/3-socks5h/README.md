# Building

```
cargo build --release
```

# Testing

This PoC demonstrates genuine SOCKS5-to-SOCKS5 forwarding through a chain of nodes. Three data paths are multiplexed onto a single `local_node` listener, all carried as genuine SOCKS5 (no custom envelope):

1. **Raw UDP capture** (Path A): Local UDP datagrams are relayed as genuine SOCKS5 UDP ASSOCIATE toward the exit node, targeting a fixed `datura-capture.invalid:1` placeholder
2. **Raw TCP capture** (Path B): Local raw TCP connections are relayed as genuine SOCKS5 CONNECT toward the exit node, also targeting `datura-capture.invalid:1`
3. **Genuine SOCKS5 client passthrough** (Path C): Inbound SOCKS5 client connections (first byte `0x05`) are relayed end-to-end through the chain via SOCKS5-in-SOCKS5 chaining

## Roles

Three roles form a chain: `local_node -> [mid_node]* -> exit_node`

- **`local_node`** (`--role local_node --port <PORT> --next-hop <host:port>`): entry point. Captures raw UDP/TCP, relays as SOCKS5. Also accepts genuine SOCKS5 clients and chains them through. Optionally co-hosts a `mid_node` on `--mid-port`.
- **`mid_node`** (`--role mid_node --port <PORT> --next-hop <host:port>`): pure SOCKS5-to-SOCKS5 relay. Terminates inbound SOCKS5, re-negotiates outbound SOCKS5 to the same target. TCP/CONNECT only (UDP ASSOCIATE not supported). Zero or more can be chained between `local_node` and `exit_node`.
- **`exit_node`** (`--role exit_node --port <PORT>`): terminal node. Terminates SOCKS5 handshakes and prints received data. Never dials real destinations. Requires no `--next-hop`.

## Capture Target Placeholder

Raw captures target `datura-capture.invalid:1` — an RFC 6761 reserved `.invalid` domain that guarantees no real DNS resolution or dialing. This self-documenting placeholder lets `exit_node` (with DNS resolution disabled) recognize capture traffic vs. passthrough traffic. The name is never resolved; only its presence in the SOCKS5 CONNECT/UDP-ASSOCIATE target signals "this is captured traffic, not a real client destination."

## Co-Hosting `--mid-port`

`local_node` can optionally run a `mid_node` relay in the same process via `--mid-port <PORT>`, sharing the same `--next-hop`. This lets a single process serve as both an entry point and a relay hop in a chain:

```
./target/release/socks5 --role local_node --port 9051 --mid-port 9052 --next-hop 127.0.0.1:9053
```

Here, `local_node` accepts traffic on 9051 and a co-hosted `mid_node` accepts traffic on 9052; both forward toward the exit node at 9053.

# Examples

## Scenario 1: Single-hop chain (local_node -> exit_node)

**Terminal 1: Start exit_node**
```
./target/release/socks5 --role exit_node --port 9053
```

**Terminal 2: Start local_node**
```
./target/release/socks5 --role local_node --port 9051 --next-hop 127.0.0.1:9053
```

**Terminal 3: Send raw UDP (Path A)**
```
echo -n "raw UDP payload" | nc -u -q1 127.0.0.1 9051
```

Exit node prints:
```
UDP reconstructed (15 bytes): "raw UDP payload"
```

**Send raw TCP (Path B)**
```
echo -n "raw TCP payload" | nc -q1 127.0.0.1 9051
```

Exit node prints:
```
TCP reconstructed (15 bytes): "raw TCP payload"
```

**Send genuine SOCKS5 client request (Path C)**
```
curl --socks5 127.0.0.1:9051 http://example.com/ 2>&1
```

Exit node prints:
```
SOCKS5 passthrough -> example.com:80
```

## Scenario 2: Multi-hop chain with co-hosted mid_node (local_node with --mid-port)

**Terminal 1: Start exit_node**
```
./target/release/socks5 --role exit_node --port 9053
```

**Terminal 2: Start local_node_a with co-hosted mid_node**
```
./target/release/socks5 --role local_node --port 9051 --mid-port 9052 --next-hop 127.0.0.1:9053
```

**Terminal 3: Start local_node_b, pointing at local_node_a's mid-port**
```
./target/release/socks5 --role local_node --port 9050 --next-hop 127.0.0.1:9052
```

**Send raw TCP from local_node_b (chains through local_node_a's mid-port relay)**
```
echo -n "data via chain" | nc -q1 127.0.0.1 9050
```

Exit node prints:
```
TCP reconstructed (14 bytes): "data via chain"
```

## Scenario 3: Multi-hop chain with explicit mid_node

**Terminal 1: Start exit_node**
```
./target/release/socks5 --role exit_node --port 9053
```

**Terminal 2: Start mid_node**
```
./target/release/socks5 --role mid_node --port 9052 --next-hop 127.0.0.1:9053
```

**Terminal 3: Start local_node**
```
./target/release/socks5 --role local_node --port 9051 --next-hop 127.0.0.1:9052
```

**Send raw TCP**
```
echo -n "relayed data" | nc -q1 127.0.0.1 9051
```

Exit node prints:
```
TCP reconstructed (12 bytes): "relayed data"
```

# Implementation Notes

- **No external proxy required**: This PoC uses only `fast_socks5` for all SOCKS5 server/client roles. There is no Dante, no external relay — every hop is self-contained.
- **Disable `execute_command`**: Each hop that terminates a SOCKS5 handshake disables `fast_socks5`'s automatic command execution to prevent unwanted real dials or DNS resolution (especially for the `.invalid` capture placeholder). Instead, each hop hand-writes its own SOCKS5 reply.
- **UDP ASSOCIATE limitation**: `mid_node` does not support UDP ASSOCIATE, only TCP CONNECT. Raw UDP captures from `local_node` must go directly to `exit_node`, not through any `mid_node` hops.
- **No return path**: This is a display-only PoC. Captured data is logged but never returned to the original sender; only the forward path through the chain is real.

# Testing (Integration Test)

Run the test suite to exercise all three data paths and the co-hosted `--mid-port` feature:

```
cargo test --test co_hosted_mid_node -- --nocapture
```

This test:
1. Spawns an `exit_node`, a `local_node_a` with co-hosted mid-port, and a separate `local_node_b`
2. Sends raw UDP to `local_node_a` (Path A)
3. Sends raw TCP to `local_node_a` (Path B)
4. Sends a genuine SOCKS5 CONNECT to `local_node_a` (Path C)
5. Chains a raw TCP request from `local_node_b` through `local_node_a`'s co-hosted mid-port relay to `exit_node`
6. Asserts the exit node received and logged each piece of data correctly
