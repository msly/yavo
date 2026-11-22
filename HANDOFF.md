# Yavo 交接文档（2026-11-22，第十六批：GitHub 发布与 CI 接入）

给下一个会话：先读第 1、2、10、21 节再开工。第 20 节是第十四、十五批（四项语言决定、下标赋值、第四轮复测，以及之后的元素修改方法和诊断）；第 19 节是第三轮实验；第 18 节是第十二批（构建缓存）。第六到第十一批的原始记录与“完整 v1 目标”清单在 `history/handoff-batches-2026-11-19-to-21.md`，保留当时的日期与数值，不代表当前源码的重新验证。

## 1. 一句话状态

规范以 v0.5 冻结版为基础，2026-11-21 按第三轮实验修订了 V2（两阶段借用）、K1（`String`/`str` 比较），新增 V6a（字段默认值）、V14（P-STACK），见 `docs/decisions.md` 第六节；冻结测试包 `yavo-tests/` 不改。Rust 编译器与 C 运行时覆盖 P0 链路及 P1—P3 的主要功能：结构体、枚举、`T ! E`、`match`、`String`/`Vec`/`Map`/`Set`、切片、移动与析构、视图与 `of`、`&T`/`&inout T`、`?T`、方法、`Drop`、元组、不逃逸闭包、`Eq`/`Ord`、`Box`、接口、泛型、浮点；并发（`Rc`/`Arc`/`Mutex`/`with`/`scope`/`spawn`/`Channel`/`Send`/`Sync`）；Heap/Deque、`Arena[T]`/`Id[T]`、源文件模块；`yavo fix`/`allocations`/`verify`；后端构建缓存；六个目标的交叉编译。第十五批补入 V2a（元素上的修改方法），见 `docs/decisions.md` 第七节。**P0—P3 主体功能已实现，完整 v1 尚未验收，P4 仍为小规模试验。** 三平台 CI 已落地；Windows/macOS 为明确选定的原生套件。完整 v1 的工具/语言缺口、Agent 成本和正式性能门尚未闭合。各批次细节见第 4 节和 `history/`。
规范一致性：**129 项中通过 124、判错 0、不支持 3、未运行 0、被新决定取代 2**（T-V1、T-GF2 断言旧 V2，登记在 `compiler/tests/conformance_superseded.txt`；不支持的 3 项需要锁定工具链无法链接的 ASan 运行时）。该统计只表示既有用例覆盖，不是完整 v1 完成比例。
**2026-11-22 用户已授权推送 `msly/yavo` 并使用 GitHub CI；已有历史和 CI 已推送，`8a65228` 的四项 CI 任务全部通过。** CI 接入结果见第 21 节；旧“暂不推送”约定已被本次授权取代。
本地测试基准 `b3c27a6`：第十五批完整单条命令 Release **129 通过 / 0 失败 / 1 忽略**、Debug **129/0/1**；这是 Cargo 测试函数数，和上面的 129 项规范用例是不同计数单位。忽略的是旧 P0 探索性报告，当前 conformance 套件会执行。第十四批 `1af1f69` 为 123/0/1，新增 6 项为 `index_assignment.rs`；六个交叉目标能构建含 P-STACK 运行时的程序，最新原生 CI 覆盖 Linux x86-64 全套与 Windows x86-64/macOS arm64 选定套件。
**最新 agent 实验（第四轮，第 20.2 节）**：沿用第三轮题包，首编正确 Yavo 8/11（第三轮 3/11）、Rust 11/11；Yavo 与 Rust 的 token 比 1.27、工具调用比 1.58、用时比 1.63。剩下 3 次失败（元素修改方法、非 Copy 元素读出即借用、`${` 未转义）已在第十五批分别实现或改进诊断。交卷后 T10 边界抽查发现 Yavo 解答错拒合法前导零 `000000001`，Rust 接受；18/18 仅为既有样例通过。成本由编排记录补回，完整逐次转录和首稿尚未归档。最新改动尚未经新题评测；CI 已通过，下一步做第五轮（第 10 节）。第三轮数据见第 19 节。

## 2. 仓库与环境

- 仓库：`/data1/workspace/github/workspace/yavo`（通过 mcp-3 访问，工作区根是 `/data1/workspace/github`）；远端 [msly/yavo](https://github.com/msly/yavo)，发布与 CI 已获用户授权。
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
- 标准库导入：已实现成员有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`math.sqrt/pow/floor/ceil/round/pi/e`，其余报 `U0001`；每个源文件须自行导入。`os.exit` 不运行析构。
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
| 冷启动实验 | 四轮 | 第四轮同题首编且既有样例正确 8/11 对 Rust 11/11；token/调用/时间为 1.27/1.58/1.63 倍；T10 新边界 Yavo 失败（第 20 节） |
| 已落地 | 三平台 CI | `8a65228` 同一提交四项任务全通过：Linux 全量、Windows/macOS 选定原生套件、独立 ASan+UBSan（第 21 节） |

批次与用户约定的历史（“可用”三批、第四到第六批）见 `history/handoff-batches-2026-11-19-to-21.md`。

## 10. 下一个会话的第一步

1. 先检查 Git 状态和最新提交；环境见第 2 节。冻结用例基线见第 1 节。代码修改按风险加回归，明确记录 Debug/Release 实际完成范围。**不要同时跑两条 `cargo test`**：它们共用临时目录，会出现 `CurrentDirUnlinked` 之类的假失败。GitHub 原生 CI 已通过，后续提交沿用 [CI 说明](tools/ci/README.md) 的平台、工具链和范围；cargo 退出 0 之外还须核对 conformance 的 wrong/not_run。
2. **下一步：第五轮新题实验**：第三、四轮复用同一题包，已无法区分“语言变好”和“题目被针对”。按第四轮报告第 92–93 行的要求：
   - 换 12 道同难度新题，每题加独立的隐藏边界用例（第四轮 T10 前导零就是旧题包漏掉的合法输入）；
   - 每种语言至少 2 次独立运行；
   - 编排会话逐次保存编译命令、退出状态、首次源码，以及 token、工具调用和单调时钟计时，**直接写进实验目录**（第四轮 Yavo 的成本就是因为没落盘，写稿时才“缺失”）。
   - 协议与复核脚本沿用 `experiments/coldstart-2026-11-21/`。
3. 成本差距是现在的主要问题：第四轮首编 8/11，但 token 仍是 Rust 的 1.27 倍、工具调用 1.58 倍、用时 1.63 倍。第五轮要能拆出原因，所以记录里要区分“读速查卡/文档”“写代码”“编译失败后修改”三类调用。若主要花在读卡上，考虑给速查卡加一页“首次写对清单”并压缩其余篇幅。
4. 可选的语言决定（目前没有新失败支持，等第五轮数据）：非 Copy 值的取法（如 `m.getClone(k)`）；`[]T` 可变切片。
5. 预编译运行时目标文件：编辑后重建 Release 仍 214–247 ms，超过 200 ms 目标；约 50–60 ms 用于重编 `yavo_rt.h`（第 18 节）。
6. 待排的完整 v1 缺口：用户测试块与 `yavo test`；依赖与程序锁文件；FFI/unsafe 与 audit；固定工具链的 ASan 能力。本轮不扩展语言功能。
7. 一般闭包值/高阶参数、公开 Join、spawn inout、callable 类型、Atomic API 等；完整清单见 history 文件中的“完整 v1 目标”。

正式门槛仍按 `docs/plan.md`：配对通过率差的单侧 95% 下界至少 −5 个百分点，单位成功交付成本较 Rust 低至少 20% 并独立复现；冻结基准的运行耗时几何均值不超过 C 的 1.5 倍。工程目标包括小程序编译 P50 ≤200 ms、Linux strip 后 ≤250 KB、Windows 真机测试及可复现产物。12 道新题用于下一轮诊断；正式题量按 60 题 pilot 估计、起点不少于 400 题的既定设计。

## 11–17. 历史批次（已移出）

第六批至第十一批的设计说明、实现记录与当时的验证矩阵原文在 `history/handoff-batches-2026-11-19-to-21.md`。要点：

- 第六批（11 节）：P3 并发设计并已实现：`Rc/Arc/Mutex/Guard/with`、`scope/spawn/join`、`Send/Sync`、`Channel`。
- 第七批（12 节）：线程借用来源、资源克隆、I/O 锁、测试时限等安全收敛；独立 ASan/UBSan、TSan 历史证据。
- 第八批（13 节）：`yavo fix` 保守自动修复与候选补丁。
- 第九批（14 节）：`yavo allocations`、绑定工件哈希的 `yavo verify` 报告、Sanitizer 能力探针。
- 第十批（15 节）：Heap/Deque、保存的拥有闭包、可变引用返回、显式丢弃与元组析构修复。
- 第十一批（17 节）：`Arena[T]`/`Id[T]`、源文件模块与项目级验证快照、Guard 越权漏洞修复。
- 第 16 节“完整 v1 目标与未闭合验收”仍有效，原文也在该文件中；剩余工作的优先级以第 10 节为准。

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

## 19. 第十三批：速查卡对齐与第三轮冷启动实验（2026-11-21）

### 19.1 速查卡对齐（开跑前，`642e8be`、`7cc377c`）

逐项实测速查卡与编译器：删除“编译器还不存在”的过时说明、6 个不存在的命令（`test/fmt/outline/doc/explain/audit`）、`test` 块、`std/time`、`std/rand`；补多文件模块写法、Arena 方法与“不能遍历”、Deque、字符串转义 `\$`、`ch.close()` 与多线程共用 channel；新增第 9 节“尚未实现”。

### 19.2 实验与结果

完整报告 `experiments/coldstart-2026-11-21/README.md`。12 道不点名特性的新题（图删除、树移动、表达式树、哈夫曼树、两道多文件、两道并发、修复题等），Sonnet 受试者，Yavo 只给速查卡，Rust 对照；Python 参考实现生成期望输出；`verify.sh` 独立复核含结构要求。

| 指标 | Yavo | Rust |
| --- | --- | --- |
| 独立复核 | 18/18 用例、4/4 结构要求、堆块检查干净 | 18/18、4/4 |
| 第一次编译即正确（11 题，不含修复题） | 3/11 | 11/11 |
| 有效代码编译失败 | 8 | 0 |
| token / 工具调用 / 用时 | 165k / 52 / 308 s | 125k / 22 / 167 s |

Yavo 8 次失败：实现缺口 2（字段默认值、`v[i] = x`）；与直觉相反的设计 4（不做两阶段借用 ×2、`String == str`、非 Copy 用 `getCopy`）；文档写了但没看到 2（`"\${"`、已有变量 `if let` 只借用）。图、树、并发、多文件题没有所有权类失败。修复题 Yavo 一次报全 7 条错误，rustc 分阶段只先报 E0106，这是 Yavo 唯一明显占优之处。受试者从未用 `yavo fix`/`check`。

### 19.3 实验后修改（本批提交）

速查卡：字段默认值与下标赋值列入第 9 节并给替代写法；规则 7 改为“实参里任何读接收者的表达式都会被拒绝”；第 3 节区分对临时值与已有变量的 `if let`；`Map` 行补非 Copy 值取法。编译器未改。HANDOFF 压缩：第 4 节完整提交表和第 11–17 节移到 `history/handoff-batches-2026-11-19-to-21.md`，本文件从 72 KB 降到约 37 KB。

### 19.4 结论

“agent 写起来比 Rust 高效”目前**不成立**：最终正确率持平，但一次写对率和成本都落后，且差距没有随功能完善而缩小。好消息是失败几乎全部集中在少数可消除的点上，而不是所有权模型本身；所以下一步应先消除这些点（第 10 节第 2、3 项），再用新题重测，而不是继续扩功能。

## 20. 第十四、十五批：语言决定、下标赋值与第四轮复测（2026-11-21—22）

### 20.1 第十四批（`1af1f69`）：按第三轮实验的四项语言决定

用户同意按开发者建议执行（`docs/decisions.md` 第六节，第 12–16 项）：

- **两阶段接收者（V2 修订）**：修改接收者的调用和 `v[i] = x` 中，接收者在实参求值期间只是预留；实参可以读，不能修改、移动，值也不能在调用开始后仍指着它（E0403）。实现：`FnCx.call_reserved`、`with_reservation`、`reserved_conflict`。T-V1、T-GF2 登记为 superseded。
- **`String`/`str` 比较（K1）**：比较时把 `String` 一侧包成 `StrView`。
- **字段默认值（V6a）**：只允许常量，见 `FieldDefault`。
- **P-STACK（V14）**：备用信号栈（mmap），保护区内的故障报为 `P-STACK at file:0:0` 并刷新 stdout，退出码 101；主线程用 RLIMIT_STACK 定界，Windows 用 VEH。不运行用户代码的 `Box` 内容走 `yv_box_release` 工作列表释放，20 万层的树、30 万长的链表都能正常释放。
- **下标赋值（C2、V5）**：先求新值，再求下标、做越界检查，再析构旧元素并写入；降级为 `{ let $value; let $slot = &place; *$slot = $value }`。

### 20.2 第四轮复测（`experiments/coldstart-2026-11-21-r4/`）

同一题包，`1af1f69` 编译器。首编正确（11 题，不含修复题）：Yavo 8/11（第三轮 3/11），Rust 11/11；18 个运行用例两组都通过。成本：Yavo 162,464 token、38 次工具调用、276.8 s；Rust 127,801、24、170 s（比值 1.27、1.58、1.63）。剩下 3 次失败：T03 非 Copy 元素读出即借用（E0102）、T04 `${` 未转义、T05 `dependents[pi].push(i)` 报 U0001。交卷后另一位 agent 审计发现 T10 的 Yavo 解答拒绝前导零输入，这是解答的 bug，不是编译器的问题。报告初稿里 Yavo 成本写成“缺失”，2026-11-22 按编排会话的记录补上。

### 20.3 第十五批（2026-11-22）

- **另一位 agent 补的下标赋值求值顺序**（`check.rs`，配套 `compiler/tests/index_assignment.rs`，6 项）：
  - 新值先求，它持有的借用记在 `FnCx.assignment_shared` / `assignment_mut` 里；目标地址求值时若修改或移动这些借用源，报 E0102。
  - 目标的上下文类型由 `assignment_target_ty` 在不产生副作用的情况下提前确定，泛型调用只实例化一次（抽出了 `prepare_generic_fn`）。
  - 无法提前确定类型的复杂目标报 U0001，而不是重排副作用。
  - 本批复核：Release、Debug 全绿。
- **决定 17：元素上的修改方法**：`v[i].push(x)`、`ms[i].set(k, v)`、`ps[i].tags.push(s)`、`ps[i].moveBy(1)`、`g[r][c].push(x)`。`method_call` 里的 `indexed_receiver` 加 `mutating_method`（只看类型：内建修改方法表，以及 `inout self` 的用户方法）。先 `mutable_address` 取得元素的 `&inout`，绑定到 `$receiver`，再走已有的方法路径。实参使用容器报 E0403，与 Rust 一致；只读方法不变。测试：`run_index_methods.yv`、`errors_index_methods.yv`。语义表新增 V2a。
- **决定 18、19（只改诊断和文档）**：
  - E0102 在借用者是 `&T`（非 Copy 元素读出）时提示 `.clone()`。
  - 词法 E0020 未闭合的 `${` 提示写 `\${`。
  - 速查卡规则 7、第 1 节转义说明、第 7 节新增两行。
- 第四轮 README / results.json 补 Yavo 成本，并注明 T03–T05 的后续处理。

### 20.4 结论

首编差距大幅缩小（3/11 → 8/11，第四轮的三处障碍本批已处理），但成本差距仍在，同题复测也不能说明泛化。下一步是用新题、多次独立运行、完整计时的第五轮（第 10 节第 2、3 项），而不是继续扩功能。

## 21. 第十六批：GitHub 发布与 CI 接入（2026-11-22）

**[第三次 CI 44991523612](https://github.com/msly/yavo/actions/runs/44991523612) 的四项任务全部通过，统一验证提交为 `8a65228f31139a4b07e256fec70ccd6aaff06176`。** GitHub 仓库为 [msly/yavo](https://github.com/msly/yavo)。语言功能基准仍是 `b3c27a6`：解析、类型/借用检查、代码生成、运行时与冻结测试未改；本批修改了 Windows `verify` 构建驱动、CI、平台测试、sanitizer 导出与文档。

### 21.1 当前验证结果

| 平台/范围 | Debug | Release | 证据 |
| --- | --- | --- | --- |
| Linux x86-64 完整 Cargo 套件 | **129/0/1** | **129/0/1** | [工件](https://github.com/msly/yavo/actions/runs/44991523612/artifacts/14742107791)；含六目标构建与冻结规范 |
| Windows x86-64 选定原生套件 | **71/0/1** | **71/0/1** | [工件](https://github.com/msly/yavo/actions/runs/44991523612/artifacts/14741324499)；空格/Unicode 模块快照与重放通过 |
| macOS arm64 选定原生套件 | **74/0/1** | **74/0/1** | [工件](https://github.com/msly/yavo/actions/runs/44991523612/artifacts/14741947108) |
| 独立 Linux ASan+UBSan | **60 通过 / 0 失败** | — | [工件](https://github.com/msly/yavo/actions/runs/44991523612/artifacts/14742196791)；GCC 13.3，全部导出的 P0 run/panic fixtures |

原生套件数字为通过/失败/忽略。Windows 比 macOS 少 3 项是 `cfg` 排除了 2 个 Unix 进程单元测试和 1 个 Unix symlink 模块测试；各平台均无运行时过滤。Linux 的 Debug/Release 规范报告均为 **124 pass / 0 wrong / 3 unsupported / 0 not_run / 2 superseded**；P0 在 C `-O0` 与 `-O2` 各执行 101 个 fixtures。

Windows/macOS 执行 [CI 说明](tools/ci/README.md) 列出的 `NATIVE_TESTS` 与编译器单元测试，其范围小于 Linux 全量。独立 GCC 的 clean、故意 UAF、带符号溢出能力探针均通过；该配置启用 Yavo 堆块检查、关闭 LeakSanitizer，不改变固定 Zig 的 3 个 ASan unsupported 项。

项目服务器此前也分别完整复跑 Release、Debug，均 **129/0/1**、各 17 组；首个合并驱动在 Debug 遭执行时限终止后，已独立完整重跑。各次 CI 的版本、范围、计数、工件与关键哈希已固化到 [验证收据](compiler/validation/github-ci-2026-11-22.json)。

### 21.2 首跑、复跑与修复

[首次运行 44989988675](https://github.com/msly/yavo/actions/runs/44989988675) 绑定 `47b5880`，Linux 全量通过，其他三项失败。[第二次运行 44990668032](https://github.com/msly/yavo/actions/runs/44990668032) 绑定 `8b49e61`，Linux、macOS、独立 sanitizer 通过，Windows 在模块验证构建中失败。第三次同一提交的四项任务全部通过后，才记录本批 CI 接入完成。

| 提交 | 修复与验证范围 |
| --- | --- |
| `c8f06ff` | 修正 clean checkout 下 Perl 读取空 Git 状态时的空值上下文，恢复 sanitizer bundle；实际导出 60 个程序并核对哈希 |
| `26378ba` | Darwin 故障注入 C fixture 的 feature macro 移到系统头之前；Markdown/验证 JSON 单独变化不重复触发 CI |
| `8b49e61` | Windows panic 路径期望兼容分隔符；全新 C 工具链顺序预热 Debug/Release，每 fixture 120 秒时限不变 |
| `8a65228` | Windows `verify` 使用经边界校验的工件目录相对路径，避免 Zig 无法识别扩展格式绝对路径；记录 cwd，现有模块快照回归加入空格/Unicode；本地 verify 15/0、模块回归 1/0，随后三平台 CI 通过 |

历史 `0a69e75` 的 58 例 sanitizer（含当时额外的 Arena/模块程序）、四项 C pilot 与可复现结果保持原版本/日期；它们与本次 60 个 P0 fixtures 的范围不同。本批未重测运行性能或 Agent 编程成本，下一步是第 10 节的新题第五轮。
