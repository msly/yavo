"""Frozen independent byte/snapshot oracle; never executes a Yavo compiler."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def payload(case, path):
    item = next(item for item in case["files"] if item["path"] == path)
    data = (ROOT / item["fixture"]).read_bytes()
    if len(data) != item["bytes"] or sha(data) != item["sha256"]:
        raise ValueError("fixture identity mismatch: " + case["id"] + "/" + path)
    return data


def spelling(case, directory=None):
    if case.get("input_mode") == "absolute":
        if directory is None:
            return "@ABS/" + case["input"]
        return str(Path(directory).resolve() / case["input"])
    return case.get("input_spelling", case["input"])


def display_path(case, path, directory=None):
    entry = spelling(case, directory)
    if path == case["input"]:
        return entry
    position = max(entry.rfind("/"), entry.rfind("\\"))
    relative = str(Path(path).relative_to(Path(case["input"]).parent)).replace("\\", "/")
    return entry[:position + 1] + relative


def arguments(case, directory=None):
    return [spelling(case, directory) if argument == "@input" else argument
            for argument in case["arguments"]]


def position(data, offset):
    if type(offset) is not int or not 0 <= offset <= len(data):
        raise ValueError("invalid byte offset")
    prefix = data[:offset].decode("utf-8", errors="strict")
    return prefix.count("\n") + 1, len(prefix.rsplit("\n", 1)[-1]) + 1


def evaluate(case, directory=None):
    kind = case["kind"]
    if kind in ("usage", "read_error"):
        return {"exit": 2 if kind == "usage" else 3, "document": None,
                "stderr": "usage" if kind == "usage" else "cannot read input\n"}
    path = spelling(case, directory)
    source = payload(case, case["input"])
    doc = {"schema": "yavo-diagnostics/p0", "frontend": "selfhost",
           "diagnostic_mode": "first-error", "file": path,
           "source_sha256": sha(source), "sources": [],
           "error_count": 0, "warning_count": 0, "diagnostics": []}
    for loaded in case["loaded"]:
        raw = payload(case, loaded)
        raw.decode("utf-8", errors="strict")
        doc["sources"].append({"path": display_path(case, loaded, directory),
                               "bytes": len(raw), "sha256": sha(raw)})
    if "diagnostic" in case:
        authored = case["location"]
        raw = payload(case, authored["file"])
        start, end = authored["start"], authored["end"]
        if not start <= end:
            raise ValueError("reversed range")
        if raw[start:end].decode("utf-8", errors="strict") != authored["anchor"]:
            raise ValueError("authored byte anchor changed: " + case["id"])
        line, column = position(raw, start)
        end_line, end_column = position(raw, end)
        primary = {"file": display_path(case, authored["file"], directory),
                   "start": start, "end": end, "line": line, "column": column,
                   "end_line": end_line, "end_column": end_column}
        item = {"severity": "ERROR", **case["diagnostic"],
                **{key: primary[key] for key in ["file", "line", "column", "end_line", "end_column"]},
                "primary": primary, "notes": [], "fixes": []}
        doc["diagnostics"] = [item]
        doc["error_count"] = 1
    return {"exit": doc["error_count"], "document": doc, "stderr": ""}


def legacy(case, directory=None):
    expected = evaluate(case, directory)
    if not expected["document"] or not expected["document"]["diagnostics"]:
        return case.get("legacy_text", {"exit": 0, "stdout": "", "stderr": ""})
    item = expected["document"]["diagnostics"][0]
    message = "{file}:{line}:{column}: {code}: {message}\n".format(**item)
    return {"exit": 1, "stdout": "", "stderr": message}


def no_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key: " + key)
        result[key] = value
    return result


def strict_equal(left, right):
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(strict_equal(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(strict_equal(a, b) for a, b in zip(left, right))
    return left == right


def check_output(expected, stdout, stderr, exit_code):
    failures = []
    if exit_code != expected["exit"]:
        failures.append("exit status")
    if expected["document"] is None:
        if stdout:
            failures.append("stdout must be empty")
        if expected["stderr"] == "usage":
            if b"usage:" not in stderr or not stderr.endswith(b"\n"):
                failures.append("missing usage diagnostic")
        elif stderr != expected["stderr"].encode():
            failures.append("read-error stderr bytes")
        return failures
    if stderr:
        failures.append("JSON mode stderr must be empty")
    if not stdout.endswith(b"\n") or stdout.count(b"\n") != 1 or b"\r" in stdout:
        failures.append("one compact JSON object plus LF required")
    try:
        document = json.loads(stdout.decode("utf-8", errors="strict"), object_pairs_hook=no_duplicates,
                              parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))
        if not strict_equal(document, expected["document"]):
            failures.append("JSON fields/types/values differ")
    except (UnicodeError, ValueError) as error:
        failures.append("invalid JSON: " + str(error))
    return failures


def numeric_end(raw, start):
    """Source-only lexical range reference; never inspects compiler tokens."""
    stop = start
    while stop < len(raw):
        c = raw[stop]
        if 48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122 or c == 95:
            stop += 1
        elif c == 46 and stop + 1 < len(raw) and 48 <= raw[stop + 1] <= 57:
            stop += 1
        elif c in (43, 45) and stop + 1 < len(raw) and 48 <= raw[stop + 1] <= 57 and re.fullmatch(rb"[0-9_]+(?:\.[0-9_]+)?[eE]", raw[start:stop]):
            stop += 1
        else:
            break
    return stop
