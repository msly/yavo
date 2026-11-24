# Batch 31: independent range / Copy Vec for oracle

Baseline: `0ae3b856bae5e795dd15ac6d74ebbfcfee539146`.
The final Rust reference is the frozen
`compiler/target/ci/batch31-stage0-fixed/yavo`, SHA-256
`2f5e7da56f11898371b0243e1d14cd9f57cd666266e3939f86811991dbac4814`.
The original frozen reference remains
`compiler/target/ci/batch31-stage0/yavo`, SHA-256
`bcbbbf91215b81ad628329320ca9b18b0c924f9735a006af2597d77d4640c2c0`.
Both identities are retained because independent probes found and then verified
a correction to Rust range lowering during this batch.

The batch registers **52 cases: 17 run, 25 reject, and 10 unsupported**.
One is a byte-identical promotion, so the manifest grows from 439 to **490 cases**:
144 run, 28 panic, 243 reject, 61 unsupported, and 14 shared_unsupported.
There are **172 executable emissions**. The ninth promotion record preserves
the identity of `unsupported_for`, now `run/for_basic.yv`, including its old
path, kind, baseline and source hash.

The manifest SHA-256 is
`6297cd871ff1a360e1f50fb880ababf98bd1fba731ce843c2852a7973dcf9498`.
`oracle-index.json` maps every registered source to its raw source hash and
successful independent reference records. All sources are standalone; no source
prefix or hidden fixture assembly changes those bytes.

## Accepted behavior

All **17 final run sources pass Rust Debug and Release: 34 source/profile
executions**, with exact fixed stdout, empty stderr, exit 0,
`YAVO_ALLOC_CHECK=1`, and `YAVO_NO_CACHE=1`. Programs return normally so runtime
allocation checking executes. Receipts record commands, compiler and runtime
identities, the locked Zig executable, generated C hashes, executable hashes,
and an end-of-run input hash check.

| Sources | Observable mechanism |
| --- | --- |
| `for_basic`, `for_range_endpoint_order` | Original fixture identity; bounds evaluated left then right exactly once, including an empty reversed range. |
| `for_range_upper_cached`, `for_range_lower_snapshot` | Body changes cannot alter the cached upper bound; a right-bound inout call cannot change the already evaluated left bound. |
| `for_range_edges`, `for_range_byte_hint` | Empty/reversed/negative ranges, byte 254 to 255, int minimum and maximum edges, and symmetric byte-literal context. |
| `for_range_continue` | Continue performs the range step and does not revisit the same value. |
| `for_parameter_plain`, `for_parameter_inout` | Stable ordinary and inout Vec parameters iterate correctly; the inout receiver becomes mutable again after the loop. |
| `for_vec_copy_types` | int, byte, bool, Copy struct and Copy enum elements are copied into loop bindings; empty Vec executes zero iterations. |
| `for_vec_nested_control` | Parenthesized stable names, catch-continue, break, nested same-root read iteration, per-iteration owned locals, and post-loop push/assignment. |
| `for_range_owned_header` | An owned header temporary survives continue and is released after normal or break exit. |
| `for_iteration_exit_cleanup` | Owned locals and iterable storage are released on return, try propagation and terminating catch-return; errors are caught and main returns normally. |
| `for_header_start_break`, `for_header_end_break`, `for_header_outer_continue` | Header catch jumps target the already enclosing loop; right-bound work is skipped if the left bound exits. |
| `for_header_owned_exit` | A left-header String temporary is released when the right header exits through try, catch-return or an outer-loop break. |

The 25 rejection cases independently establish first error codes and original
source positions: E0102 for ten mutation/move/root-lock cases, E0014 for three
readonly loop bindings, E0402 for two replayed moves, E0013 for two possibly
empty loops lacking a return, E0015 for two header jumps without an outer loop,
E0001 for three invalid iterable/range types, E0020 for two malformed range
headers, and E0010 for a binding used after its loop scope.

All ten `unsupported` sources are accepted by Rust stage0. They isolate fields,
temporary calls, literal containers, owned String/struct elements, explicit
mutable iteration, two Vec bindings, and Slice/Map/Set iteration. They remain
U0001 boundaries in the selfhost subset.

## Reference correction and retained failed attempts

The original Rust emitter returned an unmaterialized C local or field expression
for the range start, then emitted the right-bound call before initializing the
loop variable. A right-bound inout mutation therefore changed the start value.
`for_range_lower_snapshot` expects `0\n1\n2\n5\n`; the original reference produced
only `5\n` in both modes. Binding the left value before evaluating the right
corrects this order. The same final fixture passes both modes with the fixed
Rust reference. The incorrect old output was never adopted as its expected
behavior.

The independent Rust P0 regression additionally covers local, field, index,
byte and cached-upper-bound cases. Before the correction, local/field/byte
totals were zero, while index and cached-upper totals were six; its assertion
then failed with exit 101. The same source after the correction produces five
totals of six and returns normally in both modes. `rust-capture-before.json`,
`rust-capture-after.json` and `rust-capture-source.json` retain these identities,
commands, outputs and source bytes.

| Receipt | Initial result and disposition |
| --- | --- |
| `stage0-initial-debug.json` | 25/27 exploratory expectations matched. The range snapshot failure exposed the Rust defect. A move followed by break still produced E0402 and is now the unchanged `for_move_break_replay` rejection source. |
| `stage0-old-snapshot-release.json` | The same old snapshot defect reproduced in Release, 0/1 expected runtime outputs matched. |
| `stage0-fixed-debug-1.json` | 49/52 first candidate expectations matched. The nested-loop sum was initially miscalculated as 54; its actual mathematical sum is 9 + 12 + 15 = 36. Mutable iteration used an invalid assignment to a readonly binding, and a Set candidate used a Vec literal. |
| `stage0-fixed-debug-2.json` | All three corrected expectations/sources pass. The sum source is unchanged; the mutable iterator now reads its element, and the Set uses its constructor/add methods. |
| `stage0-fixed-release-1.json` | All 52 final cases pass, including all 17 native programs. |

`candidates-initial.json`, `candidates.json`, `candidate-history.json` and
`failure-history.json` retain the initial and final sources and the reasons for
each adjustment. Receipts contain source snapshots as well as hashes; failed
attempts are not replaced by their later successful runs. The saved driver
history records its severity-parser and toolchain-lock-binding corrections.

## Historical selfhost review snapshot

The supplied `selfhost-for31-smoke2/yavo-selfhost` passed 51/52 first-error-code
checks. It reported U0001 for `for value in 7`, where Rust reports E0001 because
int is not iterable. Root subsequently changed ForEach to check the iterable
type before applying the stable-name subset restriction. The original receipt
is preserved in `stage1-smoke2-checks.json`; the final full bootstrap validates
that correction instead of rerunning this historical snapshot.

The receipt also retains source-position differences between Rust and the
selfhost checker. The index records those differences explicitly; it does not
claim complete diagnostic-position equivalence. Stage1/stage2 exact output
agreement and all final generated-program execution gates are recorded by the
separate full bootstrap receipts.
