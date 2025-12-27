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

# usage

When working with other datura node you will likely want to give a varying difficulty based on your current resource usage, which will be a lot lower than the share or network difficulty.
As a node distributing PoW challenges it will thus be your responsibility to:
- check that the PoW response is actually valid for the difficulty target you set to your peer
- check whether the PoW response has a high enough difficulty to also qualify for upstreaming as a share
