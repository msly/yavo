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

For an agent's first program, start with the [compact writing guide](docs/agent-start.md)
and [on-demand API index](docs/agent-api-index.md). The complete cheatsheet remains available.

## Implementation and local validation

The latest language-feature baseline is **`b3c27a6` (2026-11-22)**. It includes ownership, views, deterministic destruction, String/Vec/Map/Set/Heap/Deque/Arena, typed generational IDs, generics, interfaces, source modules and scoped OS threads. Agent tools include `check`, `fix`, `allocations` and evidence-bound `verify`; the backend has a content-addressed build cache.

Recent changes implement index assignment, two-phase variable/field receivers, String/str comparison, constant field defaults and P-STACK. Modifying methods on indexed elements are now supported; their element borrow precedes argument evaluation. Non-Copy element reads remain borrows, with clearer `.clone()` diagnostics; literal `${` still needs escaping. See [decisions 12–19](docs/decisions.md).

| Check | Recorded local result | Scope |
| --- | --- | --- |
| Release suite | **133 passed, 0 failed, 1 ignored** | Current PCH implementation, all 17 Cargo test groups; previous language-feature baseline was 129/0/1 |
| Debug suite | **133 passed, 0 failed, 1 ignored** | Current PCH implementation, all 17 groups, independent completed local log |
| Frozen specification | **124 pass, 0 wrong, 3 unsupported, 0 not_run, 2 superseded** | 129 frozen cases; a different counting unit from Cargo test functions |
| Cross-compilation | Six target formats/architectures build | Linux (static musl), Windows and macOS on x86-64/aarch64; native CI now covers Linux x86-64 plus the Windows x86-64/macOS arm64 selections below |
| Overall `verify` | **`not_verified`** | Required audit remains unimplemented |

The ignored Cargo test is the old exploratory P0 conformance report; the current specification runner executes separately. T-GR9, T-P3 and T-R2 need an ASan runtime unavailable in the pinned toolchain on the project host. T-V1 and T-GF2 are superseded by the revised two-phase receiver rule, with reasons in `compiler/tests/conformance_superseded.txt`. The runner enforces its passing-case baseline; a green Cargo run must still be read together with the full status report.

Source modules support top-level exports, per-file imports, cyclic imports and original-file diagnostics. Package dependencies and a program lockfile remain open. Multi-file `fix` returns `U0001` without writing; symlink module aliases are unsupported.

### GitHub CI

The runtime-PCH implementation was pushed at [`7877015`](https://github.com/msly/yavo/commit/7877015ae5df2dda02ca22d1f2b621f3c3611fe8); its independent [CI run 45015772252](https://github.com/msly/yavo/actions/runs/45015772252) finished **completed/success, six of six jobs** (three native platforms, independent sanitizer, experiment evidence, agent-evaluation preparation). The latest local complete Debug and Release totals are 133/0/1 each. The following table is preserved as the preceding historical CI snapshot, not as the PCH run's counts.

**[Run 45002067581](https://github.com/msly/yavo/actions/runs/45002067581) passed all six jobs
at `073003d` on 2026-11-22.** The language-feature baseline remains `b3c27a6`;
the last compiler-tool change, `8a65228`, fixes Windows paths in the verification build driver.

| Job | Result | Scope |
| --- | --- | --- |
| Linux x86-64 | Debug **129/0/1**, Release **129/0/1** | Complete Cargo suites, frozen conformance and six-target builds; 12 separate starter-guide checks |
| Windows x86-64 | Debug **71/0/1**, Release **71/0/1** | Selected native suites and compiler unit tests |
| macOS arm64 | Debug **74/0/1**, Release **74/0/1** | The same selection, including its Unix-only cases |
| Independent Linux ASan+UBSan | **60 passed, 0 failed** | GCC 13.3; all exported P0 run/panic fixtures and clean/UAF/signed-overflow capability probes |
| Round-five experiment evidence | **Passed** | 1,344 judgments, 96 source stages, 16 structural reviews, 43 frozen files and 28 task-packet files |
| Agent evaluation preparation | **Passed** | 33 executor tests, 25 lab/audit/freeze tests, 11 judge/summary scenarios, 94 prepared files and 112 independent oracle checks |

Counts are passed/failed/ignored. Three Unix-only tests are compiled out on
Windows; no selected tests were filtered at runtime. Windows/macOS selection
is narrower than the full Linux suite. Linux conformance remains **124 pass,
0 wrong, 3 unsupported, 0 not_run, 2 superseded** in both modes; independent GCC
success does not change the pinned Zig ASan exceptions.

[CI scope](tools/ci/README.md) · [current source-bound receipt](compiler/validation/github-ci-agent-eval-2026-11-22.json) · [round-five CI receipt](compiler/validation/github-ci-round5-2026-11-22.json) · [earlier run history](compiler/validation/github-ci-2026-11-22.json)

The fifth job validates archived round-five records, source identities and
recomputed summaries, without rerunning participant programs. The sixth validates
the executor and prepared round-six materials; live launch remains **blocked**,
with **0 provider requests, 0 participant sessions and 0 deliveries**. The 12 Linux
documentation checks cover eight successful programs and four expected diagnostics,
separately from Cargo and specification counts. All six artifact downloads match
their GitHub-reported byte counts and SHA-256 digests.

### Measurements tied to earlier snapshots

| Evidence | Result | Source scope |
| --- | --- | --- |
| Independent ASan+UBSan | **58/58** application cases passed, GCC 14.2 | Arena/module snapshot `0a69e75`, 2026-11-21; separate toolchain/host, not a current pinned-ASan pass |
| Multi-file reproducibility | Identical generated C and binary hashes; exact output matched | Arena/module snapshot `0a69e75`, two independent frontends and empty C caches |
| Four-case C runtime pilot | Yavo/C geometric mean **0.949**, worst **1.069** | Same Arena/module batch, safety checks retained; four microbenchmarks outside the formal performance gate |
| Build cache | Unchanged-source Release **2–3 ms**; edited-source Release **214–247 ms**; frontend check **3 ms** | Cache batch `a9fbefc`, 2026-11-21, seven samples per case on the project host |

The [Arena/module validation](compiler/validation/arena-modules-2026-11-21.json), [ASan/UBSan receipt](compiler/validation/asan-arena-modules-2026-11-21.json) and [reproducibility receipt](compiler/validation/arena-modules-repro-2026-11-21.json) retain their original input hashes. ASan capability checks covered clean/allocation, deliberate use-after-free and signed overflow; 426 referenced artifacts were rehashed in that batch. The [runtime report](benchmarks/small-c-v1/results/arena-modules.json) and [cache-path probe](benchmarks/small-c-v1/results/arena-modules-cache-path.json) had 439 unique artifact references checked. These records are historical evidence, not reruns of `b3c27a6`.

The [original binary-cache measurements](benchmarks/small-c-v1/results/build-cache.json) distinguish unchanged-source hits from edits that change generated C. That 2026-11-21 baseline measured 214–247 ms Release edits: the binary cache alone cannot avoid repeatedly parsing the runtime header. Earlier [performance](benchmarks/small-c-v1/results/f5469dc-baseline.json) and [path-cache](benchmarks/small-c-v1/results/f5469dc-cache-path.json) baselines remain archived.

### New runtime PCH optimization (2026-11-22)

Normal pinned-Zig `build`/`run` now cache a **precompiled runtime header** independently of program source. Repeated code edits can share the header's parsed AST; `verify` and uncached builds remain on the existing direct C path. `YAVO_NO_PCH=1` disables the optimization, `YAVO_CACHE_TRACE=1` exposes cache behavior, and entries are SHA-256 verified with a 32-entry LRU. Two four-program, seven-pair edited-source Release pilots showed 1.27–1.45x PCH speedups, with byte-identical binaries in every pair. Three of four PCH P50s were below 200 ms in each pilot; different cases missed the limit. **This is build-time evidence, not the formal runtime/C or Agent-cost gate.** [First-run raw timings](benchmarks/small-c-v1/results/runtime-pch-2026-11-22-initial.json) · [final-compiler raw timings](benchmarks/small-c-v1/results/runtime-pch-2026-11-22.json) · [reproducer](benchmarks/small-c-v1/pch_probe.py).

## Latest agent experiment

[Round five](experiments/coldstart-2026-11-22-r5/README.md) completed four independent
empty-context sessions on 12 new task specifications, two sessions per language.
**Yavo and Rust each achieved 24/24 successful first compiler requests, public and
hidden correctness, and required module/thread structure.** All 48 deliveries
retained the first source unchanged; neither language needed a repair.

| Session | First-build complete task success | Controlled lab operations | Elapsed time |
| --- | ---: | ---: | ---: |
| Yavo A | 12/12 | 69 | 337.8823 s |
| Yavo B | 12/12 | 69 | 333.4053 s |
| Rust A | 12/12 | 66 | 282.2978 s |
| Rust B | 12/12 | 66 | 274.7395 s |

Yavo used **138 versus 132 controlled operations (1.0455×)** and
**671.2876 versus 557.0373 seconds (1.2051×)**. The six extra operations are exactly
three reads of the 32,889-byte Yavo cheatsheet in each session; every other
operation category has equal counts. Compiler processes consumed 5.3451 versus
3.8910 seconds, accounting for only 1.4541 seconds of the 114.2503-second total
gap. The timing also includes model processing and waiting; this packet does
not identify which of those caused the remaining gap.

The frozen packet contains 24 public and 144 hidden cases. Replaying first and
final snapshots produced **1,344/1,344 passes** (192 public, 1,152 hidden), with
Yavo heap-block checking enabled. This is 96 snapshot evaluations of 48
deliveries, not additional independent problem samples. The packet was frozen
at `8c200aa`; the language/compiler baseline is `5cd916b` with last compiler
code change `8a65228`. No compiler, runtime or specification changes were made.

Tokens, exact serving-model ID and sampling parameters remain **`null`**: the
collaboration interface exposes no verifiable usage, and the read-only CLI
initialization probe failed. The formal delivery-cost gate remains blocked.
Three tasks reuse earlier task families with new rules; nine have other new
specifications, so this is not a strict task-family holdout. The sessions also
differ from the historical Sonnet setup, preventing causal attribution of
cross-round success changes. [Round four](experiments/coldstart-2026-11-21-r4/README.md)
retains its original same-packet results and leading-zero failure.

Evidence: [results](experiments/coldstart-2026-11-22-r5/results.json),
[operation/time analysis](experiments/coldstart-2026-11-22-r5/evidence/cost-analysis.json),
[behavior replay audit](experiments/coldstart-2026-11-22-r5/evidence/behavior-review.json)
and [module/thread audit](experiments/coldstart-2026-11-22-r5/evidence/manual-review.json).

## Next work and acceptance gates

1. Qualify the implemented [per-request executor](tools/agent_eval/README.md) on an authorized API environment. It archives raw requests, response IDs, actual usage/model/settings and controlled tool results. The current CLI state-path failure and absent API credentials leave live qualification **blocked**; offline tests do not authorize a trial.
2. Run the prepared [eight-task full-vs-compact document experiment](experiments/docs-ab-2026-11-22-r6/README.md) after qualification. The 134-line starter plus API index is 12,543 bytes versus the old 32,889-byte full guide; 12 program/diagnostic checks pass. Both arms will receive the same API index/topics. **Participant sessions: 0; no round-six correctness or cost result yet.**
3. Run the planned 60-task pilot to estimate variance and task-family coverage before the formal study, whose initial design has at least 400 tasks.
4. Remaining v1 functionality includes user test blocks and `yavo test`, local/Git dependencies and a lockfile, FFI/unsafe/audit, general closures/higher-order parameters, public movable Join and `spawn inout`. The runtime PCH pilot improves edited-source builds; the representative build P50 target is not yet met by every case, and PCH is not a separate precompiled runtime object.

The [frozen goals](docs/plan.md) require success-rate noninferiority to Rust (paired one-sided 95% lower bound at least −5 percentage points), at least **20% lower cost per successful delivery** with independent replication, and runtime geometric mean at most **1.5× C** on a frozen representative benchmark set. Engineering targets include small-program build P50 **≤200 ms**, stripped Linux binaries **≤250 KB**, native Windows tests and reproducible output hashes. Neither frozen-fixture coverage nor the small C pilot closes full v1 acceptance.

Earlier [collection/reference](compiler/validation/completion-2026-11-21.json), [verification-tool](compiler/validation/verify-2026-11-21.json), [repair-tool](compiler/validation/fix-2026-11-21.json) and [thread/resource/I/O safety](compiler/validation/safety-2026-11-20.json) receipts retain their original scope.
