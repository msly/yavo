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

## Independent acceptance

The same four application modules subsequently passed the independent local
gate and all three native platforms in
[CI run 45174913978](https://github.com/msly/yavo/actions/runs/45174913978),
at `2a6480802bc222fae139b8f9458b4b9eee60a5d4`. Each platform passed 71 applicable
scenarios across Debug, Release and a separate exit-cleanup helper (213
executions), with two independent Release C/binary identities matching.
The three downloaded native archives were checked against their GitHub
size/SHA-256 and committed application sources; actual outputs and exit codes
were inspected. The [native index](../../../compiler/validation/batch28-native-2026-11-23.json)
retains the final evidence and the initial artifact-upload failure separately.

That upload failure came from deliberate CR/LF fixture filenames. Packaging
their original names, bytes and empty directories inside tar.gz fixed the
artifact-service restriction; application source and the frozen oracle did not
change. Independent acceptance builds are separate from the three authoring
compiler invocations above. The authoring count remains one source version and
zero compile-repair rounds; usage and monetary cost remain unknown.
