# API 主题：unsupported

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 511–513 行

## 9. 尚未实现（用了会报 `U0001` 或语法错误）

`test` 块与 `yavo test`、`yavo fmt/doc/explain/audit`；`std/time`、`std/rand`；`Vec` 的 `remove`、`insert`、`clear`；开放切片 `x[a..]`、`x[..b]`（写 `x[a..x.len()]`）；遍历 `Arena`；元组做 `Map` 键、元组比较和打印；泛型枚举；带载荷枚举的 `==`；`match` 里的字面量模式（`.Num(0)`、`1 =>`）；一般闭包值和把闭包当参数传给自己的函数（`sortBy`、`update`、`Heap.by` 可以）；`spawn` 之外的线程写法，`spawn` 的实参不能是 `&inout`；包依赖、`unsafe`、C 互操作。
