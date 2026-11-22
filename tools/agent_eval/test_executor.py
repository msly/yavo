#!/usr/bin/env python3
"""Offline contract tests. Synthetic captures never qualify a live trial."""
import contextlib
import copy
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import executor as e


class ExecutorContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "tools").mkdir()
        (self.root / "evidence").mkdir()
        self.script = self.root / "tools/lab.pl"
        self.script.write_text('use strict; use warnings; die "API key leaked" if exists $ENV{OPENAI_API_KEY}; print "{\\"status\\":\\"sealed\\"}\\n";\n')
        self.path = self.root / "EXECUTOR_CONFIG.json"
        self.value = {
            "schema_version": 1, "experiment_id": "fixture-experiment",
            "freeze": {"path": "evidence/frozen.json", "sha256": None},
            "provider": {"endpoint": e.ENDPOINT, "requested_model": "fixture-alias",
                         "expected_reported_model": "fixture-snapshot", "parameters": {"max_output_tokens": 512}},
            "lab": {"program": "perl", "script": "tools/lab.pl", "timeout_seconds": 1, "output_bytes": 1024},
            "limits": {"max_provider_requests": 2, "provider_timeout_seconds": 1, "max_response_bytes": 4096},
        }
        self.save_config()
        self.freeze()
        self.quiet = contextlib.redirect_stdout(io.StringIO())
        self.quiet.__enter__()
        self.addCleanup(self.quiet.__exit__, None, None, None)

    def save_config(self):
        self.path.write_text(json.dumps(self.value))
        self.config = e.Config(self.path)

    def freeze(self):
        identity = self.config.binding()
        identity.pop("freeze_sha256")
        identity["identity_files"] = {str(self.script.resolve()): e.sha(self.script.read_bytes())}
        for prompt in self.root.glob("prompt*.txt"):
            identity["identity_files"][str(prompt.resolve())] = e.sha(prompt.read_bytes())
        raw = e.canonical(identity)
        (self.root / "evidence/frozen.json").write_bytes(raw)
        self.value["freeze"]["sha256"] = e.sha(raw)
        self.save_config()

    def response(self, request):
        return {
            "id": "resp_fixture", "object": "response", "status": "completed", "error": None,
            "model": "fixture-snapshot", "temperature": 1.0, "top_p": 1.0,
            "reasoning": {"effort": None}, "service_tier": "default", "max_output_tokens": 512,
            "output": [{"type": "message", "id": "msg_fixture", "role": "assistant",
                        "content": [{"type": "output_text", "text": "READY"}]}],
            "usage": {"input_tokens": 10, "output_tokens": 3, "total_tokens": 13,
                      "input_tokens_details": {"cached_tokens": 3},
                      "output_tokens_details": {"reasoning_tokens": 1}},
        }

    def capture(self, mutate=None, smoke=True, name="capture", transport=None):
        request = self.config.request([{"role": "user", "content": "READY"}], smoke=smoke)
        if transport is None:
            def transport(raw):
                response = self.response(e.parse_json(raw))
                if mutate:
                    mutate(response)
                return 200, {"x-request-id": "req_fixture"}, e.canonical(response)
        target = self.root / name
        e.capture_response(self.config, request, target, fixture_transport=transport)
        return target

    def test_complete_fixture_has_real_counter_shape_but_never_live_qualification(self):
        target = self.capture()
        result = e.normalize_capture(self.config, target)
        self.assertEqual(result["status"], "complete")
        self.assertEqual(result["usage"]["total_tokens"], 13)
        self.assertIsNone(result["usage"]["cache_write_input_tokens"])
        self.assertEqual(result["settings"]["temperature"]["request_source"], "provider_default")
        receipt = e.qualification(self.config, None, target)
        self.assertEqual(receipt["status"], "blocked")
        self.assertIsNone(receipt["qualified_at_utc"])

    def fixture_chain(self, name="chain", wrong_operation=False):
        directory = self.root / name
        directory.mkdir()
        original_capture = e.capture_response
        count = 0
        def fake_capture(config, request, out):
            nonlocal count
            count += 1
            number = count
            def transport(raw):
                response = self.response(e.parse_json(raw))
                response["id"] = f"resp_fixture_{number}"
                if number == 1:
                    operation = {"operation": "doc", "args": ["guide" if wrong_operation else "meter-probe", "1"], "stdin": None}
                    response["output"] = [{"type": "function_call", "id": "fc_fixture", "call_id": "call_fixture",
                                           "name": "lab", "arguments": e.canonical(operation).decode()}]
                return 200, {"x-request-id": f"req_fixture_{number}"}, e.canonical(response)
            return original_capture(config, request, out, fixture_transport=transport)
        with mock.patch.object(e, "capture_response", side_effect=fake_capture):
            e.qualification_probe(self.config, directory)
        return directory

    def test_two_response_controlled_tool_fixture_is_only_fixture_qualified(self):
        directory = self.fixture_chain()
        result = e.qualification(self.config, None, directory)
        self.assertEqual(result["status"], "fixture_only", result["blockers"])
        self.assertEqual(result["observed"]["provider_requests"], 2)
        self.assertEqual(result["observed"]["qualifying_tool_calls"], 1)
        self.assertTrue(result["observed"]["per_request_evidence"])
        self.assertEqual(result["observed"]["usage"]["total_tokens"], 26)

    def test_qualification_probe_cannot_read_language_guide(self):
        directory = self.fixture_chain(wrong_operation=True)
        self.assertFalse((directory / "lab").exists())
        result = e.qualification(self.config, None, directory)
        self.assertEqual(result["status"], "blocked")
        self.assertEqual(result["observed"]["provider_requests"], 1)
        self.assertEqual(result["observed"]["qualifying_tool_calls"], 0)

    def test_continuation_must_consume_exact_recorded_tool_output(self):
        directory = self.fixture_chain()
        request_path = directory / "requests/0002/request.json"
        request = e.load_json(request_path)
        request["input"][-1]["output"] = "different tool output"
        request_path.write_bytes(e.canonical(request))
        capture_path = directory / "requests/0002/capture.json"
        capture = e.load_json(capture_path)
        capture["request"] = e.artifact(request_path)
        capture_path.write_bytes(e.canonical(capture))
        result = e.qualification(self.config, None, directory)
        self.assertEqual(result["status"], "blocked")
        self.assertTrue(any("did not consume exact" in x for x in result["blockers"]))

    def test_missing_usage_blocks(self):
        target = self.capture(lambda r: r.pop("usage"))
        self.assertEqual(e.normalize_capture(self.config, target)["status"], "blocked")

    def test_boolean_tokens_are_not_integers(self):
        target = self.capture(lambda r: r["usage"].update(input_tokens=True))
        self.assertIsNone(e.normalize_capture(self.config, target)["usage"])

    def test_inconsistent_token_total_blocks(self):
        target = self.capture(lambda r: r["usage"].update(total_tokens=99))
        self.assertEqual(e.normalize_capture(self.config, target)["status"], "blocked")

    def test_cached_subset_cannot_exceed_input(self):
        target = self.capture(lambda r: r["usage"]["input_tokens_details"].update(cached_tokens=11))
        self.assertEqual(e.normalize_capture(self.config, target)["status"], "blocked")

    def test_exact_reported_model_is_required(self):
        target = self.capture(lambda r: r.update(model="fixture-alias"))
        self.assertIn("reported_model_mismatch_or_missing", e.normalize_capture(self.config, target)["blockers"])

    def test_missing_reported_sampling_is_unknown(self):
        target = self.capture(lambda r: r.pop("temperature"))
        result = e.normalize_capture(self.config, target)
        self.assertFalse(result["settings"]["temperature"]["reported_present"])
        self.assertIn("reported_setting_missing:temperature", result["blockers"])

    def test_explicit_sampling_must_match_provider_report(self):
        self.value["provider"]["parameters"]["temperature"] = 0.2
        self.save_config(); self.freeze()
        target = self.capture()
        self.assertIn("reported_setting_mismatch:temperature", e.normalize_capture(self.config, target)["blockers"])

    def test_incomplete_response_blocks_even_with_usage(self):
        target = self.capture(lambda r: r.update(status="incomplete"))
        self.assertIn("provider_response_not_completed", e.normalize_capture(self.config, target)["blockers"])

    def test_provider_request_id_is_required(self):
        target = self.capture(transport=lambda raw: (200, {}, e.canonical(self.response(None))))
        self.assertIn("provider_request_id_missing", e.normalize_capture(self.config, target)["blockers"])

    def test_tampered_response_file_is_detected(self):
        target = self.capture()
        with (target / "response.json").open("ab") as stream:
            stream.write(b" ")
        self.assertEqual(e.normalize_capture(self.config, target)["status"], "blocked")

    def test_alternate_response_cannot_supply_usage_for_replayed_response(self):
        target = self.capture()
        original = (target / "response.json").read_bytes()
        (target / "alternate-response.json").write_bytes(original)
        wrong = e.parse_json(original)
        wrong["model"] = "UNEXPECTED"
        wrong["usage"] = None
        (target / "response.json").write_bytes(e.canonical(wrong))
        meta = e.load_json(target / "capture.json")
        meta["response"] = e.artifact(target / "alternate-response.json")
        (target / "capture.json").write_bytes(e.canonical(meta))
        normalized = e.normalize_capture(self.config, target)
        self.assertEqual(normalized["status"], "blocked")
        self.assertIsNone(normalized["usage"])

    def test_unknown_capture_kind_is_rejected(self):
        target = self.capture()
        meta = e.load_json(target / "capture.json")
        meta["capture_kind"] = "unverified"
        (target / "capture.json").write_bytes(e.canonical(meta))
        self.assertIn("capture_kind_invalid", e.normalize_capture(self.config, target)["blockers"])

    def test_duplicate_json_keys_are_rejected(self):
        with self.assertRaises(e.ContractError):
            e.parse_json('{"usage":1,"usage":2}')

    def test_unknown_or_hosted_tool_surface_blocks(self):
        target = self.capture(lambda r: r["output"].append({"type": "web_search_call", "id": "ws_fixture"}), smoke=False)
        self.assertIn("unsupported_output_type:web_search_call", e.normalize_capture(self.config, target)["blockers"])

    def test_multiple_function_calls_are_rejected(self):
        def mutate(r):
            r["output"] = [{"type": "function_call", "name": "lab", "call_id": str(n), "arguments": "{}"} for n in range(2)]
        target = self.capture(mutate, smoke=False)
        self.assertIn("parallel_function_calls_not_allowed", e.normalize_capture(self.config, target)["blockers"])

    def test_stateless_reasoning_replay_requires_encrypted_content(self):
        target = self.capture(lambda r: r["output"].append({"type": "reasoning", "id": "rs_fixture"}), smoke=False)
        self.assertIn("stateless_reasoning_item_missing_encrypted_content", e.normalize_capture(self.config, target)["blockers"])

    def test_changed_freeze_stops_before_transport(self):
        (self.root / "evidence/frozen.json").write_text("{}")
        transport = mock.Mock()
        with self.assertRaises(e.ContractError):
            self.capture(transport=transport)
        transport.assert_not_called()

    def test_changed_lab_source_stops_before_transport(self):
        self.script.write_text("print 1;")
        transport = mock.Mock()
        with self.assertRaises(e.ContractError):
            self.capture(transport=transport)
        transport.assert_not_called()

    def test_lab_redirect_in_config_is_not_permitted(self):
        self.value["lab"]["script"] = "tools/another.pl"
        self.save_config(); self.freeze()
        self.assertIn("fixed_lab_interface_required", self.config.blockers())

    def test_invalid_lab_limits_stop_before_provider_transport(self):
        original = copy.deepcopy(self.value["lab"])
        for field, invalid in (("timeout_seconds", 0), ("output_bytes", 12)):
            with self.subTest(field=field):
                self.value["lab"] = {**original, field: invalid}
                self.save_config(); self.freeze()
                transport = mock.Mock()
                with self.assertRaises(e.ContractError):
                    self.capture(transport=transport)
                transport.assert_not_called()
                self.assertIn("invalid_lab_limit:" + field, self.config.blockers())

    def test_template_is_blocked_without_network(self):
        self.value["provider"]["requested_model"] = None
        self.value["provider"]["expected_reported_model"] = None
        self.value["freeze"]["sha256"] = None
        self.save_config()
        events = io.StringIO()
        with mock.patch.object(e, "capture_response") as capture, contextlib.redirect_stdout(events):
            result = e.main(["qualify", "--config", str(self.path), "--out", str(self.root / "template-probe")])
        capture.assert_not_called()
        self.assertEqual(result, 2)
        receipt = e.load_json(self.root / "template-probe/qualification.json")
        self.assertEqual(receipt["observed"]["provider_requests"], 0)
        self.assertEqual(receipt["status"], "blocked")
        event = e.parse_json(events.getvalue().splitlines()[-1])
        self.assertEqual(event["event"], "result")
        self.assertEqual(event["kind"], "yavo.executor.qualification")

    def test_missing_api_key_records_zero_attempts(self):
        target = self.root / "no-key"
        with mock.patch.dict(os.environ, {}, clear=True):
            e.capture_response(self.config, self.config.request([], smoke=True), target)
        result = e.qualification(self.config, None, target)
        self.assertEqual(result["observed"]["provider_requests"], 0)
        self.assertEqual(result["status"], "blocked")

    def test_forged_qualified_status_does_not_override_fixture_capture(self):
        target = self.capture()
        receipt = e.qualification(self.config, None, target)
        receipt["status"] = "qualified"
        e.write_json(self.root / "qualification.json", receipt)
        with self.assertRaises(e.ContractError):
            e.verify_qualification(self.config, self.root / "qualification.json")

    def test_receipt_path_traversal_is_rejected(self):
        with self.assertRaises(e.ContractError):
            e.child(self.root, "../credential.json")

    def test_tool_argument_strings_stay_strings_and_init_is_controller_only(self):
        item = {"type": "function_call", "name": "lab", "arguments": '{"operation":"doc","args":["guide","1"],"stdin":null}'}
        self.assertEqual(e.tool_arguments(item)["args"], ["guide", "1"])
        item["arguments"] = '{"operation":"init","args":[],"stdin":null}'
        with self.assertRaises(e.ContractError):
            e.tool_arguments(item)

    def test_lab_child_does_not_receive_api_key(self):
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "SYNTHETIC_TEST_SECRET"}):
            result = e.lab_operation(self.config, "fixture-a", "init", [], None, self.root / "lab-op", self.root / "q.json")
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(e.parse_json(result["stdout"])["status"], "sealed")
        self.assertNotIn("SYNTHETIC_TEST_SECRET", (self.root / "lab-op/operation.json").read_text())

    def test_lab_output_limit_fails_closed(self):
        self.script.write_text('print "x" x 2048;')
        self.freeze()
        with self.assertRaises(e.ContractError):
            e.lab_operation(self.config, "fixture-a", "doc", [], None, self.root / "lab-overflow", self.root / "q.json")
        self.assertTrue(e.load_json(self.root / "lab-overflow/operation.json")["output_limit"])

    def test_lab_timeout_fails_closed(self):
        self.script.write_text('sleep 3;')
        self.freeze()
        with self.assertRaises(e.ContractError):
            e.lab_operation(self.config, "fixture-a", "doc", [], None, self.root / "lab-timeout", self.root / "q.json")
        self.assertTrue(e.load_json(self.root / "lab-timeout/operation.json")["timed_out"])

    def test_unmetered_response_stops_before_next_participant_operation(self):
        # Only the qualification verifier is stubbed. The request capture and
        # normalizer execute against an explicitly synthetic, incomplete reply.
        prompt = self.root / "prompt.txt"
        prompt.write_text("synthetic task")
        self.freeze()
        receipt = self.root / "synthetic-qualification.json"
        receipt.write_text("{}")
        original_capture = e.capture_response
        def fixture_capture(config, request, directory):
            def transport(raw):
                response = self.response(e.parse_json(raw))
                response.pop("usage")
                return 200, {"x-request-id": "req_incomplete_fixture"}, e.canonical(response)
            return original_capture(config, request, directory, fixture_transport=transport)
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "SYNTHETIC_TEST_SECRET"}), \
             mock.patch.object(e, "verify_qualification", return_value={"observed": {"reported_settings": {}}}), \
             mock.patch.object(e, "capture_response", side_effect=fixture_capture), \
             mock.patch.object(e, "lab_operation", return_value={"exit_code": 0, "stdout": "", "stderr": ""}) as lab:
            result = e.run_agent(self.config, "fixture-a", "prompt.txt", receipt, self.root / "offline-run")
        self.assertEqual(lab.call_count, 1)
        self.assertEqual(lab.call_args.args[2], "init")
        self.assertEqual(result["status"], "blocked")
        self.assertEqual(result["controlled_lab_operations"], 0)
        self.assertFalse(result["usage_complete"])
        self.assertIsNone(result["usage"])


if __name__ == "__main__":
    unittest.main()
