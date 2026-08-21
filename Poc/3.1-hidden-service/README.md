# PoC 3.1: hidden-service-aware SOCKS5 relaying

Built directly on top of PoC 3's genuine-SOCKS5 chain architecture
(`Poc/3-socks5h`): every hop is a pure `fast_socks5`-based SOCKS5
server/client, there is no external `dante` proxy involved anywhere in this
chain, and there is no custom envelope on the wire between nodes.

On top of that base, this PoC adds:

1. **Address classification.** The entry hop (`local_node`'s genuine-SOCKS5
   path) classifies every CONNECT target into one of: `.dn` (Datura hidden
   service), `.onion` (Tor), `.i2p`, clearnet, public IP, local/LAN IP, or
   unknown, logs the classification, and refuses (with a real SOCKS5
   `NOT_ALLOWED_BY_RULESET` reply, `0x02`) anything local/LAN or
   unclassifiable. Everything else is relayed through the chain as before;
   only logging and the local/LAN refusal are new, there is still no real
   per-address-type circuit routing.
2. **A static hidden-service map.** `exit_node` holds a hardcoded,
   `/etc/hosts`-style table from `.dn` names to a loopback `host:port`. A
   CONNECT for a mapped name is the one case in this PoC where `exit_node`
   actually dials a real destination (a `hidden_service` role instance
   simulating that hidden service) instead of only logging what it
   received.

## Roles

Four roles are selected via `--role`:

- `local_node` -- binds a UDP socket and a TCP listener on `--port`,
  requires `--next-hop`. Raw UDP/TCP is captured and forwarded as a genuine
  SOCKS5 UDP ASSOCIATE / CONNECT (same as PoC 3). Genuine inbound SOCKS5
  client connections are classified and relayed on toward `--next-hop`;
  local/LAN or unclassifiable targets are refused immediately. Optionally
  co-hosts a `mid_node` on `--mid-port` in the same process.
- `mid_node` -- binds a TCP listener on `--port`, requires `--next-hop`.
  Pure transparent SOCKS5-to-SOCKS5 relay; performs no classification of its
  own (only the entry hop does). May be chained zero or more times between
  `local_node` and `exit_node`. Does not support UDP ASSOCIATE.
- `exit_node` -- binds a TCP listener on `--port`, forbids `--next-hop`.
  Terminates every inbound SOCKS5 handshake itself and inspects the
  negotiated command/target; see "Behavior" below.
- `hidden_service` -- binds a plain TCP listener (not SOCKS5) on `--port`,
  forbids `--next-hop` and `--mid-port`. Simulates a hidden service: reads
  one request, always replies with the same fixed HTTP-ish response body,
  and closes.

## Relay Policies

The `helpers::RelayPolicy` enum governs how a hop handles the negotiated SOCKS5 target before relaying:

- **`ClassifyAndFilter`**: Entry-hop behavior used by `local_node` Path C (genuine SOCKS5 client relay). Classifies the target address and refuses (with a real SOCKS5 `NOT_ALLOWED_BY_RULESET` error reply, `0x02`) anything that is not routable (e.g., local IPs, LAN targets, or unclassifiable addresses). Routable targets (`.dn`, `.onion`, `.i2p`, clearnet, public IPs) pass through and are relayed downstream.

- **`Transparent`**: Transit-hop behavior used by `mid_node`. Relays whatever target was negotiated without any classification or filtering — just terminates the inbound SOCKS5 handshake, learns the target, and re-negotiates an identical outer SOCKS5 CONNECT to that same target one hop downstream. Only the entry hop makes routing decisions.

## Building

```
cargo build --release
```

## Manual validation walkthrough

1. Start the simulated hidden service:
```
./target/release/hidden-service --role hidden_service --port 5001
```

2. Start the exit node:
```
./target/release/hidden-service --role exit_node --port 9053
```

3. Start the local (entry) node, pointed at the exit node:
```
./target/release/hidden-service --role local_node --port 9051 --next-hop 127.0.0.1:9053
```

(Optionally insert one or more `mid_node` hops between `local_node` and
`exit_node`, and/or co-host a `mid_node` on `local_node` via `--mid-port`,
exactly as in PoC 3.)

### Case: querying the `.dn` hidden service

```
curl -s --socks5-hostname 127.0.0.1:9051 http://hiddenserviceajshhsbdbdbdb.dn/
```

Expected: prints the hidden service's fixed response body
(`hello from hiddenserviceajshhsbdbdbdb.dn`). `exit_node` logs a `[resolve]`
line followed by `[exit] dialed hidden service ...` and, once the
connection closes, a byte-count summary.

**`--socks5-hostname` is mandatory here, not `--socks5`.** `--socks5` makes
curl resolve the hostname itself, client-side, before ever opening the
SOCKS5 connection -- since `hiddenserviceajshhsbdbdbdb.dn` doesn't actually
resolve via real DNS, that would just fail locally without ever reaching
`local_node`'s classification logic at all. `--socks5-hostname` defers
resolution to the SOCKS5 server (an ordinary "SOCKS5h" client), which is
what lets the hostname reach `local_node`/`exit_node` unresolved so they can
classify and (for the `.dn` case) map it themselves.

### Case: `.onion` / `.i2p` / clearnet / a bare public IP

```
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://exampleonionaddress.onion/
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://example.i2p/
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://example.com/
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://8.8.8.8/
```

Expected: each just logs a `[classify]` line at the entry hop and a
`SOCKS5 passthrough -> ... [not routed: no circuit for this address type
yet]` line at `exit_node`, then hangs until curl's own timeout (`-m 5`
recommended). This is deliberate, not a bug: only the `.dn`-mapped case
above actually gets dialed anywhere in this PoC; every other non-local
address type is only classified and logged, per the issue's scope ("for now
just packet logging").

### Case: a local/LAN target

```
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://127.0.0.1:8080/
curl -s -m 5 --socks5-hostname 127.0.0.1:9051 http://192.168.1.1/
```

Expected: refused immediately (curl exits right away with a SOCKS5 proxy
error, no timeout needed). `local_node` logs a `[refused] ... (LocalIp) -
local/LAN targets are excluded from routing` line and writes back a real
SOCKS5 `NOT_ALLOWED_BY_RULESET` reply.

### Case: raw TCP/UDP capture (regression check against PoC 3)

```
echo -n "Sending regular TCP" | nc -q1 127.0.0.1 9051
echo -n "Proxying UDP through TCP" | nc -u -q1 127.0.0.1 9051
```

Expected: unchanged from PoC 3's base behavior -- `exit_node` logs
`TCP reconstructed (...)` / `UDP reconstructed (...)` lines. This confirms
Path A/B (raw UDP/TCP capture) still works with no regression from the
classification/hidden-service-map additions, since those additions only
touch Path C (genuine SOCKS5 client traffic) and `exit_node`'s `TCPConnect`
handling.

Note: UDP captures must go directly from `local_node` to `exit_node` (a
single `--next-hop` hop, no `mid_node` in between), since `mid_node` does
not support UDP ASSOCIATE -- the same limitation as the base PoC 3.

## Known limitations (inherited from PoC 3)

- Replies are optimistic: a CONNECT success reply is written to the client
  *before* the downstream dial/relay has actually completed, so a
  downstream failure typically surfaces to the client as an EOF rather than
  a proper SOCKS5 error reply. This PoC fixes this optimism chain-wide via
  `relay_socks5` (used by `local_node` Path C and every `mid_node` hop):
  success replies are written only *after* the downstream dial has
  succeeded, and on failure a real SOCKS5 error reply is written before
  returning, forwarding the actual downstream error code or falling back to
  a generic `REPLY_GENERAL_FAILURE` for non-SOCKS5 errors.
- Local/LAN targets are refused at classification time: the entry hop
  (`local_node` Path C) invokes `addressing::classify_address()` from
  `src/addressing.rs` immediately after learning the negotiated target and
  before any downstream dial is attempted. Non-routable targets (`LocalIp` or
  `Unknown`, per `AddressType::is_routable()`) receive a real SOCKS5
  `NOT_ALLOWED_BY_RULESET` reply (0x02) and the connection is refused. This
  is a structurally separate safety property from the dial-then-reply fix
  above: classification happens earlier in execution, preventing the dial-failure
  race for local/LAN targets entirely rather than managing it with reply ordering.
- `relay_socks5` (used by `local_node` Path C and by `mid_node`) assumes
  every inbound request is a `TCPConnect` without ever checking `cmd()`; a
  client that sends a genuine SOCKS5 UDP ASSOCIATE to `local_node` will not
  be handled correctly by this path (UDP ASSOCIATE only works via
  `local_node`'s own raw-UDP Path A capture, not via a real inbound SOCKS5
  UDP ASSOCIATE request).
