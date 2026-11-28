#!/usr/bin/env python3
"""Exercise subset receipt/diagnostic I/O with a simulated Windows ANSI default.

The small receipts contain UTF-8 0x8f, undefined in cp1252. Actual checker/analyzer
subprocesses write UTF-8 bytes; only their implicit parent-side text encoding is
changed. No compiler build or retained Full receipt is needed.
"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = load("subset_core_encoding_control", "tools/ci/subset_core.py")
sweep = load("subset_sweep_encoding_control", "selfhost/subset_sweep.py")
calibrate = load("subset_calibrate_encoding_control", "selfhost/calibrate_subset.py")


@contextlib.contextmanager
def ansi_default():
    """Force ANSI defaults even when the outer interpreter uses PYTHONUTF8=1.

    Patching these default-selection boundaries intentionally bypasses UTF-8 mode;
    explicit UTF-8 and byte I/O retain their production behavior.
    """
    read_text, write_text = Path.read_text, Path.write_text

    def ansi_read(path, encoding=None, errors=None, **kwargs):
        return read_text(path, encoding="cp1252" if encoding is None else encoding,
                         errors=errors, **kwargs)

    def ansi_write(path, data, encoding=None, errors=None, **kwargs):
        return write_text(path, data, encoding="cp1252" if encoding is None else encoding,
                          errors=errors, **kwargs)

    with patch.object(Path, "read_text", ansi_read), patch.object(Path, "write_text", ansi_write), \
            patch.object(subprocess, "_text_encoding", return_value="cp1252"):
        yield


def utf8_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes((json.dumps(value, ensure_ascii=False) + "\n").encode("utf-8"))


class SubsetEncodingTests(unittest.TestCase):
    def test_core_reads_utf8_receipt_and_preserves_unicode_argv(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "回执 with space"
            work = output / "目录 with space" / "叶.dep"
            stage0 = output / "参考 编译器" / "叶-stage0"
            receipt = output / "receipt.json"
            utf8_json(receipt, {
                "status": "passed", "work": str(work),
                "inputs": {"stage0": {"path": str(stage0)}},
                "checks": [{"kind": "编码诊断", "status": "通过：叶"}],
            })
            with ansi_default(), patch.object(sys, "argv", ["subset_core", "--bootstrap-output", str(output)]), \
                    patch.object(core.subprocess, "call", return_value=0) as call:
                # Deliberate default decoding proves ansi_default overrides UTF-8 mode.
                with self.assertRaises(UnicodeDecodeError):
                    receipt.read_text()
                self.assertEqual(core.main(), 0)
            self.assertEqual(call.call_count, 2)
            first, second = [item.args[0] for item in call.call_args_list]
            suffix = ".exe" if sys.platform == "win32" else ""
            compiler = str(work / "stages/stage2" / ("yavo-selfhost" + suffix))
            self.assertEqual(first[first.index("--stage0") + 1], str(stage0))
            self.assertEqual(second[second.index("--analyzer") + 1], str(stage0))
            for argv in (first, second):
                self.assertEqual(argv[argv.index("--selfhost") + 1], compiler)

    def test_actual_subprocess_utf8_diagnostics_and_json_survive_ansi_default(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory) / "目录 with space"
            work.mkdir()
            source = work / "叶.yv"
            source.write_bytes(b"fn main() {}\n")
            diagnostic = "叶.yv:1:1: U0001: 组合载荷尚未实现\n"
            model = {"schema": "yavo-subset-features/1", "stage0_accepted": True,
                     "ast_complete": True, "semantic_complete": False,
                     "blockers": {"组合载荷": [{"file": "目录/叶.yv", "detail": "拥有字段"}]},
                     "unknown": ["来源待追踪"]}
            # Python executes these files as scripts, so the production argv shape
            # works without a compiler, executable bit, shebang or platform wrapper.
            (work / "check").write_bytes(("import os, sys\nos.write(2, " +
                repr(diagnostic.encode("utf-8")) + ")\nsys.exit(1)\n").encode("ascii"))
            (work / "subset-features").write_bytes(("import os\nos.write(1, " +
                repr(json.dumps(model, ensure_ascii=False).encode("utf-8")) + ")\n").encode("ascii"))
            with ansi_default():
                control = subprocess.run([sys.executable, "check", source.name], cwd=work,
                                         capture_output=True, text=True, errors="replace", timeout=10)
                self.assertNotEqual(control.stderr, diagnostic, "ANSI simulation must detect implicit decoding")
                checked = sweep.check(Path(sys.executable), source, timeout=10)
                analyzed = sweep.analyze(Path(sys.executable), source, timeout=10)
            self.assertEqual(checked["exit"], 1)
            self.assertEqual(checked["code"], "U0001")
            self.assertEqual(checked["first"], diagnostic.rstrip("\n"))
            self.assertEqual(analyzed, model)

    def test_calibration_reads_and_saves_unicode_under_ansi_default(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "校准 with space"
            tests = root / "selfhost/tests"
            manifest = tests / "cases.json"
            relative = "run/叶.yv"
            utf8_json(manifest, {"cases": [{"id": "unicode_case", "kind": "run", "file": relative}]})
            (tests / "run").mkdir()
            source = tests / relative
            source.write_bytes(b"fn main() {}\n")
            compiler, analyzer = root / "候选 编译器", root / "参考 编译器"
            compiler.write_bytes(b"candidate identity")
            analyzer.write_bytes(b"analyzer identity")
            output = root / "输出" / "校准.json"
            checked = {"exit": 0, "first": "", "code": None, "seconds": 0}
            modeled = {"ast_complete": True, "blockers": {}, "unknown": ["来源仍待追踪"]}
            argv = ["calibrate_subset", "--analyzer", str(analyzer), "--selfhost", str(compiler),
                    "--json", str(output)]
            with ansi_default(), patch.object(calibrate, "ROOT", root), patch.object(sys, "argv", argv), \
                    patch.object(calibrate.sweep, "check", return_value=checked) as check, \
                    patch.object(calibrate.sweep, "analyze", return_value=modeled), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(calibrate.main(), 0)
            check.assert_called_once_with(compiler.resolve(), source)
            saved = json.loads(output.read_bytes().decode("utf-8"))
            self.assertTrue(saved["passed"])
            self.assertEqual(saved["summary"], {"passed": 1, "failed": 0, "skipped": 0})
            self.assertEqual(saved["rows"][0]["file"], relative)
            self.assertEqual(saved["rows"][0]["unknown"], modeled["unknown"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
