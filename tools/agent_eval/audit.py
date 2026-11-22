#!/usr/bin/env python3
"""Rebuild one provider/lab transcript; unverified totals stay unknown."""
from __future__ import annotations

import argparse
import collections
import json
from pathlib import Path
import re

try:
    from . import executor
except ImportError:
    import executor


def require(condition, message):
    if not condition:
        raise executor.ContractError(message)


def file_sha(path):
    return executor.sha(Path(path).read_bytes())


def verify_inventory(config, directory, summary):
    path = directory / "transcript-manifest.json"
    require(file_sha(path) == summary.get("transcript_manifest_sha256"), "transcript manifest identity mismatch")
    manifest = executor.load_json(path)
    require(manifest.get("schema_version") == 1 and manifest.get("kind") == "yavo.executor.transcript", "transcript manifest schema mismatch")
    for key, expected in {**config.binding(), "run_id": summary["run_id"]}.items():
        require(expected is not None and manifest.get(key) == expected, "transcript binding mismatch: " + key)
    files = manifest.get("files")
    require(isinstance(files, dict) and files, "empty transcript inventory")
    actual = set()
    for path in directory.rglob("*"):
        require(not path.is_symlink(), "symlink in transcript")
        relative = path.relative_to(directory).as_posix()
        if path.is_file() and relative not in ("run.json", "transcript-manifest.json"):
            actual.add(relative)
    require(set(files) == actual, "transcript inventory omits or adds files")
    for name, identity in files.items():
        raw = executor.child(directory, name).read_bytes()
        require(isinstance(identity, dict) and type(identity.get("bytes")) is int and identity["bytes"] == len(raw)
                and identity.get("sha256") == executor.sha(raw), "transcript file identity mismatch: " + name)
    return manifest


def operation_result(config, directory, run_id, operation, args, stdin, earliest):
    record = executor.load_json(directory / "operation.json")
    argv = record.get("argv")
    script = str(executor.child(config.root, config.lab.get("script", "tools/lab.pl")))
    require(isinstance(argv, list) and len(argv) >= 4 and Path(argv[0]).name in ("perl", "perl.exe")
            and argv[1:] == [script, run_id, operation, *args], "lab argv does not match provider call")
    require(record.get("cwd") == str(config.root) and record.get("operation") == operation and record.get("args") == args,
            "lab operation metadata mismatch")
    require(record.get("timed_out") is False and record.get("output_limit") is False and record.get("api_key_passed_to_lab") is False,
            "lab transport stopped or credential policy changed")
    require(type(record.get("exit_code")) is int, "lab exit status missing")
    start = executor.finite_number(record.get("started_monotonic"), "lab start")
    elapsed = executor.finite_number(record.get("elapsed_seconds"), "lab elapsed")
    require(elapsed >= 0 and start + 1e-6 >= earliest, "lab timeline is inconsistent")
    streams = {}
    for name in ("stdin", "stdout", "stderr"):
        entry = record.get(name)
        require(isinstance(entry, dict), "lab stream identity missing")
        path = executor.child(directory, entry.get("file"))
        require(path.name == name + ".txt", "lab stream path mismatch")
        raw = path.read_bytes()
        require(type(entry.get("bytes")) is int and entry["bytes"] == len(raw)
                and entry.get("sha256") == executor.sha(raw), "lab stream identity mismatch")
        streams[name] = raw.decode("utf-8", errors="strict")
    require(streams["stdin"] == (stdin or ""), "lab stdin differs from provider tool argument")
    return {"exit_code": record["exit_code"], "stdout": streams["stdout"], "stderr": streams["stderr"]}, record, start + elapsed


def verify_run(config, run_directory, qualification_path):
    """Return verified live costs or status=blocked/usage=None; never trust run.json totals."""
    result = {"schema_version": 1, "kind": "yavo.executor.audit", "status": "blocked", "run_id": None,
              "usage_complete": False, "usage": None, "money_cost": None, "blockers": [],
              "exact_model": None, "provider_requests": None, "controlled_lab_operations": None,
              "elapsed_seconds": None, "verified_request_records": [],
              "tool_accounting_scope": "declared Responses function tools; controller init separately counted"}
    try:
        config.assert_current()
        qualified = executor.verify_qualification(config, qualification_path)
        directory = Path(run_directory)
        require(not directory.is_symlink(), "run directory is a symlink")
        directory = directory.resolve()
        summary = executor.load_json(directory / "run.json")
        require(isinstance(summary, dict) and summary.get("schema_version") == 1 and summary.get("kind") == "yavo.executor.run",
                "run summary schema mismatch")
        run_id = summary.get("run_id")
        require(isinstance(run_id, str) and re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,79}", run_id), "run ID missing or invalid")
        result["run_id"] = run_id
        for key, expected in config.binding().items():
            require(expected is not None and summary.get(key) == expected, "run binding mismatch: " + key)
        require(summary.get("status") == "completed" and summary.get("sealed") is True and summary.get("usage_complete") is True,
                "run did not complete a sealed, fully metered session")
        require(summary.get("blockers") == [], "recorded run has blockers")
        require(summary.get("qualification_sha256") == file_sha(qualification_path), "run qualification changed")
        require((directory / "configuration.json").read_bytes() == config.raw, "saved run configuration differs")
        prompt_path = executor.child(config.root, summary.get("prompt_path"))
        prompt_raw = (directory / "prompt.txt").read_bytes()
        require(prompt_raw == prompt_path.read_bytes() and executor.sha(prompt_raw) == summary.get("prompt_sha256"), "run prompt identity mismatch")
        frozen = executor.load_json(executor.child(config.root, config.freeze["path"]))
        require(frozen.get("identity_files", {}).get(str(prompt_path.resolve())) == executor.sha(prompt_raw), "prompt is absent from runtime freeze")
        inventory = verify_inventory(config, directory, summary)
        start = executor.finite_number(summary.get("started_monotonic"), "run start")
        end = executor.finite_number(summary.get("finished_monotonic"), "run end")
        elapsed = executor.finite_number(summary.get("elapsed_seconds"), "run elapsed")
        require(end >= start and elapsed >= 0 and abs(end - start - elapsed) <= 1e-6, "run timeline mismatch")
        initial, init_record, earliest = operation_result(config, directory / "lab/000-init", run_id, "init", [], None, start)
        require(initial["exit_code"] == 0 and executor.parse_json(initial["stdout"]).get("run_id") == run_id
                and executor.parse_json(initial["stdout"]).get("status") == "initialized", "controller init did not initialize this run")
        lab_root = config.root / "runs" / run_id
        lab_meta = executor.load_json(lab_root / "run.json")
        require(lab_meta.get("run_id") == run_id and lab_meta.get("status") == "sealed", "lab run is not sealed")
        require(lab_meta.get("frozen_manifest_sha256") == config.freeze["sha256"]
                and lab_meta.get("qualification_sha256") == file_sha(qualification_path), "lab run binding mismatch")
        events = [executor.parse_json(line) for line in (lab_root / "events.jsonl").read_bytes().splitlines() if line.strip()]
        request_dirs = sorted(path for path in (directory / "requests").iterdir() if path.is_dir())
        require(request_dirs and [path.name for path in request_dirs] == [f"{i:04d}" for i in range(1, len(request_dirs) + 1)],
                "provider request sequence missing or noncontiguous")
        require(len(request_dirs) <= config.limits.get("max_provider_requests", 128), "recorded provider budget exceeded")
        require(len(events) == len(request_dirs), "provider calls and lab events differ in count")
        history = [{"role": "user", "content": prompt_raw.decode("utf-8")}]
        ids = {kind: set() for kind in ("request", "response", "call")}
        usage_records, lab_directories, provider_seconds, lab_seconds = [], {"000-init"}, 0.0, init_record["elapsed_seconds"]
        for index, request_dir in enumerate(request_dirs, 1):
            capture = executor.load_json(request_dir / "capture.json")
            normalized = executor.normalize_capture(config, request_dir)
            require(capture.get("capture_kind") == "live" and normalized.get("status") == "complete",
                    "capture is not complete live evidence: " + request_dir.name)
            require(executor.load_json(request_dir / "normalized.json") == normalized, "saved normalized record differs from raw capture")
            require(normalized.get("settings") == qualified["observed"]["reported_settings"], "reported settings drifted after qualification")
            require(capture["started_monotonic"] + 1e-6 >= earliest, "provider request started before prior operation ended")
            request = executor.load_json(request_dir / "request.json")
            require(request == config.request(history), "provider request does not continue exact recorded history")
            response = executor.load_json(request_dir / "response.json")
            calls = [item for item in response["output"] if item.get("type") == "function_call"]
            require(len(calls) == 1, "expected one lab call per provider response")
            call = calls[0]
            for kind, value in (("request", normalized["provider_request_id"]), ("response", normalized["provider_response_id"]), ("call", call["call_id"])):
                require(value not in ids[kind], "duplicate provider/tool identity: " + kind)
                ids[kind].add(value)
            values = executor.tool_arguments(call)
            operation = values["operation"]
            lab_name = f"{index:03d}-{operation}"
            lab_directories.add(lab_name)
            observed, operation_record, earliest = operation_result(config, directory / "lab" / lab_name, run_id,
                                                                    operation, values["args"], values["stdin"], capture["finished_monotonic"])
            event = events[index - 1]
            require(event.get("sequence") == index and event.get("run_id") == run_id and event.get("operation") == operation
                    and event.get("arguments") == values["args"], "lab event differs from provider call")
            event_dir = lab_root / "operations" / f"{index:04d}"
            require(executor.load_json(event_dir / "event.json") == event, "lab event stream differs from event file")
            require((event_dir / "output.txt").read_bytes() == observed["stdout"].encode("utf-8"), "provider saw different lab output")
            if operation == "write":
                require((event_dir / "input-source.txt").read_bytes() == (values["stdin"] or "").encode("utf-8"), "written source differs from provider argument")
            require((event.get("status") == "completed" and observed["exit_code"] == 0)
                    or (event.get("status") == "interface_error" and observed["exit_code"] != 0), "lab exit/event status mismatch")
            successful_seal = operation == "seal" and observed["exit_code"] == 0
            require(successful_seal == (index == len(request_dirs)), "successful seal is missing or followed by more provider calls")
            if successful_seal:
                seal = executor.parse_json(observed["stdout"])
                require(seal.get("status") == "sealed" and seal.get("run_id") == run_id, "seal output identity mismatch")
            history.extend(response["output"])
            history.append({"type": "function_call_output", "call_id": call["call_id"], "output": executor.canonical(observed).decode("utf-8")})
            usage_records.append(normalized["usage"])
            result["verified_request_records"].append({"sequence": index, "provider_request_id": normalized["provider_request_id"],
                                                      "provider_response_id": normalized["provider_response_id"], "usage": normalized["usage"]})
            provider_seconds += capture["elapsed_seconds"]
            lab_seconds += operation_record["elapsed_seconds"]
        require({p.name for p in (directory / "lab").iterdir() if p.is_dir()} == lab_directories, "unaccounted lab operation directories")
        require(earliest <= end + 1e-6, "run ended before final lab seal")
        n = len(request_dirs)
        expected_counts = {"provider_requests": n, "provider_responses": n, "captured_request_records": n,
                           "controlled_lab_operations": n, "controller_init_operations": 1}
        for key, expected in expected_counts.items():
            require(type(summary.get(key)) is int and summary[key] == expected, "run counter mismatch: " + key)
        require(lab_meta.get("operations") == n, "lab operation count mismatch")
        totals = {key: sum(record[key] for record in usage_records) if all(record.get(key) is not None for record in usage_records) else None
                  for key in usage_records[0]}
        require(summary.get("usage") == totals, "run usage does not equal all raw provider requests")
        require(summary.get("money_cost") is None, "unpriced executor cannot claim money cost")
        result.update({"status": "verified", **config.binding(), **expected_counts, "usage_complete": True, "usage": totals,
                       "exact_model": config.provider["expected_reported_model"], "elapsed_seconds": elapsed,
                       "provider_elapsed_seconds": provider_seconds, "lab_elapsed_seconds_including_init": lab_seconds,
                       "operation_categories": dict(sorted(collections.Counter(e.get("category", "uncategorized") for e in events).items())),
                       "run_summary_sha256": file_sha(directory / "run.json"), "transcript_manifest_sha256": file_sha(directory / "transcript-manifest.json"),
                       "lab_events_sha256": file_sha(lab_root / "events.jsonl"), "lab_metadata_sha256": file_sha(lab_root / "run.json"),
                       "qualification_sha256": file_sha(qualification_path), "transcript_file_count": len(inventory["files"]),
                       "cost_scope": "all provider requests, including those preceding task failures and repairs; cached/reasoning counts are subsets"})
    except (executor.ContractError, OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        result["blockers"].append(type(error).__name__ + ": " + str(error))
        result["usage"] = None
        result["usage_complete"] = False
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--run", required=True, type=Path)
    parser.add_argument("--qualification", required=True, type=Path)
    args = parser.parse_args()
    result = verify_run(executor.Config(args.config), args.run, args.qualification)
    print(json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False))
    return 0 if result["status"] == "verified" else 2


if __name__ == "__main__":
    raise SystemExit(main())
