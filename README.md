# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. The native compiler prototype in `compiler/` includes ownership, collections, interfaces, generics and scoped OS threads.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- Git tags `v0.1` through `v0.5` identify revision boundaries.

The prototype has been tested on Linux x86-64. It cross-compiles to Linux (static musl), Windows and macOS on x86-64 and aarch64; the output formats are verified, but running them on Windows/macOS is not. `cargo test --test conformance` measures the compiler against the 129 spec cases in `yavo-tests/` (currently 113 pass, 0 wrong, 1 unsupported, 15 not run); full phase acceptance is not claimed. The scoped-thread, resource-clone and I/O safety review and separate sanitizer results are recorded in [compiler/validation/safety-2026-11-20.json](compiler/validation/safety-2026-11-20.json).

The Git history is local. Nothing is pushed to a remote repository.
