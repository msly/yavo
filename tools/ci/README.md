# GitHub CI

`Yavo CI` runs on pushes to `main`, pull requests, and manual dispatches. Changes
limited to Markdown or archived validation JSON do not rerun it; compiler,
runtime, fixtures, scripts, locks and workflow changes still trigger every job.
It uses
Rust 1.99.1 and the existing SHA-256-locked Zig 0.17.1 archives. The checkout and
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

Batch 31 native CI is pending the implementation push. Each platform must
retain the same fixed-point, real lexer and independent test-summary gates;
successful local input loading is not GCC execution evidence.

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
These are separate counting units, derived from the current manifest and source.

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
