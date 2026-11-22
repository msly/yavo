# API 主题：io

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 291–300 行

## 5. 标准库速查

`println`、`print`、`eprintln`、`format`、`min`、`max`、`abs`、`take`、`replace`、`swap`、`assert`、`panic` 不用导入。基础类型 `Vec/Map/Set/Box/Rc/Arc/Arena/Id`、`Ordering`、`Utf8Error`、`Copy/Eq/Ord/Drop/Send/Sync` 在 prelude 中。其余按模块 `use std/<模块>` 导入，多个模块写 `use std/{io, fs}`；模块名和其公开类型进入当前文件，普通函数仍用 `fs.readText` 之类的限定名。类型重名时不能猜测来源，改用模块限定名。第三列标明操作是否可能堆分配，`yavo doc` 和 `yavo allocations` 给出同样的信息。

| 要做的事 | 写法 | 可能分配 |
| --- | --- | --- |
| 读入全部标准输入 | `let text = try io.readAll()`，返回 `String ! io.Error`；内容不是合法 UTF-8 时得到编码错误 | 是 |
| 逐行读标准输入 | `while let line = try io.readLine() { }`，返回 `?String ! io.Error`，不含换行符 | 是 |
| 读写文件 | `try fs.readText(path)`、`try fs.writeText(path, s)`；编码错误为 `fs.Error.Utf8(offset)`，从 0 开始计字节 | 都可能，包括平台路径转换 |
| 命令行参数、退出 | `os.args()` 返回 `[]str of static`，第 0 个是程序名，用户参数从 `args[1]` 开始（启动时已转换，参数不是合法 UTF-8 时程序启动即报错退出）；`os.exit(1)` | 否 |

## cheatsheet.md，第 318–361 行

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

## stdlib_contract.md，第 77–105 行

## 5. UTF-8、文件和系统输入

```text
struct Utf8Error {
    offset: int
}
String.fromUtf8(bytes: sink Vec[byte]) -> String ! Utf8Error
```

offset 是从 0 开始的第一个非法编码序列起点；结尾不完整序列报告该序列起点。成功可复用缓冲区，失败释放输入字节并返回错误；需要保留原字节时先显式 clone。调用被标记“可能分配”，不承诺无分配。

```text
enum fs.Error {
    Utf8(offset: int)
    Io(code: int)
    InvalidPath
}
fn fs.readText(path: str) -> String ! fs.Error
fn fs.writeText(path: str, text: str) -> () ! fs.Error
```

这是模块中的公开类型/API 展示法。匹配写 `err(.Utf8(offset))`，构造可写 `err(fs.Error.Utf8(offset: n))`。io.Error 具有对应 Utf8/Io 变体。内含 NUL 的路径返回 InvalidPath，不能截断后访问另一个路径。原生非 Unicode 路径 v1 暂不支持；文本输入编码错误不做替换。Windows 路径转换可能分配，读写均标记“可能分配”。

```text
fn os.args() -> []str of static
```

启动时一次转换并验证，不能转换的参数在执行 main 之前输出启动错误并非零退出；每次调用 args 不分配。Unix 的原始字节参数和 Windows UTF-16 异常输入分别在其原生 runner 注入，不能把一平台的参数格式强塞给另一平台。

