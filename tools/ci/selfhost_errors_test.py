"""Controls for accepted reference warnings without relaxing the selfhost CLI."""
import copy
import json
import unittest

import selfhost_errors as acceptance


class OriginalLiteralChecks(unittest.TestCase):
    def setUp(self):
        self.source = acceptance.FIXTURES / "programs/utf8_consumed_failure.yv"
        self.command = {"exit_code": 0, "timed_out": False, "launch_error": None}
        self.document = {
            "schema": "yavo-diagnostics/p0",
            "source_sha256": acceptance.execution.digest(self.source),
            "error_count": 0,
            "warning_count": 1,
            "diagnostics": [{"code": "W0001", "severity": "WARN", "message": "unused variable `guard`"}],
        }

    def accepts(self, document, command=None, stderr=b""):
        return acceptance.original_literal_accepted(
            "stage0", self.source, self.command if command is None else command,
            json.dumps(document).encode(), stderr)

    def test_reference_success_accepts_valid_warning_or_no_warning(self):
        self.assertTrue(self.accepts(self.document))
        document = copy.deepcopy(self.document)
        document.update(warning_count=0, diagnostics=[])
        self.assertTrue(self.accepts(document))

    def test_errors_identity_counts_and_process_failure_are_rejected(self):
        changes = {
            "hidden_error_severity": lambda d: d["diagnostics"][0].__setitem__("severity", "ERROR"),
            "hidden_error_code": lambda d: d["diagnostics"][0].__setitem__("code", "E0001"),
            "nonstring_code": lambda d: d["diagnostics"][0].__setitem__("code", 1),
            "wrong_warning_count": lambda d: d.__setitem__("warning_count", 2),
            "boolean_error_count": lambda d: d.__setitem__("error_count", False),
            "wrong_source_identity": lambda d: d.__setitem__("source_sha256", "0" * 64),
            "wrong_schema": lambda d: d.__setitem__("schema", "other"),
        }
        for name, change in changes.items():
            with self.subTest(name=name):
                document = copy.deepcopy(self.document)
                change(document)
                self.assertFalse(self.accepts(document))
        for change in ({"exit_code": 1}, {"timed_out": True}, {"launch_error": "failed"}):
            with self.subTest(process=change):
                self.assertFalse(self.accepts(self.document, dict(self.command, **change)))
        self.assertFalse(self.accepts(self.document, stderr=b"unexpected warning outside JSON\n"))
        self.assertFalse(acceptance.original_literal_accepted(
            "stage0", self.source, self.command, b"{}\nextra output", b""))

    def test_selfhost_keeps_empty_stdout_and_stderr_contract(self):
        for role in ("stage1", "stage2"):
            with self.subTest(role=role):
                self.assertTrue(acceptance.original_literal_accepted(role, self.source, self.command, b"", b""))
                self.assertFalse(acceptance.original_literal_accepted(role, self.source, self.command, b"{}\n", b""))
                self.assertFalse(acceptance.original_literal_accepted(role, self.source, self.command, b"", b"W0001\n"))


if __name__ == "__main__":
    unittest.main()
