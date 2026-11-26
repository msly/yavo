"""Validate the unchanged r5 T01 atomic-transfer ledger in both build modes.

Frozen expectations retain all archived cases and cover valid contract bounds.
An independent balance-delta journal checks the oracle without compiling or
running the application; failed batches discard their whole journal.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
EXPERIMENT = Path("experiments/coldstart-2026-11-22-r5")
APP_RELATIVE = EXPERIMENT / "runs/yavo-b/tasks/T01/final/main.yv"
APP = ROOT / APP_RELATIVE
SOURCE_SHA256 = "8a8403e63b51d427f57da3c052f9ea50c5e9c4f8a039123d707ab36976a0d4d9"
MANIFEST = ROOT / "selfhost/tests/applications/atomic-ledger.json"
CONTRACTS = (ROOT / EXPERIMENT / "TASKS.md", ROOT / EXPERIMENT / "public/T01.md")
ARCHIVED_CASES = ROOT / EXPERIMENT / "judge/cases.json"
PUBLIC_CASES = ROOT / EXPERIMENT / "public/cases.json"
RUNNER = ROOT / "selfhost/conversion_application.py"


def _digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _runner():
    spec = importlib.util.spec_from_file_location("atomic_ledger_application_runner", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inputs():
    """Bind the original source, contracts, archived cases, fixed oracle and runner."""
    files = [Path(__file__).resolve(), RUNNER, MANIFEST, APP,
             *CONTRACTS, ARCHIVED_CASES, PUBLIC_CASES]
    return {str(path): _digest(path) for path in files}


def _name(text):
    if re.fullmatch(r"[A-Za-z][A-Za-z0-9_]{0,15}", text) is None:
        raise ValueError("ledger account name is outside the published contract")
    return text


def _number(text, maximum):
    if re.fullmatch(r"[0-9]+", text) is None or int(text) > maximum:
        raise ValueError("ledger numeric field is outside the published contract")
    return int(text)


def parse_input(text):
    """Validate commands and consume every declared batch row before evaluation."""
    data = text.encode("ascii")
    if (data and not data.endswith(b"\n")) or any(byte < 32 and byte not in (9, 10) or byte == 127 for byte in data):
        raise ValueError("ledger input must use printable ASCII and spaces/tabs/LF")
    lines, index, commands = text.split("\n"), 0, []
    while index < len(lines):
        fields = lines[index].split()
        index += 1
        if not fields:
            continue
        kind = fields[0]
        if kind == "account" and len(fields) == 3:
            commands.append((kind, _name(fields[1]), _number(fields[2], 1_000_000_000)))
        elif kind == "pay" and len(fields) == 4:
            commands.append((kind, _name(fields[1]), _name(fields[2]), _number(fields[3], 1_000_000_000)))
        elif kind == "balance" and len(fields) == 2:
            commands.append((kind, _name(fields[1])))
        elif kind == "batch" and len(fields) == 2:
            count = _number(fields[1], 100)
            payments = []
            for _ in range(count):
                if index >= len(lines):
                    raise ValueError("ledger batch is missing a payment row")
                payment = lines[index].split()
                index += 1
                if len(payment) != 3:
                    raise ValueError("ledger batch payment row must contain three fields")
                payments.append((_name(payment[0]), _name(payment[1]), _number(payment[2], 1_000_000_000)))
            commands.append((kind, payments))
        else:
            raise ValueError("ledger command is outside the published grammar")
        if len(commands) > 200:
            raise ValueError("ledger input exceeds the contract's 200-command bound")
    return data, commands


def _transaction(balances, payments):
    """Evaluate against a base ledger plus deltas, without cloning or mutating it."""
    changes = {}
    for row, (source, destination, amount) in enumerate(payments, 1):
        if source not in balances:
            return row, "unknown " + source, {}
        if destination not in balances:
            return row, "unknown " + destination, {}
        if balances[source] + changes.get(source, 0) < amount:
            return row, "funds " + source, {}
        if source != destination:
            changes[source] = changes.get(source, 0) - amount
            changes[destination] = changes.get(destination, 0) + amount
    if sum(changes.values()) != 0:
        raise ValueError("ledger oracle transaction failed conservation")
    return 0, "", changes


def oracle_output(commands):
    """Derive contract output independently with atomic delta-journal commits."""
    balances, issued, output = {}, 0, []
    for command in commands:
        kind = command[0]
        if kind == "account":
            _, name, amount = command
            if name in balances:
                output.append("error: duplicate " + name)
            else:
                balances[name] = amount
                issued += amount
        elif kind == "balance":
            name = command[1]
            output.append(f"{name}: {balances[name]}" if name in balances else "error: unknown " + name)
        else:
            payments = [command[1:]] if kind == "pay" else command[1]
            row, error, changes = _transaction(balances, payments)
            if error:
                output.append("error: " + error if kind == "pay" else f"batch: row {row} {error}")
            else:
                for name, delta in changes.items():
                    balances[name] += delta
                output.append("paid" if kind == "pay" else "batch: ok")
    if sum(balances.values()) != issued or any(value < 0 or value >= 1 << 63 for value in balances.values()):
        raise ValueError("ledger oracle violated balance conservation or integer bounds")
    output.extend(f"{name}={balances[name]}" for name in sorted(balances, key=lambda name: name.encode("ascii")))
    output.append(f"total: {issued}")
    return "".join(line + "\n" for line in output)


def _task_cases(path):
    tasks = json.loads(path.read_text(encoding="utf-8"))["tasks"]
    selected = [task for task in tasks if task["id"] == "T01"]
    if len(selected) != 1:
        raise ValueError("archived cases must contain exactly one T01 task")
    return selected[0]["cases"]


def _case_io(case):
    return tuple(case[key] for key in ("stdin", "stdout", "stderr", "exit_code"))


def load_scenarios():
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("schema") != "yavo-atomic-ledger-scenarios/1":
        raise ValueError("unexpected atomic-ledger scenario schema")
    if manifest.get("application") != APP_RELATIVE.as_posix() or manifest.get("source_sha256") != SOURCE_SHA256:
        raise ValueError("scenario manifest changed the original T01 source identity")
    if _digest(APP) != SOURCE_SHA256:
        raise ValueError("original atomic-ledger source changed")
    contracts = {path.relative_to(ROOT).as_posix(): _digest(path) for path in CONTRACTS}
    if manifest.get("contracts") != contracts:
        raise ValueError("published atomic-ledger contracts changed")
    expected_archive = {"path": ARCHIVED_CASES.relative_to(ROOT).as_posix(),
                        "sha256": _digest(ARCHIVED_CASES), "task": "T01", "count": 14}
    if manifest.get("original_cases") != expected_archive:
        raise ValueError("archived T01 case identity changed")
    archived = _task_cases(ARCHIVED_CASES)
    originals = {case["id"]: case for case in archived}
    if len(archived) != 14 or len(originals) != 14:
        raise ValueError("expected 14 distinct archived T01 cases")
    public = _task_cases(PUBLIC_CASES)
    if len(public) != 2 or any(case["id"] not in originals or
                               _case_io(case) != _case_io(originals[case["id"]]) for case in public):
        raise ValueError("published T01 examples disagree with archived cases")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("atomic-ledger scenarios must be a nonempty list")
    seen, retained = set(), set()
    for case in cases:
        identifier = case["id"]
        if not isinstance(identifier, str) or not re.fullmatch(r"[a-z0-9_]+", identifier) or identifier in seen:
            raise ValueError("invalid or repeated ledger scenario id")
        seen.add(identifier)
        _, commands = parse_input(case["stdin"])
        if case.get("exit_code") != 0 or case.get("stderr") != "":
            raise ValueError(identifier + ": contract requires exit 0 and empty stderr")
        if case["stdout"] != oracle_output(commands):
            raise ValueError(identifier + ": fixed output disagrees with the independent delta-journal oracle")
        if case.get("origin") == "r5_judge":
            source = case["source_case"]
            if source not in originals or source in retained or _case_io(case) != _case_io(originals[source]):
                raise ValueError(identifier + ": archived T01 input or expectation changed")
            if originals[source].get("args") != [] or originals[source].get("files") != {}:
                raise ValueError(identifier + ": unexpected archived argument or file requirements")
            retained.add(source)
        elif case.get("origin") != "contract_boundary":
            raise ValueError(identifier + ": unknown scenario origin")
    if retained != set(originals):
        raise ValueError("fixed scenarios must retain all 14 archived T01 cases")
    return cases


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    runner = _runner()
    record = {"application": str(APP), "passed": False, "builds": {}, "scenarios": []}
    try:
        cases = load_scenarios()
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        record["error"] = "fixed atomic-ledger oracle: " + str(error)
        return record
    parsed = [parse_input(case["stdin"]) for case in cases]
    record["oracle"] = {"scenarios": len(cases), "archived_cases": 14,
                        "commands": sum(len(commands) for _, commands in parsed),
                        "batch_rows": sum(len(command[1]) for _, commands in parsed for command in commands if command[0] == "batch"),
                        "manifest_sha256": _digest(MANIFEST), "source_sha256": SOURCE_SHA256}
    record["allocation_check"] = {"enabled": True, "scope": "Every scenario returns normally from main."}
    record["compiler"] = {"path": str(selfhost), "sha256": _digest(Path(selfhost))}
    build_environment = dict(environment, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    directory.mkdir(parents=True)
    generated = directory / "atomic-ledger.c"
    emitted, out, err = runner._run([selfhost, "emit-c", APP, "-o", generated],
                                    ROOT, frontend_environment, 60)
    record["emit"] = runner._summary(emitted, out, err, not runner._succeeded(emitted))
    if not runner._succeeded(emitted) or not generated.is_file():
        record["error"] = "selfhost could not compile the original atomic-ledger application"
        return record
    record["generated_c"] = {"bytes": generated.stat().st_size, "sha256": _digest(generated)}
    binaries = {}
    for mode in ("debug", "release"):
        if "-DYV_ALLOC_CHECK" not in cc_flags[mode]:
            record["error"] = "atomic-ledger candidate build must enable allocation checking"
            return record
        candidate = directory / mode / "selfhost" / ("yavo-atomic-ledger" + exe)
        reference = directory / mode / "stage0" / ("yavo-atomic-ledger" + exe)
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
            record["error"] = "atomic-ledger application build failed in " + mode
            return record
        binaries[mode] = (candidate, reference)
    passed, executions = True, 0
    for case, (data, commands) in zip(cases, parsed):
        wanted = case["stdout"].encode("ascii")
        row = {"id": case["id"], "origin": case["origin"], "commands": len(commands),
               "batch_rows": sum(len(command[1]) for command in commands if command[0] == "batch"), "modes": {}}
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
