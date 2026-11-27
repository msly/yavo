"""Independent Linux GCC ASan/UBSan for both CSV application C producers."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import sys

import csv_support as support

FLAGS = [
    "-std=c17", "-D_POSIX_C_SOURCE=200809L", "-DYV_ALLOC_CHECK=1", "-g", "-O1",
    "-fno-omit-frame-pointer", "-fno-strict-aliasing", "-fno-pie", "-no-pie",
    "-fsanitize=address", "-fsanitize=undefined", "-fno-sanitize-recover=all", "-pthread",
]
CONTROL = r"""#include <stdlib.h>
#include <limits.h>
#include <stdio.h>
__attribute__((noinline)) static int freed_read(int *p) { return *p; }
int main(int argc, char **argv) {
  if (argc > 1 && argv[1][0] == 'a') {
    int *p = malloc(sizeof *p); if (!p) return 3;
    *p = 9; free(p); printf("%d\n", freed_read(p));
  } else if (argc > 1 && argv[1][0] == 'u') {
    volatile int a = INT_MAX, b = 1; printf("%d\n", a + b);
  } else {
    int *p = malloc(sizeof *p); if (!p) return 3;
    *p = 9; printf("%d\n", *p); free(p);
  }
  return 0;
}
"""


def completed(record, code=0):
    return (record.get("exit_code") == code and not record.get("timed_out")
            and not record.get("launch_error"))


def bind(bound, path):
    path = Path(path).resolve()
    actual = support.digest(path)
    previous = bound.get(str(path))
    if previous is not None and previous != actual:
        raise ValueError("bound input changed: " + str(path))
    bound[str(path)] = actual
    return support.identity(path)


def _path_hashes(value):
    result = {}
    if isinstance(value, dict):
        for key, item in value.items():
            if isinstance(item, str) and len(item) == 64 and Path(key).is_absolute():
                result[key] = item
            elif isinstance(item, (dict, list)):
                for name, wanted in _path_hashes(item).items():
                    if name in result and result[name] != wanted:
                        raise ValueError("conflicting bootstrap input identity")
                    result[name] = wanted
    elif isinstance(value, list):
        for item in value:
            for name, wanted in _path_hashes(item).items():
                if name in result and result[name] != wanted:
                    raise ValueError("conflicting bootstrap input identity")
                result[name] = wanted
    return result


def stage2_from_bootstrap(output, bound, stage0):
    if output is None:
        raise ValueError("--bootstrap-output is required for both-producer sanitizers")
    receipt_path = Path(output).resolve()
    if receipt_path.is_dir():
        receipt_path /= "receipt.json"
    receipt = json.loads(receipt_path.read_bytes())
    if (receipt.get("schema") != "yavo-selfhost-bootstrap/1"
            or receipt.get("status") != "passed" or receipt.get("failed") != 0
            or receipt.get("checks_failed") != 0 or receipt.get("self_hosted") is not True
            or receipt.get("achieved") != "self-hosted-fixed-point"):
        raise ValueError("sanitizers require a successful complete bootstrap receipt")
    checks = {row["id"]: row for row in receipt["checks"]}
    for name in ("inputs_unchanged_during_run", "self_compilation_fixed_point",
                 "stage3_binary_reproduces_stage2"):
        if checks.get(name, {}).get("passed") is not True:
            raise ValueError("bootstrap lacks required check: " + name)
    inputs = receipt["inputs"]
    if inputs["stage0"]["sha256"] != support.digest(stage0):
        raise ValueError("bootstrap and native application stage0 identities differ")
    for key, path in (
            ("runtime_sha256", support.ROOT / "compiler/runtime/yavo_rt.h"),
            ("toolchain_lock_sha256", support.ROOT / "compiler/toolchain.lock")):
        if inputs[key] != support.digest(path):
            raise ValueError("bootstrap input no longer matches: " + str(path))
    bundle = inputs["compiler_bundle"]
    for part in bundle["parts"]:
        for path in (support.ROOT / "selfhost" / part["file"],
                     Path(bundle["directory"]) / part["file"]):
            if support.digest(path) != part["sha256"]:
                raise ValueError("bootstrap compiler source changed: " + str(path))
            bind(bound, path)
    identities = _path_hashes(inputs)
    for name, wanted in support.input_identities(include_app=True).items():
        if identities.get(name) != wanted:
            raise ValueError("bootstrap did not bind current CSV gate input: " + name)
        bind(bound, name)
    work = Path(receipt["work"]).resolve()
    if not work.is_relative_to(receipt_path.parent):
        raise ValueError("bootstrap work directory is outside its receipt directory")
    compiler = work / "stages/stage2" / ("yavo-selfhost.exe" if os.name == "nt" else "yavo-selfhost")
    if support.digest(compiler) != receipt["stages"]["stage2_sha256"]:
        raise ValueError("final stage2 binary no longer matches bootstrap")
    bind(bound, receipt_path)
    bind(bound, compiler)
    return compiler, {"receipt": support.identity(receipt_path),
                      "compiler": support.identity(compiler),
                      "bootstrap_status": receipt["status"]}


def validate(mode, stage0_c, stage0, bootstrap_output, cases, directory,
             environment, bound, absolute_root, build_timeout=600, case_timeout=15):
    report = {"status": "not_run", "passed": None, "mode": mode, "controls": [],
              "builds": [], "runs": [], "application_executions": 0,
              "scope": "Linux GCC ASan/UBSan on stage0 and actual final-stage2 generated C",
              "limits": ["LeakSanitizer is disabled; separate exit-cleanup gates check tracked leaks.",
                         "No TSan or non-Linux sanitizer claim."]}
    if mode == "off":
        return {**report, "status": "not_applicable", "reason": "disabled for this native run"}
    if sys.platform != "linux":
        if mode == "required":
            return {**report, "status": "failed", "passed": False,
                    "error": "required sanitizer gate is Linux-only"}
        return {**report, "status": "not_applicable", "reason": "host is not Linux"}
    cc = shutil.which("gcc")
    if cc is None:
        return {**report, "status": "failed" if mode == "required" else "unavailable",
                "passed": False if mode == "required" else None,
                "reason": "system GCC is unavailable"}
    directory.mkdir(parents=True, exist_ok=False)
    env = dict(environment)
    for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "CFLAGS", "CPPFLAGS", "LDFLAGS",
                "GCC_EXEC_PREFIX", "COMPILER_PATH", "LIBRARY_PATH", "CPATH",
                "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "YAVO_CC"):
        env.pop(key, None)
    overrides = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0",
                 "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1", "LC_ALL": "C"}
    env.update(overrides)
    cc = Path(cc).resolve()
    report.update(compiler=bind(bound, cc), flags=FLAGS, environment_overrides=overrides)

    def command(label, argv, timeout=build_timeout):
        return support.execute(argv, directory / "commands" / label, directory,
                               env, overrides, timeout)

    try:
        record, out, _err = command("gcc-version", [cc, "--version"], 15)
        if not completed(record):
            raise ValueError("cannot query GCC identity")
        report["compiler"]["version"] = out.decode("utf-8", "replace").splitlines()[0]
        report["compiler"]["version_command"] = record
        libraries = {}
        for library in ("libasan.so", "libubsan.so", "libc.so.6", "libgcc_s.so.1"):
            located, out, _err = command("locate-" + library, [cc, "-print-file-name=" + library], 15)
            path = Path(out.decode("utf-8", "replace").strip())
            libraries[library] = (bind(bound, path.resolve()) if completed(located) and path.is_file()
                                  else {"status": "not_resolved", "command": located})
        report["compiler"]["runtime_libraries"] = libraries
        control_source = directory / "capability.c"
        control_source.write_text(CONTROL, encoding="utf-8", newline="\n")
        control_binary = directory / "capability"
        report["control_source"] = bind(bound, control_source)
        built, _out, _err = command("capability-build", [cc, *FLAGS, control_source, "-lm", "-o", control_binary])
        report["control_build"] = built
        if not completed(built) or not control_binary.is_file():
            report.update(status="failed" if mode == "required" else "unavailable",
                          passed=False if mode == "required" else None,
                          reason="GCC could not build the independent sanitizer controls")
            support.save(directory / "receipt.json", report)
            return report
        report["control_binary"] = bind(bound, control_binary)

        for name, arg, marker in (("clean", None, None),
                                  ("address", "a", b"AddressSanitizer: heap-use-after-free"),
                                  ("undefined", "u", b"runtime error: signed integer overflow")):
            record, out, err = command("control-" + name, [control_binary] + ([arg] if arg else []), 15)
            passed = (completed(record) and out == b"9\n" and err == b"") if marker is None else (
                not record.get("timed_out") and not record.get("launch_error")
                and isinstance(record.get("exit_code"), int) and record["exit_code"] > 0
                and marker in err)
            report["controls"].append({"id": name, "passed": passed, "command": record})
        if not all(row["passed"] for row in report["controls"]):
            raise ValueError("independent sanitizer positive/negative controls failed")

        candidate, report["stage2"] = stage2_from_bootstrap(bootstrap_output, bound, Path(stage0))
        generated = directory / "selfhost.c"
        frontend_env = dict(env, PATH="")
        emitted, _out, err = support.execute(
            [candidate, "emit-c", support.APP / "main.yv", "-o", generated],
            directory / "commands/stage2-emit", support.ROOT, frontend_env,
            {**overrides, "PATH": ""}, 60)
        report["stage2_emit"] = emitted
        if not completed(emitted) or not generated.is_file():
            raise ValueError("actual final stage2 could not emit CSV application: "
                             + err.decode("utf-8", "replace")[:1000])
        bind(bound, generated)
        report["runtime"] = bind(bound, support.ROOT / "compiler/runtime/yavo_rt.h")
        signatures = {}
        for producer, original in (("stage0", Path(stage0_c)), ("selfhost", generated)):
            local = directory / producer
            local.mkdir()
            source = local / "program.c"
            source.write_bytes(original.read_bytes())
            source_id = bind(bound, source)
            if source_id["sha256"] != bind(bound, original)["sha256"]:
                raise ValueError("sanitizer C snapshot differs from producer output")
            binary = local / "yavo-csv-check"
            built, _out, _err = command(producer + "-build",
                [cc, *FLAGS, "-I", support.ROOT / "compiler/runtime", source, "-lm", "-o", binary])
            build = {"producer": producer, "command": built, "source": source_id,
                     "passed": completed(built) and binary.is_file()}
            report["builds"].append(build)
            if not build["passed"]:
                raise ValueError("sanitized application build failed: " + producer)
            build["binary"] = bind(bound, binary)
            for case in cases:
                row, out, err = support.run_case(
                    binary, case, local / "cases" / case["id"], env, overrides,
                    timeout=case_timeout, absolute_root=absolute_root / case["id"])
                row["producer"] = producer
                signature = (row["command"]["exit_code"], support.sha(out), support.sha(err))
                if producer == "stage0":
                    signatures[case["id"]] = signature
                else:
                    row["checks"]["stage0_selfhost_equal"] = signatures[case["id"]] == signature
                    row["passed"] = all(row["checks"].values())
                report["runs"].append(row)
                for item in row.get("input_files", []):
                    if "sha256" in item:
                        if support.digest(Path(item["path"])) != item["sha256"]:
                            raise ValueError("application modified sanitizer fixture")
                        bind(bound, item["path"])
        report["application_executions"] = len(report["runs"])
        expected = {case["id"] for case in cases}
        inventory = all({row["id"] for row in report["runs"] if row["producer"] == label} == expected
                        and sum(row["producer"] == label for row in report["runs"]) == len(cases)
                        for label in ("stage0", "selfhost"))
        report["complete_inventory"] = inventory
        report["passed"] = inventory and all(row["passed"] for row in report["runs"])
        report["status"] = "passed" if report["passed"] else "failed"
    except (OSError, ValueError, KeyError, TypeError) as error:
        report.update(status="failed", passed=False, error=type(error).__name__ + ": " + str(error))
    report["application_executions"] = len(report["runs"])
    support.save(directory / "receipt.json", report)
    return report
