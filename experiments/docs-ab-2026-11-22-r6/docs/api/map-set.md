# API 主题：map-set

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 310–312 行

| `Map` 读取 | `m.get(k)`（`?&V`）、`m.getMut(k)`（`?&inout V`）、`m.getCopy(k)`（`?V`，要求可复制）、`m.has(k)`、`m.len()`；`Map[String, V]` 的 `get`、`getMut`、`getCopy`、`has`、`remove` 直接收 `str` 键，如 `m.get("a")`；`set`、`update` 要拥有的 `String` 键；默认值写 `m.getCopy(k) ?? 0`；值不可复制（如 `String`）时没有 `getCopy`，用 `if let s = m.get(k)` 再 `s.clone()` | 否 |
| `Map` 写入和计数 | `set(k, v)` 消费键和值；`remove(k) -> ?V` 返回拥有值；`update(k, initial, f)` 消费拥有键，v1 限不携带借用的 Copy 值，f 恰好执行一次 | 插入可能；remove 本身不分配 |
| 遍历 | `for x in v`、`for i, x in v`、`for k, v in m` 默认只读；`in &v`、`in &m` 显式可变；Map 键始终只读 | 否 |

## cheatsheet.md，第 314–314 行

| 双端队列、集合 | `use std/collections`；`var d = Deque[T]{}`：`pushBack`、`pushFront`、`popFront`、`popBack`（`?T`）、`len`，没有下标和遍历；`Set[T]`（prelude）：`s.add(x)`（消费元素，已有则不变）、`s.has(x)`（`Set[String]` 用 `str` 查询）、`s.remove(x)`（返回 `bool`）、`s.len()`，`for x in s` 按插入顺序只读遍历 | 插入时会 |

## stdlib_contract.md，第 33–60 行

## 3. Map、Set 和更新

以 `Map[String,V]` 为例：

```text
fn get(self, key: str) -> ?&V of self
fn getMut(inout self, key: str) -> ?&inout V of self
fn getCopy(self, key: str) -> ?V of self      // V: Copy；V 不带借用时来源约束消失
fn has(self, key: str) -> bool
fn len(self) -> int
fn set(inout self, key: sink String, value: sink V)
fn remove(inout self, key: str) -> ?V of self // 来源仅对 V 含借用时生效；不借住已移出的槽位
```

`set` 插入新键和值；已有键时保留原键和插入位置，释放传入的重复键及旧值，然后保存新值。`remove` 删除条目，释放条目拥有的键，返回拥有的值；不存在返回 none。返回值本身持有的外部借用来源保留，但不能返回已删除槽位的引用。`get` 返回的槽位借用和 `getCopy` 复制出的描述符来源不是同一层关系，由内建来源契约区分。

`Map[str,V]` 允许作为局部视图容器存在，写入合并键和值来源；仍不允许进入普通长期字段。`Set[String]` 查询接受 str；`add` 消费拥有键，`remove` 返回 bool。插入顺序固定，删除再插入位于末尾。

v1 的 `update` 限于不携带借用的 `V: Copy`：

```text
fn update(inout self, key: sink K, initial: V, f: fn(V) -> V)
```

回调不逃逸、恰好调用一次：已有键传旧值副本，否则传 initial。计算结果后保存；`initial` 是普通实参，总会按求值顺序计算，并不是惰性默认值。键为 String 时必须显式取得拥有的 String；不能把 str 隐式分配成 String。回调执行期间不得再次访问已经可变借用的 Map。

Map/Set v1 的键限于编译器内建 Hash+Eq 集合：整数、bool、byte、str、String、Id 和由支持元素组成的元组。String 与 str 的哈希按相同 UTF-8 字节计算。用户 Ord 不自动提供 Hash；float 不作为键。用户可扩展哈希协议不进入本轮。

