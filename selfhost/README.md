# Yavo-written compiler: self-hosted bootstrap subset

This directory implements the [self-hosting roadmap](../docs/self-hosting.md).
It contains a compiler written in Yavo: source is tokenized, parsed into an
AST, checked, and lowered to C17. It compiles its own source: stage0 (the Rust
compiler) builds stage1; stage1 compiles the same files to stage2 C; stage2
compiles them again to stage3 C. Stage2/stage3 C and binaries are byte-identical.
Rust remains a separate behavioral reference; no stage delegates an input back
to Rust. Current validation is recorded below.

**Batch 52:** integer radix prefixes and separators are handled in the lexer;
the original UTF-8 hexadecimal fixture is restored. bootstrap.py now defaults
to --tier core (fixed point plus stable and changed cases); --tier full
retains every semantic/application gate. Full CI runs every fifth batch and on
milestones/releases. See [validation tiers](../docs/validation-tiers.md).
subset-features/subset_sweep.py inspect complete module ASTs and typed IR,
retain unresolved restrictions, and rank feature-set candidates without claiming
future acceptance. [SH-3 exit checklist](../docs/sh3-roadmap.md).

**Status: SH-2 fixed point reached for the bootstrap subset (2026-11-23).**
The compiler sources were rewritten into a *bootstrap dialect*: a strict
subset of the stage0 language that the compiler itself implements (see
[decision 20–24](../docs/decisions.md)). It is not yet the default compiler:
stage0 accepts far more of the language (general owned Optionals,
mutable/reference iteration, float formatting, methods, generics, broader
Map/Set APIs, general closures and threads).
Batch 49 adds a native ordinary `build`/`run` entrypoint in Yavo, using the
already-installed locked backend and an embedded runtime header. Seven public
system interfaces cover environment values, host identity, temporary directories,
file removal/identity and synchronous argv-based processes. Read-only
`[]String` parameters carry process arguments and the complete source-path
inventory. The driver has no Yavo build cache and does not yet migrate cross
targets or toolchain installation. Batch 50 adds independent first-error JSON
checking with source snapshots, exact byte/scalar ranges and a pure Yavo SHA256
module. Richer diagnostics and JSON build/run remain separate work. This closes one
SH-3 entrypoint gap; it does not make selfhost the daily default.
Batch 51 adds four nominal standard error enums and prelude Utf8Error, full
Results from existing fallible calls, terminating named catches, error display
and standard-error-only payload matching. These values remain Copy while owned
success payloads follow the existing movement and cleanup rules.
Batch 48 is driven by the independently authored five-module
[yavo-dep-index](../examples/dep-index/SPEC.md): `main`, `model`, `parser`,
`graph` and `report` were implemented with stage0 and frozen before extending
selfhost. Its unchanged natural data structures require `Vec[Vec[int]]`,
flat String tuples and their Optional form, `Heap[(String, int)]` with a pure
capture-free expression comparator, temporary Optional `while let`, and
read-only access to owned structs through slice parameters. The implementation
also supports sink transfer of existing supported types and read-only alias
chains from those slice parameters. `dep_application.py` adds the application
compatibility gate; `tools/ci/dep_check.py` checks independent native delivery.
Batch 47 adds owned `Set[String]` values, driven by the independently authored
four-module [csv-check](../examples/csv-check/README.md) application. Its
source passes stage0 before selfhost support is added. `csv_application.py`
adds a sixth application compatibility gate; `tools/ci/csv_check.py` separately
validates the project's native delivery requirements.
Batch 46 adds read-only `Map[String, int/bool/byte]` traversal and stable
`Vec[String].sort()`. The unchanged r5/yavo-b/T01 atomic-transfer ledger is
checked by `ledger_application.py` as a fifth bootstrap application gate.
Batch 45 adds stable, in-place `Vec[int].sort()` on writable local, field and
inout receivers. The unchanged r5/yavo-b/T11 window-median CLI is checked by
`median_application.py` as a fourth bootstrap application gate.
Batch 44 adds int/byte `min` and `max`, preserving bidirectional literal inference
and left-to-right value snapshots. The unchanged r4 T11 diff CLI is checked by
`diff_application.py` as a third bootstrap application gate.
Batch 43 adds flat Copy tuples, numeric projection, Optional tuples and integer
`abs`. The unchanged two-module r5/yavo-b/T04 unit-conversion CLI is checked by
`conversion_application.py` as a second bootstrap application gate.
Batch 42 adds `for index, item in values`, Copy Optional `if let`, and static
`os.args()` iteration. The compiler itself uses both new binding forms;
a named match of an existing Optional keeps it unchanged for the then scope.
Batch 41 adds `std/io` (`io.readAll() catch _`), `trim`/`lines`/`fields`,
bindings of views into owned locals (the owner stays unchanged while a view is
in scope) and iteration over temporary vectors.
Batch 40 adds `Map[String, V]` for scalar values; the selfhost now compiles the
unmodified examples/test-summary, checked by `application.py` as a bootstrap
check. `subset_sweep.py` checks the strict-subset rule on every repository
program.
Batch 39 adds read-only iteration over owned `String` and struct elements; the
loop variable is a view of the element and the vector stays locked.
Batch 38 adds the str queries `startsWith`, `endsWith`, `contains`, `find`,
`split` and `parseInt`; the receiver is locked while the argument is evaluated.
Batch 37 adds read-only `[]T` parameters; a `Vec[T]` argument is passed as a
view of its storage.
Batch 36 adds jumping `??` fallbacks (`?? return`, `?? break`, `?? continue`)
as a whole let initializer or assigned value, as in stage0.
Batch 35 adds `Vec[str]`: a vector's sources join its elements' (push and
element stores join, assignment replaces), and element reads keep them.
Batch 34 adds borrowed returns: `-> str of a`, `of (a, b)` and `of static`; each str
local records which parameters it may point into, joined along control flow
like stage0's loans, and `inout str` parameters accept only static text.
Batch 33 adds multi-file source modules: `use a`, `use a/b` and `use a/{b, c}`
load files relative to the entry directory (cycles allowed), `pub` items are
exported, functions are called as `module.f(...)`, public types may be named
short or as `module.T`, and diagnostics and run-time panics name the original
file. Batch 32 adds `${expr}` interpolation in direct print/println/eprintln/format
arguments (int, byte, bool, str, String, payload-free enum) and an owned
`format` result; the compiler's diagnostic line and C name/position helpers now
use it. Batch 31's int/byte range `for` and read-only Copy Vec traversal,
batch 30's Copy Optional, `Vec.pop` and value-form `??`, and batch 29's
user-function results remain part of the subset. SH-3 is still incomplete.

## Build and validate

From the repository root, with Rust and the pinned Zig toolchain installed:

```sh
cargo build --manifest-path compiler/Cargo.toml --locked --release
python3 selfhost/embed_runtime.py --check
python3 selfhost/bootstrap.py
python3 tools/ci/selfhost_driver.py --bootstrap-output compiler/target/ci/selfhost
python3 tools/ci/selfhost_digest.py --bootstrap-output compiler/target/ci/selfhost
python3 tools/ci/selfhost_diagnostics.py --bootstrap-output compiler/target/ci/selfhost
python3 tools/ci/selfhost_errors.py --bootstrap-output compiler/target/ci/selfhost
```

The standard-error gate runs six products from actual stage0/stage1/stage2.
Linux CI additionally passes `--sanitizers required`; other native platforms
retain `--sanitizers off`. Candidate/reference-only runs are preliminary.

The compiler has ten Yavo modules: entry `main.yv`, plus `digest.yv`, `support.yv`,
`lexer.yv`, `model.yv`, `parser.yv`, `types.yv`, `codegen.yv`, `driver.yv` and
`runtime_source.yv`. It loads these through `use` and calls across files as
`module.f(...)`. `bootstrap.py` copies the complete `COMPILER_PARTS` unchanged
into the run directory and gives each compiler only the entry file; concatenating
the files is not the module build procedure. The separate lexer unit entry is
`lexer_test.yv`, which loads `support`, `lexer` and their `digest` dependency.
To build the compiler by hand:
`yavo build selfhost/main.yv --release -o yavo-selfhost`.

`runtime_source.yv` is generated from the exact bytes of
`compiler/runtime/yavo_rt.h` and the version in `compiler/toolchain.lock`.
After changing either input, run `python3 selfhost/embed_runtime.py` and commit
the regenerated module with the change. `--check` only compares the expected
bytes; it never repairs a stale embedding. Bootstrap checks freshness before
building stage1 and includes the generated module in the source identity checks.
The resulting executable can build from any working directory without locating
a runtime header in a repository or installation directory.

`bootstrap.py` accepts `--stage0`, `--zig` and `--output`. Defaults use the
Release compiler, the toolchain installation described by `compiler/toolchain.lock`,
and `compiler/target/ci/selfhost/`. Windows executable suffixes and the configured
`YAVO_HOME` are handled automatically. It uses Python's standard library.

Each run creates a fresh directory and:

1. verifies the runtime/version embedding, assembles the compiler and builds
   stage1 with stage0;
2. has stage1 check and emit its own source as `stage2.c`, builds stage2 with
   the locked Zig (`-O2`, `YV_ALLOC_CHECK`), has stage2 emit `stage3.c`, and
   requires byte-identical C and identical stage2/stage3 binaries;
3. runs every fixture: stage1 and stage2 must report identical exit codes,
   stdout and stderr, and emit byte-identical C; successful and panic
   programs are compiled through stage0 and stage1 in Debug and Release and
   must match fixed expected output, exit status or panic category;
   invalid inputs must fail before creating or changing output, and
   constructs outside the subset must report `U0001` (`shared_unsupported`
   inputs must report only `U0001` in both compilers);
4. checks the output guards and that no input changed during the run.

The runner clears `PATH` when executing Yavo-built compilers, retains complete
commands/stdout/stderr, records source/bundle/runtime/compiler/toolchain hashes,
and writes `receipt.json` with `achieved: "self-hosted-fixed-point"` and
`self_hosted: true` only when everything passes. Native CI runs this gate on
Linux, Windows and macOS; a configured job is not itself a passing result.

The separate [ordinary-driver gate](tests/driver49/README.md) runs its frozen
contract against the Rust-built stage1 and the actual final stage2 from a
successful complete bootstrap. It distinguishes real locked-Zig builds from
controlled native probes of backend arguments, streams, errors and cleanup.
It also requires final stage2's ordinary `build` of the unchanged compiler
modules to produce a compiler that emits the fixed-point C and executes the
independent multi-module fixture with ordinary `run`. This gate does not replace
the broader bootstrap, application or specification checks.

## Current validation: batch 51 (2026-11-27)

Four standard error enums keep distinct nominal identities and real integer
payloads; `Utf8Error` is a Copy struct with `offset`. The ten existing fallible
calls now produce complete typed Results. Named catch, display, propagation and
standard-error-only match use the same ownership and cleanup machinery as
supported user Results. Rust, the production runtime and the toolchain lock are
unchanged.

| Formal local gate | Result |
| --- | --- |
| Complete bootstrap | **1,136/1,136**, **20/20**, zero skips; all seven compatibility applications |
| Error I/O products | **288/288**: 48 cases through actual stage0/stage1/stage2, Debug and Release |
| Error controls | Six oracle controls, four clean/leak exit controls, three original nondecimal-source boundary checks; nine harness checks |
| Retained diagnostics | **196 JSON/CLI + 160 text + 16 supplemental = 372** commands |
| SHA256 / ordinary driver | **276** vector executions; **114/114** driver scenarios and nine checks |
| Fixed point | C **1,774,302 bytes**; Linux binaries **729,968 bytes**; stage2/stage3 byte-identical |
| Original corpus | **125/394**, **62/224** distinct entry-source bytes; six additional accepted original entries |
| Original-program runtime checks | Six unchanged source programs, 12 scenarios, **48/48** stage0/final-stage2 Debug/Release executions |

The manifest adds 73 probes and promotes ten original fixtures without editing
their source bytes: **319 run, 34 panic, 600 reject, 141 unsupported,
42 shared-unsupported**. One of the 73 newly authored semantic fixtures originally used `['a', 0xE2, 0x82]`, outside the existing nondecimal lexer subset. A single recorded amendment adds the equivalent decimal source `['a', 226, 130]` and changes only that manifest entry's file. The original source, freeze, manifest snapshot and first candidate U0001 failure remain preserved; old stage0 executes both versions in Debug/Release with the unchanged `1\n` output. Formal boundary checks retain stage0 acceptance and stage1/stage2 U0001 for the original source.

The error gate checks exact stdin/stdout/stderr and exit status, before/after
filesystem bytes, runtime-error payloads and preserved input identities. It
materializes fresh inputs for every product and case. Linux/macOS apply all 48
cases; Windows excludes directory stdin and invalid native UTF-8 environment
bytes, giving 46 cases / 276 executions. The local error sanitizer mode is off.
Implementation `76915bc` passes **6/6 jobs** in [run 46028799189](https://github.com/msly/yavo/actions/runs/46028799189). The three native error gates pass 288 executions on Linux/macOS and 276 on Windows; Linux independently instruments the original stage0 and actual final-stage2 C with GCC ASan+UBSan and passes 96 further executions. The three original native ZIP archives, GitHub sizes/hashes, raw receipts, inputs and product identities are verified. The first [run 46018585973](https://github.com/msly/yavo/actions/runs/46018585973) passed 5/6 jobs: the Windows error gate stopped before emitting C or executing scenarios because a native backslash path was compared with the frozen POSIX path. That comparison now uses `.as_posix()`; eight path controls and the 288-execution local error gate pass. The original failure and archives remain preserved. No new local full Rust suite was run for batch51. Current CI logs record linux: full suites, Debug/Release each 151 passed / 0 failed / 1 ignored; macos: selected native suites, Debug/Release each 90 passed / 0 failed / 1 ignored; windows: selected native suites, Debug/Release each 87 passed / 0 failed / 1 ignored.

Original 622 corpus bytes and stage0 classifications remain unchanged; acceptance
increases from 119/394 to 125/394, or 56/224 to 62/224 distinct entry bytes, with
zero regressions, subset violations or timeouts. The Rust local full-suite record
is inherited source-identical batch49 evidence, not a new batch51 execution.
See the [validation index](../compiler/validation/batch51-standard-errors-2026-11-27.json), [error contract](tests/errors51/CONTRACT.md)
and [fixture amendment](tests/errors51/semantic-amendment.json).

## Historical validation: batch 50 (2026-11-27)

The [independent diagnostic contract](tests/diagnostics50/CONTRACT.md), 98 cases
and strict Python oracle were frozen before the new JSON frontend was executed.
The original text frontend supplies only the recorded code/message/start;
byte spans, scalar coordinates and hashlib identities are independent expectations.
Eight additional legacy CLI cases were separately frozen before correcting
the candidate's leading-dash input/literal output-name regression. The first
bootstrap attempt stopped after 294 cases and is excluded from formal acceptance.

| Formal local gate | Passed |
| --- | --- |
| Complete bootstrap | 1,063/1,063 cases, 20/20 checks, zero skips |
| Actual stage1/stage2 JSON / text / supplemental CLI | 196 / 160 / 16; 372 commands |
| Independent oracle controls | 10 deliberately invalid documents rejected |
| Source-mutation rendezvous | Root and loaded-module FIFO snapshots; JSON and text for both producers |
| SHA256 | 46 frozen vectors × stage0/stage1/stage2 × Debug/Release = 276 |
| Ordinary native driver | 114/114 cases, 9/9 checks; self-rebuild and independent multi-module run |
| Existing source corpus | 119/394, 56/224 unique entry bytes; zero regressions/violations/timeouts |

Stage2/stage3 C is byte-identical at **1,684,975 bytes** and their Linux binaries
at **682,864 bytes**. The local ordinary rebuild also matches those bytes.
The existing 1,063-case manifest, 622 corpus sources and Rust/runtime/lock sources
are unchanged. No entry in the existing subset manifest was added or promoted; 31 new lexer range
unit scenarios, SHA256 vectors and diagnostic contract scenarios are separate units.
Local complete Rust results remain the source-identical batch49 runs; CI results
are recorded against this batch's implementation commit. Compile P50 and token/currency costs remain unmeasured.

Implementation `6e1d22f` passes **6/6 jobs** in [run 45991816609](https://github.com/msly/yavo/actions/runs/45991816609). All three native SHA256, JSON-diagnostic and ordinary-driver gates pass; Linux passes the JSON gate using the actual stage2 C compiled with ASan+UBSan. All three native ZIP archives were downloaded, matched to GitHub hashes, and their source/receipt/artifact identities independently checked. Windows excludes two POSIX-only path cases and two FIFO cases: its
dual-producer JSON/text/supplemental counts are 188/152/16 (356 commands).
Linux/macOS use 196/160/16 (372 commands). The Linux instrumented compiler gate
requires 98/80/8, 10 oracle controls and all four FIFO rendezvous, in addition
to the existing compiler/program/self-compile/stress gates.
See the [validation index](../compiler/validation/batch50-json-diagnostics-2026-11-27.json) for source and artifact identities.

## Historical validation: batch 49 (2026-11-27)

The implementation adds the native driver, public system calls and read-only
String slices. The primary driver contract and independent expectations were
frozen before executing the implementation; two later scenarios separately
freeze the review-discovered leading `-`/`@` temporary-root boundary. The gate
checks source aliases, literal argv, inherited streams, child exit/signal
statuses, backend diagnostics, failure precedence and temporary-file inventory.

The language regressions cover API arity/types, argument borrowing and
evaluation order, temporary-owner cleanup, String-slice ownership and explicit
unsupported standard-error types. Namespace regressions reject a standard and
user module with the same alias, in either import order and even when unused;
local variable shadowing and an ordinary same-named function remain distinct
from that import collision.

The final local bootstrap passes **1,063/1,063 cases, 20/20 checks, zero skips**,
including all seven existing compatibility applications. The matrix adds 36
fixtures and one unchanged-source promotion: 279 run, 34 panic, 571 reject,
141 unsupported and 38 shared-unsupported cases. Stage2/stage3 C is identical at
**1,616,267 bytes**; their Linux binaries are identical at **680,160 bytes**.

The independent ordinary-driver gate passes **114/114 and 9/9 checks** locally.
On the local Linux run, actual final stage2 rebuilds the compiler with ordinary build; the resulting
binary and emitted C match the bootstrap fixed point, and the result runs an
independent multi-module program through ordinary run. All 133 commands have
wait4 RSS observations; the maximum is 373,264,384 bytes for a waited child,
not simultaneous process-tree memory. Compile P50 and token/currency cost are unmeasured.

Rust Debug/Release each pass **151/0/1**, with frozen conformance **124/3/2**.
The production runtime changes in this batch; six new native system integration
tests, six target object builds and an independent GCC 14.2 ASan+UBSan probe pass.
Existing corpus coverage remains **119/394**, or **56/224** distinct entry bytes,
with unchanged sources and zero regressions, subset violations or timeouts.

Corrected head `e03cfc0` passes **6/6 jobs** in [run 45975880319](https://github.com/msly/yavo/actions/runs/45975880319); all three native ordinary-driver gates and Linux system ASan+UBSan pass. The [validation index](../compiler/validation/batch49-build-run-2026-11-27.json) separates final acceptance from the stopped preliminary namespace-audit run.
SH-3 default replacement, formal performance and relative delivery-cost acceptance remain open.

The macOS ordinary rebuilt executable and bootstrap stage2 have different raw
file hashes. All 1,262 differing bytes are in the code-signature region; the
preceding 632,752 bytes, code, data, load commands and UUID are identical.
The output names `yavo-selfhost` and `rebuilt-selfhost` change the CodeDirectory
identifier and its dependent lengths and offsets; all 39 code-page hashes were
independently recomputed and match. The generated C fixed point and execution
checks pass. The [byte comparison record](../compiler/validation/batch49-macos-binary-comparison-2026-11-27.json)
preserves both original file hashes and the complete comparison.

## Historical validation: batch 48 (2026-11-27)

The unchanged five-module dependency index passes the final selfhost gate:
**672/672 executions** across stage0/final-stage2 Debug, Release and exit-cleanup
artifacts. Independent local native delivery passes **336/336** and produces
byte-identical C and 39,328-byte executables in two separate Release builds.

The matrix has **1,027 cases**: 271 run, 34 panic, 549 reject, 135 unsupported
and 38 shared_unsupported. Batch 48 adds 68 fixtures and promotes two existing
sources without changing their bytes. Full bootstrap passes **1,027/1,027,
19/19 checks, zero skips**, including seven compatibility application gates.
Stage2/stage3 C is byte-identical at **1,171,654 bytes**; their Linux binaries
are identical at **545,632 bytes**. Rust Debug/Release each retain 145/0/1,
with frozen conformance 124/3/2 and no Rust or production-runtime changes.

The original 613 source files remain unchanged: coverage rises 112 to 117/392,
or 52 to 54/222 by entry-source bytes. Including CSV and dep-index gives
**119/394 accepted paths**, or **56/224 distinct entry sources**, with zero
regressions, strict-subset violations or timeouts. The five historical gains
are two sink programs with archived copies; the dependency index adds one entry.

Implementation `cc78059` passes **6/6 jobs** in [run 45955439200](https://github.com/msly/yavo/actions/runs/45955439200); all three native gates and Linux independent dual-producer C sanitizers pass. **All three formal SH-4 applications are complete within their declared scope.** Seven compatibility gates use a separate counting scope.
SH-3, formal performance thresholds and relative delivery-cost acceptance remain
open. See the [validation index](../compiler/validation/batch48-dep-index-2026-11-27.json).

## Historical validation: batch 47 (2026-11-27)

Owned String sets compile the independently authored four-module CSV checker
without changing its stage0-accepted source. The matrix has **959 cases and
274 executable inputs**: 242 run, 32 panic, 524 reject, 123 unsupported and
38 shared_unsupported. The 34 additions are four run, 21 reject, six unsupported
and three shared_unsupported fixtures; no older source was promoted. Both
compilers agree on the targeted classifications, and all four run programs
pass Debug and Release with allocation checks (16 executions).

The Set fixtures cover ownership transfer, duplicate/empty/UTF-8/NUL keys,
growth/removal/reinsertion, deep clone, aggregate fields, both Result payloads,
early exits and receiver locks. Independent checking fixed Set iteration to
reject with U0001 and Set field default `[:]` to reject with E0001, matching
the stage0 boundary. Local Set initialization with `[:]` remains accepted.

The sixth compatibility gate, `csv_check_application`, binds the SPEC, four
modules, independent oracle, fixed cases, runtime, toolchain and gate scripts.
Its 139 frozen cases include 138 POSIX / 137 Windows cases. Both frontends
produce Debug, Release and an exit-cleanup Release artifact, giving **828
POSIX / 822 Windows executions**. Clean/leak controls verify the cleanup
checker; exact streams, diagnostics and status are compared for every case.
The native delivery gate separately checks three artifacts, reproducibility,
cache conditions and Linux required ASan+UBSan on C from each frontend.

Final full local bootstrap passes **959/959, 18/18, zero skips**, with the
RSS-aware scripts bound. Stage2 and stage3 C are byte-identical at **1,057,805
bytes**, and their Linux binaries match at **490,192 bytes**. Separate native
acceptance with those scripts passes **414/414**;
two independent Release builds produce identical C and 30,456-byte binaries.
Linux/macOS runners record per-command wait4 RSS, with platform and scope
stated explicitly; Windows records unavailable rather than an invented value.
Cold, unchanged and edited builds are single observations, not P50/P95.

The 613 original sources and 392 stage0-accepted members remain byte-identical:
**112/392** accepted, **52/222** by distinct entry-source bytes, with no gains
or losses. Adding the four CSV modules gives 617 files, **113/393** accepted
and **53/223** distinct entries. The final-stage2 scan reports no subset violations
or timeouts. Complete Rust suites each
retain 145/0/1 and frozen conformance 124/3/2; no Rust/runtime files changed.
Implementation `0373fb8` passes **6/6 jobs** in [run 45937402633](https://github.com/msly/yavo/actions/runs/45937402633).
Native Linux, Windows and macOS pass bootstrap and both formal application
gates; Linux additionally passes CSV ASan+UBSan on stage0 and final-stage2 C.
Formal SH-4 delivery is **two of three applications**. Six compatibility gates
have a different counting scope.
The [validation index](../compiler/validation/batch47-csv-set-2026-11-27.json)
retains local receipt identities, compiler hashes and the exact counting scopes.

## Historical validation: batch 46 (2026-11-26)

Read-only Map[String,int/bool/byte] iteration and owned String vector sorting
compile the original r5/yavo-b/T01 atomic-transfer ledger without source edits.
The matrix has **925 cases and 270 executable inputs**: 238 run, 32 panic,
503 reject, 117 unsupported and 35 shared_unsupported. There are 28 new
fixtures (five run, 18 reject, five unsupported); indexed_map and sort45_string
move to run with identical source bytes. All 60 stage0/candidate classification
checks and 28 Debug/Release execution checks passed before the full run.

Map keys retain reference semantics: immutable, non-owning, non-movable, with
explicit clone to obtain ownership. Existing insertion order and deleted-slot
handling are tested. Whole temporary Maps survive the loop; a field borrowed
from a temporary structure fails with E0305, matching stage0. String sorting
preserves the prior receiver checks and tests duplicate, empty, UTF-8 and NUL
values, nested fields, inout and lexical view boundaries.

The fifth application gate, atomic_ledger_application, has **32 scenarios,
783 commands and 517 batch payment rows**. All 14 archived scenarios remain
byte-identical. Eighteen contract-valid boundaries include 200 commands,
batch0/100, failure at the first/middle/last payment, remaining-row consumption,
ASCII ordering, Map growth, self-transfer checks and 64-bit balances/totals.
Independent delta-journal and clone-ledger oracles agree; both compilers in
Debug and Release pass **128 normal-return allocation-check executions**.
Eight source/contract/oracle/runner inputs are bound before and after the run.

Full local bootstrap passes **925/925 cases, 17/17 checks, zero skips**.
Stage2/stage3 C is byte-identical (1,039,982 bytes), as are the Linux binaries
(484,496 bytes). All five application gates, lexer and input identity checks
pass. Rust was unchanged; complete Debug and Release suites each pass
145/0/1, with frozen conformance 124/3/2.
[CI run 45826605112](https://github.com/msly/yavo/actions/runs/45826605112)
for implementation `bc8aefa` passes **6/6 jobs** on its first attempt: native
Linux, Windows and macOS bootstrap/application gates, Linux selfhost ASan/UBSan
and the independent checks.

The frozen 613-source corpus and its 392 stage0-accepted members are unchanged.
Final stage2 coverage is **106 to 112/392**, with no losses, subset violations
or timeouts and no admission of the 221 stage0-rejected inputs. The six gains
are two T01 implementations with three snapshots each. Deduplicating entry
file bytes gives **52/222 distinct inputs**; this does not deduplicate task or
dependency graphs and is not overall completion. SH-3 and formal SH-4 remain
incomplete; five compatibility gates do not change the one-of-three formal
application count.

## Historical validation: batch 45 (2026-11-26)

The matrix contains **897 cases / 263 executable inputs**: 231 run, 32 panic,
485 reject, 114 unsupported and 35 shared unsupported. The 22 additions have
fixed stage0 oracles: three run, 12 reject, three unsupported and four shared
unsupported cases. No old fixture is promoted. Targeted runs cover empty and
reserved vectors, int extrema, repeated sorting, field/inout permissions,
push/pop and whole-owner moves, plus return/break/continue cleanup. Compound
errors retain stage0's receiver, mutability/loan and arity precedence.
Complete local bootstrap passes **897/897 cases, 16/16 checks, zero skips**.
Stage2/stage3 C (1,027,977 bytes) and Linux binaries (479,152 bytes) are
byte-identical; all four application gates and input-identity checks pass.
[CI run 45794313520](https://github.com/msly/yavo/actions/runs/45794313520)
for implementation `67407bb` passes **6/6 jobs**: native Linux, Windows and macOS
bootstrap/application gates, Linux ASan/UBSan and the independent checks.

The new `sliding_median_application` check builds the unchanged original
`experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T11/final/main.yv`.
Its 28 fixed scenarios preserve all 14 archived inputs/outputs and add 14
contract boundaries: K=25, 0/24/25/200 values, even lower medians, duplicates,
mixed whitespace and window sums of ±25,000,000,000. Independent rank counts
and a second window-sort oracle agree on every output. Across 1,405 input
values and 1,112 windows, 112 stage0/selfhost Debug/Release executions must
match exact stdout, empty stderr and exit 0. Every run returns normally with
allocation checking. Eight inputs bind source, harness, shared runner,
fixed scenarios, two contracts and the archived/public cases. The gate uses
only the published valid ASCII/LF domain; it does not infer behavior for
empty files or invalid fields. All three previous application gates remain.

The final stage2 repository sweep preserves all 613 source files and all 392
stage0-accepted members. Coverage rises **100 to 106 / 392** with no losses
or strict-subset violations; none of 221 stage0-rejected inputs is accepted.
The six additions are yavo-a/yavo-b T11 final/work/snapshot paths, comprising
two distinct entry sources. Rust is unchanged; full Debug/Release suites
were rerun and pass 145/0/1 each, with frozen outcomes 124/3/2.
See [HANDOFF section 25](../HANDOFF.md).

## Historical validation: batch 31 (2026-11-24)

The matrix contains **490 cases / 172 executable inputs**: 144 run, 28 panic,
243 reject, 61 unsupported and 14 shared unsupported. There are 51 new cases;
the old for fixture was promoted without changing its bytes. All 52 batch
cases have independent fixed Rust oracles; 17 run programs pass 34 Debug/Release
executions with normal-return allocation checking.

The [local receipt](../compiler/validation/selfhost-for-2026-11-24.json) records
**490/490 cases, 12/12 checks and no skips**. Stage2/stage3 C is byte-identical
(685,990 bytes), as are the Linux binaries (321,368 bytes). The real lexer unit
passes stage1/stage2 C equality and Debug/Release heap checks. Full Rust suites
pass 145/0/1 per mode; frozen conformance remains 124 pass / 3 unsupported /
2 superseded. The sanitizer adapter binds 173 programs: 172 fixtures plus lexer.

The [oracle index](tests/evidence/batch31-for-oracle/oracle-index.json) preserves
a reference correction. Rust formerly read local/field/byte range starts after
the end expression's inout effects; its For lowering now snapshots the start
first. A new P0 fixture verifies local, field, index, byte and cached-end cases;
old Debug/Release failures and fixed runs are retained. Runtime and frozen
specification inputs are unchanged.

[Run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591) passed all six jobs at `046ccc6`.
Linux x86-64, Windows x86-64 and macOS arm64 each pass 490 cases, 12 checks,
zero skips, within-platform C/binary fixed points and the real lexer in both
modes. Each also passes 213 executions of the existing test-summary application.
Linux GCC ASan/UBSan passes 173 generated programs plus stage2 self-compilation,
490 checks, 172 emissions and 8 malformed inputs. The
[native index](../compiler/validation/batch31-native-2026-11-24.json) records
three API-matched archives, 44,165 rehashed files, 25,476 verified embedded
streams and 837 committed local input bindings. The original local receipt's
pending fields remain historical, supplemented by this native index.

## Historical validation: batch 30 (2026-11-24)

The frozen manifest has **439 cases and 155 executable inputs**: 127 run,
28 panic, 218 reject, 52 subset-unsupported and 14 shared-unsupported cases.
Batch 30 adds 67 cases and promotes the original `unsupported_vec_pop` input
byte for byte to `vec_pop_basic`. Its 68 targeted cases cover five Copy payload
families, mutable local/field/inout receivers, repeated pop/push, implicit
wrapping, lazy defaults, context-sensitive rejection and early-exit cleanup.

All 14 former truncation sites now pop the suffix in place. `cleanTemps` and
`cleanLocals` still only emit cleanup; metadata is removed at the original
discard points. Lock marks retain their outer prefixes, and loop exit states
are copied before the frames are removed. This removes rebuilding the retained
prefix; no performance improvement is claimed without measurement.

The [local receipt](../compiler/validation/selfhost-optional-pop-2026-11-24.json)
passes **439/439 cases, 12/12 auxiliary checks and zero skips**. Stage2/stage3
C is identical (668,302 bytes), as are the Linux binaries (308,504 bytes).
The real lexer passes both modes and both compiler generations. Full Rust
Debug/Release each pass 145/0/1 with frozen conformance 124/3/2.
[Independent oracles](tests/evidence/batch30-optional-oracle/oracle-index.json)
preserve the original attempts, source identities and diagnostic differences.
[Native run 45345384747](https://github.com/msly/yavo/actions/runs/45345384747)
passes **six of six jobs** at `7297e3b`. Linux, Windows and macOS each pass
439 cases, 12 checks and zero skips, with stage2/stage3 C and binaries equal
within each platform. Each platform passes the real lexer and 213 existing
application executions. Linux GCC ASan/UBSan passes 156 generated programs
and instrumented stage2 self-compilation, 439 checks, 155 emissions and 8
malformed inputs. The [native index](../compiler/validation/batch30-native-2026-11-24.json)
records the three API-matched archives, 40,102 rehashed files, 22,892 checked
embedded streams and 783 local input bindings matched to the implementation
commit. The local receipt retains its original pending fields.

## Historical validation: batch 29 (2026-11-24)

Local validation passes **372/372 cases**, including **140 executable C
emissions**: 112 normal programs, 28 panic programs, 190 rejected programs,
33 inputs accepted by stage0 but outside this subset, and 9 boundaries
unsupported by both frontends. The expanded corpus covers user-function
Result/try/catch behavior, owned success and error payloads, early-exit cleanup,
and explicit unsupported boundaries. Rust Debug and Release each pass
**145 tests, 0 failures and 1 existing ignored test**.

The compiler now returns an owned `Diagnostic` from `lex` through `compile`;
`main` reports it and exits with status 1. Parsing, type checking and lowering
still report the first error directly. A related stage0 return-completeness
bug is fixed: a `break` nested in a catch expression must count as an exit
from its enclosing loop. Targeted validation includes 18 missing-return
rejections with E0013 and 6 legal loop/closure boundaries.

The [final local receipt](../compiler/validation/selfhost-result-2026-11-24.json)
records **372/372 cases, 12/12 auxiliary checks and no skips**. The fixed-point
C is 623,204 bytes and the identical Linux stage2/stage3 binaries are 293,640
bytes. The new `lexer_units_selfhost` check passes stage1/stage2 C equality and
Zig Debug/Release allocation checks on the whole production lexer unit.
All 420 fixture C files and 560 mode binaries were rehashed against the receipt.
The [native index](../compiler/validation/batch29-native-2026-11-24.json) binds
`e355cb3` to [run 45226925936](https://github.com/msly/yavo/actions/runs/45226925936),
**six of six jobs passed**. Linux, Windows and macOS each pass 372 cases,
12 checks and no skips, with identical stage2/stage3 C and binaries within
each native build. The whole lexer passes both modes on every platform.
Linux GCC ASan/UBSan passes 140 fixture programs plus that lexer unit; the
instrumented stage2 compiler passes self-compilation, 372 checks, 140 emissions
and 8 malformed inputs. Three downloaded native archives were independently
checked against GitHub's size/digest, committed sources and execution bytes.
The historical local pending fields and
[batch 28 native records](../compiler/validation/batch28-native-2026-11-23.json)
retain their original scope.

## Historical fixed-point snapshot (2026-11-23, batch 27)

The [fixed-point summary](../compiler/validation/selfhost-fixpoint-2026-11-23.json)
binds source `67794d9` and records **335/335 cases and 11/11 checks**, no skips:

| Case category | Passed |
| --- | ---: |
| Normal programs | 98 |
| Panic programs | 28 |
| Rejected programs | 172 |
| Inputs accepted by stage0 but outside this subset | 29 |
| Boundaries unsupported by both compilers | 8 |

| Fixed point | Value |
| --- | --- |
| Assembled compiler source | 134,212 bytes, 3,600 lines |
| stage2.c = stage3.c | 574,149 bytes, SHA-256 `794fc02b…a3335d` |
| stage2 = stage3 binary | 261,272 bytes (Linux x86-64, `-O2 -s`) |
| stage1 self-compile | 0.03 s; stage2 C build with Zig ≈ 5 s |

stage2 exits through the allocation checker after compiling itself, so the
self-compilation path frees every heap block. All 335 fixture checks and all
126 executable emissions agree byte for byte between stage1 and stage2.

All 321 preceding fixtures are retained; `unsupported_text_slice` is promoted
to `text_slice_owned_print` with unchanged source, and 14 cases are new
(inout, take, text slices, `std/{fs, os}` with `catch _`, and their stage0
rejection codes). The historical prefix workloads now use the pre-rewrite
`diagnostic.yv` and `model.yv` frozen in `tests/prefixes/`.

## Independent compiler sanitizer gate

On Linux, after a successful bootstrap, run:

```sh
python3 tools/ci/selfhost_sanitize.py
```

The existing gate still sanitizes generated applications. It now also compiles
the actual stage2 C once with independently probed GCC ASan/UBSan, then requires
self-compilation, all fixture checks, all executable C emissions and eight fixed
malformed inputs to agree with their recorded and independent expectations.
Compiler C and runtime copies are hash-checked in a fresh build directory so an
adjacent header cannot replace the declared runtime. Reports bind the source
bundle, fixed point, actual compiler, GCC/libraries and every input/output.

`--bootstrap-output DIR` selects a nondefault completed bootstrap directory.
The fixed corpus is not fuzzing, and this Linux gate does not claim TSan or
LeakSanitizer coverage. Normal compiler returns use the allocation checker;
first-error `os.exit` paths do not run its heap summary.

## CLI

```sh
compiler/target/release/yavo build selfhost/main.yv --release -o yavo-selfhost
./yavo-selfhost check selfhost/examples/gcd.yv
./yavo-selfhost emit-c selfhost/examples/gcd.yv -o gcd.c
./yavo-selfhost build selfhost/examples/gcd.yv --release -o gcd
./yavo-selfhost run selfhost/examples/gcd.yv --release
```

On Windows use `.exe` for the compiler and explicitly named native output.
The command forms are:

| Command | Supported form |
| --- | --- |
| Check | `check <input.yv> [--json \| --json-diagnostics]` (flag may precede input) |
| Emit C | `emit-c <input.yv> -o <output.c>` |
| Native build | `build <input.yv> [-o <output>] [--release]` |
| Native run | `run <input.yv> [--release] [-- <program arguments>...]` |

### Structured check diagnostics

Check accepts one input and one optional `--json` or `--json-diagnostics` flag,
before or after the input. Successful JSON mode writes one compact UTF-8 object
plus LF to stdout, empty stderr and exit 0; a frontend rejection writes one
diagnostic and exits 1. Usage errors exit 2. Root read/UTF-8 errors exit 3 with
empty stdout and the existing text stderr, since no valid source snapshot exists.
The original three-argument text form keeps leading-dash filenames; the two new
JSON spellings alone are reserved as missing-input requests. Emit-C's positional
input/output paths remain literal, including an output named `--json`.

The schema is `yavo-diagnostics/p0`, with `frontend: selfhost` and
`diagnostic_mode: first-error`. Each primary range is a UTF-8 byte half-open
interval with one-based Unicode scalar line/column coordinates. LF starts a
new line; CR and TAB each advance one column. Parser/type/ownership diagnostics
cover the original primary token, not an inferred whole-expression span;
lexical failures use the separately frozen consumed/lookahead boundary. EOF
and synthetic newline ranges may be empty. The original error precedence,
code and message are preserved; warnings are zero and notes/fixes are empty.

`source_sha256` identifies the root's original bytes. `sources` lists each
successful UTF-8 read in the existing BFS load order, including a lexically
invalid file already read, and excluding a missing or invalid-UTF-8 file.
The list is the loaded prefix at the first failure; there is no project digest.
Rendering uses the captured source even if disk contents subsequently change.
Full JSON field and lexical boundary rules are in the
[frozen contract](tests/diagnostics50/CONTRACT.md).

Build/run options may precede or follow the input. Duplicate options, additional
inputs and unknown options are usage errors. Arguments following run's `--`
are opaque, including empty strings and option-like values; `run` rejects `-o`.
Build's default output is the input basename with a trailing `.yv` removed
(a basename exactly `.yv` is preserved), with `.exe` appended on Windows.
`--target`, `--emit-c`, `--json` and `--json-diagnostics` are not supported by
build/run; the separate `emit-c` command remains available.

`emit-c` retains exclusive creation through `fs.writeNewText`: an existing
output, including a source link, is refused. Its frontend finishes before
opening the output; a failed write can leave a partial newly created file.
Native `build` may replace an existing non-source output. Before creating
temporary files or launching the backend, it compares that output's file
identity against the entry and every reachable source module, including
hardlink and symlink aliases. Frontend rejection leaves existing output
untouched. Backend output is not promised atomic on a failed compilation.
Manually compiling emitted C still requires `compiler/runtime/yavo_rt.h`;
ordinary build/run write their embedded copy automatically.

The backend is the already-installed native
`YAVO_HOME/toolchains/zig-HOST-VERSION/zig[.exe]`. If `YAVO_HOME` is absent,
the root is `HOME/.yavo` on POSIX or `USERPROFILE/.yavo` on Windows; a defined
empty root is an error. A nonempty `YAVO_CC` is rejected, while an empty value
is ignored. Installation remains a stage0 task (`yavo toolchain install`).
The driver passes `cc -std=c17 -fno-strict-aliasing`, adds `-O0 -g` for Debug
or `-O2 -s` for Release, and forwards the presence-based `YAVO_ALLOC_CHECK`
instrumentation switch. Backend stdout and stderr both go directly to driver
stderr. A launched program inherits stdin/stdout/stderr and receives its argv
without shell expansion or PATH lookup.

Temporary roots use `TMPDIR` or `/tmp` on POSIX, and `TEMP`, then `TMP`, then
the current directory on Windows. Defined empty roots fail. The driver creates
a unique directory after frontend success, deletes its known C/header/program
files on every handled return path, then requires the directory to be empty
and removable. Unexpected files are preserved and reported as cleanup failure.
There is no Yavo binary/PCH cache; the backend's own cache may still operate.

| Exit | Meaning |
| --- | --- |
| `0` | Successful check, emission or build; or a successfully run program exiting zero |
| `1` | Frontend rejection, including unsupported language constructs |
| `2` | CLI usage or attempted native output replacement of a source file |
| `3` | Input/output, environment, temporary-directory or backend-launch failure |
| `4` | Backend nonzero/signal failure, program-launch failure, or temporary-directory cleanup failure |
| Program status | Ordinary `run` forwards a completed program's normal status; POSIX signal termination maps to `128 + signal` |

Cleanup failure overrides an earlier backend or program status with `4`.
Diagnostics currently report the first error as
`path:line:column: CODE: message`; columns count Unicode scalars and stored spans
use UTF-8 byte offsets. `E0020` is syntax, `U0001` is an unimplemented construct
or explicit implementation limit, and other `E` codes indicate semantic errors.
First-error check JSON is supported as described above. Suggestions, richer
diagnostic data and multi-error recovery remain future work.

## Accepted input subset

| Area | Implemented |
| --- | --- |
| Files | Source modules via leading `use a`, `use a/b`, `use a/{b, c}`; per-file imports and top-level `pub`, cyclic imports, qualified functions and original source locations; standard fs/io/os/process imports and the supported std/collections Heap API; standard/user import alias conflicts are E0011; required entry `fn main()` |
| Types | `int` (signed 64-bit), `byte` (unsigned 8-bit), `bool`, borrowed `str`, owned `String`, supported scalar/struct/String vectors and `Vec[Vec[int]]`, unit return; nominal structs, payload-free user enums, four nominal standard Error enums, prelude Utf8Error, flat tuples including String, supported `T ! E` results, Copy `?T` and Optional flat String tuples |
| String | Empty construction, str.toString, len, clone, push, output, String/str comparison, UTF-8/NUL; byte indexing; startsWith/endsWith/contains/find/split/parseInt/trim/lines/fields |
| Vec | Typed empty construction, empty/nonempty literals including Vec[byte] and Vec[Vec[int]], len, consuming push, reserve, recursive clone/drop; stable in-place int/String sort; pop for Copy elements; checked Copy reads/writes and nested-int-vector places; expression-local owned-element access and the restricted slice-parameter aliases described below |
| Map | Map[String, V] for int/bool/byte values; contextual [:], consuming set, has/getCopy/remove, len/clone, locals/parameters/returns/fields and read-only two-binding traversal |
| Set | Set[String], empty []/[:]/Set[String]{}, consuming add, has/remove, len/clone; locals, parameters, returns, fields and Result payloads; iteration remains U0001 |
| Heap | `Heap[(String, int)].by((a, b) => expression)` after `use std/collections`; pure capture-free comparator, consuming push, owning pop, len, whole-value moves and recursive cleanup; locals, ordinary/sink/inout parameters and returns |
| Aggregates | Forward/nested fields, construction, reads and mutable fields; Copy values or owned String/Vec/Map/Set/nested struct fields; constant defaults and empty aggregates; by-value cycles rejected |
| Tuples | Flat int/byte/bool/String/non-owning struct/payload-free enum elements; left-to-right construction and ownership transfer, numeric projection, locals, arguments, returns, whole-value assignment/inout and Optional payloads; bounded structural signatures |
| Enums | Payload-free user enums support qualified/context-inferred cases, nominal type checks, equality/inequality and case-name output; standard Error enums additionally preserve their integer payloads for construction, display and match, and reject equality/order |
| Literals | Decimal integers including negative `INT64_MIN`, contextual byte-range integers, single-quoted ASCII byte literals and seven escapes, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL; vector literals with contextual or first-element type inference |
| Functions | Typed parameters and supported return types, forward calls and recursion; ordinary owned parameters borrow read-only; sink parameters take ownership of supported owning types while Copy inputs remain copies; inout with `&variable`/`&variable.field`; borrowed str/Vec[str]/slice returns require `of` sources and cannot name sink parameters |
| Results | User-function `T ! E` and complete Results from the ten supported fallible standard calls; contextual `ok(value)`/`err(error)`, implicit success wrapping, `try`, terminating `catch name { ... }`/`catch _ { ... }`; standard Error/Utf8Error and supported owned String/Vec/Map/Set/struct payloads, with cleanup on propagation |
| Optionals | `?T` for int/byte/bool/non-owning struct/payload-free enum/flat tuple including String; some/none/wrapping, tag queries, lazy coalesce and terminating fallbacks; if-let borrows an existing owned payload or moves a temporary payload; while-let supports temporary owning Optional values |
| Read-only slices | Supported scalar/str/String/named element slices, including owned structs; Vec-to-slice borrowing, read-only traversal and indexed access; direct owned-struct slice parameters support inferred aliases and alias chains, while persistent String-element aliases remain U0001 |
| Text slices | `s[start..end]` on str/String with int bounds and P-INDEX/P-STR-BOUNDARY checks; views of owned values may be bound locally while their owner stays unchanged; escaping the owner scope remains rejected |
| Standard calls | `take(&place)` for String/Vec places; `os.args()`, `os.exit(code)` and static `os.host()`; `io.readAll`, `fs.readText`, `fs.writeNewText`, `String.fromUtf8`, `os.env`, `fs.tempDir/removeFile/removeDir/sameFile` and `process.run` return full typed Results, supporting saved values, return, try and named/discard catch |
| Locals | `let`/`var`, inferred or explicit types, assignment and arithmetic compound assignment; active locals cannot be shadowed |
| Control flow | if/else-if/else, if-let and supported while-let; standard-Error-only match expressions/statements with real payload bindings; while/loop; int/byte half-open range for; one/two-binding read-only Vec/slice/args traversal and two-binding Map traversal; fresh per-iteration owning payloads, break/continue/return cleanup and return-path checks |
| Expressions | Parentheses, int unary minus, boolean not, checked int/byte arithmetic, wrapping arithmetic, bit operations/shifts, contextual integer arithmetic, equality/order, short-circuit boolean operators |
| Builtins | Single-argument `print`/`println`/`eprintln` as direct statements including decimal byte output, `assert(bool)`, explicit `int(...)`/`byte(...)` conversions, checked `abs(int)`, identity `abs(byte)` and matching int/byte `min`/`max`; user functions take precedence over assert/abs/min/max preludes |
| Ownership | Consuming bindings/assignment/construction/returns and sink calls; moved-value checking, read-only parameters/aliases, argument staging, indexed-receiver exclusive access and root-based receiver protection |
| Runtime behavior | Checked arithmetic/panic/output, ordered evaluation, moved-source zeroing and cleanup on scope/statement/control-flow exits |

Value-returning functions require an explicit return. Direct calls, method
calls, `try` and `catch` may be expression statements; a Result-valued call
must be handled or explicitly discarded with `let _ = ...`. Plain unit-returning
functions allow `return` or `return ()`. The parser bounds syntactic nesting
at 128 and expression lowering at 256; exceeding either returns `U0001`.

Aggregate fields may hold int, byte, bool, String, supported vectors/Maps/Sets,
payload-free enums, standard Error/Utf8Error values or nested structs. Borrowed `str` fields are rejected with E0301;
unit fields remain outside this subset. Struct equality/printing/clone and enum ordering are
rejected consistently with stage0. Explicit constructor fields are evaluated
in source order. Bare enum cases use annotation/parameter/return/field/
assignment/RHS comparison context.

### Public system calls and String slices

The seven new calls share the [public standard-library contract](../docs/stdlib_contract.md)
with stage0. Their runtime behavior is the same in both frontends. Supported
fallible calls produce ordinary typed Results with the corresponding nominal
standard error type; their successful payload and argument ownership are unchanged.

| Call | Successful value and ownership |
| --- | --- |
| `os.env(name, fallback)` | Independent owned String; missing variables copy fallback, while a present empty value remains empty |
| `os.host()` | Static `str` identifying compilation architecture and OS; no allocation |
| `fs.tempDir(parent, prefix)` | Owned path of a newly created unique directory under the explicit parent; dropping the String does not remove the directory |
| `fs.removeFile(path)` | Unit after removing one file directory entry; never recursive |
| `fs.removeDir(path)` | Unit after removing an empty directory; never recursively deletes contents |
| `fs.sameFile(first, second)` | Bool from native file identity, following links; missing paths yield false under the contract's ordered lookup |
| `process.run(program, args, stderrOnly)` | Int after a synchronous explicit-path launch and wait; args is a borrowed `[]String` without argv[0] |

Arguments are borrowed, not consumed, and evaluated left to right. Earlier
argument owners stay protected through later argument evaluation and the call.
Temporary program strings and argument vectors are released on success,
failure and early propagation. `process.run` distinguishes launch/wait errors
from an ordinary nonzero child exit; POSIX signals are successful results with
a negative signal number. `stderrOnly` selects inherited stdout/stderr or
both output streams directed to parent stderr. There is no shell, output
capture, timeout or environment override in this API.

Fallible results may be saved, explicitly discarded, returned, propagated with
`try`, or handled with `catch e { ... }` / `catch _ { ... }`. A catch handler must
terminate with return, loop control or another accepted terminating path;
fallthrough is E0601. The immutable error binding is local to the handler and
cannot shadow an existing binding. Error types do not implicitly convert across
standard modules. General user-function Results retain their existing support.

`[]String` parameters borrow an existing `Vec[String]` or supported temporary
vector. Read-only iteration, indexed text inspection, output, ordinary
borrowed calls and explicit `.clone()` are supported without moving an
element. Returning an indexed String as an owner is E0404; implicitly returning
it as `str` is E0001. An explicit text slice can return a view with the
corresponding `of` source. Persistent inferred aliases of String elements
remain U0001. The owner and each temporary stay alive for the full use.

### Range and vector loops

Batch 31 local and three-platform native acceptance have passed.
`for i in start..end` accepts matching `int` or `byte` bounds, with contextual
integer literals. Both bounds are evaluated once, left to right, before the
loop binding exists. The lower value is captured before upper-bound side
effects. The range is half-open: an equal or descending pair executes no
iterations. The binding is read-only, and `continue` still advances it.
Finite loops retain a possible zero-iteration path in return and ownership
analysis, even when their body always returns.

`for item in items` and `for index, item in items` accept the existing Vec and
read-only slice types, including locals, ordinary/inout parameters, fields and
temporary vectors. `os.args()` is a static []str and supports both forms.
The index is a read-only int; `_` can discard either binding. Copy elements
are copied; owned String/struct elements and the inner Vec[int] of a
Vec[Vec[int]] are read-only views. The iterable
is not consumed; its root remains borrowed throughout nested loops and calls.
An otherwise permitted mutation, move, `take` or inout access to that root
fails with E0102. Reads and nested read-only traversal remain allowed, and
the loop releases its borrow when it ends.

Headers run before entering the new loop's control context. A `break` or
`continue` inside a terminating header `catch` targets an enclosing loop.
Owned header temporaries survive `continue` in the new loop's body and are
cleaned when that loop completes or breaks; an enclosing jump, return or
propagation also cleans the temporaries whose scope it exits.

Explicit reference/mutable traversal (`in &items`) and Set traversal
remain outside the subset. Two bindings use `for i, item in items`, without
parentheses. A range with two bindings is invalid syntax (E0020); an existing
non-iterable scalar name is E0001. Collection mutation/move remains E0102,
and writing either loop binding is E0014.

The earlier batch31 migrations were these four loops; batch42 additionally
uses indexed traversal in `types.declaredIn`:

| Source function | Migration |
| --- | --- |
| [`lexer_test.detachedFailure`](lexer_test.yv) | `0..32` builds the owned token text before an escaping diagnostic |
| [`lexer_test.main`](lexer_test.yv) | `0..512` builds the leading-zero integer input |
| [`lexer_test.main`](lexer_test.yv) | `0..16` repeats owned diagnostic creation and the existing code/message/position assertions |
| [`parser.store`](parser.yv) | Read-only Copy Vec traversal appends child IDs to the shared list pool |

The migrations preserve the original input strings, loop bodies and assertions;
`parser.store` replaces the indexed element read with the copied loop binding.

### Read-only Map loops

`for key, value in map` accepts `Map[String, V]` with int, bool or byte values.
Both bindings are required; `_` may discard either. Iteration follows insertion
order, skips removed entries and visits a reinserted key at its new position.
The key is a borrowed read-only String; the value is a read-only Copy local.
`key.clone()` creates an independent owner. `let _ = key` reads without moving
or dropping it, while an inferred persistent key alias remains U0001.

The Map or named field's owner stays frozen for the loop. Queries, cloning and
nested read-only traversal are allowed; mutation, movement or inout access to
the Map fails with E0102. Key assignment/modifying methods/take are E0103;
moving a key into an owned String, return or Vec is E0404. Supplying `&key` to
an inout String parameter is E0404, and to another inout type is E0001.
Value assignment remains E0014.

A whole temporary Map lives through the loop and is cleaned on completion,
break, return or propagation. Iterating a Map field of a temporary structure
is E0305, matching stage0. Local views produced by `key.trim()` may be used
inside the iteration; returning them, assigning them to outer bindings or
storing them in another Vec remains U0001. Explicit/mutable references and
other Map key/value types remain outside this subset.

### String sets

`Set[String]` is a distinct owned language type. Locals can use `[]`, `[:]` or
`Set[String]{}`; fields accept `[]` as a constant default. Stage0 rejects `[:]`
as a Set field default, and the subset preserves that distinction. Nonempty
vector literals cannot initialize a Set.

Supported methods are `add`, `remove`, `has`, `len` and `clone`. `add` consumes
the owned String key and returns unit. A duplicate insertion drops the incoming
key and keeps the original. Text queries accept str or String; `remove` returns
bool and drops the stored key only when present. Clones own independent keys.
Locals, fields, parameters, returns and either Result payload can contain Sets;
ordinary scope and early-exit cleanup releases their keys and storage.

The C representation reuses `Map[String, bool]` helpers with a hidden true value,
while type checking keeps Set and Map distinct. Writable receivers retain the
existing reservation rules during argument evaluation. A read-only query whose
argument modifies or moves its own receiver remains U0001. Set iteration,
`take`, other key types, direct `Vec[Set[String]]` and Set-valued Optional payloads
remain outside this subset. These are explicit U0001 boundaries.

### Flat tuples

Tuple elements may be int, byte, bool, String, non-owning structs or payload-free
enums. Element expressions are evaluated left to right: Copy elements are
snapshotted and String elements transfer ownership. A later inout argument
cannot change an earlier element's snapshot. Structural type identity
uses a collision-free radix encoding bounded by signed int capacity; exceeding
that capacity is U0001. Capacity depends on the program's named-type count and
the current atom encoding, which includes String.
Layouts are emitted after complete named layouts and before function declarations,
including tuple signatures used only by otherwise-unused functions.

Tuples support local bindings, whole-value assignment, parameters, return values
and inout. Numeric projection accepts decimal ordinals (including `.01`) and
reports E0010 for unknown/out-of-range members. A singleton `(value,)` is inferred;
an explicit singleton tuple type remains E0020, matching stage0. String projection
borrows the field; whole tuples can move, but a field does not implicitly move
out of its owner. Drop releases owned String elements. Nested tuples, owned
struct elements, borrowed/container/Optional elements, tuple fields, Vec[tuple],
tuple Results, general references and destructuring remain outside the subset.
The supported Heap is the one tuple-container exception described below.
Unit element expressions are E0001. Tuple element writes are
rejected (E0014 through an immutable root, otherwise E0001); comparison is E0501
for matching tuple types and E0001 for different types; direct output is E0001.

### Nested integer vectors and Heap

`Vec[Vec[int]]` is the only admitted nested vector shape. It supports contextual
and inferred literals, whole-value moves, recursive clone/drop, checked nested
integer reads/writes, inner-vector replacement and modifying methods on writable
inner places. An indexed modifying receiver takes exclusive access before its
arguments are evaluated; reading or consuming the protected outer vector in
those arguments is E0403. Read-only traversal of the inner vectors is supported.
Other nested element types, a third vector level, persistent aliases of local
owned vector elements and taking an indexed owned value remain U0001.

`use std/collections` enables `Heap[(String, int)]`. Construction requires
`.by((a, b) => expression)` with two untyped parameters and a boolean result.
The expression may use its parameters, tuple projections, literals and supported
unary/binary operators. Captures, calls, block bodies, typed parameters and
general standalone closures remain U0001. Comparator inputs are read-only;
constructing the Heap does not transfer their String fields.

`push` consumes one tuple; `pop` transfers one tuple into an owning Optional;
`len` borrows the Heap. Remaining entries are recursively released on scope
exit, normal function fallthrough and early exits. Heap locals, parameters,
returns and whole-value moves are supported. Other Heap element shapes, fields,
Result combinations, default construction without `.by` and additional Heap
APIs remain unsupported. `Heap.clone()` is E0001 and matching-Heap comparison
is E0501; these are invalid operations, not newly supported APIs.

### Integer builtins

`abs` evaluates its argument once and preserves int/byte context. Negative int
values use checked negation, so `abs(INT64_MIN)` panics with P-OVERFLOW; byte
arguments are returned unchanged. Other numeric types remain U0001.

`min(a, b)` and `max(a, b)` require exactly two matching int/byte operands.
Each operand is evaluated once from left to right, including snapshots of locals,
fields and indexed elements before a later inout argument can change the source.
A pure numeric expression may acquire byte context from either an expected byte
result or the other operand. Its checked arithmetic still executes before RHS
effects when it is the left operand. Implicit Optional/Result wrapping supplies
no numeric payload context: `?byte = min(1, 2)` is E0001, while
`?byte = some(min(1, 2))` is accepted. User functions override the builtins;
a local with the same name remains uncallable (E0001). Wrong arity is E0012,
incompatible operands E0001 and out-of-range literals E0003.

### Integer and String vector sorting

`Vec[int].sort()` and `Vec[String].sort()` take no arguments, return Unit and
sort ascending in place. They reuse the stable runtime merge sort, with a
relational int comparator or lexicographic UTF-8 byte comparison for String.
Integer comparison cannot overflow at either extreme. String descriptors move
without cloning or dropping their allocations; the vector still owns each
string and releases it once during element cleanup. The vector descriptor,
length and ownership are preserved; the auxiliary buffer is released within
the call. Empty and single-element vectors need no auxiliary buffer.

Writable names, inout parameters and nested struct fields are supported,
including parentheses around a field chain. Stage0 rejects `(values).sort()`
when the receiver is only a parenthesized name; this subset keeps that U0001
boundary and stage0's temporary-receiver restriction. Owned indexed receivers
are additionally outside this subset. Existing loans, iteration locks and
reservations block mutation. Read-only receivers are E0014, moved roots E0402,
earlier call borrows and iteration locks E0102, and reservations E0403;
bound-view and access-lock restrictions retain their U0001 boundary. Wrong
arity is E0012 and unordered bool/Named elements are E0501. Byte and str
vector sorting and sortBy remain U0001. `let _ = values.sort()`
is allowed; a normal binding or return cannot use its no-value result.

### Optionals, pop and matching

Optional payloads are limited to int, byte, bool, non-owning structs,
payload-free enums and supported flat tuples, including tuples with String
elements. `some(value)` infers its actual payload type; `none` needs
an Optional context. Bindings, assignments, arguments and returns can wrap a
matching supported payload implicitly. Owning tuple payloads transfer and drop
their String fields; both tag methods take zero arguments.

`Vec.pop()` evaluates a writable place once, checks its existing move/borrow/
reservation permissions, then returns `none` for an empty vector or removes
and copies the last element into `some`. Capacity is retained. The returned
Optional has no borrow back to the vector. Vec pop remains limited to Copy
elements, including a writable inner Vec[int] place of Vec[Vec[int]];
Heap pop supplies the new owning tuple Optional. Temporary receivers and
other owned indexed places retain their existing U0001 boundaries.

`lhs ?? rhs` is right associative, binds more tightly than comparisons and
less tightly than bitwise OR, and evaluates the RHS only when the LHS is none.
Its branch temporaries are cleaned inside that branch and conditional moves
are merged into the following ownership state. The RHS must have the exact
payload type; it does not receive implicit wrapping. Context is also distinct
from wrapping for literals: `?byte = some(1)` is valid, while `?byte = 1` is
E0001; use `byte(1)`, a byte literal or an existing byte for implicit wrapping.
The same rule holds for tuples: `?(byte, byte) = some((1, 2))` supplies element
context, while implicit wrapping of `(1, 2)` yields an int pair and is E0001.
The same raw-context rule applies to Result success values: `return ok(1)`
can provide byte context, but `return 1` in a `byte ! E` function is E0001.

`if let name = optional { ... }` evaluates the condition once and accepts
else/else-if-let. Copy payloads are copied; an owning payload of a temporary
Optional moves into the then arm and its source Optional is cleared. An
existing owning Optional lends a read-only payload instead. A named
binding of an existing Optional keeps a scoped loan: modifying that source
inside the then arm is U0001 even after the binding's last use, because this
subset has no liveness analysis. Copy payloads may be copied or returned;
views of a borrowed String field retain the Optional owner's required `of`
source. A further inferred named alias of that owning payload is U0001;
`let _ = payload` reads without transfer, and an explicit owned annotation
cannot take its ownership (E0404). `_` has no source loan. Read-only binding
writes are E0101; writes to a Copy
binding from a mutable source or temporary are U0001, matching stage0.
Branch move/source joins and loop replays preserve these rules.

`while let` supports Copy Optionals and temporary owning tuple Optionals such
as `heap.pop()`. Each iteration creates a fresh payload binding. The failed
match releases header temporaries; normal completion, continue, break and return
release the current payload. Moving that payload clears its old owner, and
replay creates a new binding rather than carrying the prior iteration's moved
state. Matching an existing owning Optional with while-let remains U0001.

Jumping `?? return/break/continue` is accepted for a whole binding initializer
or assignment RHS. Coalescing an owning tuple Optional consumes the owned
container and transfers either its payload or the lazily evaluated fallback.
String itself, Vec/Map/Set/Heap/owned-struct/borrowed-str/unit payloads, nested
Optional, Optional fields/container elements, Optional/Result combinations,
owned-element Vec pop, destructuring patterns and match remain U0001. Optional comparison and
direct printing are also U0001, matching stage0.

### Results and error propagation

`T ! E` is supported in function signatures and local annotations. Its success
payload may use an existing supported non-Result, non-Optional, non-tuple type, including `String`,
`Vec[Token]` and owned structs. The error payload may be an owned struct or
another supported non-Result value; unit errors are rejected with E0001 and
borrowed errors with E0301. Local results can carry static `str`; borrowed
`str` function returns remain U0001. Wrapping a view into an owned String in
a Result is also U0001, preserving the existing persistent-view boundary.

Annotations, parameters, assignments and returns supply the expected Result
type for `ok`/`err` and implicit success wrapping. An unannotated `err(...)`
has no inferable success type and is E0001. A `() ! E` function accepts
`return`, `return ()`, `return ok(())` and falling through as success.
`try` requires an enclosing Result-returning function with the same error type.
`catch name { ... }` transfers the error into its binding; `catch _` discards
it. Both handlers must terminate with return, loop control or an accepted
process exit; reaching the closing brace is E0601. Arrow catch is U0001.

An owned Result moves when consumed by `try`, `catch`, assignment or return.
Its active payload is transferred before the Result is cleared, and only the
active branch is destroyed. Moving from a borrowed Result parameter is E0401;
using a consumed owner again is E0402. Early propagation cleans live locals,
statement temporaries and initialized prefixes of aggregate/vector values.
Result fields, `Vec[Result]`, nested Results and public Result member/method
operations remain outside this subset; Result comparisons are E0501.

### Standard errors and payload matching

| Nominal type | Variants / fields |
| --- | --- |
| `fs.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidPath` |
| `io.Error` | `Utf8(offset: int)`, `Io(code: int)` |
| `os.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidArgument` |
| `process.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidArgument` |
| `Utf8Error` | Prelude Copy struct with `offset: int` |

These values are Copy and may be locals, parameters, return values, ordinary
struct fields and supported Result payloads. Each standard Error requires its
module import in the current source file; standard imports do **not** introduce
the short type name `Error`. User-defined or imported user-module types named
Error retain their normal identity. Identical tag/payload layouts do not make
different modules' errors interchangeable.

Contextual `.Utf8(5)` / `.Utf8(offset: 5)` and `.Io(-7)` / `.Io(code: -7)`
construct real payloads under an expected error type. Field names, arity and
types are checked. Qualified zero-payload `fs.Error.InvalidPath` is accepted.
Qualified payload construction such as `fs.Error.Utf8(5)` remains U0001 in both
the current stage0 and this subset. `Utf8Error { offset: 5 }` exposes `.offset`;
mutation follows the existing place/borrow rules.

Error display is exactly `Utf8(offset: 5)`, `Io(code: -7)`, `InvalidPath` or
`InvalidArgument`, without a module prefix, through direct output, interpolation
and format. Display does not consume these Copy values. Error comparisons reject
with E0501. Direct output/interpolation of the Utf8Error struct rejects with E0001;
display its offset explicitly.

Only standard Error enum values may be matched. Flat contextual/qualified case
patterns bind an int payload; zero-payload cases, wildcard and whole-value
bindings are supported. The scrutinee evaluates once. Arms may be expressions,
statements or blocks; owned expression results and return/break/continue use the
existing branch-merge and cleanup rules. Matches must be exhaustive (E0602),
nominal types and payload arity must agree, and arm bindings cannot escape or
shadow existing locals. Stage0-accepted duplicate/covered arms are not turned
into new errors.

Direct Error/Utf8Error vector, Optional and tuple storage, general user payload
enums, other enum/Optional/Result match and nested payload patterns remain U0001.
Arrow catch, `io.readLine` and `fs.writeText` remain outside this scope. The
complete ten-call list, original semantic evidence and exact I/O oracle are in
the [frozen contract](tests/errors51/CONTRACT.md); the sole new-fixture source
correction is recorded in the [amendment](tests/errors51/semantic-amendment.json).

### Byte values and text reads

`byte` is a Copy unsigned 8-bit value. A single-quoted literal has a fixed
byte type; it is one ASCII character or one of `\n`, `\t`, `\r`, `\0`,
`\\`, `\'` and `\"`. Raw TAB, CR, NUL and DEL are accepted; raw newline,
non-ASCII characters, empty/multiple characters and other escapes are E0020.
Byte output is the decimal numeric value.

Integer literals and supported literal arithmetic expressions take byte
context from annotations, parameters, returns, fields, vector elements or
a typed comparison/arithmetic operand. Ordinary int variables and byte
literals do not implicitly change type. Type inference can inspect a typed
right operand without moving its runtime effects before the left operand.
A byte-context literal outside 0..255 is E0003. Constant field declarations
defer numeric range checks and fixed byte-to-int fitting until constructor
instantiation, matching stage0.

`int(byte)` widens explicitly; `byte(int)` checks its value and panics
P-CONVERT outside 0..255. Identity `int(int)` and `byte(byte)` conversions
are supported. Arithmetic uses the existing checked/wrapping u8 runtime
operations; overflow, division by zero and invalid shift width retain the
stage0 panic categories in both modes.

String/str indexing checks bounds and copies one UTF-8 **byte**, including
a continuation byte or NUL. It does not decode a Unicode scalar. Index
arguments accept int or byte, while `Vec.reserve` still requires int.
The receiver remains protected during index evaluation. The materialized
byte immediately detaches from its owner and can survive later movement or
replacement of that owner. Temporary String receivers live through the
access. Text indexing is read-only. Text slices may be bound locally; views
into an owned value keep that owner unchanged while the binding is in scope.

### Sink parameters

`name: sink T` uses the existing supported type shapes, including String,
vectors (also Vec[Vec[int]]), structs, flat String tuples, supported Optional
and Result values, Maps, Sets and Heap[(String, int)]. It adds an ownership
mode, not new payload or container forms. Owning arguments move; Copy arguments
remain copies and may be reused. A sink parameter is still a read-only binding.

Calls evaluate arguments left to right. A consumed local is marked moved
immediately, while its owning staging temporary survives until the call runs;
the caller clears that temporary after transfer. If a later argument returns
or propagates an error before the call, ordinary temporary cleanup releases
the staged owners. Callees release sink parameters on normal fallthrough and
early returns, or transfer a returned owner and clear the parameter.

An earlier borrowed argument blocks a later conflicting sink move (E0102);
an earlier sink move blocks a later read (E0402). An indexed modifying receiver
also blocks a conflicting consuming call (E0403). A returned-borrow clause
cannot name a sink parameter (E0304). Existing restrictions on references,
inout arguments, type shapes and borrow lifetimes continue to apply.

### Ownership boundaries

Ordinary string literals are `str`. Binding, assigning, returning or explicitly
initializing a String field from a literal requires `.toString()`; only a
String field's constant default has a special literal conversion. String
arguments may borrow a str view for a call. Local view bindings keep the owner
unchanged in their scope; borrowed returns must carry permitted `of` sources.

Owned locals move into bindings, assignments, constructors, returns and sink calls.
Ordinary parameters, output, comparison, len and clone borrow. Moving out of
a field or a read-only parameter is E0401; moved-value reuse is E0402.
Modification through a read-only place is E0014; a conflicting push receiver
is E0403, including sibling fields of the same root. Earlier call arguments
retain their borrows until the call and conflicting later moves are E0102.
String temporaries remain alive through the statement; short-circuit branch
temporaries are cleaned inside their branch. Locals are cleaned on return,
break/continue and scope exit; moved sources are zeroed before later cleanup.

Loop move checks reproduce stage0's conservative second body pass, including
moves before a syntactic break/return. Reinitialization restores availability.

Vectors accept `int`, `byte`, `bool`, payload-free enums, `String`, named structs
and the single nested form Vec[Vec[int]], including structs with String or
supported Vec fields. Nominal enum/struct
identity is preserved in the element type. A vector owns its buffer even
when its elements are Copy; a struct containing a vector is therefore owned.
Whole vectors move into bindings, assignments, fields and returns, while
ordinary vector parameters borrow read-only.

Owned elements move into vector literals and push; the transferred source is
cleared and later reuse is rejected. A `Vec[String]` element must already be a
`String`; no `str`-to-`String` conversion is implicit. Drop recursively releases
initialized elements before the buffer, and clone creates independent element storage
through String, Vec and struct fields. Indirect recursive types such as
`Node { children: Vec[Node] = [] }` are supported; direct by-value cycles
remain E0017. Internal struct clone helpers do not add a public built-in
`struct.clone()` method.

Reading a Copy element returns an independent value. Copy-element writes,
including nested struct-field and arithmetic compound assignments, require
a mutable place and check bounds.

Reading an owned element produces expression-local read-only access. It can
be projected through fields, read by output/comparison/len, passed to an
ordinary read-only parameter, or explicitly cloned when the selected value
is String or Vec. The borrowed slot is not an owning temporary and is never
independently dropped or cleared. Indexing never implicitly clones its
element. Its original root remains protected while
the borrow is needed; a temporary owning receiver remains alive through the
statement. Copy field reads, len and completed clone results detach from the
root, allowing later mutation such as `v.push(v[0].clone())`.

For a direct immutable read-only `[]OwnedStruct` parameter, `items[i]` may
initialize an inferred read-only alias; its foreach element can supply the
same alias. `let` and `var` aliases and further inferred alias chains retain
the slice parameter as their owner. Reads, ordinary borrowed calls and permitted
borrowed returns preserve that source. `var` does not grant mutable access:
field writes are E0101, supplying an owned inout argument is E0404, and whole
reference rebinding is U0001. An explicit owned annotation cannot take the
aliased element's ownership (E0404).

Persistent aliases of local owning Vec elements, temporary collection elements,
Map keys and other iterated owned elements remain U0001. Using a borrowed slot
as an owned return, push argument or typed vector element is E0404; moving an
owned field out of it is E0401. An arbitrary user struct still has no built-in
clone method (E0010). Owned-element replacement and mutation remain unsupported
except for writable inner Vec[int] places of Vec[Vec[int]]. Taking from an
owned indexed element, explicit general references and broader borrowed-element
returns remain separate boundaries.

Out-of-range index operations raise P-INDEX. `reserve` requests a minimum
total capacity without changing length; negative or overflowing capacities
raise P-CAPACITY. Copy reads materialize a value before later operations,
so `v.push(v[0])` does not retain an element borrow across the push.

`Vec[T]{}` supplies the element type explicitly. Empty `[]` requires type
context; a nonempty literal uses context or its first element. Enum cases
receive vector-element context in literals and push arguments. A vector field
may default to `[]` or `([])`; nonempty literals and typed constructors are not
constant field defaults. Literal storage is allocated before evaluating its
elements, and indexed assignment evaluates the RHS before the target, matching
stage0's evaluation order. Receiver/argument checks preserve root ownership
during calls and assignments.

Four accepted stage0 boundaries remain explicitly U0001 here:

| Manifest case | Unresolved stage0 behavior |
| --- | --- |
| `unsupported_loop_condition_move` | A repeated while condition consumes an outer owner; a later evaluation can read the cleared owner. |
| `unsupported_comparison_borrow_move` | The right comparison operand consumes the owner borrowed by the left operand. |
| `unsupported_vec_index_receiver_move` | Evaluating an index consumes its vector; bounds checking can then observe the moved-from descriptor. |
| `unsupported_vec_reserve_receiver_move` | Evaluating a reserve argument consumes its vector; reserve then operates on the moved-from descriptor. |

The manifest and summary retain all four minimal sources and reasons.
Stage0's observed results at these boundaries are not correctness oracles.

Three additional conservative U0001 fixtures protect an outer owner while
an indexed borrow is used in a comparison, a nested index is evaluated, or
a text index expression consumes its receiver. Stage0 accepts those inputs;
the fixtures do not execute these programs or add new known_gap records.

The principal remaining U0001 boundaries are:

| Area | Remaining boundary |
| --- | --- |
| Nested collections | Nested Vec shapes other than Vec[Vec[int]], including a third level; Vec[tuple], Vec[Result], unit elements, vector slices and general owned-element Vec pop |
| Aliases and places | Persistent local-Vec/temporary/Map-key/other-owned-foreach aliases; rebinding the supported read-only alias; owned indexed modification outside inner Vec[int] places; indexed inout/take and general explicit references |
| Tuples | Nested or borrowed/container/Optional elements, owned struct elements, tuple fields and Result combinations; destructuring and general tuple containers other than the admitted Heap |
| Owning Optionals | Payloads other than supported flat String tuples; nested Optional, fields, container elements and Result combinations; named aliases of an existing owning payload; while-let over an existing owning Optional |
| Heap and callbacks | Other element shapes, fields, Result combinations and unimplemented APIs; captures, calls or block/typed-parameter comparator bodies; ordinary standalone or escaping closures |
| Broader language | Other collection APIs, general user enum payloads and matching, imports after other items, Result fields/nesting, direct standard Error/Utf8Error Vec/Optional/tuple storage, qualified standard-error payload construction, arrow catch, io.readLine/fs.writeText, user methods, generics, mutable/reference iteration and concurrency |
| Text and numerics | Interpolation outside direct output/format arguments, format specifiers, float/nondecimal literals and integer separators |

Other invalid operations retain their stage0 error categories rather than
being classified as unsupported. String temporary receivers cannot be modified.
The frontend remains a subset of the Rust implementation.

## Implementation

| File | Responsibility |
| --- | --- |
| `digest.yv` | Pure Yavo SHA256 over the original UTF-8 bytes; independently frozen vectors |
| `support.yv` | Owned diagnostics, source identities, byte/scalar coordinates and text/JSON rendering |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination; returns `Vec[Token] ! Diagnostic`; precise lexical ends and JSON-only source snapshots on file EOF |
| `model.yv` | Copy AST nodes; child lists are ranges in shared Program pools |
| `parser.yv` | Aggregate/function/statement/expression grammar, imports, slices, borrows, catch, contextual payload construction and flat match arms, unsupported boundaries |
| `types.yv` | Resolved nominal/vector/Heap/Optional/flat tuple types within the stated shape limits, nominal standard errors and Utf8Error, bounded tuple signatures, Result pool, field/default checks, cycles/layout order and transitive ownership |
| `codegen.yv` | Places, borrowed/consuming values, Optional/Result wrapping and standard-call Results, error payload construction/display/match, lazy coalesce, propagation, inout/take/pop/sort, move-state joins, cleanup, loop replay and ordered C |
| `driver.yv` | Native build/run options, pinned backend resolution, source-identity protection, explicit argv, stream/exit forwarding and temporary-file cleanup |
| `runtime_source.yv` | Generated runtime-header bytes and pinned backend version for standalone native compilation |
| `main.yv` | Source loading and identities, JSON/text check dispatch, emit-c and ordinary command lifetime |
| `embed_runtime.py` | Deterministic generation and read-only freshness check of runtime_source.yv from the production header and toolchain lock |
| `lexer_test.yv` | Lexer boundary assertions, including owned error propagation and cleanup on normal test returns |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles, fixed point and reproducible execution |

Inside the compiler, parsing, checking and lowering still call `fail`, which
renders the first diagnostic in the selected text/JSON mode and exits 1. Lexing
returns an owned `Diagnostic` with code, message and byte start/end; `compile`
uses `try`, and the command entry catches and reports it. All frontend work
precedes creation of backend temporary files. Ordinary driver success and
handled errors return through `command`, dropping owned source/options/C
before `main` forwards a nonzero status. Lexer tests catch failures
and return normally so allocation checks can observe cleanup. Copy metadata
stacks retain capacity and pop only their discarded suffix. Loop exit states
remain in one flat `Vec[bool]` pool.

The [token metadata](examples/token_metadata.yv), [token table](examples/token_table.yv),
[owned token store](examples/owned_token_store.yv), [borrowed token scan](examples/borrowed_token_scan.yv)
and [lexer byte helpers](examples/lexer_byte_helpers.yv) examples remain
executable fixtures from earlier increments.

## Next steps

SH-4's three formal CLI applications were accepted in batch48. Native ordinary
build/run, first-error check JSON and standard error values now provide separate
SH-3 capabilities. The error test driver is not a new formal application.

Prioritize Copy tuple vectors next: the current first blocker affects 25 paths / 11 distinct entry-source byte sequences, split into 22/8 Copy tuples and 3/3 String-owning tuples. Freeze construction, push/pop, parameters/returns, fields, indexing, traversal and rejection rules before extending the subset. The 27/14 Map-value blockers mix Copy aggregates with owned String/Vec/Set values; they are not one Copy-only group. These counts do not promise equivalent coverage gains.

General owned Optionals remain a separate regression target: the unchanged
r5/yavo-b/T02 CSV program uses `?Vec[String]`, outside the supported flat
String-tuple Optional shape. That work includes owned construction/returns,
borrowed access and cleanup across branches and early exits.

The current corpus still has 269 unsupported stage0-accepted entries, or
162 distinct entry-source byte sequences. Broader semantics, richer diagnostics,
cross targets, cache/PCH, toolchain installation and remaining fix/verify/tool
entrypoints must satisfy their declared regression, resource and native-platform
gates before selfhost becomes the daily default. Performance P50 and measured
agent delivery cost remain independent acceptance requirements.

## Historical snapshots

### Historical byte and text-index snapshot (2026-11-23)

The [byte and text-index summary](../compiler/validation/selfhost-byte-2026-11-23.json)
binds source `aecdfe58e6f81b794c48827fa2e49905a59331e5` to baseline
`190f04a` and records **321/321 cases, 119 executable inputs and 10/10
auxiliary checks**, with no skipped checks:

| Case category | Passed |
| --- | ---: |
| Normal programs | 93 |
| Panic programs | 26 |
| Rejected programs | 167 |
| Inputs accepted by stage0 but outside this subset | 28 |
| Boundaries unsupported by both frontends | 7 |

All 119 executable inputs pass Debug and Release fixed output/exit/panic
oracles, separate Rust reference execution and repeated-C byte comparisons.
The summary rehashes all **476 candidate/reference mode binaries** and
binds final source, generated C, commands, toolchain and observed outputs.
Normal exits retain allocation checking. Supplementary development evidence
has separate identities: all 321 checks and 17 new/promoted programs across
both frontends and modes (68 executions), plus 51 independent contract
checks and four review programs across both frontends and modes (16 executions).

All **286 preceding fixture sources are retained byte-for-byte**:
285 manifest entries are unchanged, `unsupported_byte` is promoted to
`byte_literal_basic` with unchanged source, and 35 cases are new.
The original four `known_gap` records are unchanged. The new conservative
text-receiver U0001 case is recorded separately.

There are still **two complete original source files and three source-prefix
workloads**, plus **seven original function fragments**:

| Original input | Bytes / functions | Workload |
| --- | ---: | --- |
| `diagnostic.yv` | 438 bytes | Diagnostic execution |
| Current `model.yv` | 1,599 bytes | Owned AST storage and lifetime |
| Current `model.yv` | 1,599 bytes | Read-only AST content traversal |
| `lexer.yv` function fragments | 7 functions | Classification, UTF-8 leading-byte width and exponent lookahead |

The model grew from 1,581 to 1,599 bytes solely by appending `Byte` to
`BasicType` and `ExprKind`. Existing variants and all payload fields are
unchanged. Each model workload uses the complete current file bytes,
without source rewriting. The existing traversal oracle remains **18594**
for the Program and **20410** for six clones surviving original-owner
replacement; the full gate executes that same fixed oracle against the
current model.

The [lexer helper example](examples/lexer_byte_helpers.yv) extracts
`wordStart`, `digit`, `exponentAhead`, `scalarWidth`, `hexDigit`,
`twoSymbol` and `oneSymbol` byte-for-byte from the production lexer.
All seven also match their baseline function bytes. The receipt binds the
complete lexer hash, each original range/hash, concatenation order,
independent harness and assembled input. Its fixed oracle checks all
**256 byte values**, all **65,536 byte pairs** and **15 exponent contexts**;
the aggregate output is `53, 10, 22, 7066600, 20, 272301, 8` on separate
lines. These are seven accepted function fragments; the complete lexer
still does not belong to the input subset. The four existing data examples
and this helper example make **five published examples**, each
byte-identical to its executable fixture.

Five production frontend sources changed: lexer, parser, model, types and
codegen. `lexer_test.yv` was also updated to replace the old unsupported-byte
assertion while preserving all other old assertions. Its Debug and Release
runs cover ASCII/raw control bytes, seven escapes, positions and malformed
literals. Stage0, shared runtime, toolchain locks, language contract, frozen
specification, bootstrap harness and CI workflow are unchanged.

Complete local Rust suites were rerun: Debug and Release each pass
**145 tests, 0 failures and 1 existing ignored test**, with no filtered tests.
The frozen gate retains **124 passes, 3 unsupported and 2 superseded cases**,
with no wrong or unrun cases. The receipt remains `self_hosted: false`.

[CI run 45126097965](https://github.com/msly/yavo/actions/runs/45126097965) passed **six of six jobs** at
`aecdfe5`. The selfhost gate succeeded on Linux x86-64, Windows x86-64
and macOS arm64. The native Linux job passed **119/119 Yavo-generated C
programs and 3/3 capability probes** with independent GCC ASan/UBSan.
The native Linux archive matched GitHub's size/SHA-256; its actual source,
C, frontend/program binaries and outputs were independently verified,
including all 476 bootstrap mode execution records and the three
original-source workloads. The other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK.

Native Linux artifact **14795972640** contains
**318,163,047 bytes**, SHA-256
`2498ed6dd31fd6b26d68f799a8b584ff569015d6d912c7df6210af50da71ee77`. The verification reads archive bytes and
original receipts without re-executing the archived programs; stage0,
Zig and GCC retain receipt identity checks.

### Historical borrowed-index snapshot (2026-11-23)

The [borrowed-index summary](../compiler/validation/selfhost-borrow-index-2026-11-23.json)
binds source `b5dfe9f` to baseline `2604318` and records **286/286 cases,
102 executable inputs and 10/10 auxiliary checks**, with no skipped checks:

| Case category | Passed |
| --- | ---: |
| Normal programs | 84 |
| Panic programs | 18 |
| Rejected programs | 150 |
| Inputs accepted by stage0 but outside this subset | 27 |
| Boundaries unsupported by both frontends | 7 |

All 102 executable inputs pass Debug and Release fixed oracles, Rust
differential checks and repeated-C byte comparisons. The summary rehashes
all **408 candidate/reference mode binaries** and binds the final source,
generated C, commands, toolchain and observed outputs. Normal exits retain
allocation checking. The fresh complete bootstrap receipt supplies the
current-source result; development candidates and their historical draft
outcomes retain separate identities.

All 263 preceding fixture sources are retained byte-for-byte. There are
261 unchanged manifest entries, two unchanged-source promotions and 23 new
cases. The promotions are `unsupported_vec_string_element` to
`borrowed_index_string_storage` and `unsupported_owned_vec_index_copy_field`
to `borrowed_index_token_copy_field`.

There are **two original source files and three source-prefix workloads**:

| Original source | Original bytes | Appended workload |
| --- | ---: | --- |
| `diagnostic.yv` | 438 | Diagnostic execution |
| `model.yv` | 1,581 | Owned AST storage and lifetime |
| `model.yv` | 1,581 | Read-only AST content traversal |

The traversal workload appends 6,769 bytes of helpers/main to the unchanged
model, producing an 8,350-byte input. It reads four Expressions, three
Statements, two Functions and two TypeDecls, including all eight internal
Vec fields. Its complete Program content score is **18594**; six explicitly
cloned vectors retain score **20410** after replacing the original Program.
Those values were calculated independently before stage0 Debug/Release
execution, and the final candidate matches the same fixed output. The model
contains no String data field; the new
[borrowed token scan](examples/borrowed_token_scan.yv) separately exercises
String-bearing Token-shaped elements. All **four published data examples**
are byte-identical to their executable fixtures.

Only `selfhost/codegen.yv` changed among the Yavo frontend sources. The types
and model sources, stage0, shared runtime, locks, language contract and frozen
specification are unchanged. Complete local Rust suites were rerun: Debug
and Release each pass **145 tests, 0 failures and 1 existing ignored test**,
with no filtered tests. The frozen gate retains 124 passes, 3 unsupported
cases and 2 superseded cases, with no wrong or unrun cases.
The receipt records `self_hosted: false`.

[CI run 45121072503](https://github.com/msly/yavo/actions/runs/45121072503)
uses source `b5dfe9feb5c48bd22defd73b01f0b917dfbc2baf`.

**Six of six CI jobs passed.** The frontend gate succeeded on Linux x86-64,
Windows x86-64 and macOS arm64. The native Linux job passed
**102/102 generated-C programs and 3/3 capability probes** with independent
GCC 13.3 ASan/UBSan, with zero failed or unrun programs.

The native Linux archive (ID **14793164785**, **268,118,936 bytes**)
was downloaded and its size/SHA-256 matched GitHub. Its three original receipts,
102 actual source/C/program/output sets, all three original-source workloads,
and frontend/runtime bytes were independently checked. The 408 bootstrap mode
execution records also match the fixed oracles. Stage0, Zig and GCC retain
receipt identity checks; the other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK. TSan was not run.

### Historical owned Vec snapshot (2026-11-23)

The [owned-vector summary](../compiler/validation/selfhost-owned-vec-2026-11-23.json)
binds source `c7be682` to baseline `5f3c2f0` and records **263/263 cases,
90 executable inputs and 10/10 auxiliary checks**, with no skipped checks:

| Case category | Passed |
| --- | ---: |
| Normal programs | 74 |
| Panic programs | 16 |
| Rejected programs | 143 |
| Inputs accepted by stage0 but outside this subset | 23 |
| Boundaries unsupported by both frontends | 7 |

All 90 executable inputs pass Debug and Release fixed oracles, Rust
differential checks and repeated-C byte comparisons. The summary rehashes all
360 candidate/reference mode binaries and binds the final source, generated
C, toolchain, commands and observed results. Normal exits retain allocation
checking. The fresh full bootstrap receipt is the final evidence; development
checks and the two-case fixture refinement retain their separate identities.

All 242 preceding fixture sources are retained byte-for-byte. There are
241 unchanged manifest entries, one unchanged-source promotion from
`unsupported_vec_owned_struct_element` to `owned_vec_struct_declaration`,
and 21 new cases. The old `unsupported_vec_string_element` source remained
unsupported in that batch because it also reads a non-Copy element through an index.

Both original components have byte-preservation proofs: `diagnostic.yv` is
438 bytes and `model.yv` is 1,581 bytes. Only test helpers and an entry point
are appended. The model test populates all four `Program` vectors, including
their internal buffers, and exercises field push, whole-value moves,
returns, reassignment, recursive clone and cleanup. The three published
examples are byte-identical to executable fixtures; the new
[owned token store](examples/owned_token_store.yv) stores String-bearing
Token-shaped values without compiling the original lexer.

Only `selfhost/types.yv` and `selfhost/codegen.yv` changed among the Yavo
frontend sources. Stage0, the shared runtime, locks, language contract and
frozen specification are unchanged from the baseline. Complete local Rust
suites were rerun: Debug and Release each pass **145 tests, 0 failures and
1 existing ignored test**, with no filtered tests. The frozen gate retains
124 passes, 3 unsupported cases and 2 superseded cases, with no wrong or
unrun cases. The receipt still records `self_hosted: false`.

[CI run 45116399319](https://github.com/msly/yavo/actions/runs/45116399319) completed successfully:
**six of six jobs passed** at `c7be68216129dc9f1dad5aad668e2f1382142c1d`.
The Yavo-written frontend gate passed on Linux x86-64, Windows x86-64 and
macOS arm64. The native Linux job passed **90/90 generated-C programs and
3/3 capability probes** with independent GCC 13.3 ASan/UBSan; its failure and
not-run counts are zero.

The native Linux archive (ID **14792570849**, **243,500,405 bytes**)
was downloaded and its size/SHA-256 matched GitHub. Its three original receipts,
90 actual source/C/program/output sets, both original-component inputs, and
frontend/runtime bytes were independently checked. The 360 bootstrap mode
execution records also match their fixed oracles. Stage0, Zig and GCC retain
receipt identity checks; the other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK. TSan was not run.

### Historical Copy Vec snapshot (2026-11-23)

The [Copy-element Vec summary](../compiler/validation/selfhost-vec-2026-11-23.json)
binds source `7881db94` to baseline `ec1d3b7` and records **242/242 cases,
78 executable inputs and 10/10 auxiliary checks**, with no skipped checks:

| Case category | Passed |
| --- | ---: |
| Normal programs | 63 |
| Panic programs | 15 |
| Rejected programs | 136 |
| Inputs accepted by stage0 but outside this subset | 21 |
| Boundaries unsupported by both frontends | 7 |

All 78 executable inputs pass Debug and Release fixed oracles, Rust
differential checks and repeated-C byte comparisons. Normal exits use
allocation checking. The receipt binds the current sources, stage0, fresh
Yavo frontend, toolchain, generated C, executable bytes and observed results;
development candidates are not substituted for this complete run.

All 181 preceding fixture sources are retained byte-for-byte. Of their
manifest entries, 180 are unchanged and `unsupported_vector` is promoted to
`vector_basic` without a source change; 61 cases are new. The original 438-byte
`diagnostic.yv` prefix proof remains in the receipt. The new
[token-table example](examples/token_table.yv) is byte-identical to its fixture;
it exercises Copy token metadata and does not compile the original lexer.

That Copy Vec increment implemented the existing `Vec[T]{}` empty-constructor
contract in stage0, which previously produced E0001. The fix and its regression were confined to
`compiler/src/check.rs` and `compiler/tests/selfhost_blockers.rs`; the shared
runtime, locks, language contract and frozen specification are unchanged.
The summary retains the old constructor probe and the completed suite logs.
Empty vector field defaults still use `[]`; a `Vec[T]{}` field default remains
E0001. Complete local Rust suites pass **145 tests in Debug and 145 in Release**,
with no failures and one existing ignored test per mode. The frozen gate
retains 124 passes, 3 unsupported cases and 2 superseded cases.

[CI run 45103511821](https://github.com/msly/yavo/actions/runs/45103511821)
passed **six of six jobs** at `7881db94`. The selfhost gate succeeded on
Linux x86-64, Windows x86-64 and macOS arm64. The native Linux step passed
**78/78 Yavo-generated C programs and 3/3 GCC 13.3 ASan/UBSan capability probes**,
with zero failed or unrun programs. Its downloaded archive matches GitHub's
size/SHA-256. Actual source/C/frontend/program/output bytes, fixed oracles
and bootstrap-to-sanitizer identities were independently verified. The other
five archives were checked as metadata only. LSan is disabled; normal exits
use YV_ALLOC_CHECK. All source and verification identities are in the Vec summary.

### Historical String snapshot (2026-11-22)

The [String ownership summary](../compiler/validation/selfhost-string-2026-11-22.json)
records **181/181 cases and 10/10 auxiliary checks**, with no skipped checks:
45 normal programs, 10 panic programs, 105 rejected programs, 14 inputs accepted
by stage0 but outside this subset, and 7 boundaries unsupported by both
frontends. All 55 executable inputs pass Debug and Release fixed oracles,
Rust differential checks and repeated-C byte comparisons, with allocation
checking enabled.

All 138 preceding fixture sources are retained byte-for-byte. Two owned-value
declarations moved from unsupported to run without changing their source;
43 cases were added. The original 438 bytes of `diagnostic.yv` are a verified
prefix of a fresh test source with only a standalone main appended. The receipt
binds the original, suffix and assembled source separately; the component
executes correctly in both modes.

In that String increment, Rust language semantics, the C runtime, locked
toolchain and frozen specification were unchanged. The first CI run exposed
a concurrent PCH cache publication bug: rewriting identical runtime-header bytes changed their
mtime and invalidated another producer's PCH. The cache fix publishes the
header once, preserves its identity, and versions the PCH cache namespace.
Complete local Rust suites were rerun after this fix: Debug and Release each
passed 144 tests, with no failures and one existing ignored test. Rebuilding
stage0 and rerunning the selfhost gate again passed all 181 cases and 10 checks.
Three native CI jobs execute the new selfhost gate. A separate Linux step
recompiles the actual Yavo frontend's generated C with GCC ASan/UBSan; source
and capability checks are required.
[CI run 45044246299](https://github.com/msly/yavo/actions/runs/45044246299)
passed all six jobs at `81614a9`; the selfhost gate succeeded on all three
platforms. The new Linux sanitizer step passed 55/55 programs and all three
capability probes. Its native archive's size and SHA-256, actual source/C
identities, and program output bytes were independently checked. LSan is
disabled; normal exits retain allocation checks. Other artifact checks cover
metadata only. The String summary retains both CI attempts and all source
identities, including the first failed run and the cache fix.

The [Copy aggregate summary](../compiler/validation/selfhost-aggregates-2026-11-22.json)
retains its 138-case baseline and
[CI run 45025445954](https://github.com/msly/yavo/actions/runs/45025445954).
The earlier [SH-1 summary](../compiler/validation/selfhost-sh1-2026-11-22.json)
retains the 72-case baseline. These counts are tested behaviors, not a
self-hosting completion percentage.
