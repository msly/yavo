"""Test metadata only. This module does not parse or execute Yavo."""
from __future__ import annotations
from pathlib import Path
from typing import Any
import hashlib, json, re
DIAG = re.compile(r'//~\s+(ERROR|WARN)\s+([EW]\d{4})')
HEADER = re.compile(r'^//\s+([a-z][a-z0-9-]*):\s*(.*)$')

def read_case(path: Path, root: Path) -> dict[str, Any] | None:
    text=path.read_text(encoding='utf-8')
    if path.suffix == '.json':
        data=json.loads(text)
        if 'id' not in data: return None
        out={k:data[k] for k in ['id','kind','phase','rule','summary']}
        out['setup']=data['setup']; out['expected_report']=data['expect']
    else:
        meta:dict[str,Any]={}; stdout:list[str]=[]; seen_stdout=False
        for line in text.splitlines():
            if line.startswith('// | '):
                stdout.append(line[5:]); continue
            m=HEADER.match(line)
            if m:
                meta[m[1]]=m[2]
                if m[1]=='expect-stdout': seen_stdout=True
                continue
            if line.strip() and not line.startswith('//'): break
        if 'id' not in meta: return None
        out={k:meta[k] for k in ['id','kind','phase','rule','summary']}
        out['diagnostics']=[{'severity':m[1],'code':m[2],'line':i}
            for i,line in enumerate(text.splitlines(),1) for m in DIAG.finditer(line)]
        out['expect']=[x['code'] for x in out['diagnostics']] or None
        for key,val in meta.items():
            if key in out or key=='expect-stdout': continue
            out[key.replace('-','_')]=val
        if seen_stdout:
            out['expected_stdout']='\n'.join(stdout)+('\n' if stdout else '')
        if 'expect_panic' in out: out['expect']=out['expect_panic']
        if 'expect_candidates' in out: out['expect_candidates']=[x.strip() for x in out['expect_candidates'].split(',')]
        if 'sanitize' in out:
            out['sanitizer_profiles']=[x.strip() for x in out.pop('sanitize').split(';')]
        if 'modes' in out: out['modes']=[x.strip() for x in out['modes'].split(',')]
        if 'platforms' in out: out['platforms']=[x.strip() for x in out['platforms'].split(',')]
        if 'args_hex' in out: out['args_hex']=[x.strip() for x in out['args_hex'].split(',')]
        if 'allow_outcomes' in out: out['allowed_outcomes']=[x.strip() for x in out.pop('allow_outcomes').split(';')]
        if 'fixture' in out:
            out['fixture_sha256']=hashlib.sha256((path.parent/out['fixture']).read_bytes()).hexdigest()
    out['category']=path.relative_to(root).parts[0]
    out['file']=path.relative_to(root).as_posix()
    out['source_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
    out['execution_status']='not_run'
    return out

def scan(root:Path)->list[dict[str,Any]]:
    cases=[]
    for cat in ['check','fix','run','concurrency','report']:
        for p in sorted((root/cat).glob('*')):
            if p.suffix not in {'.yv','.json'}: continue
            item=read_case(p,root)
            if item is not None: cases.append(item)
    return cases
