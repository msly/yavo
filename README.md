# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. The native compiler prototype in `compiler/` includes ownership, collections (including Heap and Deque), saved owning comparators, mutable-reference returns, interfaces, generics and scoped OS threads. Tools include `yavo fix`, `yavo allocations` and evidence-bound `yavo verify` reports.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- Git tags `v0.1` through `v0.5` identify revision boundaries.

The prototype has been tested on Linux x86-64. It cross-compiles to Linux (static musl), Windows and macOS on x86-64 and aarch64; the output formats are verified, but running them on Windows/macOS is not. `cargo test --test conformance` measures the compiler against the 129 spec cases in `yavo-tests/`: **126 pass, 0 wrong, 3 unsupported, 0 not run**. The remaining cases (T-GR9, T-P3 and T-R2) require ASan, whose runtime cannot link with the pinned toolchain on this host. The runner checks available TSan profiles independently and does not turn unavailable instrumentation into a pass.

This count is fixture coverage, not a percentage of v1 completion. Arena/Id, user modules and packages, the user-facing test/audit toolchain, full platform execution, near-C performance and agent cost acceptance remain open. [HANDOFF.md](HANDOFF.md) records current progress, implementation boundaries and the full remaining acceptance work.

Debug and Release suites each pass **88 tests**, with zero failures or compiler warnings and one legacy report ignored. The 91 compile/run fixtures include native heap-block checks and both C optimization modes; the new collection/reference combination also passes all six cross-build format checks.

Validation records: [feature completion](compiler/validation/completion-2026-11-21.json), [allocation and verification tools](compiler/validation/verify-2026-11-21.json), [source repairs](compiler/validation/fix-2026-11-21.json), and the earlier [scoped-thread, resource-clone and I/O safety review](compiler/validation/safety-2026-11-20.json).

The Git history is local. Nothing is pushed to a remote repository.
