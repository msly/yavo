# 交接文档历史批次（从 HANDOFF.md 移出，2026-11-21）

本文件保存第四节完整提交表和第 11–17 节（第六批至第十一批）的原文，内容与日期保持当时的记录，不代表当前源码的重新验证。当前状态见 HANDOFF.md。

## 4. 已完成（按提交，最新在上）

| 提交 | 内容 | 一致性通过数 |
| --- | --- | --- |
| `0a69e75` | Arena 跨模块导出、移动、共享线程与 panic 位置组合回归；六目标构建 | 126 |
| `2b994fb`、`78ee9d5` | 固定四项 C 性能套件及源码路径缓存探针；保存的是 `f5469dc` 的历史初始基线 | 126 |
| `cd1e7ed` | 独立 ASan/UBSan 导出与执行工作流；保存实施前 52 项历史基线 | 126 |
| `12531c3` | 用户源文件模块、顶层导出、逐文件名字/标准库作用域、跨文件位置、全项目不可变快照与验证 | 126 |
| `bec0ed1` | Arena/Id、非回绕身份/代际、Id 集合键、所有权/分配分析；修复默认 Guard 参数越权 | 126 |
| `f5469dc` | Heap/Deque、拥有比较闭包、可变引用返回与候选修复；显式 discard/元组析构；故障注入/argv/允许结果/分开 Sanitizer 运行器 | 126 |
| `47df1ad` | 分配查询、真实验证报告、负例 Sanitizer 能力门、两次独立前端与空缓存构建；3 项新增通过，T-R2 明确不支持 | 122 |
| `96999de` | `yavo fix`、可审查补丁、原子写回及修复验收；6 项新增通过，T-V3 从未运行转为明确不支持 | 119 |
| `faac6ac` | 安全与验证收敛：线程借用、Send/Sync、资源克隆、I/O 锁、测试时限、实验复核、独立 sanitizer 记录 | 113 |
| `baa1bad` | 并发第 2、3 部分：`scope`（隐藏局部的析构等待线程）、`spawn`（每个调用点一个参数结构和线程入口）、`join`、结构推导的 `Send`/`Sync`、`Box[I + Send]`（接口别名）、`Channel[T]{cap}`/`send`/`recv`/`close`/`clone`、`SendError[T]`（按元素类型晚登记的库枚举）；`ok(_u)` 可忽略 `()` 载荷 | 113 |
| `907896c` | 并发第 1 部分：`Ty::Sync`、`Rc`/`Arc`/`Mutex`/`Guard`/`with`（`E0703`）、运行时（pthread/Win32 锁、条件变量、线程、channel、scope；原子堆块计数；输出锁） | 99 |
| `1c17168` | 按第二轮实验修：`Set[T]`（存成 `Map[K, bool]`，`Ty::Set`）、`${x:.N}`（`PrintPart::Fixed`、运行时 `yv_out_fixed`）、`${...}` 里允许字符串字面量（原先误报 `E0020`）；速查卡补 `String`/`str` 不能混比、`os.args()[0]` 是程序名、`Set` 用法、`Vec` 没有 `remove`/`insert` | 93 |
| `99dca58` `4f15824` | 第二轮冷启动实验（题目、Perl 参考、两组程序与日志、`verify.sh`、结果） | 93 |
| `0ed6802` | 第 6 项：`*p` 读（可复制得副本，否则只读访问）、`*p = v`/`*p op= v` 经 `&inout T` 写；`let p: &inout T = &place`、`var p: &T = &place`、`let p = &x` 报 `E0004`；再借用 `&p`、`var` 引用重绑定；独占（`E0102`）与存活（`E0305`）沿用 `&inout` 规则；`for x in &v { *x += 1 }`、`getMut` 绑定可写可复制值。`String.fromUtf8`、prelude `Utf8Error`（成功复用缓冲区） | 93 |
| `238a2cd` | 速查卡补 6 个缺口（泛型结构体、`fn main()` 与“辅助函数 + `catch` + `os.exit`”、`byte` 插值、`join`、`?T` 方法与 `if let ... else`、`Map` 的 `str` 键、`String.push`），示例都用编译器验证过 | 84 |
| `96a8ff9` | 冷启动实验（`experiments/coldstart-2026-11-19/`）与上一版交接文档 | 84 |
| `e7e106f` | 切片 `v[a..b]`（`[]T`）、`s[a..b]`（`str`，`P-INDEX`、`P-STR-BOUNDARY`）；`[]T.toVec()`；`v.reserve(n)`（`P-CAPACITY`） | 84 |
| `16527cb` | `f64`/`f32`：字面量（`1.5`、`2e-3`、`1e10`；浮点上下文里的整数字面量即浮点）、IEEE 运算与比较、`f64(n)`、带检查的浮点转整数（`P-FLOAT-CONVERT`）、`totalCmp`、`parseFloat`、`min`/`max`/`abs`、`math.sqrt/pow/floor/ceil/round/pi/e`；最短往返打印 | 79 |
| `bd8ab57` | 泛型函数和泛型结构体（单态化）：显式 `f[int](x)`、`S[String]{...}` 或按实参推断；约束 `Copy`/`Eq`/`Ord`/接口；泛型结构体的方法与 `Self`；`Wrapper[str]` 报 `E0301` | 75 |
| `832b6db` | `interface`、`struct T: I`（逐个方法检查签名，新错误码 `E0019`，已登记）、`Box[I]` 虚表分发（`self`/`inout self`/`sink self`）、`Box[T]` 自动转 `Box[I]` | 71 |
| `6456fc8` | `Box[T]`；字段/方法/下标穿过 `Box` 自动投影；可写 `if let` 绑定（非可复制载荷绑定为 `&inout`）；`inout` 实参可带字段路径；`E0301`/`E0405`/`E0401`；给 `inout` 参数赋值算使用 | 71 |
| `9ecb2d5` | 比较协议：`struct T: Ord`（`fn cmp(self, other: &T) -> Ordering`）、`T: Eq`（`fn eq`）、签名不对 `E0502`；`Ordering` 进 prelude；无载荷枚举 `==`/`!=`；用户 `Ord` 类型可 `v.sort()`；`&x` 可传给 `&T` 形参 | 65 |
| `31cc140` | 不逃逸闭包：`v.sortBy((a, b) => ...)`、`m.update(k, init, (v) => ...)`；闭包提升为独立函数（捕获变量作前置只读参数）；`sortBy` 走生成的 `yv_tramp_N` 和运行时 `yv_sort_ctx`；词法分析器在 `{` 内恢复换行（块体闭包可写在括号里）；`total += n`（`n: &inout int`）自动读取 | 63 |
| `f0b8126` | 元组：类型 `(A, B)`、字面量、`t.0`（含 `t.0.1`）、`let (a, b) = t`（`_` 立即丢弃）；闭包语法开始解析，但在 `sortBy`/`update` 之外报 `U0001` | 61 |
| `65741d7` | 用户析构 `struct X: Drop`（V10、V11、`E0801`–`E0803`）：隐藏 `yv_live` 标志，被移走的值不调用用户 `drop` | 61 |
| `7ebf94b` | 方法：写在结构体/枚举体内，`self`/`inout self`/`sink self`，`Self`，无 `self` 的函数用 `Type.f()` 调用；`-> str of self`；`return self.name` 在返回 `str` 时视为视图 | 54 |
| `e3112f8` | `format`/`min`/`max`/`abs`/`String{}`、字符串方法（`contains`/`startsWith`/`endsWith`/`find`/`split`/`toLower`/`toUpper`/`parseInt`）、`v.contains`/`join`/`sort`、`[]T` 切片类型、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`os.args`（启动时 UTF-8 检查）；一致性运行器支持夹具 | 54 |
| `899195a` | 第 4 项和第 7 项：`enum`、`T ! E`、`try`/`catch`、嵌套模式的 `match`（语句和表达式）、拥有值字段；`panic`/`assert`、字节字面量 | 50 |
| `feat(p1): maps, &inout values and std imports` | `Map[K, V]`、`[:]`、`get`/`getCopy`/`getMut`/`set`/`remove`/`has`、`for k, v in m`/`in &m`、`for i, x in v`、`for x in &v`、`&inout T` 值与独占访问检查；`use std/...`、`os.exit` | 44 |
| `feat(p1): optional values` | 可选值：`?T`、`some`/`none`、`if let`、`while let`、`match`、`??`、`take`/`replace`/`swap`、`pop`；`tools/expect_header.sh` | 34 |
| `c0f0ad7` | `&T`、`for x in v`、`inout` 参数、`v[i]` 取不可复制元素 | 29 |
| `8313043` | 字符串视图（`trim`/`fields`/`lines`）、`of` 来源、按最后一次使用的借用检查 | 24 |
| `31c2b0a` `5db1b75` | `String`/`Vec[T]`、移动与 `E0402`、析构、`sink` 参数、堆块检查 | 12 |
| `380fe97` | 标量结构体；release 默认去调试信息；许可证 Apache-2.0；`ERRORS.md` 登记前端错误码 | 9 |
| `ee40a29` | `U0001`（不支持）与语法错误分开；一致性测试；`--target` 交叉编译 | 9 |
| `76a4c22` 及之前 | 另一个 agent 完成的 P0 原型；规范 v0.1–v0.5 与 129 项测试 | — |


## 11. P3 并发设计（第六批，2026-11-20；已按此实现）

本节记录第六批设计：当时目标是解锁 `yavo-tests/concurrency/` 的 21 项，四个报告类当时未运行；报告已在第九批接入，当前状态见第 17 节。实现复用以下机制：

- **类型**：新增一个 `Ty::Sync(id)`，表里存 `(种类, 内层类型)`，种类为 `Rc`、`Arc`、`Mutex`、`Guard`、`Channel`、`Join`、`Scope`（隐藏）。都不可复制。
  `Rc`/`Arc`/`Mutex`/`Guard` 在 C 里都是 `T *`，指向“头部 + 值”分配块中的值（头部在值前面：锁、持有线程号、引用计数），
  所以字段、方法、下标的投影可以直接复用 `Box` 的 `Unbox`/`BOX_STEP`（`Rc`/`Arc` 只读，写入 `E0101`；局部独占 Guard 和显式 inout Guard 可写，默认借用参数只读，相关修复见第 17 节）。
  `Channel` 是 `yv_chan *`；`Join[T]` 是每个 `T` 一个 `yv_jrec_K *`（公共前缀 `yv_join_base` + `T ret`）。`Guard` 携带借用（`Guard[T] of self`）。
- **`Send`/`Sync`**：在检查器里按结构推导（结构体、枚举看字段，递归类型遇环当作成立）；`Rc`、`Guard`、`Join` 都不是；
  `Arc[T]` 要求 `T` 同时 `Send`+`Sync`；`Mutex[T]`、`Channel[T]` 在 `T: Send` 时两者都是；`&T`、`[]T`、`str` 看 `T: Sync`；
  `Box[I]` 看声明的能力（`Box[Named + Send]`）。类型语法 `I + Send` 解析成 `TypeExpr { name: "+", args: [I, Send, ...] }`。
- **`scope`**：检查器在块开头声明一个隐藏局部（`Scope` 类型，`pinned`），它的析构函数 `yv_scope_drop` 等待所有未 `join` 的线程、
  释放未取走的返回值。因为析构按逆序进行，`return`/`break`/`try` 提前离开时自动“先等线程、再释放外面的数据”（P3）。
  线程借用的来源都存进这个隐藏局部；`check_not_borrowed`/`check_exclusive` 对 `pinned` 局部视为一直活到 scope 结束（T-GP7）。
- **`spawn f(args)`**：只能出现在 `scope` 内，被调函数是具名函数。每个调用点生成一个参数结构和线程入口函数；
  `sink` 实参要 `Send`（`E0701`），默认借用的形参类型要 `Sync`（`E0702`），返回值要 `Send` 且不带借用；
  借用来源是临时值或在 scope 内声明的局部变量报 `E0705`（T-GP2、T-P3f）。`h.join()` 是 `sink self`，第二次 `join` 自然报 `E0402`。
  `Join` 不能写出类型名，只能 `let h = spawn ...` 或作为语句丢弃，从而不能逃出 scope。
- **`Mutex`/`Guard`/`with`**：`Mutex(v)`、`m.lock()`、`m.tryLock()`（经 `Arc`/`Rc`/`Box`/引用自动投影接收者）；
  `with g = m.lock() { }` 把守卫绑定到块，块结束析构即解锁。`store_loans` 发现来源是 `with` 守卫局部时报 `E0703` 而不是 `E0305`（T-P4）。
  同线程再次 `lock` 用持有线程号检测，panic `P-LOCK-REENTRANT`；`tryLock` 在任何竞争时返回 none。
- **`Channel[T]{cap: n}`**：`T` 带借用报 `E0704`（T-P5），不是 `Send` 报 `E0701`；`send` 返回 `() ! SendError[T]`，
  `SendError[T]` 按元素类型实例化一个库枚举（`.Closed(value)`），需要类似 `late_structs` 的晚登记；`recv`、`close`（幂等）、`clone`。
- **运行时**：pthread 与 Win32（SRWLOCK、CONDITION_VARIABLE、CreateThread）两套；线程号用 `_Thread_local` + 原子计数；
  堆块计数使用原子操作；第七批统一所有打印、flush 和终止路径的可重入输出锁，stdin 整次读取使用独立锁，嵌套顺序固定为输入→输出。

## 12. 第七批：安全与验证收敛（2026-11-20）

基线为已提交且干净的 `baa1bad`，本批未改历史快照或扩写冻结规格。

- 修复 sink 线程参数遗漏内部借用来源、Send/Sync 递归状态混用、运行时 I/O 锁覆盖不完整、容器克隆不可克隆资源四项问题。
- 独立复核又发现泛型元组能把 `&inout` 送入线程；现按既有未支持边界统一拒绝，未临时开放可变线程借用。
- 新增六个 `.yv` 回归文件：23 个应拒绝检查及合法组合运行；旧编译器对全部新负例均错误接受，修复后按指定错误码和位置拒绝。合法接口集合克隆仍通过既有回归。
- 新增 I/O 回归：并发读写、70 KB 长行、UTF-8/CRLF、输出锁内 panic/exit/OOM、输入锁内 OOM、写失败位置。两种 C 优化模式均通过。
- conformance、p0、alloc_check 分离构建与运行时限，持续排空双管道，Unix 超时清理进程组；baseline 更新不能删除旧通过项。Windows 仅有 taskkill 降级，父进程提前退出的后代清理未验收。
- 冷启动复核脚本改为仓库相对路径、独立临时目录、逐项核对 stdout/stderr/退出码、失败非零退出。已保存的 Yavo 与 Rust 两组各 15 项核验全部通过；这不是新一轮 agent 实验。
- Debug、Release 全量各 32 个测试函数通过、1 个旧报告忽略；87 个 fixture 在两种 C 模式运行。一致性保持 **113 / 0 / 1 / 15**。
- 新并发 I/O 和所有权组合程序对六目标共 12 次交叉构建通过，未在 Windows/macOS/aarch64 真机执行。
- 独立 Linux 环境使用 GCC 14.2 对生成 C 分开构建 ASan/UBSan 与 TSan，共 40 项通过，包括 T-P3。源码和输入哈希、执行主机、选项与逐项结果保存在 `compiler/validation/safety-2026-11-20.json`。
  该环境的 ptrace 使 LSan 不可运行，ASan 关闭泄漏检测，正常程序仍启用堆块计数；仅故意跨线程 panic/exit 的两个探针关闭 TSan 线程泄漏报告（V11 不做展开/join），正常程序保留此检查。
  自动 conformance 尚未接入这些独立 sanitizer 工件，T-P3 的自动统计仍为 not_run。

服务器复核日志在 `compiler/target/safety-review/`（Git 忽略）；持久化结论以上述 JSON 与已提交测试为准。

## 13. 第八批：保守源码修复（2026-11-21）

基线为已提交且干净的 `faac6ac`；本批只扩展工具与验收，没有扩写冻结语言规格或改动历史快照。

- `fix_plan.rs` 按 AST 绑定关系生成修改；`fix.rs` 重检、校验 UTF-8 byte edits 和原始文件快照、同目录原子写回。验证使用独立线程，避免类型驻留表污染原始程序的 C 生成。
- 自动修复：未使用绑定改 `_name`（包括写引用）；局部内建 Vec 的独立 `v.push(v.len())` 按原块拆分；明确缺失的标准库导入；确实没有语法依赖的未用导入。名字按源码顺序避开当前函数已有及后续名字。
- 自动修改以整组为单位重新 check；有未解决错误或编辑冲突则原文件不变。`--dry-run` 不写，退出码描述实际文件，不把预测修好当成当前已修好。
- 候选 `take`/`clone`/`borrow` 与 `add-of` 均单独重检；每个包含完整 edits、影响与条件。`id` 是修复类别，`patch_id` 是具体补丁地址，`source_sha256` 绑定原文件。
- 复核关闭三个反例：参数名 `static` 不可改变 `of static`；未实例化泛型依赖的导入不能被 W0003 误删；用户同名 `take` 不可冒充内建取值。未知或有歧义的情况保守保留诊断。
- `check --json` 是 `--json-diagnostics` 的别名；保留旧 JSON 字段并增 `primary` 与 `fixes`。`fix` 使用 `yavo-fix/1`，报告原始/结果诊断、源码哈希、自动修改和候选。
- 6 项冻结 fix 验收通过：T-F1、T-F2、T-GF1、T-GF2、T-GF3、T-V1。自动修改与固定文件按真实词法 token 比较，再验证 check 与幂等；候选在独立副本应用后 check。
- T-V3 的 `&inout` 返回、返回可变借用构造及复杂 `push` 接收者原本不支持，现在被实际执行并分类为 unsupported；一致性 **119 / 0 / 2 / 8**，旧 113 项通过全部保留。
- Debug、Release 全量各 **50 个测试函数通过、0 失败、0 编译警告，1 个旧报告忽略**；含 13 个 fix 集成测试、4 个修复引擎单元测试，原 87 个 fixture 的两种 C 模式及交叉编译回归继续通过。
- 详细测试结果、源码与日志哈希见 `compiler/validation/fix-2026-11-21.json`；原始执行日志在 `compiler/target/fix-review/`。本轮文件替换及端到端修复运行于 Linux，未增加 Windows/macOS 真机验收或性能结论。

## 14. 第九批：分配分析与验证报告（2026-11-21）

基线为已提交且干净的 `96999de`。新增 `allocations`、`verify` 与报告验收，未修改冻结语言规格、历史快照或测试输入。

- 分配查询使用 `yavo-allocations/1`，参数求值和 callee 分开记录；已知分配优先于 unknown，同时保留 `has_unknown`。递归调用、回调、线程任务和潜在隐式析构按固定点传播，包含部分聚合构造后续 `try` 的清理路径。
- 查询范围是已检查函数及具体实例；泛型模板与动态接口保留未知。启动初始化不计入函数，输出与同步原语依赖受信任运行时契约，尚未审计全部 libc/OS 内部分配。
- `verify` 使用锁定 Zig，拒绝 `YAVO_CC`，实际核对版本并记录可执行文件/锁哈希；移除开发环境中的编译器、加载器和 Sanitizer 覆盖参数。工具链库与安装归档未重新做内容审计，报告明确该范围。
- `yavo-verify/1` 分别记录前端、工具链、构建、执行、审计、两组 Sanitizer、可复现构建；每项绑定 target、实际 artifact/hash/path 和 ran_on。默认执行是一项退出状态 smoke；可指定 stdin、精确 stdout/stderr 断言和禁止运行。
- 审计是必需检查但当前 `not_run`；app/dependencies/stdlib 为 `not_audited`，固定 C 运行时最多 `trusted_pinned`。任何必需失败/超时退出 1，缺少通过证据退出 5，不把部分验证写成整体通过。
- 子进程同时排空两个输出流，每条保留上限 4 MiB，并单独记录全流字节数/哈希。未到 EOF 不给完整哈希；截断时不能把预期前缀写成完全匹配。Unix 超时清理普通进程组，再收尾最多 2 秒；Windows 仍为有界 taskkill 降级。
- 发现并修复一个实际的假通过来源：固定 Zig 接受 `-fsanitize=address,undefined`，但逗号形式没有启用 ASan。现改成分开的参数，并在普通启动探针外要求每项能力检测到故意注入的缺陷；只接受参数或生成不同哈希均不能证明插桩。
- ASan 单独或分开指定后会因缺 `__asan_*` 运行时符号而链接失败，因此 ASan+UBSan 组合为 `unsupported`。独立探针证实 UBSan 能报告整数溢出，TSan 能报告数据竞争；TSan 的实际应用检查通过。
- 对 T-N2 完成原生 musl 输出断言、独立 GNU TSan 产物执行和 Windows PE 交叉编译；Windows 执行为 `not_run`。Linux musl 与 GNU Sanitizer 工件明确分开。
- 可复现验证实际运行两次独立 Yavo 前端，再用各自新建的 local/global Zig 缓存编译。两份 C 哈希相同，两份二进制哈希均为 `3f3cf249933eb5c8df317d2ec72f65e6db295f0b10f5bdcb66e6a0081705f1f8`；范围是同一逻辑源路径下的独立构建。
- Debug、Release 全量各 **71 个测试通过、0 失败、0 编译警告，1 个旧报告忽略**。新工具有 15 个 CLI 回归，过程捕获有 4 个行为测试及测试驱动；原有运行、堆块、修复和六目标交叉构建回归继续通过。
- 一致性从 **119 / 0 / 2 / 8** 变为 **122 / 0 / 3 / 4**；新增 T-GA1/T-GA2/T-R1，T-R2 已实际执行并因 ASan 能力分类为不支持。旧 119 项全部保留。没有使用缺少负例门时出现的中间 123 结果更新基线。
- 完整结果、源码/日志哈希、实际报告和负例证据见 `compiler/validation/verify-2026-11-21.json`。独立复核重新读取并校验了 21 处工件引用（12 个不同文件）及四个隔离缓存目录。原始日志和工件在 `compiler/target/verify-review/`。

上述为第九批历史状态；Heap/保存闭包、可变借用返回和运行器随后在第十批补齐，当前剩余验收见第 16 节。本节验证未包含近 C 性能测量。

## 15. 第十批：容器、可变借用返回与运行器补齐（2026-11-21）

基线为已提交且干净的 `47df1ad`。冻结规范、历史快照和 129 个测试输入保持原样。

- `Heap[T].by` 保存拥有比较器，支持 push/pop/len；Copy 捕获复制，拥有捕获移动进环境，任何携带借用的捕获报 E0307。比较器调用借用捕获，不能重复移走捕获资源。Heap 整体移动，元素和环境恰好一次析构；不一致比较器下上滤/下滤仍有界且不丢失元素。
- `Deque[T]{}` 使用环形缓冲，支持 pushBack/pushFront/popFront/popBack/len，以及满足递归克隆条件时的 clone；扩容按逻辑顺序搬移，弹出 O(1)。两类容器的借用元素在插入、移动、弹出和返回时保留来源；Deque 按元素推导线程能力，Heap 隐藏捕获的线程能力暂报 U0001，不能只看元素就放行。
- `closure_free.rs` 按实际词法绑定收集捕获，覆盖声明初值、局部、分支/模式、循环、catch/with 和嵌套闭包；内部同名局部不会错误移动外层资源。
- 顶层 `-> &inout T of ...` 可用于函数、方法和接口方法；支持字段、Vec 元素与嵌套索引地址。接收者可直接是返回可变引用的调用，用 HIR `LetTemp` 保证只求值一次并先于实参。跨调用再借用仍指向原始拥有者，先求值的借用实参不会被后面的实参破坏；Rc/Arc 以及默认只读参数不能因此取得写权限。
- `fix` 对复杂接收者的冲突提供独立重检的 `hoist-argument` 候选，不自动改变求值顺序。Map.getMut 经临时接收者时保留真实 Map/Guard 来源，不再把隐藏指针描述符误当作拥有者。
- 修复 `let _ = 已有拥有值` 没有消费/立即析构的旧行为，之后再用报 E0402；丢弃默认借用的拥有参数报 E0401。引用描述符不析构其目标。修复拥有元组临时值遗漏清理，新增逆序析构的精确输出回归。
- 运行器接入 run-exit 原始字节 argv、main 后分配失败注入、run-allowed 和 sanitizer 分开工件；注入器只在测试 C 包装文件，不向生产构建增加开关。输出、退出码、此前 stderr、超时和负例 Sanitizer 能力证据均参与判定；所有可运行的必需配置先检查，ASan 不可用不会掩盖 TSan 失败。
- 修复运行器对空/无效 modes 零次执行也通过的问题：现在只接受非空、去重的 debug/release 模式。分配查询保留 Heap 保存比较器及其析构的 unknown，已知分配不抹掉 unknown；LetTemp 和新容器临时值清理纳入分析。

- 全量 Debug/Release 各 **88 通过、0 失败、0 编译警告，1 个旧报告忽略**；91 个编译/运行 fixture，运行用例覆盖两种 C 优化模式及堆块检查。Heap/Deque 与可变返回的组合程序对六目标构建、校验格式与架构，在匹配的 Linux x86_64 主机执行。
- 一致性 **122 / 0 / 3 / 4 → 126 / 0 / 3 / 0**；新增 T-O5/T-V3/T-GR8/T-S3，原 122 项全部保留。T-GR9/T-P3/T-R2 均因 ASan+UBSan 无法链接而明确 unsupported，现有 83 个 P1 冻结用例全部通过。
- 验证记录复核了 125 个源码文件、11 个实际工件哈希和完整 Debug/Release 日志。首次组合驱动在 Debug 完成后的 Release 构建阶段达到命令时限，Release 独立重跑完整通过；没有将中断作为通过结果。

验证工件与日志汇总写入 `compiler/validation/completion-2026-11-21.json`；原始执行记录在 `compiler/target/completion-review/`。新增语言功能仍不等于完整 v1 验收，剩余项见第 16 节。

## 16. 完整 v1 目标与未闭合验收

129 个冻结用例只覆盖规格的一部分，通过比例不能作为 v1 完成百分比。第 7 节列的是当前语言限制，本节记录更完整的工程交付范围。

| 目标或工作项 | 已有能力 | 仍需完成的功能或证据 |
| --- | --- | --- |
| 无追踪式 GC | 确定性析构、显式 `Rc`/`Arc`、拥有值移动、Arena 槽位与 Id | 后续 FFI、依赖及一般闭包继续保持同一资源规则；自动循环回收不是 v1 目标 |
| 近 C 性能 | C17 后端、release 优化、安全检查保留；固定四项 C pilot、构建延迟/产物大小/路径缓存探针 | 扩展代表性工作负载；正式验收仍要求几何平均不超过 C 的 1.5 倍并报告最差项。当前四项不能证明语言总体目标，编译延迟还需实现缓存后复测 |
| 跨平台编译与运行 | Linux/Windows/macOS 的 x86_64/aarch64 六目标交叉编译 | Windows/macOS 和其他架构实际运行与 CI；交叉编译通过不代替目标平台运行 |
| Arena / Id | 已实现存储、借用、身份/代际检查、容量/计数耗尽、Id 比较/集合键、析构、线程与模块组合 | 函数返回可变 Arena descriptor 的 payload 来源仍保守；继续扩展图工作负载性能与组合安全验证 |
| 多文件与包 | 已实现源模块、顶层导出、循环导入、逐文件名字/标准库作用域、真实跨文件位置与全项目验证快照 | 本地路径/Git 依赖、程序锁文件、多文件修复、symlink 快照别名语义；包注册中心不在 v1 范围 |
| 闭包与线程能力 | 库内不逃逸回调；Heap 保存拥有捕获；具名函数 `spawn` | 一般闭包值/高阶参数与字段存储接口；保存环境的线程能力表示；公开可移动 Join 和 `spawn inout` 的契约实现 |
| 标准库剩余面 | String/Vec/Map/Set/Heap/Deque/Arena、Id 键、基本 I/O、同步与数学 | `time.now`、`rand.int`；Map/Set 元组键；元组 Default 等契约缺口；Atomic 的具体公开接口需要确定后实现 |
| 用户测试与 agent 工具 | 编译器开发测试、`check`/`fix`/`allocations`/`verify`；除 fix 外已贯通模块图 | 测试块与 `yavo test`，fmt/outline/type-at/explain/doc；多文件 fix；verify 当前执行一个项目入口及可选输出断言 |
| unsafe / C FFI / audit | 关键字和设计预留；verify 明确记录未审计 | 真实 FFI/unsafe 入口、依赖及边界审计；在 audit 未通过前，verify 不能宣称整体验证成功 |
| 工具链分发与缓存 | 固定 Zig、下载校验；Zig 自身缓存；实测定位相同 C 内容因临时路径变化产生的缓存成本 | Yavo 内容寻址缓存与可靠失效/并发发布优先；完整 Core/Full 分发、预编译运行时、依赖/单态化缓存、容量限制与 LRU 回收 |
| 诊断机器契约 | 主诊断、notes、结构化修复和哈希绑定 | 相关位置 secondary 与被抑制级联诊断标记等剩余契约 |
| agent 冷启动与成本 | 最近仍为第二轮 10 道单文件题：最终 Yavo/Rust 均 10/10，首编 7/10 与 9/10；token 为 116096 与 89784 | 使用最新 Arena/模块/fix/verify/并发功能重做隔离实验；正式验收需配对通过率差的一侧 95% 下界不低于 −5 个百分点、每个成功任务总成本至少低 20%，并独立复现 |

正式评测按 `docs/plan.md` 的后续定量设计：60 个独立任务的 pilot，根据分歧率和功效调整，正式不少于 400 题；使用隐藏验收、多模型、每个任务×模型×语言的新上下文并保存原始成本。先做实现驱动的小规模验证，再投入正式评测。

可变切片、两阶段借用、async/M:N 调度、panic 展开、自动循环回收、WASI/MSVC ABI、自有机器码后端、包注册中心和内建 HTTP/TLS 均属于已排除的 v1 范围，不应记作本批待补功能。

## 17. 第十一批：Arena/Id、源模块与当前源码实测（2026-11-21）

实现从 `f5469dc` 开始，当前验证源码冻结在 `0a69e75`。Arena、模块、验证工作流和组合回归分别见第 4 节提交；未修改冻结 `docs/`、历史快照或 129 项规范测试输入。本批没有开展新的 agent 冷启动实验。

### 17.1 Arena 与资源/权限边界

- `Arena[T]{}` 空构造不分配堆块；`add` 移入值并返回 `Id[T]`，`get`/`getMut` 借用，`getCopy` 要求 Copy，`remove -> ?T` 移出拥有值，`has`/`len` 查询存活状态。移动 Arena 保持 identity，清理只析构仍存活的元素，移出值恰好析构一次。
- Id 包含进程内 Arena identity、slot、generation，不含指针。删除后旧 Id 返回 none，槽位复用不能复活旧 Id；外来 Id 在所有查询/移除入口报 `P-ARENA-ID`。identity 使用原子 CAS，identity/generation/capacity 达上限报 `P-CAPACITY`，两种构建模式均不回绕。
- Id 是 Copy/Send/Sync，支持 Eq/Ord、排序、contains、Map/Set 键；这些能力不依赖虚标记 T。Arena 不可 Copy/clone，也无 Default；其 Send/Sync 递归取决于 T。支持 `Vec[Id[Node]]` 图、Box 投影、Guard 与 `Arc[Mutex[Arena[T]]]` 共享、线程移出/返回 Arena。
- 补上默认只读 Guard 参数的权限门。原先函数体能修改容器，而调用方仍持共享元素引用；现在 direct/generic/field 方法、字段赋值、take、inout 转交和可变索引借用均不能越权，局部独占 Guard 和显式 inout 保持可写。独立审查复现了原漏洞并复核修复后的 `E0101`。
- 专项覆盖 8×4096 个并发身份、伪造边界值、身份/代际/容量耗尽、foreign/stale Id、精确 panic 位置、借用拒绝、析构次序、部分构造与参数失败、线程退出、分配效果。返回可变 Arena descriptor 的来源上界仍采用第 7 节的保守处理。

### 17.2 源模块与项目证据

- 入口目录是项目根；`use maths` 加载 `maths.yv`，`use util/maths` 从根加载子目录，嵌套文件也按同一根解释路径。每个文件只加载一次；支持循环导入，当前没有模块级可执行初始化。
- 顶层函数与类型用 `pub` 导出，调用按模块限定；类型重名时需 `a.Point`/`b.Point`。私有入口名字不能泄露给子模块，导入不隐式再导出，标准库导入也逐文件生效。公开类型的成员保持既有可见性。
- 原文件先独立解析，再做链接/检查；注释、字符串或代码块不能跨文件闭合。载入图限制 128 层、1024 文件、16 MiB；路径不能逃出根，symlink 模块明确不支持。
- `check/build/run/allocations/verify` 贯通可达模块图；诊断、分配查询字节范围和 panic 指向原文件。验证保存每个源文件、源清单与项目内容哈希，可复现构建使用完整不可变快照、独立前端和空 C 缓存。多文件 `fix` 明确 `U0001` 且不写入；本地/Git 依赖和程序锁文件仍未实现。
- `tests/modules.rs` 17 项与 `tests/targets.rs` 6 项覆盖模块语义、源码变更/快照、工具入口及 Arena/模块/线程组合。六目标均构建并核对文件格式，实际执行仍受宿主平台限制。

### 17.3 当前验证矩阵

| 验证 | 当前结果与执行方式 | 证据范围 |
| --- | --- | --- |
| Debug | 15 组汇总 **113 passed / 0 failed / 1 ignored**；完整驱动最后 verify 组超时，已完成 14 组 98 项加独立 verify 15 项补齐 | 不声称一次全量命令成功 |
| Release | 四个有界批次全部退出 0，15 组汇总 **113 passed / 0 failed / 1 ignored** | 使用预编译后的 Release 测试工件，逐批记录结果 |
| P0 与堆块 | 95 个编译/运行 fixture；运行测试按 C `-O0`、`-O2` 及 `YAVO_ALLOC_CHECK` 验证 | 堆块平衡不能替代访问安全检测 |
| 冻结 conformance | **126 / 0 / 3 / 0**；P1 **83/0/0/0**、P2 **20/0/1/0**、P3 **23/0/2/0** | T-GR9/T-P3/T-R2 仍受固定 Zig ASan 运行时缺失限制 |
| 独立 ASan+UBSan | 当前 GCC 14.2 环境 **58 项通过、0 失败**；包括 4 个 Arena 与 2 个多文件入口 | 独立工具链/宿主，精确输出与退出码；LSan 关闭，正常退出另验堆块平衡 |
| Arena+模块真实可复现构建 | main/graph 两文件完整快照，两次独立前端及各自空 local/global Zig 缓存：C 与二进制哈希分别相同，`reproducibility: passed` | 实际 stdout 为 `B\n1 1\n`；整体仍 `not_verified` / 退出 5，因为 audit `not_run`，本次未请求内置 sanitizer |
| 当前四项 C 性能与路径缓存探针 | 运行时间 Yavo/C 几何平均 **0.949**、最差 **1.069**；热构建 P50 **242.282–249.000 ms**；固定 C 路径 **36.292 ms**、新路径 **225.525 ms** | 单 Linux 主机的四个人工选定微基准；不作为正式 near-C 验收，前后比值变化不等于编译器优化 |

整批记录归档为 `compiler/validation/arena-modules-2026-11-21.json`，当前独立访问安全记录为 `compiler/validation/asan-arena-modules-2026-11-21.json`；真实模块可复现记录是 `compiler/validation/arena-modules-repro-2026-11-21.json`。复现对象为 `compiler/tests/modules/arena/main.yv` 及 graph 模块，两份 C SHA-256 均以 `cc2a8557` 开头，二进制哈希均以 `9e8faddd` 开头；完整哈希在报告内。

ASan 的正常分配/释放、故意 use-after-free 与有符号溢出三项能力探针均通过；使用分开的 `-fsanitize=address` 与 `-fsanitize=undefined` 参数。独立复核重新计算 426 项工件引用哈希，未见差异，141 个核心/测试源码哈希与验证前冻结清单一致。原始 bundle 与 build/run/probe 日志归档在 `compiler/target/independent-sanitizers/`（`arena-modules-0a69e75.json` 与 `arena-modules-0a69e75-outputs.tar.gz`）。独立结果不改变锁定工具链或自动 conformance 的状态，也不能填补尚未实现的 audit。

### 17.4 性能证据与下一步依据

`benchmarks/small-c-v1/` 固定整数递推、Vec 变换、可变引用随机访问、Deque 环形队列四项；同算法/布局，运行输入防常量折叠，每次校验 checksum，另用独立 Rust 实现核对正确性。Yavo 的安全检查保留；两端使用相同固定 Zig、目标、优化与静态 musl，计时不启用 sanitizer/堆块计数，交替运行并固定 CPU，保留全部样本。范围不含 Arena、Map、字符串、一般分配、并发和其他平台。

历史初始记录 `benchmarks/small-c-v1/results/f5469dc-baseline.json` 对应实施前源码：四项比值几何平均 **1.062**、最差 **1.240**；构建预热中位数约 217–261 ms。历史路径探针 `f5469dc-cache-path.json` 保持 C 内容与输出不变，稳定路径 **41.843 ms**、新路径 **211.961 ms**。这些是已提交的历史基线，不是 `0a69e75` 的最终测量。

当前源码测量见 `benchmarks/small-c-v1/results/arena-modules.json`，每项两次预热、七次运行计时，所有输出与独立正确性参考匹配：

| 微基准 | Yavo 运行中位数 ms | C 运行中位数 ms | Yavo / C | Yavo 热构建 P50 ms |
| --- | ---: | ---: | ---: | ---: |
| integer_recurrence | 70.249 | 74.598 | 0.942 | 242.282 |
| vec_transform | 156.536 | 146.413 | 1.069 | 249.000 |
| mutref_scatter | 101.606 | 132.334 | 0.768 | 247.232 |
| deque_ring | 115.015 | 109.443 | 1.051 | 245.484 |

四项几何平均 **0.949**，最差为 Vec **1.069**；它们是单 Linux 主机上的四个人工选定微基准，没有覆盖正式 near-C 验收。当前四个 C 对照二进制与历史基线逐字节相同，但随机访问 C 中位数从 **103.648 ms** 变为 **132.334 ms**，主机波动足以明显改变比值。本批未做性能优化，不能把历史 1.062 到当前 0.949 的变化归因于编译器加速。

每项五次热构建的 P50 为 **242.282–249.000 ms**，仍高于 200 ms 目标；P95 只是五次样本的描述值。`arena-modules-cache-path.json` 保持 C 字节、运行时、选项、输出不变，稳定输入路径 **36.292 ms**，新输入路径 **225.525 ms**，相差 **6.214 倍**，输出与二进制哈希全部相同；两份报告共 439 个唯一工件引用哈希已独立复核。该对照定位了路径相关的 C 编译缓存成本，不是已实现的端到端缓存加速结果。当前实现仍每次创建新 C 临时路径，因此内容寻址缓存排在第 10 节第一项；新功能降低 agent 任务成本的假设仍需单独实验。

