"""Exact P0 run/panic inventory and the original p0.rs execution protocol.

This module checks source/header expectations before any native compilation.
The frozen Rust harness identity makes changes to stdin, cwd, or fixture setup
require an explicit review rather than silently inheriting this protocol.
"""
import hashlib
import json
from pathlib import Path
import re

ORACLE = "compiler/tests/p0.rs"
ORACLE_SHA256 = "8b10a54fc6d371b295363164c7a02b852e66649451d7ae1e53fbf08012b1af3a"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def identity(path, root):
    data = path.read_bytes()
    return {"path": path.relative_to(root).as_posix(), "bytes": len(data), "sha256": sha(data)}


def expected(source, override=None):
    stdout, collecting, panic = None, False, None
    for line in source.decode("utf-8").splitlines():
        if not line.startswith("//"):
            break
        line = line[2:].removeprefix(" ")
        if line == "expect-stdout:":
            stdout, collecting = "", True
            continue
        if collecting and line.startswith("|"):
            stdout += line[1:].removeprefix(" ") + "\n"
            continue
        collecting = False
        if line.startswith("expect-error:"):
            raise ValueError("rejected source must not enter native sanitizer execution")
        if line.startswith("expect-panic:"):
            match = re.fullmatch(r"(\S+)(?:\s+(\d+):(\d+))?", line.split(":", 1)[1].strip())
            if not match:
                raise ValueError("invalid frozen panic expectation")
            panic = {"category": match[1], "line": int(match[2]) if match[2] else None,
                     "column": int(match[3]) if match[3] else None}
    if stdout is None and panic is None:
        raise ValueError("P0 source lacks a frozen exact output or panic expectation")
    result = {"stdout": stdout or "", "stderr": "", "exit_code": 101 if panic else 0, "panic": panic}
    if override is not None:
        if override.get("source_sha256") != sha(source):
            raise ValueError("stale source-bound P0 expectation")
        for key in ("stdout", "stderr"):
            if key in override:
                result[key] = override[key]
    return result


def inventory(root):
    root = Path(root).resolve()
    oracle = root / ORACLE
    if sha(oracle.read_bytes()) != ORACLE_SHA256:
        raise ValueError("p0.rs protocol changed; review stdin/cwd/fixtures before qualifying this gate")
    overrides_path = root / "tools/sanitizers/expectations.json"
    overrides = json.loads(overrides_path.read_bytes())
    directory = root / "compiler/tests/p0"
    cases = []
    for prefix in ("run_", "panic_"):
        entries = sorted(directory.glob(prefix + "*.yv"))
        if not entries:
            raise ValueError("P0 inventory is empty for " + prefix)
        for path in entries:
            source = path.read_bytes()
            entry = path.relative_to(root).as_posix()
            cases.append({"entry": entry, "id": re.sub(r"[^A-Za-z0-9_.-]+", "_", entry),
                          "kind": prefix.removesuffix("_"), "source": identity(path, root),
                          "expected": expected(source, overrides.get(entry))})
    return {
        "schema": "yavo-p0-sanitizer-contract/1",
        "oracle": identity(oracle, root),
        "supplemental_expectations": identity(overrides_path, root),
        "execution": {"stdin": "devnull", "cwd": str(root / "compiler"),
                      "cwd_protocol": "Cargo package directory inherited by compiler/tests/p0.rs",
                      "fixtures": "p0.rs creates no runtime input files or stdin payload"},
        "platform": "Linux", "posix_not_applicable": [], "cases": cases,
    }


def validate_bundle(root, bundle):
    root = Path(root).resolve()
    contract = inventory(root)
    cases = {row["entry"]: row for row in contract["cases"]}
    programs = bundle.get("programs", [])
    entries = [row.get("entry") for row in programs]
    if len(entries) != len(set(entries)) or set(entries) != set(cases):
        missing = sorted(set(cases) - set(entries))
        extra = sorted(set(entries) - set(cases), key=str)
        raise ValueError("P0 bundle inventory differs: missing=" + repr(missing) + " extra=" + repr(extra))
    if sha((root / "compiler/runtime/yavo_rt.h").read_bytes()) != bundle["runtime"]["sha256"]:
        raise ValueError("P0 runtime differs from the exporting checkout")
    for program in programs:
        case = cases[program["entry"]]
        source = (root / program["entry"]).read_bytes()
        if program["id"] != case["id"] or program["source_sha256"] != sha(source):
            raise ValueError("P0 source identity differs: " + program["entry"])
        if program["source"].encode("utf-8") != source or program["expected"] != case["expected"]:
            raise ValueError("P0 source or frozen oracle differs: " + program["entry"])
        if program.get("environment") or program.get("isolated_cwd"):
            raise ValueError("P0 execution must retain the original stdin/cwd/fixture protocol")
    return contract


def coverage(contract, results):
    by_id = {row["id"]: row for row in results}
    required = {row["id"] for row in contract["cases"]}
    if len(by_id) != len(results) or set(by_id) != required:
        raise ValueError("P0 result inventory is incomplete or duplicated")
    counts, rows = {}, []
    for kind in ("run", "panic"):
        cases = [row for row in contract["cases"] if row["kind"] == kind]
        outcomes = [by_id[row["id"]] for row in cases]
        counts[kind] = {"required": len(cases),
                        "executed": sum("execution" in row for row in outcomes),
                        "passed": sum(row.get("passed") is True for row in outcomes),
                        "failed": sum(row.get("passed") is not True for row in outcomes)}
        for case in cases:
            outcome = by_id[case["id"]]
            rows.append({"entry": case["entry"], "id": case["id"],
                         "status": "passed" if outcome.get("passed") is True else "failed",
                         "executed": "execution" in outcome,
                         "source_sha256": case["source"]["sha256"]})
    return {"status": "passed" if all(row.get("passed") is True for row in results) else "failed",
            "counts": counts, "not_applicable": [], "entries": rows}
