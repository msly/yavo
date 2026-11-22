#!/usr/bin/env python3
"""Synthetic contract tests only; never participant or live-provider evidence."""
from __future__ import annotations

from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
RUNS = ("full-a", "compact-a", "compact-b", "full-b")


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def module(name):
    spec = importlib.util.spec_from_file_location("r6_test_" + name, HERE / (name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def main():
    summary = module("summarize")
    checker = module("check_evidence")
    judge = HERE / "evaluate.pl"
    scenarios = []
    with tempfile.TemporaryDirectory(prefix="yavo-r6-judge-contract-") as temp:
        base = Path(temp)
        fake = base / "fake_compiler.py"
        fake.write_text("import sys,shutil,os\nsrc,out=sys.argv[1:]\nif '# BUILD_FAIL' in open(src).read(): sys.exit(1)\nshutil.copyfile(src,out)\nos.chmod(out,0o700)\n")
        config = base / "evidence/compiler-config.json"
        dump(config, {"yavo": {"argv": [sys.executable, str(fake), "{entry}", "{output}"], "identity_files": {str(fake): sha(fake)}}})
        tasks = []
        for i in range(1, 9):
            task = f"T{i:02}"
            cases = [{"id": f"{task}-{visibility}", "visibility": visibility, "stdin": "", "stdout": "ok\n", "stderr": "", "exit_code": 0, "args": [], "files": {}}
                     for visibility in ("public", "hidden")]
            tasks.append({"id": task, "structural": ["synthetic structural gate"] if i in (4, 6) else [], "cases": cases})
        cases_path = base / "judge/cases.json"
        dump(cases_path, {"schema_version": 1, "tasks": tasks})
        freeze_path = base / "evidence/frozen.json"
        freeze = {"kind": "yavo.docs_ab.freeze", "experiment_id": "docs-ab-2026-11-22-r6",
                  "model_settings_sha256": "1" * 64, "executor_sha256": "2" * 64,
                  "provider_policy": {"expected_reported_model": "synthetic-fixture-no-service-request"},
                  "identity_files": {str(path.resolve()): sha(path) for path in (cases_path, config, judge)}}
        dump(freeze_path, freeze)
        qpath = base / "evidence/launch-qualification.json"
        dump(qpath, {"kind": "yavo.executor.qualification", "status": "qualified", "freeze_sha256": sha(freeze_path),
                     "model_settings_sha256": freeze["model_settings_sha256"], "executor_sha256": freeze["executor_sha256"],
                     "synthetic_fixture_only": True})
        for run_id in RUNS:
            arm, replicate = run_id.split("-")
            root = base / "runs" / run_id
            entries, events, states = [], [], {}
            for i in range(1, 9):
                task = f"T{i:02}"
                source = "print('wrong')" if run_id == "full-a" and i == 1 else "# BUILD_FAIL" if run_id == "full-b" and i == 3 else "print('ok')"
                source = f"#!{sys.executable}\n" + source + "\n"
                paths = {"first": f"tasks/{task}/snapshots/0001", "final": f"tasks/{task}/final"}
                for relative in paths.values():
                    path = root / relative / "main.yv"
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(source)
                manifest = {"main.yv": sha(root / paths["first"] / "main.yv")}
                failed = "# BUILD_FAIL" in source
                entries.append({"id": task, **{stage: {"source_dir": relative, "entry": "main.yv"} for stage, relative in paths.items()}})
                states[task] = {"submitted": True, "final_manifest": manifest, "first_build_passed": not failed}
                events.append({"sequence": i, "run_id": run_id, "operation": "build", "arguments": [task], "category": "compile", "status": "completed",
                               "task": task, "build_number": 1, "source_manifest": manifest, "elapsed_seconds": 0.01,
                               "result": {"argv": [str(fake)], "elapsed_seconds": 0.01, "exit_code": 1 if failed else 0, "signal": 0, "timed_out": False, "output_limit": False}})
            events.append({"sequence": 9, "run_id": run_id, "operation": "seal", "arguments": [], "category": "submit", "status": "completed", "elapsed_seconds": 0.01})
            dump(root / "submission.json", {"schema_version": 2, "run_id": run_id, "language": "yavo", "arm": arm, "replicate": replicate, "tasks": entries})
            dump(root / "run.json", {"schema_version": 2, "run_id": run_id, "language": "yavo", "arm": arm, "replicate": replicate, "status": "sealed",
                                     "frozen_manifest_sha256": sha(freeze_path), "qualification_sha256": sha(qpath),
                                     "model": {"exact_id": "synthetic-fixture-no-service-request", "settings_sha256": freeze["model_settings_sha256"]},
                                     "tasks": states, "operations": 9, "operations_by_category": dict(Counter(e["category"] for e in events)),
                                     "usage": {"input_tokens": 999999, "output_tokens": 999999, "total_tokens": 1999998},
                                     "started_monotonic": 1.0, "ended_monotonic": 2.0, "elapsed_seconds": 1.0})
            (root / "events.jsonl").write_text("".join(json.dumps(e) + "\n" for e in events))
        def invoke(run_id, output):
            return subprocess.run(["perl", str(judge), "--experiment-root", str(base), "--cases", str(cases_path),
                                   "--submission", str(base / "runs" / run_id / "submission.json"),
                                   "--compiler-config", str(config), "--output", str(output)], capture_output=True, text=True, timeout=60)
        # Hidden execution cannot begin while any cohort member remains active.
        path = base / "runs/compact-b/run.json"
        saved = path.read_bytes()
        state = json.loads(saved); state["status"] = "active"; dump(path, state)
        result = invoke("full-a", base / "must-not-exist")
        assert result.returncode != 0 and "not sealed" in result.stderr and not (base / "must-not-exist").exists()
        path.write_bytes(saved)
        scenarios.append("hidden judging blocked before all four seals")
        state = json.loads(saved); state["frozen_manifest_sha256"] = "0" * 64; dump(path, state)
        result = invoke("full-a", base / "must-not-exist")
        assert result.returncode != 0 and "same freeze" in result.stderr
        path.write_bytes(saved)
        scenarios.append("cross-freeze cohort rejected")
        reports = []
        for run_id in RUNS:
            output = base / "judgments" / run_id
            result = invoke(run_id, output)
            assert result.returncode == 0, result.stdout + result.stderr
            reports.append(output / "report.json")
        scenarios.append("first and final behavior includes wrong answer and build failure")
        checked = checker.validate(cases_path, reports, base / "runs")
        assert checked["passed"], checked
        unreviewed = summary.summarize(reports, base / "runs", None, cases_path)
        assert unreviewed["arms"]["full"]["metrics"]["final_contract_correct"] == {"passed": 10, "failed": 2, "unmeasured": 4, "total": 16}
        assert unreviewed["arms"]["compact"]["metrics"]["final_contract_correct"] == {"passed": 12, "failed": 0, "unmeasured": 4, "total": 16}
        assert all(run["costs"]["usage"] is None for run in unreviewed["runs"])
        assert all(arm["costs"]["usage"] is None for arm in unreviewed["arms"].values())
        scenarios += ["cross-arm grouping", "missing provider costs remain null despite lab metadata numbers", "unreviewed structures remain unmeasured"]
        reviews = []
        for report_path in reports:
            report = json.loads(report_path.read_text())
            for task in report["tasks"]:
                if not task["structural_requirements"]:
                    continue
                for stage in ("first", "final"):
                    reviews.append({"run_id": report["run_id"], "task_id": task["id"], "stage": stage,
                                    "source_sha256": task[stage]["source"]["sha256"], "passed": True,
                                    "evidence": "Synthetic test fixture only; not a real module/thread review."})
        review_path = base / "reviews.json"
        dump(review_path, {"reviews": reviews})
        complete = summary.summarize(reports, base / "runs", review_path, cases_path)
        assert complete["arms"]["full"]["metrics"]["final_contract_correct"] == {"passed": 14, "failed": 2, "unmeasured": 0, "total": 16}
        assert complete["arms"]["compact"]["metrics"]["final_contract_correct"]["passed"] == 16
        assert complete["paired_final_contract"] == {"both_correct": 14, "full_only": 0, "compact_only": 2, "neither_correct": 0, "unpaired_or_unjudged": 0}
        reviews[0]["source_sha256"] = "0" * 64
        dump(review_path, {"reviews": reviews})
        stale = summary.summarize(reports, base / "runs", review_path, cases_path)
        row = next(row for row in stale["task_results"] if row["run_id"] == reviews[0]["run_id"] and row["task_id"] == reviews[0]["task_id"])
        assert row["first_contract_correct"] is None
        scenarios.append("structural reviews bind exact source identities")
        usage_runs = [{"costs": {"provider_evidence_status": "verified", "usage": {"input_tokens": x, "output_tokens": 10, "total_tokens": x + 10}}} for x in (100, 200)]
        aggregate = summary.aggregate_costs(usage_runs, {"passed": 14, "failed": 2, "unmeasured": 0, "total": 16})
        assert aggregate["usage"]["input_tokens"] == 300 and aggregate["usage_per_successful_delivery"]["input_tokens"] == 300 / 14
        scenarios.append("failed tasks stay in cost numerator")
        tampered = json.loads(reports[0].read_text())
        tampered["tasks"][0]["first"]["cases"][0]["passed"] = True
        tampered_path = base / "tampered-report.json"; dump(tampered_path, tampered)
        invalid = checker.validate(cases_path, [tampered_path, *reports[1:]], base / "runs")
        assert not invalid["passed"]
        scenarios.append("tampered behavioral totals rejected")
        tampered = json.loads(reports[0].read_text())
        tampered["tasks"][3]["structural_requirements"] = []
        dump(tampered_path, tampered)
        assert not checker.validate(cases_path, [tampered_path, *reports[1:]], base / "runs")["passed"]
        scenarios.append("structural requirements cannot be stripped from reports")
        tampered = json.loads(reports[0].read_text())
        tampered["tasks"][3]["first"]["source"]["sha256"] = "0" * 64
        dump(tampered_path, tampered)
        assert not checker.validate(cases_path, [tampered_path, *reports[1:]], base / "runs")["passed"]
        scenarios.append("source aggregate hashes are independently recomputed")
    print(json.dumps({"status": "passed", "scope": "synthetic judge/summary contract tests, no live provider or participant execution", "scenarios": scenarios}))


if __name__ == "__main__":
    main()
