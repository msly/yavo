# Yavo-written compiler: Copy aggregate subset

This directory starts the [self-hosting roadmap](../docs/self-hosting.md).
It contains a real subset compiler written in Yavo: source is tokenized,
parsed into an AST, checked, and lowered to C17. Rust is used to build this
compiler and as a separate test reference. The Yavo frontend never delegates
an input program back to the Rust frontend.

**Current status: subset compiler; self-compilation is not implemented.**
The input subset now includes Copy structs and payload-free enums. Its own
implementation still needs modules, owned strings and vectors, results, mutable
borrows and other forms that the input subset does not yet cover.
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

The [Copy aggregate summary](../compiler/validation/selfhost-aggregates-2026-11-22.json)
records **138/138 cases and 10/10 auxiliary checks**, with no skipped checks:
29 normal programs, 10 panic programs, 81 rejected programs, 14 stage0-accepted
but subset-unsupported programs, and 4 boundaries unsupported by both frontends.
All 39 executable inputs pass Debug and Release fixed oracles and Rust
differential checks; their repeated C output is byte-identical.

All 72 original fixture sources are retained byte-for-byte. The former
`unsupported_struct` declaration is now a supported run case; 66 cases were
added. Review regressions cover nominal values, temporary assignment targets,
literal capacity and conservative loop-return checking. The lexer separately
checks the `u128` token limit, including leading zeroes; used `int` expressions
still require the signed 64-bit range. A same-loop syntactic `break` counts
even after unreachable `continue` or `return`, matching stage0's conservative
return check; nested-loop breaks stay within their loop.

This increment changes only the Yavo frontend, its tests/harness and documents.
The Rust frontend, C runtime, locked toolchain and frozen specification are
unchanged. The previous complete local stage0 Debug/Release suites each passed
142 tests, failed 0 and ignored 1; those local suites were not rerun for this
increment. [CI run 45025445954](https://github.com/msly/yavo/actions/runs/45025445954)
passed all six jobs at `fbb322e`, including this frontend gate on Linux x86-64,
Windows x86-64 and macOS arm64. The aggregate summary binds the implementation
commit, successful job/step results and artifact metadata. Archive contents were
not separately downloaded or rehashed in this increment.

The [preceding SH-1 summary](../compiler/validation/selfhost-sh1-2026-11-22.json)
retains its 72-case baseline and
[CI run 45019581088](https://github.com/msly/yavo/actions/runs/45019581088).
The counts measure tested behavior, not a self-hosting completion percentage.

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
| Types | `int` (signed 64-bit), `bool`, borrowed `str`, unit return; nominal Copy structs and payload-free enums |
| Aggregates | Forward and nested struct fields, construction, reads and mutable field assignment, value copies, scalar literal field defaults, empty structs/enums; by-value cycles rejected |
| Enums | Qualified and context-inferred cases, nominal type checks, equality/inequality, case-name output |
| Literals | Decimal integers including negative `INT64_MIN`, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL |
| Functions | Typed parameters, int/bool/Copy aggregate/unit returns, forward calls, recursion, local/global name resolution; default parameters are read-only |
| Locals | `let`/`var`, inferred or explicit types, assignment and arithmetic compound assignment; active locals cannot be shadowed |
| Control flow | `if`/`else if`/`else`, `while`, `loop`, `break`, `continue`, explicit return and return-path checks |
| Expressions | Parentheses, unary minus/not, checked integer arithmetic, wrapping arithmetic, bit operations/shifts, equality/order, short-circuit boolean operators |
| Builtins | Single-argument `print`/`println`/`eprintln` as direct statements, `assert(bool)`; user functions take precedence over the assert prelude |
| Runtime behavior | Existing checked arithmetic/panic/UTF-8/output runtime; left-to-right evaluation into C temporaries |

Non-unit values require an explicit return. Only a direct call may stand alone
as an expression statement, and unit-returning functions allow `return` or
`return ()`, matching the existing language. The parser bounds syntactic nesting
at 128 and expression lowering at 256; exceeding either returns `U0001`.

Aggregate fields currently hold `int`, `bool`, payload-free enums, or other Copy
structs. Borrowed `str` fields are rejected with `E0301`; owned fields and unit
fields are outside this subset. Struct equality/printing and enum ordering are
rejected consistently with stage0. Explicit constructor fields are evaluated in
source order, independently of declaration order. Enum names are resolved from
annotations, parameters, return types, fields, assignments and the left operand
of comparisons; a bare case on the left of a comparison has no inferred context.

Unsupported input includes source imports, enum payloads, owned `String`,
collections, general references/ownership, results/options, methods, generics,
interpolation, byte/float/nondecimal literals, integer separators, `for`,
closures and concurrency. This list describes the **new frontend's** input,
not limitations of the existing Rust compiler.

## Implementation and next dependency chain

| File | Responsibility |
| --- | --- |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination |
| `model.yv` | Owned AST nodes; graph edges and spellings use indices |
| `parser.yv` | Aggregate/function/statement/expression grammar, precedence, unsupported boundaries |
| `types.yv` | Nominal type registry, field/default checks, duplicate/cycle detection, stable layout order |
| `codegen.yv` | Symbols, expected types, mutable places, return paths and ordered C lowering |
| `diagnostic.yv` | Shared diagnostic data |
| `main.yv` | CLI, input and exclusive output creation |
| `lexer_test.yv` | Lexer boundary assertions |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles and reproducible execution |

The [token metadata example](examples/token_metadata.yv) exercises `SourceSpan`,
`TokenKind`, `TokenMetadata`, `Operation` and `Statistics` through construction, function
calls, branches and nested field updates. It represents source positions and
compiler state; owned lexemes and dynamic AST storage are still pending.

The next increment is the `String`/`Failure` ownership path. Preserve the
complete original bytes of `diagnostic.yv` as a fixture prefix and append a
standalone test entry point; its existing `Failure` fields and `make` function
require owned string construction, borrowed reads, ownership transfer and
cleanup, without needing vectors or source imports.

That increment must distinguish borrowed reads from consuming bindings,
assignments, constructors and returns; generate cleanup on every supported
scope/control-flow exit; and reject moved-value reuse, moves out of borrowed
places and conflicting modification. Builtin string methods need receiver-aware
parsing, while user-defined methods remain a later feature. Normal completion
must pass allocation checks in both build modes.

Next add `Vec` with Copy elements, then owned String/Token elements with
borrowed indexing and element destruction. Explicit inout/sink/take, options,
results with try/catch, modules and the remaining literal/builtin forms follow
their actual source dependencies. Each increment needs external positive and
negative tests before attempting all compiler sources.

After the input subset covers all compiler sources, build stage2 using stage1,
then stage3 using stage2 in fresh directories; compare their generated C and
independent program behavior. Only that closes SH-2. Replacing the existing
compiler also requires the broader language, diagnostics and platform gates
in SH-3. Self-hosting and practical application acceptance remain separate.
