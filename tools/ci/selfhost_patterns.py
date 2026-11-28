#!/usr/bin/env python3
"""Focused handwritten pattern oracle, candidate checks and drop-timing observer.
Normal products use unchanged runtime + YV_ALLOC_CHECK. An additional observational
product checks live blocks at three arm output markers, and requires all markers.
"""
import argparse, datetime, json, os, re, sys, tempfile
from pathlib import Path
sys.dont_write_bytecode = True
import csv_support as execution
ROOT=Path(__file__).resolve().parents[2]
FIXTURES=ROOT/'selfhost/tests/patterns53'
FLAGS={'debug':['-std=c17','-fno-strict-aliasing','-DYV_ALLOC_CHECK','-O0','-g'],
       'release':['-std=c17','-fno-strict-aliasing','-DYV_ALLOC_CHECK','-O2','-s']}
CODE=re.compile(rb'\b([EU][0-9]{4})\b')
def completed(r,code=0):
    return r['exit_code']==code and not r['timed_out'] and not r['launch_error']
def save(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    temp=path.with_suffix('.tmp');temp.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8');temp.replace(path)
def observed_runtime(directory,markers):
    src=(ROOT/'compiler/runtime/yavo_rt.h').read_text(encoding='utf-8')
    anchor='static int yv_rt_exit(void) {'
    if src.count(anchor)!=1:raise ValueError('runtime exit observer anchor changed')
    src=src.replace(anchor,'static int yv_patterns53_events = 0;\n'+anchor+'\n'+
        f'    if (yv_patterns53_events != {len(markers)}) yv_exit_process(103);')
    anchor='static void yv_out_bytes(const yv_out *o, const char *p, size_t n) {'
    if src.count(anchor)!=1:raise ValueError('runtime output observer anchor changed')
    checks=[]
    for text,n in markers.items():
        if not re.fullmatch('[a-z0-9]+',text):raise ValueError('unsafe observer marker')
        checks.append(f'    if (n == {len(text)} && memcmp(p, "{text}", {len(text)}) == 0) {{\n'
            f'        if (yv_live_blocks != {n}) yv_exit_process(103);\n'
            '        ++yv_patterns53_events;\n    }')
    src=src.replace(anchor,anchor+'\n'+'\n'.join(checks))
    directory.mkdir(parents=True,exist_ok=True)
    p=directory/'yavo_rt.h';p.write_text(src,encoding='utf-8');return p

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    entry=parser.add_mutually_exclusive_group(required=True)
    entry.add_argument('--compiler',type=Path)
    entry.add_argument('--bootstrap-output',type=Path,help='read passing bootstrap receipt and use its actual stage2 and locked Zig')
    parser.add_argument('--role',choices=('stage0','selfhost'),default='selfhost')
    parser.add_argument('--zig',type=Path)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--checks-only',action='store_true')
    parser.add_argument('--mode',choices=('debug','release'),action='append')
    parser.add_argument('--case',action='append')
    parser.add_argument('--originals',action='store_true',help='execute unchanged original corpus sources with their own I/O fixtures')
    args=parser.parse_args()
    bootstrap=None
    if args.bootstrap_output:
        if args.role!='selfhost':parser.error('--bootstrap-output requires the selfhost role')
        bootstrap=args.bootstrap_output.resolve()/'receipt.json'
        receipt=json.loads(bootstrap.read_bytes())
        if receipt['status']!='passed':raise ValueError('pattern verification requires a passing fixed point')
        args.compiler=Path(receipt['work'])/'stages/stage2'/('yavo-selfhost.exe' if os.name=='nt' else 'yavo-selfhost')
        if args.zig is None:args.zig=Path(receipt['inputs']['zig']['path'])
        if args.output is None:
            args.output=args.bootstrap_output/'patterns53'/('originals' if args.originals else 'focused' if args.case else 'all')
    if args.output is None:parser.error('--output is required without --bootstrap-output')
    compiler=args.compiler.resolve();output=args.output.resolve()
    output.mkdir(parents=True,exist_ok=True);work=Path(tempfile.mkdtemp(prefix='run-',dir=output))
    env=dict(os.environ,YAVO_NO_CACHE='1',YAVO_ALLOC_CHECK='1')
    for key in ('YAVO_CC','YAVO_CACHE_TRACE','YAVO_UPDATE_BASELINE'):env.pop(key,None)
    manifest=json.loads((FIXTURES/'cases.json').read_bytes())
    pool=manifest['originals'] if args.originals else manifest['cases']
    cases=[c for c in pool if not args.case or c['id'] in args.case]
    def source(c):return ROOT/c['repository_file'] if 'repository_file' in c else ROOT/'selfhost/tests'/c['file']
    if not cases or args.case and set(args.case)!={c['id'] for c in cases}:raise ValueError('unknown/empty cases')
    modes=args.mode or ['debug','release'];zig=args.zig.resolve() if args.zig else None
    if not args.checks_only and not zig:raise ValueError('--zig required for executable checks')
    report={'schema':'yavo-patterns53-verification/1','role':args.role,'work':str(work),
      'recorded_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
      'inputs':{'compiler':execution.identity(compiler),'manifest':execution.identity(FIXTURES/'cases.json'),
        'runtime':execution.identity(ROOT/'compiler/runtime/yavo_rt.h'),'harness':execution.identity(Path(__file__)),
        'sources':{str(source(c).relative_to(ROOT)):execution.identity(source(c)) for c in cases},
        'auxiliary':{f:execution.identity(ROOT/f) for c in cases for f in c.get('auxiliary_files',[])+[c[k] for k in ('stdin_file','stdout_file') if k in c]}},
      'checks_only':args.checks_only,'modes':[] if args.checks_only else modes,
      'expectations_from':'handwritten manifest, independently verified by stage0 without rewriting','rows':[]}
    if zig:report['inputs']['zig']=execution.identity(zig)
    if bootstrap:report['inputs']['bootstrap_receipt']=execution.identity(bootstrap)
    def run(label,cmd,timeout=30,frontend=False,cwd=None,stdin=None):
        return execution.execute(cmd,work/'commands'/label,cwd or ROOT,env,{'PATH':''} if frontend else {},timeout,stdin=stdin)
    for case in cases:
        label=case['id'];path=source(case)
        check,out,err=run(label+'-check',[compiler,'check',path],frontend=True)
        expected=0 if case['kind']=='run' or args.role=='stage0' and case['kind']=='unsupported' else 1
        code=(case.get('reference_diagnostic',case.get('diagnostic')) if args.role=='stage0' else case.get('diagnostic')) if expected else None
        okay=completed(check,expected) and (code is None or code.encode() in CODE.findall(err))
        row={'id':label,'kind':case['kind'],'check':check,'expected_exit':expected,'diagnostic':code,'passed':okay,'products':[]}
        if okay and case['kind']=='run' and not args.checks_only:
            cfile=work/(label+'.c')
            cmd=[compiler,'check',path,'--emit-c',cfile] if args.role=='stage0' else [compiler,'emit-c',path,'-o',cfile]
            emit,_,_=run(label+'-emit',cmd,frontend=True)
            row['emit']=emit;row['passed'] &= completed(emit) and cfile.is_file()
            if row['passed']:
                row['c']=execution.identity(cfile)
                includes=[('normal',ROOT/'compiler/runtime')]
                if 'live_blocks_at_output' in case:
                    observed=observed_runtime(work/'observed-runtime',case['live_blocks_at_output'])
                    row['observer']={'runtime':execution.identity(observed),'expected':case['live_blocks_at_output'],
                                     'all_markers_required':True,'failure_exit':103}
                    includes.append(('drop-observer',observed.parent))
                for mode in modes:
                    for flavor,include in includes:
                        itemlabel=label+'-'+mode+'-'+flavor
                        product=work/(itemlabel+('.exe' if os.name=='nt' else ''))
                        build,_,_=run(itemlabel+'-build',[zig,'cc',*FLAGS[mode],'-I',include,cfile,'-o',product],180)
                        item={'mode':mode,'flavor':flavor,'build':build,'passed':False}
                        if completed(build):
                            execute,stdout,stderr=run(itemlabel+'-run',[product],frontend=True,cwd=path.parent if args.originals else None,
                                stdin=(ROOT/case['stdin_file']).read_bytes() if 'stdin_file' in case else None)
                            item.update(run=execute,binary=execution.identity(product),
                                passed=completed(execute) and stdout==((ROOT/case['stdout_file']).read_bytes() if 'stdout_file' in case else case['stdout'].encode()) and stderr==case.get('stderr','').encode())
                        row['products'].append(item);row['passed'] &= item['passed']
        report['rows'].append(row);save(output/'receipt.json',report)
        print(label+': '+('PASS' if row['passed'] else 'FAIL'),flush=True)
    report['sources_unchanged']=all(execution.digest(ROOT/name)==ident['sha256']
        for name,ident in (report['inputs']['sources']|report['inputs']['auxiliary']).items())
    report['summary']={'cases':len(cases),'passed':sum(r['passed'] for r in report['rows']),
        'products':sum(len(r['products']) for r in report['rows']),'failed':sum(not r['passed'] for r in report['rows'])}
    report['passed']=report['sources_unchanged'] and report['summary']['failed']==0
    save(output/'receipt.json',report);print(json.dumps(report['summary']),flush=True)
    return 0 if report['passed'] else 1
if __name__=='__main__':sys.exit(main())
