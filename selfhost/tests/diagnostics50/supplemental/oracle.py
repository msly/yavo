"""Independent frozen legacy CLI comparator; never executes a compiler."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def check(case, exit_code, stdout, stderr, directory):
    failures = []
    wanted = case["expected"]
    if exit_code != wanted["exit"]:
        failures.append("exit status")
    if stdout:
        failures.append("stdout must be empty")
    if wanted["stderr"] == "usage":
        if b"usage:" not in stderr or not stderr.endswith(b"\n"):
            failures.append("missing usage diagnostic")
    elif stderr != wanted["stderr"].encode():
        failures.append("stderr bytes")
    output = case.get("output")
    if output is not None:
        target = Path(directory) / output
        if wanted["output"] == "frozen_c":
            if not target.is_file() or target.read_bytes() != (ROOT / "expected.c").read_bytes():
                failures.append("generated C differs from frozen legacy product")
        elif target.exists():
            failures.append("rejected command created an output")
    return failures
