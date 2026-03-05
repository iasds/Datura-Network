# Running
```
cargo run --release
```

# Notes
Sizes
```
16 byte challenge
24 byte solution
```

API
```
create_challenge(effort: u32) -> challenge: u128
solve_challenge(num_threads: usize, challenge: u128) -> solution: [u8; 24]
verify_solution(effort: u32, challenge: u128, solution: [u8; 24]) -> result: bool
```

## Effort (difficulty)
Effort parameter determines difficulty on a linear scale from 0 to 4294967295, the probability of success each try should be about 1/effort (0 effort is 100%)

Effort difficulty is imposed by setting a requirement on the max value of the output of the hash interpreted as a u32
```
hash_u32 * effort < U32_MAX
```
If effort is 0, this will always succeed, if effort is 2, then hash_u32 must be less than half of U32_MAX, so half of the values will fail, if effort is 3, then two thirds will fail. In this way the probability of success is influenced by the effort parameter

## Implementation
Challenge format (16 bytes)
```
effort 4 bytes
random 12 bytes
```

Solution format (24 bytes)
```
salt 8 bytes
equix_hash 16 bytes
```

Solving process
```
loop
    salt = random()
    effort = (u32)(challenge[0..=3])
    equix_hash = equix(challenge || salt)

    if blake2b_u32(challenge || salt || equix_hash ) * effort < U32_MAX
        return salt || equix_hash
    else
        salt += 1
        continue
```

Verification process
```
if (u32)(challenge[0..=3]) != effort
    return false

if blake2b_u32(challenge || salt || equix_hash) * effort >= U32_MAX
    return false

return equix_verify(challenge || salt, equix_hash)
```

## Current Improvements
Now multi-threaded, also Equi-X uses ~1000x less RAM and has ~50x faster verification over RandomX

## Future Improvements
With a little added complexity we could save < 0.05ms per thread by not spawning them every time

## References
* [tor PoW](https://github.com/torproject/torspec/blob/main/proposals/327-pow-over-intro.txt)
* [drillx PoW](https://github.com/regolith-labs/drillx)

# Output
## 1 thread test:
```
How many threads should i use? (Press Enter for default of 2): 1

Running with 1 thread(s)

Solving 32 challenges of effort 0 ................................
Solution time avg: 7.92ms
Verification time avg: 82.07µs

Solving 32 challenges of effort 64 ................................
Solution time avg: 353.80ms
Verification time avg: 75.57µs

Solving 32 challenges of effort 128 ................................
Solution time avg: 1.03s
Verification time avg: 75.05µs

Solving 32 challenges of effort 192 ................................
Solution time avg: 1.33s
Verification time avg: 76.69µs

Solving 32 challenges of effort 256 ................................
Solution time avg: 2.21s
Verification time avg: 85.39µs

Solving 32 challenges of effort 320 ................................
Solution time avg: 2.32s
Verification time avg: 82.26µs

Solving 32 challenges of effort 384 ................................
Solution time avg: 2.87s
Verification time avg: 83.37µs

Solving 32 challenges of effort 448 ................................
Solution time avg: 4.54s
Verification time avg: 86.56µs

Created challenge of effort 10000
Created random solution [DC, 64, 22, 42, 58, 9B, 44, B7, 60, 9A, FC, 04, EE, 81, 33, 80, B5, 88, A8, 83, 55, 4A, 35, 02]
Solution correctly verified as false (false)
```

## 2 threads test:
```
[user ~/Documents/Datura-Network.worm/Poc/4.5-pow-equix-threaded]% cargo run --release
How many threads should i use? (Press Enter for default of 2): 2

Running with 2 thread(s)

Solving 32 challenges of effort 0 ................................
Solution time avg: 11.90ms
Verification time avg: 81.19µs

Solving 32 challenges of effort 64 ................................
Solution time avg: 276.24ms
Verification time avg: 68.23µs

Solving 32 challenges of effort 128 ................................
Solution time avg: 462.52ms
Verification time avg: 71.47µs

Solving 32 challenges of effort 192 ................................
Solution time avg: 770.23ms
Verification time avg: 70.58µs

Solving 32 challenges of effort 256 ................................
Solution time avg: 941.10ms
Verification time avg: 71.38µs

Solving 32 challenges of effort 320 ................................
Solution time avg: 1.15s
Verification time avg: 70.39µs

Solving 32 challenges of effort 384 ................................
Solution time avg: 2.41s
Verification time avg: 74.24µs

Solving 32 challenges of effort 448 ................................
Solution time avg: 1.90s
Verification time avg: 71.61µs

Created challenge of effort 10000
Created random solution [6B, 1B, 32, 27, 69, A3, 87, 0B, 1C, 7F, 7C, E4, CD, 2F, A2, 8E, 21, 0D, F3, 86, 6B, 5F, 75, 0F]
Solution correctly verified as false (false)
```
