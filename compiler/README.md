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
