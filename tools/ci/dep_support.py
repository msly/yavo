"""Configure private instances of the proven application gates for dep-index.

Only application inventory, frozen-oracle row layout and receipt labels differ.
CSV modules remain independently usable in the same bootstrap process.
"""
from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / "tools/ci"
APP = ROOT / "examples/dep-index"
MODULES = ("main.yv", "parser.yv", "graph.yv", "report.yv", "model.yv")


def load_module(name, path):
    """Execute a fresh module; do not replace CSV entries in sys.modules."""
    directory = str(CI)
    inserted = directory not in sys.path
    if inserted:
        sys.path.insert(0, directory)
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        if inserted:
            sys.path.remove(directory)


def create_support():
    support = load_module("dep_shared_support", CI / "csv_support.py")
    support.APP = APP
    support.TESTS = APP / "tests"
    support.HEADER = "module\tfile\tdependencies\n"
    support.USAGE = "Usage: yavo-dep-index [--] FILE...\n"

    def load_case(case, absolute_root=None):
        args = [support._spelling(value, absolute_root) for value in case["args"]]
        contents = {support._spelling(item["path"], absolute_root):
                    None if item.get("directory") else support._payload(item)
                    for item in case["files"]}
        wanted = copy.deepcopy(case["expected"])
        for row in wanted.get("rows") or []:
            row[1] = support._spelling(row[1], absolute_root)
        for event in wanted["diagnostics"]:
            if "path" in event:
                event["path"] = support._spelling(event["path"], absolute_root)
        return args, contents, support._payload(case["stdin"]), wanted

    def expected_stdout(wanted):
        if wanted.get("help"):
            return support.USAGE.encode("utf-8")
        if wanted["rows"] is None:
            return b""
        rows = [support.HEADER]
        for name, path, dependencies in wanted["rows"]:
            rows.append(name + "\t" + support.escape_path(path) + "\t"
                        + str(dependencies) + "\n")
        return "".join(rows).encode("utf-8")

    def input_identities(include_app=True):
        files = [Path(__file__).resolve(), APP / "SPEC.md",
                 support.TESTS / "cases.json", support.TESTS / "oracle.py",
                 support.TESTS / "oracle-freeze.json",
                 ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock",
                 CI / "test_summary.py", CI / "csv_support.py", CI / "csv_check.py",
                 CI / "csv_sanitize.py", ROOT / "selfhost/csv_application.py",
                 CI / "dep_check.py", CI / "dep_support_test.py",
                 ROOT / "selfhost/dep_application.py",
                 *[support.CONTROLS / name for name in
                   ("exit_check.h", "cleanup_clean.c", "cleanup_leak.c")]]
        if include_app:
            files.extend(sorted(APP.glob("*.yv")))
        return {str(path.resolve()): support.digest(path) for path in files}

    support.load_case = load_case
    support.expected_stdout = expected_stdout
    support.input_identities = input_identities
    return support


def create_sanitizer(support):
    runner = load_module("dep_sanitizer_engine", CI / "csv_sanitize.py")
    runner.support = support
    runner.APP_LABEL = "dependency-index"
    runner.BASENAME = "yavo-dep-index"
    return runner


def create_native():
    support = create_support()
    sanitizer = create_sanitizer(support)
    runner = load_module("dep_native_engine", CI / "csv_check.py")
    runner.support = support
    runner.sanitizer = sanitizer
    runner.completed, runner.bind = sanitizer.completed, sanitizer.bind
    runner.ROOT, runner.APP = ROOT, APP
    runner.MODULES = MODULES
    runner.APP_SLUG = "dep-index"
    runner.APP_LABEL = "dependency-index"
    runner.BASENAME = "yavo-dep-index" + runner.EXE
    runner.SCHEMA = "yavo-dep-index-acceptance/1"
    runner.SCOPE = "third independent SH-4 application; not SH-3 compiler replacement"
    runner.DEFAULT_OUTPUT = ROOT / "compiler/target/ci/dep-index"
    runner.EDIT_PREFIX = b"// Dependency-index acceptance: comment-only edit measurement.\n"
    runner.__doc__ = "Native reproducible acceptance of the five-module dependency index."
    return runner


def create_bootstrap():
    support = create_support()
    runner = load_module("dep_bootstrap_engine", ROOT / "selfhost/csv_application.py")
    runner._support = lambda: support
    runner.APP_SLUG = "dep-index"
    runner.APP_LABEL = "dependency-index"
    runner.MIN_MODULES = len(MODULES)
    return runner
