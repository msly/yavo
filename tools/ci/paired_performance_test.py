#!/usr/bin/env python3
"""Small contract/negative controls; never builds a baseline compiler."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import perf_support as p

HERE = Path(__file__).resolve().parent


class EvidenceContracts(unittest.TestCase):
    def test_only_github_hosted_linux_is_qualified(self):
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(RuntimeError): p.require_runner()
        env = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "self-hosted", "GITHUB_RUN_ID": "1", "GITHUB_SHA": "a" * 40}
        with patch.dict(os.environ, env, clear=True), patch("platform.system", return_value="Linux"), patch("platform.machine", return_value="x86_64"):
            with self.assertRaises(RuntimeError): p.require_runner()
            os.environ["RUNNER_ENVIRONMENT"] = "github-hosted"
            p.require_runner()

    def test_external_builds_are_not_confused_with_owned_descendants(self):
        def row(pid, parent, name, state="R"):
            return {"pid": pid, "ppid": parent, "pgrp": pid, "comm": name, "executable": name, "state": state}
        processes = {1: row(1, 0, "init"), 10: row(10, 1, "perf-wait4"), 11: row(11, 10, "cargo"),
                     12: row(12, 11, "rustc"), 20: row(20, 1, "clang-18"), 21: row(21, 1, "cargo", "Z")}
        self.assertEqual([r["pid"] for r in p.foreign_builds(processes, (10,))], [20])
        self.assertEqual([r["pid"] for r in p.foreign_builds(processes)], [11, 12, 20])
        for index, executable in enumerate(("x86_64-linux-gnu-gcc-13", "aarch64-linux-gnu-gcc", "rust-lld", "collect2", "x86_64-linux-gnu-ld.bfd", "yavo", "yavo-selfhost"), 30):
            processes[index] = row(index, 1, executable[:15])
            processes[index]["executable"] = executable
        self.assertEqual([r["pid"] for r in p.foreign_builds(processes, (10,))], [20, 30, 31, 32, 33, 34, 35, 36])

    def test_literal_control_has_independent_byte_expectation(self):
        data = b'#include "yavo_rt.h"\nconst char *s="A\\x42\\0034?\\\\\\\""; // "ignored"\nchar q=\'"\';\n'
        output, meta = p.literal_octal_control(data)
        expected = b'A' + b'B' + bytes([3]) + b'4?\\"'
        expected_hash = hashlib.sha256(struct.pack('<Q', len(expected)) + expected).hexdigest()
        self.assertEqual(meta, {"string_token_count": 1, "decoded_string_sequence_sha256": expected_hash})
        self.assertIn(b'"\\101\\102\\003\\064\\077\\134\\042"', output)
        self.assertIn(b'#include "yavo_rt.h"', output)
        self.assertIn(b'// "ignored"', output)
        self.assertEqual(p.literal_octal_control(output), (output, meta))

    def test_unsupported_literal_semantics_fail_closed(self):
        for data in (b'L"wide"', b'"\\u0041"', b'"\\x100"', b'"\\777"', b'"unterminated'):
            with self.subTest(data=data), self.assertRaises(ValueError):
                p.literal_octal_control(data)

    def test_function_map_tracks_breadth_first_order_and_rejects_mismatch(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            labels = ["root main unique body", "module alpha unique body", "module beta unique body", "module gamma unique body"]
            (root / "main.yv").write_text('use a\nuse b\n// fn fake() {}\nfn main() { let s = "' + labels[0] + '" }\n', encoding="utf-8")
            (root / "a.yv").write_text('use c\npub fn alpha(x: int) { let s = "' + labels[1] + '" }\n', encoding="utf-8")
            (root / "b.yv").write_text('fn beta() { let s = "' + labels[2] + '" }\n', encoding="utf-8")
            (root / "c.yv").write_text('use a\nfn gamma() { let s = "' + labels[3] + '" }\n', encoding="utf-8")
            generated = root / "out.c"
            definitions = [f'static void sh_fn_{i}({"int x" if i == 1 else "void"}) {{ puts("{label}"); }}\n'
                           for i, label in enumerate(labels)]
            wrapper = 'int main(int argc, char **argv) { sh_fn_0(); return 0; }\n'
            good = ''.join(definitions) + wrapper
            generated.write_text(good, encoding="utf-8")
            mapping = p.source_symbol_map(root, generated)
            self.assertEqual([r["module"] + "." + r["name"] for r in mapping["functions"].values()],
                             ["main.main", "a.alpha", "b.beta", "c.gamma"])
            self.assertEqual(mapping["independent_c_validation"]["anchored_modules"], ["a", "b", "c", "main"])
            # All negative controls retain four sh_fn indices: count alone is insufficient.
            bad_order = good.replace(labels[2], "temporary unique body").replace(labels[3], labels[2]).replace("temporary unique body", labels[3])
            for wrong in (bad_order, good.replace("sh_fn_0(); return", "sh_fn_2(); return"),
                          good.replace("sh_fn_1(int x)", "sh_fn_1(void)"),
                          ''.join(f'static void sh_fn_{i}(void);\n' for i in range(4)) + wrapper):
                generated.write_text(wrong, encoding="utf-8")
                with self.assertRaises(ValueError): p.source_symbol_map(root, generated)

    def test_yavo_anchor_decoding_is_distinct_from_c_spelling(self):
        data = b'"literal \\${value} \\0end \\u{4e2d}"'
        _, result = p.literal_octal_control(data, collect_values=True, yavo_strings=True)
        self.assertEqual(bytes.fromhex(result["decoded_values_hex"][0]), 'literal ${value} \0end 中'.encode('utf-8'))
        with self.assertRaises(ValueError): p.literal_octal_control(data)
        with self.assertRaises(ValueError): p.literal_octal_control(b'"${x}"', yavo_strings=True)

    def test_pch_trace_and_artifact_must_match_the_claimed_lane(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); artifact = root / "pch" / "abc123" / "yavo_rt.pch"
            artifact.parent.mkdir(parents=True); artifact.write_bytes(b"frozen pch")
            miss, hit = "runtime pch miss abc123", "runtime pch hit abc123"
            self.assertIsNone(p.runtime_pch_evidence("build cache miss 123", "plain", True, root))
            for trace in (miss, hit, "runtime pch unavailable"):
                with self.assertRaises(RuntimeError): p.runtime_pch_evidence(trace, "plain", False, root)
            binding = p.runtime_pch_evidence(miss, "pch", True, root)
            self.assertEqual(p.runtime_pch_evidence(hit, "pch", False, root, binding), binding)
            for trace, warmup in ((hit, True), (miss, False), (hit + "\n" + hit, False), (hit + " rejected", False)):
                with self.assertRaises(RuntimeError): p.runtime_pch_evidence(trace, "pch", warmup, root, binding)
            artifact.write_bytes(b"changed pch")
            with self.assertRaises(RuntimeError): p.runtime_pch_evidence(hit, "pch", False, root, binding)

    def test_symbol_numbers_do_not_create_false_growth(self):
        def symbol(name, addr, size): return {"name": name, "section": ".text", "address": addr, "size": size}
        old = {"symbols": [symbol("sh_fn_2", 100, 30), symbol("sh_fn_2.alias", 100, 30)]}
        new = {"symbols": [symbol("sh_fn_5", 200, 40)]}
        self.assertEqual(p.symbol_sizes(old, {"functions": {"2": {"module": "codegen", "name": "matchValue"}}}), {"codegen.matchValue": 30})
        self.assertEqual(p.symbol_sizes(new, {"functions": {"5": {"module": "codegen", "name": "matchValue"}}}), {"codegen.matchValue": 40})

    @unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64", "ELF64 Linux control")
    def test_real_elf_file_accounting(self):
        info = p.elf_record(Path(sys.executable).resolve())
        self.assertEqual(info["machine"], 62)
        self.assertLessEqual(info["section_file_bytes"], info["bytes"])
        self.assertTrue(any(s["name"] == ".text" and s["size"] > 0 for s in info["sections"]))
        self.assertTrue(all(s["file_bytes"] == 0 for s in info["sections"] if s["type"] == 8))


@unittest.skipUnless(platform.system() == "Linux" and shutil.which("cc"), "requires a native C compiler; no baseline build")
class MeterControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.root = Path(cls.directory.name)
        cls.meter = cls.root / "meter"
        subprocess.run(["cc", "-std=c17", "-O2", "-Wall", "-Wextra", "-Werror", str(HERE / "perf_wait4.c"), "-o", str(cls.meter)], check=True)

    @classmethod
    def tearDownClass(cls): cls.directory.cleanup()

    def invoke(self, command, timeout=5):
        directory = Path(tempfile.mkdtemp(dir=self.root))
        args = [str(self.meter), str(directory / "usage.json"), str(directory / "stdout"), str(directory / "stderr"), str(timeout), "--", *command]
        rc = subprocess.run(args, timeout=timeout + 5, check=False).returncode
        return rc, json.loads((directory / "usage.json").read_bytes()), (directory / "stdout").read_bytes()

    def test_exact_exit_and_binary_stdout(self):
        rc, usage, stdout = self.invoke([sys.executable, "-c", "import sys;sys.stdout.buffer.write(bytes([255,0,10]));sys.exit(7)"])
        self.assertEqual((rc, usage["exit_code"], stdout), (7, 7, bytes([255, 0, 10])))
        self.assertFalse(usage["timed_out"])
        self.assertIn("major_faults", usage["rusage"])
        self.assertIn("involuntary_context_switches", usage["rusage"])

    def test_large_python_parent_does_not_set_child_rss_floor(self):
        retained = bytearray(64 * 1024 * 1024)
        for i in range(0, len(retained), 4096): retained[i] = 1
        rc, usage, _ = self.invoke(["/bin/true"])
        self.assertEqual(rc, 0)
        self.assertLess(usage["rusage"]["maxrss_kib"], 16 * 1024)
        self.assertGreater(len(retained), 60 * 1024 * 1024)

    def test_timeout_only_terminates_the_measured_group(self):
        rc, usage, _ = self.invoke([sys.executable, "-c", "import time;time.sleep(10)"], timeout=1)
        self.assertTrue(usage["timed_out"])
        self.assertEqual(rc, 137)
        self.assertEqual(usage["signal"], 9)


if __name__ == "__main__":
    unittest.main()
