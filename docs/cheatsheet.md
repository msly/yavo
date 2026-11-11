# 速查卡（草案 v0.1）

这是给 agent 放进上下文的一页语言规范。语法是草案，编译器还不存在；`nl` 是占位的名字，同时用作命令和文件扩展名。

## 1. 程序骨架和基本语法

一个文件就是一个程序，入口是 `main`；语句以换行结束，不写分号，代码块必须用花括号。

```
// 导入写在文件开头，例如 use std/fs；没用到的导入会有警告

fn add(a: int, b: int) -> int {
    return a + b            // 必须写 return，没有“最后一个表达式即返回值”
}

fn main() {
    let x = 3               // let：不可变
    var total = 0           // var：可变
    for i in 0..x {         // 0, 1, 2
        total += add(i, 1)
    }
    if total > 5 {
        println("big: ${total}")
    } else {
        println("small")
    }
}
```

- 循环：`for x in xs { }`、`for i in 0..n { }`、`while cond { }`、`loop { }`，配合 `break` 和 `continue`。
- 分支：`if`/`else`，以及 `match`（必须覆盖所有情况，`_` 兜底）。
- 字符串插值：`"${expr}"`；格式：`"${x:.2}"` 保留两位小数。
- 闭包：`(a, b) => a < b`。
- 注释只有 `//`。声明顺序无关，函数可以先用后定义。
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
- 接口值可以放进集合：`Vec[Named]`。

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
    if let v = m.get("a") {               // 有值才进入
        println(v)
    }
    let n = m.get("b") ?? 0               // 缺失时用默认值

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

## 4. 所有权：六条规则

没有 GC，也没有生命周期标注；记住下面六条就能通过检查。

1. **赋值、放进集合、传给 `sink` 参数都是移动**，移动后原变量不能再用。只含数字和布尔的小类型自动拷贝；其他类型要拷贝时显式写 `.clone()`。
2. **参数默认是只读借用。** 要修改调用者的值，参数写 `inout`，调用处写 `&x`；要拿走所有权，参数写 `sink`。
3. **视图（`str`、`[]T`）只能做局部变量和参数。** 要存进字段或集合，先 `.toString()` 或 `.toVec()`，或者只存下标区间。
4. **借用期间，被借的值不能修改或移动。** 借用在最后一次使用后结束，不是到代码块结尾。
5. **不能从字段或集合元素里直接移出值。** 用 `take(&x)` 取走并留下默认值（可选类型留下 `none`），或用 `replace(&x, 新值)`、`swap(&a, &b)`。
6. **递归结构用 `?Box[T]`，共享用 `Rc[T]`（单线程）或 `Arc[T]`（跨线程），互相引用的图用 `Arena[T]` 加 `Id[T]`。**

```
struct Node {
    key: int
    left: ?Box[Node]
    right: ?Box[Node]
}

fn insert(t: inout ?Box[Node], k: int) {
    if let n = t {                        // n 借用 t 里的节点
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
            child = take(&n.left)         // 规则 5：取走子树，原处留下 none
        } else {
            child = take(&n.right)
        }
    }                                     // 规则 4：n 的借用在这里之前已经结束
    t = child                             // 现在可以给 t 赋值了
}

fn firstWord(s: str) -> str {             // 返回的视图借自参数 s
    return s.fields()[0]
}

struct Doc {
    words: Vec[String]                    // 规则 3：字段里存 String，不存 str
}

fn addWord(d: inout Doc, w: str) {
    d.words.push(w.toString())
}
```

图结构的写法：节点放在 `Arena[T]` 里，用 `Id[T]` 互相指向。

```
struct Person {
    name: String
    friends: Vec[Id[Person]]
}

fn main() {
    var people = Arena[Person]{}
    let a = people.add(Person{name: "Ann".toString(), friends: []})
    let b = people.add(Person{name: "Bo".toString(), friends: [a]})
    people[a].friends.push(b)             // 互相引用没有问题
    if let p = people.get(b) {            // 条目被删除后，旧 Id 得到 none
        println(p.name)
    }
}
```

## 5. 标准库速查

`println`、`print`、`eprintln`、`min`、`max`、`abs`、`take`、`replace`、`swap`、`assert`、`panic` 不用导入；其余按模块 `use std/<模块>` 导入，多个模块写 `use std/{io, fs}`。

| 要做的事 | 写法 |
| --- | --- |
| 读入全部标准输入 | `let text = try io.readAll()`，返回 `String ! io.Error` |
| 逐行读标准输入 | `while let line = try io.readLine() { }`，返回 `?String ! io.Error`，不含换行符 |
| 读写文件 | `try fs.readText(path)`、`try fs.writeText(path, s)` |
| 命令行参数、退出 | `os.args()`、`os.exit(1)` |
| 按行、按空白、按分隔符切分 | `s.lines()`、`s.fields()`、`s.split(",")`，结果是 `Vec[str]` 局部变量 |
| 字符串长度、字节、子串 | `s.len()`、`s[i]`（`byte`）、`s[a..b]`（视图） |
| 修剪、大小写、查找 | `s.trim()`、`s.toLower()`、`s.toUpper()`、`s.contains(t)`、`s.startsWith(t)`、`s.find(t)`（`?int`） |
| 解析数字 | `s.parseInt()`（`?int`）、`s.parseFloat()`（`?f64`） |
| 拼接字符串 | `var out = String{}`，`out.push("text")`，`out.pushByte(c)` |
| 字节字面量 | `'a'`、`'\n'`；判断：`c >= '0' && c <= '9'` |
| `Vec` 基本操作 | `v.push(x)`、`v.pop()`（`?T`）、`v.len()`、`v[i]`、`v[a..b]`、`v.contains(x)`、`v.join(", ")` |
| 排序 | `v.sort()`；`v.sortBy((a, b) => a.1 > b.1)`，两者都是稳定排序 |
| `Map` 基本操作 | `m.set(k, v)`、`m.get(k)`（`?V`）、`m.getOr(k, 默认值)`、`m.has(k)`、`m.remove(k)`、`m.len()` |
| 计数 | `m.update(w, 0, (n) => n + 1)` |
| 遍历 | `for x in v { }`、`for i, x in v { }`、`for k, v in m { }` |
| 优先队列 | `use std/collections`；`var h = Heap[(int, int)].by((a, b) => a.0 < b.0)`，`h.push(x)`、`h.pop()`（`?T`） |
| 双端队列、集合 | `Deque[T]`：`pushBack`、`popFront`；`Set[T]`：`add`、`has`、`remove` |
| 数学 | `use std/math`；`math.sqrt(x)`、`math.pow(x, y)`、`math.floor(x)`、`math.pi` |
| 时间、随机数 | `use std/time`：`time.now()`；`use std/rand`：`rand.int(lo, hi)` |

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
- 一个类型能否交给别的线程由编译器判断；`Rc[T]` 不能，`Arc[T]` 可以。

测试写在同一个文件里，用 `nl test` 运行：

```
test "sum adds up" {
    assert(sum([1, 2, 3]) == 6)
}
```

| 命令 | 作用 |
| --- | --- |
| `nl run main.nl` | 编译并运行 |
| `nl build main.nl -o app` | 编译；`--release` 开优化，`--target x86_64-windows` 交叉编译 |
| `nl check --json` | 只检查，输出 JSON 诊断 |
| `nl fix` | 应用诊断里的修复补丁 |
| `nl test` | 运行 `test` 块 |
| `nl fmt` | 格式化 |
| `nl outline <包>`、`nl doc <模块>` | 只看签名；看标准库文档 |
| `nl explain <错误码>` | 错误说明，带一个错误写法和一个正确写法 |
| `nl audit` | 列出 `unsafe`、`unwrap`、关闭检查的位置 |

## 7. 常见错误和修法

前四行来自试点里真实出现过的失败；修法是每种错误的地道写法，不只是能通过编译的写法。

| 编译器说 | 原因 | 修法 |
| --- | --- | --- |
| 不能从字段或 `Box` 的内容里移出 | 规则 5 | `take(&x.field)`；不要用 `.clone()`，那会深拷贝 |
| 借用期间不能修改 | 规则 4 | 先把需要的值取到局部变量里，等借用结束后再修改 |
| 线程借用了临时值 | 传给 `spawn` 的切片是临时的 | 先 `let part = v[a..b]`，再 `spawn f(part)` |
| 变量或导入未使用（警告） | 多写了 | 改名为 `_`，或运行 `nl fix` |
| 使用了已移动的值 | 规则 1 | 改成借用传参；确实需要两份时写 `.clone()` |
| 视图不能存进字段或集合 | 规则 3 | `.toString()`、`.toVec()`，或只存下标区间 |
| `?T` 不能当 `T` 用 | 可选值没有处理 | `if let`、`??` |
| 结果没有处理 | 调用了返回 `T ! E` 的函数 | `try`、`catch`、`match` |
| `match` 没有覆盖所有情况 | 少了分支 | 补上分支或 `_ =>` |
| 类型不匹配：`int` 和 `f64` | 没有隐式转换 | `f64(n)`、`int(x)` |
| 函数缺少 `return` | 没有隐式返回值 | 在每条路径末尾写 `return` |

## 8. 待定的设计决定

下面几项我先选了一种写法，但理由不够硬，需要你定，或者等路线图第 1 步的数据。

| 决定 | 草案里的选择 | 另一种选择 | 我为什么这样选 |
| --- | --- | --- | --- |
| 函数返回值 | 必须写 `return` | 允许最后一个表达式作为返回值（Rust 风格） | 只留一种写法；但受 Rust 训练的模型会习惯性省略，可能增加迭代 |
| 接口实现 | 显式声明 `struct Dog: Named` | 结构上满足即可（Go 风格） | 报错更准确；代价是多写一处 |
| `Map` 计数 | `m.update(k, 默认值, 函数)` | `with n = m.entry(k) { n += 1 }` | 少一个语法结构；`with` 目前只用于锁 |
| 泛型括号 | `Vec[T]` | `Vec<T>` | 解析没有歧义；Go 也用方括号 |
| 名字和扩展名 | `nl`（占位） | 待你起名 |  |
| 参数求值顺序 | 从左到右，先求值后移动 |  | 草案还没有写进规则，需要补一条 |

这张卡的边界：没有覆盖 `unsafe`、C 互操作、包和多文件项目、泛型的全部规则；这些等对应功能进入路线图后再补。放进上下文之前建议先删掉本节；这张卡的实际 token 数还没有量过。
