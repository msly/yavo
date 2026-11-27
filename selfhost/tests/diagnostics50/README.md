# Independent check JSON acceptance

The batch 50 contract, source fixtures, expected documents and Python oracle were
frozen before the new selfhost JSON implementation was executed or read by the
oracle author. The immutable batch 49 stage2 established the existing text
diagnostic code/message/start for 80 language and snapshot scenarios. Raw source
byte anchors and Unicode coordinates were authored separately; hashlib supplies
independent source hashes. No new frontend output populated expected JSON.

## Inventory

| Gate | Linux / macOS | Windows |
| --- | ---: | ---: |
| Frozen JSON / CLI scenarios per compiler | 98 | 94 |
| Existing text checks per compiler | 80 | 76 |
| Supplemental legacy CLI checks per compiler | 8 | 8 |
| Formal stage1 + actual stage2 JSON / CLI executions | 196 | 188 |
| Formal stage1 + actual stage2 text executions | 160 | 152 |
| Formal stage1 + actual stage2 supplemental executions | 16 | 16 |
| Total formal compiler commands | 372 | 356 |
| Independent oracle negative controls | 10 | 10 |

Windows exclusions are two POSIX-only adversarial filename cases and two FIFO
snapshot-barrier cases. Their exclusion is explicit and is never counted as a
successful execution. All fixture storage paths are Windows-checkout-safe;
logical adversarial names are materialized only on applicable hosts.

The cases cover the two JSON option spellings in both positions, success and
first-error behavior, CLI/read errors, lexer/parser/type/ownership/U0001/import
diagnostics, UTF-8 scalar columns, CRLF, raw string/escape ranges, zero-width EOF,
multiline invalid byte literals, SHA256 block/padding boundaries, path escaping,
module BFS prefixes, duplicate dependency loading and cycles. FIFO rendezvous
prove that later diagnostics use root/module bytes captured before a controlled
on-disk change.

## Frozen files

- `CONTRACT.md`: command/schema, range and snapshot rules.
- `cases.json`: fixed inventory, payload identities, independently authored
  locations and expected documents. `@ABS/` in an expected document denotes the
  isolated case's runtime absolute directory, not a literal compiler path.
- `oracle.py`: pure byte/hash/position oracle and strict JSON comparator; it does
  not import or execute the implementation.
- `preflight.json`: immutable baseline text observations, FIFO rendezvous and
  oracle controls, including fixture development before freezing.
- `freeze.json`: identities of all acceptance inputs.
- `storage-amendment.json`: records one storage-only correction made before any
  new JSON execution. Two raw POSIX filenames were renamed to safe repository
  storage names. Their logical names, source bytes, all expected documents,
  contract and oracle stayed unchanged; the preceding freeze is preserved.

Do not regenerate or update expected values from a new frontend's output. A
new requirement needs a separately identified supplemental contract and oracle.

`supplemental/` freezes eight compatibility cases after the original candidate
exposed a leading-dash filename regression and before the corrective CLI patch.
The immutable batch 49 compiler independently accepts all eight. Its generated C
for `main.yv` is the frozen reference for literal output names `--json` and
`--json-diagnostics`; additional JSON options remain rejected. Leading-dash input
names remain valid in the original `check INPUT` form. These observations and
their chronology are recorded in a separate contract, preflight and freeze;
the original 98-case inventory and oracle did not change.

## Execution

Formal acceptance consumes a successful complete current bootstrap receipt:

```sh
python3 tools/ci/selfhost_diagnostics.py \
  --bootstrap-output compiler/target/ci/selfhost
```

The default output is `compiler/target/ci/selfhost-diagnostics`. The runner checks
current source/fixture/harness/toolchain/compiler identities and uses actual
stage1 and final stage2 executables. Each JSON case runs in an isolated directory;
language cases also run the original text form in a fresh directory. Source,
cache and temporary-directory preservation are checked, except for the explicitly
recorded FIFO-controlled test mutations. The harness restores FIFOs to regular
payload files after execution so evidence archives remain portable.
The separately counted legacy CLI supplement also runs for each actual producer.
Candidate mode with a `--case` filter omits the supplement and cannot qualify as
complete acceptance.

Candidate preflight is explicitly non-formal and may select individual cases:

```sh
python3 tools/ci/selfhost_diagnostics.py \
  --candidate PATH_TO_CANDIDATE --case lexer_unicode
```

The previous batch's frontend is an observed negative control: its original text
output matches the frozen expectation, but its usage rejection for a JSON request fails this gate.
Resource measurements use the shared bounded process-tree runner; POSIX wait4
RSS is a waited-child observation, Windows RSS remains unavailable. Timings are
individual observations and make no compilation P50, token or cost claim.
