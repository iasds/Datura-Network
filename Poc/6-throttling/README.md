# Entering the shell

My preferred method is using [direnv](https://direnv.net/).

However, one can also enter using

```
nix --extra-experimental-features "nix-command flakes" shell
```

This will make all the build and test dependencies available.

# Testing

Start the server, and measure throughput. `nix run` starts the server, and `pv` is a
command-line utility for checking a pipe's bandwidth.

```bash
nix run | pv -pam5 > /dev/null
```

Send data to the data port in another terminal, after the server is started. `socat` is included in the nix flake.

```bash
socat FILE:/dev/zero TCP:127.0.0.1:9977
```

Over the first terminal, you can see the data cap at ~10KiB/s.

In a new terminal, run the (validation) client.

```bash
nix run .#client
```

Once the client is done (this will take some time, 5 to 10 minutes), you will see the
bandwidth drastically go up in the first terminal, then down again (hitting the 100mb
cap).
