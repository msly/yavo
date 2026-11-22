# Participant instructions

Complete T01–T08 in order using Yavo and its standard library. Each task is a separate executable with entry `main.yv`. Keep T04's required parsing and date logic in a non-entry module; implement T06's two actual OS workers. The tasks and their public examples are supplied by the assigned lab interface.

This is one independent session. Do not read repository implementations, hidden cases, judge code, other sessions, prior experiments, or another document-treatment guide. Do not delegate, use network resources, invoke another language/compiler, add third-party dependencies, or change infrastructure. Use only your assigned lab interface for all document access, source operations and program execution.

Each tool request must contain exactly one allowed lab operation. Use its actual returned output; do not guess whether it succeeded. Do not bypass the interface with shell commands or additional file tools. If the provided transport needs a completion poll, record that poll as a visible protocol event instead of hiding its cost.

Your controller supplies one run ID: `full-a`, `compact-a`, `compact-b` or `full-b`. The runner is `experiments/docs-ab-2026-11-22-r6/tools/lab.pl` in the repository root. Through the assigned tool adapter, these are the operation arguments:

```text
doc instructions 1
doc guide 1
doc index 1
doc api.TOPIC 1
doc T01 1
write T01 main.yv
read T01 main.yv
build T01
public T01
run T01
submit T01
seal
```

Document pages are numbered from 1 and contain at most 8192 UTF-8 bytes without splitting a character. Each response identifies the total page count. **Read every page of your assigned `guide` and the common `index` before your first build.** The runner enforces this. The guide is selected by your run assignment; do not request the other treatment. Both treatments expose the same API topics listed in the common index. Retrieve topics when needed. Read each task's complete specification and examples using `doc T01 PAGE` through `doc T08 PAGE`.

`write TASK FILE` takes the entire source file as the operation's input bytes. Module paths may use ASCII letters, digits, underscores, hyphens and slash-separated subdirectories, ending in `.yv`. `read` reads only your own source. Every write is preserved, and every build takes an immutable copy of the complete source graph before calling the fixed compiler.

`build TASK` uses fixed flags and consumes one of eight attempts allowed per task. There is no separate `check` operation or alternative build path. `public TASK` runs the two public inputs and shows exact expected and observed results. `run TASK` accepts your own stdin and returns the program's observed result without revealing hidden inputs. After a successful build, run public tests at least once before submitting. Repair from visible diagnostics or public/custom tests within the build limit; if a task remains unsuccessful, submit its current source and continue, preserving the failure.

`submit TASK` permanently seals that task's final sources. Do not revisit earlier submissions. After eight submissions, call `seal`, then briefly state completion, unsolved tasks and any transport polls or protocol deviations. Do not estimate tokens or reconstruct timing; the metered controller records actual quantities. Hidden outcomes are unavailable until all four sessions have sealed and never feed repairs.
