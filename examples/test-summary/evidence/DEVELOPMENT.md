# Application development record

Date: 2026-11-23. Baseline compiler repository commit: `394f1b8`.

The application author read the frozen application contract and the current
`docs/agent-start.md`, API index and relevant cheatsheet sections. Four complete
source modules were then written before the first compiler invocation. No
partial-program compiler probes preceded the recorded attempts.

## Compiler attempts

| Attempt | Operation | Result | Source repair after the attempt |
| --- | --- | --- | --- |
| 001 | Complete-project `check --json` | Exit 0, zero errors and warnings | None |
| 002 | Debug build, allocation checks, backend cache disabled | Exit 0, zero errors and warnings | None |
| 003 | Release build, allocation checks, backend cache disabled | Exit 0, zero errors and warnings | None |

The initial source version passed all three operations without source changes.
This is one authored source version, three compiler invocations and zero
compile-repair rounds; the Debug and Release builds are not independent agent
deliveries. Snapshot hashes and complete compiler output are in each numbered
`compile-attempts/` directory. The first check did not set the allocation/cache
environment overrides; attempts 002 and 003 record their explicit overrides.

Both actual archived round-four verification logs were run through the Debug
binary. Each yielded 18 passed, 0 failed, 0 skipped, 4 passed requirements,
0 failed requirements, `ok`; exit 0 and empty stderr. The complete command,
input hashes, output and binary hash are in `real-input-smoke.json`.

No compiler, runtime or language-rule change was required. API usage is limited
to existing modules, arguments, UTF-8 text reads, strings, vectors, maps, result
errors and integer operations. Later independent acceptance results and any
subsequent repairs must be appended rather than replacing this authoring record.

Token usage, serving-model identity, sampling parameters and monetary cost were
not exposed by this authoring environment. They are unknown; the compile count
is not a substitute for these metrics.
