# Yavo P0 compiler prototype

The prototype compiles a limited `.yv` subset to C17, then uses the pinned
`zig cc` toolchain (or the explicit `YAVO_CC` development override).
This is a Linux x86-64 validated prototype, not a completed three-platform
P0 acceptance or a P1 ownership implementation.

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
| `E0xxx` | An error about the program, as listed in `yavo-tests/ERRORS.md` (the front-end codes `E0010`–`E0018` and `E0020` were added there on 2026-11-16). |
| `U0001` | Valid v0.5 syntax that this compiler does not implement yet. It says nothing about whether the program is correct. |
| `W0001` | Warning: unused variable. |

Valid v0.5 syntax must never be reported as a syntax error. `tests/p0/errors_unsupported.yv`
locks this in for `inout` parameter types, `*p`, closures, `if let`, `for k, v in`, `.Case`,
`catch` and tuple literals, and the conformance test counts any `E0020` on a spec case as
wrong (slices `x[a..b]`, tuple fields `t.0` and bounds `T + Send` are reported as `U0001`).

## Structs (first P1 feature)

Supported: `struct` declarations whose fields are integers, `bool` or other structs; struct
literals `Name{a: 1, b: 2}` (every field given exactly once, values evaluated in the order
written); reading fields, including nested ones; assigning and compound-assigning fields of a
`var` (`s.to.y += 1`); structs as parameters and return values. Such structs are copied on
assignment and when passed, as v0.5 specifies for structs whose fields are all copyable.

Checked: a `str` field is rejected with `E0301` (a view cannot be stored in a field); a struct
that contains itself by value is `E0017`; missing literal fields are `E0018`; unknown and
duplicate fields are `E0010`/`E0011`; structs cannot be compared (`E0501`) or printed directly.
`tests/p0/run_structs.yv` and `tests/p0/errors_structs.yv` cover these.

Not yet: `String`/`Vec`/`Map` fields (and with them moves and `Drop`), methods, generic
structs, `struct X: Interface`, field defaults. A struct literal must be written as an
uppercase name immediately followed by `{` (no space), which keeps it apart from `if c {`.

Effect on the spec cases: none changed status. Every case that uses `struct` is also blocked
by generic types, imports or `String`; those are the next features to implement.

## Ownership: `String` and `Vec[T]`

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

Soundness rests on one restriction: a `String` never turns into a `str` yet, so every `str`
(including inside `Vec[str]`) is a string literal and cannot dangle. `String` -> `str` borrows,
`String`/`Vec` struct fields, reading non-copyable elements (`names[0]`), `v[i] = x`, iterating
with `for x in v`, `Map` and `inout` come next and need borrow tracking.

Leak and double-drop check: `YAVO_ALLOC_CHECK=1` builds with `-DYV_ALLOC_CHECK`, where the
runtime counts live heap blocks and exits with 102 on a leak or an extra free.
`tests/alloc_check.rs` runs every `run_*.yv` that way in debug and release. A mutation test
confirmed it: deleting one generated drop was reported as a leak, and deleting one move's
zeroing made the program fail (the C library's own double-free check fired first). It does not
detect use-after-free. AddressSanitizer is not available here: zig ships no ASan runtime, and
the machine has no gcc, clang or valgrind; UBSan does work.

## Spec conformance

```sh
cargo test --locked --offline --test conformance -- --nocapture
```

runs all 129 cases in `yavo-tests/MANIFEST.json`, after checking each fixture against its
`source_sha256`, and classifies every case:

| Status | Meaning |
| --- | --- |
| pass | the compiler does what the spec case expects (exact error codes and lines for check-fail; exact stdout or panic category in every listed build mode for run cases) |
| wrong | the compiler disagrees with the spec case: a bug, or a spec gap |
| unsupported | the compiler reported `U0001` |
| not_run | needs `yavo fix`, `yavo verify`, fixtures, sanitizers or other platforms |

At this commit: pass 12, wrong 0, unsupported 104, not_run 13 (P1: 4 / 0 / 72 / 7;
P2: 8 / 0 / 11 / 2; P3: 0 / 0 / 21 / 4). The per-case report is written to
`target/tmp/conformance-report.json`.

The test is a ratchet, not a gate: it fails only when a case listed in
`tests/conformance_baseline.txt` stops passing. After real progress, refresh the baseline with
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

Not verified: running the Windows, macOS and aarch64 outputs (no such machines or emulators
here).

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

## Validation record

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
