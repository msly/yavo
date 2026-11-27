# yavo-dep-index: application contract v1

Batch 48, 2026-11-27. The third independent SH-4 application indexes explicit
UTF-8 module declaration files, validates cross-file references, and emits a
deterministic dependency-first build order. Freeze this contract before the
application and independent oracle are executed. Oracle expectations must not
come from the Yavo implementation.

## Command and exit status

```
yavo-dep-index [--] FILE...
yavo-dep-index --help
```

At least one file operand is required. There is no stdin mode or recursive
discovery. Before `--`, any argument beginning with `-` is an unknown option;
`--` ends option processing. `--help` alone prints exactly
`Usage: yavo-dep-index [--] FILE...` followed by LF and exits 0. Combined with
other arguments it is a usage error unless used as a filename after `--`.
An empty operand is a path whose read fails. Repeated paths are allowed and will
normally produce duplicate module declarations. Validate all arguments before
reading files. `--` alone is a usage error.

| Exit | Meaning |
| --- | --- |
| 0 | All files and the dependency graph validate, or help was requested |
| 1 | Declaration syntax, duplicate module, missing dependency or cycle error |
| 2 | Invalid command arguments |
| 3 | File read or UTF-8 decoding failure |

On any failure write exactly one nonempty diagnostic to stderr, with the extra
usage line only for usage errors, and no stdout. All owned values and error
payloads leave scope through ordinary returns before the final process exit.
Parsing and graph routines must not call `os.exit`.

## Declaration format

Read each complete file as UTF-8 before parsing it. Invalid UTF-8 anywhere in
that file takes priority over its syntax errors. Files consist of physical lines
separated by LF or CRLF; mixed endings and a final line without a terminator are
allowed. A terminal ending does not add a meaningful line. Bare CR is invalid.
An initial UTF-8 BOM is invalid. ASCII control bytes other than TAB and line
endings, and DEL, are invalid even in comments. Other Unicode is allowed in
comments; names and directive words remain ASCII.

Strip leading/trailing ASCII space and TAB from each line. Empty lines and lines
whose first remaining byte is `#` are ignored. Comments occupy the whole line;
there are no inline comments. Every other line is exactly one of:

```
module NAME
use NAME
```

There is at least one space or TAB between the word and name, no other tokens,
and the words are case sensitive. A NAME is a nonempty dot-separated sequence
of identifiers. Each identifier starts with ASCII A-Z, a-z or underscore and
continues with those bytes or ASCII 0-9. Empty segments are invalid. No case
folding, Unicode normalization or path interpretation occurs. `core.IO` and
`core.io` are different module names.

Exactly one `module` directive is required and must be the first non-comment
directive. Another `module` directive in the same file, a `use` before it, an
unknown word, or an invalid name is a syntax error. A file containing only blank
lines/comments is a missing-module error. Each `use NAME` declares a direct
dependency. Repeated identical `use` names in one file are allowed and count
once, retaining the first occurrence for diagnostics. Self-dependencies are
allowed syntactically and are detected as cycles in graph validation.

## Validation phases and error precedence

1. Validate the entire argument list.
2. Read and parse each file in operand order, stopping at the first read or
   syntax failure. Within one decoded file check lines from first to last;
   invalid control/BOM/CR on a line precedes directive interpretation on that
   line. Missing-module is checked after all of that file's lines.
3. Check module-name uniqueness in operand order. The later declaration is
   the duplicate error, regardless of whether the paths are identical.
4. Resolve dependencies in operand order, then first-use order inside each
   file. The first unresolved name is the missing-dependency error.
5. Compute a dependency-first topological order: among all un-emitted modules
   whose dependencies have already been emitted, choose the ASCII
   lexicographically smallest module name. Repeat until all modules emit. If
   there is no ready module, report a cycle at the declaration of the ASCII
   lexicographically smallest remaining module. This selected unresolved module
   need not itself be a member of the cycle.

The complete file set is validated even when an independent module could have
been emitted earlier. A module is present only if an explicit operand declares
it. The tool never guesses paths or loads additional files from dependency names.

## Output and diagnostics

On success write this UTF-8 TSV header and one row per module, in the computed
order, with LF line endings:

```
module\tfile\tdependencies
```

The columns are the declared module name, original operand spelling, and the
number of unique direct dependencies as a nonnegative decimal integer. Escape
backslash as `\\`, TAB as `\t`, LF as `\n`, and CR as `\r` in paths; leave other
characters unchanged. Successful operation writes no stderr. Counts use signed
64-bit integers; runtime input-size bounds constrain their range.

| Code | Condition and location |
| --- | --- |
| ECLI_USAGE | Argument error; no source location |
| EIO_READ | Read or UTF-8 decoding failure; no source location |
| EDEP_FORMAT | Invalid bytes/directive/name/order; offending physical line, column 1 |
| EDEP_MODULE | File has no module directive; line 1, column 1 |
| EDEP_DUPLICATE | Later declaration of an already declared module; its module line, column 1 |
| EDEP_MISSING | First unresolved use; its first occurrence line, column 1 |
| EDEP_CYCLE | Smallest remaining unresolved module; its module line, column 1 |

Source diagnostics are `PATH:LINE:1: CODE: explanation` plus LF. Lines are 1-based
physical lines; CRLF increments once. Paths use the same escaping as output.
Read diagnostics are `PATH: EIO_READ: explanation` plus LF. Usage diagnostics are
`ECLI_USAGE: explanation` plus LF followed by the exact usage line. Explanation
wording is not frozen but must be nonempty. Codes, paths, locations, precedence,
output bytes and exits are frozen.

## Application acceptance and scope

Implement as multiple Yavo source modules with separate parsing, graph handling,
reporting and CLI entry responsibilities. First build a natural implementation
with stage0, then preserve its source when identifying selfhost limitations.
Independently frozen manual fixtures and an oracle cover valid DAGs, dependency
ordering, conflicts, missing files/references, cycles, syntax, UTF-8, path
escaping, platform-specific filesystem cases and all failure exits. Check
Debug/Release agreement, cleanup on both success and failure, independent
rebuilds in directories containing spaces/non-ASCII text, and both frontends.
Linux must run independent ASan/UBSan controls and both producers' generated C;
Linux, Windows and macOS must execute the native application gate. Record build
timings, memory and artifact sizes without claiming single observations as P50.

The input is this explicit declaration format, not arbitrary Yavo source. This
version reads whole files; it has no package download, command execution,
incremental persistence, automatic directory traversal or filesystem sandbox.
