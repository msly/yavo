# yavo-test-summary: application contract v1

Batch 28, 2026-11-23. This is the first SH-4 application, compiled with the
existing Rust stage0. Its purpose is to independently recount archived Yavo/Rust
verification logs and detect inconsistent summaries. It does not claim SH-4
completion, full-language self-hosting, or measured Agent cost superiority.

## Command and exit status

```
yavo-test-summary [--] LOG...
yavo-test-summary --help
```

At least one file is required. `--help` alone prints the usage line
`Usage: yavo-test-summary [--] LOG...` followed by LF and exits 0. Before `--`,
arguments beginning with `-` are unknown options (exit 2); `--help` combined
with other arguments is also a usage error. `--` ends option processing and
is not a filename. After it, filenames beginning with `-` are allowed.
Input files are processed in argument order; repeated path arguments are allowed
and yield repeated rows. No directory traversal, stdin, JSON parser or external
process execution is required.

| Exit | Meaning |
| --- | --- |
| 0 | Every input is valid and consistent, with no test or requirement failure; SKIP is allowed |
| 1 | At least one valid input contains failures or an inconsistent summary |
| 2 | Usage or log-format error |
| 3 | File read or UTF-8 decoding failure |

On a format/read error, stop at the first such file in argument order and write
no stdout table, even if preceding files were valid. Diagnostic text goes to
stderr. A mismatch is a valid, reportable log: print its row and return 1.
Objects owned during parsing/reporting must leave scope before any final
`os.exit`; do not use abrupt exit as a substitute for cleanup inside parsers.

## Input grammar

Read UTF-8 text. LF and CRLF are accepted. Blank lines are ignored. Lines may
have leading/trailing ASCII space or TAB, and fields are separated by one or
more ASCII spaces/TABs. NUL, other ASCII control bytes, bare CR and Unicode
whitespace used as separators are format errors. Non-ASCII printable text is
allowed in requirement descriptions. A final LF is optional.

For this contract, non-ASCII whitespace is rejected anywhere in a nonblank
line: U+0085, U+00A0, U+1680, U+2000..U+200A, U+2028, U+2029, U+202F, U+205F
and U+3000. DEL (U+007F) is also rejected. A line containing only such characters
is invalid, not a blank line.

Every nonblank line is exactly one of:

1. `CASE_ID PASS`, `CASE_ID FAIL`, or `CASE_ID SKIP`.
2. `REQ CASE_ID DESCRIPTION... ok` or `REQ CASE_ID DESCRIPTION... fail`.
   The description contains at least one field. A requirement is identified
   by CASE_ID plus its description fields joined with single spaces.
3. `verification: N passed, N failed, N skipped; requirement failures: N`.

CASE_ID matches `[A-Za-z0-9_][A-Za-z0-9_.:-]*`. Status keywords and all summary
punctuation are case-sensitive. A case ID may occur only once in test lines;
a requirement identifier may occur only once in requirement lines. Distinct
requirements for the same CASE_ID are allowed. Test and requirement namespaces
are separate. Requirements need not have a corresponding test line.

Keywords are not reserved CASE_IDs: `REQ PASS` and `verification: PASS` are
ordinary two-field test records. Recognize the complete record shape.

Each N is one or more ASCII decimal digits in 0..9223372036854775807; leading
zeros are allowed. Signs, non-digits and overflow are format errors. There must
be exactly one summary line, after all test/requirement lines. Only blank lines
may follow it. An all-zero summary with no preceding records is valid; a blank
file is not. Recount PASS/FAIL/SKIP and failed requirements and compare all four
values with the summary.

## Output and diagnostics

After every input has parsed successfully, stdout is UTF-8 TSV with LF endings:

```
file\tpassed\tfailed\tskipped\trequirement_passed\trequirement_failed\tstatus
```

One row follows per input, in argument order, using the supplied path spelling.
The counts are actual recounted values. Status is `mismatch` if any declared
count differs; otherwise `failed` if any test/requirement failed; otherwise `ok`.
No aggregate TOTAL row is emitted. The path field escapes backslash as `\\`,
TAB as `\t`, LF as `\n`, and CR as `\r`; all other text is unchanged.

Stable stderr codes:

| Code | Condition |
| --- | --- |
| ECLI_USAGE | Invalid command arguments |
| EIO_READ | Read or UTF-8 decoding failure |
| ELOG_FORMAT | Malformed line, ID, count, control character or nonterminal/duplicate summary |
| ELOG_DUPLICATE | Repeated test ID or repeated requirement identifier |
| ELOG_SUMMARY | Missing summary, reported at line count + 1 (empty file: line 1) |
| ELOG_MISMATCH | Recount differs from the summary, reported on its line |

Format, duplicate, missing-summary and mismatch diagnostics include the supplied
path and 1-based physical line number as `PATH:LINE: CODE: explanation`.
I/O errors include `PATH: EIO_READ:`. Escape path control characters/backslashes
as in TSV. Usage errors include `ECLI_USAGE` and the usage line. Human explanation
text is not frozen. A consistent log containing FAIL records does not need a
stderr diagnostic; the TSV row and exit code report it. Print one mismatch
diagnostic per mismatching file in argument order.

Emit a mismatch diagnostic as soon as that file is successfully parsed. If a
later file has a format/read error, earlier mismatch diagnostics remain on
stderr, followed by that hard error; stdout is still empty and the hard error's
exit code wins. Actual TAB/LF/CR filename tests apply only on systems that allow
them; every platform still verifies escaping of such supplied paths in read
failure diagnostics.

Physical lines are split on LF, with one terminal CR removed only when followed
by LF. A trailing LF does not create an extra physical line for missing-summary
diagnostics. A nonempty final segment counts as a line; CR by itself is invalid.

## Application acceptance

- Real inputs: `experiments/coldstart-2026-11-21-r4/evidence/yavo-verification.log`
  and `rust-verification.log` in the same directory.
- Use four source modules: main, parser, stats and report. Preserve the public
  contract even if an existing language limitation requires an internal change.
- Independent tests cover both real logs, success/failure/skip, invalid formats,
  duplicate IDs, false summaries, numeric boundaries, Unicode, CRLF, missing and
  invalid-UTF-8 files, multiple inputs, path escaping, and cleanup.
- Execute on Linux x86-64, Windows x86-64 and macOS arm64; preserve actual source,
  compiler, runtime, toolchain and output identities. Compare Debug and Release.
- Rebuild from independent directories/empty caches with identical relative source
  layouts and output basenames, comparing generated C and binaries per platform.
- Record every application compile attempt from the first complete source,
  including failed attempts and their source snapshots. Keep tokens/cost unknown
  unless real provider usage is available.

Input is loaded as a whole text file. Streaming/large-file scalability and a
general log-format autodetector are outside this application contract.
