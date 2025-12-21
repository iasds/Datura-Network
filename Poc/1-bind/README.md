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
