# Yavo — Agent-first systems language

This repository preserves the historical specification revisions as individual local Git commits and immutable copies under `history/v0.x/`.

Current specification revision: **v0.5**. A native P0 compiler prototype is now available in `compiler/`.

- `docs/`: current specification documents as of HEAD.
- `history/v0.x/`: exact original version snapshots (no retrospective rewrites).
- `yavo-tests/`: test fixtures introduced in v0.4 and extended in v0.5.
- `tools/`: metadata and toolchain probes introduced in v0.5.
- `compiler/`: Rust P0 frontend, C17 backend, small runtime, pinned C toolchain, and regression tests. See [compiler/README.md](compiler/README.md).
- Git tags `v0.1` through `v0.5` identify revision boundaries.

The P0 prototype has been tested on Linux x86-64. Windows/macOS execution and full P0/P1 conformance are not yet verified. The historical `yavo-tests/` fixtures remain specification tests, not a claim that the whole language has been implemented.

The Git history is local. Nothing is pushed to a remote repository.
