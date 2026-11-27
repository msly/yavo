# Selfhost check JSON diagnostics, batch 50

Baseline: db3b4716fe9bea730e544f09b19325c37508a721. This contract is frozen
before executing the new JSON implementation. Existing text diagnostics are
observed only from the immutable batch 49 stage2; expected byte ranges and
snapshot identities are authored and checked independently from input bytes.

## Command boundary

Accept `check INPUT --json` and `check --json INPUT`. The spelling
`--json-diagnostics` is an exact alias in either position. Exactly one mode flag
and one input are permitted. Duplicate or mixed mode flags, extra inputs and
other options return 2 with empty stdout and a usage diagnostic on stderr.
The JSON flags remain rejected by build/run/emit-c. Existing valid text check
and emit-c calls preserve their previous output and exit semantics.

Root input read failure, including missing input, directory input or invalid
UTF-8, returns 3 with empty stdout and `cannot read input` plus LF on stderr.
No source identity is invented when the root was not successfully read as UTF-8.
JSON mode never invokes a backend and creates no output source or binary.

## Structured output

A successfully read source is checked using the existing strict subset and
first-error policy. Exit 0 means no reported diagnostic; exit 1 means one error.
Stdout is exactly one compact UTF-8 JSON object followed by one LF; stderr is
empty. Whitespace/key order and equivalent Unicode escaping inside JSON do not
change meaning, but duplicate keys, nonstandard numeric constants, extra fields,
extra objects and wrong JSON value types are rejected by the independent gate.

The exact top-level fields are:

- `schema`: `yavo-diagnostics/p0`.
- `frontend`: `selfhost`.
- `diagnostic_mode`: `first-error`.
- `file`: the entry path spelling passed to the compiler.
- `source_sha256`: lowercase SHA256 of the successfully read root UTF-8 bytes.
- `sources`: ordered records `{path, bytes, sha256}` of successfully read source
  snapshots, root first and then the existing breadth-first module-load order.
- `error_count`: integer 0 or 1; `warning_count`: integer 0.
- `diagnostics`: empty on success, otherwise exactly one record.

Each diagnostic has exactly `severity` (`ERROR`), `code`, `message`, `file`,
`line`, `column`, `end_line`, `end_column`, `primary`, `notes` and `fixes`.
`primary` has exactly `file`, `start`, `end`, `line`, `column`, `end_line` and
`end_column`. Its duplicated outer location fields agree. `notes` and `fixes`
are empty arrays in this batch. Existing diagnostic codes and messages remain
the same as text mode, including U0001 for unsupported language features.

The common field names match stage0's actual p0 output. This first-error frontend
does not claim stage0's multiple-error recovery, warning, note, secondary-location
or fix support. It does not emit a project_sha256 field. A sources array is the
loaded prefix at the moment checking stops, not a claim to have loaded an entire
failed project. Successfully read but lexically invalid text is included; a
missing or invalid-UTF-8 module is excluded. Such a module failure is E0010 at
the importer's alias token. Paths preserve existing lexical display spelling;
no new canonicalization or filesystem reread is used for diagnostic rendering.

## Locations and first-error ordering

`start`/`end` are local-file UTF-8 byte offsets with a half-open interval. Lines
and columns start at 1; columns count Unicode scalar values. LF starts the next
line; CR and TAB each advance one scalar column. Bounds are valid UTF-8 scalar
boundaries in the captured source. EOF and synthetic terminal newline can be
zero width. For parser/type/ownership/codegen errors, the range is the original
primary token's complete raw range, including quotes/escapes for a Text token;
it is not an inferred whole-expression or whole-statement range.

No first-error selection is changed: the lexer finishes the current file before
parsing it; each file is parsed before breadth-first loading of its dependencies;
global type/codegen checks follow loading. Deferred interpolation lexical errors
retain their established precedence relative to a later outer lexical failure.
An error earlier in the source text is not necessarily the first phase to fail.

Lexical failures that do not form a normal token use these exact end rules,
without changing their existing start or consumption/first-error behavior:

1. An unexpected character covers its complete Unicode scalar. A forbidden block
   comment covers the two-byte `/*` opener.
2. An unsupported/invalid numeric candidate begins at the existing digit start.
   Continue through ASCII letters, digits and underscore; also through a dot
   immediately followed by a digit. Include a plus/minus only when the already
   scanned prefix matches `[0-9_]+(?:\.[0-9_]+)?[eE]` and the next byte is a digit.
   Thus `1e-2+3` covers `1e-2`, `0xFE+2` covers `0xFE`, `12abcE+3` covers
   `12abcE`, and `1..3` does not consume the range operator.
3. An unknown string escape covers its backslash and the consumed escape scalar.
   An invalid Unicode escape covers the consumed prefix and the first detected
   invalid scalar or closing `}` when present; EOF ends the range. This includes
   `\u1` from `\u1234`, `\u{}` and `\u{D800}`.
4. An unterminated string or interpolation ends immediately before the LF where
   termination is detected, or at EOF. The existing opening position is kept.
5. An invalid byte literal ends at its consumed prefix, including a closing quote
   if consumed. This can span real line breaks. It does not scan additional input
   to invent a completed token. For example, `'ab'` covers `'a`, while a literal
   containing quote/LF/quote covers all three bytes.
6. Exceeding the interpolation nesting limit covers the new opening quote.

## Snapshot lifetime and adversarial cases

All hashes, byte counts and line/column computations refer to the bytes loaded
for this compilation. The compiler must not reopen a file to construct a later
diagnostic. POSIX cases use an imported FIFO as a deterministic load barrier:
the checker has already loaded root/prior modules before a writer can open the
FIFO; the harness then changes an earlier file and only afterward supplies the
last module. Diagnostics must retain the original file's range and identity.
These cases are explicitly excluded on Windows; no platform exclusion is a pass.

The frozen oracle computes hashes and positions with Python hashlib and Unicode
decoding, separately from the implementation. It compares exact schema, JSON
types, values, source order, stdout/stderr and exits. Additional negative controls
must prove that corrupted ranges, hashes, source order, key inventory, duplicate
keys and extra stdout are rejected. Formal execution uses the actual stage1 and
stage2 bound to a complete successful bootstrap receipt, with all source,
contract, fixture, oracle, harness and compiler identities rechecked afterward.
