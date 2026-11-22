#!/usr/bin/env python3
"""Summarize replay evidence without replacing missing measurements with zero."""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path


def load(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def counted(values: list[bool | None]) -> dict:
    return {"passed": sum(value is True for value in values),
            "failed": sum(value is False for value in values),
            "unmeasured": sum(value is None for value in values), "total": len(values)}


def recorded_compile(stage: dict) -> bool | None:
    evidence = stage.get("historical_first_build")
    if not isinstance(evidence, dict) or evidence.get("exit_code") is None:
        return None
    if evidence.get("status") in ("timeout", "output_limit", "spawn_error"):
        return False
    return evidence["exit_code"] == 0


def behavior(stage: dict, visibility: str | None = None) -> bool | None:
    if stage.get("status") == "judge_error":
        return None
    if stage.get("status") in ("missing_submission", "missing_source", "build_failed"):
        return False
    if not stage.get("source_unchanged", False) or not stage.get("build_passed", False):
        return False
    selections = [visibility] if visibility else ["public", "hidden"]
    if any(name not in stage for name in selections):
        return None
    return all(stage[name]["all_passed"] for name in selections)


def structural(report: dict, task: dict, stage_name: str, reviews: dict) -> bool | None:
    if not task.get("structural_requirements"):
        return True
    review = reviews.get((report["run_id"], task["id"], stage_name))
    if not review or not review.get("evidence"):
        return None
    if review.get("source_sha256") != task[stage_name].get("source", {}).get("sha256"):
        return None
    value = review.get("passed")
    return value if isinstance(value, bool) else None


def contract(behavior_value: bool | None, structure: bool | None) -> bool | None:
    if behavior_value is False or structure is False:
        return False
    if behavior_value is None or structure is None:
        return None
    return True


def read_run_metadata(run_dir: Path | None) -> dict:
    result = {"model": None, "tokens": None, "agent_wall_seconds": None,
              "all_model_tool_calls": None, "recorded_lab_operations": None,
              "operation_categories": None, "run_metadata": None,
              "cost_metric_note": "Replay build/run time is not agent delivery time."}
    if run_dir is None:
        return result
    info_path = run_dir / "run.json"
    if info_path.exists():
        info = load(info_path)
        result["run_metadata"] = info
        result["run_metadata_sha256"] = sha(info_path)
        result["model"] = info.get("model")
        result["tokens"] = info.get("tokens", info.get("usage"))
        result["agent_wall_seconds"] = info.get("agent_wall_seconds", info.get("elapsed_seconds"))
        result["agent_wall_scope"] = info.get("time_scope")
        result["all_model_tool_calls"] = info.get("all_model_tool_calls")
    event_path = run_dir / "events.jsonl"
    if event_path.exists():
        events = [json.loads(line) for line in event_path.read_text(encoding="utf-8").splitlines() if line.strip()]
        result["recorded_lab_operations"] = len(events)
        result["operation_categories"] = dict(sorted(Counter(event.get("category", "uncategorized") for event in events).items()))
        first_builds = {}
        for event in events:
            if event.get("operation") != "build":
                continue
            task_id = event.get("task") or next(iter(event.get("arguments", [])), None)
            if task_id is None or task_id in first_builds:
                continue
            command = event.get("result")
            if command is not None:
                passed = not command.get("timed_out", False) and not command.get("signal", 0) and command.get("exit_code") == 0
                classification = "compiler_command"
            elif event.get("status") == "interface_error":
                passed, classification = False, "interface_error_before_compiler"
            else:
                passed, classification = None, "incomplete_command_evidence"
            first_builds[task_id] = {"passed": passed, "sequence": event["sequence"], "classification": classification,
                                    "exit_code": (command or {}).get("exit_code"), "error": event.get("error")}
        result["first_build_observations"] = first_builds
        result["build_operations"] = sum(event.get("operation") == "build" for event in events)
        result["compiler_process_invocations"] = sum(event.get("operation") == "build" and event.get("result") is not None for event in events)
        result["events_sha256"] = sha(event_path)
        result["recorded_lab_operations_note"] = "One archived lab event; not automatically every model tool call."
    return result


def summarize(report_paths: list[Path], runs: Path | None, review_path: Path | None) -> dict:
    reviews = {}
    if review_path:
        for item in load(review_path)["reviews"]:
            key = (item["run_id"], item["task_id"], item["stage"])
            if key in reviews:
                raise ValueError(f"duplicate review: {key}")
            reviews[key] = item
    output = {"schema": "yavo-r5-summary-v1", "runs": [], "languages": {},
              "comparability": {"round4_direct_cost_comparison": False,
                                "reason": "This round must record its own model/session/measurement provenance; prior incomplete telemetry is not interchangeable."}}
    rows = []
    run_ids = set()
    frozen_cases = set()
    for path in report_paths:
        report = load(path)
        if report.get("schema") != "yavo-r5-evaluation-v1":
            raise ValueError(f"unknown report schema: {path}")
        run_id = report["run_id"]
        if run_id in run_ids:
            raise ValueError(f"duplicate run_id: {run_id}")
        run_ids.add(run_id)
        frozen_cases.add(report["frozen_inputs"]["cases_sha256"])
        costs = read_run_metadata(runs / run_id if runs else None)
        historical = (costs.get("run_metadata") or {}).get("tasks", {})
        run_rows = []
        for task in report["tasks"]:
            first, final = task["first"], task["final"]
            first_behavior, final_behavior = behavior(first), behavior(final)
            historical_build = recorded_compile(first)
            first_observation = costs.get("first_build_observations", {}).get(task["id"])
            if first_observation is not None:
                historical_build = first_observation["passed"]
            elif historical_build is None:
                value = historical.get(task["id"], {}).get("first_build_passed")
                historical_build = value if isinstance(value, bool) else None
            row = {"run_id": run_id, "language": report["language"], "replicate": report["replicate"],
                   "task_id": task["id"], "historical_first_build_passed": historical_build,
                   "historical_first_build_observation": first_observation,
                   "first_source_replay_build_passed": first.get("build_passed"),
                   "first_public_correct": behavior(first, "public"),
                   "first_hidden_correct": behavior(first, "hidden"),
                   "historical_first_build_and_public_correct": contract(historical_build, behavior(first, "public")),
                   "historical_first_build_and_hidden_correct": contract(historical_build, behavior(first, "hidden")),
                   "first_behavior_correct": first_behavior,
                   "first_contract_correct": contract(first_behavior, structural(report, task, "first", reviews)),
                   "final_public_correct": behavior(final, "public"),
                   "final_hidden_correct": behavior(final, "hidden"),
                   "final_behavior_correct": final_behavior,
                   "final_contract_correct": contract(final_behavior, structural(report, task, "final", reviews)),
                   "first_source_sha256": first.get("source", {}).get("sha256"),
                   "final_source_sha256": final.get("source", {}).get("sha256")}
            run_rows.append(row)
        rows.extend(run_rows)
        output["runs"].append({"run_id": run_id, "language": report["language"], "replicate": report["replicate"],
                               "report": str(path), "report_sha256": sha(path), "evaluation_complete": report["complete"],
                               "metrics": metrics(run_rows), "costs": costs})
    if len(frozen_cases) > 1:
        raise ValueError("reports use different frozen case packs")
    output["task_results"] = rows
    for language in ("yavo", "rust"):
        selected = [row for row in rows if row["language"] == language]
        output["languages"][language] = metrics(selected)
    pairs = defaultdict(dict)
    for row in rows:
        key = (str(row["replicate"]), row["task_id"])
        if row["language"] in pairs[key]:
            raise ValueError(f"duplicate paired observation: {key} {row['language']}")
        pairs[key][row["language"]] = row
    paired = {"both_correct": 0, "yavo_only": 0, "rust_only": 0, "neither_correct": 0, "unpaired_or_unjudged": 0}
    for pair in pairs.values():
        y = pair.get("yavo", {}).get("final_contract_correct")
        r = pair.get("rust", {}).get("final_contract_correct")
        label = "unpaired_or_unjudged" if y is None or r is None else "both_correct" if y and r else "yavo_only" if y else "rust_only" if r else "neither_correct"
        paired[label] += 1
    output["paired_final_contract"] = paired
    output["scope"] = {"task_count": len({row["task_id"] for row in rows}), "run_count": len(run_ids),
                       "submission_count": len(rows), "case_pack_sha256": next(iter(frozen_cases), None),
                       "is_formal_v1_acceptance": False}
    output["metric_definitions"] = {"first_public_correct": "Public-case correctness of the source snapshot from the first compiler invocation, replayed after submission.",
                                    "first_hidden_correct": "Hidden-case correctness of that first source snapshot, replayed after submission.",
                                    "historical_first_build_and_public_correct": "Original first compiler invocation succeeded AND its frozen source passes every public replay case.",
                                    "contract_correct": "All public and hidden cases pass AND any mandatory source-structure requirements have hash-bound manual review."}
    output["evidence_gaps"] = []
    if output["scope"]["task_count"] != 12 or output["scope"]["run_count"] != 4 or output["scope"]["submission_count"] != 48:
        output["evidence_gaps"].append("Expected 12 tasks x 2 languages x 2 independent runs.")
    for run in output["runs"]:
        costs = run["costs"]
        for key in ("model", "tokens", "agent_wall_seconds", "all_model_tool_calls"):
            if costs[key] is None:
                output["evidence_gaps"].append(f"{run['run_id']}: {key} unavailable.")
        if isinstance(costs["model"], dict) and not costs["model"].get("exact_id"):
            output["evidence_gaps"].append(f"{run['run_id']}: exact model id and sampling provenance unavailable.")
        if run["metrics"]["historical_first_build_passed"]["unmeasured"]:
            output["evidence_gaps"].append(f"{run['run_id']}: historical first compiler exits missing; replay is reported separately.")
        if run["metrics"]["final_contract_correct"]["unmeasured"]:
            output["evidence_gaps"].append(f"{run['run_id']}: some structural or behavioral judgments incomplete.")
    output["complete_requested_evidence"] = not output["evidence_gaps"]
    return output


def metrics(rows: list[dict]) -> dict:
    names = ("historical_first_build_passed", "first_source_replay_build_passed", "first_public_correct",
             "first_hidden_correct", "historical_first_build_and_public_correct", "historical_first_build_and_hidden_correct",
             "first_behavior_correct", "first_contract_correct",
             "final_public_correct", "final_hidden_correct", "final_behavior_correct", "final_contract_correct")
    return {name: counted([row[name] for row in rows]) for name in names}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--runs", type=Path)
    parser.add_argument("--manual-review", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.reports, args.runs, args.manual_review)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "scope": result["scope"],
                      "complete_requested_evidence": result["complete_requested_evidence"]}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
