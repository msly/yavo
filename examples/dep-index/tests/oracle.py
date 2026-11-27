#!/usr/bin/env python3
"""Independent contract oracle: byte parsing plus heap-based graph elimination.

This module reads neither the Yavo implementation nor a filesystem. Its only
inputs are argument spellings and a mapping of complete file byte payloads;
missing keys and None model a failed read. stdin is deliberately unused.
"""
from __future__ import annotations

import heapq
import re

USAGE = "Usage: yavo-dep-index [--] FILE...\n"
HEADER = "module\tfile\tdependencies\n"
_DIRECTIVE = re.compile(
    rb"(module|use)[ \t]+([A-Za-z_][A-Za-z_0-9]*(?:\.[A-Za-z_][A-Za-z_0-9]*)*)"
)


class DeclarationError(Exception):
    def __init__(self, code, line):
        super().__init__(code, line)
        self.code = code
        self.line = line


def parse(data):
    """Decode the complete input first, then visit physical LF-delimited lines."""
    data.decode("utf-8", errors="strict")
    lines = data.split(b"\n")
    module = None
    module_line = None
    uses = {}
    for index, physical in enumerate(lines):
        line = index + 1
        # A CR belongs to a CRLF only when this piece has an LF terminator.
        if index < len(lines) - 1 and physical.endswith(b"\r"):
            physical = physical[:-1]
        if ((index == 0 and physical.startswith(b"\xef\xbb\xbf"))
                or any((byte < 32 and byte != 9) or byte == 127
                       for byte in physical)):
            raise DeclarationError("EDEP_FORMAT", line)
        stripped = physical.strip(b" \t")
        if not stripped or stripped.startswith(b"#"):
            continue
        match = _DIRECTIVE.fullmatch(stripped)
        if match is None:
            raise DeclarationError("EDEP_FORMAT", line)
        word, raw_name = match.groups()
        name = raw_name.decode("ascii")
        if word == b"module":
            if module is not None:
                raise DeclarationError("EDEP_FORMAT", line)
            module, module_line = name, line
        else:
            if module is None:
                raise DeclarationError("EDEP_FORMAT", line)
            uses.setdefault(name, line)
    if module is None:
        raise DeclarationError("EDEP_MODULE", 1)
    return {"name": module, "line": module_line, "uses": uses}


def result(exit_code, rows=None, diagnostics=None, help_output=False):
    return {"exit": exit_code, "rows": rows,
            "diagnostics": diagnostics or [], "help": help_output}


def failure(exit_code, code, path=None, line=None):
    event = {"code": code}
    if path is not None:
        event["path"] = path
    if line is not None:
        event.update(line=line, column=1)
    return result(exit_code, diagnostics=[event])


def evaluate(argv, contents, stdin=b"", stdin_error=False):
    """Return the same structural result as the shared application gate interface."""
    if argv == ["--help"]:
        return result(0, help_output=True)
    operands = []
    options = True
    for argument in argv:
        if options and argument == "--":
            options = False
        elif options and argument.startswith("-"):
            return failure(2, "ECLI_USAGE")
        else:
            operands.append(argument)
    if not operands:
        return failure(2, "ECLI_USAGE")

    declarations = []
    for path in operands:
        data = contents.get(path)
        if path == "" or data is None:
            return failure(3, "EIO_READ", path)
        try:
            item = parse(data)
        except UnicodeError:
            return failure(3, "EIO_READ", path)
        except DeclarationError as error:
            return failure(1, error.code, path, error.line)
        item["path"] = path
        declarations.append(item)

    by_name = {}
    for item in declarations:
        if item["name"] in by_name:
            return failure(1, "EDEP_DUPLICATE", item["path"], item["line"])
        by_name[item["name"]] = item
    for item in declarations:
        for dependency, line in item["uses"].items():
            if dependency not in by_name:
                return failure(1, "EDEP_MISSING", item["path"], line)

    outstanding = {name: len(item["uses"]) for name, item in by_name.items()}
    dependents = {name: [] for name in by_name}
    for name, item in by_name.items():
        for dependency in item["uses"]:
            dependents[dependency].append(name)
    ready = [name for name, count in outstanding.items() if count == 0]
    heapq.heapify(ready)
    rows = []
    emitted = set()
    while ready:
        name = heapq.heappop(ready)
        emitted.add(name)
        item = by_name[name]
        rows.append([name, item["path"], len(item["uses"])])
        for dependent in dependents[name]:
            outstanding[dependent] -= 1
            if outstanding[dependent] == 0:
                heapq.heappush(ready, dependent)
    if len(emitted) != len(by_name):
        blocked = by_name[min(set(by_name) - emitted)]
        return failure(1, "EDEP_CYCLE", blocked["path"], blocked["line"])
    return result(0, rows=rows)
