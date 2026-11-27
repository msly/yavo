#!/usr/bin/env python3
"""A small CLI-metered repeat of the existing twelve-task Yavo/Rust packet.

The model returns source JSON; this controller compiles and feeds back only
public failures. Provider-request IDs are not required. CLI turn usage is
preserved verbatim and labelled as such; missing counters stop the run.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import time

SCHEMA = {"type": "object", "properties": {"files": {"type": "array", "items": {
    "type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
    "required": ["path", "content"], "additionalProperties": False}}},
    "required": ["files"], "additionalProperties": False}

def write(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def usage(events):
    if any(not isinstance(e, dict) for e in events):
        raise ValueError("invalid CLI event shape")
    turns = [e for e in events if e.get("type") == "turn.completed"]
    if len(turns) != 1:
        raise ValueError("expected exactly one completed CLI turn")
    raw = turns[0].get("usage", {})
    if not isinstance(raw, dict):
        raise ValueError("missing CLI usage object")
    for key in ("input_tokens", "output_tokens"):
        if type(raw.get(key)) is not int or raw[key] <= 0:
            raise ValueError("missing or invalid CLI " + key)
    cached = raw.get("cached_input_tokens")
    if cached is not None and (type(cached) is not int or not 0 <= cached <= raw["input_tokens"]):
        raise ValueError("invalid cached input counter")
    if "total_tokens" in raw and raw["total_tokens"] != raw["input_tokens"] + raw["output_tokens"]:
        raise ValueError("inconsistent CLI token total")
    return {"scope": "codex_cli_turn_completed", "raw": raw,
            "input_tokens": raw["input_tokens"], "output_tokens": raw["output_tokens"],
            "cached_input_tokens": cached, "total_tokens": raw["input_tokens"] + raw["output_tokens"]}

def run_process(argv, cwd, stdin="", timeout=120, env=None):
    started = time.monotonic()
    try:
        p = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, env=env, start_new_session=os.name != "nt")
        try:
            stdout, stderr = p.communicate(stdin.encode("utf-8"), timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name != "nt":
                os.killpg(p.pid, signal.SIGKILL)
            else:
                p.kill()
            stdout, stderr = p.communicate()
            return {"argv": argv, "elapsed_seconds": time.monotonic() - started,
                    "exit_code": None, "stdout": stdout.decode("utf-8", "replace"),
                    "stderr": stderr.decode("utf-8", "replace"), "timeout": True}
        result = {"exit_code": p.returncode, "stdout": stdout.decode("utf-8", "replace"),
                  "stderr": stderr.decode("utf-8", "replace"),
                  "timeout": False}
    except OSError as e:
        result = {"exit_code": None, "stdout": "", "stderr": str(e), "timeout": False}
    return {"argv": argv, "elapsed_seconds": time.monotonic() - started, **result}

def model_call(args, directory, prompt, schema=None):
    directory.mkdir(parents=True, exist_ok=False)
    (directory / "prompt.txt").write_text(prompt, encoding="utf-8")
    argv = [args.codex, "exec", "--ephemeral", "--skip-git-repo-check", "--sandbox", "read-only", "--json"]
    if args.model:
        argv += ["--model", args.model]
    if args.effort:
        argv += ["-c", 'model_reasoning_effort="' + args.effort + '"']
    if schema is not None:
        write(directory / "schema.json", schema)
        argv += ["--output-schema", str((directory / "schema.json").resolve())]
    final = directory / "final.txt"
    argv += ["--output-last-message", str(final.resolve()), "-"]
    result = run_process(argv, directory, prompt, args.timeout)
    (directory / "events.jsonl").write_text(result.pop("stdout"), encoding="utf-8")
    (directory / "stderr.txt").write_text(result.pop("stderr"), encoding="utf-8")
    try:
        events = [json.loads(s) for s in (directory / "events.jsonl").read_text().splitlines() if s.strip()]
        if result["exit_code"] != 0:
            raise ValueError("CLI failed; see stderr.txt")
        result["usage"] = usage(events)
        tool_items = [e.get("item", {}).get("type") for e in events if e.get("type") == "item.completed"]
        if any(t not in (None, "agent_message", "reasoning") for t in tool_items):
            raise ValueError("participant used an unpermitted tool")
        result["status"] = "metered"
    except (ValueError, TypeError) as e:
        result.update(status="blocked", reason=str(e), usage=None)
    result["requested_model"] = args.model
    result["reported_model"] = None
    result["requested_reasoning_effort"] = args.effort
    write(directory / "capture.json", result)
    return result, final.read_text(encoding="utf-8") if final.exists() else ""

def probe(args, directory):
    result, text = model_call(args, directory, "Return exactly READY. Do not call tools or inspect files.")
    result["ready"] = text.strip() == "READY"
    result["qualified_for_this_cli_pilot"] = result["status"] == "metered" and result["ready"]
    write(directory / "probe.json", result)
    return result

def source_files(text, directory, language):
    data = json.loads(text)
    if not isinstance(data, dict) or not isinstance(data.get("files"), list) or not data["files"]:
        raise ValueError("expected files array")
    suffix = ".yv" if language == "yavo" else ".rs"
    sources = {}
    for item in data["files"]:
        if not isinstance(item, dict):
            raise ValueError("invalid source file object")
        path, content = item.get("path"), item.get("content")
        if not isinstance(path, str) or not isinstance(content, str):
            raise ValueError("file path/content must be strings")
        p = PurePosixPath(path)
        if (not path or p.is_absolute() or ".." in p.parts or "\\" in path or p.suffix != suffix
                or path in sources or p.as_posix() != path):
            raise ValueError("invalid or duplicate source path")
        sources[path] = content
    if "main" + suffix not in sources:
        raise ValueError("missing main" + suffix)
    directory.mkdir(parents=True, exist_ok=False)
    for path, content in sources.items():
        f = directory / path
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(content, encoding="utf-8")
    return {path: sha(directory / path) for path in sorted(sources)}

def compile_program(args, directory, language):
    binary = (directory / ("program.exe" if os.name == "nt" else "program")).resolve()
    if language == "yavo":
        argv = [str(Path(args.yavo).resolve()), "build", "main.yv", "-o", str(binary)]
    else:
        argv = [str(Path(args.rustc).resolve()), "--edition=2021", "-C", "overflow-checks=yes"]
        if args.rust_linker:
            argv += ["-C", "linker=" + str(Path(args.rust_linker).resolve())]
        argv += ["main.rs", "-o", str(binary)]
    env = {k: v for k, v in os.environ.items() if k not in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY")}
    return run_process(argv, directory, env=env), binary

def judge(binary, cases, directory):
    results = []
    for case in cases:
        case_dir = directory / case["id"]
        case_dir.mkdir(parents=True, exist_ok=True)
        # The frozen packet currently has no companion files. Fail closed if that changes.
        if case.get("files"):
            raise ValueError("unexpected companion files in frozen task packet")
        env = {k: v for k, v in os.environ.items() if k not in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY")}
        result = run_process([str(binary), *case.get("args", [])], case_dir, case["stdin"], timeout=5, env=env)
        result["id"] = case["id"]
        result["passed"] = (not result["timeout"] and result["exit_code"] == case["exit_code"]
                            and result["stdout"] == case["stdout"] and result["stderr"] == case["stderr"])
        results.append(result)
    return results

def packet(repo):
    base = repo / "experiments/coldstart-2026-11-22-r5"
    text = (base / "TASKS.md").read_text(encoding="utf-8")
    parts = re.split(r"(?m)^## (T\d\d) ", text)
    specs = {parts[i]: parts[0] + "\n## " + parts[i] + " " + parts[i+1] for i in range(1, len(parts), 2)}
    cases = json.loads((base / "judge/cases.json").read_text())["tasks"]
    if len(specs) != 12 or {t["id"] for t in cases} != set(specs):
        raise ValueError("expected twelve paired tasks")
    files = [base / "TASKS.md", base / "judge/cases.json", repo / "docs/cheatsheet.md"]
    return specs, cases, {str(p.relative_to(repo)): sha(p) for p in files}

def summarize(directory):
    run = json.loads((directory / "run.json").read_text())
    rows = [json.loads(p.read_text()) for p in sorted(directory.glob("tasks/*/*/result.json"))]
    review_path = directory / "contract-reviews.json"
    reviews = json.loads(review_path.read_text()) if review_path.exists() else {}
    for row in rows:
        review = reviews.get(row["task"] + "/" + row["language"])
        if review is not None:
            if (review.get("source_manifest") != row.get("source_manifest")
                    or type(review.get("passed")) is not bool or not review.get("reason")):
                raise ValueError("contract review must bind final source hashes, passed and reason")
            row["contract_correct"] = bool(row.get("behavior_correct")) and review["passed"]
    output = {"status": run["status"], "scope": "twelve_task_repeat_cli_metered_codegen_loop",
              "requested_model": run["model"], "reported_model": None, "tasks": rows, "languages": {}}
    for language in ("yavo", "rust"):
        selected = [r for r in rows if r["language"] == language]
        valid = bool(selected) and all(r["usage_complete"] for r in selected)
        totals = {key: sum(r[key] for r in selected) if valid else None
                  for key in ("input_tokens", "output_tokens", "total_tokens")}
        correct = sum(r.get("behavior_correct") is True for r in selected)
        pending = [r["task"] for r in selected if r.get("structural_requirements")
                   and r.get("behavior_correct") and r.get("contract_correct") is None]
        output["languages"][language] = {"attempted_tasks": len(selected),
            "completed_tasks": sum(type(r.get("behavior_correct")) is bool for r in selected), "target_tasks": 12,
            "behavior_correct": correct, "contract_review_pending": pending,
            "contract_correct": None if pending else sum(r.get("contract_correct") is True for r in selected),
            "compiler_invocations": sum(r["compiler_invocations"] for r in selected),
            "compiler_failures": sum(r["compiler_failures"] for r in selected),
            "repair_rounds": sum(r["repair_rounds"] for r in selected), **totals,
            "tokens_per_behavior_success": totals["total_tokens"] / correct if valid and correct else None,
            "monetary_cost": None}
    y, r = output["languages"]["yavo"], output["languages"]["rust"]
    output["tokens_ratio_yavo_over_rust"] = (y["total_tokens"] / r["total_tokens"]
        if run["status"] == "complete" and y["total_tokens"] is not None and r["total_tokens"] else None)
    write(directory / "summary.json", output)
    return output

def execute(args):
    repo = args.repo.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    specs, tasks, identities = packet(repo)
    identities.update({"yavo_binary": sha(Path(args.yavo)), "rustc_binary": sha(Path(args.rustc)),
                       "runner": sha(Path(__file__))})
    meta = {"status": "probing", "model": args.model, "reasoning_effort": args.effort,
            "max_attempts": args.max_attempts, "identities": identities,
            "cli_version": run_process([args.codex, "--version"], out), "participant_sessions": 0,
            "task_design": "same R5 twelve-task packet, one independent context per task and language"}
    write(out / "run.json", meta)
    if not probe(args, out / "probe")["qualified_for_this_cli_pilot"]:
        meta["status"] = "blocked_before_participants"
        write(out / "run.json", meta)
        summarize(out)
        return 2
    guide = (repo / "docs/cheatsheet.md").read_text(encoding="utf-8")
    sealed = []
    for number, task in enumerate(tasks):
        for language in (("yavo", "rust") if number % 2 == 0 else ("rust", "yavo")):
            directory = out / "tasks" / task["id"] / language
            directory.mkdir(parents=True)
            prompt = ("Implement this command-line task in " + language + ". Return JSON with a files array, "
                "each item containing path and complete content. Entry file must be main.yv or main.rs. "
                "Do not call tools or inspect files. The controller compiles and supplies public feedback.\n\n"
                + specs[task["id"]] + ("\n\nYavo language guide:\n" + guide if language == "yavo" else ""))
            row = {"task": task["id"], "language": language, "compiler_invocations": 0,
                   "compiler_failures": 0, "repair_rounds": 0, "usage_complete": True,
                   "input_tokens": 0, "output_tokens": 0, "total_tokens": 0,
                   "structural_requirements": task.get("structural", [])}
            final_binary = None
            for attempt in range(1, args.max_attempts + 1):
                meta["participant_sessions"] += 1
                capture, response = model_call(args, directory / f"attempt-{attempt}", prompt, SCHEMA)
                if capture["status"] != "metered":
                    row.update(usage_complete=False, behavior_correct=None, reason=capture.get("reason"))
                    write(directory / "result.json", row)
                    meta["status"] = "blocked_missing_usage_or_cli_failure"
                    write(out / "run.json", meta)
                    summarize(out)
                    return 2
                for key in ("input_tokens", "output_tokens", "total_tokens"):
                    row[key] += capture["usage"][key]
                row["repair_rounds"] = attempt - 1
                try:
                    source = directory / f"sources-{attempt}"
                    row["source_manifest"] = source_files(response, source, language)
                    build, binary = compile_program(args, source, language)
                    write(directory / f"build-{attempt}.json", build)
                    row["compiler_invocations"] += 1
                    if build["exit_code"] == 0 and not build["timeout"]:
                        final_binary = binary
                        public = judge(binary, [c for c in task["cases"] if c["visibility"] == "public"], directory / f"public-{attempt}")
                        write(directory / f"public-{attempt}.json", public)
                        feedback = [c for c in public if not c["passed"]]
                        if not feedback:
                            break
                    else:
                        final_binary = None
                        row["compiler_failures"] += 1
                        feedback = build
                except (ValueError, KeyError, TypeError) as e:
                    final_binary = None
                    feedback = {"invalid_submission": str(e)}
                prompt += "\n\nPrevious source JSON:\n" + response + "\n\nPublic feedback:\n" + json.dumps(feedback, ensure_ascii=False) + "\nReturn complete corrected source JSON."
            row["final_binary"] = str(final_binary) if final_binary else None
            write(directory / "result.json", row)
            sealed.append((directory, row, task, final_binary))
            write(out / "run.json", meta)
    # Hidden answers are never fed back, including across the twelve task pairs.
    for directory, row, task, binary in sealed:
        results = judge(binary, task["cases"], directory / "judge") if binary else []
        write(directory / "judgment.json", results)
        row["behavior_correct"] = bool(results) and all(c["passed"] for c in results)
        row["contract_correct"] = None if row["structural_requirements"] and row["behavior_correct"] else row["behavior_correct"]
        write(directory / "result.json", row)
    meta["status"] = "complete"
    write(out / "run.json", meta)
    summarize(out)
    return 0

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode", choices=["probe", "run", "summarize"])
    p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--codex", default="codex")
    p.add_argument("--model")
    p.add_argument("--effort", choices=["low", "medium", "high", "xhigh", "max", "ultra"])
    p.add_argument("--yavo")
    p.add_argument("--rustc")
    p.add_argument("--rust-linker")
    p.add_argument("--timeout", type=int, default=180)
    p.add_argument("--max-attempts", type=int, choices=range(1, 9), default=4)
    args = p.parse_args()
    if args.mode == "probe":
        result = probe(args, args.out.resolve())
        print(json.dumps(result, ensure_ascii=False))
        return 0 if result["qualified_for_this_cli_pilot"] else 2
    if args.mode == "summarize":
        result = summarize(args.out.resolve())
        print(json.dumps({k: v for k, v in result.items() if k != "tasks"}, ensure_ascii=False))
        return 0
    if not args.model or not args.effort or not args.yavo or not args.rustc:
        p.error("run requires --model, --effort, --yavo and --rustc for both language arms")
    return execute(args)

if __name__ == "__main__":
    raise SystemExit(main())
