# Small Yavo / C performance suite v1

This is a four-case implementation pilot, not the frozen formal v1 performance gate.
The manifest, algorithms, input sizes, two warmups and seven measured samples are
fixed before timing. The Yavo compiler's safety checks remain enabled.

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
