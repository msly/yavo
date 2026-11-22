# R6 packet and independent oracle

`generate.py` recreates the task specifications, two public inputs per task, twelve hidden inputs per task, and `packet-manifest.json`. It uses no participant source. The eight task IDs are T01–T08; the seed is 2026112206.

```sh
python3 judge/generate.py --self-check
python3 judge/generate.py
```

The oracle checks include independent recurrence/enumeration/formula/grid/graph implementations and published-input bound checks. The output count is 8 specifications, 16 public inputs, 96 hidden inputs and 20 hashed packet files. `packet-manifest.json` does not replace the controller's broader freeze receipt, which must also bind document treatments, common API topics, compiler, recorder and metering qualification.

Only T04 (real module decomposition) and T06 (real worker-thread decomposition) need separate structural source review. Behavioral output alone cannot certify those requirements. The review must bind the exact first/final source hashes.

Packet preparation and oracle self-checks are not participant results. Per PROTOCOL.md, no participant may start until real token usage, exact serving model identity and request/tool transcript recording have been demonstrated by live executor probes. An unqualified environment leaves this packet prepared and blocked with zero sessions.

## Post-submission replay and summary

`evaluate.pl` rejects hidden judging until `full-a`, `compact-a`, `compact-b` and `full-b` are all sealed under one runtime freeze and qualification. It verifies their first/final source identities before creating an output directory. The frozen case pack, compiler configuration and judge identity must match. Each new output directory retains exact build/run commands, stdout/stderr bytes and first/final results; previous replay outputs are never overwritten.

After all four sessions seal, invoke the judge once per submission:

```sh
perl judge/evaluate.pl --experiment-root . --cases judge/cases.json \
  --submission runs/full-a/submission.json \
  --compiler-config evidence/compiler-config.json \
  --qualification evidence/launch-qualification.json \
  --output judge-results/full-a
```

Repeat with the other three IDs. Record actual T04/T06 source reviews for every first/final stage in a JSON `reviews` array with `run_id`, `task_id`, `stage`, `source_sha256`, `passed` and concrete `evidence`. Missing or stale reviews leave structural correctness unmeasured. Source aggregate hashes are SHA-256 of the canonical compact JSON filename→hash mapping, independently recomputed by `check_evidence.py`.

`summarize.py` groups by document arm rather than language. It first rechecks behavioral evidence, then joins real provider usage only through `tools/agent_eval/audit.py`, which replays the saved request→lab-result→continuation chain and all identities. Supplying only copied token totals or lab `run.json` metadata cannot populate cost fields. Unavailable, incomplete or unverified usage remains `null`. All session requests, including those used on failing tasks, stay in the success-cost numerator.

```sh
python3 judge/summarize.py judge-results/full-a/report.json \
  judge-results/compact-a/report.json judge-results/compact-b/report.json \
  judge-results/full-b/report.json --runs runs --cases judge/cases.json \
  --manual-review evidence/manual-review.json \
  --executor-runs evidence/provider-runs --executor-config EXECUTOR_CONFIG.json \
  --qualification evidence/launch-qualification.json --output results.json
```

These commands are post-launch examples, not instructions to bypass the currently unmet launch gate. `python3 judge/selftest.py` runs only temporary synthetic fixtures for cohort gating, grouping, unknown costs, behavioral failures and structural review binding; it makes no service request and produces no experiment score.
