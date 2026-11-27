# Yavo — Agent-first systems language

Yavo is a systems-language prototype for coding agents: ownership and deterministic destruction without a tracing GC, a Rust frontend, a C17 backend, and a pinned `zig cc` toolchain. The main P0–P3 language paths are implemented; **full v1 acceptance and lower agent delivery cost than Rust remain unproven**.

**Bootstrap-subset self-hosting is complete (2026-11-23).** The
[Yavo-written compiler](selfhost/README.md) compiles its own complete source
through stage1 → stage2 → stage3, with byte-identical generated C and matching
stage2/stage3 binaries on each native platform. [CI run 45150424099](https://github.com/msly/yavo/actions/runs/45150424099)
passed all six jobs at `4739ffc`, including Linux x86-64, Windows x86-64 and
macOS arm64. The [native confirmation index](compiler/validation/selfhost-fixed-point-native-2026-11-23.json)
preserves those results separately from the original local fixed-point receipt.

**Current goal: daily compiler replacement after practical CLI acceptance.**
Rust stage0 remains the daily compiler and supports substantially more of the
language. The [roadmap](docs/self-hosting.md) separates SH-3 replacement from
SH-4 application acceptance and the completed SH-2 fixed point.
[test-summary](examples/test-summary/README.md), [csv-check](examples/csv-check/README.md)
and [dep-index](examples/dep-index/README.md) cover log statistics, structured
text validation and dependencies across files. **All three formal SH-4 applications are complete within their declared scope.**
Batch 49 adds public system interfaces and a standalone native `build/run` driver
using the locked toolchain. Structured diagnostics and broader language semantics
remain on the SH-3 path; performance and delivery-cost targets retain separate acceptance.

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
| Release suite | **151 passed, 0 failed, 1 ignored** | Batch 49: complete suite rerun after public system/runtime changes; six new integration tests |
| Debug suite | **151 passed, 0 failed, 1 ignored** | Same source, all 20 groups, independent completed local log |
| Frozen specification | **124 pass, 0 wrong, 3 unsupported, 0 not_run, 2 superseded** | 129 frozen cases; a different counting unit from Cargo test functions |
| Cross-compilation | Six target formats/architectures build | Linux (static musl), Windows and macOS on x86-64/aarch64; native CI now covers Linux x86-64 plus the Windows x86-64/macOS arm64 selections below |
| Overall `verify` | **`not_verified`** | Required audit remains unimplemented |

The ignored Cargo test is the old exploratory P0 conformance report; the current specification runner executes separately. T-GR9, T-P3 and T-R2 need an ASan runtime unavailable in the pinned toolchain on the project host. T-V1 and T-GF2 are superseded by the revised two-phase receiver rule, with reasons in `compiler/tests/conformance_superseded.txt`. The runner enforces its passing-case baseline; a green Cargo run must still be read together with the full status report.

Source modules support top-level exports, per-file imports, cyclic imports and original-file diagnostics. Package dependencies and a program lockfile remain open. Multi-file `fix` returns `U0001` without writing; symlink module aliases are unsupported.

### Native selfhost build/run and public system APIs (batch 49, 2026-11-27)

The Yavo-written compiler now directly builds and runs supported native programs
with the installed locked Zig, without shell commands or a Rust frontend fallback.
Its generated runtime embedding removes a source-checkout dependency. Seven new
public functions cover environment/host, unique temporary directories, explicit
cleanup, file identity and synchronous process execution; read-only String slices
support argv and source-path forwarding. Cross-target, cache/PCH, installation and
structured diagnostics remain outside this driver batch.

Final local bootstrap passes **1,063/1,063 cases, 20/20 checks, zero skips**;
all seven compatibility applications pass. The independent ordinary-driver gate
passes **114/114 scenarios and 9/9 checks**. Actual stage2 uses ordinary `build`
to rebuild itself; its 678,304-byte result and emitted C match the bootstrap fixed
point, and it can `run` the independent multi-module fixture. Stage2/stage3 C is
byte-identical at 1,614,200 bytes. The 36 new regressions and one unchanged-source
promotion retain strict subset boundaries. Existing corpus coverage remains
**119/394**, or **56/224** distinct entry-source bytes, with zero regressions,
subset violations or timeouts.

Local acceptance is complete; exact-commit three-platform CI remains pending. Configured jobs are not passing results. See the [validation index](compiler/validation/batch49-build-run-2026-11-27.json) and [driver contract](selfhost/tests/driver49/CONTRACT.md).

### Selfhost owned graph structures and independent dependency index (batch 48, historical)

The five-module, 309-line [dep-index](examples/dep-index/README.md) validates
explicit UTF-8 module declaration files and emits a deterministic dependency-first
TSV index. Duplicate modules, missing references and cycles follow frozen error
precedence; failures leave stdout empty. Its 113 manual cases and independent
oracle were frozen separately from the natural stage0 implementation. The
application's source remains unchanged while selfhost gains `Vec[Vec[int]]`,
flat String tuples and their Optional form, `Heap[(String,int)]` with pure
capture-free comparators, temporary Optional `while let`, read-only owned-record
slice aliases and sink parameters for supported types.

Full local bootstrap passes **1,027/1,027 cases, 19/19 checks, zero skips**,
with 68 new fixtures and two unchanged-source promotions. Stage2/stage3 C
(1,171,654 bytes) and Linux binaries (545,632 bytes) match exactly. The seventh
compatibility gate passes **672/672** dependency-index executions; independent
native delivery passes **336/336**, including deterministic separate Release
builds. Original-corpus coverage rises **112 to 117/392**; including both new
applications gives **119/394**, or **56/224** distinct entry-source bytes, with
zero regressions, subset violations or timeouts. Rust and runtime are unchanged.

Implementation `cc78059` passes **6/6 jobs** in [run 45955439200](https://github.com/msly/yavo/actions/runs/45955439200); all three native gates and Linux independent dual-producer C sanitizers pass. **All three formal SH-4 applications are complete within their declared scope.** General owned Optionals, broader containers/closures,
ordinary selfhost build/run and structured diagnostics remain open. See
[HANDOFF section 28](HANDOFF.md) and the
[validation index](compiler/validation/batch48-dep-index-2026-11-27.json).

### Selfhost String sets and independent CSV validation CLI (batch 47, historical)

Owned `Set[String]` supports empty construction, `add`, `remove`, `has`, `len`
and deep `clone`, including fields, read-only/inout parameters, returns and
Result payloads. A Set remains a distinct language type while reusing the
Map[String,bool] C helpers. Its String keys have one owner; duplicate insertion
drops the incoming key. Set iteration, other key types, direct Vec[Set] and
owned Optional remain explicit subset boundaries.

The second independent SH-4 application is the four-module, 271-line
[csv-check](examples/csv-check/README.md). It validates UTF-8 CSV from stdin or
multiple files, enforces nonempty unique headers and a consistent column count,
and emits an atomic TSV report or a located diagnostic. Its contract and 139
independent fixed cases were frozen separately from the implementation. The
unchanged stage0-accepted application exposed Set[String] as its selfhost
blocker; this batch adds that capability without rewriting the application.

The matrix has **959 cases**, with 34 new fixtures and no promotions. Final
complete bootstrap passes **959/959, 18/18, zero skips**, binding the RSS-aware
runner. Stage2/stage3 C is 1,057,805 bytes and the matching Linux binaries are
490,192 bytes. The sixth application gate executes
138 applicable POSIX scenarios across six compiler/mode/cleanup artifacts
(**828 executions**); Windows has 137 applicable scenarios and 822 executions.
Separate local native acceptance passes **414/414**, with identical C and
30,456-byte binaries from two independent Release builds. All 431 commands
have measured RSS; Debug/Release cold, unchanged and edited builds each retain
one observation with actual miss/hit/miss cache states.

The frozen original corpus remains **112/392 accepted paths**, or **52/222**
distinct entry-source byte sequences. Adding the four new application modules
gives **113/393**, or **53/223**; the original programs have no gains or losses.
The final-stage2 scan has zero subset violations and timeouts. [run 45937402633](https://github.com/msly/yavo/actions/runs/45937402633)
for implementation `0373fb8` passes **6/6 jobs**: all three native bootstrap
and CSV gates, Linux CSV ASan+UBSan on both frontends' C, and the existing
selfhost/independent checks. Full Rust Debug/Release suites each
pass 145/0/1 and frozen conformance 124/3/2; Rust and the runtime are unchanged.
The [validation index](compiler/validation/batch47-csv-set-2026-11-27.json)
binds these local results; see [HANDOFF section 27](HANDOFF.md) for remaining acceptance.

### Selfhost Map iteration, String sorting and original atomic ledger (batch 46, historical)

Read-only two-binding traversal now supports Map[String,int/bool/byte], including
named fields and whole temporary Maps. Keys remain borrowed and immutable;
Copy values are read-only, and the Map's owner is frozen throughout the loop.
Temporary-owner fields are rejected with E0305, matching stage0.
Vec[String].sort() is stable and ascending by UTF-8 bytes; rearranging its
owned descriptors leaves one owner for each string allocation.

The unchanged r5/yavo-b/T01 atomic-transfer ledger is the fifth bootstrap
application gate: 32 scenarios retain all 14 archived cases and add valid
contract boundaries for rollback/commit, consuming failed-batch rows, ordering,
Map growth, 200 commands and balances/totals beyond 32 bits. Independent delta
and clone-ledger models agree. Both compilers in Debug/Release pass all 128
normal-return executions with allocation checking; eight inputs are bound.

The matrix has **925 cases**, with 28 new fixtures and two unchanged-source
promotions. Complete local bootstrap passes **925/925, 17/17, zero skips**;
stage2/stage3 C (1,039,982 bytes) and Linux binaries (484,496 bytes) match.
Final stage2 coverage rises **106 to 112/392**, with zero losses or subset
violations; six gained paths represent two ledger implementations. By entry
source bytes, coverage is **52/222 distinct inputs**, not a project completion
percentage. Rust Debug/Release remains 145/0/1, frozen conformance 124/3/2.
[CI run 45826605112](https://github.com/msly/yavo/actions/runs/45826605112)
for implementation `bc8aefa` passes **6/6 jobs**: native Linux, Windows and
macOS bootstrap/application gates, Linux selfhost ASan/UBSan and the independent
checks. SH-3 is incomplete;
formal SH-4 delivery remains one of three, with a second structured-text
validation project next.

### Selfhost integer Vec sorting and original window-median CLI (batch 45, historical)

`Vec[int].sort()` now performs a stable, in-place ascending sort with no
arguments and a Unit result. Writable locals, inout parameters and nested
struct fields use the existing move/borrow/reservation checks. The shared
runtime comparator uses relational comparisons, including at int extrema.
Byte/text vector sorting, sortBy and owned indexed places remain outside the subset.

The unchanged r5/yavo-b/T11 sliding-window lower-median CLI is the fourth
bootstrap application gate. Its 28 fixed scenarios retain all 14 archived
inputs and add contract-valid bounds: K=25, 200 values, even windows, duplicate
values and sums of ±25,000,000,000. Independent oracles fix exact output for
1,405 input values and 1,112 windows. Both compilers in Debug and Release pass
112 normal-return executions with allocation checking.

The matrix has **897 cases**, including 22 new fixtures and no promotions.
Final stage2 coverage rises **100 to 106 of the same 392 stage0-accepted programs**, with
no lost coverage, zero strict-subset violations and none of 221 rejected
inputs admitted. The six gained paths are two T11 implementations with three
snapshots each. Rust Debug/Release passes 145/0/1 each; no Rust source changed.
Complete local bootstrap passes **897/897 cases, 16/16 checks, zero skips**.
Stage2/stage3 C (1,027,977 bytes) and Linux binaries (479,152 bytes) are
byte-identical; all four application gates and input-identity checks pass.
[CI run 45794313520](https://github.com/msly/yavo/actions/runs/45794313520)
for implementation `67407bb` passes **6/6 jobs**: native Linux, Windows and macOS
bootstrap/application gates, Linux ASan/UBSan and the independent checks.
See [HANDOFF section 25](HANDOFF.md).

### Selfhost for loops and compiler traversal (batch 31, historical)

The bootstrap compiler now accepts int/byte half-open ranges and read-only
Copy-element traversal of stable Vec locals and parameters, including inout
parameters and parenthesized names. Bounds are evaluated once from left to
right, loop bindings are read-only, continue performs the increment, and an
iteration lock protects the container root. Owned elements, mutable iteration,
field/temporary iterables and two-binding collection loops remain outside this subset.

Three lexer stress loops and `parser.store` now use for; their inputs and
assertions are unchanged. Independent comparison exposed a Rust range-bound
bug: an end expression could modify a local/field/byte start before it was
read. A typed snapshot now captures the start before evaluating the end;
the added P0 fixture passes Debug and Release, with the old failure retained.

The matrix is **490 cases and 172 executable inputs**: 51 new inputs plus one
unchanged promotion. All 52 batch cases have independent Rust oracles; the
17 run programs pass 34 Debug/Release executions. The
[local receipt](compiler/validation/selfhost-for-2026-11-24.json) passes
**490/490 cases, 12/12 checks and zero skips**, with equal stage2/stage3 C
(685,990 bytes) and Linux binaries (321,368 bytes), plus the real lexer unit.
Full Rust suites remain 145/0/1 per mode and frozen conformance remains 124/3/2.
See the [oracle index](selfhost/tests/evidence/batch31-for-oracle/oracle-index.json)
for source identities and the reference correction.

Batch 31 [run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591) passed all six jobs at `046ccc6`.
The [native index](compiler/validation/batch31-native-2026-11-24.json) binds
all three platform archives and their actual outputs to this implementation.

### Selfhost Copy Optional and pop (batch 30, historical)

The Yavo compiler now accepts Copy `?T`, `some`/`none`, implicit wrapping,
`isSome`/`isNone`, mutable-place `Vec.pop()` and lazy value-form `??`.
Payloads are int, byte, bool, non-owning structs and payload-free enums.
All **14 internal truncation sites** now pop their suffix in place; the five
copying `truncInts`/`truncBools`/`truncLocals`/`truncTemps`/`truncLoops` helpers
are removed. Cleanup generation and analysis-state removal keep their original
ordering. Owned Optional/pop and nested/container/Result combinations remain
explicit U0001 boundaries.

The matrix has **439 cases and 155 executable inputs**: 67 new inputs plus the
original pop fixture promoted byte for byte. Independent Rust oracles fix the
68 targeted expectations before selfhost comparison; all 15 run programs pass
both Debug and Release. Context typing is separate from implicit wrapping,
including byte literals and empty Vec Result rejection. Full Rust suites retain
**145/0/1 per mode** and frozen conformance **124/3/2**.

The [batch-30 local receipt](compiler/validation/selfhost-optional-pop-2026-11-24.json)
passes **439/439 cases, 12/12 auxiliary checks and zero skips**. Stage2/stage3
C (668,302 bytes) and Linux binaries (308,504 bytes) match exactly. The real
lexer retains both-generation C equality and both-mode allocation checks.
[Oracle evidence](selfhost/tests/evidence/batch30-optional-oracle/oracle-index.json)
retains the original attempts and precise source identities.

### Selfhost Result and error propagation (batch 29)

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

Batch 31 [run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591)
passed **six of six jobs** at `046ccc6`. Linux x86-64, Windows x86-64 and macOS
arm64 each pass **490 selfhost cases, 12 checks and zero skips**, with matching
stage2/stage3 C and binaries within each platform, the real lexer in both modes,
and 213 existing CLI executions. Rust Debug/Release each report 145/0/1 on full
Linux, 81/0/1 on selected Windows and 84/0/1 on selected macOS suites. Linux GCC
ASan/UBSan passes **173 generated programs**, stage2 self-compilation,
**490 checks, 172 emissions and 8 malformed inputs**. The
[batch-31 native index](compiler/validation/batch31-native-2026-11-24.json)
binds all three API-matched archives, 44,165 rehashed files, 25,476 checked
embedded streams and 837 local input identities to the implementation commit.

Batch 30 [run 45345384747](https://github.com/msly/yavo/actions/runs/45345384747)
passed **six of six jobs** at `7297e3b`. Linux x86-64, Windows x86-64 and macOS
arm64 each pass **439 selfhost cases, 12 checks and zero skips**, with matching
stage2/stage3 C and binaries within each platform, the real lexer in both modes,
and 213 existing CLI executions. Rust Debug/Release each report 145/0/1 on full
Linux, 81/0/1 on selected Windows and 84/0/1 on selected macOS suites. Linux GCC
ASan/UBSan passes **156 generated programs**, stage2 self-compilation,
**439 checks, 155 emissions and 8 malformed inputs**. The
[batch-30 native index](compiler/validation/batch30-native-2026-11-24.json)
binds all three API-matched archives, 40,102 rehashed files, 22,892 checked
embedded streams and 783 local input identities to the implementation commit.

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

1. SH-4 has two accepted applications. Deliver the third independent multi-file dependency/index CLI: freeze its format, cross-file relationships and missing/conflicting/cyclic input behavior, implement it with stage0, then address the selfhost blockers without rewriting the application. See [HANDOFF.md](HANDOFF.md).
2. Preserve the completed [SH-2 fixed point](docs/self-hosting.md), ownership and collection behavior, real lexer allocation checks and stage2 ASan/UBSan gate. Retain all six application regression gates, actual resource checks and zero subset violations.
3. Complete SH-3 before making it the default compiler: ordinary build/run with the locked backend, structured source diagnostics, semantic coverage and subsequent cache/fix/verify migration. Keep frozen conformance, resource checks and native platform acceptance as replacement gates.
4. Resume the qualified [per-request executor](tools/agent_eval/README.md), [prepared document experiment](experiments/docs-ab-2026-11-22-r6/README.md), 60-task pilot and formal cost study as later product evidence. Live API qualification is still **blocked**, with **0 round-six participant sessions**; it no longer blocks compiler development.
5. Remaining v1 functionality includes user test blocks and `yavo test`, local/Git dependencies and a lockfile, FFI/unsafe/audit, general closures/higher-order parameters, public movable Join and `spawn inout`. Prioritize these only when required by the declared application scope or a concrete bootstrap blocker.

The [frozen goals](docs/plan.md) require success-rate noninferiority to Rust (paired one-sided 95% lower bound at least −5 percentage points), at least **20% lower cost per successful delivery** with independent replication, and runtime geometric mean at most **1.5× C** on a frozen representative benchmark set. Engineering targets include small-program build P50 **≤200 ms**, stripped Linux binaries **≤250 KB**, native Windows tests and reproducible output hashes. Neither frozen-fixture coverage nor the small C pilot closes full v1 acceptance.

Earlier [collection/reference](compiler/validation/completion-2026-11-21.json), [verification-tool](compiler/validation/verify-2026-11-21.json), [repair-tool](compiler/validation/fix-2026-11-21.json) and [thread/resource/I/O safety](compiler/validation/safety-2026-11-20.json) receipts retain their original scope.
