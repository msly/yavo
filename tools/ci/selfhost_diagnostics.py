#!/usr/bin/env python3
"""Independent first-error JSON/snapshot acceptance for the Yavo frontend."""
from __future__ import annotations

import argparse
import copy
import errno
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import threading

sys.dont_write_bytecode = True
import csv_support as execution
import selfhost_driver as producers

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "selfhost/tests/diagnostics50"
SUPPLEMENT = FIXTURES / "supplemental"
PROMOTIONS = ROOT / "selfhost/tests/diagnostics52/promotions.json"
PROMOTED_IDS = {"unsupported_hex", "unsupported_hex_e_sign", "unsupported_separator"}
EXE = ".exe" if os.name == "nt" else ""
EXPECTED = {"linux": 98, "darwin": 98, "win32": 94}
EXPECTED_TEXT = {"linux": 80, "darwin": 80, "win32": 76}
SENTINEL = b"pre-existing checker neighbor\n"


def digest(path):
    return execution.digest(path)


def bind(bound, path):
    return producers.bind(bound, path)


def load_frozen(bound):
    freeze_path = FIXTURES / "freeze.json"
    freeze = json.loads(freeze_path.read_bytes())
    if freeze["schema"] != "yavo-diagnostics50-frozen-oracle/1":
        raise ValueError("unknown diagnostics freeze")
    for name, wanted in freeze["sha256"].items():
        path = (FIXTURES / name).resolve()
        if not path.is_relative_to(FIXTURES.resolve()) or digest(path) != wanted:
            raise ValueError("frozen diagnostic input changed: " + name)
        bind(bound, path)
    bind(bound, freeze_path)
    manifest = json.loads((FIXTURES / "cases.json").read_bytes())
    cases = manifest["cases"]
    if (manifest["schema"] != "yavo-selfhost-diagnostics-cases/1" or len(cases) != 98
            or len({case["id"] for case in cases}) != len(cases)
            or manifest["case_count"] != len(cases) or freeze["case_count"] != len(cases)):
        raise ValueError("frozen diagnostic case inventory changed")
    if manifest["contract_sha256"] != digest(FIXTURES / "CONTRACT.md"):
        raise ValueError("case contract identity mismatch")
    for host, wanted in EXPECTED.items():
        if sum(host in case["platforms"] for case in cases) != wanted or freeze["applicable"][host] != wanted:
            raise ValueError("frozen platform case inventory changed")
    spec = importlib.util.spec_from_file_location("diagnostics50_independent_oracle", FIXTURES / "oracle.py")
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    for case in cases:
        if not oracle.strict_equal(oracle.evaluate(case), case["expected"]):
            raise ValueError("authored expectation disagrees with independent oracle: " + case["id"])
        for item in case["files"]:
            oracle.payload(case, item["path"])
    return manifest, oracle


def apply_promotions(manifest, oracle, bound):
    """Apply explicit later semantics only after validating the original freeze."""
    record = json.loads(PROMOTIONS.read_bytes())
    rows = record.get("promotions", [])
    if (record.get("schema") != "yavo-diagnostics52-promotions/1"
            or record.get("batch") != 52 or record.get("source_bytes_unchanged") is not True
            or record.get("base_manifest") != "selfhost/tests/diagnostics50/cases.json"
            or record.get("base_manifest_sha256") != digest(FIXTURES / "cases.json")
            or record.get("base_freeze_sha256") != digest(FIXTURES / "freeze.json")
            or len(rows) != len(PROMOTED_IDS) or {row["id"] for row in rows} != PROMOTED_IDS):
        raise ValueError("diagnostic promotion inventory or frozen baseline changed")
    effective = copy.deepcopy(manifest)
    by_id = {case["id"]: case for case in effective["cases"]}
    for row in rows:
        case = by_id[row["id"]]
        canonical = json.dumps(case, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
        if (execution.sha(canonical) != row["before_case_sha256"]
                or not oracle.strict_equal(case["files"], row["files"])
                or not oracle.strict_equal(case["expected"], row["before_expected"])
                or not oracle.strict_equal(case["legacy_text"], row["before_legacy_text"])
                or case["kind"] != "json" or case["diagnostic"]["code"] != "U0001"):
            raise ValueError("diagnostic promotion precondition differs: " + row["id"])
        # Keep identity, arguments, platforms, source files and loaded graph.
        # Only the original unsupported outcome becomes an authored success.
        case.pop("diagnostic")
        case.pop("location")
        case["legacy_text"] = {"exit": 0, "stdout": "", "stderr": ""}
        expected = oracle.evaluate(case)
        if (expected["exit"] != 0
                or not oracle.strict_equal(expected, row["after_expected"])
                or not oracle.strict_equal(oracle.legacy(case), row["after_legacy_text"])):
            raise ValueError("diagnostic promotion success expectation differs: " + row["id"])
        case["expected"] = expected
    identity = bind(bound, PROMOTIONS)
    return effective, {"record": identity, "ids": sorted(PROMOTED_IDS),
                       "base_manifest_sha256": record["base_manifest_sha256"],
                       "source_bytes_unchanged": True}


def load_effective(bound):
    manifest, oracle = load_frozen(bound)
    manifest, promotions = apply_promotions(manifest, oracle, bound)
    return manifest, oracle, promotions


def oracle_controls(oracle, cases):
    chosen = next(case for case in cases if case["id"] == "module_type")
    wanted = oracle.evaluate(chosen)
    wire = lambda value: json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode() + b"\n"
    if oracle.check_output(wanted, wire(wanted["document"]), b"", wanted["exit"]):
        raise ValueError("independent oracle positive control failed")
    rows = []
    for name, change in [
        ("wrong_end", lambda doc: doc["diagnostics"][0]["primary"].__setitem__("end", 999)),
        ("wrong_root_hash", lambda doc: doc.__setitem__("source_sha256", "0" * 64)),
        ("wrong_module_hash", lambda doc: doc["sources"][1].__setitem__("sha256", "0" * 64)),
        ("wrong_source_order", lambda doc: doc["sources"].reverse()),
        ("wrong_count_type", lambda doc: doc.__setitem__("error_count", True)),
        ("extra_field", lambda doc: doc.__setitem__("extra", 0)),
        ("extra_diagnostic", lambda doc: doc["diagnostics"].append(copy.deepcopy(doc["diagnostics"][0]))),
    ]:
        value = copy.deepcopy(wanted["document"])
        change(value)
        rejected = bool(oracle.check_output(wanted, wire(value), b"", wanted["exit"]))
        rows.append({"id": name, "rejected": rejected})
    for name, data in [
        ("duplicate_key", b'{"schema":"x",' + wire(wanted["document"])[1:]),
        ("extra_stdout", wire(wanted["document"]) + b"garbage\n"),
        ("invalid_utf8", b"\xff\n"),
    ]:
        rows.append({"id": name, "rejected": bool(oracle.check_output(wanted, data, b"", wanted["exit"]))})
    if not all(row["rejected"] for row in rows) or len(rows) != 10:
        raise ValueError("independent oracle negative control failed")
    return rows


def load_supplement(bound):
    freeze = json.loads((SUPPLEMENT / "freeze.json").read_bytes())
    if freeze["schema"] != "yavo-diagnostics50-legacy-cli-freeze/1" or freeze["case_count"] != 8:
        raise ValueError("unknown legacy CLI supplement freeze")
    for name, wanted in freeze["sha256"].items():
        path = (SUPPLEMENT / name).resolve()
        if not path.is_relative_to(SUPPLEMENT.resolve()) or digest(path) != wanted:
            raise ValueError("frozen legacy CLI supplement changed: " + name)
        bind(bound, path)
    bind(bound, SUPPLEMENT / "freeze.json")
    manifest = json.loads((SUPPLEMENT / "cases.json").read_bytes())
    if (manifest["schema"] != "yavo-diagnostics50-legacy-cli-cases/1" or manifest["case_count"] != 8
            or len(manifest["cases"]) != 8 or len({case["id"] for case in manifest["cases"]}) != 8
            or manifest["contract_sha256"] != digest(SUPPLEMENT / "CONTRACT.md")
            or manifest["source"]["sha256"] != digest(SUPPLEMENT / "main.yv")
            or manifest["source"]["bytes"] != (SUPPLEMENT / "main.yv").stat().st_size):
        raise ValueError("legacy CLI supplement inventory changed")
    for host in EXPECTED:
        if freeze["applicable"][host] != 8 or sum(host in case["platforms"] for case in manifest["cases"]) != 8:
            raise ValueError("legacy CLI supplement platform inventory changed")
    spec = importlib.util.spec_from_file_location("diagnostics50_legacy_cli_oracle", SUPPLEMENT / "oracle.py")
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    return manifest, oracle


def formal_drivers(bootstrap_output, bound):
    drivers, bootstrap = producers.bootstrap_drivers(bootstrap_output, bound)
    receipt = json.loads(Path(bootstrap["receipt"]).read_bytes())
    manifest_path = ROOT / "selfhost/tests/cases.json"
    manifest = json.loads(manifest_path.read_bytes())["cases"]
    required = {case["id"] for case in manifest}
    found = {case["id"] for case in receipt["cases"]}
    if (required != found or len(receipt["cases"]) != len(required)
            or receipt["total"] != len(required) or receipt["passed"] != len(required)
            or not all(case["passed"] for case in receipt["cases"])
            or receipt["inputs"]["manifest_sha256"] != digest(manifest_path)):
        raise ValueError("bootstrap is not the current complete case inventory")
    bind(bound, manifest_path)
    bootstrap["complete_cases"] = len(required)
    return drivers, bootstrap


def inventory(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        name = path.relative_to(directory).as_posix()
        if path.is_symlink():
            result[name] = {"kind": "symlink", "target": os.readlink(path)}
        elif path.is_file():
            result[name] = {"kind": "file", "bytes": path.stat().st_size, "sha256": digest(path)}
        elif path.is_dir():
            result[name] = {"kind": "directory"}
        else:
            result[name] = {"kind": "special"}
    return result


class SnapshotBarrier:
    def __init__(self, case, directory, oracle):
        self.case, self.directory, self.oracle = case, directory, oracle
        self.stop = threading.Event()
        self.thread = None
        self.errors, self.mutations = [], []
        self.connected = False
        self.path = None
        if "fifo" in case.get("setup", {}):
            self.path = directory / case["setup"]["fifo"]
            self.path.unlink()
            os.mkfifo(self.path)

    def start(self):
        if self.path is None:
            return
        self.thread = threading.Thread(target=self._write, daemon=True)
        self.thread.start()

    def _write(self):
        fd = None
        try:
            while not self.stop.is_set():
                try:
                    fd = os.open(self.path, os.O_WRONLY | os.O_NONBLOCK)
                    break
                except OSError as error:
                    if error.errno != errno.ENXIO:
                        raise
                    self.stop.wait(0.005)
            if fd is None:
                return
            self.connected = True
            for item in self.case["setup"]["replacements"]:
                target = self.directory / item["path"]
                before = target.read_bytes()
                if before != self.oracle.payload(self.case, item["path"]):
                    raise ValueError("snapshot file changed before reader rendezvous")
                after = item["text"].encode()
                target.write_bytes(after)
                self.mutations.append({"path": item["path"], "bytes_before": len(before),
                                       "sha256_before": self.oracle.sha(before), "bytes_after": len(after),
                                       "sha256_after": self.oracle.sha(after),
                                       "fifo_reader_connected_before_mutation": True})
            data = self.oracle.payload(self.case, self.case["setup"]["fifo"])
            count = os.write(fd, data)
            if count != len(data):
                raise ValueError("short FIFO fixture write")
        except Exception as error:
            self.errors.append(repr(error))
        finally:
            if fd is not None:
                os.close(fd)

    def finish(self):
        self.stop.set()
        if self.thread is not None:
            self.thread.join(timeout=1)
            if self.thread.is_alive():
                self.errors.append("FIFO worker did not stop")
        if self.path is not None:
            if not self.connected or len(self.mutations) != len(self.case["setup"]["replacements"]):
                self.errors.append("source snapshot barrier was not reached")
        return {"applicable": self.path is not None, "reader_connected": self.connected,
                "mutations": self.mutations, "errors": self.errors}

    def archive_regular_file(self):
        if self.path is not None and self.path.exists():
            self.path.unlink()
            self.path.write_bytes(self.oracle.payload(self.case, self.case["setup"]["fifo"]))


def materialize(case, directory, oracle):
    directory.mkdir(parents=True)
    for item in case["files"]:
        path = directory / item["path"]
        if not path.resolve().is_relative_to(directory.resolve()):
            raise ValueError("fixture escapes isolated case directory")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(oracle.payload(case, item["path"]))
    if "directory" in case.get("setup", {}):
        (directory / case["setup"]["directory"]).mkdir()
    for name in ["temporary", "program-cache"]:
        path = directory / name
        path.mkdir()
        (path / "neighbor.txt").write_bytes(SENTINEL)
    return SnapshotBarrier(case, directory, oracle)


def unchanged_case(before, after, mutations):
    expected = copy.deepcopy(before)
    for item in mutations:
        expected[Path(item["path"]).as_posix()] = {
            "kind": "file", "bytes": item["bytes_after"], "sha256": item["sha256_after"]}
    return expected == after


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, default=ROOT / "compiler/target/ci/selfhost")
    parser.add_argument("--candidate", type=Path, help="preliminary only; no final-stage producer claim")
    parser.add_argument("--case", action="append", default=[], help="candidate mode only")
    parser.add_argument("--output", type=Path, default=ROOT / "compiler/target/ci/selfhost-diagnostics")
    parser.add_argument("--timeout", type=int, default=30)
    args = parser.parse_args()
    if args.case and args.candidate is None:
        parser.error("--case requires --candidate")
    if args.timeout < 1 or sys.platform not in EXPECTED:
        parser.error("positive timeout and a declared native platform are required")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="attempt-", dir=output))
    bound = {}
    report = {"schema": "yavo-selfhost-diagnostics-acceptance/1", "status": "running",
              "formal": args.candidate is None, "work": str(work), "platform": sys.platform,
              "machine": platform.machine(), "cases": [], "text_cases": [], "supplemental_cases": [], "commands": [],
              "checks": [], "excluded": [], "tokens": None, "cost": None, "compile_p50_seconds": None}
    def checkpoint():
        execution.save(work / "receipt.json", report)
        execution.save(output / "receipt.json", report)
    def check(name, passed, **detail):
        report["checks"].append({"id": name, "passed": bool(passed), **detail})
        checkpoint()
        if not passed:
            raise ValueError("required check failed: " + name)
    environment = dict(os.environ)
    for name in ["YAVO_CACHE_TRACE", "YAVO_DRIVER_PROBE_MODE"]:
        environment.pop(name, None)
    try:
        manifest, oracle, promotions = load_effective(bound)
        cases = manifest["cases"]
        report["promotions"] = promotions
        report["frozen_cases"] = len(cases)
        report["freeze"] = bind(bound, FIXTURES / "freeze.json")
        check("frozen_oracle", True, applicable=EXPECTED[sys.platform], source_expectations=len(cases))
        check("explicit_diagnostic_promotions", True, ids=promotions["ids"], source_bytes_unchanged=True)
        controls = oracle_controls(oracle, cases)
        check("oracle_controls", True, positive=True, negative_controls=controls)
        supplement, legacy_oracle = load_supplement(bound)
        report["supplemental_freeze"] = bind(bound, SUPPLEMENT / "freeze.json")
        check("frozen_legacy_cli_supplement", True, applicable=8, source_expectations=8)
        for source in [Path(__file__), ROOT / "tools/ci/csv_support.py", ROOT / "tools/ci/selfhost_driver.py",
                       ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock", ROOT / "selfhost/bootstrap.py",
                       *sorted((ROOT / "selfhost").glob("*.yv"))]:
            bind(bound, source)
        if args.candidate:
            drivers = [("candidate", args.candidate.resolve())]
            bind(bound, drivers[0][1])
        else:
            drivers, bootstrap = formal_drivers(args.bootstrap_output, bound)
            report["bootstrap"] = bootstrap
            check("actual_bootstrap_producers", True, complete_cases=bootstrap["complete_cases"])
        selected = []
        for case in cases:
            if sys.platform not in case["platforms"]:
                report["excluded"].append({"id": case["id"], "reason": "frozen POSIX-only case"})
            elif not args.case or case["id"] in args.case:
                selected.append(case)
        if args.case and set(args.case) != {case["id"] for case in selected}:
            raise ValueError("unknown or inapplicable candidate case selection")
        copies = []
        for label, source in drivers:
            target = work / "compiler copies 编译器" / label / ("yavo selfhost" + EXE)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            target.chmod(0o700)
            identity = bind(bound, target)
            if identity["sha256"] != digest(source):
                raise ValueError("compiler copy differs from producer")
            copies.append((label, target))
        report["drivers"] = [{"id": label, **bind(bound, source)} for label, source in copies]
        for label, compiler in copies:
            for case in selected:
                modes = ["json", "text"] if case["kind"] == "json" else ["json"]
                for mode in modes:
                    directory = work / "cases" / label / case["id"] / mode
                    barrier = materialize(case, directory, oracle)
                    before = inventory(directory)
                    command_id = label + "-" + case["id"] + "-" + mode
                    arguments = oracle.arguments(case, directory) if mode == "json" else ["check", oracle.spelling(case, directory)]
                    overrides = {"YAVO_HOME": str(directory / "missing-toolchain"), "YAVO_CC": "no-backend-for-check",
                                 "YAVO_CACHE_DIR": str(directory / "program-cache"), "YAVO_NO_CACHE": "1",
                                 "TMPDIR": str(directory / "temporary"), "TEMP": str(directory / "temporary"),
                                 "TMP": str(directory / "temporary")}
                    barrier.start()
                    try:
                        record, stdout, stderr = execution.execute([compiler, *arguments], work / "commands" / command_id,
                                                                   directory, environment, overrides, args.timeout)
                    finally:
                        snapshot = barrier.finish()
                    after = inventory(directory)
                    barrier.archive_regular_file()
                    failures = []
                    if record["timed_out"] or record["launch_error"]:
                        failures.append("command launch/timeout")
                    if mode == "json":
                        wanted = oracle.evaluate(case, directory)
                        failures += oracle.check_output(wanted, stdout, stderr, record["exit_code"])
                    else:
                        wanted = oracle.legacy(case, directory)
                        if (record["exit_code"] != wanted["exit"] or stdout != wanted["stdout"].encode()
                                or stderr != wanted["stderr"].encode()):
                            failures.append("legacy text output/exit differs")
                    if snapshot["errors"]:
                        failures.append("snapshot barrier: " + repr(snapshot["errors"]))
                    if not unchanged_case(before, after, snapshot["mutations"]):
                        failures.append("unexpected source/output/cache/temporary mutation")
                    row = {"id": case["id"], "driver": label, "mode": mode, "kind": case["kind"],
                           "command": record, "expected": wanted, "snapshot": snapshot,
                           "files_before": before, "files_after": after, "passed": not failures, "failures": failures}
                    report["commands"].append({"id": command_id, **record})
                    report["cases" if mode == "json" else "text_cases"].append(row)
                    checkpoint()
                    print(command_id + ": " + ("PASS" if not failures else "FAIL " + repr(failures)), flush=True)
        if not args.case:
            for label, compiler in copies:
                for case in supplement["cases"]:
                    directory = work / "supplemental" / label / case["id"]
                    directory.mkdir(parents=True)
                    (directory / case["input"]).write_bytes((SUPPLEMENT / "main.yv").read_bytes())
                    for name in ["temporary", "program-cache"]:
                        (directory / name).mkdir()
                        (directory / name / "neighbor.txt").write_bytes(SENTINEL)
                    before = inventory(directory)
                    overrides = {"YAVO_HOME": str(directory / "missing-toolchain"), "YAVO_CC": "no-backend-for-check",
                                 "YAVO_CACHE_DIR": str(directory / "program-cache"), "YAVO_NO_CACHE": "1",
                                 "TMPDIR": str(directory / "temporary"), "TEMP": str(directory / "temporary"),
                                 "TMP": str(directory / "temporary")}
                    command_id = label + "-legacy-" + case["id"]
                    record, stdout, stderr = execution.execute([compiler, *case["arguments"]],
                                                               work / "commands" / command_id,
                                                               directory, environment, overrides, args.timeout)
                    failures = legacy_oracle.check(case, record["exit_code"], stdout, stderr, directory)
                    if record["timed_out"] or record["launch_error"]:
                        failures.append("command launch/timeout")
                    after = inventory(directory)
                    wanted_files = copy.deepcopy(before)
                    if case["expected"].get("output") == "frozen_c":
                        reference = SUPPLEMENT / "expected.c"
                        wanted_files[case["output"]] = {"kind": "file", "bytes": reference.stat().st_size,
                                                       "sha256": digest(reference)}
                    if wanted_files != after:
                        failures.append("unexpected source/output/cache/temporary mutation")
                    row = {"id": case["id"], "driver": label, "mode": "legacy_cli", "command": record,
                           "expected": case["expected"], "files_before": before, "files_after": after,
                           "passed": not failures, "failures": failures}
                    report["commands"].append({"id": command_id, **record})
                    report["supplemental_cases"].append(row)
                    checkpoint()
                    print(command_id + ": " + ("PASS" if not failures else "FAIL " + repr(failures)), flush=True)
        report["applicable_cases_per_driver"] = len(selected)
        report["driver_count"] = len(copies)
        report["case_count"] = len(report["cases"])
        report["passed"] = sum(row["passed"] for row in report["cases"])
        report["failed"] = sum(not row["passed"] for row in report["cases"])
        report["text_case_count"] = len(report["text_cases"])
        report["text_passed"] = sum(row["passed"] for row in report["text_cases"])
        report["text_failed"] = sum(not row["passed"] for row in report["text_cases"])
        report["supplemental_applicable_per_driver"] = 8 if not args.case else 0
        report["supplemental_case_count"] = len(report["supplemental_cases"])
        report["supplemental_passed"] = sum(row["passed"] for row in report["supplemental_cases"])
        report["supplemental_failed"] = sum(not row["passed"] for row in report["supplemental_cases"])
        expected_text = sum(case["kind"] == "json" for case in selected) * len(copies)
        check("complete_case_inventory", len(report["cases"]) == len(selected) * len(copies)
              and len(report["text_cases"]) == expected_text)
        if not args.case:
            check("frozen_platform_inventory", len(selected) == EXPECTED[sys.platform]
                  and expected_text == EXPECTED_TEXT[sys.platform] * len(copies))
            check("legacy_cli_supplement", report["supplemental_case_count"] == 8 * len(copies)
                  and report["supplemental_failed"] == 0, passed_cases=report["supplemental_passed"])
        changed = [name for name, value in bound.items() if not Path(name).is_file() or digest(name) != value]
        check("inputs_unchanged_during_run", not changed, changed=changed)
        check("json_and_text_contracts", report["failed"] == report["text_failed"] == 0,
              json_passed=report["passed"], text_passed=report["text_passed"])
        report["status"] = "preliminary_passed" if args.candidate else "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = repr(error)
        print("FAIL: " + repr(error), flush=True)
    finally:
        # All normally completed FIFO cases were restored above. Also remove
        # only test-created special files if an unexpected harness exception hit.
        removed_special = []
        for path in work.rglob("*"):
            if not path.is_file() and not path.is_dir():
                removed_special.append(str(path.relative_to(work)))
                path.unlink()
        report["special_files_removed_after_error"] = removed_special
        report["input_identities"] = bound
        values = [record["peak_process_memory_bytes"] for record in report["commands"]]
        report["resources"] = {"commands": len(values), "commands_with_rss": sum(value is not None for value in values),
                               "max_observed_child_rss_bytes": max((value for value in values if value is not None), default=None),
                               "scope": "wait4 waited-child RSS on POSIX; unavailable on Windows; no simultaneous tree or P50 claim"}
        checkpoint()
    print(json.dumps({key: report.get(key) for key in ["status", "formal", "case_count", "passed", "failed",
                                                     "text_case_count", "text_passed", "text_failed",
                                                     "supplemental_case_count", "supplemental_passed",
                                                     "supplemental_failed", "error"]}), flush=True)
    print("Receipt: " + str(output / "receipt.json"), flush=True)
    return 0 if report["status"] in ["passed", "preliminary_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
