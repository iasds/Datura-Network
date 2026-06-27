# Building

```
cargo build --release

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
./target/release/socks5 --port 5001
```

6. start the entry node:
```
./target/release/socks5 --port 4000 --proxy 127.0.0.1 --proxy-port 9055 --remote-host hiddenserviceajshhsbdbdbdb.dn --remote-port 80
```

7. Send packet
```
echo -n "Proxying UDP through TCP" | nc -u -q1 127.0.0.1 4000
python3 -c "import socket,struct; s=socket.create_connection(('127.0.0.1',4000)); msg=b'Sending regular TCP'; s.sendall(struct.pack('>I',len(msg))+msg); s.recv(4)"
```

# Notes

# Output
## Entry Node Output

```
[entry] UDP datagram from 127.0.0.1:XXXXX → hiddenserviceajshhsbdbdbdb.dn (DaturaHidden)
[dns] hiddenserviceajshhsbdbdbdb.dn → 127.0.0.1:5001
UDP out: "Proxying UDP through TCP"
tunnel out: "Proxying UDP through TCP"
[entry] TCP connection from 127.0.0.1:XXXXX → hiddenserviceajshhsbdbdbdb.dn (DaturaHidden)
[dns] hiddenserviceajshhsbdbdbdb.dn → 127.0.0.1:5001
[conn] opened: 127.0.0.1:XXXXX → 127.0.0.1:5001 (DaturaHidden)
TCP out: "Sending regular TCP"
tunnel out: "Sending regular TCP"
[conn] closed: 127.0.0.1:XXXXX after X.XXs
```

## Mid/Exit Node Output

```
TCP through
sent through: Proxying UDP through TCP
TCP through
sent through: Sending regular TCP
```
