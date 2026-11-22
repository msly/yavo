# API 主题：strings

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 301–307 行

| 按行、按空白、按分隔符切分 | `s.lines()`、`s.fields()`、`s.split(",")`，结果是 `Vec[str]`，不复制字符串 | 只分配集合本身 |
| 字符串长度、字节、子串 | `s.len()`、`s[i]`（`byte`）按字节；`s[a..b]` 是视图，两端必须落在字符边界上。`byte` 是整数，`"${s[i]}"` 打印数字（`'a'` 打印 `97`）；要打印一个字符写 `s[i..i+1]` | 否 |
| 修剪、大小写、查找 | `s.trim()`（视图）、`s.toLower()`、`s.toUpper()`（新 `String`）、`s.contains(t)`、`s.startsWith(t)`、`s.find(t)`（`?int`） | 只有大小写转换 |
| 解析数字 | `s.parseInt()`（`?int`）、`s.parseFloat()`（`?f64`） | 否 |
| 拼接字符串 | `var out = String{}`，`out.push("text")`；`push` 接受 `str` 或 `String`，只读取实参，不消费；或 `format("${a}-${b}")` | 是 |
| 字节转字符串 | `String.fromUtf8(bytes)` 消费字节数组；成功得 `String`，失败得 `Utf8Error{offset: ...}`，输入被释放 | 契约为可能；允许复用原缓冲区 |
| 字节字面量 | `'a'`、`'\n'`；判断：`c >= '0' && c <= '9'` | 否 |

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

