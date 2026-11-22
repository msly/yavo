#!/usr/bin/env python3
"""Check bookkeeping and byte comparisons independently of the judge's totals."""
from __future__ import annotations

import argparse
import base64
from collections import Counter
import hashlib
import json
from pathlib import Path


def load(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def validate(cases_path: Path, reports: list[Path], runs: Path | None) -> dict:
    cases = load(cases_path)
    task_map = {task["id"]: task for task in cases["tasks"]}
    errors, limitations = [], []
    checked_cases, checked_snapshots = 0, 0
    run_ids = set()
    compiler_sets = {}
    def require(condition, text):
        if not condition:
            errors.append(text)
    require(len(task_map) == 8 and len(cases["tasks"]) == 8, "case pack must contain eight distinct tasks")
    for report_path in reports:
        report = load(report_path)
        label = report.get("run_id", str(report_path))
        require(label not in run_ids, f"{label}: duplicate run")
        run_ids.add(label)
        require(report.get("schema") == "yavo-r6-evaluation-v1", f"{label}: schema mismatch")
        require(report["frozen_inputs"]["cases_sha256"] == sha(cases_path.read_bytes()), f"{label}: case pack identity mismatch")
        require(report.get("compiler_identities_unchanged") is True, f"{label}: compiler identity not stable")
        identities = report.get("compiler_identities")
        require(bool(identities), f"{label}: compiler identities missing")
        if report["language"] in compiler_sets:
            require(identities == compiler_sets[report["language"]], f"{label}: compiler identities differ across replicas")
        compiler_sets[report["language"]] = identities
        require(len(report["tasks"]) == len(task_map), f"{label}: task count mismatch")
        require({task["id"] for task in report["tasks"]} == set(task_map), f"{label}: task IDs mismatch")
        run_dir = runs / label if runs else None
        meta = None
        events = []
        if run_dir and (run_dir / "run.json").exists():
            meta = load(run_dir / "run.json")
            require(meta.get("status") == "sealed", f"{label}: run not sealed")
            event_path = run_dir / "events.jsonl"
            require(event_path.exists(), f"{label}: event log missing")
            if event_path.exists():
                events = [json.loads(line) for line in event_path.read_text(encoding="utf-8").splitlines() if line.strip()]
                require([event["sequence"] for event in events] == list(range(1, len(events) + 1)), f"{label}: non-contiguous event sequence")
                require(meta.get("operations") == len(events), f"{label}: operation count mismatch")
                require(meta.get("operations_by_category") == dict(Counter(event["category"] for event in events)), f"{label}: category totals mismatch")
                for event in events:
                    require(event.get("run_id") == label, f"{label}: cross-run event")
                    require(event.get("elapsed_seconds", -1) >= 0, f"{label}: missing/negative operation duration")
                    result = event.get("result")
                    if result:
                        require(isinstance(result.get("argv"), list), f"{label}: command argv missing")
                        require(result.get("elapsed_seconds", -1) >= 0, f"{label}: command duration missing")
            limitations.append(f"{label}: lab metadata is not a provider-usage source; summary must independently audit executor transcripts")
            if not (meta.get("model") or {}).get("exact_id"):
                limitations.append(f"{label}: exact model identifier is unavailable")
            require(meta.get("elapsed_seconds", -1) >= 0, f"{label}: run duration missing")
            if all(name in meta for name in ("ended_monotonic", "started_monotonic", "elapsed_seconds")):
                require(abs(meta["ended_monotonic"] - meta["started_monotonic"] - meta["elapsed_seconds"]) < 1e-5,
                        f"{label}: monotonic run duration mismatch")
        else:
            limitations.append(f"{label}: run.json and events not provided for cross-check")
        for task in report["tasks"]:
            task_id = task["id"]
            require(task.get("structural_requirements") == task_map[task_id].get("structural", []),
                    f"{label}/{task_id}: structural requirements differ from frozen task")
            expected = {case["id"]: case for case in task_map[task_id]["cases"]}
            for stage_name in ("first", "final"):
                stage = task[stage_name]
                where = f"{label}/{task_id}/{stage_name}"
                if stage["status"] in ("missing_source", "missing_submission"):
                    require(not stage["build_passed"] and not stage["cases"], f"{where}: missing source counted as evaluated")
                    continue
                require(stage["status"] != "judge_error", f"{where}: judge error")
                if stage["status"] == "judge_error":
                    continue
                require(stage.get("source_unchanged") is True, f"{where}: source changed during judging")
                manifest = stage["source"]["files"]
                canonical_manifest = json.dumps(manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
                require(stage["source"].get("sha256") == sha(canonical_manifest), f"{where}: source manifest aggregate hash mismatch")
                if run_dir:
                    source = run_dir / stage["source_dir"]
                    for name, value in manifest.items():
                        path = source / name
                        require(path.is_file(), f"{where}: archived source missing: {name}")
                        if path.is_file():
                            require(sha(path.read_bytes()) == value, f"{where}: archived source hash mismatch: {name}")
                    checked_snapshots += 1
                if meta:
                    if stage_name == "first":
                        build_events = [event for event in events if event.get("task") == task_id and event.get("operation") == "build"]
                        require(bool(build_events), f"{where}: first build event missing")
                        if build_events:
                            require(build_events[0].get("source_manifest") == manifest, f"{where}: replay source differs from first compiler input")
                    else:
                        require(meta["tasks"][task_id].get("final_manifest") == manifest, f"{where}: final snapshot differs from sealed manifest")
                build = stage["build"]
                actual_build_success = build["status"] == "exited" and build["exit_code"] == 0
                require(stage["build_passed"] == actual_build_success, f"{where}: build result mismatch")
                if not stage["build_passed"]:
                    require(not stage["cases"], f"{where}: failed build has run cases")
                    continue
                require(len(stage["cases"]) == len(expected), f"{where}: case count mismatch")
                require({case["id"] for case in stage["cases"]} == set(expected), f"{where}: case identity mismatch")
                totals = {kind: {"passed": 0, "total": sum(case["visibility"] == kind for case in expected.values()), "executed": 0}
                          for kind in ("public", "hidden")}
                for actual in stage["cases"]:
                    if actual["id"] not in expected:
                        continue
                    want = expected[actual["id"]]
                    observed = actual["observed"]
                    require(actual["visibility"] == want["visibility"], f"{where}/{actual['id']}: visibility mismatch")
                    passed = observed["status"] == "exited" and observed["exit_code"] == want.get("exit_code", 0)
                    for stream in ("stdout", "stderr"):
                        captured = observed[stream]
                        data = base64.b64decode(captured["base64"], validate=True)
                        wanted = want.get(stream, "").encode("utf-8")
                        if not captured["capture_truncated"]:
                            require(len(data) == captured["bytes"] and sha(data) == captured["sha256"], f"{where}/{actual['id']}: {stream} evidence hash mismatch")
                        correct = not captured["capture_truncated"] and data == wanted
                        require(actual["checks"][stream] == correct, f"{where}/{actual['id']}: incorrect {stream} judgment")
                        require(actual["expected"][stream + "_sha256"] == sha(wanted), f"{where}/{actual['id']}: expected hash mismatch")
                        passed = passed and correct
                    require(actual["passed"] == passed, f"{where}/{actual['id']}: incorrect overall judgment")
                    totals[want["visibility"]]["executed"] += 1
                    totals[want["visibility"]]["passed"] += int(passed)
                    checked_cases += 1
                for kind, values in totals.items():
                    for key, value in values.items():
                        require(stage[kind][key] == value, f"{where}: {kind} {key} count mismatch")
                    all_passed = values["total"] > 0 and values["passed"] == values["executed"] == values["total"]
                    require(stage[kind]["all_passed"] == all_passed, f"{where}: {kind} all_passed mismatch")
    return {"schema": "yavo-r6-evidence-check-v1", "passed": not errors, "errors": errors,
            "limitations": limitations, "checked_runs": len(run_ids), "checked_case_results": checked_cases,
            "checked_source_snapshots": checked_snapshots,
            "scope": "evidence consistency; does not prove model independence or completeness of platform telemetry"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, required=True)
    parser.add_argument("--runs", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("reports", nargs="+", type=Path)
    args = parser.parse_args()
    result = validate(args.cases, args.reports, args.runs)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
