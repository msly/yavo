#!/usr/bin/env python3
"""Independent byte-input oracle for the frozen CSV contract; never reads Yavo code."""
from __future__ import annotations

USAGE = "Usage: yavo-csv-check [--] [FILE...]\n"
HEADER = "file\tcolumns\trows\tbytes\n"

class CsvError(Exception):
    def __init__(self, code, line, column):
        self.code, self.line, self.column = code, line, column


def validate(data):
    # Whole-input UTF-8 validation precedes every CSV or structural decision.
    data.decode("utf-8", errors="strict")
    if not data:
        raise CsvError("ECSV_EMPTY", 1, 1)
    if data.startswith(b"\xef\xbb\xbf"):
        raise CsvError("ECSV_FORMAT", 1, 1)
    index, line, column = 0, 1, 1
    size = len(data)

    def advance(count=1):
        nonlocal index, line, column
        for _ in range(count):
            if data[index] == 10:
                line, column = line + 1, 1
            else:
                column += 1
            index += 1

    def invalid():
        raise CsvError("ECSV_FORMAT", line, column)

    def control(byte):
        return (byte < 32 and byte not in (9, 10, 13)) or byte == 127

    def field():
        value = bytearray()
        if index < size and data[index] == 34:
            advance()
            while index < size:
                byte = data[index]
                if byte == 34:
                    if index + 1 < size and data[index + 1] == 34:
                        value.append(34)
                        advance(2)
                        continue
                    advance()
                    if index < size and data[index] not in (44, 10, 13):
                        invalid()
                    return bytes(value)
                if byte == 13:
                    if index + 1 >= size or data[index + 1] != 10:
                        invalid()
                    value.extend(b"\r\n")
                    advance(2)
                else:
                    if control(byte):
                        invalid()
                    value.append(byte)
                    advance()
            invalid()  # EOF position is immediately after the last byte.
        while index < size and data[index] not in (44, 10, 13):
            byte = data[index]
            if byte == 34 or control(byte):
                invalid()
            value.append(byte)
            advance()
        return bytes(value)

    def record():
        fields = []
        while True:
            fields.append(field())
            if index == size:
                return fields
            byte = data[index]
            if byte == 44:
                advance()
                continue
            if byte == 13:
                if index + 1 >= size or data[index + 1] != 10:
                    invalid()
                advance(2)
            else:
                advance()  # LF; quoted-field suffixes were already validated.
            return fields

    columns, rows = None, 0
    while index < size:
        start_line = line
        values = record()
        if columns is None:
            seen = set()
            for value in values:
                if not value or value in seen:
                    raise CsvError("ECSV_HEADER", start_line, 1)
                seen.add(value)
            columns = len(values)
        else:
            if len(values) != columns:
                raise CsvError("ECSV_WIDTH", start_line, 1)
            rows += 1
    return columns, rows, size


def result(exit_code, rows=None, diagnostics=None, help_output=False):
    return {"exit": exit_code, "rows": rows, "diagnostics": diagnostics or [], "help": help_output}


def evaluate(argv, contents, stdin=b"", stdin_error=False):
    if argv == ["--help"]:
        return result(0, help_output=True)
    paths, options = [], True
    for argument in argv:
        if options and argument == "--":
            options = False
        elif options and argument.startswith("-") and argument != "-":
            return result(2, diagnostics=[{"code": "ECLI_USAGE"}])
        else:
            paths.append(argument)
    if paths.count("-") > 1:
        return result(2, diagnostics=[{"code": "ECLI_USAGE"}])
    if not paths:
        paths = ["-"]
    rows = []
    for path in paths:
        data = stdin if path == "-" else contents.get(path)
        if data is None or path == "" or (path == "-" and stdin_error):
            return result(3, diagnostics=[{"path": path, "code": "EIO_READ"}])
        try:
            columns, count, size = validate(data)
        except UnicodeError:
            return result(3, diagnostics=[{"path": path, "code": "EIO_READ"}])
        except CsvError as error:
            return result(1, diagnostics=[{"path": path, "line": error.line,
                                         "column": error.column, "code": error.code}])
        rows.append([path, columns, count, size])
    return result(0, rows=rows)
