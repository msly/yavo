# Yavo compiler prototype

The prototype compiles a limited `.yv` subset to C17, then uses the pinned
`zig cc` toolchain (or the explicit `YAVO_CC` development override).
The implementation includes ownership, views, containers, generics, interfaces and
scoped OS threads. Native CI runs complete Linux x86-64 suites and selected
Windows x86-64/macOS arm64 suites. Complete phase acceptance still requires the
remaining tools, features and full target-platform acceptance coverage.

### Self-hosting work (updated 2026-11-27)

This Rust implementation is now **stage0** for the
[Yavo-written compiler](../selfhost/README.md). The declared bootstrap subset
compiles itself to a stage2/stage3 fixed point, originally confirmed on Linux,
Windows and macOS. Rust remains the daily compiler; SH-3 default replacement
still requires broader language coverage and structured diagnostics.

Batch 49 adds seven public system functions: `os.env`, `os.host`, `fs.tempDir`,
`fs.removeFile`, `fs.removeDir`, `fs.sameFile` and `process.run`. Their stage0
lowering and production runtime support the Yavo-written compiler's native
ordinary `build`/`run`, with an embedded runtime header, the installed locked
backend, source-file identity protection and temporary-file cleanup. This
selfhost driver has no Yavo build cache; cross-target compilation and toolchain
installation remain outside its current scope. The API contract is in
[the standard-library specification](../docs/stdlib_contract.md).

Current complete local Debug and Release suites each pass **151 tests, 0 failed,
1 ignored** (20 groups), including six new system integration tests. Frozen
conformance remains **124 pass / 3 unsupported / 2 superseded**. Local acceptance
and exact-commit CI status are recorded separately in the
[batch 49 validation index](validation/batch49-build-run-2026-11-27.json).

Batch 44 fixes MinMax operand snapshots: the old backend kept a local/field
expression until after evaluating the second argument, so an inout mutation there
changed the first operand's value. Both operands are now bound once in source
order before selection. `tests/p0/run_min_max_snapshot.yv` covers int/byte locals,
fields, inout parameters, indexed reads, nested calls and single evaluation.

Batch 43 fixes a stage0 diagnostic defect found while adding Copy tuples to the
selfhost: direct tuple printing reached an internal codegen panic. The checker
now reports E0001 for tuple output, including direct print calls, format and
interpolation paths. `tests/p0/errors_tuple_print.yv` covers eight diagnostics.
The recorded batch 43 local Debug and Release suites each passed 145 tests, with
zero failures and one existing ignored test; frozen conformance was 124 pass /
3 unsupported / 2 superseded.

Two concrete issues found while building it are addressed here:
`fs.writeNewText` exclusively creates a new file, preserving existing paths and
links; return-path analysis now treats a call as terminating only when it was
resolved to the actual builtin `panic`/`os.exit`. A user function with the same
name can no longer hide a missing return. The focused `fs_create_new` and
`selfhost_blockers` regressions are part of all three native CI selections.

Batch 29 added user-function Result/try to selfhost and migrated its lexer error
path. It also fixed `contains_break`: return-path analysis now visits
expressions, including catch handlers nested in initializers, arguments, return
values and loop headers. A break in a nested loop body or a closure still belongs
to that inner construct. The new P0 fixture requires 18 reachable missing-return
paths to report `E0013` and preserves six legal boundaries. The recorded batch 29
local Debug/Release suites each passed 145 tests, with one historical ignored test;
[source-bound evidence](validation/catch-break-return-2026-11-24.json) preserves
the old accepted inputs and generated C without executing invalid programs.

### Runtime header PCH cache (2026-11-22)

Normal `build`/`run` commands with the pinned Zig toolchain and an enabled build
cache now reuse a runtime **precompiled header** across different Yavo programs
and source edits. PCH identities cover the exact runtime header, toolchain lock,
compiler path, target, debug/release flags, allocation instrumentation and native
CPU fingerprint. The PCH bytes have a SHA-256 sidecar and are atomically published;
corruption is rebuilt. Publication and reuse work under concurrent builds. The
runtime cache has a separate LRU limit of 32 entries (overridable with
`YAVO_PCH_MAX_ENTRIES`).

Set `YAVO_NO_PCH=1` to use the original direct C compilation path. PCH build or
load failures fall back to the original path; `YAVO_CACHE_TRACE=1` reports cache
hits, misses and fallback reasons. `YAVO_NO_CACHE=1`, `YAVO_CC` overrides and
`yavo verify` bypass runtime PCH entirely. `verify` therefore keeps its isolated
compiler/sanitizer evidence semantics. The generated C and runtime ABI are
unchanged. Two independent seven-pair, four-program Linux pilots measured
edited-source Release P50 of **252.5–283.8 ms** (direct) versus **179.8–213.8 ms**
(PCH), with matching output SHA-256 in every pair. Three of four cases met the
200 ms target per run, but the outlier changed between runs. Details and script:
[`benchmarks/small-c-v1/`](../benchmarks/small-c-v1/README.md).

## Historical validation (2026-11-22)

The language-feature baseline at that point was `b3c27a6`. After the PCH-cache changes,
complete Debug and Release suites each passed **133 tests, 0 failed, 1 ignored**
(17 groups); the earlier language-feature baseline was 129/0/1. The ignored test
is the legacy exploratory P0 `conformance_report`; the current conformance suite
runs independently. Cargo functions and embedded specification fixtures are
different counting units.

The frozen 129-case specification package records **124 pass, 0 wrong,
3 unsupported, 0 not_run, 2 superseded**. T-GR9, T-P3 and T-R2 require an
ASan runtime unavailable in the pinned Zig on the project host. T-V1 and T-GF2
are superseded by the revised V2 two-phase receiver rule, with dated reasons in
`tests/conformance_superseded.txt`. The conformance test enforces a regression
baseline; read its complete report as well as the Cargo exit status.

The main P0–P3 language paths are implemented, but full v1 remains unaccepted.
Six target formats/architectures cross-compile. Overall `verify` remains
`not_verified` (exit 5), because required audit is unimplemented.

History and CI are published at [msly/yavo](https://github.com/msly/yavo).
**[CI run 45002067581](https://github.com/msly/yavo/actions/runs/45002067581) passed all six
jobs at `073003d` on 2026-11-22:**

| Platform/job | Debug | Release | Scope |
| --- | --- | --- | --- |
| Linux x86-64 | 129/0/1 | 129/0/1 | Complete Cargo suites, frozen conformance and six-target compilation; 12 separate starter-guide checks |
| Windows x86-64 | 71/0/1 | 71/0/1 | Selected native suites and compiler unit tests |
| macOS arm64 | 74/0/1 | 74/0/1 | Same selection, including Unix-only cases |
| Independent GCC 13.3 ASan+UBSan | 60 passed, 0 failed | — | All exported P0 run/panic fixtures; all capability probes passed |
| Round-five experiment evidence | Passed | — | 1,344 judgments, 96 source stages, 16 structural reviews, 43 frozen files and 28 task-packet files |
| Agent evaluation preparation | Passed | — | 33 executor tests, 25 lab/audit/freeze tests, 11 judge/summary scenarios, 94 prepared files and 112 independent oracle checks |

Native counts are passed/failed/ignored. Two Unix process unit tests and one
Unix symlink-module test are compiled out on Windows; there are no runtime
filters. Windows/macOS do not execute the Linux-specific frozen report scenarios.
Linux's Debug/Release conformance reports both satisfy the exact 124/0/3/0/2
contract. Independent GCC evidence does not reclassify pinned Zig ASan cases.

The `8a65228` verification driver uses validated artifact-relative paths on
Windows and records its working directory. The module snapshot regression
covers spaces and Unicode. Parser, type/borrow rules, codegen and runtime remain
at the `b3c27a6` language-feature baseline. See [CI scope](../tools/ci/README.md),
the [current source-bound receipt](validation/github-ci-agent-eval-2026-11-22.json),
[round-five CI receipt](validation/github-ci-round5-2026-11-22.json)
and [earlier run history](validation/github-ci-2026-11-22.json).

Both experiment jobs passed on the same commit as the four compiler/platform
jobs. Round five checks archived raw records, source identities and recomputed
summaries; round six checks preparation and launch refusal. Neither launches
participant sessions. Round-six live qualification remains blocked, with zero
provider requests, participant sessions and deliveries. Linux also compiled and
ran eight documentation/rule programs and checked four expected diagnostics.
These 12 checks are separate from the Cargo and specification totals. All six
downloaded artifacts match their GitHub-reported byte counts and SHA-256 digests.

### Earlier measurements and their source scope

The Arena/module validation at `0a69e75` on 2026-11-21 recorded 113 Cargo
passes, 95 compile/run fixtures, 58/58 independent GCC 14.2 ASan+UBSan programs
and matching generated-C/binary hashes for the two-file Arena/module project.
The ASan clean/allocation, deliberate use-after-free and signed-overflow probes
demonstrated capability; 426 referenced artifacts were rehashed. LeakSanitizer
was disabled in that ptrace-compatible profile; normal exits also required
balanced Yavo heap blocks. These are measurements of the recorded source
snapshot, not a new sanitizer or reproducibility run of `b3c27a6`.

The same four-case C pilot gave a runtime ratio geometric mean of 0.949 and
worst ratio of 1.069 with safety checks retained. Its build P50 was 242–249 ms;
the stable/fresh C-path probe was 36.292/225.525 ms. The later implemented-cache
batch (`a9fbefc`, 2026-11-21) measured unchanged-source Release builds at 2–3 ms,
edited-source misses at 214–247 ms and frontend checks at 3 ms. Thus edits still
exceed the 200 ms Release target. Neither pilot closes the formal performance gate.

Records: [Arena/module validation](validation/arena-modules-2026-11-21.json),
[independent sanitizers](validation/asan-arena-modules-2026-11-21.json),
[module reproducibility](validation/arena-modules-repro-2026-11-21.json),
[small-C method/results](../benchmarks/small-c-v1/README.md) and
[implemented-cache samples](../benchmarks/small-c-v1/results/build-cache.json).

### Agent efficiency

[Round five](../experiments/coldstart-2026-11-22-r5/README.md) ran four independent
empty-context sessions, two per language, each solving the same 12 new task
specifications. Both languages achieved **24/24 first compiler requests and
complete task passes**, including all public/hidden cases and the required
module/thread structure. All 48 first sources equal the final sources; there
were no repairs. The 96 first/final snapshot replays passed all 1,344 cases
(192 public, 1,152 hidden); these repetitions do not increase the independent
task count beyond 12 or the session count beyond four.

| Language | Session A operations / seconds | Session B operations / seconds | Combined operations / seconds |
| --- | ---: | ---: | ---: |
| Yavo | 69 / 337.8823 | 69 / 333.4053 | 138 / 671.2876 |
| Rust | 66 / 282.2978 | 66 / 274.7395 | 132 / 557.0373 |

The controlled-operation ratio is **1.0455** and elapsed-time ratio **1.2051**.
All six additional operations are Yavo cheatsheet reads: three per session for
the frozen 32,889-byte document. Other operation-category counts are equal.
Compiler subprocess totals are 5.3451 seconds for Yavo and 3.8910 for Rust;
their 1.4541-second difference cannot explain the 114.2503-second total gap.
These are recorded session timings, not a compiler-performance benchmark.

The experiment packet is frozen at `8c200aa`, using language/compiler baseline
`5cd916b` (last compiler code change `8a65228`). Compiler, runtime and frozen
specification sources are unchanged. Source graphs, first compiler requests,
operations and monotonic boundaries are archived in
[results](../experiments/coldstart-2026-11-22-r5/results.json),
[cost analysis](../experiments/coldstart-2026-11-22-r5/evidence/cost-analysis.json),
[behavior audit](../experiments/coldstart-2026-11-22-r5/evidence/behavior-review.json)
and [structure audit](../experiments/coldstart-2026-11-22-r5/evidence/manual-review.json).

Token usage, exact serving-model ID and sampling remain `null`; collaboration
does not expose verifiable usage and the read-only CLI initialization probe
failed. Three specifications extend earlier task families and nine use other
new specifications. Different tasks and the change from the historical Sonnet
setup prevent attributing the cross-round success change to compiler fixes.
The controlled lab-operation metric is not the old rounds' platform-wide tool
call metric.

Next: a metered executor, a full-versus-minimal-cheatsheet comparison under the
same model, then the planned 60-task pilot before the formal design of at least
400 tasks. No new failure in this packet calls for a semantic expansion.
Formal goals remain at least 20% lower cost per successful delivery and paired
success-rate noninferiority, as specified in [the plan](../docs/plan.md); the
token-dependent cost gate remains blocked.

## Language decisions after the third agent experiment (2026-11-21)

- **Index assignment** (C2, V5): `v[i] = x`, `v[i] op= x`, `v[i].f = x`, `grid[i][j] = x`. The
  checker lowers it to `{ let $value = x; let $slot = &v[i]; *$slot... = $value }`, so the new value
  is computed first (it may read `v`), then the element is bounds-checked (P-INDEX), the old value
  dropped and the new one written. A borrowed element (`Vec[str]`) adds its sources to the local
  container, as `push` does; through a reference or an `inout` parameter that stays U0001/E0306.
- **Two-phase receivers** (V2, revised): `push`, `set`/`remove`/`getMut`/`update`, Set
  `add`/`remove`, Deque pushes, Arena writes, `inout self` methods and the index of an element
  write reserve their receiver (`FnCx::call_reserved`) while the arguments run. Reads are allowed;
  modifying or moving the receiver is E0403 (`check_not_borrowed`), and so is an argument value
  that still points into it (`reserved_conflict`, e.g. `t.push(t)`). A receiver that is itself a
  call returning `&inout` keeps the old rule. The automatic `split-receiver-read` fix therefore has
  nothing to do any more; `hoist-argument` remains a candidate for the remaining E0403.
- **`String` vs `str` comparison** (K1): the owned side is viewed (`StrView`) for `==`, `<`, ...
- **Field defaults** (V6a): `field: T = constant` (numbers, booleans, bytes, plain string
  literals for `String`, `none`, `[]`, `[:]`), validated at the declaration and evaluated afresh in
  every literal that leaves the field out.
- **Stack overflow** (V14, P-STACK): the runtime installs an alternate signal stack per thread
  (Windows: a vectored handler with a stack guarantee) and reports a fault in the stack guard area
  as `yavo panic [P-STACK] at <file>:0:0: ...`, writing buffered stdout first. Box contents whose
  drop runs no user code (`Ty::may_run_user_drop`) are released from a per-thread worklist
  (`yv_box_release`), so dropping long lists or deep trees uses no stack depth. `yavo run` now
  names SIGXCPU and treats a raw SIGSEGV as a runtime bug.

Tests: `tests/p0/run_index_two_phase_defaults.yv`, `errors_two_phase_defaults.yv`,
`panic_index_write.yv`, `panic_stack.yv`, and `tests/build_cache.rs` (P-STACK keeps buffered
output; signal reporting via a CPU limit). Frozen conformance cases T-V1 and T-GF2 expect the old
V2 and are listed in `tests/conformance_superseded.txt`.

## Indexed modifying methods and diagnostics (2026-11-22)

`v[i].push(x)`, `ms[i].set(k, v)`, `ps[i].tags.push(s)`, user `inout self`
methods and nested indexed receivers are implemented. The element address and
exclusive borrow are obtained before the arguments, so an argument that uses
the same container is E0403 (`v[i].push(v.len())`). This V2a rule is distinct
from two-phase borrowing of a variable or field receiver.

Index assignment still evaluates the new value first. Its borrows remain locked
while the target address is evaluated; modifying or moving a borrowed source
there reports E0102. Target type discovery itself executes no side effects.
The six `tests/index_assignment.rs` cases cover these ordering and borrow rules;
`tests/p0/run_index_methods.yv` and `errors_index_methods.yv` cover methods.

Reading a non-Copy element still borrows it; E0102 now suggests `.clone()` where
an owned copy is appropriate. Literal `${` still requires `\${`; the lexer now
includes that escape in its diagnostic. See decisions 17–19 and the cheatsheet.

## Arena and typed IDs (2026-11-21)

`Arena[T]{}` creates a distinct process-wide identity without allocating heap
storage. `add(value)` moves a value into a slot and returns a copyable `Id[T]`.
`get` returns `?&T`, `getMut` returns `?&inout T`, and `getCopy` requires a
Copy element and returns `?T`. `remove` transfers an owned `?T` out of the
Arena; `has` and `len` query membership and the live count.

An Id contains Arena identity, slot and generation. Deleting an element makes
its old IDs return none; reusing the slot cannot revive those IDs. An ID from
another Arena causes P-ARENA-ID at every lookup/removal entry point. Atomic
identity allocation never wraps; identity, generation and capacity exhaustion
are P-CAPACITY in both build modes. IDs remain usable as pure values after their
Arena is dropped, but do not keep storage alive or provide pointers.

Ids support equality, ordering, sorting, contains, and Map/Set keys. Their
Copy/Send/Sync capabilities do not depend on the phantom element type. Arena is
not Copy or cloneable; its Send/Sync capabilities follow the stored element.
Arena has no Default, so default-based take is rejected. Moving an Arena preserves
its identity and ownership. Live stored elements and removed values are released
exactly once. Recursive graphs use fields such as `Vec[Id[Node]]`.

Box and Guard project to an Arena, including `Arc[Mutex[Arena[T]]]` across scoped
threads. This work also closes an existing Guard permission gap: a default
read-only Guard parameter cannot obtain mutation permission through methods,
fields, take, inout forwarding or mutable-reference returns. Local guards and
explicit inout parameters retain exclusive access.

Allocation analysis treats empty construction and lookups as intrinsically
no-allocation, insertion as possibly allocating, and includes element destruction
and removed-value cleanup. A remaining conservative case is a borrowed payload
removed through a function-returned `&inout Arena[str]`: the declared `of`
upper bound may keep the whole Arena borrowed. Direct local removal distinguishes
its external payload sources; loosening the returned-reference case requires a
more precise source representation.

Regressions: `tests/arena.rs`, `tests/arena_runtime.c`, `tests/p0/run_arena*.yv`,
and `tests/modules/arena/main.yv`. They include identity uniqueness across threads,
foreign/stale IDs, counter exhaustion, ownership, permission rejections and
Arena/module/thread combinations. The graph programs also build for all six targets.

## Source modules (2026-11-21)

The entry file's directory is the project root. A source file is one module:
`use maths` loads `maths.yv`; `use util/maths` loads `util/maths.yv` from that
root, including when the import is written in a nested file. `use util/{a, b}`
loads both files. The final path component is the module name used in expressions.
Modules are loaded once; cyclic imports are supported because this subset has no
module-level executable initialization.

Top-level functions and types need `pub` to be exported. Imported functions use
qualified calls such as `maths.double(7)`; public types are also available by
their short names. Ambiguous imported types require `a.Point` or `b.Point`.
Imports belong to the file containing them and are not implicitly re-exported;
a child module cannot see the entry file's private names. Each module has one
top-level declaration namespace. Standard-library imports are likewise scoped
to their own file.

For this implementation, exported types retain the existing accessible fields
and methods; separate private-field/private-method rules are not introduced.
Missing, private or unimported names are E0010; duplicate declarations/import
aliases are E0011; unused imports are W0003.

`maths.yv`:

```text
fn base(x: int) -> int { return x + x }
pub fn double(x: int) -> int { return base(x) }
```

`main.yv`:

```text
use maths
fn main() { println(maths.double(7)) }
```

`check`, `build`, `run`, `allocations` and `verify` load the complete reachable
source graph. Diagnostics, allocation operation byte ranges and panic locations
refer to original files. For module allocation queries, use
`yavo allocations util/maths.double --file main.yv --json`; a short module name
is accepted only when unambiguous. The entry module's functions keep their normal
query names.

Verification retains every source file, a source manifest and a project content
hash. Reproducibility rebuilds from that complete snapshot in independent front
ends and empty C caches. The required audit remains unimplemented. Multi-file
`fix` returns U0001 without writing: the existing repair planner can only recheck
one source file.

Local-path/Git dependency packages and a program lockfile are still unimplemented.
Imports cannot escape the entry directory; symlink module aliases are explicitly
unsupported until alias graphs can be preserved in verification snapshots.
The loader bounds a graph to 128 import levels, 1024 files and 16 MiB. Original
files are parsed independently before linking, so a comment, string or block
cannot be closed by a different file. Compiler module names beginning
`__yvm_` are reserved in modular projects.

Coverage is in `tests/modules.rs`; `tests/modules/basic/main.yv` is also built
for all six supported targets. Runtime execution remains limited to matching
available hosts.

## Collections and mutable-reference returns (2026-11-21)

`use std/collections` now provides `Heap[T].by(comparator)` with `push`, `pop`
and `len`, and `Deque[T]{}` with `pushBack`, `pushFront`, `popFront`, `popBack`,
`len` and `clone` when its elements are cloneable. Heap removal is logarithmic;
Deque uses a ring buffer with amortized constant-time insertion and constant-time
removal from either end. Popped values are owned `?T` results. Moving a collection
transfers its storage; stored elements are dropped exactly once. Borrow-carrying
elements retain their sources across insertion, moves and removal.

```text
use std/collections

fn main() {
    var priorities = Heap[int].by((a, b) => a < b)
    priorities.push(2)
    priorities.push(1)
    println(priorities.pop() ?? 0)
    var queue = Deque[int]{}
    queue.pushBack(7)
    println(queue.popFront() ?? 0)
}
```

Heap comparators are saved: copyable captures are copied, owning captures move
into an environment that is released with the heap. Any capture carrying a borrow
is rejected with E0307, including a `str` whose contents happen to be static.
Comparator invocation borrows the saved environment, so it cannot consume or
mutate its owning captures. Capture discovery follows lexical bindings; a local
declared inside the callback does not accidentally move an outer variable of the
same name. The existing `sortBy`/`update` callbacks retain their noescape contract.
General closure values, user higher-order parameters and closure fields are still
unimplemented. Heap cloning and cross-thread capability propagation for its erased
environment remain unsupported; Deque derives capabilities from its elements.

`&inout T` can be returned with an explicit `of` source, including a mutable
element borrowed from an `inout Vec[T]`. Local reborrows retain the original owner
and lock/guard provenance. Returning local storage, overlapping exclusive borrows,
or writing through `Rc`/`Arc` remains rejected. A call returning a mutable reference
can be a method receiver; the receiver executes exactly once before the arguments.
If an argument uses its exclusively borrowed owner, E0403 prevents the conflict.
For a complex receiver, `fix` offers a separately rechecked `hoist-argument`
candidate, preserving the decision to change evaluation order for explicit review.
Explicit `&inout` parameter/nested type syntax and storing borrowed values through
such references retain the documented implementation limits.

Explicit discard now consumes an existing owning local: `let _ = resource` drops
it immediately, and later use is E0402. `_kept` retains an owned value until scope
exit. Discarding a borrow descriptor does not release the referenced owner.

The conformance runner now handles native raw-byte arguments, allocation-failure
injection after runtime startup, allowed outcome sets, and separate sanitizer
artifacts. Injection lives only in a test translation-unit wrapper; production
builds expose no allocation-failure switch. Sanitizer cases require demonstrated
defect detection, actual artifact hashes and complete runtime output; an unavailable
ASan profile does not prevent checking the requested TSan profile for failures.

At the earlier collections/mutable-return batch, Debug and Release each passed
**88 tests**, with zero failures or compiler warnings and one legacy report ignored.
That batch had 91 compile/run fixtures; runnable
fixtures use both C optimization modes and heap-block checks. The mixed collection
and mutable-return program compiles for all six targets and executes on the Linux
x86-64 host. Frozen conformance is **126 / 0 / 3 / 0**. The final receipt independently
rechecks 125 source files and 11 retained artifact hashes:
[validation/completion-2026-11-21.json](validation/completion-2026-11-21.json).

## Allocation queries and verification (2026-11-21)

```sh
yavo allocations demo --file program.yv --json
yavo verify program.yv --json
yavo verify program.yv --target x86_64-windows-gnu --sanitize --json
yavo verify program.yv --reproducible --expect-stdout expected.txt --json
yavo check program.yv --emit-c program.c
```

Allocation reports use `yavo-allocations/1`. Operations separate argument evaluation
from the callee. Known allocation gives `may_alloc`; otherwise an opaque effect
gives `unknown`; only the remaining case is `no_alloc`. `has_unknown` preserves
opaque effects even when known allocation dominates the combined result.
The call graph includes callbacks, spawned functions and potential implicit Drop,
including cleanup of a partially constructed aggregate on early return.

Queries analyze checked functions and concrete generic instances; a generic
template name remains unknown. Startup initialization is outside a function query.
Output and synchronization operations use trusted runtime contracts; libc/OS
internals have not been allocation-audited. Branches and possible owned-value
cleanup are joined conservatively without a reachability or moved-value proof.

Verification reports use `yavo-verify/1` and persist in a fresh run directory under
`target/yavo-verify/`, or under the parent supplied to `--output-dir`.
They retain source/input snapshots, generated C, the runtime header, actual
artifacts and `report.json`. Source, compiler, dependency and toolchain fingerprints
bind the evidence; each check records its target, artifact hash/path and actual
execution host. The toolchain fingerprint currently covers the embedded lock and
installed Zig executable, with its scope recorded explicitly. Compiler libraries
and the installed distribution archive are not independently content-audited.

| Check | Prototype behavior |
| --- | --- |
| Front end | Lexing, parsing and implemented semantic checks; failure prevents compilation. |
| Toolchain | Reject any `YAVO_CC` override, probe the installed pinned Zig version, hash its executable and lock, and remove development compiler/loader flags. |
| Build | Build the requested release target using pinned Zig, `-O2 -s`; retain the actual artifact. |
| Tests | One exit-status smoke run on a matching host. `--stdin`, `--expect-stdout` and `--expect-stderr` add byte-exact input/output assertions. Cross targets without a runner and `--no-run` remain `not_run`. |
| Audit | Required and currently `not_run`. Application/dependencies/stdlib are `not_audited`; a validated pinned C runtime is `trusted_pinned`. |
| Sanitizers | When requested, independently build/run host-target ASan+UBSan and TSan artifacts. A clean capability probe and defect-detecting negative probes must pass before application results can pass. Accepting compiler flags alone is insufficient. |
| Reproducible build | When requested, execute two independent Yavo front ends against the same source snapshot, then two C builds with distinct empty local/global Zig caches; compare actual binary hashes. |

Default required checks are front end, toolchain, build, tests and audit. Requested
sanitizers and reproducibility become required too; `policy.required` lists their
exact check IDs. A required failure/timeout gives `overall: failed` and exit 1;
a required check lacking a pass gives `not_verified` and exit 5; all required
checks passing gives `verified` and exit 0. Because audit is not implemented,
the current prototype does not return complete verification even after its
implemented checks pass. Invalid inputs/options use exit 2, internal I/O failures 4.

Execution defaults to 5 seconds; each build defaults to 120 seconds. Override
with `--timeout-ms` and `--build-timeout-ms` (positive, at most one hour).
Both streams drain concurrently, retain at most 4 MiB each, and separately count
and hash the complete stream. Truncation fails a check and makes an exact-match
claim unavailable. Full-stream hashes are absent if EOF was not observed.
Unix timeout cleanup kills the ordinary process group and allows at most two
additional seconds; Windows uses a bounded taskkill fallback, without a Job
Object guarantee for orphaned descendants.

At the allocation/verification batch, Debug and Release each passed 71 test functions, with zero failures or
compiler warnings and one legacy report ignored. The current host demonstrates
TSan and UBSan defect detection; the requested ASan+UBSan profile remains
unsupported because ASan runtime symbols cannot link. Two independent source-to-
binary builds produced identical C and binary hashes. Full evidence is in
[validation/verify-2026-11-21.json](validation/verify-2026-11-21.json).

## Source repairs (2026-11-21)

```sh
yavo check program.yv --json
yavo fix program.yv --dry-run --json
yavo fix program.yv --json
```

`fix` applies only automatic repairs whose complete edited file passes a fresh
check. It is atomic as a group: unrelated errors or conflicting edits leave the
file unchanged. `--dry-run` returns the proposed automatic edits without writing;
its diagnostics and exit status still describe the actual, unchanged file.
`--json` and `--json-diagnostics` are aliases. Exit 0 means the actual file has no
compile errors, 1 means errors remain, 2 means an invalid invocation/input, and 4
means validation or writing failed.

| Repair | Behavior |
| --- | --- |
| Unused binding | Rename to `_name`, `_name2`, etc., including references to that binding; preserve destruction time. |
| `v.push(v.len())` | Split a standalone call on a known local built-in Vec into `let vLen = v.len()` and `v.push(vLen)` in the same block; avoid existing and later names. |
| Imports | Add an explicitly diagnosed standard module, or remove an unused import with no remaining syntactic references, including uninstantiated generic bodies. |
| Move from a field | Offer individually checked `take`, `clone` and `borrow` alternatives with their effects; `take` includes required write permission, and `borrow` includes the signature and affected calls. |
| Missing `of` | Offer independently checked source contracts; never change the signature automatically. |

Unknown patterns remain diagnostics. A user function that shadows `take` prevents
the built-in take candidate. An unused parameter named `static` can be renamed
without changing the special `of static` contract. Literal string text, comments
and field labels are preserved. The import rule follows W0003's deletion semantics;
the older cheatsheet row that groups imports with `_name` renaming is not applied
to imports.

`check` keeps the existing `yavo-diagnostics/p0` fields and adds `primary`,
`source_sha256` and structured `fixes`. The `yavo-fix/1` report contains original
and resulting diagnostics, source/result hashes, `automatic`, `applied` and
`candidates`. Every edit uses a half-open UTF-8 byte range (`start`, `end`) and a
`replacement`, relative to the original source hash. `id` names the repair
category; `patch_id` identifies its exact edit set. `recheck_scope` distinguishes
the complete `automatic-set` from an independently checked `single-candidate`.
Each candidate includes its title, impact and preconditions. Candidate edits are
reported for an explicit choice; this command has no automatic candidate-selection
mode. Clients must verify the source hash before applying returned edits.

Rechecks run in isolated threads so their type intern tables cannot affect the
original program's C generation. Writes preserve source permissions, check the
input snapshot again, and atomically rename a temporary file from the same
directory. Symbolic links, multiple hard links and read-only sources are not
replaced. This file handling has been executed on Linux.

The conformance runner checks original diagnostics, real lexical tokens against
the fixed fixtures, candidate application, and second-run idempotence. At the source-repair
batch, six frozen fix cases passed. Debug and Release suites each passed 50 test functions (zero failures
or compiler warnings, one legacy report ignored), including 13 fix integration
tests and four repair-engine unit tests. T-V3 was then blocked by mutable-reference returns and
complex mutable receivers, both addressed in the collections/return completion batch above. The earlier implementation
and validation receipt is [validation/fix-2026-11-21.json](validation/fix-2026-11-21.json).

## Safety review (2026-11-20)

- `spawn` checks the borrowed sources inside moved containers as well as borrowed
  arguments. Sources must outlive the scope and remain protected until it ends,
  even after `join`. Mutable borrows hidden inside generic tuples receive `U0001`,
  consistently with the existing restriction on `inout` thread arguments.
- Recursive thread-capability proofs distinguish `(type, Send)` and `(type, Sync)`.
  A struct wrapper cannot turn a Send-only interface into a Sync value.
- Built-in container cloning recursively rejects `Mutex`, `Guard` and other
  non-cloneable resources. Automatic fieldwise cloning of user `Drop` types is
  unsupported (`U0001`); explicit resource construction or per-element user methods
  remain available. Interface collections retain cloning when every registered
  concrete implementation is cloneable in this single-compilation-unit prototype.
  `Rc`/`Arc`/`Channel` clone their handles, independently of content cloneability.
- Runtime writes, flushes, panic and exit share a reentrant output lock; input
  operations serialize the entire read against the stdin buffer. The only nested
  lock order is input then output. Output source positions are thread-local, and
  interpolation operands still run before the outer print takes its lock.
- Fixture builds and execution now have separate deadlines; manifest `timeout_ms`
  is honored. Both pipes are drained concurrently, and Unix descendants are killed
  as a process group. Baseline updates cannot remove previously passing cases.
- The saved cold-start verifier uses checkout-relative sources, checks stdout,
  stderr and exit status, and exits nonzero if any build or case fails.

Six new `.yv` fixtures add 23 rejection checks and a positive combination program;
the fixture total is 87. Debug and Release Cargo suites each pass 32 test functions
(one legacy report ignored). The new runtime tests exercise concurrent I/O and
fatal paths under both C optimization modes. Native suites, 12 cross-builds and
40 separate ASan/UBSan or TSan checks are recorded in
[`validation/safety-2026-11-20.json`](validation/safety-2026-11-20.json).

Sanitizers ran with GCC 14.2 on a separate Linux analysis host, including T-P3;
at that batch the automatic conformance runner still marked sanitizer scenarios `not_run`.
LeakSanitizer cannot run under that host's ptrace environment, so ASan runs disable
LSan while native heap-block checks remain enabled. Only the two intentional
cross-thread panic/exit probes disable TSan's thread-leak report (V11 terminates
without joining); normal programs keep it enabled. Windows/macOS/aarch64 outputs
were compiled, not executed. These checks do not establish near-C performance.

## Build and test

With Rust and its host linker configured:

```sh
cd compiler
cargo build --locked --release
./target/release/yavo toolchain install
cargo test --locked --offline -- --nocapture
cargo test --locked --offline --release -- --nocapture
```

The ordinary suite runs every `tests/p0/*.yv` twice, using generated C at
`-O0` and `-O2`. Cargo's `--release` controls the Rust compiler binary;
it is separate from the generated program's optimization setting.

The ignored `conformance_report` is exploratory: rejected inputs are skipped
without distinguishing unsupported features from false rejections. Do not
use its pass count as full conformance or as an acceptance gate.
It is superseded by `tests/conformance.rs` (below), which makes that distinction.

## Diagnostic code classes

| Class | Meaning |
| --- | --- |
| `E0xxx` | An error about the program, as listed in `yavo-tests/ERRORS.md` (the front-end codes `E0010`â`E0018` and `E0020` were added there on 2026-11-16). |
| `U0001` | Valid v0.5 syntax that this compiler does not implement yet. It says nothing about whether the program is correct. |
| `W0001` | Warning: unused variable. |
| `W0002` | Warning: a `match` arm that can never run (added to `ERRORS.md` on 2026-11-18). |
| `W0003` | Warning: an unused import (added to `ERRORS.md` on 2026-11-19). |

Valid v0.5 syntax must not be misclassified as a syntax error merely because its
semantics are unimplemented. `tests/p0/errors_unsupported.yv` retains regressions
from earlier stages; implemented constructs now undergo ordinary semantic checks.
`U0001` remains for explicitly unsupported forms, including general closure values
and nested mutable-reference type syntax. The conformance runner treats an
unexpected `E0020` on a spec case as wrong.

## Historical implementation notes

These feature notes preserve the order of implementation, including the original
fixture counts and then-open items. Current coverage and remaining work are in
the sections above and [HANDOFF.md](../HANDOFF.md); the conformance and target
sections following these notes are maintained separately.

### Structs (first P1 feature)

Supported: `struct` declarations whose fields are integers, `bool` or other structs; struct
literals `Name{a: 1, b: 2}` (every field given exactly once, values evaluated in the order
written); reading fields, including nested ones; assigning and compound-assigning fields of a
`var` (`s.to.y += 1`); structs as parameters and return values. Such structs are copied on
assignment and when passed, as v0.5 specifies for structs whose fields are all copyable.

Checked: a `str` field is rejected with `E0301` (a view cannot be stored in a field); a struct
that contains itself by value is `E0017`; missing literal fields are `E0018`; unknown and
duplicate fields are `E0010`/`E0011`; structs cannot be compared (`E0501`) or printed directly.
`tests/p0/run_structs.yv` and `tests/p0/errors_structs.yv` cover these.

Owning fields (plan item 7): fields may be `String`, `Vec[T]`, `Map[K, V]`, `?String`, enums or
structs that own data. Such a struct is not copyable: it moves like a `String`, and is dropped
field by field in reverse declaration order (V10). Literal values are moved in. A field is read
in place (`p.name.len()`, `p.tags[0].trim()`, `for t in p.tags` iterates through a hidden
reference and keeps `p` unchanged for the loop); modifying methods work on field chains
(`p.tags.push(x)`, `p.scores.set(k, v)`, `p.items.pop()`); assigning a field drops the old
value; `take`/`replace`/`swap` move values out. Moving a non-copyable field out by value
(`let s = p.name`, `return p.name`, `p.nick ?? x`) is `E0401` (rule 8).

Not yet: methods, `Drop`, generic structs, `struct X: Interface`, field defaults, `.clone()` on
a struct. A struct literal must be written as an uppercase name immediately followed by `{`
(no space), which keeps it apart from `if c {`. Optional fields default to `none` when the
literal leaves them out. Tests: `tests/p0/run_fields.yv`.

Effect on the spec cases: none changed status. Every case that uses `struct` is also blocked
by generic types, imports or `String`; those are the next features to implement.

### Ownership: `String` and `Vec[T]`

Supported: `String`; `Vec[T]` for integer, `bool`, `str`, `String`, `Vec` and struct elements;
vector literals `[a, b]` (`[]` needs a type annotation); `s.len()`, `s.clone()`, `s.push(text)`,
`"..".toString()`, `v.len()`, `v.clone()`, `v.push(x)`; `v[i]` for copyable elements and `s[i]`
(a byte); printing and comparing `String` (`==`, `<`, ...); `x: sink T` parameters.

Rules checked:

- Assigning, returning, pushing, or passing to a `sink` parameter moves a `String`/`Vec`; using
  it afterwards is `E0402`. The analysis follows `if`/`else` (a branch that returns, breaks or
  continues does not count), and re-checks loop bodies so a value moved in one iteration cannot
  be used by the next. Assigning a new value makes the variable usable again.
- A non-`sink` parameter is a read-only borrow: moving out of it is `E0401`.
- Arguments are handled left to right: `consume(v, v.len())` is `E0402`; borrowing a variable
  and moving it in the same call is `E0102`; `v.push(v.len())` is `E0403` (no two-phase borrows).

Code generation: moving a variable copies it and zeroes the source; an empty or moved-from value
is all zeros, so drops never need runtime flags. Owned variables are dropped in reverse order at
the end of their scope and before `return`, `break` and `continue`; owned temporaries that are
only borrowed (e.g. `f().len()`) are dropped at the end of the statement. Heap growth checks
capacity arithmetic (`P-CAPACITY`) and allocation failure (`P-OOM`); `v[i]` checks bounds
(`P-INDEX`).

Not yet: `String`/`Vec` struct fields, `v[i] = x`, `Map` (see the references section for
`for x in v`, `names[0]` and `inout`).

Leak and double-drop check: `YAVO_ALLOC_CHECK=1` builds with `-DYV_ALLOC_CHECK`, where the
runtime counts live heap blocks and exits with 102 on a leak or an extra free.
`tests/alloc_check.rs` runs every `run_*.yv` that way in debug and release. A mutation test
confirmed it: deleting one generated drop was reported as a leak, and deleting one move's
zeroing made the program fail (the C library's own double-free check fired first). It does not
detect use-after-free. AddressSanitizer is not available here: zig ships no ASan runtime, and
the machine has no gcc, clang or valgrind; UBSan does work.

### Views: `str` borrowed from a `String`

Supported: passing a `String` to a `str` parameter borrows it for the call (the whitelist's
only automatic borrow); `s.trim()`, `s.fields()`, `s.lines()` return views into `s` (no
copying); functions return views with `-> str of a`, `of (a, b)` or `of static`.

Rules checked (semantic table O1, O2, E4, A3):

- A function returning a borrow-carrying type (`str`, `Vec[str]`) without `of` is `E0302`;
  returning a view of something `of` does not list (another parameter, a local, a temporary)
  is `E0303`; `of` naming a `sink` parameter is `E0304`.
- Each variable holding a view records which variables it points into. Storing a view into a
  variable that outlives its source, or a view of a temporary, is `E0305`.
- Moving or modifying a variable (`push`, assignment) while a view into it is still used later
  is `E0102`. "Later" means the view's last use (a borrow ends there); a use inside a loop
  that began after the view was created counts until the end of the loop, so a view printed
  before a `push` in the loop body is still live for the next iteration.

Implementation: before checking a function, a pre-pass numbers its statements in checking order
and records where each name is used and which loops enclose that use. A call's result points
into the arguments named by the callee's `of`. Tests: `tests/p0/run_views.yv`,
`tests/p0/errors_views.yv` (10 diagnostics, including the loop case and a cloned `Vec[str]`).
Three older P0 fixtures that returned `str` without `of` now say `of static` / `of name`.

### References, iteration and `inout`

Supported:

- `for x in v` over a `Vec[T]`: copyable elements are copied into `x`; other elements are
  borrowed as `x: &T`. Iterating over a temporary (`for x in v.clone()`) keeps it in a hidden
  variable until the loop ends.
- `v[i]` on a non-copyable element is a `&T` into `v`. A `&T` is read through automatically when
  printed, compared, used as a method receiver, indexed, or passed to a read-only parameter;
  functions may return one (`-> &String of names`).
- `x: inout T` parameters, called with `f(&v)`: inside, `x` is the caller's variable (it can be
  assigned and pushed to); in C it is a pointer.

Rules checked: changing or moving `v` while a `for` loop iterates over it is `E0102`; passing a
variable as `inout` together with any other use in the same call (`twice(&v, &v)`) is `E0102`;
an `inout` argument must be a `var` written as `&x` (`E0014`, `E0001`); storing a borrow into an
`inout` parameter is `E0306`; taking a `String` out of a `&String` is `E0404` (use `.clone()`);
moving out of a reference is `E0401`; iterating views of a temporary is `E0305`. A `&T` into
`v` is a borrow of `v`, so `v.push` while it is still used is `E0102`.

`*p`, the type `&inout T` and `&x` building one (A6, A7) are in "Dereference and `&inout T`
variables" below. `for x in &v`, `for i, x in v` and `getMut` are covered in the Map section.
Tests: `tests/p0/run_refs.yv`, `tests/p0/errors_refs.yv` (9 diagnostics).

### Optional values

Supported (semantic table C5, C7, V9; stdlib contract Â§1):

- `?T` for any supported `T`, including `??T`, `?Vec[String]`, `Vec[?String]` and `?str`
  (which carries a borrow and needs `of` when returned); `none`; `some(x)`; a `T` used where a
  `?T` is expected is wrapped automatically (`let a: ?int = 7`, `return i`, `describe(9)`).
- `if let x = opt { } else { }`, `while let x = opt { }` (lowered to
  `loop { if let ... else { break } }`), and `match` statements on optionals with `some(x)`,
  `some(_)`, `none` and `_` arms; an arm body is a block or a single statement.
- `a ?? b`: `b` is evaluated only when `a` is `none`. The right side may be `return [value]`,
  `break` or `continue` when the `??` is a whole `let` initializer or assigned value.
- `take(&p)`, `replace(&p, v)`, `swap(&a, &b)` on variables and their fields; `v.pop()`;
  `opt.isSome()`, `opt.isNone()`. A program's own function named `take` (etc.) wins over the
  prelude one.

Ownership: matching an existing place (a variable, its fields, an element read through `&`)
borrows it. A non-copyable payload is bound as `&T` (so moving it is `E0401`, as in T-G06); a
copyable payload is copied but still counts as a borrow of the place, so changing the place
while the binding is used later is `E0102`. Matching an owned temporary (`v.pop()`,
`take(&opt)`, a call) gives its payload to the binding. `a ?? b` consumes `a` as a whole, like
an assignment would (moving out of a parameter is `E0401`). `some(_)` on a temporary drops
the payload before the arm runs.

Rules checked: a `match` missing `none` or `some` (and without `_`) is `E0602`; an arm after
its case is covered is warning `W0002` (its body is still checked); `take` of a type without a
default (structs, `str`) is `E0405`; `replace(&v, v.clone())` is `E0102` (the place is borrowed
by the first argument); overlapping `swap` places are `E0102`; `take`/`replace`/`swap` need a
writable place (`E0014`); writing through a binding into a read-only place is `E0101`; an
untyped `none` and `??` on a non-optional are `E0001`; storing a borrow into an `inout`
optional through `replace` is `E0306` (T-G24); `?? break` outside a loop is `E0015`.
Assigning to a struct field now also checks for live borrows of the variable.

Not yet (`U0001`): writing through a binding into a writable place (valid v0.5, e.g. T-A3's
`n.size += 1`), `?? return` nested inside another expression, printing or comparing
optionals. (`match` with nested patterns, as an expression, and on enums and results is
described under "Enums, results and `match`".)

Code generation: `?T` is `struct { bool has; T val; }`; all-zero is `none`, so moved-from and
`take`n optionals need no flag. Owned optionals get drop and clone helpers, declared before
any is defined because `Vec[?String]` and `?Vec[String]` call each other's. `if let` on a
temporary holds it in a C variable, moves the payload into the binding and never drops the
holder (it is `none` on the other branch). `??` evaluates its right side in a nested C block,
whose temporaries are dropped there; a jump on the right drops the statement's temporaries and
the scopes it leaves, while the normal path still drops them at the end of the statement.
Tests: `tests/p0/run_optionals.yv` (28 lines of output, also run under the heap-block check;
once also built with UBSan, which was clean) and `tests/p0/errors_optionals.yv`
(18 errors, 1 warning).

### Imports

`use std/m` and `use std/{a, b}` import the modules `io`, `fs`, `os`, `math`, `time`, `rand`,
`collections` and `sync` (stdlib contract Â§1). An unknown module is `E0010`, a module imported
twice `E0011`, and an import nothing uses is warning `W0003`. Using `m.f(...)` or `m.name`
without importing `m` is `E0010`. User source modules are supported as described above.

`os.exit(code)` flushes output and ends the
process without running drops (V11), and counts as never returning, so a path that ends in it
needs no `return` (V8). Every other module function, constant and type (`fs.listDir`,
`math.pi`, `Mutex`, `Heap`, ...) that is not listed under "Standard library" below is `U0001`;
using one still marks the import as used. Tests: `tests/p0/run_imports.yv`; the import diagnostics are in
`tests/p0/errors_maps.yv`.

### Maps and `&inout` references

Supported (stdlib contract Â§3; semantic table A1, A2, A3, A5, C4, C6):

- `Map[K, V]` with integer, `bool`, `str`, `String` or `Id[T]` keys (other keys are
  `E0501`) and any value type; `[:]` and `[k: v, ...]` literals (`[:]` needs a type from context).
- `m.get(k)` (`?&V`, borrowing the map), `m.getCopy(k)` (`?V`, `V` must be copyable, else
  `E0404`), `m.has(k)`, `m.len()`, `m.clone()`; on a writable map `m.set(k, v)` (moves key and
  value; an existing key keeps its entry and position, the new key and the old value are
  dropped), `m.remove(k)` (`?V`, owned) and `m.getMut(k)` (`?&inout V`). `String` and `str`
  keys are looked up with a `str`, so `m.get("a")` creates no `String` (C6); inserting needs an
  owned key, `m.set("a", 1)` on a `Map[String, int]` is `E0001`.
- `for k, v in m` in insertion order; `for k, v in &m` gives `v: &inout V`. Keys are read-only
  in both (`E0103`). Removing a key and inserting it again puts it at the end.
- `for i, x in v` and `for x in &v` on vectors.
- `&inout T` values, as produced by `getMut` and `in &c`: not copyable (`let q = p` moves, using
  `p` afterwards is `E0402`), writable through whatever the binding was declared with
  (`c.n = 2`, `p.push(1)`), read automatically where a copyable `T` is expected (C4).

Rules checked: writing through a `&T` (e.g. a binding from `get`) is `E0101`; while an
`&inout` into a variable is used later, any use of that variable is `E0102` (exclusive
access), and inside `for ... in &m` the body cannot use `m` at all; modifying a map while a
`get` result is still used is `E0102`; `m.set(k, m.len())` is `E0403`; storing a borrowed key
or value into an `inout` map is `E0306`; `get` on a temporary map whose result is kept is
`E0305`. Storing `&inout` values in collections, and storing borrowed values through an
`&inout`, are `U0001`.

Code generation: the runtime keeps entries in a vector in insertion order and an
open-addressing table of entry indexes (kept at most half full, so probing ends); a removed
entry stays, not live, until the next rebuild compacts the vector. Hashing is splitmix64 for
integers and FNV-1a over the UTF-8 bytes for `str` and `String` keys, so both kinds of lookup
key agree. Id keys incorporate Arena identity, slot and generation in hashing and
equality. Each map type gets generated find/insert/remove/drop/clone functions. `&inout T` is
a `T *` and owns nothing, so it is never dropped (types now distinguish "not copyable" from
"needs a drop"). Tests: `tests/p0/run_maps.yv` (21 lines, including 100 inserts and 50 removals
that force rebuilds; heap-block check in debug and release, and UBSan once, all clean) and
`tests/p0/errors_maps.yv` (22 errors, 1 warning, together with the import checks).

### Enums, results and `match`

Supported (plan item 4; cheat sheet Â§2âÂ§3; semantic table V3, V8, V12, C7):

- `enum Name { Case, Case(field: T, ...) }`, built as `Name.Case`, `Name.Case(field: value)` or
  positionally `Name.Case(value)`; where the type is known, `.Case(...)` (also inside `?T`
  and as the success type of `T ! E`). Payload values move in. Enums with owning payloads move
  and drop only the current case's payload; enums of copyable payloads are copied.
  `println("${e}")` prints `Case` or `Case(field: value, ...)`.
- `T ! E` results: `return err(e)`; a `T` returned from such a function is wrapped as success
  (section 8 whitelist); `ok(v)` where a result is expected. `() ! E` functions succeed by
  falling off the end or with `return` / `return ()`.
- `try f()` in a function with the same error type returns the error at once; values of the
  statement built so far (struct fields, vector elements, sink arguments) and every scope are
  dropped first (V3, V6).
- `f() catch e => value` and `f() catch e { ... }`; the block form must not reach its closing
  `}` (`E0601`, checked by control flow). `catch _` drops the error before the handler runs,
  `catch _e` keeps it until the handler ends (V12). On a `() ! E` result the handler may be a
  call (`check(xs) catch e => println("${e}")`). `catch e => return/break/continue` works as a
  whole `let` initializer, assigned value or statement.
- `match` with nested patterns: names, `_`, `some(p)`, `none`, `ok(p)`, `err(p)`, `.Case(p, ...)`
  and `Type.Case(...)` (module enums too: `fs.Error.Utf8(n)`); as a statement or as an
  expression (`let n = match r { ok(v) => v, err(_) => 0 }`); expression arms may also be
  `return`/`break`/`continue` or a block that leaves. Matching an existing place binds `&T` to
  non-copyable parts and borrows the place; matching an owned temporary moves the bound parts
  into the bindings and drops the rest before the arm runs. Exhaustiveness and unreachable arms
  use the usual usefulness check over these patterns (`E0602`, `W0002`).
- `panic(msg)` and `assert(cond)` / `assert(cond, msg)`, with interpolation; categories
  `P-PANIC` and `P-ASSERT` (recorded in `yavo-tests/ERRORS.md`); both count as never returning.
- Byte literals `'a'`, `'\n'` (type `byte`).
- `fs.Error` and `io.Error` are known enum types (`Utf8(offset)`, `Io(code)`, and
  `InvalidPath` for `fs`), usable once the module is imported.

Rules checked: a result used as its success type, or as a statement, without handling is
`E0001`; `try` outside a `T ! E` function or with another error type is `E0001`; an unknown
case is `E0010`; wrong payload counts are `E0012`/`E0018`; `.Case` or `err(...)` without a
known type is `E0001`; `Enum{...}` is `E0001`; enum payloads and error types cannot hold views
(`E0301`). Not yet: literal patterns (`.Num(0)`, `1 =>`), named fields in case patterns, `if
let` with patterns, comparing enums, generic enums, methods and interfaces on enums, result
fields in structs.

Code generation: an enum is `struct { uint32_t tag; union { struct {...} v0; ... } u; }` and
a result `struct { bool ok; T val; E err; }` (no `val` for `()`); all-zero is a valid value
in both (first case, or an error holding zeros), so moved-out slots stay safe to drop. Struct,
enum and result types needing drops get drop/clone helpers; every enum gets a printer. A
`match` is an `if`/`else if` chain over tag tests on the scrutinee; a block inside an
expression (a `catch` handler or `match` arm) keeps the enclosing statement's temporaries
listed, so a jump out of it drops them too. Tests: `tests/p0/run_enums.yv`,
`tests/p0/run_results.yv` (both also under the heap-block check, which exercises the early
exits) and `tests/p0/errors_enums.yv` (16 errors, 1 warning).

### Standard library: text, sequences, I/O and arguments

Supported (cheat sheet Â§5; stdlib contract Â§2, Â§4, Â§5; semantic table S5, M5):

- Prelude: `format("...")` (a new `String`, same interpolation as `println`), `min`, `max` and
  `abs` on integers (`abs(i64.min)` panics `P-OVERFLOW`), `String{}`.
- Text (`str` and `String`): `contains`, `startsWith`, `endsWith`, `find` (`?int`, byte offset),
  `split(sep)` (`Vec[str]` viewing the receiver; empty pieces kept; an empty separator gives the
  whole text), `toLower`/`toUpper` (new `String`; ASCII letters only, other bytes unchanged),
  `parseInt()` (`?int`: optional sign and decimal digits only, `none` on overflow).
- Sequences: `[]T` slices as a type (parameters, `let`, returns with `of`): a `Vec[T]` is viewed
  as a `[]T` where one is expected (section 8 whitelist); `len()`, indexing and `for x in s`
  work on slices. `v.contains(x)` and `v.join(sep)` on vectors and slices (text elements are
  compared/joined as `str`); `v.sort()` on vectors of integers, `str` or `String` (stable merge
  sort; other element types are `E0501`).
- `io.readAll()` (`String ! io.Error`) and `io.readLine()` (`?String ! io.Error`, without the
  `\n` or `\r\n`); both share one stdin buffer and flush stdout before blocking. Invalid UTF-8
  is `.Utf8(offset)` (offset into the input, or into the line for `readLine`), a system error
  `.Io(code)` (errno / `GetLastError()`).
- `fs.readText(path)` and `fs.writeText(path, text)` (`String ! fs.Error`, `() ! fs.Error`); a
  path containing NUL is `.InvalidPath`; Windows paths go through UTF-16 (`CreateFileW`).
- `os.args()` (`[]str of static`), converted once before `main`; an argument that is not valid
  UTF-8 stops the program before `main` with `YAVO-STARTUP-UTF8` and status 2. On Windows the
  arguments come from `CommandLineToArgvW`.

Not yet: `sortBy`, `update`, `Heap`/`Deque`/`Set` (closures and generics first), `x[a..b]`
slices, `s.lines()` on slices of lines, `math`/`time`/`rand`, `String.fromUtf8`, format
specifiers (`${x:.2}`), Unicode case mapping, `Ordering`.

Code generation: printing and `format` share one output abstraction (`yv_out`: a file
descriptor or a `String` to append to), so enum printers serve both. Slices are `yv_vec`
values that own nothing. Tests: `tests/p0/run_stdlib.yv`, `tests/p0/errors_stdlib.yv`
(11 errors, 1 warning), and `tests/stdlib_io.rs`, which builds programs with the heap-block
check and runs them with stdin, arguments (including a non-UTF-8 one) and a scratch directory.

### Methods

Supported (cheat sheet Â§2; semantic table O1, O2, V1):

- Methods are written inside a struct or enum body. The receiver is the first parameter:
  `self` (a read-only borrow), `inout self` (needs a writable variable or a field chain rooted
  at one, like `push`; the receiver is exclusively borrowed for the call) or `sink self`
  (moves the receiver). `Self` names the enclosing type.
- A function in the body without `self` is called on the type: `Counter.new("x")`.
- `-> str of self` returns a view of the receiver; the caller treats it like any other `of`
  source, so writing to the receiver while the view is alive is `E0102`. `return self.name`
  in a function returning `str` views the stored `String` (a temporary `String` cannot be
  viewed this way).
- Errors: an unknown method on a struct or enum is `E0010`; calling a method on the type or a
  `self`-less function on a value is `E0001`; `inout self` on a temporary or a `let` is
  `E0014`; moving a field out of `sink self` is still `E0401` (no partial moves, rule 8).

Code generation: a method is an ordinary C function `yv_fn_Type__name` whose first parameter
is the receiver (by value for `self`/`sink self`, a pointer for `inout self`). Tests:
`tests/p0/run_methods.yv`, `tests/p0/errors_methods.yv` (11 errors).

### User destructors (`Drop`)

Supported (cheat sheet Â§5 "ææ"; semantic table V4, V6, V10, V11):

- `struct T: Drop { fn drop(inout self) { ... } }` (also on enums). The compiler calls `drop`
  when a complete value is dropped: at the end of its scope, on overwrite (after the new value
  is computed, V5), for `let _ = x`, for temporaries, for elements of a dropped `Vec`/`Map`/
  optional, and when a `try` leaves early. The user's `drop` runs first, then the fields in
  reverse declaration order. Moved-out values and fields of a struct literal that failed half
  way are not given to the user's `drop` (only the finished fields are dropped).
- A `Drop` type is never copyable, even if all its fields are scalars.
- Errors: `x.drop()` is `E0801`; `T: Drop` without exactly `fn drop(inout self)` (missing,
  extra parameters, a return type such as `() ! E`) is `E0802`, and so is a method named
  `drop` on a type that does not declare `Drop`; inside `drop`, overwriting, moving, `take`,
  `replace` or `swap` of the whole `self` is `E0803` (fields may still be taken or replaced).

Code generation: a `Drop` type has a hidden `uint8_t yv_live` member that literals set to 1.
Moves zero the source as before, so a moved-out value has `yv_live == 0`; its drop helper
returns immediately, otherwise it clears the flag, calls `yv_fn_T__drop` and drops the
fields. Tests: `tests/p0/run_drop.yv` (scope order, sink, `let _`, vectors, optionals, an enum
with `Drop`, a field taken inside `drop`), `tests/p0/errors_drop.yv` (8 errors, 2 warnings);
spec cases T-D1, T-GR1, T-GR2 and T-G10âT-G13 pass.

### Tuples

Supported (cheat sheet Â§2; semantic table E2, V10, rule 8):

- Tuple types `(A, B, ...)` in signatures, fields, collections and optionals; literals
  `(a, b)`; elements `t.0`, `t.1` (also nested: `t.0.1`); `let (a, b) = t` and
  `var (a, b) = t`, with `_` dropping that element at once.
- A tuple is copyable when all elements are, and needs dropping when any element does
  (elements are dropped in reverse order). Moving a non-copyable element out of a tuple
  variable is `E0401` (no partial moves); destructure with `let (a, b) = t` instead, which
  consumes the whole tuple.
- A tuple holding a view (`(str, int)`) carries a borrow: it may be a local, a parameter or
  a return value with `of`, and its elements keep the tuple's sources.
- Not yet: assigning to an element (`t.0 = x`), tuples as `Map` keys, comparing tuples with
  `==`/`<`, printing a whole tuple, tuple patterns in `match`/`for`.

Code generation: each tuple type is a C struct with members `f0`, `f1`, ... defined before
its first use, with drop and clone helpers when needed. The lexer never reads a number after
`.` as a float, so `t.0.1` is two element accesses. Tests: `tests/p0/run_tuples.yv`,
`tests/p0/errors_tuples.yv` (7 errors).

### Closures for `sortBy` and `update`

Supported (cheat sheet Â§5; semantic table O5, K3, V8; stdlib contract Â§3, Â§4):

- `v.sortBy((a, b) => a.1 > b.1)`: a stable sort; the closure says whether `a` goes before
  `b`. Any element type works. A comparison that breaks its contract (e.g. `a <= b`) gives some
  order but never touches memory outside the vector (K3).
- `m.update(key, initial, (v) => v + 1)`: the closure runs exactly once, on the stored value or
  on `initial`; the key is consumed (a duplicate key is dropped). v1 limits the value type to a
  copyable type without borrows (`E0001` otherwise).
- Closure syntax: `(a, b) => expr` or `(a, b) => { ... return x }` (a block body must return on
  every path, `E0013`); parameters may be typed (`(a: int, b: int) =>`). A block body inside the
  call parentheses keeps its newlines (the lexer now treats newlines directly inside `{` as
  statement ends even within `(...)`).
- Captures: these callbacks never keep the closure (O5 noescape), so it may read any outer
  variable, borrowed in place for the duration of the call. It cannot assign to them (`E0014`),
  move them (`E0401`), capture an `&inout` value (`U0001`), or use the collection being
  sorted/updated (`E0102`).
- `Heap.by` saves an owning comparator (see the current collections section above).
  General closure variables/fields and callbacks passed to user functions remain `U0001`.

Code generation: the checker lifts each closure into a function of its own (`closure.N`,
C `yv_fn_closure__N`) whose first parameters are the captured variables. `update` calls it
inline. `sortBy` goes through a generated adapter `yv_tramp_N(ctx, a, b)` that receives the
captured values as a `void *` array and is passed to the runtime's `yv_sort_ctx` (`yv_sort` is
now a wrapper around it). `total += n` with `n: &inout int` now reads `n` (C4), which T-C7
needed. Tests: `tests/p0/run_closures.yv`, `tests/p0/errors_closures.yv` (11 errors, 3
warnings).

### Comparison protocols (`Eq`, `Ord`, `Ordering`)

Supported (cheat sheet Â§2; semantic table K1, K2):

- `Ordering` (`.Less`, `.Equal`, `.Greater`) is in the prelude. Enums whose cases carry no
  payload, `Ordering` included, can be compared with `==`/`!=`; a case shorthand on the right
  (`o == .Less`) takes the left side's type.
- `struct T: Ord` with exactly `fn cmp(self, other: &T) -> Ordering`: `<`, `<=`, `>`, `>=`,
  `==`, `!=` call `cmp` (equality is `cmp == .Equal`), and `v.sort()` works on `Vec[T]`
  (stable, through the same adapter mechanism as `sortBy`).
- `struct T: Eq` with exactly `fn eq(self, other: &T) -> bool`: `==` and `!=` only.
- `&x` is accepted where a `&T` is expected, e.g. `a.cmp(&b)`: a read-only reference whose
  borrow follows `x`'s storage.
- Errors: a missing protocol method or a different signature is `E0502` (at the declaration
  or the method); ordering an `Eq`-only type, or comparing a struct/enum without a protocol,
  is `E0501`. User `Ord` does not make a type usable as a `Map` key (K2).

Code generation: a comparison is `yv_fn_T__cmp(a, &b).tag` tested against the `Ordering`
tag (Less 0, Equal 1, Greater 2). Tests: `tests/p0/run_protocols.yv`,
`tests/p0/errors_protocols.yv` (7 errors, 2 warnings); spec cases T-G25 and T-G26 pass.

### `Box[T]`

Supported (cheat sheet Â§4 rule 9; semantic table A4, A7, C5, C8, E1, E2; stdlib contract Â§1):

- `Box[T]` in signatures, fields, collections and optionals; `Box(value)` moves the value
  into one heap block. Recursive types go through `?Box[T]` (`struct Node { left: ?Box[Node] }`)
  or `Box[T]` in an enum case; a box does not count as by-value containment, so `E0017` no
  longer fires for them.
- Fields, methods (`self`, `inout self`, `sink self`) and indexing project through a box
  (`b.size += 1`, `b.grow()`, `b.len()`, `w.push(4)`, `w[3]`, `for x in b`), at any depth. A
  `T` where `?Box[T]` is expected is wrapped as before (`t = Box(Node{key: k})`).
- `if let n = t` on a writable place (`var`, `inout` parameter) whose payload is not copyable
  now binds `n` as an `&inout` to the payload (rule 3): `n.size += 1`, `insert(&n.left, k)` and
  `take(&n.left)` write through it, and `t` cannot be used while `n` is still used (`E0102`).
  A binding into a read-only place stays read-only (`E0101`).
- `inout` arguments may now be field paths (`&n.left`, `&p.items`), through `&inout` and
  `Box`; an `&inout` variable passes its target on (`&n`). Two arguments rooted at the same
  variable are `E0102` (T-A3f).
- Errors: a box holding a view, written (`Box[str]`, `Box[Vec[str]]`) or inferred
  (`Box(text.trim())`), is `E0301` (E1, E2); `take(&box)` is `E0405` (a box has no default);
  moving a non-copyable value out of a box's content is `E0401` (rule 8).
- Not yet: `.clone()` on a box (write `Box(b.clone())` for copyable/cloneable content),
  printing or comparing a box, `Box[I]` for interfaces.

Also fixed: assigning to an `inout` parameter counts as using it, so `W0001` no longer
fires for output-only parameters.

Code generation: a box is a `T *` (named by struct tag for struct-like contents, so
`?Box[Node]` can appear inside `Node`); NULL is a moved-out box. `yv_box_alloc`/`yv_box_free`
in the runtime count blocks for the heap check. Place paths use `BOX_STEP` for "into the
box" (`(*p)` in C). `&Box[T]` is `T * const *`. Tests: `tests/p0/run_box.yv`,
`tests/p0/errors_box.yv` (7 errors, 1 warning).

### Interfaces (`interface`, `struct T: I`, `Box[I]`)

Supported (cheat sheet Â§2; semantic table O1, P2; section 8 whitelist):

- `interface Named { fn name(self) -> String ... }`: method signatures with `self`,
  `inout self` or `sink self`, any parameters (`inout`/`sink` included) and return types; a
  returned view needs `of` as usual (`-> str of self`).
- `struct T: I` / `enum T: I` (combinable with `Drop`, `Eq`, `Ord` using `+`): `T` must have
  every method of `I` with the same receiver mode, parameter types and modes, and return type.
- `Box[I]` is an interface value: in variables, parameters, fields, `Vec[Box[I]]`, `?Box[I]`.
  A `Box[T]` is converted where a `Box[I]` is expected (`let s: Box[Shape] = Box(Rect{...})`,
  `all.push(Box(Dot.Small))`, field initializers, returns), which moves the box.
- `x.m(args)` on a `Box[I]` calls `T`'s method through the vtable; `inout self` works on
  any writable place (`h.item.scale(5)`, a writable `if let` binding), `sink self` consumes the
  interface value. Interface values can be cloned inside collections (`all.clone()`), dropped,
  moved and stored like other owned values.
- Errors: a missing or mismatched method is `E0019` (new, registered in `ERRORS.md`); an
  unknown interface name after `:` is `E0010`; an interface used as a type by itself (`n: Named`)
  or an interface method without `self` is `E0001`; a `Box[T]` whose `T` does not implement `I`
  is `E0001` with a note; a method the interface does not have is `E0010`.
- Not yet: capability bounds (`Box[Named + Send]`, P3), interfaces as generic constraints,
  interfaces extending interfaces, `Self` in interface signatures, default methods.

Code generation: `Box[I]` is `yv_dyn_I { void *data; const void *vt; }`. Each interface has a
vtable type `yv_vt_I` (drop, clone, then one slot per method taking the receiver as `void *`);
each `struct T: I` gets slot adapters `yv_vtm_K_J` that call `yv_fn_T__m` on the boxed value,
plus `yv_vtd_K`/`yv_vtc_K` (drop and free the block, clone into a new block) and one
`static const yv_vt_I yv_vt_K`. Tests: `tests/p0/run_interfaces.yv`,
`tests/p0/errors_interfaces.yv` (9 errors, 1 warning).

### Generics (functions and structs)

Supported (cheat sheet Â§2; semantic table O1, E2, K1; plan item 10):

- Generic functions `fn largest[T: Ord + Copy](xs: []T) -> ?T of xs`, called with explicit
  type arguments (`largest[int](v)`) or with them inferred from the arguments (`largest(v)`;
  inference reads variables, fields, literals, element accesses, tuples and simple calls, and
  sees through `Vec â []T`, `T â ?T` and `&T`).
- Generic structs `struct Stack[T] { items: Vec[T] ... }` with methods (`self`, `inout self`,
  `sink self`, `Self`), used as `Stack[String]` in types and `Stack[int]{items: []}` in
  literals; recursive ones work through `?Box` (`next: ?Box[Node[T]]`).
- Bounds `Copy`, `Eq`, `Ord` and interfaces, checked at each instantiation: a failing
  `Eq`/`Ord` bound is `E0501` (K1), any other `E0001`; an unknown bound is `E0010`.
- Monomorphized: each instantiation (`largest[int]`, `Stack[String]`) is checked and generated
  on its own, so `-> ?T of xs` only borrows `xs` when the instance's `T` carries a borrow
  (T-O6 vs T-O7, rule O1).
- A view as a generic struct's type argument (`Wrapper[str]`) is `E0301` (E2, T-E1).
- Errors: a type argument that cannot be inferred, conflicting inferences, the wrong number of
  type arguments, type arguments on a non-generic function, and a generic struct named without
  its arguments are all `E0001` with a note on what to write.
- Not yet: generic enums and interfaces, methods with their own type parameters, `struct S[T]: I`,
  `Stack[int].make()` (static calls on an instance), struct literal argument inference
  (`Stack{...}` without `[T]` outside the struct's own methods), capability bounds (`Send`,
  `Sync`). A generic function's body is only checked when it is instantiated, so a generic
  function that is never called is never checked (like C++ templates).

Implementation: the checker keeps a substitution (`Checker::subst`) consulted by `resolve_ty`;
an instance gets a function index among the lifted functions (closures now also record a
signature, so signature numbers and function indexes stay equal; interface slot signatures
moved to `DYN_BASE` and up). Function bodies are checked from a work list after the user
functions; struct instances made after `collect_structs` are finished at once and emitted after
the ordered structs. C names of instances are made C-safe (`Stack[String]` â `yv_s_Stack_String_`).
Tests: `tests/p0/run_generics.yv`, `tests/p0/errors_generics.yv` (9 errors).

### Floating point (`f64`, `f32`)

Supported (cheat sheet Â§2, Â§6; semantic table Â§7, N2, K1; stdlib contract Â§4):

- Literals `1.5`, `2.0e-3`, `1e10` (an `f64` unless the context asks for `f32`); an integer
  literal where a float is expected is that float (`let x: f64 = 2`, `2 * x`).
- `+ - * /`, unary `-`, `+=` and friends, and `== != < <= > >=` with IEEE 754 semantics: no
  panics, `NaN != NaN`, `1.0 / 0.0` is `inf`. `%`, bit operators and wrapping operators are
  `E0001` on floats; mixing `int` and `f64` (or `f32` and `f64`) is `E0001`.
- Conversions: `f64(n)`, `f32(x)`, `f64(x32)`; `int(x)` and every integer type truncate toward
  zero and panic with `P-FLOAT-CONVERT` on NaN, infinities or out-of-range values;
  `i32.wrap(1.5)` is `E0001` (T-G27).
- Floats are not `Eq`/`Ord` (K1): `v.sort()`, `contains`, `T: Ord` bounds and `Map` keys reject
  them (`E0501`); sort with `v.sortBy((a, b) => a.totalCmp(b) == .Less)`. `x.totalCmp(y)`
  gives an `Ordering` from the bit-pattern total order (stdlib contract Â§4).
- `min`, `max`, `abs` on floats; `s.parseFloat()` (`?f64`, decimal notation only);
  `use std/math`: `math.sqrt`, `math.pow`, `math.floor`, `math.ceil`, `math.round`, `math.pi`,
  `math.e`; `take(&x)` leaves `0.0`.
- Printing (and `${}` in `format`) uses the shortest decimal that reads back exactly, always
  with a `.` or an exponent: `1.0`, `0.30000000000000004`, `100.0`, `0.0000001`, `1e21`,
  `1.5e-8`, `-0.0`, `inf`, `-inf`, `NaN`. Fixed notation is used for decimal exponents -7
  to 20.

Code generation: `double`/`float`; arithmetic is plain C (the runtime never enables
fast-math). The runtime gains `yv_out_f64`/`yv_out_f32` (snprintf/strtod), `yv_f64_total_cmp`,
`yv_str_parse_float` and the `yv_f2i_*` checked conversions, and now includes `<math.h>` and
`<stdio.h>`. Windows and macOS builds link but their float printing is unverified (no device).
Tests: `tests/p0/run_floats.yv`, `tests/p0/errors_floats.yv` (10 errors, 1 warning),
`tests/p0/panic_float_convert.yv`.

### Slices `x[a..b]`, `toVec`, `reserve`

Supported (semantic table S3, A2, A3, C3, O6; stdlib contract Â§2):

- `v[a..b]` on a `Vec[T]` or `[]T` is a `[]T` view; `s[a..b]` on `str`/`String` is a `str`
  view of bytes `a` up to `b`. Bounds are `int`s; `a < 0`, `b < a` or `b > len` panics with
  `P-INDEX`, and a text bound inside a multi-byte character panics with `P-STR-BOUNDARY` (S3,
  T-S1).
- A slice of owned data borrows its storage (T-G09: `v.push` while `v[0..1]` is still used is
  `E0102`); a slice of a view points where the view points; a slice of a temporary cannot be
  kept (`E0305`). A `var` slice still cannot be iterated with `for x in &part` (`E0101`, T-G19).
- `[]T.toVec()` copies copyable elements into a new `Vec[T]` that keeps the elements' borrows
  (T-C4); non-copyable elements are `E0001`.
- `v.reserve(n)`: capacity at least `n`; negative or impossible sizes panic with `P-CAPACITY`
  (T-GR7).
- Not yet: open-ended ranges `x[a..]`, `x[..b]` (`U0001`), mutable slices (none in v1).

Code generation: runtime `yv_vec_slice` / `yv_str_slice` build descriptors without copying;
`toVec` reuses the vector clone helper; `yv_vec_reserve` goes through `yv_grow`. Tests:
`tests/p0/run_slices.yv`, `tests/p0/errors_slices.yv` (4 errors, 1 warning),
`tests/p0/panic_slice_bounds.yv`, `tests/p0/panic_str_boundary.yv`.

### Dereference and `&inout T` variables

Supported (semantic table A1, A2, A3, A4, A6, A7):

- `*p` reads the target of a `&T` or `&inout T`: a copy for a `Copy` target, otherwise read-only
  access (`let t = *p` on a `String` is `E0401`). `*x` also works on `for x in xs` and read-only
  `if let` bindings, which stand for `&T` even when the element is copied.
- `*p = v` and `*p op= v` write through an `&inout T` whatever the binding was declared with
  (`let p: &inout int` can write its target, A1); the old value of a non-copyable target is
  dropped first. Through a `&T` (even in a `var`), a `for x in xs` binding or a read-only
  binding it is `E0101`; `*n` on a plain value is `E0001`.
- `let p: &inout T = &place` and `var p: &T = &place` (A7): `&place` builds a writable borrow
  where `&inout T` is expected and a read-only one where `&T` is expected; `let p = &x` with no
  type is `E0004`. The place is a variable or a field path, possibly through `&inout`/`Box`;
  `&p` for an `&inout T` variable `p` reborrows it (A4). `p = &b` rebinds a `var` reference
  (`let` is `E0014`).
- Exclusivity (A3): building `&inout` to a place that a live view or `&inout` still uses is
  `E0102`, and so is any use of the place while the new reference is still used later. A
  reborrow `q` of `p` makes `p` unusable while `q` is live. A reference must not outlive its
  target (`E0305`). Moving `p` (`let q = p`) is checked as before (`E0402`).
- `for x in &v { *x += 1 }` and `if let c = m.getMut(k) { *c += 5 }` now write copyable
  elements and values.
- Return values may use `&inout T of source`, including mutable vector elements;
  returned references can be method receivers, as described above.
- Not yet: explicit `&inout T` parameter or nested type syntax (`U0001`; use an `inout`
  parameter); storing borrowed values through `*p` (`U0001`). Ordinary owning fields
  cannot store references under the frozen escape contract.

Code generation: `&inout T` and `&T` are C pointers; `&place` reuses the `inout`-argument
place lowering, and `*p = v` is an assignment whose path starts with `DEREF_STEP`, so the
target is written as `(*p)`. Tests: `tests/p0/run_deref.yv` (heap-block check clean) and
`tests/p0/errors_deref.yv` (14 errors, one per case). Because `*p` now parses, the placeholders
in `tests/p0/errors_unsupported.yv` were replaced with other parse-time `U0001` cases at the
same lines and columns.

### Sets, `${x:.N}` and string literals in `${...}`

Added after the second cold-start experiment found that the cheat sheet teaches them:

- `Set[T]` (stdlib contract Â§3, C6): elements are integers, `bool`, `str` or `String` (others
  `E0501`); `var s: Set[String] = []`, `[:]` or `Set[int]{}`; `s.add(x)` consumes the element
  (already there: no change), `s.has(x)` and `s.remove(x)` (`bool`) take a `str` for `String`
  elements, `s.len()`, `s.clone()`, `take(&s)`; `for x in s` reads the elements in insertion
  order (one variable, else `E0001`; `in &s` is `E0103`). A set is stored as `Map[K, bool]` (same
  C type and helpers, `Ty::Set` keeps its own methods and display name). `s.add(s.len())` is
  `E0403`; storing views follows the map rules.
- `${x:.N}` (N from 0 to 17) prints a float with exactly N decimals, rounded like C `%.*f`; NaN
  and infinities print as usual. A non-float is `E0001`; other specifiers are `U0001`.
- `${...}` may contain string literals (`${parts.join(", ")}`); they are lexed as ordinary
  strings, so quotes, braces and colons inside them belong to them. This used to be `E0020`, which
  broke the rule that valid code is never reported as a syntax error.

Tests: `tests/p0/run_sets.yv`, `tests/p0/run_format_fixed.yv` (heap-block check clean),
`tests/p0/errors_sets_format.yv` (8 errors), `tests/p0/errors_interp.yv` (now a bad escape inside a
nested string: one error, no cascade).

### Concurrency, part 1: `Rc`, `Arc`, `Mutex`, `Guard`, `with`

Supported (semantic table P4, P6, P8; stdlib contract Â§6):

- `Rc(v)`, `Arc(v)` (prelude) and `Mutex(v)` (`use std/sync`, else `E0010`) move `v` into one heap
  block that starts with a header (lock, holding thread, count); the value is a `T *` to the content.
  `.clone()` of `Rc`/`Arc` adds a handle (atomic for `Arc`); the last drop drops the content.
  Fields, methods and indexing project through `Rc`/`Arc` like `Box`, read-only: writing is `E0101`.
  A view inside is `E0301`.
- `m.lock()` gives a `Guard[T] of m` (blocking; locking again on the holding thread panics with
  `P-LOCK-REENTRANT`), `m.tryLock()` a `?Guard[T]` (none on any contention). Through a guard the
  content is writable whatever the guard's binding; dropping it unlocks (`let _ = m.lock()` at
  once, `let _g` at the end of the scope). Moving `m` while a guard is alive is `E0102`.
- `with g = m.lock() { ... }` binds the guard for the block; a borrow taken through it that would
  outlive the block is `E0703` (T-P4). A `Channel[T]` type whose elements carry borrows is `E0704`.
- `scope`, `spawn`, `Send`/`Sync` and channels: see part 2 below.

Runtime: pthread or Win32 locks and condition variables, per-thread ids, atomic heap-block
counting, and an output lock taken by print statements once a thread has started. Tests:
`tests/p0/run_sync_local.yv` (heap-block check clean), `tests/p0/errors_sync_local.yv` (6 errors).

### Concurrency, part 2: `scope`, `spawn`, `Send`/`Sync`, channels

Supported (semantic table P1–P3, P5, P7, P8; stdlib contract §6):

- `scope { ... }` declares a hidden value first (so it is dropped last) whose drop waits for every
  thread the scope started, drops results nobody took and frees the join records; `return`,
  `break`, `continue` and `try` out of the scope therefore wait before data is released (T-P3).
- `spawn f(args)` (only inside a `scope`, else `E0001`; a named function) evaluates the arguments
  in the caller as for a call, then runs `f` on a new thread and gives a `Join[T]`. Moved (`sink`)
  arguments must be `Send` (`E0701`), borrowed ones `Sync` (`E0702`); borrowed data must be declared
  before the scope and not be a temporary (`E0705`); the result must be `Send` and own its data.
  The borrows are kept by the hidden scope value until the scope ends, even after `join()` (T-GP7).
  `h.join()` consumes the handle (second join `E0402`); a handle cannot be moved (`U0001`), and
  `inout` arguments are `U0001`.
- `Send`/`Sync` are derived from the type's structure: `Rc`, `Guard` never; `Arc[T]` needs `T` both;
  `Mutex[T]` and `Channel[T]` need `T: Send`; `&T`, `[]T` need `T: Sync`. `Box[Named + Send]`,
  `+ Sync` are interface types with capabilities (aliases of the interface sharing its C type and
  vtables); boxing checks the concrete type, and more capabilities convert to fewer.
- `Channel[T]{cap: n}` (`E0704` for elements with borrows, `E0701` if not `Send`, `P-CHANNEL-CAP`
  for `n <= 0`); `send` moves the value and returns `() ! SendError[T]` (`.Closed(value)` gives
  it back after `close`); `recv` gives `?T`, draining the queue after `close`; `close` is
  idempotent; `clone` shares the queue; values still queued are dropped with the last handle.
  `SendError[T]` is a library enum made per element type on first use. `ok(_u)` may now ignore a
  `()` payload.

Code generation: each spawn site gets an argument struct and an entry function, inserted ahead
of the function bodies; join records are `{ yv_join_base b; T ret; }`. Tests:
`tests/p0/run_threads.yv` (8 threads × 10,000 locked increments, unjoined `String` results,
nested scopes, early `return`), `tests/p0/run_channels.yv` (producer and consumer threads),
both heap-block checked; `tests/p0/errors_threads.yv` (6), `tests/p0/errors_channels.yv` (5),
`tests/p0/panic_channel_cap.yv`. Windows and macOS builds compile but were not run.

### `String.fromUtf8` and `Utf8Error`

`String.fromUtf8(bytes)` (stdlib contract Â§5) consumes a `Vec[byte]` and returns
`String ! Utf8Error`; `Utf8Error { offset: int }` is in the prelude and cannot be redefined
(`E0011`). Valid bytes keep their buffer as the `String` (no copy); invalid bytes are freed and
`offset` is the start of the first invalid sequence, including a sequence cut off at the end.
Keeping the bytes needs an explicit `.clone()`; using them after the call is `E0402`. Tests:
`tests/p0/run_from_utf8.yv` (heap-block check clean), `tests/p0/errors_from_utf8.yv` (4 errors).

## Spec conformance

```sh
cargo test --locked --offline --test conformance -- --nocapture
```

runs all 129 cases in `yavo-tests/MANIFEST.json`, after checking each fixture
against its `source_sha256`, and classifies every case:

| Status | Meaning |
| --- | --- |
| pass | exact expected diagnostics, output, panic or allowed outcome in every requested build mode |
| wrong | compiler behavior disagrees with the applicable frozen expectation |
| unsupported | an explicitly unsupported feature or requested sanitizer capability unavailable on the host |
| not_run | required execution environment or harness support is unavailable |
| superseded | a dated language decision replaces the old expectation; recorded in `tests/conformance_superseded.txt` |

At `b3c27a6`: **124 pass, 0 wrong, 3 unsupported, 0 not_run, 2 superseded**.

| Phase | Pass | Wrong | Unsupported | Not run | Superseded |
| --- | ---: | ---: | ---: | ---: | ---: |
| P1 | 81 | 0 | 0 | 0 | 2 |
| P2 | 20 | 0 | 1 | 0 | 0 |
| P3 | 23 | 0 | 2 | 0 | 0 |

T-V1 and T-GF2 are the two former automatic-repair expectations for
`v.push(v.len())`; the revised V2 permits that program, so no repair is needed.
The other five frozen repair cases pass. T-GR9, T-P3 and T-R2 remain unsupported
because their ASan+UBSan profile cannot link the ASan runtime on this host.
Their scenarios are attempted, and requested TSan checks execute independently.

The runner validates fixture source hashes, build-mode metadata, complete allowed
outcomes, exact output and required preceding stderr. It uses a test-only C
allocation wrapper for after-main-entry faults and native Unix argument bytes for
startup rejection. Sanitizer scenarios reuse verify's real artifact and negative
capability evidence. Reports are written to `target/tmp/conformance-report.json`;
sanitizer receipts are retained under `target/tmp/conformance-sanitizers/`.

These fixtures do not cover every v1 commitment. Current priorities are in
[HANDOFF section 10](../HANDOFF.md); the complete goal inventory is retained in
[the historical handoff, section 16](../history/handoff-batches-2026-11-19-to-21.md).

The test is a ratchet: it fails when a baseline case stops passing (excluding
explicitly superseded cases). CI must also inspect the full status report for
wrong or not_run results. After genuine progress, update the passing baseline with
`YAVO_UPDATE_BASELINE=1 cargo test --locked --offline --test conformance`.

## Cross-compilation

```sh
yavo build hello.yv --release --target x86_64-windows-gnu -o hello.exe
```

| Target | Output |
| --- | --- |
| `x86_64-linux-musl`, `aarch64-linux-musl` | static ELF (no dynamic loader, no host libc dependency) |
| `x86_64-windows-gnu`, `aarch64-windows-gnu` | PE `.exe` (the default output name gets `.exe`) |
| `x86_64-macos`, `aarch64-macos` | Mach-O |

Cross-compilation always uses the pinned zig; with `YAVO_CC` set, `--target` exits with 3.
`yavo run --target` only accepts a target this machine can execute; otherwise it is a usage
error (exit 2). Unknown targets are a usage error that lists the supported ones.

Verified on Linux x86-64 (`tests/targets.rs`, plus a one-off build of all 14 `run_*`/`panic_*`
programs for all six targets, 84 builds, no failures):

- every target produces the right executable format and architecture (ELF / Mach-O / PE header);
- Linux outputs are statically linked;
- the `x86_64-linux-musl` output runs here and matches the expected stdout.

Native CI now executes the documented Windows x86-64 and macOS arm64 selections.
Linux aarch64, Windows aarch64 and macOS x86-64 remain cross-compiled only; the
Windows/macOS selections do not establish complete frozen-report acceptance.

Release builds are stripped (`-O2 -s`, decided 2026-11-16): `run_arith.yv` for
`x86_64-linux-musl` went from 1.3 MB to 12 KB, for `x86_64-windows-gnu` from 222 KB to 76 KB,
and Windows targets no longer leave a `.pdb` next to the output. Debug builds keep debug info
(`-O0 -g`); a Windows debug build still writes a `.pdb`.

## Four behavior fixes in this prototype commit

| Area | Behavior | Regression evidence |
| --- | --- | --- |
| Discard | `_` never enters name lookup. `let _ = expression` evaluates and discards without a binding, including unit-returning calls. `_name` remains a real binding. | `errors_discard_read`, `errors_discard_parameter`, `errors_discard_loop`, `run_discard` |
| Interpolation | All operands are evaluated left-to-right into C temporaries before the outer print writes anything. No formatted heap string is created. | `run_interpolation_order`, `panic_interpolation_no_prefix` |
| C diagnostics | Both captured C compiler streams are forwarded to stderr on success and failure; stdout remains frontend JSON in diagnostic mode. | `c_compiler_diagnostics_are_visible_on_success_and_failure` |
| Output failure | Failed writes terminate with exit 101 and a best-effort `P-SYSTEM` diagnostic. POSIX EINTR is retried; partial writes are completed; zero writes fail. Error reporting does not recurse through the failing buffered writer. | Buffered/direct stdout, flush-before-stderr, read-only stderr, both streams unusable, `/dev/full`, injected EINTR/partial/zero-write tests |

Output locations are recorded at generated print statements. When a buffered
flush fails, the report uses the most recent write location in that buffer.
If stderr is also unavailable, termination still occurs without recursive
error reporting. Normal OS signal termination such as SIGPIPE remains nonzero.

The new system-level regression runner has a 60-second child-process limit
and attempts process-tree cleanup on timeout. The earlier fixture runner is
otherwise unchanged apart from testing both C optimization modes.

## Historical P0 validation record

- Before implementation changes, the added tests failed on the original
  compiler: all six new fixture files and all seven system-level test functions
  detected the corresponding behavior or missing discard handling.
- After the fixes, 22 P0 fixture files pass in both generated-C modes.
- Seven system-level regression test functions pass on Linux x86-64.
- Both Rust Debug and Release test builds pass. The validation used the
  installed Rust 1.99.1 and Zig 0.17.1; no toolchain was downloaded or changed
  during this repair.
- The seven system-level functions cover successful/failed C compiler
  diagnostics plus six runtime I/O scenarios. Runtime I/O scenarios run with
  both C optimization settings. `/dev/full` is Linux-only; the injected
  POSIX-write test is Unix-only. Windows and macOS were not run.
- No P1 ownership, Drop, or complete 129-case conformance claim is made.

On the reviewed server, Rust linking used the existing
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$HOME/.yavo/bin/rust-linker`.
That machine-local wrapper was not changed by this repair and is not required
on machines with a working Rust host linker.

Historical documents/tags and the toolchain lock are unchanged. Target
selection, nested string literals in interpolation, the diagnostic-code table,
and full machine-interface alignment remain outside these four fixes.
