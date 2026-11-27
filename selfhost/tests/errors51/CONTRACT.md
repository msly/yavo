# Batch 51: standard error values and terminating catch bindings

Baseline: `b3cf7a22031846459d9955397759fb24823a9ee5`.
The expectations below were authored from the existing standard-library contract,
the unchanged Rust checker/runtime, and the pre-batch stage0. No new selfhost
candidate was used to derive a source, expected output, or expected diagnostic.

## Types, values, and output

| Type | Variants or fields |
|---|---|
| `fs.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidPath` |
| `io.Error` | `Utf8(offset: int)`, `Io(code: int)` |
| `os.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidArgument` |
| `process.Error` | `Utf8(offset: int)`, `Io(code: int)`, `InvalidArgument` |
| `Utf8Error` | Copy struct with `offset: int` |

The four enums are distinct nominal types even when tags and payload layouts
coincide. They require their standard module to be imported in the current source
file. Current stage0 does not introduce the short type name `Error`; both a single
and several standard imports leave that spelling unknown (`E0010`). A user type
or imported user-module type named `Error` retains its normal identity.

Values can be locals, parameters, return values, ordinary struct fields, and
supported Result error payloads. Copying and printing an error do not consume it.
These types do not implement Eq/Ord in stage0; comparisons reject with `E0501`.
An error enum prints `Utf8(offset: 5)`, `Io(code: -7)`, `InvalidPath`, or
`InvalidArgument`, without a module prefix. `format`, interpolation, stdout and
stderr use the same representation. `Utf8Error` exposes `offset`; directly
printing/interpolating that struct rejects with `E0001`.

Contextual payload construction accepts `.Utf8(5)` and `.Utf8(offset: 5)`, and the
corresponding `Io` forms, where the expected error type is known. Field names,
arity, and payload types are checked. Zero-payload qualified construction such as
`fs.Error.InvalidPath` is supported. The current stage0 rejects qualified payload
construction such as `fs.Error.Utf8(5)` and `fs.Error.Utf8(offset: 5)` with `U0001`;
the four frozen shared-unsupported cases preserve this existing implementation
boundary despite the more general syntax shown in `docs/stdlib_contract.md`.

## Standard calls and Result handling

The following already-supported calls now produce ordinary typed Result values:

- `fs.readText`, `fs.writeNewText`, `fs.tempDir`, `fs.removeFile`, `fs.removeDir`,
  and `fs.sameFile`, with `fs.Error`.
- `io.readAll`, with `io.Error`.
- `os.env`, with `os.Error`.
- `process.run`, with `process.Error`.
- `String.fromUtf8`, with `Utf8Error`.

Call results may be saved, discarded explicitly, returned, propagated with `try`,
or handled with a terminating `catch e { ... }` or `catch _ { ... }`. The same
rules apply when a user function forwards the result. A catch block must terminate
its path; falling through rejects with `E0601`. The binding is immutable, scoped
to the handler, and subject to the existing no-shadowing rule. Changing the error
type requires an explicit handler and construction of the destination error;
identical-looking standard enums do not convert implicitly.

Borrowed standard-call arguments are evaluated in the existing order and remain
owned by their caller. `String.fromUtf8` consumes its byte vector exactly once,
including the failure path; the failure payload is the first invalid UTF-8 byte
offset. Stored Result owners, partly constructed aggregate/vector values, local
guards, and successful payloads must be released on normal return, `try`, catch
return, break, and continue. Test builds retain the existing allocation counter;
the independent I/O gate also injects the existing test-only checked-exit header
without modifying production C or the runtime.

## Matching an error value

Matching is limited to a standard error enum value. Flat `.Utf8(offset)`,
`.Io(code)`, zero-payload cases, corresponding qualified patterns, wildcards and
whole-value bindings are supported. Each payload is an `int`; the scrutinee is
evaluated once. Arms may be expressions, statements, or blocks under the existing
stage0 syntax. Owned expression results and branch moves follow the existing
ownership rules; return/break/continue preserve their surrounding control flow.

Matches must be exhaustive (`E0602`), patterns must belong to the scrutinee's
nominal type, and payload counts must agree. Pattern bindings do not escape their
arm and cannot shadow a pre-existing binding. Duplicate/covered arms retain
stage0's accepted-program behavior; warnings are not converted into errors.

Result `ok`/`err` patterns, nested payload patterns, Optional/general enum matching,
user payload enums, arrow catch, `io.readLine`, `fs.writeText`, and error values in
Vec/Optional/tuple storage remain outside this declared subset. They are not
implicitly enabled by the standard-error implementation.

## Frozen semantic acceptance

`cases.json` contains 73 new probes: 30 run, 29 reject, 10 subset-unsupported, and
4 shared-unsupported. Every run output is an exact byte expectation. The old
stage0 checked all 73 classifications, and independently generated and executed
all 30 run programs in Debug and Release before first candidate execution.
`reference-checks.json` and `reference-runs.json` retain the source/producer
identities and reference command evidence.

`promotions.json` separately lists ten existing fixtures whose path and source
bytes remain unchanged. Their old manifest entries are retained in the promotion
record. Environment variables and isolated working directories are part of the
execution fixture, not changes to the programs: `name` is explicitly absent for
the environment cases; the original relative program `x` executes in a fresh
empty directory. Standard `process.run` does not search PATH.

`semantic-freeze.json` binds this contract, cases, promotions, reference evidence,
all 73 new entry sources and the user-module sibling, plus the ten original
promoted sources. The global bootstrap manifest continues to count these probes
separately from the fixed 622-file corpus scan.

## Independent real-I/O acceptance

`driver.yv` is one multi-mode test program. `io-cases.json` specifies 48 scenarios
and exact input bytes, environment overrides, output bytes, exit codes and final
filesystem trees. The Python `oracle.py` independently derives each expectation
from the contract; it never reads generated C, invokes a compiler, or learns from
candidate output. `io-freeze.json` binds those files and this contract.

Scenarios cover valid Unicode and embedded NUL text, invalid and incomplete UTF-8,
native I/O failures, invalid path/argument variants, non-destructive failed
exclusive writes, temporary-directory cleanup, file/directory removal, file
identity, environment fallback/empty values, and child nonzero success versus
launch failure. Native `Io` payloads are checked as nonzero instead of freezing
platform-specific errno/GetLastError numbers; the semantic probes separately
check exact artificial payload formatting. Source and I/O bytes are rebuilt in a
fresh directory for every product/scenario.

Linux and macOS run all 48 scenarios. Windows runs 46: directory-as-stdin and an
environment value containing invalid native UTF-8 bytes are POSIX-specific and
are excluded in the frozen manifest. This is a declared platform applicability
boundary, not a dynamically skipped failure.

Formal native acceptance uses stage0 and the actual complete-bootstrap stage1
and stage2, each in Debug and Release: 288 executions on Linux/macOS and 276 on
Windows. A developer candidate is explicitly preliminary and cannot establish
that producer identity. Linux independent GCC ASan+UBSan compiles the original
stage0 and final-stage2 C and runs 96 further scenarios after capability positive
and negative controls. No new formal CLI application or performance result is
claimed by this test driver.
