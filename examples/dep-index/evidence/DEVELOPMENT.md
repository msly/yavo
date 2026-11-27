# yavo-dep-index development record

Batch 48; application development began from repository commit
`5e527961eba9fbf4547251b42a8b1f21daff5b05` and the frozen `SPEC.md` contract.

## Inputs and independence

- Checked `/AGENTS.md`, ancestor directories through the repository, the repository
  root, `examples`, and `examples/dep-index`; no `AGENTS.md` was present.
- Read the frozen application contract, `docs/agent-start.md`, the official API
  index, and the relevant syntax/type/result/ownership/string/collection/filesystem/
  module sections of the official language documentation. Requested compiler README
  output exceeded the tool preview; no unread portion is represented as read.
- Did not read application fixtures, oracle, expected outputs, selfhost source, or
  selfhost capability lists. Development uses the Rust stage0 compiler.

## Source changes and complete compiler invocations

1. Initial implementation: five modules (`main`, `parser`, `graph`, `report`,
   `model`). Data is owned `Module` / `Dependency` / `Failure` structures. Parsing
   scans physical LF-delimited lines with explicit CRLF handling and ASCII name
   validation. Dependencies preserve first-use order and use a set for duplicates.
   Graph processing uses a name-to-index map, reverse adjacency vectors, dependency
   counts, and a minimum heap of `(String, int)` ready entries. Rendering is deferred
   until the complete graph succeeds. `main` only receives an integer from `run`
   before calling `os.exit`, so application state and caught errors return normally.

2. Complete check 1: `compiler/target/release/yavo check
   examples/dep-index/main.yv --json`; exit 1, 0.002734 s, 13,748 KiB child
   maximum RSS. Three `E0020` diagnostics in `report.yv`: Yavo requires `else`
   on the same physical line as the preceding closing brace. Raw output and
   timing are in `compiler/target/tmp/batch48-dep-dev/check-01.*`.
3. Source correction: joined `escapePath`'s four short conditional branches so
   each `else` follows its closing brace on the same line. No logic changed.
4. Complete check 2: same command; exit 0, zero diagnostics/warnings, 0.005890 s,
   13,880 KiB child maximum RSS. Stage0 project SHA-256:
   `a48f8f8dc9879fa5101efb67d72232a91f61c23a575edaa983fa4f6992d84f64`.
   Raw output and timing are in `compiler/target/tmp/batch48-dep-dev/check-02.*`.
5. Complete build 1: `YAVO_NO_CACHE=1 compiler/target/release/yavo build
   examples/dep-index/main.yv --emit-c
   compiler/target/tmp/batch48-dep-dev/dep-index-debug.c -o
   compiler/target/tmp/batch48-dep-dev/yavo-dep-index-debug --json`;
   exit 0, zero diagnostics/warnings. Elapsed 0.277328 s; child maximum RSS
   86,080 KiB; generated C 63,087 bytes; Debug executable 4,206,872 bytes.
   Raw output/timing: `compiler/target/tmp/batch48-dep-dev/build-01.*`.
6. Complete build 2: the same command with `--release` and `-release` artifact
   names; exit 0, zero diagnostics/warnings. Elapsed 1.041887 s; child maximum
   RSS 99,248 KiB; generated C 63,087 bytes; Release executable 38,272 bytes.
   Raw output/timing: `compiler/target/tmp/batch48-dep-dev/build-02.*`.
7. Ran 14 independently designed development samples in each build mode:
   help, missing operands, argument validation before reads, a DAG with duplicate
   uses and a newly-ready name preceding an independent ready module, missing
   file, duplicate module, unresolved dependency, a residual cycle diagnosis
   whose selected module is outside the actual cycle, UTF-8 failure before
   syntax, DEL inside a comment, initial BOM, bare final CR, missing module, and
   a Unicode path containing backslash/TAB/LF/CR. All 28 runs matched the manually
   specified status/output/location checks. These samples are not the independent
   application oracle. Inputs and captured results are under
   `compiler/target/tmp/batch48-dep-dev/manual-samples/` and
   `compiler/target/tmp/batch48-dep-dev/manual-samples.json`.

Elapsed times are single wall-clock observations from `time.monotonic`; maximum
RSS is Python `getrusage(RUSAGE_CHILDREN).ru_maxrss` in one fresh Python process
per compiler invocation on Linux. They are not P50 measurements. No changes to
the compiler, runtime, runner, contract, or acceptance tests were made here.

## Stable source after stage0 development

| File | SHA-256 |
| --- | --- |
| main.yv | b7b0be2648b326d20fe8d1fe9285649992df59a5cf155e9418d2812b706e683f |
| model.yv | 487760f52140c8d60ecbfc98fcf941f00ecbf53d43766a88ae0371fc34488330 |
| parser.yv | 1517dc1190107d3a603fbaafb487b071eeae922110354d1911649915854526fe |
| graph.yv | 97201407b1e5a51e794719de1345ec27c9bf303dea4e2b5eb9bf9e559bc11c25 |
| report.yv | a77af0ced5d7cfd7cf0b5cd4ac09f668bbf3e2edb6ed63c0e2d106cd7fcfbaa2 |

Both successful builds report project SHA-256
`a48f8f8dc9879fa5101efb67d72232a91f61c23a575edaa983fa4f6992d84f64`.
There were two complete checks and two complete builds in this implementation
session. The only application correction was the documented `else` syntax fix;
the natural data model and graph algorithm were preserved. Selfhost, sanitizer,
independent rebuild, and native multi-platform acceptance remain the independent
validation team's work and are not claimed by this development record.

From the repository root, a simple development run is:

```sh
compiler/target/tmp/batch48-dep-dev/yavo-dep-index-release --help
compiler/target/tmp/batch48-dep-dev/yavo-dep-index-release \
  compiler/target/tmp/batch48-dep-dev/manual-samples/entry.dep \
  compiler/target/tmp/batch48-dep-dev/manual-samples/spare.dep \
  compiler/target/tmp/batch48-dep-dev/manual-samples/core.dep
```

No model-token estimate or synthetic edit/check count is recorded.
