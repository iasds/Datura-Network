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
getrandom::fill      |   247.25 ns/call | 4.04 Mops/s
rand::rng (single)   |     9.48 ns/call | 105.47 Mops/s
rand::rng (rebuilt)  |    10.66 ns/call | 93.85 Mops/s
```

The benchmark should be run with `--release`! (`nix` does it automatically, `cargo` runs
in debug mode by default).

Indeed, `getrandom::fill` is 25x slower than `rand::rng`. There is a
[discussion](https://docs.rs/rand/latest/rand/rngs/struct.ThreadRng.html#Security)
weighing on `rand::rng`'s security. IMHO, this (way faster) random generator offers
sufficient security guarantees.

Additionally, we can see recreating `rand::rng()` on each iteration is not very costly
(1 ns/call), so this is not a big concern.
