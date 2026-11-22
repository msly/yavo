#!/usr/bin/env python3
"""Bind configured model policy, prepared inputs and actual host tools before qualification."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import subprocess

BASE = Path(__file__).resolve().parents[1]
REPO = BASE.parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def encode(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def command(argv, environment=None):
    return subprocess.check_output(argv, cwd=REPO, env=environment, text=True, timeout=30).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, default=BASE / "EXECUTOR_CONFIG.json")
    parser.add_argument("--zig", type=Path)
    args = parser.parse_args()
    frozen_path = BASE / "evidence/frozen.json"
    if frozen_path.exists() or any((BASE / "runs").glob("*/run.json")):
        parser.error("runtime freeze or participant state exists; do not overwrite an experiment")
    config_path = args.config.resolve()
    if config_path.parent != BASE:
        parser.error("executor configuration must live at the packet root")
    executor_path = REPO / "tools/agent_eval/executor.py"
    spec = importlib.util.spec_from_file_location("yavo_metered_executor", executor_path)
    executor = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(executor)
    config = executor.Config(config_path)
    for field in ("requested_model", "expected_reported_model"):
        if not isinstance(config.provider.get(field), str) or not config.provider[field].strip():
            parser.error(f"configure provider.{field} before freezing; placeholders cannot launch")
    if config.value.get("experiment_id") != "docs-ab-2026-11-22-r6":
        parser.error("wrong experiment id")
    if config.lab.get("program") != "perl" or config.lab.get("script") != "tools/lab.pl":
        parser.error("the fixed lab program/script must not change")
    if config.freeze.get("path") != "evidence/frozen.json":
        parser.error("freeze path must be evidence/frozen.json")
    static_blockers = [reason for reason in config.blockers() if reason != 'freeze_sha256_missing']
    if static_blockers:
        parser.error("invalid configuration before freeze: " + "; ".join(static_blockers))
    prepared = json.loads((BASE / "prepared-manifest.json").read_bytes())
    inputs = {}
    for name, expected in prepared["files"].items():
        path = REPO / name
        if path.is_symlink() or not path.resolve().is_relative_to(REPO) or sha(path) != expected:
            raise ValueError("prepared input changed: " + name)
        inputs[str(path)] = expected
    compiler = REPO / "compiler/target/release/yavo"
    runtime = REPO / "compiler/runtime/yavo_rt.h"
    lock = REPO / "compiler/toolchain.lock"
    yavo_home = Path(os.environ.get("YAVO_HOME", str(Path.home() / ".yavo"))).resolve()
    tool_environment = dict(os.environ, YAVO_HOME=str(yavo_home))
    for key in ("YAVO_CC", "YAVO_UPDATE_BASELINE", "YAVO_NO_CACHE", "YAVO_CACHE_TRACE"):
        tool_environment.pop(key, None)
    status = command([str(compiler), "toolchain", "status"], tool_environment)
    match = re.fullmatch(r"C toolchain: zig ([0-9.]+) \((.+)\)", status)
    if not match:
        parser.error("compiler must select the locked Zig backend: " + status)
    zig = Path(match[2]).resolve()
    if not zig.is_relative_to(yavo_home / "toolchains"):
        parser.error("reported Zig does not belong to the bound YAVO_HOME")
    if args.zig is not None and args.zig.resolve() != zig:
        parser.error("--zig must match the backend actually selected by the compiler")
    for path in (compiler, runtime, lock, zig):
        if not path.is_file():
            parser.error("missing host tool/input: " + str(path))
    core = {str(path): sha(path) for path in (compiler, runtime, lock)}
    toolchain = {str(zig): sha(zig)}
    binding = config.binding()
    compiler_config = {"yavo": {"argv": [str(compiler), "build", "{entry}", "-o", "{output}"],
                                "env": {"YAVO_ALLOC_CHECK": "1", "YAVO_HOME": str(yavo_home)}, "identity_files": {**core, **toolchain}}}
    config_artifact = BASE / "evidence/compiler-config.json"
    if config_artifact.exists():
        parser.error("compiler configuration exists; inspect it before creating a new freeze")
    config_artifact.parent.mkdir(parents=True, exist_ok=True)
    with config_artifact.open("xb") as stream:
        stream.write(encode(compiler_config))
    inputs.update(core)
    inputs[str(config_artifact)] = sha(config_artifact)
    manifest = {
        "schema_version": 1, "kind": "yavo.docs_ab.freeze", "experiment_id": config.value["experiment_id"],
        "created_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "compiler_revision": command(["git", "rev-parse", "HEAD"]),
        "prepared_manifest_sha256": sha(BASE / "prepared-manifest.json"),
        "model_settings_sha256": binding["model_settings_sha256"], "executor_sha256": binding["executor_sha256"],
        "provider_policy": config.provider, "identity_files": inputs, "toolchain_files": toolchain,
        "zig_version": command([str(zig), "version"], tool_environment), "toolchain_status": status,
        "host": {"system": platform.system(), "machine": platform.machine(), "python": platform.python_version()},
        "scope": "Every prepared input and core compiler identity checked on lab operations; Zig binary checked at start/probe/seal. Zig library archive is represented by the pinned toolchain lock.",
        "binding_order": "prepared source inputs -> runtime/model freeze -> live qualification -> launch record; no manifest includes its own hash or the qualification that references it.",
    }
    frozen_raw = encode(manifest)
    updated = dict(config.value)
    updated["freeze"] = {"path": "evidence/frozen.json", "sha256": hashlib.sha256(frozen_raw).hexdigest()}
    pending = config_path.with_suffix(config_path.suffix + ".pending")
    with pending.open("xb") as stream:
        stream.write(encode(updated))
    with frozen_path.open("xb") as stream:
        stream.write(frozen_raw)
    pending.replace(config_path)
    print(json.dumps({"status": "frozen_not_qualified", "freeze_sha256": updated["freeze"]["sha256"],
                      "model_settings_sha256": binding["model_settings_sha256"], "identity_files": len(inputs),
                      "participant_sessions": 0, "next": "Commit the freeze/config; run executor qualify before any participant."}))


if __name__ == "__main__":
    main()
