# Yavo v0.5：缺口关闭决议与交付说明

## 结果

基于本轮上传的 v0.4《规划》《语义表》《速查卡》和 76 项测试，形成 v0.5 冻结候选。原始文件未覆盖；本版改动均可在 patches 中查看。README 与语义表的清单合并后有 11 个不同事项；其中“线程临时切片”已在 v0.4 速查卡改正，是 README 的过时记录，本轮主要补回归测试。

新增 53 项测试，共 129 项；清单/路径/诊断行/夹具/期望文件已做结构校验。没有 Yavo 编译器，因此所有 Yavo 解析、check、fix 和运行均为 not_run；本版是实现依据，不是已经实现或证明安全的语言。

## 一、两份清单逐项决定

| 项 | v0.5 决定 | 定位与测试 |
| --- | --- | --- |
| 1 标量借用写入 | 用 `*p = v` / `*p += v`；`p = q` 只重新绑定引用。`inout` 形参是位置别名，无需额外解引用 | A6/A7；T-G01—T-G04 |
| 2 可选模式 | `some(x)` / `none`，显式构造 some；`match take(&opt)` 消费，直接 match 已有位置只借用 | C5/C7；T-G05—T-G07 |
| 3 Vec 到切片 | 允许在只读切片形参位置自动 `Vec[T] → []T`，不分配、不复制元素；来源跟底层数据，不跟临时描述符槽位 | O6、白名单；T-G08/T-G09 |
| 4 用户析构 | `struct X: Drop` + `fn drop(inout self)`；先用户 drop，再字段声明逆序清理。禁止手调 drop、整体替换 self；有 Drop 不 Copy | V10/V11；T-G10—T-G13、T-GR2 |
| 5 并发库 | 固定 Mutex/Guard/Channel/SendError/Join 的所有权、来源、关闭、失败和线程能力契约 | 库契约第 7 节；T-GP1—T-GP7 |
| 6 库细节 | remove 返回拥有的 ?V；update 消费键且回调恰好一次，限不带借用的 Copy 值；i64.min/max；Ordering.Less/Equal/Greater；fs.Error.Utf8(offset) | 库契约第 1/3/4/5 节；T-G25/T-G26、T-GR3/T-GR4/T-GR10 |
| 7 临时切片线程参数 | `spawn f(v[a..b])` 可用，前提是 v 活过 scope；`spawn f(makeVec()[...])` 拒绝。删除旧清单误报 | P7；T-GP1/T-GP2；原 T-P3f 保留 |
| 8 Box/字段部分移动 | 对非 Copy 字段与 Box 内容一律禁止直接部分移动；用 take/replace/swap。仅完整拥有临时值的可选/结果/枚举/元组解包按 C5 允许 | C8；T-G14/T-G15；原 T-F2 补完整候选要求 |
| 9 常量可知失败 | 已定为有效表达式的失败仍在其执行路径 panic；可折叠为 panic，不可改成默认编译错误。字面量本身无法定型仍编译报错 | N1/N2；T-G22/T-G23、T-GR5/T-GR6/T-GR12 |
| 10 catch 不用错误 | `catch _` 合法，处理体开始前丢弃错误；`catch _e` 保留到处理体结束。自动修复不把 `_e` 改 `_` | V12；T-G16、T-GR1 |
| 11 fix 临时变量命名 | `vLen`；冲突取 `vLen2`、`vLen3`。未使用变量取 `_name`、`_name2`；不能捕获现有名字，重复运行幂等 | V13；T-GF2/T-GF3 |

## 二、同步关闭的规划问题

### 遍历的默认权限

固定为 `for x in xs` 只读，`for x in &xs` 可变，不因为 xs 被声明为 var 而自动借成可写。Map 的键始终只读。这个决定改变了 v0.4 的 A2，已同步修改语义表、速查卡和 T-C7/T-A4；if let/match 对已有位置的权限规则没有顺带改掉。

这样普通读循环可以调用集合的只读方法；需要写入的循环把排他性写在入口。新增 T-G17—T-G21 和 T-GR11。

### C 方言与支持策略

默认 C17，独立 Clang 下限 16、GCC 下限 12。发布仍使用完整且固定的 Zig 工具链；Apple Clang、Zig、SDK、CRT 按实际版本/内容锁和能力探针认证，不能仅凭名称推断。debug -O0、release -O2，关闭 fast-math 和浮点收缩；保持溢出检查和安全检查。默认参数及平台例外见 TOOLCHAIN-POLICY.json。

本轮只实际运行了 Linux x86-64 的 Clang 17、GCC 14.2，各 O0/O2，共四项 C17 能力探针，均通过；没有验证最低版本 16/12，也没有执行 Windows/macOS 探针。release_zig_lock 和 sdk_locks 保留 null，不能伪造下载哈希。

### 评测题量

设计流程固定：先用不参与正式评测的 60 道配对任务估计不一致率；正式独立任务数按功效公式、预实验上界和所选区间方法重新确认，规划起点不低于 400 道。默认例子 q=0.15 为假设而非测量。配对任务是统计单位，重复 seed 不膨胀独立样本数；开正式实验前锁定样本和分析规则。工具只给近似题量，不宣称实测功效。

## 三、测试与验证机制修正

- 129 项分布：check 76、fix 7、run 21、concurrency 21、report 4；P1 83、P2 21、P3 25。
- MANIFEST 不再只列“期望诊断码”，还带主诊断行、预期 stdout、panic、固定文件、候选、夹具及源码哈希。
- 原 T-F2 中 let Box 的 take 候选只有同时修改为合法可写位置才成立；borrow 候选也必须包含被调函数及调用处的完整修改。候选分别应用后重新 check，不能只返回三个好听的名字。
- 原 `sanitize: address, thread` 改为两个独立配置：`address+undefined; thread`。Linux Sanitizer 工件允许采用动态 libc 验证配置，但不能冒充 musl 静态发布工件。
- P-OOM 与 P-CAPACITY 分开；OOM 测试采用仅限测试运行器的可控故障注入，注入器未实现就 not_run，不能消耗宿主全部内存来测。
- 新增 run-allowed，仅用于规范允许“有效排列或指定比较器 panic”的测试；未知崩溃、超时和 Sanitizer 报错一律失败。
- package 校验器做过 8 项破坏性自检：重复 ID、过时行号、缺固定文件、夹具损坏、未知错误码、混用 Sanitizer、孤儿源文件、伪造执行通过；8 项均被拒绝。

## 四、没有沿用的无来源断言

旧规划中“当前 Rust stable 为 1.99.1”的时效性断言未带可核验发布证据。本轮没有把它写成新事实，也没有擅自换成另一个版本号；保留原 1.75 试点数据，正式对照改为开测前核验并锁定的 stable。历史 Gojo/性能数值仍作为用户提供的试点记录，不作为本轮复测结果。

## 五、交付与运行

```bash
python3 tools/check_package.py
python3 tools/selftest_package.py
python3 tools/probe_c_toolchains.py --compiler clang --compiler gcc
python3 tools/plan_evaluation.py
# 已有预实验数据时：
python3 tools/plan_evaluation.py --pilot-total 60 --pilot-discordant 9
```

前三个工具各自验证元数据、校验器、C 能力；它们都不是 Yavo 编译器。真正的验收仍须运行 yavo check/fix/build/test/verify。报告已经区分 passed/not_run，不能把结构自检的 passed 抄成语言测试通过。

P0 继续完成真实工具链锁和三平台最小链路；P1 使用本版语义和 check/fix 清单实现。不再把上述 11 项作为待定语法来回选择。实现若发现反例，新增编号和回归测试后做版本化修正，不声称本版穷尽所有未来缺陷。

## 六、2026-11-21 第三轮 agent 实验后的语言决定

依据：`experiments/coldstart-2026-11-21/README.md`（Yavo 组 8 次有效编译失败中，下列规则直接或间接造成 5 次）。用户同意按开发者建议执行；冻结测试包不改，被推翻的旧期望在 `compiler/tests/conformance_superseded.txt` 中逐条登记。

| 项 | 决定 | 定位 |
| --- | --- | --- |
| 12 两阶段借用 | 修改接收者（变量或其字段）的调用和下标赋值中，接收者在实参求值期间只被预留：实参可读，不可修改、移动，值不能在调用开始后仍指向它（E0403）。推翻 v0.5 的“不做两阶段借用”；T-V1、T-GF2 登记为 superseded | V2、V13；`errors_two_phase_defaults.yv` |
| 13 `String`/`str` 比较 | 比较运算符两侧可以分别是 `String` 和 `str`，按文本比较；不改变类型、泛型约束和 Map 键 | K1 |
| 14 字段默认值 | `field: T = 常量`，限数值、布尔、字节、无插值字符串字面量（`String` 字段）、`none`、`[]`、`[:]`；每次构造重新求得 | V6a |
| 15 栈溢出 | 新 panic 类别 P-STACK，行列 `0:0`，尽力写出缓冲输出；不运行用户代码的 `Box` 内容按工作列表释放，不随嵌套深度消耗栈 | V14；ERRORS.md 补记 |
| 16 下标赋值 | 规范 C2 早已允许，这次补齐实现：先求新值，再求下标与越界检查，再析构旧元素并写入 | C2、V5 |
