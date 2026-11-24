#!/usr/bin/env python3
"""Independent Rust stage0 oracle; no selfhost output is read."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path.cwd().resolve()
HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--attempt", required=True)
    parser.add_argument("--profile", choices=("Debug", "Release"), default="Debug")
    parser.add_argument("--only", nargs="*")
    parser.add_argument("--stage0", type=Path, default=ROOT / "compiler/target/ci/batch30-stage0/yavo")
    args = parser.parse_args()
    work = ROOT / "compiler/target/ci/batch30-oracle/replay" / args.attempt
    work.mkdir(parents=True, exist_ok=False)
    compiler = args.stage0.resolve()
    zig = Path("/home/appops/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig")
    metadata_path = HERE / "candidates.json"
    metadata = json.loads(metadata_path.read_text())
    selected = [c for c in metadata["cases"] if not args.only or c["id"] in args.only]
    assert selected and (not args.only or set(args.only) == {c["id"] for c in selected})
    env = dict(os.environ, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    inputs = {str(p): sha(p) for p in (compiler, zig, metadata_path, Path(__file__),
              ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock")}
    receipt = {"schema": "yavo-batch30-stage0-oracle/1", "baseline": metadata["baseline"],
               "profile": args.profile, "inputs": inputs,
               "environment": {"YAVO_ALLOC_CHECK": "1", "YAVO_NO_CACHE": "1"},
               "records": [], "status": "running"}

    def save():
        (work / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")

    def command(argv, timeout):
        start = time.monotonic()
        argv = [str(a) for a in argv]
        try:
            process = subprocess.run(argv, cwd=ROOT, env=env, text=True,
                                     capture_output=True, timeout=timeout)
            return {"command": argv, "exit_code": process.returncode, "stdout": process.stdout,
                    "stderr": process.stderr, "timed_out": False,
                    "seconds": round(time.monotonic() - start, 3)}
        except subprocess.TimeoutExpired as error:
            decode = lambda value: value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            return {"command": argv, "exit_code": None, "stdout": decode(error.stdout),
                    "stderr": decode(error.stderr), "timed_out": True,
                    "seconds": round(time.monotonic() - start, 3)}

    save()
    for case in selected:
        source = ROOT / "selfhost/tests" / case["file"]
        inputs[str(source)] = sha(source)
        record = {"id": case["id"], "kind": case["kind"], "file": case["file"],
                  "source": source.read_text(), "source_sha256": sha(source), "passed": False}
        receipt["records"].append(record)
        checked = command([compiler, "check", source, "--json"], 30)
        record["check"] = checked
        data = json.loads(checked["stdout"]) if checked["stdout"].strip().startswith("{") else {}
        errors = [d for d in data.get("diagnostics", []) if d["severity"] == "ERROR"]
        record["first_error"] = ({key: errors[0][key] for key in ("code", "line", "column", "message")} if errors else None)
        wanted = 1 if case["kind"] in ("reject", "shared_unsupported") else 0
        record["passed"] = checked["exit_code"] == wanted and not checked["timed_out"]
        if case["kind"] in ("reject", "shared_unsupported"):
            record["passed"] = record["passed"] and bool(errors) and errors[0]["code"] == case["diagnostic"]
            if case["kind"] == "shared_unsupported":
                record["passed"] = record["passed"] and all(d["code"] == "U0001" for d in errors)
        if "first_error" in case:
            record["passed"] = record["passed"] and bool(record["first_error"]) and all(
                record["first_error"][key] == value for key, value in case["first_error"].items())
        if case["kind"] == "run" and checked["exit_code"] == 0:
            directory = work / case["id"]
            directory.mkdir()
            binary = directory / "program"
            c_file = directory / "program.c"
            argv = [compiler, "build", source, "-o", binary, "--emit-c", c_file, "--json"]
            if args.profile == "Release":
                argv.append("--release")
            built = command(argv, 180)
            record["build"] = built
            record["passed"] = record["passed"] and built["exit_code"] == 0 and not built["timed_out"]
            if built["exit_code"] == 0:
                executed = command([binary], 15)
                record["run"] = executed
                record["expected"] = {"exit_code": 0, "stdout": case["stdout"], "stderr": ""}
                record["generated_c_sha256"] = sha(c_file)
                record["binary_sha256"] = sha(binary)
                inputs[str(c_file)] = sha(c_file)
                inputs[str(binary)] = sha(binary)
                record["passed"] = record["passed"] and executed["exit_code"] == 0 and executed["stdout"] == case["stdout"] and executed["stderr"] == "" and not executed["timed_out"]
        save()
        print(case["id"], "PASS" if record["passed"] else "FAIL",
              "" if record["passed"] else record["first_error"] or record.get("run", record.get("build", checked)), flush=True)
    receipt["inputs_changed"] = [p for p, expected in inputs.items()
                                 if not Path(p).is_file() or sha(Path(p)) != expected]
    receipt["passed"] = sum(r["passed"] for r in receipt["records"])
    receipt["total"] = len(receipt["records"])
    receipt["kinds"] = dict(Counter(c["kind"] for c in selected))
    receipt["status"] = "passed" if receipt["passed"] == receipt["total"] and not receipt["inputs_changed"] else "failed"
    save()
    print("RECEIPT", work / "receipt.json", receipt["status"], receipt["passed"], "/", receipt["total"], flush=True)
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
