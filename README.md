# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. A native P0 compiler prototype is now available in `compiler/`.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust P0 frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- Git tags `v0.1` through `v0.5` identify revision boundaries.

The P0 prototype has been tested on Linux x86-64. It cross-compiles to Linux (static musl), Windows and macOS on x86-64 and aarch64; the output formats are verified, but running them on Windows/macOS is not. `cargo test --test conformance` measures the compiler against the 129 spec cases in `yavo-tests/` (currently 29 pass, 0 wrong, 87 unsupported, 13 not run); full P0/P1 conformance is not claimed.

The Git history is local. Nothing is pushed to a remote repository.
