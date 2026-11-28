#!/usr/bin/env python3
"""CI entry points with explicit scope, command logs, and strict conformance gates."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
COMPILER = ROOT / "compiler"
OUTPUT = COMPILER / "target" / "ci"
REPORT = OUTPUT / "results.json"
NATIVE_TESTS = [
    "p0", "alloc_check", "arena", "modules", "index_assignment",
    "mutref_returns", "borrow_provenance", "collections", "fix", "runtime_io_concurrency", "stdlib_io",
    "fs_create_new", "selfhost_blockers", "system",
]
SUMMARY = re.compile(
    r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out"
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_report(report):
    REPORT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def load_report():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if REPORT.exists():
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        if report["revision"] != revision:
            raise RuntimeError("CI output belongs to another revision; use a fresh checkout")
        return report
    return {
        "schema": "yavo-ci/1", "revision": revision,
        "host": {"platform": platform.platform(), "machine": platform.machine()},
        "rust_toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "1.99.1"),
        "toolchain_lock_sha256": digest(COMPILER / "toolchain.lock"),
        "cargo_lock_sha256": digest(COMPILER / "Cargo.lock"),
        "commands": [], "status": "in_progress",
    }


def invoke(report, label, argv, cwd=COMPILER, timeout=1200):
    """Always preserve the command and full combined output, including failure."""
    print("RUN " + label + ": " + repr(argv), flush=True)
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(COMPILER / "target")
    started = time.monotonic()
    timed_out = False
    path = OUTPUT / (label + ".log")
    # Files keep a surviving grandchild from holding a capture pipe open forever.
    with path.open("wb") as log:
        try:
            options = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt" else {"start_new_session": True}
            process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                       stdout=log, stderr=subprocess.STDOUT, **options)
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out, code = True, 124
                if os.name == "nt":
                    try:
                        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                       stdout=log, stderr=subprocess.STDOUT, timeout=15, check=False)
                    except (OSError, subprocess.TimeoutExpired):
                        process.kill()
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
        except OSError as error:
            code = 127
            log.write(str(error).encode("utf-8"))
    output = path.read_bytes()
    text = output.decode("utf-8", errors="replace")
    print(text, end="" if text.endswith("\n") else "\n", flush=True)
    row = {"label": label, "argv": argv, "cwd": str(cwd), "exit_code": code,
           "timed_out": timed_out, "seconds": time.monotonic() - started,
           "log": path.name, "log_sha256": digest(path)}
    report["commands"].append(row)
    write_report(report)
    return row, text


def conformance(report, mode):
    source = COMPILER / "target" / "tmp" / "conformance-report.json"
    destination = OUTPUT / ("conformance-" + mode + ".json")
    if not source.is_file():
        return {"passed": False, "errors": ["conformance report was not generated"]}
    shutil.copyfile(source, destination)
    rows = json.loads(destination.read_text(encoding="utf-8"))
    expected = json.loads((ROOT / "tools/ci/conformance.json").read_text(encoding="utf-8"))
    manifest = json.loads((ROOT / "yavo-tests/MANIFEST.json").read_text(encoding="utf-8"))
    entries = manifest["tests"] if isinstance(manifest, dict) else manifest
    ids = [row["id"] for row in rows]
    expected_ids = {row["id"] for row in entries}
    counts = Counter(row["status"] for row in rows)
    errors = []
    if len(ids) != len(set(ids)) or set(ids) != expected_ids:
        errors.append("reported case identities differ from MANIFEST.json")
    if len(rows) != expected["cases"]:
        errors.append("case count differs from the reviewed CI contract")
    wanted = {"pass": expected["pass"], "unsupported": len(expected["unsupported"]),
              "superseded": len(expected["superseded"])}
    if dict(counts) != wanted:
        errors.append("classified counts differ: " + repr(dict(counts)))
    for status in ("unsupported", "superseded"):
        actual_ids = sorted(row["id"] for row in rows if row["status"] == status)
        if actual_ids != sorted(expected[status]):
            errors.append(status + " identities differ: " + repr(actual_ids))
    return {"passed": not errors, "counts": dict(counts), "errors": errors,
            "report": destination.name, "sha256": digest(destination)}


def summarize(report):
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary:
        return
    lines = ["## Yavo validation", "", "Revision: `" + report["revision"] + "`", "",
             "Host: " + report["host"]["platform"] + " / " + report["host"]["machine"], "",
             "Scope: " + report.get("scope", "compiler/toolchain setup"), "",
             "Result: **" + report["status"] + "**", ""]
    for mode, data in report.get("suites", {}).items():
        totals = data.get("totals", {})
        lines.append("- " + mode + ": " + repr(totals) + "; exit " + str(data["exit_code"]))
        if "conformance" in data:
            lines.append("  Conformance: " + repr(data["conformance"]))
    if "sanitizers" in report:
        lines.append("- Sanitizers: " + repr(report["sanitizers"]))
    lines.extend(["", "Complete commands, logs, source/lock identities, and JSON results are in this job's artifact.", ""])
    with open(summary, "a", encoding="utf-8") as file:
        file.write("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["setup", "native", "sanitize"])
    parser.add_argument("--suite", choices=["full", "native"], default="full")
    parser.add_argument("--without-zig", action="store_true")
    args = parser.parse_args()
    for forbidden in ("YAVO_CC", "YAVO_UPDATE_BASELINE"):
        if forbidden in os.environ:
            parser.error(forbidden + " must not override the CI contract")
    report = load_report()
    expected_machine = os.environ.get("YAVO_CI_MACHINE")
    if expected_machine and report["host"]["machine"] != expected_machine:
        parser.error("runner architecture differs from the named matrix entry")
    os.environ["RUSTUP_TOOLCHAIN"] = report["rust_toolchain"]
    binary = COMPILER / "target" / "release" / ("yavo.exe" if os.name == "nt" else "yavo")
    if args.action == "setup":
        commands = [
            ("rust-install", ["rustup", "toolchain", "install", report["rust_toolchain"], "--profile", "minimal"]),
            ("rust-version", ["rustc", "-vV"]),
            ("cargo-version", ["cargo", "--version"]),
            ("cargo-fetch", ["cargo", "fetch", "--locked"]),
            ("compiler-build", ["cargo", "build", "--locked", "--offline", "--release"]),
            ("compiler-version", [str(binary), "--version"]),
        ]
        if not args.without_zig:
            commands.extend([("zig-install", [str(binary), "toolchain", "install"]),
                             ("zig-status", [str(binary), "toolchain", "status"])])
            fixture = COMPILER / "tests/p0/run_arith.yv"
            for mode in ("debug", "release"):
                artifact = OUTPUT / ("warmup-" + mode + (".exe" if os.name == "nt" else ""))
                command = [str(binary), "build", str(fixture), "-o", str(artifact)]
                if mode == "release":
                    command.append("--release")
                commands.extend([("zig-warmup-build-" + mode, command),
                                 ("zig-warmup-run-" + mode, [str(artifact)])])
            report["setup_warmup"] = {"fixture": "compiler/tests/p0/run_arith.yv",
                                      "modes": ["debug", "release"], "included_in_test_totals": False}
        for label, command in commands:
            # A cold Zig download has taken over 20 minutes on hosted Windows runners.
            row, _ = invoke(report, label, command, timeout=2700 if label == "zig-install" else 1200)
            if row["exit_code"] != 0:
                report["status"] = "setup_failed"
                write_report(report)
                summarize(report)
                return 1
        report["compiler_sha256"] = digest(binary)
        write_report(report)
        return 0
    if args.action == "native":
        if args.suite == "full" and not (sys.platform == "linux" and platform.machine() == "x86_64"):
            parser.error("the frozen full conformance gate currently requires Linux x86_64")
        selection = [] if args.suite == "full" else ["--bin", "yavo"] + [arg for test in NATIVE_TESTS for arg in ("--test", test)]
        report["scope"] = ("All Cargo tests and frozen conformance on Linux x86_64" if args.suite == "full"
                           else "Native compiler unit tests plus " + ", ".join(NATIVE_TESTS) + "; excludes platform-limited frozen conformance")
        report["suites"] = {}
        failed = False
        for mode in ("debug", "release"):
            previous = COMPILER / "target/tmp/conformance-report.json"
            if previous.exists():
                previous.unlink()
            command = ["cargo", "test", "--locked", "--offline"]
            if mode == "release":
                command.append("--release")
            command.extend(selection + ["--", "--nocapture"])
            row, output = invoke(report, "tests-" + mode, command, timeout=1800)
            matches = [tuple(map(int, match)) for match in SUMMARY.findall(output)]
            totals = dict(zip(("passed", "failed", "ignored", "measured", "filtered_out"),
                              [sum(values) for values in zip(*matches)])) if matches else {}
            data = {"exit_code": row["exit_code"], "totals": totals, "selection": selection}
            valid = row["exit_code"] == 0 and bool(matches) and totals["passed"] > 0 and totals["failed"] == 0
            valid = valid and totals["ignored"] == 1 and totals["filtered_out"] == 0
            if args.suite == "full":
                data["conformance"] = conformance(report, mode)
                valid = valid and data["conformance"]["passed"]
            data["passed"] = valid
            report["suites"][mode] = data
            failed = failed or not valid
            write_report(report)
            if row["timed_out"]:
                # Do not start a second suite after an infrastructure timeout.
                break
        report["status"] = "failed" if failed else "passed"
    else:
        report["scope"] = "Independent Linux GCC ASan+UBSan of every current P0 run/panic fixture with original stdin and Cargo-package cwd; pinned Zig conformance unchanged"
        bundle = OUTPUT / "sanitizers-bundle.json"
        run_dir = OUTPUT / "sanitizers-run"
        row, _ = invoke(report, "sanitizers-export", ["perl", "tools/sanitizers/export.pl", "--out", str(bundle)], cwd=ROOT)
        if row["exit_code"] == 0:
            row, _ = invoke(report, "sanitizers-run", [sys.executable, "tools/sanitizers/run.py", str(bundle),
                             "--cc", "gcc", "--jobs", "2", "--p0-root", str(ROOT), "--out", str(run_dir)], cwd=ROOT)
        sanitizer_report = run_dir / "report.json"
        if sanitizer_report.is_file():
            details = json.loads(sanitizer_report.read_text(encoding="utf-8"))
            report["sanitizers"] = {key: details.get(key) for key in
                                    ("status", "passed", "failed", "capabilities", "p0_contract", "p0_coverage")}
        report["status"] = "passed" if row["exit_code"] == 0 and report.get("sanitizers", {}).get("status") == "passed" else "failed"
    write_report(report)
    summarize(report)
    return int(report["status"] != "passed")


if __name__ == "__main__":
    sys.exit(main())
