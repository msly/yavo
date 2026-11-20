# 任务 T01–T10

每题写一个命令行程序。没有特别说明时：标准输出要与 `tasks/TNN.out` 逐字节相同，不检查标准错误，退出码为 0。
“行”不含换行符；输入文件以换行结尾。输出每行以换行结尾。

## T01 列统计
运行：`TNN < tasks/T01.in`
第一行是逗号分隔的列名，之后每行是同样个数的逗号分隔的值（值不含逗号，可能为空字符串）。
一列中所有非空值都能解析为十进制数（如 `3`、`-2`、`4.25`）时，该列是数值列，否则是文本列。
按列名顺序，每列输出一行：
- 数值列：`列名: min=A max=B mean=C`，三个数都保留两位小数（平均值 = 非空值之和 / 非空值个数）。
- 文本列：`列名: text (K distinct)`，K 是不同的非空值个数。

## T02 日志汇总
运行：`TNN < tasks/T02.in`
每行形如 `HH:MM:SS LEVEL 消息…`（字段以空白分隔）。少于两个字段的行跳过。消息是第三个字段起的各字段用单个空格连接（可能为空）。
输出：
1. 每个 LEVEL 一行 `LEVEL 次数`，按次数从多到少，次数相同按 LEVEL 字典序。
2. `first error: 消息`，取第一条 LEVEL 为 `ERROR` 的行；没有则 `first error: none`。
3. `busiest hour: HH (N)`：行数最多的小时（时间的前两个字符），并列取较小的小时。
4. `skipped: N`：跳过的行数。

## T03 账户
运行：`TNN < tasks/T03.in`
每行一条命令，字段以空白分隔：
- `open NAME`：开户，余额 0；已存在输出 `error: NAME exists`。
- `deposit NAME AMT`、`withdraw NAME AMT`、`transfer FROM TO AMT`。
检查顺序：命令名未知或字段个数不对 → `error: bad command`；AMT 不是正整数 → `error: bad amount 'AMT'`；
账户不存在 → `error: no account NAME`（transfer 先查 FROM 再查 TO）；transfer 的 FROM 与 TO 相同 → `error: same account`；
余额不足（withdraw，或 transfer 的 FROM）→ `error: insufficient funds in NAME`。出错的命令不改变任何余额。
空行跳过。最后按账户名字典序每行输出 `NAME BALANCE`，再输出 `total: 所有余额之和`。

## T04 矩阵乘法
运行三次：`TNN < tasks/T04a.in`、`TNN < tasks/T04b.in`、`TNN < tasks/T04c.in`。
输入是矩阵 A 的各行、一个空行、矩阵 B 的各行；每行是空格分隔的整数。
- 任一矩阵各行长度不同：标准错误输出 `error: ragged matrix`，退出码 1，无标准输出。
- A 的列数不等于 B 的行数：标准错误输出 `error: cannot multiply RxC by RxC`（A 的行数x列数、B 的行数x列数），退出码 1，无标准输出。
- 否则输出乘积，每行元素以单个空格分隔，退出码 0。
期望：`tasks/T04a.out`；T04b、T04c 的标准错误分别是 `tasks/T04b.err`、`tasks/T04c.err`。

## T05 依赖排序
运行两次：`TNN < tasks/T05a.in`、`TNN < tasks/T05b.in`。
每行 `任务: 依赖1 依赖2 …`（冒号后可以没有依赖）。只作为依赖出现的名字也是任务。
输出一个顺序，每个任务排在它所有依赖之后；每一步在所有可以排的任务里取字典序最小的。每行一个任务。
如果存在环，先照常输出能排的任务，然后输出 `cycle: ` 加上排不进去的任务（字典序、空格分隔），退出码 1。
期望：`tasks/T05a.out`（退出码 0）、`tasks/T05b.out`（退出码 1）。

## T06 游程编码
运行：`TNN encode < tasks/T06.in`，与 `tasks/T06.enc` 比较；`TNN decode < tasks/T06.enc`，与 `tasks/T06.in` 比较；
`TNN decode < tasks/T06bad.in`：无标准输出，标准错误为 `tasks/T06bad.err`，退出码 1。
encode：把整个标准输入（含换行）中相同字节的每段最长连续重复写成“十进制次数 + 该字节”，如 `aaab\n` → `3a1b1\n`（最后的 `\n` 是那个字节本身）。输入保证不含数字字符。
decode：上面的逆过程。出错时标准错误输出 `invalid input at byte N`（N 从 0 开始），退出码 1：
某个非数字字节前面没有次数 → N 是该字节的位置；次数为 0，或输入在次数之后结束 → N 是该次数第一个数字的位置。

## T07 罗马数字
运行：`TNN 1994 MCMXCIV 4 IIII 0 3999 abc MMMM XLII`
每个参数输出一行。参数是十进制整数：在 1–3999 内输出 `参数 -> 罗马数字`，否则 `参数: invalid`。
否则把参数当作罗马数字：只有大写 `IVXLCDM` 组成、且是该数值的标准写法（即再转回罗马数字得到相同字符串，值在 1–3999）时输出 `参数 -> 十进制`，否则 `参数: invalid`。

## T08 图形
运行：`TNN < tasks/T08.in`
每行是 `circle R`、`rect W H` 或 `triangle A B C`，参数是十进制数。对第 N 行（从 1 开始）：
- 形状名未知、参数个数不对或参数不是数：输出 `line N: cannot parse`。
- 三角形不满足任两边之和大于第三边：输出 `triangle: invalid`。
- 否则输出 `形状名: 面积`（两位小数；圆用 π·R²，三角形用海伦公式）。
最后输出 `total: 有效图形面积之和`（两位小数）和 `largest: 形状名`（面积最大者，并列取先出现的；没有有效图形则 `largest: none`）。

## T09 目录大小
运行：`TNN < tasks/T09.in`
每行 `路径 大小`，路径以 `/` 分隔各级目录，最后一段是文件名；同一路径出现多次时大小相加。输出一棵树：
第一行 `/ 总大小`；然后深度优先列出每个目录的内容，每深一级多缩进两个空格（根的直接子项缩进两个空格）。
同一目录下的子项按名字（字节序）排序，目录与文件一起排；目录写成 `名字/ 目录总大小`，文件写成 `名字 大小`。

## T10 最近最少使用缓存
运行：`TNN 3 < tasks/T10.in`（参数是容量）
每行 `put K V` 或 `get K`。`get` 命中输出值，否则输出 `miss`；命中的键变为最近使用。
`put` 写入或更新值，并使该键变为最近使用；写入新键后超过容量时，淘汰最久未使用的键并输出 `evict K`。
最后输出 `hits: H misses: M` 和 `order: ` 加上缓存中的键（从最近到最久，空格分隔）。
