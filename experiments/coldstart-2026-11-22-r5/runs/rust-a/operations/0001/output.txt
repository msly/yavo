# Participant instructions

Complete T01–T12 in order, using only the assigned language and its standard library. This is one independent batch. Implement each task as an executable reading standard input and writing the specified standard output. The entry file is `main.yv` for Yavo or `main.rs` for Rust. Keep T04's required parsing logic in a separate source module and implement T06's required three worker threads. All other structure is up to you.

Use only the `lab.pl` interface described below. Do not inspect the repository, implementation, hidden cases, other runs, previous experiments, or runner source. Do not ask other agents for help or delegate. Do not use network resources, external dependencies, other compilers, or alternative ways to read/write/execute files. No infrastructure or language changes are permitted during the batch.

Every remote tool request must invoke exactly one allowed lab operation. Do not combine several operations or shell commands in a request. Invoke the remote execution tool through `functions.exec` as needed; it is an orchestration envelope, not an additional lab operation. Use its observed output rather than guessing whether a command succeeded. If a remote command is still running, wait for that command using the remote session interface and record that additional poll in your final message; do not start a competing lab operation.

The runner path is `experiments/coldstart-2026-11-22-r5/tools/lab.pl`, from remote cwd `workspace/yavo`. Replace `RUN` with your assigned ID. Available commands:

```
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN doc instructions
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN doc tasks
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN doc T01
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN doc cheatsheet
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN doc cheatsheet 1 180
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN write T01 main.yv
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN read T01 main.yv
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN build T01
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN public T01
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN run T01
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN submit T01
perl experiments/coldstart-2026-11-22-r5/tools/lab.pl RUN seal
```

`doc T01` through `doc T12` return individual task specifications and examples. You may read the full task list instead, but use per-task documents if the tool truncates output. `doc cheatsheet` is available only to Yavo. The frozen sheet has 513 lines; read it in ranges 1–180, 181–360 and 361–513 to avoid output truncation. Optional inclusive START END line numbers work on every document. For Rust, use the language knowledge you already have. `write` reads the entire file from standard input; use a single quoted here-document to pass source bytes without shell interpolation. The Rust filename is `main.rs`; module names may contain ASCII letters, digits, underscores, hyphens and slash-separated subdirectories. The runner saves every file write and takes a complete immutable snapshot before each build. `read` only reads your own source.

`build` uses the fixed compiler and flags, preserves its command and diagnostics, and consumes one of eight build attempts available per task. There is no `check` operation and no way to alter build flags. `public` executes the two public examples and reports exact expected and observed output. `run` accepts your own test input on stdin and reports its output; it does not reveal hidden cases. Use `public` at least once after a successful build before submitting. You may repair code in response to visible diagnostics or public/custom tests, within the eight-build limit. If a task remains unsuccessful, submit its current source and continue; do not conceal the failure.

`submit Txx` permanently freezes that task's final source. Complete the next task without revisiting earlier submissions. After all twelve submissions, call `seal`, then give a short final message stating completion, any unsolved tasks, and any remote-session polling or protocol deviations. Do not estimate tokens or reconstruct timing; the controller records the observable quantities.

Work independently. Hidden tests are run only after all four batches have submitted, and their results are not available for repair.
