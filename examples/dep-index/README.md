# yavo-dep-index

A five-module Yavo CLI for validating an explicit set of module declarations
and producing a deterministic dependency-first index. This is the third
independently authored SH-4 application. The [contract](SPEC.md),
[manual cases and independent oracle](tests/README.md), and
[application development record](evidence/DEVELOPMENT.md) have separate roles.

## Build and use

From the repository root with the locked toolchain installed:

```sh
cargo build --manifest-path compiler/Cargo.toml --locked --release
compiler/target/release/yavo build examples/dep-index/main.yv --release -o yavo-dep-index
./yavo-dep-index core.dep app.dep
```

An input file declares exactly one module and zero or more dependencies:

```text
# app.dep
module app
use core
```

`core.dep` can contain `module core`. The output is TSV with columns
`module`, `file`, and `dependencies`; `core` precedes `app`. Among modules
whose dependencies are ready, the smallest ASCII module name is emitted first.
Repeated uses count once and keep the first source location. Every input and
the complete graph must validate before anything is written to stdout.

Names are dotted ASCII identifiers. Files are decoded as UTF-8 before parsing;
LF and CRLF, blank lines, whole-line comments, and a final unterminated line
are supported. Conflicts, missing dependencies and cycles have deterministic
error precedence and source locations. Paths in output and diagnostics escape
backslash, TAB, LF and CR. `--` allows operands starting with a hyphen;
`--help` is supported only by itself.

| Exit | Meaning |
| --- | --- |
| 0 | Valid index or help |
| 1 | Declaration syntax, duplicate module, missing dependency or cycle |
| 2 | Invalid arguments |
| 3 | File read or UTF-8 decoding failure |

The format is an explicit declaration language, not a Yavo-source parser.
This version reads whole files and does not discover directories, infer input
paths, execute build commands, download packages, or persist an incremental
index. A cycle diagnostic selects the smallest unresolved module, which can
be downstream of the cycle; it does not claim to list a cycle path.

## Implementation and source freeze

The application contains **309 lines in five Yavo modules**:

| Module | Responsibility |
| --- | --- |
| `main.yv` | Validate arguments, read all explicit operands, return exit status |
| `parser.yv` | Physical lines, ASCII names, first-use deduplication |
| `graph.yv` | Name lookup, reverse adjacency, deterministic topological order |
| `report.yv` | Escaped paths, atomic TSV rendering and diagnostics |
| `model.yv` | Owned declarations, dependencies and error payloads |

The natural stage0 implementation uses `Map[String, int]`, `Set[String]`,
`Vec[Vec[int]]`, `Heap[(String, int)]`, owned tuples, `while let`, and read-only
slices of owned records. Its author did not inspect selfhost capabilities or
the independent expected outputs. Two complete stage0 checks and two complete
builds were recorded; the only correction joined `else` to the preceding
closing brace. All five source files then froze at project SHA-256
`a48f8f8dc9879fa5101efb67d72232a91f61c23a575edaa983fa4f6992d84f64`.

The batch-47 compiler first rejects `std/collections` in `graph.yv`.
Batch 48 extends selfhost for the unchanged application. Application-owned
values and caught errors return normally through `run` before `main` calls
`os.exit`; parser and graph code do not terminate the process.

## Acceptance

The independent oracle and **113 manual contract cases** were frozen before
the application was validated. All manual expectations agree with the oracle.
There are 112 applicable POSIX cases and 107 applicable Windows cases; fixture
names that a platform cannot represent are explicitly not applicable.

```sh
python3 tools/ci/dep_support_test.py
python3 tools/ci/dep_check.py --oracle-only
python3 selfhost/bootstrap.py
python3 tools/ci/dep_check.py --bootstrap-output compiler/target/ci/selfhost --sanitizers required
```

The last command requires Linux with an independent GCC sanitizer toolchain.
Other platforms use `--sanitizers off`; a missing sanitizer compiler is never
reported as a sanitizer pass. The bootstrap gate executes stage0 and actual
final-stage2 Debug, Release and checked-exit artifacts: six per scenario.
The independent native gate executes Debug, Release and checked-exit artifacts,
performs an independent Release rebuild, binds input/artifact identities and
checks cache cleanup. Linux ASan+UBSan additionally compiles both producers'
actual generated C after validating positive and negative controls.

### Local native observations, 2026-11-27

The independent native gate passes **336/336 executions**, with no failures.
Two independent Release builds produce identical generated C and binaries:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Generated C | 62,982 | `67dcce47b79d4a255d95381ef324c6b7f6c27273ee3c480caea35a7550fbaef8` |
| Linux x86-64 Release executable | 39,328 | `fb0d4f5a330cf6865b6b141faa98e05482ba3f765cf9b8ab941d87109d4746fc` |

| Full build mode | Cold | Unchanged | Comment edit |
| --- | ---: | ---: | ---: |
| Debug | 20.316263 s | 0.017942 s | 0.282410 s |
| Release | 5.109620 s | 0.008643 s | 1.022913 s |

These are single wall-clock observations, with miss/hit/miss cache conditions,
not P50 or P95. All 353 recorded commands have Linux `wait4` RSS observations;
the maximum is 425,816,064 bytes. This is the maximum reported child RSS, not
a simultaneous process-tree total. Windows leaves unavailable RSS values null.
The preinstalled locked Zig toolchain, spaces/non-ASCII build paths and empty
independent caches are part of this gate. Local GCC is unavailable, so the local
sanitizer result is `unavailable`, with zero sanitizer executions.

Local receipt: `compiler/target/ci/dep-index-batch48-native/receipt.json`.
Token usage and monetary cost are unmeasured; this application does not establish
the separate relative-delivery-cost target.



### Complete selfhost and platform acceptance

The final full bootstrap passes **1,027/1,027 cases, 19/19 checks, zero skips**.
This application's actual final-stage2 and stage0 artifacts pass **672/672**
executions on the local Linux host; all bound sources and artifacts stay unchanged.
Implementation `cc78059` passes **6/6 jobs** in [run 45955439200](https://github.com/msly/yavo/actions/runs/45955439200); all three native gates and Linux independent dual-producer C sanitizers pass.

CI executes **672 bootstrap / 336 native** application cases on each of Linux
and macOS, and **642 / 321** on Windows. Linux independently executes
**224 ASan+UBSan runs** against stage0 and final-stage2 C, with required
positive and negative controls. Platform-inapplicable filename cases retain
explicit not-applicable status. The CI job/step evidence is recorded in the
[validation index](../../compiler/validation/batch48-dep-index-2026-11-27.json).
