"""Run the unchanged r4 T11 diff CLI against fixed line-diff and error scenarios.

Both compiler paths must match the published deletion-on-LCS-tie ordering and
byte-exact expectations in Debug and Release. The original program calls
os.exit(1/2) for differences/errors; only its exit-0 main return reaches the
allocation check, although every build enables the instrumentation.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import shutil

ROOT = Path(__file__).resolve().parents[1]
APP_RELATIVE = Path("experiments/coldstart-2026-11-21-r4/yavo/T11.yv")
APP = ROOT / APP_RELATIVE
SOURCE_SHA256 = "6caddd13162d57caa859c58323c4993e27809e5c3100bf23b3139d9880f43ec8"
MANIFEST = ROOT / "selfhost/tests/applications/text-diff.json"
CONTRACT = ROOT / "experiments/coldstart-2026-11-21-r4/TASKS.md"
LIBRARY_CONTRACT = ROOT / "docs/stdlib_contract.md"
RUNNER = ROOT / "selfhost/conversion_application.py"
FIXTURES = tuple(ROOT / "experiments/coldstart-2026-11-21-r4/tasks" / name
                 for name in ("T11a.txt", "T11b.txt", "T11aa.out", "T11ab.out"))


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _runner():
    spec = importlib.util.spec_from_file_location("text_diff_application_runner", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inputs():
    """Bind original source, contracts, fixed oracle, fixtures and shared runner."""
    files = [Path(__file__).resolve(), RUNNER, MANIFEST, CONTRACT, LIBRARY_CONTRACT, APP, *FIXTURES]
    return {str(path): _digest(path) for path in files}


def _relative(name):
    if not isinstance(name, str) or not name or "\\" in name or ":" in name:
        raise ValueError("invalid fixture or argument path")
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or ".." in path.parts:
        raise ValueError("fixture or argument path escapes its scenario")
    return Path(*path.parts)


def _repository_data(name, expected):
    path = ROOT / _relative(name)
    if path not in FIXTURES:
        raise ValueError("unregistered published fixture: " + name)
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("published fixture changed: " + name)
    return data


def load_case(case):
    args = case["args"]
    if not isinstance(args, list):
        raise ValueError("scenario arguments must be a list")
    for argument in args:
        _relative(argument)
    contents = {}
    for fixture in case["files"]:
        name = fixture["path"]
        _relative(name)
        if name in contents:
            raise ValueError("duplicate scenario fixture")
        if sum(key in fixture for key in ("text", "hex", "repository")) != 1:
            raise ValueError("fixture needs exactly one byte source")
        if "repository" in fixture:
            data = _repository_data(fixture["repository"], fixture["sha256"])
        elif "hex" in fixture:
            data = bytes.fromhex(fixture["hex"])
        else:
            data = fixture["text"].encode("utf-8")
        contents[name] = data
    expected = case["expected"]
    if expected["exit_code"] not in (0, 1, 2):
        raise ValueError("unexpected diff exit code")
    if sum(key in expected for key in ("stdout", "stdout_repository")) != 1:
        raise ValueError("expectation needs exactly one stdout source")
    if "stdout_repository" in expected:
        stdout = _repository_data(expected["stdout_repository"], expected["stdout_sha256"])
    else:
        stdout = expected["stdout"].encode("utf-8")
    stderr = expected["stderr"].encode("utf-8")
    return args, contents, (expected["exit_code"], stdout, stderr)


def load_scenarios():
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("schema") != "yavo-text-diff-scenarios/1":
        raise ValueError("unexpected text-diff scenario schema")
    if manifest.get("application") != APP_RELATIVE.as_posix() or manifest.get("source_sha256") != SOURCE_SHA256:
        raise ValueError("scenario manifest changed the original T11 source identity")
    if _digest(APP) != SOURCE_SHA256:
        raise ValueError("original T11 source changed")
    expected_contract = {"path": CONTRACT.relative_to(ROOT).as_posix(), "sha256": _digest(CONTRACT)}
    if manifest.get("contract") != expected_contract:
        raise ValueError("published T11 contract changed")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("text-diff scenarios must be a nonempty list")
    seen = set()
    for case in cases:
        identifier = case["id"]
        if not isinstance(identifier, str) or not re.fullmatch(r"[a-z0-9_]+", identifier) or identifier in seen:
            raise ValueError("invalid or repeated scenario id")
        seen.add(identifier)
        load_case(case)
    return cases


def _matches(result, stdout, stderr, wanted):
    return not result["timed_out"] and (result["exit_code"], stdout, stderr) == wanted


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    runner = _runner()
    record = {"application": str(APP), "passed": False, "builds": {}, "scenarios": []}
    try:
        cases = load_scenarios()
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        record["error"] = "fixed text-diff oracle: " + str(error)
        return record
    record["oracle"] = {"scenarios": len(cases), "manifest_sha256": _digest(MANIFEST),
                        "source_sha256": SOURCE_SHA256, "contract_sha256": _digest(CONTRACT)}
    record["allocation_check"] = {
        "enabled": True, "normal_return_cases": sum(case["expected"]["exit_code"] == 0 for case in cases),
        "os_exit_cases": sum(case["expected"]["exit_code"] != 0 for case in cases),
        "scope": "Only normal main return checks allocations; original os.exit(1/2) skips destructors."}
    record["compiler"] = {"path": str(selfhost), "sha256": _digest(Path(selfhost))}
    build_environment = dict(environment, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    directory.mkdir(parents=True)
    generated = directory / "text-diff.c"
    emitted, out, err = runner._run([selfhost, "emit-c", APP, "-o", generated],
                                    ROOT, frontend_environment, 60)
    record["emit"] = runner._summary(emitted, out, err, not runner._succeeded(emitted))
    if not runner._succeeded(emitted) or not generated.is_file():
        record["error"] = "selfhost could not compile the original T11 diff application"
        return record
    record["generated_c"] = {"bytes": generated.stat().st_size, "sha256": _digest(generated)}
    binaries = {}
    for mode in ("debug", "release"):
        if "-DYV_ALLOC_CHECK" not in cc_flags[mode]:
            record["error"] = "text-diff candidate build must enable allocation checking"
            return record
        candidate = directory / mode / "selfhost" / ("yavo-text-diff" + exe)
        reference = directory / mode / "stage0" / ("yavo-text-diff" + exe)
        candidate.parent.mkdir(parents=True)
        reference.parent.mkdir(parents=True)
        built, out, err = runner._run([zig, "cc", *cc_flags[mode], "-I", ROOT / "compiler/runtime",
                                       generated, "-o", candidate], ROOT, build_environment, 600)
        release = ["--release"] if mode == "release" else []
        rebuilt, rout, rerr = runner._run([stage0, "build", APP, *release, "-o", reference],
                                          ROOT, build_environment, 600)
        record["builds"][mode] = {
            "selfhost": runner._summary(built, out, err, not runner._succeeded(built)),
            "stage0": runner._summary(rebuilt, rout, rerr, not runner._succeeded(rebuilt))}
        if not runner._succeeded(built) or not runner._succeeded(rebuilt):
            record["error"] = "text-diff application build failed in " + mode
            return record
        binaries[mode] = (candidate, reference)
    passed = True
    executions = 0
    for case in cases:
        args, contents, wanted = load_case(case)
        row = {"id": case["id"], "scope": case["scope"],
               "allocation_exit_check": wanted[0] == 0, "modes": {}}
        for mode, (candidate, reference) in binaries.items():
            observed = []
            for label, program in (("selfhost", candidate), ("stage0", reference)):
                place = directory / "cases" / case["id"] / mode / label
                place.mkdir(parents=True)
                try:
                    for name, data in contents.items():
                        target = place / _relative(name)
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(data)
                    observed.append(runner._run([program, *args], place, build_environment, 10))
                    executions += 1
                finally:
                    shutil.rmtree(place, ignore_errors=True)
            (mine, out, err), (theirs, rout, rerr) = observed
            expected = _matches(mine, out, err, wanted) and _matches(theirs, rout, rerr, wanted)
            agrees = (mine["exit_code"], mine["timed_out"], out, err) == (
                theirs["exit_code"], theirs["timed_out"], rout, rerr)
            row["modes"][mode] = {"selfhost": runner._summary(mine, out, err, not expected),
                                  "stage0": runner._summary(theirs, rout, rerr, not expected),
                                  "matches_expectation": expected, "matches_stage0": agrees}
            passed = passed and expected and agrees
        record["scenarios"].append(row)
    record["applicable"] = len(cases)
    record["executions"] = executions
    record["passed"] = passed
    return record
