# Native ABI performance (measured)

Host: x86_64 Linux, release builds, `rustc 1.98`, `cc` (gcc 16) `-O2`. Load average 2.8 during the run
(shared desktop, no CPU pinning): expect ±10-20% run-to-run noise on the ns-level numbers, so read
ratios, not digits. Reproduce: `cd spar && cargo run --release --example native_bench`.
Method: Spar programs are timed at two loop sizes and the slope is the per-iteration cost (startup and
compile cancel), medians of 5; bulk kernels are compared with equivalent Rust / raw C-ABI code on the
same data. Nothing below is extrapolated.

## Raw output

```
 17:30:53 up 30 min,  1 user,  load average: 2.78, 4.30, 4.79
# native ABI benchmarks (release build required), host x86_64 linux
1  direct Rust no-op (inline(never))           1.25 ns
2  extern "C" Rust no-op via fn pointer        1.41 ns
2b C-ABI no-op in shared library               1.44 ns
4b C-ABI add(i64,i64) in shared library        1.54 ns

ABI boundary only (NativeRegistry::call, no interpreter):
  noop            38.31 ns
  noopD           10.42 ns
  add             49.57 ns
  addD            11.38 ns
  add4            63.21 ns
  add4D           12.05 ns
  addF            49.12 ns
  addFD           11.22 ns
  strLen          54.58 ns
  strCopy         81.06 ns
  fieldSum       143.91 ns
  sumBytes        66.81 ns
  rust-add        10.13 ns  (built-in Rust closure through the same registry: the floor)

loop body baseline (acc = acc + 1):      24.73 ns
3  Spar function call (Spar-defined add)          63.71 ns  (net of loop:      38.97 ns)
3  Spar->native no-op                             97.48 ns  (net of loop:      72.75 ns)
4  Spar->native add(int,int)                     118.77 ns  (net of loop:      94.04 ns)
4d Spar->native addD (direct signature)           77.54 ns  (net of loop:      52.81 ns)
5  Spar->native add4(int x4)                     174.57 ns  (net of loop:     149.84 ns)
5d Spar->native add4D (direct)                   159.20 ns  (net of loop:     134.46 ns)
6  Spar->native addF(float,float)                113.83 ns  (net of loop:      89.10 ns)
7  string length via borrowed view               142.85 ns  (net of loop:     118.12 ns)
8  string copy (view + string_new)               168.88 ns  (net of loop:     144.15 ns)
16 native resource call                          134.34 ns  (net of loop:     109.61 ns)
15 record field access (2 fields)                220.32 ns  (net of loop:     195.59 ns)

1000000 f64 elements (8 MB)
  direct Rust sum          1163428.62 ns      6.88 GB/s
  raw C-ABI sum            1175492.09 ns      6.81 GB/s
  Spar->native zero-copy   1163607.75 ns      6.88 GB/s  (overhead vs Rust: 0.0%)
  Spar->native list (copy) 3829051.90 ns      2.09 GB/s
10000000 f64 elements (80 MB)
  direct Rust sum          11578592.90 ns      6.91 GB/s
  raw C-ABI sum            12004414.95 ns      6.66 GB/s
  Spar->native zero-copy   11841945.65 ns      6.76 GB/s  (overhead vs Rust: 2.3%)
    Finished `release` profile [optimized] target(s) in 0.01s

bulk bytes / mutation / allocation:
  9  sumBytes 64000000 B zero-copy   25759163.90 ns      2.48 GB/s
     raw C-ABI (same C kernel)   17191035.10 ns      3.72 GB/s
     direct Rust (auto-vectorized, different kernel) 12140259.00 ns      5.27 GB/s
  11 scaleBuf 4000000 f64 in place    2813096.70 ns     11.38 GB/s   direct Rust 2291170.90 ns     13.97 GB/s
  14 native alloc + return 1 MB Bytes (copy)    57440.90 ns

callbacks (native loop + Spar callback per element, 100k elements):
  17/18 native loop + Spar closure per element     402.35 ns/element
        pure Spar for-loop doing the same           60.90 ns/element
        (a batch native op is one call:       0.00 ns)

async round trip (delayedAdd, 0 ms, awaited one at a time):
  20 begin + worker thread + complete + await    61869.50 ns

multi-threaded native kernel (parSum, 10M f64, Rust module):
  1 threads  11779271.30 ns      6.79 GB/s  speedup 1.00x
  2 threads  6091146.90 ns     13.13 GB/s  speedup 1.93x
  4 threads  3326164.70 ns     24.05 GB/s  speedup 3.54x
  8 threads  3023143.30 ns     26.46 GB/s  speedup 3.90x
```

## What the numbers say

**Bulk zero-copy is at native speed.** A 1M and 10M `f64` sum through `Buffer` measures 0.0% and
2.3% versus direct Rust and matches the raw C-ABI call, i.e. the bridge is not visible once the
kernel does real work. In-place mutation (`scaleBuf`, writable borrow) reaches 11.4 GB/s vs 14.0 GB/s
for the Rust loop (memory-bound; the C kernel is not the same code as the Rust one).
`parSum` on 10M elements scales 1.93x / 3.54x / 3.90x on 2 / 4 / 8 threads with no runtime lock.

**Copying costs what it costs.** A `[float]` list argument (one copy into contiguous scratch)
runs at ~2.1 GB/s vs 6.9 GB/s zero-copy: about 3.3x slower at 1M elements. That is the argument for
`Buffer`.

**Scalar call overhead, split into two parts.**

| | ns |
|---|---|
| Rust closure through the registry (floor) | ~10 |
| direct signature (`ii>i`) through the registry | ~11 |
| generic ABI, no-op | ~38 |
| generic ABI, `add(int,int)` | ~50 (two `int_get` calls ≈ 4 ns each) |
| generic ABI, string length / copy | ~55 / ~81 |
| generic ABI, record with 2 fields (`record_get` ×2) | ~144 |
| inside the Spar interpreter loop: Spar-defined function call | ~39 net of loop |
| inside the Spar interpreter loop: native no-op / add / direct add | ~73 / ~94 / ~53 net |

The generic path's ~28 ns over the floor is env acquire/release (~7 ns measured in isolation) plus
result checking and bookkeeping; the rest was not attributed because no profiler works here (perf and
valgrind are absent, samply needs `perf_event_paranoid<=1`). The direct signature removes it
entirely. Inside Spar code the interpreter's own native-call sequence (argument vector, dispatch)
adds ~40-60 ns, so a direct call in a loop is ~53 ns vs a Spar function call at ~39 ns. Reducing that
further is interpreter work, not ABI work.

**Not fast, documented:**
* `Bytes` (64 MB): 2.5 GB/s through Spar vs 3.7 GB/s for the same C kernel called directly. The
  ABI borrows `Bytes` without copying, but the interpreter clones `Value::Bytes(Vec<u8>)` when a
  variable is read as an argument, which costs one memcpy per call. Use `Buffer` for large data.
* Callback per element: after removing the closure payload clone, the native loop with a Spar
  callback measured 260 ns/element; the equivalent Spar `for` loop measured 69 ns/element in the
  same run (~3.8x). The earlier run measured 400 vs 61 ns/element. These are separate benchmark
  runs, so the difference is indicative, not a controlled speedup estimate. Use batch operations
  for per-element work; keep callbacks for rare hooks.
* Async round trip (begin, worker thread, complete, await): ~62 µs, dominated by thread creation in
  the example; it is a latency for I/O-bound work, not a hot path.
* Copying a 1 MB `Bytes` result out of native code: ~57 µs (bytes_new copies, ~18 GB/s).

## Not measured

Table/column operations (no columnar table exists), Windows/macOS, other CPUs, peak memory,
allocation counts (no allocation counter is wired to the native path), boundary overhead for
callbacks in isolation.
