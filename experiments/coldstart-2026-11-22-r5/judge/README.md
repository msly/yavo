# Round-five post-submission judge

`evaluate.pl` runs on the project server using core Perl modules. `summarize.py`
and `check_evidence.py` run in a Python 3 environment after the reports and
archived sources are copied there. These tools do not invoke a model or modify
participant solutions. Hidden results must remain outside measured workers'
inputs until all four runs are sealed.

## Inputs

The case pack uses `schema_version: 1` and `tasks`, each with `id`, `structural`
(human-readable mandatory source requirements), and `cases`. Each case specifies
`id`, `visibility` (`public` or `hidden`), `stdin`, `stdout`, `stderr`, `exit_code`,
`args`, and optional `files`. Text is encoded as UTF-8 and compared byte for byte;
the judge does not trim whitespace or normalize line endings.

Each run's `submission.json` has this shape. Paths are relative to that file.

```json
{
  "schema_version": 1,
  "run_id": "yavo-a",
  "language": "yavo",
  "replicate": "a",
  "tasks": [{
    "id": "T01",
    "first": {"source_dir": "tasks/T01/snapshots/0001", "entry": "main.yv"},
    "final": {"source_dir": "tasks/T01/final", "entry": "main.yv"}
  }]
}
```

`compiler-config.json` supplies an argv array for each language, environment
overrides, and a nonempty `identity_files` mapping from absolute path to SHA-256.
The argv placeholders are `{entry}`, `{output}`, and `{source_dir}`. The judge
uses the frozen build mode supplied in this file and never silently enables
optimization. Each pinned compiler/toolchain/runtime file is checked before and
after the run. Shell command strings are not accepted.

```sh
perl judge/evaluate.pl \
  --cases judge/cases.json \
  --submission runs/yavo-a/submission.json \
  --compiler-config evidence/compiler-config.json \
  --output evidence/evaluation/yavo-a
```

The default limits are 120 seconds per build, 10 seconds per test, and 1 MiB per
output stream. A process group is terminated on timeout or excessive output.
Every command saves argv, cwd, explicit environment overrides, stdin, stdout,
stderr, exit status, and monotonic timing. `report.json` also embeds output bytes
as Base64 and binds each source file and executable by hash. Failed builds and
missing submissions are counted separately from judge infrastructure errors.
The judge checkpoints its report after each task. Rerunning it must use a new
output directory if the earlier execution is to remain an independent record.

## Metrics and source requirements

The first source snapshot is the full source graph presented to the first
compiler invocation. Its post-submission replay is separate from the original
compiler invocation recorded by `lab.pl`. The summary exposes both, plus the
intersection of original first-build success with replay correctness.

Public correctness, hidden correctness, and combined behavioral correctness are
distinct fields. A task is fully contract-correct only when all its cases and
mandatory source-structure requirements pass. A source keyword or file count is
not enough to establish actual modular decomposition or threaded work.

Hash-bound manual reviews use this structure:

```json
{
  "schema_version": 1,
  "reviews": [{
    "run_id": "yavo-a",
    "task_id": "T04",
    "stage": "final",
    "source_sha256": "exact source.sha256 from the evaluation report",
    "passed": true,
    "evidence": "Concrete source locations and the requirement they establish."
  }]
}
```

Review first and final snapshots separately, or reuse a judgment only when their
source hashes are identical. An absent or stale review produces an unjudged
contract result, never an automatic pass. If behavior already fails, the task
fails regardless of pending structure review.

```sh
python3 judge/check_evidence.py --cases judge/cases.json --runs runs \
  --output evidence/evidence-check.json evidence/evaluation/*/report.json
python3 judge/summarize.py --runs runs --manual-review evidence/manual-review.json \
  --output results.json evidence/evaluation/*/report.json
```

The evidence checker independently recomputes exact output judgments and totals,
cross-checks snapshot hashes against the first build and final sealed manifests,
and verifies operation sequences and category counts. It does not establish
model-session independence or recover unexposed platform telemetry.

The summary records missing model identifiers, token usage and complete model
tool-call counts as unavailable. Recorded lab operations are a separate metric.
Agent elapsed time includes the interval defined in the run metadata, while
judge replay time is outside agent cost. These quantities must not be combined
or substituted for one another. A report may have complete behavioral results
while `complete_requested_evidence` remains false.

## Judge verification

```sh
python3 judge/selftest.py
```

The self-test uses deliberately tiny fake compiler/program fixtures to verify
exact bytes, Unicode, leading zeros, bad output, build failures, timeouts,
unexpected stderr, nonzero exits, missing submissions, trailing whitespace,
invalid UTF-8, output limits, tampered judgments, and compiler hash mismatches.
These checks are infrastructure tests and contribute no Yavo or Rust score.
