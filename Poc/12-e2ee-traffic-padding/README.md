## PoC 12: E2EE traffic padding

### Usage:
```
$ cargo run -- server [server-listen-port]
$ cargo run -- client [client-listen-port] [server-listen-port]
```

Client listens on client-listen-port, pads traffic, encrypts traffic. Then, client sends traffic to server:server-listen-port. Server decrypts traffic, identifies how much padding there is. Padding can easily be stripped.

You are able to change PACKET_SIZE to anything. KEM padding and traffic padding are automatically adjusted.


### Demo
```
# (Window 1, netcat)
$ wc -c file.small
3000 file.small

$ wc -c file.big

8534 file.big



# (Window 2, server)
$ cargo run -- server 4444
   Compiling e2ee-traffic-padding v0.0.0 (/home/user/Datura-Network/Poc/12-e2ee-traffic-padding)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running `target/debug/e2ee-traffic-padding server 4444`
PACKET_SIZE is 1337
[Node B] Listening on 4444



# (Window 3, client)
$ cargo run -- client 5555 4444
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running `target/debug/e2ee-traffic-padding client 5555 4444`
PACKET_SIZE is 1337
[Node A] Listening on 5555
[Node A] Will connect to Node B on 4444



# (Window 1, netcat)
$ nc 127.0.0.1 5555 < file.big



# (Window 3, client)
...
[Node A] Received encapsulation key from Node B
[Node A] Created and sent KEM ciphertext to Node B in 5.523482ms
[Node A] Received data to send in 25.397µs
[Node A] Encrypted padded data in 859µs
[Node A] Unencrypted size: 9343 bytes
[Node A] Unencrypted hash: 6002068591939699001
[Node A] Encrypted size: 9359 bytes
[Node A] Encrypted hash: 141972074113600315
[Node A] Number of padding bytes: 809
[Node A] Sending 7 packets
[Node A] Sent 9359 bytes in 133.029µs



# (Window 2, server)
...
[Node B] Sent encapsulation key to Node A in 3.694685ms
[Node B] Received ciphertext from Node A
[Node B] Derived shared secret in 7.852367ms
[Node B] Encrypted size: 9359 bytes
[Node B] Encrypted hash: 141972074113600315
[Node B] Decrypted data in 874.529µs
[Node B] Decrypted size: 9343 bytes
[Node B] Decrypted hash: 6002068591939699001
[Node B] Len padding: 809



# (Window 1, netcat)
$ nc 127.0.0.1 5555 < file.small



# (Window 3, client)
[Node A] Received encapsulation key from Node B
[Node A] Created and sent KEM ciphertext to Node B in 5.287049ms
[Node A] Received data to send in 19.356µs
[Node A] Encrypted padded data in 558.868µs
[Node A] Unencrypted size: 3995 bytes
[Node A] Unencrypted hash: 5478095654138720446
[Node A] Encrypted size: 4011 bytes
[Node A] Encrypted hash: 11075420337445114709
[Node A] Number of padding bytes: 995
[Node A] Sending 3 packets
[Node A] Sent 4011 bytes in 57.097µs



# (Window 2, server)  
[Node B] Sent encapsulation key to Node A in 3.474822ms
[Node B] Received ciphertext from Node A
[Node B] Derived shared secret in 7.456054ms
[Node B] Encrypted size: 4011 bytes
[Node B] Encrypted hash: 11075420337445114709
[Node B] Decrypted data in 489.758µs
[Node B] Decrypted size: 3995 bytes
[Node B] Decrypted hash: 5478095654138720446
[Node B] Len padding: 995

```

![](screenshot.png)


Screenshot of wireshark during sending file.big, with PACKET_SIZE set to 1024. The first 8588 byte packet is from netcat. After that, all PSH/ACK packets are 1078 bytes (1024 + 54), even KEM packets (although, they remain padded with 0s, as mentioned in a comment in the code).