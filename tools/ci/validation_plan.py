#!/usr/bin/env python3
"""Resolve the checked-in batch cadence; explicit full requests only escalate."""
import argparse
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def resolve(policy, requested="auto", ref=""):
    if policy.get("schema") != "yavo-validation-policy/1":
        raise ValueError("unknown validation policy")
    batch, every = policy["batch"], policy["full_every_batches"]
    if type(batch) is not int or batch < 1 or type(every) is not int or every < 1:
        raise ValueError("batch and cadence must be positive integers")
    if type(policy["milestone"]) is not bool:
        raise ValueError("milestone must be boolean")
    if requested not in ("auto", "core", "full"):
        raise ValueError("unknown tier")
    reasons = []
    if batch % every == 0:
        reasons.append("batch cadence")
    if policy["milestone"]:
        reasons.append("declared milestone")
    if ref.startswith("refs/tags/"):
        reasons.append("release tag")
    if requested == "full":
        reasons.append("manual full request")
    result = {"tier": "full" if reasons else "core", "batch": batch,
              "baseline_commit": policy["baseline_commit"],
              "reasons": reasons or ["ordinary batch"]}
    for field, output in (("p0_sanitizer_qualification_batch", "p0_sanitizer_qualification"),
                          ("performance_qualification_batch", "paired_performance")):
        qualification = policy.get(field)
        if qualification is not None and (type(qualification) is not int or qualification < 1):
            raise ValueError("qualification batch must be a positive integer")
        result[output] = qualification == batch
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--requested", default="auto", choices=("auto", "core", "full"))
    args = parser.parse_args()
    policy = json.loads((ROOT / "tools/ci/validation-policy.json").read_bytes())
    result = resolve(policy, args.requested, os.environ.get("GITHUB_REF", ""))
    print(json.dumps(result, ensure_ascii=False))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write("tier=" + result["tier"] + "\n")
            out.write("baseline=" + result["baseline_commit"] + "\n")
            for name in ("p0_sanitizer_qualification", "paired_performance"):
                out.write(name + "=" + str(result[name]).lower() + "\n")


if __name__ == "__main__":
    main()
