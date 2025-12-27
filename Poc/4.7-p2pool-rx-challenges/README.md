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
got challenge JobData { blob: "1010eef8beca06b7321f463446f2bd2aa466e1cdf87afe9b6febc4eee6140da279804eb36db00e000000d96035af001ad0bd27e84ff17821ea10b7fe28363debcca2f172efdc7461abc9c21d", job_id: "40", target: "e52b0000", algo: "rx/0", height: 3574591, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }
No new job received in 5 seconds
got challenge JobData { blob: "1010eef8beca06b7321f463446f2bd2aa466e1cdf87afe9b6febc4eee6140da279804eb36db00e000000d96035af001ad0bd27e84ff17821ea10b7fe28363debcca2f172efdc7461abc9c21d", job_id: "40", target: "e52b0000", algo: "rx/0", height: 3574591, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }
got challenge JobData { blob: "1010f7f8beca06b7321f463446f2bd2aa466e1cdf87afe9b6febc4eee6140da279804eb36db00e000000d97af370c62fb3f27d7306bd4c56ea54e2c2adb927d186daae6b4a19babbe3f2fd1e", job_id: "41", target: "1b2a0000", algo: "rx/0", height: 3574591, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }
got challenge JobData { blob: "1010f9f8beca06b7321f463446f2bd2aa466e1cdf87afe9b6febc4eee6140da279804eb36db00e000000d9bd9222e49bcf5c3dfe8e901d43c4418a607a361398386ce3a2717743287da2cb1e", job_id: "42", target: "1b2a0000", algo: "rx/0", height: 3574591, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }
No new job received in 5 seconds
got challenge JobData { blob: "1010f9f8beca06b7321f463446f2bd2aa466e1cdf87afe9b6febc4eee6140da279804eb36db00e000000d9bd9222e49bcf5c3dfe8e901d43c4418a607a361398386ce3a2717743287da2cb1e", job_id: "42", target: "1b2a0000", algo: "rx/0", height: 3574591, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }

~~~

# usage

When working with other datura node you will likely want to give a varying difficulty based on your current resource usage, which will be a lot lower than the share or network difficulty.
As a node distributing PoW challenges it will thus be your responsibility to:
- check that the PoW response is actually valid for the difficulty target you set to your peer
- check whether the PoW response has a high enough difficulty to also qualify for upstreaming as a share
