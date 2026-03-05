# Building
```
rustc -O bind.rs
./bind
```

# Notes
By default it binds on 9051, but if another TCP service is running (like Tor), it will fail to bind, so then you need to run with another port, like this.
```
./bind 9052
```

# Input
```
user@localhost:~$ bash

# test a udp packet to the UDP port
user@localhost:~$ echo "hello world" > /dev/udp/127.0.0.1/9051

# test a tcp packet to the TCP port
user@localhost:~$ echo "hello world" > /dev/tcp/127.0.0.1/9051
```

# Output
```
[user ~/Documents/Datura-Network.worm/Poc/1-bind]% ./bind 9051
Succeeded in binding UDP and TCP on port 9051
UDP: 12 Bytes from 127.0.0.1:56132, '[68, 65, 6C, 6C, 6F, 20, 77, 6F, 72, 6C, 64, 0A]'
TCP: 12 Bytes from 127.0.0.1:34662, '[68, 65, 6C, 6C, 6F, 20, 77, 6F, 72, 6C, 64, 0A]'
```
