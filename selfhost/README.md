# Yavo-written compiler: owned vector storage subset

This directory starts the [self-hosting roadmap](../docs/self-hosting.md).
It contains a real subset compiler written in Yavo: source is tokenized,
parsed into an AST, checked, and lowered to C17. Rust is used to build this
compiler and as a separate test reference. The Yavo frontend never delegates
an input program back to the Rust frontend.

**Current status: subset compiler; self-compilation is not implemented.**
The input subset includes Copy aggregates, payload-free enums, owned String
values and vectors of Copy or owned String/struct elements. The complete
original `diagnostic.yv` and `model.yv` are compiled with appended test entries
and helpers. The model component constructs and manages nonempty AST storage;
reading non-Copy vector elements remains unsupported. Full compiler sources
still require element borrowing, explicit parameter modes, options/results,
modules and other forms outside the accepted input subset.
The validation receipt explicitly records `self_hosted: false` and tests that
checking its own entry point reports `U0001`.

## Build and validate

From the repository root, with Rust and the pinned Zig toolchain installed:

```sh
cargo build --manifest-path compiler/Cargo.toml --locked --release
python3 selfhost/bootstrap.py
```

`bootstrap.py` accepts `--stage0`, `--zig` and `--output`. Defaults use the
Release compiler, the toolchain installation described by `compiler/toolchain.lock`,
and `compiler/target/ci/selfhost/`. Windows executable suffixes and the configured
`YAVO_HOME` are handled automatically. It uses Python's standard library.

Each run creates a fresh directory, builds the Yavo frontend, runs its lexer
checks, and tests independently specified programs. Successful programs and
panic cases are compiled through both frontends and executed in Debug and
Release; each result must match its fixed expected output/exit/panic category.
Each successful input is emitted twice to check deterministic C. Invalid inputs
must fail before creating or changing output, and supported-language constructs
outside the subset must report `U0001`. Boundaries that stage0 also rejects as
unimplemented are counted separately as `shared_unsupported`; these must report
only `U0001` in both frontends. Each published example is byte-identical to its
executable fixture and included in the receipt hashes.

The runner clears `PATH` when executing the Yavo frontend, retains complete
commands/stdout/stderr, records source/runtime/compiler/toolchain hashes, and
checks that inputs did not change during the run. `receipt.json` identifies the
achieved stage and each individual result. Native CI runs this gate on Linux,
Windows and macOS; a configured job is not itself a passing result.

## Validation snapshot (2026-11-23)

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
and 21 new cases. The old `unsupported_vec_string_element` source remains
unsupported because it also reads a non-Copy element through an index.

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

## CLI

```sh
compiler/target/release/yavo build selfhost/main.yv --release -o yavo-selfhost
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
| Files | One source file; function and aggregate declarations, including `pub`; required `fn main()` |
| Types | `int` (signed 64-bit), `bool`, borrowed `str`, owned `String`, `Vec[T]` with Copy or owned String/struct elements, unit return; nominal structs and payload-free enums |
| String | Empty construction, str.toString, len, clone, push, output, String/str equality and ordering, UTF-8/NUL |
| Vec | Typed empty construction, empty/nonempty literals, len, consuming push, reserve, recursive clone/drop; checked reads and index/field/compound assignments for Copy elements |
| Aggregates | Forward/nested fields, construction, reads and mutable fields; Copy values or owned String/Vec/nested struct fields; constant defaults and empty aggregates; by-value cycles rejected |
| Enums | Qualified and context-inferred cases, nominal type checks, equality/inequality, case-name output |
| Literals | Decimal integers including negative `INT64_MIN`, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL; vector literals with contextual or first-element type inference |
| Functions | Typed parameters, scalar/String/Vec/struct/unit returns, forward calls and recursion; owned ordinary parameters borrow read-only; str returns remain outside the subset |
| Locals | `let`/`var`, inferred or explicit types, assignment and arithmetic compound assignment; active locals cannot be shadowed |
| Control flow | `if`/`else if`/`else`, `while`, `loop`, `break`, `continue`, explicit return and return-path checks |
| Expressions | Parentheses, unary minus/not, checked integer arithmetic, wrapping arithmetic, bit operations/shifts, equality/order, short-circuit boolean operators |
| Builtins | Single-argument `print`/`println`/`eprintln` as direct statements, `assert(bool)`; user functions take precedence over the assert prelude |
| Ownership | Consuming bindings/assignment/construction/returns; moved-value checking, read-only parameters, root-based argument/receiver protection |
| Runtime behavior | Checked arithmetic/panic/output, ordered evaluation, moved-source zeroing and cleanup on scope/statement/control-flow exits |

Non-unit values require an explicit return. Only a direct function/method call may stand alone
as an expression statement, and unit-returning functions allow `return` or
`return ()`, matching the existing language. The parser bounds syntactic nesting
at 128 and expression lowering at 256; exceeding either returns `U0001`.

Aggregate fields may hold int, bool, String, supported vectors, payload-free
enums or nested structs. Borrowed `str` fields are rejected with E0301;
unit fields remain outside this subset. Struct equality/printing/clone and enum ordering are
rejected consistently with stage0. Explicit constructor fields are evaluated
in source order. Bare enum cases use annotation/parameter/return/field/
assignment/RHS comparison context.

### Ownership boundaries

Ordinary string literals are `str`. Binding, assigning, returning or explicitly
initializing a String field from a literal requires `.toString()`; only a
String field's constant default has a special literal conversion. String
arguments may borrow a str view for a call, but persistent String-to-str
bindings and borrowed return values are not introduced by this increment.

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

Vectors accept `int`, `bool`, payload-free enums, `String` and named structs,
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

Reading a Copy element returns an independent value. Index writes, including
nested struct-field and arithmetic compound assignments, require a mutable
place and check bounds. **All indexing through owned elements remains U0001**,
including projection of a Copy field, modifying an element field and replacing
an entire element. Owning storage does not enable borrowed element access.

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

Unsupported input includes owned-element indexing, direct nested Vec
elements, borrowed `str` or unit elements, pop, slices, other collections, imports,
enum payloads, explicit user inout/sink/references/take, options/results,
user-defined methods, generics, interpolation, byte/float/nondecimal literals,
integer separators, for loops, closures and concurrency. String temporary
receivers cannot be modified. This list describes the new frontend's input,
not the broader Rust frontend.

## Implementation and next dependency chain

| File | Responsibility |
| --- | --- |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination |
| `model.yv` | Owned AST nodes; graph edges and spellings use indices |
| `parser.yv` | Aggregate/function/statement/expression grammar, vector literals/indexes, precedence, unsupported boundaries |
| `types.yv` | Nominal and vector-element types, field/default checks, cycles/layout order, transitive ownership |
| `codegen.yv` | Places, borrowed/consuming values, vector operations, move-state joins, cleanup, loop replay and ordered C |
| `diagnostic.yv` | Shared diagnostic data |
| `main.yv` | CLI, input and exclusive output creation |
| `lexer_test.yv` | Lexer boundary assertions |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles and reproducible execution |

The [token metadata example](examples/token_metadata.yv) exercises `SourceSpan`,
`TokenKind`, `TokenMetadata`, `Operation` and `Statistics` through construction, function
calls, branches and nested field updates. The new
[token-table example](examples/token_table.yv) adds a `Vec[TokenMeta]` field,
reserve/push, bounds-checked reads and updates, read-only parameters and clone
independence. `TokenMeta` is Copy. The new
[owned token store](examples/owned_token_store.yv) uses a String-bearing
Token-shaped struct and covers owned field storage, consuming push, returns,
clone and container reassignment. Neither example is the original lexer.

The String/Failure component, Copy-element Vec and **owned AST storage**
paths are implemented. The complete original `model.yv` is preserved as a
prefix of an executable component. Its test fills the `expressions`,
`statements`, `functions` and `types` vectors, including Expression arguments
and names, Statement branches, Function parameters/body and TypeDecl
fields/variants. It validates construction and lifetime using container
lengths; it does not yet traverse the non-Copy AST elements.

The next increment is **temporary read-only access to non-Copy elements and
AST traversal**. Under stage0 semantics, such an index produces `&T`.
Support field projection, ordinary read-only arguments, and explicit
String/Vec clone while preserving the owning root. Copy reads, len and
completed clone results must end their temporary dependency; borrowed call
arguments must protect the root until the call completes. No implicit deep
copy may replace an element borrow.

Extend the original-model component with while-based traversal of all four
Program vectors and their internal buffers, checking independently specified
results. Clone results must remain usable after replacing or releasing the
original owner. Persistent bindings such as `let e = p.expressions[i]`,
owned-element writes, explicit references, borrowed returns and inout/sink/take
remain separate dependencies. Arbitrary user structs do not acquire a
built-in clone method as part of this increment.

Implementation note: inferred vector literals currently receive their final
element destructor after lowering the elements. This is valid for the current
subset. Before adding try/result paths that can leave a literal early, the
temporary's complete type/destructor must be established or backfilled before
emitting any early cleanup.

Explicit inout/sink/take, options, results with try/catch, modules and remaining
literal/builtin forms follow actual compiler-source dependencies. Each
increment requires independent positive/negative and resource checks.

After the input subset covers all compiler sources, build stage2 using stage1,
then stage3 using stage2 in fresh directories; compare their generated C and
independent program behavior. Only that closes SH-2. Replacing the existing
compiler also requires the broader language, diagnostics and platform gates
in SH-3. Self-hosting and practical application acceptance remain separate.
