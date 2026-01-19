# Entering the shell

My preferred method is using [direnv](https://direnv.net/).

However, one can also enter using

```
nix --extra-experimental-features "nix-command flakes" shell
```

This will make all the build and test dependencies available.

# Testing

## Starting the server

Start the server, and measure throughput. `nix run` starts the server, and `pv` is a
command-line utility for checking a pipe's bandwidth.

```bash
nix run | pv -am5 > /dev/null
```

This should show something like this:

```
(0.00  B/s)
```

This shows the bandwidth of data passing through the server.

## Sending data

Send data to the data port in another terminal, after the server is started. `socat` is included in the nix flake.

```bash
socat FILE:/dev/zero TCP:127.0.0.1:9977
```

This terminal will stay silent.

Over the first terminal, you can see the data cap at ~10KiB/s.

## Unlocking the bandwidth

In a new terminal, run the (validation) client.

```bash
nix run .#client
```

Once the client is done (this will take some time, 5 to 10 minutes), you will see the
bandwidth drastically go up in the first terminal, to about 1MiB/s, and then down again
(hitting the 100mb cap).

```
initializing dataset (only needs to be done once, at node startup)...
initialized, took 205.66s

Challenge received.
Challenge has been solved, and throttling lifted.
```

# Conclusion

This PoC builds upon Poc-4 to allow to throttle bandwidth. It uses a leaky bucket
algorithm, to slowly allow bandwidth to recover from spikes (without having a choppy
network).

One of the big challenges was to start the RandomX VM, that can't be moved across
threads. The best solution was to place it in its own thread. Another (positive) side
effect of this decision is that the server can listen to connections while the dataset
warms up (even though it can't validate bandwidth cap requests yet).

In the end, this PoC has been successfully implemented as specified.
