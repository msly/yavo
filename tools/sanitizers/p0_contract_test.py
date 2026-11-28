#!/usr/bin/env python3
"""Small P0 inventory/oracle controls and a real external-compiler protocol probe."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
import p0_contract as contract

ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "tools/sanitizers/run.py"


class P0ContractTests(unittest.TestCase):
    def fixtures(self, root):
        directory = root / "compiler/tests/p0"
        directory.mkdir(parents=True)
        (root / contract.ORACLE).write_bytes((ROOT / contract.ORACLE).read_bytes())
        (root / "tools/sanitizers").mkdir(parents=True)
        (root / "tools/sanitizers/expectations.json").write_bytes(b"{}\n")
        (root / "compiler/runtime").mkdir()
        header = b"/* protocol control: generated C does not require the Yavo runtime */\n"
        (root / "compiler/runtime/yavo_rt.h").write_bytes(header)
        (directory / "run_protocol.yv").write_text(
            '// expect-stdout:\n// | compiler\n// | eof\nfn main() {}\n', encoding="utf-8")
        (directory / "panic_protocol.yv").write_text(
            '// expect-panic: P-ASSERT 1:1\nfn main() {}\n', encoding="utf-8")
        (directory / "reject_escape.yv").write_text(
            '// expect-error: E0305 1:1\nfn main() {}\n', encoding="utf-8")
        native = {
            "run": '#include <stdio.h>\n#include <string.h>\n#include <unistd.h>\n'
                   'int main(void) { char cwd[4096]; if (!getcwd(cwd,sizeof(cwd))) return 9;'
                   'puts(strrchr(cwd,\'/\')+1); if (getchar()!=EOF) return 8; puts("eof"); return 0; }\n',
            "panic": '#include <stdio.h>\nint main(void) {'
                     'fputs("yavo panic [P-ASSERT] at control.yv:1:1: control\\n",stderr); return 101; }\n',
        }
        programs = []
        for row in contract.inventory(root)["cases"]:
            data = (root / row["entry"]).read_bytes()
            c = native[row["kind"]]
            programs.append({"id": row["id"], "entry": row["entry"], "source": data.decode(),
                             "source_sha256": contract.sha(data), "c": c,
                             "c_sha256": contract.sha(c.encode()), "expected": row["expected"]})
        return {"schema": "yavo-independent-sanitizer-bundle/1", "programs": programs,
                "source_revision": "protocol-control", "git_status": "", "repository_inputs": [],
                "compiler": {"scope": "handwritten C controls; no Yavo compiler execution"},
                "runtime": {"content": header.decode(), "sha256": contract.sha(header)}}

    def test_complete_inventory_rejects_omission_duplicates_and_new_unexported_entry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.fixtures(root)
            checked = contract.validate_bundle(root, bundle)
            self.assertEqual([row["kind"] for row in checked["cases"]], ["run", "panic"])
            self.assertEqual(checked["posix_not_applicable"], [])
            for programs in [bundle["programs"][:-1], bundle["programs"] + bundle["programs"][:1]]:
                changed = dict(bundle, programs=programs)
                with self.assertRaisesRegex(ValueError, "inventory differs"):
                    contract.validate_bundle(root, changed)
            (root / "compiler/tests/p0/run_new.yv").write_text(
                '// expect-stdout:\nfn main() {}\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "inventory differs"):
                contract.validate_bundle(root, bundle)

    def test_oracle_input_and_rejected_source_changes_fail_before_native_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = self.fixtures(root)
            for field, value in [("stdout", "learned from a candidate\n"), ("exit_code", 9)]:
                changed = copy.deepcopy(bundle)
                changed["programs"][0]["expected"][field] = value
                with self.assertRaisesRegex(ValueError, "frozen oracle differs"):
                    contract.validate_bundle(root, changed)
            changed = copy.deepcopy(bundle)
            changed["programs"][0]["isolated_cwd"] = True
            with self.assertRaisesRegex(ValueError, "original stdin/cwd/fixture"):
                contract.validate_bundle(root, changed)
            with self.assertRaisesRegex(ValueError, "must not enter"):
                contract.expected(b"// expect-error: E0305 1:1\nfn main() {}\n")
            with self.assertRaisesRegex(ValueError, "stale"):
                contract.expected(b"// expect-stdout:\n", {"source_sha256": "0" * 64})
            (root / contract.ORACLE).write_bytes(b"changed harness")
            with self.assertRaisesRegex(ValueError, "protocol changed"):
                contract.inventory(root)

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("gcc"),
                         "real protocol control requires external Linux GCC")
    def test_real_runner_preserves_package_cwd_devnull_and_all_result_rows(self):
        with tempfile.TemporaryDirectory(prefix="p0-contract-") as directory:
            root = Path(directory) / "checkout with space"
            bundle = self.fixtures(root)
            bundle_path, output = root / "bundle.json", root / "external-results"
            bundle_path.write_text(json.dumps(bundle, ensure_ascii=False), encoding="utf-8")
            run = subprocess.run(
                [sys.executable, str(RUNNER), str(bundle_path), "--cc", shutil.which("gcc"),
                 "--jobs", "1", "--p0-root", str(root), "--out", str(output)],
                capture_output=True, text=True, encoding="utf-8", timeout=45)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            report = json.loads((output / "report.json").read_bytes())
            self.assertEqual(report["status"], "passed")
            for kind in ("run", "panic"):
                self.assertEqual(report["p0_coverage"]["counts"][kind],
                                 {"required": 1, "executed": 1, "passed": 1, "failed": 0})
            for row in report["results"]:
                self.assertEqual(row["execution"]["cwd"], str(root / "compiler"))
                self.assertEqual(row["execution"]["stdin"], {"kind": "devnull"})
            with self.assertRaisesRegex(ValueError, "incomplete or duplicated"):
                contract.coverage(report["p0_contract"], report["results"][:-1])


if __name__ == "__main__":
    unittest.main(verbosity=2)
