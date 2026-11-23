# Yavo 交接文档（2026-11-23，第二十八批：真实 CLI 与 stage2 内存检查）

给下一个会话：先读第 1、2、10、12 节及 docs/self-hosting.md、selfhost/README.md 再开工。第十二至第二十六批的原始记录在 `history/handoff-batches-2026-11-21-to-23.md`，第六至第十一批在 `history/handoff-batches-2026-11-19-to-21.md`。

## 1. 一句话状态

**SH-2 引导子集自举固定点已完成，下一阶段是 SH-3 编译器接替与 SH-4 真实 CLI 交付。** `4739ffc` 的 [CI run 45150424099](https://github.com/msly/yavo/actions/runs/45150424099) 三平台原生固定点通过，6/6 jobs 成功；[独立原生索引](compiler/validation/selfhost-fixed-point-native-2026-11-23.json) 保存 run、job/step 与工件元数据，原 `67794d9` 本机回执不改写。

**第二十八批新增首个真实应用 `examples/test-summary/` 和 stage2 编译器自身的 GCC ASan/UBSan 门。** CLI 用现有 Rust stage0，读取仓库真实测试日志，核验逐用例结果与末行汇总，输出稳定 TSV。四模块首次 check、Debug、Release 均通过，1 个源码版本、3 次作者编译调用、0 编译修复；独立应用门已在 Linux 通过 71 个适用场景的 Debug/Release 142 次执行、退出清理辅助产物 71 次执行（合计 213/213），以及两次独立 Release 的 C/二进制一致性。真实 String clean/leak 探针分别保持 exit 2 和检测为 exit 102；本批三平台 CI 结果在完成后补充。

本批本机固定点复验 **335/335 cases、11/11 checks、0 skip**。独立 Linux/GCC 14.2 的 stage2 检查已通过：自编译、335 checks、126 emits、8 个固定坏输入；原有 126 程序 sanitizer 与 3 项能力探针也通过。完整 adapter 及本批应用的三平台原生执行尚待本批推送后的 CI 确认。

Rust stage0 仍是日常编译器，编译器/运行时/冻结规格未因本批改变；已有 Rust Debug/Release 各 145/0/1，冻结 129 项为 124 pass、3 unsupported、2 superseded。第六轮文档实验仍 0 会话、0 交付，真实 usage 资格未取得。SH-4 需要三个独立应用，首个应用不能替代整阶段验收。

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
| 已落地 | 自举固定点 | `67794d9`：引导子集编译自身，stage2.c = stage3.c，335/335 例，stage2 与 stage1 全用例一致（第 11 节） |
| 冷启动实验 | 五轮 | 第五轮两语言各 24/24 首编、公开、隐藏及结构通过；受控操作/时间比 1.0455/1.2051，token 缺失；4 会话、12 题（第 22 节） |
| 已落地 | 三平台 CI | `073003d` 的 run 45002067581 六项全通过：Linux 全量、Windows/macOS 选定原生套件、独立 ASan+UBSan，以及两项实验检查（第 23 节） |
| 已落地 | 第五项 CI | `0bad3d2` 的 run 44996118634 通过，核验 1,344 条判定、96 个源码阶段、16 项结构审查和冻结题包 |
| 已落地 | 第六项 CI | `073003d` 通过 33 项执行器、25 项 lab/audit/freeze、11 项裁判/汇总场景，以及 94 个准备文件和 112 个独立 oracle 输入；真实启动仍 blocked |
| 准备完成，资格 blocked | 第六轮文档对照 | 短卡/索引、逐请求计量执行器、8 题与 112 个输入已准备；0 受试会话，须先获得真实 usage 资格 |

批次与用户约定的历史（“可用”三批、第四到第六批）见 `history/handoff-batches-2026-11-19-to-21.md`。

## 10. 下一个会话的第一步

1. 检查 Git 状态、最新提交与 CI；先读本批第 12 节，再读 [自举路线](docs/self-hosting.md)、[selfhost 说明](selfhost/README.md)及首个[应用契约](examples/test-summary/SPEC.md)。已有固定点和首个 CLI 的工作无需从头重做。
2. **下一个编译器功能是 Result 与 `try`。** 先在外部正负例中与 stage0 接受范围、错误码对齐，再让编译器自身替换首错即退出的内部处理；覆盖临时对象和部分初始化 Vec 元素的提前退出清理，然后重新达到固定点。
3. 之后按 Optional/`pop` → `for` → 插值/`format` → 源模块顺序扩大引导子集。每项让编译器源码实际使用；现有 stage0 支持某能力不等于 selfhost 已移植。
4. SH-4 应用线继续用 stage0，后续选择结构化文本校验和多文件依赖/索引工具；用真实阻塞决定最小 `yavo test`、文件/进程 API、依赖锁定与分发的顺序。第一款应用完整入口是 `python3 tools/ci/test_summary.py`，本机 Python 路径见第 2 节。
5. 自举改动保持 stage2.c = stage3.c、stage2 与 stage1 全用例一致；Linux 的 `tools/ci/selfhost_sanitize.py` 同时要求生成程序门和编译器自身门通过。保持冻结规格、Debug/Release 和三平台原生回归；不要同时运行共用临时目录的两条 cargo test。
6. 第六轮文档实验有可计量环境才恢复，随后执行 60 题 pilot 及正式成本研究；当前工程推进不受它阻塞。

正式门槛仍按 `docs/plan.md`：配对通过率差的单侧 95% 下界至少 −5 个百分点，单位成功交付成本较 Rust 低至少 20% 并独立复现；冻结运行基准几何均值不超过 C 的 1.5 倍。工程目标包括小程序编译 P50 ≤200 ms、Linux strip 后 ≤250 KB、Windows 原生测试及可复现产物。

## 11. 第二十七批：引导子集与自举固定点（2026-11-23）

- 实现提交 `67794d9`。改写 `selfhost/` 全部编译器源码（support/lexer/model/parser/types/codegen/main，3,600 行）；删除 `diagnostic.yv`，旧 `model.yv`、`diagnostic.yv` 冻结到 `selfhost/tests/prefixes/` 供三个历史 prefix 工作负载使用。`lexer_test.yv` 改为检查 `Lexed` 返回值，由 stage0 构建。
- bootstrap.py：`COMPILER_PARTS` 拼接 → stage0 构建 stage1 → stage1 check/emit 自身得 stage2.c → Zig `-O2 -DYV_ALLOC_CHECK` 构建 stage2 → stage2 emit 得 stage3.c → 比较 C 与 stage3/stage2 二进制；每个用例另跑 stage2 check（及可执行例的 emit）并要求与 stage1 完全一致。原 `self_compilation_not_yet_supported` 检查删除，receipt 改为 `achieved: self-hosted-fixed-point`、`self_hosted: true`。
- 新增 14 例：run `inout_params`、`text_slices`、`take_places`、`std_catch_args`、`text_slice_owned_print`（由 `unsupported_text_slice` 原样晋升）；panic `text_slice_boundary`、`text_slice_bounds`；reject `inout_missing_borrow`(E0001)、`inout_later_argument_use`(E0102)、`inout_immutable_local`(E0014)、`inout_move_out`(E0401)、`catch_falls_through`(E0601)；unsupported `unsupported_string_view_binding`、`unsupported_catch_binding`；shared_unsupported `unsupported_inout_index`。错误码全部先用 stage0 实测。
- 行为差异只有一处：原前端在循环重检（replayBody）里为对齐 stage0 吞掉非 E0402 错误，首错即退出后不再吞掉。全部旧用例未出现差异；若以后遇到 stage0 接受而子集在循环重检中报错的程序，先查这里。
- 性能：stage1 编译自身约 0.03 s，生成 C 574 KB；Zig 构建约 5 s。完整门约 2 分 40 秒（4 核服务器）。
- CI：`67794d9` 的 [run 45147299836](https://github.com/msly/yavo/actions/runs/45147299836) 中 Linux x86-64（含 Rust 全量与独立 ASan/UBSan 作业）、Windows x86-64 及其余三项通过；macOS arm64 的 selfhost receipt 显示固定点成立（stage2.c = stage3.c，574,333 字节；335/335），唯一失败是 `stage3_binary_reproduces_stage2`：Mach-O 的 ad-hoc 签名写入输出文件名，stage2/stage3 用了不同文件名。修正为两代都输出为各自目录下的 `yavo-selfhost`（`4739ffc`），其 [run 45150424099](https://github.com/msly/yavo/actions/runs/45150424099) **6/6 jobs 成功**：Linux x86-64、Windows x86-64、macOS arm64 原生 selfhost 步骤全部通过，固定点在三平台原生确认。下载 macOS 归档用的是本机 git 凭据调用 artifacts API。
- 未覆盖：stage2 本身未跑 GCC ASan/UBSan（sanitizer 作业仍覆盖 stage1 生成的程序 C）。

## 12. 第二十八批：真实 CLI 与 stage2 内存检查（2026-11-23）

- 首个应用 `examples/test-summary/` 包含 main/parser/stats/report 四模块；SPEC 明确日志语法、重复项、汇总不一致、错误码、路径转义与多文件输出原子性。真实输入来自第四轮 Yavo/Rust 验证日志。无需更改编译器或运行时。
- 作者首次完整源码检查、Debug 和 Release 构建全部通过；完整三次调用、源码快照、诊断和工具身份在 `examples/test-summary/evidence/`，后续验收或修复追加记录，不替换首次结果。真实 usage 不可用，token/成本保持未知。
- 独立验收由 `examples/test-summary/tests/` 的固定期望与独立 Python oracle 组成，冻结后再执行应用。72 场景中每平台 71 适用，另外 1 项是目标系统不支持的原生文件名形式；三次独立空缓存构建，Debug/Release 逐例输出比较，Release C/二进制身份比较。此处计数是单应用场景，不是 71 个独立应用。
- 应用最终 Linux 门为 213/213（Debug/Release 142，加退出检查辅助产物 71）；原始 C/runtime 不改写，通过独立 forced-include shim 在显式退出前检查堆块。clean/leak 两个真实 String 探针先验证辅助检查有效。runtime、shim、probe、生成 C 均从全新目录的哈希绑定副本编译；三次原生构建的源副本、C、binary 及辅助产物运行后全部复验，避免同名头遮蔽或工件在执行中变化。见 `compiler/validation/test-summary-2026-11-23.json`。
- `tools/ci/selfhost_sanitize.py` 保留原 126 个生成程序门；新增 `selfhost_compiler_sanitize.py` 只额外 GCC 编译一次实际 stage2 C，随后执行自编译、335 checks、126 emits 与 8 个坏输入压力用例。当前 stage2 C 与 runtime 复制到全新构建目录并逐字节核对，防止同目录同名头遮蔽已绑定 runtime。
- 独立 GCC 14.2 本机验证通过，详细范围及来源绑定见 `compiler/validation/stage2-sanitizer-2026-11-23.json`；正常编译器退出有分配检查，首错 `os.exit` 路径不声称泄漏检查。LSan、TSan 和其他平台 sanitizer 不在本批范围。
- 已把应用验收接入 Linux x86-64、Windows x86-64、macOS arm64 原生 CI；本批 CI 尚待推送后运行。最终结果与回执索引在 CI 完成后补充。

## 历史批次（已移出）

第六至第十一批在 `history/handoff-batches-2026-11-19-to-21.md`；第十二至第二十六批（原第 18–31 节：构建缓存、第三到第五轮实验、CI、计量执行器、PCH、SH-1 各批）在 `history/handoff-batches-2026-11-21-to-23.md`。
