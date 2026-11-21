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
