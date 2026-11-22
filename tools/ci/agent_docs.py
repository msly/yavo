#!/usr/bin/env python3
"""Compile the starter guide verbatim and check its high-impact ownership rules."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run(argv, input_data, directory, environment, timeout=120):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "stdin").write_bytes(input_data)
    started = time.monotonic()
    process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, cwd=ROOT, env=environment,
                               start_new_session=(os.name == "posix"))
    timed_out = False
    try:
        stdout, stderr = process.communicate(input_data, timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        stdout, stderr = process.communicate()
    record = {"argv": argv, "cwd": str(ROOT), "elapsed_seconds": time.monotonic() - started,
              "exit_code": process.returncode, "timed_out": timed_out,
              "environment_overrides": {"YAVO_ALLOC_CHECK": "1"}}
    for name, data in (("stdin", input_data), ("stdout", stdout), ("stderr", stderr)):
        (directory / name).write_bytes(data)
        record[name] = {"bytes": len(data), "sha256": sha(data), "base64": base64.b64encode(data).decode()}
    save(directory / "command.json", record)
    return record, stdout, stderr


def diagnostic_codes(value):
    if isinstance(value, dict):
        return ({value["code"]} if isinstance(value.get("code"), str) else set()).union(
            *(diagnostic_codes(child) for child in value.values()))
    if isinstance(value, list):
        return set().union(*(diagnostic_codes(child) for child in value))
    return set()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--compiler", type=Path, default=ROOT / "compiler/target/release/yavo")
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/agent-docs")
    args = parser.parse_args()
    compiler, output = args.compiler.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    guide = (ROOT / "docs/agent-start.md").read_bytes()
    examples = {}
    for match in re.finditer(rb"^```yv example=([a-z0-9_/.]+)\n(.*?)^```\n", guide, re.M | re.S):
        name, data = match[1].decode(), match[2]
        if Path(name).is_absolute() or ".." in Path(name).parts or name in examples:
            raise ValueError("unsafe or repeated example path")
        examples[name] = data
    if len(examples) != 5:
        raise ValueError("starter guide must contain exactly five source blocks")
    cases = json.loads((ROOT / "tools/ci/agent-doc-cases.json").read_text())
    sources = output / "sources"
    for name, data in examples.items():
        path = sources / name
        if not path.resolve().is_relative_to(sources.resolve()):
            raise ValueError("example path escapes source directory")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    environment = dict(os.environ, YAVO_ALLOC_CHECK="1")
    for key in ("YAVO_CC", "YAVO_UPDATE_BASELINE", "YAVO_NO_CACHE", "YAVO_CACHE_TRACE", "OPENAI_API_KEY", "CODEX_API_KEY"):
        environment.pop(key, None)
    results = []
    for case in cases:
        entry = sources / case["entry"]
        if ".." in Path(case["entry"]).parts or Path(case["entry"]).is_absolute():
            raise ValueError("unsafe case entry")
        if not entry.resolve().is_relative_to(sources.resolve()):
            raise ValueError("case path escapes source directory")
        if "source" in case:
            entry.parent.mkdir(parents=True, exist_ok=True)
            entry.write_text(case["source"], encoding="utf-8")
        item = {"id": case["id"], "kind": case["kind"], "source_sha256": sha(entry.read_bytes()), "passed": False}
        directory = output / "commands" / case["id"]
        if "error" in case:
            record, stdout, stderr = run([str(compiler), "check", str(entry), "--json"], b"", directory / "check", environment)
            codes = set()
            for raw in (stdout, stderr):
                try:
                    codes.update(diagnostic_codes(json.loads(raw)))
                except (ValueError, UnicodeError):
                    pass
            item.update(check=record, expected_diagnostic=case["error"], observed_diagnostics=sorted(codes))
            item["passed"] = record["exit_code"] == 1 and not record["timed_out"] and case["error"] in codes
        else:
            binary = output / "bin" / case["id"]
            binary.parent.mkdir(parents=True, exist_ok=True)
            record, _, _ = run([str(compiler), "build", str(entry), "-o", str(binary)], b"", directory / "build", environment)
            item["build"] = record
            if record["exit_code"] == 0 and not record["timed_out"]:
                executed, stdout, stderr = run([str(binary)], case["stdin"].encode(), directory / "run", environment, 10)
                item.update(run=executed, binary_sha256=sha(binary.read_bytes()), expected_stdout=case["stdout"])
                item["passed"] = executed["exit_code"] == 0 and not executed["timed_out"] and stdout == case["stdout"].encode() and stderr == b""
        results.append(item)
        print(case["id"] + ": " + ("PASS" if item["passed"] else "FAIL"), flush=True)
    passed = sum(case["passed"] for case in results)
    report = {"schema_version": 1, "kind": "yavo.agent-docs.ci", "status": "passed" if passed == len(cases) else "failed",
              "passed": passed, "failed": len(cases) - passed, "total": len(cases), "results": results,
              "compiler_sha256": sha(compiler.read_bytes()), "guide_sha256": sha(guide),
              "source_blocks": {name: sha(data) for name, data in examples.items()},
              "cases_sha256": sha((ROOT / "tools/ci/agent-doc-cases.json").read_bytes())}
    save(output / "receipt.json", report)
    print(json.dumps({key: report[key] for key in ("status", "passed", "failed", "total")}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
