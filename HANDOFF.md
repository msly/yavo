# Yavo 交接文档（2026-11-18）

给下一个会话：先读完本文件，再按“第一步”开工。所有数字都是 2026-11-18 在本仓库实测的。

## 1. 一句话状态

规范 v0.5 已冻结（`docs/`、`yavo-tests/`）；Rust 写的编译器（约 5,800 行，含 C 运行时）已实现 P0 和 P1 的一部分：
结构体、`String`/`Vec[T]`、移动与析构、字符串视图与 `of` 来源、`&T` 引用、`for x in v`、`inout` 参数、六个目标的交叉编译。
规范一致性：**129 项中通过 29、判错 0、不支持 87、未运行 13**。全部提交只在本地，**按用户决定暂不推远端**。

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
| `tests/conformance.rs` | 读 `yavo-tests/MANIFEST.json`，校验源码哈希，归为通过/判错/不支持/未运行；**只在基线里的测试退化时失败**（棘轮）；规范文件出现 `E0020` 一律算判错 |
| `tests/alloc_check.rs` | 用 `YAVO_ALLOC_CHECK=1` 跑全部 `run_*.yv`：运行时统计堆块，泄漏或多释放就以 102 退出 |
| `tests/targets.rs` | 6 个交叉编译目标的文件头格式与架构、Linux 静态链接、本机可运行目标的输出、错误路径 |

常用命令：`yavo check f.yv [--json-diagnostics]`、`yavo run f.yv [--release] [--target T]`、`yavo build f.yv -o out [--release] [--target T] [--emit-c f.c]`。

## 4. 已完成（按提交，最新在上）

| 提交 | 内容 | 一致性通过数 |
| --- | --- | --- |
| `c0f0ad7` | `&T`、`for x in v`、`inout` 参数、`v[i]` 取不可复制元素 | 29 |
| `8313043` | 字符串视图（`trim`/`fields`/`lines`）、`of` 来源、按最后一次使用的借用检查 | 24 |
| `31c2b0a` `5db1b75` | `String`/`Vec[T]`、移动与 `E0402`、析构、`sink` 参数、堆块检查 | 12 |
| `380fe97` | 标量结构体；release 默认去调试信息；许可证 Apache-2.0；`ERRORS.md` 登记前端错误码 | 9 |
| `ee40a29` | `U0001`（不支持）与语法错误分开；一致性测试；`--target` 交叉编译 | 9 |
| `76a4c22` 及之前 | 另一个 agent 完成的 P0 原型；规范 v0.1–v0.5 与 129 项测试 | — |

## 5. 架构速览

流水线：`lexer.rs` → `parser.rs`（AST 在 `ast.rs`）→ `check.rs`（类型、所有权、借用，产出 HIR）→ `codegen.rs`（生成 C17）→ zig cc。

- `types.rs`：`Ty` 是小的 `Copy` 枚举；结构体、`Vec[T]`、`&T` 通过线程局部表登记（`register_struct`、`vec_of`、`ref_of`）。`is_copy()` 决定移动还是复制，`carries_borrow()` 决定是否需要跟踪来源。
- 移动：检查器把被移动的局部变量改写成 `HExprKind::Move`；生成代码时复制后把源**清零**。空值和被移走的值都是全零，所以析构永远安全，不需要运行时标志。
- 析构：代码生成维护作用域栈，作用域结束和 `return`/`break`/`continue` 前逆序释放；只被借用的临时值（`f().len()`）在语句末释放。
- 移动检查：`FnCx::moved` 按控制流合并（`if` 两支取并集，提前离开的分支不算），循环体再检查一遍。
- 借用：每个局部变量有 `loans`（指向哪些变量，或临时值）。`Pre` 预扫描给语句编号并记录名字的使用位置和所在循环，`live_after` 据此判断视图是否还会被用到。循环里的使用会延长到循环结束。
- 运行时 `runtime/yavo_rt.h`：输出缓冲、panic、整数检查、`yv_string`/`yv_vec`、视图函数、`YV_ALLOC_CHECK` 计数。

## 6. 约定

- 错误码：规范错误 `E0xxx`（见 `yavo-tests/ERRORS.md`，前端码 `E0010`–`E0018`、`E0020` 已登记）；**合法但未实现的写法一律报 `U0001`，绝不能报 `E0020`**；警告 `W0001`。
- `tests/p0/errors_*.yv` 开头是 `// expect-error: CODE 行:列`。头部每多一行，正文行号整体加一；做法是先写无头文件，用 `yavo check` 取位置，再按头部行数加偏移生成头部。
- 一个特性做完：补 `run_*` 和 `errors_*` 两类测试；跑全部测试（debug、release）；跑堆块检查；交叉编译抽查；刷新一致性基线；更新 `compiler/README.md` 对应小节和根 `README.md` 的数字；本地提交。
- 已解锁的旧写法可能让旧测试失效（例如某行不再报 `U0001`），改测试时保持行列不变，换成仍然成立的用例。

## 7. 已知限制与保守之处

- 借用检查按语句粒度和语句顺序：`if` 里修改、只在 `else` 里用视图也会报错；`E0305` 不看存储目标之后是否还被使用。只会多报，不会漏报。
- 堆块检查发现不了释放后使用。机器上没有 ASan。
- Windows、macOS、aarch64 的产物只验证了文件格式，没有实际运行（没有真机和 CI）。
- 共享的 Claude 文档仍是 v0.4；以仓库 `docs/` 为准。

## 8. 一致性测试里的卡点

`conformance` 打印的“不支持”统计**只算每个测试遇到的第一个卡点**，做完一项后要重跑看新卡点。当前前几名：

| 卡点 | 测试数 |
| --- | --- |
| `use` 导入 | 14 |
| `?T` 可选类型（以及切片、元组类型） | 14 |
| `Map` 字面量 `[:]` | 11 |
| `struct X: Interface` | 7 |
| `enum` | 5 |
| 解引用 `*p` | 5 |
| 浮点数 | 4 |
| 切片 `x[a..b]` | 4 |
| `interface` 声明 | 3 |
| `Box` | 3 |

未运行 13 项：`yavo fix` 7 项、`yavo verify` 报告 4 项、需要夹具或特定平台 2 项。

## 9. 计划表

| 序号 | 任务 | 依赖 | 预计解锁 | 规范位置 |
| --- | --- | --- | --- | --- |
| 1 | `?T`、`some`/`none`、`if let`、`match`（先支持可选值）、`??`、`take`/`replace`/`swap` | 无 | C5、C6，并为 `Map` 铺路 | 语义表 C5、V9；速查卡第 3 节 |
| 2 | `Map[K, V]`、`[:]`、`get`/`getMut`/`getCopy`/`set`/`remove`/`update`、`str` 键查询、`for k, v in m` | 1 | A1、A4、A5、C7、C8 等 | C6、A2、A5；`docs/stdlib_contract.md` |
| 3 | `use std/...` 导入（先把标准库模块名映射到内建函数） | 无 | 14 项的第一卡点 | 速查卡第 5 节 |
| 4 | `enum` 与 `match`；`T ! E`、`try`、`catch`、`ok`/`err` | 1 | V4、N 类、错误处理用例 | 速查卡第 3 节，语义表 V8 |
| 5 | 结构体方法（`self`/`inout self`）、`interface`、`struct X: I`、`Box[I]`、`Eq`/`Ord` 协议 | 4 | 10 项左右 | 语义表 K1、K2、P2 |
| 6 | `*p` 写入、`&inout T` 值、`for x in &v`、`getMut` | 2 | 5 项左右 | 语义表 A1–A4 |
| 7 | `String`/`Vec` 结构体字段（字段析构、移出字段规则） | 1、6 | C4p、E 类 | C5、规则 8 |
| 8 | 浮点 `f64`、`totalCmp` | 无 | K1 | 语义表第 7、8 节 |
| 9 | 切片 `x[a..b]`（字符边界检查 `P-STR-BOUNDARY`） | 无 | S3、若干视图用例 | 语义表 S3 |
| 10 | 泛型函数和泛型结构体（`largest[T]`、`Wrapper[T]`） | 5 | O6、O7、E1 等 | 语义表 O1 |
| 11 | `yavo fix`（只自动应用不改变语义的修复） | 无 | 7 项 fix 测试 | 规划“修复分三级” |
| 12 | P3：`scope`/`spawn`、channel、`Mutex`/`Arc`、`Send`/`Sync` | 5、10 | 21 项并发测试 | 语义表第 5 节 |
| 13 | `yavo verify` 与验证报告 | 12 | 4 项报告测试 | 规划“验证报告” |
| 并行 | 冷启动实验：只给 agent 速查卡，让它写 T01–T10 | 1–4 之后更有意义 | 验证最大风险 | 规划“风险” |
| 待定 | 三平台 CI | 需要远端仓库，用户暂不推 | Windows/macOS 实际运行 | 规划 P0 |

## 10. 下一个会话的第一步

1. 设置第 2 节的环境变量，`cargo build --locked --offline --release`。
2. 跑 `cargo test --locked --offline`，确认全部通过；跑 conformance，确认是 29 / 0 / 87 / 13。
3. 读 `docs/semantics.md` 的 C5、V9 和 `docs/cheatsheet.md` 第 3 节，开始计划表第 1 项。
