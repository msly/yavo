#!/usr/bin/env python3
"""Deterministic R6 document-treatment task packet; no participant code is used."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import date, timedelta
from functools import lru_cache
import hashlib
import itertools
import json
from pathlib import Path
import random
import re

ROOT = Path(__file__).resolve().parents[1]
SEED = 2026112206

COMMON = """# 第六轮文档对照任务 T01–T08

这八题都要求新写 Yavo 程序。每题从标准输入读取，标准输出逐字节比较，标准错误为空、退出码为 0。隐藏用例在全部会话封卷后才执行，不能用于修复。

- 每题独立目录，入口为 `main.yv`；额外模块也放在本题目录内。不得调用其他语言、外部进程或第三方依赖。
- 输入只含 ASCII，不含 NUL 或 CR。非空输入以 LF 结束；一行不含 LF。除明确按整行处理的题，字段由一个或多个空格/制表符分隔，行首尾可有这些空白。
- 输出每行以 LF 结束。整数按普通十进制输出，不保留前导零、正号或负零。所有数值运算都在有符号 64 位范围内。
- 只需处理各题规定范围内的输入；某题明确要求校验无效行时，必须执行该校验。可自由选择符合题意的容器和算法。
- T04 的真实模块分工、T06 的真实线程分工也是整题成功条件；仅输出正确不够。
- 每题两个公开输入；`public/cases.json` 保存同样的精确输入、输出和退出状态。不要读取裁判、其他受试者或此前轮次的文件。
"""

SPECS = {
"T01": ("环形淘汰", """第一行是 `N K`，0 ≤ N ≤ 60，1 ≤ K ≤ 10^9；随后恰有 N 行不同的名字，匹配 `[a-z][a-z0-9_]{0,15}`。

这些名字按输入顺序围成圆环。从第一个名字开始数 1，沿输入顺序循环数到 K，移除该人并输出 `out: NAME`；从被移除者的下一个仍在环中的人重新数 1。重复直到最多剩一人。最后输出 `winner: NAME`，没有任何人则输出 `winner: none`。只有一个人时直接胜出，不先淘汰。K 可以远大于当前人数。
"""),
"T02": ("严格百分号编解码", r"""逐行处理，最多 100 行，每行最多 300 字节。有效命令必须以恰好 `enc ` 或 `dec `（含后面的一个空格）开始；其后的全部字符是文本，保留空格。文本只含可打印 ASCII（32..126），可以为空；其他前缀或空行无效。

- `enc TEXT`：ASCII 字母、数字及 `-._~` 原样保留；其他字节替换为 `%HH`，HH 为两位大写十六进制。
- `dec TEXT`：`%` 后必须恰有可解析的两个十六进制数字，接受大小写；每个转义只消费这两个数字。解码得到的字节必须在 32..126 内，否则整行无效。未转义字符原样保留，`+` 就是加号。只解码一遍，不再次解释解码得到的 `%`。

有效行输出 `K: len=L text=RESULT`，K 是从 1 开始的输入行号，L 是结果的字节数。无效行输出 `K: invalid`。空文件无输出。
"""),
"T03": ("稀疏多项式", """第一行是 `N M`，0 ≤ N,M ≤ 40；随后 N 行是多项式 A 的项，再随后 M 行是多项式 B 的项。每项为 `COEFFICIENT EXPONENT`，系数在 -10^6..10^6，指数在 0..30。同一多项式可有重复指数和零系数，输入顺序任意。

依次输出三行 `sum: RESULT`、`product: RESULT`、`derivative: RESULT`，分别是 A+B、A×B、A 对变量的一阶形式导数。合并同指数的项，删除系数为零的项，按指数从大到小输出 `EXPONENT:COEFFICIENT`，项之间一个空格。零多项式的 RESULT 为 `0`，非零常数项也写 `0:COEFFICIENT`。不能按输入顺序保留重复项。
"""),
"T04": ("公历日期运算（多文件）", r"""至少两个源文件。命令字段解析、日期校验、日期加减和日差计算都在非入口模块；入口文件只读输入、调用模块和输出。

逐行处理，最多 100 行，每行最多 120 字节。字段由空格/制表符分隔；日期必须严格匹配四位年、两位月、两位日的 `YYYY-MM-DD`。年份范围 1600..2400，日期遵循公历：被 4 整除的是闰年，但被 100 整除且不能被 400 整除的年份不是闰年。

- `add DATE DELTA`：DELTA 语法为 `-?[0-9]+`，最多 16 字节、绝对值 ≤ 10^6，允许前导零和负零，不允许 `+`。输出把 DATE 加上 DELTA 天后的日期；结果超出 1600-01-01..2400-12-31 则无效。
- `diff DATE1 DATE2`：输出 DATE2 减 DATE1 的天数，可以为负。

有效行输出 `K: RESULT`；日期结果仍为补零的 YYYY-MM-DD，日差为普通整数。未知命令、字段数、日期或 DELTA 任一无效都输出 `K: invalid`，空行也无效。K 从 1 开始；空文件无输出。
"""),
"T05": ("有界找零", """第一行是 `TARGET N`，0 ≤ TARGET ≤ 400，0 ≤ N ≤ 8；随后 N 行为 `VALUE COUNT`。各 VALUE 不同、范围 1..100，COUNT 为 0..10；面额顺序任意。

从每种面额最多取 COUNT 枚，计算恰好凑成 TARGET 的组合数 W，以及所有可行组合中的最少硬币数 M。每种面额的取用数量决定一个组合，取硬币的先后顺序不产生新组合。输出 `ways: W` 和 `min: M` 两行；没有组合时第二行是 `min: none`。TARGET=0 时空组合算一种，最少硬币数为 0。组合数在有符号 64 位范围内。
"""),
"T06": ("双线程质因数分解", """第一个字段为 N，0 ≤ N ≤ 100；后面恰有 N 个整数，范围 1..10^9，可分布在多行。

必须启动两个 OS 工作线程，并在等待任一线程前启动完两个。第一个线程负责原序列从 0 开始的偶数索引，第二个负责奇数索引；空分片也必须启动对应线程。每个线程实际分解自己收到的整数并返回质因数及重数；主线程只负责输入、分配、收集、按原顺序输出，不能重新做质因数分解或让一个线程完成全部工作。

每个输入数输出 `K: N=FACTORS`，K 从 1 开始。质因数升序，每个都写成 `PRIME^COUNT`，以 `*` 连接，即使重数为 1 也保留 `^1`。整数 1 的 FACTORS 为 `1`。最后输出 `count: N`；空序列只有该行。
"""),
"T07": ("矩形并集面积与周长", """第一行是矩形个数 N，0 ≤ N ≤ 60；随后 N 行是 `X1 Y1 X2 Y2`。坐标是 -1000..1000 的整数，X1 ≤ X2、Y1 ≤ Y2；矩形边平行于坐标轴，使用连续平面几何面积和长度。宽或高为 0 的矩形没有面积，完全忽略。

输出全部非退化矩形并集的面积和边界总长度两行：`area: A`、`perimeter: P`。重叠部分只计算一次；相邻矩形的共同边若位于并集内部，不计入周长；孔洞边界计入周长；只在角点相接不会减去边长。重复矩形不重复计数。没有非退化矩形时两个结果均为 0。
"""),
"T08": ("井字棋最优一步", """逐行输入棋盘，最多 20 行，每行最多 20 个可打印 ASCII 字符；不修剪空白。有效棋盘恰有 9 个字符，只能为 `X`、`O`、`.`，按行优先表示 3×3 棋盘。空文件无输出。

X 先走，双方严格轮流在一个空格落子；同行、同列或两条对角线有三个相同棋子时该方立即获胜，之后不能继续落子。没有胜者且全满为和棋。只有能从空棋盘遵循这些规则到达的棋盘才有效。

- 无效棋盘输出 `K: invalid`。
- 已结束的有效棋盘输出 `K: terminal X`、`K: terminal O` 或 `K: terminal draw`。
- 未结束时，双方之后都选择对自己最有利的走法：获胜优于和棋，和棋优于失败。输出 `K: play SIDE POSITION OUTCOME`，SIDE 为轮到的 X/O，POSITION 是 1..9 的行优先格号；有多个同样最优的位置时选最小格号，不再比较赢得快慢。OUTCOME 为从当前落子方视角的 `win`、`draw` 或 `lose`。

K 为从 1 开始的输入行号。空行无效。
"""),
}


def t01(data: str) -> str:
    fields = data.split()
    n, k = map(int, fields[:2])
    circle = fields[2:]
    assert len(circle) == n
    cursor, lines = 0, []
    while len(circle) > 1:
        cursor = (cursor + k - 1) % len(circle)
        lines.append("out: " + circle.pop(cursor))
    lines.append("winner: " + (circle[0] if circle else "none"))
    return "\n".join(lines) + "\n"


SAFE = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
HEX = frozenset("0123456789abcdefABCDEF")


def percent(text: str, decode: bool) -> str | None:
    if not decode:
        return "".join(c if c in SAFE else f"%{ord(c):02X}" for c in text)
    out, i = [], 0
    while i < len(text):
        c = text[i]
        if c == "%":
            if i + 2 >= len(text) or any(h not in HEX for h in text[i + 1:i + 3]):
                return None
            value = int(text[i + 1:i + 3], 16)
            if not 32 <= value <= 126:
                return None
            c, i = chr(value), i + 2
        out.append(c)
        i += 1
    return "".join(out)


def t02(data: str) -> str:
    lines = []
    for index, row in enumerate(data.splitlines(), 1):
        valid = row.startswith("enc ") or row.startswith("dec ")
        result = percent(row[4:], row[:3] == "dec") if valid else None
        lines.append(f"{index}: invalid" if result is None else f"{index}: len={len(result)} text={result}")
    return "".join(x + "\n" for x in lines)


def polynomial(terms):
    result = Counter()
    for coefficient, exponent in terms:
        result[exponent] += coefficient
    return {e: c for e, c in result.items() if c}


def show_poly(poly):
    return " ".join(f"{e}:{poly[e]}" for e in sorted(poly, reverse=True) if poly[e]) or "0"


def t03(data: str) -> str:
    fields = list(map(int, data.split()))
    n, m = fields[:2]
    terms = list(zip(fields[2::2], fields[3::2]))
    assert len(terms) == n + m
    a, b = polynomial(terms[:n]), polynomial(terms[n:])
    summed = polynomial([(c, e) for p in (a, b) for e, c in p.items()])
    product = polynomial([(ca * cb, ea + eb) for ea, ca in a.items() for eb, cb in b.items()])
    derivative = {e - 1: e * c for e, c in a.items() if e}
    return f"sum: {show_poly(summed)}\nproduct: {show_poly(product)}\nderivative: {show_poly(derivative)}\n"


LOW_DATE, HIGH_DATE = date(1600, 1, 1), date(2400, 12, 31)


def parse_date(text):
    if re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", text) is None:
        raise ValueError("date grammar")
    parsed = date(*map(int, text.split("-")))
    if not LOW_DATE <= parsed <= HIGH_DATE:
        raise ValueError("date range")
    return parsed


def t04(data: str) -> str:
    out = []
    for i, row in enumerate(data.splitlines(), 1):
        try:
            fields = row.split()
            if len(fields) != 3:
                raise ValueError("field count")
            op, a0, b0 = fields
            a = parse_date(a0)
            if op == "add":
                if len(b0) > 16 or re.fullmatch(r"-?[0-9]+", b0) is None:
                    raise ValueError("delta grammar")
                delta = int(b0)
                if abs(delta) > 10**6:
                    raise ValueError("delta range")
                result_date = a + timedelta(days=delta)
                if not LOW_DATE <= result_date <= HIGH_DATE:
                    raise ValueError("result range")
                result = result_date.isoformat()
            elif op == "diff":
                result = str((parse_date(b0) - a).days)
            else:
                raise ValueError("unknown command")
        except (ValueError, OverflowError):
            result = "invalid"
        out.append(f"{i}: {result}")
    return "".join(x + "\n" for x in out)


def coin_solve(target, coins):
    ways, best = [0] * (target + 1), [None] * (target + 1)
    ways[0], best[0] = 1, 0
    for value, count in coins:
        next_ways, next_best = [0] * (target + 1), [None] * (target + 1)
        for total in range(target + 1):
            for take in range(min(count, (target - total) // value) + 1):
                at = total + take * value
                next_ways[at] += ways[total]
                if best[total] is not None:
                    candidate = best[total] + take
                    if next_best[at] is None or candidate < next_best[at]:
                        next_best[at] = candidate
        ways, best = next_ways, next_best
    return ways[target], best[target]


def t05(data: str) -> str:
    fields = list(map(int, data.split()))
    target, n = fields[:2]
    coins = list(zip(fields[2::2], fields[3::2]))
    assert len(coins) == n
    ways, best = coin_solve(target, coins)
    return f"ways: {ways}\nmin: {best if best is not None else 'none'}\n"


def factors(value):
    result, prime = [], 2
    while prime * prime <= value:
        count = 0
        while value % prime == 0:
            value //= prime
            count += 1
        if count:
            result.append((prime, count))
        prime += 1 if prime == 2 else 2
    if value > 1:
        result.append((value, 1))
    return result


def t06(data: str) -> str:
    fields = list(map(int, data.split()))
    n, values = fields[0], fields[1:]
    assert n == len(values)
    out = []
    for index, value in enumerate(values, 1):
        encoded = "*".join(f"{p}^{count}" for p, count in factors(value)) or "1"
        out.append(f"{index}: {value}={encoded}")
    out.append(f"count: {n}")
    return "\n".join(out) + "\n"


def rectangle_solve(rectangles):
    rectangles = [(a, b, c, d) for a, b, c, d in rectangles if a < c and b < d]
    if not rectangles:
        return 0, 0
    xs = sorted({v for r in rectangles for v in (r[0], r[2])})
    ys = sorted({v for r in rectangles for v in (r[1], r[3])})
    cells = set()
    for i in range(len(xs) - 1):
        for j in range(len(ys) - 1):
            if any(a <= xs[i] < c and b <= ys[j] < d for a, b, c, d in rectangles):
                cells.add((i, j))
    area = perimeter = 0
    for i, j in cells:
        width, height = xs[i + 1] - xs[i], ys[j + 1] - ys[j]
        area += width * height
        perimeter += height * sum((i + step, j) not in cells for step in (-1, 1))
        perimeter += width * sum((i, j + step) not in cells for step in (-1, 1))
    return area, perimeter


def t07(data: str) -> str:
    fields = list(map(int, data.split()))
    n = fields[0]
    rectangles = list(zip(fields[1::4], fields[2::4], fields[3::4], fields[4::4]))
    assert len(rectangles) == n
    area, perimeter = rectangle_solve(rectangles)
    return f"area: {area}\nperimeter: {perimeter}\n"


LINES = ((0, 1, 2), (3, 4, 5), (6, 7, 8), (0, 3, 6), (1, 4, 7), (2, 5, 8), (0, 4, 8), (2, 4, 6))


def winner(board):
    for a, b, c in LINES:
        if board[a] != "." and board[a] == board[b] == board[c]:
            return board[a]
    return None


def reachable_boards():
    reached = set()
    def visit(board, side):
        if board in reached:
            return
        reached.add(board)
        if winner(board) or "." not in board:
            return
        for i, cell in enumerate(board):
            if cell == ".":
                visit(board[:i] + side + board[i + 1:], "O" if side == "X" else "X")
    visit(".........", "X")
    return frozenset(reached)


REACHABLE = reachable_boards()


@lru_cache(None)
def game_value(board):
    """Value from the side whose turn follows this board, terminal wins are -1."""
    if winner(board):
        return -1
    if "." not in board:
        return 0
    side = "X" if board.count("X") == board.count("O") else "O"
    return max(-game_value(board[:i] + side + board[i + 1:]) for i, c in enumerate(board) if c == ".")


def t08(data: str) -> str:
    out = []
    for index, board in enumerate(data.splitlines(), 1):
        if board not in REACHABLE:
            result = "invalid"
        elif winner(board):
            result = "terminal " + winner(board)
        elif "." not in board:
            result = "terminal draw"
        else:
            side = "X" if board.count("X") == board.count("O") else "O"
            candidates = [(-game_value(board[:i] + side + board[i + 1:]), i + 1) for i, c in enumerate(board) if c == "."]
            best = max(v for v, _ in candidates)
            position = min(i for v, i in candidates if v == best)
            result = f"play {side} {position} " + {-1: "lose", 0: "draw", 1: "win"}[best]
        out.append(f"{index}: {result}")
    return "".join(x + "\n" for x in out)


ORACLES = {f"T{i:02}": globals()[f"t{i:02}"] for i in range(1, 9)}


PUBLIC = {
"T01": ["5 2\na\nb\nc\nd\ne\n", "0 1000000000\n"],
"T02": ["enc a+b c\ndec a%2Bb%20c\nenc -._~\n", "enc \ndec %25%32%30\ndec %0A\ndec x%2\n"],
"T03": ["3 2\n2 2\n-2 2\n3 0\n1 1\n-3 0\n", "0 1\n5 0\n"],
"T04": ["add 2000-02-28 1\nadd 1900-02-28 1\ndiff 2000-03-01 2000-02-28\n", "add 1600-01-01 -1\nadd 2024-02-30 0\nadd 2024-01-01 -000\n"],
"T05": ["6 3\n1 2\n2 3\n5 1\n", "3 2\n2 1\n4 5\n"],
"T06": ["5\n1 12 17 36 100\n", "0\n"],
"T07": ["2\n0 0 2 2\n1 0 3 2\n", "3\n0 0 1 1\n1 1 2 2\n0 0 1 1\n"],
"T08": [".........\nXX.OO....\nXXXOO....\n", "OOOXX.X..\nXXXOOO...\nXOXOOXXXO\n"],
}


HIDDEN = {
"T01": [
    ("single-person", "1 1000000000\nsolo\n"),
    ("step-one", "4 1\nz\na\nm\nx\n"),
    ("huge-step", "7 999999937\na\nb\nc\nd\ne\nf\ng\n"),
    ("two-parity", "2 999999999\nleft\nright\n"),
    ("wrap-last-removal", "6 6\na\nb\nc\nd\ne\nf\n"),
    ("name-prefixes", "5 3\na\naa\na1\na_\nz0\n"),
    ("maximum-population", "60 1000000000\n" + "".join(f"p{i}\n" for i in range(60))),
    ("zero-population", "0 1\n"),
],
"T02": [
    ("empty-file", ""),
    ("space-plus-and-empty", "enc \ndec \nenc   \ndec +++\ndec %20%2b%2B\n"),
    ("safe-ascii", "enc AZaz09-._~\ndec AZaz09-._~\n"),
    ("invalid-escapes", "dec %\ndec %0\ndec %GG\ndec %1z\ndec x%20y%\n"),
    ("nonprintable-results", "dec %00\ndec %1F\ndec %7f\ndec %80\ndec %ff\ndec %20%7E\n"),
    ("single-pass", "dec %2520\ndec %2541\ndec %25%2b\n"),
    ("syntax-and-line-count", "\nENC a\nenc\n enc a\nenc  a \ndec a b\n"),
    ("punctuation-and-interpolation-text", "enc ${x} \\ \"%&?/\ndec %24%7Bx%7D\n"),
],
"T03": [
    ("both-empty", "0 0\n"),
    ("zero-terms", "2 2\n0 0\n0 30\n0 5\n0 5\n"),
    ("full-cancellation", "3 2\n2 7\n-1 7\n-1 7\n4 3\n-4 3\n"),
    ("constant-derivative", "2 1\n-5 0\n2 0\n3 0\n"),
    ("product-middle-cancel", "2 2\n1 1\n1 0\n1 1\n-1 0\n"),
    ("maximum-exponents", "2 2\n1000000 30\n1000000 30\n-1000000 30\n-1000000 0\n"),
    ("unordered-and-repeated", "4 4\n5 1\n2 3\n-2 1\n7 2\n-7 2\n3 0\n-2 3\n1 0\n"),
    ("maximum-term-count", "40 40\n" + "1000000 30\n" * 40 + "-1000000 30\n" * 40),
],
"T04": [
    ("empty-file", ""),
    ("century-leap-rules", "add 1600-02-28 1\nadd 1700-02-28 1\nadd 1900-02-28 1\nadd 2000-02-28 1\nadd 2100-02-28 1\nadd 2400-02-28 1\n"),
    ("range-ends", "add 1600-01-01 0\nadd 1600-01-01 -1\nadd 2400-12-31 0\nadd 2400-12-31 1\ndiff 1600-01-01 2400-12-31\n"),
    ("reverse-difference", "diff 2024-03-01 2024-02-28\ndiff 2023-03-01 2023-02-28\ndiff 2020-01-01 2020-01-01\n"),
    ("invalid-dates", "add 2000-00-01 1\nadd 2000-13-01 1\nadd 2000-04-31 1\nadd 1900-02-29 0\nadd 1599-12-31 1\nadd 2401-01-01 -1\n"),
    ("strict-grammar", "\nadd 2000-1-01 0\nadd 2000-01-1 0\nadd 2000/01/01 0\nadd 2000-01-01 +1\nadd 2000-01-01 1.0\ndiff 2000-01-01\nfoo 2000-01-01 0\n"),
    ("delta-bounds-and-zeroes", "add 2000-01-01 0000000000000000\nadd 2000-01-01 -000000000000000\nadd 2000-01-01 00000000000000000\nadd 2000-01-01 1000001\nadd 2000-01-01 1000000\n"),
    ("month-year-crossing-whitespace", " add\t1999-12-31\t1 \nadd 2000-03-01 -1\nadd 2001-03-01 -1\ndiff 1999-12-31 2001-01-01\n"),
],
"T05": [
    ("empty-coins-zero-target", "0 0\n"),
    ("empty-coins-positive-target", "1 0\n"),
    ("all-unavailable", "3 3\n1 0\n2 0\n3 0\n"),
    ("bounded-not-unbounded", "10 2\n2 2\n3 1\n"),
    ("combination-not-permutation", "4 2\n1 4\n2 2\n"),
    ("minimum-not-greedy", "6 3\n4 1\n3 2\n1 2\n"),
    ("unsorted-values", "12 4\n7 1\n1 5\n5 2\n3 4\n"),
    ("target-and-count-limits", "400 8\n1 10\n2 10\n3 10\n4 10\n5 10\n50 10\n99 10\n100 10\n"),
],
"T06": [
    ("one-item-empty-second-worker", "1\n1\n"),
    ("two-items", "2\n2 3\n"),
    ("prime-powers", "6\n1024 59049 390625 823543 214358881 1000000000\n"),
    ("large-primes-and-square", "4\n999999937 999999929 999950884 999999999\n"),
    ("repeated-values-preserve-index", "7\n12 12 1 12 1 12 12\n"),
    ("odd-split-whitespace", "5\n 6\t35\n77 143\n221\n"),
    ("many-distinct-primes", "8\n2 3 5 7 11 13 17 19\n"),
    ("maximum-length", "100\n" + " ".join(str(1 + i * 100003) for i in range(100)) + "\n"),
],
"T07": [
    ("empty", "0\n"),
    ("degenerate-only", "3\n0 0 0 5\n0 0 5 0\n-1 -1 -1 -1\n"),
    ("shared-full-edge", "2\n0 0 2 2\n2 0 4 2\n"),
    ("contained-and-duplicate", "3\n-5 -5 5 5\n-1 -1 1 1\n-5 -5 5 5\n"),
    ("partial-edge", "2\n0 0 2 4\n2 1 4 3\n"),
    ("hole-boundary", "4\n0 0 4 1\n0 3 4 4\n0 1 1 3\n3 1 4 3\n"),
    ("negative-and-limit-coordinates", "2\n-1000 -1000 0 1000\n0 -1000 1000 1000\n"),
    ("maximum-count-disjoint-strips", "60\n" + "".join(f"{3*i} 0 {3*i+2} {1+i%4}\n" for i in range(60))),
],
"T08": [
    ("empty-file", ""),
    ("empty-and-opening", ".........\nX........\n....X....\n........X\n"),
    ("winning-and-blocking", "XX.OO....\nOO.XX.X..\nX...O...X\nXO.XO....\n"),
    ("terminal-boards", "XXXOO....\nOOOXX.X..\nXOXOOXXXO\nXOXOXOXOX\n"),
    ("invalid-counts-and-dual-wins", "O........\nXX.......\nXXXOOO...\nOOOXX....\nXXXOO.O..\n"),
    ("invalid-text", "\n........\n..........\nXOXOXOxo.\n X.......\n......... \n....................\n"),
    ("late-forced-draws", "XOXOXO.OX\nXOXXOOOX.\nOXOXOX.X.\n"),
    ("tie-break-symmetric", "....X....\nX...O....\n..X.O....\n....O...X\n"),
],
}


def random_cases():
    results = {task: [] for task in SPECS}
    reachable = sorted(REACHABLE)
    for repetition in range(4):
        rng = random.Random(SEED + repetition)
        n, k = rng.randrange(8, 30), rng.randrange(1, 10**9 + 1)
        names = [f"name{i}" for i in range(n)]
        rng.shuffle(names)
        results["T01"].append((f"seed-{SEED + repetition}", f"{n} {k}\n" + "\n".join(names) + "\n"))
        rows = []
        for _ in range(20):
            text = "".join(chr(rng.randrange(32, 127)) for _ in range(rng.randrange(25)))
            rows.append("enc " + text)
            rows.append("dec " + percent(text, False))
        results["T02"].append((f"seed-{SEED + repetition}", "\n".join(rows) + "\n"))
        n, m = 10 + repetition, 12 + repetition
        terms = [(rng.randrange(-10, 11), rng.randrange(7)) for _ in range(n + m)]
        results["T03"].append((f"seed-{SEED + repetition}", f"{n} {m}\n" + "".join(f"{c} {e}\n" for c, e in terms)))
        rows = []
        for _ in range(25):
            a = LOW_DATE + timedelta(days=rng.randrange((HIGH_DATE - LOW_DATE).days + 1))
            if rng.randrange(2):
                rows.append(f"add {a.isoformat()} {rng.randrange(-90000, 90001)}")
            else:
                b = LOW_DATE + timedelta(days=rng.randrange((HIGH_DATE - LOW_DATE).days + 1))
                rows.append(f"diff {a.isoformat()} {b.isoformat()}")
        results["T04"].append((f"seed-{SEED + repetition}", "\n".join(rows) + "\n"))
        values = rng.sample(range(1, 21), 6)
        coins = [(v, rng.randrange(6)) for v in values]
        results["T05"].append((f"seed-{SEED + repetition}", f"{rng.randrange(40, 161)} 6\n" + "".join(f"{v} {c}\n" for v, c in coins)))
        values = [rng.randrange(1, 10**9 + 1) for _ in range(20 + repetition)]
        results["T06"].append((f"seed-{SEED + repetition}", f"{len(values)}\n" + " ".join(map(str, values)) + "\n"))
        rectangles = []
        for _ in range(18 + repetition):
            a, c = sorted([rng.randrange(-20, 21), rng.randrange(-20, 21)])
            b, d = sorted([rng.randrange(-20, 21), rng.randrange(-20, 21)])
            rectangles.append((a, b, c, d))
        results["T07"].append((f"seed-{SEED + repetition}", f"{len(rectangles)}\n" + "".join(" ".join(map(str, r)) + "\n" for r in rectangles)))
        boards = rng.sample(reachable, 12) + ["".join(rng.choices("XO.", k=9)) for _ in range(8)]
        results["T08"].append((f"seed-{SEED + repetition}", "\n".join(boards) + "\n"))
    return results


def self_check():
    checks = {}
    assert t01("5 2\na\nb\nc\nd\ne\n") == "out: b\nout: d\nout: a\nout: e\nwinner: c\n"
    for n in range(1, 61):
        for k in (1, 2, 3, 17, 10**9):
            survivor = 0
            for count in range(2, n + 1):
                survivor = (survivor + k) % count
            data = f"{n} {k}\n" + "".join(f"p{i}\n" for i in range(n))
            assert t01(data).splitlines()[-1] == f"winner: p{survivor}"
    checks["circle_against_recurrence"] = 300
    text = "".join(map(chr, range(32, 127)))
    assert percent(percent(text, False), True) == text
    assert percent("%2520", True) == "%20"
    assert percent("a+b", True) == "a+b"
    assert all(percent(x, True) is None for x in ("%", "%G0", "%0A", "%7f", "%ff"))
    checks["ascii_codec_roundtrip_bytes"] = 95
    rng = random.Random(SEED)
    for _ in range(100):
        a = [(rng.randrange(-5, 6), rng.randrange(6)) for _ in range(8)]
        b = [(rng.randrange(-5, 6), rng.randrange(6)) for _ in range(8)]
        source = "8 8\n" + "".join(f"{c} {e}\n" for c, e in a + b)
        result = t03(source).splitlines()
        def evaluate_shown(line, x):
            value = line.split(": ", 1)[1]
            if value == "0":
                return 0
            return sum(int(term.split(":")[1]) * x ** int(term.split(":")[0]) for term in value.split())
        for x in (-2, -1, 0, 1, 2):
            av, bv = (sum(c * x**e for c, e in terms) for terms in (a, b))
            assert evaluate_shown(result[0], x) == av + bv
            assert evaluate_shown(result[1], x) == av * bv
            assert evaluate_shown(result[2], x) == sum(c * e * x ** (e - 1) for c, e in a if e)
    checks["polynomial_evaluation_identities"] = 1500
    def manual_ordinal(y, m, d):
        prior = y - 1
        days = 365 * prior + prior // 4 - prior // 100 + prior // 400
        lengths = [31, 29 if y % 4 == 0 and (y % 100 != 0 or y % 400 == 0) else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        return days + sum(lengths[:m - 1]) + d
    for year in range(1600, 2401):
        for month in range(1, 13):
            assert date(year, month, 1).toordinal() == manual_ordinal(year, month, 1)
    assert t04("add 2000-02-28 1\nadd 1900-02-28 1\n") == "1: 2000-02-29\n2: 1900-03-01\n"
    checks["calendar_month_boundaries_against_manual_ordinal"] = 801 * 12
    for _ in range(150):
        coins = [(v, rng.randrange(4)) for v in rng.sample(range(1, 10), 4)]
        target = rng.randrange(25)
        candidates = [sum(takes) for takes in itertools.product(*(range(c + 1) for _, c in coins)) if sum(t * v for t, (v, _) in zip(takes, coins)) == target]
        assert coin_solve(target, coins) == (len(candidates), min(candidates) if candidates else None)
    checks["bounded_change_against_enumeration"] = 150
    for value in list(range(1, 1001)) + [999999937, 999999999, 10**9]:
        factors0 = factors(value)
        rebuilt = 1
        for p, count in factors0:
            assert p >= 2 and count > 0 and all(p % d for d in range(2, int(p**0.5) + 1))
            rebuilt *= p**count
        assert rebuilt == value
    checks["factorization_prime_product_checks"] = 1003
    for _ in range(200):
        rectangles = []
        selected = set()
        for _ in range(rng.randrange(8)):
            a, c = sorted([rng.randrange(-4, 5), rng.randrange(-4, 5)])
            b, d = sorted([rng.randrange(-4, 5), rng.randrange(-4, 5)])
            rectangles.append((a, b, c, d))
            selected.update(itertools.product(range(a, c), range(b, d)))
        perimeter = sum((x + dx, y + dy) not in selected for x, y in selected for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        assert rectangle_solve(rectangles) == (len(selected), perimeter)
    assert rectangle_solve([(0, 0, 4, 1), (0, 3, 4, 4), (0, 1, 1, 3), (3, 1, 4, 3)]) == (12, 24)
    checks["rectangle_compression_against_unit_cells"] = 200
    assert len(REACHABLE) == 5478
    assert game_value(".........") == 0
    assert t08(".........\nXX.OO....\nXXXOO....\n") == "1: play X 1 draw\n2: play X 3 win\n3: terminal X\n"
    # A count/winner predicate independently characterizes all reachable 3x3 boards.
    for cells in itertools.product("XO.", repeat=9):
        board = "".join(cells)
        x, o = board.count("X"), board.count("O")
        winners = {board[a] for a, b, c in LINES if board[a] != "." and board[a] == board[b] == board[c]}
        valid = x in (o, o + 1) and len(winners) <= 1 and ("X" not in winners or x == o + 1) and ("O" not in winners or x == o)
        assert (board in REACHABLE) == valid
    # Bottom-up graph evaluation is independent of recursive memoization.
    values = {}
    for board in sorted(REACHABLE, key=lambda b: b.count(".")):
        if winner(board):
            values[board] = -1
        elif "." not in board:
            values[board] = 0
        else:
            side = "X" if board.count("X") == board.count("O") else "O"
            values[board] = max(-values[board[:i] + side + board[i + 1:]] for i, c in enumerate(board) if c == ".")
        assert values[board] == game_value(board)
    checks["board_validity_exhaustive"] = 3**9
    checks["minimax_against_bottom_up_graph"] = len(REACHABLE)
    return {"status": "passed", "seed": SEED, "checks": checks}


def encode_json(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def validate_inputs(task, data):
    """Reject generated examples outside the published contract, before freezing."""
    assert not data or data.endswith("\n"), task
    assert "\r" not in data and "\x00" not in data and data.isascii(), task
    rows = data.splitlines()
    fields = data.split()
    if task == "T01":
        n, k = map(int, fields[:2])
        assert 0 <= n <= 60 and 1 <= k <= 10**9
        assert len(fields) == n + 2 and len(rows) == n + 1
        names = fields[2:]
        assert len(set(names)) == n and all(re.fullmatch(r"[a-z][a-z0-9_]{0,15}", x) for x in names)
    elif task in ("T02", "T04", "T08"):
        limits = {"T02": (100, 300), "T04": (100, 120), "T08": (20, 20)}
        row_count, width = limits[task]
        assert len(rows) <= row_count and all(len(row) <= width for row in rows)
        assert all(all(32 <= ord(c) <= 126 or (task == "T04" and c == "\t") for c in row) for row in rows)
    elif task == "T03":
        nums = list(map(int, fields))
        n, m = nums[:2]
        assert 0 <= n <= 40 and 0 <= m <= 40 and len(nums) == 2 + 2 * (n + m)
        assert all(-10**6 <= c <= 10**6 and 0 <= e <= 30 for c, e in zip(nums[2::2], nums[3::2]))
    elif task == "T05":
        nums = list(map(int, fields))
        target, n = nums[:2]
        assert 0 <= target <= 400 and 0 <= n <= 8 and len(nums) == 2 + 2 * n
        assert len(set(nums[2::2])) == n
        assert all(1 <= v <= 100 and 0 <= c <= 10 for v, c in zip(nums[2::2], nums[3::2]))
    elif task == "T06":
        nums = list(map(int, fields))
        assert 0 <= nums[0] <= 100 and len(nums) == nums[0] + 1
        assert all(1 <= value <= 10**9 for value in nums[1:])
    elif task == "T07":
        nums = list(map(int, fields))
        assert 0 <= nums[0] <= 60 and len(nums) == 1 + 4 * nums[0]
        assert all(-1000 <= v <= 1000 for v in nums[1:])
        assert all(a <= c and b <= d for a, b, c, d in zip(nums[1::4], nums[2::4], nums[3::4], nums[4::4]))
    else:
        raise AssertionError(task)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-check", action="store_true", help="only run independent oracle checks")
    args = parser.parse_args()
    receipt = self_check()
    if args.self_check:
        print(json.dumps(receipt, ensure_ascii=False))
        return
    random = random_cases()
    tasks, public_tasks, markdown = [], [], [COMMON.rstrip()]
    (ROOT / "public").mkdir(exist_ok=True)
    for task, (title, spec) in SPECS.items():
        structural = []
        if task == "T04":
            structural = ["At least two source files; command field parsing, calendar validation, date arithmetic and day differences occur in a non-entry module; entry only performs I/O and calls that module."]
        elif task == "T06":
            structural = ["Two real OS worker threads are both started before any join, including empty partitions; worker 0 factors original even-index values and worker 1 factors odd-index values; main only distributes, collects and prints in original order and does not factor values."]
        base = {"id": task, "kind": "program", "title": title, "structural": structural}
        cases = []
        for i, data in enumerate(PUBLIC[task], 1):
            validate_inputs(task, data)
            cases.append({"id": f"{task}-public-{i:02}", "visibility": "public", "stdin": data, "stdout": ORACLES[task](data), "stderr": "", "exit_code": 0, "args": [], "files": {}})
        public = dict(base, cases=list(cases))
        public_tasks.append(public)
        for i, (purpose, data) in enumerate(HIDDEN[task] + random[task], 1):
            validate_inputs(task, data)
            cases.append({"id": f"{task}-hidden-{i:02}", "visibility": "hidden", "purpose": purpose, "stdin": data, "stdout": ORACLES[task](data), "stderr": "", "exit_code": 0, "args": [], "files": {}})
        assert len(cases) == 14
        tasks.append(dict(base, cases=cases))
        (ROOT / "public" / f"{task}.json").write_bytes(encode_json({"schema_version": 1, "tasks": [public]}))
        task_md = [f"## {task} {title}\n", spec.rstrip()]
        for i, case in enumerate(public["cases"], 1):
            task_md += [f"\n### 公开样例 {i}\n", "输入：\n\n```text\n" + case["stdin"] + "```", "\n输出：\n\n```text\n" + case["stdout"] + "```"]
        (ROOT / "public" / f"{task}.md").write_text(COMMON.rstrip() + "\n\n" + "\n".join(task_md) + "\n", encoding="utf-8")
        markdown.append("\n" + "\n".join(task_md))
    (ROOT / "TASKS.md").write_text("\n".join(markdown) + "\n", encoding="utf-8")
    (ROOT / "public" / "cases.json").write_bytes(encode_json({"schema_version": 1, "tasks": public_tasks}))
    (ROOT / "judge" / "cases.json").write_bytes(encode_json({"schema_version": 1, "seed": SEED, "tasks": tasks}))
    paths = [ROOT / "TASKS.md", ROOT / "judge" / "generate.py", ROOT / "judge" / "cases.json"] + sorted(p for p in (ROOT / "public").iterdir() if p.is_file())
    manifest = {
        "schema_version": 1, "seed": SEED, "task_count": 8,
        "public_case_count": 16, "hidden_case_count": 96,
        "planned_sessions": 4, "planned_deliveries": 32,
        "freeze_policy": "Generate and hash before any participant launch. Launch additionally requires verified real usage, exact model identity and saved request records. Expose only participant docs and public inputs; seal every session before hidden judging. Root freeze receipt binds task packet, guide treatments, API topics, compiler and recorder.",
        "files": [{"path": str(p.relative_to(ROOT)), "sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size} for p in paths],
    }
    (ROOT / "packet-manifest.json").write_bytes(encode_json(manifest))
    print(json.dumps({"status": "generated", "task_count": 8, "public_case_count": 16, "hidden_case_count": 96, "packet_files": len(paths), "seed": SEED, "oracle_checks": receipt["checks"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
