#!/usr/bin/env python3
"""Compare PCH-backed and direct C builds on identical edited Yavo sources.

Runs with Python's standard library. Both arms receive the same modified source
bytes and independent build-cache roots. Different numbers of leading comments
force fresh generated C for every sample. The execution order alternates.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CASES = ("integer_recurrence", "vec_transform", "mutref_scatter", "deque_ring")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(compiler, source, out, cache, *, pch):
    env = os.environ.copy()
    env["YAVO_CACHE_DIR"] = str(cache)
    env["YAVO_CACHE_TRACE"] = "1"
    env.pop("YAVO_CC", None)
    env.pop("YAVO_NO_CACHE", None)
    env.pop("YAVO_ALLOC_CHECK", None)
    env.pop("YAVO_PCH_MAX_ENTRIES", None)
    env.pop("YAVO_CACHE_MAX_ENTRIES", None)
    if pch:
        env.pop("YAVO_NO_PCH", None)
    else:
        env["YAVO_NO_PCH"] = "1"
    start = time.perf_counter_ns()
    result = subprocess.run([str(compiler), "build", str(source), "-o", str(out), "--release"],
                            env=env, capture_output=True, text=True, timeout=120, check=False)
    elapsed = (time.perf_counter_ns() - start) / 1_000_000
    if result.returncode != 0 or "build cache miss" not in result.stderr:
        raise RuntimeError(f"invalid {source.name} build: {result.returncode} {result.stderr}")
    if pch and "runtime pch hit" not in result.stderr:
        raise RuntimeError(f"PCH was not reused for {source.name}: {result.stderr}")
    return round(elapsed, 3), sha256(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.samples < 3:
        parser.error("need at least three samples")
    compiler = ROOT / "compiler/target/release/yavo"
    target = ROOT / f"compiler/target/pch-latency-probe-{os.getpid()}-{time.time_ns()}"
    target.mkdir(parents=True, exist_ok=True)
    results = {}
    for case in CASES:
        original = (Path(__file__).parent / f"{case}.yv").read_text()
        base = target / case
        base.mkdir(exist_ok=True)
        source = base / f"{case}.yv"
        arms = {m: {"cache": base / f"{m}-cache", "out": base / f"{m}-bin", "ms": []}
                for m in ("pch", "plain")}
        # Unmeasured warm-up: establish the PCH and the direct build's own Zig cache.
        source.write_text("// warmup\n" + original)
        for mode, arm in arms.items():
            env = os.environ.copy()
            env.update(YAVO_CACHE_DIR=str(arm["cache"]), YAVO_CACHE_TRACE="1")
            env.pop("YAVO_CC", None)
            env.pop("YAVO_NO_CACHE", None)
            env.pop("YAVO_ALLOC_CHECK", None)
            env.pop("YAVO_PCH_MAX_ENTRIES", None)
            env.pop("YAVO_CACHE_MAX_ENTRIES", None)
            if mode == "plain":
                env["YAVO_NO_PCH"] = "1"
            else:
                env.pop("YAVO_NO_PCH", None)
            run = subprocess.run([str(compiler), "build", str(source), "-o", str(arm["out"]), "--release"],
                                 env=env, capture_output=True, text=True, timeout=120)
            if run.returncode or (mode == "pch" and "runtime pch" not in run.stderr):
                raise RuntimeError(f"warm-up {case}/{mode}: {run.stderr}")
        for i in range(args.samples):
            source.write_text("// touched\n" * (i + 2) + original)
            hashes = []
            order = ("pch", "plain") if i % 2 else ("plain", "pch")
            for mode in order:
                arm = arms[mode]
                ms, digest = build(compiler, source, arm["out"], arm["cache"], pch=mode == "pch")
                arm["ms"].append(ms)
                hashes.append(digest)
            if hashes[0] != hashes[1]:
                raise RuntimeError(f"{case} sample {i}: compiled binaries differ")
        pch_ms, plain_ms = arms["pch"]["ms"], arms["plain"]["ms"]
        results[case] = {"pch_ms": pch_ms, "direct_ms": plain_ms,
                         "pch_p50_ms": round(statistics.median(pch_ms), 3),
                         "direct_p50_ms": round(statistics.median(plain_ms), 3),
                         "speedup_direct_over_pch": round(statistics.median(plain_ms) / statistics.median(pch_ms), 4),
                         "binary_sha256_equal_on_all_samples": True}
    report = {"schema": "yavo-runtime-pch-latency/1",
              "measured_at_utc": datetime.now(timezone.utc).isoformat(),
              "host": platform.platform(), "sample_count_per_arm_per_case": args.samples,
              "mode": "release, edited sources, independent backend caches, alternating execution order",
              "script_sha256": sha256(Path(__file__)),
              "compiler_sha256": sha256(compiler),
              "runtime_header_sha256": sha256(ROOT / "compiler/runtime/yavo_rt.h"),
              "benchmark_input_sha256": {case: sha256(Path(__file__).parent / f"{case}.yv") for case in CASES},
              "results": results}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({k: {"pch_p50_ms": v["pch_p50_ms"],
                          "direct_p50_ms": v["direct_p50_ms"],
                          "speedup": v["speedup_direct_over_pch"]} for k, v in results.items()}, indent=2))


if __name__ == "__main__":
    main()
