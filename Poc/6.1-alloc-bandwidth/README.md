# Random tests

Run to test randomness sources.

```
nix run .#benchmark
```

getrandom::fill() documentation says :

In general, `getrandom` will be fast enough for interactive usage, though
significantly slower than a user-space CSPRNG; for the latter consider
[`rand::thread_rng`](https://docs.rs/rand/*/rand/fn.thread_rng.html).

```
getrandom::fill      |   253.27 ns/call | 3.95 Mops/s
rand::rng            |     9.14 ns/call | 109.42 Mops/s
```

The benchmark should be run with `--release`! (`nix` does it automatically, `cargo` runs
in debug mode by default).

Indeed, `getrandom::fill` is 25x slower than `rand::rng`. There is a
[discussion](https://docs.rs/rand/latest/rand/rngs/struct.ThreadRng.html#Security)
weighing on `rand::rng`'s security. IMHO, this (way faster) random generator offers
sufficient security guarantees.
