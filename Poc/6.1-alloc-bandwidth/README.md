This PoC heavily builds upon 6-throttling. Most of the documentation is located in it.

There are some architectural changes (the code has been heavily refactored), but the
interface stays the same.

Some notable differences:

- 6-throttling serves a random new challenge on each connection, 6.1-alloc-bandwidth
  saves a challenge for 24h.
- 6-throttling uses `getrandom::fill()` to generate challenges, while
  6.1-alloc-bandwidth uses `rand::rng().fill()` (rationale below)
- The proof of work's difficulty of 6.1-alloc-bandwidth is not a fixed constant, but
  varies according to the total bandwidth of the node.

## Difficulty

The difficulty increases this way:
- Until reaching 90% of bandwidth occupation, the difficulty stays at a defined
  constant (currently 12).
- Over 90%, the (average) time to calculate a solution will double for every additional
  10% of bandwidth used.
  
# Testing

## Testing the bandwidth

We reproduce the same setup as 6-throttling.

However, we can change some variables :

```rs
pub const NODE_BANDWIDTH: usize = 850 * 1024; // 850kb
const NORMAL_DIFFICULTY: u8 = 3;
const DATA_CAP: usize = 5 * 1024 * 1024; // 5mb
```

This way, a single connection will be able to fill the node's bandwidth, be quickly
rejected, and iterate over the PoW requests.

Running the client in a loop

```
while true; do nix run .#client; done
```

We observe that the bandwidth grows quickly to a point of stability.

```
Challenge received.
Difficulty is 6.
Challenge has been solved, and throttling lifted.
Challenge received.
Difficulty is 7.
Challenge has been solved, and throttling lifted.
Challenge received.
Difficulty is 8.
Challenge has been solved, and throttling lifted.
Challenge received.
Difficulty is 7.
Challenge has been solved, and throttling lifted.
Challenge received.
Difficulty is 8.
Challenge has been solved, and throttling lifted.
Challenge received.
Difficulty is 8.
Challenge has been solved, and throttling lifted.
```

Here, it oscillates between 7 and 8, before settling on 8.

## Random tests

Run to test randomness sources.

```
nix run .#benchmark
```

`getrandom::fill()` documentation says :

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

# Conclusion

The improvements made over 6-throttling make sense, and it's the start of something that
could reasonably used in the final project. The challenging part here was mainly to
manage the difficulty according to the bandwidth.

This PoC is successfully implemented.
