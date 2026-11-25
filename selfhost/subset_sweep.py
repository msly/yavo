#!/usr/bin/env python3
"""Check the strict-subset rule on every Yavo program in the repository.

For each .yv file outside selfhost/ and build outputs, stage0 `check` decides
acceptance. A program stage0 accepts must either pass the selfhost `check` or be
refused with U0001 as its first diagnostic; a program stage0 rejects must not be
accepted by the selfhost. Exit status 1 lists every violation. The accepted
count is the subset's coverage of the repository's real programs.

    python3 selfhost/subset_sweep.py --selfhost <yavo-selfhost> [--stage0 <yavo>]
"""
from __future__ import annotations

import argparse
from collections import Counter
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EXE = ".exe" if os.name == "nt" else ""
CODE = re.compile(r"\b([EU][0-9]{4})\b")


def check(compiler, path):
    try:
        done = subprocess.run([str(compiler), "check", path.name], cwd=path.parent, capture_output=True,
                              timeout=30, text=True, errors="replace")
    except subprocess.TimeoutExpired:
        return None, "timeout"
    lines = done.stderr.strip().splitlines()
    return done.returncode, lines[0] if lines else ""


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selfhost", type=Path, required=True)
    parser.add_argument("--stage0", type=Path, default=ROOT / ("compiler/target/release/yavo" + EXE))
    args = parser.parse_args()
    accepted = covered = rejected = 0
    violations, reasons = [], Counter()
    for path in sorted(ROOT.rglob("*.yv")):
        relative = path.relative_to(ROOT)
        if relative.parts[0] == "selfhost" or "target" in relative.parts:
            continue
        reference, _ = check(args.stage0.resolve(), path)
        if reference is None:
            continue
        result, first = check(args.selfhost.resolve(), path)
        if reference == 0:
            accepted += 1
            if result == 0:
                covered += 1
                continue
            found = CODE.search(first)
            if found and found[1] == "U0001":
                reasons[first.split("U0001: ", 1)[-1]] += 1
            else:
                violations.append(f"{relative}: stage0 accepts, selfhost: {first}")
        else:
            rejected += 1
            if result == 0:
                violations.append(f"{relative}: stage0 rejects, selfhost accepts")
    print(f"stage0 accepts {accepted}; selfhost accepts {covered} of them; stage0 rejects {rejected}")
    for reason, count in reasons.most_common(15):
        print(f"{count:5d}  {reason}")
    for line in violations:
        print("VIOLATION " + line)
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
