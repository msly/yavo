"""Offline transcript-contract fixtures; the qualification verifier is explicitly stubbed."""
from pathlib import Path
import json
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import executor as e
import audit


class TranscriptAuditContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="synthetic-transcript-contract-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "tools").mkdir()
        (self.root / "evidence").mkdir()
        script = self.root / "tools/lab.pl"
        script.write_text("# synthetic placeholder, never executed\n")
        prompt = self.root / "PROMPT.md"; prompt.write_text("synthetic transcript contract; no real task\n")
        self.path = self.root / "EXECUTOR_CONFIG.json"
        self.value = {"schema_version": 1, "experiment_id": "synthetic-audit-contract",
                      "provider": {"endpoint": e.ENDPOINT, "requested_model": "synthetic-alias", "expected_reported_model": "synthetic-snapshot",
                                   "parameters": {"max_output_tokens": 512}},
                      "freeze": {"path": "evidence/frozen.json", "sha256": None},
                      "lab": {"program": "perl", "script": "tools/lab.pl", "timeout_seconds": 1, "output_bytes": 1024},
                      "limits": {"max_provider_requests": 2, "provider_timeout_seconds": 1, "max_response_bytes": 4096}}
        self.dump(self.path, self.value)
        config = e.Config(self.path)
        frozen = config.binding(); frozen.pop("freeze_sha256")
        frozen["identity_files"] = {str(p.resolve()): audit.file_sha(p) for p in (script, prompt)}
        self.dump(self.root / "evidence/frozen.json", frozen)
        self.value["freeze"]["sha256"] = audit.file_sha(self.root / "evidence/frozen.json")
        self.dump(self.path, self.value)
        self.config = e.Config(self.path)
        self.qpath = self.root / "SYNTHETIC_QUALIFICATION.json"
        self.dump(self.qpath, {"scope": "test stub; never usable for real qualification"})
        self.out = self.root / "provider-run"
        self.out.mkdir()
        (self.out / "configuration.json").write_bytes(self.path.read_bytes())
        (self.out / "prompt.txt").write_bytes(prompt.read_bytes())
        initial = {"exit_code": 0, "stdout": '{"run_id":"full-a","status":"initialized"}\n', "stderr": ""}
        self.operation("000-init", "init", [], initial, 1.0)
        history = [{"role": "user", "content": prompt.read_text()}]
        events = []
        self.normalized = []
        for index, (op, args) in enumerate((("doc", ["guide", "1"]), ("seal", [])), 1):
            directory = self.out / "requests" / f"{index:04d}"; directory.mkdir(parents=True)
            request = self.config.request(history)
            values = {"operation": op, "args": args, "stdin": None}
            response = {"id": f"synthetic-response-{index}", "object": "response", "status": "completed", "error": None,
                        "model": "synthetic-snapshot", "temperature": 1.0, "top_p": 1.0, "reasoning": {"effort": None},
                        "service_tier": "default", "max_output_tokens": 512,
                        "output": [{"type": "function_call", "id": f"synthetic-item-{index}", "call_id": f"synthetic-call-{index}",
                                    "name": "lab", "arguments": e.canonical(values).decode()}],
                        "usage": {"input_tokens": 10 * index, "output_tokens": 3, "total_tokens": 10 * index + 3,
                                  "input_tokens_details": {"cached_tokens": 2}, "output_tokens_details": {"reasoning_tokens": 1}}}
            self.dump(directory / "request.json", request); self.dump(directory / "response.json", response)
            start = 1.2 + (index - 1) * 0.3
            capture = {"schema_version": 1, "kind": "yavo.provider.capture", "capture_kind": "live", **self.config.binding(),
                       "endpoint": e.ENDPOINT, "client_request_id": f"synthetic-client-{index}", "provider_request_id": f"synthetic-request-{index}",
                       "http_status": 200, "http_attempts": 1, "automatic_retries": False, "transport_error": None,
                       "started_monotonic": start, "finished_monotonic": start + 0.05, "elapsed_seconds": 0.05,
                       "request": e.artifact(directory / "request.json"), "response": e.artifact(directory / "response.json")}
            # 'live' exercises that schema branch only. No transport is called and
            # verify_qualification is stubbed inside this unit test, never persisted.
            self.dump(directory / "capture.json", capture)
            normalized = e.normalize_capture(self.config, directory)
            self.assertEqual(normalized["status"], "complete", normalized)
            self.dump(directory / "normalized.json", normalized)
            self.normalized.append(normalized)
            observed = {"exit_code": 0, "stdout": '{"content":"synthetic"}\n' if op == "doc" else '{"run_id":"full-a","status":"sealed"}\n', "stderr": ""}
            self.operation(f"{index:03d}-{op}", op, args, observed, start + 0.1)
            event = {"sequence": index, "run_id": "full-a", "operation": op, "arguments": args, "status": "completed",
                     "category": "initial_guide" if op == "doc" else "submit"}
            events.append(event)
            event_dir = self.root / "runs/full-a/operations" / f"{index:04d}"
            self.dump(event_dir / "event.json", event)
            (event_dir / "output.txt").write_text(observed["stdout"])
            history = history + response["output"] + [{"type": "function_call_output", "call_id": f"synthetic-call-{index}", "output": e.canonical(observed).decode()}]
        lab_root = self.root / "runs/full-a"
        self.dump(lab_root / "run.json", {"run_id": "full-a", "status": "sealed", "frozen_manifest_sha256": self.config.freeze["sha256"],
                                          "qualification_sha256": audit.file_sha(self.qpath), "operations": 2})
        (lab_root / "events.jsonl").write_text("".join(json.dumps(event) + "\n" for event in events))
        self.summary = {"schema_version": 1, "kind": "yavo.executor.run", **self.config.binding(), "run_id": "full-a",
                        "status": "completed", "sealed": True, "usage_complete": True, "blockers": [], "money_cost": None,
                        "qualification_sha256": audit.file_sha(self.qpath), "prompt_path": "PROMPT.md", "prompt_sha256": audit.file_sha(prompt),
                        "started_monotonic": 1.0, "finished_monotonic": 2.0, "elapsed_seconds": 1.0,
                        "provider_requests": 2, "provider_responses": 2, "captured_request_records": 2, "controlled_lab_operations": 2,
                        "controller_init_operations": 1,
                        "usage": {"input_tokens": 30, "output_tokens": 6, "total_tokens": 36, "cached_input_tokens": 4,
                                  "cache_write_input_tokens": None, "reasoning_output_tokens": 2}}
        self.rehash()

    def dump(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(e.canonical(value))

    def operation(self, name, operation, args, result, start):
        directory = self.out / "lab" / name; directory.mkdir(parents=True)
        for stream in ("stdin", "stdout", "stderr"):
            (directory / (stream + ".txt")).write_text(result.get(stream, ""))
        self.dump(directory / "operation.json", {"argv": ["/usr/bin/perl", str(self.root / "tools/lab.pl"), "full-a", operation, *args],
                                                 "cwd": str(self.root), "operation": operation, "args": args, "exit_code": result["exit_code"],
                                                 "timed_out": False, "output_limit": False, "api_key_passed_to_lab": False,
                                                 "started_monotonic": start, "elapsed_seconds": 0.05,
                                                 **{stream: e.artifact(directory / (stream + ".txt")) for stream in ("stdin", "stdout", "stderr")}})

    def rehash(self):
        files = {str(path.relative_to(self.out)): {"bytes": path.stat().st_size, "sha256": audit.file_sha(path)}
                 for path in self.out.rglob("*") if path.is_file() and path.relative_to(self.out).as_posix() not in ("run.json", "transcript-manifest.json")}
        self.dump(self.out / "transcript-manifest.json", {"schema_version": 1, "kind": "yavo.executor.transcript", **self.config.binding(), "run_id": "full-a", "files": files})
        self.summary["transcript_manifest_sha256"] = audit.file_sha(self.out / "transcript-manifest.json")
        self.dump(self.out / "run.json", self.summary)

    def verify(self):
        qualified = {"observed": {"reported_settings": self.normalized[0]["settings"]}}
        with mock.patch.object(e, "verify_qualification", return_value=qualified):
            return audit.verify_run(self.config, self.out, self.qpath)

    def test_consistent_synthetic_branch_recomputes_every_request(self):
        result = self.verify()
        self.assertEqual(result["status"], "verified", result["blockers"])
        self.assertEqual(result["usage"]["total_tokens"], 36)
        self.assertIsNone(result["usage"]["cache_write_input_tokens"])
        self.assertEqual(len(result["verified_request_records"]), 2)

    def test_real_qualification_verifier_never_accepts_fixture_stub(self):
        result = audit.verify_run(self.config, self.out, self.qpath)
        self.assertEqual(result["status"], "blocked")
        self.assertIsNone(result["usage"])

    def test_fixture_capture_cannot_join_real_cost(self):
        path = self.out / "requests/0001/capture.json"
        value = e.load_json(path); value["capture_kind"] = "fixture"; self.dump(path, value); self.rehash()
        result = self.verify()
        self.assertEqual(result["status"], "blocked"); self.assertIsNone(result["usage"])

    def test_missing_usage_is_not_zero(self):
        path = self.out / "requests/0002/response.json"
        value = e.load_json(path); value.pop("usage"); self.dump(path, value)
        capture_path = path.parent / "capture.json"; capture = e.load_json(capture_path); capture["response"] = e.artifact(path); self.dump(capture_path, capture)
        self.dump(path.parent / "normalized.json", e.normalize_capture(self.config, path.parent)); self.rehash()
        result = self.verify()
        self.assertEqual(result["status"], "blocked"); self.assertIsNone(result["usage"])

    def test_forged_summary_usage_rejected(self):
        self.summary["usage"]["total_tokens"] = 99999; self.rehash()
        result = self.verify()
        self.assertEqual(result["status"], "blocked"); self.assertIn("run usage", result["blockers"][0])

    def test_request_continuation_must_consume_exact_result(self):
        path = self.out / "requests/0002/request.json"
        value = e.load_json(path); value["input"][-1]["output"] = "different result"; self.dump(path, value)
        capture_path = path.parent / "capture.json"; capture = e.load_json(capture_path); capture["request"] = e.artifact(path); self.dump(capture_path, capture)
        self.dump(path.parent / "normalized.json", e.normalize_capture(self.config, path.parent)); self.rehash()
        result = self.verify()
        self.assertEqual(result["status"], "blocked"); self.assertIn("exact recorded history", result["blockers"][0])

    def test_unrecorded_nested_run_json_is_detected(self):
        self.dump(self.out / "requests/0001/run.json", {"unaccounted": True})
        result = self.verify()
        self.assertEqual(result["status"], "blocked"); self.assertIn("inventory", result["blockers"][0])


if __name__ == "__main__":
    unittest.main()
