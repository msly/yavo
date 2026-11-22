# Small Yavo / C performance suite v1

This is a four-case implementation pilot, not the frozen formal v1 performance gate.
The manifest, algorithms, input sizes, two warmups and seven measured samples are
fixed before timing. The Yavo compiler's safety checks remain enabled.

## Edited-source build latency: runtime PCH pilot (2026-11-22)

The separate [`pch_probe.py`](pch_probe.py) compares Release builds after edits:
each pair compiles **the same source bytes** using the cached Zig runtime PCH or
direct header compilation (`YAVO_NO_PCH=1`). Both arms have independent Yavo
cache roots; every new source prepends one more comment line to force a true
backend cache miss. The order alternates, and PCH cache hits and equal binary
SHA-256 digests are asserted. Each pilot uses seven paired samples per program:

| Program | First direct/PCH P50 | Final-compiler direct/PCH P50 | Final speedup |
| --- | ---: | ---: | ---: |
| integer_recurrence | 253.8 / 179.8 ms | 252.5 / 180.8 ms | 1.40x |
| vec_transform | 260.3 / 204.1 ms | 256.1 / 190.6 ms | 1.34x |
| mutref_scatter | 264.8 / 196.9 ms | 271.0 / 193.0 ms | 1.40x |
| deque_ring | 283.8 / 195.3 ms | 271.2 / 213.8 ms | 1.27x |

Reproduce from the repository root using the server's Python 3.13 environment:

    python3 benchmarks/small-c-v1/pch_probe.py --samples 7 --output benchmarks/small-c-v1/results/runtime-pch-current.json

Raw timings and compiler/runtime SHA-256 identities are in the
[first measurement](results/runtime-pch-2026-11-22-initial.json) and the
[final-compiler measurement](results/runtime-pch-2026-11-22.json). The latter
also records its script SHA-256. This is **build latency**, not program runtime.
In both pilots three of four PCH P50s were under 200 ms, but the case exceeding
the target changed (`vec_transform` first, `deque_ring` second). The 200 ms gate
is not consistently met and this is not cross-platform acceptance evidence.

## Cases and comparability

| Case | Timed input (n, work, seed) | Shared algorithm and layout |
| --- | --- | --- |
| integer_recurrence | 15000000, 1, 17 | Signed int64 modular recurrence and dependent checksum |
| vec_transform | 524288, 192, 17 | Contiguous int64 values, in-place modular transformation, final checksum |
| mutref_scatter | 262144, 15000000, 17 | Same seeded index recurrence, returned element pointer/reference, final checksum |
| deque_ring | 4096, 20000000, 17 | Same ring-buffer layout and doubling growth, front-pop/back-push, final drain |

The C files are independent small implementations, without the Yavo runtime header.
Both Vec descriptors contain pointer/length/capacity, and both Deque descriptors add
head in the same order. Element width is 64 bits. Capacity starts at four and doubles.
The ring's relocation copies its two contiguous runs; popped slots are cleared.
The measured C hot arithmetic/index operations are unchecked. Accepted input bounds
keep all C operations defined: the largest recurrence product is below 3.6e15,
and aggregate checksums are below 1.5e12. Yavo retains its ordinary bounds,
overflow, capacity and allocation checks.

Runtime inputs prevent constant-folding the entire computation. The output checksum
depends on the work and is checked for every measured run. A separately compiled
Rust implementation using ordinary Vec/VecDeque is a correctness oracle only:
three small inputs per case plus the full timed input must match in Yavo and C.
The reference's timing is excluded.

## Reproduce on the project server

From the repository root:

    perl benchmarks/small-c-v1/run.pl --report benchmarks/small-c-v1/results/current.json

Optional arguments are --compiler-repo PATH, --out NEW_DIRECTORY and --cpu NUMBER.
The runner requires Linux, Perl core modules, timeout/taskset, the pinned Zig
installation, and the Rust/linker setup described in HANDOFF.md. It invokes an
offline locked release Cargo build before measuring, and fails if compiler inputs,
benchmark inputs or the compiler executable change during the run. It refuses
YAVO_CC and YAVO_ALLOC_CHECK overrides.

The compiler executable, all compiler source inputs, runtime, toolchain lock,
actual Zig/Rust executables, benchmark sources, generated C, binaries and raw
stdout/stderr logs receive SHA-256 records. Each command records argv, elapsed
time and exit status. Logs and binaries remain below compiler/target; JSON reports
are portable summaries with explicit absolute paths to those retained artifacts.

Both measured languages use the pinned zig cc with C17, -fno-strict-aliasing,
-target x86_64-linux-musl, -O2 and -s. Each executable is checked for ELF64/x86_64,
absence of dynamic/interpreter program headers and absence of a full symbol table.
Both link the same static musl allocator. No sanitizer or allocation counter runs
inside the performance measurement.

## Measurement scope

A measured child runs on one allowed CPU (last allowed CPU by default). C/Yavo
order alternates within each pair. Timing uses CLOCK_MONOTONIC and includes
process startup, argv parsing, allocation, final output and teardown. Argument
processing differs between the languages; the workloads are chosen to make that
fixed cost small, not to remove it from the measurement. The shared VM's frequency,
scheduler and other workloads are not controlled; raw samples, ranges, CPU model,
affinity and load averages are retained.

Each language/case also has a first-observed release build and five subsequent
warm release builds. "First observed" is not "cold": existing Zig caches are not
cleared. The report gives P50 and nearest-rank P95 descriptively; five samples are
insufficient for a stable tail estimate. No debug/cold compilation result is claimed.

The geometric mean is computed over per-case median(Yavo)/median(C), with every
ratio and the worst case retained. This suite excludes strings, maps, general
allocation workloads, concurrency, FFI, Arena and other operating systems. Its
results cannot establish the formal near-C goal or a language-wide performance
percentage. Use regressions or unusually slow cases to choose the next measurements.

## Results

Measured reports are stored in results/. The source revision, compiler executable
hash and all input hashes in each report identify what was actually tested.

### Initial implementation baseline (2026-11-21)

The first run used worktree commit 39254e4, whose compiler/runtime inputs are
identical to f5469dc. The four-case runtime suite was committed before timing.
Full raw measurements and 385 unique referenced file hashes were checked in
results/f5469dc-baseline.json. These figures are a baseline for the forthcoming
Arena/module changes, not measurements of those changes.

| Case | Yavo median ms | C median ms | Yavo / C | Yavo warm build P50 ms | Stripped Yavo / C bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer_recurrence | 70.134 | 74.317 | 0.944 | 217.006 | 16896 / 21576 |
| vec_transform | 160.242 | 129.270 | 1.240 | 218.753 | 17952 / 24200 |
| mutref_scatter | 100.450 | 103.648 | 0.969 | 224.323 | 18048 / 24120 |
| deque_ring | 128.784 | 114.929 | 1.121 | 260.611 | 18208 / 24424 |

The four median ratios have geometric mean 1.062; the worst case is vec_transform
at 1.240. This pilot's four warm build medians exceed the 200 ms target. Small
static binary sizes satisfy the numerical size threshold only for these programs;
C uses libc printf while Yavo uses its runtime formatter, so their size difference
does not isolate an intrinsic language cost.

### Current Arena/module compiler (2026-11-21)

The final run used main commit **0a69e75aa9e6671858f5d167bffd27de451f6f5c**
after the full Debug/Release and ASan builds finished. Its release compiler SHA-256
is 1688b0bc62552dc3718c55641e0c2e1a687e779e2f137ce7eee3fa32276abc33.
Compiler source inputs and the benchmark harness stayed unchanged during the run.
Both the performance command and the separate cache diagnostic completed with
exit 0 within their 115-second command limits; no timed attempt was discarded.

| Case | Yavo median ms | C median ms | Yavo / C |
| --- | ---: | ---: | ---: |
| integer_recurrence | 70.249 | 74.598 | 0.942 |
| vec_transform | 156.536 | 146.413 | 1.069 |
| mutref_scatter | 101.606 | 132.334 | 0.768 |
| deque_ring | 115.015 | 109.443 | 1.051 |

The geometric mean is **0.949x**; the worst ratio is **1.069x** for vec_transform.
All measured outputs and all small correctness cases matched the independent
reference. The four workloads remain single-file programs: they validate the
current compiler on the original suite, not Arena operation costs or multi-file
project compilation performance. Reports are results/arena-modules.json and
results/arena-modules-cache-path.json; their 439 unique referenced files were
independently rehashed with zero mismatches. The hash, baseline and driver-exit
review is recorded in results/arena-modules-review.json.

| Case | Yavo warm build P50 / P95 ms | C warm build P50 / P95 ms | Stripped Yavo / C bytes |
| --- | ---: | ---: | ---: |
| integer_recurrence | 242.282 / 264.825 | 38.333 / 44.099 | 16896 / 21576 |
| vec_transform | 249.000 / 271.954 | 43.588 / 53.755 | 17936 / 24200 |
| mutref_scatter | 247.232 / 293.445 | 36.085 / 42.183 | 18032 / 24120 |
| deque_ring | 245.484 / 260.387 | 40.241 / 40.749 | 18192 / 24424 |

P95 here is the nearest-rank maximum of five warm-build samples. All four Yavo
warm-build medians remain above 200 ms. The small stripped programs remain below
250 KB; cold/debug compilation and broader program sizes remain unmeasured.

The change from the initial geometric mean of 1.062x to 0.949x is not evidence of
a compiler speedup. All four C executables are byte-identical to the initial
baseline, yet the C scatter median changed from 103.648 to 132.334 ms; its current
samples span 107.048 to 156.288 ms. The shared VM introduces material run-to-run
variation. This four-case pilot establishes reproducible correctness checks and
descriptive timing, not formal near-C acceptance or statistical superiority.

### Source-path cache diagnostic

Run after the main suite, selecting its report:

    perl benchmarks/small-c-v1/cache_probe.pl --baseline-json benchmarks/small-c-v1/results/current.json --report benchmarks/small-c-v1/results/current-cache-path.json

The diagnostic holds generated C bytes, runtime include directory, compiler flags
and output path fixed. A primed stable C path is alternated with five fresh C
paths. Every resulting executable must have identical SHA-256 and exact output.
The initial baseline gave 41.843 ms for a stable input path and 211.961 ms for a
fresh input path (5.066x); see results/f5469dc-cache-path.json. The current compiler
uses a per-process temporary C directory. This contrast identifies a substantial
path-dependent cache cost; it does not measure every source of frontend or build
overhead. No compiler/cache optimization was made for this experiment.

For the final 0a69e75 compiler, the same contrast measured **36.292 ms** with a
stable C path and **225.525 ms** with a fresh path (**6.214x**); all eleven resulting
executables had identical hashes and output. The repeated controlled contrast,
together with 242-249 ms Yavo warm-build medians, supports implementing a stable
content-addressed intermediate/build cache next. A correct cache must include
source/runtime/toolchain/target/flag identities and concurrent-build isolation;
reusing a fixed filename without those keys would not be sufficient.
