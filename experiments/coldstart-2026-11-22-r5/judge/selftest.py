#!/usr/bin/env python3
"""Exercise false-pass risks in the judge; these are not Yavo/Rust task scores."""
from __future__ import annotations
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False) + "\n", encoding="utf-8")


def module(name):
    path = Path(__file__).with_name(name + ".py")
    specification = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(loaded)
    return loaded


def main():
    judge = Path(__file__).with_name("evaluate.pl")
    evidence = module("check_evidence")
    summary = module("summarize")
    with tempfile.TemporaryDirectory(prefix="yavo-r5-judge-selftest-") as temporary:
        root = Path(temporary)
        compiler = root / "fake_compiler.py"
        compiler.write_text("import os,sys,shutil\nsrc,out=sys.argv[1:]\nif '# BUILD_FAIL' in open(src).read(): sys.exit(1)\nshutil.copyfile(src,out)\nos.chmod(out,0o700)\n")
        programs = ["print('ok')", "print('wrong')", "# BUILD_FAIL", "while True: pass",
                    "import sys; print('bad',file=sys.stderr); print('ok')", "import sys; print('ok'); sys.exit(2)",
                    None, "print('雪')", "print('000000001')", "print('ok ')",
                    "import sys; sys.stdout.buffer.write(b'\\xff')", "print('x'*20000)"]
        tasks, submitted = [], []
        for index, program in enumerate(programs, 1):
            task_id = f"T{index:02d}"
            expected = "雪\n" if index == 8 else "000000001\n" if index == 9 else "ok\n"
            cases = [{"id": task_id + "-" + visibility, "visibility": visibility, "stdin": "", "stdout": expected,
                      "stderr": "", "exit_code": 0, "args": [], "files": {}} for visibility in ("public", "hidden")]
            tasks.append({"id": task_id, "structural": [], "cases": cases})
            item = {"id": task_id}
            if program is not None:
                source_dir = root / "source" / task_id
                source_dir.mkdir(parents=True)
                (source_dir / "main.rs").write_text(f"#!{sys.executable}\n" + program + "\n", encoding="utf-8")
                item.update({stage: {"source_dir": f"source/{task_id}", "entry": "main.rs"} for stage in ("first", "final")})
            submitted.append(item)
        dump(root / "cases.json", {"schema_version": 1, "tasks": tasks})
        dump(root / "submission.json", {"schema_version": 1, "run_id": "rust-test", "language": "rust", "replicate": "test", "tasks": submitted})
        config = {"rust": {"argv": [sys.executable, str(compiler), "{entry}", "{output}"],
                           "identity_files": {str(compiler): hashlib.sha256(compiler.read_bytes()).hexdigest()}}}
        dump(root / "config.json", config)
        command = ["perl", str(judge), "--cases", str(root / "cases.json"), "--submission", str(root / "submission.json"),
                   "--compiler-config", str(root / "config.json"), "--output", str(root / "evaluation"),
                   "--run-timeout", "0.15", "--output-limit", "8192"]
        result = subprocess.run(command, capture_output=True, text=True)
        assert result.returncode == 0, result.stdout + result.stderr
        report_path = root / "evaluation/report.json"
        report = json.loads(report_path.read_text())
        expected_passes = {"T01", "T08", "T09"}
        for task in report["tasks"]:
            for stage in ("first", "final"):
                assert summary.behavior(task[stage]) == (task["id"] in expected_passes), task
        assert report["tasks"][3]["final"]["cases"][0]["observed"]["status"] == "timeout"
        assert report["tasks"][11]["final"]["cases"][0]["observed"]["status"] == "output_limit"
        good = evidence.validate(root / "cases.json", [report_path], None)
        assert good["passed"], good
        report["tasks"][1]["final"]["cases"][0]["passed"] = True
        dump(root / "tampered-report.json", report)
        bad = evidence.validate(root / "cases.json", [root / "tampered-report.json"], None)
        assert not bad["passed"] and any("incorrect overall judgment" in error for error in bad["errors"])
        compiler.write_text(compiler.read_text() + "# changed compiler\n")
        mismatch = subprocess.run(command, capture_output=True, text=True)
        assert mismatch.returncode != 0 and "compiler identity mismatch" in mismatch.stderr
        print(json.dumps({"status": "passed", "scenarios": ["exact bytes", "Unicode", "leading zeros", "wrong output", "build failure",
              "timeout", "stderr mismatch", "nonzero exit", "missing submission", "trailing whitespace", "invalid UTF-8", "output limit",
              "tampered judgment", "compiler identity mismatch"]}))


if __name__ == "__main__":
    main()
