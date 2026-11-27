"""Shared frozen-oracle, byte matching and process-tree execution for CSV gates."""
from __future__ import annotations

import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "examples/csv-check"
TESTS = APP / "tests"
CONTROLS = ROOT / "examples/test-summary/tests"
HEADER = "file\tcolumns\trows\tbytes\n"
USAGE = "Usage: yavo-csv-check [--] [FILE...]\n"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    with Path(path).open("rb") as source:
        state = hashlib.sha256()
        for block in iter(lambda: source.read(1024 * 1024), b""):
            state.update(block)
    return state.hexdigest()


def identity(path):
    path = Path(path).resolve()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def _payload(value):
    if "text" in value:
        return value["text"].encode("utf-8")
    return base64.b64decode(value["base64"], validate=True)


def _spelling(value, absolute_root=None):
    if not value.startswith("@abs:") or absolute_root is None:
        return value
    root = Path(absolute_root).resolve()
    path = (root / value[len("@abs:"):]).resolve()
    if not path.is_relative_to(root):
        raise ValueError("absolute fixture escapes its isolated root")
    return str(path)


def load_case(case, absolute_root=None):
    args = [_spelling(value, absolute_root) for value in case["args"]]
    contents = {_spelling(item["path"], absolute_root):
                None if item.get("directory") else _payload(item) for item in case["files"]}
    wanted = copy.deepcopy(case["expected"])
    for row in wanted.get("rows") or []:
        row[0] = _spelling(row[0], absolute_root)
    for event in wanted["diagnostics"]:
        if "path" in event:
            event["path"] = _spelling(event["path"], absolute_root)
    return args, contents, _payload(case["stdin"]), wanted


def frozen_oracle():
    paths = {"spec": APP / "SPEC.md", "cases": TESTS / "cases.json",
             "oracle": TESTS / "oracle.py", "freeze": TESTS / "oracle-freeze.json"}
    bound = {str(path.resolve()): digest(path) for path in paths.values()}
    freeze = json.loads(paths["freeze"].read_text(encoding="utf-8"))
    manifest = json.loads(paths["cases"].read_text(encoding="utf-8"))
    for name in ("spec", "cases", "oracle"):
        if freeze[name + "_sha256"] != bound[str(paths[name].resolve())]:
            raise ValueError("frozen oracle identity changed: " + name)
    if manifest["spec_sha256"] != freeze["spec_sha256"]:
        raise ValueError("case contract identity differs")
    cases = manifest["cases"]
    if not cases or len(cases) != freeze["case_count"]:
        raise ValueError("frozen case count differs")
    if len({item["id"] for item in cases}) != len(cases):
        raise ValueError("duplicate scenario id")
    spec = importlib.util.spec_from_file_location("csv_contract_oracle", paths["oracle"])
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    for case in cases:
        args, contents, stdin, wanted = load_case(case)
        got = oracle.evaluate(args, contents, stdin, case.get("stdin_error", False))
        if got != wanted:
            raise ValueError("manual expectation disagrees with independent oracle: " + case["id"])
    review = {"status": "pass", "scenarios": len(cases), "case_count": len(cases),
              "manual_expected_match": True, "cases_sha256": freeze["cases_sha256"],
              "oracle_sha256": freeze["oracle_sha256"], "spec_sha256": freeze["spec_sha256"],
              "applicable": sum(os.name in case.get("platforms", [os.name]) for case in cases)}
    return cases, bound, review


def input_identities(include_app=True):
    files = [Path(__file__).resolve(), APP / "SPEC.md", TESTS / "cases.json",
             TESTS / "oracle.py", TESTS / "oracle-freeze.json",
             ROOT / "compiler/runtime/yavo_rt.h", ROOT / "compiler/toolchain.lock",
             ROOT / "tools/ci/test_summary.py",
             *[CONTROLS / name for name in ("exit_check.h", "cleanup_clean.c", "cleanup_leak.c")]]
    for name in ("tools/ci/csv_check.py", "tools/ci/csv_sanitize.py", "selfhost/csv_application.py"):
        path = ROOT / name
        if path.is_file():
            files.append(path)
    if include_app:
        files.extend(sorted(APP.glob("*.yv")))
    return {str(path.resolve()): digest(path) for path in files}


def locked_zig():
    values = {}
    for line in (ROOT / "compiler/toolchain.lock").read_text(encoding="utf-8").splitlines():
        if "=" in line and not line.lstrip().startswith("#"):
            key, value = line.split("=", 1)
            values[key.strip()] = value.strip()
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64",
            "aarch64": "aarch64"}.get(platform.machine().lower())
    system = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(platform.system())
    host = str(arch) + "-" + str(system)
    if host not in values:
        raise ValueError("host has no locked Zig toolchain: " + host)
    tool_home = Path(os.environ["YAVO_HOME"]) if os.environ.get("YAVO_HOME") else Path.home() / ".yavo"
    path = tool_home / "toolchains" / ("zig-" + host + "-" + values["zig_version"])
    path /= "zig.exe" if os.name == "nt" else "zig"
    if not path.is_file():
        raise ValueError("locked Zig is not installed; automatic download forbidden: " + str(path))
    return path.resolve(), values["zig_version"], values[host]


def escape_path(value):
    return value.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r")


def expected_stdout(wanted):
    if wanted.get("help"):
        return USAGE.encode("utf-8")
    if wanted["rows"] is None:
        return b""
    rows = [HEADER]
    for path, columns, count, size in wanted["rows"]:
        rows.append(escape_path(path) + "\t" + "\t".join(str(n) for n in (columns, count, size)) + "\n")
    return "".join(rows).encode("utf-8")


def stderr_matches(data, wanted):
    events = wanted["diagnostics"]
    if not events:
        return data == b""
    if len(events) != 1 or not data.endswith(b"\n"):
        return False
    lines = data[:-1].split(b"\n")
    event = events[0]
    if event["code"] == "ECLI_USAGE":
        if len(lines) != 2 or lines[1] != USAGE.rstrip("\n").encode("utf-8"):
            return False
        prefix = b"ECLI_USAGE: "
    else:
        if len(lines) != 1:
            return False
        prefix = escape_path(event["path"])
        if "line" in event:
            prefix += ":" + str(event["line"]) + ":" + str(event["column"])
        prefix = (prefix + ": " + event["code"] + ": ").encode("utf-8")
    if not lines[0].startswith(prefix):
        return False
    try:
        explanation = lines[0][len(prefix):].decode("utf-8", errors="strict")
    except UnicodeError:
        return False
    return bool(explanation.strip()) and "\r" not in explanation


def materialize(case, directory, contents, absolute_root=None):
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    written = []
    for source in case["files"]:
        spelling = _spelling(source["path"], absolute_root)
        if source["path"].startswith("@abs:"):
            if absolute_root is None:
                raise ValueError("absolute fixture needs a stable isolated root")
            root = Path(absolute_root).resolve()
            path = Path(spelling)
        else:
            root = directory
            path = (directory / source["path"]).resolve()
        if not path.is_relative_to(root):
            raise ValueError("fixture escapes its isolated directory")
        path.parent.mkdir(parents=True, exist_ok=True)
        if source.get("directory"):
            path.mkdir(exist_ok=True)
            written.append({"path": str(path), "kind": "directory"})
        else:
            data = contents[spelling]
            if path.exists() and path.read_bytes() != data:
                raise ValueError("shared absolute fixture changed")
            path.write_bytes(data)
            written.append(identity(path))
    return written


def _wait_process(process, timeout):
    """Reap exactly this child without losing its wait4 resource usage."""
    if sys.platform not in ("linux", "darwin") or not hasattr(os, "wait4"):
        return (process.wait(timeout=timeout), None,
                "unavailable: wait4 ru_maxrss is not supported by this runner on " + sys.platform)
    deadline = time.monotonic() + timeout
    delay = 0.0005
    while True:
        try:
            pid, status, usage = os.wait4(process.pid, os.WNOHANG)
        except InterruptedError:
            continue
        if pid:
            process.returncode = os.waitstatus_to_exitcode(status)
            scale = 1024 if sys.platform == "linux" else 1
            unit = "KiB converted to bytes" if sys.platform == "linux" else "bytes"
            return (process.returncode, int(usage.ru_maxrss) * scale,
                    "wait4 ru_maxrss (" + unit + "); OS-reported RSS for the waited child; "
                    "not a simultaneous process-tree RSS sum")
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise subprocess.TimeoutExpired(process.args, timeout)
        time.sleep(min(delay, remaining))
        delay = min(delay * 2, 0.01)


def _kill_process(process):
    # Popen.kill() polls on POSIX and could reap the child before wait4 records RSS.
    if os.name == "posix":
        try:
            os.kill(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    else:
        process.kill()


def execute(argv, directory, cwd, environment, overrides, timeout, stdin=None, stdin_error=False):
    """Regular-file streams plus process-group kill bound descendants and stdin."""
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    arguments = [str(value) for value in argv]
    actual_environment = dict(environment)
    actual_environment.update({str(key): str(value) for key, value in overrides.items()})
    stdout_path, stderr_path = directory / "stdout.bin", directory / "stderr.bin"
    stdin_path, stdin_handle, stdin_fd = None, None, None
    started = time.monotonic()
    timed_out, exit_code, launch_error = False, None, None
    peak_memory = None
    memory_measurement = "unavailable: command did not start or was not reaped"
    try:
        if stdin_error:
            if os.name != "posix":
                raise ValueError("directory stdin read-error probe is POSIX-only")
            stdin_path = directory / "stdin-directory"
            stdin_path.mkdir()
            stdin_fd = os.open(stdin_path, os.O_RDONLY)
            child_stdin = stdin_fd
        elif stdin is not None:
            stdin_path = directory / "stdin.bin"
            stdin_path.write_bytes(stdin)
            stdin_handle = stdin_path.open("rb")
            child_stdin = stdin_handle
        else:
            child_stdin = subprocess.DEVNULL
        with stdout_path.open("wb") as stdout_file, stderr_path.open("wb") as stderr_file:
            options = {"start_new_session": True} if os.name == "posix" else {
                "creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
            try:
                process = subprocess.Popen(arguments, cwd=cwd, env=actual_environment,
                                           stdin=child_stdin, stdout=stdout_file, stderr=stderr_file, **options)
                try:
                    exit_code, peak_memory, memory_measurement = _wait_process(process, timeout)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    if os.name == "posix":
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    else:
                        try:
                            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                           stderr=subprocess.DEVNULL, timeout=10)
                        except (OSError, subprocess.TimeoutExpired):
                            _kill_process(process)
                    try:
                        exit_code, peak_memory, memory_measurement = _wait_process(process, 10)
                    except subprocess.TimeoutExpired:
                        _kill_process(process)
                        exit_code, peak_memory, memory_measurement = _wait_process(process, 10)
            except OSError as error:
                launch_error = str(error)
    finally:
        if stdin_handle is not None:
            stdin_handle.close()
        if stdin_fd is not None:
            os.close(stdin_fd)
    elapsed = time.monotonic() - started
    out, err = stdout_path.read_bytes(), stderr_path.read_bytes()
    record = {"argv": arguments, "cwd": str(Path(cwd).resolve()), "environment": dict(overrides),
              "exit_code": exit_code, "timed_out": timed_out, "launch_error": launch_error,
              "elapsed_seconds": elapsed, "peak_process_memory_bytes": peak_memory,
              "memory_measurement": memory_measurement,
              "stdout": identity(stdout_path), "stderr": identity(stderr_path),
              "stdin": ({"path": str(stdin_path), "kind": "directory-read-error"} if stdin_error else
                        identity(stdin_path) if stdin_path is not None else {"kind": "devnull"})}
    save(directory / "command.json", record)
    return record, out, err


def run_case(program, case, directory, environment, overrides, timeout=10, absolute_root=None):
    args, contents, stdin, wanted = load_case(case, absolute_root)
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    inputs = directory / "inputs"
    files = materialize(case, inputs, contents, absolute_root)
    command, out, err = execute([program, *args], directory / "command", inputs,
                                environment, overrides, timeout, stdin=stdin,
                                stdin_error=case.get("stdin_error", False))
    checks = {"completed": not command["timed_out"] and command["launch_error"] is None,
              "exit": command["exit_code"] == wanted["exit"],
              "stdout": out == expected_stdout(wanted), "stderr": stderr_matches(err, wanted)}
    row = {"id": case["id"], "passed": all(checks.values()), "checks": checks,
           "command": command, "input_files": files, "expected": wanted}
    return row, out, err
