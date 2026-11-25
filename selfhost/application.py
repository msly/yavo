"""SH-3 application check: the Yavo-written compiler builds examples/test-summary.

The selfhost compiler emits C for the unmodified four-module application; the
locked Zig builds it in Debug and Release. Every scenario applicable to this host
from the application's frozen acceptance manifest must then match its manual
expectation (stdout bytes, ordered diagnostics, exit status) and the stage0-built
application byte for byte. Scenario loading and expectation matching reuse
tools/ci/test_summary.py, whose oracle check runs first.
"""
from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "examples/test-summary"
DRIVER = ROOT / "tools/ci/test_summary.py"


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    """Files whose bytes the check depends on, for the bootstrap's change guard."""
    files = [Path(__file__).resolve(), DRIVER, *sorted(APP.glob("*.yv")), APP / "SPEC.md",
             APP / "tests/cases.json", APP / "tests/oracle.py", APP / "tests/oracle-freeze.json"]
    return {str(path): _digest(path) for path in files}


def _driver():
    spec = importlib.util.spec_from_file_location("test_summary_driver", DRIVER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _run(argv, cwd, environment, timeout):
    try:
        done = subprocess.run([str(arg) for arg in argv], cwd=cwd, env=environment,
                              stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout)
        return {"exit_code": done.returncode, "timed_out": False}, done.stdout, done.stderr
    except subprocess.TimeoutExpired as expired:
        return {"exit_code": None, "timed_out": True}, expired.stdout or b"", expired.stderr or b""


def _summary(record, stdout, stderr):
    return {**record, "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
            "stderr_sha256": hashlib.sha256(stderr).hexdigest()}


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    driver = _driver()
    record = {"application": str(APP), "passed": False, "builds": {}, "scenarios": []}
    try:
        cases, _bound, oracle = driver.frozen_oracle()
    except (OSError, ValueError, KeyError) as error:
        record["error"] = "frozen oracle: " + str(error)
        return record
    record["oracle"] = {"status": oracle["status"], "scenarios": oracle["scenarios"]}
    directory.mkdir(parents=True)
    generated = directory / "test-summary.c"
    emitted, out, err = _run([selfhost, "emit-c", APP / "main.yv", "-o", generated], ROOT,
                             frontend_environment, 60)
    record["emit"] = _summary(emitted, out, err)
    if emitted["exit_code"] != 0 or not generated.is_file():
        record["error"] = "selfhost could not compile the application: " + err.decode("utf-8", "replace")
        return record
    record["generated_c"] = {"bytes": generated.stat().st_size, "sha256": _digest(generated)}
    binaries = {}
    for mode in ("debug", "release"):
        candidate = directory / mode / "selfhost" / ("yavo-test-summary" + exe)
        reference = directory / mode / "stage0" / ("yavo-test-summary" + exe)
        candidate.parent.mkdir(parents=True)
        reference.parent.mkdir(parents=True)
        built, out, err = _run([zig, "cc", *cc_flags[mode], "-I", ROOT / "compiler/runtime", generated,
                                "-o", candidate], ROOT, environment, 600)
        release = ["--release"] if mode == "release" else []
        rebuilt, rout, rerr = _run([stage0, "build", APP / "main.yv", *release, "-o", reference], ROOT,
                                   environment, 600)
        record["builds"][mode] = {"selfhost": _summary(built, out, err), "stage0": _summary(rebuilt, rout, rerr)}
        if built["exit_code"] != 0 or rebuilt["exit_code"] != 0:
            record["error"] = "application build failed in " + mode
            return record
        binaries[mode] = (candidate, reference)
    passed = True
    for case in cases:
        if os.name not in case.get("platforms", [os.name]):
            record["scenarios"].append({"id": case["id"], "applicable": False})
            continue
        args, contents, wanted = driver.load_case(case)
        row = {"id": case["id"], "applicable": True, "modes": {}}
        for mode, (candidate, reference) in binaries.items():
            observed = []
            for label, program in (("selfhost", candidate), ("stage0", reference)):
                place = directory / "cases" / case["id"] / mode / label
                driver.materialize(case, place, contents)
                result, out, err = _run([program, *args], place, environment, 10)
                observed.append((result, out, err))
                shutil.rmtree(place, ignore_errors=True)
            (mine, out, err), (theirs, rout, rerr) = observed
            expected = (not mine["timed_out"] and mine["exit_code"] == wanted["exit"]
                        and out == driver.expected_stdout(wanted) and driver.stderr_matches(err, wanted))
            agrees = (mine["exit_code"], out, err) == (theirs["exit_code"], rout, rerr)
            row["modes"][mode] = {"selfhost": _summary(mine, out, err), "stage0": _summary(theirs, rout, rerr),
                                  "matches_expectation": expected, "matches_stage0": agrees}
            passed = passed and expected and agrees
        record["scenarios"].append(row)
    applicable = [row for row in record["scenarios"] if row["applicable"]]
    record["applicable"] = len(applicable)
    record["passed"] = passed and len(applicable) > 0
    return record
