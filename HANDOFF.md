# Yavo 交接文档（2026-11-20，第七批：安全与验证收敛）

给下一个会话：先读完本文件，再按第 10 节开工。所有数字都是 2026-11-20 在本仓库实测的。

## 1. 一句话状态

规范 v0.5 已冻结（`docs/`、`yavo-tests/`）；Rust 写的编译器（约 16,000 行，含 C 运行时）已覆盖 P0 链路及 P1—P3 的主要语言功能，完整阶段验收仍未闭合：
结构体、`enum`、`T ! E` 与 `try`/`catch`、`match`、`String`/`Vec`/`Map`、`[]T` 与 `x[a..b]`、移动与析构、视图与 `of`、`&T`、`inout`、`?T`、
方法、`Drop`、元组、不逃逸闭包、`Eq`/`Ord`、`Box[T]`、`interface` 与 `Box[I]`、泛型、`f64`/`f32`、切片、`reserve`，
以及第四批新增的 **`*p`、`&inout T` 局部变量、`&x` 构造（A6、A7、`E0004`）和 `String.fromUtf8`**，第五批新增的 **`Set[T]`、`${x:.N}`、`${...}` 里的字符串字面量**，
第六批新增的 **并发：`Rc`/`Arc`、`Mutex`/`Guard`/`with`、`scope`/`spawn`/`join`、`Send`/`Sync`（含 `Box[Named + Send]`）、`Channel`**；
标准库 I/O、`os.args`、`std/math`；六个目标的交叉编译。
规范一致性：**129 项中通过 113、判错 0、不支持 1、未运行 15**（第四批开始时 84 / 0 / 31 / 14）。唯一不支持的是 T-O5（`Heap` 与保存的比较闭包）；
未运行的是 `yavo fix`（7）、`yavo verify` 报告（4）、运行器不支持的 3 项和要求 sanitizer 分开构建的 T-P3（手动运行结果正确）。
全部提交只在本地，**按用户决定暂不推远端**。

“可用”三批已完成；2026-11-20 用户让我自己定后续顺序并分批：**第四批**（速查卡缺口 + 第 6 项 + 小卡点）已完成，
**第五批**（第二轮冷启动实验 + 按实验结果修速查卡和编译器）已完成：换模型（Sonnet）、不点名特性的新题，Yavo 10/10、Rust 对照 10/10
（`experiments/coldstart-2026-11-20/README.md`）。**第六批**（P3 并发）已完成，分三次提交。**第七批**按用户授权处理安全与验证收敛，结果见第 12 节；第 10 节列出后续优先级。

## 2. 仓库与环境

- 仓库：`/data1/workspace/github/workspace/yavo`（通过 mcp-3 访问，工作区根是 `/data1/workspace/github`）。
- 构建前必须设置（否则找不到 cargo 或链接失败）：

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$HOME/.yavo/bin/rust-linker
cd /data1/workspace/github/workspace/yavo/compiler
```

- C 工具链：固定版本 zig 0.17.1，在 `~/.yavo/toolchains/zig-x86_64-linux-0.17.1/zig`（`toolchain.lock` 有各平台哈希）。
- 这台机器**没有** python3、gcc、clang、valgrind；zig **没有** AddressSanitizer 运行时（UBSan 可用）。`tools/*.py` 跑不了。
- mcp-3 单次调用大约 60 秒就会超时。长任务用 `nohup bash -c '...' > /tmp/x.log &`，再用 `sleep` 轮询日志。
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

常用命令：`yavo check f.yv [--json-diagnostics]`、`yavo run f.yv [--release] [--target T]`、`yavo build f.yv -o out [--release] [--target T] [--emit-c f.c]`。

## 4. 已完成（按提交，最新在上）

| 提交 | 内容 | 一致性通过数 |
| --- | --- | --- |
| 本文件所在提交 | 并发第 2、3 部分：`scope`（隐藏局部的析构等待线程）、`spawn`（每个调用点一个参数结构和线程入口）、`join`、结构推导的 `Send`/`Sync`、`Box[I + Send]`（接口别名）、`Channel[T]{cap}`/`send`/`recv`/`close`/`clone`、`SendError[T]`（按元素类型晚登记的库枚举）；`ok(_u)` 可忽略 `()` 载荷 | 113 |
| `907896c` | 并发第 1 部分：`Ty::Sync`、`Rc`/`Arc`/`Mutex`/`Guard`/`with`（`E0703`）、运行时（pthread/Win32 锁、条件变量、线程、channel、scope；原子堆块计数；输出锁） | 99 |
| 本文件所在提交 | 按第二轮实验修：`Set[T]`（存成 `Map[K, bool]`，`Ty::Set`）、`${x:.N}`（`PrintPart::Fixed`、运行时 `yv_out_fixed`）、`${...}` 里允许字符串字面量（原先误报 `E0020`）；速查卡补 `String`/`str` 不能混比、`os.args()[0]` 是程序名、`Set` 用法、`Vec` 没有 `remove`/`insert` | 93 |
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

流水线：`lexer.rs` → `parser.rs`（AST 在 `ast.rs`）→ `check.rs`（类型、所有权、借用，产出 HIR）→ `codegen.rs`（生成 C17）→ zig cc。

- `types.rs`：`Ty` 是小的 `Copy` 枚举；结构体和枚举共用一张聚合表（`register_struct`/`register_enum`，`finish_agg` 按依赖顺序算出能否复制、是否要析构），`Vec[T]`、`[]T`、`&T`、`?T`、`T ! E` 等通过线程局部表登记（`vec_of`、`slice_of`、`ref_of`、`opt_of`、`res_of`）。`is_copy()` 决定移动还是复制，`carries_borrow()` 决定是否需要跟踪来源。
- 移动：检查器把被移动的局部变量改写成 `HExprKind::Move`；生成代码时复制后把源**清零**。空值和被移走的值都是全零，所以析构永远安全，不需要运行时标志。
- 析构：代码生成维护作用域栈，作用域结束和 `return`/`break`/`continue` 前逆序释放；只被借用的临时值（`f().len()`）在语句末释放。
- 移动检查：`FnCx::moved` 按控制流合并（`if` 两支取并集，提前离开的分支不算），循环体再检查一遍。
- 借用：每个局部变量有 `loans`（指向哪些变量，或临时值）。`Pre` 预扫描给语句编号并记录名字的使用位置和所在循环，`live_after` 据此判断视图是否还会被用到。循环里的使用会延长到循环结束。
- 运行时 `runtime/yavo_rt.h`：输出缓冲、panic、整数检查、`yv_string`/`yv_vec`、视图函数、`YV_ALLOC_CHECK` 计数。 新增 `yv_out`（打印和 `format` 共用的输出目标）、文本查询、稳定归并排序、标准输入缓冲、文件读写（含 Windows 的 UTF-16 路径）、启动时的参数转换。
- 枚举是 `{ tag; union { v0, v1, ... } u; }`，结果是 `{ ok; val; err; }`；全零都是合法值，所以“移走后清零”的做法照旧成立。`match` 统一走 `HPat` 模式树：检查器做可用性（usefulness）分析判断穷尽和不可达分支，代码生成成 `if`/`else if` 链；匹配拥有的临时值时，绑定拿走对应部分，其余部分在分支开头析构。只含 `some(x)`/`none`/`_` 的 `match` 仍走原来的 `match_opt`。
- 表达式里的语句块（`catch` 块、`match` 分支块）用 `Gen::outer` 保存外层语句的临时值，跳转时一并释放；`try` 提前返回时，正在构造的结构体字段、向量元素、`sink` 实参通过 `hold`/`release` 记在临时值里，一并释放（V3、V6）。
- 方法：解析器把方法改名为 `Type.m` 放进 `Program::fns`（`FnDecl::owner`），接收者是名为 `self` 的第一个参数；检查器 `method_call` 先查用户方法（`user_method`、`method_on_place`），统一走 `call_fn`。`inout self` 的接收者是 `HExprKind::InoutPlace(HPlace)`（可带字段路径）。C 函数名把 `.` 换成 `__`。
- `Drop`：`types.rs` 的聚合表有 `user_drop`；带 `Drop` 的类型 C 结构里多一个 `uint8_t yv_live`，字面量置 1，析构帮助函数先看标志、清零、调用 `yv_fn_T__drop`，再析构字段。`FnCx::in_drop` 用于 `E0803`。
- 元组：`Ty::Tuple(id)`（`tuple_of`/`tuple_elems`），C 里是成员 `f0`、`f1`…的结构；元素访问复用 `HExprKind::Field`（基类型是元组时下标就是元素号），所以移动、借用来源等规则和字段一致。
- 闭包：`lift_closure` 用新的 `FnCx` 把闭包体检查成独立 `HFn`（名字 `closure.N`，追加在用户函数之后，下标 `n_fns + k`）；捕获变量由 `idents_expr` 找出，作为前置 `Param`（`FnCx::captures` 记录，用于错误信息）。`HProgram::tramps` 记录每个 `sortBy`/`Ord` 排序的适配函数。外层 `Pre` 只记录闭包里的名字使用，不给闭包体的语句编号。
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
  `&inout T` 类型只在局部变量注解里由 `local_ty` 解析，其他位置 `U0001`。`String.fromUtf8` 是 `HExprKind::FromUtf8`，`Utf8Error` 在 `LIBRARY_ENUMS` 之后登记为库结构体。
- 并发（第六批）：`types.rs` 的 `Ty::Sync(id)` + `SyncKind`；`ptr_inner`/`ptr_read_only` 让 `unbox`、`field_path`、`mutable_place` 穿过 `Rc`/`Arc`/`Guard`
  （`BOX_STEP` 复用；`Rc`/`Arc` 写入报 `E0101`）。`LocalState` 新增 `guard`（`with` 绑定，`store_loans` 据此报 `E0703`）和 `pinned`（scope 的隐藏局部，
  `check_not_borrowed` 视为活到 scope 结束）。`FnCx::thread_scopes` 是当前嵌套的 scope。接口能力用 `dyn_with_caps`/`dyn_base`/`dyn_caps`（别名 id 共享 C 类型和虚表，
  检查器里按 `dyn_base` 查表）。代码生成：`Gen::spawns` 收集参数结构和 `yv_run_N`，在函数体之前插入；`yv_jrec_K` 在辅助函数之后定义。

## 6. 约定

- 错误码：规范错误 `E0xxx`（见 `yavo-tests/ERRORS.md`，前端码 `E0010`–`E0018`、`E0020` 已登记）；**合法但未实现的写法一律报 `U0001`，绝不能报 `E0020`**；警告 `W0001`。
- `tests/p0/errors_*.yv` 开头是 `// expect-error: CODE 行:列`。头部每多一行，正文行号整体加一；做法是先写无头文件，用 `yavo check` 取位置，再按头部行数加偏移生成头部。
  现在用 `tools/expect_header.sh compiler/tests/p0/errors_x.yv` 自动完成（去掉旧头部、重跑 `yavo check`、按头部行数加偏移写回）；它记录的是编译器当前的输出，生成后要逐条核对。
- 解析阶段的 `U0001`（parser 报的）会让整个文件不再进入检查器，所以它们只能和别的解析期 `U0001` 放在一起（`errors_unsupported.yv`），不能和检查器的错误混在一个测试文件里。
- 一个特性做完：补 `run_*` 和 `errors_*` 两类测试；跑全部测试（debug、release）；跑堆块检查；交叉编译抽查；刷新一致性基线；更新 `compiler/README.md` 对应小节和根 `README.md` 的数字；本地提交。
- 已解锁的旧写法可能让旧测试失效（例如某行不再报 `U0001`），改测试时保持行列不变，换成仍然成立的用例。
- `tools/expect_header.sh` 默认用 **debug** 版 `yavo`；只构建了 release 时要写 `YAVO=$PWD/target/release/yavo bash ../tools/expect_header.sh ...`，否则会拿旧二进制的输出生成头部。
- 新错误码登记在 `yavo-tests/ERRORS.md` 的“前端通用错误”表里（本轮加了 `E0019`，注明日期）。

## 7. 已知限制与保守之处

- 借用检查按语句粒度和语句顺序：`if` 里修改、只在 `else` 里用视图也会报错；`E0305` 不看存储目标之后是否还被使用。第七批已修复 sink 视图容器跨线程的来源漏检，并补组合回归；此前“不会漏报”的表述没有依据。
- 可选值：对可写位置的 `if let`，**不可复制**载荷的绑定可写（`&inout`）；**可复制**载荷仍按值复制，给绑定赋值报 `U0001`。可写绑定还在使用时，原变量的任何使用都报 `E0102`（保守）。
  对 `&inout` 绑定的字段再做 `if let`（`if let l = n.left`）得到只读绑定。`a ?? b` 把 `a` 整体消费。`?? return/break/continue` 只允许作为整个 `let` 初值或赋值右侧。
- `&inout T`：局部变量可以写出类型并用 `&place` 构造，`*p` 读写。仍不能作参数、字段、元素或返回类型（`U0001`，参数用 `inout`）；
  经 `*p` 存入带借用的值 `U0001`。借用按语句粒度，所以 `println("${q.len()} ${s}")`（`q` 指向 `s`）会多报 `E0102`，要拆成两句。
- `Map`：键只支持整数、`bool`、`str`、`String`；`update` 的值类型限可复制且不带借用。
- 导入：已实现的模块成员有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`math.sqrt/pow/floor/ceil/round/pi/e`，其余报 `U0001`。`os.exit` 不运行析构。
- 枚举和结果：不支持字面量模式（`.Num(0)`、`1 =>`）、模式里的命名字段、`if let` 带模式、带载荷枚举的比较、泛型枚举、结构体里的结果字段。
- `Box`：没有 `.clone()`（写 `Box(b.clone())`）、不能打印或比较。
- 接口：已支持 `+ Send`/`+ Sync`；接口作泛型约束只做检查（单态化后直接调用具体方法），没有接口继承、`Self`、默认方法；`struct S[T]: I` 报 `U0001`。
- 泛型：**单态化，函数体只在实例化时检查**——从没被调用的泛型函数完全不检查，同一错误可能在每个实例报一次；结构体字面量要写类型参数（结构体自己的方法里除外）；
  没有泛型枚举、方法自己的类型参数、`Stack[int].make()` 这类在实例上的静态调用。
- 浮点：`%` 和位运算不支持；`1 + 1.5`（两个字面量）报类型不符；Windows/macOS 上的浮点打印未在真机验证（依赖各自 libc 的 `snprintf`/`strtod`）。
- 切片：没有 `x[a..]`、`x[..b]`；v1 本来就没有可变切片。
- 闭包：只能直接作为 `sortBy`/`update` 的实参。
- 元组：不能给元素赋值、不能比较、不能整体打印、不能做 `Map` 键、没有元组模式。
- 格式说明只支持浮点的 `:.N`；`String == str` 混比报 `E0001`（规范 K1 没定义跨类型比较，速查卡已写明；要不要放开是语言决定，留给用户）。
- `Set`：没有集合运算（并、交）、没有 `Set` 字面量里的元素（`[1, 2]` 不能直接当 `Set`）。
- 堆块检查发现不了释放后使用；项目服务器没有 ASan/TSan，第七批已把同一生成 C 与运行时带到另一 Linux 环境分开运行 ASan/UBSan、TSan，详见第 12 节。Windows、macOS、aarch64 产物只验证了编译与文件格式。
- 并发：`spawn` 只接受具名函数调用，可变借用（包括泛型元组中嵌套的 `&inout`）暂报 `U0001`；`Join` 不能写出类型名或移动（`U0001`）。规范已定义 inout 线程参数与公开 Join，属于实现缺口。
  容器递归克隆遇到不可克隆资源现已在检查期拒绝；用户 `Drop` 类型的自动逐字段克隆报 `U0001`，须显式构建资源或逐元素调用用户方法。
  当前接口集合的克隆检查遍历本编译单元所有具体实现；未来分离编译需要显式传递该保证。`&b` 不会把 `Rc[T]` 转成 `&T`（规范只对字段、方法、下标投影）。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

当前不支持 1 项、未运行 15 项（统计只算每个测试的第一个卡点）：

| 卡点 | 测试 |
| --- | --- |
| `std/collections` 的 `Heap[T].by(闭包)`（保存比较闭包，O5） | T-O5（P1/P2 唯一剩下的） |
| `yavo fix` | 7 项（未运行） |
| `yavo verify` 报告 | 4 项（未运行） |
| 运行器：`run-allowed`（T-GR9）、`run-exit`（T-S3）、分配失败注入（T-GR8） | 3 项（未运行） |
| 运行器：`sanitize-mode: separate-artifacts`（ASan/UBSan 与 TSan 分开构建） | T-P3（未运行；本机没有 ASan/TSan 运行时） |

## 9. 计划表

| 序号 | 任务 | 状态 |
| --- | --- | --- |
| 1–4、7 | 可选值、`Map`、导入、枚举与结果、拥有值字段 | 已完成（上一轮） |
| 5 | 方法、`Drop`、`Eq`/`Ord`、`interface`、`Box` | **已完成**（`Box` `6456fc8`、接口 `832b6db`） |
| 6 | `*p` 写入、写出 `&inout T`、`&x` 构造（A7、`E0004`） | **已完成**（`0ed6802`，8 项一致性测试） |
| 8 | 浮点 `f64`、`totalCmp` | **已完成**（`16527cb`） |
| 9 | 切片 `x[a..b]`、`P-STR-BOUNDARY` | **已完成**（`e7e106f`，顺带 `toVec`、`reserve`） |
| 10 | 泛型函数和泛型结构体 | **已完成**（`bd8ab57`） |
| 11 | `yavo fix`（只自动应用不改变语义的修复） | 未开始；7 项 |
| 12 | P3：`scope`/`spawn`、channel、`Mutex`/`Arc`、`Send`/`Sync` | **已完成**（`907896c` 与本文件所在提交；20 项通过，T-P3 未运行） |
| 13 | `yavo verify` 与验证报告 | 依赖 12；4 项 |
| 冷启动实验 | 只给速查卡写 T01–T10 | **已做一轮**：10/10 一次通过（局限见实验 README） |
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
| 第六批 | 第 12 项 P3 并发（`scope`/`spawn`、`Channel`、`Mutex`/`Arc`/`with`、`Send`/`Sync`） | **已完成**：(a) `Rc`/`Arc`/`Mutex`/`Guard`/`with` 与运行时（`907896c`，93 → 99）；(b) `scope`/`spawn`/`join`/`Send`/`Sync`、(c) `Channel`（本文件所在提交，99 → 113） |

## 10. 下一个会话的第一步

1. 设置第 2 节的环境变量，`cargo build --locked --offline --release`；跑 `cargo test --locked --offline`（和 `--release`），确认全部通过，conformance 是 113 / 0 / 1 / 15。
2. 第七批安全与验证收敛已完成。后续建议按以下顺序推进：
   - **`yavo fix`**（7 项未运行）：规范要求“只自动应用不改变语义的修复”，速查卡和错误表多处引用它，agent 最直接受益。
   - **T-O5：`std/collections` 的 `Heap`/`Deque`** 和能保存的闭包（O5）：最后一个不支持项，也是速查卡里列了但没实现的库。
   - **第三轮冷启动实验，带并发题**：本轮实验没测并发；同时可以量速查卡的实际 token 数（28 KB，未量过）。
   - **`yavo verify` 与报告**（4 项）、运行器支持 sanitizer 分开构建（T-P3）。
   - 平台与性能证据：Windows/macOS 真机运行、少量与 C 同算法同数据布局的基准。当前尚无近 C 性能结论。
   - 语言决定：`String == str` 是否允许混比。`Join` 的公开类型与作用域限制、`spawn inout` 的 Send 条件已在标准库契约规定，按实现缺口处理。

## 11. P3 并发设计（第六批，2026-11-20；已按此实现）

目标：解锁 `yavo-tests/concurrency/` 的 21 项（T-GA1/GA2/R1/R2 是报告类，未运行）。思路是尽量复用现有机制：

- **类型**：新增一个 `Ty::Sync(id)`，表里存 `(种类, 内层类型)`，种类为 `Rc`、`Arc`、`Mutex`、`Guard`、`Channel`、`Join`、`Scope`（隐藏）。都不可复制。
  `Rc`/`Arc`/`Mutex`/`Guard` 在 C 里都是 `T *`，指向“头部 + 值”分配块中的值（头部在值前面：锁、持有线程号、引用计数），
  所以字段、方法、下标的投影可以直接复用 `Box` 的 `Unbox`/`BOX_STEP`（`Rc`/`Arc` 只读，写入 `E0101`；`Guard` 可写）。
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
