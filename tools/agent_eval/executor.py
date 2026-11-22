#!/usr/bin/env python3
"""Recorded Responses API executor. Missing telemetry prevents the next lab step."""
from __future__ import annotations

import argparse
import collections
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid

SCHEMA = 1
ENDPOINT = "https://api.openai.com/v1/responses"
OPERATIONS = ("doc", "write", "read", "build", "public", "run", "submit", "seal")
LAB_TOOL = {
    "type": "function", "name": "lab", "strict": True,
    "description": "Execute one permitted lab operation. The controller fixes the run ID. Use args as strings; stdin is UTF-8 text or null. No shell is available.",
    "parameters": {
        "type": "object", "additionalProperties": False,
        "properties": {
            "operation": {"type": "string", "enum": list(OPERATIONS)},
            "args": {"type": "array", "items": {"type": "string"}},
            "stdin": {"type": ["string", "null"]},
        },
        "required": ["operation", "args", "stdin"],
    },
}
PARAMETERS = {"reasoning", "temperature", "top_p", "max_output_tokens", "service_tier"}
REPORTED_FIELDS = ("temperature", "top_p", "reasoning.effort", "service_tier", "max_output_tokens")
SHA_RE = re.compile(r"[0-9a-f]{64}\Z")
QUALIFY_PROMPT = ('Call the lab function exactly once with operation "doc", '
                  'args ["meter-probe", "1"], and stdin null. After receiving '
                  'that tool result, reply with exactly READY. Do not call any '
                  'other operation or access task or language documentation.')


class ContractError(ValueError):
    pass


def utc():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def unique_object(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ContractError("duplicate JSON key: " + key)
        out[key] = value
    return out


def parse_json(raw):
    def bad_constant(value):
        raise ContractError("non-finite JSON number: " + value)
    return json.loads(raw, object_pairs_hook=unique_object, parse_constant=bad_constant)


def load_json(path):
    return parse_json(Path(path).read_bytes())


def write_bytes(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(data)


def write_json(path, value):
    write_bytes(path, (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False) + "\n").encode("utf-8"))


def artifact(path):
    path = Path(path)
    data = path.read_bytes()
    return {"file": path.name, "bytes": len(data), "sha256": sha(data)}


def child(root, relative):
    if not isinstance(relative, str) or not relative or Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise ContractError("expected a non-traversing relative path")
    root = Path(root).resolve()
    path = root / relative
    if path.is_symlink() or not path.resolve().is_relative_to(root):
        raise ContractError("path escapes its evidence root or is a symlink")
    return path


def nonnegative_int(value, label):
    if type(value) is not int or value < 0:
        raise ContractError(label + " must be a nonnegative integer")
    return value


def finite_number(value, label):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ContractError(label + " must be finite")
    return value


def get_field(obj, dotted):
    value = obj
    for part in dotted.split("."):
        if not isinstance(value, dict) or part not in value:
            return False, None
        value = value[part]
    return True, value


def stream_event(event_name, **values):
    print(json.dumps({"event": event_name, "at_utc": utc(), **values}, ensure_ascii=False, allow_nan=False), flush=True)


class Config:
    def __init__(self, path):
        self.path = Path(path).resolve()
        self.root = self.path.parent
        self.raw = self.path.read_bytes()
        self.value = parse_json(self.raw)
        if not isinstance(self.value, dict) or type(self.value.get("schema_version")) is not int or self.value.get("schema_version") != SCHEMA:
            raise ContractError("unsupported configuration schema")
        self.provider = self.value.get("provider", {})
        if not isinstance(self.provider, dict):
            raise ContractError("provider must be an object")
        self.parameters = self.provider.get("parameters", {})
        if not isinstance(self.parameters, dict) or set(self.parameters) - PARAMETERS:
            raise ContractError("unsupported provider parameters")
        if self.provider.get("endpoint", ENDPOINT) != ENDPOINT:
            raise ContractError("only the explicitly supported official Responses endpoint is accepted")
        self.freeze = self.value.get("freeze", {})
        self.lab = self.value.get("lab", {})
        self.limits = self.value.get("limits", {})
        if any(not isinstance(value, dict) for value in (self.freeze, self.lab, self.limits)):
            raise ContractError("freeze, lab and limits must be objects")

    def binding(self):
        policy = {"provider": self.provider, "wire_api": "responses", "store": False,
                  "parallel_tool_calls": False, "include": ["reasoning.encrypted_content"],
                  "lab_tool": LAB_TOOL, "lab": self.lab, "limits": self.limits}
        return {"experiment_id": self.value.get("experiment_id"),
                "freeze_sha256": self.freeze.get("sha256"),
                "model_settings_sha256": sha(canonical(policy)),
                "executor_sha256": sha(Path(__file__).read_bytes())}

    def blockers(self):
        errors = []
        if self.path.read_bytes() != self.raw:
            errors.append("configuration_changed")
        if not isinstance(self.value.get("experiment_id"), str) or not self.value["experiment_id"]:
            errors.append("experiment_id_missing")
        for field in ("requested_model", "expected_reported_model"):
            if not isinstance(self.provider.get(field), str) or not self.provider[field].strip():
                errors.append(field + "_missing")
        if not isinstance(self.freeze.get("sha256"), str) or not SHA_RE.fullmatch(self.freeze["sha256"]):
            errors.append("freeze_sha256_missing")
        else:
            try:
                frozen = child(self.root, self.freeze.get("path"))
                if not frozen.is_file() or sha(frozen.read_bytes()) != self.freeze["sha256"]:
                    errors.append("freeze_identity_mismatch")
                else:
                    identity = load_json(frozen)
                    for field in ("experiment_id", "model_settings_sha256", "executor_sha256"):
                        if identity.get(field) != self.binding()[field]:
                            errors.append("frozen_binding_mismatch:" + field)
                    script = child(self.root, "tools/lab.pl")
                    expected = identity.get("identity_files", {}).get(str(script.resolve()))
                    if not script.is_file() or expected != sha(script.read_bytes()):
                        errors.append("frozen_lab_identity_mismatch")
            except (ContractError, OSError, ValueError, TypeError, AttributeError):
                errors.append("freeze_identity_unreadable")
        if self.lab.get("program", "perl") != "perl" or self.lab.get("script", "tools/lab.pl") != "tools/lab.pl":
            errors.append("fixed_lab_interface_required")
        for name, minimum, maximum, default in (
            ("max_provider_requests", 1, 1024, 128),
            ("provider_timeout_seconds", 1, 600, 120),
            ("max_response_bytes", 1024, 67108864, 16777216),
        ):
            val = self.limits.get(name, default)
            if type(val) is not int or not minimum <= val <= maximum:
                errors.append("invalid_limit:" + name)
        for name, minimum, maximum, default in (
            ("timeout_seconds", 1, 600, 180),
            ("output_bytes", 1024, 16777216, 1048576),
        ):
            val = self.lab.get(name, default)
            if type(val) is not int or not minimum <= val <= maximum:
                errors.append("invalid_lab_limit:" + name)
        mot = self.parameters.get("max_output_tokens")
        if type(mot) is not int or not 1 <= mot <= 131072:
            errors.append("explicit_max_output_tokens_required")
        # An omitted setting is a provider default, never a fabricated resolved value.
        for name in ("temperature", "top_p"):
            if name in self.parameters:
                val = self.parameters[name]
                if type(val) not in (int, float) or not math.isfinite(val):
                    errors.append("invalid_parameter:" + name)
        if "reasoning" in self.parameters:
            reasoning = self.parameters["reasoning"]
            if not isinstance(reasoning, dict) or set(reasoning) - {"effort", "summary"} or not isinstance(reasoning.get("effort"), str):
                errors.append("explicit_reasoning_effort_invalid")
        return errors

    def assert_current(self):
        errors = self.blockers()
        if errors:
            raise ContractError("; ".join(errors))

    def request(self, inputs, smoke=False):
        return {"model": self.provider["requested_model"], **self.parameters,
                "input": inputs, "store": False, "parallel_tool_calls": False,
                "include": ["reasoning.encrypted_content"], "tools": [] if smoke else [LAB_TOOL]}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ContractError("provider redirects are not accepted")


def capture_response(config, request, directory, fixture_transport=None):
    """One HTTP attempt, with no automatic retry and no authorization bytes logged."""
    config.assert_current()
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    request_raw = canonical(request)
    write_bytes(directory / "request.json", request_raw)
    client_id = str(uuid.uuid4())
    meta = {"schema_version": SCHEMA, "kind": "yavo.provider.capture",
            "capture_kind": "fixture" if fixture_transport else "live", **config.binding(),
            "endpoint": ENDPOINT, "client_request_id": client_id,
            "provider_request_id": None, "http_status": None, "http_attempts": 0,
            "automatic_retries": False, "started_at_utc": utc(),
            "started_monotonic": time.monotonic(), "transport_error": None,
            "request": artifact(directory / "request.json")}
    raw = b""
    key = None
    try:
        if fixture_transport:
            meta["http_attempts"] = 1
            status, headers, raw = fixture_transport(request_raw)
        else:
            key = os.environ.get("OPENAI_API_KEY")
            if not key:
                raise ContractError("OPENAI_API_KEY is not available; no request sent")
            req = urllib.request.Request(ENDPOINT, data=request_raw, method="POST", headers={
                "Authorization": "Bearer " + key, "Content-Type": "application/json",
                "X-Client-Request-Id": client_id,
            })
            opener = urllib.request.build_opener(NoRedirect())
            meta["http_attempts"] = 1
            try:
                response = opener.open(req, timeout=config.limits.get("provider_timeout_seconds", 120))
            except urllib.error.HTTPError as error:
                response = error
            with response:
                status = response.status
                headers = {"x-request-id": response.headers.get("x-request-id")}
                raw = response.read(config.limits.get("max_response_bytes", 16777216) + 1)
        meta["http_status"] = status
        meta["provider_request_id"] = headers.get("x-request-id")
        if len(raw) > config.limits.get("max_response_bytes", 16777216):
            raise ContractError("provider response exceeds configured byte limit")
    except Exception as error:
        message = str(error)
        if key:
            message = message.replace(key, "<redacted>")
        meta["transport_error"] = type(error).__name__ + ": " + message[:1000]
    meta["finished_monotonic"] = time.monotonic()
    meta["elapsed_seconds"] = meta["finished_monotonic"] - meta["started_monotonic"]
    write_bytes(directory / "response.json", raw)
    meta["response"] = artifact(directory / "response.json")
    write_json(directory / "capture.json", meta)
    stream_event("provider_request_recorded", directory=str(directory), http_attempts=meta["http_attempts"],
                 http_status=meta["http_status"], provider_request_id=meta["provider_request_id"],
                 elapsed_seconds=meta["elapsed_seconds"], transport_error=meta["transport_error"])
    return meta


def verify_artifact(root, entry):
    if not isinstance(entry, dict):
        raise ContractError("artifact entry is missing")
    path = child(root, entry.get("file"))
    raw = path.read_bytes()
    if type(entry.get("bytes")) is not int or entry["bytes"] != len(raw) or entry.get("sha256") != sha(raw):
        raise ContractError("artifact identity mismatch: " + path.name)
    return parse_json(raw)


def normalize_capture(config, directory):
    directory = Path(directory)
    meta = load_json(directory / "capture.json")
    if not isinstance(meta, dict):
        raise ContractError("capture metadata must be an object")
    errors = config.blockers()
    if meta.get("schema_version") != SCHEMA or meta.get("kind") != "yavo.provider.capture":
        errors.append("capture_schema_invalid")
    if meta.get("capture_kind") not in ("live", "fixture"):
        errors.append("capture_kind_invalid")
    for key, expected in config.binding().items():
        if meta.get(key) != expected:
            errors.append("capture_binding_mismatch:" + key)
    if meta.get("endpoint") != ENDPOINT or type(meta.get("http_attempts")) is not int or meta.get("http_attempts") != 1 or meta.get("automatic_retries") is not False:
        errors.append("single_attempt_transport_not_proven")
    if meta.get("http_status") != 200 or meta.get("transport_error") is not None:
        errors.append("transport_not_successful")
    if not isinstance(meta.get("provider_request_id"), str) or not meta["provider_request_id"]:
        errors.append("provider_request_id_missing")
    observed = {"provider_request_id": meta.get("provider_request_id"), "provider_response_id": None,
                "http_attempts": meta.get("http_attempts"),
                "requested_model": None, "reported_model": None, "usage": None,
                "settings": {}, "tool_calls": [], "tool_accounting_scope": "declared Responses function tools",
                "elapsed_seconds": meta.get("elapsed_seconds"), "capture_kind": meta.get("capture_kind")}
    try:
        for name in ("request", "response"):
            entry = meta.get(name)
            if not isinstance(entry, dict) or entry.get("file") != name + ".json":
                raise ContractError("capture artifact must use its canonical filename: " + name)
        start = finite_number(meta.get("started_monotonic"), "started_monotonic")
        end = finite_number(meta.get("finished_monotonic"), "finished_monotonic")
        elapsed = finite_number(meta.get("elapsed_seconds"), "elapsed_seconds")
        if end < start or elapsed < 0 or abs((end - start) - elapsed) > 1e-6:
            raise ContractError("provider timing is inconsistent")
        request = verify_artifact(directory, meta.get("request"))
        response = verify_artifact(directory, meta.get("response"))
        if not isinstance(request, dict) or not isinstance(response, dict):
            raise ContractError("request/response must be JSON objects")
        observed["requested_model"] = request.get("model")
        observed["reported_model"] = response.get("model")
        observed["provider_response_id"] = response.get("id")
        if request.get("model") != config.provider.get("requested_model"):
            errors.append("requested_model_mismatch")
        if response.get("model") != config.provider.get("expected_reported_model") or not isinstance(response.get("model"), str):
            errors.append("reported_model_mismatch_or_missing")
        if not isinstance(response.get("id"), str) or not response["id"]:
            errors.append("provider_response_id_missing")
        if response.get("object") != "response" or response.get("status") != "completed" or response.get("error") is not None:
            errors.append("provider_response_not_completed")
        if request.get("store") is not False or request.get("parallel_tool_calls") is not False:
            errors.append("request_policy_mismatch")
        if request.get("include") != ["reasoning.encrypted_content"]:
            errors.append("stateless_reasoning_capture_policy_mismatch")
        if request.get("tools") not in ([], [LAB_TOOL]):
            errors.append("unaccounted_tool_surface")
        for name in PARAMETERS:
            if (name in request) != (name in config.parameters) or (name in request and request[name] != config.parameters[name]):
                errors.append("supplied_parameter_mismatch:" + name)
        for name in REPORTED_FIELDS:
            supplied, requested = get_field(request, name)
            reported, actual = get_field(response, name)
            observed["settings"][name] = {"request_source": "explicit" if supplied else "provider_default",
                                           "requested": requested, "reported_present": reported, "reported": actual}
            if not reported:
                errors.append("reported_setting_missing:" + name)
            elif supplied and actual != requested:
                errors.append("reported_setting_mismatch:" + name)
        u = response.get("usage")
        if not isinstance(u, dict):
            raise ContractError("provider usage missing")
        normalized = {name: nonnegative_int(u.get(name), name) for name in ("input_tokens", "output_tokens", "total_tokens")}
        for field, container, nested in (
            ("cached_input_tokens", "input_tokens_details", "cached_tokens"),
            ("cache_write_input_tokens", "input_tokens_details", "cache_write_tokens"),
            ("reasoning_output_tokens", "output_tokens_details", "reasoning_tokens"),
        ):
            detail = u.get(container)
            value = detail.get(nested) if isinstance(detail, dict) else None
            normalized[field] = None if value is None else nonnegative_int(value, field)
        if normalized["input_tokens"] + normalized["output_tokens"] != normalized["total_tokens"]:
            raise ContractError("usage total is inconsistent")
        for detail, total in (("cached_input_tokens", "input_tokens"), ("cache_write_input_tokens", "input_tokens"), ("reasoning_output_tokens", "output_tokens")):
            if normalized[detail] is not None and normalized[detail] > normalized[total]:
                raise ContractError("usage subset exceeds total: " + detail)
        observed["usage"] = normalized
        output = response.get("output")
        if not isinstance(output, list):
            raise ContractError("response output must be a list")
        ids = set()
        for item in output:
            if not isinstance(item, dict):
                raise ContractError("response output item must be an object")
            kind = item.get("type")
            if kind not in ("message", "reasoning", "function_call"):
                errors.append("unsupported_output_type:" + str(kind))
            if kind == "function_call":
                if request.get("tools") != [LAB_TOOL] or item.get("name") != "lab":
                    errors.append("undeclared_function_call")
                call_id = item.get("call_id")
                if not isinstance(call_id, str) or not call_id or call_id in ids:
                    errors.append("duplicate_or_missing_tool_call_id")
                ids.add(call_id)
                observed["tool_calls"].append({"call_id": call_id, "name": item.get("name")})
            if kind == "reasoning" and request.get("tools") and not item.get("encrypted_content"):
                errors.append("stateless_reasoning_item_missing_encrypted_content")
        if len(observed["tool_calls"]) > 1:
            errors.append("parallel_function_calls_not_allowed")
        observed["output_text"] = "".join(part.get("text", "") for item in output if item.get("type") == "message" for part in item.get("content", []) if part.get("type") == "output_text")
    except (ContractError, OSError, ValueError, TypeError, KeyError) as error:
        errors.append("raw_capture_invalid:" + str(error))
    observed["blockers"] = sorted(set(errors))
    observed["status"] = "complete" if not errors else "blocked"
    return observed


def qualification(config, directory, evidence_root=None):
    """Rebuild the provider -> read-only lab -> provider qualification chain."""
    errors = config.blockers()
    normalized, captures = [], []
    evidence = {"request_captures": [], "tool_capture": None}
    if evidence_root is not None:
        evidence_root = Path(evidence_root)
        for number in (1, 2):
            relative = f"requests/{number:04d}"
            capture = evidence_root / relative
            if (capture / "capture.json").is_file():
                evidence["request_captures"].append({"directory": relative, "capture_sha256": sha((capture / "capture.json").read_bytes())})
                try:
                    record = normalize_capture(config, capture)
                    normalized.append(record)
                    captures.append(capture)
                    errors += record["blockers"]
                except (ContractError, OSError, ValueError, TypeError) as error:
                    errors.append("capture_unreadable:" + str(error))
    if len(normalized) != 2:
        errors.append("two_provider_responses_required")
    kinds = {r.get("capture_kind") for r in normalized}
    if kinds and kinds != {"live"}:
        errors.append("fixture_is_not_live_qualification")
    if len(kinds) > 1:
        errors.append("mixed_capture_kinds")
    for record in normalized:
        usage = record.get("usage")
        if not usage or usage["input_tokens"] <= 0 or usage["output_tokens"] <= 0:
            errors.append("smoke_has_no_positive_provider_usage")
    complete = len(normalized) == 2 and all(r["status"] == "complete" for r in normalized)
    tool_verified = False
    if evidence_root is not None and (evidence_root / "lab/001-doc/operation.json").is_file():
        evidence["tool_capture"] = {"directory": "lab/001-doc", "operation_sha256": sha((evidence_root / "lab/001-doc/operation.json").read_bytes())}
    if complete and evidence["tool_capture"]:
        try:
            for name in ("provider_request_id", "provider_response_id"):
                if normalized[0][name] == normalized[1][name]:
                    raise ContractError("qualification provider IDs repeat: " + name)
            if normalized[0]["settings"] != normalized[1]["settings"]:
                raise ContractError("qualification reported settings changed")
            first_request = load_json(captures[0] / "request.json")
            first_response = load_json(captures[0] / "response.json")
            second_request = load_json(captures[1] / "request.json")
            initial = [{"role": "user", "content": QUALIFY_PROMPT}]
            if first_request != config.request(initial):
                raise ContractError("qualification initial request differs from fixed probe")
            calls = [item for item in first_response["output"] if item.get("type") == "function_call"]
            if len(calls) != 1:
                raise ContractError("qualification requires exactly one first-response tool call")
            values = tool_arguments(calls[0])
            if values != {"operation": "doc", "args": ["meter-probe", "1"], "stdin": None}:
                raise ContractError("qualification requested a non-probe operation")
            tool_dir = evidence_root / "lab/001-doc"
            operation = load_json(tool_dir / "operation.json")
            expected_argv = [shutil.which("perl"), str(child(config.root, "tools/lab.pl")), "probe", "doc", "meter-probe", "1"]
            if operation.get("argv") != expected_argv or operation.get("cwd") != str(config.root):
                raise ContractError("qualification lab argv/cwd mismatch")
            if operation.get("operation") != "doc" or operation.get("args") != ["meter-probe", "1"]:
                raise ContractError("qualification lab operation mismatch")
            if operation.get("exit_code") != 0 or operation.get("timed_out") is not False or operation.get("output_limit") is not False or operation.get("api_key_passed_to_lab") is not False:
                raise ContractError("qualification lab operation did not complete cleanly")
            streams = {}
            for name in ("stdin", "stdout", "stderr"):
                entry = operation[name]
                path = child(tool_dir, entry["file"])
                raw = path.read_bytes()
                if entry.get("bytes") != len(raw) or entry.get("sha256") != sha(raw):
                    raise ContractError("qualification lab stream identity mismatch")
                streams[name] = raw.decode("utf-8", errors="strict")
            if streams["stdin"] != "":
                raise ContractError("qualification lab stdin must be empty")
            result = {"exit_code": 0, "stdout": streams["stdout"], "stderr": streams["stderr"]}
            history = initial + first_response["output"] + [{"type": "function_call_output", "call_id": calls[0]["call_id"], "output": canonical(result).decode("utf-8")}]
            if second_request != config.request(history):
                raise ContractError("provider continuation did not consume exact recorded tool result")
            if normalized[1]["tool_calls"] or normalized[1].get("output_text", "").strip() != "READY":
                raise ContractError("qualification final response mismatch")
            tool_verified = True
        except (ContractError, OSError, ValueError, TypeError, KeyError) as error:
            errors.append("qualification_chain_invalid:" + str(error))
    if not tool_verified:
        errors.append("qualifying_tool_chain_missing_or_invalid")
    models = sorted({r["reported_model"] for r in normalized if isinstance(r.get("reported_model"), str)})
    totals = None
    if complete:
        totals = {name: sum(r["usage"][name] for r in normalized) if all(r["usage"].get(name) is not None for r in normalized) else None for name in normalized[0]["usage"]}
    receipt = {"schema_version": SCHEMA, "kind": "yavo.executor.qualification", **config.binding(),
               "status": "qualified" if not errors else "blocked", "qualified_at_utc": utc() if not errors else None,
               "recorded_at_utc": utc(), "blockers": sorted(set(errors)),
               "observed": {"provider_requests": sum(r.get("http_attempts", 0) for r in normalized),
                            "provider_responses": sum(bool(r.get("provider_response_id")) for r in normalized),
                            "qualifying_tool_calls": 1 if tool_verified else 0,
                            "reported_models": models, "usage_complete": bool(complete),
                            "tool_accounting_complete": tool_verified, "per_request_evidence": bool(complete),
                            "requested_model": config.provider.get("requested_model"),
                            "reported_settings": normalized[0].get("settings", {}) if normalized else {},
                            "usage": totals, "tool_accounting_scope": "declared Responses function tools"},
               "evidence": evidence, "money_cost": None}
    if kinds == {"fixture"} and set(errors) == {"fixture_is_not_live_qualification"}:
        receipt["status"] = "fixture_only"
    if directory is not None:
        write_json(Path(directory) / "qualification.json", receipt)
        write_json(Path(directory) / "normalized.json", normalized)
    return receipt


def qualification_probe(config, directory):
    """Two provider requests around one frozen neutral, read-only lab call."""
    directory = Path(directory)
    history = [{"role": "user", "content": QUALIFY_PROMPT}]
    first = directory / "requests/0001"
    capture_response(config, config.request(history), first)
    normalized = normalize_capture(config, first)
    if normalized["status"] != "complete":
        return
    response = load_json(first / "response.json")
    calls = [item for item in response["output"] if item.get("type") == "function_call"]
    if len(calls) != 1:
        return
    values = tool_arguments(calls[0])
    if values != {"operation": "doc", "args": ["meter-probe", "1"], "stdin": None}:
        return
    result = lab_operation(config, "probe", "doc", values["args"], None,
                           directory / "lab/001-doc", directory / "qualification.json")
    if result["exit_code"] != 0:
        return
    history.extend(response["output"])
    history.append({"type": "function_call_output", "call_id": calls[0]["call_id"], "output": canonical(result).decode("utf-8")})
    capture_response(config, config.request(history), directory / "requests/0002")


def verify_qualification(config, receipt_path):
    receipt_path = Path(receipt_path).resolve()
    recorded = load_json(receipt_path)
    if recorded.get("status") != "qualified":
        raise ContractError("qualification is not live and qualified")
    for key, value in config.binding().items():
        if value is None or recorded.get(key) != value:
            raise ContractError("qualification binding mismatch: " + key)
    rebuilt = qualification(config, None, receipt_path.parent)
    if rebuilt["status"] != "qualified" or rebuilt["observed"] != recorded.get("observed") or rebuilt["evidence"] != recorded.get("evidence"):
        raise ContractError("raw evidence does not reproduce qualification")
    return recorded


def tool_arguments(item):
    if item.get("type") != "function_call" or item.get("name") != "lab":
        raise ContractError("only the declared lab function is executable")
    value = parse_json(item.get("arguments", ""))
    if not isinstance(value, dict) or set(value) != {"operation", "args", "stdin"}:
        raise ContractError("lab arguments have an invalid shape")
    if value["operation"] not in OPERATIONS or not isinstance(value["args"], list) or len(value["args"]) > 32:
        raise ContractError("lab operation or argument list is invalid")
    if any(not isinstance(x, str) or "\x00" in x or len(x.encode("utf-8")) > 8192 for x in value["args"]):
        raise ContractError("lab arguments must be bounded strings without NUL")
    if value["stdin"] is not None and (not isinstance(value["stdin"], str) or len(value["stdin"].encode("utf-8")) > 1048576):
        raise ContractError("lab stdin must be null or bounded UTF-8 text")
    return value


def lab_operation(config, run_id, operation, args, stdin, directory, receipt_path):
    config.assert_current()
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,79}", run_id):
        raise ContractError("invalid controller-selected run ID")
    if operation not in (*OPERATIONS, "init"):
        raise ContractError("invalid lab operation")
    program = config.lab.get("program", "perl")
    if program != "perl":
        raise ContractError("only the pinned Perl lab interface is supported")
    executable = shutil.which(program)
    if not executable:
        raise ContractError("Perl is unavailable")
    script = child(config.root, config.lab.get("script", "tools/lab.pl"))
    argv = [executable, str(script), run_id, operation, *args]
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    write_bytes(directory / "stdin.txt", (stdin or "").encode("utf-8"))
    # The API credential never reaches the compiler, lab, or submitted programs.
    env = {key: os.environ[key] for key in ("PATH", "HOME", "TMPDIR", "LANG", "LC_ALL") if key in os.environ}
    env["YAVO_QUALIFICATION_RECEIPT"] = str(Path(receipt_path).resolve())
    limit = config.lab.get("output_bytes", 1048576)
    timeout = config.lab.get("timeout_seconds", 180)
    if type(limit) is not int or not 1024 <= limit <= 16777216 or type(timeout) is not int or not 1 <= timeout <= 600:
        raise ContractError("invalid lab resource limit")
    start = time.monotonic()
    timed_out = output_limit = False
    with (directory / "stdin.txt").open("rb") as inf, (directory / "stdout.txt").open("xb") as outf, (directory / "stderr.txt").open("xb") as errf:
        process = subprocess.Popen(argv, cwd=config.root, stdin=inf, stdout=outf, stderr=errf,
                                   env=env, start_new_session=(os.name == "posix"))
        while process.poll() is None:
            timed_out = time.monotonic() - start > timeout
            output_limit = outf.tell() > limit or errf.tell() > limit
            if timed_out or output_limit:
                if os.name == "posix":
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
                break
            time.sleep(0.01)
        process.wait()
    rawout = (directory / "stdout.txt").read_bytes()
    rawerr = (directory / "stderr.txt").read_bytes()
    output_limit = output_limit or len(rawout) > limit or len(rawerr) > limit
    result = {"argv": argv, "cwd": str(config.root), "operation": operation, "args": args,
              "exit_code": process.returncode, "timed_out": timed_out, "output_limit": output_limit,
              "started_monotonic": start, "elapsed_seconds": time.monotonic() - start,
              "stdin": artifact(directory / "stdin.txt"), "stdout": artifact(directory / "stdout.txt"),
              "stderr": artifact(directory / "stderr.txt"), "api_key_passed_to_lab": False}
    write_json(directory / "operation.json", result)
    stream_event("lab_operation_recorded", operation=operation, exit_code=process.returncode,
                 elapsed_seconds=result["elapsed_seconds"], directory=str(directory))
    if timed_out or output_limit:
        raise ContractError("lab timeout or output limit; run stopped")
    return {"exit_code": process.returncode, "stdout": rawout.decode("utf-8", errors="strict"),
            "stderr": rawerr.decode("utf-8", errors="strict")}


def run_agent(config, run_id, prompt, receipt_path, out):
    config.assert_current()
    qualified = verify_qualification(config, receipt_path)
    if not os.environ.get("OPENAI_API_KEY"):
        raise ContractError("OPENAI_API_KEY is not available; lab init was not invoked")
    prompt_path = child(config.root, prompt)
    prompt_raw = prompt_path.read_bytes()
    frozen = load_json(child(config.root, config.freeze["path"]))
    if frozen.get("identity_files", {}).get(str(prompt_path.resolve())) != sha(prompt_raw):
        raise ContractError("participant prompt is not bound by the freeze")
    prompt_text = prompt_raw.decode("utf-8")
    out = Path(out)
    out.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    initial_binding = config.binding()
    write_bytes(out / "prompt.txt", prompt_raw)
    write_bytes(out / "configuration.json", config.raw)
    history = [{"role": "user", "content": prompt_text}]
    request_ids, response_ids, call_ids = set(), set(), set()
    normalized_records = []
    status, blockers, sealed = "blocked", [], False
    operation_count = 0
    try:
        init = lab_operation(config, run_id, "init", [], None, out / "lab" / "000-init", receipt_path)
        if init["exit_code"] != 0:
            raise ContractError("lab init failed")
        for number in range(1, config.limits.get("max_provider_requests", 128) + 1):
            config.assert_current()
            directory = out / "requests" / f"{number:04d}"
            capture_response(config, config.request(history), directory)
            normalized = normalize_capture(config, directory)
            write_json(directory / "normalized.json", normalized)
            normalized_records.append(normalized)
            if normalized["status"] != "complete":
                raise ContractError("provider evidence incomplete before next participant step: " + "; ".join(normalized["blockers"]))
            if normalized["capture_kind"] != "live":
                raise ContractError("participant loop requires live provider captures")
            if normalized["settings"] != qualified["observed"]["reported_settings"]:
                raise ContractError("provider-reported settings changed since qualification")
            for value, seen, label in ((normalized["provider_request_id"], request_ids, "request"), (normalized["provider_response_id"], response_ids, "response")):
                if value in seen:
                    raise ContractError("duplicate provider " + label + " ID")
                seen.add(value)
            response = load_json(directory / "response.json")
            history.extend(response["output"])
            calls = [item for item in response["output"] if item.get("type") == "function_call"]
            if len(calls) != 1:
                raise ContractError("participant finished without a successful lab seal")
            call = calls[0]
            if call["call_id"] in call_ids:
                raise ContractError("repeated lab call ID")
            call_ids.add(call["call_id"])
            values = tool_arguments(call)
            operation_count += 1
            result = lab_operation(config, run_id, values["operation"], values["args"], values["stdin"],
                                   out / "lab" / f"{operation_count:03d}-{values['operation']}", receipt_path)
            history.append({"type": "function_call_output", "call_id": call["call_id"], "output": canonical(result).decode("utf-8")})
            if values["operation"] == "seal" and result["exit_code"] == 0:
                seal = parse_json(result["stdout"])
                if not isinstance(seal, dict) or seal.get("status") != "sealed":
                    raise ContractError("successful seal response is malformed")
                sealed = True
                status = "completed"
                break
        if not sealed:
            raise ContractError("provider request budget exhausted before seal")
    except (ContractError, OSError, ValueError, TypeError, KeyError) as error:
        blockers.append(str(error))
    all_usage = all(r.get("status") == "complete" for r in normalized_records) and bool(normalized_records)
    totals = None
    if all_usage:
        totals = {name: sum(r["usage"][name] for r in normalized_records) if all(r["usage"].get(name) is not None for r in normalized_records) else None
                  for name in normalized_records[0]["usage"]}
    finished = time.monotonic()
    manifest = {"schema_version": SCHEMA, "kind": "yavo.executor.transcript", **initial_binding, "run_id": run_id,
                "files": {str(path.relative_to(out)): {"bytes": path.stat().st_size, "sha256": sha(path.read_bytes())}
                          for path in sorted(out.rglob("*")) if path.is_file()}}
    write_json(out / "transcript-manifest.json", manifest)
    summary = {"schema_version": SCHEMA, "kind": "yavo.executor.run", **initial_binding, "status": status,
               "run_id": run_id, "started_monotonic": started, "finished_monotonic": finished, "elapsed_seconds": finished - started,
               "provider_requests": sum(r.get("http_attempts", 0) for r in normalized_records),
               "provider_responses": sum(bool(r.get("provider_response_id")) for r in normalized_records),
               "captured_request_records": len(normalized_records), "controlled_lab_operations": operation_count,
               "controller_init_operations": 1, "sealed": sealed, "usage_complete": all_usage,
               "usage": totals, "money_cost": None, "blockers": blockers,
               "qualification_sha256": sha(Path(receipt_path).read_bytes()), "prompt_sha256": sha(prompt_raw),
               "qualification_path": str(Path(receipt_path).resolve()), "prompt_path": prompt,
               "transcript_manifest_sha256": sha((out / "transcript-manifest.json").read_bytes()),
               "timing_scope": "controller init through successful seal or stop; provider and lab intervals stored separately"}
    write_json(out / "run.json", summary)
    return summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("binding", "qualify", "verify", "normalize", "run"):
        p = sub.add_parser(name)
        p.add_argument("--config", required=True, type=Path)
        if name in ("qualify", "run"):
            p.add_argument("--out", required=True, type=Path)
        if name in ("verify", "run"):
            p.add_argument("--qualification", required=True, type=Path)
        if name == "normalize":
            p.add_argument("--capture", required=True, type=Path)
        if name == "run":
            p.add_argument("--run", required=True)
            p.add_argument("--prompt", required=True)
    args = parser.parse_args(argv)
    try:
        config = Config(args.config)
        if args.command == "binding":
            result = {"status": "binding", **config.binding()}
        elif args.command == "qualify":
            args.out.mkdir(parents=True, exist_ok=False)
            write_bytes(args.out / "configuration.json", config.raw)
            if not config.blockers():
                try:
                    qualification_probe(config, args.out)
                except (ContractError, OSError, ValueError, KeyError, TypeError) as error:
                    write_json(args.out / "probe-error.json", {"error": str(error), "recorded_at_utc": utc()})
            result = qualification(config, args.out, args.out)
        elif args.command == "verify":
            result = verify_qualification(config, args.qualification)
        elif args.command == "normalize":
            result = normalize_capture(config, args.capture)
        else:
            result = run_agent(config, args.run, args.prompt, args.qualification, args.out)
        stream_event("result", **result)
        return 0 if result["status"] in ("binding", "qualified", "complete", "completed") else 2
    except (ContractError, OSError, ValueError, KeyError, TypeError) as error:
        stream_event("blocked", error=str(error))
        return 2


if __name__ == "__main__":
    sys.exit(main())
