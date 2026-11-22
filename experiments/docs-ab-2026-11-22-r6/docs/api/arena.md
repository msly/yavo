# API 主题：arena

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 267–289 行

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

## cheatsheet.md，第 315–315 行

| Arena | `var a = Arena[T]{}`；`a.add(x)` 返回 `Id[T]`；`a.get(id)`（`?&T`）、`a.getMut(id)`（`?&inout T`）、`a.getCopy(id)`、`a.remove(id)`（`?T`，拥有的值）、`a.has(id)`、`a.len()`。Arena **不能遍历**：需要按顺序访问时，自己另存一个 `Vec[Id[T]]`。`Id` 可复制，可比较、排序，可做 `Map`/`Set` 键 | `add` 时会 |

## semantics.md，第 136–167 行

## 7. 数值、字符串与内存

| 情况 | 行为 |
| --- | --- |
| 加、减、乘、取负溢出 | panic；生成的 C 用 `__builtin_*_overflow` 检查 |
| 除以 0、对 0 取余 | panic |
| 最小整数除以 -1，或对 -1 取余 | panic |
| 移位量为负，或不小于位宽 | panic |
| 有效移位的结果 | 左移丢弃高位；无符号右移补零，有符号右移补符号位；不依赖 C 编译器的默认行为 |
| 窄化转换超出范围，如 `i32(x)` | panic |
| 负数转无符号 | panic |
| 截断转换 `T.wrap(x)` | 只接受整数；对目标位宽 N，先算数学上的 x mod 2^N，无符号目标直接取这个值，有符号目标按 N 位补码解释。例如 `i8.wrap(130)` 是 -126，`u8.wrap(-1)` 是 255 |
| 浮点转整数 | 向零截断；NaN、无穷或超出范围时 panic |
| 浮点运算和比较 | 按 IEEE 754，不 panic；`NaN != NaN`。浮点数不满足 `Eq`、`Ord`，见第 8 节 |
| `+%`、`-%`、`*%` | 按位宽回绕 |

`-fwrapv` 只是有符号加、减、乘的回绕兜底，不实现上面的 panic，也不覆盖除法、移位和转换；每种运算都有独立的生成规则和边界测试。

- **N1 常量可知的失败。** 普通表达式的溢出、除零、非法移位、越界和失败转换仍按执行时 panic 定义；即使编译器能预先算出，也不能仅因此把有效程序变成编译错误。优化可生成无条件 panic，但必须保留原来的执行路径、先前副作用和 panic 类别。未执行的分支不 panic。默认不产生会被 CI 升级的警告。
- **N2 字面量类型错误。** 无法表示为目标类型的原始字面量（如 `let b: u8 = 256`）是编译错误 E0003；`u8(256)` 是合法的运行时转换表达式，执行时 P-CONVERT。未标注整数/浮点字面量默认 `int`/`f64`，上下文可直接确定字面量类型，这不是已有运行时值的隐式转换。整数 `min/max` 为类型固有常量；`int` 是 `i64` 的别名，`byte` 是 `u8` 的别名。

- **S1** `String` 和 `str` 始终是合法的 UTF-8。
- **S2** `s.len()` 和 `s[i]` 按字节计算，`s[i]` 返回 `byte`。
- **S3** `s[a..b]` 的两端必须落在字符边界上，否则 panic。
- **S4** 任意字节先放进 `Vec[byte]`，再用 `String.fromUtf8(bytes)` 验证，得到 `String ! Utf8Error`；没有 `pushByte`。
- **S5 系统输入。** 文件和标准输入的文本读取遇到非法 UTF-8 时返回编码错误。命令行参数在启动时转换，无法表示为合法 UTF-8 时打印诊断并以非零状态退出。两者都不做静默替换。v1 没有原生路径类型，暂不覆盖所有系统路径。
- **M1 内存耗尽** 时终止进程，和 panic 一样不展开栈。
- **M2 容量计算溢出**（元素个数乘以元素大小等）时 panic，不会分配出过小的缓冲区。
- **M3 分配契约。** 标准库每个操作标注“不分配”或“可能分配”。`yavo allocations` 对一个函数给出三种结果：已知不分配、可能分配、未知（接口调用、C 函数、未知闭包）。参数求值单独计算：`println("${makeString()}")` 里 `println` 不分配，但 `makeString()` 可能分配。
- **M4 集合字面量。** `[1, 2, 3]`、`[:]` 是显式的分配构造；除此之外，普通语法不引入堆分配。
- **M5 `os.args()`** 在进程启动时转换一次（Windows 上这一步会分配），失败按 S5 处理；之后返回 `[]str of static`，调用本身不分配。

