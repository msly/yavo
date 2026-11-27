# Batch 48 regression additions

68 `batch48_` cases are appended to `cases.json`: 27 run, 2 panic, 25 reject,
and 14 unsupported. Two existing unsupported cases are additionally promoted
without changing their IDs, source paths or source bytes; the other 957 earlier
registrations and all 32 existing promotion records are preserved. The final
manifest contains 1,027 cases and 34 promotion records.

## Source-preserving promotions

| Existing source | Stage0 check/run | Fixed stdout |
| --- | --- | --- |
| `unsupported/unsupported_vec_nested_element.yv` | Both exit 0, empty stderr | `1\n` |
| `unsupported/tuple43_pair_owned.yv` | Both exit 0, empty stderr | `x\n` |

The two sources are byte-identical to baseline
`5e527961eba9fbf4547251b42a8b1f21daff5b05`. Their original SHA-256 identities are
recorded in the new promotion records and in
`compiler/target/batch48/promotion-review.json`. The manifest permits a `run`
entry to retain its source under `unsupported/`; directory names do not determine
case behavior. `promotions` records provenance while `cases` controls execution.

Final classifications: 271 run, 34 panic, 549 reject, 135 unsupported and
38 shared_unsupported. Ordinary `[]String`, `?String` and `?Vec[int]` boundaries
remain unchanged; the owned-named slice and owned-tuple additions do not promote
them.

## Runtime coverage

| Area | Run cases | Behavior fixed by stage0 |
| --- | ---: | --- |
| `Heap[(String, int)]` | 9 | ASCII key ordering, secondary key ties, 17 equal elements, empty/refill, pure unary/binary comparators, continue/break/return cleanup, Optional payload returns through an inout function |
| Flat owned tuples | 5 | Recursive function returns, mixed Copy fields, move/reassignment, multiple String fields, sink return |
| `Vec[Vec[int]]` | 6 | Deep clone independence, returned and moved storage, nested iteration, indexed push loops, index side effects evaluated once |
| `[]OwnedNamed` | 4 | Read-only field access, persistent element aliases and alias chains, forwarded iteration, empty inputs and temporary owners |
| Sink and Optional follow-up | 3 | Owned sink parameters on normal/early return, argument-staging cleanup, Copy sink reuse, Optional payload discard and source-bound view returns |

Two additional nested-vector cases freeze outer and inner out-of-bounds behavior
as `P-INDEX` with exit 101. All 27 normal runs exit 0 with empty stderr under
`YAVO_ALLOC_CHECK=1`; normal, continue, break and return paths are included.

## Rejection and unsupported boundaries

The 25 rejection cases cover use after move, repeated sink transfer, moving a
read-only parameter, immutable/read-only Heap mutation, wrong tuple payloads,
comparator arity/result types, read-only slice alias writes/moves, conflicting
slice arguments, nested indexed-push receiver conflicts, Heap equality/ordering,
borrow-before-sink and sink-before-borrow argument ordering, returning a view of
a dropped sink parameter, explicit owned moves from Optional payload borrows,
and a borrowed payload view escaping its declared source.

The outer nested-vector owner remains locked while evaluating both the index
and push argument. Stage0 reports `E0403` for all three explicit mutation/move
probes. Slice alias writes produce `E0101`; returning an alias as an owner
produces `E0404`; a simultaneous slice and inout owner argument produces `E0102`.

The 14 unsupported programs are accepted by stage0. They cover nested String
vectors, three Vec levels, Heap elements other than `(String, int)`, Copy/owned
captures, block comparators, comparator user calls, and ordinary String/Vec
Optional payloads, inferred aliases of borrowed owned Optional payloads, and
imported standard `io.Error`/`fs.Error` types.
Heap clone (`E0001`), Heap peek (`U0001`) and stored general
closures (`U0001`) are registered as **reject**, because stage0 rejects them.

## Stage0 evidence

The reference binary is `compiler/target/release/yavo`, SHA-256
`3db1eb23f6abf2478081f2e805e0491fac00d71800ef699f796a76710cf2fddd`.
Each source was checked with `yavo check FILE --json`; every run/panic source
was executed with `yavo run FILE`, `YAVO_ALLOC_CHECK=1`, and `YAVO_NO_CACHE=1`.
The final source hashes, actual diagnostic codes/messages, stdout/stderr, exits
and single-run durations are recorded compactly in
`compiler/target/batch48/regression-stage0-results.json` and
`compiler/target/batch48/supplemental-regressions-stage0.json`; the two standard
error type checks are recorded in `compiler/target/batch48/standard-error-stage0-review.json`.
The final manifest and 68 added source identities are frozen in
`compiler/target/batch48/regression-registration-freeze.json`.

The stage0 review has no unresolved anomalies. The manifest was checked with
`bootstrap.validate_cases()` without invoking bootstrap execution. These results
establish reference behavior; selfhost differential execution is a separate gate.

## Repeatable classification scan

`selfhost/tests/batch48_check_classifications.py` runs the complete manifest with
an explicitly supplied compiler, using the same `source_prefix` byte assembly
and diagnostic predicates as bootstrap. It checks no generated C and executes
no fixture application. Source, prefix, module and compiler identities are
bound before checking and verified afterward.

```
python3 selfhost/tests/batch48_check_classifications.py --compiler compiler/target/batch48/candidate5 --output compiler/target/batch48/classification-candidate5 --expected-count 1027
```

The final candidate5 scan passed all 1,027 classifications, including three
assembled prefix cases, with zero changed inputs. Its receipt is
`compiler/target/batch48/classification-candidate5/results.json`.
