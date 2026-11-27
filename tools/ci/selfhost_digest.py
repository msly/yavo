#!/usr/bin/env python3
"""Require frozen SHA-256 vectors from stage0 and actual bootstrap stage1/stage2."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import sys
import tempfile

sys.dont_write_bytecode = True
import csv_support as execution
import selfhost_driver as producer

ROOT = Path(__file__).resolve().parents[2]
VECTORS = ROOT / "selfhost/tests/digest50"
COMPILER_PARTS = {
    "digest.yv", "support.yv", "lexer.yv", "model.yv", "parser.yv",
    "types.yv", "codegen.yv", "driver.yv", "runtime_source.yv", "main.yv",
}
VECTOR_CASES = 46
VECTOR_EXECUTIONS = 276


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked_identity(record, bound):
    current = producer.bind(bound, Path(record["path"]))
    require(current == record, "recorded file identity changed: " + record["path"])
    return current


def complete_producer(path, bound):
    path = path.resolve()
    if path.is_dir():
        path /= "receipt.json"
    receipt = json.loads(path.read_bytes())
    parts = receipt.get("inputs", {}).get("compiler_bundle", {}).get("parts", [])
    require(len(parts) == len(COMPILER_PARTS) and {part["file"] for part in parts} == COMPILER_PARTS,
            "digest acceptance requires the complete ten-module compiler bundle")
    drivers, source = producer.bootstrap_drivers(path, bound)
    manifest_path = ROOT / "selfhost/tests/cases.json"
    manifest = json.loads(manifest_path.read_bytes())
    require(producer.digest(manifest_path) == receipt["inputs"]["manifest_sha256"],
            "bootstrap no longer binds the current fixture manifest")
    require(producer.digest(ROOT / "selfhost/bootstrap.py") == receipt["inputs"]["harness_sha256"],
            "bootstrap was produced by a different harness")
    expected_ids = [case["id"] for case in manifest["cases"]]
    actual_ids = [case["id"] for case in receipt["cases"]]
    require(len(expected_ids) == len(set(expected_ids)), "duplicate current bootstrap case IDs")
    require(len(actual_ids) == len(set(actual_ids)) and set(actual_ids) == set(expected_ids),
            "bootstrap does not contain the complete current case inventory")
    require(receipt["passed"] == receipt["total"] == len(expected_ids),
            "bootstrap did not pass the complete current case inventory")
    producer.bind(bound, manifest_path)
    producer.bind(bound, ROOT / "selfhost/bootstrap.py")
    stage0 = Path(receipt["inputs"]["stage0"]["path"]).resolve()
    zig = Path(receipt["inputs"]["zig"]["path"]).resolve()
    compilers = [("stage0", stage0)] + [(name, binary.resolve()) for name, binary in drivers]
    require([name for name, _ in compilers] == ["stage0", "stage1", "stage2"],
            "unexpected bootstrap compiler roles")
    require(len({str(binary) for _, binary in compilers}) == 3, "bootstrap compiler paths overlap")
    source.update(receipt_identity=producer.bind(bound, path), cases=receipt["total"],
                  checks_passed=receipt["checks_passed"], checks_skipped=receipt["checks_skipped"],
                  source_modules=len(parts), manifest=producer.bind(bound, manifest_path),
                  harness=producer.bind(bound, ROOT / "selfhost/bootstrap.py"))
    return compilers, zig, source


def frozen_vectors(bound):
    freeze_path = VECTORS / "freeze.json"
    manifest_path = VECTORS / "vectors.json"
    freeze = json.loads(freeze_path.read_bytes())
    manifest = json.loads(manifest_path.read_bytes())
    require(freeze["schema"] == "yavo-digest50-freeze/1" and freeze["cases"] == VECTOR_CASES,
            "frozen digest inventory changed")
    require(manifest["schema"] == "yavo-digest50-vectors/1", "unknown digest vector schema")
    for relative, expected in freeze["inputs"].items():
        current = producer.bind(bound, ROOT / relative)
        require({"bytes": current["bytes"], "sha256": current["sha256"]} == expected,
                "frozen digest input changed: " + relative)
    cases = manifest["cases"]
    require(len(cases) == VECTOR_CASES and len({case["id"] for case in cases}) == VECTOR_CASES,
            "frozen digest case count differs")
    expected_output = "".join(case["id"] + "\t" + case["sha256"] + "\n" for case in cases).encode("ascii")
    return expected_output, {
        "cases": VECTOR_CASES, "freeze": producer.bind(bound, freeze_path),
        "vectors": producer.bind(bound, manifest_path),
        "fixture": producer.bind(bound, ROOT / "selfhost/tests/digest50_test.yv"),
        "expected_stdout_sha256": hashlib.sha256(expected_output).hexdigest(),
    }


def verify_result(path, compilers, zig, expected_output, bound):
    result = json.loads(path.read_bytes())
    require(result.get("schema") == "yavo-digest50-validation/1" and result.get("status") == "passed",
            "underlying digest validation did not pass")
    require(result.get("cases") == VECTOR_CASES and result.get("executions") == VECTOR_EXECUTIONS,
            "underlying digest execution inventory differs")
    require(result.get("inputs_unchanged") is True and result.get("isolated_zig_caches_removed") is True,
            "underlying digest inputs or cache cleanup did not pass")
    input_rows = result["inputs"]
    recorded = {str(Path(row["path"]).resolve()): row for row in input_rows}
    require(len(recorded) == len(input_rows), "duplicate underlying input identities")
    for row in input_rows:
        checked_identity(row, bound)
    required_inputs = [binary for _, binary in compilers] + [zig, VECTORS / "verify.py",
                       VECTORS / "vectors.json", VECTORS / "freeze.json",
                       ROOT / "selfhost/digest.yv", ROOT / "selfhost/tests/digest50_test.yv",
                       ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock"]
    require(set(recorded) == {str(path.resolve()) for path in required_inputs},
            "underlying digest compiler/source inventory differs")
    role_names = {"stage0": "stage0", "selfhost-1": "stage1", "selfhost-2": "stage2"}
    expected_runs = {(name, mode) for name in role_names for mode in ("debug", "release")}
    actual_runs = [(row["frontend"], row["mode"]) for row in result["runs"]]
    require(len(actual_runs) == len(expected_runs) and set(actual_runs) == expected_runs,
            "underlying digest six-product inventory differs")
    runs = []
    for row in result["runs"]:
        require(row["cases"] == row["passed"] == VECTOR_CASES and row["alloc_check"] is True,
                "digest vector or allocation check failed")
        for name in ("generated_c", "binary", "output"):
            checked_identity(row[name], bound)
        require(Path(row["output"]["path"]).read_bytes() == expected_output,
                "actual digest output differs from frozen expectations")
        runs.append({**row, "frontend": role_names[row["frontend"]], "underlying_frontend": row["frontend"]})
    labels = {"zig-version"}
    for frontend in role_names:
        labels.add(frontend + "-emit")
        for mode in ("debug", "release"):
            for action in ("build", "run"):
                labels.add(frontend + "-" + mode + "-" + action)
    actual_labels = [row["label"] for row in result["commands"]]
    require(len(actual_labels) == len(labels) and set(actual_labels) == labels,
            "underlying digest command inventory differs")
    for row in result["commands"]:
        require(row.get("exit_code") == 0 and row.get("timed_out") is False and not row.get("launch_error"),
                "underlying digest command failed")
        checked_identity(row["stdout"], bound)
        checked_identity(row["stderr"], bound)
        if row["label"].endswith("-run"):
            require(Path(row["stderr"]["path"]).read_bytes() == b"", "digest runtime wrote stderr")
    return {"receipt": producer.bind(bound, path), "status": result["status"],
            "cases": result["cases"], "executions": result["executions"],
            "inputs_unchanged": True, "isolated_zig_caches_removed": True,
            "command_count": len(result["commands"])}, runs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, default=ROOT / "compiler/target/ci/selfhost")
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/selfhost-digest")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    bound = {}
    report = {"schema": "yavo-selfhost-digest/1", "status": "running", "formal": True,
              "required": True, "work": str(work), "checks": [], "runs": [],
              "scope": "Frozen SHA-256 vectors through stage0 and actual complete-bootstrap stage1/stage2; Debug/Release with YV_ALLOC_CHECK",
              "host": {"platform": sys.platform, "machine": platform.machine()},
              "started_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        for path in (Path(__file__), Path(producer.__file__), Path(execution.__file__),
                     VECTORS / "verify.py", Path(sys.executable)):
            producer.bind(bound, path)
        compilers, zig, report["bootstrap"] = complete_producer(args.bootstrap_output, bound)
        report["compilers"] = [{"role": role, **producer.bind(bound, binary)} for role, binary in compilers]
        report["zig"] = producer.bind(bound, zig)
        report["checks"].append({"id": "complete_bootstrap_producer", "passed": True})
        expected_output, report["frozen_vectors"] = frozen_vectors(bound)
        report["checks"].append({"id": "frozen_vector_inventory", "passed": True})
        underlying = work / "vectors"
        argv = [sys.executable, str(VECTORS / "verify.py"), "--stage0", str(compilers[0][1]),
                "--selfhost", str(compilers[1][1]), "--selfhost", str(compilers[2][1]),
                "--zig", str(zig), "--output", str(underlying)]
        record, stdout, stderr = execution.execute(argv, work / "commands/vectors", ROOT,
                                                   dict(os.environ), {}, 2400)
        report["command"] = record
        require(producer.completed(record), "three-frontend digest runner failed: " +
                (stderr + stdout).decode("utf-8", "replace")[-4000:])
        report["underlying"], report["runs"] = verify_result(
            underlying / "receipt.json", compilers, zig, expected_output, bound)
        report["checks"].append({"id": "three_frontend_six_product_vectors", "passed": True})
        report.update(cases=VECTOR_CASES, executions=VECTOR_EXECUTIONS, passed=VECTOR_EXECUTIONS, failed=0)
    except Exception as error:
        report.update(status="failed", error=str(error))
    finally:
        changed = [path for path, expected in bound.items()
                   if not Path(path).is_file() or producer.digest(Path(path)) != expected]
        report["bound_inputs"] = bound
        report["changed_inputs"] = changed
        report["checks"].append({"id": "inputs_unchanged_during_run", "passed": not changed})
        if changed:
            report.update(status="failed", error="bound inputs changed during digest acceptance")
        if report["status"] == "running":
            report["status"] = "passed"
        report["checks_passed"] = sum(row["passed"] for row in report["checks"])
        report["completed_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        producer.save(output / "receipt.json", report)
    print(json.dumps({"status": report["status"], "receipt": str(output / "receipt.json"),
                      "executions": report.get("executions", 0), "error": report.get("error")}, ensure_ascii=False))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
