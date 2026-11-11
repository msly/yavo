# Zhunsheng 速查卡（草案 v0.2）

这是给 agent 放进上下文的一页语言规范。语法是草案，编译器还不存在；命令是 `zhun`，源文件扩展名是 `.zhun`。

## 1. 程序骨架和基本语法

一个文件就是一个程序，入口是 `main`；语句以换行结束，不写分号，代码块必须用花括号。

```
// 导入写在文件开头，例如 use std/fs；没用到的导入会有警告

pub fn add(a: int, b: int) -> int {    // pub：从包导出
    return a + b                         // 必须写 return
}

fn main() {
    let x = 3                            // let：不可变
    var total = 0                        // var：可变
    for i in 0..x {                      // 0, 1, 2
        total += add(i, 1)
    }
    if total > 5 {
        println("big: ${total}")         // 插值直接写到输出，不分配内存
    } else {
        println("small")
    }
}
```

- 循环：`for x in xs { }`、`for i in 0..n { }`、`while cond { }`、`loop { }`，配合 `break` 和 `continue`。
- 分支：`if`/`else`，以及 `match`（必须覆盖所有情况，`_` 兜底）。
- 插值字符串 `"${expr}"` 只能直接传给 `println`、`print`、`eprintln`、`panic`、`assert` 和 `format`。前五个直接写输出，不分配内存；要得到 `String` 用 `format("${x}")`，会分配。保留两位小数写 `"${x:.2}"`。
- 闭包：`(a, b) => a < b`。
- 注释只有 `//`。声明顺序无关，函数可以先用后定义；`pub` 标记从包导出的函数和类型。
- 整数运算溢出会 panic；需要回绕时写 `+%`、`-%`、`*%`。

## 2. 类型

| 类型 | 写法 | 说明 |
| --- | --- | --- |
| 整数 | `int`、`i8`…`i64`、`u8`…`u64`、`byte` | `int` 是 64 位有符号 |
| 浮点、布尔 | `f64`、`f32`、`bool` |  |
| 字符串 | `String`（拥有）、`str`（视图） | 字面量是 `str`；`s.toString()` 得到 `String` |
| 动态数组 | `Vec[T]`，字面量 `[1, 2, 3]` | 切片视图是 `[]T` |
| 映射、集合 | `Map[K, V]`（字面量 `[:]`）、`Set[T]` | 按插入顺序遍历 |
| 元组 | `(int, String)` | 用 `.0`、`.1` 取，或 `let (a, b) = t` |
| 借用 | `&T`（只读）、`&inout T`（可变） | 属于视图，规则见第 4 节 |
| 可选、结果 | `?T`、`T ! E` | 见第 3 节 |
| 堆和共享 | `Box[T]`、`Rc[T]`、`Arc[T]`、`Arena[T]` 加 `Id[T]` | 见第 4 节 |

```
struct Point {
    x: f64
    y: f64 = 0.0                      // 字段可以有默认值

    fn norm(self) -> f64 {            // 方法写在结构体里
        return math.sqrt(self.x * self.x + self.y * self.y)
    }
    fn shift(inout self, dx: f64) {   // 要修改自己时写 inout self
        self.x += dx
    }
}

enum Shape {
    Circle(r: f64)
    Rect(w: f64, h: f64)
    Empty
}

fn area(s: Shape) -> f64 {
    match s {
        .Circle(r) => return 3.14159 * r * r
        .Rect(w, h) => return w * h
        .Empty => return 0.0
    }
}

interface Named {
    fn name(self) -> String
}

struct Dog: Named {                   // 实现接口要显式声明
    fn name(self) -> String {
        return "dog".toString()
    }
}

fn largest[T: Ord + Copy](xs: []T) -> ?T {   // 泛型和接口约束
    if xs.len() == 0 {
        return none
    }
    var best = xs[0]
    for x in xs {
        if x > best {
            best = x
        }
    }
    return best
}
```

- 构造：`Point{x: 1.0}`、`Shape.Circle(r: 2.0)`；类型已知时可以简写成 `.Circle(r: 2.0)`。
- 类型转换必须显式：`f64(n)`、`int(x)`。没有隐式转换。
- 接口值放进集合或字段时要显式装箱，不会在背后分配：`var all: Vec[Box[Named]] = []`，`all.push(Box(Dog{}))`。

## 3. 可选值和错误

没有 nil，也没有异常：可能缺失的值是 `?T`，可能失败的函数返回 `T ! E`，两者都必须处理后才能用。

```
enum ParseError {
    Empty
    BadNumber(text: String)
}

fn parsePort(s: str) -> int ! ParseError {
    if s.len() == 0 {
        return err(.Empty)
    }
    let n = s.parseInt() ?? return err(.BadNumber(text: s.toString()))
    return n                              // 成功值直接返回
}

fn load(path: str) -> int ! ParseError {
    let port = try parsePort(path)        // try：出错就把错误原样向上返回
    return port + 1
}

fn main() {
    let m: Map[String, int] = [:]
    if let v = m.get("a") {               // 有值才进入；v 是只读借用 &int
        println(v)
    }
    let n = m.getOr("b", 0)               // 可拷贝的类型可以直接取值

    match parsePort("80") {               // 分情况处理
        ok(p) => println(p)
        err(e) => eprintln("bad port: ${e}")
    }
    let p = parsePort("x") catch e {      // catch：处理错误并给出替代值，或者离开
        eprintln("${e}")
        return
    }
    println(p + n)
}
```

- `?T` 的三种用法：`if let v = opt { }`、`opt ?? 默认值`、`opt ?? return …`。
- `T ! E` 的三种用法：`try f()`、`f() catch e { }`、`match f() { ok(v) => … err(e) => … }`。
- `try` 只能用在返回 `… ! E` 的函数里，而且错误类型要一致。
- 不处理就使用是编译错误。越界、溢出、除零是 panic，不是错误值。

## 4. 所有权：七条规则

没有 GC，也没有生命周期标注；记住下面七条就能通过检查。

1. **赋值、放进集合、传给 `sink` 参数都是移动**，移动后原变量不能再用。只含数字和布尔的小类型自动拷贝；其他类型要拷贝时显式写 `.clone()`。
2. **参数默认是只读借用。** 要修改调用者的值，参数写 `inout`，调用处写 `&x`；要拿走所有权，参数写 `sink`。
3. **视图不能逃逸。** 视图包括 `str`、`[]T`、`&T`、`&inout T`，以及由视图组成的局部集合（如 `Vec[str]`）。它们可以做局部变量、参数和返回值，不能进入普通字段、`Box`、接口值或逃逸的闭包。要长期保存，先 `.toString()`、`.toVec()`，或只存下标区间。
4. **借用持续到最后一次使用为止**；借用期间，来源不能被修改或移动。`if let`、`for`、`match` 的绑定跟随来源的可变性：来源是 `var` 或 `inout` 时可以通过绑定修改，否则只读。
5. **导出函数返回视图时，在签名里写明来源**：`pub fn first(s: str) -> str of s`。包内私有函数可以省略，由编译器推断。
6. **不能从字段或集合元素里直接移出值。** 有默认值的类型用 `take(&x)` 取走并留下默认值（可选值留下 `none`，数字留下 0，`String`、`Vec`、`Map` 留下空值）；其他类型用 `replace(&x, 新值)` 或 `swap(&a, &b)`。
7. **递归结构用 `?Box[T]`；共享用 `Rc[T]`（单线程）或 `Arc[T]`（跨线程，且 `T` 本身要能被多线程共享）；互相引用的图用 `Arena[T]` 加 `Id[T]`。**

```
struct Node {
    key: int
    left: ?Box[Node]
    right: ?Box[Node]
}

fn insert(t: inout ?Box[Node], k: int) {
    if let n = t {                        // t 是 inout，所以 n 可以修改（规则 4）
        if k < n.key {
            insert(&n.left, k)            // 规则 2：inout 参数，调用处写 &
        } else if k > n.key {
            insert(&n.right, k)
        }
        return
    }
    t = Box(Node{key: k})                 // 未写的可选字段默认是 none
}

fn removeRoot(t: inout ?Box[Node]) {      // 假设根最多只有一个孩子
    var child: ?Box[Node] = none
    if let n = t {
        if n.left.isSome() {
            child = take(&n.left)         // 规则 6：取走子树，原处留下 none
        } else {
            child = take(&n.right)
        }
    }                                     // n 的最后一次使用已过，借用结束
    t = child                             // 现在可以给 t 赋值
}

pub fn firstWord(s: str) -> str of s {    // 规则 5：导出函数写明视图来自 s
    return s.fields()[0]
}

pub fn words(text: str) -> Vec[str] of text {   // 规则 3：局部视图集合，不复制字符串
    var out: Vec[str] = []
    for w in text.fields() {
        out.push(w)
    }
    return out
}

struct Doc {
    words: Vec[String]                    // 规则 3：字段里存 String，不存 str
}
```

图结构的写法：节点放在 `Arena[T]` 里，用 `Id[T]` 互相指向；读取返回借用。

```
struct Person {
    name: String
    friends: Vec[Id[Person]]
}

fn main() {
    var people = Arena[Person]{}
    let a = people.add(Person{name: "Ann".toString(), friends: []})
    let b = people.add(Person{name: "Bo".toString(), friends: [a]})
    if let p = people.getMut(a) {         // p 是 &inout Person
        p.friends.push(b)                 // 互相引用没有问题
    }
    if let p = people.get(b) {            // 条目被删除后得到 none
        println(p.name)
    }
    // 把 a 用在另一个 Arena 上会 panic：Id 里带着它所属 Arena 的身份
}
```

## 5. 标准库速查

`println`、`print`、`eprintln`、`format`、`min`、`max`、`abs`、`take`、`replace`、`swap`、`assert`、`panic` 不用导入；其余按模块 `use std/<模块>` 导入，多个模块写 `use std/{io, fs}`。第三列标明操作是否可能堆分配，`zhun doc` 和 `zhun allocations` 给出同样的信息。

| 要做的事 | 写法 | 可能分配 |
| --- | --- | --- |
| 读入全部标准输入 | `let text = try io.readAll()`，返回 `String ! io.Error` | 是 |
| 逐行读标准输入 | `while let line = try io.readLine() { }`，返回 `?String ! io.Error`，不含换行符 | 是 |
| 读写文件 | `try fs.readText(path)`、`try fs.writeText(path, s)` | 读文件时是 |
| 命令行参数、退出 | `os.args()`、`os.exit(1)` | 否 |
| 按行、按空白、按分隔符切分 | `s.lines()`、`s.fields()`、`s.split(",")`，结果是局部视图集合 `Vec[str]`，不复制字符串 | 只分配集合本身 |
| 字符串长度、字节、子串 | `s.len()`、`s[i]`（`byte`）、`s[a..b]`（视图） | 否 |
| 修剪、大小写、查找 | `s.trim()`（视图）、`s.toLower()`、`s.toUpper()`（新 `String`）、`s.contains(t)`、`s.startsWith(t)`、`s.find(t)`（`?int`） | 只有大小写转换 |
| 解析数字 | `s.parseInt()`（`?int`）、`s.parseFloat()`（`?f64`） | 否 |
| 拼接字符串 | `var out = String{}`，`out.push("text")`，`out.pushByte(c)`；或 `format("${a}-${b}")` | 是 |
| 字节字面量 | `'a'`、`'\n'`；判断：`c >= '0' && c <= '9'` | 否 |
| `Vec` 基本操作 | `v.push(x)`、`v.pop()`（`?T`）、`v.len()`、`v[i]`、`v[a..b]`、`v.contains(x)`、`v.join(", ")` | `push` 扩容和 `join` 时会 |
| 排序 | `v.sort()`；`v.sortBy((a, b) => a.1 > b.1)`，两者都是稳定排序 | 是（临时缓冲区） |
| `Map` 读取 | `m.get(k)`（`?&V`）、`m.getMut(k)`（`?&inout V`）、`m.getOr(k, 默认值)`（只用于可拷贝的类型）、`m.has(k)`、`m.len()` | 否 |
| `Map` 写入和计数 | `m.set(k, v)`、`m.remove(k)`；计数写 `m.update(w, 0, (n) => n + 1)` | 插入新键时会 |
| 遍历 | `for x in v { }`、`for i, x in v { }`、`for k, v in m { }`（绑定是借用） | 否 |
| 优先队列 | `use std/collections`；`var h = Heap[(int, int)].by((a, b) => a.0 < b.0)`，`h.push(x)`、`h.pop()`（`?T`） | `push` 扩容时会 |
| 双端队列、集合 | `Deque[T]`：`pushBack`、`popFront`；`Set[T]`：`add`、`has`、`remove` | 插入时会 |
| 数学 | `use std/math`；`math.sqrt(x)`、`math.pow(x, y)`、`math.floor(x)`、`math.pi` | 否 |
| 时间、随机数 | `use std/time`：`time.now()`；`use std/rand`：`rand.int(lo, hi)` | 否 |

读入并处理输入的标准写法：

```
use std/{io, os}

fn main() {
    let text = io.readAll() catch e {
        eprintln("read error: ${e}")
        os.exit(1)
    }
    var counts: Map[String, int] = [:]
    for w in text.fields() {
        let key = w.toLower()                 // toLower 返回新的 String
        counts.update(key, 0, (n) => n + 1)   // 键不存在时先放入 0，再用函数更新
    }
    var items: Vec[(String, int)] = []
    for k, n in counts {
        items.push((k.clone(), n))
    }
    items.sortBy((a, b) => a.1 > b.1 || (a.1 == b.1 && a.0 < b.0))
    for it in items[0..min(5, items.len())] {
        println("${it.0} ${it.1}")
    }
}
```

## 6. 并发、测试和命令行

并发只有 OS 线程：`scope` 块里用 `spawn` 启动线程，块结束时所有线程都已结束，所以线程可以借用块外的数据。

```
fn sum(xs: []int) -> int {
    var s = 0
    for x in xs {
        s += x
    }
    return s
}

fn parallelSum(nums: Vec[int]) -> int {
    let mid = nums.len() / 2
    let left = nums[0..mid]               // 先把切片存成局部变量，再交给线程
    let right = nums[mid..nums.len()]
    var total = 0
    scope {
        let a = spawn sum(left)
        let b = spawn sum(right)
        total = a.join() + b.join()
    }
    return total
}
```

- 线程之间传值用 channel：`let ch = Channel[int]{cap: 8}`，`ch.send(v)`，`ch.recv()`（`?int`，关闭后得到 `none`），`ch.close()`。
- 共享可变状态用 `Arc[Mutex[T]]`：`with g = m.lock() { g.count += 1 }`。
- 一个值能否交给别的线程、能否被多个线程同时读，由编译器从类型推导：`Rc[T]` 都不能；`Arc[T]` 只有在 `T` 能被多线程共享时才能跨线程，`Arc[Mutex[T]]` 可以。
- panic 直接终止整个进程，不展开栈，也不会只结束一个线程；可能失败的操作要用 `T ! E`。

测试写在同一个文件里，用 `zhun test` 运行；每个测试在单独的子进程里跑，一个测试 panic 不影响其他测试：

```
test "sum adds up" {
    assert(sum([1, 2, 3]) == 6)
}
```

| 命令 | 作用 |
| --- | --- |
| `zhun run main.zhun` | 编译并运行 |
| `zhun build main.zhun -o app` | 编译；`--release` 开优化，`--target x86_64-windows-gnu` 交叉编译 |
| `zhun toolchain install <目标>` | 提前下载并校验交叉编译工具链；`build` 从不联网 |
| `zhun check --json` | 只检查，输出 JSON 诊断 |
| `zhun fix` | 应用诊断里的修复补丁 |
| `zhun test` | 运行 `test` 块 |
| `zhun fmt` | 格式化 |
| `zhun outline <包>`、`zhun doc <模块>` | 只看签名；看标准库文档 |
| `zhun explain <错误码>` | 错误说明，带一个错误写法和一个正确写法 |
| `zhun allocations <函数>` | 列出可能堆分配的操作 |
| `zhun audit` | 列出 `unsafe`、C 互操作、关闭检查的位置 |
| `zhun verify --target <目标>` | 构建、测试、审计，输出验证报告 |

## 7. 常见错误和修法

前四行来自试点里真实出现过的失败；修法是每种错误的地道写法，不只是能通过编译的写法。

| 编译器说 | 原因 | 修法 |
| --- | --- | --- |
| 不能从字段或 `Box` 的内容里移出 | 规则 6 | `take(&x.field)`；不要用 `.clone()`，那会深拷贝 |
| 借用期间不能修改 | 规则 4 | 先把需要的值取到局部变量里，等借用结束后再修改 |
| 线程借用了临时值 | 传给 `spawn` 的切片是临时的 | 先 `let part = v[a..b]`，再 `spawn f(part)` |
| 变量或导入未使用（警告） | 多写了 | 改名为 `_`，或运行 `zhun fix` |
| 使用了已移动的值 | 规则 1 | 改成借用传参；确实需要两份时写 `.clone()` |
| 视图不能存进字段、`Box` 或接口值 | 规则 3 | `.toString()`、`.toVec()`，或只存下标区间 |
| 导出函数返回视图，没写来源 | 规则 5 | 在返回类型后加 `of <参数名>` |
| 这个类型没有默认值，不能 `take` | 规则 6 | `replace(&x, 新值)` 或 `swap(&a, &b)` |
| 接口值不能直接放进集合 | 需要显式装箱 | `all.push(Box(x))`，集合类型写 `Vec[Box[Named]]` |
| 这个 `Arc` 不能交给别的线程 | `T` 不能被多线程共享 | 改用 `Arc[Mutex[T]]`，或把里面的 `Rc` 换成 `Arc` |
| `?T` 不能当 `T` 用 | 可选值没有处理 | `if let`、`??` |
| 结果没有处理 | 调用了返回 `T ! E` 的函数 | `try`、`catch`、`match` |
| `match` 没有覆盖所有情况 | 少了分支 | 补上分支或 `_ =>` |
| 类型不匹配：`int` 和 `f64` | 没有隐式转换 | `f64(n)`、`int(x)` |
| 函数缺少 `return` | 没有隐式返回值 | 在每条路径末尾写 `return` |
| 运行时 panic：Id 属于另一个 Arena | 把一个 Arena 的 Id 用在了另一个上 | 检查 Id 是从哪个 Arena 的 `add` 得到的 |

## 8. 待定的设计决定

下面几项我先选了一种写法，但理由不够硬，需要你定，或者等 P1 的数据。名字已定为 Zhunsheng，不再列在这里。

| 决定 | 草案里的选择 | 另一种选择 | 取舍 |
| --- | --- | --- | --- |
| 函数返回值 | 必须写 `return` | 最后一个表达式作为返回值（Rust 风格） | 只留一种写法；但受 Rust 训练的模型会习惯性省略，可能增加迭代 |
| 接口实现 | 显式声明 `struct Dog: Named` | 结构上满足即可（Go 风格） | 报错更准确；代价是多写一处 |
| 借用类型的写法 | `&T`、`&inout T` | 不暴露借用类型，只在 `if let`、`with` 块里借出（Gojo 的做法） | 写法与 `inout` 参数一致；代价是多一个类型概念 |
| 私有函数返回视图的来源 | 由编译器推断 | 所有函数都必须标注 | 推断少写代码；强制标注让签名完全不依赖函数体 |
| 可拷贝值的读取 | `m.getOr(k, 默认值)` | `m.getCopy(k)` 返回 `?V` | 少写一个 `??`；`getCopy` 能区分“缺失”和“正好等于默认值” |
| 泛型括号 | `Vec[T]` | `Vec<T>` | 解析没有歧义；Go 也用方括号 |
| 参数求值顺序 | 从左到右，先求值后移动 |  | 还没写进规则，需要补一条 |

这张卡的边界：没有覆盖 `unsafe`、C 互操作、包和多文件项目、泛型的全部规则；这些等对应功能进入路线图后再补。放进上下文之前建议先删掉本节；这张卡的实际 token 数还没有量过。
