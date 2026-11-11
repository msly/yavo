# Yavo 测试集（草案 v0.4）

这是“语义表”第 9 节的测试要点落成的测试文件。编译器还不存在，所以这些测试都没有运行过；它们是 P1 到 P3 的验收依据，也是对规范的反向检查。写测试时发现的规范缺口列在文末，冻结 P1 前需要先定下来。

## 目录

| 目录 | 内容 | 阶段 |
| --- | --- | --- |
| `check/` | 类型、移动、借用、来源、逃逸；只运行 `yavo check` | P1 |
| `fix/` | `yavo fix` 是否自动应用补丁；`*.fixed.yv` 是期望结果 | P1 |
| `run/` | 运行结果、panic 类别、析构顺序、系统输入 | P2 |
| `concurrency/` | 线程能力、`scope`、锁守卫、channel | P3 |
| `report/` | `yavo verify` 验证报告的场景和期望字段（JSON） | P3 |

另有 `ERRORS.md`（诊断码和 panic 类别）和 `MANIFEST.json`（全部测试的机器可读清单）。

## 文件格式

每个 `.yv` 文件以注释头开始：

```
// id: T-O2
// kind: check-fail
// phase: P1
// rule: O2
// summary: 声明 of a，实际返回 b 的视图；错误报在函数内
```

预期的诊断写在出错的那一行末尾：`//~ ERROR E0303` 或 `//~ WARN W0001`。一行可以有多个，用多个标注。

| kind | 判定方法 |
| --- | --- |
| `check-pass` | `yavo check` 没有错误；允许有警告 |
| `check-fail` | 每个 `//~ ERROR` 标注的行都报出了对应的错误码，并且没有其他错误。文件必须能解析：因语法错误被拒绝不算通过 |
| `fix-auto` | 先满足 `//~` 标注；`yavo fix` 自动修改文件，结果与 `expect-fixed` 指定的文件一致（比较时忽略注释），修改后的文件通过 `check` |
| `fix-candidate` | 先满足 `//~` 标注；`yavo fix` 不修改文件，诊断里给出 `expect-candidates` 列出的候选补丁，并说明各自的影响 |
| `run-pass` | 编译运行，退出码为 0，标准输出与 `expect-stdout` 下以 `| ` 开头的各行一致 |
| `run-panic` | 编译运行，非零退出，stderr 中的 panic 记录类别等于 `expect-panic` |
| `run-exit` | 按 `args-hex` 传入参数运行，退出码与 `expect-exit` 一致，`main` 没有执行（标准输出为空） |
| `report` | 按 `setup` 运行 `yavo verify`，报告里的字段满足 `expect` |

其他注释头：`platforms`（只在列出的平台运行）、`fixture` 和 `cwd`（运行时需要的输入文件）、`sanitize`（必须在这些 Sanitizer 下运行）。

`yavo fix` 拆出的临时变量按“接收者名 + 方法名首字母大写”命名，冲突时加数字后缀，例如 `v.len()` 拆成 `vLen`。

## 覆盖情况

76 个测试：check 47、fix 4、run 9、并发 14、报告 2；P1 51 个，P2 9 个，P3 16 个。

同时有正例和反例的规则：A1、A3、A4、A5、复制表（第 2 节）、C1 到 C6、O1、O2、O4、O5、E2、K1、V1、V7、V8、P1 到 P6、S5、第 7 节的数值规则。

只有一侧的规则：运行行为类规则（V3 到 V6、S3、M5）由期望输出或 panic 类别判定，一侧就够；E1、E4、E5、A2 借用其他规则的测试覆盖。

还没有测试的规则：A2 的反例、E3、O3、K2、K3、S1、S2、S4、M1 到 M4。

## 写测试时发现的规范缺口

这些都是测试必须做出假设的地方，P1 冻结前要在语义表或速查卡里定下来。

1. **标量借用怎么写入没有定义。** `&inout int` 没有类似 `*p = 1` 的写法；测试只通过字段写入（`c.n = 2`）。
2. **`?T` 在 `match` 里的模式没有定义。** 语义表 C5 写了 `match take(&opt)`，但没有规定 `some(x)`、`none` 这类模式；测试暂用 `if let`。
3. **`Vec[T]` 传给 `[]T` 参数的自动借用不在白名单里。** 语义表第 8 节只举了 `String` 传给 `str`；测试（`largest(words)`、`spawn sumAll(data)`）假设可以。
4. **析构函数的写法没有定义。** 测试假设 `struct X: Drop` 并实现 `fn drop(inout self)`。
5. **并发库没有定义。** 测试假设 `use std/sync` 提供 `Mutex(...)`、`Arc(...)`、`Rc(...)`、`Channel[T]{cap: n}`、`Guard[T]`、`m.tryLock()`。
6. **几个标准库细节是测试做的假设：** `m.remove(k)` 返回 `?V`；`m.update` 的键是拥有的 `String`；`i64.min`；`Ordering` 写作 `.Less`；文件编码错误是 `fs.Error.Utf8(offset)`。
7. **速查卡第 7 节“线程借用了临时值”一行与复制规则矛盾。** 切片描述符可以复制，按 v0.4 规则 `spawn f(v[a..b])` 应当合法；这一行沿用自 Gojo。测试改为检查“线程借用了在 `scope` 内部声明的局部变量”（T-P3f）。
8. **从拥有的局部 `Box` 里移出字段是否允许（部分移动）没有说清。** T-F2 按“一律不能直接移出字段”处理。
9. **编译期就能确定的溢出、越界移位是否允许在编译期报错。** 运行类测试把这些值放进函数返回值里，并假设 v1 一律在运行时 panic。
10. **`catch` 不用错误值时的写法。** 测试用 `catch _e`，按“未使用变量”规则加下划线；是否允许单独写 `_` 还没定。
