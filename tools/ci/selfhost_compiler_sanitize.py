"""Instrument the actual stage2 compiler once, then exercise its frontend.

Called by selfhost_sanitize.py after the independent runner's clean/UAF/overflow
probes pass. Every case is checked against both its manifest expectation and the
unmodified bootstrap stage2 result; emitted C must match its recorded bytes.
The compiler, its C input, bootstrap module files, runtime, GCC and sanitizer libraries
remain identity-bound. This is Linux ASan/UBSan, not TSan or LeakSanitizer.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import sys

import selfhost_diagnostics as diagnostic_acceptance


def clean_environment(sanitizer):
    environment = dict(os.environ)
    for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "CFLAGS", "CPPFLAGS", "LDFLAGS",
                "GCC_EXEC_PREFIX", "COMPILER_PATH", "LIBRARY_PATH", "CPATH",
                "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "YAVO_CC"):
        environment.pop(key, None)
    expected = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0",
                "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1", "LC_ALL": "C"}
    if sanitizer != expected:
        raise ValueError("compiler sanitizer environment differs from the proven probes")
    environment.update(expected, YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")
    return environment


def recorded_bytes(item):
    data = base64.b64decode(item["base64"], validate=True)
    if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
        raise ValueError("bootstrap stream bytes do not match their recorded identity")
    return data


def same_result(result, expected):
    return (not result["record"]["timed_out"] and not expected["timed_out"]
            and result["record"]["exit_code"] == expected["exit_code"]
            and result["stdout"] == recorded_bytes(expected["stdout"])
            and result["stderr"] == recorded_bytes(expected["stderr"]))


def require_command(record, argv):
    if (record.get("argv") != [str(arg) for arg in argv]
            or record.get("exit_code") != 0 or record.get("timed_out") is not False
            or recorded_bytes(record["stdout"]) != b""
            or recorded_bytes(record["stderr"]) != b""):
        raise ValueError("bootstrap stage command does not establish the required provenance")


def load_stage_inputs(bootstrap, receipt, bound):
    """Refuse an old SH-1 receipt or a substituted compiler, C file or bundle."""
    checks = {row["id"]: row for row in receipt["checks"]}
    required = ("self_compilation_fixed_point", "stage3_binary_reproduces_stage2")
    if (receipt.get("status") != "passed" or receipt.get("self_hosted") is not True
            or receipt.get("achieved") != "self-hosted-fixed-point"
            or any(checks.get(name, {}).get("passed") is not True for name in required)):
        raise ValueError("stage2 sanitizer requires a successful self-hosted fixed point")
    work = Path(receipt["work"]).resolve()
    if not work.is_relative_to((bootstrap.COMPILER / "target").resolve()):
        raise ValueError("bootstrap work directory escapes compiler/target")

    def bind(path, wanted):
        path = path.resolve()
        if not path.is_relative_to(work) or bootstrap.digest(path) != wanted:
            raise ValueError("bootstrap stage artifact identity mismatch: " + str(path))
        bound[str(path)] = wanted
        return path

    inputs, stages = receipt["inputs"], receipt["stages"]
    source = bind(Path(inputs["compiler_bundle"]["path"]), inputs["compiler_bundle"]["sha256"])
    parts = inputs["compiler_bundle"]["parts"]
    expected_parts = [{"file": part, "sha256": bootstrap.digest(bootstrap.SELFHOST / part)}
                      for part in bootstrap.COMPILER_PARTS]
    directory = Path(inputs["compiler_bundle"]["directory"]).resolve()
    # The compiler is a module program: every file it loads must be an exact
    # copy of the declared source, and the bundle path must be its entry file.
    if (parts != expected_parts or source != (directory / bootstrap.COMPILER_ENTRY).resolve()
            or source.stat().st_size != inputs["compiler_bundle"]["bytes"]
            or any(bind(directory / part, bootstrap.digest(bootstrap.SELFHOST / part)).read_bytes()
                   != (bootstrap.SELFHOST / part).read_bytes() for part in bootstrap.COMPILER_PARTS)):
        raise ValueError("compiler bundle is not the exact declared set of Yavo module files")
    stage1 = Path(inputs["subset_compiler"]["path"])
    paths = {}
    for name in ("stage2", "stage3"):
        directory = work / "stages" / name
        paths[name + "_c"] = bind(directory / "yavo-selfhost.c", stages[name + "_c"]["sha256"])
        paths[name] = bind(directory / ("yavo-selfhost" + bootstrap.EXE), stages[name + "_sha256"])
        if paths[name + "_c"].stat().st_size != stages[name + "_c"]["bytes"]:
            raise ValueError("bootstrap stage C length mismatch")
    if (paths["stage2_c"].read_bytes() != paths["stage3_c"].read_bytes()
            or paths["stage2"].read_bytes() != paths["stage3"].read_bytes()):
        raise ValueError("recorded stage2/stage3 fixed point no longer holds")
    require_command(stages["stage1_check"], [stage1, "check", source])
    require_command(stages["stage1_emit"], [stage1, "emit-c", source, "-o", paths["stage2_c"]])
    require_command(stages["stage2_emit"], [paths["stage2"], "emit-c", source, "-o", paths["stage3_c"]])
    for name in ("stage2", "stage3"):
        require_command(stages[name + "_build"],
                        [inputs["zig"]["path"], "cc", *bootstrap.CC_FLAGS["release"], "-I",
                         bootstrap.COMPILER / "runtime", paths[name + "_c"], "-o", paths[name]])
    return source, paths


def stress_inputs():
    """Bounded malformed inputs with fixed expectations, not generated oracles."""
    return [
        ("truncated_function", b"fn main( {\n", 1, "E0020"),
        ("unterminated_string", b'fn main() { println("' + b"x" * 131072, 1, "E0020"),
        ("invalid_character", 'fn main() {\n println("汉字")\n @\n}\n'.encode(), 1, "E0020"),
        ("embedded_nul", b"fn main() {\n\x00\n}\n", 1, "E0020"),
        ("deep_parentheses", b"fn main() { let x = " + b"(" * 300 + b"0" + b")" * 300 + b"\n}\n", 1, "U0001"),
        ("deep_blocks", b"fn main() {\n" + b"if true {\n" * 160 + b"}\n" * 161, 1, "U0001"),
        ("invalid_utf8", b"fn main() {\n\xff\n}\n", 3, None),
        ("long_unexpected_token", b"fn main() {}\n" + b"identifier" * 16384, 1, "E0020"),
    ]


def validate_json_diagnostics(bootstrap, compiler, stages, directory, command, bound, sanitizer_environment, detail):
    """Reuse the frozen oracle with an instrumented candidate bound to stage2 C."""
    detail.update(status="running", mode="instrumented_candidate", formal_frontend_acceptance=False,
                  source_stage2_c={"path": str(stages["stage2_c"]),
                                   "sha256": bootstrap.digest(stages["stage2_c"])},
                  instrumented_compiler={"path": str(compiler), "sha256": bootstrap.digest(compiler)},
                  sanitizer_environment=dict(sanitizer_environment))

    def bind(path, expected=None):
        path = Path(path).resolve()
        actual = bootstrap.digest(path)
        if ((expected is not None and actual != expected)
                or (str(path) in bound and bound[str(path)] != actual)):
            raise ValueError("JSON sanitizer input identity changed: " + str(path))
        bound[str(path)] = actual
        return {"path": str(path), "bytes": path.stat().st_size, "sha256": actual}

    try:
        if sys.platform != "linux":
            raise ValueError("instrumented JSON diagnostics require Linux")
        fixtures = bootstrap.SELFHOST / "tests/diagnostics50"
        freeze = json.loads((fixtures / "freeze.json").read_bytes())
        if freeze.get("schema") != "yavo-diagnostics50-frozen-oracle/1" or freeze.get("case_count") != 98:
            raise ValueError("unexpected frozen JSON diagnostic inventory")
        detail["freeze"] = bind(fixtures / "freeze.json")
        for name, expected in freeze["sha256"].items():
            path = (fixtures / name).resolve()
            if not path.is_relative_to(fixtures.resolve()):
                raise ValueError("frozen JSON fixture escapes its root")
            bind(path, expected)
        effective, _oracle, promotions = diagnostic_acceptance.load_effective(bound)
        cases = effective["cases"]
        detail["promotions"] = promotions
        expected_ids = {case["id"] for case in cases if "linux" in case["platforms"]}
        expected_text_ids = {case["id"] for case in cases if "linux" in case["platforms"] and case["kind"] == "json"}
        if len(cases) != 98 or len(expected_ids) != 98 or len(expected_text_ids) != 80:
            raise ValueError("frozen Linux JSON/text inventory changed")
        supplement = fixtures / "supplemental"
        supplemental_freeze = json.loads((supplement / "freeze.json").read_bytes())
        if (supplemental_freeze.get("schema") != "yavo-diagnostics50-legacy-cli-freeze/1"
                or supplemental_freeze.get("case_count") != 8
                or supplemental_freeze.get("applicable", {}).get("linux") != 8):
            raise ValueError("unexpected frozen legacy CLI supplement inventory")
        detail["supplemental_freeze"] = bind(supplement / "freeze.json")
        for name, expected in supplemental_freeze["sha256"].items():
            path = (supplement / name).resolve()
            if not path.is_relative_to(supplement.resolve()):
                raise ValueError("frozen legacy CLI supplement escapes its root")
            bind(path, expected)
        supplemental_cases = json.loads((supplement / "cases.json").read_bytes())["cases"]
        _supplement_manifest, legacy_oracle = diagnostic_acceptance.load_supplement(bound)
        detail["supplemental_product"] = legacy_oracle.product_upgrade
        expected_supplemental_ids = {case["id"] for case in supplemental_cases if "linux" in case["platforms"]}
        if len(supplemental_cases) != 8 or len(expected_supplemental_ids) != 8:
            raise ValueError("frozen Linux legacy CLI supplement inventory changed")
        harness = bootstrap.ROOT / "tools/ci/selfhost_diagnostics.py"
        for path in (harness, bootstrap.ROOT / "tools/ci/csv_support.py",
                     bootstrap.ROOT / "tools/ci/selfhost_driver.py", Path(sys.executable)):
            bind(path)
        output = directory / "json-diagnostics"
        checked = command("json-diagnostics", [sys.executable, harness, "--candidate", compiler,
                                               "--output", output], timeout=600)
        detail["command"] = checked["record"]
        receipt_path = output / "receipt.json"
        if not receipt_path.is_file():
            raise ValueError("instrumented JSON harness did not produce its receipt")
        candidate = json.loads(receipt_path.read_bytes())
        detail["receipt"] = bind(receipt_path)
        detail["candidate_status"] = candidate.get("status")
        detail["counts"] = {name: candidate.get(name) for name in
                            ("case_count", "passed", "failed", "text_case_count", "text_passed", "text_failed")}
        detail["supplemental_counts"] = {name: candidate.get(name) for name in
                                         ("supplemental_applicable_per_driver", "supplemental_case_count",
                                          "supplemental_passed", "supplemental_failed")}
        if (not bootstrap.succeeded(checked) or checked["stderr"] != b""
                or candidate.get("schema") != "yavo-selfhost-diagnostics-acceptance/1"
                or candidate.get("status") != "preliminary_passed" or candidate.get("formal") is not False
                or candidate.get("platform") != "linux" or candidate.get("driver_count") != 1
                or candidate.get("applicable_cases_per_driver") != 98
                or candidate.get("case_count") != candidate.get("passed") or candidate.get("passed") != 98
                or candidate.get("text_case_count") != candidate.get("text_passed") or candidate.get("text_passed") != 80
                or candidate.get("failed") != 0 or candidate.get("text_failed") != 0
                or candidate.get("supplemental_applicable_per_driver") != 8
                or candidate.get("supplemental_case_count") != 8 or candidate.get("supplemental_passed") != 8
                or candidate.get("supplemental_failed") != 0
                or candidate.get("freeze") != detail["freeze"]
                or candidate.get("promotions") != detail["promotions"]
                or candidate.get("supplemental_freeze") != detail["supplemental_freeze"]
                or candidate.get("supplemental_product") != detail["supplemental_product"]
                or candidate.get("excluded") != [] or candidate.get("special_files_removed_after_error") != []):
            raise ValueError("instrumented JSON candidate did not pass the complete frozen Linux gate")
        checks = candidate["checks"]
        check_names = {row["id"] for row in checks}
        required_checks = {"frozen_oracle", "oracle_controls", "complete_case_inventory",
                           "frozen_platform_inventory", "inputs_unchanged_during_run", "json_and_text_contracts",
                           "frozen_legacy_cli_supplement", "legacy_cli_supplement", "explicit_diagnostic_promotions",
                           "explicit_legacy_product_upgrade"}
        if (len(check_names) != len(checks) or not required_checks.issubset(check_names)
                or not all(row.get("passed") is True for row in checks)):
            raise ValueError("instrumented JSON candidate omitted or failed a required check")
        controls = next(row for row in checks if row["id"] == "oracle_controls")
        if (controls.get("positive") is not True or len(controls.get("negative_controls", [])) != 10
                or not all(row.get("rejected") is True for row in controls["negative_controls"])):
            raise ValueError("instrumented JSON oracle capability controls failed")
        drivers = candidate["drivers"]
        if (len(drivers) != 1 or drivers[0].get("id") != "candidate"
                or drivers[0]["sha256"] != detail["instrumented_compiler"]["sha256"]):
            raise ValueError("JSON cases did not use the instrumented compiler")
        detail["executed_compiler"] = bind(drivers[0]["path"], drivers[0]["sha256"])
        snapshots = []
        expected_commands = {}
        for name, expected, mode in (("cases", expected_ids, "json"),
                                     ("text_cases", expected_text_ids, "text"),
                                     ("supplemental_cases", expected_supplemental_ids, "legacy_cli")):
            rows = candidate[name]
            if len(rows) != len(expected) or {row["id"] for row in rows} != expected:
                raise ValueError("instrumented JSON/text/supplemental case identities differ")
            for row in rows:
                record = row["command"]
                if (row.get("passed") is not True or row["driver"] != "candidate" or row["mode"] != mode
                        or record["argv"][0] != drivers[0]["path"] or record.get("timed_out")
                        or record.get("launch_error")
                        or any(key in record["environment"] and record["environment"][key] != value
                               for key, value in sanitizer_environment.items())):
                    raise ValueError("instrumented JSON/text/supplemental execution identity or environment differs")
                command_id = ("candidate-legacy-" + row["id"] if mode == "legacy_cli"
                              else "candidate-" + row["id"] + "-" + mode)
                expected_commands[command_id] = {"id": command_id, **record}
                for stream in ("stdout", "stderr"):
                    bind(record[stream]["path"], record[stream]["sha256"])
                if row.get("snapshot", {}).get("applicable"):
                    if not row["snapshot"]["reader_connected"] or row["snapshot"]["errors"]:
                        raise ValueError("instrumented source snapshot barrier did not complete")
                    snapshots.append({"id": row["id"], "mode": row["mode"], **row["snapshot"]})
        if (len(candidate["commands"]) != 186 or len(expected_commands) != 186
                or {row["id"]: row for row in candidate["commands"]} != expected_commands):
            raise ValueError("instrumented JSON/text/supplemental command inventory is incomplete")
        fifo_ids = {case["id"] for case in cases if "fifo" in case.get("setup", {})}
        if ({(row["id"], row["mode"]) for row in snapshots}
                != {(name, mode) for name in fifo_ids for mode in ("json", "text")}):
            raise ValueError("instrumented JSON snapshot inventory is incomplete")
        for path, expected in candidate["input_identities"].items():
            bind(path, expected)
        required_inputs = {str(harness.resolve()), str(compiler.resolve()), str((fixtures / "freeze.json").resolve()),
                           str(diagnostic_acceptance.PROMOTIONS.resolve()),
                           str(diagnostic_acceptance.PROMOTIONS53.resolve()),
                           str(diagnostic_acceptance.PRODUCT53.resolve()),
                           str(diagnostic_acceptance.PRODUCT53_C.resolve()),
                           str((supplement / "freeze.json").resolve()),
                           *[str((bootstrap.SELFHOST / part).resolve()) for part in bootstrap.COMPILER_PARTS],
                           *[str((fixtures / name).resolve()) for name in freeze["sha256"]],
                           *[str((supplement / name).resolve()) for name in supplemental_freeze["sha256"]]}
        if not required_inputs.issubset(candidate["input_identities"]):
            raise ValueError("instrumented JSON receipt lacks source or fixture bindings")
        detail.update(status="passed", checks=checks, snapshot_barriers=snapshots,
                      bound_inputs=len(candidate["input_identities"]), total_commands=len(candidate["commands"]))
    except Exception as error:
        detail.update(status="failed", error=str(error))
        raise


def validate(bootstrap, receipt, cc, runtime, sanitizer, directory, bound):
    directory.mkdir(parents=True, exist_ok=False)
    manifest = bootstrap.validate_cases()
    stress = stress_inputs()
    report = {"schema": "yavo-selfhost-compiler-sanitizer-validation/1", "status": "running",
              "scope": "GCC-instrumented compiler built from the actual bootstrap stage2 C",
              "validator": {"path": str(Path(__file__).resolve()),
                            "sha256": bootstrap.digest(Path(__file__))},
              "cases": [], "stress": [], "case_total": len(manifest),
              "emission_total": sum(case["kind"] in ("run", "panic") for case in manifest),
              "stress_total": len(stress), "case_passed": 0, "emission_passed": 0,
              "stress_passed": 0, "self_compilation_passed": False,
              "json_diagnostics_passed": False, "json_diagnostics": {"status": "not_run"},
              "limits": ["Linux ASan/UBSan only; no TSan.",
                         "LeakSanitizer disabled; successful compiler exits use YV_ALLOC_CHECK.",
                         "Invalid-source paths terminate at their first diagnostic; heap leak checks do not run after os.exit.",
                         "Malformed inputs are a fixed bounded stress corpus, not fuzzing."]}

    def save():
        bootstrap.save(directory / "receipt.json", report)

    save()
    try:
        source, stages = load_stage_inputs(bootstrap, receipt, bound)
        identity = sanitizer.get("c_compiler", {})
        capabilities = sanitizer.get("capabilities", {})
        flags = sanitizer.get("flags", [])
        required_flags = {"-fsanitize=address", "-fsanitize=undefined", "-fno-sanitize-recover=all",
                          "-DYV_ALLOC_CHECK=1"}
        if (sanitizer.get("status") != "passed"
                or identity.get("path") != cc or identity.get("sha256") != bootstrap.digest(Path(cc))
                or set(capabilities) != {"clean", "address", "undefined"}
                or not all(probe.get("passed") is True for probe in capabilities.values())
                or not required_flags.issubset(flags)):
            raise ValueError("matching GCC ASan/UBSan capability proof is required before compiler execution")
        libraries = identity.get("runtime_libraries", {})
        if not {"libasan.so", "libubsan.so"}.issubset(libraries):
            raise ValueError("sanitizer runtime identities are missing")
        for name, library in libraries.items():
            if "path" in library:
                path = Path(library["path"])
                if bootstrap.digest(path) != library["sha256"]:
                    raise ValueError("sanitizer runtime library changed: " + name)
                bound[str(path.resolve())] = library["sha256"]
            elif name in ("libasan.so", "libubsan.so"):
                raise ValueError("sanitizer runtime identity is unresolved: " + name)
        environment = clean_environment(sanitizer.get("sanitizer_environment"))
        frontend_environment = dict(environment, PATH="")
        report.update(inputs={"source": {"path": str(source), "sha256": bootstrap.digest(source)},
                              "stage2_c": {"path": str(stages["stage2_c"]), "sha256": bootstrap.digest(stages["stage2_c"])},
                              "bootstrap_stage2": {"path": str(stages["stage2"]), "sha256": bootstrap.digest(stages["stage2"])},
                              "runtime": {"path": str(runtime), "sha256": bootstrap.digest(runtime)}},
                      c_compiler=identity, flags=flags, capabilities=capabilities,
                      sanitizer_environment=sanitizer["sanitizer_environment"])

        def command(label, argv, frontend=True, timeout=30):
            return bootstrap.run(argv, directory / "commands" / label,
                                 frontend_environment if frontend else environment, timeout)

        # Quoted includes search beside the C file before -I. Copy both verified
        # inputs into a fresh directory so an unbound adjacent header cannot win.
        build_directory = directory / "build"
        build_directory.mkdir()
        copied_c, copied_runtime = build_directory / "stage2.c", build_directory / "yavo_rt.h"
        copies = []
        for original, copied, wanted in (
                (stages["stage2_c"], copied_c, receipt["stages"]["stage2_c"]["sha256"]),
                (runtime, copied_runtime, receipt["inputs"]["runtime_sha256"])):
            copied.write_bytes(original.read_bytes())
            if bootstrap.digest(copied) != wanted:
                raise ValueError("compiler build input copy changed bytes")
            bound[str(copied)] = wanted
            copies.append({"source_path": str(original), "source_sha256": wanted,
                           "copy_path": str(copied), "copy_sha256": wanted,
                           "bytes": copied.stat().st_size})
        report["isolated_build_inputs"] = copies
        compiler = build_directory / "yavo-stage2-sanitized"
        built = command("build-stage2", [cc, *flags, "-I", build_directory,
                                        copied_c, "-lm", "-o", compiler],
                        frontend=False, timeout=600)
        report["build"] = built["record"]
        if not bootstrap.succeeded(built) or not compiler.is_file():
            raise RuntimeError("GCC could not build the instrumented stage2 compiler")
        compiler_hash = bootstrap.digest(compiler)
        bound[str(compiler)] = compiler_hash
        report["artifact"] = {"path": str(compiler), "sha256": compiler_hash,
                              "bytes": compiler.stat().st_size}
        checked = command("check-self", [compiler, "check", source], timeout=120)
        generated = directory / "self-compiled.c"
        emitted = command("emit-self", [compiler, "emit-c", source, "-o", generated], timeout=120)
        self_ok = (same_result(checked, receipt["stages"]["stage1_check"])
                   and same_result(emitted, receipt["stages"]["stage2_emit"])
                   and generated.is_file() and generated.read_bytes() == stages["stage3_c"].read_bytes())
        report["self_compilation"] = {"check": checked["record"], "emit": emitted["record"],
                                      "passed": self_ok, "c_bytes_equal": self_ok}
        if generated.is_file():
            report["self_compilation"]["c_sha256"] = bootstrap.digest(generated)
            bound[str(generated)] = bootstrap.digest(generated)
        report["self_compilation_passed"] = self_ok
        save()
        if not self_ok:
            raise RuntimeError("instrumented stage2 self-compilation differs or has sanitizer diagnostics")

        rows = {row["id"]: row for row in receipt["cases"]}
        original_work = Path(receipt["work"])
        (directory / "programs").mkdir()
        for case in manifest:
            name, kind = case["id"], case["kind"]
            baseline = rows[name]
            original_directory = original_work / "programs" / name
            fixture = (original_directory / "input.yv" if "source_prefix" in case
                       else bootstrap.SELFHOST / "tests" / case["file"])
            if bootstrap.digest(fixture) != baseline["source_sha256"]:
                raise ValueError("stage2 fixture identity changed: " + name)
            bound[str(fixture)] = baseline["source_sha256"]
            reference_check = baseline["stage2_check"]
            if reference_check["argv"] != [str(stages["stage2"]), "check", str(fixture)]:
                raise ValueError("case check was not performed by the recorded stage2: " + name)
            checked = command(name + "-check", [compiler, "check", fixture])
            executable = kind in ("run", "panic")
            code = 0 if executable else 1
            expected_diagnostic = case.get("diagnostic", "U0001" if kind in ("unsupported", "shared_unsupported") else None)
            codes = bootstrap.codes(checked)
            independent = (checked["record"]["exit_code"] == code and checked["stdout"] == b""
                           and (checked["stderr"] == b"" if executable else
                                bool(bootstrap.LOCATION.search(checked["stderr"]))
                                and (expected_diagnostic in codes if expected_diagnostic else
                                     any(value.startswith("E") for value in codes) and "U0001" not in codes)))
            good = independent and same_result(checked, reference_check)
            result = {"id": name, "kind": kind, "source_sha256": bootstrap.digest(fixture),
                      "check": checked["record"], "independent_expectation_matches": independent,
                      "bootstrap_check_matches": same_result(checked, reference_check), "passed": good}
            if executable:
                reference_c = original_directory / "stage2.c"
                require_command(baseline["stage2_emit"],
                                [stages["stage2"], "emit-c", fixture, "-o", reference_c])
                if bootstrap.digest(reference_c) != baseline["generated_c_sha256"]:
                    raise ValueError("recorded stage2 case C identity changed: " + name)
                bound[str(reference_c)] = baseline["generated_c_sha256"]
                generated = directory / "programs" / (name + ".c")
                emitted = command(name + "-emit", [compiler, "emit-c", fixture, "-o", generated])
                emits_ok = (same_result(emitted, baseline["stage2_emit"]) and generated.is_file()
                            and generated.read_bytes() == reference_c.read_bytes())
                result.update(emit=emitted["record"], c_bytes_equal=emits_ok, passed=good and emits_ok)
                if generated.is_file():
                    result["c_sha256"] = bootstrap.digest(generated)
                    bound[str(generated)] = result["c_sha256"]
                report["emission_passed"] += int(emits_ok)
            report["cases"].append(result)
            report["case_passed"] += int(result["passed"])
            if len(report["cases"]) % 25 == 0:
                save()
        (directory / "stress-inputs").mkdir()
        for name, data, exit_code, diagnostic in stress:
            source = directory / "stress-inputs" / (name + ".yv")
            source.write_bytes(data)
            bound[str(source)] = bootstrap.digest(source)
            checked = command("stress-" + name + "-check", [compiler, "check", source])
            generated = directory / "stress-inputs" / (name + ".c")
            emitted = command("stress-" + name + "-emit", [compiler, "emit-c", source, "-o", generated])
            ok = (not checked["record"]["timed_out"] and not emitted["record"]["timed_out"]
                  and checked["record"]["exit_code"] == emitted["record"]["exit_code"] == exit_code
                  and checked["stdout"] == emitted["stdout"] == b""
                  and checked["stderr"] == emitted["stderr"]
                  and not generated.exists())
            if diagnostic:
                ok = (ok and bootstrap.codes(checked) == [diagnostic]
                      and bool(bootstrap.LOCATION.search(checked["stderr"])))
            else:
                ok = ok and checked["stderr"] == b"cannot read input\n"
            # Exact one-line frontend rejection excludes additional sanitizer output.
            if diagnostic:
                ok = ok and len(checked["stderr"].splitlines()) == 1
            report["stress"].append({"id": name, "input_sha256": bootstrap.digest(source),
                                     "input_bytes": len(data), "expected_exit": exit_code,
                                     "expected_diagnostic": diagnostic, "check": checked["record"],
                                     "emit": emitted["record"], "passed": bool(ok)})
            report["stress_passed"] += int(bool(ok))
        validate_json_diagnostics(bootstrap, compiler, stages, directory, command, bound,
                                  sanitizer["sanitizer_environment"], report["json_diagnostics"])
        report["json_diagnostics_passed"] = report["json_diagnostics"]["status"] == "passed"
        changed = [path for path, wanted in bound.items()
                   if not Path(path).is_file() or bootstrap.digest(Path(path)) != wanted]
        report["changed_inputs_and_outputs"] = changed
        report["status"] = ("passed" if report["case_passed"] == report["case_total"]
                            and report["emission_passed"] == report["emission_total"]
                            and report["stress_passed"] == report["stress_total"]
                            and report["json_diagnostics_passed"] and not changed else "failed")
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
    report["case_not_run"] = report["case_total"] - len(report["cases"])
    report["emission_not_run"] = report["emission_total"] - sum("emit" in row for row in report["cases"])
    report["stress_not_run"] = report["stress_total"] - len(report["stress"])
    save()
    print("Stage2 compiler sanitizers: " + report["status"] + "; checks "
          + str(report["case_passed"]) + "/" + str(report["case_total"])
          + "; emissions " + str(report["emission_passed"]) + "/" + str(report["emission_total"])
          + "; stress " + str(report["stress_passed"]) + "/" + str(report["stress_total"])
          + "; JSON diagnostics " + report["json_diagnostics"]["status"], flush=True)
    return report
