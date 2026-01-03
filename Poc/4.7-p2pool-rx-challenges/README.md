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
