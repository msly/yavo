# Frozen dependency-index acceptance inputs

`cases.json` contains 113 manually specified outcomes from `../SPEC.md`:

| Group | Cases |
| --- | ---: |
| Valid declaration format | 11 |
| DAG ordering and dependency counts | 11 |
| Declaration syntax | 27 |
| UTF-8, BOM, controls and line endings | 15 |
| Duplicate, missing and cyclic graphs | 12 |
| Validation-phase precedence | 11 |
| CLI arguments | 10 |
| Filesystem and operand spellings | 16 |

The expected rows, diagnostic codes/paths/physical lines and exits were written
as literals. No expected result was produced by the application or oracle.
`oracle.py` independently parses UTF-8-validated byte lines with an ASCII grammar,
retains first dependency occurrences, and performs min-heap Kahn elimination.
The first complete comparison found **0 conflicts across 113 cases**; no manual
expectation or oracle correction was needed to make that comparison pass.
`oracle-freeze.json` binds the SPEC, cases and oracle by SHA-256 before the formal
application gate. Its application-access flags describe the isolated oracle
author, who did not read or execute the dependency-index implementation or read
its outputs.

## Fixture and oracle interface

Each case has `id`, `group`, `args`, `files`, `stdin` and `expected`. File payloads
use UTF-8 `text` or exact `base64`; `directory: true` models a failed file read.
Absent paths also fail. `@abs:` names are mapped by the gate into an isolated
absolute directory without changing the suffix. Sources not named in `args`
exist in the fixture but cannot satisfy a module dependency.

`evaluate(argv, contents, stdin=b"", stdin_error=False)` returns
`{exit, rows, diagnostics, help}`. Successful rows are
`[module_name, original_operand_spelling, unique_direct_dependency_count]`.
The path occupies row index **1**. Failures have `rows: null` and exactly one
diagnostic; help has `rows: null`, no diagnostic and `help: true`. The oracle
ignores stdin because the contract has no stdin mode. Explanations are not frozen;
the gate verifies their presence while comparing all frozen diagnostic fields.

## Platform applicability

112 cases apply on POSIX systems and 107 on Windows. Six POSIX-only cases cover
TAB, LF, CR, literal backslash and escaped source/read diagnostics in path names.
The Windows-only case uses a backslash directory separator. Other cases include
non-ASCII paths, spaces, original `./` spelling, absolute paths, path aliases,
directory operands and missing files. Platform filtering controls execution;
every one of the 113 structural expectations is checked against the oracle.

Invalid UTF-8 file contents are covered by six explicit byte payloads. Invalid
UTF-8 argv byte strings have no frozen behavior in SPEC v1 and are not represented
as portable JSON path strings here. The files make no claims about OS-specific
permission-denial behavior or filesystem Unicode normalization beyond preserving
the original operand spelling in output.
