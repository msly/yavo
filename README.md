# Yavo — Agent-first systems language

Yavo is a systems-language prototype for coding agents: ownership and deterministic destruction without a tracing GC, a Rust frontend, a C17 backend, and a pinned `zig cc` toolchain. The main P0–P3 language paths are implemented; **full v1 acceptance and lower agent delivery cost than Rust remain unproven**.

The specification starts from **v0.5**, with dated revisions in [docs/decisions.md](docs/decisions.md). Historical revisions remain unchanged under `history/v0.x/` and Git tags `v0.1` through `v0.5`.

- `docs/`: current specification and the agent cheatsheet.
- `compiler/`: compiler, runtime, regression tests and [implementation guide](compiler/README.md).
- `yavo-tests/`: the frozen 129-case specification package.
- `tools/sanitizers/`: source-bound export and independent ASan/UBSan execution.
- `benchmarks/small-c-v1/`: four-case C comparison and build/cache measurements.
- `experiments/`: archived agent task sets, solutions, verification and cost records.
- [HANDOFF.md](HANDOFF.md): current status, environment, constraints and next actions.

## Implementation and local validation

The latest language-feature baseline is **`b3c27a6` (2026-11-22)**. It includes ownership, views, deterministic destruction, String/Vec/Map/Set/Heap/Deque/Arena, typed generational IDs, generics, interfaces, source modules and scoped OS threads. Agent tools include `check`, `fix`, `allocations` and evidence-bound `verify`; the backend has a content-addressed build cache.

Recent changes implement index assignment, two-phase variable/field receivers, String/str comparison, constant field defaults and P-STACK. Modifying methods on indexed elements are now supported; their element borrow precedes argument evaluation. Non-Copy element reads remain borrows, with clearer `.clone()` diagnostics; literal `${` still needs escaping. See [decisions 12–19](docs/decisions.md).

| Check | Recorded local result | Scope |
| --- | --- | --- |
| Release suite | **129 passed, 0 failed, 1 ignored** | Complete single-command run for the language-feature baseline |
| Debug suite | **129 passed, 0 failed, 1 ignored** | Complete single-command run for the same baseline |
| Frozen specification | **124 pass, 0 wrong, 3 unsupported, 0 not_run, 2 superseded** | 129 frozen cases; a different counting unit from Cargo test functions |
| Cross-compilation | Six target formats/architectures build | Linux (static musl), Windows and macOS on x86-64/aarch64; recorded native execution is Linux x86-64 |
| Overall `verify` | **`not_verified`** | Required audit remains unimplemented |

The ignored Cargo test is the old exploratory P0 conformance report; the current specification runner executes separately. T-GR9, T-P3 and T-R2 need an ASan runtime unavailable in the pinned toolchain on the project host. T-V1 and T-GF2 are superseded by the revised two-phase receiver rule, with reasons in `compiler/tests/conformance_superseded.txt`. The runner enforces its passing-case baseline; a green Cargo run must still be read together with the full status report.

Source modules support top-level exports, per-file imports, cyclic imports and original-file diagnostics. Package dependencies and a program lockfile remain open. Multi-file `fix` returns `U0001` without writing; symlink module aliases are unsupported.

### Measurements tied to earlier snapshots

| Evidence | Result | Source scope |
| --- | --- | --- |
| Independent ASan+UBSan | **58/58** application cases passed, GCC 14.2 | Arena/module snapshot `0a69e75`, 2026-11-21; separate toolchain/host, not a current pinned-ASan pass |
| Multi-file reproducibility | Identical generated C and binary hashes; exact output matched | Arena/module snapshot `0a69e75`, two independent frontends and empty C caches |
| Four-case C runtime pilot | Yavo/C geometric mean **0.949**, worst **1.069** | Same Arena/module batch, safety checks retained; four microbenchmarks outside the formal performance gate |
| Build cache | Unchanged-source Release **2–3 ms**; edited-source Release **214–247 ms**; frontend check **3 ms** | Cache batch `a9fbefc`, 2026-11-21, seven samples per case on the project host |

The [Arena/module validation](compiler/validation/arena-modules-2026-11-21.json), [ASan/UBSan receipt](compiler/validation/asan-arena-modules-2026-11-21.json) and [reproducibility receipt](compiler/validation/arena-modules-repro-2026-11-21.json) retain their original input hashes. ASan capability checks covered clean/allocation, deliberate use-after-free and signed overflow; 426 referenced artifacts were rehashed in that batch. The [runtime report](benchmarks/small-c-v1/results/arena-modules.json) and [cache-path probe](benchmarks/small-c-v1/results/arena-modules-cache-path.json) had 439 unique artifact references checked. These records are historical evidence, not reruns of `b3c27a6`.

The [implemented-cache measurements](benchmarks/small-c-v1/results/build-cache.json) distinguish unchanged-source hits from edits that change generated C. Edits still exceed the 200 ms Release target; recompiling the runtime header accounts for about 50–60 ms per miss. Earlier [performance](benchmarks/small-c-v1/results/f5469dc-baseline.json) and [path-cache](benchmarks/small-c-v1/results/f5469dc-cache-path.json) baselines remain archived.

## Latest agent experiment

[Round four](experiments/coldstart-2026-11-21-r4/README.md) reused the round-three task packet with compiler `1af1f69`. First compilation plus existing-sample correctness rose from 3/11 to 8/11 for Yavo; Rust remained 11/11. The repair task is excluded from that denominator.

| Metric | Yavo | Rust | Yavo / Rust |
| --- | ---: | ---: | ---: |
| Tokens | 162,464 | 127,801 | 1.27 |
| Tool calls | 38 | 24 | 1.58 |
| Time | 276.8 s | 170 s | 1.63 |

Both languages passed the frozen 18 runtime examples. A later independent T10 boundary check found that the Yavo solution wrongly rejected the valid input `000000001`; Rust accepted it. The archived solution and original statistics are unchanged. Costs were recovered from orchestration records; full interaction transcripts and first-source snapshots were not archived. Same-task improvement does not establish generalization, and the three subsequent fixes in `b3c27a6` have not yet been evaluated on new tasks.

## Next work and acceptance gates

1. Complete native CI bring-up. Existing history was pushed to [msly/yavo](https://github.com/msly/yavo) on 2026-11-22 with user authorization (`origin/main`: `b3c27a6`). The first CI execution is pending; see [CI scope and commands](tools/ci/README.md) and [the workflow](.github/workflows/ci.yml).
2. Run a fifth round with 12 new tasks, independent hidden edge cases and at least two independent runs per language. Save first-source snapshots, commands, outcomes, token/tool counts and monotonic timings directly in the experiment directory; separate reading documentation, writing code and repair costs.
3. Use those results to choose further language/tool changes. Remaining v1 functionality includes user test blocks and `yavo test`, local/Git dependencies and a lockfile, FFI/unsafe/audit, general closures/higher-order parameters, public movable Join and `spawn inout`. Precompiled runtime objects address the observed edit-build latency.

The [frozen goals](docs/plan.md) require success-rate noninferiority to Rust (paired one-sided 95% lower bound at least −5 percentage points), at least **20% lower cost per successful delivery** with independent replication, and runtime geometric mean at most **1.5× C** on a frozen representative benchmark set. Engineering targets include small-program build P50 **≤200 ms**, stripped Linux binaries **≤250 KB**, native Windows tests and reproducible output hashes. Neither frozen-fixture coverage nor the small C pilot closes full v1 acceptance.

Earlier [collection/reference](compiler/validation/completion-2026-11-21.json), [verification-tool](compiler/validation/verify-2026-11-21.json), [repair-tool](compiler/validation/fix-2026-11-21.json) and [thread/resource/I/O safety](compiler/validation/safety-2026-11-20.json) receipts retain their original scope.
