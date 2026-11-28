#!/usr/bin/env python3
"""Use the actual final bootstrap compiler for the unchanged original corpus."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bootstrap-output", type=Path, required=True)
    parser.add_argument("--analyzer", type=Path,
                        help="current Rust analyzer when bootstrap used a preserved historical stage0")
    args = parser.parse_args()
    receipt = json.loads((args.bootstrap_output / "receipt.json").read_bytes())
    if receipt["status"] != "passed":
        raise RuntimeError("core sweep requires a passing fixed point")
    suffix = ".exe" if sys.platform == "win32" else ""
    compiler = Path(receipt["work"]) / "stages/stage2" / ("yavo-selfhost" + suffix)
    output = args.bootstrap_output / "subset-sweep.json"
    command = [sys.executable, str(ROOT / "selfhost/subset_sweep.py"),
                            "--selfhost", str(compiler),
                            "--stage0", receipt["inputs"]["stage0"]["path"],
                            "--corpus", str(ROOT / "selfhost/tests/original-corpus.json"),
                            "--json", str(output)]
    if args.analyzer is not None:
        command += ["--analyzer", str(args.analyzer.resolve())]
    result = subprocess.call(command, cwd=ROOT)
    if result:
        return result
    analyzer = args.analyzer.resolve() if args.analyzer is not None else receipt["inputs"]["stage0"]["path"]
    return subprocess.call([sys.executable, str(ROOT / "selfhost/calibrate_subset.py"),
                            "--analyzer", str(analyzer), "--selfhost", str(compiler),
                            "--json", str(args.bootstrap_output / "profile-calibration.json")], cwd=ROOT)


if __name__ == "__main__":
    sys.exit(main())
