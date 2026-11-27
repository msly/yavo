# Ordinary selfhost driver acceptance, batch 49

Frozen against baseline e4c0302 before executing the new driver. This is an independent native build/run acceptance gate; it does not claim complete SH-3, structured diagnostics, caching, installer support, cross compilation, or cost/performance targets.

## Commands and failure order

- `build <input.yv> [-o <output>] [--release]`, with options before or after input. Default output is the input basename with a final `.yv` removed, except basename `.yv` is preserved; Windows appends `.exe`.
- `run <input.yv> [--release] [-- program_args...]`. Arguments after the separator are opaque strings, including empty strings and option-like values. Run rejects `-o`.
- Duplicate options, extra inputs, unknown options, and `--target`, `--json`, or `--emit-c` return 2. Existing strict `check` and `emit-c` forms remain available.
- Order: parse arguments; read and compile the complete module graph; protect source/output identities; resolve environment and locked backend; create a unique temporary directory; write C/header; execute backend; execute program for run; clean known temporary files and the now-empty directory.
- All reachable source files are protected from output aliases, including existing hardlinks and symlinks. This is an identity check, not just a spelling check. Source-identity rejection returns 2. Existing non-source output may be replaced.
- Invalid source returns 1 and leaves a pre-existing output untouched. Missing/unreadable/invalid-UTF-8 input returns 3. Missing imported modules return 1 with E0010. The selfhost language subset retains U0001 for unsupported programs.

## System boundary

Runtime header and pinned version are embedded in the compiler, with generated-source freshness checked independently. A compiler invoked outside the repository does not read a runtime header from the working directory.

The pinned native executable is `YAVO_HOME/toolchains/zig-HOST-VERSION/zig[.exe]`. If YAVO_HOME is absent, use HOME/.yavo on POSIX or USERPROFILE/.yavo on Windows. A defined empty home is an error. A nonempty YAVO_CC is rejected (3); an empty value is ignored.

On POSIX use TMPDIR, default /tmp. On Windows use TEMP, then TMP, then the current directory. Defined empty temporary roots are errors. Unique directories must not alter pre-existing neighbors. Only known compiler files may be removed; an unexpected residual file makes cleanup fail visibly rather than being recursively deleted.

Backend execution uses an explicit executable path and argv, without a shell or PATH lookup. Flags include cc, -std=c17 and -fno-strict-aliasing; Debug uses -O0 -g; Release uses -O2 -s. Backend stdout and stderr both flow to driver stderr without byte transcoding. Program stdin/stdout/stderr are inherited. Arguments must retain UTF-8, empty strings, quotes, backslashes, and shell metacharacters.

Backend launch failure returns 3; backend nonzero/signal failure returns 4. Run launch failure returns 4. Ordinary program exits propagate, and POSIX signals map to 128+signal. Failure to remove the temporary directory returns 4 and includes "cannot remove temporary directory", overriding earlier status so cleanup failure remains visible. Backend build output is not promised atomic on failure.

## Independent cases and evidence

`cases.json` defines expected exits, stdout, stderr constraints, preservation, backend argv, and cleanup state. `main.yv` plus `utility.yv` is the same small independent multi-module program for all successful language builds. `native_probe.c` is a test-only native executable used in an isolated pinned-toolchain location; it records argv and produces controlled backend failures or child exits. Probe cases are reported separately from real locked-Zig builds.

The same applicable cases run against the Rust-built stage1 and the actual final stage2 from a successful complete bootstrap receipt. A separate final-stage2 ordinary build of the unchanged compiler modules must yield a compiler that emits the fixed-point C bytes. Every receipt binds source, contract, fixture, harness, compiler, runtime, toolchain lock, and executable identities and verifies them again after execution.

POSIX-only cases test symlinks and signals; platform exclusions are explicit. Every case checks a pre-existing temporary-root sentinel, expected temporary residue, and an untouched isolated Yavo program-cache directory. Zig's own cache is isolated but reusable within this gate; this is not a cold-build benchmark. Timings/RSS are observations, not P50 claims.
