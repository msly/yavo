#!/usr/bin/env python3
"""Run a small native C17 capability probe, not a Yavo compiler test."""
from __future__ import annotations
import argparse, hashlib, json, platform, shutil, subprocess, tempfile
from pathlib import Path

SOURCE=r'''#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <limits.h>
#include <float.h>
#include <stdatomic.h>
#include <string.h>
#include <stdio.h>
_Static_assert(CHAR_BIT == 8, "8-bit bytes required");
_Static_assert(sizeof(int64_t) == 8, "64-bit int required");
_Static_assert(sizeof(void*) == 8, "v1 desktop target is 64-bit");
_Static_assert(FLT_RADIX == 2 && FLT_MANT_DIG == 24 && DBL_MANT_DIG == 53, "binary32/binary64 required");
static int64_t shr(int64_t x, unsigned n) {
    uint64_t bits;
    memcpy(&bits, &x, sizeof bits);
    if (n != 0) {
        uint64_t sign = bits >> 63;
        bits >>= n;
        if (sign) bits |= UINT64_MAX << (64u - n);
    }
    int64_t out;
    memcpy(&out, &bits, sizeof out);
    return out;
}
int main(void) {
    int64_t out;
    if (!__builtin_add_overflow(INT64_MAX, INT64_C(1), &out)) return 1;
    if (out != INT64_MIN) return 2;
    if (!__builtin_sub_overflow(INT64_C(0), INT64_MIN, &out)) return 3;
    if (!__builtin_mul_overflow(INT64_MAX, INT64_C(2), &out)) return 4;
    if (__builtin_add_overflow(INT64_C(7), INT64_C(9), &out) || out != 16) return 5;
    if (shr(-8,1) != -4 || shr(INT64_MIN,63) != -1) return 6;
    _Atomic unsigned count = 1;
    if (atomic_fetch_add_explicit(&count, 2, memory_order_relaxed) != 1) return 7;
    if (atomic_load_explicit(&count, memory_order_relaxed) != 3) return 8;
    volatile double zero = 0.0;
    double nan = zero / zero;
    if (nan == nan) return 9;
    puts("probe-ok");
    return 0;
}
'''
FLAGS=['-std=c17','-fwrapv','-fno-strict-aliasing','-fno-fast-math','-ffp-contract=off','-Wall','-Wextra','-Werror=implicit-function-declaration']

def main()->int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--compiler',action='append',dest='compilers')
    ap.add_argument('--output',type=Path)
    args=ap.parse_args(); records=[]
    with tempfile.TemporaryDirectory(prefix='yavo-c-probe-') as tmp:
        work=Path(tmp); src=work/'probe.c'; src.write_text(SOURCE)
        for name in args.compilers or ['clang','gcc']:
            exe=shutil.which(name)
            if not exe:
                records.append({'compiler':name,'status':'not_run','reason':'compiler not found'}); continue
            version=subprocess.run([exe,'--version'],capture_output=True,text=True,timeout=10).stdout.splitlines()[0]
            for opt in ['-O0','-O2']:
                binary=work/(name.replace('/','_')+opt.replace('-','')+'.exe')
                record={'compiler':exe,'version':version,'optimization':opt,'flags':FLAGS,'ran_on':platform.platform()}
                try:
                    cmd=[exe,*FLAGS,opt,str(src),'-o',str(binary)]
                    built=subprocess.run(cmd,capture_output=True,text=True,timeout=25)
                    record['build_exit']=built.returncode
                    if built.returncode:
                        record.update(status='failed',stderr=built.stderr); records.append(record); continue
                    record['artifact_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
                    run=subprocess.run([str(binary)],capture_output=True,text=True,timeout=10)
                    record.update(run_exit=run.returncode,stdout=run.stdout,stderr=run.stderr,
                                  status='passed' if run.returncode==0 and run.stdout=='probe-ok\n' else 'failed')
                except subprocess.TimeoutExpired:
                    record.update(status='timeout')
                records.append(record)
    result={'language':'C17','purpose':'native capability probe only',
            'source_sha256':hashlib.sha256(SOURCE.encode()).hexdigest(),'results':records,
            'minimum_versions_tested':False,'cross_platform_tested':False,'yavo_execution':'not_run'}
    data=json.dumps(result,ensure_ascii=False,indent=2)+'\n'
    if args.output: args.output.write_text(data)
    print(data,end='')
    return 0 if records and all(r['status']=='passed' for r in records) else 1
if __name__=='__main__': raise SystemExit(main())
