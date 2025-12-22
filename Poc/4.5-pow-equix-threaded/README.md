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
    effort = (u32)(challenge[0..3])
    equix_hash = equix(challenge || salt)

    if blake2b_u32(challenge || salt || equix_hash ) * effort < U32_MAX
        return salt || equix_hash
    else
        salt += 1
        continue
```

Verification process
```
if (u32)(challenge[0..3]) != effort
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

