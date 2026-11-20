# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. The native compiler prototype in `compiler/` includes ownership, collections, interfaces, generics, scoped OS threads and verified source repairs through `yavo fix`.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- Git tags `v0.1` through `v0.5` identify revision boundaries.

The prototype has been tested on Linux x86-64. It cross-compiles to Linux (static musl), Windows and macOS on x86-64 and aarch64; the output formats are verified, but running them on Windows/macOS is not. `cargo test --test conformance` measures the compiler against the 129 spec cases in `yavo-tests/` (currently 119 pass, 0 wrong, 2 unsupported, 8 not run); full phase acceptance is not claimed. Six fix cases now pass. T-V3 is classified as unsupported because mutable-reference returns and complex mutable receivers are still missing.

Validation records: [source repairs](compiler/validation/fix-2026-11-21.json), and the earlier [scoped-thread, resource-clone and I/O safety review](compiler/validation/safety-2026-11-20.json).

The Git history is local. Nothing is pushed to a remote repository.
