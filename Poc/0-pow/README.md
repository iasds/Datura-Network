# Building

```
sudo apt install cmake -y
cargo build --release
./target/release/pow
```

# PoC Output:

```
[user ~/Documents/Datura-Network.worm/Poc/4-pow/target/debug]% ./pow 
initializing dataset (only needs to be done once, at node startup)...
initialized, took 27.84s

created challenge of difficulty 0
solving challenge [00, 69, 20, D1, 9D, B3, 6D, 85, 88, FC, 8D, FF, F9, A3, 11, 13]
took 7.83ms to find solution (6C84F2E5AC067293)
took 3.48ms to validate solution

created challenge of difficulty 1
solving challenge [01, 23, FA, 9B, F9, 60, 13, 1A, 42, 7E, 13, 92, B2, 81, E1, 75]
took 63.73ms to find solution (76410609C91BE580)
took 3.55ms to validate solution

created challenge of difficulty 2
solving challenge [02, B3, 0C, B5, 58, 4C, 44, 84, 56, B1, 72, 05, 2E, 20, 92, 98]
took 22.56ms to find solution (7B409A96796F0091)
took 4.57ms to validate solution

created challenge of difficulty 3
solving challenge [03, 1F, 30, 42, E0, 7F, 70, A8, 96, EF, D3, BF, 06, 5D, BB, 2A]
took 70.56ms to find solution (79D67B7CB921D919)
took 3.53ms to validate solution

created challenge of difficulty 4
solving challenge [04, 59, 34, 50, 13, CB, BD, 32, 22, D3, A4, 7D, 3E, 68, 86, A5]
took 26.03ms to find solution (F603909752DC57BA)
took 4.59ms to validate solution

created challenge of difficulty 5
solving challenge [05, 1C, 20, F3, 7A, D0, C6, 29, F6, 66, 25, 6A, 72, E6, BF, 27]
took 150.68ms to find solution (4C7F4F5ADCEC585F)
took 4.29ms to validate solution

created challenge of difficulty 6
solving challenge [06, DB, A4, 31, BB, 13, 4C, 22, 23, 30, 27, CD, 20, 7D, 5F, BA]
took 191.74ms to find solution (98CDF9CD706B402A)
took 3.34ms to validate solution

created challenge of difficulty 7
solving challenge [07, A7, 1A, FA, 7F, 0A, 4E, 93, 2B, BC, C1, F4, D0, 9D, FA, EF]
took 1.09s to find solution (D538C1C8AD1EBCE6)
took 4.44ms to validate solution

created challenge of difficulty 8
solving challenge [08, 9B, 99, FD, 61, 3B, FE, 24, FD, 8F, 71, 76, F4, 61, B5, 65]
took 275.33ms to find solution (17731067A037B667)
took 3.60ms to validate solution

created challenge of difficulty 9
solving challenge [09, 95, 6C, BC, F4, 5F, 20, 4E, 9C, 9C, D8, 7A, 41, DD, A3, 9C]
took 3.31s to find solution (61C5CC2F7E9759B6)
took 4.24ms to validate solution

created challenge of difficulty 10
solving challenge [0A, 61, 94, D7, 9E, CB, 10, 4B, 9B, B6, 2B, CF, 60, F2, 71, 0C]
took 4.26s to find solution (D836DB6E218FCC00)
took 3.30ms to validate solution

created challenge of difficulty 11
solving challenge [0B, 94, A6, AD, 7C, 74, 1D, EF, B0, 08, A8, 5A, B1, 12, 6F, 7C]
took 49.69s to find solution (3C43F517085E0AC3)
took 3.60ms to validate solution

created challenge of difficulty 12
solving challenge [0C, 50, F3, 3E, 08, 42, 59, 5D, 63, C3, 48, 98, 3E, FB, E7, DC]
took 11.45s to find solution (3A379CDB4C5A7C57)
took 3.54ms to validate solution

created challenge of difficulty 13
solving challenge [0D, 07, 0B, 02, 76, 2F, AC, C5, A4, B8, F0, 2F, 13, 79, 1A, 71]
took 26.94s to find solution (9F37E0E64ABE184F)
took 4.18ms to validate solution

created challenge of difficulty 14
solving challenge [0E, C1, EC, AB, 67, DD, 1C, 95, B9, 5E, 8F, FF, 44, 2A, 03, A7]
took 23.71s to find solution (4032B90E0B9212EC)
took 4.84ms to validate solution

created challenge of difficulty 40
created random solution 2260828546387633553
solution correctly validated as false (false)
```
