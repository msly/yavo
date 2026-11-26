# Yavo-written compiler: self-hosted bootstrap subset

This directory implements the [self-hosting roadmap](../docs/self-hosting.md).
It contains a compiler written in Yavo: source is tokenized, parsed into an
AST, checked, and lowered to C17. **It compiles its own source.** stage0
(the Rust compiler) builds stage1 from these files; stage1 compiles the same
files to stage2 C; stage2 compiles them again to stage3 C, and the stage2 and
stage3 C files are byte-identical. Rust is used to build stage1 and as a
separate behavioral reference; no stage delegates an input back to Rust.

**Status: SH-2 fixed point reached for the bootstrap subset (2026-11-23).**
The compiler sources were rewritten into a *bootstrap dialect*: a strict
subset of the stage0 language that the compiler itself implements (see
[decision 20–24](../docs/decisions.md)). It is not yet the default compiler:
stage0 accepts far more of the language (owned optionals, mutable/reference iteration,
float formatting, methods, generics, broader Map/Set APIs, closures, threads...).
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
python3 selfhost/bootstrap.py
```

The compiler is a module program whose entry is `main.yv`; it loads
`support.yv`, `lexer.yv`, `model.yv`, `parser.yv`, `types.yv` and `codegen.yv`
through `use` and calls across files as `module.f(...)`. `bootstrap.py` copies
these files (`COMPILER_PARTS`) unchanged into the run directory and gives every
compiler only the entry file. The lexer unit test's entry is `lexer_test.yv`,
which uses `support` and `lexer`; its current validation status is listed below.
To build the compiler by hand: `yavo build selfhost/main.yv -o yavo-selfhost`.

`bootstrap.py` accepts `--stage0`, `--zig` and `--output`. Defaults use the
Release compiler, the toolchain installation described by `compiler/toolchain.lock`,
and `compiler/target/ci/selfhost/`. Windows executable suffixes and the configured
`YAVO_HOME` are handled automatically. It uses Python's standard library.

Each run creates a fresh directory and:

1. assembles the compiler and builds stage1 with stage0;
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

## Current validation: batch 44 (2026-11-26)

The matrix contains **875 cases / 260 executable inputs**: 228 run, 32 panic,
473 reject, 111 unsupported and 31 shared unsupported. The 28 additions have
fixed stage0 oracles: five run, one panic, 21 reject and one unsupported case.
The old builtin_max fixture is promoted unchanged from unsupported to run.
They cover int/byte inference, raw versus explicit Optional/Result wrapping,
arity/type/range diagnostics, name shadowing, ordered snapshots, catch exits,
and a left-side division panic that must precede right-side effects.
Complete local bootstrap passes **875/875 cases, 15/15 checks, zero skips**.
Stage2/stage3 C is byte-identical at 1,018,462 bytes; both Linux binaries are
476,112 bytes and byte-identical. The final stage2 sweep confirms 100/392
accepted programs and zero subset violations. Exact-commit
[CI run 45783874254](https://github.com/msly/yavo/actions/runs/45783874254)
for implementation `1c8207d` passes **6/6 jobs**: native Linux, Windows and macOS
bootstrap/application gates, Linux ASan/UBSan and the independent checks.

The new `text_diff_application` check builds the unchanged original
`experiments/coldstart-2026-11-21-r4/yavo/T11.yv`. Its 35 fixed scenarios require
140 stage0/selfhost Debug/Release executions to match exact stdout, stderr and
exit codes. Expectations come from the public deletion-first LCS tie rule,
independent recurrences and published fixtures. Cases cover equal/empty files,
insertions/deletions/replacements, duplicates, blanks, UTF8 and read/argument
errors; line-API and CLI boundaries are explicitly separated from the public
LF-only input contract. Ten inputs bind the original source, harness, shared
runner, fixed scenarios, contracts and public fixtures. All builds enable
allocation instrumentation: 32 runs return normally and check it, while 108
use the original os.exit(1/2) paths, which skip normal destructors and that check.
The previous test-summary and unit-conversion gates remain in bootstrap.

The candidate repository sweep preserves all 612 original source files and
all 391 stage0-accepted members. Original-program coverage is **98 to 99 / 391**,
gaining only the original T11 CLI and losing none. The new Rust P0 regression
is accepted by both compilers, so the full corpus is **100 / 392**, with zero
strict-subset violations and none of 221 stage0-rejected inputs accepted.
Rust MinMax previously read a local/field only after a later argument mutated it;
its lowering now binds both operand values before selecting one. The P0 fixture
preserves the intended snapshots for int/byte, fields, parameters, indexes and
nested calls. See [HANDOFF section 24](../HANDOFF.md).

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
cat selfhost/{support,lexer,model,parser,types,codegen,main}.yv > yavo-selfhost.yv
compiler/target/release/yavo build yavo-selfhost.yv --release -o yavo-selfhost
./yavo-selfhost check selfhost/examples/gcd.yv
./yavo-selfhost emit-c selfhost/examples/gcd.yv -o gcd.c
```

The output path must be new: successful emission uses `fs.writeNewText`, which
creates exclusively and refuses an existing path, including a link to the input.
The frontend completes checking before opening that output. A write failure
after creation can leave a partial new output file; remove that failed artifact
before retrying. Compiling generated C requires `compiler/runtime/yavo_rt.h`
and the locked Zig C compiler, as demonstrated by `bootstrap.py`.

Exit codes: `0` success, `1` frontend rejection, `2` CLI usage, `3` I/O failure.
Diagnostics currently report the first error as
`path:line:column: CODE: message`; columns count Unicode scalars and stored spans
use UTF-8 byte offsets. `E0020` is syntax, `U0001` is an unimplemented construct
or explicit implementation limit, and other `E` codes indicate semantic errors.
JSON diagnostics, suggestions and recovery remain future work.

## Accepted input subset

| Area | Implemented |
| --- | --- |
| Files | Source modules via leading `use a`, `use a/b`, `use a/{b, c}`; per-file imports and top-level `pub`, cyclic imports, qualified functions and original source locations; standard fs/io/os imports; required entry `fn main()` |
| Types | `int` (signed 64-bit), `byte` (unsigned 8-bit), `bool`, borrowed `str`, owned `String`, `Vec[T]` with existing scalar/struct/String element forms, unit return; nominal structs, payload-free enums, flat Copy tuples, supported `T ! E` results and Copy `?T` |
| String | Empty construction, str.toString, len, clone, push, output, String/str comparison, UTF-8/NUL; byte indexing; startsWith/endsWith/contains/find/split/parseInt/trim/lines/fields |
| Vec | Typed empty construction, empty/nonempty literals including Vec[byte], len, consuming push, reserve, recursive clone/drop; zero-argument pop for Copy elements on mutable local/field/inout places; checked reads and index/field/compound assignments for Copy elements; expression-local read-only indexing and field projection for owned elements |
| Aggregates | Forward/nested fields, construction, reads and mutable fields; Copy values or owned String/Vec/nested struct fields; constant defaults and empty aggregates; by-value cycles rejected |
| Tuples | Flat int/byte/bool/non-owning struct/payload-free enum elements; construction, numeric `.0`/`.1` projection, locals, arguments, returns, whole-value assignment/inout and Optional payloads; bounded structural signatures |
| Enums | Qualified and context-inferred cases, nominal type checks, equality/inequality, case-name output |
| Literals | Decimal integers including negative `INT64_MIN`, contextual byte-range integers, single-quoted ASCII byte literals and seven escapes, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL; vector literals with contextual or first-element type inference |
| Functions | Typed parameters, scalar/String/Vec/struct/Copy-tuple/unit returns, forward calls and recursion; owned ordinary parameters borrow read-only; `inout` parameters with `&variable` or `&variable.field` arguments (later arguments may not use that root, E0102); borrowed str/Vec[str]/slice returns carry their required `of` sources |
| Results | User-function `T ! E`, contextual `ok(value)`/`err(error)` and implicit success wrapping, `try`, terminating `catch name { ... }`/`catch _ { ... }`; owned String/Vec/struct payloads and cleanup on propagation |
| Optionals | `?T` for int/byte/bool/non-owning struct/payload-free enum/flat Copy tuple; `some`, contextual `none`, implicit wrapping, isSome/isNone, lazy `??` and terminating `??` fallbacks; `if let` with else/else-if-let and `_` |
| Text slices | `s[start..end]` on str/String with int bounds and P-INDEX/P-STR-BOUNDARY checks; views of owned values may be bound locally while their owner stays unchanged; escaping the owner scope remains rejected |
| Standard calls | `take(&place)` for String/Vec places; `os.args()` (len, indexing to static str), `os.exit(code)`; `io.readAll`, `fs.readText`, `fs.writeNewText` and `String.fromUtf8` only with a `catch _ { ... }` handler that cannot fall through (E0601) |
| Locals | `let`/`var`, inferred or explicit types, assignment and arithmetic compound assignment; active locals cannot be shadowed |
| Control flow | if/else-if/else and Copy Optional if-let; while/loop; int/byte half-open range for; one/two-binding read-only Vec/slice/args traversal including fields and temporary vectors; break/continue/return and return-path checks |
| Expressions | Parentheses, int unary minus, boolean not, checked int/byte arithmetic, wrapping arithmetic, bit operations/shifts, contextual integer arithmetic, equality/order, short-circuit boolean operators |
| Builtins | Single-argument `print`/`println`/`eprintln` as direct statements including decimal byte output, `assert(bool)`, explicit `int(...)`/`byte(...)` conversions, checked `abs(int)`, identity `abs(byte)` and matching int/byte `min`/`max`; user functions take precedence over assert/abs/min/max preludes |
| Ownership | Consuming bindings/assignment/construction/returns; moved-value checking, read-only parameters, expression-local element borrows, root-based argument/receiver protection |
| Runtime behavior | Checked arithmetic/panic/output, ordered evaluation, moved-source zeroing and cleanup on scope/statement/control-flow exits |

Value-returning functions require an explicit return. Direct calls, method
calls, `try` and `catch` may be expression statements; a Result-valued call
must be handled or explicitly discarded with `let _ = ...`. Plain unit-returning
functions allow `return` or `return ()`. The parser bounds syntactic nesting
at 128 and expression lowering at 256; exceeding either returns `U0001`.

Aggregate fields may hold int, byte, bool, String, supported vectors, payload-free
enums or nested structs. Borrowed `str` fields are rejected with E0301;
unit fields remain outside this subset. Struct equality/printing/clone and enum ordering are
rejected consistently with stage0. Explicit constructor fields are evaluated
in source order. Bare enum cases use annotation/parameter/return/field/
assignment/RHS comparison context.

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
are copied and owned String/struct elements are read-only views. The iterable
is not consumed; its root remains borrowed throughout nested loops and calls.
An otherwise permitted mutation, move, `take` or inout access to that root
fails with E0102. Reads and nested read-only traversal remain allowed, and
the loop releases its borrow when it ends.

Headers run before entering the new loop's control context. A `break` or
`continue` inside a terminating header `catch` targets an enclosing loop.
Owned header temporaries survive `continue` in the new loop's body and are
cleaned when that loop completes or breaks; an enclosing jump, return or
propagation also cleans the temporaries whose scope it exits.

Explicit reference/mutable traversal (`in &items`) and Map/Set traversal
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

### Copy tuples

Tuple elements may be int, byte, bool, non-owning structs or payload-free enums.
Element expressions are evaluated and copied left to right, so a later inout
argument cannot change an earlier element's snapshot. Structural type identity
uses a collision-free radix encoding bounded by signed int capacity; exceeding
that capacity is U0001. Capacity depends on the program's named-type count: a
program with no named types can encode 31 int elements, while 32 is unsupported.
Layouts are emitted after complete named layouts and before function declarations,
including tuple signatures used only by otherwise-unused functions.

Tuples support local bindings, whole-value assignment, parameters, return values
and inout. Numeric projection accepts decimal ordinals (including `.01`) and
reports E0010 for unknown/out-of-range members. A singleton `(value,)` is inferred;
an explicit singleton tuple type remains E0020, matching stage0. Nested, owned,
borrowed or Optional elements, tuple fields/containers, tuple Results, references
and destructuring are not implemented. Unit element expressions are E0001. Tuple element writes are
rejected (E0014 through an immutable root, otherwise E0001); comparison is E0501
for matching tuple types and E0001 for different types; direct output is E0001.

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

### Copy Optional and pop

Optional payloads are limited to int, byte, bool, non-owning structs,
payload-free enums and supported Copy tuples. `some(value)` infers its actual payload type; `none` needs
an Optional context. Bindings, assignments, arguments and returns can wrap a
matching Copy payload implicitly. Both tag methods take zero arguments.

`Vec.pop()` evaluates a writable place once, checks its existing move/borrow/
reservation permissions, then returns `none` for an empty vector or removes
and copies the last element into `some`. Capacity is retained. The returned
Optional has no borrow back to the vector. Temporary receivers and owned
indexed places retain the previous U0001 boundary.

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

`if let name = optional { ... }` evaluates the condition once, copies its
supported payload only in the then arm, and accepts else/else-if-let. A named
binding of an existing Optional keeps a scoped loan: modifying that source
inside the then arm is U0001 even after the binding's last use, because this
subset has no liveness analysis. Copying or returning the payload is allowed;
`_` has no source loan. Read-only binding writes are E0101; writes to a Copy
binding from a mutable source or temporary are U0001, matching stage0.
Branch move/source joins and loop replays preserve these rules.

Jumping `?? return/break/continue` is accepted for a whole binding initializer
or assignment RHS. Owned/String/Vec/borrowed-str/unit payloads, nested Optional,
Optional fields/container elements, Optional/Result combinations, owned-element
pop, destructuring patterns and match remain U0001. Optional comparison and
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

### Ownership boundaries

Ordinary string literals are `str`. Binding, assigning, returning or explicitly
initializing a String field from a literal requires `.toString()`; only a
String field's constant default has a special literal conversion. String
arguments may borrow a str view for a call. Local view bindings keep the owner
unchanged in their scope; borrowed returns must carry permitted `of` sources.

Owned locals move into bindings, assignments, constructors and returns.
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

Vectors accept `int`, `byte`, `bool`, payload-free enums, `String` and named structs,
including structs with String or supported Vec fields. Nominal enum/struct
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

Persistent bindings of a non-Copy indexed borrow, such as
`let element = v[i]`, remain U0001.
Using that borrowed result as an owned annotation, return, push argument or
typed vector element is E0404; moving an owned field out of it is E0401.
An arbitrary user struct still has no built-in clone method (E0010).
Owned-element replacement, indexed-field mutation and mutation methods
through an owned element remain outside the subset. General explicit references,
sink parameters, taking from owned indexed elements and borrowed returns
remain separate boundaries.

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

Unsupported input includes persistent bindings of borrowed collection elements and
mutation through non-Copy indexes, direct nested Vec elements, unit or tuple
vector elements, owned-element pop, vector slices, other collections, imports after other items,
enum payloads, sink parameters and general references, owned/nested Optionals,
Optional fields/container/Result combinations, Result fields,
`Vec[Result]`, nested Results, arrow catch,
user-defined methods, generics, interpolation outside direct output/format
arguments, format specifiers, float/nondecimal literals,
integer separators, mutable/reference iteration, Map iteration, owned or nested
tuples and tuple fields/containers/Results, destructuring, closures and concurrency. String temporary
receivers cannot be modified. This list describes the new frontend's input,
not the broader Rust frontend.

## Implementation

| File | Responsibility |
| --- | --- |
| `support.yv` | Decimal formatting/parsing, small string joins, owned `Diagnostic` and `path:line:column: CODE: message` reporting |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination; returns `Vec[Token] ! Diagnostic` |
| `model.yv` | Copy AST nodes; child lists are ranges in shared Program pools |
| `parser.yv` | Aggregate/function/statement/expression grammar, imports, slices, borrows, catch, unsupported boundaries |
| `types.yv` | Resolved nominal/vector/Copy Optional/flat tuple types, bounded tuple signatures, Result pool, field/default checks, cycles/layout order, transitive ownership |
| `codegen.yv` | Places, borrowed/consuming values, Optional/Result wrapping, lazy coalesce, Result propagation, inout/take/pop, move-state joins, cleanup, loop replay and ordered C |
| `main.yv` | `compile` returns `String ! Diagnostic`; CLI reports errors, reads input and creates output exclusively |
| `lexer_test.yv` | Lexer boundary assertions, including owned error propagation and cleanup on normal test returns |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles, fixed point and reproducible execution |

Inside the compiler, parsing, checking and lowering still call `fail`, which
prints the first diagnostic and exits with status 1. Lexing returns an owned
`Diagnostic` containing code, message and byte/line/column location; `compile`
uses `try`, and `main` catches, reports and exits. Lexer tests catch failures
and return normally so allocation checks can observe cleanup. Copy metadata
stacks retain capacity and pop only their discarded suffix. Loop exit states
remain in one flat `Vec[bool]` pool.

The [token metadata](examples/token_metadata.yv), [token table](examples/token_table.yv),
[owned token store](examples/owned_token_store.yv), [borrowed token scan](examples/borrowed_token_scan.yv)
and [lexer byte helpers](examples/lexer_byte_helpers.yv) examples remain
executable fixtures from earlier increments.

## Next steps

Self-hosting remains a regression gate. Batch 44 has 875 fixtures and three
application checks; current results are recorded above. Full local sweep coverage
is 100/392, including the new accepted P0 regression; the original-corpus gain is
98 to 99 / 391. The next target is the unchanged
`experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T11/final/main.yv`, a
sliding-window lower-median CLI whose only observed blocker is `Vec[int].sort()`.
It uses already-supported I/O, integer vectors, indexed access and iteration.
The runtime has integer comparators and stable sorting; add receiver permission
checks and direct application execution before claiming acceptance. Keep the
native fixed point and independent behavior/resource oracles on all three
platforms. SH-3 remains incomplete and formal SH-4 delivery remains one of three.

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
