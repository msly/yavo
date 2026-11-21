#!/usr/bin/env python3
"""Validate an exported Yavo C bundle with independent ASan and UBSan.

No network, package installation, production compiler changes, or test retries.
All input digests are checked before compilation; positive and intentional-negative
probes gate application results. Exact program output and exits are required.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time


def sha(data):
    return hashlib.sha256(data).hexdigest()


def artifact(path):
    data = path.read_bytes()
    return {"path": str(path.resolve()), "sha256": sha(data), "bytes": len(data)}


def check_digest(data, wanted, label):
    actual = sha(data)
    if actual != wanted:
        raise ValueError(f"{label}: input digest mismatch {actual} != {wanted}")


def invoke(argv, directory, prefix, environment, timeout):
    stdout_path = directory / (prefix + ".stdout")
    stderr_path = directory / (prefix + ".stderr")
    started = time.monotonic()
    with stdout_path.open("wb") as out, stderr_path.open("wb") as err:
        process = subprocess.Popen(argv, cwd=directory, env=environment,
                                   stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                   start_new_session=True)
        timed_out = False
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait(timeout=5)
    return {"argv": argv, "cwd": str(directory.resolve()), "exit_code": code,
            "timed_out": timed_out, "timeout_seconds": timeout,
            "elapsed_seconds": time.monotonic() - started,
            "stdout": artifact(stdout_path), "stderr": artifact(stderr_path)}


def bytes_of(record, name):
    return Path(record[name]["path"]).read_bytes()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--out", type=Path, required=True, help="new artifact directory")
    parser.add_argument("--cc", default="gcc", help="one C compiler executable, not shell text")
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 16:
        parser.error("--jobs must be between 1 and 16")
    cc = shutil.which(args.cc)
    if not cc:
        parser.error("C compiler executable not found")
    cc = str(Path(cc).resolve())
    if args.out.exists():
        parser.error("--out must not exist; results are never overwritten")
    args.out.mkdir(parents=True)
    directory = args.out.resolve()
    raw_bundle = args.bundle.read_bytes()
    bundle = json.loads(raw_bundle)
    if bundle.get("schema") != "yavo-independent-sanitizer-bundle/1":
        parser.error("unsupported bundle schema")
    programs = bundle.get("programs")
    if not isinstance(programs, list) or not programs:
        parser.error("bundle has no programs")
    ids = set()
    header = bundle["runtime"]["content"].encode("utf-8")
    check_digest(header, bundle["runtime"]["sha256"], "runtime")
    (directory / "yavo_rt.h").write_bytes(header)
    for p in programs:
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", p["id"]) or p["id"] in ids:
            parser.error("invalid or duplicate program ID")
        ids.add(p["id"])
        check_digest(p["source"].encode("utf-8"), p["source_sha256"], p["id"] + " source")
        check_digest(p["c"].encode("utf-8"), p["c_sha256"], p["id"] + " C")
        expected = p["expected"]
        if not isinstance(expected.get("stdout"), str) or not isinstance(expected.get("stderr", ""), str) or expected.get("exit_code") not in (0, 101):
            parser.error("invalid exact expectation")
        if (expected["exit_code"] == 101) != bool(expected.get("panic")):
            parser.error("panic and exit expectations disagree")
    (directory / "bundle.json").write_bytes(raw_bundle)
    environment = dict(os.environ)
    for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "CFLAGS", "CPPFLAGS", "LDFLAGS", "GCC_EXEC_PREFIX",
                "COMPILER_PATH", "LIBRARY_PATH", "CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "YAVO_CC"):
        environment.pop(key, None)
    environment.update(ASAN_OPTIONS="detect_leaks=0:halt_on_error=1:abort_on_error=0",
                       UBSAN_OPTIONS="halt_on_error=1:print_stacktrace=1", LC_ALL="C")
    flags = ["-std=c17", "-D_POSIX_C_SOURCE=200809L", "-DYV_ALLOC_CHECK=1", "-g", "-O1",
             "-fno-omit-frame-pointer", "-fno-strict-aliasing", "-fno-pie", "-no-pie",
             "-fsanitize=address", "-fsanitize=undefined", "-fno-sanitize-recover=all", "-pthread"]
    report = {
        "schema": "yavo-independent-sanitizer-validation/1", "started_epoch": time.time(),
        "host": {"platform": platform.platform(), "machine": platform.machine(),
                 "uname": list(platform.uname()), "libc": list(platform.libc_ver())},
        "bundle": artifact(directory / "bundle.json"), "source_revision": bundle["source_revision"].strip(),
        "source_git_status": bundle["git_status"], "repository_inputs": bundle["repository_inputs"],
        "yavo_compiler": bundle["compiler"], "runtime": artifact(directory / "yavo_rt.h"),
        "validator": artifact(Path(__file__).resolve()), "c_compiler": artifact(Path(cc)),
        "flags": flags, "sanitizer_environment": {k: environment[k] for k in ("ASAN_OPTIONS", "UBSAN_OPTIONS", "LC_ALL")},
        "capabilities": {}, "results": [], "limits": [
            "Independent GCC/Clang validation of generated C; pinned Zig conformance statuses are unchanged.",
            "ASan and UBSan are enabled separately in one artifact; TSan is not part of this report.",
            "LeakSanitizer is disabled in this ptrace-compatible profile; normal returns use YV_ALLOC_CHECK=1.",
            "Only this Linux host is executed; this is not Windows, macOS, or cross-architecture validation."]}
    version = invoke([cc, "--version"], directory, "compiler-version", environment, 10)
    report["c_compiler"]["version"] = bytes_of(version, "stdout").decode("utf-8", "strict").splitlines()[0]
    report["c_compiler"]["version_receipt"] = version
    libraries = {}
    for name in ("libasan.so", "libubsan.so", "libc.so.6", "libgcc_s.so.1"):
        probe = invoke([cc, "-print-file-name=" + name], directory, "locate-" + name, environment, 10)
        path = Path(bytes_of(probe, "stdout").decode().strip())
        libraries[name] = artifact(path.resolve()) if path.is_file() else {"status": "not_resolved", "result": str(path)}
    report["c_compiler"]["runtime_libraries"] = libraries

    def save():
        (directory / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")

    probe_src = r'''#include <stdlib.h>
#include <limits.h>
#include <stdio.h>
__attribute__((noinline)) static int freed_read(int *p) { return *p; }
int main(int argc, char **argv) {
  if (argc > 1 && argv[1][0] == 'a') { int *p=malloc(sizeof *p); if (!p) return 3; *p=9; free(p); printf("%d\n",freed_read(p)); }
  else if (argc > 1 && argv[1][0] == 'u') { volatile int a=INT_MAX,b=1; printf("%d\n",a+b); }
  else { int *p=malloc(sizeof *p); if (!p) return 3; *p=9; printf("%d\n",*p); free(p); }
  return 0;
}
'''
    probe_path = directory / "capability.c"
    probe_path.write_text(probe_src)
    probe_binary = directory / "capability"
    built = invoke([cc, *flags, str(probe_path), "-lm", "-o", str(probe_binary)], directory, "capability-build", environment, 90)
    report["capability_build"] = built
    report["capability_source"] = artifact(probe_path)
    if built["timed_out"] or built["exit_code"] != 0:
        report["status"] = "unsupported"
        save()
        return 2
    report["capability_artifact"] = artifact(probe_binary)
    for name, arg, marker in (("clean", None, None), ("address", "a", b"AddressSanitizer: heap-use-after-free"),
                              ("undefined", "u", b"runtime error: signed integer overflow")):
        execution = invoke([str(probe_binary)] + ([arg] if arg else []), directory, "probe-" + name, environment, 15)
        out, err = bytes_of(execution, "stdout"), bytes_of(execution, "stderr")
        ok = (not execution["timed_out"] and execution["exit_code"] == 0 and out == b"9\n" and err == b"") if name == "clean" else (
            not execution["timed_out"] and execution["exit_code"] > 0 and marker in err)
        report["capabilities"][name] = {"passed": ok, "execution": execution,
                                          "diagnostic": err.decode("utf-8", "replace")}
    if not all(x["passed"] for x in report["capabilities"].values()):
        report["status"] = "capability_failed"
        save()
        return 2

    def run_one(p):
        local = directory / p["id"]
        local.mkdir()
        src_path, c_path, binary = local / "source.yv", local / "source.c", local / "program"
        src_path.write_text(p["source"], encoding="utf-8")
        c_path.write_text(p["c"], encoding="utf-8")
        row = {"id": p["id"], "entry": p["entry"], "source": artifact(src_path), "c": artifact(c_path),
               "expected": p["expected"], "passed": False}
        command = [cc, *flags, "-I", str(directory), str(c_path), "-lm", "-o", str(binary)]
        built = invoke(command, local, "build", environment, 90)
        row["build"] = built
        if built["timed_out"] or built["exit_code"] != 0:
            row["failure"] = "build"
            row["diagnostic"] = bytes_of(built, "stderr").decode("utf-8", "replace")
            return row
        row["artifact"] = artifact(binary)
        execution = invoke([str(binary)], local, "run", environment, 20)
        row["execution"] = execution
        row["ran_on"] = report["host"]
        out, err = bytes_of(execution, "stdout"), bytes_of(execution, "stderr")
        expected = p["expected"]
        panic = expected.get("panic")
        if panic:
            location = (rb":" + str(panic["line"]).encode() + rb":" + str(panic["column"]).encode()) if panic.get("line") else rb":\d+:\d+"
            pattern = rb"yavo panic \[" + re.escape(panic["category"].encode()) + rb"\] at [^\n]+" + location + rb": [^\n]*\n"
            stderr_ok = re.fullmatch(re.escape(expected.get("stderr", "").encode("utf-8")) + pattern, err) is not None
        else:
            stderr_ok = err == expected.get("stderr", "").encode("utf-8")
        stdout_ok = out == expected["stdout"].encode("utf-8")
        artifact_unchanged = artifact(binary) == row["artifact"]
        row.update(stdout_match=stdout_ok, stderr_match=stderr_ok, artifact_unchanged=artifact_unchanged)
        row["passed"] = (not execution["timed_out"] and execution["exit_code"] == expected["exit_code"]
                         and stdout_ok and stderr_ok and artifact_unchanged)
        if not row["passed"]:
            row["diagnostic"] = err.decode("utf-8", "replace")
        return row

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(run_one, p): p["id"] for p in programs}
        for future in concurrent.futures.as_completed(futures):
            try:
                row = future.result()
            except Exception as error:
                row = {"id": futures[future], "passed": False, "failure": repr(error)}
            report["results"].append(row)
            print(("PASS " if row["passed"] else "FAIL ") + row["id"], flush=True)
            save()
    report["results"].sort(key=lambda row: row["id"])
    report["passed"] = sum(row["passed"] for row in report["results"])
    report["failed"] = len(programs) - report["passed"]
    report["status"] = "passed" if report["failed"] == 0 else "failed"
    report["finished_epoch"] = time.time()
    save()
    print(f"RESULT {report['passed']} passed, {report['failed']} failed; {directory / 'report.json'}", flush=True)
    return int(bool(report["failed"]))


if __name__ == "__main__":
    sys.exit(main())
