#!/usr/bin/env python3
"""Calibrate planning flags using accepted run/panic fixtures, without C builds."""
from __future__ import annotations
import argparse
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("subset_sweep", ROOT/"selfhost/subset_sweep.py")
sweep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sweep)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--analyzer",type=Path,required=True)
    parser.add_argument("--selfhost",type=Path,required=True)
    parser.add_argument("--json",type=Path,required=True)
    args=parser.parse_args()
    analyzer,selfhost=args.analyzer.resolve(),args.selfhost.resolve()
    manifest=json.loads((ROOT/"selfhost/tests/cases.json").read_text())
    rows=[]
    for case in manifest["cases"]:
        if case["kind"] not in ("run","panic"):continue
        row={"id":case["id"],"file":case["file"],"kind":case["kind"]}
        if case.get("source_prefix"):
            row.update({"status":"skipped","reason":"requires source_prefix composition",
                        "prefix":case["source_prefix"]})
        else:
            path=ROOT/"selfhost/tests"/case["file"]
            actual=sweep.check(selfhost,path)
            model=sweep.analyze(analyzer,path,30,"batch52")
            row.update({"selfhost":actual,"raw_model_blockers":model.get("blockers",{}),
                        "unknown":model.get("unknown",[]),
                        "ast_complete":model.get("ast_complete",False)})
            row["status"]="passed" if (actual["exit"]==0 and row["ast_complete"] and not row["raw_model_blockers"]) else "failed"
        rows.append(row)
    report={"schema":"yavo-subset-profile-calibration/1",
            "scope":"Read-only check and raw AST/HIR flags for current run/panic fixtures; source_prefix cases explicitly skipped; no program execution or C compilation.",
            "analyzer_sha256":sweep.sha256(analyzer),"selfhost_sha256":sweep.sha256(selfhost),
            "summary":{k:sum(r["status"]==k for r in rows) for k in ("passed","failed","skipped")},
            "rows":rows}
    report["passed"]=report["summary"]["failed"]==0
    args.json.parent.mkdir(parents=True,exist_ok=True)
    args.json.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(report["summary"]))
    for row in rows:
        if row["status"]=="failed":
            print(row["file"],row["selfhost"]["exit"],list(row["raw_model_blockers"]),row["unknown"])
    return 0 if report["passed"] else 1


if __name__=="__main__":sys.exit(main())
