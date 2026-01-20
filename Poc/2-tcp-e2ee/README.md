# Building
```
cargo build --release
```

# Usage
#### Start server
```
./target/release/tcp-e2ee 9999
```

#### Start client
```
./target/release/tcp-e2ee connect 9999
```

# Implementation
i used X-Wing (X25519 + ML-KEM-768) as the post-quantum hybrid KEM, and ChaCha20Poly1305 for AEAD

the client and server exchange a shared secret using X-Wing, the client encrypts and sends 100mb of data using ChaCha20, then the server decrypts

using wireshark, i verified that the data transferred was encrypted

#### Sizes
```
32 byte private key
1216 byte public key
```

#### Performance
```
> 200mbps for encryption and decryption
< 0.5ms for one-time key sharing
```

# Output
#### Client output
```
successfully connected to server at 127.0.0.1:9999
took 317.647µs for client KEM
sent KEM ciphertext to server
took 381.041164ms to encrypt 100mb
sent encrypted data to server
```

#### Server output
```
127.0.0.1:34662 connected
received KEM ciphertext from 127.0.0.1:34662, deriving shared key
took 257.227µs for server KEM
finished reading encrypted data
took 389.462087ms to decrypt 100mb
data received was correct
```

# References
* [X-Wing paper](https://eprint.iacr.org/2024/039.pdf)
* [X-Wing spec](https://www.ietf.org/archive/id/draft-connolly-cfrg-xwing-kem-06.html)
