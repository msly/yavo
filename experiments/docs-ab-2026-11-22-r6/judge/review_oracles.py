#!/usr/bin/env python3
"""Independent R6 expected-output review; does not import the task generator."""
from pathlib import Path
from collections import deque
from functools import lru_cache
from urllib.parse import quote,unquote_to_bytes
import argparse,hashlib,json,re

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--packet-root',type=Path,required=True)
parser.add_argument('--output',type=Path)
options=parser.parse_args()
packet=options.packet_root.resolve()

def rows(s):
    assert not s or s.endswith('\n')
    return s.split('\n')[:-1] if s else []

def circle(s):
    data=rows(s); n,k=map(int,data[0].split()); queue=deque(x.strip() for x in data[1:]); out=[]
    assert len(queue)==n
    while len(queue)>1:
        queue.rotate(-((k-1)%len(queue)))
        out.append('out: '+queue.popleft())
    return '\n'.join(out+['winner: '+(queue[0] if queue else 'none')])+'\n'

def codec(s):
    out=[]
    for i,line in enumerate(rows(s),1):
        value=None
        if line[:4]=='enc ':
            value=quote(line[4:],safe='-._~',encoding='ascii')
        elif line[:4]=='dec ' and not re.search(r'%(?![0-9A-Fa-f]{2})',line[4:]):
            b=unquote_to_bytes(line[4:])
            if all(32<=x<=126 for x in b):value=b.decode('ascii')
        out.append(f'{i}: invalid' if value is None else f'{i}: len={len(value)} text={value}')
    return ''.join(x+'\n' for x in out)

def polys(s):
    f=list(map(int,s.split())); n,m=f[:2]; a=[0]*31;b=[0]*31
    for i in range(n+m):
        c,e=f[2+2*i:4+2*i]
        (a if i<n else b)[e]+=c
    added=[x+y for x,y in zip(a,b)]
    product=[0]*61
    for i,x in enumerate(a):
        for j,y in enumerate(b):product[i+j]+=x*y
    derived=[(i+1)*a[i+1] for i in range(30)]
    def show(v):return ' '.join(f'{i}:{v[i]}' for i in range(len(v)-1,-1,-1) if v[i]) or '0'
    return ''.join(name+': '+show(v)+'\n' for name,v in [('sum',added),('product',product),('derivative',derived)])

def month_days(y):return [31,28+int(y%4==0 and (y%100!=0 or y%400==0)),31,30,31,30,31,31,30,31,30,31]
def jan1(y):
    p=y-1
    return 365*p+p//4-p//100+p//400

def ordinal(s):
    if not re.fullmatch(r'[0-9]{4}-[0-9]{2}-[0-9]{2}',s):raise ValueError()
    y,m,d=map(int,s.split('-'))
    if not 1600<=y<=2400 or not 1<=m<=12 or not 1<=d<=month_days(y)[m-1]:raise ValueError()
    return jan1(y)+sum(month_days(y)[:m-1])+d-1

def render_date(n):
    if not jan1(1600)<=n<jan1(2401):raise ValueError()
    lo,hi=1600,2401
    while lo+1<hi:
        mid=(lo+hi)//2
        if jan1(mid)<=n:lo=mid
        else:hi=mid
    d=n-jan1(lo);m=1
    for span in month_days(lo):
        if d<span:return f'{lo:04d}-{m:02d}-{d+1:02d}'
        d-=span;m+=1
    raise AssertionError()

def dates(s):
    out=[]
    for i,line in enumerate(rows(s),1):
        try:
            op,a,b=line.split()
            start=ordinal(a)
            if op=='diff':value=str(ordinal(b)-start)
            elif op=='add':
                if len(b)>16 or not re.fullmatch(r'-?[0-9]+',b) or abs(int(b))>10**6:raise ValueError()
                value=render_date(start+int(b))
            else:raise ValueError()
        except ValueError:value='invalid'
        out.append(f'{i}: {value}\n')
    return ''.join(out)

def coins(s):
    f=list(map(int,s.split()));target,n=f[:2];kinds=tuple(zip(f[2::2],f[3::2]));assert len(kinds)==n
    @lru_cache(None)
    def choose(i,left):
        if i==n:return (1,0) if left==0 else (0,None)
        v,count=kinds[i];ways=0;best=None
        for k in range(min(count,left//v)+1):
            w,m=choose(i+1,left-k*v);ways+=w
            if m is not None:best=min(best,m+k) if best is not None else m+k
        return ways,best
    w,m=choose(0,target)
    return f"ways: {w}\nmin: {m if m is not None else 'none'}\n"

sieve=bytearray([1])*31624
sieve[:2]=b'\x00\x00'
for p in range(2,178):
    if sieve[p]:sieve[p*p::p]=b'\x00'*len(sieve[p*p::p])
primes=[i for i in range(2,len(sieve)) if sieve[i]]

def factor(s):
    f=list(map(int,s.split()));n=f[0];assert len(f)==n+1;out=[]
    for i,original in enumerate(f[1:],1):
        x=original;parts=[]
        for p in primes:
            if p*p>x:break
            count=0
            while x%p==0:count+=1;x//=p
            if count:parts.append(f'{p}^{count}')
        if x>1:parts.append(f'{x}^1')
        out.append(f"{i}: {original}="+('*'.join(parts) or '1'))
    return '\n'.join(out+[f'count: {n}'])+'\n'

def merge(intervals):
    out=[]
    for a,b in sorted(intervals):
        if out and a<=out[-1][1]:out[-1]=(out[-1][0],max(out[-1][1],b))
        else:out.append((a,b))
    return out

def length(intervals):return sum(b-a for a,b in intervals)
def overlap(a,b):
    i=j=total=0
    while i<len(a) and j<len(b):
        total+=max(0,min(a[i][1],b[j][1])-max(a[i][0],b[j][0]))
        if a[i][1]<b[j][1]:i+=1
        else:j+=1
    return total

def rectangles(s):
    f=list(map(int,s.split()));n=f[0];rs=[tuple(f[i:i+4]) for i in range(1,len(f),4)];assert len(rs)==n
    rs=[r for r in rs if r[0]<r[2] and r[1]<r[3]]
    xs=sorted({x for r in rs for x in (r[0],r[2])})
    previous=[];area=perimeter=0
    for i,x in enumerate(xs):
        current=merge([(b,d) for a,b,c,d in rs if a<=x<c])
        perimeter+=length(previous)+length(current)-2*overlap(previous,current)
        if i+1<len(xs):
            width=xs[i+1]-x
            area+=width*length(current)
            perimeter+=width*len(current)*2
        previous=current
    return f'area: {area}\nperimeter: {perimeter}\n'

wins=[(0,1,2),(3,4,5),(6,7,8),(0,3,6),(1,4,7),(2,5,8),(0,4,8),(2,4,6)]
def won(b,p):return any(all(b[i]==p for i in line) for line in wins)
@lru_cache(None)
def absolute_value(b):
    if won(b,'X'):return 1
    if won(b,'O'):return -1
    if '.' not in b:return 0
    player='X' if b.count('X')==b.count('O') else 'O'
    vals=[absolute_value(b[:i]+player+b[i+1:]) for i,x in enumerate(b) if x=='.']
    return max(vals) if player=='X' else min(vals)

def game(s):
    out=[]
    for num,b in enumerate(rows(s),1):
        x,o=b.count('X'),b.count('O');wx,wo=won(b,'X') if len(b)==9 else False,won(b,'O') if len(b)==9 else False
        valid=len(b)==9 and set(b)<=set('XO.') and x-o in (0,1) and not (wx and wo) and (not wx or x==o+1) and (not wo or x==o)
        if not valid:result='invalid'
        elif wx or wo:result='terminal '+('X' if wx else 'O')
        elif '.' not in b:result='terminal draw'
        else:
            p='X' if x==o else 'O';target=absolute_value(b)
            pos=next(i+1 for i,c in enumerate(b) if c=='.' and absolute_value(b[:i]+p+b[i+1:])==target)
            from_player=target if p=='X' else -target
            result=f'play {p} {pos} '+{1:'win',0:'draw',-1:'lose'}[from_player]
        out.append(f'{num}: {result}\n')
    return ''.join(out)

methods=[circle,codec,polys,dates,coins,factor,rectangles,game]
data=json.loads((packet/'judge/cases.json').read_text());checks=[]
assert [task['id'] for task in data['tasks']]==[f'T{i:02}' for i in range(1,9)]
for task,solver in zip(data['tasks'],methods):
    assert len(task['cases'])==14
    assert sum(case['visibility']=='public' for case in task['cases'])==2
    assert sum(case['visibility']=='hidden' for case in task['cases'])==12
    for case in task['cases']:
        actual=solver(case['stdin'])
        checks.append({'task':task['id'],'case':case['id'],'passed':actual==case['stdout'],'stdout_sha256':hashlib.sha256(actual.encode()).hexdigest()})
        if actual!=case['stdout']:print(task['id'],case['id'],repr(actual),repr(case['stdout']))
report={'status':'passed' if all(x['passed'] for x in checks) else 'failed','scope':'Independent algorithms, all 16 public and 96 hidden input/output pairs; no generator imported and no participant runs.',
 'methods':['deque rotation','urllib percent codec with separate strict validation','dense polynomial arrays','manual Gregorian ordinals and binary-search inverse','memoized choice enumeration','sieve prime division','x-sweep interval union and symmetric-difference boundaries','absolute X-score minimax with count/winner reachability predicate'],
 'reviewer_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases_sha256':hashlib.sha256((packet/'judge/cases.json').read_bytes()).hexdigest(),'generator_sha256':hashlib.sha256((packet/'judge/generate.py').read_bytes()).hexdigest(),'passed':sum(x['passed'] for x in checks),'total':len(checks),'checks':checks}
if options.output:
    options.output.parent.mkdir(parents=True,exist_ok=True)
    options.output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='checks'}))
assert report['status']=='passed'
