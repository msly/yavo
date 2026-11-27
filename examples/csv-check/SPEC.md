# yavo-csv-check: application contract v1

Batch 47, 2026-11-27. This is the second independent SH-4 application: a
multi-module validator for UTF-8 CSV tables exchanged between tools. The contract
is fixed before application execution; independent fixtures and a Python oracle
must not obtain their expected results from the Yavo implementation.

## Command and exit status

```
yavo-csv-check [--] [FILE...]
yavo-csv-check --help
```

With no operands, read stdin once and label it `-`. An operand equal to `-` also
means stdin, including after `--`; it may occur at most once. Other operands are
file paths, processed in argument order. Repeated file paths are allowed. Before
`--`, any other argument beginning with `-` is an unknown option. `--` ends option
processing; a literal file named `-` is addressed as `./-`. `--help` alone prints
`Usage: yavo-csv-check [--] [FILE...]` followed by LF and exits 0. Combined with any
other argument, it is a usage error unless it occurs as a filename after `--`.
An empty path is a file operand whose read fails. Validate the entire argument
list before reading inputs. `--` alone reads stdin.

| Exit | Meaning |
| --- | --- |
| 0 | Every input is a valid table, or help was requested |
| 1 | CSV syntax, header or row-width error |
| 2 | Invalid command arguments |
| 3 | File/stdin read or UTF-8 decoding failure |

Stop at the first failed input in argument order. Emit exactly one diagnostic on
stderr and no stdout table on any failure, even if preceding inputs were valid.
Input values, result rows and error payloads must leave scope through ordinary
returns before the final process exit. Parsers and validators must not call
`os.exit`.

## CSV profile

This profile follows the comma, double-quote and doubled-quote conventions in
[RFC 4180 section 2](https://www.rfc-editor.org/rfc/rfc4180#section-2), with the
explicit application choices below. It is not an autodetector for CSV dialects.

- Input is valid UTF-8. Invalid UTF-8 is a read/decoding failure, before CSV
  validation of that input. The whole input is read before parsing.
- A table contains one required header record followed by zero or more data
  records. An empty byte stream is an error; a header-only table is valid.
- Fields are separated by ASCII comma. A record contains at least one field.
  Empty fields are allowed in data records, including a final empty field after
  a trailing comma. A blank physical line is one record with one empty field.
- Records end at LF or CRLF outside quoted fields. Mixed LF/CRLF is allowed.
  The last record may omit its line ending. One terminal line ending completes
  the preceding record and does not add another record; a second line ending
  creates an empty record. A bare CR is invalid everywhere, including in quotes.
- An unquoted field contains any permitted text except comma, double quote, CR
  and LF. A quote may only open at the first byte of a field.
- A quoted field starts with `"`. Inside it, `""` decodes to one literal quote;
  comma and LF/CRLF are field data. Line ending bytes inside a quoted field are
  preserved. A closing quote must be followed immediately by comma, a record
  ending or EOF. Spaces after a closing quote are invalid. An unclosed quoted
  field is an error at EOF.
- ASCII TAB and space are ordinary field data and are never trimmed. Other
  ASCII control bytes U+0000..U+001F, except TAB/CR/LF as specified above, and DEL
  U+007F are invalid in every field. Non-ASCII Unicode characters, including
  Unicode whitespace, are ordinary field data.
- An initial UTF-8 BOM is rejected at line 1, column 1. U+FEFF elsewhere is
  ordinary field data. No Unicode normalization or case folding is performed.

## Structural validation

After decoding the header, each header field must have nonzero length and be
unique by exact decoded UTF-8 bytes. A field consisting of spaces or TAB is
nonempty. `Name` and `name` are distinct. Quoted and unquoted spellings that
decode to the same value are duplicates. Check header fields from left to right,
reporting the first empty field or repeated value encountered.

Each data record must have the same number of fields as the header. Field
contents are opaque text: signs, decimal limits, very large numbers and empty
values are not interpreted as numbers. Counts in output are nonnegative signed
64-bit integers; input sizes supported by the runtime bound their values.

Complete and validate one record before moving to the next. Within a record,
the first lexical error takes priority over header/width checks. After a record
parses, its structural error takes priority over all later records. Invalid
UTF-8 anywhere in that input takes priority over all CSV errors because decoding
precedes parsing. Usage validation precedes all reads.

## Output and diagnostics

Only after every input validates, stdout is UTF-8 TSV with LF endings:

```
file\tcolumns\trows\tbytes
```

Emit one row per operand in argument order; implicit stdin supplies one row.
`columns` is the header field count, `rows` counts data records only, and `bytes`
is the original input byte length, including line endings and quotes. Preserve
the supplied file path spelling, or `-` for stdin. In the path field, escape
backslash as `\\`, TAB as `\t`, LF as `\n` and CR as `\r`; all other text is
unchanged. Successful validation writes nothing to stderr.

| Code | Condition and location |
| --- | --- |
| ECLI_USAGE | Unknown option, invalid help use or repeated stdin operand |
| EIO_READ | File/stdin read or UTF-8 decoding failure; no source location |
| ECSV_EMPTY | Empty input, at 1:1 |
| ECSV_FORMAT | Invalid control/BOM/quote/CR or unterminated quoted field |
| ECSV_HEADER | Empty or duplicate decoded header name, at the header record's start |
| ECSV_WIDTH | Data field count differs from header, at the data record's start |

CSV diagnostics have the form `PATH:LINE:COLUMN: CODE: explanation` followed by
LF. Lines and columns are 1-based; columns count UTF-8 bytes, not characters.
A CRLF is one physical line ending; the byte after LF is column 1 on the next
line. LF in a quoted field also advances the physical line. For quote/control
errors use the offending byte; for bare CR use the CR byte; for an unclosed quote
use the position immediately after the last input byte. Header and width
diagnostics use the record's starting physical line and column 1, even when its
fields span multiple lines. The initial BOM is the sole multi-byte lexical
special case and is reported at its first byte.

I/O diagnostics are `PATH: EIO_READ: explanation` followed by LF. Usage failures
write `ECLI_USAGE: explanation` followed by LF and then the exact usage line.
Escape paths as in TSV. Explanation wording is not frozen; codes, locations,
order, output bytes and exit status are frozen. The explanation must be nonempty.
Filename tests containing actual TAB/LF/CR apply on hosts that allow them; all
platforms test escaping those supplied paths in I/O diagnostics.

## Application acceptance

- Use at least four source modules separating CLI/I/O, CSV parsing, structural
  validation and output. Preserve this contract if compiler defects require
  fixes. Do not weaken inputs or rewrite historical programs for selfhost.
- Freeze manual expected cases and an independent oracle before executing the
  application against them. Cover files and stdin, multiple/repeated inputs,
  valid and invalid quoting, multiline fields, decoded header duplicates, empty
  values, row widths, newline variants, byte columns, non-ASCII paths and data,
  large opaque numeric fields, invalid UTF-8 and I/O errors.
- Record every complete application compile attempt from the first complete
  source, including failures, command/output/source identities and elapsed time.
  Provider tokens and cost remain unknown unless actually measured.
- Compare stage0 and the selfhost-generated C in Debug and Release using every
  applicable fixed case, including failure paths. All owned values must be
  released on both successful and failed execution. Verify this with a tracked
  allocation exit helper and positive/negative controls; use independent ASan
  and UBSan on Linux with their own capability controls.
- Validate on native Linux x86-64, Windows x86-64 and macOS arm64. Bind source,
  specification, oracle, compiler, runtime and locked toolchain identities into
  receipts. A platform skip is not a pass.
- Build from two independent directories and empty caches with the same relative
  source layout/output basename and compare generated C and executable bytes
  within each platform. Disable automatic toolchain downloading for offline
  runs. Include paths with spaces and non-ASCII text. Publish clean-checkout
  reproduction commands, build/run timings, peak process memory when observable,
  and artifact sizes. Distinguish cold, unchanged and edited-source builds.

This application reads each input into memory. Streaming, configurable dialects,
schema inference, type conversion, network fetching and spreadsheet formula
interpretation are outside this contract.
