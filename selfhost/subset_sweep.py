#!/usr/bin/env python3
"""Check the strict-subset rule and inventory all modeled blockers per module graph.

Normal selfhost check remains fail-fast. The separate stage0 subset-features
command walks the complete AST and typed IR even when selfhost stops at U0001.
It never edits source or treats an unsupported node as a successful check.

--corpus pins a source inventory (including ignored historical copies).
--json records every entry, its full module graph, blocker set and unknowns.
Feature rankings are candidates from the current incomplete capability model,
not rigorous bounds or claimed coverage gains. Unknowns remain explicit.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
EXE = ".exe" if os.name == "nt" else ""
CODE = re.compile(r"\b([EU][0-9]{4})\b")

# Capabilities are bundles, not a lexical-word census. Every occurrence is
# produced from parsed AST or checked IR and carries its original source span.
PLANS = {
    "copy_tuple_vec": {"vec_copy_tuple"},
    "copy_tuple_vec_and_numeric_builtins": {"vec_copy_tuple", "numeric_builtins"},
    "user_enum_and_match": {"enum_payload", "match_user_enum", "patterns_nested"},
    "optional_result_patterns": {"patterns_optional_result", "patterns_nested", "match_user_enum"},
    "generics_and_interfaces": {"generics", "generic_construction", "generic_expressions", "interfaces", "methods"},
    "complete_tuples": {"vec_copy_tuple", "vec_owned_tuple", "tuple_general", "tuple_destructure",
                        "aggregate_composition", "result_composition", "slice_general"},
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(compiler, path, timeout=30):
    started = time.monotonic()
    try:
        done = subprocess.run([str(compiler), "check", path.name], cwd=path.parent,
                              capture_output=True, timeout=timeout, text=True, encoding="utf-8", errors="replace")
    except subprocess.TimeoutExpired:
        return {"exit": None, "first": "timeout", "code": None, "seconds": time.monotonic()-started}
    lines = done.stderr.strip().splitlines()
    first = lines[0] if lines else ""
    found = CODE.search(first)
    return {"exit": done.returncode, "first": first, "code": found[1] if found else None,
            "seconds": time.monotonic()-started}


def inventory(corpus=None):
    if corpus:
        value = json.loads(corpus.read_text(encoding="utf-8"))
        entries = value.get("files", value.get("rows", []))
        if not isinstance(entries, list) or not entries:
            raise ValueError("corpus must have a nonempty files/rows array")
        seen = set()
        for row in entries:
            relative = Path(row["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("corpus path escapes repository: " + str(relative))
            if relative.as_posix() in seen:
                raise ValueError("duplicate corpus path: " + str(relative))
            seen.add(relative.as_posix())
            path = ROOT / relative
            actual = sha256(path)
            if row.get("sha256") and actual != row["sha256"]:
                raise ValueError("frozen corpus bytes changed: " + str(relative))
            yield path, row
        return
    # Prune build trees before descending: rglob over all target directories
    # spent substantial time walking artifacts that would later be discarded.
    for directory, dirs, files in os.walk(ROOT):
        dirs[:] = sorted(d for d in dirs if d not in {"target", ".git"} and
                         not (Path(directory) == ROOT and d == "selfhost"))
        for name in sorted(files):
            if name.endswith(".yv"):
                yield Path(directory)/name, {}


def analyze(analyzer, path, timeout, profile="batch53"):
    try:
        done = subprocess.run([str(analyzer), "subset-features", path.name, "--profile", profile], cwd=path.parent,
                              capture_output=True, timeout=timeout, text=True, encoding="utf-8", errors="replace")
    except subprocess.TimeoutExpired:
        return {"ast_complete": False, "semantic_complete": False, "blockers": {},
                "unknown": ["analyzer_timeout"]}
    try:
        result = json.loads(done.stdout)
    except (json.JSONDecodeError, TypeError):
        return {"ast_complete": False, "semantic_complete": False, "blockers": {},
                "unknown": ["analyzer_unavailable_or_invalid_json"], "stderr": done.stderr[:1000]}
    if result.get("schema") != "yavo-subset-features/1" or done.returncode != 0:
        return {"ast_complete": False, "semantic_complete": False, "blockers": {},
                "unknown": ["analyzer_failed"], "detail": result}
    return result


def counts(rows):
    return {"paths": len(rows), "unique_entry_bytes": len({r["sha256"] for r in rows}),
            "unique_module_graphs": len({r.get("project_sha256", r["sha256"]) for r in rows})}


def candidates(rows, features):
    # Unknowns deliberately do not disappear here. The capability model can
    # miss restrictions or conservatively flag supported shapes. These are
    # planning candidates, not a proof of either exclusion or acceptance.
    return [r for r in rows if r["blockers"] and set(r["blockers"]) <= features]


def rank(rows):
    features = sorted(set().union(*(set(r["blockers"]) for r in rows))) if rows else []
    plans = {name: set(value) for name, value in PLANS.items()}
    plans.update({"single:" + f: {f} for f in features})
    for row in rows:
        bs = frozenset(row["blockers"])
        if 1 < len(bs) <= 6:
            plans.setdefault("bundle:" + "+".join(sorted(bs)), set(bs))
    result = []
    for name, fs in plans.items():
        eligible = candidates(rows, fs)
        result.append({"name": name, "features": sorted(fs),
                       "confirmed_full_unlock": {"paths": 0, "unique_entry_bytes": 0, "unique_module_graphs": 0},
                       "modeled_candidates": counts(eligible),
                       "unknown_paths": sum(bool(r["unknown"]) for r in eligible),
                       "candidate_paths": [r["path"] for r in eligible]})
    return sorted(result, key=lambda r: (-r["modeled_candidates"]["unique_module_graphs"],
                                         -r["modeled_candidates"]["paths"],
                                         len(r["features"]), r["name"]))


def first_reason_feature(reason):
    exact = {
        "tuple vector elements are outside the selfhost subset": None,  # Copy requires type information.
        "Map values other than int, bool and byte are outside the selfhost subset": "map_values",
        "Map keys other than String are outside the selfhost subset": "map_keys",
        "floating-point numbers are not in the bootstrap subset": "float",
        "numeric conversions are outside the selfhost subset": "numeric_conversions",
        "numeric type constants are outside the selfhost subset": "numeric_constants",
        "tuple destructuring is outside the selfhost subset": "tuple_destructure",
        "enum payloads are outside the selfhost subset": "enum_payload",
        "user enum payload construction is outside this subset": "enum_payload",
        "generic types and interfaces are outside the selfhost subset": "generics",
        "generics are outside the selfhost SH-1 subset": "generics",
        "generic calls are outside the selfhost subset": "generic_expressions",
        "generic expressions are outside the selfhost subset": "generic_expressions",
        "methods are outside the selfhost subset": "methods",
        "this standard module is outside the selfhost subset": "std_modules",
        "vector slices are outside the selfhost subset": "sequence_slices",
        "expression catch handlers are outside the selfhost subset": "catch_expression",
        "assert messages are outside the selfhost subset": "assert_message",
        "zero-argument output is outside the selfhost subset": "empty_output",
        "Set iteration is outside the selfhost subset": "set_iteration",
    }
    return exact.get(reason)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selfhost", type=Path, required=True)
    parser.add_argument("--stage0", type=Path, default=ROOT / ("compiler/target/release/yavo" + EXE))
    parser.add_argument("--analyzer", type=Path, help="stage0 with the read-only subset-features command (default: --stage0)")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--corpus", "--baseline-corpus", type=Path, help="frozen source inventory")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--no-analysis", action="store_true", help="strict subset check only")
    parser.add_argument("--profile", choices=("batch51", "batch52", "batch53"), default="batch53",
                        help="selfhost capability baseline (batch53 includes unified patterns and owned String/named Optional)")
    parser.add_argument("--enabled-feature", action="append", default=[],
                        help="capability independently verified in this selfhost since the batch51 profile")
    args = parser.parse_args()
    stage0, selfhost = args.stage0.resolve(), args.selfhost.resolve()
    analyzer = (args.analyzer or args.stage0).resolve()
    rows, violations, reasons, timeouts, abnormal = [], [], Counter(), [], []
    try:
        entries = list(inventory(args.corpus))
    except (OSError, ValueError, KeyError) as error:
        print("ERROR " + str(error), file=sys.stderr)
        return 2
    compiler_identities = {"stage0": sha256(stage0), "selfhost": sha256(selfhost)}
    if not args.no_analysis:
        compiler_identities["analyzer"] = sha256(analyzer)
    for path, previous in sorted(entries, key=lambda x: str(x[0])):
        relative = path.relative_to(ROOT).as_posix()
        row = {"path": relative, "sha256": sha256(path)}
        row["stage0"] = check(stage0, path, args.timeout)
        row["selfhost"] = check(selfhost, path, args.timeout)
        ref, result = row["stage0"], row["selfhost"]
        for which, observed in (("stage0", ref), ("selfhost", result)):
            if observed["exit"] is None:
                timeouts.append({"path": relative, "compiler": which})
            elif observed["exit"] < 0 or observed["exit"] not in (0,1):
                abnormal.append({"path":relative, "compiler":which, "exit":observed["exit"]})
        expected = previous.get("stage0_accepted")
        if expected is not None and (ref["exit"] == 0) != expected:
            violations.append(relative + ": frozen stage0 classification changed")
        if ref["exit"] == 0 and result["exit"] != 0:
            if result["code"] != "U0001":
                violations.append(relative + ": stage0 accepts, selfhost: " + result["first"])
            else:
                reasons[result["first"].split("U0001: ",1)[-1]] += 1
        elif ref["exit"] not in (None,0) and result["exit"] == 0:
            violations.append(relative + ": stage0 rejects, selfhost accepts")
        row.update({"blockers": {}, "unknown": [], "analysis_complete": False})
        if ref["exit"] == 0 and not args.no_analysis:
            profile = analyze(analyzer,path,args.timeout,args.profile)
            row["project_sha256"] = profile.get("project_sha256")
            row["sources"] = profile.get("sources",[])
            row["profile"] = profile.get("profile")
            row["analysis_complete"] = profile.get("ast_complete",False)
            row["semantic_complete"] = profile.get("semantic_complete",False)
            row["blockers"] = {k:v for k,v in profile.get("blockers",{}).items()
                               if k not in args.enabled_feature}
            row["unknown"] = profile.get("unknown",[])
            if not row["analysis_complete"]:
                violations.append(relative + ": full AST/HIR analyzer failed")
            if result["exit"] != 0:
                reason=result["first"].split("U0001: ",1)[-1]
                feature=first_reason_feature(reason)
                if feature and feature not in args.enabled_feature:
                    row["blockers"].setdefault(feature,[]).append({"source":"selfhost_first_diagnostic","detail":reason})
                if not row["blockers"]:
                    row["unknown"].append("unmodeled_first_diagnostic: " + reason)
            else:
                # Preserve raw model flags even when the real compiler accepts.
                # They are calibration failures, not actual missing features.
                row["profile_false_positives"] = sorted(row["blockers"])
                row["unknown"] = []
                row["semantic_complete"] = True
        elif ref["exit"] == 0 and result["exit"] != 0:
            row["unknown"] = ["analysis_disabled"]
        rows.append(row)
    source_changes = [r["path"] for r in rows if sha256(ROOT/r["path"]) != r["sha256"]]
    if source_changes:
        violations += [p + ": source changed during scan" for p in source_changes]
    accepted = [r for r in rows if r["stage0"]["exit"] == 0]
    covered = [r for r in accepted if r["selfhost"]["exit"] == 0]
    blocked = [r for r in accepted if r["selfhost"]["code"] == "U0001"]
    rankings = rank(blocked)
    report = {"schema":"yavo-subset-sweep/2", "passed":not (violations or timeouts or abnormal),
              "corpus":str(args.corpus) if args.corpus else None,
              "profile":args.profile,
              "compiler_sha256":compiler_identities, "enabled_features":args.enabled_feature,
              "summary":{"sources":len(rows), "stage0_accepted":counts(accepted),
                         "selfhost_accepted":counts(covered), "stage0_rejected":len(rows)-len(accepted),
                         "blocked":counts(blocked),
                         "analysis_complete":sum(r["analysis_complete"] for r in accepted),
                         "unresolved_semantics":sum(bool(r["unknown"]) for r in blocked),
                         "accepted_profile_false_positive_paths":sum(bool(r.get("profile_false_positives")) for r in covered)},
              "rankings":rankings, "rows":rows, "violations":violations,
              "timeouts":timeouts, "abnormal_exits":abnormal, "first_reasons":reasons,
              "ranking_contract":"All loaded AST declarations/bodies and instantiated HIR are visited; capability detection remains incomplete. modeled_candidates matches known feature sets to plans and is an estimate, not a rigorous bound. confirmed_full_unlock remains zero until an implementation is checked on original bytes. Raw model flags on accepted programs are preserved as profile_false_positives."}
    if args.json:
        args.json.parent.mkdir(parents=True,exist_ok=True)
        args.json.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(f"stage0 accepts {len(accepted)}; selfhost accepts {len(covered)} of them; stage0 rejects {len(rows)-len(accepted)}")
    if not args.no_analysis:
        for result in rankings[:10]:
            c=result["modeled_candidates"]
            print(f"{c['paths']:5d} paths/{c['unique_module_graphs']:3d} graphs modeled candidates  {result['name']}")
        copy=next(r for r in rankings if r["name"]=="copy_tuple_vec")
        print("Copy tuple Vec modeled full-unlock candidates: " + json.dumps(copy["modeled_candidates"]))
        print(f"Unresolved flow-sensitive semantics: {sum(bool(r['unknown']) for r in blocked)} entries")
    for line in violations:
        print("VIOLATION " + line)
    if timeouts or abnormal:
        print(f"FAIL: {len(timeouts)} timeouts; {len(abnormal)} abnormal exits")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
