/*!
  This crate proposes an approach to the pow based resource management (issue 111, poc21)

# Main idea
When allocating resources (eg: Bandwidth, memory) one opens oneself to starvation based attacks
where malicious peers would seek to grab as much as possible in order to deny those resources
to legitimate users.

## Fundamental barrier
To tackle this kind of behaviour, the datura net project opted for a PoW mechanism. This has
also the added benefits of creating economic incentives to run notes. All interactions are
gated behind PoWs, and the higher the difficulty the more resource you can obtain.

## Issues
- **Weak devices**
  - Weak devices with low hashpower would be at a disadvantage for resource allocation if only
    raw hashing power is considered
- **Very Strong devices**: a determined attacker can allocate high hashrate to try and monopolize
  resources on a node
- **Botnets**: botnets of very weak devices could coordinate to attack nodes in order to starve legitimate users

## Principes
This crate implements the following principes in order to counter the three issues above:

- **Tenure**: the longer a device stays connected with a node and behaves in a trustworthy manner the more resources
  they can be allocated (within limits) even though they are not very powerful => *solves the legitimate weak device issue**
- **Sublinear normalization**: all contributions are normalized with an exponent < 1 in order to prevent high disparity in power to create
  monopolies.
- **Reserve resource**: a percentage of resource is kept from normal allocation to ensure that all clients qualifying will receive some resources**
*/


pub mod resource_manager;
pub mod consts;
