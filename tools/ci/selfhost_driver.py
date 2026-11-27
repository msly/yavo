#!/usr/bin/env python3
"""Independent native acceptance of ordinary Yavo-written build/run."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import sys
import tempfile

sys.dont_write_bytecode = True
import csv_support as execution

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "selfhost/tests/driver49"
EXE = ".exe" if os.name == "nt" else ""
EXPECTED_PLATFORM_COUNTS = {"linux": 55, "darwin": 55, "win32": 52}
NEIGHBOR = b"pre-existing temporary-root neighbor\n"
OLD_OUTPUT = b"pre-existing output, must survive a frontend failure\n"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def bind(bound, path):
    path = Path(path).resolve()
    value = digest(path)
    if str(path) in bound and bound[str(path)] != value:
        raise ValueError("bound file changed: " + str(path))
    bound[str(path)] = value
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": value}


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                    encoding="utf-8")


def completed(record, code=0):
    return record["exit_code"] == code and not record["timed_out"] and not record["launch_error"]


def host_key():
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}[machine]
    host = {"linux": "linux", "darwin": "macos", "win32": "windows"}[sys.platform]
    return arch + "-" + host


def lock_version():
    for line in (ROOT / "compiler/toolchain.lock").read_text().splitlines():
        if line.strip().startswith("zig_version"):
            return line.split("=", 1)[1].strip()
    raise ValueError("toolchain lock has no version")


def installed_zig(version):
    if "YAVO_HOME" in os.environ:
        home = Path(os.environ["YAVO_HOME"])
    else:
        home = Path.home() / ".yavo"
    return (home / "toolchains" / ("zig-" + host_key() + "-" + version) / ("zig" + EXE)).resolve()


def bootstrap_drivers(path, bound):
    receipt_path = path.resolve()
    if receipt_path.is_dir():
        receipt_path /= "receipt.json"
    receipt = json.loads(receipt_path.read_bytes())
    if (receipt.get("schema") != "yavo-selfhost-bootstrap/1" or receipt.get("status") != "passed"
            or receipt.get("failed") != 0 or receipt.get("checks_failed") != 0
            or receipt.get("self_hosted") is not True
            or receipt.get("achieved") != "self-hosted-fixed-point"):
        raise ValueError("formal driver acceptance requires a successful complete bootstrap")
    checks = {row["id"]: row for row in receipt["checks"]}
    for name in ("inputs_unchanged_during_run", "self_compilation_fixed_point",
                 "stage3_binary_reproduces_stage2"):
        if checks.get(name, {}).get("passed") is not True:
            raise ValueError("bootstrap lacks required check: " + name)
    inputs = receipt["inputs"]
    for name, source in (("runtime_sha256", ROOT / "compiler/runtime/yavo_rt.h"),
                         ("toolchain_lock_sha256", ROOT / "compiler/toolchain.lock")):
        if digest(source) != inputs[name]:
            raise ValueError("bootstrap no longer binds current " + str(source))
        bind(bound, source)
    bundle = inputs["compiler_bundle"]
    names = {part["file"] for part in bundle["parts"]}
    if not {"driver.yv", "runtime_source.yv"}.issubset(names):
        raise ValueError("bootstrap source bundle predates the ordinary driver")
    for part in bundle["parts"]:
        for source in (ROOT / "selfhost" / part["file"], Path(bundle["directory"]) / part["file"]):
            if digest(source) != part["sha256"]:
                raise ValueError("bootstrap compiler source changed: " + str(source))
            bind(bound, source)
    for name in ("stage0", "zig", "subset_compiler"):
        source = Path(inputs[name]["path"])
        if digest(source) != inputs[name]["sha256"]:
            raise ValueError("bootstrap executable identity changed: " + name)
        bind(bound, source)
    work = Path(receipt["work"]).resolve()
    if not work.is_relative_to(receipt_path.parent):
        raise ValueError("bootstrap work directory escapes receipt directory")
    stage2 = work / "stages/stage2" / ("yavo-selfhost" + EXE)
    fixed_c = work / "stages/stage2/yavo-selfhost.c"
    if digest(stage2) != receipt["stages"]["stage2_sha256"]:
        raise ValueError("actual final stage2 differs from bootstrap")
    if digest(fixed_c) != receipt["stages"]["stage2_c"]["sha256"]:
        raise ValueError("fixed-point C differs from bootstrap")
    bind(bound, receipt_path)
    bind(bound, stage2)
    bind(bound, fixed_c)
    return [("stage1", Path(inputs["subset_compiler"]["path"])), ("stage2", stage2)], {
        "receipt": str(receipt_path), "receipt_sha256": digest(receipt_path),
        "bundle": bundle, "fixed_c": str(fixed_c), "zig": inputs["zig"],
    }


def frozen_inputs(bound):
    freeze_path = FIXTURES / "freeze.json"
    freeze = json.loads(freeze_path.read_bytes())
    if freeze["schema"] != "yavo-driver49-frozen-oracle/1":
        raise ValueError("unknown frozen driver oracle")
    for name, value in freeze["sha256"].items():
        source = FIXTURES / name
        if digest(source) != value:
            raise ValueError("frozen driver input changed: " + name)
        bind(bound, source)
    bind(bound, freeze_path)
    manifest = json.loads((FIXTURES / "cases.json").read_bytes())
    cases = manifest["cases"]
    if manifest["schema"] != "yavo-selfhost-driver-cases/1" or len(cases) != 55:
        raise ValueError("frozen driver case inventory changed")
    if len({case["id"] for case in cases}) != len(cases):
        raise ValueError("duplicate driver case ID")
    for host, count in EXPECTED_PLATFORM_COUNTS.items():
        if sum(host in case["platforms"] for case in cases) != count:
            raise ValueError("frozen platform inventory changed")
    return manifest


def supplemental_inputs(bound):
    freeze_path = FIXTURES / "supplemental-freeze.json"
    freeze = json.loads(freeze_path.read_bytes())
    source = FIXTURES / "supplemental-cases.json"
    if (freeze["schema"] != "yavo-driver49-supplemental-freeze/1"
            or freeze["sha256"] != digest(source)
            or freeze["primary_freeze_unchanged"] != digest(FIXTURES / "freeze.json")):
        raise ValueError("supplemental frozen regression identity changed")
    result = json.loads(source.read_bytes())
    cases = result["cases"]
    if (result["schema"] != "yavo-driver49-supplemental-cases/1" or len(cases) != 2
            or {case["id"] for case in cases} !=
            {"relative_temporary_dash", "relative_temporary_responsefile"}):
        raise ValueError("supplemental regression inventory changed")
    bind(bound, source)
    bind(bound, freeze_path)
    return cases


def read_probe_arguments(path):
    data = path.read_bytes()
    result = []
    while data:
        if len(data) < 4:
            raise ValueError("truncated probe argument length")
        size = struct.unpack("<I", data[:4])[0]
        if len(data) < 4 + size:
            raise ValueError("truncated probe argument")
        result.append(data[4:4 + size].decode("utf-8", errors="strict"))
        data = data[4 + size:]
    return result


def checked_environment(directory, zig_home, caches, temporary_name="temporary"):
    temporary = directory / temporary_name
    temporary.mkdir()
    (temporary / "neighbor.txt").write_bytes(NEIGHBOR)
    program_cache = directory / "program-cache"
    program_cache.mkdir()
    return {
        "YAVO_HOME": str(zig_home), "YAVO_ALLOC_CHECK": "1", "YAVO_NO_CACHE": "1",
        "YAVO_CACHE_DIR": str(program_cache), "YAVO_NO_PCH": "1", "YAVO_CC": "",
        "ZIG_GLOBAL_CACHE_DIR": str(caches / "global"),
        "ZIG_LOCAL_CACHE_DIR": str(caches / "local"),
        "TMPDIR": str(temporary), "TEMP": str(temporary), "TMP": str(temporary),
    }


def check_streams(record, out, err, expected, case):
    failures = []
    if not completed(record, expected["exit"]):
        failures.append("exit/launch/timeout")
    if out != expected["stdout"].encode("utf-8"):
        failures.append("stdout bytes")
    stderr = expected["stderr"]
    if stderr == "empty":
        if err:
            failures.append("stderr expected empty")
    elif stderr == "nonempty":
        if not err:
            failures.append("stderr expected diagnostic")
    elif err != stderr.encode("utf-8"):
        failures.append("stderr bytes")
    for value in case.get("stderr_contains", []):
        if value.encode("utf-8") not in err:
            failures.append("missing diagnostic fragment: " + value)
    for value in case.get("stderr_contains_hex", []):
        if bytes.fromhex(value) not in err:
            failures.append("backend raw stream bytes")
    return failures


def verify_backend_arguments(values, case, directory, output):
    failures = []
    if len(values) < 2 or values[1] != "cc":
        return ["backend program/cc argv"]
    arguments = values[2:]
    required = ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK"]
    required += ["-O2", "-s"] if case["backend_flags"] == "release" else ["-O0", "-g"]
    for value in required:
        if arguments.count(value) != 1:
            failures.append("backend flag count: " + value)
    remaining = list(arguments)
    for value in required:
        if value in remaining:
            remaining.remove(value)
    try:
        at = remaining.index("-o")
        output_arg = remaining[at + 1]
        del remaining[at:at + 2]
        include = None
        if "-I" in remaining:
            at = remaining.index("-I")
            include = remaining[at + 1]
            del remaining[at:at + 2]
        if len(remaining) != 1:
            failures.append("unexpected backend argv")
        else:
            c_file = Path(remaining[0])
            if not c_file.is_absolute() or c_file.name != "main.c":
                failures.append("backend generated C identity")
            if c_file.parent.parent != directory / "temporary":
                failures.append("backend C temporary parent")
            if include is not None and Path(include) != c_file.parent:
                failures.append("backend include parent")
            actual_output = (directory / output_arg).resolve()
            if case["arguments"][0] == "run":
                if actual_output != c_file.parent / ("program" + EXE):
                    failures.append("run temporary output identity")
            elif actual_output != output.resolve():
                failures.append("build requested output identity")
    except (ValueError, IndexError):
        failures.append("backend missing output operands")
    return failures


def inspect_cleanup(directory, expected, temporary_name="temporary"):
    temporary = directory / temporary_name
    failures = []
    if (temporary / "neighbor.txt").read_bytes() != NEIGHBOR:
        failures.append("pre-existing temporary neighbor changed")
    residual = sorted(str(p.relative_to(temporary)) for p in temporary.rglob("*") if p.name != "neighbor.txt")
    if expected == "clean":
        if residual:
            failures.append("temporary residue")
    else:
        direct = [p for p in temporary.iterdir() if p.name != "neighbor.txt"]
        if len(direct) != 1 or not direct[0].is_dir():
            failures.append("unexpected cleanup-failure directory inventory")
        else:
            children = list(direct[0].iterdir())
            if (len(children) != 1 or children[0].name != "unexpected-probe-file"
                    or children[0].read_bytes() != b"retain me\n"):
                failures.append("known files not removed or unexpected file changed")
    if any((directory / "program-cache").iterdir()):
        failures.append("unexpected program-cache write")
    return failures, residual


def materialize(case, directory, manifest, probe, version, bound, zig_home, caches):
    directory.mkdir(parents=True)
    sources = directory / manifest["source_directory"]
    sources.mkdir()
    copied = {}
    for name in ("main.yv", "utility.yv", "syntax.yv", "unsupported.yv", "missing_module.yv"):
        path = sources / name
        path.write_bytes((FIXTURES / name).read_bytes())
        copied[name] = bind(bound, path)
    (sources / "invalid-utf8.yv").write_bytes(b"\xff\n")
    bind(bound, sources / "invalid-utf8.yv")
    output = directory / (("main" if case.get("default_output") else manifest["output_name"]) + EXE)
    output_directory = directory / "output-directory"
    output_directory.mkdir()
    tokens = {
        "@source": str(Path(manifest["source_directory"]) / "main.yv"),
        "@source_normalized": manifest["source_directory"] + "/./main.yv",
        "@module": str(Path(manifest["source_directory"]) / "utility.yv"),
        "@syntax": str(Path(manifest["source_directory"]) / "syntax.yv"),
        "@unsupported": str(Path(manifest["source_directory"]) / "unsupported.yv"),
        "@missing_module": str(Path(manifest["source_directory"]) / "missing_module.yv"),
        "@invalid_utf8": str(Path(manifest["source_directory"]) / "invalid-utf8.yv"),
        "@missing": "not-present.yv", "@output": output.name, "@other_output": "second-output" + EXE,
        "@source_alias": "source-alias.yv", "@emitted": "generated.c",
        "@missing_output": str(Path("missing parent") / ("program" + EXE)),
        "@output_directory": output_directory.name,
    }
    overrides = checked_environment(directory, zig_home, caches, case.get("temporary_name", "temporary"))
    setup = case.get("setup", "normal")
    if setup == "existing_output":
        output.write_bytes(OLD_OUTPUT)
    if setup in ("entry_hardlink", "module_hardlink", "entry_symlink"):
        source = sources / ("utility.yv" if setup == "module_hardlink" else "main.yv")
        alias = directory / tokens["@source_alias"]
        if setup == "entry_symlink":
            alias.symlink_to(source)
        else:
            os.link(source, alias)
    if setup == "missing_temp":
        root = str(directory / "missing temporary" / "not-created")
        overrides.update(TMPDIR=root, TEMP=root, TMP=root)
    if setup == "empty_temp":
        overrides.update(TMPDIR="", TEMP="", TMP="")
    if setup == "relative_temp":
        root = case["temporary_name"]
        overrides.update(TMPDIR=root, TEMP=root, TMP=root)
    if case["kind"] == "probe_backend" or setup in ("missing_backend", "unexecutable_backend"):
        if setup == "fallback_home":
            home = directory / "fallback home 家"
            overrides["HOME" if os.name != "nt" else "USERPROFILE"] = str(home)
            overrides.pop("YAVO_HOME")
            tool_home = home / ".yavo"
        else:
            tool_home = directory / "tool home 后端"
            overrides["YAVO_HOME"] = str(tool_home)
        target = tool_home / "toolchains" / ("zig-" + host_key() + "-" + version) / ("zig" + EXE)
        target.parent.mkdir(parents=True)
        if case["kind"] == "probe_backend":
            shutil.copyfile(probe, target)
            target.chmod(0o700)
            bind(bound, target)
            overrides["YAVO_DRIVER_PROBE_MODE"] = case["probe_mode"]
        elif setup == "unexecutable_backend":
            target.write_bytes(b"This is not a native executable.\n")
            target.chmod(0o700)
            bind(bound, target)
    overrides.update(case.get("environment", {}))
    arguments = [tokens.get(value, value) for value in case["arguments"]]
    return arguments, overrides, output, copied


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, default=ROOT / "compiler/target/ci/selfhost")
    parser.add_argument("--candidate", type=Path, help="preliminary only: no final-stage identity claim")
    parser.add_argument("--case", action="append", default=[], help="preliminary candidate only")
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/selfhost-driver")
    parser.add_argument("--timeout", type=int, default=240)
    args = parser.parse_args()
    if args.case and args.candidate is None:
        parser.error("--case is allowed only for preliminary candidate checks")
    if args.timeout < 1:
        parser.error("--timeout must be positive")
    if sys.platform not in EXPECTED_PLATFORM_COUNTS:
        parser.error("driver gate supports the three declared native CI platforms")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    bound = {}
    caches = None
    report = {
        "schema": "yavo-selfhost-driver-acceptance/1", "status": "running", "work": str(work),
        "scope": "preliminary candidate" if args.candidate else "ordinary native build/run, not complete SH-3",
        "formal": args.candidate is None, "platform": sys.platform, "machine": platform.machine(),
        "commands": [], "cases": [], "checks": [], "excluded": [],
        "tokens": None, "cost": None, "compile_p50_seconds": None,
    }
    def checkpoint():
        save(work / "receipt.json", report)
        save(output / "receipt.json", report)
    def command(label, arguments, cwd, overrides, stdin=None, timeout=None):
        record, out, err = execution.execute(arguments, work / "commands" / label, cwd,
                                              environment, overrides, timeout or args.timeout, stdin=stdin)
        report["commands"].append({"id": label, **record})
        checkpoint()
        return record, out, err
    def check(name, passed, **detail):
        report["checks"].append({"id": name, "passed": bool(passed), **detail})
        checkpoint()
        if not passed:
            raise ValueError("required check failed: " + name)
    try:
        manifest = frozen_inputs(bound)
        supplemental = supplemental_inputs(bound)
        supplemental_ids = {case["id"] for case in supplemental}
        report["primary_frozen_cases"] = len(manifest["cases"])
        report["supplemental_regressions"] = len(supplemental)
        for source in (Path(__file__), ROOT / "tools/ci/csv_support.py", ROOT / "selfhost/embed_runtime.py",
                       ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock"):
            bind(bound, source)
        version = lock_version()
        bootstrap = None
        if args.candidate:
            drivers = [("candidate", args.candidate.resolve())]
            bind(bound, drivers[0][1])
            zig = installed_zig(version)
        else:
            drivers, bootstrap = bootstrap_drivers(args.bootstrap_output, bound)
            report["bootstrap"] = bootstrap
            zig = Path(bootstrap["zig"]["path"])
        bind(bound, zig)
        expected_layout = "zig-" + host_key() + "-" + version
        if zig.parent.name != expected_layout or zig.parent.parent.name != "toolchains":
            raise ValueError("native driver gate requires the actual pinned installation layout")
        zig_home = zig.parent.parent.parent
        environment = dict(os.environ)
        for name in ("YAVO_HOME", "YAVO_CC", "YAVO_CACHE_TRACE", "YAVO_DRIVER_PROBE_MODE"):
            environment.pop(name, None)
        caches = work / "zig-caches"
        for name in ("global", "local"):
            (caches / name).mkdir(parents=True)
        version_record, out, _ = command("locked-zig-version", [zig, "version"], work, {})
        check("locked_zig", completed(version_record) and out.strip() == version.encode(),
              binary=bind(bound, zig), version=version)
        embed_record, _, _ = command("embedded-runtime-freshness",
                                     [sys.executable, ROOT / "selfhost/embed_runtime.py", "--check"], ROOT, {})
        check("embedded_runtime_freshness", completed(embed_record))
        probe = work / ("native-probe" + EXE)
        probe_command = [zig, "cc", "-std=c17", "-O2", FIXTURES / "native_probe.c", "-o", probe]
        if os.name == "nt":
            probe_command.append("-municode")
        probe_record, _, _ = command("probe-build", probe_command, work, {
            "ZIG_GLOBAL_CACHE_DIR": str(caches / "global"), "ZIG_LOCAL_CACHE_DIR": str(caches / "local")})
        check("native_probe_build", completed(probe_record) and probe.is_file())
        bind(bound, probe)
        probe_record, out, err = command("probe-independent-control",
                                         [probe, *manifest["probe_control_arguments"]], work,
                                         {"YAVO_DRIVER_PROBE_MODE": "control"})
        probe_argv = read_probe_arguments(work / "child-argv.bin")
        check("native_probe_independent_argv_control",
              completed(probe_record) and not out and not err
              and probe_argv[1:] == manifest["probe_control_arguments"], arguments=probe_argv[1:])
        selected = []
        for case in manifest["cases"] + supplemental:
            if sys.platform not in case["platforms"]:
                report["excluded"].append({"id": case["id"], "reason": "frozen POSIX-only case"})
            elif not args.case or case["id"] in args.case:
                selected.append(case)
        if args.case and set(args.case) != {case["id"] for case in selected}:
            raise ValueError("unknown or inapplicable requested preliminary case")
        if not args.case and len(selected) != EXPECTED_PLATFORM_COUNTS[sys.platform] + len(supplemental):
            raise ValueError("applicable frozen case count differs")
        copies = []
        for label, compiler in drivers:
            destination = work / "compiler copies 编译器" / label / ("yavo selfhost" + EXE)
            destination.parent.mkdir(parents=True)
            shutil.copyfile(compiler, destination)
            destination.chmod(0o700)
            identity = bind(bound, destination)
            if identity["sha256"] != digest(compiler):
                raise ValueError("compiler copy changed bytes")
            copies.append((label, destination))
        report["drivers"] = [{"id": label, **bind(bound, path)} for label, path in copies]
        for label, compiler in copies:
            for case in selected:
                directory = work / "cases" / label / case["id"]
                arguments, overrides, binary, sources = materialize(
                    case, directory, manifest, probe, version, bound, zig_home, caches)
                stdin = manifest["stdin"].encode() if case["kind"] == "real_run" else None
                record, out, err = command(label + "-" + case["id"], [compiler, *arguments],
                                          directory, overrides, stdin=stdin)
                failures = check_streams(record, out, err, case["expected"], case)
                row = {"id": case["id"], "driver": label, "kind": case["kind"],
                       "command": record, "sources": sources, "expected": case["expected"],
                       "origin": "supplemental_regression" if case["id"] in supplemental_ids else "primary_frozen"}
                if case["kind"] == "real_build" and completed(record):
                    if not binary.is_file():
                        failures.append("build did not create executable")
                    else:
                        row["binary"] = bind(bound, binary)
                        child_record, child_out, child_err = command(
                            label + "-" + case["id"] + "-program", [binary], directory, overrides,
                            stdin=manifest["stdin"].encode())
                        row["program"] = child_record
                        failures += check_streams(child_record, child_out, child_err,
                                                  case["expected"]["program"], {})
                if case["kind"] == "probe_backend":
                    path = directory / "backend-argv.bin"
                    if not path.is_file():
                        failures.append("probe backend did not run")
                    else:
                        values = read_probe_arguments(path)
                        row["backend_argv"] = values
                        failures += verify_backend_arguments(values, case, directory, binary)
                        bind(bound, path)
                if case["kind"] == "emit" and completed(record):
                    generated = directory / "generated.c"
                    if not generated.is_file() or not generated.stat().st_size:
                        failures.append("emit-c did not create nonempty C")
                    else:
                        row["generated_c"] = bind(bound, generated)
                if case["expected"].get("old_output") == "unchanged":
                    if not binary.is_file() or binary.read_bytes() != OLD_OUTPUT:
                        failures.append("frontend failure altered existing output")
                cleanup_failures, residue = inspect_cleanup(directory, case["expected"]["temp"],
                                                           case.get("temporary_name", "temporary"))
                failures += cleanup_failures
                for item in sources.values():
                    if digest(item["path"]) != item["sha256"]:
                        failures.append("source bytes changed")
                row.update(passed=not failures, failures=failures, temporary_residue=residue)
                report["cases"].append(row)
                checkpoint()
                print(label + "/" + case["id"] + ": " + ("PASS" if not failures else "FAIL " + repr(failures)),
                      flush=True)
        if bootstrap is not None:
            stage2 = next(path for label, path in copies if label == "stage2")
            directory = work / "ordinary self build 自举"
            directory.mkdir()
            overrides = checked_environment(directory, zig_home, caches)
            source = Path(bootstrap["bundle"]["path"])
            rebuilt = directory / ("rebuilt-selfhost" + EXE)
            record, out, err = command("ordinary-build-self",
                                      [stage2, "build", source, "-o", rebuilt, "--release"],
                                      directory, overrides, timeout=max(args.timeout, 600))
            check("ordinary_stage2_builds_compiler",
                  completed(record) and rebuilt.is_file() and not out, command=record)
            bind(bound, rebuilt)
            generated = directory / "rebuilt-fixed-point.c"
            record, out, err = command("ordinary-rebuilt-emit",
                                      [rebuilt, "emit-c", source, "-o", generated], directory, overrides,
                                      timeout=max(args.timeout, 120))
            check("ordinary_rebuilt_emits_fixed_point_c", completed(record) and not out and not err
                  and generated.is_file() and generated.read_bytes() == Path(bootstrap["fixed_c"]).read_bytes(),
                  command=record)
            bind(bound, generated)
            record, out, err = command("ordinary-rebuilt-run",
                                      [rebuilt, "run", FIXTURES / "main.yv"], directory, overrides,
                                      stdin=manifest["stdin"].encode())
            wanted = next(case["expected"] for case in manifest["cases"] if case["id"] == "run_debug_stdio")
            failures = check_streams(record, out, err, wanted, {})
            clean, residue = inspect_cleanup(directory, "clean")
            check("ordinary_rebuilt_runs_program", not failures and not clean,
                  failures=failures + clean, temporary_residue=residue, command=record)
        changed = [name for name, value in bound.items()
                   if not Path(name).is_file() or digest(name) != value]
        check("inputs_unchanged_during_run", not changed, changed=changed)
        report["passed"] = sum(case["passed"] for case in report["cases"])
        report["failed"] = sum(not case["passed"] for case in report["cases"])
        report["applicable_cases_per_driver"] = len(selected)
        report["driver_count"] = len(copies)
        report["case_count"] = len(report["cases"])
        report["kind_counts"] = dict(Counter(case["kind"] for case in report["cases"]))
        check("complete_case_inventory", len(report["cases"]) == len(selected) * len(copies))
        report["status"] = ("preliminary_passed" if args.candidate else "passed") if report["failed"] == 0 else "failed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = repr(error)
        print("FAIL: " + repr(error), flush=True)
    finally:
        # The portable evidence packer refuses symlinks. Preserve their exact
        # spelling/target after execution, then remove only these test-created
        # links; the linked source bytes remain bound and available.
        removed_links = []
        for path in work.rglob("*"):
            if path.is_symlink():
                removed_links.append({"path": str(path), "target": os.readlink(path),
                                      "removed_after_execution": True})
                path.unlink()
        report["materialized_symlinks"] = removed_links
        if caches is not None and caches.exists():
            try:
                shutil.rmtree(caches)
                report["zig_cache_cleanup"] = {"status": "removed", "path": str(caches)}
            except OSError as error:
                report["status"] = "failed"
                report["zig_cache_cleanup"] = {"status": "failed", "path": str(caches), "error": repr(error)}
        report["input_identities"] = bound
        measurements = [record["peak_process_memory_bytes"] for record in report["commands"]]
        report["resources"] = {
            "commands": len(measurements), "commands_with_rss": sum(value is not None for value in measurements),
            "max_observed_child_rss_bytes": max((value for value in measurements if value is not None), default=None),
            "scope": "wait4 waited-child RSS on POSIX; not simultaneous process-tree RSS; no P50 or token/cost claim",
        }
        checkpoint()
    print(json.dumps({key: report.get(key) for key in
                      ("status", "formal", "case_count", "passed", "failed", "applicable_cases_per_driver", "error")},
                     ensure_ascii=False), flush=True)
    print("Receipt: " + str(output / "receipt.json"), flush=True)
    return 0 if report["status"] in ("passed", "preliminary_passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
