"""CSV gate: both frontends, Debug/Release, and supplementary exit cleanup."""
from __future__ import annotations
import importlib.util
import os
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]


def _module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _support():
    return _module("csv_support", ROOT / "tools/ci/csv_support.py")


def inputs():
    return _support().input_identities()


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    support = _support()
    record = {"application": str(support.APP), "passed": False, "builds": {}, "scenarios": [],
              "scope": "ordinary stage0/selfhost Debug+Release and supplementary Release exit-check artifacts"}
    directory = Path(directory).resolve()
    try:
        cases, bound, review = support.frozen_oracle()
        bound.update(support.input_identities())
        directory.mkdir(parents=True, exist_ok=False)
        for path in (Path(selfhost), Path(stage0), Path(zig)):
            bound[str(path.resolve())] = support.digest(path)
        record.update(oracle=review, compilers={"selfhost": support.identity(selfhost),
                                              "stage0": support.identity(stage0)},
                      toolchain=support.identity(zig))
        if len(list(support.APP.glob("*.yv"))) < 4:
            raise ValueError("CSV application must retain at least four source modules")
        generated = directory / "selfhost.c"
        emitted, _, err = support.execute([selfhost, "emit-c", support.APP / "main.yv", "-o", generated],
                                          directory / "selfhost-emit", ROOT, frontend_environment,
                                          {"PATH": "", "YAVO_ALLOC_CHECK": "1"}, 60)
        record["emit"] = emitted
        if emitted["exit_code"] != 0 or emitted["timed_out"] or not generated.is_file():
            raise ValueError("selfhost could not emit CSV application: " + err.decode("utf-8", "replace"))
        record["generated_c"] = support.identity(generated)
        bound[str(generated)] = support.digest(generated)
        binaries, stage0_generated = {}, {}
        for mode in ("debug", "release"):
            build_dir = directory / "builds" / mode
            build_dir.mkdir(parents=True)
            candidate = build_dir / ("selfhost-csv-check" + exe)
            reference = build_dir / ("stage0-csv-check" + exe)
            reference_c = build_dir / "stage0.c"
            built, _, _ = support.execute([zig, "cc", *cc_flags[mode], "-I", ROOT / "compiler/runtime",
                                           generated, "-o", candidate], build_dir / "selfhost-command",
                                          ROOT, environment, {"YAVO_ALLOC_CHECK": "1"}, 600)
            release = ["--release"] if mode == "release" else []
            rebuilt, _, _ = support.execute([stage0, "build", support.APP / "main.yv", *release,
                                             "-o", reference, "--emit-c", reference_c],
                                            build_dir / "stage0-command", ROOT, environment,
                                            {"YAVO_ALLOC_CHECK": "1", "YAVO_NO_CACHE": "1"}, 600)
            record["builds"][mode] = {"selfhost": built, "stage0": rebuilt}
            if any(command["exit_code"] != 0 or command["timed_out"] for command in (built, rebuilt)):
                raise ValueError("CSV application build failed in " + mode)
            for label, binary in (("selfhost", candidate), ("stage0", reference)):
                if not binary.is_file():
                    raise ValueError("CSV build did not produce its executable")
                binaries[label + "-" + mode] = binary
                bound[str(binary)] = support.digest(binary)
            bound[str(reference_c)] = support.digest(reference_c)
            stage0_generated[mode] = reference_c
        record["stage0_generated_c"] = {mode: support.identity(path) for mode, path in stage0_generated.items()}
        # Reuse byte-bound controls; production generated C and runtime stay unchanged.
        cleanup_driver = _module("csv_cleanup_controls", ROOT / "tools/ci/test_summary.py")
        record["cleanup"] = {}
        for label, source in (("selfhost", generated), ("stage0", stage0_generated["release"])):
            place = directory / "cleanup" / label
            place.mkdir(parents=True)
            cleanup_record = {}
            binary, _, _ = cleanup_driver.cleanup_artifact(
                zig, source, place, environment, {"YAVO_ALLOC_CHECK": "1"}, cleanup_record, bound, 600)
            record["cleanup"][label] = cleanup_record["cleanup_validation"]
            binaries[label + "-cleanup-release"] = binary
        record["artifacts"] = {label: support.identity(path) for label, path in binaries.items()}
        passed = True
        for case in cases:
            if os.name not in case.get("platforms", [os.name]):
                record["scenarios"].append({"id": case["id"], "applicable": False,
                                            "reason": "fixture filename or stdin-error capability is host-specific"})
                continue
            row = {"id": case["id"], "applicable": True, "artifacts": {}}
            case_root = directory / "cases" / case["id"]
            absolute_root = case_root / "absolute inputs 資料"
            reference_signature, agrees = None, True
            for label, binary in binaries.items():
                place = case_root / label
                result, out, err = support.run_case(binary, case, place, environment,
                                                    {"PATH": "", "YAVO_ALLOC_CHECK": "1"},
                                                    absolute_root=absolute_root)
                signature = (result["command"]["exit_code"], out, err)
                if reference_signature is None:
                    reference_signature = signature
                agrees = agrees and signature == reference_signature
                row["artifacts"][label] = result
                passed = passed and result["passed"]
                # Preserve safe command/stream files; remove actual special filenames.
                shutil.rmtree(place / "inputs")
            if absolute_root.exists():
                shutil.rmtree(absolute_root)
            row["same_stdout_stderr_exit"] = agrees
            row["passed"] = agrees and all(value["passed"] for value in row["artifacts"].values())
            passed = passed and row["passed"]
            record["scenarios"].append(row)
        applicable = sum(row["applicable"] for row in record["scenarios"])
        changed = [path for path, wanted in bound.items()
                   if not Path(path).is_file() or support.digest(path) != wanted]
        record.update(applicable=applicable, ordinary_executions=applicable * 4,
                      cleanup_executions=applicable * 2, executions=applicable * 6,
                      inputs_unchanged=not changed, changed_inputs=changed,
                      bound_inputs=bound, passed=passed and applicable > 0 and not changed)
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        record["error"] = str(error)
    if directory.is_dir():
        support.save(directory / "result.json", record)
    return record
