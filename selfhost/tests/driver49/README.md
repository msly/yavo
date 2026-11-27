# Batch 49 ordinary compiler-driver gate

The primary contract, 55 scenarios and small independent multi-module program
were frozen before executing or reading the new driver implementation. The
independent native argument probe and the immutable batch-48 Rust compiler both
passed their preflight; `oracle-preflight.json` records that evidence. An initial
missing `of static` annotation was corrected before freezing the small fixture.

- `freeze.json` binds the primary contract, scenarios, fixtures and probe.
- `supplemental-freeze.json` separately binds two later regression scenarios:
  relative temporary roots beginning with `-` and `@`. These follow an actual
  driver-author failure and the review of option/response-file interpretation.
  The original 55 scenarios remain byte-identical.
- Linux/macOS apply **57 scenarios per driver** (55 primary + 2 supplemental);
  Windows applies **54** (52 primary + 2 supplemental). The three POSIX-only
  exclusions are source symlinks and two signal cases. Hardlink source protection
  applies on all platforms.
- The formal gate runs every applicable scenario against both the Rust-built
  stage1 and the actual final stage2 bound by a successful complete bootstrap.
  This means **114 driver invocations on POSIX or 108 on Windows**. Additional
  setup controls, execution of built binaries and the ordinary self-build checks
  are recorded separately, not folded into those scenario totals.
- A final stage2 ordinary `build` of unchanged compiler sources must produce a
  compiler whose `emit-c` matches the bootstrap fixed point and whose ordinary
  `run` executes the frozen multi-module program.

The native probe is a controlled substitute in an isolated pinned-toolchain
directory. Its cases check exact argument boundaries, backend flags, raw
diagnostic stream routing, launch and exit failures, signals and cleanup
precedence. They are not counted as real Zig application builds. The real Zig
cases cover Debug/Release, replacement of a non-source output, default output,
UTF-8 paths, opaque program arguments, inherited streams and nonzero exits.

Run the complete gate after a complete bootstrap:

```sh
python3 tools/ci/selfhost_driver.py --bootstrap-output compiler/target/ci/selfhost
```

A development-only run can use `--candidate PATH --case CASE_ID`. Its receipt is
explicitly preliminary, has no final-stage2 identity claim, and cannot satisfy the
formal CI gate.

Receipts preserve exact streams, commands, exits, hashes, cleanup inventory and
per-command timing/RSS. The gate reuses only an isolated Zig cache, checks that
the isolated Yavo cache remains empty, and removes regenerable Zig caches even
on failure. Unexpected files deliberately left by a probe remain available as
cleanup-failure evidence; the compiler must not recursively delete them.
