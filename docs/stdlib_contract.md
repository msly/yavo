# Yavo 最小标准库契约 v0.5

本文件把 v0.4 测试中的假设改成明确的设计决定，不表示库已实现。签名中的 `self`、`inout self`、`sink self` 分别表示借用、可变借用、消费接收者；构造器行是内建构造签名，不是新增的用户函数声明语法。`()` 是单位类型，没有返回箭头的函数等价于返回 `()`。

## 1. 导入、基础类型和默认值

prelude 提供 `Vec`、`Map`、`Set`、`String`、`Box`、`Rc`、`Arc`、`Arena`、`Id`、`Ordering`、`Utf8Error`，以及 `Copy`、`Eq`、`Ord`、`Drop`、`Send`、`Sync`。这些是类型和协议，不是自动分配操作。

`use std/m` 引入模块名 `m` 和它公开的类型名；模块函数仍写限定名。两个导入产生同名类型时，不自动选择，必须使用模块限定名。`use std/sync` 的类型包括 `Mutex`、`Guard`、`Channel`、`SendError`、`Join`；`use std/collections` 提供 `Heap`、`Deque`。

`int` 与 `i64` 是同一个类型，`byte` 与 `u8` 是同一个类型。`T.min/T.max` 是整数固有常量；类型定型支持上下文数值字面量，已有数值的类型转换必须显式。

`take` 的默认值是编译器认可的封闭集合：整数 0、浮点 +0、bool false、可选 none、空 String/Vec/Map/Set，以及各分量均有默认值的元组。用户类型不因字段有初始化表达式而自动拥有 `take` 所需默认值；Box、Rc、Arc、Mutex、Guard、Join 没有默认值。普通结构体未填写的可选字段默认为 none，其余字段必须有显式字段默认值或构造实参。

`take(&place)` 把旧值整体取出并原子地留下默认值；`replace(&place, value)` 返回旧值，保留位置始终有效；`swap(&a, &b)` 交换两个不重叠的可写位置。三者不绕过 A3/O4，不隐式克隆。容量为零的空容器可以不分配；集合字面量是“可能分配的显式构造”，不是要求空字面量必须向分配器申请内存。

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

## 6. 析构和错误丢弃

`struct T: Drop` 的唯一方法为 `fn drop(inout self)`。用户不能调用该方法，也不能用它返回错误；先本体 drop，后字段自动清理，细则见 V10/V11。Rc/Arc 的 clone 只增加显式共享计数，不深拷贝对象；String/Vec 等的 clone 按对应类型深拷贝。没有自动循环收集，不保证共享环被释放。

`catch _` 丢弃拥有错误后再进入处理体；`catch _e` 保留到体结束。`let _` 明确消费并立即释放整个拥有结果；`let _name` 只抑制未使用警告。修复工具不能把这两种情况混同。

## 7. 并发最小接口

构造与签名：

```text
Rc[T](value: sink T) -> Rc[T]
Arc[T](value: sink T) -> Arc[T]
Mutex[T](value: sink T) -> Mutex[T]
fn Mutex.lock(self) -> Guard[T] of self
fn Mutex.tryLock(self) -> ?Guard[T] of self
Channel[T]{cap: positiveInt}
fn Channel.send(self, value: sink T) -> () ! SendError[T]
fn Channel.recv(self) -> ?T
fn Channel.close(self)
fn Channel.clone(self) -> Channel[T]
fn Join.join(sink self) -> T

enum SendError[T] {
    Closed(value: T)
}
```

Mutex 不是 Copy，也不 clone；当 T:Send 时 Mutex[T] 可 Send/Sync。Guard 不 Copy、不 Clone、不 Send、不 Sync，携带 Mutex 来源和锁许可；可在同线程传给 sink 参数，但不能进入普通持久字段。`lock` 阻塞，当前线程再次获取同一把锁触发 P-LOCK-REENTRANT；`tryLock` 不等待，锁被任何线程持有时返回 none。底层系统错误触发 P-SYSTEM，不伪装成正常竞争。

`with g = m.lock()` 将 Guard 的拥有权绑定到块；守卫内的字段/标量可写，但借用只在许可有效期间可用。`let _ = m.lock()` 立即释放，`let _g = m.lock()` 保留到作用域结束。Guard 的源 Mutex 在守卫活跃期间不能被移动或释放。

Channel[T] 的 T 必须 Send 且不携带借用；cap 必须大于 0，否则 P-CHANNEL-CAP。send 在缓冲区满时阻塞；关闭时返还原值 `err(.Closed(value))`。close 幂等，唤醒等待者；recv 先排空缓冲区，关闭且空时 none。Channel 是内部同步的拥有句柄，显式 clone 共享队列，Send/Sync 受 T:Send 约束；最后一个句柄释放后，队列中尚未取出的对象恰好释放一次。

spawn 的实际参数在父线程中按 V1 求值，移动的拥有值要 Send，默认借用要 Sync，inout 的排他借用要求目标可 Send。只读视图描述符可以直接复制，但底层拥有数据必须覆盖 scope。Join 由 scope 管理，不能逃逸；join 最多一次并消费句柄，丢弃句柄不使线程脱离。线程返回值要求 Send 且不携带借用。线程创建失败 P-THREAD-SPAWN；任一线程 panic 终止整个进程。

## 8. 诊断和分配查询

`yavo check --json <file>` 的主诊断用 code/severity/primary.line/primary.column；有关来源和冲突的位置放 secondary，不能因为测试只标主行而漏报关联位置。根因产生的级联错误标记 suppressed，不作为额外主错误。`yavo fix` 的每个候选包含稳定 id、完整编辑集合、影响说明、适用条件；应用任一候选后单独重跑 check。

`yavo allocations <function> --file <source> --json` 返回 `subject`、`status`、`operations`。status 是 `no_alloc`、`may_alloc`、`unknown` 之一；发现任何已知可能分配操作则 may_alloc，否则存在未知调用则 unknown，否则 no_alloc。parameters 和 callee 的操作分别记录。动态接口实现未知不能因接口方法体不可见就判 no_alloc。

查询和 verify 用例的 setup 是运行器场景要求，不是已经运行的日志。C 运行时锁定但未审计时标 trusted_pinned，不可改成 audited。
