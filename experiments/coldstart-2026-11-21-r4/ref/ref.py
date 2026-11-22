#!/usr/bin/env python3
"""Round-3 reference implementation and fixture generator (not shown to subjects).

Usage: python3 ref.py OUTDIR   -> writes tasks/ fixtures (inputs and expected outputs).
"""
import os
import random
import sys
from collections import deque

OUT = sys.argv[1] if len(sys.argv) > 1 else "tasks"
os.makedirs(OUT, exist_ok=True)


def w(name, text):
    with open(os.path.join(OUT, name), "w", newline="\n") as f:
        f.write(text)


def lines_of(text):
    ls = text.split("\n")
    if ls and ls[-1] == "":
        ls.pop()
    return ls


def is_int(s):
    if s.startswith("-"):
        s2 = s[1:]
    else:
        s2 = s
    return len(s2) > 0 and all("0" <= c <= "9" for c in s2)


def is_uint(s):
    return len(s) > 0 and all("0" <= c <= "9" for c in s)


def fmt2(x):
    # Guard against round-half ties that languages could resolve differently.
    s = "%.6f" % x
    assert not s.endswith("5000"), x
    return "%.2f" % x


# ---------------- T01 ----------------
def t01(text):
    users = {}  # name -> set
    out = []
    for line in lines_of(text):
        f = line.split()
        if not f:
            continue
        cmd, args = f[0], f[1:]
        arity = {"add": 1, "link": 2, "unlink": 2, "remove": 1, "path": 2, "friends": 1}
        if cmd not in arity or len(args) != arity[cmd]:
            out.append("error: bad command")
            continue
        if cmd == "add":
            if args[0] in users:
                out.append("error: %s exists" % args[0])
            else:
                users[args[0]] = set()
            continue
        missing = [a for a in args if a not in users]
        if missing:
            out.append("error: no user %s" % missing[0])
            continue
        if cmd == "link":
            a, b = args
            if a == b:
                out.append("error: self link")
            else:
                users[a].add(b)
                users[b].add(a)
        elif cmd == "unlink":
            a, b = args
            if a == b or b not in users[a]:
                out.append("error: not linked")
            else:
                users[a].discard(b)
                users[b].discard(a)
        elif cmd == "remove":
            n = args[0]
            for o in users[n]:
                users[o].discard(n)
            del users[n]
        elif cmd == "friends":
            n = args[0]
            fs = sorted(users[n])
            out.append("%s: %s" % (n, " ".join(fs) if fs else "(none)"))
        elif cmd == "path":
            a, b = args
            prev = {a: None}
            q = deque([a])
            while q:
                u = q.popleft()
                if u == b:
                    break
                for v in sorted(users[u]):
                    if v not in prev:
                        prev[v] = u
                        q.append(v)
            if b not in prev:
                out.append("path: none")
            else:
                p = []
                c = b
                while c is not None:
                    p.append(c)
                    c = prev[c]
                out.append("path: " + " -> ".join(reversed(p)))
    links = sum(len(s) for s in users.values()) // 2
    out.append("users: %d links: %d" % (len(users), links))
    return "\n".join(out) + "\n"


def gen_t01():
    fixed = """add ann
add bob
add cy
add dee
add eve
add ann
link ann bob
link bob cy
link cy dee
link ann eve
link eve dee
link ann ann
link ann zed
path ann dee
path dee ann
friends ann
friends zed
unlink ann cy
unlink cy cy
unlink bob ann
path ann cy
remove eve
path ann dee
friends dee
add eve
friends eve
path eve ann
link eve ann
link eve ann

path ann ann
hello
link ann
path ann bob cy
remove bob
path ann dee
"""
    r = random.Random(101)
    names = ["u%02d" % i for i in range(27)]
    cmds = []
    for n in names[:25]:
        cmds.append("add " + n)
    for _ in range(400):
        k = r.random()
        a, b = r.choice(names), r.choice(names)
        if k < 0.50:
            cmds.append("link %s %s" % (a, b))
        elif k < 0.58:
            cmds.append("unlink %s %s" % (a, b))
        elif k < 0.61:
            cmds.append("remove " + a)
        elif k < 0.65:
            cmds.append("add " + a)
        elif k < 0.90:
            cmds.append("path %s %s" % (a, b))
        else:
            cmds.append("friends " + a)
    return fixed + "\n".join(cmds) + "\n"


# ---------------- T02 ----------------
def t02(text):
    ls = lines_of(text)
    buf = ls[0] if ls else ""
    cur = len(buf)
    undo = []
    out = []
    for k, line in enumerate(ls[1:], start=2):
        ok = True
        if line == "P":
            out.append(buf[:cur] + "|" + buf[cur:])
        elif line == "U":
            if undo:
                buf, cur = undo.pop()
            else:
                out.append("nothing to undo")
        elif line.startswith("I ") and len(line) > 2:
            undo.append((buf, cur))
            t = line[2:]
            buf = buf[:cur] + t + buf[cur:]
            cur += len(t)
        elif len(line) > 2 and line[0] in "LRDB" and line[1] == " " and is_uint(line[2:]) and int(line[2:]) > 0:
            n = int(line[2:])
            c = line[0]
            if c == "L":
                cur = max(0, cur - n)
            elif c == "R":
                cur = min(len(buf), cur + n)
            elif c == "D":
                undo.append((buf, cur))
                buf = buf[:cur] + buf[cur + n:]
            else:
                undo.append((buf, cur))
                s = max(0, cur - n)
                buf = buf[:s] + buf[cur:]
                cur = s
        else:
            ok = False
        if not ok:
            out.append("bad command at line %d" % k)
    out.append("final: " + buf)
    out.append("length: %d" % len(buf))
    return "\n".join(out) + "\n"


def gen_t02():
    fixed = """hello world
P
L 5
P
I big
P
R 100
I !
P
B 3
P
U
P
U
U
P
U
U
nothing
L 0
L x
D 2

L 100
D 3
P
I >>
P
R 2
B 1
P
D 1000
P
U
U
P
L 3
I a b  c
P
"""
    r = random.Random(202)
    cmds = []
    for _ in range(300):
        k = r.random()
        if k < 0.15:
            cmds.append("L %d" % r.randint(1, 8))
        elif k < 0.30:
            cmds.append("R %d" % r.randint(1, 8))
        elif k < 0.55:
            word = "".join(r.choice("abcxyz -_") for _ in range(r.randint(1, 5)))
            if word.strip() == "":
                word = "q" + word
            cmds.append("I " + word)
        elif k < 0.65:
            cmds.append("D %d" % r.randint(1, 4))
        elif k < 0.75:
            cmds.append("B %d" % r.randint(1, 4))
        elif k < 0.87:
            cmds.append("U")
        else:
            cmds.append("P")
    return fixed + "\n".join(cmds) + "\n"


# ---------------- T03 ----------------
class PErr(Exception):
    pass


def t03_line(line):
    toks = []
    i = 0
    while i < len(line):
        c = line[i]
        if c == " ":
            i += 1
            continue
        if c.isdigit():
            j = i
            while j < len(line) and line[j].isdigit():
                j += 1
            toks.append(("num", line[i:j], i + 1))
            i = j
            continue
        toks.append(("op" if c in "+-*/%()" else "bad", c, i + 1))
        i += 1
    pos = [0]

    def peek():
        return toks[pos[0]] if pos[0] < len(toks) else None

    def fail():
        t = peek()
        if t is None:
            raise PErr("error: unexpected end")
        raise PErr("error at col %d: unexpected '%s'" % (t[2], t[1]))

    def expr():
        n = term()
        while peek() and peek()[0] == "op" and peek()[1] in "+-":
            op = peek()[1]
            pos[0] += 1
            n = ("bin", op, n, term())
        return n

    def term():
        n = unary()
        while peek() and peek()[0] == "op" and peek()[1] in "*/%":
            op = peek()[1]
            pos[0] += 1
            n = ("bin", op, n, unary())
        return n

    def unary():
        t = peek()
        if t and t[0] == "op" and t[1] == "-":
            pos[0] += 1
            return ("neg", unary())
        return primary()

    def primary():
        t = peek()
        if t is None:
            fail()
        if t[0] == "num":
            pos[0] += 1
            return ("num", int(t[1]))
        if t[0] == "op" and t[1] == "(":
            pos[0] += 1
            n = expr()
            t2 = peek()
            if t2 is None or not (t2[0] == "op" and t2[1] == ")"):
                fail()
            pos[0] += 1
            return n
        fail()

    try:
        tree = expr()
        if peek() is not None:
            fail()
    except PErr as e:
        return str(e)

    def show(n):
        if n[0] == "num":
            return str(n[1])
        if n[0] == "neg":
            return "(-%s)" % show(n[1])
        return "(%s %s %s)" % (show(n[2]), n[1], show(n[3]))

    class Z(Exception):
        pass

    def ev(n):
        if n[0] == "num":
            return n[1]
        if n[0] == "neg":
            return -ev(n[1])
        a = ev(n[2])
        b = ev(n[3])
        op = n[1]
        if op == "+":
            r = a + b
        elif op == "-":
            r = a - b
        elif op == "*":
            r = a * b
        else:
            if b == 0:
                raise Z()
            q = abs(a) // abs(b)
            if (a < 0) != (b < 0):
                q = -q
            r = q if op == "/" else a - q * b
        assert -(2**63) <= r < 2**63
        return r

    try:
        v = str(ev(tree))
    except Z:
        v = "error: division by zero"
    return "%s = %s" % (show(tree), v)


def t03(text):
    return "".join(t03_line(l) + "\n" for l in lines_of(text))


def gen_t03():
    fixed = [
        "1+2*-3", "  7 ", "(1 + 2) * 3", "--3", "-(-(4))", "10 / 3", "-10 / 3", "10 / -3", "-7 % 3",
        "7 % -3", "8 - 2 - 1", "100 / 10 / 5", "2 * (3 + 4) * 5", "1 / 0", "(1 + 1) % (2 - 2)",
        "1 / (2 - 2) + 5 / 0", "", "   ", "1 +", "(1 + 2", "1 + 2)", "1 2", "3 + x", "* 4", "12 + (34 * 56",
        "9223372036854775807", "-9223372036854775807 - 1", "((((5))))", "1 - -1", "4 % 0 * 2", "6 * 7 /",
        "(", ")", "2 * ( )", "5 $ 3", "1000000 * 1000000", "123 456",
    ]
    r = random.Random(303)

    def rnd(d):
        if d == 0 or r.random() < 0.3:
            return str(r.randint(0, 50))
        k = r.random()
        if k < 0.15:
            return "-" + rnd(d - 1)
        if k < 0.3:
            return "(" + rnd(d - 1) + ")"
        return rnd(d - 1) + r.choice([" + ", " - ", " * ", " / ", " % ", "+", "*"]) + rnd(d - 1)

    for _ in range(80):
        fixed.append(rnd(4))
    return "\n".join(fixed) + "\n"


# ---------------- T04 ----------------
def t04_parse(text):
    data = {}
    sec = ""
    for n, raw in enumerate(lines_of(text), start=1):
        line = raw.strip()
        if line == "" or line[0] in "#;":
            continue
        if line.startswith("[") and line.endswith("]") and line[1:-1].strip() != "":
            sec = line[1:-1].strip()
            data.setdefault(sec, {})
            continue
        if "=" in line:
            k, v = line.split("=", 1)
            if k.strip() != "":
                data.setdefault(sec, {})[k.strip()] = v.strip()
                continue
        return None, n
    return data, 0


def t04(text, args):
    data, bad = t04_parse(text)
    if data is None:
        return "", "line %d: syntax error\n" % bad, 2

    def look(q):
        if "." in q:
            s, k = q.split(".", 1)
        else:
            s, k = "", q
        return data.get(s, {}).get(k)

    class Miss(Exception):
        pass

    class Cyc(Exception):
        pass

    def expand(q, stack):
        v = look(q)
        stack.append(q)
        out = []
        i = 0
        while i < len(v):
            if v.startswith("${", i):
                j = v.find("}", i + 2)
                if j < 0:
                    out.append(v[i:])
                    break
                ref = v[i + 2:j]
                if ref in stack:
                    raise Cyc()
                if look(ref) is None:
                    raise Miss(ref)
                out.append(expand(ref, stack))
                i = j + 1
            else:
                out.append(v[i])
                i += 1
        stack.pop()
        return "".join(out)

    res = []
    for q in args:
        if look(q) is None:
            res.append("%s: not found" % q)
            continue
        try:
            res.append("%s = %s" % (q, expand(q, [])))
        except Miss as e:
            res.append("%s: missing %s" % (q, e.args[0]))
        except Cyc:
            res.append("%s: cycle" % q)
    return "\n".join(res) + "\n", "", 0


T04_IN = """# global settings
name = demo-${server.host}
  ; indented comment

[server]
host = example.org
port = 80
port = 8080
url = http://${server.host}:${server.port}/

[ db ]
user=admin
url = ${server.url}db?u=${db.user}
dsn = ${db.url} (unterminated ${db.user
empty =
[loop]
a = x${loop.b}
b = y${loop.c}
c = ${loop.a}z

[bad]
ref = ok ${server.port} then ${nope.k} and ${loop.a}
[server]
host = example.com
"""
T04_ARGS = ["server.port", "db.url", "name", "db.dsn", "loop.a", "db.missing", "nosuch.key", "bad.ref"]
T04_BAD = """[a]
x = 1
# fine
[]
y
"""


# ---------------- T05 ----------------
def t05(text, cap):
    courses = {}
    order = []
    rows = []
    for n, line in enumerate(lines_of(text), start=1):
        f = line.split()
        if not f:
            continue
        if len(f) < 2 or not is_uint(f[1]) or int(f[1]) <= 0:
            return "", "line %d: invalid\n" % n, 1
        rows.append(f)
    for f in rows:
        if f[0] in courses:
            return "", "duplicate %s\n" % f[0], 1
        courses[f[0]] = (int(f[1]), f[2:])
        order.append(f[0])
    for f in rows:
        for p in f[2:]:
            if p not in courses:
                return "", "unknown %s\n" % p, 1
    for f in rows:
        if int(f[1]) > cap:
            return "", "too big %s\n" % f[0], 1
    # cycle detection: Kahn
    indeg = {c: len(set(courses[c][1])) for c in courses}
    dependents = {c: [] for c in courses}
    for c in courses:
        for p in set(courses[c][1]):
            dependents[p].append(c)
    q = [c for c in courses if indeg[c] == 0]
    seen = set()
    while q:
        c = q.pop()
        seen.add(c)
        for d in dependents[c]:
            indeg[d] -= 1
            if indeg[d] == 0:
                q.append(d)
    if len(seen) != len(courses):
        rest = sorted(set(courses) - seen)
        return "", "cycle: %s\n" % " ".join(rest), 1
    depth = {}

    def dep(c):
        if c not in depth:
            depth[c] = 1 + max([dep(d) for d in dependents[c]], default=0)
        return depth[c]

    for c in courses:
        dep(c)
    taken = set()
    out = []
    term = 0
    while len(taken) < len(courses):
        term += 1
        avail = [c for c in courses if c not in taken and all(p in taken for p in courses[c][1])]
        avail.sort(key=lambda c: (-depth[c], c.encode()))
        chosen = []
        total = 0
        for c in avail:
            if total + courses[c][0] <= cap:
                chosen.append(c)
                total += courses[c][0]
        taken.update(chosen)
        out.append("term %d: %s (%d)" % (term, " ".join(chosen), total))
    out.append("terms: %d" % term)
    return "\n".join(out) + "\n", "", 0


T05A = """intro 4
algebra 3
calc1 4 algebra
calc2 4 calc1
linalg 3 algebra
prog1 4 intro
prog2 4 prog1
ds 3 prog2 algebra

algo 4 ds calc1
os 4 ds
net 3 os
ml 4 linalg calc2 algo
db 3 ds
writing 2
ethics 2 writing
capstone 5 ml db net ethics
"""
T05B = """a 3
b 2 a
c x a
"""
T05C = """a 3
b 2 a zz
c 4 b
a 1
d 2 yy
"""
T05D = """a 2
b 2 a d
c 3 b
d 2 c
e 1 d
f 1
g 2 f
"""


# ---------------- T06 ----------------
def isprime(n):
    if n < 2:
        return False
    if n % 2 == 0:
        return n == 2
    i = 3
    while i * i <= n:
        if n % i == 0:
            return False
        i += 2
    return True


def t06(text):
    ls = lines_of(text)
    n = len(ls)
    base, extra = divmod(n, 4)
    out = []
    idx = 0
    tv = ts = tp = 0
    mn = mx = None
    for k in range(4):
        size = base + (1 if k < extra else 0)
        chunk = ls[idx:idx + size]
        idx += size
        v = s = p = 0
        for l in chunk:
            if is_int(l):
                x = int(l)
                v += 1
                s += x
                p += isprime(x)
                mn = x if mn is None else min(mn, x)
                mx = x if mx is None else max(mx, x)
        out.append("chunk %d: lines=%d valid=%d sum=%d primes=%d" % (k + 1, size, v, s, p))
        tv += v
        ts += s
        tp += p
    out.append("total: lines=%d valid=%d invalid=%d sum=%d min=%s max=%s primes=%d" % (
        n, tv, n - tv, ts, "none" if mn is None else mn, "none" if mx is None else mx, tp))
    return "\n".join(out) + "\n"


def gen_t06():
    r = random.Random(606)
    ls = []
    for _ in range(1203):
        k = r.random()
        if k < 0.04:
            ls.append(r.choice(["", "abc", "12x", "-", "+5", "3.0", " 7", "--2"]))
        elif k < 0.1:
            ls.append(str(-r.randint(0, 10**9)))
        elif k < 0.3:
            ls.append(str(r.randint(0, 100)))
        else:
            ls.append(str(r.randint(999_000_000, 10**9)))
    ls[0] = "999999937"  # largest prime below 1e9
    ls[1] = "1000000000"
    return "\n".join(ls) + "\n"


# ---------------- T07 ----------------
def t07(text):
    jobs = []
    err = []
    for n, line in enumerate(lines_of(text), start=1):
        f = line.split()
        if len(f) != 4 or not all(is_uint(x) for x in f[1:]) or int(f[2]) <= 0:
            err.append("line %d: invalid" % n)
            continue
        jobs.append((f[0], int(f[1]), int(f[2]), int(f[3])))
    out = []
    t = 0
    idle = 0
    pending = list(jobs)
    waits = []
    while pending:
        ready = [j for j in pending if j[1] <= t]
        if not ready:
            nt = min(j[1] for j in pending)
            idle += nt - t
            t = nt
            continue
        ready.sort(key=lambda j: (-j[3], j[1], j[0].encode()))
        j = ready[0]
        pending.remove(j)
        waits.append(t - j[1])
        out.append("t=%d %s (waited %d)" % (t, j[0], t - j[1]))
        t += j[2]
    avg = sum(waits) / len(waits) if waits else 0.0
    out.append("avg wait: %s" % fmt2(avg))
    out.append("idle: %d" % idle)
    out.append("end: %d" % (t if jobs else 0))
    return "\n".join(out) + "\n", "".join(e + "\n" for e in err)


def gen_t07():
    fixed = """build 0 5 1
lint 0 2 3
test 1 4 3
docs 2 1 0
bad 1
deploy 20 3 9
neg -1 2 2
zero 3 0 1
late 30 2 1
alpha 30 2 1
beta 30 2 5
x 31 1 x
"""
    r = random.Random(707)
    ls = []
    for i in range(60):
        ls.append("j%02d %d %d %d" % (i, r.randint(40, 400), r.randint(1, 12), r.randint(0, 6)))
    return fixed + "\n".join(ls) + "\n"


# ---------------- T09 ----------------
class Node:
    def __init__(self, is_dir, size=0):
        self.is_dir = is_dir
        self.size = size
        self.kids = {}


def t09(text):
    root = Node(True)
    out = []

    def comps(p):
        return [] if p == "/" else p[1:].split("/")

    def valid(p):
        return p.startswith("/") and (p == "/" or (not p.endswith("/") and "" not in p[1:].split("/")))

    def get(p):
        n = root
        for c in comps(p):
            if not n.is_dir or c not in n.kids:
                return None
            n = n.kids[c]
        return n

    def parent(p):
        cs = comps(p)
        return "/" + "/".join(cs[:-1]) if len(cs) > 1 else "/"

    def name(p):
        return comps(p)[-1]

    def size(n):
        return sum(size(k) for k in n.kids.values()) if n.is_dir else n.size

    def join(d, nm):
        return ("/" if d == "/" else d + "/") + nm

    arity = {"mkdir": 1, "touch": 2, "rm": 1, "mv": 2, "du": 1, "ls": 1}
    for line in lines_of(text):
        f = line.split()
        if not f:
            continue
        cmd, a = f[0], f[1:]
        if cmd not in arity or len(a) != arity[cmd] or not all(valid(x) for x in a[: (1 if cmd == "touch" else len(a))]) \
                or (cmd == "touch" and not is_uint(a[1])):
            out.append("error: bad command")
            continue
        if cmd == "mkdir":
            n = root
            path = ""
            err = None
            for c in comps(a[0]):
                path += "/" + c
                if c in n.kids:
                    if not n.kids[c].is_dir:
                        err = "error: not a directory " + path
                        break
                else:
                    n.kids[c] = Node(True)
                n = n.kids[c]
            if err:
                out.append(err)
        elif cmd == "touch":
            p = a[0]
            if p == "/":
                out.append("error: is a directory /")
                continue
            par = get(parent(p))
            if par is None or not par.is_dir:
                out.append("error: no such directory " + parent(p))
                continue
            ex = par.kids.get(name(p))
            if ex is not None and ex.is_dir:
                out.append("error: is a directory " + p)
                continue
            par.kids[name(p)] = Node(False, int(a[1]))
        elif cmd == "rm":
            p = a[0]
            if get(p) is None:
                out.append("error: no such path " + p)
            elif p == "/":
                out.append("error: cannot remove root")
            else:
                del get(parent(p)).kids[name(p)]
        elif cmd == "mv":
            s, d = a
            sn = get(s)
            if sn is None:
                out.append("error: no such path " + s)
                continue
            if s == "/":
                out.append("error: cannot move root")
                continue
            dn = get(d)
            if dn is not None and dn.is_dir:
                target = join(d, name(s))
            elif dn is not None:
                out.append("error: exists " + d)
                continue
            else:
                pn = get(parent(d))
                if pn is None or not pn.is_dir:
                    out.append("error: no such directory " + parent(d))
                    continue
                target = d
            if target == s or target.startswith(s + "/"):
                out.append("error: cannot move into itself")
                continue
            if get(target) is not None:
                out.append("error: exists " + target)
                continue
            del get(parent(s)).kids[name(s)]
            get(parent(target)).kids[name(target)] = sn
        elif cmd == "du":
            n = get(a[0])
            out.append("error: no such path " + a[0] if n is None else "%s %d" % (a[0], size(n)))
        elif cmd == "ls":
            p = a[0]
            n = get(p)
            if n is None:
                out.append("error: no such path " + p)
            elif not n.is_dir:
                out.append(p)
            else:
                ks = sorted(n.kids, key=lambda s: s.encode())
                items = [k + ("/" if n.kids[k].is_dir else "") for k in ks]
                out.append("%s: %s" % (p, " ".join(items) if items else "(empty)"))
    out.append("/ %d" % size(root))

    def walk(n, depth):
        for k in sorted(n.kids, key=lambda s: s.encode()):
            c = n.kids[k]
            if c.is_dir:
                out.append("  " * depth + "%s/ %d" % (k, size(c)))
                walk(c, depth + 1)
            else:
                out.append("  " * depth + "%s %d" % (k, c.size))

    walk(root, 1)
    return "\n".join(out) + "\n"


T09_IN = """mkdir /home/ann/docs
mkdir /home/bob
touch /home/ann/docs/a.txt 100
touch /home/ann/docs/b.txt 250
touch /home/ann/notes 7
touch /home/bob/x 1
touch /tmp/y 5
mkdir /home/ann/notes/deeper
touch /home 3
touch /home/ann/docs/a.txt 120
ls /home
ls /home/ann
ls /home/ann/notes
ls /nope
du /home
du /home/ann/docs
mv /home/ann/docs /home/bob
ls /home/bob
mv /home/bob /home/bob/docs
mv /home /home
mv / /tmp
mv /home/ann/notes /home/ann/notes2
mv /home/ann/notes2 /home/bob/x
mv /home/ann/nothing /home
mv /home/bob/x /home/ann/missing/x
mkdir /home/ann/x
touch /home/ann/x/inner 9
mv /home/bob/x /home/ann
mv /home/bob/docs /home/ann/sub
mv /home/ann/sub /home/ann/x
ls /home/ann
ls /home/ann/x
rm /home/ann/x/sub/a.txt
du /home/ann/x
rm /
rm /zzz
mkdir /a/b/c/d
touch /a/b/c/d/e 11
touch /a/b/f 2
mv /a/b /home
du /a
rm /a
ls /
mkdir /home/b/c
touch /home/b/c/x 1
bogus
touch /q
touch /q -1
touch /q 3 4
mv /home/b /home/b/c

ls /home/b/c/d/e
touch /home/b/c/d/e/f 1
mkdir /home/b/c/d/e/z
"""


# ---------------- T10 ----------------
def collatz(n):
    s = 0
    p = n
    while n != 1:
        n = n // 2 if n % 2 == 0 else 3 * n + 1
        p = max(p, n)
        s += 1
    return s, p


def t10(text):
    out = []
    best = None
    for k, l in enumerate(lines_of(text), start=1):
        if is_uint(l) and 1 <= int(l) <= 10**7 and not (len(l) > 1 and l[0] == "0"):
            n = int(l)
            s, p = collatz(n)
            out.append("%d: steps=%d peak=%d" % (n, s, p))
            if best is None or s > best[1]:
                best = (n, s)
        else:
            out.append("line %d: invalid" % k)
    out.append("longest: none" if best is None else "longest: %d (%d steps)" % best)
    return "\n".join(out) + "\n"


def gen_t10():
    r = random.Random(1010)
    ls = ["1", "27", "0", "10000001", "10000000", "abc", "", "-5", "8400511", "6171", "77031"]
    for _ in range(900):
        ls.append(str(r.randint(1, 10**7)) if r.random() > 0.01 else r.choice(["x", "", "1e5", "+3"]))
    return "\n".join(ls) + "\n"


# ---------------- T11 ----------------
def t11(a, b):
    n, m = len(a), len(b)
    L = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n - 1, -1, -1):
        for j in range(m - 1, -1, -1):
            L[i][j] = L[i + 1][j + 1] + 1 if a[i] == b[j] else max(L[i + 1][j], L[i][j + 1])
    out = []
    i = j = 0
    rem = add = 0
    while i < n and j < m:
        if a[i] == b[j]:
            out.append("  " + a[i])
            i += 1
            j += 1
        elif L[i + 1][j] >= L[i][j + 1]:
            out.append("- " + a[i])
            i += 1
            rem += 1
        else:
            out.append("+ " + b[j])
            j += 1
            add += 1
    for x in a[i:]:
        out.append("- " + x)
        rem += 1
    for x in b[j:]:
        out.append("+ " + x)
        add += 1
    out.append("summary: -%d +%d" % (rem, add))
    return "\n".join(out) + "\n", (0 if rem == 0 and add == 0 else 1)


def gen_t11():
    r = random.Random(1111)
    words = ["alpha", "beta", "gamma", "delta", "", "  indented", "x = 1", "return x", "}", "{", "fn main()"]
    a = [r.choice(words) for _ in range(180)]
    b = []
    for x in a:
        k = r.random()
        if k < 0.08:
            continue
        if k < 0.16:
            b.append(r.choice(words))
        else:
            b.append(x)
        if r.random() < 0.06:
            b.append(r.choice(words) + " new")
    b += ["tail 1", "tail 2"]
    return "\n".join(a) + "\n", "\n".join(b) + "\n"


# ---------------- T12 ----------------
def t12(data):
    if not data:
        return "bits: 0\n"
    import heapq
    cnt = {}
    for c in data:
        cnt[c] = cnt.get(c, 0) + 1
    heap = []
    uid = 0
    for b, c in cnt.items():
        heap.append(((c, b), uid, ("leaf", b)))
        uid += 1
    heapq.heapify(heap)
    while len(heap) > 1:
        kx, _, x = heapq.heappop(heap)
        ky, _, y = heapq.heappop(heap)
        heapq.heappush(heap, ((kx[0] + ky[0], min(kx[1], ky[1])), uid, ("node", x, y)))
        uid += 1
    root = heap[0][2]
    codes = {}

    def walk(n, path):
        if n[0] == "leaf":
            codes[n[1]] = path or "0"
        else:
            walk(n[1], path + "0")
            walk(n[2], path + "1")

    walk(root, "")
    out = []
    for b in sorted(codes):
        rep = chr(b) if 33 <= b <= 126 else "\\x%02X" % b
        out.append("%s %d %s" % (rep, cnt[b], codes[b]))
    out.append("bits: %d" % sum(cnt[b] * len(codes[b]) for b in codes))
    out.append("depth: %d" % max(len(c) for c in codes.values()))
    return "\n".join(out) + "\n"


T12_IN = """the quick brown fox jumps over the lazy dog
THE QUICK BROWN FOX!!! 1234567890
	tabs	and	spaces   mixed ~ `backticks` "quotes" \\back\\slash
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
zzzz yyy xx w
"""


def main():
    t = gen_t01()
    w("T01.in", t)
    w("T01.out", t01(t))
    t = gen_t02()
    w("T02.in", t)
    w("T02.out", t02(t))
    t = gen_t03()
    w("T03.in", t)
    w("T03.out", t03(t))
    w("T04.in", T04_IN)
    o, e, rc = t04(T04_IN, T04_ARGS)
    assert rc == 0 and e == ""
    w("T04.out", o)
    w("T04bad.in", T04_BAD)
    o, e, rc = t04(T04_BAD, ["x"])
    assert rc == 2 and o == ""
    w("T04bad.err", e)
    for suffix, src, cap, want in [("a", T05A, 10, 0), ("b", T05B, 10, 1), ("c", T05C, 10, 1), ("d", T05D, 6, 1)]:
        w("T05%s.in" % suffix, src)
        o, e, rc = t05(src, cap)
        assert rc == want, (suffix, rc, e)
        if rc == 0:
            w("T05%s.out" % suffix, o)
        else:
            w("T05%s.err" % suffix, e)
    t = gen_t06()
    w("T06.in", t)
    w("T06.out", t06(t))
    t = gen_t07()
    w("T07.in", t)
    o, e = t07(t)
    w("T07.out", o)
    w("T07.err", e)
    w("T09.in", T09_IN)
    w("T09.out", t09(T09_IN))
    t = gen_t10()
    w("T10.in", t)
    w("T10.out", t10(t))
    a, b = gen_t11()
    w("T11a.txt", a)
    w("T11b.txt", b)
    o, rc = t11(lines_of(a), lines_of(b))
    assert rc == 1
    w("T11ab.out", o)
    o, rc = t11(lines_of(a), lines_of(a))
    assert rc == 0
    w("T11aa.out", o)
    w("T12.in", T12_IN)
    w("T12.out", t12(T12_IN.encode()))


main()
