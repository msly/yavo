#!/usr/bin/env python3
"""Verify prepared R6 packet identities and clean, byte-identical regeneration."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    manifest_path = ROOT / "packet-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    expected_paths = {"TASKS.md", "judge/generate.py", "judge/cases.json", "public/cases.json"}
    expected_paths |= {f"public/T{i:02}.{ext}" for i in range(1, 9) for ext in ("md", "json")}
    recorded_paths = [record["path"] for record in manifest["files"]]
    assert len(recorded_paths) == 20 and set(recorded_paths) == expected_paths
    assert (manifest["task_count"], manifest["public_case_count"], manifest["hidden_case_count"]) == (8, 16, 96)
    for record in manifest["files"]:
        path = ROOT / record["path"]
        assert path.is_file() and not path.is_symlink()
        assert path.stat().st_size == record["bytes"] and digest(path) == record["sha256"], record["path"]
    generator = ROOT / "judge" / "generate.py"
    specification = importlib.util.spec_from_file_location("r6_packet_generator", generator)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    oracle_checks = module.self_check()
    packet = json.loads((ROOT / "judge" / "cases.json").read_text())
    public = json.loads((ROOT / "public" / "cases.json").read_text())
    assert [t["id"] for t in packet["tasks"]] == [f"T{i:02}" for i in range(1, 9)]
    cases_checked = 0
    all_ids = set()
    for task, public_task in zip(packet["tasks"], public["tasks"], strict=True):
        assert task["id"] == public_task["id"]
        assert task["cases"][:2] == public_task["cases"]
        assert [case["visibility"] for case in task["cases"]] == ["public"] * 2 + ["hidden"] * 12
        assert len({case["stdin"] for case in task["cases"]}) == 14, (task["id"], "duplicate input")
        for case in task["cases"]:
            assert case["id"] not in all_ids
            all_ids.add(case["id"])
            module.validate_inputs(task["id"], case["stdin"])
            assert case["stdout"] == module.ORACLES[task["id"]](case["stdin"])
            assert case["stderr"] == "" and case["exit_code"] == 0
            assert case["args"] == [] and case["files"] == {}
            cases_checked += 1
    assert cases_checked == 112
    regeneration = []
    for repeat in range(2):
        with tempfile.TemporaryDirectory(prefix="yavo-r6-packet-") as temporary:
            temp = Path(temporary)
            (temp / "judge").mkdir()
            shutil.copyfile(generator, temp / "judge" / "generate.py")
            command = [sys.executable, str(temp / "judge" / "generate.py")]
            result = subprocess.run(command, capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, result.stderr
            checked = sorted(expected_paths | {"packet-manifest.json"})
            for path in checked:
                assert (temp / path).read_bytes() == (ROOT / path).read_bytes(), (repeat, path)
            regeneration.append({"repeat": repeat + 1, "exit_code": result.returncode, "byte_identical_files": len(checked), "generator_stdout": json.loads(result.stdout)})
    receipt = {
        "schema_version": 1, "status": "passed", "scope": "prepared task packet only; no participant execution or live token qualification",
        "generator_sha256": digest(generator), "verifier_sha256": digest(Path(__file__)),
        "packet_manifest_sha256": digest(manifest_path),
        "task_count": 8, "public_case_count": 16, "hidden_case_count": 96,
        "input_contract_and_expected_output_checks": cases_checked,
        "duplicate_inputs_within_task": 0, "oracle_self_checks": oracle_checks,
        "clean_regeneration": regeneration,
        "participant_sessions": 0, "participant_deliveries": 0,
    }
    encoded = json.dumps(receipt, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(json.dumps({k: receipt[k] for k in ("status", "task_count", "public_case_count", "hidden_case_count", "input_contract_and_expected_output_checks", "packet_manifest_sha256", "participant_sessions")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
