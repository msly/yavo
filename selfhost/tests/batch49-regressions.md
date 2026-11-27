# Batch 49 system-interface regression additions

36 `system49_` cases are appended to `cases.json`: seven run, 22 reject and
seven unsupported. The existing `slice_owned_elements` source is promoted
without changing its ID, path or bytes. All 1,027 earlier source files and the
other earlier registrations remain unchanged from `e4c0302`.

The resulting 1,063-case matrix contains 279 run, 34 panic, 571 reject,
141 unsupported and 38 shared-unsupported registrations. The promoted source
prints `1\n`; its original identity is retained in the manifest's promotion
record. A path under `unsupported/` does not determine the registered behavior.

## Runtime expectations

| Area | Frozen observations |
| --- | --- |
| Environment and host | Missing environment fallback, empty fallback, static nonempty host identifier, invalid environment-name failure |
| Temporary files | Two distinct atomic directory creations, UTF-8 file write/read, native same-file identity, missing-file false, complete explicit cleanup |
| Process calls | Invalid executable failure distinguished from an integer result, borrowed and temporary String argv, left-to-right program/argv/mode evaluation |
| Early exits | A later argument's `try` failure releases earlier temporary program and argv owners without launching a child |
| Read-only String slices | Forwarded parameters, indexed read/clone, iteration, and explicit source-bound string views |
| Invalid paths | NUL rejection for identity/removal and non-component temporary-directory prefixes |

The bootstrap runner explicitly removes `YAVO_BATCH49_ABSENT_75bcac8f` before
execution so the fallback observation does not depend on the user's environment.
All normal cases have exact stdout, empty stderr and exit zero. Executable
fixtures run with allocation checking in both frontend and build-mode variants.
Actual successful child launches, native argument encoding, streams, exit codes,
signals and concurrent process execution are additionally checked by
`compiler/tests/system.rs` and the independent ordinary-driver gate.

## Rejection and unsupported boundaries

The rejection cases cover each public signature, wrong argument types,
conflicting mutations of earlier borrowed arguments, moving out of a read-only
String slice, and returning a view of a dropped local owner. Two probes require
E0001 for returning an indexed `&String` directly as `str`, for both a vector and
a slice; an explicit string slice is the supported view conversion. This exposed
and corrected a broader implicit conversion in the earlier selfhost generator.

Named `os.Error` / `process.Error`, named standard-error catch bindings,
first-class standard Result bindings, and persistent indexed String aliases
remain explicit U0001 boundaries. The new system operations use the existing
discarded-error `catch _ { ... }` form. Their handlers must terminate control
flow, and the runtime borrows all arguments until the operation returns.

The source-preserving promotion and all 36 additions were checked against stage0
before registration. The seven new runtime fixtures and the promoted source
matched their fixed outputs with allocation checking. Registration hashes and
that preliminary reference review are recorded in the batch validation index;
the complete final bootstrap supplies the authoritative differential execution.

The general classification checker remains reusable:

```sh
python3 selfhost/tests/batch48_check_classifications.py \
  --compiler PATH_TO_SELFHOST --output OUTPUT_DIRECTORY --expected-count 1063
```

It checks frontend classifications only; generated C execution and the ordinary
`build/run` contract are separate gates.

Three additional module fixtures require E0011 for standard/user module alias
collisions in both import orders and when the imports are unused. They were
added after an independent audit found an erroneous acceptance; the initial
bootstrap was stopped and is excluded from final acceptance.
