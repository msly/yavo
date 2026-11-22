#!/usr/bin/env python3
"""Check prepared R6 infrastructure without making model requests or accepting a trial."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PACKET = ROOT / "experiments/docs-ab-2026-11-22-r6"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/agent-eval")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"schema_version": 1, "kind": "yavo.agent-eval.preparation-ci", "status": "failed",
              "created_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(), "commands": [],
              "launch_status": "blocked", "participant_sessions": 0, "participant_deliveries": 0,
              "scope": "Offline executor/protocol contracts, task/document reproducibility and launch refusal; no provider requests."}
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    environment.pop("OPENAI_API_KEY", None)
    environment.pop("CODEX_API_KEY", None)

    def command(name, argv, expected=0):
        result = subprocess.run(argv, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=120)
        log = result.stdout + b"\n--- stderr ---\n" + result.stderr
        path = output / (name + ".log")
        path.write_bytes(log)
        record = {"name": name, "argv": argv, "exit_code": result.returncode, "log_sha256": sha(path)}
        match = re.search(rb"Ran (\d+) tests? in", log)
        if match:
            record["unittest_count"] = int(match[1])
        report["commands"].append(record)
        if result.returncode != expected:
            raise ValueError(f"{name} returned {result.returncode}; see {path}")
        return result

    try:
        manifest = json.loads((PACKET / "prepared-manifest.json").read_bytes())
        if manifest.get("status") != "prepared_blocked" or not manifest.get("files"):
            raise ValueError("missing prepared input inventory")
        for name, digest in manifest["files"].items():
            path = ROOT / name
            if Path(name).is_absolute() or ".." in Path(name).parts or path.is_symlink() or not path.resolve().is_relative_to(ROOT):
                raise ValueError("unsafe prepared input path")
            if sha(path) != digest:
                raise ValueError("prepared input changed: " + name)
        report["prepared_files"] = len(manifest["files"])
        report["prepared_manifest_sha256"] = sha(PACKET / "prepared-manifest.json")
        if list((PACKET / "runs").glob("*/run.json")):
            raise ValueError("prepared-only packet unexpectedly contains participant sessions")
        command("executor-contracts", [sys.executable, "-m", "unittest", "discover", "-s", "tools/agent_eval", "-p", "test_*.py", "-v"])
        command("lab-contracts", [sys.executable, "-m", "unittest", "discover", "-s", "tools/agent_eval/tests", "-p", "test_*.py", "-v"])
        command("document-packet", [sys.executable, str(PACKET / "tools/prepare_docs.py"), "--check"])
        command("task-packet", [sys.executable, str(PACKET / "judge/verify_packet.py"), "--output", str(output / "packet.json")])
        command("independent-oracles", [sys.executable, str(PACKET / "judge/review_oracles.py"), "--packet-root", str(PACKET), "--output", str(output / "oracles.json")])
        command("summary-contracts", [sys.executable, str(PACKET / "judge/selftest.py")])
        command("lab-syntax", ["perl", "-c", str(PACKET / "tools/lab.pl")])
        command("judge-syntax", ["perl", "-c", str(PACKET / "judge/evaluate.pl")])
        with tempfile.TemporaryDirectory(prefix="yavo-blocked-qualification-") as temporary:
            target = Path(temporary) / "probe"
            command("template-launch-refusal", [sys.executable, "tools/agent_eval/executor.py", "qualify", "--config", str(PACKET / "EXECUTOR_CONFIG.json"), "--out", str(target)], expected=2)
            receipt = json.loads((target / "qualification.json").read_bytes())
            if receipt["status"] != "blocked" or receipt["observed"]["provider_requests"] != 0 or receipt["observed"]["provider_responses"] != 0:
                raise ValueError("null configuration did not block before provider use")
            report["template_qualification"] = receipt
        result = command("participant-init-refusal", ["perl", str(PACKET / "tools/lab.pl"), "full-a", "init"], expected=255)
        if b"executor is not qualified" not in result.stderr or list((PACKET / "runs").glob("*/run.json")):
            raise ValueError("unqualified participant initialization was not refused")
        report["status"] = "passed"
    except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
        report["error"] = str(error)
    save(output / "receipt.json", report)
    print(json.dumps({key: report[key] for key in ("status", "launch_status", "participant_sessions", "participant_deliveries")}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
