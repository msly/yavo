import argparse, collections, hashlib, json, os, pathlib, subprocess, time

parser = argparse.ArgumentParser()
parser.add_argument('--candidates', required=True)
parser.add_argument('--out', required=True)
parser.add_argument('--stage0', default='compiler/target/ci/batch32-stage0/yavo')
parser.add_argument('--profile', choices=['Debug', 'Release'], default='Debug')
parser.add_argument('--ids', default='')
args = parser.parse_args()
root = pathlib.Path.cwd()
out = root / args.out
out.mkdir(parents=True, exist_ok=False)
stage0 = (root / args.stage0).resolve()
environment = dict(os.environ, YAVO_ALLOC_CHECK='1', YAVO_NO_CACHE='1')
environment.pop('YAVO_CC', None)
def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
bound = {}
def bind(path):
    path = pathlib.Path(path).resolve()
    bound[str(path)] = digest(path)
    return bound[str(path)]
for p in [stage0, root/'compiler/runtime/yavo_rt.h', root/'compiler/toolchain.lock', pathlib.Path('/home/appops/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig'), pathlib.Path(__file__)]:
    if p.exists(): bind(p)
candidates_path = (root / args.candidates).resolve()
bind(candidates_path)
candidates = json.loads(candidates_path.read_text())
if isinstance(candidates, dict): candidates = candidates['cases']
if args.ids:
    wanted = set(args.ids.split(','))
    candidates = [c for c in candidates if c['id'] in wanted]
def call(command, timeout=120):
    before = time.monotonic()
    try:
        p = subprocess.run([str(c) for c in command], text=True, capture_output=True, env=environment, timeout=timeout)
        return dict(command=[str(c) for c in command], exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr, seconds=round(time.monotonic()-before,3), timed_out=False)
    except subprocess.TimeoutExpired as e:
        def s(x): return x.decode(errors='replace') if isinstance(x, bytes) else x or ''
        return dict(command=[str(c) for c in command], exit_code=None, stdout=s(e.stdout), stderr=s(e.stderr), seconds=round(time.monotonic()-before,3), timed_out=True)
receipt = dict(schema='yavo-batch32-stage0-oracle/1', baseline='08f19faf834f1b89ddad39fb4e77f97753cc8641', profile=args.profile, environment_overrides={'YAVO_ALLOC_CHECK':'1','YAVO_NO_CACHE':'1','YAVO_CC':None}, inputs=bound, records=[])
receipt_path=out/'receipt.json'
def save(): receipt_path.write_text(json.dumps(receipt, ensure_ascii=False, indent=2)+'\n')
for case in candidates:
    directory=out/case['id']; directory.mkdir()
    source=case.get('source')
    if source is None: source=(root/'selfhost/tests'/case['file']).read_text()
    path=directory/'source.yv'; path.write_text(source)
    item=dict(id=case['id'], kind=case['kind'], source=source, source_sha256=bind(path))
    expected_exit=1 if case['kind'] in ['reject','shared_unsupported'] else 0
    item['expected_exit']=expected_exit
    if 'diagnostic' in case: item['expected_diagnostic']=case['diagnostic'] if expected_exit else None
    result=call([stage0,'check',path,'--json']); item['check']=result
    diagnostic=None
    try:
        data=json.loads(result['stdout'])
        diagnostic=next((d for d in data.get('diagnostics',[]) if str(d.get('severity','')).upper()=='ERROR'), None)
    except (json.JSONDecodeError, TypeError): pass
    if diagnostic:
        primary=diagnostic.get('primary',{})
        item['first_error']={k:diagnostic.get(k) for k in ['code','message']}
        item['first_error'].update({k:primary.get(k) for k in ['line','column']})
    else: item['first_error']=None
    item['passed']=result['exit_code']==expected_exit and not result['stderr'] and not result['timed_out']
    if expected_exit:
        item['passed']=item['passed'] and diagnostic is not None and diagnostic['code']==case['diagnostic']
    elif case['kind']=='run' and item['passed']:
        item['expected_stdout']=case['stdout']
        item['expected_stderr']=case.get('stderr','')
        binary=directory/'program'; generated=directory/'program.c'
        command=[stage0,'build',path,'-o',binary,'--emit-c',generated,'--json']
        if args.profile=='Release': command.append('--release')
        item['build']=call(command)
        item['passed']=item['build']['exit_code']==0 and not item['build']['stderr'] and not item['build']['timed_out']
        if generated.exists(): item['generated_c_sha256']=bind(generated)
        if binary.exists(): item['binary_sha256']=bind(binary)
        if item['passed']:
            item['run']=call([binary],10)
            item['passed']=item['run']['exit_code']==0 and item['run']['stdout']==case['stdout'] and item['run']['stderr']==case.get('stderr','') and not item['run']['timed_out']
    receipt['records'].append(item)
    save()
    print(case['id'], 'PASS' if item['passed'] else 'FAIL', json.dumps(item.get('first_error'),ensure_ascii=False), json.dumps(item.get('run',{}).get('stdout'),ensure_ascii=False), flush=True)
receipt['inputs_changed']=[p for p,h in bound.items() if not pathlib.Path(p).exists() or digest(p)!=h]
receipt['passed']=sum(x['passed'] for x in receipt['records'])
receipt['total']=len(receipt['records'])
receipt['status']='passed' if receipt['passed']==receipt['total'] and not receipt['inputs_changed'] else 'failed'
save()
print(json.dumps({k:receipt[k] for k in ['status','passed','total','inputs_changed']}))
