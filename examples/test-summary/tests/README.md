# Independent application acceptance

`cases.json` fixes expected counts, statuses, exit codes and ordered diagnostic
locations directly from `../SPEC.md`. `oracle.py` independently derives those
values using Python regexes, sets and byte decoding. Neither was based on Yavo
program output. `oracle-freeze.json` records their byte identities before the
first application execution. The two archived R4 logs are checked against their
actual committed SHA-256, not regenerated from application output.

There are 72 scenarios, including the contract's 19 explicitly forbidden Unicode
whitespace code points. Each supported host executes 71 scenarios. The one
non-applicable case is documented: POSIX permits TAB/LF/CR in real filenames;
Windows instead exercises native backslash path separators. Every host checks
escaped TAB/LF/CR/backslash in read-error diagnostics.

Run from the repository root after building the stage0 Release compiler and
installing its locked Zig:

```
python3 tools/ci/test_summary.py
```

`--compiler` selects an explicit compiler artifact, `--output` selects the
receipt directory, and `--oracle-only` reviews the frozen expectations without
reading or executing the application. Do not enable Git newline conversion:
the frozen inputs are byte identities, including the intentional CRLF fixtures.

The driver retains every attempt in a new directory, three source snapshots,
all commands and raw stdout/stderr/exit codes. Native Debug and Release execute
the same scenarios. Two Release builds use independent source directories,
identical relative layouts and output basenames, disabled Yavo/PCH caching and
separate initially empty Zig caches. Generated C and binary hashes must match.
Debug and Release program behavior must match byte for byte.

## Explicit error-exit cleanup

The production runtime's `os.exit` does not invoke `yv_rt_exit`. Ordinary
allocation-check builds alone do not certify cleanup on that path.
`exit_check.h` is a test-only forced include: it loads the unchanged runtime,
then redirects generated C's `yv_os_exit` calls to a wrapper that first invokes
`yv_rt_exit` and preserves the requested status when the live-block count is zero.
The original generated C and production runtime are never modified.
The runtime, shim, two probes and application C are copied byte for byte into a
fresh build directory before compilation. Quoted runtime includes resolve to
that bound local copy. The original inputs, compiled copies, three ordinary
source snapshots, emitted C and all executed binaries are rehashed at the end.

Before accepting the instrumented application, a clean tracked String allocation
must be freed and preserve exit 2; a deliberately leaked real String must be
detected and return 102. The instrumented application then executes the same 71
scenarios, with stdout, stderr and status identical to the unmodified artifact.
This measures tracked heap-block balance at explicit exits; it does not detect
use-after-free or untracked native allocations and is not an ASan claim.

The report separates 142 normal application executions, 71 supplementary cleanup
executions and two capability probes. These are executions of one application,
not independent Agent tasks or proof that all SH-4 goals are complete. Actual
model usage and monetary cost remain null.
