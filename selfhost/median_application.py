"""Validate the unchanged r5 T11 sliding lower-median CLI in both build modes.

The fixed oracle preserves the 14 archived cases and adds contract-valid bounds.
Expected medians were selected independently by rank counts before either
compiler ran. Every execution returns normally with allocation checks enabled.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
EXPERIMENT = Path("experiments/coldstart-2026-11-22-r5")
APP_RELATIVE = EXPERIMENT / "runs/yavo-b/tasks/T11/final/main.yv"
APP = ROOT / APP_RELATIVE
SOURCE_SHA256 = "e4d211582198c47570790de1791bba37e3907d6524736e1b3ee73c97e6206828"
MANIFEST = ROOT / "selfhost/tests/applications/sliding-median.json"
CONTRACTS = (ROOT / EXPERIMENT / "TASKS.md", ROOT / EXPERIMENT / "public/T11.md")
ARCHIVED_CASES = ROOT / EXPERIMENT / "judge/cases.json"
PUBLIC_CASES = ROOT / EXPERIMENT / "public/cases.json"
RUNNER = ROOT / "selfhost/conversion_application.py"


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _runner():
    spec = importlib.util.spec_from_file_location("sliding_median_application_runner", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inputs():
    """Bind the original source, contracts, archived cases, fixed oracle and runner."""
    files = [Path(__file__).resolve(), RUNNER, MANIFEST, APP,
             *CONTRACTS, ARCHIVED_CASES, PUBLIC_CASES]
    return {str(path): _digest(path) for path in files}


def parse_input(text):
    """Require the published valid-input domain, including a separate K line."""
    data = text.encode("ascii")
    if not data or not data.endswith(b"\n") or re.fullmatch(rb"[0-9+\- \t\n]+", data) is None:
        raise ValueError("median input must be nonempty ASCII numeric fields and spaces/tabs/LF")
    first, rest = data.split(b"\n", 1)
    if re.fullmatch(rb"[ \t]*[+]?[0-9]+[ \t]*", first) is None:
        raise ValueError("median first line must contain only K")
    size = int(first)
    fields = rest.split()
    if any(re.fullmatch(rb"[+-]?[0-9]+", field) is None for field in fields):
        raise ValueError("median sequence contains an invalid integer field")
    values = [int(field) for field in fields]
    if not 1 <= size <= 25 or len(values) > 200 or any(abs(value) > 1_000_000_000 for value in values):
        raise ValueError("median input is outside the published K/length/value bounds")
    return data, size, values


def _task_cases(path):
    tasks = json.loads(path.read_text(encoding="utf-8"))["tasks"]
    selected = [task for task in tasks if task["id"] == "T11"]
    if len(selected) != 1:
        raise ValueError("archived cases must contain exactly one T11 task")
    return selected[0]["cases"]


def _case_io(case):
    return tuple(case[key] for key in ("stdin", "stdout", "stderr", "exit_code"))


def load_scenarios():
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("schema") != "yavo-sliding-median-scenarios/1":
        raise ValueError("unexpected sliding-median scenario schema")
    if manifest.get("application") != APP_RELATIVE.as_posix() or manifest.get("source_sha256") != SOURCE_SHA256:
        raise ValueError("scenario manifest changed the original T11 source identity")
    if _digest(APP) != SOURCE_SHA256:
        raise ValueError("original sliding-median source changed")
    contracts = {path.relative_to(ROOT).as_posix(): _digest(path) for path in CONTRACTS}
    if manifest.get("contracts") != contracts:
        raise ValueError("published sliding-median contracts changed")
    expected_archive = {"path": ARCHIVED_CASES.relative_to(ROOT).as_posix(),
                        "sha256": _digest(ARCHIVED_CASES), "task": "T11", "count": 14}
    if manifest.get("original_cases") != expected_archive:
        raise ValueError("archived T11 case identity changed")
    archived = _task_cases(ARCHIVED_CASES)
    originals = {case["id"]: case for case in archived}
    if len(archived) != 14 or len(originals) != 14:
        raise ValueError("expected 14 distinct archived T11 cases")
    public = _task_cases(PUBLIC_CASES)
    if len(public) != 2 or any(case["id"] not in originals or
                               _case_io(case) != _case_io(originals[case["id"]]) for case in public):
        raise ValueError("published T11 examples disagree with archived cases")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("sliding-median scenarios must be a nonempty list")
    seen, retained = set(), set()
    for case in cases:
        identifier = case["id"]
        if not isinstance(identifier, str) or not re.fullmatch(r"[a-z0-9_]+", identifier) or identifier in seen:
            raise ValueError("invalid or repeated median scenario id")
        seen.add(identifier)
        _, size, values = parse_input(case["stdin"])
        if case.get("exit_code") != 0 or case.get("stderr") != "":
            raise ValueError(identifier + ": contract requires exit 0 and empty stderr")
        wanted = case["stdout"].encode("ascii")
        windows = max(0, len(values) - size + 1)
        if wanted.count(b"\n") != windows + 1 or not wanted.endswith(f"windows: {windows}\n".encode("ascii")):
            raise ValueError(identifier + ": fixed output has the wrong window count")
        if case.get("origin") == "r5_judge":
            source = case["source_case"]
            if source not in originals or source in retained or _case_io(case) != _case_io(originals[source]):
                raise ValueError(identifier + ": archived T11 input or expectation changed")
            if originals[source].get("args") != [] or originals[source].get("files") != {}:
                raise ValueError(identifier + ": unexpected archived argument or file requirements")
            retained.add(source)
        elif case.get("origin") != "contract_boundary":
            raise ValueError(identifier + ": unknown scenario origin")
    if retained != set(originals):
        raise ValueError("fixed scenarios must retain all 14 archived T11 cases")
    return cases


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    runner = _runner()
    record = {"application": str(APP), "passed": False, "builds": {}, "scenarios": []}
    try:
        cases = load_scenarios()
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        record["error"] = "fixed sliding-median oracle: " + str(error)
        return record
    parsed = [parse_input(case["stdin"]) for case in cases]
    record["oracle"] = {"scenarios": len(cases), "archived_cases": 14,
                        "input_values": sum(len(values) for _, _, values in parsed),
                        "windows": sum(max(0, len(values) - size + 1) for _, size, values in parsed),
                        "manifest_sha256": _digest(MANIFEST), "source_sha256": SOURCE_SHA256}
    record["allocation_check"] = {"enabled": True, "scope": "Every scenario returns normally from main."}
    record["compiler"] = {"path": str(selfhost), "sha256": _digest(Path(selfhost))}
    build_environment = dict(environment, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    directory.mkdir(parents=True)
    generated = directory / "sliding-median.c"
    emitted, out, err = runner._run([selfhost, "emit-c", APP, "-o", generated],
                                    ROOT, frontend_environment, 60)
    record["emit"] = runner._summary(emitted, out, err, not runner._succeeded(emitted))
    if not runner._succeeded(emitted) or not generated.is_file():
        record["error"] = "selfhost could not compile the original sliding-median application"
        return record
    record["generated_c"] = {"bytes": generated.stat().st_size, "sha256": _digest(generated)}
    binaries = {}
    for mode in ("debug", "release"):
        if "-DYV_ALLOC_CHECK" not in cc_flags[mode]:
            record["error"] = "sliding-median candidate build must enable allocation checking"
            return record
        candidate = directory / mode / "selfhost" / ("yavo-sliding-median" + exe)
        reference = directory / mode / "stage0" / ("yavo-sliding-median" + exe)
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
            record["error"] = "sliding-median application build failed in " + mode
            return record
        binaries[mode] = (candidate, reference)
    passed, executions = True, 0
    for case, (data, size, values) in zip(cases, parsed):
        wanted = case["stdout"].encode("ascii")
        row = {"id": case["id"], "origin": case["origin"], "window_size": size,
               "input_values": len(values), "windows": max(0, len(values) - size + 1), "modes": {}}
        for mode, (candidate, reference) in binaries.items():
            mine, out, err = runner._run([candidate], directory, build_environment, 10, data)
            theirs, rout, rerr = runner._run([reference], directory, build_environment, 10, data)
            executions += 2
            expected = runner._matches(mine, out, err, wanted) and runner._matches(theirs, rout, rerr, wanted)
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
