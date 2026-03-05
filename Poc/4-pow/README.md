# Building

```
sudo apt install nix -y
nix run
```

# Rationale
To create incentives for running nodes this PoC means to trade monero mining hashrate for all the tasks requiring PoW on the datura network.
We are using randomX to test the creation of a challenge and validation of a solution. this POC also allows to run in either fast or light mode.

# PoC Output:

```
[user ~/Documents/Datura-Network.worm/Poc/4-pow/target/debug]% nix run -- --light
created challenge of difficulty 0
solving challenge [00, 0D, 6E, F3, 2E, E0, 6E, 75, 11, 5B, 73, FB, 70, 11, B6, 16]
took 82.05ms to find solution (46E138B7FC1B7E99)
took 38.78ms to validate solution

created challenge of difficulty 1
solving challenge [01, AC, C9, 6F, 23, BD, 98, 95, 9A, 1D, 19, 2D, 04, 9A, 92, 11]
took 119.86ms to find solution (B621F51A8A0E1B33)
took 42.49ms to validate solution

created challenge of difficulty 2
solving challenge [02, 54, 4E, 84, 34, 8A, 29, 02, 96, 39, 88, C8, 05, C4, 12, 70]
took 46.38ms to find solution (A8B52E25DB30EBCC)
took 45.41ms to validate solution

created challenge of difficulty 3
solving challenge [03, 1C, A8, 72, D6, 4A, 6E, DB, B9, 27, BA, 99, B6, F2, 16, CB]
took 1.70s to find solution (32BAED5408C1F432)
took 41.84ms to validate solution

created challenge of difficulty 4
solving challenge [04, F2, 8C, 6C, E3, 2D, AA, A4, 2E, 55, 68, 3E, 8C, C4, 92, 22]
took 1.75s to find solution (5A78EB12BD763858)
took 45.34ms to validate solution

created challenge of difficulty 5
solving challenge [05, D2, B4, F3, 30, CB, 69, 4C, 9D, 6C, C3, 7F, 84, 3E, 7E, D1]
took 5.77s to find solution (EF8872B05864F368)
took 40.05ms to validate solution

created challenge of difficulty 6
solving challenge [06, 4D, 1D, CA, F1, 96, 9F, C5, 0E, A7, 7E, CB, 5D, 8D, BD, EC]
took 522.64ms to find solution (570EFDDC93D92BB4)
took 42.66ms to validate solution
```
