# Benchmarks
This README explains how to run benchmarks to compare the performance of `evm_mlir` and `revm` when running different contracts. The benchmarking tool used to gather performance metrics is [hyperfine](https://github.com/sharkdp/hyperfine), and the obtained results will be included for reference.

To run the benchmarks (from the crate's root):
```bash
make revm-comparison
```

## Factorial
This program computes the nth factorial number, with n passed via calldata. We chose 1000 as n and ran the program on a loop 100,000 times.

These are the obtained results:

### MacBook Air M1 (16 GB RAM)
|            |     Mean [s]     | Min [s] | Max [s] |  Relative   |
|------------|------------------|---------|---------|-------------|
| `evm_mlir` | 1.114 s ±  0.015 |  1.100  |  1.151  |    1.00     |
|  `revm`    | 6.735 s ±  0.125 |  6.617  |  7.002  | 6.04 ± 0.14 |

## Fibonacci
This program computed the nth Fibonacci number, with n passed via calldata. Again, we chose 1000 as n and ran the program on a loop 100,000 times.

These are the obtained results:

### MacBook Air M1 (16 GB RAM)
|            |     Mean [s]     | Min [s] | Max [s] |  Relative   |
|------------|------------------|---------|---------|-------------|
| `evm_mlir` | 1.048 s ±  0.017 |  1.027  |  1.073  |    1.00     |
|  `revm`    | 6.182 s ±  0.030 |  6.147  |  6.221  | 5.90 ± 0.10 |
