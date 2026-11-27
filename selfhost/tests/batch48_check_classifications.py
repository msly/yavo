#!/usr/bin/env python3
"""Check the complete registered subset classification without compiling C.

Example: python selfhost/tests/batch48_check_classifications.py --compiler
compiler/target/batch48/candidate5 --output compiler/target/batch48/classification-candidate5
"""
from __future__ import annotations

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SELFHOST = ROOT / "selfhost"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected-count", type=int)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=15)
    args = parser.parse_args()
    if args.jobs < 1 or args.timeout < 1:
        parser.error("--jobs and --timeout must be positive")
    compiler = args.compiler.resolve()
    if not compiler.is_file():
        parser.error("--compiler must name an existing executable")
    loader = importlib.util.spec_from_file_location("classification_bootstrap", SELFHOST / "bootstrap.py")
    bootstrap = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(bootstrap)
    cases = bootstrap.validate_cases()
    if args.expected_count is not None and len(cases) != args.expected_count:
        parser.error("manifest count differs from --expected-count")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="check-", dir=output))
    manifest = SELFHOST / "tests/cases.json"
    bound = {str(path): digest(path) for path in (compiler, manifest, Path(__file__), SELFHOST / "bootstrap.py")}
    sources = {}
    for case in cases:
        fixture = SELFHOST / "tests" / case["file"]
        bound[str(fixture)] = digest(fixture)
        source, assembly = fixture, None
        if "source_prefix" in case:
            prefix = bootstrap.prefix_path(case)
            bound[str(prefix)] = digest(prefix)
            place = work / case["id"]
            place.mkdir()
            source = place / "input.yv"
            source.write_bytes(prefix.read_bytes() + fixture.read_bytes())
            bound[str(source)] = digest(source)
            assembly = {"source_prefix": case["source_prefix"], "prefix_sha256": digest(prefix),
                        "fixture_sha256": digest(fixture)}
        sources[case["id"]] = (source, assembly)
    for path in (SELFHOST / "tests/modules").rglob("*.yv"):
        bound[str(path)] = digest(path)
    environment = dict(os.environ, PATH="", YAVO_ALLOC_CHECK="1", YAVO_NO_CACHE="1")

    def check(case):
        source, assembly = sources[case["id"]]
        try:
            result = subprocess.run([str(compiler), "check", str(source)], cwd=ROOT,
                                    env=environment, capture_output=True, timeout=args.timeout)
            code, out, err = result.returncode, result.stdout, result.stderr
            timed_out = False
        except subprocess.TimeoutExpired:
            code, out, err, timed_out = 124, b"", b"classification check timed out", True
        except OSError as error:
            code, out, err, timed_out = 127, b"", str(error).encode(), False
        codes = sorted({value.decode("ascii") for value in bootstrap.DIAGNOSTIC.findall(out + err)})
        kind, expected = case["kind"], case.get("diagnostic")
        if kind in ("run", "panic"):
            passed = code == 0 and not timed_out
        else:
            wanted = expected in codes if expected else (any(value.startswith("E") for value in codes)
                                                        and "U0001" not in codes)
            if kind == "shared_unsupported":
                wanted = codes == ["U0001"]
            passed = (code == 1 and wanted and not timed_out and not out
                      and bool(bootstrap.LOCATION.search(err)))
        row = {"id": case["id"], "file": case["file"], "kind": kind,
               "expected_diagnostic": expected, "exit": code, "codes": codes,
               "passed": passed, "source_sha256": digest(source)}
        if assembly is not None:
            row["assembly"] = assembly
        if not passed:
            row.update(stdout=out.decode("utf-8", "replace"), stderr=err.decode("utf-8", "replace"))
        return row

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(pool.map(check, cases))
    changed = [path for path, wanted in bound.items() if not Path(path).is_file() or digest(path) != wanted]
    report = {"schema": "yavo-batch48-full-classification-review/1", "compiler": str(compiler),
              "compiler_sha256": bound[str(compiler)], "manifest_sha256": bound[str(manifest)],
              "work": str(work), "case_count": len(rows), "prefix_cases": sum("assembly" in row for row in rows),
              "passed": sum(row["passed"] for row in rows), "failed": sum(not row["passed"] for row in rows),
              "kind_counts": dict(Counter(row["kind"] for row in rows)), "changed_inputs": changed,
              "input_identities": bound, "cases": rows,
              "scope": "check-only; source_prefix bytes assembled exactly as bootstrap; no C build or run"}
    report["status"] = "passed" if report["failed"] == 0 and not changed else "failed"
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    (work / "results.json").write_text(rendered, encoding="utf-8")
    (output / "results.json").write_text(rendered, encoding="utf-8")
    print(json.dumps({**{key: report[key] for key in
          ("status", "case_count", "prefix_cases", "passed", "failed", "compiler_sha256", "manifest_sha256", "changed_inputs")},
          "differences": [row for row in rows if not row["passed"]]}, ensure_ascii=False))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
