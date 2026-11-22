# API 主题：errors

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 134–200 行

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
- `if let`/`match` 对**临时值**（如 `if let x = v.pop()`）取得拥有的值；对**已有变量**（`let o = v.pop()` 之后 `if let x = o`）只借用，要拿走里面的值写 `match take(&o) { some(x) => … none => … }`。
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


## semantics.md，第 44–55 行

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

## semantics.md，第 127–133 行

- **V8 `return` 和分支取值。** 有返回值的具名函数、方法和块体闭包，每条能正常结束的路径都必须写 `return`；没有返回值的函数可以自然结束；永不返回的路径（`panic`、`os.exit`）不需要 `return`。`return` 总是从当前函数或闭包返回，不给分支提供值。需要分支取值时用表达式形式：单表达式闭包 `(a, b) => a < b`、`match` 的 `=> 表达式` 分支、`f() catch e => 替代值`。块形式的 `catch e { }` 不能正常走到结束的花括号；这一点按控制流判断，而不是看块里有没有出现 `return` 等关键字。
- **V9 `??` 短路。** 只有左侧是 `none` 时才计算右侧。

- **V10 用户析构。** 使用 `struct T: Drop { fn drop(inout self) { ... } }`。仅编译器可以调用这个方法；用户不能显式调用 `.drop()`。它返回 `()`，不能声明 `! E`，失败须在方法内处理或 panic。正常释放时先调用本体 drop，再按字段声明逆序自动析构字段；完整元组按位置逆序、容器按元素索引/插入顺序逆序，枚举只析构当前有效载荷。部分初始化失败只析构已完成字段（逆完成顺序），不调用未完成整体的用户 drop。
- **V11 析构安全边界。** 有 Drop 或含需析构资源的类型不自动 Copy；移动转移唯一析构责任，`clone` 必须显式提供有效语义。析构体不得把 `self` 整体移动、`take`、覆盖或替换，也不得让其借用逃逸；可以通过 `take/replace` 操作有完整替代值的字段。panic、内存耗尽和 `os.exit` 终止进程，不执行栈展开；普通错误返回执行正常清理。借用活跃时不能析构拥有者。
- **V12 错误绑定。** 允许 `catch _ => value`、`catch _ { ... }`。拥有的错误值在处理体开始前丢弃；`catch _e` 保留错误值到该处理体离开时。`try`、`catch` 对完整拥有结果执行整体消费，不允许从借用中搬出非 Copy 的成功值或错误值。修复未使用的 `e` 只能改 `_e`，不能自动改成 `_`。
- **V13 自动修复命名。** （2026-11-21：V2 修订后 `v.push(v.len())` 本身合法，不再产生诊断，下述命名规则保留给其他已证明安全的拆分。）固定模式 `v.push(v.len())` 生成 `vLen`；名字冲突使用 `vLen2`、`vLen3`，取当前作用域及补丁覆盖范围内首个不捕获现有绑定的名字。同一文件按源码位置升序产生补丁；下次运行不重复修改。未使用变量按 `_name`、`_name2` 命名。复杂接收者和条件求值不进入自动拆分白名单。候选补丁必须完整、可单独应用并重新通过 check；`let b` 上的 `take` 候选须包含合法化写权限的修改并说明影响。
