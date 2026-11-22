#!/usr/bin/env python3
"""Frozen round-five task packet and independent, serial Python oracle.

No submitted source is imported or examined. Running this script recreates
TASKS.md, public inputs, hidden judge cases and packet-manifest.json exactly.
Seeded random cases complement named, manually selected boundary cases.
"""
from __future__ import annotations

import hashlib
import json
import math
import random
import re
from collections import Counter, deque
from functools import cmp_to_key
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEED = 2026112205

COMMON = r"""# 第五轮新题 T01–T12

这 12 题全部为新写程序，不含修复题。每题由标准输入读取，标准输出逐字节匹配，标准错误为空，退出码为 0；隐藏用例只在交卷后运行，不提供修复反馈。

- 每题使用独立目录，入口统一为该题目录内的 `main.yv` 或 `main.rs`；T04 的其他模块也放在该题目录中。
- 输入只含 ASCII；不存在 NUL 或 CR。所有非空输入以 LF 换行结束；“一行”不包含 LF。除本题特别说明，字段以一个或多个空格/制表符分隔，行首尾可有这些空白。
- 输出的每一行以 LF 结束。所有排序中的字符串顺序均为 ASCII 字节序。数字输出为普通十进制，不保留输入的前导零、正号或负零。
- 没有要求处理题目有效输入范围之外的数据；明确要求判定无效行的题除外。每题上限覆盖全部测试；算术运算的值均在有符号 64 位范围内。T12 只用于比较的纯数字 PRE 标识符按该题规则处理，不受 64 位范围限制。
- 不得调用其他语言、执行外部程序或使用第三方依赖。可以任选符合题意的数据结构，不要求某个具体容器。
- T04 的模块结构和 T06 的真实线程要求属于验收条件；仅输出正确仍不足以通过这两题。

每题下方有两个公开样例，`public/cases.json` 保存同样的输入、精确输出和退出码。不要读取任何裁判或其他受试者文件。
"""

SPECS = {
"T01": ("原子转账账本", r"""维护若干账户，开始为空。账户名匹配 `[A-Za-z][A-Za-z0-9_]{0,15}`。输入中的初始余额和转账金额为 0..10^9 的十进制整数，允许前导零；转账后的余额可以超过 10^9。所有命令和 batch 内行的语法有效；最多 200 条命令，空行跳过。

- `account NAME BALANCE`：创建账户；已存在则输出 `error: duplicate NAME`，保持原余额。
- `pay FROM TO AMOUNT`：先检查 FROM 存在，再检查 TO 存在，再检查 FROM 余额不少于 AMOUNT；失败输出 `error: unknown NAME` 或 `error: funds FROM`。成功完成转账并输出 `paid`。同名账户转账也先检查余额，成功后余额不变。
- `batch K`：紧跟 K 行 `FROM TO AMOUNT`，0 ≤ K ≤ 100；这些行不是命令且不会为空。按顺序在临时账本上转账，第一条失败时整个 batch 的任何变更都不保留，输出 `batch: row J unknown NAME` 或 `batch: row J funds NAME`，J 从 1 开始。剩余 batch 行仍须读完，但不再执行。全部成功则一次提交，输出 `batch: ok`。batch 内不能创建账户。
- `balance NAME`：输出 `NAME: BALANCE`，不存在则输出 `error: unknown NAME`。

输入结束时，按账户名排序逐个输出 `NAME=当前余额`，最后输出 `total: 所有账户余额之和`。空文件只输出 `total: 0`。
"""),
"T02": ("严格 CSV 记录", r"""标准输入每一行是一条独立 CSV 记录，不支持跨行字段。字段内容为可打印 ASCII（32..126），每行最多 200 字节，共最多 200 行。空文件没有记录、没有输出；空行表示一个空字段。

字段以逗号分隔，空字段合法。未加引号字段中不能出现双引号；空格是字段内容，不修剪。加引号字段必须以 `"` 开始，以 `"` 结束；内部的 `""` 解码为一个 `"`，其他字符（含逗号）原样保留；结束引号后只能紧接逗号或行尾。未闭合引号、未加引号字段内的引号、结束引号后的其他字符均使整行无效。

有效记录先输出 `K: N`（K 为输入行号、N 为字段数），随后每个解码字段输出一行 `L:内容`，L 为内容字节数；无效记录只输出 `K: invalid`。K 从 1 开始。不要为最后的 LF 额外创建一条空记录。
"""),
"T03": ("整数区间集合", r"""维护一个整数集合，开始为空。每行命令语法有效，最多 200 条，空行跳过；所有端点和查询值在 -10^9..10^9 内，区间均满足 L ≤ R。区间为两端都包含的闭区间。

- `add L R`：加入区间中的全部整数。
- `del L R`：移除区间中的全部整数，不存在的元素忽略。
- `has X`：存在输出 `yes`，否则 `no`。
- `show`：把当前集合压缩成按左端点升序排列的最大连续区间，输出 `L:R L:R ...`；相邻区间必须合并，单元素也输出 `X:X`。空集合输出 `empty`。

最后输出 `count: N`，N 是当前集合包含的整数个数。空文件输出 `count: 0`。端点跨度可能很大，不能假定集合元素很少。
"""),
"T04": ("精确长度换算（多文件）", r"""至少两个源文件。所有数字解析、单位校验、换算和约分逻辑放在非入口模块；入口文件仅读输入、调用模块、输出。

每行应有三个字段 `AMOUNT FROM TO`。有效单位仅为 `mm`、`cm`、`m`、`km`，分别等于 1、10、1000、1000000 mm。AMOUNT 语法为 `-?[0-9]+(\.[0-9]{1,3})?`：不允许 `+`、指数、`.5`、`1.`；允许任意前导零和负零。整个数字字段最多 64 字节，数值绝对值 ≤ 10^6。有效行的精确换算结果输出 `K: N TO`（结果为整数）或 `K: N/D TO`（最简分数，D > 1，符号仅在 N 上）。零统一写 `0`。

字段数、数值语法、绝对值或单位任一不合法，输出 `K: invalid`。每行都处理，空行也无效；K 从 1 开始。最多 200 行；空文件无输出。不可用浮点近似代替精确结果。
"""),
"T05": ("网格连通域", r"""第一行是 `H W`，然后恰有 H 行长度 W 的网格，仅含 `.` 和 `#`。H、W 同时为 0，或均在 1..25 内；空网格输入为 `0 0\n`，不会给空文件。

把 `#` 按上下左右相邻划分连通域，斜接触不连通。按行优先扫描网格：首次遇到尚未归属的 `#` 时给其整个连通域分配下一个编号（从 1 开始）。每个格子的某条边邻接 `.` 或网格外，该边就计入所属连通域周长，内部孔洞的边也计入。

先输出 `components: N`；再按编号输出 `K: area=A perimeter=P box=r0,c0:r1,c1`。边界框端点都包含，坐标从 0 开始。最后输出 `largest: K`，面积最大者胜出、同面积取编号最小者；没有连通域时输出 `largest: none`。
"""),
"T06": ("三线程游程编码", r"""输入恰好一行可打印 ASCII 文本（32..126），长度 n 为 0..600，行末 LF 不属于文本。不能给空文件；空文本是一个 LF。

必须启动 **3 个 OS 工作线程**，在等待任一线程之前已启动全部三个。把文本分成连续三段：每段 n/3 字节，前 n%3 段各多一字节；空段也由对应线程处理。每个工作线程只为自己的段做游程编码，返回连续相同字节的 `(字节值, 次数)` 列表。主线程按段顺序合并结果，并合并跨段边界的相同字节；不能在主线程对整个文本重新编码，也不能用一个线程完成全部任务。

对合并后的每个游程输出 `BYTE COUNT`，BYTE 为 ASCII 字节值的十进制数。最后输出 `runs: R chars: n`。空文本只输出 `runs: 0 chars: 0`。
"""),
"T07": ("带 TTL 的 LRU 缓存", r"""第一行是容量 C，0 ≤ C ≤ 8；之后最多 200 条命令，命令语法有效，空行跳过。每条命令以时间 T 开始，时间为非递减的 0..10000 整数；键匹配 `[a-z][a-z0-9_]{0,15}`，值是 -10^9..10^9 的整数。不存在空文件。

每条命令执行前，移除全部到期时间 ≤ T 的条目。

- `T put KEY VALUE TTL`：TTL 为 0..100。TTL=0 时删除 KEY，不插入；C=0 时也不保存。其他情况下插入或覆盖 KEY，其到期时间设为 T+TTL，并变为最近使用。超过容量时逐个淘汰最久未使用的条目。成功 put 没有输出。
- `T get KEY`：存在输出 `KEY: VALUE`，并变为最近使用，但**不延长到期时间**；不存在输出 `KEY: missing`。
- `T keys`：输出 `keys: ` 后接从最近使用到最久未使用的键，以空格分隔；为空输出 `keys: none`。keys 不改变使用顺序。

EOF 不推进时间，最后再按同样格式输出一次 `keys: ...`。不同 put/get 的执行顺序决定 LRU，即使时间相同也依次生效。
"""),
"T08": ("标签目录", r"""维护“文档名 → 标签集合”，开始为空。文档名和标签均匹配 `[a-z][a-z0-9_]{0,15}`。最多 200 条命令，语法有效，空行跳过。

- `set DOC TAG...`：创建或完整替换该文档的标签；允许零个标签，重复标签只保留一次。
- `add DOC TAG`：加入标签；文档不存在则先创建。
- `remove DOC TAG`：移除标签；标签或文档不存在时什么也不做；不会因此删除文档。
- `drop DOC`：删除文档；不存在时什么也不做。
- `tags DOC`：不存在输出 `DOC: missing`；存在则输出 `DOC: ` 后接按字节序的标签，以空格分隔；无标签时输出 `DOC: none`。
- `query TAG...`：输出包含所列**全部**标签的文档，按名字字节序，格式 `docs: A B ...`，没有匹配输出 `docs: none`。重复查询标签无额外影响；零个查询标签匹配全部文档，包括没有标签的文档。

EOF 输出 `total: docs=N tags=U`，U 是当前所有文档中不同标签的总数。空文件输出 `total: docs=0 tags=0`。
"""),
"T09": ("带转义的 glob 匹配", r"""每两行一组：第一行是模式，第二行是待匹配文本。两行均可为空；行数总为偶数，空文件是零组。模式最多 80 字节，文本最多 120 字节，均为可打印 ASCII（32..126），最多 100 组。

模式中的 `?` 匹配恰好一个任意字节，`*` 匹配零个或多个任意字节；相邻多个 `*` 合法。反斜线 `\` 使紧随的一个字符按字面匹配，包括 `*`、`?`、`\` 以及普通字符。模式末尾单独一个反斜线使该组无效。必须匹配整个文本，不是子串查找。

按组从 1 开始输出 `K: yes`、`K: no` 或 `K: invalid`。空文件无输出。
"""),
"T10": ("相对路径归一化", r"""第一行是基准绝对路径，已经规范化：为 `/` 或用单个 `/` 连接的非空普通组件，不含 `.`、`..`，也不带结尾 `/`。之后每行是独立待解析路径，可以为空。最多 200 个查询，每条路径最多 200 字节；不存在空文件。

路径组件只含 `[A-Za-z0-9_.-]`；`/` 为分隔符，重复分隔符视为一个。以 `/` 开头的查询从根目录开始，其他查询从固定基准路径开始；每个查询互不影响。忽略空组件和恰好为 `.` 的组件；恰好为 `..` 时移除一个已有组件，在根目录继续 `..` 仍停留在根目录。其他组件原样保留，例如 `...` 和 `.hidden` 都不是特殊组件。空查询得到基准路径。

每个查询输出一个规范绝对路径：开头一个 `/`、组件间一个 `/`、根目录以外无末尾 `/`。仅有基准行时无输出。
"""),
"T11": ("滑动窗口下中位数", r"""第一行是窗口大小 K，1 ≤ K ≤ 25。之后所有空白分隔字段组成整数序列，长度 0..200，每个数在 -10^9..10^9 内；这些字段均合法。不存在空文件；只含 K 的输入表示空序列。

对每个完整的连续 K 元素窗口输出 `I: median=M sum=S`，I 是该窗口最后一个元素在原序列中的位置（从 1 开始）。把窗口值升序排列，下中位数 M 是从 0 开始的第 `(K-1)/2` 个值；重复值分别计数。S 是窗口元素之和。最后输出 `windows: N`。少于 K 个元素时没有窗口行。
"""),
"T12": ("版本号稳定排序", r"""每一行是一个版本字符串，不修剪空白；空行无效。最多 100 行，每行最多 160 个可打印 ASCII 字节；空文件合法。

语法为 `MAJOR.MINOR.PATCH[-PRE][+BUILD]`。三个核心分量均为一个或多个数字，值在 0..10^9，允许前导零。PRE 和 BUILD 是一个或多个由点分隔的非空标识符；每个标识符只含 `[A-Za-z0-9-]`，长度最多 32。只有数字的 PRE 标识符允许前导零，不受 64 位整数范围限制。BUILD 只校验语法，不参与排序。只能有上述位置的一次 `+`；PRE 内的 `-` 只是普通字符。

先按输入顺序，对无效行输出 `line K: invalid`，K 从 1 开始。随后将所有有效版本按下列升序规则稳定排序，每行原样输出原版本字符串：

1. 依次比较三个核心分量的数值。
2. 核心相同，有 PRE 的版本排在没有 PRE 的正式版本之前。
3. 两者都有 PRE，逐个比较标识符：两个纯数字按数学整数值比较；纯数字排在非纯数字前；两个非纯数字按 ASCII 字节序比较。
4. 共同前缀相同，标识符较少者在前；全部比较相同则保持输入顺序（包括前导零、BUILD 不同但排序等价）。

最后输出 `valid: N invalid: M`。
"""),
}


def t01(data: str) -> str:
    accounts: dict[str, int] = {}
    lines = data.splitlines()
    out: list[str] = []

    def transfer(state: dict[str, int], row: list[str]) -> str | None:
        a, b, n0 = row
        n = int(n0)
        if a not in state:
            return f"unknown {a}"
        if b not in state:
            return f"unknown {b}"
        if state[a] < n:
            return f"funds {a}"
        state[a] -= n
        state[b] += n
        return None

    i = 0
    while i < len(lines):
        fields = lines[i].split()
        i += 1
        if not fields:
            continue
        cmd = fields[0]
        if cmd == "account":
            name, raw = fields[1:]
            if name in accounts:
                out.append(f"error: duplicate {name}")
            else:
                accounts[name] = int(raw)
        elif cmd == "pay":
            error = transfer(accounts, fields[1:])
            out.append("paid" if error is None else f"error: {error}")
        elif cmd == "balance":
            name = fields[1]
            out.append(f"{name}: {accounts[name]}" if name in accounts else f"error: unknown {name}")
        elif cmd == "batch":
            n = int(fields[1])
            scratch = accounts.copy()
            error = None
            for j in range(n):
                if error is None:
                    reason = transfer(scratch, lines[i + j].split())
                    if reason is not None:
                        error = f"batch: row {j + 1} {reason}"
            i += n
            if error is None:
                accounts = scratch
                out.append("batch: ok")
            else:
                out.append(error)
        else:
            raise AssertionError(cmd)
    out += [f"{name}={accounts[name]}" for name in sorted(accounts)]
    out.append(f"total: {sum(accounts.values())}")
    return "\n".join(out) + "\n"


def csv_record(s: str) -> list[str] | None:
    result: list[str] = []
    i = 0
    while True:
        value = ""
        if i < len(s) and s[i] == '"':
            i += 1
            while True:
                if i == len(s):
                    return None
                if s[i] != '"':
                    value += s[i]
                    i += 1
                elif i + 1 < len(s) and s[i + 1] == '"':
                    value += '"'
                    i += 2
                else:
                    i += 1
                    break
            if i < len(s) and s[i] != ',':
                return None
        else:
            while i < len(s) and s[i] != ',':
                if s[i] == '"':
                    return None
                value += s[i]
                i += 1
        result.append(value)
        if i == len(s):
            return result
        assert s[i] == ','
        i += 1


def t02(data: str) -> str:
    out: list[str] = []
    for k, row in enumerate(data.splitlines(), 1):
        fields = csv_record(row)
        if fields is None:
            out.append(f"{k}: invalid")
        else:
            out.append(f"{k}: {len(fields)}")
            out.extend(f"{len(f)}:{f}" for f in fields)
    return "\n".join(out) + ("\n" if out else "")


def t03(data: str) -> str:
    intervals: list[tuple[int, int]] = []
    out: list[str] = []
    for line in data.splitlines():
        f = line.split()
        if not f:
            continue
        if f[0] == "add":
            left, right = map(int, f[1:])
            ordered = sorted(intervals + [(left, right)])
            intervals = []
            for a, b in ordered:
                if intervals and a <= intervals[-1][1] + 1:
                    intervals[-1] = (intervals[-1][0], max(intervals[-1][1], b))
                else:
                    intervals.append((a, b))
        elif f[0] == "del":
            left, right = map(int, f[1:])
            next_intervals = []
            for a, b in intervals:
                if b < left or a > right:
                    next_intervals.append((a, b))
                else:
                    if a < left:
                        next_intervals.append((a, left - 1))
                    if b > right:
                        next_intervals.append((right + 1, b))
            intervals = next_intervals
        elif f[0] == "has":
            x = int(f[1])
            out.append("yes" if any(a <= x <= b for a, b in intervals) else "no")
        elif f[0] == "show":
            out.append(" ".join(f"{a}:{b}" for a, b in intervals) or "empty")
        else:
            raise AssertionError(f)
    out.append(f"count: {sum(b - a + 1 for a, b in intervals)}")
    return "\n".join(out) + "\n"


FACTORS = {"mm": 1, "cm": 10, "m": 1000, "km": 1000000}


def t04(data: str) -> str:
    out: list[str] = []
    for k, line in enumerate(data.splitlines(), 1):
        f = line.split()
        if len(f) != 3 or f[1] not in FACTORS or f[2] not in FACTORS or len(f[0]) > 64 or re.fullmatch(r"-?[0-9]+(?:\.[0-9]{1,3})?", f[0]) is None:
            out.append(f"{k}: invalid")
            continue
        s, src, dst = f
        sign = -1 if s.startswith("-") else 1
        s = s.removeprefix("-")
        whole, _, frac = s.partition(".")
        scaled = int(whole) * 1000 + int(frac.ljust(3, "0") or "0")
        if scaled > 1000000000:
            out.append(f"{k}: invalid")
            continue
        num, den = sign * scaled * FACTORS[src], 1000 * FACTORS[dst]
        divisor = math.gcd(num, den)
        num, den = num // divisor, den // divisor
        value = str(num) if den == 1 else f"{num}/{den}"
        out.append(f"{k}: {value} {dst}")
    return "\n".join(out) + ("\n" if out else "")


def t05(data: str) -> str:
    lines = data.splitlines()
    h, w = map(int, lines[0].split())
    grid = lines[1:]
    assert len(grid) == h and all(len(row) == w for row in grid)
    visited: set[tuple[int, int]] = set()
    components = []
    for r in range(h):
        for c in range(w):
            if grid[r][c] != "#" or (r, c) in visited:
                continue
            cells = [(r, c)]
            visited.add((r, c))
            queue = deque(cells)
            perimeter = 0
            while queue:
                a, b = queue.popleft()
                for x, y in ((a - 1, b), (a + 1, b), (a, b - 1), (a, b + 1)):
                    if not (0 <= x < h and 0 <= y < w) or grid[x][y] != "#":
                        perimeter += 1
                    elif (x, y) not in visited:
                        visited.add((x, y))
                        queue.append((x, y))
                        cells.append((x, y))
            components.append((len(cells), perimeter, min(a for a, _ in cells), min(b for _, b in cells), max(a for a, _ in cells), max(b for _, b in cells)))
    out = [f"components: {len(components)}"]
    for k, (area, p, r0, c0, r1, c1) in enumerate(components, 1):
        out.append(f"{k}: area={area} perimeter={p} box={r0},{c0}:{r1},{c1}")
    best = str(max(range(len(components)), key=lambda k: components[k][0]) + 1) if components else "none"
    out.append(f"largest: {best}")
    return "\n".join(out) + "\n"


def t06(data: str) -> str:
    assert data.endswith("\n") and "\n" not in data[:-1]
    text = data[:-1]
    runs: list[list[int]] = []
    for ch in text:
        b = ord(ch)
        if runs and runs[-1][0] == b:
            runs[-1][1] += 1
        else:
            runs.append([b, 1])
    out = [f"{b} {n}" for b, n in runs]
    out.append(f"runs: {len(runs)} chars: {len(text)}")
    return "\n".join(out) + "\n"


def t07(data: str) -> str:
    lines = data.splitlines()
    capacity = int(lines[0])
    cache: dict[str, tuple[int, int]] = {}
    recent: list[str] = []  # least recent to most recent
    out = []

    def erase(key: str) -> None:
        if key in cache:
            del cache[key]
            recent.remove(key)

    def keys() -> str:
        return "keys: " + (" ".join(reversed(recent)) or "none")

    for line in lines[1:]:
        f = line.split()
        if not f:
            continue
        now, cmd = int(f[0]), f[1]
        for key in list(cache):
            if cache[key][1] <= now:
                erase(key)
        if cmd == "put":
            key, value, ttl = f[2], int(f[3]), int(f[4])
            erase(key)
            if ttl and capacity:
                cache[key] = (value, now + ttl)
                recent.append(key)
                while len(cache) > capacity:
                    erase(recent[0])
        elif cmd == "get":
            key = f[2]
            if key not in cache:
                out.append(f"{key}: missing")
            else:
                out.append(f"{key}: {cache[key][0]}")
                recent.remove(key)
                recent.append(key)
        elif cmd == "keys":
            out.append(keys())
        else:
            raise AssertionError(cmd)
    out.append(keys())
    return "\n".join(out) + "\n"


def t08(data: str) -> str:
    docs: dict[str, set[str]] = {}
    out: list[str] = []
    for line in data.splitlines():
        f = line.split()
        if not f:
            continue
        cmd = f[0]
        if cmd == "set":
            docs[f[1]] = set(f[2:])
        elif cmd == "add":
            docs.setdefault(f[1], set()).add(f[2])
        elif cmd == "remove":
            if f[1] in docs:
                docs[f[1]].discard(f[2])
        elif cmd == "drop":
            docs.pop(f[1], None)
        elif cmd == "tags":
            name = f[1]
            out.append(f"{name}: " + ((" ".join(sorted(docs[name])) or "none") if name in docs else "missing"))
        elif cmd == "query":
            required = set(f[1:])
            matching = sorted(name for name, tags in docs.items() if required <= tags)
            out.append("docs: " + (" ".join(matching) or "none"))
        else:
            raise AssertionError(cmd)
    all_tags = set().union(*docs.values()) if docs else set()
    out.append(f"total: docs={len(docs)} tags={len(all_tags)}")
    return "\n".join(out) + "\n"


def glob_regex(pattern: str) -> str | None:
    regex = []
    i = 0
    while i < len(pattern):
        ch = pattern[i]
        i += 1
        if ch == "\\":
            if i == len(pattern):
                return None
            regex.append(re.escape(pattern[i]))
            i += 1
        elif ch == "*":
            regex.append(".*")
        elif ch == "?":
            regex.append(".")
        else:
            regex.append(re.escape(ch))
    return "".join(regex)


def t09(data: str) -> str:
    lines = data.splitlines()
    assert len(lines) % 2 == 0
    out = []
    for i in range(0, len(lines), 2):
        pattern, text = lines[i:i + 2]
        regex = glob_regex(pattern)
        result = "invalid" if regex is None else ("yes" if re.fullmatch(regex, text) else "no")
        out.append(f"{i // 2 + 1}: {result}")
    return "\n".join(out) + ("\n" if out else "")


def t10(data: str) -> str:
    lines = data.splitlines()
    base = [p for p in lines[0].split("/") if p]
    out = []
    for line in lines[1:]:
        parts = [] if line.startswith("/") else base.copy()
        for part in line.split("/"):
            if part == "..":
                if parts:
                    parts.pop()
            elif part and part != ".":
                parts.append(part)
        out.append("/" + "/".join(parts))
    return "\n".join(out) + ("\n" if out else "")


def t11(data: str) -> str:
    fields = data.split()
    k = int(fields[0])
    values = list(map(int, fields[1:]))
    out = []
    for end in range(k, len(values) + 1):
        window = values[end - k:end]
        median = sorted(window)[(k - 1) // 2]
        out.append(f"{end}: median={median} sum={sum(window)}")
    out.append(f"windows: {max(0, len(values) - k + 1)}")
    return "\n".join(out) + "\n"


VERSION_RE = re.compile(r"([0-9]+)\.([0-9]+)\.([0-9]+)(?:-([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*))?(?:\+([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*))?", re.ASCII)


def version_parse(s: str):
    match = VERSION_RE.fullmatch(s)
    if match is None:
        return None
    major, minor, patch, pre, build = match.groups()
    core = tuple(map(int, (major, minor, patch)))
    if any(value > 1000000000 for value in core):
        return None
    if any(len(part) > 32 for value in (pre, build) if value is not None for part in value.split(".")):
        return None
    return core, None if pre is None else pre.split(".")


def version_cmp(a, b):
    if a[0] != b[0]:
        return (a[0] > b[0]) - (a[0] < b[0])
    p, q = a[1], b[1]
    if p is None or q is None:
        return (p is None) - (q is None)
    for x, y in zip(p, q):
        nx, ny = x.isdigit(), y.isdigit()
        if nx != ny:
            return -1 if nx else 1
        vx, vy = (int(x), int(y)) if nx else (x, y)
        if vx != vy:
            return (vx > vy) - (vx < vy)
    return (len(p) > len(q)) - (len(p) < len(q))


def t12(data: str) -> str:
    out = []
    valid = []
    for k, line in enumerate(data.splitlines(), 1):
        parsed = version_parse(line)
        if parsed is None:
            out.append(f"line {k}: invalid")
        else:
            valid.append((line, parsed))
    count_invalid = len(out)
    valid.sort(key=cmp_to_key(lambda a, b: version_cmp(a[1], b[1])))
    out.extend(line for line, _ in valid)
    out.append(f"valid: {len(valid)} invalid: {count_invalid}")
    return "\n".join(out) + "\n"


ORACLES = {f"T{i:02}": globals()[f"t{i:02}"] for i in range(1, 13)}

# Two public records per task. Hidden cases never enter these files or TASKS.md.
PUBLIC = {
"T01": [
    "account bob 20\naccount ann 10\npay bob ann 3\nbalance ann\nbatch 2\nann bob 4\nbob ann 100\nbalance ann\n",
    "account a 0007\naccount a 99\npay a a 7\nbatch 0\npay ghost absent 0\n",
],
"T02": ['a,"b,c",\n"say ""hi""",x\n', '\n""\na"b,c\n"x" tail\n'],
"T03": ["add 1 3\nadd 5 7\nadd 4 4\nshow\ndel 3 5\nshow\nhas 3\nhas 7\n", "show\nadd -2 2\ndel -2 -2\ndel 2 2\nshow\n"],
"T04": ["1.5 m cm\n001 mm cm\n-0.125 km m\n", "+1 m mm\n.5 m cm\n-0.000 km mm\n1 m inch\n"],
"T05": ["3 4\n##..\n.#.#\n...#\n", "3 3\n###\n#.#\n###\n"],
"T06": ["aaabbbbbcc\n", "A\n"],
"T07": ["2\n0 put a 10 5\n0 put b 20 10\n1 get a\n2 put c 30 10\n2 keys\n5 get a\n5 keys\n", "1\n0 put a 7 4\n1 get a\n4 get a\n4 put a 9 0\n4 keys\n"],
"T08": ["set a red blue red\nset b blue\nadd c red\nquery blue\nquery red blue\nremove a red\ntags a\n", "set empty\nadd b x\nquery\ndrop b\nquery x\ntags empty\n"],
"T09": ["a*?c\nabbc\na\\*b\na*b\na\\*b\naxxb\n", "\n\n*\n\nabc\\\nabc\n"],
"T10": ["/a/b\n../c//./d/\n/../../x\n\n", "/\n.../.hidden/../z\na/../../b\n////\n"],
"T11": ["3\n4 1 7 2 2\n", "2\n-3 -1 -1 8\n"],
"T12": ["1.0.0\n1.0.0-beta.2\n1.0.0-beta.11\n1.0.0-alpha\n0.9.9\n", "01.0.0+z\n1.0.0+a\n1.0.0-01\n1.0.0-1\n1.0\n\n"],
}

HIDDEN = {
"T01": [
    ("empty", ""),
    ("self-funds-before-noop", "account a 3\npay a a 4\npay a a 0\n"),
    ("destination-check-before-funds", "account a 0\npay a missing 5\npay missing absent 5\n"),
    ("batch-rollback-and-consume", "account a 10\naccount b 0\nbatch 3\na b 6\na b 6\nb a 1\nbalance a\nbalance b\npay b a 0\n"),
    ("batch-sequential-credit", "account a 10\naccount b 0\naccount c 0\nbatch 3\na b 10\nb c 10\nc a 10\n"),
    ("duplicates-sort-and-zero", "account z 0\naccount Aa 3\naccount A 7\naccount z 9\nbatch 0\nbalance nope\n"),
    ("large-sums", "account a 1000000000\naccount b 1000000000\naccount c 1000000000\nbatch 2\na c 1000000000\nb c 1000000000\n"),
    ("whitespace-and-leading-zero", " \n\taccount\ta\t0000\naccount b 000010\n batch 2\nb a 000001\na b 000000\nbalance a\n"),
],
"T02": [
    ("empty-file", ""),
    ("empty-record-vs-final-newline", "\n\n"),
    ("trailing-and-consecutive-empty-fields", ",\n,,\na,,b,\n"),
    ("escaped-quote-runs", '""\n""""\n""""""\n"a""b"\n'),
    ("unclosed-and-junk", '"\n"abc\na"\n"ok"x\n"ok" \n'),
    ("spaces-are-data", ' a , b \n" a "," "\n "a"\n'),
    ("delimiters-as-data", '"a,b","|:","${x}"\n'),
    ("max-width", "x" * 200 + "\n"),
],
"T03": [
    ("empty", ""),
    ("single-point", "add 0 0\nshow\nhas 0\ndel 0 0\nshow\n"),
    ("merge-both-sides", "add -5 -1\nadd 1 4\nadd 0 0\nshow\n"),
    ("split-and-rejoin", "add 0 10\ndel 3 7\nshow\nadd 3 7\nshow\n"),
    ("outside-deletion", "add 2 5\ndel -9 1\ndel 6 99\nshow\n"),
    ("overlap-idempotence", "add 1 9\nadd 3 4\nadd 1 9\ndel 4 6\ndel 4 6\nshow\n"),
    ("large-span", "add -1000000000 1000000000\nhas -1000000000\nhas 1000000000\ndel -999999999 999999999\nshow\n"),
    ("erase-everything", "add -3 -1\nadd 2 7\ndel -100 100\nshow\nhas 0\n"),
],
"T04": [
    ("empty", ""),
    ("zero-normalization", "-0 mm km\n-0.000 m cm\n00000.000 km mm\n"),
    ("leading-zero-length", "0000000000000000000000000000000000000000000000000000000000000001 mm cm\n"),
    ("fraction-reduction", "0.125 mm cm\n2.500 cm m\n-1.250 m km\n"),
    ("bounds", "1000000 km mm\n-1000000 km mm\n1000000.001 mm mm\n1000001 m m\n"),
    ("grammar", "1. m cm\n.1 m cm\n+1 m cm\n1e2 m cm\n1.0000 m cm\n--1 m cm\n"),
    ("fields-units-case", "\n1 m\n1 m cm extra\n1 M cm\n1 m CM\n1 mm km\n"),
    ("precision-direction", "0.001 km mm\n0.001 mm km\n-0.001 cm m\n"),
],
"T05": [
    ("zero-grid", "0 0\n"),
    ("one-water", "1 1\n.\n"),
    ("one-land", "1 1\n#\n"),
    ("diagonal-separate", "3 3\n#.#\n.#.\n#.#\n"),
    ("equal-area-stable", "2 5\n##.##\n.....\n"),
    ("border-full", "2 3\n###\n###\n"),
    ("multiple-holes", "5 5\n#####\n#.#.#\n#####\n#.#.#\n#####\n"),
    ("snake-and-island", "4 5\n###.#\n..#..\n###..\n#....\n"),
],
"T06": [
    ("empty", "\n"),
    ("two-empty-worker", "xx\n"),
    ("one-run-across-all-chunks", "x" * 600 + "\n"),
    ("boundary-same", "abbbbc\n"),
    ("remainder-one", "aaabaaa\n"),
    ("remainder-two", "aabbbbaa\n"),
    ("spaces-and-backslash", "  $$\\\\\"\"  \n"),
    ("alternating", "ab" * 30 + "\n"),
],
"T07": [
    ("no-events", "3\n"),
    ("zero-capacity", "0\n0 put a 1 10\n0 get a\n0 keys\n"),
    ("zero-ttl-removes-existing", "2\n0 put a 1 10\n1 put a 2 0\n1 get a\n"),
    ("expiry-before-eviction", "2\n0 put a 1 2\n0 put b 2 10\n1 get a\n2 put c 3 10\n2 keys\n2 get b\n"),
    ("get-does-not-renew", "1\n0 put a 1 3\n1 get a\n2 get a\n3 get a\n"),
    ("overwrite-refreshes", "2\n0 put a 1 1\n0 put b 2 10\n0 put a 3 10\n1 keys\n1 put c 4 10\n1 keys\n"),
    ("same-time-order", "2\n5 put a 1 2\n5 put b 2 2\n5 get a\n5 put c 3 2\n5 keys\n7 keys\n"),
    ("missing-get-and-keys-no-touch", "2\n0 put a 1 100\n0 put b 2 100\n0 get missing\n0 keys\n0 put c 3 100\n0 get a\n"),
],
"T08": [
    ("empty", ""),
    ("duplicates", "set z a a b\nadd z a\nquery a a\ntags z\n"),
    ("replacement-removes-stale", "set a red blue\nset b blue\nset a green\nquery red\nquery blue\n"),
    ("empty-query-includes-empty", "set a\nset b x\nquery\nquery x\n"),
    ("drop-recreate", "set a old\ndrop a\nadd a new\ntags a\nquery old\n"),
    ("remove-last-keeps-document", "add a x\nremove a x\nquery\ntags a\n"),
    ("missing-noops", "remove missing x\ndrop missing\ntags missing\nquery\n"),
    ("sort-and-union-count", "set z b a\nset aa c b\nset a b\nquery b\ntags z\ndrop aa\n"),
],
"T09": [
    ("empty-file", ""),
    ("empty-combinations", "\n\n\na\n*\n\n?\n\n"),
    ("full-match", "a\ncat\na*\nba\n*a\nab\n"),
    ("literal-wildcards", "\\*\\?\n*?\n\\*\\?\nxx\n"),
    ("backslash-and-trailing", "\\\\\n\\\nabc\\\nabc\n"),
    ("ordinary-escape", "\\a\\b\nab\n\\a\n\\a\n"),
    ("multiple-stars", "***a**b***\naaab\n**?**?**\na\n**?**?**\nab\n"),
    ("spaces-punctuation", "[x].*\n[x]. abc\n? ?\na b\n"),
],
"T10": [
    ("no-queries", "/a\n"),
    ("empty-query", "/a/b\n\n"),
    ("root-clamp", "/a\n../../../\n/../../..\n"),
    ("absolute-resets", "/a/b\n/x/y\n/x/../z\n"),
    ("dot-literals", "/a\n.../.hidden/..hidden/..\n./.\n"),
    ("independent-queries", "/a/b\n../x\n../y\n\n"),
    ("repeated-slashes", "/\n////a////b////\n////\n"),
    ("cancel-and-reenter", "/a/b/c\n../../../../d/../e\nq/./../r\n"),
],
"T11": [
    ("empty-sequence", "1\n"),
    ("shorter-than-window", "5\n1 2 3 4\n"),
    ("single-window", "3\n8 -2 4\n"),
    ("even-lower-median", "4\n1 2 8 9 10\n"),
    ("duplicates-leaving", "3\n2 2 2 1 1 3 3\n"),
    ("large-sum", "4\n1000000000 1000000000 1000000000 1000000000 -1000000000\n"),
    ("size-one", "1\n-2 0 5 -1\n"),
    ("multiline-whitespace", "2\n\n1\t3\n -5 7\n"),
],
"T12": [
    ("empty", ""),
    ("release-after-prerelease", "1.0.0\n1.0.0-z\n1.0.0-0\n"),
    ("numeric-vs-lexical", "1.0.0-10\n1.0.0-2\n1.0.0-a\n1.0.0-01\n"),
    ("prefix-length", "1.0.0-a.1\n1.0.0-a\n1.0.0-a.1.z\n"),
    ("stable-equivalence", "0001.0.0+second\n1.00.0+first\n1.0.00\n1.0.0-01+b\n1.0.0-1+a\n"),
    ("large-numeric-prerelease", "1.0.0-99999999999999999999999999999999\n1.0.0-10000000000000000000000000000000\n1.0.0-00000000000000000000000000000001\n"),
    ("invalid-grammar", "\n 1.0.0\n1.0.0 \n1.0\n1.0.0-\n1.0.0+\n1.0.0-a..b\n1.0.0+a+b\n1.0.0_a\n"),
    ("core-boundary-case-hyphen", "1000000000.0.0\n1000000001.0.0\n1.0.0-a-1\n1.0.0-Z\n1.0.0-a\n"),
],
}


def randomized_cases() -> dict[str, list[tuple[str, str]]]:
    result = {task: [] for task in SPECS}
    for repeat in range(4):
        rng = random.Random(SEED + repeat)
        rows = [f"account {key} {rng.randrange(30)}" for key in "abcd"]
        for _ in range(25):
            if rng.randrange(3) == 0:
                count = rng.randrange(5)
                rows.append(f"batch {count}")
                rows += [f"{rng.choice('abcdx')} {rng.choice('abcdx')} {rng.randrange(20)}" for _ in range(count)]
            else:
                rows.append(f"pay {rng.choice('abcdx')} {rng.choice('abcdx')} {rng.randrange(20)}")
        result["T01"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = []
        for _ in range(25):
            fields = ["".join(rng.choice('ab, "|') for _ in range(rng.randrange(8))) for _ in range(1 + rng.randrange(5))]
            row = ",".join('"' + f.replace('"', '""') + '"' if any(x in f for x in ',"') or rng.randrange(2) else f for f in fields)
            if rng.randrange(4) == 0:
                row += '"bad'
            rows.append(row)
        result["T02"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = []
        for _ in range(45):
            cmd = rng.choice(["add", "del", "has", "show"])
            if cmd in ("add", "del"):
                a, b = sorted([rng.randrange(-40, 41), rng.randrange(-40, 41)])
                rows.append(f"{cmd} {a} {b}")
            elif cmd == "has":
                rows.append(f"has {rng.randrange(-45, 46)}")
            else:
                rows.append("show")
        result["T03"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = [f"{'-' if rng.randrange(2) else ''}{rng.randrange(1000)}.{rng.randrange(1000):03} {rng.choice(list(FACTORS))} {rng.choice(list(FACTORS))}" for _ in range(25)]
        result["T04"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        h, w = 5 + repeat, 9
        rows = [f"{h} {w}"] + ["".join(rng.choice("..##") for _ in range(w)) for _ in range(h)]
        result["T05"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        text = "".join(rng.choice("abc $\\") * (1 + rng.randrange(12)) for _ in range(20))
        result["T06"].append((f"seed-{SEED + repeat}", text + "\n"))
        now = 0
        rows = [str(1 + repeat)]
        for _ in range(50):
            now += rng.randrange(3)
            cmd = rng.choice(["put", "put", "get", "keys"])
            if cmd == "put":
                rows.append(f"{now} put {rng.choice('abcdef')} {rng.randrange(-20, 21)} {rng.randrange(15)}")
            elif cmd == "get":
                rows.append(f"{now} get {rng.choice('abcdef')}")
            else:
                rows.append(f"{now} keys")
        result["T07"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = []
        for _ in range(45):
            cmd = rng.choice(["set", "add", "remove", "drop", "tags", "query"])
            name = rng.choice(["a", "b", "c", "aa", "zz"])
            if cmd == "set":
                rows.append("set " + name + " " + " ".join(rng.choices(["red", "blue", "green"], k=rng.randrange(5))))
            elif cmd in ("add", "remove"):
                rows.append(f"{cmd} {name} {rng.choice(['red', 'blue', 'green'])}")
            elif cmd in ("drop", "tags"):
                rows.append(f"{cmd} {name}")
            else:
                rows.append("query " + " ".join(rng.choices(["red", "blue", "green"], k=rng.randrange(4))))
        result["T08"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = []
        for _ in range(30):
            rows += ["".join(rng.choice("ab *?\\") for _ in range(rng.randrange(12))), "".join(rng.choice("ab *?\\") for _ in range(rng.randrange(10)))]
        result["T09"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        rows = ["/root/base"]
        for _ in range(30):
            rows.append(("/" if rng.randrange(3) == 0 else "") + "/".join(rng.choice([".", "..", "", "a", "b", "...", ".x"]) for _ in range(rng.randrange(8))))
        result["T10"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
        values = [str(rng.randrange(-20, 21)) for _ in range(40)]
        result["T11"].append((f"seed-{SEED + repeat}", f"{2 + repeat}\n" + " ".join(values) + "\n"))
        rows = []
        for _ in range(35):
            version = f"{rng.randrange(3)}.{rng.randrange(3)}.{rng.randrange(3)}"
            if rng.randrange(3):
                version += "-" + ".".join(rng.choice(["1", "01", "2", "10", "a", "A", "beta", "x-1"]) for _ in range(1 + rng.randrange(3)))
            if rng.randrange(3) == 0:
                version += "+" + rng.choice(["a", "b", "001"])
            rows.append(version)
        result["T12"].append((f"seed-{SEED + repeat}", "\n".join(rows) + "\n"))
    return result


def self_check() -> None:
    """Hand-calculated anchors plus alternate small-domain oracle checks."""
    assert t01("") == "total: 0\n"
    assert t01("account a 4\naccount b 0\nbatch 2\na b 3\na b 2\n") == "batch: row 2 funds a\na=4\nb=0\ntotal: 4\n"
    assert csv_record('a,"b,c",') == ["a", "b,c", ""]
    assert csv_record('"x" ') is None
    assert csv_record('""""') == ['"']
    assert t03("add 1 3\ndel 2 2\nshow\n") == "1:1 3:3\ncount: 2\n"
    assert t04("0.125 mm cm\n-0.000 km m\n") == "1: 1/80 cm\n2: 0 m\n"
    assert t05("3 3\n###\n#.#\n###\n") == "components: 1\n1: area=8 perimeter=16 box=0,0:2,2\nlargest: 1\n"
    assert t06("aa bb\n") == "97 2\n32 1\n98 2\nruns: 3 chars: 5\n"
    assert t07("1\n0 put a 2 1\n1 get a\n") == "a: missing\nkeys: none\n"
    assert t08("set a\nquery\n") == "docs: a\ntotal: docs=1 tags=0\n"
    assert t09("a\\*\na*\n?\n\n") == "1: yes\n2: no\n"
    assert t10("/a/b\n../../..\n.x\n") == "/\n/a/b/.x\n"
    assert t11("4\n1 3 7 9\n") == "4: median=3 sum=20\nwindows: 1\n"
    assert t12("1.0.0-01\n1.0.0-1\n1.0.0\n") == "1.0.0-01\n1.0.0-1\n1.0.0\nvalid: 3 invalid: 0\n"
    # Independent enumeration for interval mutation in a small bounded domain.
    rng = random.Random(SEED)
    for _ in range(50):
        selected = set()
        commands, expected = [], []
        for _ in range(25):
            a, b = sorted([rng.randrange(-15, 16), rng.randrange(-15, 16)])
            if rng.randrange(2):
                commands.append(f"add {a} {b}")
                selected.update(range(a, b + 1))
            else:
                commands.append(f"del {a} {b}")
                selected.difference_update(range(a, b + 1))
            x = rng.randrange(-16, 17)
            commands.append(f"has {x}")
            expected.append("yes" if x in selected else "no")
        expected.append(f"count: {len(selected)}")
        assert t03("\n".join(commands) + "\n") == "\n".join(expected) + "\n"
    # Independent dynamic programming for glob matching against regex oracle.
    for _ in range(200):
        p = "".join(rng.choice("ab*?") for _ in range(rng.randrange(8)))
        s = "".join(rng.choice("ab") for _ in range(rng.randrange(8)))
        cells = [[False] * (len(s) + 1) for _ in range(len(p) + 1)]
        cells[0][0] = True
        for i, ch in enumerate(p, 1):
            for j in range(len(s) + 1):
                if ch == "*":
                    cells[i][j] = cells[i - 1][j] or (j > 0 and cells[i][j - 1])
                elif j > 0:
                    cells[i][j] = cells[i - 1][j - 1] and (ch == "?" or ch == s[j - 1])
        assert bool(re.fullmatch(glob_regex(p), s)) == cells[-1][-1]


def encode_json(obj) -> bytes:
    return (json.dumps(obj, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main() -> None:
    self_check()
    random_cases = randomized_cases()
    tasks, public_tasks = [], []
    markdown = [COMMON.rstrip()]
    for task, (title, spec) in SPECS.items():
        structural = []
        if task == "T04":
            structural = ["At least two source files; all numeric parsing, unit validation, exact conversion and fraction reduction occur in a non-entry module; entry performs I/O and calls that module."]
        elif task == "T06":
            structural = ["Three OS worker threads are all started before any join; each encodes its prescribed contiguous chunk, including empty chunks; main only merges ordered worker runs and does not re-encode the complete input."]
        base = {"id": task, "kind": "program", "title": title, "structural": structural}
        cases = []
        for index, data in enumerate(PUBLIC[task], 1):
            cases.append({"id": f"{task}-public-{index:02}", "visibility": "public", "stdin": data, "stdout": ORACLES[task](data), "stderr": "", "exit_code": 0, "args": [], "files": {}})
        public = dict(base, cases=list(cases))
        public_tasks.append(public)
        for index, (name, data) in enumerate(HIDDEN[task] + random_cases[task], 1):
            cases.append({"id": f"{task}-hidden-{index:02}", "visibility": "hidden", "purpose": name, "stdin": data, "stdout": ORACLES[task](data), "stderr": "", "exit_code": 0, "args": [], "files": {}})
        tasks.append(dict(base, cases=cases))
        (ROOT / "public" / f"{task}.json").write_bytes(encode_json({"schema_version": 1, "tasks": [public]}))
        task_markdown = [f"## {task} {title}\n", spec.rstrip()]
        for index, case in enumerate(public["cases"], 1):
            task_markdown += [f"\n### 公开样例 {index}\n", "输入：\n\n```text\n" + case["stdin"] + "```", "\n输出：\n\n```text\n" + case["stdout"] + "```"]
        (ROOT / "public" / f"{task}.md").write_text(COMMON.rstrip() + "\n\n" + "\n".join(task_markdown) + "\n", encoding="utf-8")
        markdown += ["\n" + "\n".join(task_markdown)]
    packet = {"schema_version": 1, "seed": SEED, "tasks": tasks}
    public_packet = {"schema_version": 1, "tasks": public_tasks}
    (ROOT / "TASKS.md").write_text("\n".join(markdown) + "\n", encoding="utf-8")
    (ROOT / "public" / "cases.json").write_bytes(encode_json(public_packet))
    (ROOT / "judge" / "cases.json").write_bytes(encode_json(packet))
    paths = [ROOT / "TASKS.md", ROOT / "judge" / "generate.py", ROOT / "judge" / "cases.json"] + sorted(p for p in (ROOT / "public").iterdir() if p.is_file())
    manifest = {"schema_version": 1, "seed": SEED, "task_count": len(tasks), "public_case_count": sum(len(t["cases"]) for t in public_tasks), "hidden_case_count": sum(c["visibility"] == "hidden" for t in tasks for c in t["cases"]), "freeze_policy": "Generate and hash before any participant receives the packet. Give workers only TASKS.md and public/. Keep judge/ and this manifest unavailable to participants until all four runs are sealed; hidden outcomes never feed repairs. Freeze time and compiler/runner identity belong to the orchestration receipt.", "files": [{"path": str(p.relative_to(ROOT)), "sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size} for p in paths]}
    (ROOT / "packet-manifest.json").write_bytes(encode_json(manifest))
    print(json.dumps({k: manifest[k] for k in ("task_count", "public_case_count", "hidden_case_count", "seed")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
