#!/usr/bin/env python3
"""Capture the independent stage0 oracle without reading selfhost output."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
TESTS = ROOT / "selfhost/tests"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--attempt", required=True)
    parser.add_argument("--profile", choices=("Debug", "Release"), default="Debug")
    parser.add_argument("--only", nargs="*")
    args = parser.parse_args()
    destination = HERE / ("attempt-" + args.attempt + ".json")
    if destination.exists():
        raise SystemExit("refusing to replace an existing oracle attempt: " + str(destination))
    build_root = ROOT / "compiler/target/batch29-oracle" / args.attempt
    build_root.mkdir(parents=True, exist_ok=False)
    compiler = ROOT / "compiler/target/release/yavo"
    zig = Path("/home/appops/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig")
    metadata = json.loads((HERE / "candidates.json").read_text())
    selected = [c for c in metadata["cases"] if not args.only or c["id"] in args.only]
    env = dict(os.environ, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    paths = [compiler, zig, ROOT / "compiler/runtime/yavo_rt.h",
             ROOT / "compiler/toolchain.lock", Path(__file__), HERE / "candidates.json"]
    bound = {str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p): sha(p) for p in paths}
    receipt = {"schema": "yavo-result-stage0-oracle/1", "baseline": metadata["baseline"],
               "profile": args.profile, "environment": {"YAVO_ALLOC_CHECK": "1", "YAVO_NO_CACHE": "1"},
               "inputs": bound, "records": [], "status": "running"}

    def save():
        destination.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")

    def command(argv, timeout):
        start = time.monotonic()
        try:
            process = subprocess.run([str(x) for x in argv], cwd=ROOT, env=env,
                                     text=True, capture_output=True, timeout=timeout)
            return {"command": [str(x) for x in argv], "exit_code": process.returncode,
                    "stdout": process.stdout, "stderr": process.stderr,
                    "seconds": round(time.monotonic() - start, 3), "timed_out": False}
        except subprocess.TimeoutExpired as error:
            def decode(value):
                return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            return {"command": [str(x) for x in argv], "exit_code": None,
                    "stdout": decode(error.stdout), "stderr": decode(error.stderr),
                    "seconds": round(time.monotonic() - start, 3), "timed_out": True}

    save()
    for case in selected:
        path = TESTS / case["file"]
        relative = str(path.relative_to(ROOT))
        bound[relative] = sha(path)
        record = {"id": case["id"], "kind": case["kind"], "file": case["file"],
                  "source_sha256": sha(path), "source": path.read_text(), "passed": False}
        receipt["records"].append(record)
        check = command([compiler, "check", path, "--json"], 30)
        record["check"] = check
        diagnostics = json.loads(check["stdout"]).get("diagnostics", []) if check["stdout"].strip().startswith("{") else []
        errors = [d for d in diagnostics if d["severity"] == "ERROR"]
        record["first_error"] = ({key: errors[0][key] for key in ("code", "line", "column", "message")} if errors else None)
        wanted_exit = 1 if case["kind"] in ("reject", "shared_unsupported") else 0
        record["passed"] = check["exit_code"] == wanted_exit and not check["timed_out"]
        if case["kind"] == "shared_unsupported":
            record["passed"] = record["passed"] and bool(errors) and all(d["code"] == "U0001" for d in errors)
        if case["kind"] == "reject" and "first_error" in case:
            record["passed"] = record["passed"] and bool(record["first_error"]) and all(record["first_error"].get(k) == v for k, v in case["first_error"].items())
        if case["kind"] == "run" and check["exit_code"] == 0:
            directory = build_root / case["id"]
            directory.mkdir()
            binary = directory / "program"
            argv = [compiler, "build", path, "-o", binary, "--emit-c", directory / "program.c", "--json"]
            if args.profile == "Release":
                argv.append("--release")
            build = command(argv, 180)
            record["build"] = build
            record["passed"] = record["passed"] and build["exit_code"] == 0 and not build["timed_out"]
            if build["exit_code"] == 0:
                run = command([binary], 15)
                record["run"] = run
                record["binary_sha256"] = sha(binary)
                record["generated_c_sha256"] = sha(directory / "program.c")
                record["expected"] = {"exit_code": 0, "stdout": case["stdout"], "stderr": ""}
                record["passed"] = record["passed"] and run["exit_code"] == 0 and run["stdout"] == case["stdout"] and run["stderr"] == "" and not run["timed_out"]
        save()
        print(case["id"], "PASS" if record["passed"] else "FAIL", record["first_error"] or "",
              "runtime=" + repr(record.get("run", {}).get("stdout")), flush=True)

    receipt["inputs_changed"] = [name for name, expected in bound.items()
                                 if not (ROOT / name).is_file() or sha(ROOT / name) != expected]
    receipt["passed"] = sum(r["passed"] for r in receipt["records"])
    receipt["total"] = len(receipt["records"])
    receipt["status"] = "passed" if receipt["passed"] == receipt["total"] and not receipt["inputs_changed"] else "failed"
    save()
    print("Receipt:", destination)
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
