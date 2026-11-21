# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. The Rust compiler in `compiler/` generates C17 and uses a pinned `zig cc` toolchain. The prototype includes ownership, deterministic destruction, collections (including Heap, Deque and Arena), typed generational IDs, saved owning comparators, mutable-reference returns, interfaces, generics and scoped OS threads. User source modules support top-level `pub` exports, per-file imports, cyclic imports and diagnostics in the original source files. Tools include `yavo fix`, `yavo allocations` and evidence-bound `yavo verify` reports.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- `tools/sanitizers/`: source-bound export and independent ASan/UBSan execution workflow.
- `benchmarks/small-c-v1/`: four-case C comparison, correctness oracle, build measurements and source-path cache probe.
- Git tags `v0.1` through `v0.5` identify revision boundaries.

## Current implementation and validation

The current code snapshot is **`0a69e75`**. Arena IDs retain identity across moves, reject IDs from another Arena, and never revive deleted IDs when slots are reused. IDs support ordering and Map/Set keys; empty Arenas allocate no heap storage. Arena/Box/Guard/thread combinations are covered, including a fix that prevents default read-only Guard parameters from acquiring mutation permission.

Source modules are loaded relative to the entry file's directory. `check`, `build`, `run`, `allocations` and `verify` operate on the reachable source graph; verification retains every source file and a project content hash. Package dependencies and a program lockfile remain open. Multi-file `fix` currently returns `U0001` without writing, and symlink module aliases are unsupported.

| Check | Current result | Scope |
| --- | --- | --- |
| Debug tests | **113 passed, 0 failed, 1 ignored** across 15 groups | The full driver timed out in its final verify group; 98 completed tests plus 15 independently rerun verify tests establish the aggregate. |
| Release tests | **113 passed, 0 failed, 1 ignored** across 15 groups | Four bounded batches completed successfully. |
| Compile/run fixtures | **95** in both C optimization modes | Native output/error checks and runtime heap-block accounting; dedicated Arena and module combinations are included. |
| Frozen specification cases | **126 pass, 0 wrong, 3 unsupported, 0 not run** | P1: 83/0/0/0; P2: 20/0/1/0; P3: 23/0/2/0. |
| Independent ASan+UBSan | **58 application cases passed, 0 failed**, using GCC 14.2 | Current generated C, including Arena and multi-file entries; separate compiler and host from the pinned production toolchain. |
| Multi-file reproducibility | **Passed** for the Arena/module project | Two independent frontends and empty C caches produced identical C and binary hashes; exact output also matched. Overall verification remains `not_verified` because audit is not implemented. |
| Cross-compilation | Six targets build and pass format/architecture checks | Linux (static musl), Windows and macOS on x86-64/aarch64; native execution is validated on Linux x86-64. |
| Four-case C runtime pilot | Yavo/C geometric mean **0.949**, worst **1.069** | Safety checks retained; four manually selected microbenchmarks on one Linux host, outside the formal performance gate. |
| Build/cache measurements | Warm-build P50 **242–249 ms**; stable C path **36.292 ms** versus fresh path **225.525 ms** | Warm builds remain above 200 ms; the path probe isolates a cache cost, not an implemented end-to-end optimization. |

The three frozen cases T-GR9, T-P3 and T-R2 still require ASan, whose runtime cannot link with the pinned toolchain on the project host. Independent ASan validation does not replace that toolchain or convert its unsupported results into passes. Available TSan profiles are checked separately. Windows/macOS and other-architecture execution remain unverified.

Current records: [Arena/module implementation and validation](compiler/validation/arena-modules-2026-11-21.json), [independent current ASan/UBSan](compiler/validation/asan-arena-modules-2026-11-21.json), and [multi-file reproducibility](compiler/validation/arena-modules-repro-2026-11-21.json). The ASan run passed its clean/allocation, deliberate use-after-free and signed-overflow capability checks; 426 referenced artifact hashes were independently rechecked.

The [current four-case performance report](benchmarks/small-c-v1/results/arena-modules.json) and [current cache-path probe](benchmarks/small-c-v1/results/arena-modules-cache-path.json) retain raw samples and input/toolchain/artifact identities; 439 unique artifact references were checked. The [initial baseline](benchmarks/small-c-v1/results/f5469dc-baseline.json) and [initial cache probe](benchmarks/small-c-v1/results/f5469dc-cache-path.json) remain historical measurements. Identical C-control binaries had materially different timings between runs, including random access at 103.648 ms versus 132.334 ms. No compiler performance optimization was made in this batch; the ratio change cannot establish an improvement. The unchanged-output cache-path contrast supports making content-addressed build caching the next implementation priority.

Earlier records retain their original source scope: [collection/reference feature completion](compiler/validation/completion-2026-11-21.json), [allocation and verification tools](compiler/validation/verify-2026-11-21.json), [source repairs](compiler/validation/fix-2026-11-21.json), and [scoped-thread, resource-clone and I/O safety](compiler/validation/safety-2026-11-20.json).

## Remaining work

Fixture coverage is not a percentage of v1 completion. Priorities are content-addressed build caching, local/Git dependency packages and a program lockfile, user test blocks and `yavo test`, FFI/unsafe/audit, target-platform execution, and general closure/environment capabilities. The small C pilot does not establish the formal near-C performance gate. The latest agent experiment is still the earlier 10-task single-file study: Yavo finished 10/10 after 7/10 first compilations, while Rust finished 10/10 after 9/10 first compilations. Arena, modules and the newer tools have not been evaluated for cold-start success or task cost.

[HANDOFF.md](HANDOFF.md) records current progress, conservative borrow-analysis boundaries and the remaining acceptance work.

The Git history is local. Nothing is pushed to a remote repository.
