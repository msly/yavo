# GitHub CI

## Current routing: Core / Full (batch 52)

Every batch runs the three native fixed points, selected/changed semantic cases,
original-corpus classification, the blocker-analysis contract and self-compilation
measurements. Every fifth batch, a milestone, a release tag, or manual full adds
the retained full suites, product matrices, sanitizers and archive gates.
validation_plan.py cannot downgrade required full validation. The policy is
[validation-policy.json](validation-policy.json); details and commands are in
[validation-tiers.md](../../docs/validation-tiers.md).

The batch 51 section below records that historical run. Since batch 52 the
original hexadecimal fixture must be accepted; the decimal amendment remains
historical evidence rather than a permanent manifest restriction.

## Batch 51 standard-error acceptance (historical evidence; Full gate retained)

`tools/ci/selfhost_errors.py` requires the complete current bootstrap and uses
its actual stage0, stage1 and final stage2. The three frontends emit independent
C for the frozen multi-mode driver; each original C is compiled in Debug and
Release with allocation counters and the existing test-only checked-exit header.
Candidate and reference-only preflights are explicitly preliminary and cannot
establish an actual stage2 identity.

| Required inventory | Linux / macOS | Windows |
| --- | ---: | ---: |
| Frozen real-I/O cases | 48 | 46 |
| Actual frontends × native build modes | 3 × 2 | 3 × 2 |
| Native application executions | **288** | **276** |
| Oracle positive / negative controls | 1 / 5 | 1 / 5 |
| Allocation-exit clean / leak controls | 2 / 2 | 2 / 2 |
| Original nondecimal-source frontend checks | 3 | 3 |
| Independent GCC ASan+UBSan executions | **96 on Linux**, off on macOS | off |

Each case gets a fresh input directory. The receipt preserves exact stdin,
stdout/stderr, exit status, input-file identities in a separate `before` snapshot
and the final filesystem tree. The independent frozen Python oracle checks
output bytes and side effects, including non-destructive exclusive-write
failure. Native Io payloads must be nonzero; semantic fixtures separately check
exact artificial payload formatting without assuming portable errno numbers.
Windows explicitly excludes directory-as-stdin and invalid native UTF-8
environment bytes; neither is a dynamically skipped failure.

The [contract and freezes](../../selfhost/tests/errors51/CONTRACT.md) include 73
new semantic probes, ten unchanged original-source promotions and 48 I/O cases.
One of the 73 newly authored semantic fixtures originally used `['a', 0xE2, 0x82]`, outside the existing nondecimal lexer subset. A single recorded amendment adds the equivalent decimal source `['a', 226, 130]` and changes only that manifest entry's file. The original source, freeze, manifest snapshot and first candidate U0001 failure remain preserved; old stage0 executes both versions in Debug/Release with the unchanged `1\n` output. Formal boundary checks retain stage0 acceptance and stage1/stage2 U0001 for the original source. The formal gate verifies the exact one-field manifest overlay and
all **101** bound frozen inputs; every other original input remains unchanged.

```sh
python3 tools/ci/selfhost_errors.py \
  --bootstrap-output compiler/target/ci/selfhost \
  --sanitizers required
python3 tools/ci/package_test_summary.py \
  --input compiler/target/ci/selfhost-errors \
  --output compiler/target/ci/selfhost-errors.tar.gz \
  --prefix selfhost-errors
```

Use `--sanitizers required` on Linux CI; native macOS/Windows use `off`.
The Linux gate first proves clean/address/undefined capability controls, then
builds the **original stage0 C and actual final-stage2 C** with independent GCC
ASan+UBSan and executes 48 cases per product. Sanitizer flags/environment,
compiler/library identities and raw outputs are recorded. LSan remains disabled;
allocation counters and checked exit controls cover tracked leaks. No non-Linux
sanitizer or performance result is implied by native success.

Default output is `compiler/target/ci/selfhost-errors/receipt.json`, schema
`yavo-selfhost-errors/1`, plus per-attempt evidence. CI runs the gate after
bootstrap and packages `selfhost-errors.tar.gz` even after failure. A formal
receipt must bind all current compiler modules, manifest/harness, producer
binaries and frozen inputs, and verify they remain unchanged. Isolated Zig
caches are removed on success and failure.

### Promoted-fixture execution isolation

The bootstrap manifest may attach `environment` and `isolated_cwd` only to
run/panic cases. Environment keys are restricted to literal `name` and `PATH`;
string values must not contain NUL, and null removes the variable. Each
reference/candidate × Debug/Release execution receives its own fresh cwd when
requested. These options affect execution only; compiler environments are
unchanged. `selfhost_sanitize.py` verifies and transports the same options in its
program bundle, and `tools/sanitizers/run.py` applies them without replacing
ASAN_OPTIONS, UBSAN_OPTIONS or YAVO_ALLOC_CHECK. Five real subprocess probes and
27 invalid-option controls verify absence/empty PATH, isolation and preserved
sanitizer controls.

### Current evidence and archive audit

Local bootstrap passes **1136/1136 and 20/20**, the formal native error gate
**288/288 with nine checks**, and the six newly accepted original programs pass
**48/48** runtime checks. The local error sanitizer mode is off. Implementation `76915bc` passes **6/6 jobs** in [run 46028799189](https://github.com/msly/yavo/actions/runs/46028799189). The three native error gates pass 288 executions on Linux/macOS and 276 on Windows; Linux independently instruments the original stage0 and actual final-stage2 C with GCC ASan+UBSan and passes 96 further executions. The three original native ZIP archives, GitHub sizes/hashes, raw receipts, inputs and product identities are verified. The first [run 46018585973](https://github.com/msly/yavo/actions/runs/46018585973) passed 5/6 jobs: the Windows error gate stopped before emitting C or executing scenarios because a native backslash path was compared with the frozen POSIX path. That comparison now uses `.as_posix()`; eight path controls and the 288-execution local error gate pass. The original failure and archives remain preserved. No new local full Rust suite was run for batch51. Current CI logs record linux: full suites, Debug/Release each 151 passed / 0 failed / 1 ignored; macos: selected native suites, Debug/Release each 90 passed / 0 failed / 1 ignored; windows: selected native suites, Debug/Release each 87 passed / 0 failed / 1 ignored.
The [batch51 index](../../compiler/validation/batch51-standard-errors-2026-11-27.json) binds current evidence and the explicit
fixture correction. The offline auditor checks actual six-product provenance,
raw command/stream identities, before/after trees, oracle and cleanup controls,
the Linux instrumented products, and promoted-fixture environment/cwd records.
Nested archives retain bounded streaming TAR indexing and validated member
paths instead of fully extracting arbitrary evidence trees.

## Batch 50 structured diagnostics acceptance (historical batch; retained gate)

`tools/ci/selfhost_diagnostics.py` requires a successful complete current
bootstrap and checks the Rust-built stage1 and actual final stage2 on all three
native platforms. Producer identities bind the ten compiler modules and their
bootstrap copies, runtime, toolchain lock, executable and fixed-point C hashes.
The gate also binds the current bootstrap manifest, harnesses and frozen inputs,
then checks the bound files again before reporting a pass.

| Required inventory | Linux / macOS | Windows |
| --- | ---: | ---: |
| Primary JSON / CLI scenarios per producer | 98 | 94 |
| Existing text checks per producer | 80 | 76 |
| Supplemental legacy CLI checks per producer | 8 | 8 |
| Stage1 + actual stage2 primary executions | 196 | 188 |
| Stage1 + actual stage2 text executions | 160 | 152 |
| Stage1 + actual stage2 supplemental executions | 16 | 16 |
| Total formal compiler commands | **372** | **356** |

The [frozen diagnostic contract and oracle](../../selfhost/tests/diagnostics50/README.md)
predate execution or inspection of the new JSON implementation by the oracle
author. The original contract and expectations remain unchanged; a recorded
storage-only amendment preserves the earlier freeze while making repository
filenames safe for Windows checkout. Eight legacy CLI regressions have their own
supplemental contract, baseline preflight and freeze, established before the
corrective patch. They preserve leading-dash input names and literal JSON-option
output names, and reject additional JSON options on `emit-c`.

Each run requires a valid oracle positive control and **10 negative controls**
covering ranges, hashes, source order, types, extra fields/diagnostics, duplicate
keys, trailing output and invalid UTF-8. On Linux/macOS, two FIFO scenarios each
run in JSON and text modes: **four snapshot-barrier executions per producer**.
They mutate an already loaded root or imported file only after the next import
has connected to its FIFO. Windows explicitly excludes these two scenarios and
two POSIX-only filename scenarios.

JSON acceptance checks the first diagnostic's raw half-open byte range and
one-based Unicode-scalar coordinates against the original UTF-8 bytes. The
source list records the loaded breadth-first prefix, including a successfully
read file whose lexing fails; unreadable or invalid-UTF-8 imports are excluded.
JSON mode retains each loaded source snapshot and its SHA-256 at load time, so
later diagnostics use those bytes without reopening the source. The runner
preserves command streams, source inventories and controlled mutation records;
FIFO payloads become regular files after execution for portable archives.

```sh
python3 tools/ci/selfhost_diagnostics.py \
  --bootstrap-output compiler/target/ci/selfhost
python3 tools/ci/package_test_summary.py \
  --input compiler/target/ci/selfhost-diagnostics \
  --output compiler/target/ci/selfhost-diagnostics.tar.gz \
  --prefix selfhost-diagnostics
```

The default output contains `receipt.json` with schema
`yavo-selfhost-diagnostics-acceptance/1` and per-attempt evidence. CI packages
`compiler/target/ci/selfhost-diagnostics.tar.gz` even after failure.
`--candidate PATH` produces `formal: false` and `preliminary_passed` on success;
it cannot establish acceptance of the actual bootstrap producers. A candidate
`--case` selection also omits the supplemental inventory.

### Compiler sanitizer JSON requirement

The Linux `selfhost_sanitize.py` gate now also requires the ASan/UBSan compiler
built from actual stage2 C to pass all **98 primary + 80 text + 8 supplemental
commands**, totaling **186**. `selfhost_compiler_sanitize.py` verifies the full
case IDs, eight required harness checks, oracle controls, four FIFO executions,
sanitizer environment, exact instrumented executable identity, source/fixture
bindings and saved command streams. It rejects partial or substituted candidate
receipts. This internal candidate retains `formal: false`; the surrounding
sanitizer receipt separately binds it to the actual stage2 C.

Run the combined gate with
`python3 tools/ci/selfhost_sanitize.py --bootstrap-output compiler/target/ci/selfhost`.
CI packages its complete `selfhost-sanitizers` directory as
`selfhost-sanitizers.tar.gz` and excludes the raw directory from artifact upload;
this preserves the JSON cases' control-character filenames inside the TAR.
Its compiler receipt retains the nested `json-diagnostics` evidence alongside
the existing compiler sanitizer artifacts. The batch50 exact-commit Linux CI
run passed this instrumented gate; the [validation index](../../compiler/validation/batch50-json-diagnostics-2026-11-27.json)
binds its downloaded original receipt and all three native platform results.
Ordinary candidate success alone does not establish a sanitizer pass. Existing
LSan exclusions for fatal `os.exit` paths remain.

## Batch 49 ordinary selfhost build/run acceptance

`tools/ci/selfhost_driver.py` runs after a successful complete bootstrap on all
three native platforms. It binds the actual final stage2 and the Rust-built
stage1 to unchanged compiler/runtime/toolchain inputs, then applies the same
frozen driver cases to both. The independent primary inventory is 55 on POSIX
and 52 on Windows; two separately frozen relative-temporary-root regressions
bring each driver to 57 or 54. The expected scenario execution inventory is
therefore **114 on POSIX or 108 on Windows**, excluding setup controls, explicit
execution of built programs and the ordinary self-build checks.

The driver gate covers native Debug/Release `build/run`, multiple source files,
spaces/Unicode paths, empty/quoted/backslash/shell-metacharacter arguments,
stdio and exit propagation, source-identity protection including hardlinks,
pinned-backend failure modes, unsupported options, no program-cache writes,
and temporary cleanup including deliberate unknown residue. A controlled native
probe in a private pinned-toolchain path checks backend argv and raw diagnostic
bytes separately from real Zig builds. Final stage2 also uses ordinary `build`
on unchanged compiler sources; the resulting compiler must emit the existing
fixed-point C and run the independent test program. This establishes the native
uncached driver closure, not complete SH-3 or a performance/cost target.

Bootstrap adds an `embedded_runtime_freshness` check and binds the embedding
generator, production header, toolchain lock and generated Yavo module. Native
Rust CI also runs the `system` integration target in Debug and Release. Driver
evidence is packed as `compiler/target/ci/selfhost-driver.tar.gz`; regenerable Zig
caches are removed first. The [frozen driver contract and cases](../../selfhost/tests/driver49/README.md)
describe the independent-preflight and supplemental-regression provenance.

`Yavo CI` runs on pushes to `main`, pull requests, and manual dispatches. Changes
limited to ordinary Markdown or archived validation JSON normally do not rerun
it. Frozen experiment inputs are exceptions: changes to `docs/agent-api-index.md`,
`docs/agent-start.md`, `experiments/docs-ab-2026-11-22-r6/**` or
`tools/agent_eval/README.md` trigger CI even when the changed files are Markdown,
so their preparation gate checks the frozen bytes. Compiler, runtime, fixtures,
scripts, locks and workflow changes also trigger every job. CI uses Rust 1.99.1
and the existing SHA-256-locked Zig 0.17.1 archives. The checkout and
artifact actions are pinned to reviewed full commits; workflow permissions are
read-only. Debug and Release are sequential within each job because the existing
test runners share temporary paths.

The bootstrap-subset fixed point was confirmed on all three native platforms in
[45150424099](https://github.com/msly/yavo/actions/runs/45150424099) at `4739ffc`:
six of six jobs passed, including each native selfhost step. The
[native confirmation index](../../compiler/validation/selfhost-fixed-point-native-2026-11-23.json)
binds the run, job/step results and artifact metadata. It preserves the original
`67794d9` local fixed-point receipt rather than rewriting its historical pending
platform status. That historical run sanitized stage1-generated applications,
not the stage2 compiler itself.

## Batch 47 CSV application acceptance

`examples/csv-check` is the second independent application accepted for
SH-4. Its frozen contract and Python oracle contain **139 manual scenarios**:
138 apply on Linux/macOS and 137 on Windows. The remaining cases are explicitly
not applicable because of native filename or directory-stdin behavior. They are
not counted as passed executions. The four application modules separate CLI/I/O,
CSV parsing, header/width validation and output.

Bootstrap's `csv_check_application` check uses the final stage2 compiler after
its fixed-point checks. For every applicable scenario it compares exact stdout,
stderr and exit status across **six artifacts**: ordinary stage0/selfhost Debug
and Release, plus a supplementary Release exit-check artifact from each frontend.
The expected execution inventory is 828 on POSIX or 822 on Windows. The test-only
exit shim leaves generated C and production runtime bytes unchanged; independent
clean/leak controls must preserve exit 2 and detect the tracked leak with exit
102. This covers both successful and failed application exits.

The separate native driver, `tools/ci/csv_check.py`, runs three artifacts per
scenario: stage0 Debug, Release and the exit-check artifact. It also builds two
Release copies in independent directories with empty Yavo/Zig/temp caches and
identical relative source layout/output basename, then compares C and executable
bytes. Separate Debug/Release cold, unchanged and comment-edited source builds
record cache miss/hit/miss and timings; the comment edit is confined to an
isolated copy. Paths contain spaces and non-ASCII text. The driver binds source,
oracle, compiler, runtime and installed locked Zig identities, refuses `YAVO_CC`,
and never installs or downloads a missing toolchain.

The local Linux RSS run at
`compiler/target/ci/csv-check-batch47-rss/attempt-xyp1f13q/receipt.json` passes
**414/414 executions**, both independent Release comparisons, clean/leak controls,
all six cache-condition observations, and input/artifact change guards. Its
Release executable is **30,456 bytes** and emitted C is **46,702 bytes**. All
**431 commands** have measured RSS; the maximum reported command value is
417,312,768 bytes. [The application README](../../examples/csv-check/README.md)
records the individual timing/RSS observations and reproduction hashes.

On Linux and macOS the shared runner reaps each command with `wait4` and records
its OS-reported `ru_maxrss`: Linux KiB are converted to bytes; macOS already
reports bytes. The receipt's overall memory value is the maximum of these
per-command observations, not a simultaneous process-tree total or the sum of
command peaks. Windows records memory as unavailable/null. Timing samples are
single observations, not P50/P95 estimates or a performance gate; provider token
usage and monetary cost remain unknown.

Linux CI invokes `--sanitizers required --bootstrap-output
compiler/target/ci/selfhost`. It must validate clean, ASan use-after-free and UBSan
signed-overflow capability controls, then run every applicable scenario against
**both** stage0 C and CSV C freshly emitted by the final stage2 identified and
verified from the successful bootstrap receipt. The existing general selfhost
sanitizer gate does not substitute for this application coverage. Other native
platforms use `--sanitizers off`, recorded as not applicable. This local host has
no GCC: its sanitizer result is **unavailable, passed null, zero executions**.
Implementation `0373fb8` passes **6/6 jobs** in [run 45937402633](https://github.com/msly/yavo/actions/runs/45937402633).
All three native bootstrap and CSV application steps, CSV evidence packaging
and artifact upload succeed. Linux passes the required two-producer sanitizer
gate (276 CSV executions) as well as the general selfhost sanitizer gate.
The [validation index](../../compiler/validation/batch47-csv-set-2026-11-27.json)
records exact-head job/step verification separately from the local receipts.

The shared `csv_support.py` preserves raw stdin/stdout/stderr, commands, exit
status, elapsed time and memory observations, and bounds subprocess trees on
timeout. The bootstrap wrapper removes materialized special filenames after
execution while retaining safely named stream evidence. The native driver first
archives materialized inputs as `fixture-inputs.tar.gz`, verifies every file's
hash against its execution identity, and then removes the raw names. Rebuildable
caches are removed. CI additionally packages the full evidence tree as
`compiler/target/ci/csv-check.tar.gz` using `package_test_summary.py --input
compiler/target/ci/csv-check --output compiler/target/ci/csv-check.tar.gz --prefix
csv-check`; upload excludes the unpackaged `csv-check/` tree.

## Batch 31 for and traversal checks

The manifest has **490 cases and 172 executable inputs**. Its 52 batch cases
(51 new plus the unchanged for promotion) cover int/byte ranges, Copy Vec
iteration, bounds and exit control, container locks, replay and explicit subset
boundaries. All have independent Rust oracles; 17 run programs pass both modes.
The reference range-start snapshot fix has an additional automatically enumerated
P0 fixture, with old failures and fixed outputs retained in the oracle index.

The [local receipt](../../compiler/validation/selfhost-for-2026-11-24.json)
passes 490/490, 12/12 and zero skips, including fixed C/binaries and the real
lexer unit. Full Rust Debug/Release each pass 145/0/1; frozen conformance is
124/3/2. The four compiler/lexer migrations are checked against the baseline
function bodies. The GCC input loader validates **173 programs** (172 fixtures
plus lexer); the native job must additionally run stage2 self-compilation,
**490 checks, 172 emissions and 8 malformed inputs**.

[Run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591) passed all six jobs at `046ccc6`.
All three native platforms pass 490/12 with zero skips, within-platform C and
binary fixed points, the real lexer in both modes and 213 test-summary
executions each. Linux GCC executed the complete 173-program and stage2 gate
described above. Rust totals per mode are Linux 145/0/1, Windows 81/0/1 and
macOS 84/0/1, preserving each platform's documented selection.
The [batch-31 native index](../../compiler/validation/batch31-native-2026-11-24.json)
binds three API-matched archives, 44,165 rehashed files, 25,476 embedded
streams and 837 local input identities to the implementation commit. It
supplements the local receipt without rewriting its historical pending status.

## Batch 30 Copy Optional and pop checks (historical)

That manifest contains **439 cases and 155 executable inputs**. Its 68
targeted cases comprise 67 additions and the byte-for-byte promotion of the
existing pop input. They cover Copy payloads, receiver permissions, lazy
coalesce, contextual typing, branch cleanup and explicit unsupported boundaries.
The compiler itself uses pop at all 14 former truncation sites.

Bootstrap still requires 12 auxiliary checks, including real lexer stage1/stage2
C equality and Debug/Release allocation checks. The Linux adapter requires
**156 generated programs** (155 fixtures plus the real lexer), instrumented
stage2 self-compilation, **439 checks, 155 emissions and 8 malformed inputs**.
These are separate counting units, derived from that batch's manifest and sources.

The [local receipt](../../compiler/validation/selfhost-optional-pop-2026-11-24.json)
passes 439/439 cases, 12/12 checks and zero skips, with identical stage2/stage3
C and binaries. Full Rust Debug/Release each pass 145/0/1 and frozen
conformance remains 124/3/2. [Run 45345384747](https://github.com/msly/yavo/actions/runs/45345384747)
passed all six jobs at `7297e3b`. Each native platform passes 439 cases,
12 checks, zero skips and the existing application's 213 executions. Linux
passes the complete 156-program and stage2 GCC sanitizer gates above.
The [native index](../../compiler/validation/batch30-native-2026-11-24.json)
records three downloaded archives matched to GitHub size/digest, 40,102
rehashed files, 22,892 checked embedded streams and 783 local input bindings
matched to the implementation commit. The historical local pending fields
remain unchanged.

## Batch 29 Result and real lexer checks (historical)

[Run 45226925936](https://github.com/msly/yavo/actions/runs/45226925936) passed
all six jobs at `e355cb3`. All three native platforms pass 372 cases and
12 auxiliary checks with zero skips; the Linux integrated sanitizer gate passes
all 141 generated programs and the stage2 compiler checks below. Each platform
also passes the existing application's 213 executions. The
[native index](../../compiler/validation/batch29-native-2026-11-24.json)
records exact platform Rust selections/counts, three downloaded archive digests,
and independent source/artifact/output inspection.

The batch-29 manifest had 372 cases and 140 executable inputs. Bootstrap retains
the stage0 lexer unit and adds `lexer_units_selfhost`: stage1 and stage2 must emit
identical C from the current `support.yv + lexer.yv + lexer_test.yv`, and that C
runs in Debug/Release with `YV_ALLOC_CHECK`. Expected lexical errors are caught;
the successful test process returns normally after checking diagnostics that
outlive their source/path allocations and 16 rounds of partial-token cleanup.
Together with the existing guards this makes 12 auxiliary checks.

The Linux adapter requires the same exact source concatenation, stage1/stage2
emission commands, C hashes, two mode binaries and successful output before
adding `lexer_selfhost_unit` to the GCC sanitizer bundle. The generated-program
gate therefore had 141 programs (140 manifest programs plus the real lexer unit).
The separately instrumented stage2 compiler still covers every manifest check
and executable emission (372/140), self-compilation and eight malformed inputs.
The source-bound local and native receipts distinguish these counting units.

## Batch 28 application and compiler checks

[Run 45174913978](https://github.com/msly/yavo/actions/runs/45174913978) passed
all six jobs at `2a64808`. Each native platform passed 71 application scenarios
across 213 executions and 335 bootstrap cases with 11 auxiliary checks. Linux
also passed the complete stage2 ASan/UBSan adapter. All three native archives
were downloaded and inspected; the [final index](../../compiler/validation/batch28-native-2026-11-23.json)
binds the source, jobs, archive digests and inspected receipt identities.

Every native job now runs `python3 tools/ci/test_summary.py`. It compiles the
first non-compiler application from four modules in three independent empty-cache
builds, compares Release C and binary identities, and executes the frozen log
contract in Debug and Release. A separately compiled exit-check helper validates
tracked heap cleanup on nonzero exits: a real String clean/leak probe must
preserve exit 2 or detect the leak with exit 102 before application cases run.
The helper leaves production C/runtime bytes unchanged and has its own identity.
This is one application's acceptance, not completion of all three SH-4 projects.
Each run writes a new attempt and a current receipt under
`compiler/target/ci/test-summary/`.

The gate creates real CR/LF filenames on POSIX. `package_test_summary.py` keeps
all application evidence with its original bytes and names in
`compiler/target/ci/test-summary.tar.gz`, omitting only reproducible
`empty-caches` directories. The upload excludes the original application tree
and includes this portable container; readers inspect its tar members without
renaming the tested paths. The first batch-28 run failed at macOS artifact upload
after its application gate passed, as preserved in the
[initial-run note](../../compiler/validation/batch28-ci-initial-2026-11-23.json).

The existing Linux `selfhost_sanitize.py` step now requires both the generated
program gate and `selfhost_compiler_sanitize.py` to pass. The latter builds one
GCC ASan/UBSan compiler from the actual stage2 C and exercises self-compilation,
every manifest check and executable emission, and eight fixed malformed inputs.
The copied compiler C/runtime are bound to their bootstrap hashes in a fresh
build directory; GCC and sanitizer-library identities match the proven probes.
No new Rust frontend is used to generate the tested compiler C. LSan remains
disabled; compiler rejection via `os.exit` is not reported as a leak-check pass.

`python3 tools/ci/selfhost_sanitize.py --bootstrap-output PATH` selects another
completed bootstrap output directory. Native CI keeps the default paths, and
uploads the full application, compiler-sanitizer and bootstrap receipts with
the existing platform artifact.

## Earlier confirmed runs

The preceding byte validation completed successfully in
[45126097965](https://github.com/msly/yavo/actions/runs/45126097965) at
`aecdfe58e6f81b794c48827fa2e49905a59331e5`.
The [byte receipt](../../compiler/validation/selfhost-byte-2026-11-23.json)
records **321 local cases, 119 executable inputs and 10 auxiliary checks**.
All local input identities, fixed oracles and 476 mode binaries were verified.

[CI run 45126097965](https://github.com/msly/yavo/actions/runs/45126097965) passed **six of six jobs** at
`aecdfe5`. The selfhost gate succeeded on Linux x86-64, Windows x86-64
and macOS arm64. The native Linux job passed **119/119 Yavo-generated C
programs and 3/3 capability probes** with independent GCC ASan/UBSan.
The native Linux archive matched GitHub's size/SHA-256; its actual source,
C, frontend/program binaries and outputs were independently verified,
including all 476 bootstrap mode execution records and the three
original-source workloads. The other five archives have metadata checks only.
LSan is disabled; normal exits use YV_ALLOC_CHECK.

The preceding borrowed-index validation is
[45121072503](https://github.com/msly/yavo/actions/runs/45121072503) at
`b5dfe9feb5c48bd22defd73b01f0b917dfbc2baf`.
**All six jobs passed.** The
[borrowed-index receipt](../../compiler/validation/selfhost-borrow-index-2026-11-23.json)
records successful selfhost steps on all three native platforms and
**102/102 programs plus 3/3 capability probes** in the Linux frontend ASan/UBSan
step. The downloaded native Linux archive matched GitHub's size/SHA-256;
its three original receipts and actual source/C/frontend/program/output bytes
were independently checked against fixed oracles. All 408 bootstrap mode
execution records were checked. The other five archives have metadata checks only.

The preceding owned-vector validation is
[45116399319](https://github.com/msly/yavo/actions/runs/45116399319) at
`c7be68216129dc9f1dad5aad668e2f1382142c1d`.
**All six jobs passed.** The
[owned-vector receipt](../../compiler/validation/selfhost-owned-vec-2026-11-23.json)
records successful selfhost steps on all three native platforms and
**90/90 programs plus 3/3 capability probes** in the Linux frontend ASan/UBSan
step. The downloaded native Linux archive matched GitHub's size/SHA-256;
its three original receipts and actual source/C/frontend/program/output bytes
were independently checked against the fixed oracles. The other five archives
have metadata checks only.

The preceding Copy-vector validation is
[45103511821](https://github.com/msly/yavo/actions/runs/45103511821) at
`7881db94f1509baa317d9bf84fae2ae28f402a80`: **all six jobs passed**.
The [Vec receipt](../../compiler/validation/selfhost-vec-2026-11-23.json)
records all job/step results and the downloaded native Linux archive's verified
size/SHA-256. Its three original receipts and actual source/C/frontend/program/
output bytes were independently checked; all 78 selfhost sanitizer programs
and 3 capability probes passed. The other five archives have metadata checks only.

The preceding [String receipt](../../compiler/validation/selfhost-string-2026-11-22.json)
retains both of that increment's CI attempts and the PCH concurrency fix.
The historical
[45002067581 receipt](../../compiler/validation/github-ci-agent-eval-2026-11-22.json)
retains its separate six-archive download verification. Round-six live
qualification remains blocked with zero participant sessions and deliveries.

Setup first builds and executes one small native program in C Debug and Release,
sequentially, to initialize the new C toolchain before concurrent fixture tests.
These commands have separate logs and are not performance measurements or Cargo
test totals; the existing 120-second per-fixture build deadline is unchanged.

| Job | Executed gate |
| --- | --- |
| Ubuntu 24.04, x86_64 | All Cargo tests in Debug and Release, six-target cross-compilation, frozen conformance, selfhost gate and independent sanitizer validation of its generated C |
| Windows 2025, x86_64 | Native compiler unit tests and the explicitly listed portable integration suites in Debug/Release, then the selfhost gate |
| macOS 15, arm64 | The same native selection in Debug/Release, then the selfhost gate |
| Independent Linux ASan+UBSan | Export the current generated C and runtime, validate sanitizer positive/negative probes, then execute every exported P0 run/panic fixture with strict output and exit expectations |
| Round-five experiment evidence | Recreate the frozen task packet, verify archived source/output identities, recompute all 1,344 judgments and summary values, and check 16 hash-bound module/thread reviews |
| Agent evaluation preparation | Test per-request usage and launch contracts, deterministic R6 task/document reconstruction, independent oracles and refusal to initialize an unqualified participant; no model requests |

The native selection is declared in `NATIVE_TESTS` in `run.py`: P0 fixtures,
allocation checks, Arena, modules, indexed assignment/methods, mutable-reference
returns, collections, fixes, concurrent runtime I/O, standard-library I/O,
exclusive output-file creation, and self-hosting stage0 blockers.
P0 and heap checks build/run their programs with C `-O0` and `-O2`; they include
floats, threads, channels, and the stack-panic fixture. Windows/macOS numbers are
reported as this selection, not as the complete Linux suite. Their frozen report
scenarios remain excluded because the current conformance runner explicitly
requires Linux x86_64 for those scenarios.

All native jobs run `selfhost/bootstrap.py`: the Yavo-written frontend is built
by stage0, then independently checks/emits C for 321 fixed cases. The 119 executable
inputs run in both C modes against fixed oracles and a Rust differential
reference. Two original sources, diagnostic and AST-model, are verified as unchanged
prefixes of three executable workloads: diagnostic, model storage and model content
traversal. The traversal reads all four AST vectors and their nested buffers,
including clones after the original owner is released. A separate workload
executes seven unchanged lexer function fragments against independent byte,
symbol-pair and exponent-lookahead oracles. This remains a subset gate,
not stage2/stage3 self-compilation.

The Linux native job additionally runs `selfhost_sanitize.py`. It verifies the
bootstrap receipt, manifest, source identities and the candidate frontend's
actual repeated C outputs before reusing the independent GCC sanitizer runner.
All current executable inputs, the real lexer unit and the clean/UAF/signed-overflow
capability probes are mandatory; historical program counts above retain their
original source scope.
C is not regenerated through the Rust frontend. This step is separate from
the existing P0 sanitizer job. LSan is disabled; normal exits use YV_ALLOC_CHECK.

Linux conformance is checked twice: the compiler's existing monotonic ratchet and
a CI contract matching all 129 manifest case identities. The current contract is
124 pass, zero wrong/not_run, three unsupported (`T-GR9`, `T-P3`, `T-R2`), and two
superseded (`T-GF2`, `T-V1`). Changes to those exceptions require explicit review
of `conformance.json`; CI never updates the baseline. The one ignored Cargo test
is the old exploratory `p0::conformance_report`; the active conformance suite is
not ignored. This gate does not measure v1 completeness.

Each job uploads `compiler/target/ci/` for 14 days, even after failure. Native artifacts contain
the source revision, host, exact command/exit/time records, toolchain and Cargo
lock hashes, compiler hash, full logs, actual test totals, and (on Linux) separate
Debug/Release conformance JSON. Native artifacts also contain
`selfhost/receipt.json` and per-case generated C, executables and logs; the Linux
native archive includes `selfhost-sanitizers/receipt.json` and its source-bound
GCC sanitizer bundle and detailed logs. The separate sanitizer artifact retains the
source/C/runtime bundle, capability probes, executables, and detailed validator
receipts. Independent GCC results do not turn the pinned Zig ASan exceptions
into conformance passes.

The experiment job runs `experiment_evidence.py` against the archived four
sessions at the frozen `8c200aa` packet. It checks evidence integrity rather than
requiring every participant answer to be correct; a recorded failure is valid
experiment data, while missing or inconsistent evidence fails CI. Unavailable
token usage remains `null`. This job does not invoke a model or rerun participant
executables. Its artifact contains the source revision, gate hash, packet
regeneration/self-test logs, independently recomputed totals, and integrity
receipt. The six recorded string-to-integer conversions of documentation line
numbers are checked under the narrow policy documented in the receipt; original
request and event logs are preserved.

The Linux full-suite job also runs `agent_docs.py`: it extracts the starter guide's
five source blocks, builds four complete projects and checks eight focused rules,
including four expected diagnostic failures. Its 12-case receipt is separate from
the Cargo and frozen-specification counts. The new preparation job archives
`launch_status: blocked`, with zero participants; a passing infrastructure check
must not be reported as a completed document experiment.

For local reproduction in a fresh checkout with Rustup and Python available:

```sh
python3 tools/ci/run.py setup
python3 tools/ci/run.py native --suite full   # Linux x86_64
python3 tools/ci/run.py native --suite native # Windows/macOS selection
python3 selfhost/bootstrap.py               # All native platforms
python3 tools/ci/csv_check.py --bootstrap-output compiler/target/ci/selfhost --sanitizers auto
python3 tools/ci/selfhost_sanitize.py        # Linux with GCC; consumes bootstrap output
python3 tools/ci/run.py sanitize             # Separate Rust/P0 gate: Linux with Perl and GCC
python3 tools/ci/experiment_evidence.py       # Archived round-five evidence
python3 tools/ci/agent_docs.py                # Current starter examples and diagnostics
python3 tools/ci/agent_eval.py                # Offline R6 preparation and launch refusal
```

Use `python` on Windows. A setup/download failure is a failing job, never a skip.
Reports and working outputs are under the ignored Cargo target directory; the
workflow does not publish releases, change repository permissions, or use a
personal access token.

## Pure Yavo source-digest acceptance

`selfhost_digest.py` runs the frozen SHA-256 vectors only after a successful
complete bootstrap. It binds the current ten compiler modules, bootstrap
harness, complete fixture inventory, runtime and toolchain lock to the producer
receipt, then uses that receipt's stage0, Rust-built stage1, actual final stage2
and locked Zig executable. There is no candidate fallback in this formal gate.

The unchanged `selfhost/tests/digest50/verify.py` checks **46 vectors across
three frontends and Debug/Release**, totaling **276 vector executions** with
`YV_ALLOC_CHECK`. The wrapper verifies the six generated-product records,
command outcomes, exact output bytes, compiler/source/artifact hashes and
private-cache cleanup, then checks all bound files again before reporting a
pass. The original vectors, input recipes, unit source and frozen expectations
remain under `selfhost/tests/digest50/` and `selfhost/tests/digest50_test.yv`.

```sh
python3 tools/ci/selfhost_digest.py \
  --bootstrap-output compiler/target/ci/selfhost \
  --output compiler/target/ci/selfhost-digest
```

Both paths above are defaults. The output root contains `receipt.json` with
schema `yavo-selfhost-digest/1`; each `attempt-*` retains the underlying complete
vector receipt, commands, logs, copied inputs, generated C and executables for
artifact packaging. A stale or incomplete producer fails before vector
compilation. Vector counts are functional acceptance, not compile-latency or
delivery-cost measurements.
