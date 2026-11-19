# Yavo 交接文档（2026-11-19）

给下一个会话：先读完本文件，再按“第一步”开工。所有数字都是 2026-11-19 在本仓库实测的。

## 1. 一句话状态

规范 v0.5 已冻结（`docs/`、`yavo-tests/`）；Rust 写的编译器（约 11,300 行，含 C 运行时）已实现 P0 和 P1 的大部分单线程写法：
结构体（字段可以是 `String`/`Vec`/`Map`/枚举等拥有值）、`enum`、`T ! E` 与 `try`/`catch`、带嵌套模式的 `match`（语句和表达式）、
`String`/`Vec[T]`/`Map[K, V]`、`[]T` 切片类型、移动与析构、字符串视图与 `of` 来源、`&T`、`&inout T` 值、`for`、`inout` 参数、可选值 `?T`、
`panic`/`assert`/`format`/`min`/`max`/`abs`、字符串与序列方法、`io.readAll`/`io.readLine`、`fs.readText`/`fs.writeText`、`os.args`、六个目标的交叉编译。
规范一致性：**129 项中通过 54、判错 0、不支持 62、未运行 13**。全部提交只在本地，**按用户决定暂不推远端**。

用户与我约定的“可用”标准：能用 Yavo 写出常见命令行小程序并在 Linux 上跑通，分三批完成（见第 9 节）。**第一批已完成**。

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
| `feat(p1): standard library ...` | `format`/`min`/`max`/`abs`/`String{}`、字符串方法（`contains`/`startsWith`/`endsWith`/`find`/`split`/`toLower`/`toUpper`/`parseInt`）、`v.contains`/`join`/`sort`、`[]T` 切片类型、`io.readAll`/`readLine`、`fs.readText`/`writeText`、`os.args`（启动时 UTF-8 检查）；一致性运行器支持夹具 | 54 |
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

## 6. 约定

- 错误码：规范错误 `E0xxx`（见 `yavo-tests/ERRORS.md`，前端码 `E0010`–`E0018`、`E0020` 已登记）；**合法但未实现的写法一律报 `U0001`，绝不能报 `E0020`**；警告 `W0001`。
- `tests/p0/errors_*.yv` 开头是 `// expect-error: CODE 行:列`。头部每多一行，正文行号整体加一；做法是先写无头文件，用 `yavo check` 取位置，再按头部行数加偏移生成头部。
  现在用 `tools/expect_header.sh compiler/tests/p0/errors_x.yv` 自动完成（去掉旧头部、重跑 `yavo check`、按头部行数加偏移写回）；它记录的是编译器当前的输出，生成后要逐条核对。
- 解析阶段的 `U0001`（parser 报的）会让整个文件不再进入检查器，所以它们只能和别的解析期 `U0001` 放在一起（`errors_unsupported.yv`），不能和检查器的错误混在一个测试文件里。
- 一个特性做完：补 `run_*` 和 `errors_*` 两类测试；跑全部测试（debug、release）；跑堆块检查；交叉编译抽查；刷新一致性基线；更新 `compiler/README.md` 对应小节和根 `README.md` 的数字；本地提交。
- 已解锁的旧写法可能让旧测试失效（例如某行不再报 `U0001`），改测试时保持行列不变，换成仍然成立的用例。

## 7. 已知限制与保守之处

- 借用检查按语句粒度和语句顺序：`if` 里修改、只在 `else` 里用视图也会报错；`E0305` 不看存储目标之后是否还被使用。只会多报，不会漏报。
- 可选值：对已有位置的 `if let`/`match` 绑定一律只读；规范允许通过可写位置的绑定写入（如 T-A3 的 `n.size += 1`），这里报 `U0001`。可复制的载荷虽然按值复制，仍算作借用原位置（保守）。`a ?? b` 把 `a` 整体消费（移动）。`?? return/break/continue` 只允许作为整个 `let` 初值或赋值右侧。
- `&inout T`：只能由 `getMut` 和 `for … in &c` 产生；还不能写出这个类型、不能 `&x` 构造（A7）、不能 `*p = v`，不能放进集合，也不能通过它存入借用值（都报 `U0001`）。独占访问检查：`&inout` 之后还会被用到时，原变量的任何使用都报 `E0102`（按“最后一次使用”判断，同样只会多报）。
- `Map`：键只支持整数、`bool`、`str`、`String`（`Id`、元组键未做）；`update` 需要闭包，报 `U0001`。修改方法（`set`/`remove`/`getMut`/`push`/`pop`/`sort`）的接收者可以是变量或其字段链。
- 导入：已实现的模块成员只有 `os.exit`、`os.args`、`io.readAll`/`readLine`、`fs.readText`/`writeText` 和类型 `io.Error`/`fs.Error`，其余报 `U0001`。`os.exit` 不运行析构，所以放在 `run_*` 测试末尾会让堆块检查失效（它只在 `yv_rt_exit` 里检查）。
- 枚举和结果：不支持字面量模式（`.Num(0)`、`1 =>`）、模式里的命名字段、`if let` 带模式、比较枚举、泛型枚举、结构体里的结果字段。`catch e => return` 只能作为整个 `let` 初值、赋值右侧或语句。表达式形式的 `match` 分支和 `catch e => 值` 里的绑定，按“同一语句内的后续使用都算”处理，只会多报。
- 标准库：`toLower`/`toUpper` 只改 ASCII 字母；`split("")` 返回整个字符串；`sort` 只支持整数、`str`、`String` 元素；`sortBy`、`update`、`Heap` 等等闭包。插值 `${...}` 里仍不能写字符串字面量（原有限制，我自己写测试时就踩了好几次，冷启动实验时值得关注）。
- 堆块检查发现不了释放后使用。机器上没有 ASan。
- Windows、macOS、aarch64 的产物只验证了文件格式，没有实际运行（没有真机和 CI）。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

`conformance` 打印的“不支持”统计**只算每个测试遇到的第一个卡点**，做完一项后要重跑看新卡点。当前前几名：

| 卡点 | 测试数 |
| --- | --- |
| `struct X: Interface`（含 `Drop`） | 9 |
| `scope`（线程） | 7 |
| 解引用 `*p` | 5 |
| `Box` | 5 |
| `with`（锁守卫块） | 4 |
| 浮点数 | 4 |
| 泛型结构体字面量（如 `Channel[int]{cap: 2}`） | 4 |
| 切片 `x[a..b]` | 4 |
| `interface` 声明 | 3 |
| 写出 `&inout T` 类型 | 2 |
| `Mutex` | 2 |
| 泛型函数 | 2 |

另外 T-A3/T-A3f 卡在 `Box`，T-C7、T-GR9 卡在闭包（`update`、`sortBy`），T-D1、T-GR1、T-G12 卡在 `struct X: Drop`（第 5 项）。原来卡在导入的 14 项里，12 项是 `std/sync` 的并发用例，现在卡在 `Mutex`、`Channel`、`with` 等第 12 项的内容上。

未运行 13 项：`yavo fix` 7 项、`yavo verify` 报告 4 项、运行器还不支持的 `run-allowed`（T-GR9）和 `run-exit`（T-S3）各 1 项。`run-exit` 需要用平台原生方式传入非法参数，`tests/stdlib_io.rs` 已在 Unix 上验证同样的行为。

## 9. 计划表

| 序号 | 任务 | 依赖 | 预计解锁 | 规范位置 |
| --- | --- | --- | --- | --- |
| ~~1~~ | ~~`?T`、`some`/`none`、`if let`、`match`（先支持可选值）、`??`、`take`/`replace`/`swap`~~ **已完成**（新增通过 T-C5、T-G05、T-G06、T-G07、T-G24） | 无 | C5、C6，并为 `Map` 铺路 | 语义表 C5、V9；速查卡第 3 节 |
| ~~2~~ | ~~`Map[K, V]`、`[:]`、`get`/`getMut`/`getCopy`/`set`/`remove`/`update`、`str` 键查询、`for k, v in m`~~ **已完成**，`update` 等闭包（新增通过 T-A1、T-A1p、T-A4、T-A4p、T-C1、T-C1p、T-C7f、T-C8、T-C8f、T-GR10） | 1 | A1、A4、A5、C7、C8 等 | C6、A2、A5；`docs/stdlib_contract.md` |
| ~~3~~ | ~~`use std/...` 导入~~ **已完成**（模块内容只有 `os.exit`；导入用例转到各自的下一个卡点） | 无 | 14 项的第一卡点 | 速查卡第 5 节 |
| ~~4~~ | ~~`enum` 与 `match`；`T ! E`、`try`、`catch`、`ok`/`err`~~ **已完成**（`899195a`） | 1 | V4、N 类、错误处理用例 | 速查卡第 3 节，语义表 V8 |
| 5 | 结构体方法（`self`/`inout self`）、`interface`、`struct X: I`、`Box[I]`、`Eq`/`Ord` 协议 | 4 | 10 项左右 | 语义表 K1、K2、P2 |
| 6 | `*p` 写入、写出 `&inout T` 类型并用 `&x` 构造（A7、`E0004`）；（`&inout` 值、`for x in &v`、`getMut` 已随第 2 项完成） | 2 | 5 项左右 | 语义表 A1–A4、A7 |
| ~~7~~ | ~~`String`/`Vec` 结构体字段（字段析构、移出字段规则）~~ **已完成**（`899195a`，未依赖第 6 项） | 1、6 | C4p、E 类 | C5、规则 8 |
| 8 | 浮点 `f64`、`totalCmp` | 无 | K1 | 语义表第 7、8 节 |
| 9 | 切片 `x[a..b]`（字符边界检查 `P-STR-BOUNDARY`） | 无 | S3、若干视图用例 | 语义表 S3 |
| 10 | 泛型函数和泛型结构体（`largest[T]`、`Wrapper[T]`） | 5 | O6、O7、E1 等 | 语义表 O1 |
| 11 | `yavo fix`（只自动应用不改变语义的修复） | 无 | 7 项 fix 测试 | 规划“修复分三级” |
| 12 | P3：`scope`/`spawn`、channel、`Mutex`/`Arc`、`Send`/`Sync` | 5、10 | 21 项并发测试 | 语义表第 5 节 |
| 13 | `yavo verify` 与验证报告 | 12 | 4 项报告测试 | 规划“验证报告” |
| 并行 | 冷启动实验：只给 agent 速查卡，让它写 T01–T10 | 1–4 之后更有意义 | 验证最大风险 | 规划“风险” |
| 待定 | 三平台 CI | 需要远端仓库，用户暂不推 | Windows/macOS 实际运行 | 规划 P0 |

“可用”分三批（2026-11-19 与用户约定）：

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| 第一批 | 第 4、7 项，标准库 I/O 与 `os.args`，`format`/`panic`/`assert`/`min`/`max`，字符串方法 | **已完成**，一致性 44 → 54 |
| 第二批 | 第 5 项（方法、`interface`、`Box`、`Drop`、`Eq`/`Ord`），闭包（`sortBy`、`update`，规则 O5），第 10 项泛型，元组 | 未开始 |
| 第三批 | 第 8 项浮点，第 9 项切片 `x[a..b]`，冷启动实验（只给速查卡，写一组典型任务） | 未开始 |

判断“可用”看能否写出真实程序和冷启动实验结果，不看一致性数字：剩下的大头是 21 项并发和 11 项 `fix`/`verify`，不在可用范围内。

## 10. 下一个会话的第一步

1. 设置第 2 节的环境变量，`cargo build --locked --offline --release`。
2. 跑 `cargo test --locked --offline`，确认全部通过；跑 conformance，确认是 54 / 0 / 62 / 13。
3. 开始第二批，先做第 5 项：结构体方法（`self`/`inout self`、`of self`）、`struct X: Drop`（V10、V11，`E0801`–`E0803`；T-D1、T-GR1、T-G12 等着它），再做 `interface`、`struct X: I`、`Box[I]`、`Eq`/`Ord`。方法调用的接收者可以复用 `mutable_place`（修改方法）和 `storage_loans`（借用来源）。
4. 然后做闭包：先只支持库已知不逃逸的回调（`sortBy`、`update`），再按规则 O5 处理会被保存的闭包；最后第 10 项泛型和元组。
