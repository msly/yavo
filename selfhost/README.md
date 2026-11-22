# Yavo-written compiler: String ownership subset

This directory starts the [self-hosting roadmap](../docs/self-hosting.md).
It contains a real subset compiler written in Yavo: source is tokenized,
parsed into an AST, checked, and lowered to C17. Rust is used to build this
compiler and as a separate test reference. The Yavo frontend never delegates
an input program back to the Rust frontend.

**Current status: subset compiler; self-compilation is not implemented.**
The input subset includes Copy aggregates, payload-free enums, owned String
values and structs containing String fields. The complete original
`diagnostic.yv` is compiled with an appended test entry point. Full compiler
sources still require vectors, explicit parameter modes, options/results,
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

## Validation snapshot (2026-11-22)

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

Rust language semantics, the C runtime, locked toolchain and frozen
specification are unchanged. The first CI run exposed a concurrent PCH cache
publication bug: rewriting identical runtime-header bytes changed their
mtime and invalidated another producer's PCH. The cache fix publishes the
header once, preserves its identity, and versions the PCH cache namespace.
Complete local Rust suites and the selfhost gate are rerun after this fix.
Three native CI jobs execute the new selfhost gate. A separate Linux step
recompiles the actual Yavo frontend's generated C with GCC ASan/UBSan; source
and capability checks are required. Results, source commits and the first
failed CI attempt are recorded in the String summary.

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
| Types | `int` (signed 64-bit), `bool`, borrowed `str`, owned `String`, unit return; nominal structs and payload-free enums |
| String | Empty construction, str.toString, len, clone, push, output, String/str equality and ordering, UTF-8/NUL |
| Aggregates | Forward/nested fields, construction, reads and mutable fields; Copy values or owned String/nested struct fields; constant defaults and empty aggregates; by-value cycles rejected |
| Enums | Qualified and context-inferred cases, nominal type checks, equality/inequality, case-name output |
| Literals | Decimal integers including negative `INT64_MIN`, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL |
| Functions | Typed parameters, scalar/String/struct/unit returns, forward calls and recursion; owned ordinary parameters borrow read-only; str returns remain outside the subset |
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

Aggregate fields may hold int, bool, String, payload-free enums or nested
structs. Borrowed `str` fields are rejected with E0301; unit fields remain
outside this subset. Struct equality/printing/clone and enum ordering are
rejected consistently with stage0. Explicit constructor fields are evaluated
in source order. Bare enum cases use annotation/parameter/return/field/
assignment/RHS comparison context.

### String ownership boundaries

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
Two accepted stage0 boundaries remain explicitly U0001 here: consuming an
existing owner in a while condition, and transferring the root of a borrowed
comparison operand while evaluating the other operand. Their minimal sources
and the unresolved stage0 behavior are recorded in the manifest; stage0's
observed output is not used to redefine a correctness oracle.

Unsupported input includes vectors/other collections, imports, enum payloads,
explicit user inout/sink/references/take, options/results, user-defined methods,
generics, interpolation, byte/float/nondecimal literals, integer separators,
for loops, closures and concurrency. String temporary receivers cannot be
modified. This list describes the new frontend's input, not the broader Rust
frontend.

## Implementation and next dependency chain

| File | Responsibility |
| --- | --- |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination |
| `model.yv` | Owned AST nodes; graph edges and spellings use indices |
| `parser.yv` | Aggregate/function/statement/expression grammar, precedence, unsupported boundaries |
| `types.yv` | Nominal types, field/default checks, cycles/layout order, transitive ownership |
| `codegen.yv` | Places, borrowed/consuming values, move-state joins, cleanup, loop replay and ordered C |
| `diagnostic.yv` | Shared diagnostic data |
| `main.yv` | CLI, input and exclusive output creation |
| `lexer_test.yv` | Lexer boundary assertions |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles and reproducible execution |

The [token metadata example](examples/token_metadata.yv) exercises `SourceSpan`,
`TokenKind`, `TokenMetadata`, `Operation` and `Statistics` through construction, function
calls, branches and nested field updates. It represents source positions and
compiler state; the full lexer and dynamic AST storage still require the next dependencies.

The String/Failure component path is implemented. The next increment is
`Vec` with Copy elements: typed empty construction, push/len/indexing, checked
bounds, value reads, owned-container moves and cleanup. A vector itself owns
its buffer even when its elements are Copy. After that, String/Token elements
need element destruction and borrowed indexing; blanket deep copying of index
results cannot replace stage0's semantics.

Explicit inout/sink/take, options, results with try/catch, modules and remaining
literal/builtin forms follow actual compiler-source dependencies. Each
increment requires independent positive/negative and resource checks.

After the input subset covers all compiler sources, build stage2 using stage1,
then stage3 using stage2 in fresh directories; compare their generated C and
independent program behavior. Only that closes SH-2. Replacing the existing
compiler also requires the broader language, diagnostics and platform gates
in SH-3. Self-hosting and practical application acceptance remain separate.
