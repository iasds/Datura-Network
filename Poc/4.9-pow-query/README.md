# Building
```
cargo build --release
```

# Usage
#### Start server
```
./target/release/pow-query 9999
```

#### Start honest client
```
./target/release/pow-query connect 9999
```

##### Honest client output
```
successfully connected to server at 127.0.0.1:9999

got challenge 'B7359D4D12268DDF006AF55600000FA0' of difficulty 4000 from server
solving challenge...
solved challenge
sent challenge to server
server replied with 79 bytes
"127.0.0.1:10000
127.0.0.1:10001
127.0.0.1:10002
127.0.0.1:20000
127.0.0.1:20001"
```

##### Server output for honest client
```
127.0.0.1:34662 connected, sent difficulty 4000 challenge 'B7359D4D12268DDF006AF55600000FA0'
127.0.0.1:34662 replied with solution
solution from 127.0.0.1:34662 was correct, sending list of nodes
```

#### Start random solution client
```
./target/release/pow-query random 9999
```

##### Random solution client output
```
successfully connected to server at 127.0.0.1:9999
got challenge 'B7359D4D12268DDF006AF55600000FA0' of difficulty 4000 from server
generating random solution...
sent challenge to server
server replied with 0 bytes
""
```

##### Server output for random solution client
```
127.0.0.1:34662 connected, sent difficulty 4000 challenge 'B7359D4D12268DDF006AF55600000FA0'
127.0.0.1:34662 replied with solution
solution from 127.0.0.1:34662 was incorrect, closing stream
```
