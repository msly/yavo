"""Independent standard-library error contract; never invokes a Yavo compiler."""
from __future__ import annotations

from pathlib import PurePosixPath
import re


def payload_error(data):
    try:
        data.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        return error.start
    return None


def evaluate(case):
    mode, args = case["mode"], case["args"]
    before = dict(case["files"])
    after = dict(before)
    output = b""
    path = str(PurePosixPath(args[0])) if args else ""
    if mode.endswith("-nul"):
        family = "os" if mode == "env-nul" else "process" if mode.startswith("process") else "fs"
        output = (family + ":invalid\n").encode()
    elif mode == "read":
        if path not in before or before[path] is None:
            output = b"fs:io:true\n"
        else:
            data = bytes.fromhex(before[path])
            offset = payload_error(data)
            output = b"read:" + data + b"\n" if offset is None else f"fs:utf8:{offset}\n".encode()
    elif mode == "stdin":
        if case.get("stdin_error"):
            output = b"io:io:true\n"
        else:
            data = bytes.fromhex(case["stdin_hex"])
            offset = payload_error(data)
            output = b"stdin:" + data + b"\n" if offset is None else f"io:utf8:{offset}\n".encode()
    elif mode == "write":
        parent = str(PurePosixPath(path).parent)
        if path in before or (parent != "." and before.get(parent, "missing") is not None):
            output = b"fs:io:true\n"
        else:
            after[path] = b"written\n".hex()
            output = b"written\n"
    elif mode == "temp":
        prefix = args[1]
        if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", prefix):
            output = b"fs:invalid\n"
        elif path != "." and before.get(path, "missing") is not None:
            output = b"fs:io:true\n"
        else:
            output = b"temporary-clean\n"
    elif mode == "remove-file":
        if path in before and before[path] is not None:
            del after[path]
            output = b"removed\n"
        else:
            output = b"fs:io:true\n"
    elif mode == "remove-dir":
        if path in before and before[path] is None and not any(p.startswith(path + "/") for p in before):
            del after[path]
            output = b"removed\n"
        else:
            output = b"fs:io:true\n"
    elif mode == "same":
        second = str(PurePosixPath(args[1]))
        same = path in before and second in before and path == second
        output = b"same:true\n" if same else b"same:false\n"
    elif mode == "env":
        if not args[0] or "=" in args[0]:
            output = b"os:invalid\n"
        else:
            value = case["environment"].get(args[0])
            data = b"fallback\x00tail" if value is None else bytes.fromhex(value["hex"])
            offset = payload_error(data)
            output = b"env:" + data + b"\n" if offset is None else f"os:utf8:{offset}\n".encode()
    elif mode == "process-self":
        output = b"child\nstatus:17\n"
    elif mode == "process":
        output = b"process:invalid\n" if not args[0] else b"process:io:true\n"
    elif mode == "utf8":
        output = b"utf8:1\n"
    else:
        raise ValueError("undefined frozen operation: " + mode)
    return {"exit": 0, "stdout_hex": output.hex(), "stderr_hex": "", "files_after": after}


def compare(wanted, exit_code, stdout, stderr, files_after):
    return {
        "exit": type(exit_code) is int and exit_code == wanted["exit"],
        "stdout": stdout == bytes.fromhex(wanted["stdout_hex"]),
        "stderr": stderr == bytes.fromhex(wanted["stderr_hex"]),
        "files_after": files_after == wanted["files_after"],
    }
