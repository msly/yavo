# GitHub CI

`Yavo CI` runs on pushes to `main`, pull requests, and manual dispatches. Changes
limited to Markdown or archived validation JSON do not rerun it; compiler,
runtime, fixtures, scripts, locks and workflow changes still trigger every job.
It uses
Rust 1.99.1 and the existing SHA-256-locked Zig 0.17.1 archives. The checkout and
artifact actions are pinned to reviewed full commits; workflow permissions are
read-only. Debug and Release are sequential within each job because the existing
test runners share temporary paths.

Setup first builds and executes one small native program in C Debug and Release,
sequentially, to initialize the new C toolchain before concurrent fixture tests.
These commands have separate logs and are not performance measurements or Cargo
test totals; the existing 120-second per-fixture build deadline is unchanged.

| Job | Executed gate |
| --- | --- |
| Ubuntu 24.04, x86_64 | All Cargo tests in Debug and Release, including six-target cross-compilation and frozen conformance |
| Windows 2025, x86_64 | Native compiler unit tests and the explicitly listed portable integration suites, in Debug and Release |
| macOS 15, arm64 | The same native selection, in Debug and Release |
| Independent Linux ASan+UBSan | Export the current generated C and runtime, validate sanitizer positive/negative probes, then execute every exported P0 run/panic fixture with strict output and exit expectations |

The native selection is declared in `NATIVE_TESTS` in `run.py`: P0 fixtures,
allocation checks, Arena, modules, indexed assignment/methods, mutable-reference
returns, collections, fixes, concurrent runtime I/O, and standard-library I/O.
P0 and heap checks build/run their programs with C `-O0` and `-O2`; they include
floats, threads, channels, and the stack-panic fixture. Windows/macOS numbers are
reported as this selection, not as the complete Linux suite. Their frozen report
scenarios remain excluded because the current conformance runner explicitly
requires Linux x86_64 for those scenarios.

Linux conformance is checked twice: the compiler's existing monotonic ratchet and
a CI contract matching all 129 manifest case identities. The current contract is
124 pass, zero wrong/not_run, three unsupported (`T-GR9`, `T-P3`, `T-R2`), and two
superseded (`T-GF2`, `T-V1`). Changes to those exceptions require explicit review
of `conformance.json`; CI never updates the baseline. The one ignored Cargo test
is the old exploratory `p0::conformance_report`; the active conformance suite is
not ignored. This gate does not measure v1 completeness.

Each job uploads `compiler/target/ci/` for 14 days, even after failure. It contains
the source revision, host, exact command/exit/time records, toolchain and Cargo
lock hashes, compiler hash, full logs, actual test totals, and (on Linux) separate
Debug/Release conformance JSON. The sanitizer artifact additionally retains the
source/C/runtime bundle, capability probes, executables, and detailed validator
receipts. Independent GCC results do not turn the pinned Zig ASan exceptions
into conformance passes.

For local reproduction in a fresh checkout with Rustup and Python available:

```sh
python3 tools/ci/run.py setup
python3 tools/ci/run.py native --suite full   # Linux x86_64
python3 tools/ci/run.py native --suite native # Windows/macOS selection
python3 tools/ci/run.py sanitize             # Linux with Perl and GCC
```

Use `python` on Windows. A setup/download failure is a failing job, never a skip.
Reports and working outputs are under the ignored Cargo target directory; the
workflow does not publish releases, change repository permissions, or use a
personal access token.
