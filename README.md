# Yavo — Agent-first systems language

Yavo is a systems-language prototype for coding agents: ownership and deterministic destruction without a tracing GC, a Rust frontend, a C17 backend, and a pinned `zig cc` toolchain. The main P0–P3 language paths are implemented; **full v1 acceptance and lower agent delivery cost than Rust remain unproven**.

**Bootstrap-subset self-hosting is complete (2026-11-23).** The
[Yavo-written compiler](selfhost/README.md) compiles its own complete source
through stage1 → stage2 → stage3, with byte-identical generated C and matching
stage2/stage3 binaries on each native platform. [CI run 45150424099](https://github.com/msly/yavo/actions/runs/45150424099)
passed all six jobs at `4739ffc`, including Linux x86-64, Windows x86-64 and
macOS arm64. The [native confirmation index](compiler/validation/selfhost-fixed-point-native-2026-11-23.json)
preserves those results separately from the original local fixed-point receipt.

**Current goal: practical CLI delivery and gradual replacement of the Rust
frontend.** Rust stage0 remains the daily compiler and supports substantially
more of the language. The [roadmap](docs/self-hosting.md) keeps SH-3 compiler
replacement and SH-4 non-compiler application acceptance separate from the
completed SH-2 fixed point. The first application,
[yavo-test-summary](examples/test-summary/README.md), a verifier for existing
experiment logs, has passed independent acceptance on all three native platforms.
This completes **one of the three SH-4 applications**; it uses stage0.

The specification starts from **v0.5**, with dated revisions in [docs/decisions.md](docs/decisions.md). Historical revisions remain unchanged under `history/v0.x/` and Git tags `v0.1` through `v0.5`.

- `docs/`: current specification and the agent cheatsheet.
- `compiler/`: compiler, runtime, regression tests and [implementation guide](compiler/README.md).
- `selfhost/`: Yavo-written compiler, independent fixtures and bootstrap validation.
- `yavo-tests/`: the frozen 129-case specification package.
- `tools/sanitizers/`: source-bound export and independent ASan/UBSan execution.
- `benchmarks/small-c-v1/`: four-case C comparison and build/cache measurements.
- `experiments/`: archived agent task sets, solutions, verification and cost records.
- [HANDOFF.md](HANDOFF.md): current status, environment, constraints and next actions.

For an agent's first program, start with the [compact writing guide](docs/agent-start.md)
and [on-demand API index](docs/agent-api-index.md). The complete cheatsheet remains available.

## Implementation and local validation

The broad language-feature baseline **`b3c27a6` (2026-11-22)** includes ownership, views, deterministic destruction, String/Vec/Map/Set/Heap/Deque/Arena, typed generational IDs, generics, interfaces, source modules and scoped OS threads. Agent tools include `check`, `fix`, `allocations` and evidence-bound `verify`; the backend has a content-addressed build cache.

Recent changes implement index assignment, two-phase variable/field receivers, String/str comparison, constant field defaults and P-STACK. Modifying methods on indexed elements are now supported; their element borrow precedes argument evaluation. Non-Copy element reads remain borrows, with clearer `.clone()` diagnostics; literal `${` still needs escaping. See [decisions 12–19](docs/decisions.md).

| Check | Recorded local result | Scope |
| --- | --- | --- |
| Release suite | **145 passed, 0 failed, 1 ignored** | Batch 29: complete suite after the catch-expression break/return-analysis correction |
| Debug suite | **145 passed, 0 failed, 1 ignored** | Same source, all 19 groups, independent completed local log |
| Frozen specification | **124 pass, 0 wrong, 3 unsupported, 0 not_run, 2 superseded** | 129 frozen cases; a different counting unit from Cargo test functions |
| Cross-compilation | Six target formats/architectures build | Linux (static musl), Windows and macOS on x86-64/aarch64; native CI now covers Linux x86-64 plus the Windows x86-64/macOS arm64 selections below |
| Overall `verify` | **`not_verified`** | Required audit remains unimplemented |

The ignored Cargo test is the old exploratory P0 conformance report; the current specification runner executes separately. T-GR9, T-P3 and T-R2 need an ASan runtime unavailable in the pinned toolchain on the project host. T-V1 and T-GF2 are superseded by the revised two-phase receiver rule, with reasons in `compiler/tests/conformance_superseded.txt`. The runner enforces its passing-case baseline; a green Cargo run must still be read together with the full status report.

Source modules support top-level exports, per-file imports, cyclic imports and original-file diagnostics. Package dependencies and a program lockfile remain open. Multi-file `fix` returns `U0001` without writing; symlink module aliases are unsupported.

### Selfhost Result and error propagation (2026-11-24)

The Yavo compiler now accepts user-function `T ! E`, success wrapping, `ok`/`err`,
`try`, and terminating named/underscore `catch` blocks. Its actual lexer returns
`Vec[Token] ! Diagnostic`; `compile` propagates the owned diagnostic and `main`
prints the existing first-error message. Owned String, Vec and struct payloads
are moved and destroyed on the active branch, including failed partial Vec/struct
construction. Transient views of owned Strings in Result remain `U0001`.

The matrix grows from 335 to **372 cases** with **140 executable inputs**.
Independent Rust oracles cover all 37 additions, including 14 normal-return
programs in Debug/Release. The complete Rust suites retain **145/0/1 per mode**
and frozen conformance **124 pass / 3 unsupported / 2 superseded**. A discovered
return-analysis defect in Rust and selfhost is also fixed: a `break` nested in
a catch expression must prevent a loop from concealing a reachable missing
return. [Focused evidence](compiler/validation/catch-break-return-2026-11-24.json)
and [Result oracles](selfhost/tests/evidence/batch29-result-oracle/README.md)
retain the original failures and corrected observations. The
[final local bootstrap receipt](compiler/validation/selfhost-result-2026-11-24.json)
passes **372/372 cases, 12/12 checks and zero skips**, including the real lexer
unit in Debug/Release. Stage2/stage3 C (623,204 bytes) and binaries match exactly.

### GitHub CI

Batch 29 [run 45226925936](https://github.com/msly/yavo/actions/runs/45226925936)
passed **six of six jobs** at `e355cb3`. Linux x86-64, Windows x86-64 and macOS
arm64 each passed **372 selfhost cases, 12 auxiliary checks, zero skips**, the
real lexer unit in Debug/Release, and **213 test-summary executions**. Rust
Debug/Release each report 145/0/1 on full Linux, 81/0/1 on selected Windows and
84/0/1 on selected macOS suites. Linux GCC ASan/UBSan passed **141 generated
programs**, stage2 self-compilation, **372 checks, 140 emissions and 8 malformed
inputs**. All three native archives were downloaded and checked against their
API digests, committed sources and actual command/output files; the
[batch-29 native index](compiler/validation/batch29-native-2026-11-24.json)
preserves the platform counts and binds 685 local test-input paths to this commit.

The preceding batch-28
[CI run 45174913978](https://github.com/msly/yavo/actions/runs/45174913978) passed
**six of six jobs** at `2a64808`. Each native platform passed **335 selfhost cases,
11 auxiliary checks and 213 test-summary executions** (71 scenarios in Debug,
Release and an exit-cleanup helper), with reproducible Release C/binaries.
Linux also passed the complete GCC ASan/UBSan adapter: **126 generated programs,
stage2 self-compilation, 335 compiler checks, 126 emissions and 8 malformed inputs**.
All three native archives were downloaded, matched to GitHub's size/SHA-256 and
inspected against their committed source and execution receipts. See the
[batch-28 native index](compiler/validation/batch28-native-2026-11-23.json).

The earlier byte increment passed **321/321 local selfhost cases and
10/10 auxiliary checks**, with all **119 executable inputs** passing both
frontends in Debug/Release against fixed oracles. Repeated C, normal-exit
allocation checks and all **476 mode-binary identities** were verified.
The [byte receipt](compiler/validation/selfhost-byte-2026-11-23.json) binds
implementation `aecdfe5`, complete local Rust suites, two current original
sources across three prefix workloads, and seven unchanged lexer function
fragments with independent byte/symbol oracles.

[CI run 45126097965](https://github.com/msly/yavo/actions/runs/45126097965) passed **six of six jobs** at
`aecdfe5`. The selfhost gate succeeded on Linux x86-64, Windows x86-64
and macOS arm64. The native Linux job passed **119/119 Yavo-generated C
programs and 3/3 capability probes** with independent GCC ASan/UBSan.
The native Linux archive matched GitHub's size/SHA-256; its actual source,
C, frontend/program binaries and outputs were independently verified,
including all 476 bootstrap mode execution records and the three
original-source workloads. The other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK.

The preceding borrowed-index increment passed **286/286 selfhost cases and
10/10 auxiliary checks**, with all 102 executable inputs passing Debug/Release
fixed oracles, Rust differential checks and repeated-C comparisons.
[CI run 45121072503](https://github.com/msly/yavo/actions/runs/45121072503) passed **six of six jobs** at `b5dfe9f`,
including the selfhost gate on Linux, Windows and macOS. The native Linux gate
passed **102/102 Yavo-generated C programs with GCC 13.3 ASan/UBSan and
3/3 capability probes**.
The [borrowed-index receipt](compiler/validation/selfhost-borrow-index-2026-11-23.json)
binds the source, complete local Rust suites, two unchanged original sources
across three workloads, and CI results. The native Linux archive matched
GitHub's size/SHA-256; actual source, C, frontend/program binaries and outputs
were independently verified. The other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK.

The preceding owned-vector increment passed **263/263 selfhost cases and
10/10 auxiliary checks**, with all 90 executable inputs passing Debug/Release
fixed oracles, Rust differential checks and repeated-C comparisons.
[CI run 45116399319](https://github.com/msly/yavo/actions/runs/45116399319) passed **six of six jobs** at `c7be682`,
including the selfhost gate on Linux, Windows and macOS. The native Linux
gate passed **90/90 Yavo-generated C programs with GCC 13.3 ASan/UBSan and
3/3 capability probes**.
The [owned-vector receipt](compiler/validation/selfhost-owned-vec-2026-11-23.json)
binds the source, complete local Rust suites, both unchanged original
components and CI results. The native Linux archive was downloaded and matched
GitHub's size/SHA-256; actual source, C, frontend/program binaries and outputs
were independently verified. The other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK.

The preceding Copy-vector increment passed **242/242 selfhost cases and
10/10 auxiliary checks**, with all 78 executable inputs passing Debug/Release
fixed oracles, Rust differential checks and repeated-C comparisons.
[CI run 45103511821](https://github.com/msly/yavo/actions/runs/45103511821)
passed **six of six jobs** at `7881db94`, including the selfhost gate on Linux,
Windows and macOS. The native Linux gate passed **78/78 Yavo-generated C
programs with GCC 13.3 ASan/UBSan and 3/3 capability probes**.
The [Vec receipt](compiler/validation/selfhost-vec-2026-11-23.json) binds the
source, final local candidate, complete Rust suites and CI results.
The native Linux archive was downloaded and matched GitHub's size/SHA-256;
actual source, C, frontend/program binaries and output bytes were independently
verified against the bootstrap receipt and fixed oracles. Other five archives
have metadata checks only. LSan is disabled; normal exits use YV_ALLOC_CHECK.

The String ownership increment passes **181/181 local selfhost cases and
10/10 auxiliary checks**. All 55 executable inputs pass Debug/Release fixed
oracles and Rust differential checks with allocation checking; the original
438-byte diagnostic component is preserved and executed.
[CI run 45044246299](https://github.com/msly/yavo/actions/runs/45044246299)
passed **six of six jobs** at `81614a9`, including the selfhost gate on Linux,
Windows and macOS. The new Linux gate passed **55/55 Yavo-generated C programs**
with GCC 13.3 ASan/UBSan, plus all three sanitizer capability probes.
The native Linux archive was downloaded and verified against its GitHub size
and SHA-256; source/C/output bytes were checked against the bootstrap receipt
and fixed program oracles. LSan is disabled; normal exits use YV_ALLOC_CHECK.
The [String receipt](compiler/validation/selfhost-string-2026-11-22.json)
retains the first CI failure, the PCH concurrency fix, complete local Rust
rechecks and final CI evidence. Other artifacts were verified as metadata only.

The Copy aggregate increment passes **138/138 local selfhost cases and 10/10
auxiliary checks**: 39 executable inputs in both modes, 81 rejection cases,
14 stage0-supported features pending in selfhost and 4 shared unsupported
boundaries. The original 72 input sources are preserved. Rust/runtime sources
are unchanged; complete local Rust suites retain the preceding baseline below.
[CI run 45025445954](https://github.com/msly/yavo/actions/runs/45025445954)
passed **six of six jobs** at [`fbb322e`](https://github.com/msly/yavo/commit/fbb322e7f1eb7342f69fb19c6a6f76d01a8ee76f);
the selfhost gate passed on Linux x86-64, Windows x86-64 and macOS arm64.
The [aggregate receipt](compiler/validation/selfhost-aggregates-2026-11-22.json)
binds source identities, successful job/step results and artifact metadata;
archive contents were not independently downloaded or rehashed.

The self-hosting implementation [`353523a`](https://github.com/msly/yavo/commit/353523a62fc57d5249544c9f5df7083051d4cd78) passed [CI run 45019581088](https://github.com/msly/yavo/actions/runs/45019581088): **six of six jobs succeeded**. The local SH-1 gate passed **72/72 cases and 10/10 auxiliary checks**; complete local Debug/Release suites each passed **142/0/1**. [Source-bound local summary](compiler/validation/selfhost-sh1-2026-11-22.json). The selfhost gate passed on **Linux x86-64, Windows x86-64 and macOS arm64**. This batch verifies job/step results and artifact metadata; it does not claim independent archive download/hash verification.

The runtime-PCH implementation was pushed at [`7877015`](https://github.com/msly/yavo/commit/7877015ae5df2dda02ca22d1f2b621f3c3611fe8); its independent [CI run 45015772252](https://github.com/msly/yavo/actions/runs/45015772252) finished **completed/success, six of six jobs** (three native platforms, independent sanitizer, experiment evidence, agent-evaluation preparation). That preceding batch recorded local complete Debug and Release totals of 133/0/1 each. The following table is preserved as the preceding historical CI snapshot, not as the PCH run's counts.

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

[CI scope](tools/ci/README.md) · [archived source-bound receipt](compiler/validation/github-ci-agent-eval-2026-11-22.json) · [round-five CI receipt](compiler/validation/github-ci-round5-2026-11-22.json) · [earlier run history](compiler/validation/github-ci-2026-11-22.json)

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

1. Add Optional/`Vec.pop` to selfhost, then replace the compiler's copying `truncInts`, `truncBools`, `truncLocals`, `truncTemps` and `truncLoops` helpers. Validate empty/nonempty vectors, owned-element moves, discarded values and cleanup before restoring the full fixed point; scope is in [HANDOFF.md](HANDOFF.md).
2. Preserve the completed [SH-2 fixed point](docs/self-hosting.md), real lexer normal-return allocation checks and stage2 ASan/UBSan gate. Continue through `for`, formatting and source modules, using each addition in the compiler itself.
3. Complete SH-3 before making it the default compiler. SH-4 has one accepted application; add independent structured-text validation and multi-file dependency/index projects, retaining frozen conformance, native checks, diagnostics and resource validation.
4. Resume the qualified [per-request executor](tools/agent_eval/README.md), [prepared document experiment](experiments/docs-ab-2026-11-22-r6/README.md), 60-task pilot and formal cost study as later product evidence. Live API qualification is still **blocked**, with **0 round-six participant sessions**; it no longer blocks compiler development.
5. Remaining v1 functionality includes user test blocks and `yavo test`, local/Git dependencies and a lockfile, FFI/unsafe/audit, general closures/higher-order parameters, public movable Join and `spawn inout`. Prioritize these only when required by the declared application scope or a concrete bootstrap blocker.

The [frozen goals](docs/plan.md) require success-rate noninferiority to Rust (paired one-sided 95% lower bound at least −5 percentage points), at least **20% lower cost per successful delivery** with independent replication, and runtime geometric mean at most **1.5× C** on a frozen representative benchmark set. Engineering targets include small-program build P50 **≤200 ms**, stripped Linux binaries **≤250 KB**, native Windows tests and reproducible output hashes. Neither frozen-fixture coverage nor the small C pilot closes full v1 acceptance.

Earlier [collection/reference](compiler/validation/completion-2026-11-21.json), [verification-tool](compiler/validation/verify-2026-11-21.json), [repair-tool](compiler/validation/fix-2026-11-21.json) and [thread/resource/I/O safety](compiler/validation/safety-2026-11-20.json) receipts retain their original scope.
