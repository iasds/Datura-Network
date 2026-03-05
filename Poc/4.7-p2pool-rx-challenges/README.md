# Rationale
In order to create incentives we want to be able to send PoW challenges that could actually end up as shares in p2pool or even as mined monero blocks

# Prerequisites
For this to work you need a running p2pool instance (or xmrig-proxy) on localhost port 3355.

If you have a mining node with p2pool you can run

~~~
ssh -NL 3355:127.0.0.1:3333 myuser@myp2poolnode
~~~

To create a tunnel to the appropriate local port.

Alternatively you can also edit the source to change the port/host used.

# output

The client will obtain new jobs from p2pool consisting of:
- seed hash (for randomX cache/dataset initialization)
- blob (ready to use input you can update with your nonces)
- target difficulty: difficulty to reach if you want to get a share accepted

## example

~~~
user@node:~$ nix develop #enter devshell
user@node:~$ cargo test -- --no-capture
~~~

### output

~~~
running 2 tests                                                                                                       
diff to target for min: ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
test solver::utils::tests::test_diff_to_target ... ok
proptest: FileFailurePersistence::SourceParallel set, but failed to find lib.rs or main.rs
test solver::models::test_get_nonce ... ok                                                                            
                                                           
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running tests/local_light_single_thread.rs (target/debug/deps/local_light_single_thread-46b7bc99a6e7fc3e)

running 1 test                                                                                                        
creating a single thread light solver
creating client for local pow gen                   
solving jobs as fast as we can (each . is a new job, is $ is a valid solution produced
.$.$.$.$.$.$.$.$.$.$test run_single_thread_light_10_pows ... ok
                                                           
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.54s
                                                           
   Doc-tests xmr_pow_challenges      
                                                           
running 0 tests                            
                                                                                                                      
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

~~~

# usage

When working with other datura node you will likely want to give a varying difficulty based on your current resource usage, which will be a lot lower than the share or network difficulty.
As a node distributing PoW challenges it will thus be your responsibility to:
- check that the PoW response is actually valid for the difficulty target you set to your peer
- check whether the PoW response has a high enough difficulty to also qualify for upstreaming as a share

# Documentation

## Requirement
- nix package manager (apt install nix)

## Building
~~~
nix build .#doc
~~~

## Access
Point your browser to the crate doc

~~~
firefox result/doc/xmr_pow_challenge/index.html
~~~

# Conclusion

This POC experimented with the following idea: in order to create useful proof of work, why not take them from p2pool jobs? The main difficulty encountered was that p2pool jobs, even from nano pool, are incredibly difficult to solve in a very short time (which is expected), given that for the project a client would need to be able to connect in a few seconds at most, having them spend a day or more (depending on their hashrate) solving PoW wouldn't be advisable.


Instead of doing that, this PoC creates essentially a Stratum Proxy, like running an xmrig-proxy. For the demo the consts showcase the lowest possible difficulty.

## How difficulty and work relate

To find a PoW satisfying a specific difficulty takes an amount of time at a fixed hashrate proportional to said difficulty. Mining for oneself (lottery mining) takes usually a long time before bringing in a reward, pool mining uses a share mechanism for this. One miner mines against a pool difficulty (which is lower than the network's) and sends upstream shares that satisfy the pool's difficulty target. Most of the time those shares aren't good enough to satisfy the network target but they show that the client worked and deserves a part of the reward should one block be found by the pool.

In the present case the PoC reproduces this mechanism and drastically scales it, each node runs its own sub-pool of the p2pool nano (or mini or main depending on what chain you are running against) and each PoW represents the node asking from other networks participants for a share.

Most of the time, shares submitted back for PoWs are too low difficulty to even qualify as p2pool shares, but they are quickly calculated and their difficulty scaling will allow for market effects (higher load: higher difficulty, more profitable for the running node).


Different configurations are supported, light mode (slow, perfect for low-power mining, 256Mb of memory consumption), fast mode (2Gb required, faster mining) and complete async and parallelism are supported for easier integration. API is built to allow the user to choose how many threads to run and use them efficiently in an async manner.
