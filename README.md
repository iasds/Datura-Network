# Datura Network (WIP)

Datura Network is going to be a new Darknet (like [Tor](http://opbible7nans45sg33cbyeiwqmlp5fu7lklu6jd6f3mivrjeqadco5yd.onion/opsec/torhoneypot/) or I2P), that is going to be written in Rust. It will be designed in such a way that state-level threats of passive network analysis, active Sybil attacks and disruptive DDoS attacks are rendered useless to conduct against its very design.

![](daturasmall.png)

Datura Network will provide bi-directional anonymity on the TCP/IP layer **at all costs** (keeping both clients and servers anonymous), as long as it remains usable. We're going to implement every feature the other anonymity networks should've already implemented by now to protect against today's state-level threats landscape.

Please [read the specification document](spec/specification.md) to know where we're at on the brainstorming of the project, it'll get you up to speed.

## How can I contribute?

The contribution process heavily follows how we handle [contributions for the opsec bible](http://opbible7nans45sg33cbyeiwqmlp5fu7lklu6jd6f3mivrjeqadco5yd.onion/opsec/contribute/). Payments are as they are advertised on our [xmrbazaar listing](https://xmrbazaar.com/listing/cwGf/):

The rewards are based on the (justified) complexity tags on the project board http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/projects/19 (i pledge to not change those prices for the entirety of 2026)

- Simple (0.035 to 0.1402xmr)
- Doable (0.1402 to 0.2104xmr)
- Complex (0.2104 to 0.3508xmr)

First of all you can create a git account on the git datura instance ran by us by [signing up here](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/user/sign_up), (you don't need to mention a real email, there's no email verification anyway).

So you have 3 ways you can contribute to the project:

1) help us brainstorm the project: pick any issue on the [brainstorming project board](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/projects/16) and challenge it in case if we missed something
2) help us write the proof of concepts: simply ask to be assigned on any issue on the [PoC project board](http://gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.onion/nihilist/Datura-Network/projects/19) and we'll assign it to you with a deadline of 2 weeks if that's your first contribution, or 4 weeks if you have delivered before. **(and of course we'll pay you in monero if you complete a todolist for us)**
3) and lastly if you want to donate to us to fund this project you can donate to us at this XMR address: **84Zqdr7o2RfTKRhjc6SR3TdhK1yLxRLMPARU3PMvmyH8XmCgMoBHa7X8YoM7WphfbkJsjQ4SeEQCr4Nn2uzJSfCD9KiBu9E** (we're anyway going to reuse those funds to pay the developers on this project). When the project will be fully planned out, we'll also create a crowd fundraiser to ensure the developers are all properly paid for their work.

## What's the overall progress on the project?

For now we are still in the brainstorming and planning phase.

We are currently focusing on writing the proof of concepts to help us see more clearly what is possible and not possible.

Once all of the proof of concepts are written we'll be able to effectively write ALL of the todolists for all the features we want to have in the network, and fully plan out the project.

And Once the project is fully planned out, we'll be able to estimate the cost of building the network, and make a 

## Our SimpleX chatroom

If you have any questions on the project, don't hesitate to ask [in our simplex chatroom](https://dghh7wjyjrectfblqnyuy4mbshrybvlvnc5d5if5uyaceogtiiq5fnyd.onion/g#Th9RZoJW4_wESf2IrfnSWBmAsJi6NfJqmo2WF1OqgeE?p=443&c=9u8jZd9XK7B_jSBYgA3Dz1r1AMxAPgdgnwdHMO_v2FA).

# Roadmap (DRAFT):

```
DOING:
-Phase 1: (Visualizing)
    - Brainstorming how the network should work and it's features (to defend against all threats)
    - Brainstorming the threat model (Passive, Active and Forceful adversaries)

TODO:
-Phase 1.5: (Testing the Proof of Concepts)
    - Making a barebones binary with local socks5 proxying and tcp port binding to further explain the threat model (and what the adversary can see)
    - Making PoCs of the critical features of the network to clarify what's possible and what's not possible:
        - Vanity v3 hidden service names, with easy retrieval of hidden services' public keys (the hidden service name alone should be enough to get the public key of a hidden service, to encrypt traffic with it)
        - Rust TCP packet handling, E2EE encryption (with post quantum algos?), and decryption on the other end.
            -> Benchmarking how quickly we can recieve packets, decrypt them and afterward forward them to the next hop (ideally 3mbps in and 3mbps out!) (Node A -> Node B -> Node C)
        - Distributed Hash table: with the hash of a hidden service: quickly finding the closest nodes on a hashring
        - RandomX Challenges with variable difficulty (benchmarks on an average consumer laptop), and showcasing the challenge and solution format
        - Zero Knowledge Proofs (Input: rdv node hash, expiration timestamp, hidden service hash, validated randomX challenge. Output: ZKP (that must be verifiable as either valid or invalid with the same inputs by other nodes.)
        - Rust Libp2p UDP Hole punching for nodes behind NATs
        - Rust TCP packet traffic padding (both in packet sizes and in packet sending intervals)
        - Rust SOCKS5 Proxying : tcp + udp ?
        - Routing rules for nodes: packet recieved, packet decrypted, it says "this packet is for hash 44AWD", it matches a routing rule (packet for hash 44AWD = route this to node hash 88QWD) -> node routes packet to the node whose hash is 88QWD
- Phase 2: (Clarifying the Vision)
    - Writing the specification of all of the network features, 
        (once we have a clear idea of how the network should function with all the above PoCs (in text format))
- Phase 3: (Making the basic routing functionnality of the network)
    - v0.0.1 : Basic binary with local socks5 proxying, and tcp port binding
    - v0.0.2 : Nodes all have a default hidden service, and use that as their hash to identify themselves. (which is also used to recieve E2EE packets)
    - v0.0.3 : Nodes can be asked to give randomX challenges, and Nodes can be given those randomx challenges solutions, and they can tell if they're valid or invalid.
    - v0.0.4: nodes can ask nodes closest to the hidden service's hash (or BS hashes) to route packets for them either as rendezvous nodes or nodes in between RDVs and themselves. In exchange for valid RandomX solutions
    - v0.0.5 : Clients can decide where their circuits can go through (unidirectionnal streams), and the packets are encrypted using the destination node's public key.
    - v0.0.6 : Clients can ask destination nodes to route packets back through a rendezvous node that they themselves picked.
    - v0.0.7: Knowledge of a given hidden service's rendezvous node is propagating throughout the network (further outward from where rendezvous is, on the hashring), progressively. (the least likely place to find the knowledge is on the other side of the hashring.)
    - v0.1: Fully functionnal Hidden service DNS resolution and dual, uni-directionnal packet routing. (with double rendezvous points)
    - v0.1-6: Making the network protect against all traffic analysis threats:
        - (Packet padding, decoy destinations, + E2EE, etc)
    - v0.6-9: Network protects against both DDoS and sybil attacks: (decentralized verifying trust system)
Phase 4: Stressnet phase
    - v0.9.0-5: Double and triple check that all attacks are neutralized, ideally with an independant audit
        -> everyone must behave like the adversary would behave and document what they find along the way
    - v0.9.5-9: fix all the weaknesses discovered by the above audits
Phase 5: Network Release
    - v1.0.0: ramping up the SEO as much as possible
```

## License - CC0

This project is under the CC0 License - Public Domain
