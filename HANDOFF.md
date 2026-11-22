# Yavo 交接文档（2026-11-22，第十九批：运行时 PCH 缓存与编译延迟验证）

给下一个会话：先读第 1、2、10、24 节再开工。第 24 节是第十九批运行时 PCH 构建优化；第 23 节是第十八批第六轮准备，真实计量资格仍未通过、受试会话为 0；第 22 节保留已完成的第五轮实验。第 21 节是第十六批三平台 CI；第 20 节是第十四、十五批语言修订与第四轮；第 19 节是第三轮；第 18 节是构建缓存。第六到第十一批原始记录与完整 v1 清单在 `history/handoff-batches-2026-11-19-to-21.md`。

## 1. 一句话状态

**第十九批新增后端优化，不修改任何 Yavo 语言语义。** 在已有完整二进制内容寻址缓存之外，正常 `build`/`run` 的 pinned Zig 路径可把 1931 行运行时头文件缓存为按目标/模式/插桩划分的 SHA-256 校验 PCH，源文件改动后也复用。两轮四程序×每程序七配对 Release 构建测得 1.27–1.45x 加速，全部配对二进制 SHA-256 一致；每轮三项 ≤200 ms，超标程序并不一致，正式的代表性 200 ms 门尚未闭合。最新 Debug/Release 全套各 **133/0/1**（17 测试组，服务器完整退出码均 0）。细节见第 24 节。

规范以 v0.5 冻结版为基础，2026-11-21 按第三轮实验修订了 V2（两阶段借用）、K1（`String`/`str` 比较），新增 V6a（字段默认值）、V14（P-STACK），见 `docs/decisions.md` 第六节；冻结测试包 `yavo-tests/` 不改。Rust 编译器与 C 运行时覆盖 P0 链路及 P1—P3 的主要功能：结构体、枚举、`T ! E`、`match`、`String`/`Vec`/`Map`/`Set`、切片、移动与析构、视图与 `of`、`&T`/`&inout T`、`?T`、方法、`Drop`、元组、不逃逸闭包、`Eq`/`Ord`、`Box`、接口、泛型、浮点；并发（`Rc`/`Arc`/`Mutex`/`with`/`scope`/`spawn`/`Channel`/`Send`/`Sync`）；Heap/Deque、`Arena[T]`/`Id[T]`、源文件模块；`yavo fix`/`allocations`/`verify`；后端构建缓存；六个目标的交叉编译。第十五批补入 V2a（元素上的修改方法），见 `docs/decisions.md` 第七节。**P0—P3 主体功能已实现，完整 v1 尚未验收，P4 仍为小规模试验。** 三平台 CI 已落地；Windows/macOS 为明确选定的原生套件。完整 v1 的工具/语言缺口、Agent 成本和正式性能门尚未闭合。各批次细节见第 4 节和 `history/`。
规范一致性：**129 项中通过 124、判错 0、不支持 3、未运行 0、被新决定取代 2**（T-V1、T-GF2 断言旧 V2，登记在 `compiler/tests/conformance_superseded.txt`；不支持的 3 项需要锁定工具链无法链接的 ASan 运行时）。该统计只表示既有用例覆盖，不是完整 v1 完成比例。
**2026-11-22 用户已授权推送 `msly/yavo` 并使用 GitHub CI；本批实现已推送，`073003d` 的六项 CI 任务全部通过。** 当前 [run 45002067581](https://github.com/msly/yavo/actions/runs/45002067581) 同时覆盖三平台、独立 ASan/UBSan、第五轮证据审计及第六轮准备检查；六份产物均已下载并核对内容、字节数与 SHA-256，回执在 [github-ci-agent-eval-2026-11-22.json](compiler/validation/github-ci-agent-eval-2026-11-22.json)。Linux 另有 12 项文档程序/诊断检查通过。第 21、22 节保留前两批 CI 历史；旧“暂不推送”约定已被本次授权取代。
本地测试基准 `b3c27a6`：第十五批完整单条命令 Release **129 通过 / 0 失败 / 1 忽略**、Debug **129/0/1**；这是 Cargo 测试函数数，和上面的 129 项规范用例是不同计数单位。忽略的是旧 P0 探索性报告，当前 conformance 套件会执行。第十四批 `1af1f69` 为 123/0/1，新增 6 项为 `index_assignment.rs`；六个交叉目标能构建含 P-STACK 运行时的程序，最新原生 CI 覆盖 Linux x86-64 全套与 Windows x86-64/macOS arm64 选定套件。
**最新已完成 agent 实验（第五轮，第 22 节）**：12 份新规格，4 个不继承父会话的独立会话，每语言 2 次；Yavo、Rust 均 **24/24 首编与完整题目要求通过**，包括公开、隐藏用例及模块/线程结构。48 份交付的首稿与终稿完全相同，0 次修复；96 个首/终稿快照合计 1,344 次重放全部通过。Yavo/Rust 的受控 lab 操作为 138/132（1.0455 倍），端到端时间为 671.2876/557.0373 秒（1.2051 倍）；多出的 6 次操作全是两批各 3 次速查卡读取。token、精确模型 ID、采样参数缺失并保留 `null`，正式成本门仍 blocked。三题是旧题型的新规则，模型配置也不同于历史 Sonnet，不能将跨轮首编提升归因于编译器修改。第四轮原始结果与 T10 前导零失败保留在第 20 节。

**第六轮准备（第 23 节）**：短版启动卡与按需索引已实现，12 项程序/诊断检查通过；逐请求 Responses 执行器、资格验证、八题文档对照及证据审计已准备。当前 CLI 状态数据库不可写，环境无可用 API key；真实资格 `blocked`，**0 次受试模型请求、0 会话、0 交付**。下一步先在具备授权计量凭据的环境中完成模型配置、runtime freeze 与真实工具往返资格，不启动缺少 usage 的第六轮。

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
| 冷启动实验 | 五轮 | 第五轮两语言各 24/24 首编、公开、隐藏及结构通过；受控操作/时间比 1.0455/1.2051，token 缺失；4 会话、12 题（第 22 节） |
| 已落地 | 三平台 CI | `073003d` 的 run 45002067581 六项全通过：Linux 全量、Windows/macOS 选定原生套件、独立 ASan+UBSan，以及两项实验检查（第 23 节） |
| 已落地 | 第五项 CI | `0bad3d2` 的 run 44996118634 通过，核验 1,344 条判定、96 个源码阶段、16 项结构审查和冻结题包 |
| 已落地 | 第六项 CI | `073003d` 通过 33 项执行器、25 项 lab/audit/freeze、11 项裁判/汇总场景，以及 94 个准备文件和 112 个独立 oracle 输入；真实启动仍 blocked |
| 准备完成，资格 blocked | 第六轮文档对照 | 短卡/索引、逐请求计量执行器、8 题与 112 个输入已准备；0 受试会话，须先获得真实 usage 资格 |

批次与用户约定的历史（“可用”三批、第四到第六批）见 `history/handoff-batches-2026-11-19-to-21.md`。

## 10. 下一个会话的第一步

1. 先检查 Git 状态和最新提交；环境见第 2 节。冻结用例基线见第 1 节。代码修改按风险加回归，明确记录 Debug/Release 实际完成范围。**不要同时跑两条 `cargo test`**：它们共用临时目录，会出现 `CurrentDirUnlinked` 之类的假失败。GitHub 原生 CI 已通过，后续提交沿用 [CI 说明](tools/ci/README.md) 的平台、工具链和范围；cargo 退出 0 之外还须核对 conformance 的 wrong/not_run。
2. **先完成真实计量资格**：`tools/agent_eval/` 的 Responses 执行器和严格审计已实现；接口、配置及原始探针见 [说明](tools/agent_eval/README.md)。当前环境没有 API key，Codex stdio 初始化在状态 SQLite 文件处失败。不要重复已失败的探针、移动凭据或更改只读权限。到具备已授权 API 环境变量的 Linux 主机后，配置实际模型/参数，创建并提交 runtime freeze，再完成两次真实响应围绕一次固定只读工具的资格链。缺模型、usage、唯一标识或工具链任一项就保持 blocked，不能用合成记录替代。
3. **运行已准备的文档对照**：使用 [第六轮题包](experiments/docs-ab-2026-11-22-r6/README.md)，同一 Yavo、模型和工具配置比较 full/compact，8 题、每臂 2 个空白会话；四会话封卷后才判隐藏用例。两组必读 payload 为 36,736/12,783 字节，预定最少读取操作为 6/3；实际后续检索、修复和失败成本全部计入。短卡保留借用、V2/V2a、索引赋值和字符串转义规则，完整卡及第五轮冻结输入保留原样。第六轮只测文档策略，不能验收相对 Rust 的 20% 成本目标。
4. **进入 60 题 pilot，再决定正式样本量**：覆盖任务类型和难度，题型留出与同型新规则分别统计。第五轮 12 份新规格中三题延续旧题型、九题为其他新规格；4 个会话内共享题目上下文，不能按 48 份交付计独立会话。没有新失败支持时，不扩展语言语义；`m.getClone(k)`、可变切片等候选继续等待具体失败证据。
5. 预编译运行时目标文件：编辑后重建 Release 仍 214–247 ms，超过 200 ms 目标；约 50–60 ms 用于重编 `yavo_rt.h`（第 18 节）。
6. 待排的完整 v1 缺口：用户测试块与 `yavo test`；依赖与程序锁文件；FFI/unsafe 与 audit；固定工具链的 ASan 能力。本轮不扩展语言功能。
7. 一般闭包值/高阶参数、公开 Join、spawn inout、callable 类型、Atomic API 等；完整清单见 history 文件中的“完整 v1 目标”。

正式门槛仍按 `docs/plan.md`：配对通过率差的单侧 95% 下界至少 −5 个百分点，单位成功交付成本较 Rust 低至少 20% 并独立复现；冻结基准的运行耗时几何均值不超过 C 的 1.5 倍。工程目标包括小程序编译 P50 ≤200 ms、Linux strip 后 ≤250 KB、Windows 真机测试及可复现产物。第五轮 12 题只完成诊断；正式题量仍按 60 题 pilot 估计、起点不少于 400 题的既定设计。

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

## 22. 第十七批：第五轮新规格实验与成本分解（2026-11-22）

**Yavo、Rust 各 24/24 首次编译、公开用例、隐藏用例和必需结构要求通过。** 本轮有 12 份新规格、4 个独立 `fork_turns="none"` 会话，每语言两批；首波 Yavo A/Rust A 并发，次波交换启动次序为 Rust B/Yavo B。全部 48 份首稿与终稿逐字节相同，没有编译失败、公开样例失败或修复。实验包冻结提交为 `8c200aa`，语言/编译器基准为 `5cd916b`，最后编译器代码改动为 `8a65228`；本批未改编译器、运行时、语言规范或冻结 129 项规范输入。

### 22.1 正确性与证据

每题 2 个公开用例和 12 个隐藏用例（8 个明确边界、4 个固定种子随机用例），共 168 个不同输入。四批全部交卷封存后才运行隐藏裁判，结果不反馈受试者修复。96 个首/终稿快照分别重放，**公开 192/192、隐藏 1,152/1,152，合计 1,344/1,344**；48 对首/终稿源码相同，这些重放不增加独立题目或会话数。Yavo 重放启用 `YAVO_ALLOC_CHECK=1`。

行为复核逐条核对输入 hash、完整输出字节、落盘输出、退出状态、104 个源文件 hash 和 96 个工件 hash；未发现行为失败可归因于解答算法或编译器。T04 全部解析/精确换算位于非入口模块，T06 三个真实 OS 线程先启动再等待、分别编码指定片段后有序合并，两项结构要求均逐份源码审计通过。

原始受试操作、每次源码写入、首编前源图、实际构建命令、退出状态与单调时钟已直接保存在实验目录。来源：[报告](experiments/coldstart-2026-11-22-r5/README.md)、[结果](experiments/coldstart-2026-11-22-r5/results.json)、[行为复核](experiments/coldstart-2026-11-22-r5/evidence/behavior-review.json)、[结构复核](experiments/coldstart-2026-11-22-r5/evidence/manual-review.json)。

### 22.2 操作与时间

| 会话 | 首编且完整题目通过 | 受控 lab 操作 | 端到端秒数 |
| --- | ---: | ---: | ---: |
| Yavo A | 12/12 | 69 | 337.8823 |
| Yavo B | 12/12 | 69 | 333.4053 |
| Rust A | 12/12 | 66 | 282.2978 |
| Rust B | 12/12 | 66 | 274.7395 |
| Yavo 合计 | 24/24 | 138 | 671.2876 |
| Rust 合计 | 24/24 | 132 | 557.0373 |

Yavo/Rust 操作比 **1.0455**，时间比 **1.2051**。多出的 6 次操作全部来自 Yavo 两批各 3 次读取冻结速查卡（32,889 字节）；读题、写代码、构建、公开运行等其他类别计数完全相同。该指标是单请求单操作的受控 lab 计数，不是平台总工具调用，不能直接与旧轮 38/24 求改善比例。

实际编译进程累计 Yavo 5.3451 秒、Rust 3.8910 秒，差 1.4541 秒，只占端到端差 114.2503 秒的一小部分。会话区间还包含模型处理、操作等待与并发影响，现有记录未分离其各自贡献。完整边界与类别见 [成本分解](experiments/coldstart-2026-11-22-r5/evidence/cost-analysis.json)。

### 22.3 保留的缺口与后续动作

token、精确服务模型 ID 和 sampling 无可核验来源，均为 `null`；协作工具未暴露 usage，只读 CLI 初始化探针失败。正式“单位成功交付成本至少低 20%”门仍为 blocked，不估算 token 或货币成本。当前会话配置不同于历史报告中的 Sonnet，且题包更换，不能把第四轮到第五轮的首编变化归因于语言/诊断修改。

三题沿用旧题型并加入新规则：T01 账本增加原子批处理，T06 游程编码增加三线程切分与边界归并，T07 LRU 增加 TTL；其余九题是其他新规格。本轮是新规格诊断，不是严格的全题型留出。

下一步依次为：可记录真实 usage 的执行器、同模型完整/最小速查卡对照、60 题 pilot。未出现新失败，不据此扩大语言语义。完整 v1 缺口、正式不少于 400 题设计与验收门保持第 7、10 节及 `docs/plan.md` 的原口径。

本批增加第五项 CI 任务检查实验证据；**`0bad3d2` 已推送，[run 44996118634](https://github.com/msly/yavo/actions/runs/44996118634) 五项全部通过**。Linux Debug/Release 均 129/0/1，Windows 选定套件均 71/0/1，macOS 选定套件均 74/0/1；独立 ASan/UBSan 为 60 通过、0 失败；实验归档的 1,344 条判定、96 个源码阶段、16 项结构审查、43 个冻结文件及 28 个题包文件全部核验通过。Linux 两种模式的一致性均为 124/0/3/0/2。五份下载产物的大小与 SHA-256 均匹配 GitHub API；完整来源、命令记录和范围在 [CI 回执](compiler/validation/github-ci-round5-2026-11-22.json)。第 21 节保留 `8a65228` 的历史 CI 结果。最终文档与回执提交沿用上述已验证实现，不改变编译器或实验证据检查程序。

## 23. 第十八批：计量执行器、短版文档与第六轮准备（2026-11-22）

### 23.1 可用产物与实际验证

- `docs/agent-start.md`：134 行、8,936 字节；`docs/agent-api-index.md`：33 行、3,607 字节。合计比 32,889 字节完整卡少 61.9%。四个文档项目、四个规则正例和四个诊断反例全部通过，见 `compiler/validation/agent-start-2026-11-22.json`。字节下降尚未转化为受试 token 成本证据。
- `tools/agent_eval/`：每次 provider 请求保存原始请求/响应、唯一标识、usage、返回模型及参数；严格区分请求模型和返回模型、显式设置和服务默认值。API key 不传给 lab/编译器/受试程序。资格链必须消费真实只读 probe 的结果，合成 fixture 不可进入参与者运行。
- `experiments/docs-ab-2026-11-22-r6/`：8 道新规格，16 公开、96 隐藏输入；两次重建一致，独立替代算法复核 112/112。两组共用索引与十八个按需 API 主题，分别提供完整卡和短卡。协议计划 32 份交付，当前为 0。
- 新记录器保留首次实际编译前源图；缺入口文件不占首编编号、不允许空提交；模型设置/执行器身份、文档页字节和源码漂移均有针对性检查。汇总须从原始 provider 与 lab 链重算，缺失量保留 null。

### 23.2 资格阻塞与接手条件

已有 Codex 0.172.0-alpha.2 的新 stdio 初始化路径，在打开 `/home/agent/.codex/state_5.sqlite` 时失败。当前环境没有 API key；没有移动 HOME/CODEX_HOME、复制认证或更改权限。CLI turn 汇总与 thread/turn usage 通知也不能单独证明逐 provider 请求身份。原始错误和导出 schema 观察在第六轮 `evidence/executor-probe.json` 及其子目录。

当前配置模型与 freeze 字段为 null，实际模板调用在网络前拒绝，provider 请求 0；默认 `launch-qualification.json` 为 blocked。准备清单绑定当前源文件，但尚未创建带实际模型/主机工具身份的 runtime freeze。必须先选择并冻结可计量模型，完成真实资格，再开始受试；不能把离线测试或 CI 绿灯当作第六轮完成。

服务器已安装 Python 3.13.18 独立运行时，官方发布包的 35,596,130 字节及 SHA-256 已核对；SSL 默认 CA 验证 HTTPS、urllib 和 Perl 均可用。服务器实跑 33 项执行器、25 项 lab/audit/freeze、11 项裁判/汇总场景，94 个准备文件与 112 个独立 oracle 输入全部通过；启动状态仍 blocked，0 模型请求/会话/交付。环境部署回执为 `compiler/validation/agent-eval-runtime-2026-11-22.json`，没有为绕过认证而修改 HOME、CODEX_HOME 或系统权限。

### 23.3 CI 与未完成验收

本批实现提交 **`073003dff72de36dfb9fd7b33afb774dea24eebc`** 已推送；[run 45002067581](https://github.com/msly/yavo/actions/runs/45002067581) **六项任务全部通过**。Linux Debug/Release 均 129/0/1，Windows 选定原生套件均 71/0/1，macOS 选定原生套件均 74/0/1；Linux 两种模式规范一致性均为 124/0/3/0/2；独立 ASan+UBSan 为 60 通过、0 失败。Linux 重编短卡示例和规则程序、检查预期诊断，12/12 通过，单独计数。

新增 Agent evaluation preparation 检查通过 33 项执行器测试、25 项 lab/audit/freeze 测试、11 项裁判/汇总场景，核对 94 个准备文件及 112/112 个独立 oracle 输入；第五轮 1,344 条判定、96 个源码阶段、16 项结构审查继续通过。六份产物的下载字节数和 SHA-256 均匹配 GitHub API，实际报告、来源提交与范围见 [CI 回执](compiler/validation/github-ci-agent-eval-2026-11-22.json)。这些检查没有发起真实模型请求；第六轮仍为 0 会话、0 交付。

最后的文档与回执提交仅记录上述已验证实现。编译器、运行时、语言语义和 R5 冻结输入均未修改；完整 v1、Agent 正式成本门和正式 C 性能门继续未验收。

## 24. 第十九批：运行时 PCH 内容缓存与构建性能（2026-11-22）

### 24.1 实现范围

- `compiler/src/toolchain.rs`：保持原 `compile` 接口；新增 `precompile_header`（以真实的 target/Release/Debug/`YV_ALLOC_CHECK` 参数执行 `zig cc -x c-header`）和 `compile_with_pch`。保留原 `verify` 编译路径；PCH 不更改运行时 C 代码、ABI 或 Yavo 语言规则。
- `compiler/src/cache.rs`：独立 `<root>/pch/<key>/`，PCH key 包括整个运行时头、完整工具链锁、Zig 程序路径、C flags、目标和原生 CPU 指纹，不包括用户代码。PCH 与 checksum 原子发布，使用前检查 SHA-256，损坏或缺失时重建；并发构建用独立临时文件。默认只保留最近 32 个 PCH 条目，`YAVO_PCH_MAX_ENTRIES` 可覆盖。
- `compiler/src/main.rs`：仅在固定 Zig、已启用构建缓存且二进制缓存未命中时尝试运行时 PCH；`YAVO_NO_PCH=1` 可关闭。创建或加载 PCH 失败，以及使用 PCH 编译失败，均回退到完整直接 C 编译；`YAVO_CACHE_TRACE=1` 输出 PCH 命中/缺失/回退。`YAVO_NO_CACHE=1`、`YAVO_CC`、`verify` 不走 PCH。
- `compiler/tests/build_cache.rs`：缓存复用、跨模式/跨目标/插桩隔离、损坏恢复、PCH LRU；6 路并发检验增查 PCH 临时文件。缓存套件从 10 个测试增加到 14 个。

### 24.2 证据与界限

- 本轮 Release、Debug 各完整跑过 **17 个测试组，合计 133 通过、0 失败、1 忽略**，退出码均为 0。两个先前的 Debug 驱动被单次命令执行时限截断，未形成完整成绩；第三次通过服务器端独立日志执行完成。这 133 是 Cargo 测试函数数，冻结规范仍另按 129 项计数。
- 初步 pinned Zig 0.17.1 实探 6 个目标（Linux x86-64/aarch64，Windows x86-64/aarch64，macOS x86-64/aarch64）全部可进行 `-x c-header`/`-include-pch` 的交叉编译；该探针在项目 Linux 主机执行，不等于原生 Windows/macOS PCH 测速。
- `benchmarks/small-c-v1/pch_probe.py`：独立二进制缓存，每次修改增加源文件前置注释的**行数**使生成 C 变化，两个编译路径使用完全相同的源码文件，顺序交替；每程序 7 个配对 Release 样本，校验每次二进制 SHA-256 相同。`results/runtime-pch-2026-11-22.json` 保留原始毫秒数组、每组 P50、工具/运行时哈希。

| 题目 | 首轮直接/PCH P50 | 最终编译器直接/PCH P50 | 后轮加速比 |
| --- | ---: | ---: | ---: |
| integer_recurrence | 253.8 / 179.8 ms | 252.5 / 180.8 ms | 1.40x |
| vec_transform | 260.3 / 204.1 ms | 256.1 / 190.6 ms | 1.34x |
| mutref_scatter | 264.8 / 196.9 ms | 271.0 / 193.0 ms | 1.40x |
| deque_ring | 283.8 / 195.3 ms | 271.2 / 213.8 ms | 1.27x |

上述是构建时间，不是生成程序的执行时间。两轮各有三项满足 200 ms 门，但首轮 `vec_transform` 204.1 ms、第二轮 `deque_ring` 213.8 ms；只有改进的趋势在这两轮内得到支持。PCH 免去重复解析运行时头，但不能避免每次重新编译用户生成 C 和链接；继续优化若要覆盖所有代表性程序，应先拆解余下的 Zig 启动、C 优化与链接耗时，而不是增加语言特性。初轮回执 `results/runtime-pch-2026-11-22-initial.json`、最终编译器回执 `results/runtime-pch-2026-11-22.json`；路径相对 `benchmarks/small-c-v1/`。

### 24.3 接手与剩余门

本轮代码提交 [`7877015`](https://github.com/msly/yavo/commit/7877015ae5df2dda02ca22d1f2b621f3c3611fe8) 已推送至 `origin/main`；跨平台新 CI 运行 [45015772252](https://github.com/msly/yavo/actions/runs/45015772252)，必须用该运行的最终结论核验新代码，不能直接沿用第十八批六项全绿的结论。随后追加的基准数据与交接文档不改变编译器实现。

新优化不改变冻结 129 项规范输入或 agent 实验题包；旧第 17/18 批 CI 回执继续作为对应提交的历史证据，不能移用为本次改动的 CI 通过证明。第六轮真实计量仍因模型凭据与 CLI 状态数据库问题 blocked，0 真实模型请求、0 会话、0 交付。优先执行获授权的 runtime freeze 和真实 provider usage 资格检查，之后才能做 8 题短卡 A/B，接 60 题 pilot；`yavo test`、审核/包/FFI 等完整 v1 缺口不变。
