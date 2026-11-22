#!/usr/bin/env python3
"""Validate the archived round-five experiment, not participant success rates.

The experiment's measured inputs and judge are frozen at 8c200aa. This gate
checks their hashes, byte evidence, sealed submissions and reproducible totals.
Wrong participant answers and explicitly unavailable telemetry are valid data.
"""
from __future__ import annotations

import argparse
import base64
from collections import Counter
import copy
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time


EXPERIMENT = "experiments/coldstart-2026-11-22-r5"
FREEZE_COMMIT = "8c200aae9a5901325da193d6d8167aa6900b79b2"
FROZEN_SHA256 = "aeb5c46db375e84c3d90257aec501161f29d4e4802f4a1a884b699753750bee3"
RUN_IDS = ("yavo-a", "rust-a", "rust-b", "yavo-b")
TASK_IDS = tuple(f"T{index:02d}" for index in range(1, 13))
HEX256 = re.compile(r"[a-f0-9]{64}\Z")
SOURCE_NAME = re.compile(r"[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*\.(?:rs|yv)\Z")


class EvidenceError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise EvidenceError(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def strict_pairs(items):
    result = {}
    for name, value in items:
        require(name not in result, f"duplicate JSON key: {name}")
        result[name] = value
    return result


def load(path: Path):
    def reject_constant(value):
        raise EvidenceError(f"non-finite JSON number: {value}")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=strict_pairs,
                      parse_constant=reject_constant)


def save(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def safe(root: Path, name: str) -> Path:
    relative = PurePosixPath(name)
    require(name and not relative.is_absolute() and ".." not in relative.parts and "\\" not in name,
            f"unsafe evidence path: {name!r}")
    path = root
    for part in relative.parts:
        path = path / part
        require(not path.is_symlink(), f"evidence symlink: {path}")
    path.resolve().relative_to(root.resolve())
    return path


def source_files(directory: Path) -> dict:
    require(directory.is_dir(), f"missing source snapshot: {directory}")
    files = {}
    for path in sorted(directory.rglob("*")):
        require(not path.is_symlink(), f"source symlink: {path}")
        if path.is_file():
            name = path.relative_to(directory).as_posix()
            require(SOURCE_NAME.fullmatch(name), f"unexpected source snapshot file: {path}")
            files[name] = sha(path)
    return files


def source_digest(files: dict) -> str:
    # JSON::PP canonical->pretty->utf8 format used by the frozen Perl judge.
    require(all(SOURCE_NAME.fullmatch(k) and HEX256.fullmatch(v) for k, v in files.items()),
            "invalid source-manifest entry")
    data = "{}\n" if not files else "{\n" + ",\n".join(
        "   " + json.dumps(k, ensure_ascii=False) + " : " + json.dumps(files[k])
        for k in sorted(files)) + "\n}\n"
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def run_tool(argv: list[str], cwd: Path, log: Path, seconds=120) -> dict:
    start = time.monotonic()
    child = subprocess.Popen(argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=(os.name == "posix"))
    try:
        stdout, stderr = child.communicate(timeout=seconds)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(child.pid, signal.SIGKILL)
        else:
            child.kill()
        stdout, stderr = child.communicate()
        log.write_bytes(stdout + b"\n--- stderr ---\n" + stderr)
        raise EvidenceError(f"evidence tool timed out: {argv}")
    log.write_bytes(stdout + b"\n--- stderr ---\n" + stderr)
    require(child.returncode == 0, f"evidence tool failed ({child.returncode}); see {log}")
    return {"argv": argv, "exit_code": child.returncode, "elapsed_seconds": time.monotonic() - start,
            "log_sha256": sha(log)}


def import_file(path: Path, name: str):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def normalize_summary(summary):
    value = copy.deepcopy(summary)
    for run in value["runs"]:
        run["report"] = run["run_id"] + "/report.json"
    value["runs"].sort(key=lambda run: run["run_id"])
    value["task_results"].sort(key=lambda task: (task["run_id"], task["task_id"]))
    value["evidence_gaps"].sort()
    return value


def compare_arguments(request: dict, event: dict) -> list[int]:
    """Permit only the frozen Perl recorder's doc line-number scalar flags.

    CLI arguments begin as strings. Numeric comparisons in lab.pl can cause
    JSON::PP to serialize the two doc range arguments as integers in event.json.
    This does not permit floats, booleans, leading zeros, or a changed value.
    """
    before, after = request["arguments"], event["arguments"]
    require(type(before) is list and type(after) is list and len(before) == len(after),
            "request/event argument shape differs")
    converted = []
    doc_range = request["operation"] == event["operation"] == "doc" and len(before) == 3
    for index, (left, right) in enumerate(zip(before, after)):
        if doc_range and index in (1, 2):
            require(type(left) is str and re.fullmatch(r"[1-9][0-9]*", left),
                    "original doc line number is not a canonical positive decimal string")
            if type(right) is int:
                require(right > 0 and str(right) == left, "doc line-number value differs")
                converted.append(index)
            else:
                require(type(right) is str and right == left, "doc line-number type or value differs")
        else:
            require(type(left) is type(right) and left == right, "request/event argument type or value differs")
    return converted


def serialization_note(root: Path, observations: list[dict], frozen: dict) -> dict:
    entries = []
    for observation in observations:
        for entry in observation["argument_serialization_normalizations"]:
            prefix = f"runs/{observation['run_id']}/operations/{entry['sequence']:04d}"
            entries.append({"run_id": observation["run_id"], **entry,
                            "request_path": prefix + "/request.json", "event_path": prefix + "/event.json",
                            "request_sha256": sha(root / prefix / "request.json"),
                            "event_sha256": sha(root / prefix / "event.json")})
    return {"schema_version": 1, "frozen_recorder_sha256": frozen["files"]["tools/lab.pl"],
            "compatibility_policy": "Only operation=doc with three arguments: canonical positive decimal strings in original argv may remain identical strings or serialize as equal positive integers at indexes 1 and 2. All other arguments preserve both JSON type and value; booleans and floats are rejected.",
            "mechanism": "The frozen Perl recorder stores a reference to argv in each event. Its doc range comparison and arithmetic set numeric flags on the existing scalar values; JSON::PP subsequently emits those line numbers as JSON integers. The archived request and event represent the same document and line-number values. Neither archive is rewritten.",
            "normalization_count": len(entries), "observations": entries}


def check_packet(root: Path, scratch: Path) -> dict:
    frozen_path = root / "evidence/frozen.json"
    require(sha(frozen_path) == FROZEN_SHA256, "frozen.json differs from the pre-run identity pinned by this gate")
    frozen = load(frozen_path)
    require(frozen["files"], "empty frozen file manifest")
    for name, expected in frozen["files"].items():
        require(HEX256.fullmatch(expected), f"invalid frozen SHA-256: {name}")
        require(sha(safe(root, name)) == expected, f"frozen input changed: {name}")
    packet = load(root / "packet-manifest.json")
    require((packet["task_count"], packet["public_case_count"], packet["hidden_case_count"]) == (12, 24, 144),
            "packet declared counts differ from protocol")
    names = [item["path"] for item in packet["files"]]
    require(len(names) == len(set(names)), "duplicate packet-manifest path")
    for item in packet["files"]:
        path = safe(root, item["path"])
        require(path.stat().st_size == item["bytes"] and sha(path) == item["sha256"],
                f"packet input changed: {item['path']}")
    cases = load(root / "judge/cases.json")
    require([task["id"] for task in cases["tasks"]] == list(TASK_IDS), "judge task IDs differ from protocol")
    public = load(root / "public/cases.json")
    require([task["id"] for task in public["tasks"]] == list(TASK_IDS), "public task IDs differ from protocol")
    case_ids = set()
    for task, public_task in zip(cases["tasks"], public["tasks"]):
        counts = Counter(case["visibility"] for case in task["cases"])
        require(counts == {"public": 2, "hidden": 12}, f"{task['id']}: wrong public/hidden case counts")
        require(public_task["cases"] == [case for case in task["cases"] if case["visibility"] == "public"],
                f"{task['id']}: public packet differs from judge cases")
        require(load(root / "public" / (task["id"] + ".json"))["tasks"] == [public_task],
                f"{task['id']}: individual public packet differs")
        for case in task["cases"]:
            require(case["id"] not in case_ids, f"duplicate case: {case['id']}")
            case_ids.add(case["id"])
    with tempfile.TemporaryDirectory(prefix="yavo-r5-packet-") as temporary:
        rebuilt = Path(temporary)
        (rebuilt / "judge").mkdir()
        (rebuilt / "public").mkdir()
        shutil.copyfile(root / "judge/generate.py", rebuilt / "judge/generate.py")
        generation = run_tool([sys.executable, str(rebuilt / "judge/generate.py")], rebuilt, scratch / "packet-replay.log")
        for item in packet["files"]:
            require(sha(safe(rebuilt, item["path"])) == item["sha256"],
                    f"deterministic packet regeneration differs: {item['path']}")
        require(sha(rebuilt / "packet-manifest.json") == sha(root / "packet-manifest.json"),
                "regenerated packet manifest differs")
    return {"frozen": frozen, "cases": cases, "frozen_file_count": len(frozen["files"]),
            "packet_file_count": len(packet["files"]), "generation": generation}


def compare_observation(observed: dict, directory: Path, expected_stdin: bytes):
    require(load(directory / "command.json") == observed, f"command receipt differs: {directory}")
    require((directory / "stdin").read_bytes() == expected_stdin, f"stdin artifact differs: {directory}")
    require(observed["stdin_sha256"] == hashlib.sha256(expected_stdin).hexdigest(), f"stdin identity differs: {directory}")
    for stream in ("stdout", "stderr"):
        path = directory / stream
        data = path.read_bytes()
        capture = observed[stream]
        require(capture["bytes"] == len(data) and capture["sha256"] == sha(path), f"raw output identity differs: {path}")
        encoded = base64.b64decode(capture["base64"], validate=True)
        require(data[:observed["output_limit_bytes_per_stream"]] == encoded, f"embedded output bytes differ: {path}")
        require(bool(capture["capture_truncated"]) == (len(data) > observed["output_limit_bytes_per_stream"]),
                f"output truncation flag differs: {path}")
    elapsed = (observed["finished_monotonic_ns"] - observed["started_monotonic_ns"]) / 1e9
    require(elapsed >= 0 and abs(elapsed - observed["elapsed_seconds"]) < 1e-6, f"command duration differs: {directory}")


def check_run(root: Path, run_id: str, frozen: dict, cases: dict, config: dict) -> dict:
    directory = root / "runs" / run_id
    meta = load(directory / "run.json")
    submission = load(directory / "submission.json")
    report_path = root / "judge-results" / run_id / "report.json"
    report = load(report_path)
    language, replicate = run_id.split("-")
    for document in (meta, submission, report):
        require((document["run_id"], document["language"], document["replicate"]) == (run_id, language, replicate),
                f"{run_id}: run identity mismatch")
    require(meta["status"] == "sealed" and set(meta["tasks"]) == set(TASK_IDS), f"{run_id}: incomplete sealed run")
    require(meta["frozen_manifest_sha256"] == FROZEN_SHA256, f"{run_id}: wrong frozen inputs")
    require(meta["compiler_revision"] == frozen["compiler_revision"], f"{run_id}: compiler revision differs")
    require("usage" in meta and (meta["usage"] is not None or meta.get("usage_status")), f"{run_id}: missing telemetry status")
    require(report["complete"] is True and report["compiler_identities_unchanged"] is True, f"{run_id}: incomplete judge report")
    require(report["compiler_identities"] == config[language]["identity_files"], f"{run_id}: compiler/toolchain identities differ")
    identities = report["frozen_inputs"]
    require(identities["submission_sha256"] == sha(directory / "submission.json"), f"{run_id}: submission identity differs")
    require(identities["cases_sha256"] == sha(root / "judge/cases.json"), f"{run_id}: judge cases identity differs")
    require(identities["compiler_config_sha256"] == sha(root / "evidence/compiler-config.json"), f"{run_id}: compiler configuration differs")
    require(identities["judge_sha256"] == frozen["files"]["judge/evaluate.pl"], f"{run_id}: evaluator version differs")
    require([item["id"] for item in submission["tasks"]] == list(TASK_IDS), f"{run_id}: submission tasks differ")
    require([item["id"] for item in report["tasks"]] == list(TASK_IDS), f"{run_id}: evaluated tasks differ")
    events = [json.loads(line, object_pairs_hook=strict_pairs) for line in (directory / "events.jsonl").read_text().splitlines() if line.strip()]
    require([event["sequence"] for event in events] == list(range(1, len(events) + 1)), f"{run_id}: event sequence gap")
    require(meta["operations"] == len(events), f"{run_id}: event count differs")
    require(meta["operations_by_category"] == dict(Counter(event["category"] for event in events)), f"{run_id}: event categories differ")
    serializations = []
    for event in events:
        operation = directory / "operations" / f"{event['sequence']:04d}"
        require(load(operation / "event.json") == event, f"{run_id}: event archive differs at {event['sequence']}")
        request = load(operation / "request.json")
        for key in ("sequence", "run_id", "operation", "started_at", "started_monotonic"):
            require(request[key] == event[key], f"{run_id}: request identity differs at {event['sequence']} ({key})")
        converted = compare_arguments(request, event)
        if converted:
            serializations.append({"sequence": event["sequence"], "operation": event["operation"],
                                   "request_arguments": request["arguments"], "event_arguments": event["arguments"],
                                   "normalized_positions": converted})
        require((operation / "output.txt").is_file(), f"{run_id}: operation response missing at {event['sequence']}")
        if event["operation"] == "write":
            source = operation / "input-source.txt"
            require(sha(source) == event["sha256"] and source.stat().st_size == event["bytes"], f"{run_id}: source edit archive differs")
        if event["operation"] == "build" and "result" in event:
            task_id, build_number = event["task"], f"{event['build_number']:04d}"
            build_dir = directory / "tasks" / task_id / "builds" / build_number
            require(load(build_dir / "result.json") == event["result"], f"{run_id}/{task_id}: compiler command archive differs")
            require(source_files(directory / "tasks" / task_id / "snapshots" / build_number) == event["source_manifest"],
                    f"{run_id}/{task_id}: compiler input archive differs")
            for stream in ("stdout", "stderr"):
                require((build_dir / (stream + ".txt")).read_bytes().decode("utf-8", errors="replace") == event["result"][stream],
                        f"{run_id}/{task_id}: compiler output archive differs")
    cases_by_task = {task["id"]: task for task in cases["tasks"]}
    snapshots = 0
    for item, submitted in zip(report["tasks"], submission["tasks"]):
        task_id = item["id"]
        require(meta["tasks"][task_id]["submitted"] is True, f"{run_id}/{task_id}: task not sealed")
        require(item["structural_requirements"] == cases_by_task[task_id]["structural"], f"{run_id}/{task_id}: structural requirements differ")
        for stage_name in ("first", "final"):
            stage, specification = item[stage_name], submitted[stage_name]
            expected_path = f"tasks/{task_id}/" + ("snapshots/0001" if stage_name == "first" else "final")
            require(specification["source_dir"] == expected_path, f"{run_id}/{task_id}: {stage_name} source path differs")
            require(specification["entry"] == ("main.yv" if language == "yavo" else "main.rs"), f"{run_id}/{task_id}: entry differs")
            files = source_files(safe(directory, expected_path))
            snapshots += 1
            require(stage["status"] not in ("judge_error", "missing_submission"), f"{run_id}/{task_id}: missing judge evidence")
            if stage["status"] == "missing_source":
                require(stage_name == "first" and not meta["tasks"][task_id]["first_build_passed"],
                        f"{run_id}/{task_id}: unexplained missing source")
                continue
            require(stage["source"]["files"] == files and stage["source"]["sha256"] == source_digest(files),
                    f"{run_id}/{task_id}/{stage_name}: source identity differs")
            if stage_name == "final":
                require(meta["tasks"][task_id]["final_manifest"] == files, f"{run_id}/{task_id}: sealed source identity differs")
            stage_dir = root / "judge-results" / run_id / task_id / stage_name
            compare_observation(stage["build"], stage_dir / "build", b"")
            if stage["build_passed"]:
                expected_cases = {case["id"]: case for case in cases_by_task[task_id]["cases"]}
                require(len(stage["cases"]) == 14 and {case["id"] for case in stage["cases"]} == set(expected_cases),
                        f"{run_id}/{task_id}/{stage_name}: missing evaluated cases")
                for case in stage["cases"]:
                    compare_observation(case["observed"], stage_dir / "cases" / case["id"], expected_cases[case["id"]]["stdin"].encode("utf-8"))
            else:
                require(stage["cases"] == [], f"{run_id}/{task_id}/{stage_name}: failed build has case executions")
    return {"run_id": run_id, "report": report, "report_path": report_path, "snapshots": snapshots,
            "operation_count": len(events), "token_usage_available": meta["usage"] is not None,
            "argument_serialization_normalizations": serializations}


def check_reviews(root: Path, observations: list[dict]) -> int:
    expected = {}
    for observation in observations:
        report = observation["report"]
        for task in report["tasks"]:
            if task["structural_requirements"]:
                for stage in ("first", "final"):
                    expected[(report["run_id"], task["id"], stage)] = task[stage].get("source", {}).get("sha256")
    reviews = load(root / "evidence/manual-review.json")["reviews"]
    seen = set()
    for review in reviews:
        key = (review["run_id"], review["task_id"], review["stage"])
        require(key in expected and key not in seen, f"unexpected or duplicate manual review: {key}")
        require(expected[key] is not None and review["source_sha256"] == expected[key], f"manual review source changed: {key}")
        require(isinstance(review["passed"], bool) and isinstance(review["evidence"], str) and review["evidence"].strip(),
                f"incomplete manual review: {key}")
        seen.add(key)
    require(seen == set(expected) and len(seen) == 16, "expected sixteen hash-bound T04/T06 first/final reviews")
    return len(seen)


def verify(root: Path, scratch: Path) -> dict:
    packet = check_packet(root, scratch)
    judge_selftest = run_tool([sys.executable, str(root / "judge/selftest.py")], root, scratch / "judge-selftest.log")
    config = load(root / "evidence/compiler-config.json")
    observations = [check_run(root, run_id, packet["frozen"], packet["cases"], config) for run_id in RUN_IDS]
    serializations = serialization_note(root, observations, packet["frozen"])
    if serializations["normalization_count"]:
        require(load(root / "evidence/serialization-note.json") == serializations,
                "archived scalar-serialization explanation differs from observed compatibility cases")
    require(sum(item["snapshots"] for item in observations) == 96, "expected 96 archived first/final source graphs")
    review_count = check_reviews(root, observations)
    reports = [item["report_path"] for item in observations]
    checker = import_file(root / "judge/check_evidence.py", "frozen_r5_check_evidence")
    consistency = checker.validate(root / "judge/cases.json", reports, root / "runs")
    save(scratch / "experiment-evidence-consistency.json", consistency)
    require(consistency["passed"], "frozen independent evidence checker failed: " + "; ".join(consistency["errors"][:10]))
    summarizer = import_file(root / "judge/summarize.py", "frozen_r5_summarize")
    recomputed = summarizer.summarize(reports, root / "runs", root / "evidence/manual-review.json")
    save(scratch / "experiment-evidence-summary.json", recomputed)
    require(normalize_summary(recomputed) == normalize_summary(load(root / "results.json")), "archived results.json differs from frozen summarizer output")
    return {"status": "passed", "frozen_file_count": packet["frozen_file_count"], "packet_file_count": packet["packet_file_count"],
            "packet_regeneration": packet["generation"], "judge_selftest": judge_selftest,
            "runs": [{key: item[key] for key in ("run_id", "snapshots", "operation_count", "token_usage_available")} for item in observations],
            "source_snapshot_count": 96, "manual_review_count": review_count,
            "serialization_compatibility": serializations,
            "checked_case_results": consistency["checked_case_results"],
            "telemetry_limitations": consistency["limitations"],
            "scope": "Archived experiment integrity and arithmetic; participant failures are measurements, not CI failures."}


def self_test():
    try:
        strict_pairs([("x", 1), ("x", 2)])
        raise AssertionError("duplicate JSON key accepted")
    except EvidenceError:
        pass
    request = {"operation": "doc", "arguments": ["cheatsheet", "1", "180"]}
    event = {"operation": "doc", "arguments": ["cheatsheet", 1, 180]}
    require(compare_arguments(request, event) == [1, 2], "valid frozen doc scalar serialization rejected")
    invalid_pairs = [
        (request, {"operation": "doc", "arguments": ["cheatsheet", 2, 180]}),
        (request, {"operation": "doc", "arguments": ["cheatsheet", 1.0, 180]}),
        (request, {"operation": "doc", "arguments": ["cheatsheet", True, 180]}),
        ({"operation": "doc", "arguments": ["cheatsheet", "01", "180"]}, event),
        ({"operation": "doc", "arguments": ["cheatsheet", 1, "180"]}, event),
        (request, {"operation": "doc", "arguments": ["tasks", 1, 180]}),
        ({"operation": "read", "arguments": ["T01", "1"]}, {"operation": "read", "arguments": ["T01", 1]}),
    ]
    for before, after in invalid_pairs:
        try:
            compare_arguments(before, after)
            raise AssertionError("non-equivalent or noncanonical arguments accepted")
        except EvidenceError:
            pass
    with tempfile.TemporaryDirectory(prefix="yavo-r5-gate-selftest-") as temporary:
        root = Path(temporary)
        for path in ("../outside", "/absolute", "x\\y"):
            try:
                safe(root, path)
                raise AssertionError("unsafe path accepted")
            except EvidenceError:
                pass
        files = {"main.rs": "a" * 64, "util/convert.rs": "b" * 64}
        command = ["perl", "-MJSON::PP", "-MDigest::SHA=sha256_hex", "-e",
                   "local $/; my $x=decode_json(<STDIN>); print sha256_hex(JSON::PP->new->canonical->pretty->utf8->encode($x));"]
        digest = subprocess.run(command, input=json.dumps(files), text=True, capture_output=True, check=True).stdout
        require(source_digest(files) == digest, "source manifest serializer differs from frozen Perl implementation")
        (root / "main.rs").write_bytes(b"original\n")
        before = source_files(root)
        (root / "main.rs").write_bytes(b"tampered\n")
        require(source_files(root) != before, "source mutation undetected")
        observed = {"output_limit_bytes_per_stream": 10, "stdin_sha256": hashlib.sha256(b"").hexdigest(),
                    "started_monotonic_ns": 1, "finished_monotonic_ns": 1000000001, "elapsed_seconds": 1,
                    **{stream: {"bytes": 3, "sha256": hashlib.sha256(b"ok\n").hexdigest(), "base64": base64.b64encode(b"ok\n").decode(),
                                "capture_truncated": False} for stream in ("stdout", "stderr")}}
        directory = root / "command"
        directory.mkdir()
        (directory / "stdin").write_bytes(b"")
        for stream in ("stdout", "stderr"):
            (directory / stream).write_bytes(b"ok\n")
        save(directory / "command.json", observed)
        compare_observation(observed, directory, b"")
        (directory / "stdout").write_bytes(b"no\n")
        try:
            compare_observation(observed, directory, b"")
            raise AssertionError("raw output tampering accepted")
        except EvidenceError:
            pass
    return {"status": "passed", "checks": ["duplicate JSON keys", "canonical doc line-number scalar compatibility and rejection cases", "path traversal", "Perl source-manifest digest",
                                              "source mutation", "raw output bytes and tampering"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    repository = Path(__file__).resolve().parents[2]
    parser.add_argument("--experiment", type=Path, default=repository / EXPERIMENT)
    parser.add_argument("--output", type=Path, default=repository / "compiler/target/ci/experiment-evidence.json")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        print(json.dumps(self_test()))
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    receipt = {"schema_version": 1, "freeze_commit": FREEZE_COMMIT, "frozen_manifest_sha256": FROZEN_SHA256,
               "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(), "gate_sha256": sha(Path(__file__)),
               "github_sha": os.environ.get("GITHUB_SHA"), "status": "running"}
    started = time.monotonic()
    try:
        receipt.update(verify(args.experiment.resolve(), args.output.parent))
    except (EvidenceError, OSError, ValueError, KeyError, TypeError, AssertionError) as error:
        receipt.update({"status": "failed", "error": f"{type(error).__name__}: {error}"})
    receipt["elapsed_seconds"] = time.monotonic() - started
    save(args.output, receipt)
    print(json.dumps({"status": receipt["status"], "receipt": str(args.output), "error": receipt.get("error")}))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
