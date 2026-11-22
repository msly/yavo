#!/usr/bin/env python3
"""Summarize the document treatment, joining costs only from verified live transcripts."""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {"full-a": ("full", "a"), "compact-a": ("compact", "a"), "compact-b": ("compact", "b"), "full-b": ("full", "b")}
TASKS = [f"T{i:02d}" for i in range(1, 9)]


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def counted(values):
    return {"passed": sum(v is True for v in values), "failed": sum(v is False for v in values),
            "unmeasured": sum(v is None for v in values), "total": len(values)}


def conjunction(*values):
    if any(v is False for v in values):
        return False
    if any(v is None for v in values):
        return None
    return True


def behavior(stage, visibility=None):
    if stage.get("status") == "judge_error":
        return None
    if stage.get("status") in ("missing_submission", "missing_source", "build_failed"):
        return False
    if stage.get("source_unchanged") is not True or stage.get("build_passed") is not True:
        return False
    selections = [visibility] if visibility else ["public", "hidden"]
    if any(name not in stage for name in selections):
        return None
    return all(stage[name].get("all_passed") is True for name in selections)


def structure(report, task, stage, reviews):
    if not task.get("structural_requirements"):
        return True
    review = reviews.get((report["run_id"], task["id"], stage))
    if not review or not review.get("evidence"):
        return None
    if review.get("source_sha256") != task[stage].get("source", {}).get("sha256"):
        return None
    return review.get("passed") if type(review.get("passed")) is bool else None


def first_builds(events):
    first = {}
    for event in events:
        if event.get("operation") != "build":
            continue
        task = event.get("task") or next(iter(event.get("arguments", [])), None)
        if task is None or task in first:
            continue
        command = event.get("result")
        if command is not None:
            passed = not command.get("timed_out", False) and not command.get("output_limit", False) and not command.get("signal", 0) and command.get("exit_code") == 0
        elif event.get("status") == "interface_error":
            passed = False
        else:
            passed = None
        first[task] = {"passed": passed, "sequence": event.get("sequence"), "exit_code": (command or {}).get("exit_code"),
                       "classification": "compiler_command" if command is not None else "interface_before_compiler"}
    return first


def read_lab(run_dir):
    if run_dir is None or not (run_dir / "run.json").is_file() or not (run_dir / "events.jsonl").is_file():
        return {"historical": {}, "metadata": None, "costs": {"lab_operations": None, "lab_elapsed_seconds": None, "documentation_bytes": None}}
    meta = load(run_dir / "run.json")
    events = [json.loads(row) for row in (run_dir / "events.jsonl").read_text().splitlines() if row.strip()]
    document_bytes = Counter()
    for event in events:
        value = event.get("document_bytes_returned")
        if type(value) is int and value >= 0:
            document_bytes[event.get("category", "uncategorized")] += value
    return {"historical": first_builds(events), "metadata": meta,
            "costs": {"lab_operations": len(events), "lab_elapsed_seconds": meta.get("elapsed_seconds"),
                      "operation_categories": dict(sorted(Counter(e.get("category", "uncategorized") for e in events).items())),
                      "documentation_bytes": dict(sorted(document_bytes.items())),
                      "compiler_process_seconds": sum(e["result"]["elapsed_seconds"] for e in events if e.get("operation") == "build" and isinstance(e.get("result"), dict)),
                      "build_requests": sum(e.get("operation") == "build" for e in events),
                      "run_metadata_sha256": sha(run_dir / "run.json"), "events_sha256": sha(run_dir / "events.jsonl")}}


METRICS = ("historical_first_build_passed", "first_source_replay_build_passed", "first_public_correct", "first_hidden_correct",
           "first_behavior_correct", "first_contract_correct", "first_attempt_contract_correct",
           "final_public_correct", "final_hidden_correct", "final_behavior_correct", "final_contract_correct")


def metrics(rows):
    return {name: counted([row[name] for row in rows]) for name in METRICS}


def audited_cost(run_id, report, lab, executor_runs, executor_config, qualification_path):
    costs = {**lab["costs"], "usage": None, "money_cost": None, "exact_model": None,
             "provider_requests": None, "declared_model_tool_calls": None, "executor_elapsed_seconds": None,
             "provider_evidence_status": "unavailable", "provider_evidence_blockers": ["live executor transcript not supplied"]}
    if executor_runs is None:
        return costs
    try:
        repo = ROOT.parents[1]
        sys.path.insert(0, str(repo / "tools/agent_eval"))
        import executor
        import audit
        config = executor.Config(executor_config)
        verification = audit.verify_run(config, executor_runs / run_id, qualification_path)
        costs["provider_evidence_status"] = verification["status"]
        costs["provider_evidence_blockers"] = verification["blockers"]
        costs["provider_audit"] = verification
        if verification["status"] != "verified":
            return costs
        guard = report["cohort_guard"]
        if verification.get("run_id") != run_id or verification.get("freeze_sha256") != guard["freeze_sha256"] or verification.get("qualification_sha256") != guard["qualification_sha256"]:
            raise ValueError("audited provider usage belongs to another run/freeze/qualification")
        if verification.get("lab_events_sha256") != costs.get("events_sha256") or verification.get("lab_metadata_sha256") != costs.get("run_metadata_sha256"):
            raise ValueError("audited provider usage does not bind the summarized lab records")
        costs.update({"usage": verification["usage"], "exact_model": verification["exact_model"],
                      "provider_requests": verification["provider_requests"], "declared_model_tool_calls": verification["controlled_lab_operations"],
                      "executor_elapsed_seconds": verification["elapsed_seconds"]})
    except (OSError, ValueError, TypeError, KeyError, ImportError, AttributeError) as error:
        costs["provider_evidence_status"] = "blocked"
        costs["provider_evidence_blockers"] = [type(error).__name__ + ": " + str(error)]
        costs["usage"] = None
    return costs


def aggregate_costs(runs, successful, expected_runs=2):
    complete = len(runs) == expected_runs and all(run["costs"].get("provider_evidence_status") == "verified" and isinstance(run["costs"].get("usage"), dict) for run in runs)
    totals = None
    if complete:
        usages = [run["costs"]["usage"] for run in runs]
        totals = {key: sum(item[key] for item in usages) if all(item.get(key) is not None for item in usages) else None for key in usages[0]}
    def observed_sum(key):
        values = [run["costs"].get(key) for run in runs]
        return sum(values) if len(values) == expected_runs and all(type(value) in (int, float) and math.isfinite(value) for value in values) else None
    per_success = None
    if totals is not None and successful["unmeasured"] == 0 and successful["passed"] > 0:
        per_success = {key: value / successful["passed"] if value is not None else None for key, value in totals.items()}
    return {"observed_runs": len(runs), "expected_runs": expected_runs, "verified_cost_runs": sum(run["costs"].get("provider_evidence_status") == "verified" for run in runs),
            "usage_complete": complete, "usage": totals, "usage_per_successful_delivery": per_success,
            "success_denominator": successful, "lab_operations": observed_sum("lab_operations"),
            "lab_elapsed_seconds": observed_sum("lab_elapsed_seconds"), "executor_elapsed_seconds": observed_sum("executor_elapsed_seconds"),
            "money_cost": None, "scope": "All requests in both sessions remain in the numerator, including failed tasks and repairs."}


def summarize(report_paths, runs, review_path, cases_path, executor_runs=None, executor_config=None, qualification_path=None):
    if any(x is not None for x in (executor_runs, executor_config, qualification_path)) and not all(x is not None for x in (executor_runs, executor_config, qualification_path)):
        raise ValueError("executor-runs, executor-config and qualification must be supplied together")
    specification = importlib.util.spec_from_file_location("r6_check_evidence", Path(__file__).with_name("check_evidence.py"))
    check_evidence = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(check_evidence)
    evidence = check_evidence.validate(cases_path, report_paths, runs)
    if not evidence["passed"]:
        raise ValueError("replay evidence failed independent consistency checks: " + "; ".join(evidence["errors"]))
    reviews = {}
    if review_path:
        for item in load(review_path)["reviews"]:
            key = (item["run_id"], item["task_id"], item["stage"])
            if key in reviews:
                raise ValueError("duplicate structural review: " + str(key))
            reviews[key] = item
    output = {"schema": "yavo-r6-summary-v1", "runs": [], "arms": {}, "task_results": [], "evidence_gaps": [],
              "scope": {"independent_unit": "session", "tasks_per_session": 8, "planned_sessions": 4,
                        "planned_deliveries": 32, "language": "yavo", "formal_rust_cost_acceptance": False,
                        "single_task_cold_start_measured": False}, "replay_evidence_check": evidence}
    run_ids, freezes, qualification_hashes, cohorts = set(), set(), set(), set()
    for path in report_paths:
        report = load(path)
        run_id = report.get("run_id")
        if run_id in run_ids or run_id not in EXPECTED:
            raise ValueError("duplicate or unplanned run ID")
        run_ids.add(run_id)
        arm, replicate = EXPECTED[run_id]
        if report.get("language") != "yavo" or report.get("arm") != arm or report.get("replicate") != replicate:
            raise ValueError("run treatment identity mismatch")
        if [task["id"] for task in report["tasks"]] != TASKS:
            raise ValueError("run task inventory mismatch")
        guard = report.get("cohort_guard", {})
        if guard.get("guard") != "all_four_sealed_same_freeze_and_source_identities" or set(guard.get("all_runs", {})) != set(EXPECTED):
            raise ValueError("report lacks all-four-sealed guard")
        freezes.add(guard.get("freeze_sha256")); qualification_hashes.add(guard.get("qualification_sha256"))
        cohorts.add(json.dumps(guard["all_runs"], sort_keys=True))
        lab = read_lab(runs / run_id if runs else None)
        rows = []
        for task in report["tasks"]:
            first, final = task["first"], task["final"]
            first_behavior, final_behavior = behavior(first), behavior(final)
            historical = lab["historical"].get(task["id"], {}).get("passed")
            first_contract = conjunction(first_behavior, structure(report, task, "first", reviews))
            rows.append({"run_id": run_id, "arm": arm, "replicate": replicate, "task_id": task["id"],
                         "historical_first_build_passed": historical, "first_source_replay_build_passed": first.get("build_passed"),
                         "first_public_correct": behavior(first, "public"), "first_hidden_correct": behavior(first, "hidden"),
                         "first_behavior_correct": first_behavior, "first_contract_correct": first_contract,
                         "first_attempt_contract_correct": conjunction(historical, first_contract),
                         "final_public_correct": behavior(final, "public"), "final_hidden_correct": behavior(final, "hidden"),
                         "final_behavior_correct": final_behavior, "final_contract_correct": conjunction(final_behavior, structure(report, task, "final", reviews)),
                         "first_source_sha256": first.get("source", {}).get("sha256"), "final_source_sha256": final.get("source", {}).get("sha256")})
        costs = audited_cost(run_id, report, lab, executor_runs, executor_config, qualification_path)
        output["runs"].append({"run_id": run_id, "arm": arm, "replicate": replicate, "report": str(path), "report_sha256": sha(path),
                               "evaluation_complete": report.get("complete"), "metrics": metrics(rows), "costs": costs})
        output["task_results"].extend(rows)
    if len(freezes) != 1 or None in freezes or len(qualification_hashes) != 1 or None in qualification_hashes or len(cohorts) != 1:
        raise ValueError("reports do not share one frozen, qualified, sealed cohort")
    output["scope"].update({"observed_sessions": len(run_ids), "observed_deliveries": len(output["task_results"]),
                             "freeze_sha256": next(iter(freezes)), "qualification_sha256": next(iter(qualification_hashes)),
                             "case_pack_sha256": sha(cases_path)})
    for arm in ("full", "compact"):
        rows = [row for row in output["task_results"] if row["arm"] == arm]
        selected = [run for run in output["runs"] if run["arm"] == arm]
        metric = metrics(rows)
        output["arms"][arm] = {"metrics": metric, "costs": aggregate_costs(selected, metric["final_contract_correct"])}
    pairs = defaultdict(dict)
    for row in output["task_results"]:
        pairs[(row["replicate"], row["task_id"])][row["arm"]] = row["final_contract_correct"]
    paired = Counter({key: 0 for key in ("both_correct", "full_only", "compact_only", "neither_correct", "unpaired_or_unjudged")})
    for values in pairs.values():
        full, compact = values.get("full"), values.get("compact")
        key = "unpaired_or_unjudged" if full is None or compact is None else "both_correct" if full and compact else "full_only" if full else "compact_only" if compact else "neither_correct"
        paired[key] += 1
    output["paired_final_contract"] = dict(paired)
    if run_ids != set(EXPECTED):
        output["evidence_gaps"].append("planned session reports missing: " + ", ".join(sorted(set(EXPECTED) - run_ids)))
    for run in output["runs"]:
        if run["costs"]["provider_evidence_status"] != "verified":
            output["evidence_gaps"].append(run["run_id"] + ": live provider costs unavailable or unverified")
        if run["metrics"]["final_contract_correct"]["unmeasured"] or run["metrics"]["first_attempt_contract_correct"]["unmeasured"]:
            output["evidence_gaps"].append(run["run_id"] + ": first-build or structural judgments incomplete")
    output["complete_requested_evidence"] = not output["evidence_gaps"]
    output["comparability"] = {"treatment": "initial guide strategy, same Yavo and on-demand API", "formal_acceptance": False,
                                "scope": "two shared-context sessions per arm; descriptive document comparison, not stable population evidence"}
    output["metric_definitions"] = {
        "historical_first_build_passed": "The first lab build request succeeded; interface rejection before a compiler invocation still counts as a failed request.",
        "first_source_replay_build_passed": "Replay of the source graph captured before the first actual compiler invocation; a rejected interface request creates no source stage or compiler ordinal.",
        "first_attempt_contract_correct": "The first build request succeeded AND the first actual compiler source satisfies public, hidden and hash-bound structural requirements.",
        "usage_per_successful_delivery": "All verified provider requests in both sessions, including failures and repairs, divided by final complete deliveries; unknown usage remains null.",
    }
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--runs", type=Path, default=ROOT / "runs")
    parser.add_argument("--cases", type=Path, default=ROOT / "judge/cases.json")
    parser.add_argument("--manual-review", type=Path)
    parser.add_argument("--executor-runs", type=Path)
    parser.add_argument("--executor-config", type=Path)
    parser.add_argument("--qualification", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = summarize(args.reports, args.runs, args.manual_review, args.cases, args.executor_runs, args.executor_config, args.qualification)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False) + "\n")
    print(json.dumps({"scope": result["scope"], "complete_requested_evidence": result["complete_requested_evidence"]}))


if __name__ == "__main__":
    main()
