#!/usr/bin/env python3
"""Independent SHA-256 vectors for the pure Yavo source-identity module."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent


def identity(path: Path) -> dict:
    data = path.read_bytes()
    return {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def yavo_literal(text: str) -> str:
    escaped = {'"': '\\"', '\\': '\\\\', '\n': '\\n', '\r': '\\r',
               '\t': '\\t', '\0': '\\0', '$': '\\$'}
    return '"' + ''.join(escaped.get(c, '\\u{' + format(ord(c), 'x') + '}'
                         if ord(c) < 32 or ord(c) == 127 else c) for c in text) + '"'


def expected_fixture(cases: list[dict]) -> bytes:
    lines = ["use digest", "", "fn repeated(unit: str, count: int, suffix: str) -> String {",
             "    var text = String{}", "    for _item in 0..count { text.push(unit) }",
             "    text.push(suffix)", "    return text", "}", "", "fn main() {"]
    for case in cases:
        recipe = case["recipe"]
        lines.append("    print(" + yavo_literal(case["id"] + "\t") + ")")
        lines.append("    println(digest.sha256(repeated(" + yavo_literal(recipe["unit"])
                     + ", " + str(recipe["repeat"]) + ", " + yavo_literal(recipe["suffix"]) + ")))")
    return ("\n".join(lines + ["}"]) + "\n").encode("utf-8")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--stage0", required=True, type=Path)
    p.add_argument("--selfhost", required=True, action="append", type=Path,
                   help="Selfhost executable to verify; repeat for stage1 and stage2")
    p.add_argument("--zig", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    args = p.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="run-", dir=output))
    compilers = [("stage0", args.stage0.resolve())] + [
        (f"selfhost-{i + 1}", path.resolve()) for i, path in enumerate(args.selfhost)]
    zig = args.zig.resolve()
    bound_paths = [ROOT / "selfhost/digest.yv", ROOT / "selfhost/tests/digest50_test.yv",
                   HERE / "vectors.json", HERE / "freeze.json", Path(__file__).resolve(),
                   ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock",
                   zig] + [path for _, path in compilers]
    before = [identity(path) for path in bound_paths]
    receipt = {"schema": "yavo-digest50-validation/1", "status": "running",
               "started_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
               "host": {"os": os.name, "platform": platform.platform(), "machine": platform.machine()},
               "work": str(work), "inputs": before, "commands": [], "runs": []}
    receipt_path = output / "receipt.json"
    cache_paths = [work / "zig-global-cache", work / "zig-local-cache"]
    env = dict(os.environ)
    env["ZIG_GLOBAL_CACHE_DIR"] = str(cache_paths[0])
    env["ZIG_LOCAL_CACHE_DIR"] = str(cache_paths[1])
    env["YAVO_ALLOC_CHECK"] = "1"

    def command(label: str, argv: list[str], cwd: Path = work) -> dict:
        start = time.monotonic()
        record = {"label": label, "argv": argv, "cwd": str(cwd)}
        stdout_path, stderr_path = work / (label + ".stdout"), work / (label + ".stderr")
        try:
            run = subprocess.run(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, timeout=240)
            record.update(exit_code=run.returncode, timed_out=False)
            stdout_path.write_bytes(run.stdout)
            stderr_path.write_bytes(run.stderr)
        except subprocess.TimeoutExpired as error:
            record.update(exit_code=None, timed_out=True)
            stdout_path.write_bytes(error.stdout or b"")
            stderr_path.write_bytes(error.stderr or b"")
        except OSError as error:
            record.update(exit_code=None, launch_error=str(error), timed_out=False)
            stdout_path.write_bytes(b"")
            stderr_path.write_bytes(str(error).encode("utf-8"))
        record.update(seconds=time.monotonic() - start, stdout=identity(stdout_path),
                      stderr=identity(stderr_path))
        receipt["commands"].append(record)
        require(record.get("exit_code") == 0, label + " failed")
        return record

    try:
        freeze = json.loads((HERE / "freeze.json").read_bytes())
        for relative, wanted in freeze["inputs"].items():
            got = identity(ROOT / relative)
            require({"bytes": got["bytes"], "sha256": got["sha256"]} == wanted,
                    "frozen input changed: " + relative)
        vectors = json.loads((HERE / "vectors.json").read_bytes())
        cases = vectors["cases"]
        require(len(cases) == freeze["cases"] and len({case["id"] for case in cases}) == len(cases),
                "vector inventory mismatch")
        fixture = ROOT / "selfhost/tests/digest50_test.yv"
        require(fixture.read_bytes() == expected_fixture(cases), "fixture differs from independent input recipes")
        expected = b""
        for case in cases:
            recipe = case["recipe"]
            raw = (recipe["unit"] * recipe["repeat"] + recipe["suffix"]).encode("utf-8")
            require(len(raw) == case["bytes"], "input length changed: " + case["id"])
            require(hashlib.sha256(raw).hexdigest() == case["sha256"], "oracle mismatch: " + case["id"])
            expected += (case["id"] + "\t" + case["sha256"] + "\n").encode("ascii")
        (work / "expected.stdout").write_bytes(expected)
        shutil.copyfile(ROOT / "selfhost/digest.yv", work / "digest.yv")
        shutil.copyfile(fixture, work / "main.yv")
        version = command("zig-version", [str(zig), "version"])
        version_text = Path(version["stdout"]["path"]).read_text().strip()
        locked = next(line.split("=", 1)[1].strip() for line in
                      (ROOT / "compiler/toolchain.lock").read_text().splitlines()
                      if line.startswith("zig_version ="))
        require(version_text == locked, "locked Zig version mismatch")
        for frontend, compiler in compilers:
            c_path = work / (frontend + ".c")
            argv = ([str(compiler), "check", str(work / "main.yv"), "--emit-c", str(c_path)]
                    if frontend == "stage0" else
                    [str(compiler), "emit-c", str(work / "main.yv"), "-o", str(c_path)])
            emitted = command(frontend + "-emit", argv)
            require(Path(emitted["stdout"]["path"]).read_bytes() == b"" and
                    Path(emitted["stderr"]["path"]).read_bytes() == b"", "unexpected frontend output")
            for mode in ["debug", "release"]:
                binary = work / (frontend + "-" + mode + (".exe" if os.name == "nt" else ""))
                flags = ["-O0", "-g"] if mode == "debug" else ["-O2", "-s"]
                command(frontend + "-" + mode + "-build", [str(zig), "cc", "-std=c17",
                        "-fno-strict-aliasing", "-DYV_ALLOC_CHECK", *flags, "-I",
                        str(ROOT / "compiler/runtime"), str(c_path), "-o", str(binary)])
                run = command(frontend + "-" + mode + "-run", [str(binary)])
                actual = Path(run["stdout"]["path"]).read_bytes()
                errors = Path(run["stderr"]["path"]).read_bytes()
                require(actual == expected and errors == b"", frontend + "/" + mode + " digest output mismatch")
                receipt["runs"].append({"frontend": frontend, "mode": mode, "cases": len(cases),
                        "passed": len(cases), "alloc_check": True, "generated_c": identity(c_path),
                        "binary": identity(binary), "output": run["stdout"]})
        after = [identity(path) for path in bound_paths]
        require(after == before, "bound inputs changed during validation")
        receipt.update(status="passed", cases=len(cases),
                       executions=sum(run["cases"] for run in receipt["runs"]),
                       inputs_unchanged=True)
    except Exception as error:
        receipt.update(status="failed", error=str(error))
    finally:
        for cache in cache_paths:
            if cache.exists():
                shutil.rmtree(cache)
        receipt["isolated_zig_caches_removed"] = all(not cache.exists() for cache in cache_paths)
        receipt["completed_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        receipt_path.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": receipt["status"], "receipt": str(receipt_path),
                      "cases": receipt.get("cases"), "executions": receipt.get("executions"),
                      "error": receipt.get("error")}, ensure_ascii=False))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
