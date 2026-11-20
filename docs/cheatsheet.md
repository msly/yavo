# Yavo 速查卡（v0.5 冻结候选）

这是给 agent 放进上下文的一页语言写法。语法是草案，编译器还不存在；命令是 `yavo`，源文件扩展名是 `.yv`。本卡只给常用写法，精确的安全规则以“语义表”为准，库签名见《标准库契约》。v0.5 的新增决定及与旧版差异见《决议与变更》，没有声称 Yavo 测试已经运行。

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

- 循环：`for x in xs { }` 默认只读；`for x in &xs { }` 显式可变，Map 键始终只读。范围循环 `for i in 0..n { }`、`while cond { }`、`loop { }` 配合 `break`、`continue`。
- 分支：`if`/`else` 是语句；要按情况取值，用 `match` 的 `=> 表达式` 分支（必须覆盖所有情况，`_` 兜底）。
- 返回：有返回值的函数每条路径都写 `return`，没有返回值的函数可以自然结束。块体闭包也要写 `return`；单表达式闭包 `(a, b) => a < b` 不用。
- 插值字符串 `"${expr}"` 只能直接传给 `println`、`print`、`eprintln`、`panic`、`assert` 和 `format`。前五个直接写输出，不分配内存；要得到 `String` 用 `format("${x}")`，会分配。保留两位小数写 `"${x:.2}"`。
- 注释只有 `//`。声明顺序无关，函数可以先用后定义；`pub` 标记从包导出的函数和类型。
- 入口只能写成 `fn main()`：没有参数、没有返回值（否则 `E0016`）。所以 `main` 里不能用 `try`；可能失败的步骤放进返回 `T ! E` 的辅助函数，在 `main` 里用 `catch` 处理后 `os.exit(1)`，写法见第 5 节。
- 整数溢出、除零、越界移位、超出范围的转换执行时 panic，包括常量可知的表达式。`let b: u8 = 256` 则是字面量类型错误。回绕写 `+%`、`-%`、`*%`；截断转换写 `i32.wrap(x)`，仅接受整数。整数边界写 `i64.min`、`i64.max`。
- 集合字面量 `[1, 2, 3]`、`[:]` 会分配内存；除此之外，普通语法不分配。

## 2. 类型

| 类型 | 写法 | 说明 |
| --- | --- | --- |
| 整数 | `int`、`i8`…`i64`、`u8`…`u64`、`byte` | `int` 是 64 位有符号 |
| 浮点、布尔 | `f64`、`f32`、`bool` |  |
| 字符串 | `String`（拥有）、`str`（视图） | 始终是合法 UTF-8；字面量是 `str`，`s.toString()` 得到 `String` |
| 动态数组 | `Vec[T]`，字面量 `[1, 2, 3]` | 切片视图是 `[]T` |
| 映射、集合 | `Map[K, V]`（字面量 `[:]`）、`Set[T]` | 按插入顺序遍历 |
| 元组 | `(int, String)` | 用 `.0`、`.1` 取，或 `let (a, b) = t` |
| 借用 | `&T`（只读）、`&inout T`（可变） | `*p = v` 修改目标，`p = q` 重新绑定；构造时写如 `let p: &inout int = &n` |
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

fn largest[T: Ord + Copy](xs: []T) -> ?T of xs {   // 泛型和接口约束
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
- 类型转换必须显式：`f64(n)`、`int(x)`。没有隐式转换，只有语义表第 8 节列出的几种自动操作。
- 比较：`==` 需要类型实现 `Eq`，`<` 等需要实现 `Ord`；整数、`str`、`String`、可比较元素组成的元组自动实现。浮点数可以直接用 `==`、`<`（按 IEEE 754），但不满足 `Eq`、`Ord`：不能用 `v.sort()` 或 `largest[f64]`，排序写 `v.sortBy((a, b) => a.totalCmp(b) == .Less)`。自定义类型写 `struct Version: Ord` 并实现 `cmp` 方法。
- 泛型函数的返回值可能携带借用时也写来源，例如上面的 `largest` 写 `-> ?T of xs`；用在 `int` 上时不会借住数组。
- 泛型结构体同样用方括号；字面量要写出类型参数，方法里用 `T`、`Self`：

```
struct Stack[T] {
    items: Vec[T]

    fn push(inout self, x: sink T) {      // sink：把 x 移进栈
        self.items.push(x)
    }
    fn pop(inout self) -> ?T {
        return self.items.pop()
    }
}

fn main() {
    var s = Stack[int]{items: []}        // 写 Stack[int]{...}，不写 Stack{...}
    s.push(1)
    while let x = s.pop() {
        println(x)
    }
}
```

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
    let n = m.getCopy("b") ?? 0               // getCopy 要求可复制；?? 只在缺失时计算右侧

    match parsePort("80") {               // 分情况处理
        ok(p) => println(p)
        err(e) => eprintln("bad port: ${e}")
    }
    let p = parsePort("x") catch e {      // 块形式的 catch：处理错误后必须离开；要替代值写 catch e => 值
        eprintln("${e}")
        return
    }
    println(p + n)
}
```

- `?T` 的模式是 `some(x)`、`none`；构造可以显式 `some(value)`，也保留有值自动包装。`if let` 是只处理有值分支的简写，`??` 只在缺失时计算右侧。
- `?T` 的方法：`o.isSome()`、`o.isNone()`（都返回 `bool`）；取值用 `if let`、`while let`、`??`、`match`（标准库契约里没有 `unwrap`）。`if let` 可以带 `else`：

```
if let v = m.get("a") {
    println(v)
} else {
    println("missing")
}
```


```text
match take(&opt) {
    some(value) => useOwned(value)
    none => println("empty")
}
```

- 不使用错误时允许 `catch _ => 0` 或 `catch _ { return }`：错误在处理体开始前释放；`catch _e` 则保留到处理体结束。自动修复只改为 `_e`，不改为 `_`。
- `T ! E` 的四种用法：`try f()`、`f() catch e => 替代值`、`f() catch e { … }`（块里必须离开：`return`、`break`、`continue` 或 `panic`）、`match f() { ok(v) => … err(e) => … }`。
- `try` 只能用在返回 `… ! E` 的函数里，而且错误类型要一致。
- 不处理就使用是编译错误。越界、溢出、除零是 panic，不是错误值。

## 4. 所有权：九条规则

没有 GC，也没有生命周期标注。写代码时记住下面九条；完整规则和边界情况见语义表。

1. **移动与复制。** 按值赋值、放进集合、传给 `sink` 参数时，拥有值（`String`、`Vec`、`Box` 等）被移动，之后原变量不能再用。标量及无析构、有效载荷递归满足 Copy 的类型自动复制；`str`、`&T` 复制的只是描述符；`&inout T` 不能复制。要第二份拥有值时写 `.clone()`。
2. **参数默认是只读借用。** 要修改调用者的值，参数写 `inout`，调用处写 `&x`；要拿走所有权，参数写 `sink`。
3. **权限看借用类型，不看 `var`。** `let`/`var` 只管能否重新赋值；`&T` 只读，`&inout T` 可写；把 `&T` 放进 `var` 也不能写。`if let`、`match` 对已有位置继承访问权限。`for` 默认只读；写 `for k, v in &m` 才取得 `&inout V`，循环体此时不能再直接访问 `m`。`Map` 键永远只读。标量借用通过 `*p = value` 写入。
4. **视图不能逃逸。** 视图（`str`、`[]T`、`&T`、`&inout T`）和携带视图的值（如 `Vec[str]`）可以做局部变量、参数和返回值，不能进入普通字段、`Box`、接口值、channel 或被保存的闭包。`.toVec()` 不会消除元素里的借用；要长期保存，逐个 `.toString()`，或者只存下标区间。v1 没有可变切片。
5. **每个具名函数返回借用时都写来源**，私有函数和泛型函数也一样：`-> str of s`、`-> str of (a, b)`、`-> str of self`、`-> str of static`、`-> ?T of xs`。`of` 只能指向借用参数：`fn bad(text: sink String) -> str of text` 会被拒绝，因为 `text` 在函数结束时就被释放了。
6. **不能通过参数写入新借用。** 函数不能把来自其他参数的借用写进 `inout` 参数（如 `out: inout Vec[str]`），要改为返回 `Vec[str] of text`。
7. **借用持续到最后一次使用；实参从左到右，每个立即完成移动或借用。** `v.push(v.len())` 会被拒绝，写成 `let n = v.len()` 再 `v.push(n)`；`yavo fix` 对这类已证明安全的固定写法会自动拆开，其他情况只给候选。
8. **不允许部分移动。** 非 Copy 字段、`Box` 内容或集合元素都不能直接移出，即使外层由本函数拥有。 有默认值的类型用 `take(&x)` 取走并留下默认值（可选值留下 `none`，数字留下 0，`String`、`Vec`、`Map` 留下空值）；其他类型用 `replace(&x, 新值)` 或 `swap(&a, &b)`。例外：匹配拥有的临时值（如 `match ch.recv()`）可以移出其中的值；要消费已有的可选变量，写 `match take(&opt)`。
9. **结构的选择。** 递归结构用 `?Box[T]`；单线程共享用 `Rc[T]`；跨线程共享用 `Arc[T]`，条件见第 6 节；互相引用的图用 `Arena[T]` 加 `Id[T]`。

```
struct Node {
    key: int
    left: ?Box[Node]
    right: ?Box[Node]
}

fn insert(t: inout ?Box[Node], k: int) {
    if let n = t {                        // 规则 3：t 是 inout，n 可写
        if k < n.key {
            insert(&n.left, k)            // 规则 2：再借用子字段，调用处写 &
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
            child = take(&n.left)         // 规则 8：取走子树，原处留下 none
        } else {
            child = take(&n.right)
        }
    }                                     // n 的最后一次使用已过，借用结束
    t = child
}

fn firstWord(s: str) -> str of s {        // 规则 5：私有函数也写来源
    return s.fields()[0]                  // 复制出的 str 指向 s，不指向临时的 Vec
}

fn words(text: str) -> Vec[str] of text { // 规则 4、6：返回带来源的视图集合，不复制字符串
    var out: Vec[str] = []
    for w in text.fields() {
        out.push(w)                       // 局部容器可以写入借用
    }
    return out
}

fn countWords(text: str) -> int {
    let ws = words(text)
    let n = ws.len()                      // 规则 7：先算出来，再作为实参
    return n
}

struct Doc {
    words: Vec[String]                    // 规则 4：字段里存 String，不存 str
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

`println`、`print`、`eprintln`、`format`、`min`、`max`、`abs`、`take`、`replace`、`swap`、`assert`、`panic` 不用导入。基础类型 `Vec/Map/Set/Box/Rc/Arc/Arena/Id`、`Ordering`、`Utf8Error`、`Copy/Eq/Ord/Drop/Send/Sync` 在 prelude 中。其余按模块 `use std/<模块>` 导入，多个模块写 `use std/{io, fs}`；模块名和其公开类型进入当前文件，普通函数仍用 `fs.readText` 之类的限定名。类型重名时不能猜测来源，改用模块限定名。第三列标明操作是否可能堆分配，`yavo doc` 和 `yavo allocations` 给出同样的信息。

| 要做的事 | 写法 | 可能分配 |
| --- | --- | --- |
| 读入全部标准输入 | `let text = try io.readAll()`，返回 `String ! io.Error`；内容不是合法 UTF-8 时得到编码错误 | 是 |
| 逐行读标准输入 | `while let line = try io.readLine() { }`，返回 `?String ! io.Error`，不含换行符 | 是 |
| 读写文件 | `try fs.readText(path)`、`try fs.writeText(path, s)`；编码错误为 `fs.Error.Utf8(offset)`，从 0 开始计字节 | 都可能，包括平台路径转换 |
| 命令行参数、退出 | `os.args()` 返回 `[]str of static`（启动时已转换，参数不是合法 UTF-8 时程序启动即报错退出）、`os.exit(1)` | 否 |
| 按行、按空白、按分隔符切分 | `s.lines()`、`s.fields()`、`s.split(",")`，结果是 `Vec[str]`，不复制字符串 | 只分配集合本身 |
| 字符串长度、字节、子串 | `s.len()`、`s[i]`（`byte`）按字节；`s[a..b]` 是视图，两端必须落在字符边界上。`byte` 是整数，`"${s[i]}"` 打印数字（`'a'` 打印 `97`）；要打印一个字符写 `s[i..i+1]` | 否 |
| 修剪、大小写、查找 | `s.trim()`（视图）、`s.toLower()`、`s.toUpper()`（新 `String`）、`s.contains(t)`、`s.startsWith(t)`、`s.find(t)`（`?int`） | 只有大小写转换 |
| 解析数字 | `s.parseInt()`（`?int`）、`s.parseFloat()`（`?f64`） | 否 |
| 拼接字符串 | `var out = String{}`，`out.push("text")`；`push` 接受 `str` 或 `String`，只读取实参，不消费；或 `format("${a}-${b}")` | 是 |
| 字节转字符串 | `String.fromUtf8(bytes)` 消费字节数组；成功得 `String`，失败得 `Utf8Error{offset: ...}`，输入被释放 | 契约为可能；允许复用原缓冲区 |
| 字节字面量 | `'a'`、`'\n'`；判断：`c >= '0' && c <= '9'` | 否 |
| `Vec` 基本操作 | `v.push(x)`、`v.pop()`（`?T`，拥有的值）、`v.len()`、`v[i]`、`v[a..b]`、`v.contains(x)`、`v.join(", ")`（元素必须是 `str` 或 `String`，数字先 `format` 成 `String`） | `push` 扩容和 `join` 时会 |
| 排序 | `v.sort()`（要求 `Ord`）；`v.sortBy((a, b) => a.1 > b.1)`；浮点数用 `sortBy` 加 `totalCmp`；两者都是稳定排序 | 是（临时缓冲区） |
| `Map` 读取 | `m.get(k)`（`?&V`）、`m.getMut(k)`（`?&inout V`）、`m.getCopy(k)`（`?V`，要求可复制）、`m.has(k)`、`m.len()`；`Map[String, V]` 的 `get`、`getMut`、`getCopy`、`has`、`remove` 直接收 `str` 键，如 `m.get("a")`；`set`、`update` 要拥有的 `String` 键；默认值写 `m.getCopy(k) ?? 0` | 否 |
| `Map` 写入和计数 | `set(k, v)` 消费键和值；`remove(k) -> ?V` 返回拥有值；`update(k, initial, f)` 消费拥有键，v1 限不携带借用的 Copy 值，f 恰好执行一次 | 插入可能；remove 本身不分配 |
| 遍历 | `for x in v`、`for i, x in v`、`for k, v in m` 默认只读；`in &v`、`in &m` 显式可变；Map 键始终只读 | 否 |
| 优先队列 | `use std/collections`；`var h = Heap[(int, int)].by((a, b) => a.0 < b.0)`，`h.push(x)`、`h.pop()`（`?T`）；比较闭包会被保存，不能捕获借用 | `push` 扩容时会 |
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

`main` 不能用 `try`，所以把会失败的步骤放进辅助函数，`main` 只负责报告并退出：

```
use std/{io, os}

fn countLines() -> int ! io.Error {
    let text = try io.readAll()
    return text.lines().len()
}

fn main() {
    let n = countLines() catch e {
        eprintln("error: ${e}")
        os.exit(1)
    }
    println(n)
}
```

### 析构

```text
struct Resource: Drop {
    label: String
    fn drop(inout self) {
        println("close ${self.label}")
    }
}
```

只能编译器调用 `drop`，不能手动 `.drop()`；要提前释放已有拥有值，写 `let _ = resource`。完整对象先执行用户 drop，再按字段声明逆序清理；未完成构造不执行整体 drop。带析构的类型不是 Copy。普通退出有清理，panic 和 os.exit 不展开栈。

## 6. 并发、测试和命令行

并发只有 OS 线程：`scope` 块里用 `spawn` 启动线程，块结束时等待所有线程。`use std/sync` 引入 `Mutex`、`Guard`、`Channel`、`SendError`、`Join`。`Rc/Arc` 已在 prelude 中。底层数据必须在 scope 外已存在；复制临时切片描述符合法，借用临时拥有数组不合法。

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
    let left = nums[0..mid]               // 局部命名只是可读性选择；spawn sum(nums[0..mid]) 也合法
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

- 线程借用保守持续到 scope 结束；`join()` 消耗句柄取得返回值，但不提前缩短借用。丢弃句柄不会分离线程。中途 `return` 或 `try` 也先等待。v1 线程返回值必须不携带借用且满足 Send。
- 线程之间传值用 channel：`let ch = Channel[Job]{cap: 8}`；`ch.send(v)` 移动值，关闭后返回 `err(.Closed(v))` 把值还回来；`ch.recv()` 返回拥有的 `?Job`，关闭后先取完缓冲区，再返回 `none`。channel 只传拥有值，不传 `str`、`Vec[str]` 这类携带借用的值。
- `Channel[T]{cap: n}` 要求 `n > 0`；关闭幂等并唤醒等待者。`Mutex(T)` 接收拥有值，`m.lock() -> Guard[T] of self`，`m.tryLock() -> ?Guard[T] of self`；竞争时 tryLock 返回 none。
- 共享可变状态用 `Arc[Mutex[T]]`，要求 `T` 可以交给别的线程（`Send`）：`with g = m.lock() { g.count += 1 }`。从 `g` 借出的引用不能带出 `with` 块；锁守卫也不能交给别的线程。
- 通过 `Rc`、`Arc` 只能读里面的值，把句柄放进 `var` 也一样；要修改就用 `Mutex`。
- `Arc[T]` 跨线程要求 `T` 同时满足 `Send` 和 `Sync`；`Rc` 永远不能跨线程，所以 `Arc[Mutex[Rc[T]]]` 也不行。接口值默认不能跨线程；`Box[Named + Send]` 只能转交给另一个线程，要被多个线程共享还需要 `+ Sync`。
- panic 直接终止整个进程，不展开栈，也不会只结束一个线程；可能失败的操作要用 `T ! E`。

测试写在同一个文件里，用 `yavo test` 运行；每个测试在单独的子进程里跑并带超时，一个测试 panic 或卡死不影响其他测试。随机测试的种子只保证随机输入可以重放，不保证线程调度可以重放。ASan/UBSan 与 TSan 分开构建和执行，不合并为同一二进制：

```
test "sum adds up" {
    assert(sum([1, 2, 3]) == 6)
}
```

| 命令 | 作用 |
| --- | --- |
| `yavo run main.yv` | 编译并运行 |
| `yavo build main.yv -o app` | 编译；`--release` 开优化，`--target x86_64-windows-gnu` 交叉编译 |
| `yavo toolchain install <目标>` | 提前下载并校验交叉编译工具链；`build` 从不联网 |
| `yavo check --json` | 只检查，输出 JSON 诊断 |
| `yavo fix` | 只应用不改变语义的修复；其余只给候选补丁 |
| `yavo test` | 运行 `test` 块 |
| `yavo fmt` | 格式化 |
| `yavo outline <包>`、`yavo doc <模块>` | 只看签名；看标准库文档 |
| `yavo explain <错误码>` | 错误说明，带一个错误写法和一个正确写法 |
| `yavo allocations <函数>` | 每个操作给出“不分配、可能分配、未知”之一 |
| `yavo audit` | 列出 `unsafe`、C 互操作、关闭检查的位置 |
| `yavo verify --target <目标>` | 构建、测试、审计，输出验证报告；没执行的检查记为 `not_run` |

## 7. 常见错误和修法

表中同时包含试点中的失败和后续规范新增诊断；每个修复必须满足当前规则，不能把“能编译”当成“意图相同”。

| 编译器说 | 原因 | 修法 |
| --- | --- | --- |
| 不能从字段或 `Box` 的内容里移出 | 规则 8 | 按意图选：要取走用 `take(&x.field)`，要保留原值用 `.clone()`，只读就改成借用；三者语义不同，`yavo fix` 只列候选 |
| 借用期间不能修改 | 规则 7 | 先把需要的值取到局部变量里，等借用结束后再修改 |
| 线程借用的数据活得不够久 | 数据在 `scope` 内部声明，比 `scope` 更短命 | 把数据放到 `scope` 外面声明；切片可以直接传，如 `spawn f(v[a..b])` |
| 变量或导入未使用（警告） | 多写了 | 改名为 `_name`，对象照常活到作用域结束；`yavo fix` 会这样改。注意 `_` 会立即丢弃对象 |
| 实参用了已被借用或移动的值，如 `v.push(v.len())` | 规则 7 | 先 `let n = v.len()`，再 `v.push(n)`；`yavo fix` 只对已证明安全的固定写法自动拆开 |
| 使用了已移动的值 | 规则 1 | 改成借用传参；确实需要两份时写 `.clone()` |
| 只读借用不能写入 | 规则 3 | 改用 `getMut`，或从可写的来源借用；`var` 不能让 `&T` 变成可写 |
| 视图不能存进字段、`Box` 或接口值 | 规则 4 | `.toString()`、逐个转成 `String`，或只存下标区间 |
| 函数返回借用，没写来源（包括泛型函数） | 规则 5 | 在返回类型后加 `of <参数名>`；`yavo fix` 只给候选，不自动加 |
| `of` 指向了 `sink` 参数 | 规则 5 | 改成借用参数，或者返回拥有的 `String` |
| 不能把借用写进 `inout` 参数 | 规则 6 | 改为返回 `Vec[str] of text` 这类带来源的结果 |
| 这个类型没有默认值，不能 `take` | 规则 8 | `replace(&x, 新值)` 或 `swap(&a, &b)` |
| 不能从已有的可选变量里移出值 | 规则 8 | `match take(&opt)` |
| 接口值不能直接放进集合 | 需要显式装箱 | `all.push(Box(x))`，集合类型写 `Vec[Box[Named]]` |
| `f64` 不满足 `Ord` | 浮点数没有全序 | 排序用 `sortBy` 加 `totalCmp` |
| 这个值不能交给别的线程 | 缺少 `Send` 或 `Sync` | 把 `Rc` 换成 `Arc`；`Arc[Mutex[T]]` 只在 `T: Send` 时成立，包住 `Rc` 也不行；接口值写 `Box[Named + Send]` |
| 锁内借出的引用离开了 `with` 块，或锁守卫被交给别的线程 | 访问只在持锁期间、持锁的线程里受保护 | 在块内用完，或在块内 `.clone()` 出拥有值 |
| `?T` 不能当 `T` 用 | 可选值没有处理 | `if let`、`??`，或 `match` 的 `some(x)` / `none` |
| 结果没有处理 | 调用了返回 `T ! E` 的函数 | `try`、`catch`、`match` |
| 块形式的 `catch` 能走到结尾 | 块里必须离开 | 需要替代值时改用 `catch e => 替代值` |
| `match` 没有覆盖所有情况 | 少了分支 | 补上分支或 `_ =>` |
| 类型不匹配：`int` 和 `f64` | 没有隐式转换 | `f64(n)`、`int(x)` |
| 函数缺少 `return` | 没有尾表达式返回 | 在每条路径末尾写 `return` |
| 运行时 panic：Id 属于另一个 Arena | 把一个 Arena 的 Id 用在了另一个上 | 检查 Id 是从哪个 Arena 的 `add` 得到的 |
| 运行时 panic：字符串切在字符中间 | `s[a..b]` 两端要落在字符边界 | 用 `s.find` 等返回的下标 |

## 8. v1 已定设计

下面七项已经定下，不再保留两种并行写法。

| 项目 | 决定 | 理由 |
| --- | --- | --- |
| 函数返回值 | 有返回值的具名函数、方法和块体闭包必须写 `return`；没有返回值的函数可以自然结束；保留单表达式闭包 `=> 表达式` | 不让代码块的最后一个表达式隐式成为结果 |
| 接口实现 | 显式声明，如 `struct Dog: Named` | 在声明处检查接口是否完整，不因碰巧有同名方法就算实现 |
| 借用类型 | 公开 `&T` 和 `&inout T`；没有 `&mut`、`ref` 等同义写法 | 权限在签名、变量类型和诊断里都看得见 |
| 视图来源 | 所有具名函数（包括私有函数）都写 `of` | 修改函数体不会悄悄改变调用方看到的借用关系 |
| 可复制值的读取 | `m.getCopy(k)` 返回 `?V`，要求可复制；默认值写 `??`；不提供 `getOr` | 复制操作明确，保留“键不存在”的信息 |
| 泛型括号 | 统一用方括号：`Vec[T]`、`Map[K, V]` | 不同时支持尖括号 |
| 参数求值顺序 | 先处理接收者，再从左到右处理实参，每个实参立即完成复制、移动或借用；不做两阶段借用 | 消除移动时点的歧义，C 后端按同样的顺序生成代码 |

本轮补定：标量写入 `*p`；可选模式 `some/none`；视图形参接受 `Vec → []`；析构 `Drop.drop(inout self)`；标准库接口见《标准库契约》；不允许部分移动；常量可知的有效表达式失败仍是运行时 panic；`catch _` 与 `_e` 有不同丢弃时点；修复临时名为 `vLen`，冲突加 `2/3/...`；`for` 默认只读、`in &xs` 才可写。

这张卡的边界：没有覆盖 `unsafe`、C 互操作、包和多文件项目、泛型的全部规则，这些等对应功能进入路线图后再补。放进上下文之前建议先删掉本节；这张卡的实际 token 数还没有量过。
