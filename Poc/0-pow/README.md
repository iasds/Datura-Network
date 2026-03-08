# Building
```
doctor@dev:~$ rustc -O main.rs
```

# Notes

- self ip resolution needs to be decided, using public api can get blocked cause its using tor ips

- when connecting to a node the connecting node sends it listening port, we need to find a way to change this, reciving data from a connecting node can be an attack surface

## Arguments
- Port: The port that the node running will listen to.
- Node Address: The address and port of the node we want to connect to (ex: 127.0.0.1:8080).

# Input
## Creating a source node
```
doctor@dev:~$ ./main 8000
```

## Creating a bootstrap node
```
doctor@dev:~$ ./main 8010 127.0.0.1:8000
```

# Output
## Creating a source node
```
# Prints what port the node is listening on
Listening on port 8000

# Prints when new node is connected with ip and port
New node connected: 127.0.0.1:8010
```

## Creating a bootstrap node
```
# Prints that it has connected to a node
Connected to node localhost:8000

# Prints that its listening on a specific port
Listening on port 8010
```
