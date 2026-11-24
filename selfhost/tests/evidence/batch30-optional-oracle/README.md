# Batch 30: independent Copy Optional / Vec.pop oracle

Baseline: `e3222d87e6220f2447ec2b9e8d458b6f02e60a21`.
Rust stage0 SHA-256:
`bcbbbf91215b81ad628329320ca9b18b0c924f9735a006af2597d77d4640c2c0`.
The stable copy used during the final probes is
`compiler/target/ci/batch30-stage0/yavo`; rebuilding the ordinary Release
compiler does not change that reference. The recorded Zig is locked 0.17.1.

The batch registers **68 cases**: 15 run, 28 reject, 20 unsupported, and five
shared_unsupported. One is a byte-identical promotion of an existing case, so
the whole manifest grows from 372 to **439 cases**: 127 run, 28 panic, 218 reject,
52 unsupported, and 14 shared_unsupported. There are **155 executable emissions**.

The original `unsupported_vec_pop` is now `vec_pop_basic`. Its source bytes
are unchanged, SHA-256
`f9566d147e9c04e4a97153cc74aa7fbbf79fd5d6498df8a2b50e7e11f65e2446`.
The manifest retains its previous ID, file, kind, baseline and source hash in
the eighth promotion record.

## Scope and executable evidence

The supported payloads are int, byte, bool and non-owning nominal types.
The runtime cases cover writable local/field/inout pop receivers, a single
Optional layer, explicit some/none, automatic success wrapping, assignment,
parameters, returns, isSome/isNone, and ordinary-value, lazy, right-associative
coalescing. The fallback may contain existing try or terminating catch
expressions. Direct `?? return/break/continue`, if-let and match are boundaries.

All **15 final run sources pass stage0 Debug and Release: 30 native executions**,
with exact fixed stdout, empty stderr, exit 0, `YAVO_ALLOC_CHECK=1`, and
`YAVO_NO_CACHE=1`. The sources return normally; no os.exit or panic bypasses
allocation checking. Expectations come from Rust stage0 and source semantics,
not selfhost output.

| Sources | Mechanism |
| --- | --- |
| `vec_pop_basic`, `vec_pop_primitives` | Original fixture identity, empty/nonempty pop, int/bool/byte payloads, and false versus absent. |
| `vec_pop_copy_nominal` | Copy enum and nested Copy struct payloads, with actual returned values inspected through coalescing. |
| `vec_pop_receivers`, `vec_pop_rewind` | Writable nested fields/inout receivers and repeated rollback of a Copy metadata stack. |
| `optional_copy_forms`, `optional_coalesce_order`, `optional_some_shadow` | Constructors, context, assignment, parameter/return wrapping, copying, lazy RHS, precedence, right association, and user-function shadowing of the some prelude. |
| `optional_byte_context`, `result_vec_raw_context` | Raw type context versus constructor payload context for byte and vector values, including existing Result behavior. |
| `optional_fallback_temporaries`, `optional_conditional_move` | Owned argument/vector temporaries on a taken fallback, skipped allocations, and an owner moved on only one branch. |
| `optional_try_fallback`, `optional_catch_return_fallback`, `optional_catch_loop_fallback` | Failure propagation and return/break/continue inside a fallback catch, while caller locals and expression temporaries are live. |

Owned pop and Optional payloads remain explicit U0001 boundaries, including
String, Vec, owned structs and str. Unit, nested Optional, Optional fields/vector
elements, and Optional/Result combinations are also outside this batch.
Internal Copy-stack tests are not presented as proof of owned Optional support.

## Context-sensitive Rust observations

The checked frontend does not generally remove a container from its expected
type before checking an expression. Constructor arguments have their own
payload contexts. Coalesce RHS values are compared without the normal fit
wrapping step.

| Source context | Rust result / registered expectation |
| --- | --- |
| `let x: ?int = none ?? 1` | E0001: expected ?int, found int. |
| `fn f() -> ?int { return none ?? 1 }` | E0001 for the same RHS mismatch. |
| `let x: int = none ?? 1` | Accepted; the expected int supplies the Optional[int] hint for none. |
| `let x = none ?? 1` | E0001: none has no type context. |
| `let x: ?int = some(1) ?? 2` | Accepted; the coalesced int is wrapped when assigned. |
| `let x: ?int = none ?? some(2)` | Accepted by Rust; the hidden nested Optional is subset U0001. |
| `let x: ?byte = 1` | E0001; raw literal inference yields int. |
| `let x: ?byte = some(1)` | Accepted; some supplies a direct byte argument context. |
| `let x: ?byte = some(1) ?? 2` | E0001; the resulting int does not match ?byte. |
| `?byte` initialized by byte(1), a byte literal, or a typed byte variable | Accepted. |
| `let x: byte ! int = 1`, or `return 1` from byte ! int | E0001; Result is not a direct byte-literal hint. |
| `return ok(1)`, a byte literal, or a typed byte variable from byte ! int | Accepted. |
| `return []` from Vec[int] ! int | E0001: the empty vector has no direct Vec element hint. |
| `return [1]` or `return ok([])` from Vec[int] ! int | Accepted. |
| `return none ?? ok(1)` from int ! int | Accepted by Rust; inferred Optional[Result] is subset U0001. |
| `let s: String = none ?? "x".toString()` | Accepted by Rust; inferred owned Optional is subset U0001. |

The independent negatives also distinguish E0403 for
`values.push(values.pop().isSome())` from E0102 when an earlier call argument
already borrows the vector. A fallback may conditionally move an owner, but a
subsequent use of that owner is E0402. Optional comparison and direct printing
are U0001 in Rust as well, so these are shared boundaries rather than claims
that legal Rust source is rejected.

All 33 negative/shared first-error codes and positions are frozen in
`candidates.json` and copied into `oracle-index.json`. The general bootstrap
gate requires the designated code, a valid source location and stage1/stage2
agreement; it does not assert equality to every Rust location.

## Complete attempt history

| Evidence | Checks / native runs | Outcome |
| --- | --- | --- |
| `exploratory-probes.json` | 23 context/ownership + 11 byte + 5 container checks | Every original source, command and diagnostic is preserved, including rejected exploratory inputs. |
| `stage0-matrix-debug-1.json` | 63 checks / 12 Debug runs | 60/63 expected outcomes; three authored sources lacked required separators in same-line declarations. All executed programs passed. |
| `stage0-matrix-release-1.json` | 63 checks / 14 Release runs | 63/63 after the declaration syntax corrections. |
| `stage0-syntax-debug-2.json` | 3 checks / 2 Debug runs | Corrected Copy nominal, rollback and owned-struct boundary sources pass. |
| `stage0-final-context-debug-1.json` | 5 checks / 1 Debug run | Final vector-context, hidden Optional-hint and direct-print cases pass their independent expectations. |
| `stage0-final-context-release-1.json` | 1 check / 1 Release run | The final vector-context run source passes Release. |
| `stage1-attempt2-checks.json` | 68 checks | 67 matching code/exit outcomes; the supplied snapshot predates the direct-print U0001 correction. No unexpected code difference. |

The three initial E0020 failures belong to `vec_pop_copy_nominal`,
`vec_pop_rewind` and `unsupported_vec_pop_owned_struct`. Their enum/struct
declarations were changed to one field/case per line. No stdout expectation
was changed. The original failed source versions remain in the first receipt.

The attempt-2 stage1 hash is
`d1cb6d84a771f4e8b056f1d0b7a15d897695cca63deeba0f4c0fd625cd788553`.
Its direct-print E0001 and location differences are historical observations;
they were not used to rewrite the Rust oracle. Full final bootstrap results
are maintained separately.

`oracle-index.json` matches all final source hashes to per-case proofs and
records 30 final-source profile executions. When a valid case came from the
otherwise failed first matrix attempt, its proof still names that failed
receipt status. All capture receipts rechecked their bound inputs at the end.
Runtime receipts include generated-C and native-binary hashes.

`driver-history.json` preserves both original capture-driver versions.
`candidate-history.json` preserves all historical metadata versions, with
hashes checked against the capture receipts. The failed and successful
attempts are retained in full.

## Replay

From the repository root in the recorded Linux environment:

```sh
YAVO_PYTHON=/home/appops/.yavo/python/cpython-3.13.18-20261110/python/bin/python3
"$YAVO_PYTHON" selfhost/tests/evidence/batch30-optional-oracle/verify_stage0.py --attempt fresh-debug --stage0 compiler/target/ci/batch30-stage0/yavo
"$YAVO_PYTHON" selfhost/tests/evidence/batch30-optional-oracle/verify_stage0.py --attempt fresh-release --profile Release --stage0 compiler/target/ci/batch30-stage0/yavo
```

An independently rebuilt stage0 can be selected with --stage0. The helper
refuses to overwrite an attempt and writes replay output under the ignored
`compiler/target/ci/batch30-oracle/replay/` directory. --only selects individual
case IDs. The normal three-platform CI uses `selfhost/bootstrap.py`.
