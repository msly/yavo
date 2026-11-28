#!/usr/bin/env python3
"""Frozen standard-error I/O through actual bootstrap compilers and six products."""
from __future__ import annotations

import argparse
import copy
import datetime
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
import csv_support as execution
import csv_sanitize
import selfhost_digest
import selfhost_driver as producers

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "selfhost/tests/errors51"
CONTROLS = ROOT / "examples/test-summary/tests"
EXE = ".exe" if os.name == "nt" else ""
FLAGS = {
    "debug": ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O0", "-g"],
    "release": ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O2", "-s"],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bind(bound, path):
    return producers.bind(bound, Path(path))


def load_frozen(bound):
    records = {}
    amendment_path = FIXTURES / "semantic-amendment.json"
    amendment = json.loads(amendment_path.read_bytes())
    require(amendment["schema"] == "yavo-errors51-semantic-amendment/1"
            and amendment["candidate_failure_observed"] is True
            and amendment["candidate_output_used_to_derive_expectations"] is False,
            "unknown semantic-input amendment")
    replacement = amendment["replacement"]
    require(replacement["id"] == "errors51_utf8_consumed_failure"
            and replacement["old_file"] == "errors51/programs/utf8_consumed_failure.yv"
            and replacement["new_file"] == "errors51/programs/utf8_consumed_failure_decimal.yv"
            and replacement["literal_replacements"] == [{"old": "0xE2", "new": "226"}, {"old": "0x82", "new": "130"}],
            "semantic amendment is not the one frozen literal-syntax correction")
    snapshot = FIXTURES / "manifest-before-amendment.json"
    require(amendment["manifest"]["original_snapshot"] == snapshot.relative_to(ROOT).as_posix(),
            "semantic manifest snapshot path differs")
    for relative, expected in amendment["inputs"].items():
        path = (ROOT / relative).resolve()
        require(path.is_relative_to(ROOT), "amendment input escapes repository")
        current = bind(bound, path)
        require({key: current[key] for key in ("bytes", "sha256")} == expected,
                "frozen amendment input changed: " + relative)
    for name, schema in (
        ("semantic-freeze.json", "yavo-errors51-semantic-freeze/1"),
        ("io-freeze.json", "yavo-errors51-io-freeze/1"),
    ):
        path = FIXTURES / name
        freeze = json.loads(path.read_bytes())
        require(freeze["schema"] == schema and freeze["new_candidate_used"] is False,
                "invalid standard-error freeze: " + name)
        for relative, expected in freeze["inputs"].items():
            source = snapshot if name == "semantic-freeze.json" and relative == "selfhost/tests/cases.json" else (ROOT / relative).resolve()
            require(source.is_relative_to(ROOT), "frozen source escapes repository")
            current = bind(bound, source)
            require({key: current[key] for key in ("bytes", "sha256")} == expected,
                    "frozen standard-error input changed: " + relative)
        records[name] = bind(bound, path)
    old_source = ROOT / "selfhost/tests" / replacement["old_file"]
    new_source = ROOT / "selfhost/tests" / replacement["new_file"]
    old_bytes = old_source.read_bytes()
    require(old_bytes.count(b"0xE2") == old_bytes.count(b"0x82") == 1
            and old_bytes.replace(b"0xE2", b"226").replace(b"0x82", b"130") == new_source.read_bytes(),
            "amended source has changes beyond the two equivalent literals")
    require(execution.digest(old_source) == replacement["old_sha256"]
            and execution.digest(new_source) == replacement["new_sha256"], "amended source identity differs")
    effective = json.loads(snapshot.read_bytes())
    old_cases = [case for case in effective["cases"] if case["id"] == replacement["id"]]
    require(len(old_cases) == 1 and old_cases[0]["file"] == replacement["old_file"], "old amended case differs")
    require({"kind": old_cases[0]["kind"], "stdout": old_cases[0]["stdout"],
             "stderr": old_cases[0].get("stderr", ""), "exit": 0} == amendment["expected_unchanged"],
            "amendment changed a semantic expectation")
    # The amendment is immutable historical evidence. Reconstruct its original
    # effective bytes instead of pinning the evolving current corpus to batch 51.
    historical = snapshot.read_bytes().replace(
        ('"file": "' + replacement["old_file"] + '"').encode(),
        ('"file": "' + replacement["new_file"] + '"').encode())
    require({"bytes": len(historical), "sha256": execution.sha(historical)} == amendment["manifest"]["effective"],
            "historical effective amended manifest identity changed")
    current_manifest = ROOT / "selfhost/tests/cases.json"
    current = json.loads(current_manifest.read_bytes())
    current_cases = {case["id"]: case for case in current["cases"]}
    require(len(current_cases) == len(current["cases"]), "duplicate current semantic case ID")
    require(current_cases.get(replacement["id"]) == old_cases[0],
            "original hexadecimal source or its semantic expectation was not restored")
    current_identity = bind(bound, current_manifest)
    reference = json.loads((FIXTURES / "amendment-reference.json").read_bytes())
    require(reference["passed"] is True and reference["executions"] == 4
            and reference["stdout_hex"] == "310a" and reference["stderr_hex"] == "" and reference["exit"] == 0
            and [row["version"] for row in reference["versions"]] == ["original", "decimal"],
            "literal amendment lacks the frozen two-version stage0 reference")
    for version in reference["versions"]:
        require([row["mode"] for row in version["modes"]] == ["debug", "release"], "missing literal-reference mode")
        for row in version["modes"]:
            require(row["passed"] is True and producers.completed(row["build"]) and producers.completed(row["run"])
                    and row["run"]["stdout"]["sha256"] == execution.sha(b"1\n")
                    and row["run"]["stderr"]["sha256"] == execution.sha(b""), "literal-reference output differs")
    records["semantic_amendment"] = {"record": bind(bound, amendment_path), "id": replacement["id"],
                                     "original_manifest": bind(bound, snapshot),
                                     "historical_effective_manifest": amendment["manifest"]["effective"],
                                     "current_manifest": current_identity, "original_source_restored": True,
                                     "old_source": bind(bound, old_source), "new_source": bind(bound, new_source),
                                     "expectations_unchanged": True, "stage0_equivalence_executions": 4}
    semantic = json.loads((FIXTURES / "cases.json").read_bytes())
    promotions = json.loads((FIXTURES / "promotions.json").read_bytes())
    manifest = json.loads((FIXTURES / "io-cases.json").read_bytes())
    require(semantic["case_count"] == len(semantic["cases"]) == 73,
            "semantic probe count changed")
    require(len(promotions["cases"]) == 10, "original-source promotion count changed")
    # Later batches may add probes and promote previously unsupported syntax.
    # Keep this gate's frozen executable/error expectations and source paths.
    for case in semantic["cases"] + [item["after"] for item in promotions["cases"]]:
        active = current_cases.get(case["id"])
        require(active is not None and active["file"] == case["file"],
                "frozen error probe removed or source path changed: " + case["id"])
        if case["kind"] in ("run", "panic", "reject"):
            keys = ("kind", "stdout", "stderr", "diagnostic", "panic", "exit", "environment", "isolated_cwd")
            require(all(active.get(key) == case.get(key) for key in keys),
                    "frozen error expectation changed: " + case["id"])
    require(manifest["schema"] == "yavo-standard-errors51-io-cases/1"
            and manifest["case_count"] == len(manifest["cases"]) == 48,
            "I/O case inventory changed")
    require(len({case["id"] for case in manifest["cases"]}) == 48, "duplicate I/O case ID")
    expected_counts = {"linux": 48, "darwin": 48, "win32": 46}
    require(manifest["applicable"] == expected_counts, "I/O platform scope changed")
    for host, count in expected_counts.items():
        require(sum(host in case["platforms"] for case in manifest["cases"]) == count,
                "wrong platform case inventory: " + host)
    require(manifest["contract_sha256"] == execution.digest(FIXTURES / "CONTRACT.md"),
            "I/O contract identity mismatch")
    spec = importlib.util.spec_from_file_location("errors51_independent_oracle", FIXTURES / "oracle.py")
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    for case in manifest["cases"]:
        require(oracle.evaluate(case) == case["expected"],
                "independent oracle disagrees with frozen expectation: " + case["id"])
    applicable = [case for case in manifest["cases"] if sys.platform in case["platforms"]]
    require(len(applicable) == expected_counts.get(sys.platform), "unsupported host platform")
    records.update(semantic_cases=73, promoted_original_cases=10, io_cases=48,
                   applicable_cases=len(applicable),
                   excluded_cases=[case["id"] for case in manifest["cases"] if case not in applicable],
                   contract=bind(bound, FIXTURES / "CONTRACT.md"),
                   oracle=bind(bound, FIXTURES / "oracle.py"),
                   cases=bind(bound, FIXTURES / "io-cases.json"))
    return applicable, oracle, records


def original_literal_accepted(role, source, command, out, err):
    if not producers.completed(command) or err != b"":
        return False
    if role != "stage0":
        return out == b""
    # The restored source deliberately keeps an unused allocation guard.
    # A reference warning does not turn an accepted program into a rejection.
    try:
        document = json.loads(out)
    except (ValueError, UnicodeError):
        return False
    if not isinstance(document, dict):
        return False
    diagnostics = document.get("diagnostics")
    warnings = document.get("warning_count")
    return (document.get("schema") == "yavo-diagnostics/p0"
            and document.get("source_sha256") == execution.digest(source)
            and type(document.get("error_count")) is int and document["error_count"] == 0
            and type(warnings) is int and isinstance(diagnostics, list) and warnings == len(diagnostics)
            and all(isinstance(row, dict) and row.get("severity") == "WARN"
                    and isinstance(row.get("code"), str)
                    and re.fullmatch(r"W[0-9]{4}", row["code"]) for row in diagnostics))


def original_literal_checks(compilers, work, environment):
    source = FIXTURES / "programs/utf8_consumed_failure.yv"
    rows = []
    for role, binary in compilers:
        argv = [binary, "check", source]
        if role == "stage0":
            argv.append("--json")
        command, out, err = execution.execute(argv,
                                              work / "commands" / (role + "-original-hex-supported"),
                                              ROOT, environment, {"PATH": ""}, 30)
        passed = original_literal_accepted(role, source, command, out, err)
        rows.append({"producer": role, "source": execution.identity(source), "command": command,
                     "expected_exit": 0, "expected_error_count": 0,
                     "reference_warnings_allowed": role == "stage0", "passed": passed})
    require(all(row["passed"] for row in rows), "the restored hexadecimal source was not accepted")
    return rows


def oracle_controls(oracle, cases):
    base = next(case for case in cases if case["id"] == "write_existing")
    wanted = base["expected"]
    output = bytes.fromhex(wanted["stdout_hex"])
    after = wanted["files_after"]
    positive = all(oracle.compare(wanted, 0, output, b"", after).values())
    rows = [{"id": "positive", "passed": positive}]
    for name, code, stdout, stderr, tree in [
        ("wrong_exit", 3, output, b"", after),
        ("extra_stdout", 0, output + b"extra\n", b"", after),
        ("unexpected_stderr", 0, output, b"diagnostic\n", after),
        ("changed_preserved_file", 0, output, b"", {"existing": b"overwritten".hex()}),
    ]:
        rows.append({"id": name, "passed": not all(oracle.compare(wanted, code, stdout, stderr, tree).values())})
    created = next(case for case in cases if case["id"] == "write_new")["expected"]
    rows.append({"id": "missing_created_file", "passed": not all(oracle.compare(
        created, 0, bytes.fromhex(created["stdout_hex"]), b"", {}).values())})
    require(len(rows) == 6 and all(row["passed"] for row in rows), "oracle controls failed")
    return rows


def materialize(case, directory):
    directory.mkdir(parents=True, exist_ok=False)
    snapshots, inputs = directory / "before", directory / "inputs"
    snapshots.mkdir()
    records = []
    for spelling, data in sorted(case["files"].items(), key=lambda item: (item[0].count("/"), item[0])):
        relative = Path(spelling)
        require(not relative.is_absolute() and ".." not in relative.parts, "fixture path escapes case")
        path = snapshots / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if data is None:
            path.mkdir(exist_ok=True)
            records.append({"relative": spelling, "kind": "directory", "path": str(path)})
        else:
            path.write_bytes(bytes.fromhex(data))
            records.append({"relative": spelling, "kind": "file", **execution.identity(path)})
    shutil.copytree(snapshots, inputs)
    return inputs, records


def tree_snapshot(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        name = path.relative_to(directory).as_posix()
        if path.is_symlink():
            result[name] = {"unexpected_symlink": os.readlink(path)}
        else:
            result[name] = None if path.is_dir() else path.read_bytes().hex()
    return result


def run_case(binary, case, directory, environment, overrides, oracle, timeout):
    inputs, files = materialize(case, directory)
    actual_environment = dict(environment)
    for name, value in case["environment"].items():
        if value is None:
            actual_environment.pop(name, None)
        else:
            raw = bytes.fromhex(value["hex"])
            actual_environment[name] = os.fsdecode(raw) if os.name == "posix" else raw.decode("utf-8")
    command, stdout, stderr = execution.execute(
        [binary, case["mode"], *case["args"]], directory / "command", inputs,
        actual_environment, overrides, timeout, stdin=bytes.fromhex(case["stdin_hex"]),
        stdin_error=case.get("stdin_error", False))
    after = tree_snapshot(inputs)
    checks = oracle.compare(case["expected"], command["exit_code"], stdout, stderr, after)
    checks["completed"] = not command["timed_out"] and not command["launch_error"]
    return {"id": case["id"], "passed": all(checks.values()), "checks": checks,
            "command": command, "input_files": files, "files_after": after,
            "environment": case["environment"], "expected": case["expected"]}


def cleanup_controls(zig, work, environment, bound, timeout):
    rows = []
    for mode, flags in FLAGS.items():
        for name, expected_exit in (("clean", 2), ("leak", 102)):
            source = CONTROLS / ("cleanup_" + name + ".c")
            binary = work / ("cleanup-" + name + "-" + mode + EXE)
            command, _out, err = execution.execute(
                [zig, "cc", *flags, "-I", ROOT / "compiler/runtime", "-include",
                 CONTROLS / "exit_check.h", source, "-o", binary],
                work / "commands" / ("cleanup-build-" + name + "-" + mode), ROOT,
                environment, {}, timeout)
            require(producers.completed(command) and binary.is_file(), "cleanup control did not compile: " + err.decode(errors="replace"))
            ran, out, err = execution.execute([binary], work / "commands" / ("cleanup-run-" + name + "-" + mode),
                                               work, environment, {}, 15)
            passed = producers.completed(ran, expected_exit) and out == b"" and (
                err == b"" if name == "clean" else b"heap block(s) still alive at exit" in err)
            rows.append({"id": name + "-" + mode, "passed": passed, "expected_exit": expected_exit,
                         "source": bind(bound, source), "binary": bind(bound, binary),
                         "build": command, "run": ran})
    require(all(row["passed"] for row in rows), "allocation-cleanup positive/negative controls failed")
    return rows


def sanitizer_gate(mode, formal, emissions, cases, oracle, work, environment, bound, timeout, parent_report):
    result = {"status": "not_applicable", "mode": mode, "controls": [], "builds": [], "runs": [],
              "executions": 0, "scope": "Independent Linux GCC ASan/UBSan on original stage0 and actual final-stage2 C",
              "limits": ["LeakSanitizer disabled; allocation counters and the checked-exit header check tracked leaks.",
                         "No TSan or non-Linux sanitizer claim."]}
    parent_report["sanitizers"] = result
    if mode == "off":
        result["reason"] = "disabled for this native acceptance run"
        return result
    result["status"] = "running"
    require(formal and sys.platform == "linux", "required sanitizer gate needs a Linux complete-bootstrap producer")
    compiler_name = shutil.which("gcc")
    require(compiler_name is not None, "required independent GCC is unavailable")
    compiler = Path(compiler_name).resolve()
    result["compiler"] = bind(bound, compiler)
    directory = work / "sanitizers"
    directory.mkdir()
    env = dict(environment)
    for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "CFLAGS", "CPPFLAGS", "LDFLAGS", "GCC_EXEC_PREFIX",
                "COMPILER_PATH", "LIBRARY_PATH", "CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "YAVO_CC"):
        env.pop(key, None)
    overrides = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0",
                 "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1", "LC_ALL": "C"}
    env.update(overrides)
    result.update(flags=csv_sanitize.FLAGS, environment_overrides=overrides)
    version, out, _err = execution.execute([compiler, "--version"], directory / "commands/version", directory, env, overrides, 15)
    require(producers.completed(version), "cannot query GCC identity")
    result["compiler"].update(version=out.decode(errors="replace").splitlines()[0], command=version)
    libraries = {}
    for name in ("libasan.so", "libubsan.so", "libc.so.6", "libgcc_s.so.1"):
        command, out, _err = execution.execute([compiler, "-print-file-name=" + name],
                                             directory / "commands" / ("locate-" + name), directory, env, overrides, 15)
        path = Path(out.decode(errors="replace").strip())
        libraries[name] = bind(bound, path.resolve()) if producers.completed(command) and path.is_file() else {
            "status": "not_resolved", "command": command}
    result["libraries"] = libraries
    control_source = directory / "control.c"
    control_source.write_text(csv_sanitize.CONTROL, encoding="utf-8")
    control = directory / "control"
    command, _out, err = execution.execute([compiler, *csv_sanitize.FLAGS, control_source, "-o", control],
                                         directory / "commands/control-build", directory, env, overrides, timeout)
    require(producers.completed(command) and control.is_file(), "GCC sanitizer capability build failed: " + err.decode(errors="replace"))
    result.update(control_source=bind(bound, control_source), control_binary=bind(bound, control), control_build=command)
    for name, arguments, marker in (("clean", [], None), ("address", ["a"], b"AddressSanitizer: heap-use-after-free"),
                                    ("undefined", ["u"], b"runtime error: signed integer overflow")):
        command, out, err = execution.execute([control, *arguments], directory / "commands" / ("control-" + name),
                                             directory, env, overrides, 15)
        passed = producers.completed(command) and out == b"9\n" and err == b"" if marker is None else (
            not command["timed_out"] and not command["launch_error"] and isinstance(command["exit_code"], int)
            and command["exit_code"] > 0 and marker in err)
        result["controls"].append({"id": name, "passed": passed, "command": command})
    require(all(row["passed"] for row in result["controls"]), "GCC sanitizer capability controls failed")
    for role in ("stage0", "stage2"):
        original = next(row for row in emissions if row["producer"] == role)["source"]
        source = Path(original["path"])
        require(bind(bound, source) == original, "native C changed before sanitizer compile")
        binary = directory / ("errors-" + role)
        command, _out, err = execution.execute(
            [compiler, *csv_sanitize.FLAGS, "-I", ROOT / "compiler/runtime", "-include",
             CONTROLS / "exit_check.h", source, "-lm", "-o", binary],
            directory / "commands" / (role + "-build"), directory, env, overrides, timeout)
        require(producers.completed(command) and binary.is_file(), "sanitized product failed to compile: " + err.decode(errors="replace"))
        result["builds"].append({"producer": role, "source": original, "binary": bind(bound, binary),
                                 "command": command, "passed": True})
        for case in cases:
            row = run_case(binary, case, directory / role / case["id"], env, overrides, oracle, 30)
            row.update(producer=role, mode="asan-ubsan")
            result["runs"].append(row)
    result.update(executions=len(result["runs"]), passed=sum(row["passed"] for row in result["runs"]),
                  failed=sum(not row["passed"] for row in result["runs"]))
    require(result["executions"] == 96 and result["failed"] == 0, "sanitized standard-error scenarios failed")
    result["status"] = "passed"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, default=ROOT / "compiler/target/ci/selfhost")
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/selfhost-errors")
    parser.add_argument("--sanitizers", choices=("off", "required"), default="off")
    choice = parser.add_mutually_exclusive_group()
    choice.add_argument("--candidate", type=Path)
    choice.add_argument("--reference-only", action="store_true")
    parser.add_argument("--stage0", type=Path)
    parser.add_argument("--zig", type=Path)
    parser.add_argument("--build-timeout", type=int, default=600)
    args = parser.parse_args()
    formal = args.candidate is None and not args.reference_only
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    bound = {}
    report = {"schema": "yavo-selfhost-errors/1", "status": "running", "formal": formal,
              "required": formal, "work": str(work), "checks": [], "emissions": [], "builds": [], "runs": [],
              "host": {"platform": sys.platform, "machine": platform.machine()},
              "scope": "Frozen standard-error semantic inputs and independent real-I/O oracle; actual complete-bootstrap producers are required for formal status",
              "started_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    caches = [work / "zig-local", work / "zig-global"]
    environment = dict(os.environ, ZIG_LOCAL_CACHE_DIR=str(caches[0]), ZIG_GLOBAL_CACHE_DIR=str(caches[1]), YAVO_NO_CACHE="1")
    try:
        for path in (Path(__file__), Path(execution.__file__), Path(producers.__file__), Path(selfhost_digest.__file__),
                     Path(csv_sanitize.__file__), Path(sys.executable), ROOT / "compiler/runtime/yavo_rt.h",
                     ROOT / "compiler/toolchain.lock", CONTROLS / "exit_check.h"):
            bind(bound, path)
        cases, oracle, report["frozen"] = load_frozen(bound)
        report["checks"].append({"id": "frozen_contracts_and_independent_expectations", "passed": True})
        report["oracle_controls"] = oracle_controls(oracle, cases)
        report["checks"].append({"id": "oracle_positive_and_negative_controls", "passed": True})
        if formal:
            compilers, zig, report["bootstrap"] = selfhost_digest.complete_producer(args.bootstrap_output, bound)
            require(args.stage0 is None and args.zig is None, "formal producer/toolchain overrides are forbidden")
        else:
            require(args.stage0 is not None, "preliminary runs require --stage0")
            stage0 = args.stage0.resolve()
            zig = args.zig.resolve() if args.zig else execution.locked_zig()[0]
            compilers = [("stage0", stage0)]
            if args.candidate is not None:
                compilers.append(("candidate", args.candidate.resolve()))
            report["bootstrap"] = {"status": "not_used", "scope": "reference-only" if args.reference_only else "developer candidate; not an actual stage2 claim"}
            if args.candidate is not None:
                for name in selfhost_digest.COMPILER_PARTS:
                    bind(bound, ROOT / "selfhost" / name)
        report["checks"].append({"id": "producer_identity", "passed": True})
        report["compilers"] = [{"role": role, **bind(bound, binary)} for role, binary in compilers]
        report["original_literal_checks"] = original_literal_checks(compilers, work, environment)
        report["checks"].append({"id": "original_hexadecimal_source_supported", "passed": True})
        report["zig"] = bind(bound, zig)
        command, out, _err = execution.execute([zig, "version"], work / "commands/zig-version", ROOT, environment, {}, 15)
        lock = dict(tuple(part.strip() for part in line.split("=", 1)) for line in (ROOT / "compiler/toolchain.lock").read_text().splitlines()
                    if "=" in line and not line.lstrip().startswith("#"))
        expected_version = lock["zig_version"].strip()
        require(producers.completed(command) and out.decode().strip() == expected_version, "locked Zig version differs")
        report["zig"].update(version=expected_version, command=command)
        report["cleanup_controls"] = cleanup_controls(zig, work, environment, bound, args.build_timeout)
        report["checks"].append({"id": "allocation_exit_controls", "passed": True})
        source = FIXTURES / "driver.yv"
        for role, compiler in compilers:
            directory = work / role
            directory.mkdir()
            generated = directory / "program.c"
            argv = [compiler, "check", source, "--emit-c", generated] if role == "stage0" else [compiler, "emit-c", source, "-o", generated]
            command, _out, err = execution.execute(argv, work / "commands" / (role + "-emit"), ROOT,
                                                  environment, {"PATH": ""}, 60)
            require(producers.completed(command) and generated.is_file(), "error driver emission failed: " + err.decode(errors="replace"))
            generated_identity = bind(bound, generated)
            report["emissions"].append({"producer": role, "source": generated_identity, "command": command})
            for mode, flags in FLAGS.items():
                binary = directory / ("errors-" + mode + EXE)
                command, _out, err = execution.execute(
                    [zig, "cc", *flags, "-I", ROOT / "compiler/runtime", "-include", CONTROLS / "exit_check.h",
                     generated, "-o", binary], work / "commands" / (role + "-" + mode + "-build"), ROOT,
                    environment, {}, args.build_timeout)
                require(producers.completed(command) and binary.is_file(), "error driver product failed to compile: " + err.decode(errors="replace"))
                report["builds"].append({"producer": role, "mode": mode, "source": generated_identity,
                                        "binary": bind(bound, binary), "command": command, "flags": flags, "passed": True})
                for case in cases:
                    row = run_case(binary, case, directory / mode / case["id"], environment, {}, oracle, 30)
                    row.update(producer=role, mode=mode)
                    report["runs"].append(row)
                producers.save(output / "receipt.json", report)
        report.update(cases=48, applicable_cases=len(cases), executions=len(report["runs"]),
                      passed=sum(row["passed"] for row in report["runs"]),
                      failed=sum(not row["passed"] for row in report["runs"]))
        require(report["executions"] == len(compilers) * 2 * len(cases) and report["failed"] == 0,
                "native standard-error I/O scenarios failed")
        report["checks"].append({"id": "native_products_and_frozen_scenarios", "passed": True})
        report["sanitizers"] = sanitizer_gate(args.sanitizers, formal, report["emissions"], cases, oracle,
                                             work, environment, bound, args.build_timeout, report)
        report["checks"].append({"id": "independent_sanitizers", "passed": report["sanitizers"]["status"] in ("passed", "not_applicable")})
    except Exception as error:
        report.update(status="failed", error=str(error))
        if report.get("sanitizers", {}).get("status") == "running":
            report["sanitizers"].update(status="failed", error=str(error))
    finally:
        cleanup_errors = []
        for path in caches:
            try:
                if path.exists():
                    shutil.rmtree(path)
            except OSError as error:
                cleanup_errors.append(str(error))
        report["isolated_zig_caches_removed"] = not cleanup_errors and all(not path.exists() for path in caches)
        report["cache_cleanup_errors"] = cleanup_errors
        report["checks"].append({"id": "isolated_build_cache_cleanup", "passed": report["isolated_zig_caches_removed"]})
        changed = [path for path, expected in bound.items() if not Path(path).is_file() or execution.digest(path) != expected]
        report.update(bound_inputs=bound, changed_inputs=changed)
        report["checks"].append({"id": "inputs_unchanged_during_run", "passed": not changed})
        if changed or cleanup_errors:
            report.update(status="failed", error="bound input mutation or build-cache cleanup failure")
        if report["status"] == "running":
            report["status"] = "passed" if formal else "preliminary_passed"
        report["checks_passed"] = sum(row["passed"] for row in report["checks"])
        report["completed_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        producers.save(output / "receipt.json", report)
    print(json.dumps({"status": report["status"], "formal": formal, "executions": report.get("executions", 0),
                      "receipt": str(output / "receipt.json"), "error": report.get("error")}, ensure_ascii=False))
    return 0 if report["status"] in ("passed", "preliminary_passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
