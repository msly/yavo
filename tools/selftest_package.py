#!/usr/bin/env python3
"""Mutation tests for metadata validation; never runs Yavo."""
from __future__ import annotations
import json, shutil, tempfile
from pathlib import Path
from check_package import validate

ROOT=Path(__file__).resolve().parents[1]

def mutate(root:Path, mode:str)->None:
    manifest=root/'MANIFEST.json'
    rows=json.loads(manifest.read_text())
    if mode=='duplicate-id':
        rows[1]['id']=rows[0]['id']; manifest.write_text(json.dumps(rows))
    elif mode=='stale-line':
        next(r for r in rows if r.get('diagnostics'))['diagnostics'][0]['line']+=1
        manifest.write_text(json.dumps(rows))
    elif mode=='missing-fixed': (root/'fix/T-V1.fixed.yv').unlink()
    elif mode=='corrupt-fixture': (root/'run/T-S2.input.bin').write_bytes(b'abc')
    elif mode=='unknown-diagnostic':
        p=root/'check/T-A1.yv'; p.write_text(p.read_text().replace('E0101','E9999'))
    elif mode=='incompatible-sanitizers':
        p=root/'concurrency/T-P3.yv'; p.write_text(p.read_text().replace('address+undefined; thread','address+thread'))
    elif mode=='orphan-source': (root/'check/orphan.yv').write_text('fn main() {}\n')
    elif mode=='invented-pass':
        rows[0]['execution_status']='passed'; manifest.write_text(json.dumps(rows))
    else: raise ValueError(mode)

def main():
    records=[]
    for mode in ['duplicate-id','stale-line','missing-fixed','corrupt-fixture','unknown-diagnostic','incompatible-sanitizers','orphan-source','invented-pass']:
        with tempfile.TemporaryDirectory(prefix='yavo-metadata-test-') as tmp:
            r=Path(tmp)/'tests'; shutil.copytree(ROOT/'yavo-tests',r)
            mutate(r,mode); result=validate(r)
            records.append({'mutation':mode,'rejected':result['status']=='failed'})
    output={'language':'Python','purpose':'metadata validator mutation tests','cases':records,
            'passed':sum(x['rejected'] for x in records),'total':len(records),'yavo_execution':'not_run'}
    (ROOT/'VALIDATOR-SELFTEST.json').write_text(json.dumps(output,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(output,ensure_ascii=False,indent=2))
    return 0 if all(x['rejected'] for x in records) else 1
if __name__=='__main__': raise SystemExit(main())
