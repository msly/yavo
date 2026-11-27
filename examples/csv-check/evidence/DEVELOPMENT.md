# Application development record

Date: 2026-11-27. Repository baseline: `84cd8ef`.

The author read the frozen CSV contract, the test-summary application contract
and source, and the current language and standard-library documentation. Four
complete modules were written before the first compiler invocation. There were
no partial-program compiler probes. The implementation was not given the
independent oracle's outputs.

## Complete-project compiler attempts

Every attempt recorded its command, environment, compiler/runtime/toolchain and
source hashes, pre-invocation source snapshots, output and elapsed time in the
ignored directory `compiler/target/batch47/csv-authoring/NNN`. The checked-in
record is this compact summary, following decisions.md section 9.

| Attempt | Compiler and operation | Result | Seconds |
| --- | --- | --- | ---: |
| 001 | Frozen stage0, `check --json` | Exit 0; no errors; one W0003 unused import | 0.012373 |
| 002 | Frozen stage0, Debug `build --emit-c --json` | Exit 0; no errors or warnings | 0.265236 |
| 003 | Frozen stage0, Release `build --release --emit-c --json` | Exit 0; no errors or warnings | 0.793488 |
| 004 | Batch46 stage2, `check` | Exit 1; U0001 at validator.yv:15:20, Set construction outside its subset | 0.001737 |
| 005 | Batch47 candidate stage1, `check` | Exit 0 | 0.002316 |
| 006 | Batch47 candidate stage1, `emit-c` | Exit 0 | 0.003130 |
| 007 | Batch47 candidate stage2, `check` | Exit 0 | 0.002869 |
| 008 | Batch47 candidate stage2, `emit-c` | Exit 0 | 0.002993 |

All commands use `examples/csv-check/main.yv` as the complete-project entry.
Stage0 Debug and Release builds set `YAVO_ALLOC_CHECK=1` and `YAVO_NO_CACHE=1`;
they use output basenames `csv-check-debug` and `csv-check-release`. The latter
flag bypasses the program build cache; these authoring times do not measure a
controlled cold runtime-PCH/backend environment or establish a performance gate.

After attempt 001, the unused `use parser` line was removed from main.yv. No
behavioral source repair was required. The application was then frozen before
attempt 002. The older selfhost compiler's Set rejection was preserved; the
application was not rewritten to bypass it. Batch47's compiler support accepted
the same frozen source. All eight attempts verified their source files unchanged
while the compiler ran. These are three stage0 invocations, one older-selfhost
rejection and four candidate compiler invocations, not independent deliveries.

Stage1 and stage2 emitted identical C: 40,789 bytes, SHA-256
`f48fda138642c420883044e59c02eec0bc6bf449017e240abc3757355f334a16`.
The author's stage0 Release executable is 30,520 bytes; the Debug executable is
4,173,672 bytes. Independent acceptance builds have their own identities and
timings and must not be combined with these authoring observations.

## Frozen identities

| Input | SHA-256 |
| --- | --- |
| SPEC.md | `4a87a12f941122c8dea2244c87ba0124ad72311f20fe1369ba8814237da61a89` |
| main.yv | `878624551562be406ccab79818450359ec5557eebd04edc9526343135f2b2584` |
| parser.yv | `10fb26268e3baac19205349a5d8c4bb72f5c1e54fa34f83adbd5c80a36d6220a` |
| validator.yv | `5a4dc8c5c3f6e041fec6331cbb232c555875059be035ffae8a708829ba005b64` |
| report.yv | `89a1bc7d4818b7eb39e9add9d4ac5ed2338529fa100d1ca7156e3d1ff09013a0` |
| compiler/runtime/yavo_rt.h | `b1ac73b27e3620a353cb62dff7ebe6b3ef7b945f7a8b97ae7efcc747bd4ffdfc` |
| compiler/toolchain.lock | `a9bbe83e644b0e1cee19c8dba8b5157eaed3e9ddc71b96d8aab2b2d2bb15f53c` |

| Compiler | SHA-256 |
| --- | --- |
| Frozen stage0 | `0aa26645f51df9f0ddb1429f9bbf7b32145eca0a513193deffb7d81f0e262992` |
| Batch46 stage2 | `cebfe5fe704adbe56ca7c44b6462a19503050d569109af96dec6a48bd08d8aa6` |
| Batch47 candidate stage1 | `72c16b5d7ce94645703c12fb6397bea04df15becf07d053386bef86fece7dcc7` |
| Batch47 candidate stage2 | `7ac3ecaf42a8f4130fec77cf51bfbc33a516800a23e437589304a77c1afc1aa4` |

The initial main.yv with the unused import had SHA-256
`292b524e6174021209c6b8ba2999898227e31a383e36554b67dc4feb822a9fa6`;
the other three modules were unchanged. Provider token usage, serving-model
identity, sampling parameters and monetary cost were unavailable and remain
unknown. Compilation counts and file sizes are not substitutes for those data.

## Independent acceptance

The independent fixed fixtures and oracle, resource-check controls, native
platform runs and reproducibility builds are separate from the authoring
attempts above. Subsequent results or repairs must be appended, preserving this
initial source and compile history.
