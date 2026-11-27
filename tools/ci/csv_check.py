#!/usr/bin/env python3
"""Native, reproducible acceptance of the frozen four-module CSV application."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tarfile
import tempfile

sys.dont_write_bytecode = True
import csv_support as support
import csv_sanitize as sanitizer

ROOT, APP = support.ROOT, support.APP
MODULES = ("main.yv", "parser.yv", "validator.yv", "report.yv")
EXE = ".exe" if os.name == "nt" else ""
BASENAME = "yavo-csv-check" + EXE
APP_SLUG = "csv-check"
APP_LABEL = "CSV"
SCHEMA = "yavo-csv-check-acceptance/1"
SCOPE = "second independent SH-4 application acceptance; not complete SH-4"
DEFAULT_OUTPUT = ROOT / "compiler/target/ci/csv-check"
EDIT_PREFIX = b"// CSV acceptance: comment-only edit measurement.\n"
completed, bind = sanitizer.completed, sanitizer.bind


def source_snapshot(directory, contents, bound=None):
    sources = directory / "src"
    sources.mkdir(parents=True)
    result = {}
    for name, data in contents.items():
        path = sources / name
        path.write_bytes(data)
        result[name] = support.identity(path) if bound is None else bind(bound, path)
    return result


def cache_environment(directory, enabled):
    cache = directory / "caches"
    paths = {name: cache / name for name in ("yavo", "zig-global", "zig-local", "temporary")}
    for path in paths.values():
        path.mkdir(parents=True)
        if any(path.iterdir()):
            raise ValueError("build cache is not initially empty")
    overrides = {"YAVO_ALLOC_CHECK": "1", "YAVO_CACHE_DIR": str(paths["yavo"]),
                 "ZIG_GLOBAL_CACHE_DIR": str(paths["zig-global"]),
                 "ZIG_LOCAL_CACHE_DIR": str(paths["zig-local"]),
                 "TMPDIR": str(paths["temporary"]), "TMP": str(paths["temporary"]),
                 "TEMP": str(paths["temporary"])}
    if enabled:
        overrides["YAVO_CACHE_TRACE"] = "1"
    else:
        overrides.update(YAVO_NO_CACHE="1", YAVO_NO_PCH="1")
    return overrides


def build_argv(compiler, mode):
    return [compiler, "build", "src/main.yv", "-o", BASENAME, "--emit-c", "program.c",
            *(["--release"] if mode == "release" else []), "--json"]


def native_build(compiler, directory, mode, contents, environment, bound, timeout, label):
    directory.mkdir()
    snapshot = source_snapshot(directory, contents, bound)
    overrides = cache_environment(directory, False)
    record, _out, _err = support.execute(build_argv(compiler, mode), directory / "command",
                                         directory, environment, overrides, timeout)
    binary, generated = directory / BASENAME, directory / "program.c"
    row = {"id": label, "mode": mode, "command": record, "source_snapshot": snapshot,
           "cache": "independent empty Yavo/Zig/temp directories; program cache and runtime PCH disabled",
           "passed": completed(record) and binary.is_file() and generated.is_file()}
    if binary.is_file():
        row["binary"] = bind(bound, binary)
    if generated.is_file():
        row["generated_c"] = bind(bound, generated)
    return row, binary, generated, overrides


def cleanup_artifact(zig, generated, directory, environment, overrides, bound, timeout, report):
    directory.mkdir()
    inputs = directory / "inputs"
    inputs.mkdir()
    originals = {"program.c": generated, "yavo_rt.h": ROOT / "compiler/runtime/yavo_rt.h",
                 **{name: support.CONTROLS / name
                    for name in ("exit_check.h", "cleanup_clean.c", "cleanup_leak.c")}}
    copies = {}
    for name, original in originals.items():
        original_id = bind(bound, original)
        target = inputs / name
        target.write_bytes(Path(original).read_bytes())
        copied = bind(bound, target)
        if copied["sha256"] != original_id["sha256"]:
            raise ValueError("cleanup snapshot differs from source")
        copies[name] = {"original": original_id, "compiled_copy": copied}
    report.update(status="running", controls=[], inputs=copies,
                  scope="test-only forced-include exit check; original generated C and runtime unchanged",
                  not_covered=["untracked native allocations", "use-after-free", "panic unwinding"])
    flags = [zig, "cc", "-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK",
             "-O2", "-s", "-I", inputs, "-include", inputs / "exit_check.h"]
    for label, expected in (("clean", 2), ("leak", 102)):
        binary = directory / (label + EXE)
        built, _out, _err = support.execute(
            [*flags, inputs / ("cleanup_" + label + ".c"), "-o", binary],
            directory / (label + "-build"), directory, environment, overrides, timeout)
        row = {"id": label, "build": built, "expected_exit": expected, "passed": False}
        report["controls"].append(row)
        if not completed(built) or not binary.is_file():
            raise ValueError("cleanup control build failed: " + label)
        row["binary"] = bind(bound, binary)
        record, out, err = support.execute([binary], directory / (label + "-run"),
                                           directory, environment, overrides, 10)
        row["run"] = record
        row["passed"] = (completed(record, expected) and out == b"" and
                         (err == b"" if label == "clean" else
                          b"yavo alloc-check: 1 heap block(s) still alive at exit" in err))
        if not row["passed"]:
            raise ValueError("cleanup control did not detect its expected behavior: " + label)
    binary = directory / BASENAME
    built, _out, _err = support.execute([*flags, inputs / "program.c", "-o", binary],
                                       directory / "application-build", directory,
                                       environment, overrides, timeout)
    report["application_build"] = built
    if not completed(built) or not binary.is_file():
        raise ValueError("cleanup application build failed")
    report.update(status="passed", binary=bind(bound, binary))
    support.save(directory / "receipt.json", report)
    return binary, report


def build_timings(compiler, directory, mode, contents, environment, bound, timeout):
    directory.mkdir()
    source_snapshot(directory, contents)
    overrides = cache_environment(directory, True)
    rows = []
    for phase in ("cold", "unchanged", "edited"):
        if phase == "edited":
            (directory / "src/main.yv").write_bytes(
                EDIT_PREFIX + contents["main.yv"])
        source_before = {name: support.digest(directory / "src" / name) for name in MODULES}
        record, _out, err = support.execute(
            build_argv(compiler, mode), directory / phase / "command",
            directory, environment, overrides, timeout)
        binary, generated = directory / BASENAME, directory / "program.c"
        row = {"phase": phase, "mode": mode, "command": record, "source_sha256": source_before,
               "sample_count": 1, "p50_ms": None, "p95_ms": None,
               "peak_process_memory_bytes": record.get("peak_process_memory_bytes"),
               "cache_events": [line for line in err.decode("utf-8", "replace").splitlines()
                                if line.startswith("yavo: ")],
               "checks": {"completed": completed(record),
                          "artifacts": binary.is_file() and generated.is_file(),
                          "source_unchanged_during_command": all(
                              support.digest(directory / "src" / name) == wanted
                              for name, wanted in source_before.items())}}
        hit = b"yavo: build cache hit " in err
        miss = b"yavo: build cache miss " in err
        row["checks"]["cache_trace"] = (hit and not miss) if phase == "unchanged" else (miss and not hit)
        row["cache_classification"] = "hit" if hit else "miss" if miss else "unobserved"
        if binary.is_file() and generated.is_file():
            snapshot = directory / phase / "artifacts"
            snapshot.mkdir()
            for key, original in (("binary", binary), ("generated_c", generated)):
                copy = snapshot / original.name
                shutil.copyfile(original, copy)
                row[key] = bind(bound, copy)
            if phase == "unchanged":
                row["checks"]["cold_artifacts_equal"] = all(
                    row[key]["sha256"] == rows[0][key]["sha256"] for key in ("binary", "generated_c"))
            if phase == "edited":
                row["checks"]["edit_changed_generated_c"] = (
                    row["generated_c"]["sha256"] != rows[0]["generated_c"]["sha256"])
        row["passed"] = all(row["checks"].values())
        rows.append(row)
        support.save(directory / "receipt.json",
                     {"mode": mode, "measurements": rows,
                      "scope": "single observations, not P50/P95 or a compiler performance gate"})
        if not row["passed"]:
            raise ValueError("build timing/cache validation failed: " + mode + "/" + phase)
    for name in MODULES:
        bind(bound, directory / "src" / name)
    bind(bound, directory / BASENAME)
    bind(bound, directory / "program.c")
    return {"mode": mode, "status": "passed", "measurements": rows,
            "scope": "isolated cache cold/unchanged/comment-edited builds, one sample per condition",
            "original_source_unmodified": True, "runtime_pch": "enabled; actual hit/miss trace recorded",
            "p50_ms": None, "p95_ms": None}


def bind_case_files(bound, row):
    for item in row.get("input_files", []):
        if "sha256" in item:
            if support.digest(Path(item["path"])) != item["sha256"]:
                raise ValueError("application modified a fixture input")
            bind(bound, item["path"])


def changed_inputs(bound):
    return [name for name, wanted in bound.items()
            if not Path(name).is_file() or support.digest(Path(name)) != wanted]


def archive_fixture_inputs(work, rows, absolute_root, bound):
    """Keep byte-exact fixtures in one upload-safe archive, including odd names."""
    roots = {Path(row["command"]["cwd"]).resolve() for row in rows}
    if absolute_root.is_dir():
        roots.add(absolute_root.resolve())
    for root in roots:
        if not root.is_relative_to(work.resolve()):
            raise ValueError("fixture archive root escapes this acceptance run")
    roots = [root for root in roots if not any(root != other and root.is_relative_to(other)
                                               for other in roots)]
    expected = {}
    for row in rows:
        for item in row.get("input_files", []):
            if "sha256" in item:
                path = str(Path(item["path"]).resolve())
                if path in expected and expected[path] != item["sha256"]:
                    raise ValueError("fixture identity differs between executions")
                expected[path] = item["sha256"]
    archive = work / "fixture-inputs.tar.gz"
    with tarfile.open(archive, "w:gz", format=tarfile.PAX_FORMAT) as stream:
        for root in sorted(roots):
            stream.add(root, arcname=root.relative_to(work).as_posix(), recursive=True)
    observed = {}
    with tarfile.open(archive, "r:gz") as stream:
        for item in stream.getmembers():
            if item.isfile():
                path = (work / item.name).resolve()
                if not path.is_relative_to(work):
                    raise ValueError("fixture archive member escapes its run")
                with stream.extractfile(item) as source:
                    observed[str(path)] = support.sha(source.read())
    if observed != expected:
        raise ValueError("fixture archive contents differ from executed input identities")
    result = {"artifact": bind(bound, archive), "verified": True, "regular_files": len(expected),
              "member_base": str(work), "original_paths_removed": True,
              "scope": "byte-exact materialized case inputs; source fixtures stay frozen in the repository"}
    for root in roots:
        shutil.rmtree(root)
    for name in expected:
        bound.pop(name, None)
    return result


def remove_transient_caches(work):
    removed = []
    for label in ("debug", "release-a", "release-b", "timing-debug", "timing-release"):
        path = work / (label + " space-目录") / "caches"
        if path.is_dir():
            shutil.rmtree(path)
            removed.append(str(path))
    return removed


def command_memory(value):
    """Summarize recorded command peaks with max, never by adding RSS peaks."""
    records = []

    def visit(node):
        if isinstance(node, dict):
            if "argv" in node and "elapsed_seconds" in node and "peak_process_memory_bytes" in node:
                records.append(node)
            else:
                for child in node.values():
                    visit(child)
        elif isinstance(node, list):
            for child in node:
                visit(child)

    visit(value)
    peaks = [row["peak_process_memory_bytes"] for row in records
             if row["peak_process_memory_bytes"] is not None]
    return {"recorded_commands": len(records), "measured_commands": len(peaks),
            "unavailable_commands": len(records) - len(peaks),
            "maximum_reported_rss_bytes": max(peaks) if peaks else None,
            "measurement_descriptions": sorted({row["memory_measurement"] for row in records})}


def record_memory(report):
    all_commands = command_memory(report)
    report["peak_process_memory_bytes"] = all_commands["maximum_reported_rss_bytes"]
    report["memory_measurement"] = (
        "maximum of per-command wait4 ru_maxrss observations; Linux KiB converted to bytes, "
        "macOS bytes; OS-reported RSS for each waited child, not simultaneous process-tree RSS "
        "or the sum of command peaks; Windows measurements unavailable")
    report["memory_summary"] = {"all_commands": all_commands, **{
        name: command_memory(report.get(name, []))
        for name in ("builds", "runs", "cleanup_validation", "timings", "sanitizers")}}



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path,
                        default=ROOT / "compiler/target/release" / ("yavo" + EXE))
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--oracle-only", action="store_true")
    parser.add_argument("--sanitizers", choices=("auto", "required", "off"), default="auto")
    parser.add_argument("--bootstrap-output", type=Path)
    parser.add_argument("--build-timeout", type=int, default=600)
    parser.add_argument("--case-timeout", type=int, default=15)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    report = {"schema": SCHEMA, "status": "running",
              "scope": SCOPE,
              "work": str(work), "host": {"platform": platform.platform(),
                                        "machine": platform.machine(),
                                        "python": sys.version, "cpu_count": os.cpu_count()},
              "builds": [], "runs": [], "checks": [], "timings": [],
              "sanitizers": {"status": "not_run", "passed": None},
              "peak_process_memory_bytes": None,
              "memory_measurement": "no commands executed yet",
              "model_usage": None, "monetary_cost": None,
              "compiler_built_by_runner": False,
              "application_source_inspected_by_oracle": False}
    bound = {}
    environment = dict(os.environ)
    for key in ("YAVO_NO_CACHE", "YAVO_NO_PCH", "YAVO_CACHE_TRACE", "YAVO_CACHE_DIR",
                "ZIG_GLOBAL_CACHE_DIR", "ZIG_LOCAL_CACHE_DIR", "ZIG_LIB_DIR"):
        environment.pop(key, None)

    def flush():
        record_memory(report)
        support.save(work / "receipt.json", report)
        support.save(output / "receipt.json", report)

    def check(name, passed, **details):
        report["checks"].append({"id": name, "passed": passed, "required": True, **details})

    flush()
    try:
        cases, frozen, review = support.frozen_oracle()
        bound.update(frozen)
        report["oracle_review"] = review
        gate_inputs = support.input_identities(include_app=True)
        for name, wanted in gate_inputs.items():
            if support.digest(Path(name)) != wanted:
                raise ValueError(APP_LABEL + " gate input changed before binding")
            bind(bound, name)
        report["not_applicable"] = [{"id": case["id"], "platforms": case["platforms"],
                                    "status": "not_applicable", "passed": None}
                                   for case in cases if os.name not in case.get("platforms", [os.name])]
        cases = [case for case in cases if os.name in case.get("platforms", [os.name])]
        if not cases:
            raise ValueError("no " + APP_LABEL + " application cases apply to this host")
        report["applicable_scenarios"] = len(cases)
        if args.oracle_only:
            changed = changed_inputs(bound)
            check("inputs_unchanged", not changed, changed=changed)
            report.update(status="passed" if not changed else "failed",
                          validation_mode="oracle-only", input_identities=bound,
                          application_executions=0, build_attempts=0)
            flush()
            print(json.dumps({"status": report["status"], "oracle_scenarios": review["case_count"],
                              "application_executions": 0, "receipt": str(output / "receipt.json")}))
            return 0 if report["status"] == "passed" else 1
        if "YAVO_CC" in os.environ:
            raise ValueError("YAVO_CC override is forbidden for native acceptance")
        compiler = args.compiler.resolve()
        if not compiler.is_file():
            raise ValueError("the requested prebuilt stage0 compiler does not exist")
        if {path.name for path in APP.glob("*.yv")} != set(MODULES):
            raise ValueError("frozen application source module inventory changed")
        contents = {name: (APP / name).read_bytes() for name in MODULES}
        for name, data in contents.items():
            if support.sha(data) != bound[str((APP / name).resolve())]:
                raise ValueError("application source changed before snapshot")
        report["compiler"] = bind(bound, compiler)
        report["runtime"] = bind(bound, ROOT / "compiler/runtime/yavo_rt.h")
        for path in [ROOT / "compiler/Cargo.lock", ROOT / "compiler/Cargo.toml",
                     *sorted((ROOT / "compiler/src").glob("*.rs"))]:
            bind(bound, path)
        zig, version, locked_archive = support.locked_zig()
        report["toolchain"] = {"executable": bind(bound, zig), "version": version,
                               "lock": bind(bound, ROOT / "compiler/toolchain.lock"),
                               "locked_archive": locked_archive,
                               "automatic_download": False,
                               "offline_scope": "preinstalled locked Zig only; driver never invokes installation"}
        version_run, version_out, _err = support.execute(
            [zig, "version"], work / "toolchain-version", ROOT, environment, {}, 15)
        report["toolchain"]["version_command"] = version_run
        if not completed(version_run) or version_out.decode("utf-8").strip() != version:
            raise ValueError("installed Zig version differs from the lock")
        for name, argv in (("head", ["git", "rev-parse", "HEAD"]),
                           ("status", ["git", "status", "--porcelain"])):
            record, out, _err = support.execute(argv, work / "git" / name, ROOT, environment, {}, 15)
            if not completed(record):
                raise ValueError("cannot record repository " + name)
            report.setdefault("git", {})[name] = {"value": out.decode("utf-8", "replace").strip(),
                                                  "command": record}
        artifacts = {}
        for label, mode in (("debug", "debug"), ("release-a", "release"), ("release-b", "release")):
            print(APP_SLUG + ": building " + label, flush=True)
            row, binary, generated, overrides = native_build(
                compiler, work / (label + " space-目录"), mode, contents,
                environment, bound, args.build_timeout, label)
            report["builds"].append(row)
            flush()
            if not row["passed"]:
                raise ValueError("native application build failed: " + label)
            artifacts[label] = (binary, generated, overrides)
        for field in ("generated_c", "binary"):
            left, right = report["builds"][1][field], report["builds"][2][field]
            check("independent_release_" + field, left["sha256"] == right["sha256"],
                  left_sha256=left["sha256"], right_sha256=right["sha256"])

        report["cleanup_validation"] = {"status": "not_run"}
        cleanup_binary, _cleanup = cleanup_artifact(
            zig, artifacts["release-a"][1], work / "cleanup-checked",
            environment, artifacts["release-a"][2], bound, args.build_timeout,
            report["cleanup_validation"])
        artifacts["cleanup-checked"] = (cleanup_binary, None, artifacts["release-a"][2])
        absolute_root = work / "absolute-fixtures space-目录"
        signatures = {}
        for label in ("debug", "release-a", "cleanup-checked"):
            binary, _generated, overrides = artifacts[label]
            for case in cases:
                row, out, err = support.run_case(
                    binary, case, work / "runs" / label / case["id"],
                    environment, overrides, timeout=args.case_timeout,
                    absolute_root=absolute_root / case["id"])
                row["build"] = label
                signature = (row["command"]["exit_code"], support.sha(out), support.sha(err))
                if label == "debug":
                    signatures[case["id"]] = signature
                else:
                    key = "debug_release_equal" if label == "release-a" else "original_cleanup_equal"
                    row["checks"][key] = signatures[case["id"]] == signature
                    row["passed"] = all(row["checks"].values())
                bind_case_files(bound, row)
                report["runs"].append(row)
            print(APP_SLUG + ": " + label + " "
                  + str(sum(row["passed"] for row in report["runs"] if row["build"] == label))
                  + "/" + str(len(cases)), flush=True)
            flush()
        expected_ids = {case["id"] for case in cases}
        inventory = {label: [row["id"] for row in report["runs"] if row["build"] == label]
                     for label in ("debug", "release-a", "cleanup-checked")}
        complete = (len(report["builds"]) == 3 and len(report["runs"]) == 3 * len(cases)
                    and all(len(ids) == len(cases) and set(ids) == expected_ids
                            for ids in inventory.values())
                    and len(report["cleanup_validation"]["controls"]) == 2
                    and all(row["passed"] for row in report["cleanup_validation"]["controls"]))
        check("complete_native_inventory", complete,
              observed_scenarios_per_artifact={label: len(ids) for label, ids in inventory.items()},
              required_scenarios_per_artifact=len(cases))
        check("normal_and_cleanup_outputs", all(row["passed"] for row in report["runs"]))
        for mode in ("debug", "release"):
            print(APP_SLUG + ": measuring " + mode + " cache conditions", flush=True)
            report["timings"].append(build_timings(
                compiler, work / ("timing-" + mode + " space-目录"), mode, contents,
                environment, bound, args.build_timeout))
            flush()
        check("timing_inventory", len(report["timings"]) == 2
              and all(len(row["measurements"]) == 3 and row["status"] == "passed"
                      for row in report["timings"]))
        report["sanitizers"] = sanitizer.validate(
            args.sanitizers, artifacts["release-a"][1], compiler, args.bootstrap_output,
            cases, work / "sanitizers", environment, bound, absolute_root,
            build_timeout=args.build_timeout, case_timeout=args.case_timeout)
        required = args.sanitizers == "required" or report["sanitizers"]["status"] in ("passed", "failed")
        report["checks"].append({"id": "independent_sanitizers",
                                 "required": required,
                                 "passed": report["sanitizers"]["passed"],
                                 "status": report["sanitizers"]["status"]})
        check("source_inventory_unchanged", {path.name for path in APP.glob("*.yv")} == set(MODULES))
        check("gate_input_inventory_unchanged",
              set(support.input_identities(include_app=True)) == set(gate_inputs))
        changed = changed_inputs(bound)
        check("inputs_and_artifacts_unchanged", not changed, changed=changed)
        if not changed:
            report["fixture_archive"] = archive_fixture_inputs(
                work, [*report["runs"], *report["sanitizers"]["runs"]], absolute_root, bound)
        report["transient_caches_removed"] = remove_transient_caches(work)
        remaining_changes = changed_inputs(bound)
        check("retained_inputs_and_artifacts_unchanged", not remaining_changes, changed=remaining_changes)
        report["passed"] = sum(row["passed"] for row in report["runs"])
        report["failed"] = len(report["runs"]) - report["passed"]
        report["application_executions"] = len(report["runs"])
        report["normal_application_executions"] = 2 * len(cases)
        report["cleanup_application_executions"] = len(cases)
        report["sanitizer_application_executions"] = report["sanitizers"]["application_executions"]
        report["build_attempts"] = len(report["builds"])
        report["timing_build_attempts"] = sum(len(row["measurements"]) for row in report["timings"])
        report["status"] = ("passed" if all(row["passed"] is True for row in report["checks"]
                                             if row.get("required", True)) else "failed")
    except (OSError, ValueError, KeyError, TypeError) as error:
        report.update(status="failed", error=type(error).__name__ + ": " + str(error),
                      application_executions=len(report["runs"]), build_attempts=len(report["builds"]))
        print(report["error"], file=sys.stderr, flush=True)
    finally:
        try:
            removed = remove_transient_caches(work)
            report["transient_caches_removed"] = sorted(set(
                report.get("transient_caches_removed", []) + removed))
        except OSError as error:
            report.update(status="failed", cache_cleanup_error=type(error).__name__ + ": " + str(error))
        flush()
    report["input_identities"] = bound
    flush()
    print(json.dumps({key: report.get(key) for key in
                      ("status", "applicable_scenarios", "application_executions",
                       "build_attempts", "passed", "failed", "sanitizer_application_executions")}),
          flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
