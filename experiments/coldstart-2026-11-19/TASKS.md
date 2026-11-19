# Yavo 冷启动任务（T01–T10）

每题写一个 Yavo 程序 `work/TNN.yv`，用 `bin/yavo build work/TNN.yv -o work/TNN` 编译，再按“运行方式”运行，
把标准输出与 `tasks/TNN.out` 逐字节比较（`diff`）。没有特别说明时，不检查标准错误，退出码应为 0。
程序只能用速查卡（`cheatsheet.md`）里的写法。

## T01 词频
运行：`work/T01 < tasks/T01.in`
单词是连续的 ASCII 字母（a–z、A–Z），统一转成小写。按出现次数从多到少排序，次数相同按单词字典序；
只输出前 5 个，每行 `单词 次数`。

## T02 成绩汇总
运行：`work/T02 < tasks/T02.in`
每行 `姓名,分数`。空行跳过。逗号不是恰好一个、或分数不是整数的行，向标准错误输出 `bad line N: 原行`
（N 从 1 开始，空行也计入行号），然后继续。标准错误应与 `tasks/T02.err` 一致。
标准输出四行：`count: 有效行数`、`total: 分数和`、`mean_x100: M`、`best: 姓名`。
M 为平均分（用 f64 计算：总分除以行数）乘以 100.0 后转换为 int（向零截断）。best 为最高分的姓名，同分取先出现者。

## T03 查找行
运行：`work/T03 apples tasks/T03.txt`，输出应与 `tasks/T03.out` 一致（每个匹配行 `行号: 行内容`，行号从 1 开始，按字节子串匹配）。
另外两种情况：`work/T03 kiwi tasks/T03.txt` 不输出任何标准输出，向标准错误输出 `no match`，退出码 1；
`work/T03 apples missing.txt` 向标准错误输出 `cannot read missing.txt`，退出码 2。

## T04 逆波兰计算器
运行：`work/T04 < tasks/T04.in`
每行一个表达式，记号用空白分隔：整数，或运算符 `+ - * /`（整数除法向零截断）。对每行输出结果，或 `error: 原因`，原因是：
`bad token 'X'`（既不是整数也不是运算符）、`stack underflow`（运算时操作数不足，或行结束时栈为空）、
`division by zero`、`leftover values`（行结束时栈里不止一个值）。遇到第一个错误就停止处理这一行。
要求：用 `enum` 表示错误，求值函数返回 `int ! 你的错误类型`。

## T05 二叉搜索树
运行：`work/T05 50 30 70 20 40 60 80 30 65`
按参数顺序把整数插入二叉搜索树（重复值忽略），树必须用 `?Box[...]` 实现。输出三行：中序遍历（空格分隔）、
`height: H`（空树为 0，单个节点为 1）、`size: N`。

## T06 库存
运行：`work/T06 < tasks/T06.in`
命令：`add 名称 数量`、`remove 名称 数量`、`report`。remove 的数量超过库存（包括没有这个名称）时输出
`error: not enough 名称`，库存不变；数量正好减到 0 时删除该名称。`report` 按名称字典序输出每行 `名称 数量`。
其他命令输出 `error: unknown command 'X'`。

## T07 折行
运行：`work/T07 20 < tasks/T07.in`
把输入拆成单词（空白分隔），贪心地放入行中，单词之间一个空格，每行字节数不超过宽度（命令行参数）；
比宽度长的单词单独占一行。

## T08 图形（接口）
运行：`work/T08 < tasks/T08.in`
每行 `circle R`、`rect W H` 或 `square S`，数值是浮点数。定义接口 `Shape`（至少有 `area` 和 `name` 两个方法），
三种图形各自实现它，全部放进 `Vec[Box[Shape]]`。按面积从小到大排序（用 `totalCmp`），每行输出
`名称 A`，A 为面积乘以 100.0 后转换为 int。圆周率用 `math.pi`。

## T09 括号匹配（泛型）
运行：`work/T09 < tasks/T09.in`
检查每行的 `()[]{}` 是否匹配，列号是从 1 开始的字节位置。每行输出其一：`ok`；
`error at column C: unexpected 'X'`（右括号时栈为空）；`error at column C: expected 'Y' found 'X'`
（右括号与栈顶的左括号不匹配，Y 是栈顶左括号对应的右括号）；`unclosed 'X' at column C`
（行结束时栈不为空，报告栈顶也就是最内层的左括号）。遇到第一个错误就停止处理这一行。
要求：自己写一个泛型结构体 `Stack[T]`（至少有 push、pop 方法）并用它实现。

## T10 生命游戏
运行：`work/T10 < tasks/T10.in`
第一行是代数 N，之后是网格，`#` 为活细胞、`.` 为死细胞，各行等宽。边界外视为死细胞（不环绕）。
标准的康威规则（活细胞有 2 或 3 个活邻居时存活；死细胞恰有 3 个活邻居时变活）。
输出 N 代之后的网格，最后一行 `alive: 活细胞数`。
