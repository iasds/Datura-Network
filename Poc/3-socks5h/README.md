# Building

```
cargo build --release

./target/release/client - the client that will try to communicate using udp
./target/release/server - the server that will receive the udp encapsuled inside the tcp
```

# Testing:

1. install dante-server to emulate tor, configure dante to proxy to local, or any other proxy(if so skip to step 5)

2. edit the dante configuration
```
nano /etc/danted.conf
```

3. add the next rows just make sure to change eth0 to the local network adapter
```
    logoutput: stderr
    internal: 127.0.0.1 port = 9055
    external: eth0
    clientmethod: none
    socksmethod: none
    client pass { from: 127.0.0.0/8 to: 0.0.0.0/0 }
    socks pass  { from: 0.0.0.0/0 to: 0.0.0.0/0 }
```

4. run dante to simulate tor
```
sudo danted -f /etc/danted.conf
```

5. start the mid/exit node:
```
./target/release/socks5 --listen 0.0.0.0:5000
```

6. start the entry node:
```
./target/release/socks5 --proxy 127.0.0.1:9055 --remote-host 127.0.0.1 --remote-port 5000 --listen 127.0.0.1:4000
```

7. Send packet
```
echo -n "Proxying UDP through TCP" | nc -u -q1 127.0.0.1 4000
echo -n "Sending regular TCP" | nc -q1 127.0.0.1 4000
```

# Notes
- This POC is completely based on POC-1.
- Added a separation from entry node to mid/exit node
- Changed the arguments to be specific by name (--proxy, --proxy-port, --remote-host, --remote-port).
- UDP Tunnel Proxy(--proxy) argument is currently a must, can be 127.0.0.1 and can be any external locations.
- I will suggest adding a config file as arguments begin to pile up

# Output
## Entry Node Output

```
UDP out: "Proxying UDP through TCP"
tunnel out: "Proxying UDP through TCP"
TCP out: "Sending regular TCP"
tunnel out: "Sending regular TCP"
```

## Mid/Exit Node Output

```
TCP through
sent through: Proxying UDP through TCP
TCP through
sent through: Sending regular TCP
```
