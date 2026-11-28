# 第 53 批：通用枚举、模式与拥有值

`cases.json` 的 59 项固定源码分为 32 个运行用例、24 个拒绝用例、3 个 stage0 已接受但 selfhost 保守拒绝的借用缺口用例。输入与输出在新候选结果生成前编写；stage0 独立验证预期，不用候选输出反写 oracle。总 `selfhost/tests/cases.json` 合入相同片段，由常规 bootstrap 执行。

## 语义范围

- 用户枚举的无载荷、多载荷、Copy/拥有值、位置/命名/混合参数、按源码顺序求值；结构字段、Vec、参数、返回和打印。
- 同一 match 路径检查用户 enum、标准 Error、Optional、Result 和标量 Wild/Bind。嵌套多字段模式的覆盖矩阵、67 变体、空枚举、缺失分支、名义类型、字段数量与绑定作用域分别有正负例。
- 重复/遮蔽分支在 stage0 是 W0002 警告；本批要求合法输入仍可接受，不能把警告改成错误。
- 已有 place 的非 Copy 绑定只读借用，不能移走 payload；拥有临时值的绑定可以移动。分支返回、循环 break/continue、部分字段丢弃及借用来源合流有专门用例。
- `source_mutation` 的原前端诊断为 E0102。selfhost 当前以 U0001 拒绝绑定作用域内修改来源，保留原有保守活跃期边界。

## 析构时序观察器

`drop_before_body.yv` 除了使用生产 runtime 的正常运行，还通过 `selfhost_patterns.py` 生成一份只增加观测的 runtime。进入三个分支体、写出标记时，`yv_live_blocks` 必须分别为：

| 标记 | 活跃块 | 条件 |
| --- | ---: | --- |
| `drop0` | 0 | 拥有临时 enum 的 String 与 Vec 都由通配符丢弃，进入分支体前已释放 |
| `keep1` | 1 | String 转移到绑定，未绑定 Vec 已释放 |
| `borrow2` | 2 | 已有 place 被借用，String 与 Vec 都仍存活 |

观察器要求三个标记全部执行；错误计数或缺少标记退出 103。正常退出仍检查 `YV_ALLOC_CHECK`。生产 runtime 文件不修改，观察版仅写入验收临时目录。Debug/Release 分开执行；该检查不取代 milestone sanitizer。

## stage0 借用来源缺口

stage0 `compiler/src/check.rs::loans_of` 尚未传播 `HExprKind::Match` 返回值的来源。下面三个输入被 stage0 接受，本批保持原稿并让 selfhost 拒绝：

| 用例 | stage0 实际行为 | selfhost |
| --- | --- | --- |
| `escaping_view` | 返回拥有临时 enum 中 String 的切片；生成 C 在返回 view 之前 drop 对应 String | U0001 |
| `escape_local_view` | local String 的切片经 match Bind 返回 `str of static`，来源未进入结果检查 | U0001 |
| `borrow_wrong_source` | 返回参数 a 的切片，但函数只声明 `of b` | E0303 |

`escape_temporary_view` 不属于 stage0 缺口：临时 String 切片直接进入 match Bind 时，stage0 已在绑定阶段报 E0305，selfhost 以 U0001 保守拒绝 transient scrutinee。

后续修复 stage0 时，应在每个分支作用域仍存活时保存/核验结果来源，再合并可到达分支；拥有分支局部值的来源不能冒充函数参数来源。单纯对已经退出作用域的 arm HIR 递归调用 `loans_of` 还需核对历史 loan 状态。

## 原稿运行验收

`cases.json.originals` 指向 7 个原路径：T-S2 的三个版本、T-GR4 的两个版本、P0 `run_enums.yv` 和 R3 T02 编辑器。T-S2 在原目录读取原 `T-S2.input.bin`；T02 使用既有 `tasks/T02.in` 与 `tasks/T02.out`。源码与 I/O 文件逐个记录身份，执行后确认没有变化。T-C5 的三个版本按原 `check-pass` 契约由固定语料扫描验证。

```sh
python tools/ci/selfhost_patterns.py --bootstrap-output compiler/target/ci/selfhost --originals
python tools/ci/selfhost_patterns.py --bootstrap-output compiler/target/ci/selfhost --case patterns53_drop_before_body
```

两条命令读取通过的 bootstrap receipt，使用真实 stage2 与同一锁定 Zig；报告分别在 `patterns53/originals/receipt.json` 和 `patterns53/focused/receipt.json`。常规 bootstrap 已运行全部语义用例，这两条只补原稿运行与析构时序。
