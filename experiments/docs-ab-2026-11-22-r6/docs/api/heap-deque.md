# API 主题：heap-deque

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 313–314 行

| 优先队列 | `use std/collections`；`var h = Heap[(int, int)].by((a, b) => a.0 < b.0)`，`h.push(x)`、`h.pop()`（`?T`）；比较闭包会被保存，不能捕获借用 | `push` 扩容时会 |
| 双端队列、集合 | `use std/collections`；`var d = Deque[T]{}`：`pushBack`、`pushFront`、`popFront`、`popBack`（`?T`）、`len`，没有下标和遍历；`Set[T]`（prelude）：`s.add(x)`（消费元素，已有则不变）、`s.has(x)`（`Set[String]` 用 `str` 查询）、`s.remove(x)`（返回 `bool`）、`s.len()`，`for x in s` 按插入顺序只读遍历 | 插入时会 |
