#!/usr/bin/env python3
"""Preserve every application compile attempt and its complete source input."""
import datetime
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
APP = HERE.parent
ROOT = APP.parent.parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    attempts = HERE / "compile-attempts"
    attempts.mkdir(exist_ok=True)
    numbers = [int(p.name) for p in attempts.iterdir() if p.is_dir() and p.name.isdigit()]
    index = max(numbers, default=0) + 1
    destination = attempts / f"{index:03d}"
    snapshot = destination / "source"
    snapshot.mkdir(parents=True)
    sources = {}
    for path in sorted(APP.glob("*.yv")):
        shutil.copyfile(path, snapshot / path.name)
        sources[str(path.relative_to(ROOT))] = sha(path)
    compiler = ROOT / "compiler/target/release/yavo"
    command = [str(compiler), *sys.argv[1:]]
    record = {
        "attempt": index,
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "command": command,
        "cwd": str(ROOT),
        "sources": sources,
        "spec_sha256": sha(APP / "SPEC.md"),
        "compiler_sha256": sha(compiler),
        "runtime_sha256": sha(ROOT / "compiler/runtime/yavo_rt.h"),
        "toolchain_lock_sha256": sha(ROOT / "compiler/toolchain.lock"),
        "environment": {name: os.environ.get(name) for name in ("YAVO_ALLOC_CHECK", "YAVO_NO_CACHE", "YAVO_CACHE_DIR", "YAVO_HOME", "YAVO_CC")},
        "provider_usage": None,
        "cost": None,
        "status": "started",
    }
    output = destination / "attempt.json"
    output.write_text(json.dumps(record, indent=2) + "\n")
    started = time.monotonic()
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=120)
        record.update(status="completed", exit_code=result.returncode)
        stdout, stderr = result.stdout, result.stderr
    except subprocess.TimeoutExpired as error:
        record.update(status="timeout", exit_code=124)
        stdout, stderr = error.stdout or b"", error.stderr or b""
    record["elapsed_seconds"] = time.monotonic() - started
    (destination / "stdout.txt").write_bytes(stdout)
    (destination / "stderr.txt").write_bytes(stderr)
    record["stdout_sha256"] = hashlib.sha256(stdout).hexdigest()
    record["stderr_sha256"] = hashlib.sha256(stderr).hexdigest()
    output.write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({"attempt": index, "exit_code": record["exit_code"], "elapsed_seconds": record["elapsed_seconds"], "record": str(output.relative_to(ROOT))}))
    sys.stdout.flush()
    sys.stdout.buffer.write(stdout)
    sys.stderr.buffer.write(stderr)
    return record["exit_code"]


if __name__ == "__main__":
    raise SystemExit(main())
