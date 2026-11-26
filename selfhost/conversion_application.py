"""Compile the unchanged two-module T04 unit converter through selfhost and stage0.

The frozen stdin/stdout scenarios come from the published exact-rational contract,
not from either executable. Both Debug and Release builds must match those bytes
and each other with allocation checking enabled.
"""
from __future__ import annotations

import hashlib
import json
from contextlib import ExitStack
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
APP_RELATIVE = Path("experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T04/final")
APP = ROOT / APP_RELATIVE
MANIFEST = ROOT / "selfhost/tests/applications/unit-conversion.json"
CONTRACT = ROOT / "experiments/coldstart-2026-11-22-r5/public/T04.md"
SOURCE_SHA256 = {
    "main.yv": "beb85b017433598ead7e1a1651d7bc76d43a965e26ce0645e5363a4d33283e1b",
    "conversion.yv": "162fea991ec55945b120288141e6b453968aa221aa83752d8c17eadc7741eb89",
}


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    """Bind the harness, fixed oracle, published contract and original modules."""
    files = [Path(__file__).resolve(), MANIFEST, CONTRACT,
             *(APP / name for name in SOURCE_SHA256)]
    return {str(path): _digest(path) for path in files}


def load_scenarios():
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("schema") != "yavo-unit-converter-scenarios/1":
        raise ValueError("unexpected unit-conversion scenario schema")
    if manifest.get("application") != (APP_RELATIVE / "main.yv").as_posix():
        raise ValueError("scenario manifest does not select the original yavo-b T04 application")
    expected_sources = {(APP_RELATIVE / name).as_posix(): value for name, value in SOURCE_SHA256.items()}
    if manifest.get("source_sha256") != expected_sources:
        raise ValueError("scenario manifest changed the pinned original source identities")
    for name, expected in SOURCE_SHA256.items():
        if _digest(APP / name) != expected:
            raise ValueError("original unit-conversion source changed: " + name)
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("unit-conversion scenarios must be a nonempty list")
    seen = set()
    for case in cases:
        identifier = case["id"]
        if not isinstance(identifier, str) or not re.fullmatch(r"[a-z0-9_]+", identifier) or identifier in seen:
            raise ValueError("invalid or repeated scenario id")
        seen.add(identifier)
        if case.get("exit_code") != 0 or case.get("stderr") != "":
            raise ValueError(identifier + ": contract requires exit 0 and empty stderr")
        data = case["stdin"].encode("ascii")
        wanted = case["stdout"].encode("ascii")
        if b"\r" in data or b"\0" in data or (data and not data.endswith(b"\n")):
            raise ValueError(identifier + ": input is outside the published ASCII/LF contract")
        if data.count(b"\n") > 200:
            raise ValueError(identifier + ": input exceeds the contract's 200-record bound")
        if wanted and not wanted.endswith(b"\n"):
            raise ValueError(identifier + ": expected output lacks its final LF")
    return cases


def _run(argv, cwd, environment, timeout, stdin=None):
    started = time.monotonic()
    record = {"exit_code": 127, "timed_out": False}
    out, err = b"", b""
    options = ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
               if os.name == "nt" else {"start_new_session": True})
    try:
        # Regular files cannot keep communicate() waiting on pipes inherited by
        # a compiler's descendants after the direct child has been terminated.
        with ExitStack() as streams:
            stdout = streams.enter_context(tempfile.TemporaryFile(dir=cwd))
            stderr = streams.enter_context(tempfile.TemporaryFile(dir=cwd))
            input_stream = subprocess.DEVNULL
            if stdin is not None:
                input_stream = streams.enter_context(tempfile.TemporaryFile(dir=cwd))
                input_stream.write(stdin)
                input_stream.seek(0)
            process = subprocess.Popen([str(arg) for arg in argv], cwd=cwd, env=environment,
                                       stdin=input_stream, stdout=stdout, stderr=stderr, **options)
            try:
                record["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record.update(exit_code=124, timed_out=True)
                if os.name == "nt":
                    try:
                        killed = subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                                stderr=subprocess.DEVNULL, timeout=10, check=False)
                        if killed.returncode != 0:
                            record["cleanup_error"] = "taskkill exit " + str(killed.returncode)
                            process.kill()
                    except (OSError, subprocess.TimeoutExpired) as error:
                        record["cleanup_error"] = str(error)
                        process.kill()
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        record["cleanup_error"] = "process did not exit after tree termination"
            stdout.seek(0)
            stderr.seek(0)
            out, err = stdout.read(), stderr.read()
    except OSError as error:
        err += str(error).encode("utf-8")
    record["elapsed_seconds"] = time.monotonic() - started
    return record, out, err


def _summary(record, stdout, stderr, preview=False):
    result = {**record, "stdout_bytes": len(stdout), "stderr_bytes": len(stderr),
              "stdout_sha256": hashlib.sha256(stdout).hexdigest(),
              "stderr_sha256": hashlib.sha256(stderr).hexdigest()}
    if preview:
        result["stdout_preview"] = stdout[:2048].decode("utf-8", "replace")
        result["stderr_preview"] = stderr[:2048].decode("utf-8", "replace")
    return result


def _succeeded(record):
    return record["exit_code"] == 0 and not record["timed_out"]


def _matches(record, stdout, stderr, wanted):
    return _succeeded(record) and stdout == wanted and stderr == b""


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    record = {"application": str(APP), "passed": False, "builds": {}, "scenarios": []}
    try:
        cases = load_scenarios()
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        record["error"] = "fixed unit-conversion oracle: " + str(error)
        return record
    record["oracle"] = {"scenarios": len(cases), "input_records": sum(case["stdin"].count("\n") for case in cases),
                        "manifest_sha256": _digest(MANIFEST), "source_sha256": dict(SOURCE_SHA256)}
    record["compiler"] = {"path": str(selfhost), "sha256": _digest(Path(selfhost))}
    build_environment = dict(environment, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    directory.mkdir(parents=True)
    generated = directory / "unit-conversion.c"
    emitted, out, err = _run([selfhost, "emit-c", APP / "main.yv", "-o", generated],
                              ROOT, frontend_environment, 60)
    record["emit"] = _summary(emitted, out, err, not _succeeded(emitted))
    if not _succeeded(emitted) or not generated.is_file():
        record["error"] = "selfhost could not compile the original unit-conversion application"
        return record
    record["generated_c"] = {"bytes": generated.stat().st_size, "sha256": _digest(generated)}
    binaries = {}
    for mode in ("debug", "release"):
        if "-DYV_ALLOC_CHECK" not in cc_flags[mode]:
            record["error"] = "unit-conversion candidate build must enable allocation checking"
            return record
        candidate = directory / mode / "selfhost" / ("yavo-unit-conversion" + exe)
        reference = directory / mode / "stage0" / ("yavo-unit-conversion" + exe)
        candidate.parent.mkdir(parents=True)
        reference.parent.mkdir(parents=True)
        built, out, err = _run([zig, "cc", *cc_flags[mode], "-I", ROOT / "compiler/runtime",
                                generated, "-o", candidate], ROOT, build_environment, 600)
        release = ["--release"] if mode == "release" else []
        rebuilt, rout, rerr = _run([stage0, "build", APP / "main.yv", *release, "-o", reference],
                                    ROOT, build_environment, 600)
        record["builds"][mode] = {"selfhost": _summary(built, out, err, not _succeeded(built)),
                                  "stage0": _summary(rebuilt, rout, rerr, not _succeeded(rebuilt))}
        if not _succeeded(built) or not _succeeded(rebuilt):
            record["error"] = "unit-conversion application build failed in " + mode
            return record
        binaries[mode] = (candidate, reference)
    passed = True
    for case in cases:
        data, wanted = case["stdin"].encode("ascii"), case["stdout"].encode("ascii")
        row = {"id": case["id"], "input_records": data.count(b"\n"), "modes": {}}
        for mode, (candidate, reference) in binaries.items():
            mine, out, err = _run([candidate], directory, build_environment, 10, data)
            theirs, rout, rerr = _run([reference], directory, build_environment, 10, data)
            expected = _matches(mine, out, err, wanted) and _matches(theirs, rout, rerr, wanted)
            agrees = (mine["exit_code"], mine["timed_out"], out, err) == (
                theirs["exit_code"], theirs["timed_out"], rout, rerr)
            row["modes"][mode] = {"selfhost": _summary(mine, out, err, not expected),
                                  "stage0": _summary(theirs, rout, rerr, not expected),
                                  "matches_expectation": expected, "matches_stage0": agrees}
            passed = passed and expected and agrees
        record["scenarios"].append(row)
    record["applicable"] = len(cases)
    record["passed"] = passed
    return record
