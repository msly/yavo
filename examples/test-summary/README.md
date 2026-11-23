# yavo-test-summary

A four-module Yavo command-line application that recounts archived test logs and
checks their declared summaries. It is the first application in the SH-4 work;
the current Rust stage0 compiles it. The complete input, output and error
contract is [SPEC.md](SPEC.md).

## Build and use

From the repository root, with stage0 and the locked Zig toolchain installed:

```sh
mkdir -p compiler/target/test-summary-app
compiler/target/release/yavo build examples/test-summary/main.yv --release \
  -o compiler/target/test-summary-app/yavo-test-summary
compiler/target/test-summary-app/yavo-test-summary \
  experiments/coldstart-2026-11-21-r4/evidence/yavo-verification.log \
  experiments/coldstart-2026-11-21-r4/evidence/rust-verification.log
```

On Windows, use the corresponding `.exe` executable names. The CLI accepts
`[--] LOG...`; use `--` before a filename that begins with `-`. `--help` prints
the usage line. Paths remain in argument order, including repeated paths.

The two archived inputs each report 18 passed tests and 4 passed requirements.
The application emits a TSV header and one row per file, with actual counts and
an `ok`, `failed` or `mismatch` status. Backslash, TAB, LF and CR in paths are
escaped so every row remains one TSV line.

Exit statuses are 0 for valid logs without failures, 1 for a test/requirement
failure or summary mismatch, 2 for usage or malformed logs, and 3 for file-read
or UTF-8 decoding errors. SKIP does not itself cause failure. A hard error in any
file suppresses the whole stdout table. Previously emitted mismatch diagnostics
remain on stderr, followed by the hard-error diagnostic.

## Source structure

| Module | Responsibility |
| --- | --- |
| `main.yv` | Validate arguments, read files, retain successful rows, select exit status |
| `parser.yv` | Validate physical lines, ASCII separators, identifiers, records, counts and duplicates |
| `stats.yv` | Count actual records and compare declared totals |
| `report.yv` | Escape paths, render TSV, and emit stable diagnostic codes |

All work occurs in `execute`, which returns before `main` invokes `os.exit` for
a nonzero result. Parsing and reporting do not terminate the process, so their
owned inputs, containers and error values leave scope on ordinary return paths.
Input is read as a whole UTF-8 file. Its supported format is the archived
verification-log grammar in SPEC.md.

## Development evidence

[evidence/DEVELOPMENT.md](evidence/DEVELOPMENT.md) records the initial authoring
and compile sequence. Each complete-source compiler invocation is preserved in
`evidence/compile-attempts/` with the four source snapshots, command, diagnostics,
exit status, elapsed time and source/compiler/runtime/toolchain identities.
Provider token usage and cost were unavailable and remain unknown.

The author's real-input smoke result is
`evidence/real-input-smoke.json`. Independent contract tests, platform execution
and reproducibility evidence are separate from the authoring check; consult
their final receipts for the tested source identity and scope.
