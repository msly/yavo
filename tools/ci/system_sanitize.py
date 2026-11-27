#!/usr/bin/env python3
"""Mandatory Linux GCC ASan+UBSan for the public system runtime and both frontends."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
import csv_sanitize as sanitizer
import csv_support as support

SYSTEM_CASES = {
    "system49_env", "system49_temp", "system49_process_failure",
    "system49_evaluation_order", "system49_slice",
    "system49_failure_cleanup", "system49_invalid_paths",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, required=True)
    parser.add_argument("--output", type=Path,
                        default=support.ROOT / "compiler/target/ci/system-sanitize")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    report = {
        "schema": "yavo-system-sanitizers/1", "status": "running", "required": True,
        "work": str(directory), "controls": [], "builds": [], "runs": [],
        "scope": "Linux system runtime and actual stage0/final-stage2 system API lowering",
        "limits": ["ASan+UBSan only; no TSan or Windows/macOS sanitizer claim.",
                   "LeakSanitizer disabled; generated programs and direct probe also use YV_ALLOC_CHECK.",
                   "Native malloc buffers are covered for invalid access, not by YV_ALLOC_CHECK leak counting."],
    }
    bound = {}
    env = dict(os.environ)
    for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "CFLAGS", "CPPFLAGS", "LDFLAGS",
                "GCC_EXEC_PREFIX", "COMPILER_PATH", "LIBRARY_PATH", "CPATH",
                "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "YAVO_CC", "YAVO_BATCH49_ABSENT_75bcac8f"):
        env.pop(key, None)
    overrides = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0",
                 "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1", "LC_ALL": "C",
                 "YAVO_NO_CACHE": "1"}
    env.update(overrides)

    def bind(path):
        return sanitizer.bind(bound, path)

    def command(label, argv, timeout=120, cwd=directory):
        return support.execute(argv, directory / "commands" / label, cwd,
                               env, overrides, timeout)

    def build(label, argv, binary):
        record, out, err = command(label, argv)
        row = {"id": label, "command": record,
               "passed": sanitizer.completed(record) and binary.is_file()}
        report["builds"].append(row)
        if not row["passed"]:
            raise ValueError(label + " failed: " + err.decode("utf-8", "replace")[:2000])
        row["binary"] = bind(binary)

    def run(label, argv, stdout, stderr=b"", cwd=directory):
        record, out, err = command(label, argv, 30, cwd=cwd)
        row = {"id": label, "command": record, "passed": (
            sanitizer.completed(record) and out == stdout and err == stderr)}
        report["runs"].append(row)
        if not row["passed"]:
            raise ValueError(label + " did not match its frozen output or sanitizer requirement")

    try:
        if sys.platform != "linux":
            raise ValueError("required system sanitizer gate needs Linux")
        cc_name = shutil.which("gcc")
        if cc_name is None:
            raise ValueError("required system sanitizer gate cannot find system GCC")
        cc = Path(cc_name).resolve()
        report["compiler"] = bind(cc)
        report["flags"] = sanitizer.FLAGS
        report["environment_overrides"] = overrides
        record, out, err = command("gcc-version", [cc, "--version"])
        if not sanitizer.completed(record):
            raise ValueError("cannot establish GCC version")
        report["compiler"]["version"] = out.decode("utf-8", "replace").splitlines()[0]
        report["compiler"]["version_command"] = record
        libraries = {}
        for name in ("libasan.so", "libubsan.so", "libc.so.6", "libgcc_s.so.1"):
            located, out, err = command("locate-" + name, [cc, "-print-file-name=" + name])
            path = Path(out.decode("utf-8", "replace").strip())
            if not sanitizer.completed(located) or not path.is_file():
                raise ValueError("cannot bind required sanitizer/runtime library " + name)
            libraries[name] = bind(path)
        report["compiler"]["libraries"] = libraries
        for path in (Path(__file__), Path(sanitizer.__file__), Path(support.__file__),
                     support.ROOT / "compiler/runtime/yavo_rt.h",
                     support.ROOT / "compiler/toolchain.lock"):
            bind(path)
        control = directory / "capability.c"
        control.write_text(sanitizer.CONTROL, encoding="utf-8", newline="\n")
        bind(control)
        control_binary = directory / "capability"
        build("control-build", [cc, *sanitizer.FLAGS, control, "-lm", "-o", control_binary], control_binary)
        for name, argument, marker in (
                ("clean", None, None),
                ("address", "a", b"AddressSanitizer: heap-use-after-free"),
                ("undefined", "u", b"runtime error: signed integer overflow")):
            record, out, err = command("control-" + name, [control_binary] + ([argument] if argument else []))
            passed = (sanitizer.completed(record) and out == b"9\n" and not err) if marker is None else (
                not record.get("timed_out") and not record.get("launch_error")
                and isinstance(record.get("exit_code"), int) and record["exit_code"] > 0 and marker in err)
            report["controls"].append({"id": name, "passed": passed, "command": record})
        if not all(row["passed"] for row in report["controls"]):
            raise ValueError("sanitizer positive/negative capability controls failed")

        bootstrap_path = args.bootstrap_output.resolve()
        if bootstrap_path.is_dir():
            bootstrap_path /= "receipt.json"
        bootstrap = json.loads(bootstrap_path.read_bytes())
        stage0 = Path(bootstrap["inputs"]["stage0"]["path"]).resolve()
        report["stage0"] = bind(stage0)
        stage2, report["stage2"] = sanitizer.stage2_from_bootstrap(bootstrap_path, bound, stage0)
        probe = support.ROOT / "tools/ci/system_runtime_probe.c"
        report["probe"] = bind(probe)
        helper = directory / "native child 汉"
        build("helper-build", [cc, "-std=c17", "-O0", "-DYV_SYSTEM_HELPER", probe, "-o", helper], helper)
        binary = directory / "system-runtime"
        build("runtime-build", [cc, *sanitizer.FLAGS, "-I", support.ROOT / "compiler/runtime",
                               probe, "-lm", "-o", binary], binary)
        run("system-runtime", [binary, helper, directory],
            b"probe-stdout\nargv-ok\nsystem-runtime-ok\n",
            b"probe-stderr\nprobe-stdout\nprobe-stderr\n")
        report["runtime_process_calls"] = {
            "normal_exit": 67, "signal_exit": 1, "launch_failure": 33,
            "invalid_arguments": 1,
            "note": "Includes 32 each of exit 37/127 and launch failure; stdout/stderr routing, argv and closed stdout."}

        manifest_path = support.ROOT / "selfhost/tests/cases.json"
        bind(manifest_path)
        if bootstrap["inputs"]["manifest_sha256"] != support.digest(manifest_path):
            raise ValueError("system fixture manifest differs from validated bootstrap")
        manifest = json.loads(manifest_path.read_bytes())["cases"]
        selected = [case for case in manifest if case["id"] in SYSTEM_CASES]
        if {case["id"] for case in selected} != SYSTEM_CASES or len(selected) != len(SYSTEM_CASES):
            raise ValueError("system API regression inventory changed")
        if any(case["kind"] != "run" for case in selected):
            raise ValueError("system API fixture is no longer an executable pass")
        for producer, frontend in (("stage0", stage0), ("stage2", stage2)):
            for case in selected:
                local = directory / producer / case["id"]
                local.mkdir(parents=True)
                source = support.ROOT / "selfhost/tests" / case["file"]
                bind(source)
                if bootstrap["inputs"]["fixtures"][case["file"]] != support.digest(source):
                    raise ValueError("system source differs from validated bootstrap: " + case["file"])
                generated = local / "program.c"
                argv = ([frontend, "check", source, "--emit-c", generated] if producer == "stage0"
                        else [frontend, "emit-c", source, "-o", generated])
                label = producer + "-" + case["id"]
                record, out, err = command(label + "-emit", argv, 60, cwd=support.ROOT)
                if not sanitizer.completed(record) or out or err or not generated.is_file():
                    raise ValueError(label + " emission failed")
                bind(generated)
                program = local / "program"
                build(label + "-build", [cc, *sanitizer.FLAGS, "-I", support.ROOT / "compiler/runtime",
                                        generated, "-lm", "-o", program], program)
                run(label, [program], case["stdout"].encode("utf-8"), cwd=local)
                report["runs"][-1].update(producer=producer, source=support.identity(source),
                                         generated=support.identity(generated), emit=record)
        report["interface_executions"] = 2 * len(selected)
        report["complete_inventory"] = len(report["runs"]) == 1 + 2 * len(selected)
        if not report["complete_inventory"]:
            raise ValueError("system sanitizer execution inventory incomplete")
        changed = [path for path, digest in bound.items() if support.digest(Path(path)) != digest]
        report["inputs_unchanged"] = not changed
        if changed:
            raise ValueError("bound inputs changed: " + ", ".join(changed))
        report["status"] = "passed"
    except (OSError, ValueError, KeyError, TypeError) as error:
        report.update(status="failed", error=type(error).__name__ + ": " + str(error))
    report["bound_inputs"] = bound
    report["execution_count"] = len(report["runs"])
    support.save(directory / "receipt.json", report)
    support.save(output / "receipt.json", report)
    print(json.dumps({"status": report["status"], "receipt": str(output / "receipt.json"),
                      "executions": len(report["runs"]), "error": report.get("error")},
                     ensure_ascii=False))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
