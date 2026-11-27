# Yavo 交接文档（2026-11-27，第五十一批：标准错误值、完整 Result 与真实 I/O）

给下一个会话：先读第1、2、10、31节及 docs/self-hosting.md、selfhost/README.md。第二十七至第三十批在 `history/handoff-batches-2026-11-23-to-24.md`，第十二至第二十六批在 `history/handoff-batches-2026-11-21-to-23.md`，第六至第十一批在 `history/handoff-batches-2026-11-19-to-21.md`。

## 1. 一句话状态

**第五十一批交付四种名义标准 Error、prelude Utf8Error、已有十个 fallible 调用的完整 Result，以及命名 catch 和标准 Error 的真实载荷 match。** 错误可保存、传参/返回、显示、匹配及 try 传播；相同载荷布局的 fs/io/os/process Error 仍互不转换。一般用户载荷枚举、普通 match、错误容器、qualified payload 构造及未声明API继续保持边界。

本机完整自举 **1,136/1,136、20/20、0 skip**；标准错误 **48场景 × 3实际前端 × 2模式 = 288/288**，另6项oracle控制、4项清理控制和3项原词法边界检查通过。stage2/stage3 C **1,774,302字节**、Linux二进制 **729,968字节**，两代分别逐字节相同；JSON/text/supplement **196/160/16**、SHA256 **276**、普通driver **114/114、9/9**通过。本机正式门已通过；本提交的三平台 CI 和 Linux 独立 ASan+UBSan 标准错误门仍待确认。本机该 sanitizer 门为 off，未计为通过。

新增73项、10项旧例原源码提升，清单共1136：run319、panic34、reject600、unsupported141、shared_unsupported42。73个新增语义用例中，一项最初使用 `['a', 0xE2, 0x82]`，触及既有非十进制词法 U0001 边界。按显式唯一 amendment 新增等值 `['a', 226, 130]` 源文件，并只替换该 manifest 项的 file；原稿、原 freeze、原 manifest 快照和首次候选失败全部保留。旧 stage0 对原/新源 Debug/Release 的4次执行均输出 `1\n`，预期未改；正式 stage0 接受原稿，stage1/2仍报 U0001。 原622份语料字节与stage0分类不变，覆盖 **119→125/394**、入口字节去重 **56→62/224**，零退步/越界/超时；6个新接受原入口另经12场景、48次Debug/Release差分运行。未修改Rust、生产runtime或工具链锁。本批未重新运行本机 Rust 全量；同源 batch49 的 Debug/Release 各151/0/1保留为历史证据。

**SH-2保持固定点，SH-4正式CLI保持3/3，SH-3仍未默认接替Rust。** 下一批优先 Copy 元组 Vec：当前首阻塞共25路径/11份入口字节，其中Copy为22/8，另3/3是包含String的拥有型元组。先冻结空/非空构造、push/pop、字段及参数/返回、索引读写/遍历与边界诊断，再用原文暴露后续限制。Map值首阻塞27/14混合Counter、元组、String、Vec和Set，不统一归入Copy。首阻塞计数不等于预期覆盖增量。 交叉目标、缓存/PCH、安装、其他工具入口、更广语义及丰富诊断仍待迁移；正式性能P50与相对Rust的交付成本另验。[完整验证索引](compiler/validation/batch51-standard-errors-2026-11-27.json)。

## 2. 仓库与环境

- 仓库：`/data1/workspace/github/workspace/yavo`（通过 mcp-3 访问，工作区根是 `/data1/workspace/github`）；远端 [msly/yavo](https://github.com/msly/yavo)，发布与 CI 已获用户授权。
- Agent 计量工具的服务器 Python：`/home/appops/.yavo/python/cpython-3.13.18-20261110/python/bin/python3`。项目专用安装，未改系统包或全局 PATH；使用该绝对路径运行 `tools/agent_eval/` 和第六轮脚本。SSL、HTTPS、Perl 与服务器离线检查已通过，见 [运行时回执](compiler/validation/agent-eval-runtime-2026-11-22.json)。
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
| `tests/build_cache.rs` | 14 个集成测试：原有的内容缓存、错误路径、并发、回退和 LRU；新增 PCH 跨源码变更复用/二进制一致、目标/模式/插桩隔离、损坏自愈、PCH 独立 LRU，6 路并发增查 PCH 临时文件清理 |
| `tools/sanitizers/` | 独立源码导出与 ASan+UBSan 执行；能力负例先通过，再检查精确输出、退出状态、源码/工具链/工件哈希 |
| `benchmarks/small-c-v1/` | 固定四项 C 对照、独立正确性参考、两次预热与七次计时；编译延迟与源码路径缓存诊断单独记录 |

常用命令：`yavo check f.yv [--json] [--emit-c f.c]`、`yavo fix f.yv [--dry-run] [--json]`、`yavo run f.yv [--release] [--target T]`、`yavo build f.yv -o out [--release] [--target T] [--emit-c f.c]`。
新增：`yavo allocations 函数名 --file f.yv --json`、`yavo verify f.yv [--target T] [--sanitize] [--reproducible] --json`；全部选项见 `compiler/README.md`。
入口文件可通过 `use` 加载整个可达模块图；模块函数查询使用 `yavo allocations util/maths.double --file main.yv --json`。多文件 `fix` 当前明确报 `U0001` 且不写入。长套件可以分组执行，但必须逐组保留结果，不能把驱动超时描述为一次完整通过。

## 4. 已完成（摘要）

完整提交表（P0 到第十一批，每个提交的内容与当时的一致性通过数）已移到 `history/handoff-batches-2026-11-19-to-21.md`；`git log --oneline` 是最终依据。最近的提交：

| 提交 | 内容 |
| --- | --- |
| `b3c27a6` | 元素上的修改方法、下标赋值求值与借用顺序、诊断提示（第十五批） |
| `6716b47` | 第四轮同题实验与成本、T10 边界失败归档 |
| `1af1f69` | 下标赋值、两阶段接收者、String/str 比较、字段默认值、P-STACK |
| `20b84a6` `7cc377c` `642e8be` | 速查卡与编译器对齐；第三轮冷启动实验（第十三批） |
| `a9fbefc` | 后端内容寻址构建缓存；`yavo run` 报告信号（第 18 节） |
| `68024e2` `0a69e75` `bec0ed1` `12531c3` | Arena/Id、源文件模块、Arena 跨模块与线程组合回归 |
| `cd1e7ed` `78ee9d5` `2b994fb` | 独立 ASan/UBSan 工作流；小规模 C 性能对照 |

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
- 一个特性做完：补 `run_*` 和 `errors_*` 两类测试；跑全部测试（debug、release）；跑堆块检查；交叉编译抽查；刷新一致性基线；更新 `compiler/README.md` 对应小节和根 `README.md` 的数字；提交并按已授权的远端/CI 流程验证。
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
- 标准库导入：已实现成员有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText`/`writeNewText`、`math.sqrt/pow/floor/ceil/round/pi/e`，其余报 `U0001`；每个源文件须自行导入。`os.exit` 不运行析构。
- 枚举和结果：不支持字面量模式（`.Num(0)`、`1 =>`）、模式里的命名字段、`if let` 带模式、带载荷枚举的比较、泛型枚举、结构体里的结果字段。
- `Box`：没有 `.clone()`（写 `Box(b.clone())`）、不能打印或比较。
- 接口：已支持 `+ Send`/`+ Sync`；接口作泛型约束只做检查（单态化后直接调用具体方法），没有接口继承、`Self`、默认方法；`struct S[T]: I` 报 `U0001`。
- 泛型：**单态化，函数体只在实例化时检查**——从没被调用的泛型函数完全不检查，同一错误可能在每个实例报一次；结构体字面量要写类型参数（结构体自己的方法里除外）；
  没有泛型枚举、方法自己的类型参数、`Stack[int].make()` 这类在实例上的静态调用。
- 浮点：`%` 和位运算不支持；`1 + 1.5`（两个字面量）报类型不符；浮点打印与解析已由 Windows/macOS 原生 P0 fixtures 执行验证（依赖各自 libc 的 `snprintf`/`strtod`）。
- 切片：没有 `x[a..]`、`x[..b]`；v1 本来就没有可变切片。
- 闭包：支持 `sortBy`/`update` 的不逃逸借用回调和 `Heap.by` 保存的拥有比较器；一般闭包值、用户高阶参数和闭包字段仍未实现。Heap 的 clone 与隐藏捕获跨线程能力暂报 U0001；Deque 未提供索引、切片或迭代接口。
- 元组：不能给元素赋值、不能比较、不能整体打印、不能做 `Map` 键、没有元组模式。
- 格式说明只支持浮点的 `:.N`；`String` 与 `str` 已可直接比较（K1 修订）。
- `Set`：没有集合运算（并、交）、没有 `Set` 字面量里的元素（`[1, 2]` 不能直接当 `Set`）。
- 堆块检查发现不了释放后使用；项目服务器的固定 Zig 仍缺少 ASan 运行时。独立 ASan+UBSan 工作流已建立，当前源码运行结果与第七批的 TSan 历史证据见 `history/handoff-batches-2026-11-19-to-21.md`（原第 12、17 节）。原生 CI 新增 Windows x86-64/macOS arm64 选定套件；Linux aarch64、Windows aarch64、macOS x86-64 仍只有交叉编译与文件格式证据。
- **栈溢出（V14）**：P-STACK 依赖信号备用栈（Linux/macOS）或向量化异常（Windows，已执行原生 P0 panic fixture）。故障地址判定用线程栈顶与最大长度（主线程取 `RLIMIT_STACK`）；含用户 `Drop` 或接口值的深层 `Box` 结构仍递归析构，极深时会以 P-STACK 结束。
- 并发：`spawn` 只接受具名函数调用，可变借用（包括泛型元组中嵌套的 `&inout`）暂报 `U0001`；`Join` 不能写出类型名或移动（`U0001`）。规范已定义 inout 线程参数与公开 Join，属于实现缺口。
  容器递归克隆遇到不可克隆资源现已在检查期拒绝；用户 `Drop` 类型的自动逐字段克隆报 `U0001`，须显式构建资源或逐元素调用用户方法。
  当前接口集合的克隆检查遍历本编译单元所有具体实现；未来分离编译需要显式传递该保证。`&b` 不会把 `Rc[T]` 转成 `&T`（规范只对字段、方法、下标投影）。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

当前 **124 通过、0 判错、3 不支持、0 未运行、2 被后续决定取代**。P1 为 81 通过、2 superseded；P2 为 20 通过、1 不支持；P3 为 23 通过、2 不支持。完整阶段承诺仍需 history 中“完整 v1 目标”清单的功能与证据。

| 卡点 | 测试与状态 |
| --- | --- |
| 锁定 Zig 缺少可链接 ASan 运行时；ASan+UBSan 能力探针失败 | T-GR9、T-P3、T-R2：unsupported |
| TSan | 有负例数据竞争检测证明；要求该配置的场景实际单独运行，不能替代 ASan |

第十批解锁 T-O5、T-V3、T-GR8 和 T-S3；第十一批新增 Arena/模块实现回归，冻结 129 项统计保持不变。运行器已接入 run-allowed、run-exit、故障注入和 sanitizer 分开构建；缺少平台或插桩能力会保留明确状态。

## 9. 计划表

| 序号 | 任务 | 状态 |
| --- | --- | --- |
| 1–10 | 可选值、Map、导入、枚举与结果、方法/Drop/接口/Box、`*p`/`&inout T`、浮点、切片、泛型 | 已完成（第一至第四批） |
| 11 | `yavo fix` | 已实现保守修复与候选补丁；V2 修订后 `v.push(v.len())` 本身合法；多文件 fix 仍报 U0001 |
| 12 | P3 并发 | 已实现主要路径；公开 Join / spawn inout 仍缺 |
| 13 | `yavo verify` | 已实现；审计未实现，总体 `not_verified` |
| 14–15 | Arena/Id；多文件源模块 | 已实现；包依赖与程序锁文件待做 |
| 已落地 | 自举固定点 | `67794d9`：引导子集编译自身，stage2.c = stage3.c，335/335 例，stage2 与 stage1 全用例一致（history 原第 11 节） |
| 已落地 | Result 与真实词法传播 | 第二十九批：372 个自举用例，用户 Result/try 与拥有型 Diagnostic；真实 lexer 正常返回清理门（history 原第 13 节） |
| 已落地 | Copy Optional 与原地退栈 | 第三十批：439 个自举用例、14 处 pop 替换五个 helper；run 45345384747 六项成功并完成三平台归档审计（history 原第 14 节） |
| 已落地 | for 与真实遍历 | 第三十一批：490 例，range/Copy Vec for，四处真实迁移（第 11 节） |
| 已落地 | 插值与 format | 第三十二批：547 例，四处真实迁移，修复 Rust 输出片段借用缺陷（第 12 节） |
| 冷启动实验 | 五轮 | 第五轮两语言各 24/24 首编、公开、隐藏及结构通过；受控操作/时间比 1.0455/1.2051，token 缺失；4 会话、12 题（第 22 节） |
| 已落地 | 三平台 CI | `073003d` 的 run 45002067581 六项全通过：Linux 全量、Windows/macOS 选定原生套件、独立 ASan+UBSan，以及两项实验检查（第 23 节） |
| 已落地 | 第五项 CI | `0bad3d2` 的 run 44996118634 通过，核验 1,344 条判定、96 个源码阶段、16 项结构审查和冻结题包 |
| 已落地 | 第六项 CI | `073003d` 通过 33 项执行器、25 项 lab/audit/freeze、11 项裁判/汇总场景，以及 94 个准备文件和 112 个独立 oracle 输入；真实启动仍 blocked |
| 准备完成，资格 blocked | 第六轮文档对照 | 短卡/索引、逐请求计量执行器、8 题与 112 个输入已准备；0 受试会话，须先获得真实 usage 资格 |

批次与用户约定的历史（“可用”三批、第四到第六批）见 `history/handoff-batches-2026-11-19-to-21.md`。

## 10. 下一个会话的第一步

1. 核对Git与最新CI，先读第31节、[自举路线](docs/self-hosting.md)、[selfhost支持范围](selfhost/README.md)和[验证索引](compiler/validation/batch51-standard-errors-2026-11-27.json)。正式本机1136/1136、20/20，标准错误288，JSON/text/supplement196/160/16、SHA256276、普通driver114/9。本机正式门已通过；本提交的三平台 CI 和 Linux 独立 ASan+UBSan 标准错误门仍待确认。本机该 sanitizer 门为 off，未计为通过。
2. **下一批优先 Copy 元组 Vec：当前首阻塞共25路径/11份入口字节，其中Copy为22/8，另3/3是包含String的拥有型元组。先冻结空/非空构造、push/pop、字段及参数/返回、索引读写/遍历与边界诊断，再用原文暴露后续限制。Map值首阻塞27/14混合Counter、元组、String、Vec和Set，不统一归入Copy。首阻塞计数不等于预期覆盖增量。** 保留原622文件及全部原例字节；新增能力必须经旧stage0独立预期与声明的U0001边界验收。
3. 本批标准Error是四种不同名义Copy类型，短名Error不会由std导入自动引入。Utf8Error是有offset字段的Copy struct，直接显示仍E0001；标准Error-only match不能顺带开放用户enum/Optional/Result匹配。现有十个fallible调用使用完整Result并共享传播/清理路径。
4. 保留[测试输入修订记录](selfhost/tests/errors51/semantic-amendment.json)：原hex源和失败不能删除，也不能把本批描述成73份新源码全程未修订。五个旧例的name/PATH环境和独立cwd仅在执行时生效，编译环境与ASAN/UBSAN控制不改。
5. 394个stage0接受入口仍有 **269** 个未接受，按入口字节去重为 **162** 个。原生普通build/run和首错check JSON已交付；接着按实际依赖迁移交叉目标、cache/PCH、安装及fix/verify等工具。build/run JSON、warning/note/fix和多错误恢复各自冻结合同；冻结agent文档不可直接改写。

正式门槛仍按`docs/plan.md`：配对通过率差的单侧95%下界至少−5个百分点，单位成功交付成本较Rust低至少20%并独立复现；冻结运行基准几何均值不超过C的1.5倍。小程序编译P50≤200ms、Linux小程序strip产物≤250KB、原生平台和可复现构建分别验收。本批命令耗时/RSS是单次观测，token/金额/P50未测。

## 历史批次（已移出）

第六至第十一批在 `history/handoff-batches-2026-11-19-to-21.md`；第十二至第二十六批（原第 18–31 节：构建缓存、第三到第五轮实验、CI、计量执行器、PCH、SH-1 各批）在 `history/handoff-batches-2026-11-21-to-23.md`。
第二十七至第三十批（原第 11–14 节：引导子集固定点、首个 CLI 与 stage2 sanitizer、Result、Copy Optional/pop）在 `history/handoff-batches-2026-11-23-to-24.md`。

## 11. 第三十一批：for 与真实遍历迁移（2026-11-24）

- AST 追加 ForRange/ForEach，复用 name/expr/target/body 池。range 支持 int/byte 半开区间、双向字面量上下文、空/逆向/极值；两个端点在新 loop frame 前物化，头部 catch break/continue 归属外层循环。C for 第三子句步进，有限循环保留零次路径。
- Copy Vec 支持名称及括号形式的局部、普通参数和 inout 参数；元素限 int/byte/bool/非拥有 struct/enum。独立 iterationLocks 防止遍历根的写入、push/pop/reserve、替换、take、移动和 inout 调用；嵌套退出仅撤销本层锁，所有权回放保留循环变量和锁。循环头拥有临时值在外层 C 作用域关闭前释放。
- 四处迁移：lexer_test 的 32 次 token 构造、512 个前导零、16 轮 Diagnostic 清理，以及 parser.store 的 Vec[int] 复制。结构复核要求三个受影响函数除这些机械替换外保持原文，输入字符串及断言原样。
- 独立对照发现 Rust For lowering 延迟读取 local/field/byte 起点。仅新增 self.bind(start.ty, &a)，在右端副作用前快照；新增 compiler/tests/p0/run_for_range_capture.yv。旧 Debug/Release 的 local/field/index/byte/cached 和为 0/0/6/0/6 并 P-ASSERT；修后五项均为 6、exit 0。原失败与修后证据保留在本批 oracle。
- 52 项本批语料包括 17 run、25 reject、10 unsupported；旧 for fixture 原样晋升，净新增 51。总表 490 = 144 run + 28 panic + 243 reject + 61 unsupported + 14 shared unsupported；172 个可执行输入。修后 Rust 独立 oracle 34 次运行通过。字段/临时/拥有/可变借用/双绑定集合/slice/Map/Set 保持 U0001；双绑定 range 为 E0020，不可迭代标量为 E0001。
- 本机回执 selfhost-for-2026-11-24.json：490/490、12/12、0 skip，stage2/stage3 C 685,990 字节、Linux 二进制 321,368 字节分别一致；真实 lexer 两模式通过。修后完整 Rust Debug/Release 各145/0/1，冻结规格124/3/2；生成程序 sanitizer 输入为173（172 fixtures + lexer），实际 GCC 门由本批 CI 提供。
- 实现 `046ccc6` 的 [run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591) 六项全过；三平台各490/12/0、平台内C/二进制固定点、真实lexer与CLI各213次执行通过。Linux GCC生成程序173、stage2自编译及490checks/172emits/8坏输入通过。三份原生归档复核44,165文件、25,476嵌入流，837个本机输入绑定到提交；详见[原生索引](compiler/validation/batch31-native-2026-11-24.json)，原本机回执历史字段未改。

## 12. 第三十二批：插值与 format（2026-11-25）

- 基线 `08f19fa`。lexer 把含 `${` 的字符串拆成 StringStart / Text / InterpolationStart … InterpolationEnd / StringEnd 记号序列；片段内表达式就地重新词法分析（保留原文件偏移、行列），嵌套上限 128。片段内的词法错误存为 DeferredError 记号，解析到该处才报告，从而与 Rust 的报错先后一致。格式说明符为 FormatSpec 记号；`:.N` 用于非 float 报 E0001，其他说明符 U0001。
- parser 新增 `Interpolated` 表达式与 `Program.stringParts`；括号包裹的裸插值不是消息参数（E0001），与 Rust 一致。codegen 的 `messageWrites` 先把所有片段求值进 g.out，再把写出代码集中追加；String 片段以借用描述符快照，并在整条消息期间锁住其根。format 的结果只在全部片段成功后才登记为临时值，避免后续片段 try 时清理未声明的结果。
- 真实迁移四处：support.report 的完整诊断行、codegen.temporaryName、localName、lineColumn；诊断与生成 C 中对应字符串保持逐字节不变（固定点与全用例 stage1/stage2 一致即为证明）。其余 join/decimal 调用未改。
- 接手时的偏差与修正：上一会话的实现在 lexer 里写了 `tokens[head].end = …` 和 `tokens[marker].extra = …`，即给拥有型 Vec 元素的字段赋值——stage0 接受但超出引导子集，stage1 编译自身时报 U0001，固定点断开，全部 559 项失败。修正为不回写已入栈的记号：StringStart/StringEnd 只覆盖各自的引号；删掉只写不读的 `Token.extra`，DeferredError 紧跟在片段内记号之后。lexer_test 两处断言相应改为 `end == 3` 与 `fault == marker + 1`。此后 559/559 通过。
- Rust 修复见第 1 节；本批独立定标材料在 `selfhost/tests/evidence/batch32-format-oracle/`（58 例：16 run、36 reject、4 unsupported、2 shared；原 `unsupported_interpolation` 原字节晋升为 `run/interpolation_basic.yv`）。这是按旧流程生成的最后一份 oracle 包。
- 验收（新流程）：本机自举 547/547、12/12、0 skip，stage2/stage3 C 727,751 字节、Linux 二进制 339,512 字节一致；Rust Debug/Release 各 145/0/1、冻结 124/3/2。CI：`cc21ae5` 的 [run 45533511159](https://github.com/msly/yavo/actions/runs/45533511159) 五项成功、Windows 作业被后续推送取消；包含本批全部改动的 `1708d4b` [run 45535558990](https://github.com/msly/yavo/actions/runs/45535558990) 6/6 成功（含 Windows 上的 Rust Debug/Release 与 selfhost）。

## 13. 第三十三批：源模块（2026-11-25）

- 基线 `cc21ae5`。词法：新增 `lexInto(source, path, &tokens)`，多个文件共用一个 token 向量，每个文件以自己的 Eof（文本为路径）结尾；`fail` 从出错 token 向后找到所属文件的 Eof 取路径，单文件行为不变。token 新增创建时写入的 `lineBase`（前面各文件的行数之和），生成程序的运行时位置使用程序级行号，并在多文件时输出与 stage0 相同的 `yv_source_entry` 源文件表，panic 因此报在模块原文件。
- 解析：`parse` 拆为 `startParse`/`parseFile`/`finishParse`；Function/TypeDecl 记录 `file` 与 `public`；`SourceFile` 记录每个文件的首 token、std 导入与模块导入区间；`ModuleImport` 记录别名与路径前缀，文件号由加载器回填（Copy 元素，可在子集内写回）。别名在同一文件内重复报 E0011，与本文件顶层函数/类型同名报 E0011。类型位置的 `m.T` 以模块 token 为名、后随 `.`；表达式里的 `m.T{...}` 把模块 token 存在 Construct 的 extra。
- 加载（main.yv `compile`）：以入口文件所在目录为根（同时识别 `/` 与 `\`），按模块路径（`a/b`）去重、广度优先；入口自身的路径键是其文件名去掉 `.yv`，所以自导入与循环导入都复用已有文件号。读不到文件报 E0010（位置在导入别名），模块里的词法错误以模块路径报告。
- 名字解析：无限定调用只看本文件的函数；`m.f()` 需要 m 是本文件的导入且 f 为 pub；类型短名先找本文件，再找导入模块中唯一的公开类型（二义 E0010）；`m.T` 当值是 E0001；局部变量遮蔽模块名。只有入口文件的 `main` 是程序入口，模块中的 `main` 是普通函数。函数、类型的 C 名称本来就按下标编号，跨文件同名不冲突。
- 用例 21 个（`selfhost/tests/modules/`）：run 4（basic 含子目录与传递使用、cycle 含互递归/菱形/自导入、scopes 含同名类型/函数/局部遮蔽/模块 main、owned 含 String/Vec/inout/Result/per-file `use std/{os}`），panic 1（模块中除零，位置为 math.yv），reject 15，unsupported 1（`use` 在其他项之后）。所有错误码先用 stage0 实测；`modules_duplicate_in_module` 中 stage0 先报入口里的 E0010、随后报 E0011，selfhost 首错即停报 E0011。
- 验收：本机 568/568、12/12、0 skip，stage2/stage3 C 783,814 字节、Linux 二进制 365,864 字节一致。未改 Rust。
- 编译器自身改为模块（单独提交，决定 28）：`/tmp` 下的脚本把跨文件调用改成 `模块.函数(...)`（codegen 541 处、types 47、parser 24、main 15、lexer_test 3、lexer 2），跨文件使用的项补 `pub`，各文件补 `use`；main.yv 的局部 `parser` 改名 `state` 以免遮蔽模块名。bootstrap.py 把 `COMPILER_PARTS` 与 lexer_test.yv 原样复制到 `run-*/selfhost-src/`，只交入口文件；回执的 `compiler_bundle`/`lexer_unit_bundle` 保留键名，新增 `directory`，`path` 指入口。`tools/ci/selfhost_compiler_sanitize.py` 与 `selfhost_sanitize.py` 的拼接字节核对改为逐文件核对（本机无 GCC，靠 CI 验证）。模块用例的非入口文件纳入 `fixture_modules` 输入哈希。改后本机 568/568、12/12，stage2/stage3 C 785,468 字节、二进制 366,936 字节一致。
- CI：模块功能 `1708d4b` 的 [run 45535558990](https://github.com/msly/yavo/actions/runs/45535558990) **6/6 成功**：Linux x86-64（Rust 全量、selfhost 568 例、test-summary、stage2 ASan/UBSan）、Windows x86-64、macOS arm64 原生均通过，模块路径在 Windows 分隔符下正常。编译器改为模块的提交 `636f84a`：[run 45539950828](https://github.com/msly/yavo/actions/runs/45539950828) 成功。注意工作流 `cancel-in-progress: true`，同分支新推送会取消进行中的运行，推送前先等上一次运行结束。


## 14. 第三十四批：借用返回（2026-11-25）

- 基线 `636f84a`。目标是解除 examples/test-summary 的首个阻塞（`-> Vec[str] of line`、`-> str of raw`）。本批覆盖 `-> str of …`；`Vec[str]` 留给下一批。
- 解析：`of name` / `of (a, b, …)` 跟在返回类型后，与 stage0 一样接受任何词（含 `static`、`self`、关键字），检查时才解析为参数；非词报 E0020。Function 新增 `sourced/sourceStart/sourceCount`（名字记号存在 Program.lists）。
- 签名规则（函数体之前检查，同 stage0 的 check_ret_of）：返回 str（或 ok 为 str 的 Result）未写 `of` 报 E0302；名字不是参数报 E0010；ok 为 str 的 Result 写了 `of` 报 U0001；`of` 用在不带借用的返回类型上照常接受。
- 来源模型：子集里持久的 str 只指向参数或静态数据（拥有 String 的视图是 transient，不能绑定），所以 Local/Value 各加一个 `sources` 位掩码，第 k 位是第 k 个参数，静态数据没有位。字面量 0；str 参数 `1 << k`；切片继承；赋值替换。if 按可落下的路径合并（与 moved 状态同一判断），break/continue 的来源记在与 `exits` 平行的 `sourceExits`，循环用 replayBody 重放到入口状态不再增长，退出状态 = 入口不动点 ∪ break。参数超过 62 个且需要位时报 U0001。
- 返回检查（O2）：非 transient 值的来源须 ⊆ `of` 列出的参数；transient 值指向拥有局部或临时值报 E0303，指向 String/inout 参数时未列出报 E0303、列出报 U0001（stage0 接受）。调用方按被调函数的 `of` 合并对应实参的来源；对应实参是拥有 String 或 transient 时结果是同一根的 transient（不能绑定，输出期间锁根，所以 `"${first(owned)} ${change(&owned)}"` 报 E0102）；两个不同根报 U0001；`of` 指向 inout 实参报 U0001。
- O4：给 `inout str` 参数赋值时右值来源必须为 0（只能是静态文本），否则 E0306，与 stage0 相同（连 `s = s[1..2]` 也拒绝）。这顺带堵上了接手前 selfhost 接受 `fn set(s: inout str, b: str) { s = b }` 而 stage0 拒绝的差异。
- 顺带修复：replayBody 的探针没有复制 `g.file`，在非入口模块里重放会按入口文件解析无限定调用。此前 moved 重放几乎总会以 E0402 结束所以没暴露，本批的来源重放通常会成功，`modules_borrowed` 用例覆盖了这一点。
- **Rust stage0 缺陷（决定 27）**：loans 按最后一次写入记录，if/match/catch 之后不合并，循环也不把后面写入的来源带回下一轮。`if c { t = b } else { t = a }`、循环里先 `return t` 后 `t = b`、`t = u; u = b` 链式传递、只在 continue 路径上写入等都被接受；实测 `pick("lit", temp.toString())` 返回已释放内存（打印出后来分配的字节）。修复（check.rs）：新增 `Loans` 快照/恢复/合并，if、两种 match、catch、`?? return` 与 moved 同处处理；loop_body 重放到 loans 不再增长（上限 256 轮），重放中保留 E0402/E0303/E0305/E0306/E0102/E0703。新增 `compiler/tests/p0/errors_loan_paths.yv`（9 处，旧 stage0 只报出其中 break 那 1 处）与 `run_loan_paths.yv`（合法流程仍通过，Debug/Release 堆检查干净）。
- 用例 30 个：run 3（`borrowed_return_basic`、`borrowed_return_flow`、`modules_borrowed`），reject 22（E0302/E0010/E0020/E0303 各类/E0102/E0306 四类），unsupported 5（String 参数来源、inout 来源、持久视图、ok 为 str 的 Result、两个拥有视图）。全部先用修复后的 stage0 实测；其中 7 个拒绝在旧 stage0 上会被接受。
- 编译器自身迁移：`integerOperation` 的运行时函数名改由 `checkedStem`/`wrappingStem`（`-> str of static`）给出，不再为每个运算符分配 String；生成的 C 逐字节不变。
- 验收：本机 598/598、12/12、0 skip，stage2/stage3 C 818,715 字节、Linux 二进制 381,784 字节一致；Rust Debug/Release 各 145/0/1，冻结 124/3/2。CI：`01c7856`（Rust 修复）与 `b6694a7`（本批）一起推送，[run 45558307665](https://github.com/msly/yavo/actions/runs/45558307665) **6/6 成功**：三平台 Rust Debug/Release（含修复后的 loans 检查与新 p0 用例）、selfhost 598 例与固定点、test-summary，Linux stage2 ASan/UBSan。

## 15. 第三十五批：Vec[str]（2026-11-25）

- 基线 `f3fac89`。目标是 test-summary 的 `fn fields(line: str) -> Vec[str] of line`；该函数原样放进 `run/vec_str_basic.yv`。
- 类型：`types.vector` 放行 str 元素；新增 `carriesBorrow`（str 或 `Vec[str]`），字段与错误类型里出现即 E0301（与 stage0 相同），参数、签名 E0302 检查、返回检查与调用合并都改用它。
- 来源：`Vec[str]` 局部的位掩码是元素来源的并集。`push`、`v[i] = s` 合并；整体赋值替换；移动（修正了 `takeValue` 移动拥有值时丢失来源）、`take`（被清空的位置保留来源，同 stage0）、`clone` 保留；字面量取元素并集；元素读与 `for` 绑定继承容器来源（`for` 现在允许 str 元素）。元素来自拥有 String 的视图时 U0001。
- 调用：列在 `of` 中的实参若是元素为 Copy 的 Vec，贡献其来源；其他拥有值（String、`Vec[String]`、拥有结构体）或 transient 视为语句内视图；结果为 `Vec[str]` 时这种视图 U0001。
- O4 与 inout：`storeSources` 统一处理三种写入。`inout` 参数只能接收静态文本（E0306）；`out.push(out[0])` 报 E0403（同 stage0）；只要还有别的局部指向该 inout 参数就报 U0001。stage0 按活跃性判断（不再使用则接受、仍使用则 E0102），U0001 对两者都安全。这关掉了第三十四批遗留的差异：子集曾接受 `let t = s; s = "x"; println(t)`（`s: inout str`），stage0 报 E0102。
- **Rust stage0 缺陷（决定 27）**：调用点 `recv_loans` 对 String/结构体以外的实参只取元素借用。`fn first(v: Vec[String]) -> str of v { return v[0][0..1] }`、`fn head(v: Vec[int]) -> []int of v` 的结果实际指向实参存储，`owned = [...]` 后仍可使用，实测打印乱码。修复：拥有堆内存的实参按存储借用记录（局部/字段/下标取 storage_loans，临时值记 Temp）；只有元素为 Copy 的 Vec 且返回类型的借用全是 `str`（新增 `only_text_borrows`）时保持只取元素借用，所以 `let s = first(words); words = []` 仍合法。新增 p0 `errors_owned_view_args.yv`（3 处，旧版只报出结构体那 1 处）与 `run_text_vector_views.yv`（合法用法，Debug/Release 堆检查干净）。
- 用例：新增 25 个（run 1、reject 18、unsupported 6），全部先用修复后的 stage0 实测；旧的 `unsupported_vec_str_element`（`["first", "second"]`）原字节晋升为 `run/vec_str_literal.yv`，记入 promotions。约 100 个探针上子集与 stage0 无“stage0 拒绝而子集接受”的情况。
- 编译器自身没有为绕开 `Vec[str]` 写的变通代码（`main.yv` 的模块路径是拼出来的拥有字符串，lexer_test 的 `Vec[String]` 是有意测试清理），本批无迁移。
- 验收：本机 623/623、12/12、0 skip，stage2/stage3 C 826,223 字节、Linux 二进制 386,344 字节一致；Rust Debug/Release 各 145/0/1，冻结 124/3/2。CI：`fbbffb0`（Rust 修复）与 `7825d08`（本批）一起推送，[run 45567225110](https://github.com/msly/yavo/actions/runs/45567225110) 的三个原生作业都在 setup 步骤失败（日志需管理员权限，本机按 CI 方式复现 setup 全部通过）；随后包含本批的下一次推送 `0db8fb7` 的 [run 45571423128](https://github.com/msly/yavo/actions/runs/45571423128) **6/6 成功**，三平台 setup、Rust 全量（含本批 stage0 修复与 p0 用例）、selfhost 与 test-summary 均通过，确认前一次是环境问题。

## 16. 第三十六批：跳转式 ??（2026-11-25）

- 基线 `2c01ceb`。目标是 test-summary 72 行 `let value = word.parseInt() ?? return err(...)` 的解析期阻塞（`parseInt` 本身属于 str 方法组，留给后续）。
- 解析：`??` 右侧遇到 `return`/`break`/`continue` 时生成新表达式 `Jump`（记号为关键字，`left` 为返回值或 -1）；`return` 后紧跟换行、`}`、`)`、`,` 或文件结尾时无值，同 stage0 的 `jump()`。
- 位置规则（同 stage0）：只能是 `let` 的整个初始值或 `=` 赋值的右侧，生成器用 `jumpExpr` 标记允许的表达式 id，`a ?? b ?? return` 沿右侧链传递；加括号、实参、运算式、复合赋值报 U0001（stage0 也是 U0001，记为 shared_unsupported）。
- 生成：把 `return` 语句与 `break`/`continue` 语句的生成抽成 `returnStatement`、`loopExit`，语句与跳转分支共用（抽取后已有用例生成的 C 逐字节不变）。跳转分支放在 `if (!x.has) { … }` 中，结束后恢复分支前的 moved 与来源状态。
- 顺带修复第三十四批遗留的子集差异：三处 `catch _ { … }` 块（用户 Result 与两个标准库调用）执行后只恢复 moved 状态，没有恢复来源状态。块里 `t = a; return a` 之后 `return t`（`of a`，`t` 仍指向 `b`）会被子集接受，stage0 报 E0303；上一版 stage1 实测接受，现已拒绝（`catch_block_sources`）。
- 编译器自身迁移：`types.importedFile` 改返回 `?int`；`qualifiedType` 用 `importedFile(...) ?? return -1` 代替 `if module < 0 { return -1 }`，`moduleOf` 用 `?? -1` 保持原接口。stage1 编译自身因此必须正确处理跳转式 `??`。
- 用例：新增 14 个（run 1、reject 9、shared_unsupported 4），旧的 `unsupported_optional_coalesce_return/break/continue` 原字节晋升为 `run/optional_coalesce_*.yv`。
- 验收：本机 637/637、12/12、0 skip，stage2/stage3 C 836,154 字节、Linux 二进制 390,648 字节一致；本批未改 Rust，Rust 全量沿用第三十五批结果（各 145/0/1，冻结 124/3/2）。CI：`0db8fb7` 的 [run 45571423128](https://github.com/msly/yavo/actions/runs/45571423128) **6/6 成功**（Linux x86-64 含 stage2 ASan/UBSan、Windows x86-64、macOS arm64；同时覆盖第三十五批）。

## 17. 第三十七批：只读 []T 参数（2026-11-25）

- 基线 `37d358f`。目标是 test-summary 76 行 `fn declaredCounts(words: []str, line: int)`（实参为 `Vec[str]`）。
- 类型：`BasicType.Slice`；解析 `[]T`（元素为单个原子类型，容器/Optional/Result 元素 U0001）；解析期只接受 int/byte/bool/str/Copy 命名类型元素，拥有元素 U0001。`cType` 为 `yv_vec`，不拥有，`carriesBorrow` 为真（字段/错误类型 E0301，返回需 `of`）；`same` 把 `os.args()` 的 `Args` 视为 `[]str`。
- 操作：下标读走 Copy 路径并继承来源；`len`；`for` 遍历；写元素/`push`/给参数赋值 E0014，`var` 切片局部写元素 E0101（同 stage0）；比较 E0501、打印与插值 E0001 沿用通用路径。
- 调用：形参为 `[]T` 时 `Vec[T]` 实参直接传描述符（调用期间锁根）；字面量不按切片元素类型推断（stage0 报 E0001；本批曾误加推断导致子集接受 `total([1, 2, 250])`，已撤销并做成拒绝用例）。列在 `of` 中的 `Vec` 实参被转为切片、或结果本身是切片时，结果视为指向该 Vec 的存储（与 stage0 第三十五批修复后的 SliceOf 行为一致），绑定为局部时 U0001。
- 不支持（U0001）：`let ys: []T = v`、返回 `Vec` 当切片、子切片、`toVec`、拥有元素切片、`inout []T` 参数；`clone` 两边都是 U0001。
- 编译器自身迁移：`contains`、`restoreState`、`restoreSources`、`joinSources`、`sourcesGrew`、`iterationSources` 与 parser 的 `store` 改为 `[]T` 参数（`joinStates` 用 `clone`，保持 Vec）；用户程序生成的 C 不变。
- 用例：新增 26 个（run 1、panic 1、reject 16、unsupported 7、shared_unsupported 1）；约 160 个旧探针无回退。
- 验收：本机 663/663、12/12、0 skip，stage2/stage3 C 849,712 字节、Linux 二进制 394,552 字节一致；本批未改 Rust（Rust 全量沿用第三十五批：各 145/0/1，冻结 124/3/2）。CI：`0889cb8` 的 [run 45576210191](https://github.com/msly/yavo/actions/runs/45576210191) **6/6 成功**（Linux x86-64 含 stage2 ASan/UBSan、Windows x86-64、macOS arm64）。

## 18. 第三十八批：str 查询方法（2026-11-25）

- 基线 `40547b5`。目标是 test-summary `main.yv:18` 的 `arg.startsWith("-")` 及 parser.yv 的 `split`/`endsWith`/`parseInt`。
- selfhost：`textQuery` 处理 str/String 接收者的 `startsWith`/`endsWith`/`contains`（Bool）、`find`/`parseInt`（`?int`，`sh_optional_int`）、`split`（`Vec[str]`），C 与 stage0 同用 `yv_str_starts/ends/find/split/parse_int`。实参个数错 E0012，实参非文本 E0001（String 实参取视图）。`split` 结果来源随接收者：str 接收者继承来源位（可返回 `-> Vec[str] of text`，元素也可按来源返回；未列来源 E0303）；String 或临时接收者的结果是语句内视图，绑定为局部 U0001（stage0 对临时 String 报 E0305、绑定后修改报 E0102，子集一律 U0001）。
- 求值顺序：实参先于接收者文本被读取，所以实参求值期间锁住拥有型接收者（经 inout 修改报 E0102，同 stage0 修复后行为）；str 局部接收者的槽也被守护，实参经 inout 改写它时 U0001（stage0 接受）。
- **Rust stage0 缺陷修复**：`method2` 在求值单实参的文本方法（`startsWith`/`endsWith`/`contains`/`find`/`split` 等）实参前没有持有接收者，`owned.startsWith(grow(&owned))` 通过检查，生成的 C 在实参扩容 String 后读旧缓冲区（释放后使用）。现在实参求值期间以 `hold_call_argument` 持有接收者，报 E0102。P0 回归 `errors_text_method_receiver.yv`（startsWith/endsWith/contains/find/split 五处 E0102）。
- 编译器自身迁移：`main.yv` 的 `.yv` 后缀判断改为 `path.endsWith(".yv")`。
- 用例：新增 12 个（run 1、reject 7、unsupported 2、shared_unsupported 2）；无旧 unsupported 用例被提升；约 200 个旧探针无回退。
- 验收：本机 675/675、12/12、0 skip，stage2/stage3 C 864,824 字节、Linux 二进制 406,680 字节一致；Rust Debug/Release 各 145/0/1，冻结 124/3/2。CI：`72cafa1` 的 [run 45586217122](https://github.com/msly/yavo/actions/runs/45586217122) **5/6**：Linux x86-64（含 stage2 ASan/UBSan）、macOS arm64 与三个独立作业成功；Windows x86-64 在 setup 步骤下载 95 MB 工具链时速度约 60 KB/s，约 20 分钟后被终止（环境问题，未进入测试）。服务器上的令牌没有 actions 写权限（rerun 返回 403），后由第三十九批 `aa3097d` 的 6/6 覆盖（见第 19 节）。

## 19. 第三十九批：遍历拥有元素（2026-11-25）

- 基线 `919c42f`。目标是 test-summary `main.yv:31` 的 `for path in paths`（`Vec[String]`）。
- selfhost：`for x in v` 的元素可以是 `String` 或拥有型结构体（其余拥有元素如 `Vec`、Optional 仍 U0001）。循环变量是元素的只读视图（`Local.element`，按 borrowed 处理：不 drop、不可移动），C 中按值复制描述符；被遍历的 Vec 在循环内加 iterationLock。规则与 stage0 定标一致：移动元素（push、赋值、结构体字段、返回）E0404，修改/赋值/`&x` E0014，移动元素的字段 E0401，循环内 push 或移动该 Vec E0102，`let s: str = x` E0001。stage0 接受而子集 U0001：`let y = x`（stage0 生成第二个只读别名）、遍历临时 Vec、返回元素的视图（如 `x.split(",")`）。
- **子集缺陷修复**：第三十六批的跳转式 `??` 回退分支里创建的拥有型临时值（如 `?? return err(format(...))`、`?? return err("x".toString())`）在分支内清理后仍留在语句的临时列表中，分支后再次 drop，生成的 C 引用未声明的变量，无法编译。现在分支结束时丢弃这些记录。旧用例未覆盖，新增 run 用例 `optional_jump_owned_fallback`。
- test-summary 里程碑：把 parser.yv 的两个 Map 临时换成 `Vec[String]`（`has` 改为恒假，`set` 改为 `push`）后，selfhost `check` 通过，`emit-c` + zig 构建的程序与 stage0 构建在 `tests/cases.json` 的 70 个可在本机执行的场景（跳过需要仓库文件与目录输入的 2 个）及两份归档 R4 日志上 stdout/stderr/退出码逐字节一致（`YAVO_ALLOC_CHECK=1`）。差分脚本思路：逐场景建临时目录写入 `files`，分别运行两个二进制比较三元组。
- 编译器自身：没有可迁移的写法（`Vec[String]` 只有 main.yv 的 `keys`/`paths`，循环需要下标）。
- 用例：新增 18 个（run 3、reject 11、unsupported 4），并把 `unsupported_for_owned_string`、`unsupported_for_owned_struct` 原样提升为 run（`for_owned_string`、`for_owned_struct`，记录在 promotions）；约 220 个旧探针无回退。
- 验收：本机 693/693、12/12、0 skip，stage2/stage3 C 868,714 字节、Linux 二进制 407,544 字节一致；本批未改 Rust（沿用第三十八批：各 145/0/1，冻结 124/3/2）。CI：`aa3097d` 的 [run 45594410475](https://github.com/msly/yavo/actions/runs/45594410475) **6/6 成功**（Linux x86-64 含 stage2 ASan/UBSan、Windows x86-64、macOS arm64），同时补上第三十八批 Windows 未跑完的覆盖（`aa3097d` 包含其全部代码）。

## 20. 第四十批：Map 与 test-summary 自举构建（2026-11-25）

- 基线 `fd7333f`。目标是 test-summary parser.yv 101–142 行的两个 `Map[String, int]`。
- selfhost Map：类型 `Map[String, V]`（`BasicType.Map`，V 为 int/bool/byte；其他键类型、`str` 键、拥有值或枚举值 U0001），`[:]`（`ExprKind.EmptyMap`，需要期望类型，否则 E0001；也可作结构体字段默认值），方法 `set`（消费 String 键，键先于值取走，同 stage0：`m.set(k, m.getCopy(k) ?? 0)` 报 E0402；接收者预留期间修改它报 E0403）、`remove`/`getCopy`（`?V`）、`has`（str 或 String 查询）、`len`、`clone`；可作局部、参数（只读/inout）、返回值、Result 载荷、结构体字段，drop 与 clone 按值类型生成辅助函数（`sh_map_*_<int|bool|byte>`，与 stage0 生成的辅助函数同结构：插入顺序条目 + 开放寻址槽 + 重建），按需拼接在布局之后。`get`（返回引用）、非空字面量、遍历、比较（E0501）与打印（E0001）同 stage0 或为 U0001。stage0 对 `m.has(grow(&m))` 不持有接收者（生成的 C 在实参之后才读 Map，安全），子集为 U0001。
- 编译器自身迁移：main.yv 的模块键线性查找改为 `Map[String, int]`。
- **SH-3 实际判据达成**：selfhost 原样编译 examples/test-summary（生成 C 约 56.6 KB），zig 构建的 Debug/Release 程序在 `tests/cases.json` 的 71 个本机适用场景上都符合人工期望（stdout、诊断顺序、退出码），且与 stage0 构建逐字节一致（`YAVO_ALLOC_CHECK=1`）。实现为 `selfhost/application.py`（复用 `tools/ci/test_summary.py` 的场景加载、冻结 oracle 校验与期望匹配），由 bootstrap 作为第 13 项检查 `test_summary_application` 运行，三平台 CI 自动覆盖；相关输入纳入 bootstrap 的输入不变检查。
- **子集越界修复**（均为旧缺陷，新扫描发现）：块语句 `{ ... }`（原 E0020）、`assert(cond, "msg")`（原 E0012）、`*x` 解引用（原 E0020）、数值类型转换与常量 `i32(x)`/`u8(x)`/`i64.min`（原 E0010），现在都报 U0001。新增 `selfhost/subset_sweep.py`：全仓 391 个 stage0 接受的程序 0 越界（selfhost 接受 70 个），220 个 stage0 拒绝的程序 selfhost 一个也不接受。
- 用例：新增 40 个（run 7、reject 18、unsupported 13、shared_unsupported 2）；无旧用例提升；约 250 个旧探针无回退。
- 验收：本机 733/733、13/13、0 skip，stage2/stage3 C 937,451 字节、Linux 二进制 439,760 字节一致；本批未改 Rust（沿用第三十八批：各 145/0/1，冻结 124/3/2）。CI：`3e14afc` 的 [run 45608756237](https://github.com/msly/yavo/actions/runs/45608756237) Linux（含 stage2 ASan/UBSan）、macOS 与三个独立作业成功，bootstrap 第 13 项在这两个平台通过；Windows 因慢 runner 超过 45 分钟作业上限被取消（工具链下载 24 分钟，bootstrap 进行到模块用例时到时，此前已通过的 Windows 运行同一作业耗时 17–44 分钟），未出现失败。随后把原生作业上限调到 75 分钟（`.github/workflows/ci.yml`），`82c4254` 的 [run 45616232256](https://github.com/msly/yavo/actions/runs/45616232256) **6/6 成功**（Linux 含 stage2 ASan/UBSan、Windows、macOS；三平台的 bootstrap 均含 `test_summary_application`）。

## 21. 第四十一批：std/io、视图绑定与 str 方法（2026-11-26）

- 基线 `4afc1f6`。按第四十批扫描选择：`std/io`（107 个程序的首个阻塞）与 str 方法 `fields`/`trim`/`lines`（21 个）。
- `std/io`：`use std/io`（含 `use std/{io, os}`）；`io.readAll() catch _ { ... }` 产生拥有 String（`yv_io_read_all`，任何失败进入处理块，块必须离开）。`catch e` 绑定标准库错误、`io.readLine()`（返回 `?String`）、不带 catch 的调用为 U0001；实参个数 E0012、处理块可落空 E0601、未导入 E0010，同 stage0。
- str 方法：`trim`（str）、`lines`/`fields`（`Vec[str]`），C 用 `yv_str_trim/lines/fields`，来源规则与 `split` 相同；带实参 E0012。
- **视图绑定**（原 U0001 “persistent views of owned strings”）：`let lines = text.lines()`、`let t = s.trim()`、`let w = rows[0].trim()` 等指向拥有局部（或循环元素）的视图现在可以绑定，记为 `Local.viewOf`；读出时仍是带所有者的瞬时值（`Value.owner`），可以再绑定、移动、下标、`clone`、遍历，但不能赋给外层变量、push 进别的 Vec、放进 Result 或返回（指向局部 E0303）。视图在作用域内时，修改或移动所有者为 U0001（stage0 只在视图之后仍被使用时报 E0102，子集没有活跃性分析）。指向只读 String 参数的视图按普通来源处理，可以经 `of` 返回（以前是 U0001）；指向 inout 参数或临时值的视图仍为 U0001（stage0 对临时值报 E0305）。
- 遍历临时 Vec：`for x in make()`、`for w in text.fields()`、`for x in v.clone()` 等；临时 Vec 作为循环头的所有者活到循环结束后释放，遍历视图会锁住其所有者，遍历字段锁住整个结构体（E0102，同 stage0）。
- 扫描又发现一处越界并修复：内建 `max`/`min`/`abs`（及 `Box`/`Rc`/`Arc`/`Mutex` 构造）原报 E0010，现为 U0001。
- 覆盖：当时记录全仓 391 个 stage0 接受程序中 selfhost 接受 88 个（第四十批 70 个），0 越界；220 个 stage0 拒绝程序均未被接受。第四十二批对本批最终源码 `3ce5d36` 无缓存重建得到 91/391；88 未能复现，见第 22 节校正。
- 用例：新增 23 个（run 5、reject 11、unsupported 7），原样提升 11 个旧 unsupported 用例为 run（记录在 promotions）；约 290 个旧探针无回退（8 个由 U0001 变为接受，均与 stage0 一致）。
- 验收：本机 756/756、13/13、0 skip，stage2/stage3 C 953,599 字节、Linux 二进制 443,536 字节一致；本批未改 Rust（沿用：各 145/0/1，冻结 124/3/2）。CI：`0c270fa` 的 run 45653050661 中 Windows 在 setup 下载 Zig（约 70 KB/s）时撞上 tools/ci/run.py 单命令 20 分钟上限（第二次出现，未进入测试）；随后 `87623d6` 给原生作业加了按 `toolchain.lock` 键控的 Zig 工具链缓存（actions/cache v6.1.0），冷缓存时安装命令放宽到 45 分钟。`87623d6`（含本批全部代码）的 [run 45657470106](https://github.com/msly/yavo/actions/runs/45657470106) **6/6 成功**（Linux 含 stage2 ASan/UBSan、Windows、macOS；Windows 冷下载 22 分钟后成功并写入缓存）。

## 22. 第四十二批：带下标遍历与 Copy Optional if let（2026-11-26）

- 基线 `3ce5d36`。根据上批扫描的带下标遍历与 if-let 阻塞扩展现有表示，不在本批加入元组或拥有型 Optional。
- **带下标遍历**：语法为 `for index, item in values`（没有括号）。索引是只读 int；支持既有 Vec/只读切片、拥有 String/结构体元素、临时 Vec 和 `fields()` 临时视图；`_` 可丢弃任意绑定。循环仍锁住集合根，continue 仍递增，break/return/循环头 catch 的清理保持原规则。重复/遮蔽 E0011、越作用域 E0010、修改绑定 E0014、修改/移动集合 E0102、移动拥有元素 E0404。
- **旧边界修复**：`os.args()` 的内部 Args 类型现在按静态 []str 遍历，单绑定与双绑定均可用；以前会误报 E0001。双绑定 Map 遍历仍为 U0001。范围仍仅允许一个绑定。
- **if let**：支持现有 Optional 的 int/byte/bool/非拥有结构体/无载荷枚举载荷，含 else、else-if-let、丢弃 `_`；条件只求值一次，绑定仅在 then 可见。沿用分支 moved/source 合并与循环重放。`Local.matchRoot` 单独记录匹配借用，不把 Copy 载荷伪装成视图，允许复制/返回载荷。
- **保守边界**：then 内修改被匹配的源变量 U0001（stage0 按最后一次使用判断接受或 E0102）；`_` 不保留此锁。修改来自只读源的绑定 E0101；来自可写源或临时值的 Copy 绑定修改 U0001，同 stage0。拥有/str 载荷、Optional 字段、解构模式均未开放。
- **真实迁移**：`types.declaredIn` 用双绑定扫描类型表；`types.qualifiedType` 用 if-let 处理模块查找。两条路径由编译器自身、模块正反例和固定点共同覆盖。
- **用例**：新增 36（run 8、reject 20、unsupported 4、shared_unsupported 4）；原样提升 `unsupported_optional_if_let` 与 `unsupported_for_two_bindings`，promotions 记录基线与源码哈希。合计 792（run 213、panic 30、reject 426、unsupported 94、shared_unsupported 29）。包含循环头 catch + 拥有临时 Vec + indexed for + if-let 的组合清理回归。
- **独立核对**：35 个额外差分检查无越界，5 个运行组合在分配检查下与 stage0 输出/退出一致。最终 stage2 全仓扫描覆盖 92/391、0 越界；220 个 stage0 拒绝程序均未被放行。
- **覆盖基线校正**：从精确基线 `3ce5d36` 提取全部 selfhost 源码并禁用缓存重建，得到 **91/391**；有缓存和无缓存构建的基线二进制哈希一致，391/220 的语料成员及内容复查一致。故本批实际是 **91 → 92，新增 1、退步 0**，新增路径为 `experiments/coldstart-2026-11-20/yavo/T07.yv`（罗马数字与整数转换 CLI，原首阻塞为 if-let）。该原文件经 stage2 生成 C、构建并运行 21 个输入，双向转换、1/3999 边界与非法输入均符合固定期望及 stage0，分配检查无错误。旧记录 88 无法从该基线复现，原因未定位；不能用它宣称本批新增 4 个。最小复核记录见 [coverage 记录](compiler/validation/batch42-coverage-2026-11-26.json)。
- **本机验收**：792/792、13/13、0 skip；stage2/stage3 C 967,346 字节、Linux 二进制 448,416 字节一致；真实 lexer、test-summary 的 71 个本机场景、输入不变检查全部通过。最终 stage2 全仓复扫仍为 92/391、0 越界。CI：实现提交 `819ea1e` 的 [run 45763574200](https://github.com/msly/yavo/actions/runs/45763574200) **6/6 成功**；Linux x86-64（含 Yavo 前端生成 C 与 stage2 自身 ASan/UBSan）、Windows x86-64、macOS arm64 原生作业及三个独立作业全部通过。

## 23. 第四十三批：Copy 元组与原始单位换算 CLI（2026-11-26）

- 基线 `dcb562f`。以原样编译 `experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T04/final/{main,conversion}.yv` 为目标；没有修改历史程序或冻结规格用例。
- **Copy 元组**：平坦的 int/byte/bool/非拥有结构体/无载荷枚举元素；支持类型注解、构造、数字投影、参数/返回、局部值、整体赋值与 inout，以及 Optional 元组的 some/none、隐式包装、两类 ?? 和 if-let。逐元素从左到右求值并保存 Copy 快照，后续元素的 inout 副作用不改变前面的元素。
- **表示与布局**：TypeRef 的 tupleStart/tupleCount 指向共享元素类型池；解析后的 Type 继续为 Copy，以命名类型数加 4 为基数编码结构签名，逐步检查 int 溢出并报 U0001，无哈希碰撞。无命名类型时 31 个 int 元素可编码、32 个拒绝；容量随命名类型数变化。C tuple/Optional tuple 布局位于完整命名布局之后、函数声明之前；只出现在未调用函数签名中的元组也生成布局。
- **边界与诊断**：嵌套/拥有/借用/Optional 元素、元组字段、Vec/Slice/Result 元组、引用和解构仍 U0001；unit 元素表达式 E0001。推导单元素 `(x,)` 可用，显式单元素类型 E0020。十进制数字投影（包括 `.01`）可用，越界/未知字段 E0010；同形比较 E0501、异形 E0001；元素写入经不可变根 E0014，否则 E0001。Optional 不透传隐式包装的字面量上下文：`?(byte, byte) = (1, 2)` 为 E0001，`some((1, 2))` 可接受。
- **abs**：支持 int 与 byte，参数只求值一次，保留期望 byte 上下文；负 int 调用既有检查式取负，最小值报 P-OVERFLOW，byte 原样返回。用户同名函数优先，局部遮蔽保持原诊断；其他数字类型、min/max 仍 U0001。
- **Rust 修复**：元组直接输出原先到 codegen 才 panic；`compiler/src/check.rs` 现在与结构体/容器一样在检查期报 E0001。`compiler/tests/p0/errors_tuple_print.yv` 覆盖 8 处 print/println/eprintln、format/插值、assert 消息及拥有元组诊断。完整 Rust Debug/Release 各 145 通过、0 失败、1 个既有忽略，冻结规格仍为 124/3/2。
- **用例**：新增 55（abs 14、tuple 41），无原样提升；合计 847（run 222、panic 31、reject 452、unsupported 111、shared_unsupported 31）。新增支持、拒绝与保守边界均先定标 stage0；额外差分审查覆盖求值快照、上下文、仅签名类型和长签名边界。全仓扫描暴露的解构误报 E0020 已改为 U0001，未开放解构。
- **应用门**：新增 `selfhost/conversion_application.py` 与 `tests/applications/unit-conversion.json`，纳入 bootstrap 第 14 项 `unit_conversion_application`。固定两份原文件哈希，20 场景/330 条输入，双方 Debug/Release 共 80 次分配检查执行；固定 stdout 独立来自公开契约与精确分数，覆盖 16 种单位对、符号/约分/零、数值/精度/长度限制、非法行、空白/编号及 200 行输入。验收器、契约、场景和两份源码参与输入首尾哈希；优先用固定点 stage2。超时杀进程树并有界回收，临时普通文件承载输入输出，避免后代持有管道导致永久等待。
- **全仓覆盖**：最终 stage2 **92 → 98 / 391，新增 6、退步 0、越界 0**；新增均为 r5 的 yavo-a/yavo-b T04 各 final/work/snapshots/0001，入口源码只有两种。stage0 接受集合仍为原来 391 个；拒绝数增至 221，唯一新增是上述 P0 元组输出 fixture，全部未被 selfhost 放行。
- **本机与 CI**：最终完整自举 **847/847、14/14、0 skip**；stage2/stage3 C **1,010,808 字节**、Linux 二进制 **472,064 字节**逐字节一致；真实 lexer、test-summary 的 71 个本机场景、新单位换算的 20 场景/80 次执行、输入不变检查全部通过。首轮新增应用构建曾因宿主磁盘耗尽失败，清除可再生 Zig 缓存后完整复验通过。CI：实现提交 `7a2d08f` 的 [run 45771997599](https://github.com/msly/yavo/actions/runs/45771997599) **6/6 成功**（首次运行）；Linux x86-64、Windows x86-64、macOS arm64 原生自举和应用门均成功，Linux 生成 C/stage2 ASan/UBSan 与三个独立作业全部通过。

## 24. 第四十四批：整数 min/max 与原始文本 diff CLI（2026-11-26）

- 基线 `91bb20a`。目标为原样构建运行 `experiments/coldstart-2026-11-21-r4/yavo/T11.yv`，只补实际阻塞的整数 min/max；原文件和冻结规格均未改。
- **min/max**：恰好两个同型 int/byte 参数，保留双向数字上下文、用户同名函数优先和局部同名不可调用。用 emitContext 保持包装边界；`?byte = min(1, 2)` 为 E0001，some/ok 等显式构造才传入 byte 提示。参数个数 E0012、不兼容类型 E0001、越界字面量 E0003；浮点及其他整数宽度仍 U0001。
- **值顺序**：普通路径的标量读取立即物化，右侧 inout 修改不影响第一参数；两个参数均恰好一次。左侧是无类型纯数字树、右侧有类型时，先推导右类型，但把左 C 放回右 C 之前；左侧除零必须先于右侧 assert(false)。没有为数值参数增加借用锁。
- **Rust 修复**：旧 MinMax 代码生成保留本地/字段表达式到第二实参求值之后，`min(value, writeInt(&value, 100, 10, ...))` 在 value=5 时错误输出 10，正确为 5。`compiler/src/codegen.rs` 现在依次 bind 两个操作数；新增 `compiler/tests/p0/run_min_max_snapshot.yv` 覆盖 int/byte、本地/字段/inout参数/下标/嵌套副作用/单次求值与极值。新 P0 两模式通过，完整 Rust Debug/Release 各 145/0/1，冻结规格仍 124/3/2。
- **用例**：新增 28（run 5、panic 1、reject 21、unsupported 1），原样提升 builtin_max；合计 875（run 228、panic 32、reject 473、unsupported 111、shared_unsupported 31）。提升记录绑定基线 91bb20a 和原始源码哈希，双方 D/R 固定输出均为 2。各诊断/输出先按修复后的 stage0 定标；新增 12 组 D/R 运行差分全部通过。额外 RHS try 成功/失败与临时拥有 Vec 清理组合也经四次分配检查执行核对，无新增缺陷。
- **应用门**：`selfhost/diff_application.py` 与 `tests/applications/text-diff.json` 纳入 bootstrap 第 15 项 text_diff_application，复用已验证的进程树超时 runner。原始 T11 SHA256 为 `6caddd13162d57caa859c58323c4993e27809e5c3100bf23b3139d9880f43ec8`，35 场景/140 次执行通过，退出码 0/1/2 各 8/19/8 个场景。期望独立遵循公开 LCS 平局先删除规则并核对公开 fixture；27 contract、3 line API、3 CLI、2 file API 边界区分记录。10 项输入绑定原始源码、验收器、共享 runner、契约、固定场景和公开输入输出。
- **分配范围**：全部构建启用分配插桩，32 次正常返回执行分配检查；108 次差异/错误按原程序 os.exit(1/2) 退出，不执行正常析构及正常返回分配校验。后者只声明输出、错误和退出码正确，不混作资源检查通过。
- **覆盖口径**：沿用 subset_sweep.py 的全工作区 rglob（包含已有未跟踪实验快照）。原有 612 源文件内容和 391 个 stage0 接受成员均未变；原有覆盖 98→99，仅新增原始 T11、减少 0。新增 P0 单列后，全仓 100/392，stage0 拒绝 221，越界 0。
- **本机验收**：完整自举 **875/875、15/15、0 skip**；stage2/stage3 C **1,018,462 字节**、Linux 二进制 **476,112 字节**逐字节一致。真实 lexer、test-summary 的 71 个本机场景、单位换算 20 场景/80 次执行、文本 diff 35 场景/140 次执行和输入不变检查全部通过。最终 stage2 全仓扫描 **100/392、0 越界**；再逐一复核原有 98 路径及新增 T11/P0，100/100 接受，确认接受集合一致、退步 0。首轮仅旧 builtin_max 的 unsupported 分类过时，源码原样提升后完整复验通过。
- **CI**：实现提交 `1c8207d` 的 [run 45783874254](https://github.com/msly/yavo/actions/runs/45783874254) **6/6 成功**（首次运行）。Linux x86-64、Windows x86-64、macOS arm64 原生 Rust Debug/Release、自举及应用门全部通过；Linux 生成 C/stage2 ASan/UBSan 与三个独立作业成功。已核对 run/head 精确绑定 `1c8207dcd77a193b7ccf3ec3e42abedb089c0681`，分支 main，六个 job 均 completed/success。

## 25. 第四十五批：整数 Vec 排序与原始窗口中位数 CLI（2026-11-26）

- 基线 `1a7fc4a`。目标为原样编译运行 `experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T11/final/main.yv`，SHA256 `e4d211582198c47570790de1791bba37e3907d6524736e1b3ee73c97e6206828`；历史应用、冻结规格和 Rust 源码均未改。
- **sort**：仅开放无参 Vec[int].sort()，稳定原地升序、返回 Unit。复用 runtime 的 yv_sort 与关系比较器 yv_cmp_i64，比较不做整数减法；空/单元素直接返回，其他情况的辅助缓冲区在调用内释放。支持可写变量、inout 参数、嵌套结构体字段及字段括号，不复制或移动 Vec 描述符。
- **接收者与诊断**：名称路径先检查借用再检查可变性；有效字段路径先检查可变性再检查借用，坏字段在接收者预检查中先报 E0010。只读根 E0014、已移动 E0402、循环/先前借用 E0102、预留根 E0403；参数个数 E0012、无 Ord 的 bool/Named 元素 E0501。`let _ = values.sort()` 可用；把无值结果作普通绑定或 return 值仍 E0001。裸变量外加括号、临时、未知名称和非 Vec 接收者按 stage0 保持 U0001；拥有元素的下标 place、byte/str/String 排序、sortBy 仍在子集外。
- **用例**：新增 22（run 3、reject 12、unsupported 3、shared_unsupported 4），无旧用例提升；合计 897（run 231、panic 32、reject 485、unsupported 114、shared_unsupported 35），263 个可执行输入。22 条 stage0/candidate 定标全部匹配，三个运行程序的双方 D/R 共 12 次分配检查执行通过。覆盖整数极值、重复排序、字段/inout、排序后 push/pop/整体移动、return/break/continue 清理及错误优先级。独立组合探针还验证视图作用域结束后排序和资源释放。
- **应用门**：新增 median_application.py 与 tests/applications/sliding-median.json，纳入 bootstrap 第 16 项 sliding_median_application。原 judge 14 个场景（含两个公开样例）逐字节保留，补 14 个契约内边界，共 28 场景、1,405 个输入值、1,112 个窗口、双方 D/R 112 次正常返回分配检查执行。覆盖 K=25、长度 0/24/25/200、K=24 偶数下中位数、K=1、重复值离窗、混合空白、正号/前导零/负零及窗口和 ±25,000,000,000。期望先用秩计数独立确定，再由另一套窗口排序计算复核，0 不符。
- **输入范围与绑定**：仅公开有效域：非空 ASCII/LF，独立 K 首行 1..25，后续 0..200 个整数，每项 ±10^9。空序列不等于空文件；不把无效整数、CR 或空文件行为纳入契约。8 项输入绑定原源码、harness、共享 runner、场景 JSON、两份契约和 judge/public cases，首尾哈希一致；优先用最终 stage2 执行。
- **全仓覆盖**：沿用 subset_sweep.py 的全工作区 rglob。613 个源文件、392 个 stage0 接受成员均未变；最终 stage2 覆盖 **100→106/392、新增 6、退步 0、越界 0**。新增是 yavo-a/yavo-b T11 各 final/work/snapshots/0001，只有两种入口源码；221 个 stage0 拒绝输入全部未放行。
- **Rust 验收**：本批无 Rust 改动，仍重新完成 Debug/Release 全量，各 145 通过、0 失败、1 个既有忽略；冻结规格 124 pass、3 unsupported、2 superseded。stage0 固定副本 SHA256 为 `3db1eb23f6abf2478081f2e805e0491fac00d71800ef699f796a76710cf2fddd`。
- **本机验收**：完整自举 **897/897、16/16、0 skip**；stage2/stage3 C **1,027,977 字节**、Linux 二进制 **479,152 字节**逐字节一致。真实 lexer、四项应用门和输入不变检查全部通过；最终 stage2 对全 613 个路径复核，与候选的 106 个接受成员完全一致；旧 100 个接受路径全部保留。
- **CI**：实现提交 `67407bb` 的 [run 45794313520](https://github.com/msly/yavo/actions/runs/45794313520) **6/6 成功**（首次运行）。Linux x86-64、Windows x86-64、macOS arm64 原生 Rust Debug/Release、自举及应用门全部通过；Linux 生成 C/stage2 ASan/UBSan 与三个独立作业成功。已核对 run/head 精确绑定 `67407bbcf25cf7b737ec372961d2348b722a3b9b`，分支 main，六个 job 均 completed/success。

## 26. 第四十六批：Map 遍历、String 排序与原始原子账本 CLI（2026-11-26）

- 基线 `9d55af8`。目标为原样编译运行 `experiments/coldstart-2026-11-22-r5/runs/yavo-b/tasks/T01/final/main.yv`，SHA256 `8a8403e63b51d427f57da3c052f9ea50c5e9c4f8a039123d707ab36976a0d4d9`；历史应用、冻结规格及Rust源码未改。
- **Map遍历**：`for key, value in map`支持Map[String,V]，V为int/bool/byte；必须两绑定，可用 `_`。按插入顺序扫描entries并跳过删除空槽，更新已有键不换位，重插键到尾部。键为只读借用String，不独立析构或移动；值为只读Copy局部。支持具名Map、普通字段、只读/inout参数及完整临时Map；循环期间其根不可修改、移动或inout访问（E0102），可查询、clone及嵌套只读遍历。
- **键与生命周期诊断**：键赋值/push/take为E0103；显式String绑定、入Vec或拥有返回为E0404；inout String(&key)首E0404，inout int/str首E0001，值赋值E0014。`let _ = key`只读取，推断引用别名仍U0001；`key.trim()`可在循环内绑定，返回、赋外层或放入另一Vec仍U0001。独立审查发现 `for k,v in makeHolder().values` 原先误放行，已在头部按stage0报E0305；直接makeMap()等合法控制项保留。
- **String排序**：沿用上一批接收者和借用规则，将Vec[String]接入稳定的yv_sort/yv_cmp_String。按UTF-8字节比较，支持空串、重复、NUL、组合/预组合Unicode和emoji。只排列String描述符，不clone或析构其分配；最终由Vec逐元素释放。byte/str排序、sortBy和拥有索引place仍U0001。
- **用例**：新增28（run5、reject18、unsupported5），indexed_map与sort45_string两条旧源码逐字节提升为run；合计925（run238、panic32、reject503、unsupported117、shared_unsupported35），270个可执行输入。60次双方分类定标和7个运行程序双方D/R的28次分配检查执行全部通过；覆盖Map三种Copy值、顺序/删除/重插/扩容、借用键、临时拥有者、return/break/continue及String排序资源边界。
- **应用门**：新增ledger_application.py及tests/applications/atomic-ledger.json，纳入bootstrap第17项atomic_ledger_application，优先用最终stage2。原judge14组（含2公开）逐字节保留，补18组合同内边界；32场景、783命令、517条batch支付行，双方D/R共128次正常返回分配检查执行全部通过。独立差额日志oracle与第二套clone账本模型逐条输出一致。
- **合同与绑定**：有效ASCII/LF，账户名1..16字节、输入金额0..10^9、最多200命令、batch0..100，支付行不计命令数；空文件合法。覆盖失败批次首/中/末回滚且消费剩余行、来源/目的/资金检查顺序、自转账、零金额、前导零、ASCII排序、Map扩容、99,000,000,000单户余额和200,000,000,000总额。8项源/契约/场景/验收器/共享runner输入及编译器首尾哈希一致。
- **全仓覆盖**：全工作区rglob的613源路径及552,219字节与冻结基线完全一致；本批stage0重新定标为392接受/221拒绝。最终stage2覆盖106→112，新增6路径来自yavo-a/yavo-b T01各final/work/snapshots/0001、只有两种入口hash；旧106路径全部保留，221拒绝输入无放行，越界/超时0。最终接受集合SHA256 `06f02862e4844c751cf7cc2d1d8cb1f8208ae13afd402ef0d36fd8c968fadf7d`。
- **距目标的计数口径**：剩余280路径首拒全部U0001；入口源码hash去重后，stage0接受222份、selfhost接受52份、剩余170份。前十首拒桶涉及181路径，仍可能存在后续阻塞。该计数不衡量等权任务、完整依赖图或项目完成率；SH-3的build/run/诊断入口与SH-4剩余两个正式项目另计。
- **Rust验收**：本批无Rust改动，Debug/Release全量各19组、145通过/0失败/1既有忽略；冻结124pass/3unsupported/2superseded。本批固定stage0 SHA256 `0aa26645f51df9f0ddb1429f9bbf7b32145eca0a513193deffb7d81f0e262992`，不同于上批构建副本，因此基线分类已重跑。
- **本机验收**：完整自举925/925、17/17、0 skip；stage2/stage3 C **1,039,982字节**、Linux二进制 **484,496字节**逐字节一致。C SHA256 `661b1a564a2af55946b44fc3717419a99cd3cb35050edc291eaa300496ae8b2d`，二进制SHA256 `cebfe5fe704adbe56ca7c44b6462a19503050d569109af96dec6a48bd08d8aa6`。真实lexer、五项应用门及输入不变检查通过；回执在compiler/target/ci/selfhost-batch46/receipt.json，work为run-_nhr3raf。
- **CI**：实现提交 `bc8aefa` 的 [run 45826605112](https://github.com/msly/yavo/actions/runs/45826605112) **6/6成功**（首次运行）。Linux x86-64、Windows x86-64、macOS arm64的Rust Debug/Release、自举及应用门全部通过；Linux selfhost生成C/stage2 ASan/UBSan与三个独立作业成功。已分别核对run及jobs：head为 `bc8aefa53a469de4cdc888652efbf8bae4e559b4`、main、push、attempt1，六job均completed/success；Windows最后作业于2026-11-26北京时间21:52:15完成。

## 27. 第四十七批：Set[String] 与独立 CSV 校验 CLI（2026-11-27）

- **基线与目标**：基线 `84cd8ef`。第二个正式SH-4项目为 `examples/csv-check`，main/parser/validator/report四模块共271行；[SPEC](examples/csv-check/SPEC.md)先冻结，应用作者未读取独立oracle输出。完整项目首次stage0 check成功，仅W0003未使用导入；删除该导入后冻结源码，Debug/Release均0错误、0警告，无行为修复。八次完整编译调用及源码/工具身份见[开发记录](examples/csv-check/evidence/DEVELOPMENT.md)，未把局部片段探针算成项目交付。
- **真实阻塞**：第46批stage2对冻结原文只在 `validator.yv:15:20` 的Set构造报U0001；补Set后候选stage1/stage2均可check/emit-c，无需改写CSV程序或引入拥有Optional。
- **Set边界**：仅拥有型 `Set[String]`；局部支持 `[]`、`[:]`、`Set[String]{}`，字段默认值只支持 `[]`。`add`消费String并返回Unit，重复键释放新传入所有者；`has/remove`接受str或String查询且不消费查询值，remove删除时释放存储键并返回bool；支持len和独立拥有键的clone。局部、参数、返回、结构体字段及Result成功/错误载荷均接入正常与提前退出析构。C内部复用Map[String,bool] helper，语言类型检查仍禁止Set/Map互换。
- **审查修复与限制**：保留写接收者的两阶段预留及借用检查；只读查询参数反过来修改/移动其接收者仍U0001。独立审查修正Set遍历误报E0001、非空Set初始化的分类及字段默认 `[:]` 被误接受三处边界；Set遍历/take、其他键类型、直接Vec[Set[String]]及拥有Optional仍U0001。新增34例（run4、reject21、unsupported6、shared_unsupported3），无旧例提升；总计959（242/32/524/123/38）。
- **CSV合同**：完整UTF-8解码后逐记录解析；支持引号、双引号转义、逗号、LF/CRLF及引号内跨行字段。表头按解码后内容判非空/唯一且不trim，数据列数须一致；同行词法优先、早记录结构错误先于后记录词法错误。行列按UTF-8字节，结构错误定位记录起始行第1列；拒绝裸CR、开头BOM、除TAB外的ASCII控制符与DEL。stdin/文件可组合，首个失败仅写一条稳定stderr且stdout原子；退出码成功0、CSV错误1、用法2、读取/UTF-8错误3。
- **独立预期**：139条手工合同预期与独立字节解析oracle在执行应用前一致；POSIX适用138，Windows适用137，其余明确not_applicable。冻结记录为 `examples/csv-check/tests/oracle-freeze.json`；cases SHA256 `a0abf46feaf6185864573641cfc4df6f64776f7b734907dec2f2d4862929f257`，oracle SHA256 `eff38afb971cd0064430cc0623c3e453696b57788700cecc868e2ead7edd82f9`，SPEC SHA256 `4a87a12f941122c8dea2244c87ba0124ad72311f20fe1369ba8814237da61a89`。
- **应用门矩阵**：bootstrap第18项 `csv_check_application`优先最终stage2，双方Debug/Release及双方Release exit-shim共六产物，POSIX828次、Windows822次；最终本机828/828。独立native门运行Debug、Release、exit-shim三种产物，POSIX414次、Windows411次，另建独立空缓存Release副本比较C/二进制。清洁/泄漏退出控制先校准，成功和错误路径均检查输出、退出与清理。Linux CI须先通过独立GCC clean/UAF/overflow控制，再对stage0 C及从最终bootstrap stage2重新生成的C各跑138例，共276次sanitizer执行；Linux原生CI已通过此项；Linux/macOS六产物门各完成828/828、Windows822/822，native三产物门对应414/414和411/411。
- **新runner本机native**：`compiler/target/ci/csv-check-batch47-rss/attempt-xyp1f13q/receipt.json`为passed，414/414、3次native构建、6次计时构建，Release两副本C/二进制逐字节相同。应用C46,702字节，SHA256 `6d32d6dd002adda1377bd3eba57d22a49f3cca98dbe2a0b10d77123b18c56a27`；Release二进制30,456字节，SHA256 `bd53883ea38843a63561a22bae375298462df64b5743117bdc9087aefdcbb996`。Debug冷/不变/注释编辑为22.284126/0.170244/0.486040秒，Release为4.942010/0.020308/0.825170秒，均miss/hit/miss；各条件只有一次观察，非P50/P95。锁定工具链预安装、空缓存和含空格/非ASCII路径均纳入验收，源/工件首尾身份一致。
- **RSS与超时**：新runner全部431条命令均有wait4 RSS，最大417,312,768字节；414次应用执行最大26,951,680字节。Linux将ru_maxrss KiB转bytes，macOS为bytes，Windows不可测时留null及原因；这是OS对waited child的报告，非同时进程树RSS总量。正常exit0、非零exit7和带子/孙进程超时控制均通过，超时1.005秒返回−9且三PID均已终止；手动reap同步Popen.returncode。finally路径清缓存通过注入构建异常控制，新native五棵缓存均清除。控制回执在 `compiler/target/batch47/rss-controls-7x7tqsxm/receipt.json`；本机无GCC，sanitizer为unavailable/null，未记为通过。
- **覆盖口径**：原613文件、392个stage0接受成员和全部原字节不变；最终stage2仍112/392，去重52/222。加CSV四模块后617文件、393个stage0接受，最终stage2为113/393，去重53/223；退步/越界/超时0。最终扫描在 `compiler/target/batch47/final-stage2-coverage.json`；该增量为一个完整CSV入口，覆盖计数不换算SH里程碑完成率。
- **Rust与首轮自举**：无Rust/runtime改动；Debug/Release各145/0/1，冻结124/3/2。首轮完整自举959/959、18/18、0 skip，回执 `compiler/target/ci/selfhost-batch47/run-wc7teq0m/receipt.json`。stage2/3 C1,057,613字节，SHA256 `694a454c2e1ba59f4b19fd2bd1e4a641472433e74e53f461baf8b576b4416ded`；Linux二进制490,144字节，SHA256 `7dfc1373b1e0ab8ae510c61565abdc93e813748d8869905124a89e255675bcbe`，两代逐字节一致。
- **最终自举**：新runner的 `compiler/target/ci/selfhost-batch47-final/receipt.json` 已通过959/959、18/18、0 skip，六项应用门及输入身份检查全部通过，stage2/3 C与二进制逐字节一致。较长输出目录进入源诊断路径，故本轮C为1,057,805字节、SHA256 `6b88386c7a52c839e6c937345250a7125e187d61686cdab9f0c766dcd5ab8107`，二进制490,192字节、SHA256 `9d3e68bce7b96e59ddd25281a1787d2b006b3aeae8e99c66955b66344d5a5cbb`；首轮身份仅保留为该次运行历史。
- **验收修复与CI**：首轮之后发现可观测RSS恒为null及失败时冷缓存未清理两个验收器缺口，已修复并重跑native。RSS脚本冻结后的完整959例、18项门及最终stage2覆盖复扫均已通过；[验证索引](compiler/validation/batch47-csv-set-2026-11-27.json)保存回执、输入与产物身份。实现提交 `0373fb8a36579a4db8d2fbaedfc853e3bb242492` 的 [run 45937402633](https://github.com/msly/yavo/actions/runs/45937402633) **6/6成功**，首次运行。已核对main/push/exact head、三平台bootstrap/CSV/归档/上传及Linux双份C sanitizer和selfhost sanitizer步骤；CI归档内容未另行下载，平台结论依据执行脚本的必需门与API成功状态。SH-4正式交付更新为2/3；SH-3仍check/emit-c。下一批为第三个独立多文件依赖/索引CLI。



## 28. 第四十八批：拥有型图结构与独立依赖索引 CLI（2026-11-27）

- **基线与应用**：基线 `5e527961eba9fbf4547251b42a8b1f21daff5b05`；五模块309行，源项目SHA256 `a48f8f8dc9879fa5101efb67d72232a91f61c23a575edaa983fa4f6992d84f64`。SPEC先冻结，应用作者未读selfhost或独立预期；两次check、两次build，仅修else同行语法。应用原文未为selfhost改写；见[开发记录](examples/dep-index/evidence/DEVELOPMENT.md)。
- **合同与独立预期**：显式UTF-8声明文件，重复模块/缺失引用/循环按固定阶段报错；ASCII最小就绪节点的依赖优先顺序，成功后原子TSV。113手工预期与独立oracle在执行应用前一致；POSIX112、Windows107适用。不是Yavo源码解析器，无目录发现、增量持久化或构建命令执行。
- **selfhost扩展**：Vec[Vec[int]]及深clone/索引写入，String平坦tuple与Optional，Heap[(String,int)]及无捕获纯表达式比较器，临时Optional while-let，[]拥有struct的只读索引/foreach别名链，已支持类型的sink参数。正常/提前返回、循环退出及实参求值中断清理；一般拥有Optional、其他Heap/嵌套Vec形状、捕获闭包、局部Vec持久别名、命名io/fs.Error等保持U0001。
- **回归与审查**：新增68（27run、2panic、25reject、14unsupported），原字节提升2，合计1027=271/34/549/135/38。修正Heap比较错误接受、过宽String→str转换导致的三条旧E0001误接受、Optional来源与别名边界、readonly-slice foreach别名及标准Error晚边界。两轮中止的初验不记最终通过；最终清单分类1027/1027及最终全量均已复验。
- **最终本机**：完整自举 **1027/1027、19/19、0 skip**，七项应用门、真实lexer及输入身份通过。C **1,171,654字节**、SHA256 `4bab3b7a9711045e910a99300b6710435132e1b769164670b2cc72707617163f`；Linux二进制 **545,632字节**、SHA256 `1d39ff5a88157ce680676510c215720e9d4f19be7acf800d1f1480b57244987c`，stage2/3逐字节一致。最终回执 `compiler/target/ci/selfhost-batch48-complete/receipt.json`。
- **应用与资源**：最终stage2/0六产物672/672，独立native336/336。stage0独立Release C62,982字节，二进制39,328字节，两副本字节相同；Release冷/不变/编辑5.109620/0.008643/1.022913秒，均单次miss/hit/miss。353/353命令有wait4 RSS，最大425,816,064字节；本机无GCC，sanitizer为unavailable/null。CI要求Linux对两个实际producer的C执行ASan+UBSan，正负控必须先通过。
- **覆盖与Rust**：原613文件和392个stage0接受成员字节不变，112→117/392，去重52→54/222；上批617文件113→118/393；全622文件119/394、去重56/224。历史新增五路径为两种sink源，dep-index新增一个完整入口；退步/越界/超时0。无Rust/runtime变更，完整Debug/Release各145/0/1，冻结124/3/2。
- **CI与目标**：实现提交 `cc78059` 的 [run 45955439200](https://github.com/msly/yavo/actions/runs/45955439200) **6/6成功**，三平台原生门及Linux双前端C sanitizer均通过。 **SH-4正式独立应用完成3/3**：test-summary、csv-check与dep-index均完成声明范围内的三平台验收。 SH-3仍check/emit-c；公开系统接口、锁定后端普通build/run、结构化诊断和更广语义仍是主工程。性能与成本目标另验收。完整来源见[验证索引](compiler/validation/batch48-dep-index-2026-11-27.json)。

## 29. 第四十九批：公开系统接口与普通 build/run（2026-11-27）

- **范围**：基线 `e4c0302`。新增os.env/host，fs.tempDir/removeFile/removeDir/sameFile，process.run；stage0与C runtime同步实现。selfhost以catch-_接入，增加只读[]String；标准命名Error/一般Result仍U0001。进程为显式程序+argv，无shell/PATH搜索；正常退出与负信号码分开，driver把信号转为128+信号。
- **普通驱动**：原生build/input/-o/--release及run/--argv；内嵌生产header和锁版本，bootstrap检查生成物新鲜度。完整前端成功后才创建唯一目录，源码图身份保护覆盖hardlink/symlink，已知临时文件删除后验证空目录；清理失败报告位置并返回4，不递归删除未知文件。
- **独立合同**：55主案例在读取/执行新driver前冻结；另两项相对-temp/@temp独立补充，原55不变。POSIX每driver57，Windows54；正式stage1+最终stage2对应114/108。实际stage2普通build自身、产物emit-c固定点以及产物普通run独立多模块程序单列检查。
- **修正与回归**：36新增+1旧原字节提升，1063=279/34/571/141/38。审查修复std/user模块别名冲突漏拒E0011、索引String隐式返回str误接受、POSIX pipe/fcntl/fork并发描述符继承窗口及fd0/1/2重用、NULL空路径内存函数UB、临时根选项/响应文件解释。最初bootstrap在268例处停止，排除最终验收。
- **最终本机**：完整bootstrap1063/1063、20/20、0skip；七应用门和输入身份全部通过。stage2/3 C1,616,267字节、SHA256 `9219295c09b89fb96cd47db73b59eee142e053936466abb1817713b6f68148ce`；二进制680,160字节、SHA256 `163025a368ebe8f038eafe01b8a9e0a012327c595a68b07af7b0eefa6068aee0`，两代一致。
- **本机普通driver与资源**：114/114、9/9；普通重建二进制也与上述stage2一致。133/133命令有wait4 RSS，最大373,264,384字节，口径为单个等待子进程而非进程树并发总和。普通重建编译器12.932秒是单次观测；P50、token、金额未测。
- **系统运行时**：6新增Rust集成测试在生成的Debug/Release程序中验证，包含8路temp创建、成功/127/失败区分、Unicode argv、stdio、并发快慢子进程。六目标真实C对象编译通过。独立GCC14.2的ASan+UBSan正负控与直接probe通过；probe102次API调用=67正常退出+1信号+33启动失败+1非法参数。CI门另要求7个固定程序×2实际前端+1Cprobe，共15插桩入口，controls另计。
- **Rust/覆盖/目标**：完整Debug/Release各151/0/1、冻结124/3/2；622源字节和stage0分类不变，119/394、56/224，退步/越界/超时0。扫描器首轮误用git文件清单漏掉30份ignored历史副本，已恢复原有全文件口径；不计为语言退步。SH-4维持3/3；本批关闭native普通入口，SH-3整体仍未完成。
- **CI**：修正后的提交 `e03cfc0` 的 [run 45975880319](https://github.com/msly/yavo/actions/runs/45975880319) **6/6成功**；三平台原生普通驱动及Linux系统ASan+UBSan门通过。 首次run 45971497025的准备门因API索引触碰R6冻结输入而失败；提交修正后旧原生作业被并发策略取消。修正原字节恢复该输入，并让41个冻结Markdown输入的变更触发CI；完整准备门重新通过。后续run 45972361245的Windows测试helper输出CRLF、macOS覆盖输出保留0644两处失败已按原合同修正；修正提交e03cfc0只改driver和测试helper，runtime及冻结用例未改。 完整来源见[验证索引](compiler/validation/batch49-build-run-2026-11-27.json)。
- **macOS普通重建二进制**：完整文件哈希不同；全部1,262个不同字节位于代码签名区，前632,752字节、全部机器码/数据/装载命令/UUID相同，39个代码页哈希独立重算一致。输出名 `yavo-selfhost` / `rebuilt-selfhost` 进入CodeDirectory标识，导致签名字段长度和偏移变化。生成C固定点与运行验收通过；保留原始完整文件差异，详见[字节比较记录](compiler/validation/batch49-macos-binary-comparison-2026-11-27.json)。

## 30. 第五十批：结构化诊断、UTF-8 范围与不可变源码身份（2026-11-27，历史记录）

- **实现**：新增内部 `digest.yv`，纯 int 位运算 SHA256，编译器成为十个 Yavo 模块；`support` 统一 JSON 渲染，`lexer` 在 JSON 模式的文件 EOF 持有原始源码与摘要，既有 parser/types/codegen 的 fail 调用保留原位置和首错优先级。普通文本模式不保存这些快照。`check` 支持两种 JSON 拼写和前后位置；成功 exit0、首错 exit1；参数错 exit2，根读取/UTF-8 失败 exit3 且 stdout 为空。
- **冻结合同**：主合同、98 个固定案例与独立 oracle 在首次执行新 JSON 前冻结；旧 batch49 stage2 仅提供 80 个文本起点/消息观察。位置/范围按源字节独立定义，哈希来自 hashlib。两份 POSIX 特殊文件名的仓库存储名在执行前改为 Windows 可 checkout 名，逻辑路径、源字节和预期不变，amendment 保留旧身份。另 8 个 legacy CLI 案例在发现回归后、修复前单独冻结，旧 stage2 8/8 通过。
- **兼容修正**：初候选通过 98 JSON/CLI+80 text 后，审查发现普通 `check -legacy.yv` 和 `emit-c ... -o --json` 被误解释为选项；首次完整自举在 294 项处停止并排除验收。恢复原位置语法后，修正候选 98+80+8 及 8 项检查全过，再从新目录完整自举。原主合同及预期未放宽。
- **正式本机**：1063/1063、20/20、0 skip，七应用门和输入身份通过。C 1,684,975 字节，SHA256 `3e453870eb13aae13d310333cf3995f3a4f35e106e15f670febd95ff92f4f546`；Linux 二进制 682,864 字节，SHA256 `7553ed87ca0082ade3834a388eeeac956459d45553e8cf56568c400d7ad39f67`；stage2/3 逐字节一致。最终 stage1/stage2 JSON/CLI 196、文本 160、补充 16，372/372 命令；10 个 oracle 负控全部拒绝，FIFO 对根/已加载模块的修改证明使用此前快照。46 SHA256 向量经 stage0/stage1/stage2 Debug/Release 共 276 次通过，包括块边界、Unicode、NUL 与一百万个 a。
- **普通驱动与范围**：最终双 producer 114/114、9/9，本机实际 stage2 普通 build 自身的 C/二进制等于固定点，再普通 run 独立多模块程序。原 1063 清单和 Rust/runtime/lock 的 163 个来源文件字节不变；本机 Rust 完整结果引用同源 batch49 的 151/0/1，不冒充本批新运行。原 622 语料仍 119/394、去重 56/224，零退步/越界/超时；原 613 仍 117/392。
- **CI**：实现提交 `6e1d22f` 的 [run 45991816609](https://github.com/msly/yavo/actions/runs/45991816609) **6/6 成功**；三平台 SHA256、JSON 诊断和普通驱动门全部通过，Linux 实际 stage2 C 的 ASan+UBSan 诊断门通过。三份原生 ZIP 已下载并核对 GitHub 哈希，原始回执、源码和产物身份均已复核。Linux 编译器插桩门通过 98 JSON/CLI+80 text+8 supplement、10 负控与 4 次 FIFO 屏障；已有 program/compiler/self-compile/stress 门全部通过。Linux/macOS 双 producer 为 196+160+16，Windows 为 188+152+16，四个 POSIX 专属案例明确排除。当前 CI Rust Debug/Release 各为 Linux 完整 151/0/1、macOS 原生选定集 90/0/1、Windows 原生选定集 87/0/1。详见[验证索引](compiler/validation/batch50-json-diagnostics-2026-11-27.json)。
- **macOS 普通重建差异**：原始 stage2 与普通 build 自身的二进制各 634,224 字节，1,264 个差异字节全部位于 1,376 字节的代码签名区；Mach-O 头、加载命令、代码/数据节和重算的代码页哈希一致。签名标识分别含输出名 `yavo-selfhost`、`rebuilt-selfhost`，连带改变标识长度、哈希表偏移和签名 blob 长度。两份原始文件保留不同哈希，详见[独立原始字节比较](compiler/validation/batch50-macos-binary-comparison-2026-11-27.json)。
- **下一批**：标准库错误值与 catch e 优先；独立定义命名错误、错误显示/匹配/传播和拥有资源清理，再以未改原文暴露后续阻塞。SH-3 默认接替、正式性能和成本仍未完成。

## 31. 第五十一批：标准错误值、完整 Result 与真实 I/O（2026-11-27）

- **实现范围**：`fs.Error`、`io.Error`、`os.Error`、`process.Error`为独立名义Copy枚举，载荷保留真实int；prelude `Utf8Error`为Copy struct。支持上下文`.Utf8(offset: n)`/`.Io(code: n)`与零载荷构造、参数/返回/普通struct字段、标准错误显示及标准Error-only match。match只求值一次，沿已有分支状态合并与临时/局部清理处理拥有结果和return/break/continue。
- **完整Result**：fs.readText/writeNewText/tempDir/removeFile/removeDir/sameFile、io.readAll、os.env、process.run、String.fromUtf8可保存、返回、try或命名catch。catch必须终止；错误绑定不可变、不越域、不遮蔽既有局部。String.fromUtf8成功或失败均只消费一次Vec[byte]；不同Error不能隐式传播转换。
- **边界**：std导入不引入短名Error；用户同名类型继续普通解析。直接Error/Utf8Error Vec、Optional与tuple、一般用户payload enum及match、Result/Optional模式、arrow catch、io.readLine、fs.writeText均未开放。`fs.Error.Utf8(...)`一类qualified payload构造维持旧stage0的U0001；Error比较E0501，Utf8Error直接显示E0001。
- **独立合同与修订**：73新例按30run、29reject、10unsupported、4shared_unsupported预先判定；10原例仅提升分类，源字节不变。73个新增语义用例中，一项最初使用 `['a', 0xE2, 0x82]`，触及既有非十进制词法 U0001 边界。按显式唯一 amendment 新增等值 `['a', 226, 130]` 源文件，并只替换该 manifest 项的 file；原稿、原 freeze、原 manifest 快照和首次候选失败全部保留。旧 stage0 对原/新源 Debug/Release 的4次执行均输出 `1\n`，预期未改；正式 stage0 接受原稿，stage1/2仍报 U0001。 语义/IO freeze及amendment合计绑定101输入，当前manifest1136；[合同](selfhost/tests/errors51/CONTRACT.md)和[修订](selfhost/tests/errors51/semantic-amendment.json)分别保留。
- **真实IO门**：48固定场景独立oracle覆盖Unicode/NUL、UTF-8偏移、IO失败、路径/参数错误、独占写不覆盖、临时目录/删除/身份、环境缺失与空值、子进程非零正常退出与启动失败；native errno只检非零，合成payload另验精确显示。三个实际前端各D/R、每次独立输入目录，验精确stdout/stderr/exit及前后文件树；Linux/macOS288、Windows276（目录stdin和无效native UTF-8环境两例不适用）。6oracle控制、4checked-exit清理控制、3原hex边界另计；Linux原stage0/最终stage2 C的GCC ASan+UBSan96场景及3能力控制是独立门。
- **环境隔离**：bootstrap只对run/panic接受`environment`（仅name/PATH，str或null）及`isolated_cwd`；执行时逐producer/模式新建cwd，编译环境不变。program sanitizer bundle保留同一配置，ASAN/UBSAN/YAVO_ALLOC_CHECK不可覆盖；5个真实子进程与27项非法参数负控已验证。
- **正式本机**：1136/1136、20/20、0skip；七应用兼容门通过。C **1,774,302字节**、SHA256 `25d8e8a287e3997ca157bc1362df371f10b1abe96c406b36452f206cef95f7f7`；Linux二进制 **729,968字节**、SHA256 `179cb25e6a07503e085cb58cfd703ea2839bd7dcce0bb94fcaca8495d01dcbfb`；stage2/3分别逐字节相同。标准错误288/288、9检查通过，本机sanitizers off；JSON/text/supplement196/160/16、SHA256276、普通driver114/9和普通重建闭环通过。
- **原始语料与Rust**：622源字节和stage0分类不变，119→125/394、56→62/224，0退步/越界/超时。6个原入口经12场景、48次stage0/最终stage2 D/R运行通过。清单1136=319run+34panic+600reject+141unsupported+42shared_unsupported；原1063源码不变。本批未重新运行本机 Rust 全量；同源 batch49 的 Debug/Release 各151/0/1保留为历史证据。
- **CI与归档**：本机正式门已通过；本提交的三平台 CI 和 Linux 独立 ASan+UBSan 标准错误门仍待确认。本机该 sanitizer 门为 off，未计为通过。 正式审计复核实际编译器/原始C/二进制身份、101冻结输入、原始命令与字节流、前后文件树、环境与sanitizer控制；ZIP身份和TAR安全索引独立核验。[验证索引](compiler/validation/batch51-standard-errors-2026-11-27.json)。
- **下一批与目标**：下一批优先 Copy 元组 Vec：当前首阻塞共25路径/11份入口字节，其中Copy为22/8，另3/3是包含String的拥有型元组。先冻结空/非空构造、push/pop、字段及参数/返回、索引读写/遍历与边界诊断，再用原文暴露后续限制。Map值首阻塞27/14混合Counter、元组、String、Vec和Set，不统一归入Copy。首阻塞计数不等于预期覆盖增量。 SH-4仍3/3，本测试driver不另计正式应用；SH-3默认接替、正式性能与成本仍未完成。
