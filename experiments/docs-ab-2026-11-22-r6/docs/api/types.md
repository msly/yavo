# API 主题：types

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 40–133 行

## 2. 类型

| 类型 | 写法 | 说明 |
| --- | --- | --- |
| 整数 | `int`、`i8`…`i64`、`u8`…`u64`、`byte` | `int` 是 64 位有符号 |
| 浮点、布尔 | `f64`、`f32`、`bool` |  |
| 字符串 | `String`（拥有）、`str`（视图） | 始终是合法 UTF-8；字面量是 `str`，`s.toString()` 得到 `String` |
| 动态数组 | `Vec[T]`，字面量 `[1, 2, 3]` | 切片视图是 `[]T` |
| 映射、集合 | `Map[K, V]`（字面量 `[:]`）、`Set[T]`（`var s: Set[String] = []` 或 `Set[int]{}`） | 按插入顺序遍历 |
| 元组 | `(int, String)` | 用 `.0`、`.1` 取，或 `let (a, b) = t` |
| 借用 | `&T`（只读）、`&inout T`（可变） | `*p = v` 修改目标，`p = q` 重新绑定；构造时写如 `let p: &inout int = &n` |
| 可选、结果 | `?T`、`T ! E` | 见第 3 节 |
| 堆和共享 | `Box[T]`、`Rc[T]`、`Arc[T]`、`Arena[T]` 加 `Id[T]` | 见第 4 节 |

```
struct Point {
    x: f64
    y: f64 = 0.0                      // 字段默认值：只能是常量（数字、true/false、字符串字面量、none、[]、[:]）

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
- 比较：`==` 需要类型实现 `Eq`，`<` 等需要实现 `Ord`；整数、`str`、`String`、可比较元素组成的元组自动实现。两边类型必须相同，例外是 `String` 和 `str` 可以直接比较（`name == "bob"`、`s < t`）。浮点数可以直接用 `==`、`<`（按 IEEE 754），但不满足 `Eq`、`Ord`：不能用 `v.sort()` 或 `largest[f64]`，排序写 `v.sortBy((a, b) => a.totalCmp(b) == .Less)`。自定义类型写 `struct Version: Ord` 并实现 `cmp` 方法。
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


## semantics.md，第 27–85 行

## 2. 复制、移动与读取

普通参数默认是只读借用，不受下表影响；下表只说明按值赋值和传给 `sink` 参数时发生什么。

| 类型 | 按值赋值或传给 `sink` 时 | 说明 |
| --- | --- | --- |
| 标量（整数、浮点、`bool`、`byte`）、`Id[T]` | 复制 |  |
| `str`、`&T`、`[]T`（v1 只有只读切片） | 复制描述符 | 不复制被引用的数据；副本携带同样的来源 |
| `&inout T` | 不可复制 | 只能移动或受检查地再借用 |
| `String`、`Vec[T]`、`Map[K, V]`、`Box[T]` 等拥有值 | 移动 | 要第二份写 `.clone()` |
| 携带借用的拥有值，如 `Vec[str]` | 移动 | 容器拥有自己的存储，元素借用外部数据；借用关系随容器移动 |
| 无用户析构、各有效载荷递归满足 Copy 的元组、可选值、结果值、结构体和枚举 | 复制 | 仍须满足字段逃逸规则；实现 Drop 或包含需析构资源的类型不为 Copy |

- **C1 携带借用。** 一个类型直接或递归地包含 `str`、`&T`、`&inout T` 或 `[]T` 时，就“携带借用”。携带借用的值服从第 4 节的逃逸限制，即使它自己拥有存储。销毁它只释放它自己的存储，不释放被引用的数据。
- **C2 `v[i]` 的含义。** 在读取上下文中：元素可复制时得到副本（`str` 元素得到描述符副本，指向原字符串，而不是容器的槽位）；元素不可复制时得到 `&T`，同时借用容器。在赋值左侧或 `&v[i]` 中得到可变位置，要求可变访问路径。v1 没有可变切片，修改元素用 `inout Vec[T]` 或元素的可变借用。
- **C3 `.toVec()` 与 `.toString()`。** `.toVec()` 只让容器拥有元素存储，元素是否携带借用取决于元素类型：`[]str` 得到的 `Vec[str]` 仍然携带借用；要完全独立，逐个转成 `Vec[String]`。`.toString()` 得到不携带借用的 `String`。
- **C4 读取可复制的值。** 期望类型是可复制的 `T`、实际是 `&T` 或 `&inout T` 时，编译器自动读取一份 `T`：复制的是被借的值，不是复制引用。普通遍历得到 `&int`，`for ... in &map` 得到 `&inout int`；两者都可在期望 `int` 的位置复制读取，`items.push((k.clone(), n))` 合法。无期望值类型的 `let q = p` 保留描述符类型；若 `p` 是可变借用则移动它，而不是自动读取目标。
- **C5 模式匹配的所有权。** 对已有位置（变量、字段、集合元素）做匹配时，按访问权限借用其中的值；对拥有的临时结果（如 `ch.recv()`、`v.pop()`、函数返回值）做匹配时，可以把其中的值移出来。要消费一个已有的可选变量，写 `match take(&opt)`。这是禁止部分移动的限定例外：对完整拥有的临时可选值、结果值、枚举或元组做解包，可以移出其有效载荷。直接从普通字段、`Box` 内容或集合元素移出非 Copy 值一律禁止，即使外层对象由当前函数唯一拥有。已有位置仍用 `take`、`replace`、`swap`，或复制/借用；不启用 Rust 式任意部分移动。实现了用户 Drop 的类型不能被模式解体。
- **C6 借用键查询。** `Map[String, V]`、`Set[String]` 的查询接受 `str`，不创建临时 `String`；插入接收拥有的键。查询形式和拥有形式的哈希与相等语义必须一致。

- **C7 可选模式和通配符。** `?T` 的构造为 `some(value)`、`none`，模式为 `some(x)`、`none`，`if let x = opt` 等价于只处理 `some(x)` 的匹配。`match` 必须穷尽。对已有位置做匹配只借用；对拥有临时值的 `some(x)` 绑定取得拥有值。临时有效载荷匹配 `_` 时在进入该分支前丢弃；匹配已有位置的 `_` 不移动或销毁原值。命名 `_x` 保留值到分支作用域结束，只抑制未使用警告。
- **C8 字段和整体移动。** `sink` 可以接收整个拥有变量；不能因此允许 `sink` 接收非 Copy 字段。`let (a, b) = pair` 对已有位置按 C5 借用，`let (a, b) = makePair()` 可消费完整临时元组。`Box` 可整个移动，但不增加直接取出其内容或字段的特殊语法。

```text
var opt: ?String = some("job".toString())
match take(&opt) {
    some(job) => consume(job)  // job 是拥有值
    none => println("empty")
}
```

## 3. 借用来源（`of`）与写入关系

- **O1 所有具名函数和方法都写来源。** 包括私有函数和接口方法；返回值携带借用时（含 `Vec[str]`、可选值、元组、结果类型里的借用），签名必须写 `of`。泛型返回类型里的类型参数可能携带借用时，也必须写来源上界；实例化后不携带借用（如 `largest[int]`），这个标注就不产生借用约束。纯拥有值的返回不写。

```
fn trimmed(text: str) -> str of text
fn choose(a: str, b: str, pickA: bool) -> str of (a, b)
fn name(self) -> str of self
fn languageName() -> str of static
fn words(text: str) -> Vec[str] of text
fn largest[T: Ord + Copy](xs: []T) -> ?T of xs
```

| 写法 | 含义 |
| --- | --- |
| `of text` | 来自参数 `text` |
| `of self` | 来自接收者 |
| `of (a, b)` | 可能来自其中一个或多个；调用方按全部列出的来源保守检查 |
| `of static` | 只指向程序全程有效的数据，例如字符串字面量、`os.args()` |

- **O2 来源是上界。** 编译器检查函数体没有超出声明；调用方只看签名，不读函数体去缩小或扩大来源。实现改了而违反声明时，错误报在函数内部。`of` 只能指向借用参数（默认参数、`inout` 参数）或借用的接收者，不能指向 `sink` 参数或局部变量：来源匹配只是必要条件，`of` 不会延长拥有者的寿命。所以 `fn bad(text: sink String) -> str of text` 会被拒绝。
- **O3 不自动补 `of`。** 缺失或写错时，编译器给出候选补丁，但不自动应用，也不自动扩大已声明的来源。
- **O4 不能通过参数引入新的外部来源。** 对普通函数的每个 `inout` 参数，退出时所携带的来源只能来自该参数进入函数时已有的来源或 `static`，不能新增其他参数或局部拥有者的来源。规则同时检查赋值、`push`、`set`、`replace`、`swap` 和回调写入，不能借内建调用绕过。需要新增来源时改为返回带 `of` 的拥有容器。局部容器允许聚合来源。内建操作有固定的前态/后态来源契约：例如 `out.push(s)` 合并来源，`get` 返回元素槽位借用，复制出的 `str` 只保留底层文本来源。来自输入的来源集合须在调用前快照，不能把后写入的来源倒灌给已返回的旧值。
- **O5 闭包。** 不逃逸的匿名闭包（立即传给 `sortBy`、`update` 等）可以借用外部变量，来源在当前函数内推断。v1 只有编译器已知 noescape 契约的标准库回调可以这样借用；没有契约的用户高阶参数按可能逃逸处理，借用捕获被拒绝，不开放临时推断出来的隐含例外。会被保存的闭包（如 `Heap.by` 的比较函数、放进字段的闭包）不能捕获任何借用，只能移动捕获拥有值。

- **O6 参数视图的描述符。** `str`、只读 `[]T` 和 `&T` 传参时复制小型描述符，来源指向实际数据，不指向调用者用于保存描述符的局部槽位。默认 `Vec[T]` 参数借用拥有容器；`Vec[T]` 到 `[]T` 的自动视图转换见第 8 节。泛型来源可以保守合并参数存储与其嵌套来源；内建 `str` 和 `[]T` 操作必须区分描述符存储与底层数据，避免临时容器销毁造成假悬空或真悬空。
- **O7 库特殊资源。** `Guard[T]`、`Join[T]` 是带编译器资源契约的内建类型，不以普通用户字段模拟。`Guard` 携带锁的来源与许可；`Join` 受所属 scope 限制。这些类型同样不能进入普通长期字段、共享盒、channel 或全局变量，除非将来增加独立且经过测试的规则。

