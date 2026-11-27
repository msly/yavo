# Independent CSV acceptance

`cases.json` contains 139 manual contract cases. `oracle.py` is an independent
byte parser and argument validator; it does not read Yavo source or application
output. `oracle-freeze.json` binds both files and `../SPEC.md` before application
execution. Both manual results and oracle must agree without regenerating the
expected results. POSIX runs 138 cases and Windows runs 137; unsupported filename
and directory-stdin probes are recorded as not applicable, never as passed.

The cases cover full CSV records, UTF-8 byte columns, record-versus-lexical error
priority, all 30 forbidden ASCII control bytes, header identity, widths, stdin,
file failures, argument precedence, multiple-input atomic output, and path
escaping. Exact stdout and exit status are fixed. Diagnostic path, code,
location, line count, and nonempty explanation are checked; explanation wording
is deliberately outside the contract.

From a clean checkout, install the repository's locked tools first, then run:

```sh
python tools/ci/csv_check.py --oracle-only
python tools/ci/csv_check.py --compiler compiler/target/release/yavo --sanitizers auto
python selfhost/bootstrap.py --stage0 compiler/target/release/yavo
```

The native driver accepts `--output` (default `compiler/target/ci/csv-check`).
Linux CI uses `--sanitizers required` and supplies the completed bootstrap output
through `--bootstrap-output`; the sanitizer gate independently emits the CSV C
from the recorded final selfhost compiler and exercises both frontend products.
Other native platforms use `--sanitizers off`, recorded as not applicable.
No gate installs a missing toolchain automatically.

The shared helper is `tools/ci/csv_support.py`. The bootstrap wrapper
`selfhost/csv_application.py` tests ordinary stage0/selfhost Debug and Release
programs and supplementary Release programs built with the existing byte-bound
`examples/test-summary/tests/exit_check.h`. Clean/leak capability controls verify
that the shim detects outstanding tracked allocations on explicit process exit.
Generated C and the production runtime are unchanged. Raw command, stdin,
stdout, stderr, output status, elapsed time, and identities are retained. The
wrapper deletes materialized case files after each scenario, preserving safe
artifact filenames even for CR/LF path cases. The native delivery driver retains
its isolated case inputs in the packaged evidence.

Two independent cold builds compare C and binary bytes; separate unchanged and
comment-edited source builds measure their actual configurations. Measurements
are observations, not performance thresholds. Unmeasured memory, provider token
counts, and cost remain null or explicitly unavailable.
