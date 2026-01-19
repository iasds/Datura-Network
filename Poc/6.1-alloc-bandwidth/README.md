# Random tests

Run to test randomness sources.

```
cargo run --bin benchmark
```

getrandom::fill() documentation says :

In general, `getrandom` will be fast enough for interactive usage, though
significantly slower than a user-space CSPRNG; for the latter consider
[`rand::thread_rng`](https://docs.rs/rand/*/rand/fn.thread_rng.html).


However, on my system, `rand::rng` (new name of `thread_rng`) performs more than 2.5
times (!) slower. I'd like inputs from some other systems, to test if I'm an oddity.

```
getrandom::fill      |   294.05 ns/call | 3.40 Mops/s
rand::rng            |   768.22 ns/call | 1.30 Mops/s
```
