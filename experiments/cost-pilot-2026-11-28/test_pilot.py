import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("pilot", Path(__file__).with_name("pilot.py"))
pilot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pilot)

class MeteringTests(unittest.TestCase):
    def test_missing_and_duplicate_turns_do_not_become_zero_tokens(self):
        for events in [[], [None], [{"type": "turn.completed"}],
                       [{"type": "turn.completed", "usage": None}],
                       [{"type": "turn.completed"}, {"type": "turn.completed"}],
                       [{"type": "turn.completed", "usage": {"input_tokens": True, "output_tokens": 1}}]]:
            with self.assertRaises(ValueError):
                pilot.usage(events)

    def test_cached_tokens_are_subset_not_added_twice(self):
        result = pilot.usage([{"type": "turn.completed", "usage": {"input_tokens": 100, "output_tokens": 20, "cached_input_tokens": 80}}])
        self.assertEqual(result["total_tokens"], 120)
        self.assertEqual(result["cached_input_tokens"], 80)
        for invalid in [{"input_tokens": 1, "output_tokens": 2, "cached_input_tokens": 3},
                        {"input_tokens": 1, "output_tokens": 2, "total_tokens": 99}]:
            with self.assertRaises(ValueError):
                pilot.usage([{"type": "turn.completed", "usage": invalid}])

    def test_source_cannot_escape_snapshot(self):
        for name in ["../main.rs", "/main.rs", "a\\main.rs", "main.py"]:
            with tempfile.TemporaryDirectory() as d:
                with self.assertRaises(ValueError):
                    pilot.source_files(json.dumps({"files": [{"path": name, "content": ""}]}), Path(d) / "source", "rust")

    def test_real_process_retains_crlf_and_timeout_failure(self):
        with tempfile.TemporaryDirectory() as d:
            result = pilot.run_process([sys.executable, "-c", "import sys;sys.stdout.buffer.write(b'x\\r\\n')"], d)
            self.assertEqual(result["stdout"], "x\r\n")
            result = pilot.run_process([sys.executable, "-c", "import time;time.sleep(5)"], d, timeout=0.05)
            self.assertTrue(result["timeout"])
            self.assertIsNone(result["exit_code"])

if __name__ == "__main__":
    unittest.main()
