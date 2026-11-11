# Yavo v0.5 - ASCII Filename Release

This is a filename-only repackaging of the Yavo v0.5 artifacts. The content of design documents and tests is unchanged.

| Filename | Description |
|---|---|
| plan.md | Project goals, compiler and cross-platform roadmap |
| semantics.md | Ownership, borrowing and resource semantics |
| cheatsheet.md | Agent quick syntax and standard library guide |
| stdlib_contract.md | Standard library API contracts |
| decisions.md | Gap closure decisions and changelog |
| yavo-tests/ | 129 test cases, manifests and expected outcomes |
| tools/ | Python validation and C compiler capability probes |
| patches/ | Diffs against v0.4 documents and tests |

## Validation

Run: `python3 tools/check_package.py`

This checks the test package structure. No Yavo compiler was available, so none of the Yavo test cases have been executed.

All paths in this ZIP are ASCII-only. Some document contents are in Chinese.
