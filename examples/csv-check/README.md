# yavo-csv-check

A four-module command-line validator for UTF-8 CSV tables. It checks CSV quoting,
nonempty unique header names, and a consistent field count in every data record.
The exact format, diagnostic locations, error precedence and acceptance contract
are in [SPEC.md](SPEC.md).

## Build and use

From the repository root, with stage0 and the locked Zig toolchain installed:

```sh
mkdir -p compiler/target/csv-check-app
compiler/target/release/yavo build examples/csv-check/main.yv --release \
  -o compiler/target/csv-check-app/yavo-csv-check
compiler/target/csv-check-app/yavo-csv-check data.csv
printf 'name,value\nalpha,3\n' | compiler/target/csv-check-app/yavo-csv-check
```

On Windows, use the corresponding `.exe` names. With no operands, the program
reads stdin once. An explicit `-` also means stdin and may appear only once.
Other operands are paths, processed in the supplied order; repeated paths are
allowed. `--` ends option processing, allowing paths that begin with `-`.
A literal file named `-` can be passed as `./-`. `--help` alone prints usage.

After every input validates, stdout is TSV with this header and one row per
input:

```text
file	columns	rows	bytes
```

The stdin example above emits `-`, `2`, `1`, and `19` as its four fields.
`rows` excludes the header; `bytes` includes the original quoting and line-ending
bytes. Paths keep their supplied spelling, with backslash, TAB, LF and CR escaped.

| Exit | Meaning |
| --- | --- |
| 0 | Every input is valid, or help was requested |
| 1 | CSV syntax, header or row-width error |
| 2 | Invalid command arguments |
| 3 | File/stdin read or UTF-8 decoding failure |

Any failure suppresses the entire stdout table and emits one diagnostic to
stderr. CSV diagnostics include a stable code and 1-based physical line and
UTF-8 byte column. Header and width errors point to the record's starting line,
column 1. The complete argument list is checked before any input is read.

## Supported CSV profile

Fields use comma separators and optional double quotes. Doubled quotes inside
a quoted field decode to a literal quote; quoted fields may contain commas,
LF and CRLF. LF and CRLF record endings may be mixed, and the final record need
not have an ending. Bare CR, malformed quotes, an initial BOM and disallowed
ASCII controls are errors. TAB and spaces remain data. Non-ASCII text is allowed.

Header names are compared after decoding, without trimming, case folding or
Unicode normalization. Every name must be nonempty and unique. Empty data fields
are allowed. A blank line is a record containing one empty field. Field values
are opaque text: there is no numeric conversion, formula evaluation or schema
inference.

The whole input is read and decoded as UTF-8 before CSV validation. The parser
finishes one record, then its header or width is checked before parsing the next
record. Within a record, the first lexical error wins over structural errors;
an earlier record's structural error wins over errors in later records.

## Source structure and cleanup

| Module | Responsibility |
| --- | --- |
| `main.yv` | Validate arguments, read inputs, retain result rows, and select exit status |
| `parser.yv` | Decode one CSV record, preserving quoted newline bytes and physical byte positions |
| `validator.yv` | Check decoded header names and each data record's field count |
| `report.yv` | Escape supplied paths, render TSV and stable diagnostics |

`execute` returns before `main` calls `os.exit` for a failure. Parser and validator
errors propagate through ordinary returns. Inputs, decoded fields, header names,
result rows and error payloads therefore leave their scopes before final process
exit. The parser retains one decoded record at a time; each input file is still
loaded in full. The validator keeps the header and the current data record.
Streaming and configurable CSV dialects are outside this contract.

## Clean checkout and independent acceptance

With the repository's pinned Rust toolchain and Python installed, prepare stage0
and the SHA-256-locked Zig toolchain from a clean checkout:

```sh
cargo build --manifest-path compiler/Cargo.toml --locked --release
compiler/target/release/yavo toolchain install
python3 selfhost/bootstrap.py
python3 tools/ci/csv_check.py --bootstrap-output compiler/target/ci/selfhost --sanitizers auto
```

On Windows use `python` and the `.exe` compiler name. Once Cargo dependencies and
Zig are installed, the compiler can be rebuilt with `--locked --offline`; the
application acceptance driver never invokes toolchain installation. It requires
the installed version from `compiler/toolchain.lock`, records its binary hash,
and rejects a `YAVO_CC` override.

The independent oracle has 139 fixed scenarios: 138 apply on Linux/macOS and 137
on Windows. [The test inventory](tests/README.md) records the platform-specific
stdin and filename cases; not-applicable cases are not counted as passes.
Bootstrap uses the final stage2 compiler after its fixed-point checks and runs
every applicable case through six artifacts: ordinary stage0/selfhost Debug and
Release, plus a supplementary Release exit-cleanup artifact from each frontend.
It compares exact stdout, stderr and exit status with the frozen expectations and
between artifacts. The expected inventory is 828 executions on POSIX or 822 on
Windows. The separate native driver runs stage0 Debug, Release and a cleanup
helper, compares two independent Release builds' C and binary bytes, and records
cold, unchanged and comment-edited builds separately in both modes. Build
directories and fixture paths include spaces and non-ASCII text.

Every native job requires clean/leak controls for the tracked-allocation exit
helper: the clean program preserves exit 2 and the leaked String is detected
with exit 102. The test-only shim leaves production C and runtime unchanged.
Linux CI additionally uses `--sanitizers required`: its independent GCC gate
must pass clean, ASan use-after-free and UBSan signed-overflow capability controls,
then test both stage0 C and CSV C freshly emitted by the final stage2 identified
and verified from a successful bootstrap receipt. `auto` reports missing GCC
support as unavailable; that is not a sanitizer pass. Other native platforms use
`--sanitizers off`, recorded as not applicable. The separate general selfhost
sanitizer gate does not provide this application's coverage.

Receipts, raw command/stdin/stdout/stderr records, source identities and build
measurements are written under `compiler/target/ci/csv-check/` and the selected
bootstrap output directory. Linux/macOS commands report the OS `wait4` resource
usage's `ru_maxrss` as bytes: Linux KiB are converted; macOS values are already
bytes. This is a per-command observation for the waited child. The receipt's
overall memory value is the maximum of these observations, not a simultaneous
process-tree total or a sum of command peaks. Windows memory is unavailable/null.
Timing samples are individual observations, not P50/P95 performance claims;
provider token usage and monetary cost remain unknown.

The native driver archives materialized inputs in `fixture-inputs.tar.gz`,
verifies each member's bytes against its execution identity, then removes raw
special filenames and rebuildable caches. The bootstrap wrapper removes its
case input directories while retaining safely named command and stream files.
CI packages the full native evidence as `compiler/target/ci/csv-check.tar.gz` and
excludes the original directory from direct artifact upload.

## Local Linux acceptance, 2026-11-27

The RSS-enabled run at
`compiler/target/ci/csv-check-batch47-rss/attempt-xyp1f13q/receipt.json` passes
**414/414 executions**: 138 cases each in Debug, Release and the supplementary
exit-check artifact. Both independent cold Release builds reproduce identical
C and executable bytes; clean/leak controls, all six cache-condition observations
and source/artifact guards pass. The source copies used for timing edits are
separate from the frozen application and oracle.

| Release artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Generated C | 46,702 | `6d32d6dd002adda1377bd3eba57d22a49f3cca98dbe2a0b10d77123b18c56a27` |
| Executable | 30,456 | `bd53883ea38843a63561a22bae375298462df64b5743117bdc9087aefdcbb996` |

The run records RSS for **431/431 commands**; its maximum reported command value
is **417,312,768 bytes**. The six timing observations use enabled Yavo build and
runtime PCH caches, with the actual hit/miss trace retained. Each condition has
one sample:

| Mode | Source/cache condition | Cache result | Elapsed seconds | Command RSS bytes |
| --- | --- | --- | ---: | ---: |
| Debug | Cold | miss | 22.284 | 411,893,760 |
| Debug | Unchanged | hit | 0.170 | 30,924,800 |
| Debug | Comment-edited copy | miss | 0.486 | 84,553,728 |
| Release | Cold | miss | 4.942 | 219,435,008 |
| Release | Unchanged | hit | 0.020 | 31,305,728 |
| Release | Comment-edited copy | miss | 0.825 | 90,292,224 |

This host has no GCC, so its sanitizer result is `unavailable`, `passed: null`,
with zero sanitizer application executions. Native Linux/Windows/macOS CI and
the required Linux two-producer sanitizer run remain pending. These local results
do not yet establish completion of a second formal SH-4 application.

## Development evidence

[evidence/DEVELOPMENT.md](evidence/DEVELOPMENT.md) records complete-project compiler
attempts, the source freeze and compiler identities. Independent application
acceptance and platform results are recorded separately from these authoring
checks.
