#!/usr/bin/env python3
"""Independent contract oracle. It consumes bytes, never application output.

This deliberately uses Python regexes/sets and arbitrary-precision integers;
it does not inspect, import or translate the Yavo implementation.
"""
from __future__ import annotations

import re

USAGE = "Usage: yavo-test-summary [--] LOG...\n"
CASE_ID = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.:-]*\Z", re.ASCII)
DIGITS = re.compile(r"[0-9]+\Z", re.ASCII)
MAX_COUNT = 9223372036854775807
NON_ASCII_WHITESPACE = frozenset(
    [0x85, 0xA0, 0x1680, *range(0x2000, 0x200B), 0x2028, 0x2029,
     0x202F, 0x205F, 0x3000]
)


class LogError(Exception):
    def __init__(self, code, line):
        self.code, self.line = code, line


def number(token, line):
    if not DIGITS.fullmatch(token):
        raise LogError("ELOG_FORMAT", line)
    significant = token.lstrip("0") or "0"
    if len(significant) > 19:
        raise LogError("ELOG_FORMAT", line)
    value = int(significant)
    if value > MAX_COUNT:
        raise LogError("ELOG_FORMAT", line)
    return value


def parse(data):
    text = data.decode("utf-8", errors="strict")
    lines = text.split("\n")
    if text.endswith("\n") or text == "":
        lines.pop()
    tests, requirements = set(), set()
    counts = [0, 0, 0, 0, 0]
    declared, summary_line = None, None
    for index, raw in enumerate(lines, 1):
        followed_by_lf = index < len(lines) or text.endswith("\n")
        line = raw[:-1] if followed_by_lf and raw.endswith("\r") else raw
        if any((ord(ch) < 32 and ch != "\t") or ord(ch) == 127
               or ord(ch) in NON_ASCII_WHITESPACE for ch in line):
            raise LogError("ELOG_FORMAT", index)
        stripped = line.strip(" \t")
        if not stripped:
            continue
        if declared is not None:
            raise LogError("ELOG_FORMAT", index)
        fields = re.split(r"[ \t]+", stripped)
        # Keywords are ordinary CASE_ID values when the entire line is a test.
        if len(fields) == 2 and fields[1] in ("PASS", "FAIL", "SKIP"):
            if not CASE_ID.fullmatch(fields[0]):
                raise LogError("ELOG_FORMAT", index)
            if fields[0] in tests:
                raise LogError("ELOG_DUPLICATE", index)
            tests.add(fields[0])
            counts[("PASS", "FAIL", "SKIP").index(fields[1])] += 1
        elif (len(fields) >= 4 and fields[0] == "REQ"
              and fields[-1] in ("ok", "fail")):
            if not CASE_ID.fullmatch(fields[1]):
                raise LogError("ELOG_FORMAT", index)
            identity = (fields[1], tuple(fields[2:-1]))
            if identity in requirements:
                raise LogError("ELOG_DUPLICATE", index)
            requirements.add(identity)
            counts[3 if fields[-1] == "ok" else 4] += 1
        elif (len(fields) == 10 and fields[0] == "verification:"
              and fields[2] == "passed," and fields[4] == "failed,"
              and fields[6] == "skipped;" and fields[7] == "requirement"
              and fields[8] == "failures:"):
            declared = [number(fields[i], index) for i in (1, 3, 5, 9)]
            summary_line = index
        else:
            raise LogError("ELOG_FORMAT", index)
    if declared is None:
        raise LogError("ELOG_SUMMARY", len(lines) + 1)
    observed = [counts[0], counts[1], counts[2], counts[4]]
    status = "mismatch" if declared != observed else (
        "failed" if counts[1] or counts[4] else "ok")
    return counts, status, summary_line


def result(exit_code, rows=None, diagnostics=None, help_output=False):
    return {"exit": exit_code, "rows": rows, "diagnostics": diagnostics or [],
            "help": help_output}


def evaluate(argv, contents):
    """Return semantic expectations; contents maps supplied paths to bytes/None."""
    if argv == ["--help"]:
        return result(0, help_output=True)
    paths, options = [], True
    for arg in argv:
        if options and arg == "--":
            options = False
        elif options and arg.startswith("-"):
            return result(2, diagnostics=[{"code": "ECLI_USAGE"}])
        else:
            paths.append(arg)
    if not paths:
        return result(2, diagnostics=[{"code": "ECLI_USAGE"}])
    rows, diagnostics, code = [], [], 0
    for path in paths:
        data = contents.get(path)
        if data is None:
            return result(3, diagnostics=diagnostics + [
                {"path": path, "code": "EIO_READ"}])
        try:
            counts, status, line = parse(data)
        except UnicodeDecodeError:
            return result(3, diagnostics=diagnostics + [
                {"path": path, "code": "EIO_READ"}])
        except LogError as exc:
            return result(2, diagnostics=diagnostics + [
                {"path": path, "line": exc.line, "code": exc.code}])
        rows.append([path, *counts, status])
        if status != "ok":
            code = 1
        if status == "mismatch":
            diagnostics.append({"path": path, "line": line,
                                "code": "ELOG_MISMATCH"})
    return result(code, rows, diagnostics)
