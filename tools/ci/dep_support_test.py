#!/usr/bin/env python3
"""Check application configuration isolation and the differing TSV/path layout."""
from pathlib import Path
import sys
import tempfile

sys.dont_write_bytecode = True
import dep_support


def main():
    checks = []

    def check(name, passed):
        if not passed:
            raise AssertionError(name)
        checks.append(name)

    # Hold a live CSV runner while loading both dep runners, then load CSV again.
    csv_before = dep_support.load_module("csv_before_dep", dep_support.CI / "csv_check.py")
    csv_boot = dep_support.load_module("csv_boot_before_dep", dep_support.ROOT / "selfhost/csv_application.py")
    csv_wanted = {"exit": 0, "rows": [["a\tfile", 2, 3, 4]], "diagnostics": [], "help": False}
    csv_bytes = b"file\tcolumns\trows\tbytes\na\\tfile\t2\t3\t4\n"
    check("csv_before", csv_before.support.expected_stdout(csv_wanted) == csv_bytes)

    native = dep_support.create_native()
    bootstrap = dep_support.create_bootstrap()
    support = native.support
    check("private_support_instances", support is not bootstrap._support())
    check("native_metadata", native.SCHEMA == "yavo-dep-index-acceptance/1"
          and native.DEFAULT_OUTPUT.name == "dep-index" and native.APP == dep_support.APP
          and set(native.MODULES) == {"main.yv", "parser.yv", "graph.yv", "report.yv", "model.yv"})
    check("native_function_globals", native.native_build.__globals__["support"] is support)
    check("sanitizer_function_globals", native.bind.__globals__["support"] is support
          and native.sanitizer.stage2_from_bootstrap.__globals__["support"] is support)
    check("bootstrap_function_globals", bootstrap.check.__globals__["_support"]().APP == dep_support.APP
          and bootstrap.APP_SLUG == "dep-index" and bootstrap.MIN_MODULES == 5)
    check("oracle_function_globals", support.frozen_oracle.__globals__["load_case"] is support.load_case)
    check("execution_function_globals", support.run_case.__globals__["expected_stdout"] is support.expected_stdout)

    wanted = {"exit": 0, "rows": [["core.a", "dir\\file\tname", 12]], "diagnostics": [], "help": False}
    check("dep_tsv_layout", support.expected_stdout(wanted)
          == b"module\tfile\tdependencies\ncore.a\tdir\\\\file\\tname\t12\n")
    check("dep_help", support.expected_stdout({"help": True}) == b"Usage: yavo-dep-index [--] FILE...\n")
    check("failure_stdout_atomic", support.expected_stdout({"help": False, "rows": None}) == b"")
    bad = {"diagnostics": [{"code": "EDEP_MISSING", "path": "a\tfile", "line": 7, "column": 1}]}
    check("dep_diagnostic", support.stderr_matches(b"a\\tfile:7:1: EDEP_MISSING: absent module\n", bad))
    check("reject_wrong_location", not support.stderr_matches(b"a\\tfile:8:1: EDEP_MISSING: absent module\n", bad))

    case = {"args": ["@abs:unit.dep"], "files": [{"path": "@abs:unit.dep", "text": "module core\n"}],
            "stdin": {"text": ""},
            "expected": {"exit": 0, "rows": [["core", "@abs:unit.dep", 0]], "diagnostics": [], "help": False}}
    with tempfile.TemporaryDirectory(prefix="dep-configuration-") as directory:
        absolute = str((Path(directory) / "unit.dep").resolve())
        args, contents, _stdin, actual = support.load_case(case, directory)
        check("absolute_path_column", args == [absolute] and contents[absolute] == b"module core\n"
              and actual["rows"] == [["core", absolute, 0]])
        check("fixture_not_mutated", case["expected"]["rows"] == [["core", "@abs:unit.dep", 0]])

    csv_after = dep_support.load_module("csv_after_dep", dep_support.CI / "csv_check.py")
    for label, runner in (("existing", csv_before), ("fresh", csv_after)):
        check("csv_" + label + "_isolated", runner.support.APP.name == "csv-check"
              and runner.APP.name == "csv-check" and runner.SCHEMA == "yavo-csv-check-acceptance/1"
              and runner.DEFAULT_OUTPUT.name == "csv-check"
              and runner.support.expected_stdout(csv_wanted) == csv_bytes
              and runner.sanitizer.APP_LABEL == "CSV")
    check("csv_bootstrap_isolated", csv_boot._support().APP.name == "csv-check"
          and csv_boot.APP_SLUG == "csv-check" and csv_boot.MIN_MODULES == 4)
    print("dep-index configuration: " + str(len(checks)) + "/" + str(len(checks)) + " passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
