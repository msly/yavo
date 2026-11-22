#!/usr/bin/env python3
"""Build and independently validate the Yavo-written subset frontend.

This is a stage-0-built frontend, not a self-hosting success claim. Python only
orchestrates commands. All subset parsing, checking and C generation happen in
the Yavo binary; Rust is used separately as a behavioral reference.
"""
from __future__ import annotations

import argparse
import base64
from collections import Counter
import hashlib
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

ROOT = Path(__file__).resolve().parents[1]
SELFHOST = ROOT / "selfhost"
COMPILER = ROOT / "compiler"
EXE = ".exe" if os.name == "nt" else ""
PANIC = re.compile(rb"yavo panic \[(P-[A-Z-]+)\]")
DIAGNOSTIC = re.compile(rb"\b([EU][0-9]{4})\b")
LOCATION = re.compile(rb":[1-9][0-9]*:[1-9][0-9]*:")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


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
        if case["kind"] not in ("run", "panic", "reject", "unsupported"):
            raise ValueError("invalid fixture kind")
        file = Path(case["file"])
        source = (SELFHOST / "tests" / file).resolve()
        if file.is_absolute() or not source.is_relative_to((SELFHOST / "tests").resolve()):
            raise ValueError("fixture path escapes selfhost/tests")
        if not source.is_file():
            raise ValueError("missing fixture source: " + str(source))
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
              "scope": "independent subset compiler; no stage2/stage3 self-compilation",
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
        }
        version_result = command("zig-version", [zig, "version"])
        check("locked_zig", succeeded(version_result) and
              version_result["stdout"].decode("utf-8").strip() == version,
              command=version_result["record"])
        if not report["checks"][-1]["passed"]:
            raise RuntimeError("Zig version does not match compiler/toolchain.lock")

        binary = work / ("yavo-selfhost" + EXE)
        build = command("build-selfhost", [stage0, "build", SELFHOST / "main.yv",
                                         "--release", "-o", binary], timeout=240)
        check("build_yavo_frontend", succeeded(build) and binary.is_file(), command=build["record"])
        if not report["checks"][-1]["passed"]:
            raise RuntimeError("stage0 could not compile selfhost/main.yv")
        report["inputs"]["subset_compiler"] = {"path": str(binary), "sha256": digest(binary)}

        unit = work / ("lexer-test" + EXE)
        unit_build = command("build-lexer-unit", [stage0, "build", SELFHOST / "lexer_test.yv",
                                                "-o", unit], timeout=240)
        unit_run = command("run-lexer-unit", [unit], frontend=True, timeout=15) if succeeded(unit_build) else None
        check("lexer_units", succeeded(unit_build) and unit_run is not None and succeeded(unit_run),
              build=unit_build["record"], run=unit_run["record"] if unit_run else None)

        for case in cases:
            name, kind = case["id"], case["kind"]
            source = SELFHOST / "tests" / case["file"]
            directory = work / "programs" / name
            directory.mkdir(parents=True)
            item = {"id": name, "kind": kind, "passed": False,
                    "file": case["file"], "source_sha256": digest(source)}
            oracle = command(name + "-reference-check", [stage0, "check", source, "--json"])
            checked = command(name + "-subset-check", [binary, "check", source], frontend=True, timeout=15)
            item.update(reference_check=oracle["record"], subset_check=checked["record"])
            if kind in ("reject", "unsupported"):
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
                item.update(observed_diagnostics=diagnostic_codes,
                            emit_rejected_new=emitted["record"], emit_rejected_existing=preserved["record"],
                            no_output_created=no_new_file, existing_output_preserved=target.read_bytes() == sentinel)
                item["passed"] = (oracle_matches and rejected(checked) and wanted_code
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
                    cc_flags = ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK"]
                    cc_flags += ["-O2", "-s"] if mode == "release" else ["-O0", "-g"]
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
        own_source = command("self-source-boundary", [binary, "check", SELFHOST / "main.yv"],
                             frontend=True, timeout=15)
        check("self_compilation_not_yet_supported", own_source["record"]["exit_code"] == 1
              and not own_source["record"]["timed_out"] and "U0001" in codes(own_source),
              command=own_source["record"])
        # A receipt must not attribute binaries to a source tree edited during the run.
        bound_paths = {
            str(stage0): report["inputs"]["stage0"]["sha256"],
            str(zig): report["inputs"]["zig"]["sha256"],
            str(COMPILER / "runtime/yavo_rt.h"): report["inputs"]["runtime_sha256"],
            str(COMPILER / "toolchain.lock"): report["inputs"]["toolchain_lock_sha256"],
            str(Path(__file__).resolve()): report["inputs"]["harness_sha256"],
            str(SELFHOST / "tests/cases.json"): report["inputs"]["manifest_sha256"],
            **{str(ROOT / name): value for name, value in report["inputs"]["yavo_sources"].items()},
            **{str(SELFHOST / "tests" / name): value for name, value in report["inputs"]["fixtures"].items()},
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
        report["achieved"] = "subset-compiler" if passed else "not_yet_validated"
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        report.update(status="failed", error=str(error))
    checkpoint()
    print("Receipt: " + str(output / "receipt.json"), flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
