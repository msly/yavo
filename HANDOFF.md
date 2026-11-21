# Yavo 交接文档（2026-11-21，第十二批：后端构建缓存、run 信号报告与上一批复核）

给下一个会话：先读第 1、2、10、18 节再开工。第 18 节是本批实现、对上一批的复核结论和更正；第 17 节是第十一批（Arena/模块）的实现与证据；完整 v1 剩余工作见第 16 节。第 11–17 节保留历史批次的日期、数值与验证范围，不能当作当前源码的重新验证。

## 1. 一句话状态

规范 v0.5 已冻结（`docs/`、`yavo-tests/`）；Rust 编译器与 C 运行时已覆盖 P0 链路及 P1—P3 的主要语言功能，完整阶段验收仍未闭合：
结构体、`enum`、`T ! E` 与 `try`/`catch`、`match`、`String`/`Vec`/`Map`、`[]T` 与 `x[a..b]`、移动与析构、视图与 `of`、`&T`、`inout`、`?T`、
方法、`Drop`、元组、不逃逸闭包、`Eq`/`Ord`、`Box[T]`、`interface` 与 `Box[I]`、泛型、`f64`/`f32`、切片、`reserve`，
以及第四批新增的 **`*p`、`&inout T` 局部变量、`&x` 构造（A6、A7、`E0004`）和 `String.fromUtf8`**，第五批新增的 **`Set[T]`、`${x:.N}`、`${...}` 里的字符串字面量**，
第六批新增的 **并发：`Rc`/`Arc`、`Mutex`/`Guard`/`with`、`scope`/`spawn`/`join`、`Send`/`Sync`（含 `Box[Named + Send]`）、`Channel`**；
标准库 I/O、`os.args`、`std/math`；六个目标的交叉编译。
第八批新增 **`yavo fix`**：保守自动修复、独立重检的候选、`--dry-run` 和结构化补丁。
第九批新增分配查询、绑定实际工件的验证报告、Sanitizer 能力探针与独立前端/空缓存可复现构建。
第十批补齐 **Heap/Deque、Heap 保存的拥有闭包、可变引用返回和复杂接收者**；修复显式丢弃与元组临时值析构，接入故障注入、原始 argv 和分开 Sanitizer 工件的运行器。
第十一批补齐 **`Arena[T]` / `Id[T]`、Id 比较与集合键、源文件模块、顶层 `pub`、跨文件诊断与全项目验证快照**；关闭默认只读 Guard 参数取得写权限的漏洞，建立独立 ASan/UBSan 和小规模 C 对照及缓存路径测量工作流。
规范一致性：**129 项中通过 126、判错 0、不支持 3、未运行 0**。剩余 T-GR9/T-P3/T-R2 都要求锁定工具链当前无法链接的 ASan 运行时；其中要求的 TSan 路径单独执行，不冒充 ASan 通过。该统计只表示既有用例覆盖，不是完整 v1 完成比例。
全部提交只在本地，**按用户决定暂不推远端**。
当前源码验证基准为第十二批提交（见第 18 节）。在空闲服务器上**单条命令**完整运行：Release 16 组 **122 通过、0 失败、1 忽略**（约 60 s），Debug 同样 **122 / 0 / 1**（约 135 s）；其中 113 项是第十一批已有测试（本批开工前在 `68024e2` 上单命令复跑同为 113/0/1，Release 55 s、Debug 117 s），9 项是新增 `tests/build_cache.rs`。第十一批记录里 Debug 超时、Release 分四批，是当时执行环境的时限所致，测试套件本身不需要分批。独立 GCC 14.2 的 ASan+UBSan 58 项结果来自第十一批，本批未改运行时与代码生成，未重跑。

“可用”三批已完成；2026-11-20 用户让我自己定后续顺序并分批：**第四批**（速查卡缺口 + 第 6 项 + 小卡点）已完成，
**第五批**（第二轮冷启动实验 + 按实验结果修速查卡和编译器）已完成：换模型（Sonnet）、不点名特性的新题，Yavo 10/10、Rust 对照 10/10
（`experiments/coldstart-2026-11-20/README.md`）。**第六批**（P3 并发）已完成，分三次提交。**第七批**安全与验证收敛见第 12 节；**第八批**源码修复见第 13 节；**第九批**分配分析与验证工具见第 14 节；**第十批**功能补齐见第 15 节；**第十一批**见第 17 节；第 10 节列出后续优先级。最近的 agent 实验仍是上述 10 道题，尚未测量 Arena、模块、fix/verify 等新功能的冷启动效果与任务成本。

## 2. 仓库与环境

- 仓库：`/data1/workspace/github/workspace/yavo`（通过 mcp-3 访问，工作区根是 `/data1/workspace/github`）。
- 构建前必须设置（否则找不到 cargo 或链接失败）：

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$HOME/.yavo/bin/rust-linker
cd /data1/workspace/github/workspace/yavo/compiler
```

- C 工具链：固定版本 zig 0.17.1，在 `~/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig`（`toolchain.lock` 有各平台哈希）。
- 这台机器**没有** python3、gcc、clang、valgrind，Python 验证器需在独立环境运行。固定 Zig 的 UBSan 与 TSan 已用故意出错的探针验证；ASan 插桩缺少可链接运行时，ASan+UBSan 组合仍不支持。`tools/sanitizers/export.pl` 在服务器导出绑定源码哈希的 C，`tools/sanitizers/run.py` 在独立 Linux/GCC 14.2 环境执行 ASan+UBSan；它不替换生产工具链，也不改变自动 conformance 的 unsupported 分类。
- 长任务用 mcp-3 `exec_command` 后台运行并写入独立日志和 `.exit` 文件（`(cmd > log 2>&1; echo $? > x.exit) > /dev/null 2>&1 &`）；再用短命令读取。完整测试套件单条命令即可跑完（见第 1 节），不必分批。
- 构建缓存默认在 `~/.cache/yavo`（`YAVO_CACHE_DIR` 可改，`YAVO_NO_CACHE=1` 关闭，`YAVO_CACHE_TRACE=1` 打印命中/未命中与键）。测量编译延迟时注意区分命中与未命中。
- 提交身份沿用已有提交：`git -c user.name="Yavo Development" -c user.email="yavo-dev@localhost" commit ...`。
- 用户用中文交流；用户请我“帮你定”的事项我直接决定并在回复里写明理由。

## 3. 构建与测试

```sh
cargo build --locked --offline --release
cargo test --locked --offline              # debug，全部测试
cargo test --locked --offline --release    # release，全部测试
cargo test --locked --offline --test conformance -- --nocapture   # 打印规范一致性统计
YAVO_UPDATE_BASELINE=1 cargo test --locked --offline --test conformance   # 有进展后刷新基线
```

| 测试 | 内容 |
| --- | --- |
| `tests/p0.rs` | `tests/p0/*.yv`：`run_*` 比对输出，`panic_*` 比对 panic 类别，`errors_*` 比对错误码和行列；debug、release 各跑一遍 |
| `tests/p0_regressions.rs` | 7 个系统级回归（C 编译器诊断透传、写失败、EINTR 等） |
| `tests/conformance.rs` | 读 `yavo-tests/MANIFEST.json`，校验源码哈希，归为通过/判错/不支持/未运行；**只在基线里的测试退化时失败**（棘轮）；规范文件出现 `E0020` 一律算判错；带 `fixture_hex` 的用例在独立目录里写好夹具再运行 |
| `tests/alloc_check.rs` | 用 `YAVO_ALLOC_CHECK=1` 跑全部 `run_*.yv`：运行时统计堆块，泄漏或多释放就以 102 退出 |
| `tests/targets.rs` | 6 个交叉编译目标的文件头格式与架构、Linux 静态链接、本机可运行目标的输出、错误路径 |
| `tests/stdlib_io.rs` | 需要标准输入、命令行参数或临时目录的程序（`io`、`fs`、`os.args`），带堆块检查构建 |
| `tests/fix.rs` | 源码字节、名字捕获、析构次序、条件执行、泛型导入、候选影响、JSON 和幂等；冻结 fix 用例由 conformance 执行 |
| `tests/verify.rs` | 分配三态、未知效应保留、隐式析构、工件与输入哈希、真实执行、超时、跨平台限制、工具链覆盖拒绝、输出截断、Sanitizer 负例能力门 |
| `tests/collections.rs`、`src/closure_free.rs` | 保存比较器与析构的分配未知、绑定感知捕获的 5 个解析器回归；运行/拒绝 fixture 在 p0 目录 |
| `tests/mutref_returns.rs` | 6 个集成测试：返回来源、索引/字段、转交/重绑定、12 类拒绝、Guard 边界、准确求值顺序、候选修复 |
| `tests/arena.rs`、`tests/arena_runtime.c` | 6 个集成测试：身份/代际、所有权与借用、计数耗尽、8×4096 个线程身份、外来 Id 精确 panic、Guard 权限及分配效果 |
| `tests/modules.rs` | 17 个集成测试：导出与逐文件导入、循环、同名类型、真实文件位置、项目快照/verify/allocations、Arena 与模块/线程组合 |
| `tests/build_cache.rs` | 9 个集成测试：命中且产物逐字节相同、键覆盖代码/模式/目标/`YAVO_ALLOC_CHECK`、行号移动必须重建（panic 位置正确）、模块文件变化失效、产物损坏与校验和缺失时重建并自愈、6 路并发构建只发布一个一致条目且无临时文件残留、条目数上限淘汰、`YAVO_NO_CACHE` 与不可用缓存目录回退、`yavo run` 报告被信号终止的程序 |
| `tools/sanitizers/` | 独立源码导出与 ASan+UBSan 执行；能力负例先通过，再检查精确输出、退出状态、源码/工具链/工件哈希 |
| `benchmarks/small-c-v1/` | 固定四项 C 对照、独立正确性参考、两次预热与七次计时；编译延迟与源码路径缓存诊断单独记录 |

常用命令：`yavo check f.yv [--json] [--emit-c f.c]`、`yavo fix f.yv [--dry-run] [--json]`、`yavo run f.yv [--release] [--target T]`、`yavo build f.yv -o out [--release] [--target T] [--emit-c f.c]`。
新增：`yavo allocations 函数名 --file f.yv --json`、`yavo verify f.yv [--target T] [--sanitize] [--reproducible] --json`；全部选项见 `compiler/README.md`。
入口文件可通过 `use` 加载整个可达模块图；模块函数查询使用 `yavo allocations util/maths.double --file main.yv --json`。多文件 `fix` 当前明确报 `U0001` 且不写入。长套件可以分组执行，但必须逐组保留结果，不能把驱动超时描述为一次完整通过。

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

## 5. 架构速览

流水线：`project.rs` 从入口加载/快照模块图，各文件经 `lexer.rs` → `parser.rs`（AST 在 `ast.rs`）独立解析，再链接到 `check.rs`（类型、所有权、借用，产出 HIR）→ `codegen.rs`（生成一个 C17 翻译单元）→ zig cc。

- `types.rs`：`Ty` 是小的 `Copy` 枚举；结构体和枚举共用一张聚合表（`register_struct`/`register_enum`，`finish_agg` 按依赖顺序算出能否复制、是否要析构），`Vec[T]`、`[]T`、`&T`、`?T`、`T ! E` 等通过线程局部表登记（`vec_of`、`slice_of`、`ref_of`、`opt_of`、`res_of`）。`is_copy()` 决定移动还是复制，`carries_borrow()` 决定是否需要跟踪来源。
- 移动：检查器把被移动的局部变量改写成 `HExprKind::Move`；生成代码时转移后把源**清零**。运行时容器析构接受移走后的零状态；用户 `Drop` 类型另有 `yv_live` 防止重复执行用户析构。零状态可安全清理不表示每种类型都拥有可构造的 Default，例如 Arena 没有 Default。
- 析构：代码生成维护作用域栈，作用域结束和 `return`/`break`/`continue` 前逆序释放；只被借用的临时值（`f().len()`）在语句末释放。
- 移动检查：`FnCx::moved` 按控制流合并（`if` 两支取并集，提前离开的分支不算），循环体再检查一遍。
- 借用：每个局部变量有 `loans`（指向哪些变量，或临时值）。`Pre` 预扫描给语句编号并记录名字的使用位置和所在循环，`live_after` 据此判断视图是否还会被用到。循环里的使用会延长到循环结束。
- 运行时 `runtime/yavo_rt.h`：输出缓冲、panic、整数检查、`yv_string`/`yv_vec`、视图函数、`YV_ALLOC_CHECK` 计数。 新增 `yv_out`（打印和 `format` 共用的输出目标）、文本查询、稳定归并排序、标准输入缓冲、文件读写（含 Windows 的 UTF-16 路径）、启动时的参数转换。
- 枚举是 `{ tag; union { v0, v1, ... } u; }`，结果是 `{ ok; val; err; }`；清理路径接受移走后的零状态。`match` 统一走 `HPat` 模式树：检查器做可用性（usefulness）分析判断穷尽和不可达分支，代码生成成 `if`/`else if` 链；匹配拥有的临时值时，绑定拿走对应部分，其余部分在分支开头析构。只含 `some(x)`/`none`/`_` 的 `match` 仍走原来的 `match_opt`。
- 表达式里的语句块（`catch` 块、`match` 分支块）用 `Gen::outer` 保存外层语句的临时值，跳转时一并释放；`try` 提前返回时，正在构造的结构体字段、向量元素、`sink` 实参通过 `hold`/`release` 记在临时值里，一并释放（V3、V6）。
- 方法：解析器把方法改名为 `Type.m` 放进 `Program::fns`（`FnDecl::owner`），接收者是名为 `self` 的第一个参数；检查器 `method_call` 先查用户方法（`user_method`、`method_on_place`），统一走 `call_fn`。`inout self` 的接收者是 `HExprKind::InoutPlace(HPlace)`（可带字段路径）。C 函数名把 `.` 换成 `__`。
- `Drop`：`types.rs` 的聚合表有 `user_drop`；带 `Drop` 的类型 C 结构里多一个 `uint8_t yv_live`，字面量置 1，析构帮助函数先看标志、清零、调用 `yv_fn_T__drop`，再析构字段。`FnCx::in_drop` 用于 `E0803`。
- 元组：`Ty::Tuple(id)`（`tuple_of`/`tuple_elems`），C 里是成员 `f0`、`f1`…的结构；元素访问复用 `HExprKind::Field`（基类型是元组时下标就是元素号），所以移动、借用来源等规则和字段一致。
- 闭包：`lift_closure` 用新的 `FnCx` 把闭包体检查成独立 `HFn`（名字 `closure.N`，追加在用户函数之后，下标 `n_fns + k`）；捕获由 `closure_free::free_variables` 按词法绑定收集，借用回调的捕获作为前置 `Param`（`FnCx::captures` 记录，用于错误信息），Heap 保存比较器则生成拥有环境。`HProgram::tramps` 记录每个 `sortBy`/`Ord` 排序的适配函数。外层 `Pre` 只记录闭包里的名字使用，不给闭包体的语句编号。
- 比较协议：`Checker::protocols`（按聚合 id 记 `cmp`/`eq` 函数），`protocol_compare` 生成 `Call` 加 `HExprKind::TagTest`；无载荷枚举用 `HExprKind::SameTag`。
- `Box[T]`：`Ty::Box(id)`（`box_of`/`box_inner`），C 里是 `T *`（结构体类内容用 struct 标签写，所以 `?Box[Node]` 能出现在 `Node` 里），NULL 即被移走。
  位置路径里的 `BOX_STEP`（`usize::MAX`）表示“进入盒子”，`field_path` 统一处理字段链；读取用 `HExprKind::Unbox`（像字段一样不能移出）。
  `&Box[T]` 的 C 类型是 `T * const *`（`ref_of` 对指针类型把 `const` 放后面）。
- 可写 `if let`：`match_opt` 对可写根（`var`、`inout` 参数）且载荷不可复制时，绑定类型为 `RefMut`、种类 `BindMut`；独占检查沿用 `&inout` 的规则。
  `inout_arg` 接受字段路径（`InoutPlace`）和 `&inout` 变量（转交其目标）。
- 接口：`Ty::Dyn(id)` 就是 `Box[I]`（`register_iface`），C 里 `{ void *data; const void *vt; }`；每个接口一个 `yv_vt_I`（drop、clone、各方法槽），
  每个 `struct T: I` 生成适配函数 `yv_vtm_K_J` 和 `yv_vt_K`（`HProgram::ifaces`/`impls`）。接口方法的签名放在 `Checker::dyn_sigs`，编号从 `DYN_BASE` 起；
  `call_fn` 遇到这类编号产出 `HExprKind::DynCall`。`Box[T]` 到 `Box[I]` 的转换在 `coerce`/`to_dyn`（`HExprKind::ToDyn`）。
- 泛型（单态化）：`Checker::subst` 是当前的类型参数替换，`resolve_ty` 先查它。实例的函数下标排在 lifted 函数里（闭包现在也登记一个 `Sig`，
  保证“签名编号 = 函数下标”）；`instantiate_fn` 先占位再把函数体放进 `pending`，`check()` 在用户函数之后逐个检查。
  泛型结构体在 `instantiate_struct` 里登记（先登记再解析字段，支持递归），`collect_structs` 之后产生的实例立即 `finish_agg` 并放进 `late_structs`。
  泛型结构体的方法在第一次调用时按实例化（`user_method` → `inst_of`）。实例的 C 名由 `types::c_safe` 生成（`Stack[String]` → `yv_s_Stack_String_`）。
  推断用不产生诊断的 `peek_ty` + `unify`，看不出类型的实参就要求写类型参数。
- 浮点：`Ty::Float(FloatTy)`；`HExprKind::Float`、`ToFloat`、`FloatToInt`、`Math`；`Method2::TotalCmp`/`ParseFloat`。运行时新增 `yv_out_f64/f32`、
  `yv_f64_total_cmp`、`yv_str_parse_float`、`yv_f2i_*`，并引入 `<math.h>`、`<stdio.h>`。
- 切片：`ExprKind::SliceRange` → `HExprKind::SliceRange`；借用来源规则见 `loans_of`；运行时 `yv_vec_slice`/`yv_str_slice`。
- 解引用（第四批）：`*p` 是 `UnOp::Deref`，检查器读时降为 `HExprKind::Deref`；赋值目标 `*p`（可再接字段）在 `StmtKind::Assign` 里识别，
  路径开头插入 `DEREF_STEP`（`usize::MAX - 1`），代码生成的 `walk` 跳过它、`place_root` 走 `(*p)`。
  期望 `&inout T` 的 `&place` 由 `borrow_mut` 降为 `InoutPlace`（类型 `RefMut`），`loans_of` 对 `InoutPlace` 记根变量及其已有借用（再借用因此能挡住原引用）。
  第十批把 `local_ty` 扩展到顶层返回类型；`LetTemp` 先保存复杂可变接收者，避免重复求值，`mutable_address` 支持字段与 Vec 元素地址，来源保留实际拥有者及 Guard。`String.fromUtf8` 是 `HExprKind::FromUtf8`，`Utf8Error` 在 `LIBRARY_ENUMS` 之后登记为库结构体。
- 并发（第六批）：`types.rs` 的 `Ty::Sync(id)` + `SyncKind`；`ptr_inner`/`ptr_read_only` 让 `unbox`、`field_path`、`mutable_place` 穿过 `Rc`/`Arc`/`Guard`
  （`BOX_STEP` 复用；`Rc`/`Arc` 写入报 `E0101`）。`LocalState` 新增 `guard`（`with` 绑定，`store_loans` 据此报 `E0703`）和 `pinned`（scope 的隐藏局部，
  `check_not_borrowed` 视为活到 scope 结束）。`FnCx::thread_scopes` 是当前嵌套的 scope。接口能力用 `dyn_with_caps`/`dyn_base`/`dyn_caps`（别名 id 共享 C 类型和虚表，
  检查器里按 `dyn_base` 查表）。代码生成：`Gen::spawns` 收集参数结构和 `yv_run_N`，在函数体之前插入；`yv_jrec_K` 在辅助函数之后定义。
- Arena：`Ty::Arena`/`Ty::Id` 共用元素类型登记；C 侧 `yv_arena` 持有槽位向量、free list 和进程内唯一 identity，`yv_id` 是 identity/slot/generation 三个整数。`ArenaRead`/`ArenaWrite` 保留存储与 payload 来源，生成按元素类型区分的槽位及析构帮助函数；身份由原子 CAS 分配，溢出不回绕。
- 模块：入口目录固定导入根，文件独立解析后解析名字与可见性；AST/HIR 的文件表把内部偏移映射回原始文件。诊断与运行时 panic 共用不可变源码映射，验证报告保存完整源图和项目内容哈希；当前仍生成一个 C 文件，未引入分离编译或包依赖管理。

## 6. 约定

- 错误码：规范错误 `E0xxx`（见 `yavo-tests/ERRORS.md`，前端码 `E0010`–`E0018`、`E0020` 已登记）；**合法但未实现的写法一律报 `U0001`，绝不能报 `E0020`**；警告 `W0001`。
- `tests/p0/errors_*.yv` 开头是 `// expect-error: CODE 行:列`。头部每多一行，正文行号整体加一；做法是先写无头文件，用 `yavo check` 取位置，再按头部行数加偏移生成头部。
  现在用 `tools/expect_header.sh compiler/tests/p0/errors_x.yv` 自动完成（去掉旧头部、重跑 `yavo check`、按头部行数加偏移写回）；它记录的是编译器当前的输出，生成后要逐条核对。
- 解析阶段的 `U0001`（parser 报的）会让整个文件不再进入检查器，所以它们只能和别的解析期 `U0001` 放在一起（`errors_unsupported.yv`），不能和检查器的错误混在一个测试文件里。
- 一个特性做完：补 `run_*` 和 `errors_*` 两类测试；跑全部测试（debug、release）；跑堆块检查；交叉编译抽查；刷新一致性基线；更新 `compiler/README.md` 对应小节和根 `README.md` 的数字；本地提交。
- 已解锁的旧写法可能让旧测试失效（例如某行不再报 `U0001`），改测试时保持行列不变，换成仍然成立的用例。
- `tools/expect_header.sh` 默认用 **debug** 版 `yavo`；只构建了 release 时要写 `YAVO=$PWD/target/release/yavo bash ../tools/expect_header.sh ...`，否则会拿旧二进制的输出生成头部。
- 历史批次曾在 `yavo-tests/ERRORS.md` 登记前端通用错误（如 `E0019`）；当前该测试包已冻结，新工作优先复用既有码，不回写历史快照或冻结输入来适配实现。

## 7. 已知限制与保守之处

- 借用检查按语句粒度和语句顺序：`if` 里修改、只在 `else` 里用视图也会报错；`E0305` 不看存储目标之后是否还被使用。第七批已修复 sink 视图容器跨线程的来源漏检，并补组合回归；此前“不会漏报”的表述没有依据。
- 可选值：对可写位置的 `if let`，**不可复制**载荷的绑定可写（`&inout`）；**可复制**载荷仍按值复制，给绑定赋值报 `U0001`。可写绑定还在使用时，原变量的任何使用都报 `E0102`（保守）。
  对 `&inout` 绑定的字段再做 `if let`（`if let l = n.left`）得到只读绑定。`a ?? b` 把 `a` 整体消费。`?? return/break/continue` 只允许作为整个 `let` 初值或赋值右侧。
- `&inout T`：局部变量可写类型并用 `&place` 构造，`*p` 读写；顶层返回携带 `of`，支持可变 Vec 元素。显式 `&inout` 参数、嵌套类型注解和任意下标位置的 `inout` 实参仍未开放（参数用 `inout`）；普通拥有字段保存借用本来违反逃逸规则。
  经 `*p` 存入带借用的值 `U0001`。借用按语句粒度，所以 `println("${q.len()} ${s}")`（`q` 指向 `s`）会多报 `E0102`，要拆成两句。
- `Map`/`Set`：键支持整数、`bool`、`str`、`String`、`Id[T]`，元组键仍未实现；`update` 的值类型限可复制且不带借用。
- Arena：返回 `&inout Arena[str] of a` 的函数只给出来源上界，经该 descriptor 的 remove/getCopy 可能继续借用整个 Arena，因此提前丢弃容器会被 `E0102` 保守拒绝；直接本地 remove/getCopy 能只保留外部 payload 来源。准确放宽需区分存储与 payload 来源，不能按来源类型直接删除 loans。
- 用户模块：已支持根目录相对路径、顶层 `pub`、逐文件作用域、循环导入和跨文件位置；公开类型的成员沿用既有可见性。尚无包依赖与程序锁文件、独立字段/方法隐私规则、分离编译；symlink 模块明确不支持，多文件 `fix` 报 `U0001` 且不写入。
- 标准库导入：已实现成员有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`math.sqrt/pow/floor/ceil/round/pi/e`，其余报 `U0001`；每个源文件须自行导入。`os.exit` 不运行析构。
- 枚举和结果：不支持字面量模式（`.Num(0)`、`1 =>`）、模式里的命名字段、`if let` 带模式、带载荷枚举的比较、泛型枚举、结构体里的结果字段。
- `Box`：没有 `.clone()`（写 `Box(b.clone())`）、不能打印或比较。
- 接口：已支持 `+ Send`/`+ Sync`；接口作泛型约束只做检查（单态化后直接调用具体方法），没有接口继承、`Self`、默认方法；`struct S[T]: I` 报 `U0001`。
- 泛型：**单态化，函数体只在实例化时检查**——从没被调用的泛型函数完全不检查，同一错误可能在每个实例报一次；结构体字面量要写类型参数（结构体自己的方法里除外）；
  没有泛型枚举、方法自己的类型参数、`Stack[int].make()` 这类在实例上的静态调用。
- 浮点：`%` 和位运算不支持；`1 + 1.5`（两个字面量）报类型不符；Windows/macOS 上的浮点打印未在真机验证（依赖各自 libc 的 `snprintf`/`strtod`）。
- 切片：没有 `x[a..]`、`x[..b]`；v1 本来就没有可变切片。
- 闭包：支持 `sortBy`/`update` 的不逃逸借用回调和 `Heap.by` 保存的拥有比较器；一般闭包值、用户高阶参数和闭包字段仍未实现。Heap 的 clone 与隐藏捕获跨线程能力暂报 U0001；Deque 未提供索引、切片或迭代接口。
- 元组：不能给元素赋值、不能比较、不能整体打印、不能做 `Map` 键、没有元组模式。
- 格式说明只支持浮点的 `:.N`；`String == str` 混比报 `E0001`（规范 K1 没定义跨类型比较，速查卡已写明；要不要放开是语言决定，留给用户）。
- `Set`：没有集合运算（并、交）、没有 `Set` 字面量里的元素（`[1, 2]` 不能直接当 `Set`）。
- 堆块检查发现不了释放后使用；项目服务器的固定 Zig 仍缺少 ASan 运行时。独立 ASan+UBSan 工作流已建立，当前源码运行结果见第 17 节；第七批的独立 ASan/UBSan、TSan 为历史证据，见第 12 节。Windows、macOS、aarch64 产物只验证了编译与文件格式。
- **栈溢出未定义（规范缺口，需用户决定）**：深递归或析构很深的 `Box` 链（如 20 万层 `Node{kids: [root]}`）直接 SIGSEGV；程序已缓冲未刷新的标准输出全部丢失（例中 `println("built")` 不可见）。规范没有对应 panic 类别。本批只让 `yavo run` 明确报告信号并以 128+N 退出，没有改运行时语义。
- 并发：`spawn` 只接受具名函数调用，可变借用（包括泛型元组中嵌套的 `&inout`）暂报 `U0001`；`Join` 不能写出类型名或移动（`U0001`）。规范已定义 inout 线程参数与公开 Join，属于实现缺口。
  容器递归克隆遇到不可克隆资源现已在检查期拒绝；用户 `Drop` 类型的自动逐字段克隆报 `U0001`，须显式构建资源或逐元素调用用户方法。
  当前接口集合的克隆检查遍历本编译单元所有具体实现；未来分离编译需要显式传递该保证。`&b` 不会把 `Rc[T]` 转成 `&T`（规范只对字段、方法、下标投影）。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

当前 **3 项不支持、0 项未运行**；全部冻结 P1 用例已通过，但完整阶段承诺仍需第 16 节的功能与证据。

| 卡点 | 测试与状态 |
| --- | --- |
| 锁定 Zig 缺少可链接 ASan 运行时；ASan+UBSan 能力探针失败 | T-GR9、T-P3、T-R2：unsupported |
| TSan | 有负例数据竞争检测证明；要求该配置的场景实际单独运行，不能替代 ASan |

第十批解锁 T-O5、T-V3、T-GR8 和 T-S3；第十一批新增 Arena/模块实现回归，冻结 129 项统计保持不变。运行器已接入 run-allowed、run-exit、故障注入和 sanitizer 分开构建；缺少平台或插桩能力会保留明确状态。

## 9. 计划表

| 序号 | 任务 | 状态 |
| --- | --- | --- |
| 1–4、7 | 可选值、`Map`、导入、枚举与结果、拥有值字段 | 已完成（上一轮） |
| 5 | 方法、`Drop`、`Eq`/`Ord`、`interface`、`Box` | **已完成**（`Box` `6456fc8`、接口 `832b6db`） |
| 6 | `*p` 写入、写出 `&inout T`、`&x` 构造（A7、`E0004`） | **已完成**（`0ed6802`，8 项一致性测试） |
| 8 | 浮点 `f64`、`totalCmp` | **已完成**（`16527cb`） |
| 9 | 切片 `x[a..b]`、`P-STR-BOUNDARY` | **已完成**（`e7e106f`，顺带 `toVec`、`reserve`） |
| 10 | 泛型函数和泛型结构体 | **已完成**（`bd8ab57`） |
| 11 | `yavo fix`（只自动应用不改变语义的修复） | **已实现**；7 项冻结修复用例通过，T-V3 只给重检后的候选 |
| 12 | P3：`scope`/`spawn`、channel、`Mutex`/`Arc`、`Send`/`Sync` | **已实现主要路径**（`907896c`、`baa1bad`）；T-P3 已尝试，受 ASan 能力限制；公开 Join / spawn inout 仍缺 |
| 13 | `yavo verify` 与验证报告 | **已实现**；GA1/GA2/R1 通过，R2 受本机 ASan 能力限制 |
| 14 | Arena/Id、Id 比较/集合键、资源与身份边界 | **已实现**；Arena 6 项集成测试、4 个运行 fixture 与模块/线程组合回归 |
| 15 | 多文件源模块、顶层导出、全项目验证 | **已实现**；17 项模块测试；包依赖与程序锁文件仍待做 |
| 冷启动实验 | 两轮各 10 道单文件任务 | **历史实验**：最近一轮最终 Yavo 10/10、首编 7/10；Rust 最终 10/10、首编 9/10。未重测本批功能 |
| 待定 | 三平台 CI | 需要远端仓库，用户暂不推 |

“可用”三批（2026-11-19 与用户约定）：

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| 第一批 | 第 4、7 项，标准库 I/O 与 `os.args`，`format`/`panic`/`assert`/`min`/`max`，字符串方法 | 已完成，一致性 44 → 54 |
| 第二批 | 第 5 项，闭包，第 10 项泛型，元组 | **已完成**，一致性 54 → 75 |
| 第三批 | 第 8 项浮点，第 9 项切片，冷启动实验 | **已完成**，一致性 75 → 84；冷启动 10/10 |

后续三批（2026-11-20 用户让我定顺序、能合并就合并）：

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| 第四批 | 速查卡 6 个缺口，第 6 项，`String.fromUtf8` | **已完成**，一致性 84 → 93 |
| 第五批 | 第二轮冷启动实验：不点名特性的新题、换模型做受试者、Rust 对照并记 token；按结果修 | **已完成**：Yavo 10/10（首编 7/10，116k token）、Rust 10/10（首编 9/10，90k token）；修了 `Set`、`:.N`、嵌套字符串 |
| 第六批 | 第 12 项 P3 并发（`scope`/`spawn`、`Channel`、`Mutex`/`Arc`/`with`、`Send`/`Sync`） | **已完成**：(a) `Rc`/`Arc`/`Mutex`/`Guard`/`with` 与运行时（`907896c`，93 → 99）；(b) `scope`/`spawn`/`join`/`Send`/`Sync`、(c) `Channel`（`baa1bad`，99 → 113） |

## 10. 下一个会话的第一步

1. 先检查 Git 状态和最新提交，确认其他人是否有后续修改；环境见第 2 节。当前冻结用例基线为 **126 / 0 / 3 / 0**，实施与证据见第 17 节。分析或文档维护不需要重复全量测试；代码修改按具体风险加回归，明确记录 Debug/Release 实际完成范围。
2. ~~内容寻址构建缓存~~：第十二批已完成，见第 18 节。
3. **第三轮隔离 agent 冷启动实验（提前到第一优先）**。“agent 优先”是这门语言存在的理由，而唯一实测（第二轮）里 Yavo 首编 7/10 落后 Rust 9/10，token 多 29%，工具调用约 3 倍；之后六批都没再测。沿用第二轮协议（不同模型做受试者、只给速查卡、Rust 对照、独立复核、记 token），10–15 道新题，其中至少几道需要多文件模块、图结构（Arena/Id）或 `fix`/`verify`。先看失败集中在哪里，再按结果修速查卡与编译器的不一致和小卡点；这比继续扩功能更能说明方向对不对。
4. **栈溢出语义**（需要用户先做语言决定，见第 7 节）：建议定义一个 panic 类别（如 `P-STACK`），运行时用备用信号栈（Unix `sigaltstack`、Windows 向量化异常处理）打印精确信息并尽力刷新输出；`Box` 链析构是否改为迭代另行决定。
5. **预编译运行时目标文件**：编辑后重建（缓存必然未命中）Release 仍为 214–247 ms，超过 200 ms 目标；每次未命中约 50–60 ms 用在重新编译 1759 行的 `yavo_rt.h`。按（目标、模式、alloc-check）缓存运行时 `.o`，生成 C 只包含声明。
6. 用户测试块与 `yavo test`（直接服务 agent 工作流），然后是本地路径/Git 依赖与程序锁文件；FFI/unsafe 与 audit、Windows/macOS 真机运行与 CI。固定工具链的 ASan 能力仍要解决，独立验证不能替代 verifier 的必需检查。
7. 一般闭包值/高阶参数/环境线程能力、公开 Join 与 spawn inout 等既定实现缺口；callable 类型、Atomic API、String/str 混比的语言决定。完整清单见第 16 节。扩展 C pilot 到 Arena/Map/字符串/分配/并发工作负载。

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

## 18. 第十二批：后端构建缓存、run 信号报告与上一批复核（2026-11-21）

### 18.1 对第十一批的复核结论

- **实现与测试结论可信。** 在 `68024e2` 上单条命令复跑：Release 113/0/1（55 s）、Debug 113/0/1（117 s），与记录一致。另写了 21 个对抗程序，其中 16 个真正触及目标检查（其余 5 个是我自己的语法错误、漏写 `use std/sync` 或用了未实现的 `Vec.clear`）：迭代中修改 Vec/Map/Arena、借用期间扩容、跨函数返回借用、视图写进 inout、可变/只读借用冲突、线程可变借用、`Rc` 跨线程、溢出、越界、Drop 次序，15 个得到正确的拒绝或 panic，诊断定位准确、提示可操作；第 16 个发现了下面的栈溢出问题。
- **更正 1：测试不需要分批。** 记录里 Debug 超时、Release 分四批是执行环境时限所致；空闲机器上单命令即可跑完。
- **更正 2：缓存键的范围。** 第 10 节原写法要求把“完整源图、编译器”纳入键。实际只需对后端真正的输入取键：生成的 C 字节、运行时头、实际编译参数（含目标）、工具链锁与编译器路径、本机构建的 CPU 特征。源图或编译器的任何变化只要改变生成的 C 就自动改变键；不改变 C 的变化（如不移动行号的注释）复用产物是正确的。前端只要约 3 ms，不需要缓存。
- **更正 3：缓存不能把编辑循环降到 200 ms 以下。** 路径对照（36 ms 对 226 ms）测的是“相同 C 换路径”；agent 每次编辑都会改变 C，必然未命中。实测见 18.3。
- **新发现：栈溢出与崩溃时丢失输出**（见第 7 节）；`yavo run` 原先把被信号终止的程序当作内部错误以退出码 4 静默退出，与“编译器内部错误”混淆。
- **方向性问题：** 六批以来工作集中在功能和证据完备性（哈希绑定的工件、数百项引用复核），而语言的核心指标——agent 编程效率——仍停留在第二轮结果，且该结果落后 Rust。第 10 节已把第三轮实验提到第一优先。另：本文件已约 65 KB，每个新会话读一遍成本不低，后续宜把第 11–17 节历史批次压缩为摘要并移到 `history/`。

### 18.2 实现

- `src/cache.rs`：内容寻址后端缓存。布局 `<root>/build/<key>/{main.c, yavo_rt.h, out, out.sha256}`；C 输入放在每个键的稳定路径上，缓存条目被淘汰后 zig 自身缓存仍能命中。命中前校验产物 SHA-256，不符或校验和缺失按未命中重建并覆盖；发布顺序为“写唯一临时文件 → 改名为 out → 原子写校验和”，并发写同一键结果一致。复制而非链接到用户输出，用户修改输出不影响缓存。条目数上限 256（`YAVO_CACHE_MAX_ENTRIES`），按最后使用时间淘汰。
- 只在固定 zig 工具链下启用；`YAVO_CC` 覆盖没有可哈希的身份，照旧不缓存。缓存目录不可用时回退为无缓存构建，不让构建失败。`yavo verify` 走自己的构建路径与全新 zig 缓存，不读写本缓存，可复现检查不受影响。
- `toolchain.rs`：编译参数集中到 `Toolchain::flags`，缓存键与实际命令用同一份列表，不会漂移。
- `yavo run`：程序被信号终止时打印信号名，SIGSEGV/SIGBUS 附“多半是栈溢出、未刷新输出已丢失”的提示，退出码 128+N（原为 4）。USAGE 写明退出码与缓存环境变量。

### 18.3 测量（`benchmarks/small-c-v1/results/build-cache.json`）

空闲 4 核服务器，固定 zig 0.17.1，每项 7 次取中位数。

| 场景 | Release | Debug |
| --- | ---: | ---: |
| 未改源码重建（命中） | 2–3 ms | 10 ms |
| 编辑后重建（未命中，四项基准） | 214–247 ms | 159 ms（vec_transform） |
| `YAVO_NO_CACHE=1` | 215–240 ms | — |
| 前端 `check` | 3 ms | 3 ms |

只编译空 C 并链接约 47 ms；只含运行时头约 105 ms（`-O2`）/ 96 ms（`-O0 -g`）。每次未命中约 50–60 ms 花在运行时头上，预编译运行时 `.o` 是下一步（第 10 节第 5 项）。完整测试套件在缓存加入后用时基本不变（Release 58–65 s）：大头是 verify/conformance（按设计绕过缓存）和实际运行程序。

### 18.4 验证

- `tests/build_cache.rs` 9 项，连续 8 次运行全部通过（并发用例无抖动）。
- 完整套件单命令：Release 122/0/1（冷缓存 65 s、热缓存 58 s），Debug 122/0/1（135 s）。冻结 conformance 统计不变（126/0/3/0）。
- 本批未改代码生成与运行时，第十一批的 ASan+UBSan、可复现构建与运行性能结果仍然适用，未重跑。
