#!/usr/bin/env python3
"""Measure frozen batches 52/53 on one dedicated GitHub-hosted Linux runner.

No server/local override is provided. Preparation and all compiler invocations
are sequential. Only evidence is written under --output; worktrees and caches
live in an owned RUNNER_TEMP directory and are never uploaded by this command.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import gc
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import tarfile
import tempfile
import time

import perf_support as p

ROOT = Path(__file__).resolve().parents[2]
REVISIONS = {"52": "5fa6401db5726c635a34f8d4ce5c6cc32abc88cc",
             "53": "f0183c07195c658de0d0fc859d79406b53893655"}
CASES = ("integer_recurrence", "vec_transform", "mutref_scatter", "deque_ring")
FLAGS = ["-std=c17", "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", "-O2", "-s"]
SMALL_FLAGS = ["-std=c17", "-fno-strict-aliasing", "-O2", "-s"]


def utc():
    return datetime.now(timezone.utc).isoformat()


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args]).decode("utf-8").strip()


def clean_environment():
    env = dict(os.environ)
    for name in list(env):
        if name.startswith(("YAVO_", "ZIG_")) and name != "YAVO_HOME":
            env.pop(name)
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(name, None)
    env.update(LC_ALL="C.UTF-8", LANG="C.UTF-8", RUSTFLAGS="", CARGO_INCREMENTAL="0")
    return env


def export_revision(revision, destination):
    tracked = git("ls-tree", "-r", "--name-only", revision).splitlines()
    files = [name for name in tracked if name.startswith(("compiler/src/", "compiler/runtime/"))
             or name in ("compiler/Cargo.toml", "compiler/Cargo.lock", "compiler/toolchain.lock")
             or name.startswith("selfhost/") and name.count("/") == 1 and name.endswith(".yv")
             or name.startswith("benchmarks/small-c-v1/") and name.count("/") == 2
             and Path(name).suffix in (".yv", ".rs", ".json")]
    destination.mkdir(parents=True)
    archive = destination.parent / (destination.name + ".tar")
    with archive.open("wb") as out:
        subprocess.run(["git", "-C", str(ROOT), "archive", revision, "--", *files], stdout=out, check=True)
    with tarfile.open(archive) as source:
        for member in source:
            target = (destination / member.name).resolve()
            if not target.is_relative_to(destination.resolve()) or not (member.isdir() or member.isfile()):
                raise ValueError("unexpected snapshot archive member")
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as src, target.open("wb") as out:
                    shutil.copyfileobj(src, out)
    archive.unlink()
    return {"revision": revision, "files": {name: p.identity(destination / name) for name in files}}


class Experiment:
    def __init__(self, args):
        self.args = args
        self.output = args.output.resolve()
        self.output.mkdir(parents=True, exist_ok=False)
        self.work = Path(tempfile.mkdtemp(prefix="yavo-paired-perf-", dir=os.environ["RUNNER_TEMP"]))
        self.base_env = clean_environment()
        self.versions = {}
        self.setup_serial = 0
        self.meter = None
        self.report = {"schema": "yavo-isolated-paired-performance/1", "status": "preparing",
                       "created_at": utc(), "source_commit": git("rev-parse", "HEAD"),
                       "revisions": dict(REVISIONS), "host": p.host_record(),
                       "work_directory_not_uploaded": str(self.work), "preparation": [],
                       "isolation": {"dedicated_github_hosted_job_required": True,
                           "sequential_builds": True, "foreign_build_poll_seconds": .05,
                           "foreign_build_policy": "invalidate and retain; never stop other processes",
                           "process_audit_scope": "50ms recognized-name observations during measured commands; preparation checked immediately before each command",
                           "process_audit_limit": "very short or unrecognized processes can evade polling; dedicated hosted VM and serial scheduling are the primary isolation boundary",
                           "cpu_affinity": "same inherited full allowed CPU set for every arm",
                           "os_page_cache": "not flushed; no cold-OS claim"},
                       "measurement": {"clock": "C supervisor CLOCK_MONOTONIC immediately before fork through wait4",
                           "usage": "raw Linux wait4 rusage; maxrss is kernel peak accounting, not simultaneous tree-RSS sum",
                           "python_receipts": "no Full receipt is parsed; child fork occurs in a low-RSS C supervisor",
                           "primary_backend": "fresh translation-unit contents; independent arm caches; warmed toolchain after one recorded warmup",
                           "small_program": "Rust stage0 Release/default native target; edited sources/build-cache miss; PCH hit required",
                           "small_target_ms": 200, "formal_general_performance_goal_completed": False,
                           "job_pass_means": "identity, isolation, correctness and sample contract completed; threshold results are separate",
                           "cold_cache_warmups": "retained single observations, not a cold-build distribution"}}
        self.report["harness"] = {f: p.identity(ROOT / "tools/ci" / f) for f in
                                  ("paired_performance.py", "perf_support.py", "perf_wait4.c")}

    def checkpoint(self):
        if self.meter is not None:
            for row in self.meter.rows:
                p.save(Path(row["stdout"]["path"]).parent / "record.json", row)
            self.report["commands"] = self.meter.rows
        p.save(self.output / "report.json", self.report)

    def setup(self, label, argv, cwd=ROOT, env=None, timeout=1200):
        self.setup_serial += 1
        log = self.output / "preparation" / (f"{self.setup_serial:03d}-" + label)
        log.mkdir(parents=True)
        outside = p.foreign_builds(p.proc_table())
        if outside:
            p.save(log / "foreign-builds.json", outside)
            raise RuntimeError("another build is active during preparation")
        with (log / "stdout.bin").open("wb") as out, (log / "stderr.bin").open("wb") as err:
            rc = subprocess.run(list(map(str, argv)), cwd=cwd, env=env or self.base_env,
                                stdin=subprocess.DEVNULL, stdout=out, stderr=err, timeout=timeout).returncode
        row = {"label": label, "argv": list(map(str, argv)), "cwd": str(cwd), "exit_code": rc,
               "scope": "preparation_or_correctness_not_a_performance_sample",
               "stdout": p.identity(log / "stdout.bin"), "stderr": p.identity(log / "stderr.bin")}
        self.report["preparation"].append(row)
        if rc:
            raise RuntimeError("preparation failed: " + label)
        return (log / "stdout.bin").read_bytes()

    def environment(self, lane, allocation=False, cache=None, pch=True):
        env = dict(self.base_env)
        root = self.work / "caches" / lane
        root.mkdir(parents=True, exist_ok=True)
        env.update(ZIG_GLOBAL_CACHE_DIR=str(root / "zig-global"), ZIG_LOCAL_CACHE_DIR=str(root / "zig-local"))
        if allocation:
            env["YAVO_ALLOC_CHECK"] = "1"
        if cache is None:
            env["YAVO_NO_CACHE"] = "1"
        else:
            cache.mkdir(parents=True, exist_ok=True)
            env.update(YAVO_CACHE_DIR=str(cache), YAVO_CACHE_TRACE="1")
            if not pch:
                env["YAVO_NO_PCH"] = "1"
        return env

    def measure(self, label, argv, env, **extra):
        row = self.meter.run(label, argv, self.work, env)
        row.update(extra)
        p.save(Path(row["stdout"]["path"]).parent / "record.json", row)
        return row

    def retained(self, source, relative):
        target = self.output / "artifacts" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        return p.identity(target)

    def prepare(self):
        if self.report["source_commit"] != os.environ["GITHUB_SHA"]:
            raise RuntimeError("checkout HEAD must equal the recorded GitHub SHA")
        meter = self.work / "perf-wait4"
        self.setup("build-low-rss-meter", ["cc", "-std=c17", "-O2", "-Wall", "-Wextra", "-Werror",
                                          ROOT / "tools/ci/perf_wait4.c", "-o", meter])
        self.meter = p.Meter(meter, self.output / "commands")
        self.report["meter_identity"] = p.identity(meter)
        rust_toolchain = self.base_env.get("RUSTUP_TOOLCHAIN", "")
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", rust_toolchain):
            raise RuntimeError("RUSTUP_TOOLCHAIN must pin an exact Rust release")
        self.setup("install-locked-rust", ["rustup", "toolchain", "install", rust_toolchain, "--profile", "minimal"])
        self.report["rust_toolchain"] = rust_toolchain
        self.report["tool_versions"] = {name: self.setup("version-" + name, command).decode("utf-8").strip()
                                        for name, command in [("rustc", ["rustc", "-Vv"]),
                                                              ("cargo", ["cargo", "--version"]),
                                                              ("cc", ["cc", "--version"])]}
        revisions = dict(REVISIONS)
        if self.args.observe_current:
            revisions["54"] = self.report["source_commit"]
        for label, revision in revisions.items():
            if git("rev-parse", revision + "^{commit}") != revision:
                raise RuntimeError("frozen revision is not an exact commit")
            snapshot = self.work / "snapshots" / label
            manifest = export_revision(revision, snapshot)
            p.save(self.output / "inputs" / (label + ".json"), manifest)
            build = self.work / "cargo-target" / label
            env = dict(self.base_env, CARGO_TARGET_DIR=str(build))
            self.setup("stage0-" + label, ["cargo", "build", "--locked", "--release", "--manifest-path",
                                          snapshot / "compiler/Cargo.toml"], cwd=snapshot, env=env)
            binary = build / "release/yavo"
            self.versions[label] = {"revision": revision, "snapshot": snapshot, "stage0": binary,
                                    "input_manifest": manifest, "stage0_identity": p.identity(binary)}
        locks = {p.digest(v["snapshot"] / "compiler/toolchain.lock") for v in self.versions.values()}
        if len(locks) != 1:
            raise RuntimeError("paired revisions do not use an identical toolchain lock")
        self.setup("install-locked-zig", [self.versions["52"]["stage0"], "toolchain", "install"])
        home = Path(self.base_env.get("YAVO_HOME", str(Path.home() / ".yavo")))
        lock = (self.versions["52"]["snapshot"] / "compiler/toolchain.lock").read_text(encoding="utf-8")
        version = re.search(r"(?m)^zig_version\s*=\s*(\S+)", lock)[1]
        self.zig = home / "toolchains" / ("zig-x86_64-linux-" + version) / "zig"
        actual = self.setup("zig-version", [self.zig, "version"]).decode().strip()
        if actual != version:
            raise RuntimeError("installed Zig does not match the pinned version")
        self.report["zig"] = {**p.identity(self.zig), "version": version,
                              "lock_sha256": next(iter(locks))}
        suite = self.versions["52"]["snapshot"] / "benchmarks/small-c-v1"
        self.manifest = json.loads((suite / "manifest.json").read_bytes())
        self.originals = {case: (suite / (case + ".yv")).read_bytes() for case in CASES}
        for v in self.versions.values():
            for case in CASES:
                if (v["snapshot"] / "benchmarks/small-c-v1" / (case + ".yv")).read_bytes() != self.originals[case]:
                    raise RuntimeError("benchmark source differs between revisions")
        self.oracle = self.work / "small-reference"
        self.setup("independent-small-oracle", ["rustc", "--edition=2021", "-O", suite / "reference.rs", "-o", self.oracle])
        self.report["oracle"] = p.identity(self.oracle)
        for label in ("52", "53"):
            v = self.versions[label]
            root = self.work / "selfhost" / label
            root.mkdir(parents=True)
            source = v["snapshot"] / "selfhost/main.yv"
            stage1, stage2, stage3 = [root / name / "yavo-selfhost" for name in ("stage1", "stage2", "stage3")]
            for binary in (stage1, stage2, stage3): binary.parent.mkdir()
            c2, c3 = stage2.with_suffix(".c"), stage3.with_suffix(".c")
            env = self.environment(label + "/preparation", allocation=True)
            self.setup(label + "-stage1", [v["stage0"], "build", source, "--release", "-o", stage1], env=env)
            self.setup(label + "-emit2", [stage1, "emit-c", source, "-o", c2], env=env)
            self.setup(label + "-stage2", [self.zig, "cc", *FLAGS, "-I", v["snapshot"] / "compiler/runtime", c2, "-o", stage2], env=env)
            self.setup(label + "-emit3", [stage2, "emit-c", source, "-o", c3], env=env)
            if c2.read_bytes() != c3.read_bytes(): raise RuntimeError("C fixed point failed: " + label)
            self.setup(label + "-stage3", [self.zig, "cc", *FLAGS, "-I", v["snapshot"] / "compiler/runtime", c3, "-o", stage3], env=env)
            if p.digest(stage2) != p.digest(stage3): raise RuntimeError("binary fixed point failed: " + label)
            v.update(stage2=stage2, generated=c3, source=source)
            v["retained"] = {"generated_c": self.retained(c3, label + "/selfhost.c"),
                             "stripped_binary": self.retained(stage2, label + "/selfhost")}
        self.report["versions"] = {label: {"revision": v["revision"], "stage0": v["stage0_identity"],
                                          "retained": v.get("retained"), "fixed_point": label in ("52", "53")}
                                   for label, v in self.versions.items()}
        self.report["preparation_completed_at"] = utc()
        self.checkpoint()

    def memory_control(self):
        before = self.measure("control-true-before", ["/bin/true"], self.base_env, scope="instrumentation_control")
        retained = bytearray(64 * 1024 * 1024)
        for i in range(0, len(retained), 4096): retained[i] = 1
        python_rss = Path("/proc/self/status").read_text(encoding="utf-8")
        after = self.measure("control-true-high-parent-rss", ["/bin/true"], self.base_env, scope="instrumentation_control")
        del retained
        gc.collect()
        allocated = self.measure("control-known-child-allocation", [sys.executable, "-c",
            "x=bytearray(32*1024*1024); x[::4096]=b'x'*len(x[::4096])"], self.base_env, scope="instrumentation_control")
        if after["rusage"]["maxrss_kib"] >= 16 * 1024 or allocated["rusage"]["maxrss_kib"] < 30 * 1024:
            raise RuntimeError("child RSS controls failed; measurement contract is invalid")
        self.report["memory_control"] = {"passed": True, "python_status_with_64mb_live": python_rss,
                                        "before": before["label"], "high_parent": after["label"],
                                        "known_allocation": allocated["label"]}

    def selfhost_pairs(self):
        groups = {kind: {label: [] for label in ("52", "53")} for kind in ("frontend", "backend")}
        warmups = {kind: {} for kind in groups}
        for kind, samples in (("frontend", self.args.frontend_samples), ("backend", self.args.backend_samples)):
            for round_id in range(samples + 1):
                order = ("52", "53") if round_id % 2 == 0 else ("53", "52")
                for label in order:
                    v = self.versions[label]
                    lane = self.work / "selfhost-measure" / label / kind
                    lane.mkdir(parents=True, exist_ok=True)
                    env = self.environment(label + "/" + kind, allocation=True)
                    if kind == "frontend":
                        target = lane / (str(round_id) + ".c")
                        argv = [v["stage2"], "emit-c", v["source"], "-o", target]
                    else:
                        target = lane / "selfhost"
                        source = lane / "input.c"
                        source.write_bytes(v["generated"].read_bytes() + ("\n/* perf54 translation-unit miss round %d */\n" % round_id).encode())
                        argv = [self.zig, "cc", *FLAGS, "-I", v["snapshot"] / "compiler/runtime", source, "-o", target]
                    row = self.measure(f"selfhost-{kind}-{label}-{round_id}", argv, env,
                                       scope="warmup" if not round_id else "paired_performance", revision=label,
                                       round=round_id, order=list(order), flags=FLAGS if kind == "backend" else None)
                    expected = v["generated"] if kind == "frontend" else v["stage2"]
                    if p.digest(target) != p.digest(expected):
                        raise RuntimeError("fixed-point output changed during measurement: " + row["label"])
                    row["output_identity"] = p.identity(target)
                    row["matches_frozen_output"] = True
                    if round_id: groups[kind][label].append(row)
                    else: warmups[kind][label] = row["label"]
            differences = [{"round": a["round"], "delta_53_minus_52_seconds": b["elapsed_seconds"] - a["elapsed_seconds"],
                            "ratio_53_over_52": b["elapsed_seconds"] / a["elapsed_seconds"]}
                           for a, b in zip(groups[kind]["52"], groups[kind]["53"])]
            self.report["selfhost_" + kind] = {"paired_differences": differences,
                "median_paired_delta_seconds": statistics.median(d["delta_53_minus_52_seconds"] for d in differences),
                "median_paired_ratio_53_over_52": statistics.median(d["ratio_53_over_52"] for d in differences),
                "warmups": warmups[kind],
                "arms": {label: {"distribution": p.distribution(rows), "samples": [r["label"] for r in rows]}
                         for label, rows in groups[kind].items()},
                "order": "52/53 then 53/52 on successive rounds; warmup is round zero"}
            self.checkpoint()

    def small_programs(self, labels, direct, section):
        modes = ("plain", "pch") if direct else ("pch",)
        records = {}
        results = {}
        for case in CASES:
            root = self.work / section / case
            root.mkdir(parents=True)
            source = root / (case + ".yv")
            lanes = {}
            for label in labels:
                for mode in modes:
                    lane = root / label / mode
                    lane.mkdir(parents=True)
                    cache = lane / "yavo-cache"
                    lanes[(label, mode)] = {"env": self.environment(label + "/small-" + mode, cache=cache, pch=mode == "pch"),
                                           "cache": cache, "output": lane / "program", "rows": [], "warmup": None}
            all_rounds = []
            for round_id in range(self.args.compile_samples + 1):
                source.write_bytes((b"// warmup\n" if not round_id else b"// touched\n" * (round_id + 1)) + self.originals[case])
                base_order = [(label, mode) for mode in modes for label in labels]
                order = base_order if round_id % 2 == 0 else list(reversed(base_order))
                hashes = {}
                for label, mode in order:
                    lane = lanes[(label, mode)]
                    row = self.measure(f"{section}-{case}-{label}-{mode}-{round_id}",
                        [self.versions[label]["stage0"], "build", source, "-o", lane["output"], "--release"], lane["env"],
                        scope="small_warmup" if not round_id else section, case=case, revision=label, mode=mode,
                        round=round_id, order=[list(x) for x in order], source_sha256=p.digest(source))
                    trace = Path(row["stderr"]["path"]).read_text(encoding="utf-8")
                    build = re.search(r"build cache miss ([0-9a-f]+)", trace)
                    if not build:
                        raise RuntimeError("small-program build-cache miss is missing: " + row["label"])
                    binding = p.runtime_pch_evidence(trace, mode, round_id == 0, lane["cache"], lane.get("pch_binding"))
                    c_files = [path for path in lane["cache"].rglob("main.c") if path.parent.name == build[1]]
                    if len(c_files) != 1: raise RuntimeError("cannot bind generated C to the actual cache entry")
                    row.update(build_cache="miss", runtime_pch=("miss" if round_id == 0 else "hit") if binding else "disabled",
                               generated_c=p.identity(c_files[0]), binary=p.identity(lane["output"]))
                    if binding:
                        row["pch"] = {k: v for k, v in binding.items() if k != "key"}
                        row["pch_key"] = binding["key"]
                        if round_id == 0: lane["pch_binding"] = binding
                    p.save(Path(row["stdout"]["path"]).parent / "record.json", row)
                    hashes[(label, mode)] = row["binary"]["sha256"]
                    if round_id: lane["rows"].append(row)
                    else: lane["warmup"] = row["label"]
                for label in labels:
                    if direct and hashes[(label, "plain")] != hashes[(label, "pch")]:
                        raise RuntimeError("PCH/direct binaries differ on the same input")
                all_rounds.append({"round": round_id, "source_sha256": p.digest(source), "order": [list(x) for x in order]})
            result = {"rounds": all_rounds, "arms": {}}
            oracle_case = next(c for c in self.manifest["cases"] if c["id"] == case)
            correctness_args = list(map(str, oracle_case["small_args"][-1]))
            expected = self.setup(section + "-oracle-" + case, [self.oracle, case, *correctness_args])
            for label in labels:
                result["arms"][label] = {}
                for mode in modes:
                    lane = lanes[(label, mode)]
                    actual = self.setup(section + "-correct-" + case + "-" + label + "-" + mode,
                                        [lane["output"], *correctness_args])
                    if actual != expected: raise RuntimeError("small program does not match independent oracle")
                    if len({r["generated_c"]["sha256"] for r in lane["rows"]}) != self.args.compile_samples:
                        raise RuntimeError("edited sources did not produce distinct translation-unit contents")
                    stats = p.distribution(lane["rows"])
                    result["arms"][label][mode] = {"distribution": stats, "warmup": lane["warmup"],
                        "samples": [row["label"] for row in lane["rows"]], "oracle_match": True,
                        "target_p50_le_200ms": stats["p50_seconds"] <= .200 if mode == "pch" else None}
                records[(case, label)] = {"lane": lanes[(label, "pch")], "source": source}
            if tuple(labels) == ("52", "53"):
                result["paired_pch_differences"] = [{"round": a["round"],
                    "delta_53_minus_52_seconds": b["elapsed_seconds"] - a["elapsed_seconds"],
                    "ratio_53_over_52": b["elapsed_seconds"] / a["elapsed_seconds"]}
                    for a, b in zip(lanes[("52", "pch")]["rows"], lanes[("53", "pch")]["rows"])]
            results[case] = result
            self.report[section] = {"cases": results, "revisions": list(labels),
                                   "role": "primary paired baseline" if direct else "separate current-54 observation; not merged with 52/53 trend"}
            self.checkpoint()
        return records

    def phase_diagnostics(self, records, labels, section):
        selected = [case for case in CASES if any(not self.report[section]["cases"][case]["arms"][label]["pch"]["target_p50_le_200ms"] for label in labels)]
        result = {"trigger": "any PCH arm exceeds 200ms; target misses do not fail the job", "cases": {},
                  "interpretation": "separate controlled contrasts, not an additive in-process profiler; residual includes driver/cache/launch and interaction effects"}
        for case in selected:
            rows = {label: {"frontend": [], "backend": []} for label in labels}
            for round_id in range(self.args.compile_samples + 1):
                order = list(labels if round_id % 2 == 0 else reversed(labels))
                for label in order:
                    original = records[(case, label)]["lane"]["rows"][max(0, round_id - 1)]
                    source = records[(case, label)]["source"]
                    original_round = original["round"]
                    source.write_bytes(b"// touched\n" * (original_round + 1) + self.originals[case])
                    lane = self.work / (section + "-phases") / case / label / str(round_id)
                    lane.mkdir(parents=True)
                    c_file, binary = lane / "main.c", lane / "program"
                    env = self.environment(label + "/phase-" + section)
                    frontend = self.measure(f"{section}-phase-front-{case}-{label}-{round_id}",
                        [self.versions[label]["stage0"], "check", source, "--emit-c", c_file], env,
                        scope="phase_warmup" if not round_id else "phase_diagnostic")
                    if p.digest(c_file) != original["generated_c"]["sha256"]:
                        raise RuntimeError("phase frontend C differs from the corresponding measured driver build")
                    if not round_id:
                        # Warm the private toolchain cache without caching any retained C input.
                        with c_file.open("ab") as warm_source:
                            warm_source.write(b"\n/* phase cache warmup, excluded from samples */\n")
                    pch = Path(original["pch"]["path"])
                    backend = self.measure(f"{section}-phase-back-{case}-{label}-{round_id}",
                        [self.zig, "cc", *SMALL_FLAGS, "-include-pch", pch, "-I",
                         self.versions[label]["snapshot"] / "compiler/runtime", c_file, "-o", binary], env,
                        scope="phase_warmup" if not round_id else "phase_diagnostic")
                    if p.digest(binary) != original["binary"]["sha256"]:
                        raise RuntimeError("equivalent frozen-C/PCH backend did not reproduce the actual driver binary")
                    backend.update(corresponding_full_build=original["label"], binary_sha256=p.digest(binary),
                                   binary_equal_to_driver=True, generated_c=p.identity(c_file), pch_identity=original["pch"])
                    if round_id:
                        rows[label]["frontend"].append(frontend); rows[label]["backend"].append(backend)
                    if round_id == self.args.compile_samples:
                        backend["retained_c"] = self.retained(c_file, "phases/" + section + "/" + case + "-" + label + ".c")
            result["cases"][case] = {}
            for label in labels:
                front, back = [p.distribution(rows[label][kind]) for kind in ("frontend", "backend")]
                full = self.report[section]["cases"][case]["arms"][label]["pch"]["distribution"]
                result["cases"][case][label] = {"frontend_check_emit": front, "frozen_c_pch_backend": back,
                    "full_driver_p50_seconds": full["p50_seconds"],
                    "full_minus_backend_p50_seconds": full["p50_seconds"] - back["p50_seconds"],
                    "full_minus_frontend_minus_backend_p50_seconds": full["p50_seconds"] - front["p50_seconds"] - back["p50_seconds"],
                    "samples": {kind: [r["label"] for r in rows[label][kind]] for kind in rows[label]}}
        self.report[section + "_phase_diagnostics"] = result
        self.checkpoint()

    def attribution(self):
        root = self.work / "attribution"
        root.mkdir()
        source, binary = root / "input.c", root / "selfhost"
        headers = [v["snapshot"] / "compiler/runtime/yavo_rt.h" for k, v in self.versions.items() if k in ("52", "53")]
        if p.digest(headers[0]) != p.digest(headers[1]):
            raise RuntimeError("ELF attribution requires identical runtime headers")
        shutil.copyfile(headers[0], root / "yavo_rt.h")
        env = self.environment("attribution", allocation=True)
        data = {}
        for label in ("52", "53"):
            v = self.versions[label]
            source.write_bytes(v["generated"].read_bytes())
            self.setup("unstripped-" + label, [self.zig, "cc", *[f for f in FLAGS if f != "-s"], "-I", root, source, "-o", binary], env=env)
            kept = self.retained(binary, label + "/selfhost-unstripped")
            symbols = p.elf_record(kept["path"])
            stripped = p.elf_record(v["retained"]["stripped_binary"]["path"])
            for name in (".text", ".rodata", ".data", ".data.rel.ro"):
                a = next((s for s in stripped["sections"] if s["name"] == name), None)
                b = next((s for s in symbols["sections"] if s["name"] == name), None)
                if a and (not b or a["sha256"] != b["sha256"]):
                    raise RuntimeError("unstripped attribution changes loadable bytes: " + label + "/" + name)
            mapping = p.source_symbol_map(v["snapshot"] / "selfhost", v["generated"])
            p.save(self.output / "attribution" / (label + "-symbol-map.json"), mapping)
            p.save(self.output / "attribution" / (label + "-elf.json"), {"stripped": stripped, "unstripped": symbols})
            data[label] = {"stripped": stripped, "unstripped": symbols, "sizes": p.symbol_sizes(symbols, mapping)}
        sections = {}
        for label in ("52", "53"):
            for row in data[label]["stripped"]["sections"]:
                sections.setdefault(row["name"], {"52": 0, "53": 0})[label] = row["file_bytes"]
        deltas = [{"section": name, **sizes, "delta": sizes["53"] - sizes["52"]} for name, sizes in sections.items()]
        functions = [{"name": name, "52": data["52"]["sizes"].get(name, 0), "53": data["53"]["sizes"].get(name, 0),
                      "delta": data["53"]["sizes"].get(name, 0) - data["52"]["sizes"].get(name, 0)}
                     for name in data["52"]["sizes"].keys() | data["53"]["sizes"].keys()]
        original = self.versions["53"]["generated"].read_bytes()
        normalized, literal = p.literal_octal_control(original)
        again, check = p.literal_octal_control(normalized)
        if check != literal or again != normalized:
            raise RuntimeError("string representation control is not byte-preserving/idempotent")
        controls = {}
        for label, contents in (("original", original), ("all-octal", normalized)):
            source.write_bytes(contents)
            self.setup("literal-control-" + label, [self.zig, "cc", *FLAGS, "-I", root, source, "-o", binary], env=env)
            controls[label] = {"c": self.retained(source, "string-control/" + label + ".c"),
                               "binary": self.retained(binary, "string-control/" + label)}
        same = controls["original"]["binary"]["sha256"] == controls["all-octal"]["binary"]["sha256"]
        if not same: raise RuntimeError("pure string spelling control changes the stripped binary")
        byte_delta = data["53"]["stripped"]["bytes"] - data["52"]["stripped"]["bytes"]
        module_sizes = {}
        for row in functions:
            module = row["name"].split(".", 1)[0] if not row["name"].startswith("generated-helper:") else "generated-helper"
            values = module_sizes.setdefault(module, {"52": 0, "53": 0, "delta": 0})
            for key in ("52", "53", "delta"): values[key] += row[key]
        self.report["binary_attribution"] = {"module_text_deltas": module_sizes, "flags": FLAGS, "same_header_sha256": p.digest(headers[0]),
            "stripped_binary_delta_bytes": byte_delta, "section_file_deltas": deltas,
            "non_section_file_delta_bytes": byte_delta - sum(d["delta"] for d in deltas),
            "text_symbol_deltas": sorted(functions, key=lambda x: abs(x["delta"]), reverse=True),
            "symbol_accounting": "defined .text symbols, exact-region aliases deduplicated; inlined code is charged to its compiled caller; alignment/padding and uncovered bytes remain separate",
            "text_bytes_not_explained_by_symbol_size_delta": next(d["delta"] for d in deltas if d["section"] == ".text") - sum(row["delta"] for row in functions),
            "string_representation_control": {**literal, "arms": controls, "identical_stripped_binary": same,
                "scope": "same batch53 program, flags, input filename, output filename, header and toolchain; only byte-equivalent ordinary C string token spelling changes"}}
        self.checkpoint()

    def run(self):
        try:
            self.prepare()
            self.memory_control()
            self.report["status"] = "measuring"
            self.selfhost_pairs()
            primary = self.small_programs(("52", "53"), True, "small_paired")
            self.phase_diagnostics(primary, ("52", "53"), "small_paired")
            if self.args.observe_current:
                current = self.small_programs(("54",), False, "current54_pch_observation")
                self.phase_diagnostics(current, ("54",), "current54_pch_observation")
            self.attribution()
            for version in self.versions.values():
                for item in version["input_manifest"]["files"].values():
                    if p.digest(item["path"]) != item["sha256"]: raise RuntimeError("source inputs changed during the run")
                if p.digest(version["stage0"]) != version["stage0_identity"]["sha256"]:
                    raise RuntimeError("stage0 changed during measurement")
            if p.digest(self.zig) != self.report["zig"]["sha256"]:
                raise RuntimeError("Zig executable changed during measurement")
            for label in ("52", "53"):
                v = self.versions[label]
                if p.digest(v["stage2"]) != v["retained"]["stripped_binary"]["sha256"] or p.digest(v["generated"]) != v["retained"]["generated_c"]["sha256"]:
                    raise RuntimeError("selfhost compiler/generated C changed during measurement")
            self.report["status"] = "passed"
            self.report["qualified_for_paired_comparison"] = True
        except Exception as error:
            self.report["status"] = "failed"
            self.report["error"] = {"type": type(error).__name__, "message": str(error)}
            raise
        finally:
            self.report["finished_at"] = utc()
            self.report["host_after"] = p.snapshot()
            self.checkpoint()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--backend-samples", type=int, default=7)
    parser.add_argument("--frontend-samples", type=int, default=9)
    parser.add_argument("--compile-samples", type=int, default=7)
    parser.add_argument("--observe-current", action="store_true")
    args = parser.parse_args()
    if min(args.backend_samples, args.frontend_samples, args.compile_samples) < 5:
        parser.error("at least five retained samples per arm are required")
    p.require_runner()
    if not os.environ.get("RUNNER_TEMP"):
        parser.error("RUNNER_TEMP is required; working trees/caches must not enter the evidence artifact")
    experiment = Experiment(args)
    experiment.run()
    print(json.dumps({"status": experiment.report["status"], "report": str(experiment.output / "report.json"),
                      "measurement_contract_valid": True, "performance_target_is_separate": True}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
