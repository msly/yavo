# Yavo 交接文档（2026-11-25，第三十八批：str 查询方法）

给下一个会话：先读第 1、2、10、17、18 节及 docs/self-hosting.md、selfhost/README.md 再开工。第二十七至第三十批在 `history/handoff-batches-2026-11-23-to-24.md`，第十二至第二十六批在 `history/handoff-batches-2026-11-21-to-23.md`，第六至第十一批在 `history/handoff-batches-2026-11-19-to-21.md`。

## 1. 一句话状态

**SH-2 引导子集固定点保持；第三十八批让 selfhost 支持 str 查询方法 `startsWith`/`endsWith`/`contains`/`find`/`split`/`parseInt`，并修复 Rust stage0 文本方法接收者在实参求值期间未被锁住的释放后使用缺陷（见第 18 节）；第三十七批支持只读 `[]T` 参数与 `Vec[T]` → `[]T` 自动视图（第 17 节）；第三十六批支持 `?? return`/`?? break`/`?? continue`（第 16 节）；第三十五批让 selfhost 支持 `Vec[str]`，并修复 Rust stage0 对拥有型实参（`Vec[String]` 等）返回视图的释放后使用缺陷（见第 15 节）；第三十四批加入借用返回 `-> str of …` 并修复 loans 在分支与循环中丢失的缺陷（第 14 节）。** 第三十三批起 selfhost 支持多文件源模块： `use a`、`use a/b`、`use a/{b, c}` 以入口文件目录为根加载，允许循环导入与自导入；顶层 `pub` 决定导出；导入逐文件生效、不传递；函数一律写 `module.f(...)`，公开类型可用短名或 `module.T`（含 `module.T{...}`、`module.E.V`），短名二义报 E0010；诊断和运行时 panic 都指向原文件行列。第三十二批的插值与 format（含 Rust 输出片段借用修复）见第 12 节。

当前语料 **675 cases**。第三十八批本机完整自举 **675/675、12/12、0 skip**，固定点数字见第 18 节；Rust Debug/Release 各 145/0/1（p0 用例文件计在同一测试内）、冻结 124/3/2。

编译器自身也已改为以 `selfhost/main.yv` 为入口的 `use` 多文件（决定 28），bootstrap 不再拼接源码；改后本机 568/568、12/12，stage2/stage3 C（785,468 字节）与二进制（366,936 字节）一致。

**SH-3 尚未接替，Rust stage0 仍承担日常编译；SH-4 完成 1/3 个独立应用。** 验收流程按 decisions.md 第九节。下一步见第 10 节：test-summary 的借用返回、`Vec[str]`、`?? return`、`[]str` 与 str 查询方法已解除，首错仍是 101 行的 `Map[String, int]`，绕开后是 `main.yv:31` 遍历 `Vec[String]`。

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

1. 检查 Git 状态、最新提交与 CI；先读第 12 节、[自举路线](docs/self-hosting.md)、[selfhost 说明](selfhost/README.md)。
2. **下一步：继续让 selfhost 编译 examples/test-summary（SH-3 的实际判据）。** 第三十四至三十八批解除了借用返回、`Vec[str]`、`?? return`、`[]str` 与 str 查询方法。首错仍是解析期的 `parser.yv:101:19: U0001: type suffix is outside the selfhost subset`（`var cases: Map[String, int] = [:]`）。把 Map 换成 `Vec[String]` 绕开后，下一个阻塞是 `main.yv:31:5` 的 `for path in paths`（遍历 `Vec[String]`，循环变量是元素的视图），之后可见 `catch problem { }`、`toString`、`FileRow{path: path.clone(), ...}` 等。建议下一批二选一：Map 一批（类型、`[:]` 字面量、`has`/`set`/`get`、String 键的所有权与 drop），或先做遍历拥有元素的 `Vec`（循环变量为借用视图、循环体内锁住 Vec）。每项先用 stage0 定标拒绝码，并迁移编译器自身里对应的变通写法。
3. 模块尚缺：包依赖与锁文件（stage0 也没有）、`use` 出现在其他项之后（stage0 接受，子集 U0001）、跨模块的枚举载荷/方法/泛型（子集本身没有）。

4. 拥有型 Optional/pop、if let/match、跳转式 ??、Result 字段/容器、方法、泛型按实际阻塞另列；SH-3 的判据是能否编译 examples/test-summary 这样的真实程序，而不是特性数量。
5. SH-4 继续用 stage0，后续选结构化文本校验和多文件依赖/索引工具。不要同时运行共用临时目录的两条 cargo test。
6. 第六轮文档实验有可计量环境才恢复；当前工程推进不受它阻塞。

正式门槛仍按 `docs/plan.md`：配对通过率差的单侧 95% 下界至少 −5 个百分点，单位成功交付成本较 Rust 低至少 20% 并独立复现；冻结运行基准几何均值不超过 C 的 1.5 倍。工程目标包括小程序编译 P50 ≤200 ms、Linux strip 后 ≤250 KB、Windows 原生测试及可复现产物。

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
- 验收：本机 675/675、12/12、0 skip，stage2/stage3 C 864,824 字节、Linux 二进制 406,680 字节一致；Rust Debug/Release 各 145/0/1，冻结 124/3/2。CI：`72cafa1` 的 [run 45586217122](https://github.com/msly/yavo/actions/runs/45586217122) **5/6**：Linux x86-64（含 stage2 ASan/UBSan）、macOS arm64 与三个独立作业成功；Windows x86-64 在 setup 步骤下载 95 MB 工具链时速度约 60 KB/s，约 20 分钟后被终止（环境问题，未进入测试）。服务器上的令牌没有 actions 写权限（rerun 返回 403），需在 GitHub 页面手动重跑，或由下一个改代码的提交覆盖；下一会话先确认 Windows 结果。
