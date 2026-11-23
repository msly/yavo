#!/usr/bin/env python3
"""Independent application acceptance for yavo-test-summary, never an SH-4 gate.

The manual expectations and independent Python oracle are frozen before the
application is executed. Each invocation preserves a new attempt directory.
"""
from __future__ import annotations

import argparse
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "examples/test-summary"
TESTS = APP / "tests"
HEADER = "file\tpassed\tfailed\tskipped\trequirement_passed\trequirement_failed\tstatus\n"
USAGE = "Usage: yavo-test-summary [--] LOG...\n"
MODULES = ("main.yv", "parser.yv", "stats.yv", "report.yv")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    with path.open("rb") as source:
        state = hashlib.sha256()
        for block in iter(lambda: source.read(1024 * 1024), b""):
            state.update(block)
    return state.hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True)
                    + "\n", encoding="utf-8", newline="\n")


def identity(path):
    return {"path": str(path.resolve()), "bytes": path.stat().st_size,
            "sha256": digest(path)}


def repo_path(name):
    path = (ROOT / name).resolve()
    if not path.is_relative_to(ROOT):
        raise ValueError("repository input escapes root")
    return path


def spelling(name):
    return str(repo_path(name[6:])) if name.startswith("@repo:") else name


def load_case(case):
    """Resolve archived inputs without consulting or executing application code."""
    contents = {}
    for source in case["files"]:
        name = spelling(source["path"])
        if "repository" in source:
            data = repo_path(source["repository"]).read_bytes()
            if sha(data) != source["sha256"]:
                raise ValueError("real fixture identity changed: " + name)
        elif source.get("directory"):
            data = None
        elif "base64" in source:
            data = base64.b64decode(source["base64"], validate=True)
        else:
            data = source["text"].encode("utf-8")
        if name in contents:
            raise ValueError("duplicate fixture path")
        contents[name] = data
    wanted = copy.deepcopy(case["expected"])
    for row in wanted["rows"] or []:
        row[0] = spelling(row[0])
    for diagnostic in wanted["diagnostics"]:
        if "path" in diagnostic:
            diagnostic["path"] = spelling(diagnostic["path"])
    return [spelling(arg) for arg in case["args"]], contents, wanted


def frozen_oracle():
    freeze_path = TESTS / "oracle-freeze.json"
    freeze = json.loads(freeze_path.read_text(encoding="utf-8"))
    bound = {str(freeze_path): digest(freeze_path)}
    for name, wanted in {**freeze["files"], **freeze["real_logs"]}.items():
        path = repo_path(name)
        if digest(path) != wanted:
            raise ValueError("frozen oracle input changed: " + name)
        bound[str(path)] = wanted
    manifest = json.loads((TESTS / "cases.json").read_text(encoding="utf-8"))
    if manifest["spec_sha256"] != digest(APP / "SPEC.md"):
        raise ValueError("case contract identity mismatch")
    spec = importlib.util.spec_from_file_location("test_summary_independent_oracle", TESTS / "oracle.py")
    reference = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reference)
    cases, review = manifest["cases"], []
    if not cases or len(cases) != freeze["scenario_count"]:
        raise ValueError("frozen scenario inventory is empty or incomplete")
    seen = set()
    for case in cases:
        if case["id"] in seen:
            raise ValueError("duplicate scenario identity")
        seen.add(case["id"])
        args, contents, wanted = load_case(case)
        actual = reference.evaluate(args, contents)
        if actual != wanted:
            raise ValueError("manual expectation disagrees with independent oracle: "
                             + case["id"] + " " + repr(actual))
        review.append({"id": case["id"], "passed": True,
                       "applicable": os.name in case.get("platforms", [os.name])})
    return cases, bound, {"status": "passed", "scenarios": len(cases), "results": review,
                          "freeze": identity(freeze_path),
                          "scope": "manual expectations versus independent byte-input oracle; no application execution"}


def execute(argv, directory, cwd, environment, overrides, timeout):
    directory.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    record = {"argv": [str(arg) for arg in argv], "cwd": str(cwd),
              "environment_overrides": overrides, "timed_out": False,
              "exit_code": None, "launch_error": None}
    out, err = directory / "stdout.bin", directory / "stderr.bin"
    with out.open("wb") as stdout, err.open("wb") as stderr:
        try:
            options = ({"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
                       if os.name == "nt" else {"start_new_session": True})
            process = subprocess.Popen(record["argv"], cwd=cwd, env=environment,
                                       stdin=subprocess.DEVNULL, stdout=stdout,
                                       stderr=stderr, **options)
            try:
                record["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record["timed_out"] = True
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   timeout=15, check=False)
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                record["exit_code"] = process.wait(timeout=15)
        except (OSError, subprocess.SubprocessError) as exc:
            record["launch_error"] = str(exc)
    record["elapsed_seconds"] = time.monotonic() - started
    record["stdout"], record["stderr"] = identity(out), identity(err)
    save(directory / "command.json", record)
    return record, out.read_bytes(), err.read_bytes()


def escaped(path):
    return path.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r")


def expected_stdout(wanted):
    if wanted["help"]:
        return USAGE.encode("utf-8")
    if wanted["rows"] is None:
        return b""
    lines = [HEADER]
    for row in wanted["rows"]:
        lines.append("\t".join([escaped(row[0]), *map(str, row[1:])]) + "\n")
    return "".join(lines).encode("utf-8")


def stderr_matches(data, wanted):
    try:
        text = data.decode("utf-8", errors="strict")
    except UnicodeError:
        return False
    events = wanted["diagnostics"]
    if events and events[0]["code"] == "ECLI_USAGE":
        return ("ECLI_USAGE" in text and USAGE.rstrip("\n") in text
                and not any(code in text for code in ("ELOG_", "EIO_")))
    if not events:
        return data == b""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if len(lines) != len(events):
        return False
    for line, event in zip(lines, events):
        prefix = escaped(event["path"]) + ":"
        if "line" in event:
            prefix += str(event["line"]) + ":"
        prefix += " " + event["code"] + ":"
        if not line.startswith(prefix):
            return False
    return True


def materialize(case, directory, contents):
    directory.mkdir(parents=True, exist_ok=False)
    written = []
    for source in case["files"]:
        if "repository" in source:
            written.append(identity(repo_path(source["repository"])))
            continue
        path = (directory / source["path"]).resolve()
        if not path.is_relative_to(directory.resolve()):
            raise ValueError("fixture path escapes its isolated directory")
        path.parent.mkdir(parents=True, exist_ok=True)
        if source.get("directory"):
            path.mkdir()
            written.append({"path": str(path), "kind": "directory"})
        else:
            path.write_bytes(contents[spelling(source["path"])])
            written.append(identity(path))
    return written


def locked_zig():
    lock = ROOT / "compiler/toolchain.lock"
    values = {}
    for line in lock.read_text(encoding="utf-8").splitlines():
        if "=" in line and not line.lstrip().startswith("#"):
            key, value = line.split("=", 1)
            values[key.strip()] = value.strip()
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64",
            "aarch64": "aarch64"}.get(platform.machine().lower())
    system = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(platform.system())
    host = str(arch) + "-" + str(system)
    if host not in values:
        raise ValueError("host has no locked Zig toolchain: " + host)
    home = Path(os.environ["YAVO_HOME"]) if os.environ.get("YAVO_HOME") else Path.home() / ".yavo"
    path = home / "toolchains" / ("zig-" + host + "-" + values["zig_version"])
    path /= "zig.exe" if os.name == "nt" else "zig"
    if not path.is_file():
        raise ValueError("locked Zig is not installed: " + str(path))
    return path.resolve(), values["zig_version"], values[host]


def cleanup_artifact(zig, generated_c, work, environment, overrides, report, bound, timeout):
    """Exercise explicit os.exit paths without changing source C or production runtime."""
    directory = work / "cleanup-checked"
    directory.mkdir()
    inputs = directory / "build"
    inputs.mkdir()
    originals = {"yavo_rt.h": ROOT / "compiler/runtime/yavo_rt.h",
                 "exit_check.h": TESTS / "exit_check.h",
                 "cleanup_clean.c": TESTS / "cleanup_clean.c",
                 "cleanup_leak.c": TESTS / "cleanup_leak.c",
                 "program.c": generated_c}
    copied = {}
    for name, source in originals.items():
        wanted = bound.get(str(source), digest(source))
        data = source.read_bytes()
        if sha(data) != wanted:
            raise ValueError("cleanup input changed before snapshot: " + str(source))
        destination = inputs / name
        destination.write_bytes(data)
        if digest(destination) != wanted:
            raise ValueError("cleanup input copy differs from its bound source")
        bound[str(source)] = bound[str(destination)] = wanted
        copied[name] = {"original": identity(source), "compiled_copy": identity(destination)}
    shim = inputs / "exit_check.h"
    validation = {"scope": "supplementary artifact with test-only os.exit allocation check; production C and runtime unmodified",
                  "shim": identity(shim), "original_generated_c": identity(generated_c),
                  "runtime": identity(inputs / "yavo_rt.h"), "copied_inputs": copied, "probes": [],
                  "not_covered": ["use-after-free", "untracked native allocations", "panic unwinding"]}
    report["cleanup_validation"] = validation
    flags = [zig, "cc", "-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O2", "-s",
             "-I", inputs, "-include", shim]
    for label, wanted_exit in (("clean", 2), ("leak", 102)):
        source = inputs / ("cleanup_" + label + ".c")
        binary = directory / (label + (".exe" if os.name == "nt" else ""))
        build, _, _ = execute([*flags, source, "-o", binary], directory / (label + "-build"),
                              directory, environment, overrides, timeout)
        item = {"id": label, "source": identity(source), "build": build, "passed": False,
                "expected_exit": wanted_exit}
        validation["probes"].append(item)
        if build["exit_code"] != 0 or build["timed_out"] or not binary.is_file():
            raise ValueError("cleanup capability probe failed to build: " + label)
        bound[str(binary)] = digest(binary)
        run, stdout, stderr = execute([binary], directory / (label + "-run"), directory,
                                      environment, overrides, 10)
        expected_stderr = (stderr == b"") if label == "clean" else (
            b"yavo alloc-check: 1 heap block(s) still alive at exit" in stderr)
        item.update(run=run, binary=identity(binary), passed=(run["exit_code"] == wanted_exit
                    and not run["timed_out"] and stdout == b"" and expected_stderr))
        if not item["passed"]:
            raise ValueError("cleanup capability probe was not detected correctly: " + label)
    binary = directory / ("yavo-test-summary.exe" if os.name == "nt" else "yavo-test-summary")
    build, _, _ = execute([*flags, inputs / "program.c", "-o", binary], directory / "application-build",
                          directory, environment, overrides, timeout)
    validation["application_build"] = build
    if build["exit_code"] != 0 or build["timed_out"] or not binary.is_file():
        raise ValueError("supplementary cleanup artifact failed to build")
    validation["binary"] = identity(binary)
    bound[str(binary)] = validation["binary"]["sha256"]
    return binary, environment, overrides


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path,
                        default=ROOT / "compiler/target/release" / ("yavo.exe" if os.name == "nt" else "yavo"))
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/test-summary")
    parser.add_argument("--oracle-only", action="store_true")
    parser.add_argument("--build-timeout", type=int, default=300)
    parser.add_argument("--case-timeout", type=int, default=10)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    report = {"schema": "yavo-test-summary-acceptance/1", "status": "running",
              "scope": "one stage0-compiled external CLI; not complete SH-4 or full self-hosted language acceptance",
              "host": {"platform": platform.platform(), "machine": platform.machine()},
              "work": str(work), "builds": [], "runs": [], "checks": [],
              "model_usage": None, "monetary_cost": None,
              "compiler_built_by_runner": False, "application_source_inspected_by_oracle": False}
    bound = {}

    def flush():
        save(work / "receipt.json", report)
        save(output / "receipt.json", report)

    try:
        report["revision"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        report["git_status"] = subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True)
        cases, bound, report["oracle_review"] = frozen_oracle()
        bound[str(Path(__file__).resolve())] = digest(Path(__file__))
        report["not_applicable"] = [{"id": case["id"], "platforms": case["platforms"]}
                                    for case in cases if os.name not in case.get("platforms", [os.name])]
        cases = [case for case in cases if os.name in case.get("platforms", [os.name])]
        if not cases:
            raise ValueError("no application scenarios apply to this host")
        report["applicable_scenarios"] = len(cases)
        if args.oracle_only:
            report.update(status="passed", application_executions=0, build_attempts=0,
                          validation_mode="oracle-only")
            flush()
            print(json.dumps({"status": "passed", "oracle_scenarios": report["oracle_review"]["scenarios"],
                              "application_executions": 0, "receipt": str(output / "receipt.json")}))
            return 0
        compiler = args.compiler.resolve()
        if os.environ.get("YAVO_CC"):
            raise ValueError("YAVO_CC override is forbidden in application acceptance")
        if not compiler.is_file():
            raise ValueError("build the current stage0 Release compiler before this acceptance run")
        present = {p.name for p in APP.glob("*.yv")}
        if present != set(MODULES):
            raise ValueError("application must supply exactly the four contracted modules")
        source_bytes = {name: (APP / name).read_bytes() for name in MODULES}
        bound.update({str(APP / name): sha(data) for name, data in source_bytes.items()})
        for path in [compiler, ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock",
                     ROOT / "compiler/Cargo.lock", ROOT / "compiler/Cargo.toml"]:
            bound[str(path)] = digest(path)
        for path in sorted((ROOT / "compiler/src").glob("*.rs")):
            bound[str(path)] = digest(path)
        zig, version, locked_archive = locked_zig()
        bound[str(zig)] = digest(zig)
        report["compiler"] = identity(compiler)
        report["runtime"] = identity(ROOT / "compiler/runtime/yavo_rt.h")
        report["toolchain"] = {"executable": identity(zig), "version": version,
                               "locked_archive": locked_archive,
                               "lock": identity(ROOT / "compiler/toolchain.lock")}
        report["application_sources"] = {name: {"bytes": len(data), "sha256": sha(data)}
                                          for name, data in source_bytes.items()}
        environment = dict(os.environ)
        for key in ("YAVO_CC", "YAVO_CACHE_TRACE", "YAVO_UPDATE_BASELINE", "OPENAI_API_KEY", "CODEX_API_KEY"):
            environment.pop(key, None)
        version_command, stdout, _ = execute([zig, "version"], work / "zig-version", ROOT,
                                             environment, {}, 20)
        report["toolchain"]["version_command"] = version_command
        if (version_command["exit_code"] != 0 or version_command["timed_out"]
                or stdout.decode().strip() != version):
            raise ValueError("installed Zig does not match the locked version")
        builds = {}
        for label, release in (("debug", False), ("release-a", True), ("release-b", True)):
            directory = work / label
            directory.mkdir()
            sources = directory / "src"
            sources.mkdir()
            for name, data in source_bytes.items():
                path = sources / name
                path.write_bytes(data)
                if digest(path) != sha(data):
                    raise ValueError("application source snapshot differs from original")
                bound[str(path)] = sha(data)
            cache = directory / "empty-caches"
            for name in ("yavo", "zig-global", "zig-local", "temporary"):
                (cache / name).mkdir(parents=True)
            overrides = {"YAVO_ALLOC_CHECK": "1", "YAVO_NO_CACHE": "1", "YAVO_NO_PCH": "1",
                         "YAVO_CACHE_DIR": str(cache / "yavo"),
                         "ZIG_GLOBAL_CACHE_DIR": str(cache / "zig-global"),
                         "ZIG_LOCAL_CACHE_DIR": str(cache / "zig-local"),
                         "TMPDIR": str(cache / "temporary"), "TMP": str(cache / "temporary"),
                         "TEMP": str(cache / "temporary")}
            build_env = dict(environment, **overrides)
            basename = "yavo-test-summary.exe" if os.name == "nt" else "yavo-test-summary"
            binary, emitted = directory / basename, directory / "program.c"
            argv = [compiler, "build", "src/main.yv", "-o", basename, "--emit-c", "program.c"]
            if release:
                argv.append("--release")
            command, _, _ = execute(argv, directory / "build-command", directory,
                                     build_env, overrides, args.build_timeout)
            item = {"id": label, "mode": "release" if release else "debug", "command": command,
                    "source_snapshot": {name: identity(sources / name) for name in MODULES},
                    "initial_cache_state": "all four directories newly created and empty",
                    "passed": command["exit_code"] == 0 and not command["timed_out"]
                              and command["launch_error"] is None and binary.is_file() and emitted.is_file()}
            if binary.is_file():
                item["binary"] = identity(binary)
                bound[str(binary)] = item["binary"]["sha256"]
            if emitted.is_file():
                item["generated_c"] = identity(emitted)
                bound[str(emitted)] = item["generated_c"]["sha256"]
            report["builds"].append(item)
            flush()
            if not item["passed"]:
                raise ValueError("application build failed: " + label)
            builds[label] = (binary, build_env, overrides)
        left, right = report["builds"][1:3]
        for field in ("generated_c", "binary"):
            report["checks"].append({"id": "independent_release_" + field,
                                     "passed": left[field]["sha256"] == right[field]["sha256"],
                                     "left_sha256": left[field]["sha256"], "right_sha256": right[field]["sha256"]})
        _, release_env, release_overrides = builds["release-a"]
        builds["cleanup-checked"] = cleanup_artifact(
            zig, Path(left["generated_c"]["path"]), work, release_env, release_overrides,
            report, bound, args.build_timeout)
        signatures = {}
        for label in ("debug", "release-a", "cleanup-checked"):
            binary, run_env, overrides = builds[label]
            for case in cases:
                argv, contents, wanted = load_case(case)
                directory = work / label / "cases" / case["id"]
                inputs = materialize(case, directory, contents)
                command, stdout, stderr = execute([binary, *argv], directory / "command", directory,
                                                   run_env, overrides, args.case_timeout)
                wanted_stdout = expected_stdout(wanted)
                (directory / "expected-stdout.bin").write_bytes(wanted_stdout)
                save(directory / "expected.json", wanted)
                checks = {"exit_code": command["exit_code"] == wanted["exit"],
                          "stdout": stdout == wanted_stdout, "stderr": stderr_matches(stderr, wanted),
                          "completed": not command["timed_out"] and command["launch_error"] is None}
                signature = (command["exit_code"], sha(stdout), sha(stderr))
                if label == "debug":
                    signatures[case["id"]] = signature
                elif label == "release-a":
                    checks["debug_release_equal"] = signatures[case["id"]] == signature
                else:
                    checks["unmodified_application_behavior_equal"] = signatures[case["id"]] == signature
                item = {"id": case["id"], "build": label, "group": case.get("group", case["id"]),
                        "passed": all(checks.values()), "checks": checks, "command": command,
                        "input_files": inputs, "expected": wanted,
                        "expected_stdout_sha256": sha(wanted_stdout)}
                report["runs"].append(item)
                print(label + "/" + case["id"] + ": " + ("PASS" if item["passed"] else "FAIL"), flush=True)
            flush()
        report["input_identities"] = bound
        changed = [name for name, wanted in bound.items() if not Path(name).is_file() or digest(Path(name)) != wanted]
        report["checks"].append({"id": "inputs_unchanged", "passed": not changed, "changed": changed})
        required_ids = {case["id"] for case in cases}
        inventory = {label: [row["id"] for row in report["runs"] if row["build"] == label]
                     for label in ("debug", "release-a", "cleanup-checked")}
        complete = (len(report["builds"]) == 3
                    and len(report["cleanup_validation"]["probes"]) == 2
                    and all(probe["passed"] for probe in report["cleanup_validation"]["probes"])
                    and len(report["runs"]) == 3 * len(cases)
                    and all(len(ids) == len(cases) and set(ids) == required_ids for ids in inventory.values()))
        report["checks"].append({"id": "complete_application_inventory", "passed": complete,
                                 "required_scenarios_per_artifact": len(cases),
                                 "observed_scenarios_per_artifact": {label: len(ids) for label, ids in inventory.items()},
                                 "required_native_builds": 3, "observed_native_builds": len(report["builds"]),
                                 "required_cleanup_capabilities": 2})
        report["passed"] = sum(row["passed"] for row in report["runs"])
        report["failed"] = len(report["runs"]) - report["passed"]
        report["application_executions"] = len(report["runs"])
        report["build_attempts"] = len(report["builds"])
        report["supplementary_cleanup_executions"] = sum(row["build"] == "cleanup-checked" for row in report["runs"])
        report["normal_application_executions"] = len(report["runs"]) - report["supplementary_cleanup_executions"]
        report["supplementary_compile_commands"] = 3
        report["cleanup_validation"]["status"] = ("passed" if all(row["passed"] for row in report["runs"]
                                                                   if row["build"] == "cleanup-checked") else "failed")
        report["status"] = "passed" if report["failed"] == 0 and all(row["passed"] for row in report["checks"]) else "failed"
    except Exception as exc:
        report.update(status="failed", error=type(exc).__name__ + ": " + str(exc), input_identities=bound,
                      application_executions=len(report["runs"]), build_attempts=len(report["builds"]))
        print(report["error"], file=sys.stderr, flush=True)
    flush()
    print(json.dumps({key: report.get(key) for key in
                      ("status", "applicable_scenarios", "application_executions", "build_attempts", "passed", "failed")}), flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
