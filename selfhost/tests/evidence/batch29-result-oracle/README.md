# Batch 29: independent Result oracle

Baseline: `aecd7c3`. The original Rust stage0 compiler has SHA-256
`4482658b76d489863bdd16d1bc4263e87cdabf2fda0b3557cba16944ff1aaab8`.
The locked Zig is 0.17.1, SHA-256
`58959f01285c05a74276f09327b551d42ed6006bf325a78dc661a024e314987b`.

The registered batch contains 37 cases: 14 `run`, 18 `reject`, four
`unsupported`, and one `shared_unsupported`. The complete manifest has
372 checks and 140 executable emissions (112 normal runs plus 28 panic cases).
Existing promotion history is unchanged. Expected stdout and stage0 diagnostic
codes/positions were established independently of selfhost output.

One negative oracle requires a correction to Rust stage0:
`result_catch_break_missing_return`. The old checker incorrectly accepted a
non-unit function that falls through after a break hidden in a catch handler.
The corrected Rust Debug binary has SHA-256
`ac2c0f2d03076747b98ba912285ddd3789fc866b0dfbf8ea2d180958ea744679`.
Its E0013 result, the old acceptance, and both compiler identities are retained.
The unsafe program was never compiled or executed for this oracle.

## Accepted runtime mechanisms

All 14 final source versions pass stage0 Debug and Release: 28 successful
executions, exact stdout, empty stderr, exit 0, `YAVO_ALLOC_CHECK=1`, and
`YAVO_NO_CACHE=1`. The independent stage1 attempt02 runtime gate also emits,
builds, and runs all 14 through locked Zig Debug with `-DYV_ALLOC_CHECK`;
all 14 pass the same fixed outputs. These programs return normally; none uses
`os.exit` or panic to bypass allocation checks.

| Fixture | Independent mechanism |
| --- | --- |
| `result_forms` | Inferred and typed local Result; automatic success, explicit ok/err, direct Result forwarding, Copy Result parameters, unit Result bare return/fallthrough/explicit unit, explicit discard. |
| `result_owned_string` | String success/error payloads; local Result consumption; named catch error transferred to err; discarded and unhandled saved owned Results cleaned up. |
| `result_owned_vec` | Vec[String] success/error payloads through try; both variants explicitly discarded. |
| `result_owned_struct` | Struct success/error payloads containing String and Vec[String]; named catch rethrow and a further try. |
| `result_multilevel_try` | Three Result functions with different success types; failure releases String, Vec and struct locals before a normal caller catch. |
| `result_vec_partial_try` | A Vec literal fails after one owned element; later element is not evaluated. |
| `result_vec_struct_partial_try` | A Vec[Group] fails after a complete first struct; the second struct has an initialized String and a partially initialized Vec[String]. |
| `result_struct_partial_try` | Owned fields evaluated in source order, including a partial Vec field; declaration order differs from source order. |
| `result_argument_try_order` | Left-to-right argument evaluation; earlier owned temporary released on error and later argument skipped; success path invokes the callee. |
| `result_short_circuit_try` | Try inside both boolean operators; skipped RHS is silent, evaluated success/error RHS is observable. |
| `result_catch_return` | Named and ignored catches return a value from a non-Result function while owning locals. |
| `result_catch_loop` | Named catch conditionally continues or breaks a while loop; normal and error payloads and loop locals are released. |
| `result_try_loop_cleanup` | Try propagates from a while body through live inner and outer owned locals. |
| `result_catch_discard_owned` | A unit Result carries an owned struct error; catch _ releases it and caller execution resumes. |

Exact outputs are in `candidates.json` and the main `../../cases.json`.
The oracle program sources are the registered files in `../../run`,
`../../reject`, and `../../unsupported`.
`oracle-index.json` matches every final source hash to its independent evidence;
per-case proofs from an otherwise failed attempt retain that attempt's status.

## Fixed first-error observations

Positions below are stage0 JSON diagnostics for the final negative sources.
The local replay helper asserts these codes and positions. The bootstrap
manifest requires the listed diagnostic codes; its general runner does not
assert equality to these particular stage0 positions.

| Fixture | First code | Line:column |
| --- | --- | --- |
| `result_unhandled_statement` | E0001 | 2:13 |
| `result_unhandled_value` | E0001 | 4:13 |
| `result_try_outside` | E0001 | 2:25 |
| `result_try_error_type` | E0001 | 2:36 |
| `result_success_type` | E0001 | 1:35 |
| `result_error_type` | E0001 | 1:39 |
| `result_err_inferred` | E0001 | 1:26 |
| `result_borrowed_try` | E0401 | 2:16 |
| `result_borrowed_catch` | E0401 | 2:16 |
| `result_use_after_try` | E0402 | 5:22 |
| `result_use_after_catch` | E0402 | 5:18 |
| `result_catch_falls_through` | E0601 | 3:32 |
| `result_catch_partial_termination` | E0601 | 3:32 |
| `result_missing_return` | E0013 | 1:27 |
| `result_nested_chain` | E0020 | 1:26 |
| `result_nested_parenthesized` | E0020 | 1:16 |
| `result_compare` | E0501 | 5:12 |
| `result_catch_break_missing_return` | E0013 | 6:1 |

Arrow catch and Vec[Result] pass stage0 but remain U0001 subset boundaries.
Result fields are already U0001 in stage0 (1:24), so that case is
`shared_unsupported`. The original `unsupported_catch_binding` source and
identity are retained: it binds standard-library fs.Error, outside the
user-function Result scope of this batch.

The two transient-view boundaries also pass stage0: implicit success wrapping
of an owned String's slice, and explicit ok wrapping of a temporary String's
slice consumed during a function call. Both have independently passed stage0
Debug execution with output `a\n` and empty stderr. In the explicit case,
generated C drops the temporary String after the consuming function returns.
Their lifetime tracking is outside this selfhost batch, which reports U0001.

An attempted stored temporary-view Result was correctly rejected by stage0
E0305; its source and diagnostic remain in
`stage0-debug-followup-oracles.json`. It was replaced by the safe immediate-call
case before registration. The separate owner-mutation example remains review
evidence only. `result-view-mutation-before-boundary-fix.json` records Rust
E0102 at 4:5 and the earlier selfhost acceptance. The final bootstrap stage1
rejects the unchanged source with U0001 at 3:34, where the owned String view is
wrapped in Result, before the later owner mutation. The check-only result and
bound source/binary hashes are in
`result-view-mutation-after-boundary-fix.json`. Both records are linked by
`oracle-index.json`; they do not claim diagnostic equivalence, execute the
invalid program, or add a registered manifest case.

## Complete attempt history

| Evidence | Checks / native executions | Outcome |
| --- | --- | --- |
| `phase1-probes.json` | 21 checks / 16 Debug runs | Exploratory stage0 oracle; all selected valid probes execute successfully. Arrow syntax here is historical exploration. |
| `attempt-matrix-debug-1.json` | 33 checks / 13 Debug runs | 31/33 expected outcomes. Untyped ok(4) lacked an inferable error type; Result fields were initially misclassified as subset-only. All executed programs passed. |
| `attempt-matrix-release-1.json` | 33 checks / 14 Release runs | 33/33 after correcting the forms source and field classification. |
| `attempt-forms-debug-2.json` | 1 check / 1 Debug run | Corrected forms source passes. |
| `attempt-compare-check-1.json` | 1 check | Result comparison rejected with E0501. |
| `attempt-vec-struct-debug-1.json`, `attempt-vec-struct-release-1.json` | 1 check / 1 run each | Final supported aggregate cleanup source passes both modes. |
| `stage0-catch-break-before-fix.json` | 1 check | Old Rust incorrectly accepts the catch-break missing-return input. |
| `stage0-debug-followup-oracles.json` | 3 checks | Corrected Rust rejects catch-break fallthrough; implicit view is valid; the first stored-temporary candidate is rejected E0305. |
| `stage0-view-boundaries-debug.json` | 2 checks / 2 Debug runs | Both final legal transient-view boundary sources pass. |
| `stage1-attempt02-checks.json` | 33 checks | 32 expected codes/exits; nested Vec is outside the existing subset. Six stage0/selfhost position differences are retained without changing stage0 expectations. |
| `stage1-attempt02-replacement.json` | 1 check | The final Vec[owned struct] replacement passes the same stage1 snapshot. |
| `stage1-attempt02-debug-runtime.json` | 14 emits / 14 builds / 14 runs | All final runtime sources pass their independent fixed stdout/exit oracle with allocation checking. |
| `result-view-mutation-after-boundary-fix.json` | 1 check | Final bootstrap stage1 rejects the unchanged owner-mutation review input at Result wrapping with U0001, 3:34; exit 1 and empty stdout. |

The forms source was corrected once: `let inferred = ok(4)` became a local
initialized from a function returning `int ! int`. The original source and
E0001 remain in the failed attempt. The field source did not change; its kind
changed from `unsupported` to `shared_unsupported` after observing stage0.

The initially proposed `result_nested_vec_partial_try` used Vec[Vec[String]],
which stage0 supports but the existing selfhost subset does not. The agreed
scope keeps nested Vec unsupported. The final case uses an already supported
Vec of owned structs containing Vec[String], retaining the nested partial
construction cleanup test. Both versions' exact sources and observations are
preserved. Twelve other final Debug runtime sources are unchanged from the
initial matrix; forms and the aggregate replacement have their own later runs.

The phase1 archive stores all 129 original files as exact UTF-8 content with a
SHA-256 per file. Compaction checked every stored byte sequence before removing
redundant individual files. Formal attempts retain source snapshots, compiler
identities, commands, diagnostics and outputs. Runtime receipts additionally
include generated-C/binary hashes. The stage1 runtime gate compiles from fresh
source/runtime copies and rechecks all bound inputs and artifacts at the end.
Failed attempts remain available.

## Replay

After rebuilding the corrected Rust Release stage0, from the repository root
in the recorded Linux environment with pinned Zig:

```sh
YAVO_PYTHON=/home/appops/.yavo/python/cpython-3.13.18-20261110/python/bin/python3
"$YAVO_PYTHON" selfhost/tests/evidence/batch29-result-oracle/verify_stage0.py --attempt fresh-debug
"$YAVO_PYTHON" selfhost/tests/evidence/batch29-result-oracle/verify_stage0.py --attempt fresh-release --profile Release
```

The helper refuses to overwrite an existing attempt or build directory.
`--only <case-id> ...` restricts a replay to selected cases. Native CI continues
to use the repository's platform-aware `selfhost/bootstrap.py` harness.
