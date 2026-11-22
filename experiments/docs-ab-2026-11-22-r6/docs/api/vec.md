# API 主题：vec

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 308–309 行

| `Vec` 基本操作 | `v.push(x)`、`v.pop()`（`?T`，拥有的值）、`v.len()`、`v[i]`、`v[i] = x`、`v[i] += x`、`v[a..b]`、`v.contains(x)`、`v.join(", ")`（元素必须是 `str` 或 `String`，数字先 `format` 成 `String`）；v1 没有 `remove`、`insert`、`clear`，要删除或插入中间元素就构造新的 `Vec` | `push` 扩容和 `join` 时会 |
| 排序 | `v.sort()`（要求 `Ord`）；`v.sortBy((a, b) => a.1 > b.1)`；浮点数用 `sortBy` 加 `totalCmp`；两者都是稳定排序 | 是（临时缓冲区） |

## cheatsheet.md，第 312–312 行

| 遍历 | `for x in v`、`for i, x in v`、`for k, v in m` 默认只读；`in &v`、`in &m` 显式可变；Map 键始终只读 | 否 |

## stdlib_contract.md，第 17–32 行

## 2. Vec、切片和读取

```text
Vec[T]{}
fn len(self) -> int
fn push(inout self, value: sink T)
fn pop(inout self) -> ?T
fn reserve(inout self, capacity: int)
```

以上是接口概要。T 携带借用时，内建 `push/pop` 通过编译器来源契约保留相应来源；不能把这些概要当作用户函数绕过 `of`。`reserve` 保证总容量至少为 capacity，不改变长度；负数、容量乘法/指针跨度超界触发 P-CAPACITY，分配器失败触发 P-OOM。

`Vec[T]` 传给只读 `[]T` 形参时自动形成切片，不复制元素、不分配。`let s = v[0..v.len()]` 是显式形成局部切片的写法。v1 不支持可变切片；`[]T` 的 var 绑定不会增加修改数据的权限。切片传参复制描述符，不借用描述符槽位。

只读遍历 `for x in v` 取得只读元素访问；可写遍历 `for x in &v` 取得排他元素访问。其余权限及循环借用边界见 A2/A8。`[]T.toVec()` 在 v1 要求 `T: Copy`，复制元素，不自动深拷贝；非 Copy 元素显式逐项 `.clone()`。如果元素是 `str`，新容器仍借用文本。


## stdlib_contract.md，第 61–76 行

## 4. 比较和排序

```text
enum Ordering {
    Less
    Equal
    Greater
}
```

`Ordering` 是 prelude 类型，支持 Eq/Ord，顺序为 Less < Equal < Greater。显式实现 Ord 的用户类型提供 `fn cmp(self, other: &Self) -> Ordering`；Eq 由 cmp==Equal 产生。只实现 Eq 时提供 `fn eq(self, other: &Self) -> bool`。普通比较不会为不同数值类型自动转换。

`sort()` 要求 Ord，`sortBy((a,b)=>bool)` 的 true 表示 a 应在 b 前，要求严格弱序。两种排序都稳定；错误比较器只能导致错误的排列、保留原序或 P-COMPARATOR，不得越界、破坏元素所有权或无限循环。验收检查元素多重集合保持、Sanitizer 和超时；不从“一个错误比较器未崩溃”推导一般安全证明。

浮点直接比较仍为 IEEE 语义，不实现 Eq/Ord。`totalCmp` 对同类型的所有位模式给出确定全序（负 NaN、负无穷、负有限数、-0、+0、正有限数、正无穷、正 NaN；同组 NaN 的符号/有效载荷按固定的位模式顺序区分），v1 用符号位翻转/取反后的无符号键实现；不启用 fast-math。

