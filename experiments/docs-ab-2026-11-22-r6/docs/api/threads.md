# API 主题：threads

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 410–444 行

### 并发

并发只有 OS 线程：`scope` 块里用 `spawn` 启动线程，块结束时等待所有线程。`use std/sync` 引入 `Mutex`、`Guard`、`Channel`、`SendError`、`Join`。`Rc/Arc` 已在 prelude 中。底层数据必须在 scope 外已存在；复制临时切片描述符合法，借用临时拥有数组不合法。

```
fn sum(xs: []int) -> int {
    var s = 0
    for x in xs {
        s += x
    }
    return s
}

fn parallelSum(nums: Vec[int]) -> int {
    let mid = nums.len() / 2
    let left = nums[0..mid]               // 局部命名只是可读性选择；spawn sum(nums[0..mid]) 也合法
    let right = nums[mid..nums.len()]
    var total = 0
    scope {
        let a = spawn sum(left)
        let b = spawn sum(right)
        total = a.join() + b.join()
    }
    return total
}
```

- 线程借用保守持续到 scope 结束；`join()` 消耗句柄取得返回值，但不提前缩短借用。丢弃句柄不会分离线程。中途 `return` 或 `try` 也先等待。v1 线程返回值必须不携带借用且满足 Send。
- 线程之间传值用 channel：`let ch = Channel[Job]{cap: 8}`，传给线程函数时写 `spawn worker(&ch)`，参数类型 `&Channel[Job]`，多个线程可以共用一个 channel 收发。`ch.send(v)` 移动值，关闭后返回 `err(.Closed(v))` 把值还回来；`ch.recv()` 返回拥有的 `?Job`，关闭后先取完缓冲区，再返回 `none`，所以工作线程写 `while let job = ch.recv() { }`；`ch.close()` 关闭。channel 只传拥有值，不传 `str`、`Vec[str]` 这类携带借用的值。
- `Channel[T]{cap: n}` 要求 `n > 0`；关闭幂等并唤醒等待者。`Mutex(T)` 接收拥有值，`m.lock() -> Guard[T] of self`，`m.tryLock() -> ?Guard[T] of self`；竞争时 tryLock 返回 none。
- 共享可变状态用 `Arc[Mutex[T]]`，要求 `T` 可以交给别的线程（`Send`）：`with g = m.lock() { g.count += 1 }`。从 `g` 借出的引用不能带出 `with` 块；锁守卫也不能交给别的线程。
- 通过 `Rc`、`Arc` 只能读里面的值，把句柄放进 `var` 也一样；要修改就用 `Mutex`。
- `Arc[T]` 跨线程要求 `T` 同时满足 `Send` 和 `Sync`；`Rc` 永远不能跨线程，所以 `Arc[Mutex[Rc[T]]]` 也不行。接口值默认不能跨线程；`Box[Named + Send]` 只能转交给另一个线程，要被多个线程共享还需要 `+ Sync`。
- panic 直接终止整个进程，不展开栈，也不会只结束一个线程；可能失败的操作要用 `T ! E`。


## semantics.md，第 94–115 行

## 5. 并发与资源

编译器从类型结构自动推导两种能力：`Send`（拥有值可以交给另一个线程）和 `Sync`（`&T` 可以被多个线程同时使用）。v1 不允许手写这两种能力。

| 操作 | 条件 |
| --- | --- |
| 把拥有值 `T` 交给另一个线程（`spawn` 的移动参数、channel 发送） | `T: Send` |
| 多个线程共享只读借用 `&T` | `T: Sync` |
| `Arc[T]` 跨线程 | `T: Send` 且 `T: Sync` |
| `Arc[Mutex[T]]` 跨线程 | `T: Send` |
| `Rc[T]` | 永远不能跨线程 |

- **P1 `Arc[Mutex[Rc[T]]]` 必须拒绝。** 锁只保护它里面那一份值，保护不了 `Rc` 在别处的别名；这个类型也不能作为任何诊断的自动修复结果。
- **P2 接口值的能力。** `Box[Named]` 擦除了具体类型，默认既不是 `Send` 也不是 `Sync`；需要跨线程时写 `Box[Named + Send]`，装箱时检查具体类型。`+ Send` 只代表能转交给另一个线程，不代表能被多个线程共享，共享还需要 `+ Sync`。
- **P3 线程借用保守持续到 scope 结束。** `scope` 里 `spawn` 借用的数据，借用一直持续到 `scope` 等待所有线程结束，而不是到线程句柄最后一次被使用。`return`、`try`、`break` 提前离开 `scope` 时，先等待所有线程结束，再释放数据；丢弃句柄不解除借用；线程不能借用比 `scope` 更短命的局部变量；v1 的线程返回值必须为 `Send` 且不携带借用。`join()` 消耗句柄、取得返回值，但 v1 不据此提前缩短借用：仍保守保留到所属 scope 结束。丢弃句柄只丢弃取得返回值的权利；scope 仍等待线程。`Join` 不得逃出所属 scope。
- **P4 锁守卫。** `with g = m.lock() { }` 里从 `g` 借出的引用只能在块内使用：离开块后，即使 `m` 还活着，访问也不再受锁保护。借用系统同时跟踪两种存活：数据的存活，以及访问许可（锁守卫）的存活。
- **P5 channel 契约（v1）。** 元素类型必须是 `Send` 且不携带借用；`send` 移动值；关闭后 `send` 返回 `err(.Closed(value))`，把值还给调用者；关闭后 `recv` 先排空缓冲区，再返回 `none`。
- **P6 内建类型的契约。** 由编译器规定，用户不能声明：通过 `Rc[T]`、`Arc[T]` 只能得到共享的只读访问，把句柄放进 `var` 也不会变成排他可写；`Mutex[T]` 只通过锁守卫提供可变访问；锁守卫不是 `Send`，不能移到别的线程，必须在取得锁的线程析构。普通用户类型的 `Send`、`Sync` 再按字段结构推导。

- **P7 临时切片不是临时数据。** `spawn sum(v[a..b])` 合法，前提是 `v` 在当前 scope 之前已存在并满足共享条件；切片描述符会复制到启动参数存储，底层数据仍借用 `v`。`spawn sum(makeVec()[0..2])` 不合法，因为拥有数据的临时 `Vec` 在语句末销毁。在 scope 内创建的短命拥有者，即使马上 `join`，v1 仍拒绝；可以先创建拥有者，再进入一个更内层的 scope。
- **P8 并发 API。** 构造、`lock`、`tryLock`、channel、`Join` 的签名与错误路径以《标准库契约》为准。`Guard` 既不是 Send 也不是 Sync；同线程递归 `lock` panic，`tryLock` 在任何竞争（含同线程）时返回 none。关闭 channel 幂等，唤醒阻塞的发送和接收，发送失败必须返还原值。


## stdlib_contract.md，第 112–141 行

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

