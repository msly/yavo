#!/usr/bin/env python3
"""Record reproducible frontend samples and a separate backend observation."""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile

import csv_support as execution

ROOT = Path(__file__).resolve().parents[2]
EXE = ".exe" if os.name == "nt" else ""


def host_snapshot():
    try:
        load = list(os.getloadavg())
    except (AttributeError, OSError):
        load = None
    return {"timestamp_utc": datetime.now(timezone.utc).isoformat(), "load_average": load}


def environment_qualification(env):
    # Keep this probe small: loading a complete /proc/cpuinfo on a large host can
    # itself inflate the Python launcher's historical RSS before the compiler.
    cpu = platform.processor() or env.get("PROCESSOR_IDENTIFIER") or None
    cpu_source = "platform.processor/PROCESSOR_IDENTIFIER"
    if platform.system() == "Linux":
        try:
            with open("/proc/cpuinfo", encoding="utf-8") as stream:
                for line in stream:
                    if line.startswith(("model name", "Hardware")):
                        cpu = line.partition(":")[2].strip()
                        cpu_source = "/proc/cpuinfo"
                        break
        except OSError:
            pass
    elif platform.system() == "Darwin":
        try:
            cpu = subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"],
                encoding="utf-8", timeout=3).strip()
            cpu_source = "sysctl machdep.cpu.brand_string"
        except (OSError, subprocess.SubprocessError):
            pass
    try:
        affinity = sorted(os.sched_getaffinity(0))
    except (AttributeError, OSError):
        affinity = None
    hosted = env.get("GITHUB_ACTIONS") == "true" and env.get("RUNNER_ENVIRONMENT") == "github-hosted"
    return {"status": "github_hosted_observation" if hosted else "unqualified",
            "github_hosted": hosted, "run_id": env.get("GITHUB_RUN_ID"),
            "run_attempt": env.get("GITHUB_RUN_ATTEMPT"), "head_sha": env.get("GITHUB_SHA"),
            "runner_os": env.get("RUNNER_OS"), "runner_arch": env.get("RUNNER_ARCH"),
            "runner_name": env.get("RUNNER_NAME"), "image_os": env.get("ImageOS"),
            "image_version": env.get("ImageVersion"), "cpu_model": cpu, "cpu_model_source": cpu_source,
            "cpu_count": os.cpu_count(), "affinity": affinity,
            "isolation_evidence": "separate GitHub job; no per-sample foreign-build audit" if hosted else "not demonstrated",
            "eligible_for_controlled_performance_trend": False}


def receipt_configuration(path):
    """Run only in the short-lived reader; a Full receipt can expand to >80 MB."""
    payload = path.read_bytes()
    receipt = json.loads(payload)
    if receipt["status"] != "passed":
        raise RuntimeError("metrics require a passing bootstrap")
    return {"work": receipt["work"], "zig": receipt["inputs"]["zig"]["path"],
            "receipt": {"path": str(path.resolve()), "bytes": len(payload),
                        "sha256": hashlib.sha256(payload).hexdigest()}}


def isolated_configuration(path):
    # Linux fork/exec can retain the launcher's historical RSS high-water mark.
    # Never parse the large validation receipt in the process that measures the
    # compiler. Deleting the parsed object or collecting garbage does not reset it.
    command = [sys.executable, str(Path(__file__).resolve()), "--receipt-config", str(path)]
    return json.loads(subprocess.check_output(command))


def distribution(rows):
    values = sorted(row["elapsed_seconds"] for row in rows)
    rss = [row["peak_process_memory_bytes"] for row in rows
           if row["peak_process_memory_bytes"] is not None]
    return {"samples": len(values), "p50_seconds": statistics.median(values),
            "p95_seconds": values[math.ceil(.95 * len(values)) - 1],
            "peak_rss_bytes": max(rss) if rss else None,
            "rss_observations": len(rss)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=5)
    args = parser.parse_args()
    if args.samples < 3:
        parser.error("at least three samples are required")
    config = isolated_configuration(args.bootstrap_output / "receipt.json")
    work = Path(config["work"])
    compiler = work / "stages/stage2" / ("yavo-selfhost" + EXE)
    source = work / "selfhost-src/main.yv"
    expected_c = work / "stages/stage3/yavo-selfhost.c"
    zig = Path(config["zig"])
    directory = Path(tempfile.mkdtemp(prefix="metrics-", dir=args.bootstrap_output.resolve()))
    inputs = {str(path): execution.digest(path) for path in
              [compiler, zig, expected_c, *sorted(source.parent.glob("*.yv"))]}
    inputs[config["receipt"]["path"]] = config["receipt"]["sha256"]
    env = dict(os.environ)
    # Bootstrap uses the allocation counter; metrics keep that same compiler
    # identity. These observations are not the no-counter small-program gate.
    overrides = {"YAVO_NO_CACHE": "1", "YAVO_ALLOC_CHECK": "1"}
    rows = []
    report = {"schema": "yavo-selfhost-metrics/1", "batch": json.loads(
        (ROOT / "tools/ci/validation-policy.json").read_bytes())["batch"],
        "host": {"system": platform.system(), "machine": platform.machine(), **host_snapshot()},
        "environment_qualification": environment_qualification(env),
        "source": str(source), "compiler": execution.identity(compiler),
        "generated_c": execution.identity(expected_c), "warmups": 1,
        "scope": "stage2 emit-c of the compiler itself; separate native backend observation",
        "cache": "Yavo frontend cache disabled; existing OS and Zig caches; not cold builds",
        "memory_scope": "OS wait4 RSS of the waited compiler child, not a process-tree sum; the measurement process does not parse the Full receipt; Python launcher floor remains; null where unavailable",
        "receipt_loading": "separate short-lived reader process; its RSS is not used as compiler RSS",
        "backend_scope": "single_observation_not_distribution",
        "bootstrap_receipt": config["receipt"],
        "formal_small_program_target": {"compile_p50_ms": 200, "stripped_linux_bytes": 250000,
                                        "evaluated_by_this_self_compile_probe": False},
        "inputs": inputs, "frontend_samples": rows}
    ok = True
    for sample in range(args.samples + 1):
        output = directory / ("sample-" + str(sample) + ".c")
        before = host_snapshot()
        row, _, _ = execution.execute([compiler, "emit-c", source, "-o", output],
            directory / ("emit-" + str(sample)), ROOT, env, overrides, 120)
        row["host_before"] = before
        row["host_after"] = host_snapshot()
        row["generated_c_bytes"] = output.stat().st_size if output.exists() else None
        row["matches_fixed_point"] = output.exists() and output.read_bytes() == expected_c.read_bytes()
        ok &= row["exit_code"] == 0 and not row["timed_out"] and row["matches_fixed_point"]
        if sample == 0:
            report["warmup"] = row
        else:
            rows.append(row)
    report["frontend"] = distribution(rows)
    binary = directory / ("yavo-selfhost" + EXE)
    command = [zig, "cc", "-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK",
               "-O2", "-s", "-I", ROOT / "compiler/runtime", output, "-o", binary]
    before = host_snapshot()
    backend, _, _ = execution.execute(command, directory / "backend", ROOT, env, overrides, 600)
    backend["host_before"] = before
    backend["host_after"] = host_snapshot()
    report["backend_observation"] = backend
    ok &= backend["exit_code"] == 0 and not backend["timed_out"] and binary.is_file()
    report["input_changes"] = [path for path, value in inputs.items()
                               if not Path(path).is_file() or execution.digest(path) != value]
    report["status"] = "passed" if ok and not report["input_changes"] else "failed"
    execution.save(directory / "receipt.json", report)
    execution.save(args.bootstrap_output / "metrics.json", report)
    print(json.dumps({"status": report["status"], "frontend": report["frontend"],
                      "generated_c_bytes": report["generated_c"]["bytes"],
                      "binary_bytes": report["compiler"]["bytes"],
                      "environment_qualification": report["environment_qualification"],
                      "backend_seconds": backend["elapsed_seconds"],
                      "backend_peak_rss_bytes": backend["peak_process_memory_bytes"]}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--receipt-config":
        print(json.dumps(receipt_configuration(Path(sys.argv[2]))))
    else:
        sys.exit(main())
