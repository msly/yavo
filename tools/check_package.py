#!/usr/bin/env python3
"""Validate fixture bookkeeping. Not a Yavo syntax/type checker or runtime."""
from __future__ import annotations
import argparse, collections, hashlib, json, re, sys
from pathlib import Path
from package_common import scan

KINDS={'check-pass','check-fail','fix-auto','fix-candidate','run-pass','run-panic','run-exit','run-allowed','report'}

def validate(root:Path)->dict:
    errors=[]
    try:
        manifest=json.loads((root/'MANIFEST.json').read_text())
        actual=scan(root)
        known=set(re.findall(r'\| ([EW]\d{4}) \|',(root/'ERRORS.md').read_text()))
        panics=set(re.findall(r'\| (P-[A-Z0-9-]+) \|',(root/'ERRORS.md').read_text()))
    except Exception as e:
        return {'status':'failed','errors':[f'metadata load: {e}'],'yavo_execution':'not_run'}
    if actual!=manifest: errors.append('MANIFEST.json differs from fixture contents/headers/hashes')
    ids=[x['id'] for x in manifest]
    if len(ids)!=len(set(ids)): errors.append('duplicate test id')
    rootsafe=root.resolve()
    used={}
    for x in manifest:
        ident=x['id']; used[x['file']]=ident
        if x['kind'] not in KINDS: errors.append(f'{ident}: unknown kind')
        if x['phase'] not in {'P1','P2','P3'}: errors.append(f'{ident}: unknown phase')
        p=(root/x['file']).resolve()
        if not p.is_relative_to(rootsafe): errors.append(f'{ident}: path outside root'); continue
        if not p.is_file(): errors.append(f'{ident}: missing source'); continue
        ds=x.get('diagnostics',[])
        for d in ds:
            if d['code'] not in known: errors.append(f'{ident}: undefined diagnostic {d["code"]}')
            if (d['severity']=='ERROR') != d['code'].startswith('E'): errors.append(f'{ident}: severity/code mismatch')
        if x['kind']=='check-fail' and not any(d['severity']=='ERROR' for d in ds): errors.append(f'{ident}: negative test without expected ERROR')
        if x['kind']=='check-pass' and any(d['severity']=='ERROR' for d in ds): errors.append(f'{ident}: pass test expects ERROR')
        if x['kind']=='fix-auto':
            fixed=(p.parent/x.get('expect_fixed','')).resolve()
            if not fixed.is_relative_to(rootsafe) or not fixed.is_file(): errors.append(f'{ident}: missing fixed fixture')
            else: used[fixed.relative_to(root.resolve()).as_posix()]=ident
        if x['kind']=='fix-candidate' and not x.get('expect_candidates'): errors.append(f'{ident}: missing candidates')
        if x['kind'] in {'run-pass','run-exit'} and 'expected_stdout' not in x: errors.append(f'{ident}: missing expected stdout')
        if x['kind']=='run-panic' and x.get('expect_panic') not in panics: errors.append(f'{ident}: missing/undefined panic')
        if x['kind']=='run-allowed':
            if not x.get('allowed_outcomes'): errors.append(f'{ident}: no allowed outcome')
            for o in x.get('allowed_outcomes',[]):
                if o.startswith('panic:') and o[6:] not in panics: errors.append(f'{ident}: undefined allowed panic')
        for profile in x.get('sanitizer_profiles',[]):
            parts=set(profile.split('+'))
            if {'address','thread'}<=parts: errors.append(f'{ident}: incompatible Sanitizers combined')
        if x.get('fixture'):
            f=(p.parent/x['fixture']).resolve()
            if not f.is_relative_to(rootsafe) or not f.is_file(): errors.append(f'{ident}: bad fixture')
            elif x.get('fixture_hex') and f.read_bytes().hex()!=x['fixture_hex']: errors.append(f'{ident}: fixture bytes mismatch')
        if x['kind']=='report':
            f=(p.parent/x['setup'].get('program','')).resolve()
            if not f.is_relative_to(rootsafe) or not f.is_file(): errors.append(f'{ident}: missing scenario program')
            elif f.suffix=='.yv': used[f.relative_to(root.resolve()).as_posix()]=ident
            if not x.get('expected_report'): errors.append(f'{ident}: empty report assertion')
        if x.get('execution_status')!='not_run': errors.append(f'{ident}: fixture contains invented execution evidence')
    for p in root.rglob('*.yv'):
        if p.relative_to(root).as_posix() not in used: errors.append(f'orphan source: {p.relative_to(root)}')
    return {'status':'passed' if not errors else 'failed','checks':'metadata, file existence, SHA256, exact diagnostic lines, expected outputs, fixtures, fixed sources, scenario paths, sanitizer profiles, orphan files',
            'test_count':len(manifest),'by_category':dict(collections.Counter(x['category'] for x in manifest)),
            'by_phase':dict(collections.Counter(x['phase'] for x in manifest)),
            'errors':errors,'yavo_parse':'not_run','yavo_typecheck':'not_run','yavo_runtime':'not_run'}

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('root',nargs='?',type=Path,default=Path(__file__).resolve().parents[1]/'yavo-tests')
    ap.add_argument('--output',type=Path)
    args=ap.parse_args(); result=validate(args.root.resolve())
    s=json.dumps(result,ensure_ascii=False,indent=2)+'\n'
    if args.output: args.output.write_text(s)
    print(s,end=''); return 0 if result['status']=='passed' else 1
if __name__=='__main__': raise SystemExit(main())
