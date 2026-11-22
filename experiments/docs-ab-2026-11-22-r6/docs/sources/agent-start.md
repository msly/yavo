# Yavo：最小编写说明

当前实现入口；完整 API 按 [主题索引](agent-api-index.md) 查询。源文件 `.yv`，构建 `yavo build main.yv -o app`，检查 `yavo check main.yv --json`。只构建入口，导入的源文件自动加入。

## 1. 语法与输入输出

- 换行结束语句，不写分号；块必须有 `{}`；注释只有 `//`。`let` 不可重新赋值，`var` 可赋值。
- 入口固定 `fn main()`。有返回值的函数、方法和块体闭包必须显式 `return`；`if` 是语句，按条件取值用 `match`。单表达式闭包写 `(a, b) => a < b`。
- `int` 是 i64；`byte` 是 u8。转换显式写 `int(x)`、`f64(n)`。整数溢出、除零、越界会 panic；回绕加减乘为 `+%`、`-%`、`*%`。
- `for i in 0..n`、`while cond`、`loop`，配合 `break`/`continue`。`for x in xs` 只读；`for x in &xs` 才可写，循环体不能再直接访问被可变遍历的容器。
- 字面量是 `str`；拥有字符串用 `"text".toString()`。插值写 `"${x}"`；**字面的 `${` 必须写 `"\${"`**。`\n`、`\t`、`\\`、`\"` 是转义。
- 插值仅直接传给 `print/println/eprintln/panic/assert/format`；输出直接写，构造字符串用 `format("${x}")`，会分配。`"${x:.2}"` 要求浮点值。

以下独立程序读入整数，排序并打印总和与数量。输入保证为整数；遇到非法项会提前返回。

```yv example=numbers/main.yv
use std/{io, os}

fn main() {
    let text = io.readAll() catch e {
        eprintln("${e}")
        os.exit(1)
    }
    var nums: Vec[int] = []
    for word in text.fields() {
        let n = word.parseInt() ?? return
        nums.push(n)
    }
    nums.sort()
    var sum = 0
    for n in nums { sum += n }
    println("${sum} ${nums.len()}")
}
```

## 2. 类型、集合和错误

- 类型参数用 `[]`：`Vec[T]`、`Map[K, V]`；元组为 `(int, String)`，用 `.0/.1`。结构体构造 `Point{x: 1}`；方法写在结构体内，修改方法声明 `inout self`。
- 字段默认值 `field: T = 常量`，限数字、布尔、字节、无插值字符串字面量（String 字段）、`none`、`[]`、`[:]`。省略可选字段默认为 `none`；其他字段须显式给值或声明默认值。
- `?T`：`some(x)`/`none`，用 `if let`、`while let`、`??` 或穷尽 `match` 处理；没有 `unwrap`。`??` 只在缺失时求右侧。
- `T ! E`：成功直接 `return value`，失败 `return err(error)`。调用处用 `try f()`、`f() catch e => value`、`f() catch e { return }`，或 `match` 的 `ok(v)/err(e)`。块式 catch 必须离开。`try` 仅用于返回相同错误类型的函数，不能在 `main` 用。

| 常用操作 | 当前写法 |
| --- | --- |
| Vec | `var v: Vec[int] = []`；`push`、`pop -> ?T`、`len`、`v[i]`、`v[i] = x`、`v[a..b]`、`contains`、稳定 `sort/sortBy` |
| Map | `var m: Map[String, int] = [:]`；`get -> ?&V`、`getMut -> ?&inout V`、`getCopy -> ?V`（V: Copy）、`has`、`set`、`remove -> ?V` |
| Map 键 | `get/has/remove` 等查询直接收 `str`；`set/update` 消费拥有的 `String` 键；`m.update(key, 0, (n) => n + 1)` |
| Set | `var s = Set[int]{}`；`add` 消费元素、`has`、`remove -> bool`、`len`；按插入顺序只读遍历 |
| 文本 | `lines/fields/split` 返回 `Vec[str]`；`trim` 返回视图；`parseInt -> ?int`、`parseFloat -> ?f64`；`toLower/toUpper` 返回 String |
| 拼接 | `var out = String{}`；`out.push(text)` 只读实参；字符串集合 `join(",")`，数字先 `format` |

`s.len()`、`s[i]`、`s[a..b]` 按 UTF-8 字节；切片边界必须是字符边界。字节 `'a'` 打印为数字 97，要输出一个 ASCII 字符用单字节切片。`String` 可直接与 `str` 比较。整数和字符串支持 Ord；浮点不满足 Ord，排序用 `v.sortBy((a, b) => a.totalCmp(b) == .Less)`。

## 3. 所有权与求值顺序

- 默认参数只读借用；修改参数写 `x: inout T`，调用写 `f(&x)`；消费参数写 `x: sink T`。拥有值赋值、装入集合或传给 sink 会移动；第二份拥有值显式 `.clone()`。
- 标量及递归满足 Copy、无析构的值自动复制。`str/[]T/&T` 复制描述符；`&inout T` 不可复制。借用构造写明类型，如 `let p: &inout int = &n`；修改目标用 `*p = 1`，`p = q` 是重绑定。
- **非 Copy 元素 `let t = v[i]` 是借用**，最后一次使用 t 之前不能修改 v。需独立值写 `v[i].clone()`。非 Copy 字段、Box 内容和集合元素不能直接移出；用 `take(&place)` 留默认值，或 `replace(&place, value)`、`swap(&a, &b)`。
- 已有可选/结果变量的匹配只借用；匹配拥有临时结果可取得所有权，例如 `if let x = v.pop()`。消费已有可选变量写 `match take(&opt)`。
- 返回值携带借用必须写来源：`-> str of text`、`-> Vec[str] of text`、`-> str of self`、`-> str of static`；不能借用局部变量或 sink 参数。借用值不能存入普通字段、Box、channel、被保存的闭包；`[]str` 转 `.toVec()` 仍携带借用，长期保存需逐个 `.toString()`。
- 不能把其他参数的新借用写入 `inout` 参数；需要这种结果时返回 `Vec[str] of text`。借用一般持续到最后一次使用；拥有者析构仍在作用域结束。
- **V2：变量/字段接收者**先预留，再从左到右求实参，所以 `v.push(v.len())` 合法；实参不得修改/移动接收者，也不得在调用时仍借住它。
- **V2a：经过下标的修改接收者**先定位并可变借用元素，再求实参；`g[i].push(g.len())` 报 E0403。先 `let n = g.len()` 再 `g[i].push(n)`。嵌套下标、元素字段和用户 `inout self` 方法同理。
- **下标赋值** `v[i] = rhs` / `v[i] += rhs` 先算 rhs，再算下标并检查越界，最后读写元素；rhs 的借用在求目标位置期间保持有效，冲突报 E0102。
- 普通调用实参逐个完成移动/借用：`consume(v, v.len())` 若第一个参数为 sink，则第二个参数不能再用 v。不要靠拆分语句改变副作用顺序。

以下独立程序演示字段默认值、克隆元素、索引写入及字面插值转义。

```yv example=ownership/main.yv
struct Bag {
    name: String = "items"
    values: Vec[int] = []
    fn add(inout self, n: int) { self.values.push(n) }
}
fn first(s: str) -> str of s { return s.fields()[0] }
fn main() {
    var b = Bag{}
    b.add(7)
    var names: Vec[String] = ["ann".toString()]
    let saved = names[0].clone()
    names.push("bo".toString())
    var grid: Vec[Vec[int]] = [[1]]
    let n = grid.len()
    grid[0].push(n)
    grid[0][0] = b.values[0]
    println("${saved} ${first(b.name)} ${grid[0][0]} ${grid[0][1]}")
    println("\${HOME}")
}
```

## 4. 源文件与线程

入口文件目录是项目根；`use geom` 加载 `geom.yv`，`use util/text` 加载 `util/text.yv`。顶层函数/类型写 `pub` 才能导出；函数用模块名限定，每个文件独立导入。以下两块组成一个项目。

```yv example=modules/geom.yv
pub struct Rect {
    w: int
    h: int
}
pub fn area(r: Rect) -> int { return r.w * r.h }
```

```yv example=modules/main.yv
use geom
fn main() { println(geom.area(Rect{w: 2, h: 3})) }
```

并发只有 OS 线程。借用的数据在 scope 前创建；线程返回值须 Send 且不携带借用。join 消耗句柄，借用仍持续到 scope 结束；scope 会等待全部线程。以下是独立程序。

```yv example=threads/main.yv
fn sum(xs: []int) -> int {
    var n = 0
    for x in xs { n += x }
    return n
}
fn main() {
    let xs = [1, 2, 3, 4]
    scope {
        let a = spawn sum(xs[0..2])
        let b = spawn sum(xs[2..4])
        println(a.join() + b.join())
    }
}
```

`use std/sync` 引入 Mutex/Channel；共享可变状态用 `Arc[Mutex[T]]`（T: Send），`with g = m.lock()` 内修改。Rc 永远不能跨线程；Arc[T] 要求 Send＋Sync。channel 只传拥有且不携带借用的 Send 值；锁守卫和从它借出的值不能逃出持锁块。API 细节按索引查。

## 5. 分配、工具与缺口

无 GC；正常离开作用域自动析构。`clone/toString/format`、集合构造/扩容、排序缓冲区和装箱可能分配；借用、普通语法和直接输出插值不分配。`yavo allocations fnName --file main.yv` 查询 no_alloc/may_alloc/unknown；unknown 不能当作无分配。

`yavo fix main.yv --dry-run` 预览修复；实际自动修复限编译器证明不改语义的操作，多文件 fix 为 U0001。`yavo verify` 的总体结果仍为 not_verified（审计未实现）。自测写小程序或 assert；没有 test 块或 `yavo test/doc/explain/fmt/audit`。

尚无 Vec.remove/insert/clear、开放切片 `x[a..]`、可变切片、Arena 遍历、元组比较/Map 键、泛型枚举、match 字面量模式、通用高阶闭包参数、std/time、std/rand、async、unsafe、C 互操作、包依赖。不要从 Rust API 名称推测 Yavo 支持；按 [主题索引](agent-api-index.md) 查询当前写法。
