# Yavo 交接文档（2026-11-19，第二、三批已完成）

给下一个会话：先读完本文件，再按第 10 节开工。所有数字都是 2026-11-19 在本仓库实测的。

## 1. 一句话状态

规范 v0.5 已冻结（`docs/`、`yavo-tests/`）；Rust 写的编译器（约 14,400 行，含 C 运行时）已实现 P0、P1 的单线程写法和 P2 的大部分：
结构体、`enum`、`T ! E` 与 `try`/`catch`、`match`、`String`/`Vec`/`Map`、`[]T` 与 `x[a..b]`、移动与析构、视图与 `of`、`&T`、`inout`、`?T`、
方法、`Drop`、元组、不逃逸闭包、`Eq`/`Ord`，以及本轮新增的 **`Box[T]`、`interface` 与 `Box[I]`、泛型函数和泛型结构体、`f64`/`f32`、切片、`reserve`**；
标准库 I/O、`os.args`、`std/math`；六个目标的交叉编译。
规范一致性：**129 项中通过 84、判错 0、不支持 31、未运行 14**（会话开始时 65 / 0 / 51 / 13）。全部提交只在本地，**按用户决定暂不推远端**。

“可用”三批（第 9 节）**全部完成**。冷启动实验：新开的 agent 只看速查卡写 10 道命令行小题，**10/10 一次编译通过且输出正确**
（`experiments/coldstart-2026-11-19/README.md`，含局限说明）。剩下的不支持项几乎都是并发（P3）、`*p`/`&inout T`（第 6 项）和 `yavo fix`/`verify`。

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
| 本文件所在提交 | 冷启动实验（`experiments/coldstart-2026-11-19/`）与本交接文档 | 84 |
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

- 借用检查按语句粒度和语句顺序：`if` 里修改、只在 `else` 里用视图也会报错；`E0305` 不看存储目标之后是否还被使用。只会多报，不会漏报。
- 可选值：对可写位置的 `if let`，**不可复制**载荷的绑定可写（`&inout`）；**可复制**载荷仍按值复制，给绑定赋值报 `U0001`。可写绑定还在使用时，原变量的任何使用都报 `E0102`（保守）。
  对 `&inout` 绑定的字段再做 `if let`（`if let l = n.left`）得到只读绑定。`a ?? b` 把 `a` 整体消费。`?? return/break/continue` 只允许作为整个 `let` 初值或赋值右侧。
- `&inout T`：由 `getMut`、`for … in &c`、可写 `if let` 产生；`inout` 实参可以是字段路径或 `&inout` 变量。仍不能写出这个类型、不能 `&x` 构造（A7）、不能 `*p = v`、不能放进集合（第 6 项）。
- `Map`：键只支持整数、`bool`、`str`、`String`；`update` 的值类型限可复制且不带借用。
- 导入：已实现的模块成员有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`math.sqrt/pow/floor/ceil/round/pi/e`，其余报 `U0001`。`os.exit` 不运行析构。
- 枚举和结果：不支持字面量模式（`.Num(0)`、`1 =>`）、模式里的命名字段、`if let` 带模式、带载荷枚举的比较、泛型枚举、结构体里的结果字段。
- `Box`：没有 `.clone()`（写 `Box(b.clone())`）、不能打印或比较。
- 接口：没有 `+ Send`/`+ Sync`（P3）、接口作泛型约束只做检查（单态化后直接调用具体方法）、没有接口继承、`Self`、默认方法；`struct S[T]: I` 报 `U0001`。
- 泛型：**单态化，函数体只在实例化时检查**——从没被调用的泛型函数完全不检查，同一错误可能在每个实例报一次；结构体字面量要写类型参数（结构体自己的方法里除外）；
  没有泛型枚举、方法自己的类型参数、`Stack[int].make()` 这类在实例上的静态调用。
- 浮点：`%` 和位运算不支持；`1 + 1.5`（两个字面量）报类型不符；Windows/macOS 上的浮点打印未在真机验证（依赖各自 libc 的 `snprintf`/`strtod`）。
- 切片：没有 `x[a..]`、`x[..b]`；v1 本来就没有可变切片。
- 闭包：只能直接作为 `sortBy`/`update` 的实参。
- 元组：不能给元素赋值、不能比较、不能整体打印、不能做 `Map` 键、没有元组模式。
- 插值 `${...}` 里仍不能写字符串字面量；`"${b}"` 对 `byte` 打印数字（速查卡没写，冷启动实验里 agent 靠猜绕开）。
- 堆块检查发现不了释放后使用；机器上没有 ASan。Windows、macOS、aarch64 产物只验证了文件格式。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

当前不支持 31 项、未运行 14 项（统计只算每个测试的第一个卡点）：

| 卡点 | 测试 |
| --- | --- |
| 第 6 项：`*p`、写出 `&inout T`、`let p: &T = &x` | T-G01、T-G02、T-G03、T-G04、T-G17、T-G18、T-G21、T-GR11（共 8 项，是 P1/P2 剩下的主体） |
| 实例上的类型参数表达式（`Name[T]` 作为值） | T-O5 |
| `String.fromUtf8` | T-GR4 |
| P3 并发：`scope`、`Channel`、`Mutex`、`with`、`Guard`、`+ Send` | 21 项 |
| `yavo fix` | 7 项（未运行） |
| `yavo verify` 报告 | 4 项（未运行） |
| 运行器：`run-allowed`（T-GR9）、`run-exit`（T-S3）、分配失败注入（T-GR8） | 3 项（未运行） |

## 9. 计划表

| 序号 | 任务 | 状态 |
| --- | --- | --- |
| 1–4、7 | 可选值、`Map`、导入、枚举与结果、拥有值字段 | 已完成（上一轮） |
| 5 | 方法、`Drop`、`Eq`/`Ord`、`interface`、`Box` | **已完成**（`Box` `6456fc8`、接口 `832b6db`） |
| 6 | `*p` 写入、写出 `&inout T`、`&x` 构造（A7、`E0004`） | 未开始；8 项一致性测试 |
| 8 | 浮点 `f64`、`totalCmp` | **已完成**（`16527cb`） |
| 9 | 切片 `x[a..b]`、`P-STR-BOUNDARY` | **已完成**（`e7e106f`，顺带 `toVec`、`reserve`） |
| 10 | 泛型函数和泛型结构体 | **已完成**（`bd8ab57`） |
| 11 | `yavo fix`（只自动应用不改变语义的修复） | 未开始；7 项 |
| 12 | P3：`scope`/`spawn`、channel、`Mutex`/`Arc`、`Send`/`Sync` | 未开始；21 项 |
| 13 | `yavo verify` 与验证报告 | 依赖 12；4 项 |
| 冷启动实验 | 只给速查卡写 T01–T10 | **已做一轮**：10/10 一次通过（局限见实验 README） |
| 待定 | 三平台 CI | 需要远端仓库，用户暂不推 |

“可用”三批（2026-11-19 与用户约定）：

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| 第一批 | 第 4、7 项，标准库 I/O 与 `os.args`，`format`/`panic`/`assert`/`min`/`max`，字符串方法 | 已完成，一致性 44 → 54 |
| 第二批 | 第 5 项，闭包，第 10 项泛型，元组 | **已完成**，一致性 54 → 75 |
| 第三批 | 第 8 项浮点，第 9 项切片，冷启动实验 | **已完成**，一致性 75 → 84；冷启动 10/10 |

## 10. 下一个会话的第一步

1. 设置第 2 节的环境变量，`cargo build --locked --offline --release`；跑 `cargo test --locked --offline`（和 `--release`），确认全部通过，conformance 是 84 / 0 / 31 / 14。
2. 三批“可用”都已完成，**接下来做什么需要用户定**。可选方向（我的建议顺序）：
   - **补速查卡**（小、收益直接）：冷启动实验列出的 6 个缺口——泛型结构体例子、`main` 只能是 `fn main()` 及“辅助函数返回 `T ! E`、`main` 里 `catch` 后 `os.exit`”的标准写法、
     `byte` 插值打印数字、`join` 只收文本、`?T` 方法列表与 `if let ... else`、哪些 `Map` 方法收 `str` 键。改的是规范文档，先问用户。
   - **第 6 项**（`*p`、`&inout T` 值、`let p: &T = &x`）：P1/P2 剩下的 8 项一致性测试都卡在这里。
   - **第二轮冷启动实验**：换一组不点名特性的题、换一个模型做受试者、同时用 Rust 做对照并记 token，结论才更可信。
   - **第 12 项并发**：最大的一块（21 项），也是 `Send`/`Sync`、接口能力 `+ Send` 的前提。
   - `yavo fix`（7 项）可以和上面任一项并行。
