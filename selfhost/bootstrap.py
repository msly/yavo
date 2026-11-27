#!/usr/bin/env python3
"""Build and independently validate the Yavo-written compiler and its fixed point.

stage0 (Rust) builds stage1 from the Yavo compiler's module files. stage1 compiles
the same files to stage2 C, stage2 compiles them again to stage3 C, and the two
generated C files must be byte-identical. Python only copies the listed source
files into the run directory and orchestrates commands; module loading, parsing,
checking and C generation happen in the Yavo binaries. Rust is used separately
as a behavioral reference.
"""
from __future__ import annotations

import argparse
import base64
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SELFHOST = ROOT / "selfhost"
COMPILER = ROOT / "compiler"
EXE = ".exe" if os.name == "nt" else ""
PANIC = re.compile(rb"yavo panic \[(P-[A-Z-]+)\]")
DIAGNOSTIC = re.compile(rb"\b([EU][0-9]{4})\b")
LOCATION = re.compile(rb":[1-9][0-9]*:[1-9][0-9]*:")
# The compiler is a module program whose entry is main.yv; these are the files it
# loads through `use`. The lexer unit's entry is lexer_test.yv. Both are copied
# byte for byte into one run directory and built from their entry files.
COMPILER_PARTS = ["support.yv", "lexer.yv", "model.yv", "parser.yv", "types.yv", "codegen.yv", "main.yv"]
LEXER_UNIT_PARTS = ["support.yv", "lexer.yv", "lexer_test.yv"]
COMPILER_ENTRY = "main.yv"
LEXER_UNIT_ENTRY = "lexer_test.yv"
CC_FLAGS = {"debug": ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O0", "-g"],
            "release": ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O2", "-s"]}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def application_module(name="application"):
    spec = importlib.util.spec_from_file_location("selfhost_" + name, SELFHOST / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                    encoding="utf-8")


def bytes_record(data):
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "base64": base64.b64encode(data).decode("ascii")}


def codes(result):
    return sorted(set(code.decode("ascii") for code in DIAGNOSTIC.findall(
        result["stdout"] + result["stderr"])))


def succeeded(result):
    return result["record"]["exit_code"] == 0 and not result["record"]["timed_out"]


def run(argv, directory, environment, timeout=120):
    """Bound child trees, preserve full streams, and never reuse old output."""
    directory.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    timed_out = False
    code = 127
    options = ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
               if os.name == "nt" else {"start_new_session": True})
    with (directory / "stdout").open("wb") as stdout, (directory / "stderr").open("wb") as stderr:
        try:
            process = subprocess.Popen([str(arg) for arg in argv], cwd=ROOT,
                                       env=environment, stdin=subprocess.DEVNULL,
                                       stdout=stdout, stderr=stderr, **options)
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                if os.name == "nt":
                    try:
                        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                       stdin=subprocess.DEVNULL, stdout=stderr, stderr=stderr,
                                       timeout=10, check=False)
                    except (OSError, subprocess.TimeoutExpired):
                        process.kill()
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
                code = 124
        except OSError as error:
            stderr.write(str(error).encode("utf-8"))
    out, err = (directory / "stdout").read_bytes(), (directory / "stderr").read_bytes()
    record = {"argv": [str(arg) for arg in argv], "cwd": str(ROOT),
              "exit_code": code, "timed_out": timed_out,
              "elapsed_seconds": time.monotonic() - started,
              "stdout": bytes_record(out), "stderr": bytes_record(err),
              "environment": {key: environment.get(key) for key in
                              ("PATH", "YAVO_ALLOC_CHECK", "YAVO_NO_CACHE")}}
    # PATH contains executable locations only; no inherited secrets are recorded.
    save(directory / "command.json", record)
    return {"record": record, "stdout": out, "stderr": err}


def locked_zig(explicit):
    lock = {}
    for line in (COMPILER / "toolchain.lock").read_text(encoding="utf-8").splitlines():
        if "=" in line and not line.lstrip().startswith("#"):
            key, value = line.split("=", 1)
            lock[key.strip()] = value.strip()
    version = lock["zig_version"]
    if explicit:
        return explicit.resolve(), version
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "x86_64": "x86_64",
            "arm64": "aarch64", "aarch64": "aarch64"}.get(machine)
    host = {"linux": "linux", "darwin": "macos", "win32": "windows"}.get(sys.platform)
    if arch is None or host is None:
        raise RuntimeError("unsupported host; pass --zig for the locked Zig executable")
    home = Path(os.environ["YAVO_HOME"]) if "YAVO_HOME" in os.environ else Path.home() / ".yavo"
    return home / "toolchains" / ("zig-" + arch + "-" + host + "-" + version) / ("zig" + EXE), version


def assemble(parts, directory, entry):
    """Copy module files unchanged into directory; the record's path is the entry file."""
    directory.mkdir(exist_ok=True)
    for part in parts:
        data = (SELFHOST / part).read_bytes()
        target = directory / part
        if target.exists():
            if target.read_bytes() != data:
                raise RuntimeError("copied module file differs from its source: " + part)
        else:
            target.write_bytes(data)
    target = directory / entry
    return {"path": str(target), "directory": str(directory),
            "parts": [{"file": part, "sha256": digest(SELFHOST / part)} for part in parts],
            "bytes": target.stat().st_size, "sha256": digest(target)}


def prefix_path(case):
    relative = Path(case["source_prefix"])
    source = (SELFHOST / relative).resolve()
    if relative.is_absolute() or not source.is_relative_to(SELFHOST.resolve()) or source.suffix != ".yv":
        raise ValueError("source prefix must be a Yavo source within selfhost")
    if not source.is_file():
        raise ValueError("missing source prefix: " + str(source))
    return source


def validate_cases():
    path = SELFHOST / "tests/cases.json"
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest["schema"] != "yavo-selfhost-cases/1":
        raise ValueError("unknown selfhost fixture schema")
    ids = set()
    for case in manifest["cases"]:
        if not re.fullmatch(r"[a-z][a-z0-9_]*", case["id"]) or case["id"] in ids:
            raise ValueError("invalid or duplicate fixture ID")
        ids.add(case["id"])
        if case["kind"] not in ("run", "panic", "reject", "unsupported", "shared_unsupported"):
            raise ValueError("invalid fixture kind")
        file = Path(case["file"])
        source = (SELFHOST / "tests" / file).resolve()
        if file.is_absolute() or not source.is_relative_to((SELFHOST / "tests").resolve()):
            raise ValueError("fixture path escapes selfhost/tests")
        if not source.is_file():
            raise ValueError("missing fixture source: " + str(source))
        if case["kind"] == "shared_unsupported" and case.get("diagnostic") != "U0001":
            raise ValueError("shared unsupported fixtures must require U0001 on both compilers")
        if "source_prefix" in case:
            if case["kind"] != "run" or "example" in case:
                raise ValueError("source-prefix cases must be standalone run fixtures")
            prefix_path(case)
        if "example" in case:
            example = (SELFHOST / "examples" / case["example"]).resolve()
            if not example.is_relative_to((SELFHOST / "examples").resolve()):
                raise ValueError("example path escapes selfhost/examples")
            if example.read_bytes() != source.read_bytes():
                raise ValueError("example differs from its executable fixture: " + str(example))
    return manifest["cases"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage0", type=Path, default=COMPILER / ("target/release/yavo" + EXE))
    parser.add_argument("--zig", type=Path)
    parser.add_argument("--output", type=Path, default=COMPILER / "target/ci/selfhost")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="run-", dir=output))
    stage0 = args.stage0.resolve()
    environment = dict(os.environ, YAVO_NO_CACHE="1", YAVO_ALLOC_CHECK="1")
    for key in ("YAVO_CC", "YAVO_CACHE_TRACE", "YAVO_UPDATE_BASELINE"):
        environment.pop(key, None)
    frontend_env = dict(environment, PATH="")
    report = {"schema": "yavo-selfhost-bootstrap/1", "status": "running",
              "achieved": "not_yet_validated", "self_hosted": False,
              "scope": "bootstrap-subset compiler written in Yavo; stage1 -> stage2 -> stage3 fixed point",
              "host": {"platform": platform.platform(), "machine": platform.machine()},
              "work": str(work), "checks": [], "cases": []}

    def checkpoint():
        save(work / "receipt.json", report)
        save(output / "receipt.json", report)

    def command(label, argv, frontend=False, timeout=120):
        return run(argv, work / "commands" / label,
                   frontend_env if frontend else environment, timeout)

    def check(label, passed, **details):
        report["checks"].append({"id": label, "status": "passed" if passed else "failed",
                                 "passed": bool(passed), **details})
        checkpoint()
        print(label + ": " + ("PASS" if passed else "FAIL"), flush=True)

    def skip(label, reason):
        report["checks"].append({"id": label, "status": "skipped", "passed": None, "reason": reason})
        checkpoint()
        print(label + ": SKIP (" + reason + ")", flush=True)

    checkpoint()
    try:
        cases = validate_cases()
        zig, version = locked_zig(args.zig)
        report["inputs"] = {
            "stage0": {"path": str(stage0), "sha256": digest(stage0)},
            "zig": {"path": str(zig), "sha256": digest(zig), "expected_version": version},
            "runtime_sha256": digest(COMPILER / "runtime/yavo_rt.h"),
            "toolchain_lock_sha256": digest(COMPILER / "toolchain.lock"),
            "harness_sha256": digest(Path(__file__)),
            "manifest_sha256": digest(SELFHOST / "tests/cases.json"),
            "yavo_sources": {str(path.relative_to(ROOT)): digest(path)
                             for path in sorted(SELFHOST.glob("*.yv"))},
            "fixtures": {case["file"]: digest(SELFHOST / "tests" / case["file"]) for case in cases},
            # Module fixtures load sibling files that are not entries of their own.
            "fixture_modules": {str(path.relative_to(SELFHOST / "tests")): digest(path)
                                for path in sorted((SELFHOST / "tests" / "modules").rglob("*.yv"))},
            "examples": {case["example"]: digest(SELFHOST / "examples" / case["example"])
                         for case in cases if "example" in case},
            "source_prefixes": {case["source_prefix"]: digest(prefix_path(case))
                                for case in cases if "source_prefix" in case},
            "assembled_sources": {},
            "application": application_module().inputs(),
            "unit_conversion_application": application_module("conversion_application").inputs(),
            "text_diff_application": application_module("diff_application").inputs(),
            "sliding_median_application": application_module("median_application").inputs(),
            "atomic_ledger_application": application_module("ledger_application").inputs(),
            "csv_check_application": application_module("csv_application").inputs(),
        }
        version_result = command("zig-version", [zig, "version"])
        check("locked_zig", succeeded(version_result) and
              version_result["stdout"].decode("utf-8").strip() == version,
              command=version_result["record"])
        if not report["checks"][-1]["passed"]:
            raise RuntimeError("Zig version does not match compiler/toolchain.lock")

        sources = work / "selfhost-src"
        report["inputs"]["compiler_bundle"] = assemble(COMPILER_PARTS, sources, COMPILER_ENTRY)
        report["inputs"]["lexer_unit_bundle"] = assemble(LEXER_UNIT_PARTS, sources, LEXER_UNIT_ENTRY)
        bundle = sources / COMPILER_ENTRY
        unit_source = sources / LEXER_UNIT_ENTRY
        binary = work / ("yavo-selfhost" + EXE)
        build = command("build-selfhost", [stage0, "build", bundle,
                                         "--release", "-o", binary], timeout=240)
        check("build_yavo_frontend", succeeded(build) and binary.is_file(), command=build["record"])
        if not report["checks"][-1]["passed"]:
            raise RuntimeError("stage0 could not compile the assembled selfhost compiler")
        report["inputs"]["subset_compiler"] = {"path": str(binary), "sha256": digest(binary)}

        # Self-compilation: stage1 (built by stage0) -> stage2 C -> stage2 -> stage3 C.
        # Each generation is built under the same file name in its own directory:
        # Mach-O ad-hoc signatures embed the output name, so differing names would
        # make otherwise identical builds differ.
        stages = work / "stages"
        (stages / "stage2").mkdir(parents=True)
        (stages / "stage3").mkdir()
        stage2_c, stage3_c = stages / "stage2" / "yavo-selfhost.c", stages / "stage3" / "yavo-selfhost.c"
        stage2 = stages / "stage2" / ("yavo-selfhost" + EXE)
        stage3 = stages / "stage3" / ("yavo-selfhost" + EXE)
        self_check = command("stage1-check-self", [binary, "check", bundle], frontend=True, timeout=60)
        emit2 = command("stage1-emit-stage2", [binary, "emit-c", bundle, "-o", stage2_c], frontend=True, timeout=60)
        build2 = command("build-stage2", [zig, "cc", *CC_FLAGS["release"], "-I", COMPILER / "runtime",
                                          stage2_c, "-o", stage2], timeout=600) if succeeded(emit2) else None
        emit3 = (command("stage2-emit-stage3", [stage2, "emit-c", bundle, "-o", stage3_c], frontend=True, timeout=60)
                 if build2 is not None and succeeded(build2) else None)
        build3 = (command("build-stage3", [zig, "cc", *CC_FLAGS["release"], "-I", COMPILER / "runtime",
                                           stage3_c, "-o", stage3], timeout=600)
                  if emit3 is not None and succeeded(emit3) else None)
        fixed_point = (succeeded(self_check) and succeeded(emit2) and emit3 is not None and succeeded(emit3)
                       and stage2_c.read_bytes() == stage3_c.read_bytes())
        stage_record = {
            "stage1_check": self_check["record"], "stage1_emit": emit2["record"],
            "stage2_build": build2["record"] if build2 else None,
            "stage2_emit": emit3["record"] if emit3 else None,
            "stage3_build": build3["record"] if build3 else None,
        }
        if stage2_c.is_file():
            stage_record["stage2_c"] = {"bytes": stage2_c.stat().st_size, "sha256": digest(stage2_c)}
        if stage3_c.is_file():
            stage_record["stage3_c"] = {"bytes": stage3_c.stat().st_size, "sha256": digest(stage3_c)}
        if stage2.is_file():
            stage_record["stage2_sha256"] = digest(stage2)
        if stage3.is_file():
            stage_record["stage3_sha256"] = digest(stage3)
        report["stages"] = stage_record
        check("self_compilation_fixed_point", fixed_point, stage2_c_equals_stage3_c=fixed_point)
        check("stage3_binary_reproduces_stage2", build3 is not None and succeeded(build3) and stage2.is_file()
              and stage3.is_file() and digest(stage2) == digest(stage3))
        if not fixed_point:
            stage2 = None

        unit = work / ("lexer-test" + EXE)
        unit_build = command("build-lexer-unit", [stage0, "build", unit_source,
                                                "-o", unit], timeout=240)
        unit_run = command("run-lexer-unit", [unit], frontend=True, timeout=15) if succeeded(unit_build) else None
        def lexer_unit_passed(result):
            return (result is not None and succeeded(result)
                    and result["stdout"] == b"lexer tests passed\n" and result["stderr"] == b"")

        check("lexer_units", succeeded(unit_build) and lexer_unit_passed(unit_run),
              build=unit_build["record"], run=unit_run["record"] if unit_run else None)

        # Exercise the real lexer and its owned error propagation through the
        # Yavo backend. Expected failures are caught and main returns normally,
        # so YV_ALLOC_CHECK observes partial token/text and Diagnostic cleanup.
        lexer_directory = work / "lexer-selfhost"
        lexer_directory.mkdir()
        lexer_c = lexer_directory / "program.c"
        lexer_c2 = lexer_directory / "stage2.c"
        lexer_emit = command("lexer-unit-stage1-emit", [binary, "emit-c", unit_source, "-o", lexer_c],
                             frontend=True, timeout=30)
        lexer_emit2 = (command("lexer-unit-stage2-emit", [stage2, "emit-c", unit_source, "-o", lexer_c2],
                              frontend=True, timeout=30) if stage2 is not None else None)
        lexer_equal = (succeeded(lexer_emit) and lexer_emit2 is not None and succeeded(lexer_emit2)
                       and lexer_c.is_file() and lexer_c2.is_file()
                       and lexer_c.read_bytes() == lexer_c2.read_bytes())
        lexer_record = {"source": {"path": str(unit_source), "sha256": digest(unit_source)},
                        "stage1_emit": lexer_emit["record"],
                        "stage2_emit": lexer_emit2["record"] if lexer_emit2 else None,
                        "deterministic_c": lexer_equal, "modes": []}
        for key, path in (("generated_c", lexer_c), ("stage2_c", lexer_c2)):
            if path.is_file():
                lexer_record[key] = {"path": str(path), "bytes": path.stat().st_size,
                                     "sha256": digest(path)}
        if lexer_equal:
            for mode in ("debug", "release"):
                target = lexer_directory / ("lexer-" + mode + EXE)
                built = command("lexer-unit-" + mode + "-build",
                                [zig, "cc", *CC_FLAGS[mode], "-I", COMPILER / "runtime",
                                 lexer_c, "-o", target], timeout=240)
                executed = (command("lexer-unit-" + mode + "-run", [target], frontend=True, timeout=30)
                            if succeeded(built) else None)
                row = {"mode": mode, "build": built["record"],
                       "run": executed["record"] if executed else None,
                       "passed": succeeded(built) and lexer_unit_passed(executed)}
                if target.is_file():
                    row["binary"] = {"path": str(target), "sha256": digest(target)}
                lexer_record["modes"].append(row)
        report["lexer_selfhost_unit"] = lexer_record
        check("lexer_units_selfhost", lexer_equal and len(lexer_record["modes"]) == 2
              and all(row["passed"] for row in lexer_record["modes"]))

        for case in cases:
            name, kind = case["id"], case["kind"]
            fixture = SELFHOST / "tests" / case["file"]
            source = fixture
            directory = work / "programs" / name
            directory.mkdir(parents=True)
            prefix_record = None
            if "source_prefix" in case:
                prefix = prefix_path(case)
                original = prefix.read_bytes()
                source = directory / "input.yv"
                source.write_bytes(original + fixture.read_bytes())
                preserved = source.read_bytes()[:len(original)] == original
                if not preserved:
                    raise RuntimeError("source prefix changed during assembly")
                prefix_record = {"file": case["source_prefix"], "bytes": len(original),
                                 "sha256": digest(prefix), "preserved": preserved,
                                 "fixture_sha256": digest(fixture)}
                report["inputs"]["assembled_sources"][str(source)] = digest(source)
            item = {"id": name, "kind": kind, "passed": False,
                    "file": case["file"], "source_sha256": digest(source)}
            if prefix_record is not None:
                item["source_prefix"] = prefix_record
            oracle = command(name + "-reference-check", [stage0, "check", source, "--json"])
            checked = command(name + "-subset-check", [binary, "check", source], frontend=True, timeout=15)
            item.update(reference_check=oracle["record"], subset_check=checked["record"])
            # stage2 is the compiler that compiled itself; it must agree with stage1 exactly.
            stage2_agrees = False
            if stage2 is not None:
                again = command(name + "-stage2-check", [stage2, "check", source], frontend=True, timeout=15)
                stage2_agrees = (again["record"]["exit_code"] == checked["record"]["exit_code"]
                                 and again["stdout"] == checked["stdout"] and again["stderr"] == checked["stderr"])
                item["stage2_check"] = again["record"]
            if kind in ("reject", "unsupported", "shared_unsupported"):
                diagnostic_codes = codes(checked)
                expected_code = case.get("diagnostic")
                wanted_code = expected_code in diagnostic_codes if expected_code else (
                    any(code.startswith("E") for code in diagnostic_codes) and "U0001" not in diagnostic_codes)
                target = directory / "rejected.c"
                emitted = command(name + "-reject-new", [binary, "emit-c", source, "-o", target],
                                  frontend=True, timeout=15)
                no_new_file = not target.exists()
                sentinel = b"existing output must survive a rejected input\n"
                target.write_bytes(sentinel)
                preserved = command(name + "-reject-existing", [binary, "emit-c", source, "-o", target],
                                    frontend=True, timeout=15)
                rejected = lambda result: result["record"]["exit_code"] == 1 and not result["record"]["timed_out"]
                oracle_matches = succeeded(oracle) if kind == "unsupported" else rejected(oracle)
                if kind == "shared_unsupported":
                    # Keep stage0's own unsupported boundaries distinct from legal inputs
                    # that only the subset compiler has yet to implement.
                    oracle_matches = rejected(oracle) and codes(oracle) == ["U0001"]
                    wanted_code = diagnostic_codes == ["U0001"]
                item.update(observed_diagnostics=diagnostic_codes,
                            reference_diagnostics=codes(oracle),
                            emit_rejected_new=emitted["record"], emit_rejected_existing=preserved["record"],
                            no_output_created=no_new_file, existing_output_preserved=target.read_bytes() == sentinel)
                item["passed"] = (stage2_agrees and oracle_matches and rejected(checked) and wanted_code
                                  and bool(LOCATION.search(checked["stderr"]))
                                  and checked["stdout"] == b"" and rejected(emitted) and rejected(preserved)
                                  and no_new_file and target.read_bytes() == sentinel)
            else:
                c_file, repeated = directory / "program.c", directory / "repeat.c"
                emit = command(name + "-emit", [binary, "emit-c", source, "-o", c_file],
                               frontend=True, timeout=15)
                repeat = command(name + "-emit-repeat", [binary, "emit-c", source, "-o", repeated],
                                 frontend=True, timeout=15)
                deterministic = (succeeded(emit) and succeeded(repeat) and c_file.is_file()
                                 and repeated.is_file() and c_file.read_bytes() == repeated.read_bytes())
                if stage2 is not None and deterministic:
                    stage2_file = directory / "stage2.c"
                    stage2_emit = command(name + "-stage2-emit", [stage2, "emit-c", source, "-o", stage2_file],
                                          frontend=True, timeout=15)
                    stage2_agrees = (stage2_agrees and succeeded(stage2_emit) and stage2_file.is_file()
                                     and stage2_file.read_bytes() == c_file.read_bytes())
                    item["stage2_emit"] = stage2_emit["record"]
                deterministic = deterministic and stage2_agrees
                item.update(emit=emit["record"], emit_repeat=repeat["record"],
                            deterministic_c=deterministic, modes=[])
                if c_file.exists():
                    item["generated_c_sha256"] = digest(c_file)
                for mode in ("debug", "release"):
                    if not (succeeded(oracle) and succeeded(checked) and deterministic):
                        break
                    reference = directory / ("reference-" + mode + EXE)
                    candidate = directory / ("candidate-" + mode + EXE)
                    flags = ["--release"] if mode == "release" else []
                    reference_build = command(name + "-" + mode + "-reference-build",
                                              [stage0, "build", source, *flags, "-o", reference])
                    cc_flags = CC_FLAGS[mode]
                    candidate_build = command(name + "-" + mode + "-candidate-build",
                                              [zig, "cc", *cc_flags, "-I", COMPILER / "runtime",
                                               c_file, "-o", candidate])
                    result = {"mode": mode, "passed": False,
                              "reference_build": reference_build["record"],
                              "candidate_build": candidate_build["record"]}
                    if succeeded(reference_build) and succeeded(candidate_build):
                        reference_run = command(name + "-" + mode + "-reference-run", [reference], timeout=10)
                        candidate_run = command(name + "-" + mode + "-candidate-run", [candidate], timeout=10)
                        expected_out = case["stdout"].encode("utf-8")
                        expected_exit = 101 if kind == "panic" else 0
                        observed = []
                        for executed in (reference_run, candidate_run):
                            panic = PANIC.search(executed["stderr"])
                            observation = {"exit_code": executed["record"]["exit_code"],
                                           "stdout": bytes_record(executed["stdout"]),
                                           "panic_category": panic[1].decode("ascii") if panic else None}
                            wanted_stderr = (observation["panic_category"] == case["panic"] if kind == "panic"
                                             else executed["stderr"] == case.get("stderr", "").encode("utf-8"))
                            observed.append(executed["record"]["exit_code"] == expected_exit
                                            and not executed["record"]["timed_out"]
                                            and executed["stdout"] == expected_out and wanted_stderr)
                        result.update(reference_run=reference_run["record"], candidate_run=candidate_run["record"],
                                      passed=all(observed), reference_sha256=digest(reference),
                                      candidate_sha256=digest(candidate))
                    item["modes"].append(result)
                item["passed"] = (succeeded(oracle) and succeeded(checked) and deterministic
                                  and len(item["modes"]) == 2 and all(mode["passed"] for mode in item["modes"]))
            report["cases"].append(item)
            checkpoint()
            print(name + ": " + ("PASS" if item["passed"] else "FAIL"), flush=True)

        guard_source = work / "guard-source.yv"
        original = b"fn main() { println(1) }\n"
        guard_source.write_bytes(original)
        guard = command("guard-source-output", [binary, "emit-c", guard_source, "-o", guard_source],
                        frontend=True, timeout=15)
        check("input_output_guard", guard["record"]["exit_code"] == 2 and not guard["record"]["timed_out"]
              and guard_source.read_bytes() == original, command=guard["record"])

        existing = work / "existing-output.c"
        sentinel = b"existing output must not be overwritten even for a valid input\n"
        existing.write_bytes(sentinel)
        guard = command("guard-existing-output", [binary, "emit-c", guard_source, "-o", existing],
                        frontend=True, timeout=15)
        check("existing_output_guard", guard["record"]["exit_code"] == 3 and not guard["record"]["timed_out"]
              and existing.read_bytes() == sentinel, command=guard["record"])

        # Preserve the dot segment as text: Path would normalize it away before argv construction.
        alias = str(work) + os.sep + "." + os.sep + guard_source.name
        guard = command("guard-dot-alias", [binary, "emit-c", guard_source, "-o", alias],
                        frontend=True, timeout=15)
        check("dot_alias_output_guard", guard["record"]["exit_code"] == 3 and not guard["record"]["timed_out"]
              and guard_source.read_bytes() == original, command=guard["record"])

        hardlink = work / "hardlink-output.c"
        os.link(guard_source, hardlink)
        guard = command("guard-hardlink", [binary, "emit-c", guard_source, "-o", hardlink],
                        frontend=True, timeout=15)
        check("hardlink_output_guard", guard["record"]["exit_code"] == 3 and not guard["record"]["timed_out"]
              and guard_source.read_bytes() == original and hardlink.read_bytes() == original,
              command=guard["record"])

        symlink = work / "symlink-output.c"
        try:
            os.symlink(guard_source, symlink)
        except (OSError, NotImplementedError) as error:
            # Some Windows hosts do not grant symlink privileges; this is never counted as PASS.
            skip("symlink_output_guard", "host cannot create a symbolic link: " + str(error))
        else:
            guard = command("guard-symlink", [binary, "emit-c", guard_source, "-o", symlink],
                            frontend=True, timeout=15)
            check("symlink_output_guard", guard["record"]["exit_code"] == 3
                  and not guard["record"]["timed_out"] and guard_source.read_bytes() == original
                  and symlink.is_symlink() and symlink.read_bytes() == original, command=guard["record"])
        # SH-3: the Yavo-written compiler builds the unmodified test-summary
        # application, which then matches its frozen scenarios and stage0's build.
        app_record = application_module().check(binary, stage0, zig, work / "test-summary", environment,
                                                frontend_env, CC_FLAGS, EXE)
        report["test_summary_application"] = app_record
        check("test_summary_application", app_record["passed"], applicable=app_record.get("applicable"),
              error=app_record.get("error"))

        # The original two-module T04 converter must also match fixed rational
        # arithmetic scenarios and stage0 in both optimization modes.
        conversion_record = application_module("conversion_application").check(
            stage2 if stage2 is not None else binary, stage0, zig, work / "unit-conversion",
            environment, frontend_env, CC_FLAGS, EXE)
        report["unit_conversion_application"] = conversion_record
        check("unit_conversion_application", conversion_record["passed"],
              applicable=conversion_record.get("applicable"), error=conversion_record.get("error"))

        diff_record = application_module("diff_application").check(
            stage2 if stage2 is not None else binary, stage0, zig, work / "text-diff",
            environment, frontend_env, CC_FLAGS, EXE)
        report["text_diff_application"] = diff_record
        check("text_diff_application", diff_record["passed"],
              applicable=diff_record.get("applicable"), error=diff_record.get("error"))

        median_record = application_module("median_application").check(
            stage2 if stage2 is not None else binary, stage0, zig, work / "sliding-median",
            environment, frontend_env, CC_FLAGS, EXE)
        report["sliding_median_application"] = median_record
        check("sliding_median_application", median_record["passed"],
              applicable=median_record.get("applicable"), error=median_record.get("error"))

        ledger_record = application_module("ledger_application").check(
            stage2 if stage2 is not None else binary, stage0, zig, work / "atomic-ledger",
            environment, frontend_env, CC_FLAGS, EXE)
        report["atomic_ledger_application"] = ledger_record
        check("atomic_ledger_application", ledger_record["passed"],
              applicable=ledger_record.get("applicable"), error=ledger_record.get("error"))

        # A second independent SH-4 project validates structured CSV input.
        # Use the final self-hosted compiler, with fixed cases shared with its
        # separate native delivery gate.
        csv_record = application_module("csv_application").check(
            stage2 if stage2 is not None else binary, stage0, zig, work / "csv-check",
            environment, frontend_env, CC_FLAGS, EXE)
        report["csv_check_application"] = csv_record
        check("csv_check_application", csv_record["passed"],
              applicable=csv_record.get("applicable"), error=csv_record.get("error"))

        # A receipt must not attribute binaries to a source tree edited during the run.
        bound_paths = {
            str(stage0): report["inputs"]["stage0"]["sha256"],
            str(zig): report["inputs"]["zig"]["sha256"],
            str(COMPILER / "runtime/yavo_rt.h"): report["inputs"]["runtime_sha256"],
            str(COMPILER / "toolchain.lock"): report["inputs"]["toolchain_lock_sha256"],
            str(Path(__file__).resolve()): report["inputs"]["harness_sha256"],
            str(SELFHOST / "tests/cases.json"): report["inputs"]["manifest_sha256"],
            **{str(Path(record["directory"]) / part["file"]): part["sha256"]
               for record in (report["inputs"]["compiler_bundle"], report["inputs"]["lexer_unit_bundle"])
               for part in record["parts"]},
            **{str(SELFHOST / "tests" / name): value for name, value in report["inputs"]["fixture_modules"].items()},
            **{str(ROOT / name): value for name, value in report["inputs"]["yavo_sources"].items()},
            **{str(SELFHOST / "tests" / name): value for name, value in report["inputs"]["fixtures"].items()},
            **{str(SELFHOST / "examples" / name): value for name, value in report["inputs"]["examples"].items()},
            **{str(SELFHOST / name): value for name, value in report["inputs"]["source_prefixes"].items()},
            **report["inputs"]["assembled_sources"],
            **report["inputs"]["application"],
            **report["inputs"]["unit_conversion_application"],
            **report["inputs"]["text_diff_application"],
            **report["inputs"]["sliding_median_application"],
            **report["inputs"]["atomic_ledger_application"],
            **report["inputs"]["csv_check_application"],
        }
        changed = [name for name, expected in bound_paths.items()
                   if not Path(name).is_file() or digest(Path(name)) != expected]
        current_yavo_sources = {str(path.relative_to(ROOT)) for path in SELFHOST.glob("*.yv")}
        changed.extend(sorted(current_yavo_sources.symmetric_difference(report["inputs"]["yavo_sources"])))
        check("inputs_unchanged_during_run", not changed, changed=changed)
        report["case_counts"] = dict(Counter(case["kind"] for case in report["cases"]))
        report["passed"] = sum(case["passed"] for case in report["cases"])
        report["failed"] = len(report["cases"]) - report["passed"]
        report["checks_passed"] = sum(item["passed"] is True for item in report["checks"])
        report["checks_failed"] = sum(item["passed"] is False for item in report["checks"])
        report["checks_skipped"] = sum(item["passed"] is None for item in report["checks"])
        report["total"] = len(report["cases"])
        passed = (len(report["cases"]) == len(cases) and report["failed"] == 0
                  and report["checks_failed"] == 0)
        report["status"] = "passed" if passed else "failed"
        report["achieved"] = "self-hosted-fixed-point" if passed else "not_yet_validated"
        report["self_hosted"] = bool(passed)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        report.update(status="failed", error=str(error))
    checkpoint()
    print("Receipt: " + str(output / "receipt.json"), flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
