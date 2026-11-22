# Yavo-written compiler: SH-1

This directory starts the [self-hosting roadmap](../docs/self-hosting.md).
It contains a real subset compiler written in Yavo: source is tokenized,
parsed into an AST, checked, and lowered to C17. Rust is used to build this
compiler and as a separate test reference. The Yavo frontend never delegates
an input program back to the Rust frontend.

**Current status: subset compiler; self-compilation is not implemented.**
Its own implementation uses modules, owned strings, vectors, structs, enums,
results and mutable borrows that its accepted input subset does not yet cover.
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
outside SH-1 must report `U0001`.

The runner clears `PATH` when executing the Yavo frontend, retains complete
commands/stdout/stderr, records source/runtime/compiler/toolchain hashes, and
checks that inputs did not change during the run. `receipt.json` identifies the
achieved stage and each individual result. Native CI runs this gate on Linux,
Windows and macOS; a configured job is not itself a passing result.

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
| Files | One source file; `fn` and `pub fn`; required `fn main()` |
| Types | `int` (signed 64-bit), `bool`, borrowed `str`, unit return |
| Literals | Decimal integers including negative `INT64_MIN`, booleans, unit, non-interpolated UTF-8 strings with supported escapes and NUL |
| Functions | Typed parameters, int/bool/unit returns, forward calls, recursion, local/global name resolution |
| Locals | `let`/`var`, inferred or explicit types, assignment and arithmetic compound assignment; active locals cannot be shadowed |
| Control flow | `if`/`else if`/`else`, `while`, `loop`, `break`, `continue`, explicit return and return-path checks |
| Expressions | Parentheses, unary minus/not, checked integer arithmetic, wrapping arithmetic, bit operations/shifts, equality/order, short-circuit boolean operators |
| Builtins | Single-argument `print`/`println`/`eprintln` as direct statements, `assert(bool)`; user functions take precedence over the assert prelude |
| Runtime behavior | Existing checked arithmetic/panic/UTF-8/output runtime; left-to-right evaluation into C temporaries |

Non-unit values require an explicit return. Only a direct call may stand alone
as an expression statement, and unit-returning functions allow `return` or
`return ()`, matching the existing language. The parser bounds syntactic nesting
at 128 and expression lowering at 256; exceeding either returns `U0001`.

Unsupported input includes source imports, structs/enums, owned `String`,
collections, general references/ownership, results/options, methods, generics,
interpolation, byte/float/nondecimal literals, integer separators, `for`,
closures and concurrency. This list describes the **new frontend's** input,
not limitations of the existing Rust compiler.

## Implementation and next dependency chain

| File | Responsibility |
| --- | --- |
| `lexer.yv` | UTF-8-aware tokens, escapes, source spans, newline termination |
| `model.yv` | Owned AST nodes; graph edges and spellings use indices |
| `parser.yv` | Function/statement/expression grammar, precedence, unsupported boundaries |
| `codegen.yv` | Types, symbols, return paths and ordered C lowering |
| `diagnostic.yv` | Shared diagnostic data |
| `main.yv` | CLI, input and exclusive output creation |
| `lexer_test.yv` | Lexer boundary assertions |
| `tests/` and `bootstrap.py` | Independent behavior/rejection oracles and reproducible execution |

The next milestone is to compile the data structures this compiler actually
uses. That requires struct/enum declarations and field access first, then
owned String/Vec values, borrow/inout/sink rules and destruction, results with
try/catch, module loading, and the remaining literal/builtin forms used by the
implementation. Each addition must have positive and negative external tests.

After the input subset covers all compiler sources, build stage2 using stage1,
then stage3 using stage2 in fresh directories; compare their generated C and
independent program behavior. Only that closes SH-2. Replacing the existing
compiler also requires the broader language, diagnostics and platform gates
in SH-3. Self-hosting and practical application acceptance remain separate.
