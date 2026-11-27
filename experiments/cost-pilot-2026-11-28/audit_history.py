#!/usr/bin/env python3
"""Recompute available historical costs; never fill missing provider usage."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    experiments = args.repo / "experiments"
    result = {"schema": "yavo-agent-cost-evidence-audit-v1", "audit_date": "2026-11-28", "rounds": []}
    for name, folder, token_y, token_r, first_y, first_r in [
        ("R3", "coldstart-2026-11-21", 165176, 124860, 3, 11),
        ("R4", "coldstart-2026-11-21-r4", 162464, 127801, 8, 11),
    ]:
        d = experiments / folder
        row = {"round": name, "independent_tasks": 12, "evidence_grade": "historical_reports_and_participant_logs",
               "reported_first_correct_excluding_repair_task": {"yavo": first_y, "rust": first_r, "denominator": 11},
               "reported_token_totals": {"yavo": token_y, "rust": token_r},
               "reported_token_ratio_yavo_over_rust": token_y / token_r,
               "raw_provider_usage_available": False, "reported_model": "Claude Sonnet (exact id not archived)",
               "reported_frozen_runtime_cases_passed": {"yavo": 18, "rust": 18, "denominator": 18},
               "task_builds_from_participant_log": {}, "sources": {}}
        for language in ["yavo", "rust"]:
            f = d / language / "LOG.md"
            attempts = {}
            for task, attempt in re.findall(r"\b(T\d\d) build (\d+):", f.read_text()):
                attempts[task] = max(attempts.get(task, 0), int(attempt))
            if len(attempts) != 12:
                raise ValueError("historical task log incomplete")
            row["task_builds_from_participant_log"][language] = attempts
            row["sources"][str(f.relative_to(args.repo))] = hashlib.sha256(f.read_bytes()).hexdigest()
        row["total_logged_builds"] = {l: sum(a.values()) for l, a in row["task_builds_from_participant_log"].items()}
        row["extra_logged_builds_after_first"] = {l: n - 12 for l, n in row["total_logged_builds"].items()}
        row["sources"][str((d / "README.md").relative_to(args.repo))] = hashlib.sha256((d / "README.md").read_bytes()).hexdigest()
        row["limits"] = ["Token totals are reported claims, not provider usage reconstructed in this audit.",
                         "Extra builds include environment/output and runtime correction; not all are compiler code repairs."]
        if name == "R4":
            row["limits"].append("Same tasks as R3; post-submission T10 leading-zero boundary failed for Yavo and passed for Rust.")
        result["rounds"].append(row)
    path = experiments / "coldstart-2026-11-22-r5/tools/analyze_costs.py"
    spec = importlib.util.spec_from_file_location("r5_costs", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    r5 = module.analyze(path.parents[1])
    result["rounds"].append({"round": "R5", "independent_tasks": 12, "replicates_per_language": 2,
        "evidence_grade": "instrumented_operation_logs_and_independent_hidden_replay",
        "languages": r5["languages"], "ratios": r5["ratios_yavo_over_rust"],
        "raw_provider_usage_available": False, "tokens": None, "reported_model": None,
        "source": "experiments/coldstart-2026-11-22-r5/results.json",
        "limits": ["24 successful deliveries per language are 12 tasks x 2 sessions, not 24 independent tasks.",
                   "Operation counts and elapsed time are not token counts."]})
    result["rounds"].append({"round": "R6", "evidence_grade": "prepared_only", "participant_sessions": 0,
        "tokens": None, "source": "experiments/docs-ab-2026-11-22-r6/README.md"})
    result["conclusion"] = {"never_compared_to_rust": False,
        "claimed_lower_cost_than_rust_demonstrated": False,
        "missing": ["current-version paired real usage", "exact reported model provenance", "price/cached-token billing if claiming money"]}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"output": str(args.out), "R3_builds": result["rounds"][0]["total_logged_builds"],
        "R4_builds": result["rounds"][1]["total_logged_builds"], "R5_languages": r5["languages"],
        "conclusion": result["conclusion"]}, ensure_ascii=False))

if __name__ == "__main__":
    main()
