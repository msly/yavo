# 第 53 批：通用枚举、模式与拥有值

`cases.json` 的 59 项固定源码在第 53 批分为 32 个运行用例、24 个拒绝用例、3 个 stage0 已接受但 selfhost 保守拒绝的借用缺口用例。第 54 批修复 stage0 后，后 3 项安全重分类为拒绝用例，现为 32 个运行用例、27 个拒绝用例；源码、selfhost 诊断和第 53 批冻结回执均保持原样。输入与输出在候选结果生成前编写；stage0 独立验证预期，不用候选输出反写 oracle。总 `selfhost/tests/cases.json` 合入相同片段，由常规 bootstrap 执行。

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

## 第 53 批 stage0 缺口与第 54 批安全重分类

第 53 批的 stage0 `compiler/src/check.rs::loans_of` 未传播 `HExprKind::Match` 返回值的来源。第 54 批在分支作用域退出前保存结果来源，并核验生成 C 的分支清理边界；下面三个原输入现由 stage0 正确拒绝，清单从 `unsupported` 迁移为 `reject`，独立记录 `reference_diagnostic`：

| 用例 | 第 53 批 stage0 漏检 | 第 54 批 stage0 | selfhost 保持原诊断 |
| --- | --- | --- | --- |
| `escaping_view` | 返回拥有临时 enum 中 String 的切片；生成 C 在外层使用 view 之前 drop 对应 String | E0305：分支结果跨越拥有绑定的清理边界 | U0001 |
| `escape_local_view` | local String 的切片经 match Bind 返回 `str of static` | E0303：返回值指向函数局部拥有值 | U0001 |
| `borrow_wrong_source` | 返回参数 a 的切片，但函数只声明 `of b` | E0303：返回来源不在声明集合中 | E0303 |

`escape_temporary_view` 不属于 stage0 缺口：临时 String 切片直接进入 match Bind 时，stage0 已在绑定阶段报 E0305，selfhost 以 U0001 保守拒绝 transient scrutinee。

结果来源在求值时形成快照，仅合并可到达且产出值的分支。Copy 模式绑定区分来源 place 的访问限制与复制视图的真实来源；指向外层数据的局部可变引用描述符在退出前归一为底层来源，真正的拥有存储与临时值仍受生命周期检查。`match` arm、`catch` handler 和 `??` 右侧会在外层消费者运行前清理，故即使直接 `println(...)` 也拒绝悬垂结果；外层活跃 owner、合法参数来源与完整语句内的临时借用仍可接受。这 3 项迁移是安全性修复，不计作功能覆盖增量，也不修改原语料的冻结分母。

## 原稿运行验收

`cases.json.originals` 指向 7 个原路径：T-S2 的三个版本、T-GR4 的两个版本、P0 `run_enums.yv` 和 R3 T02 编辑器。T-S2 在原目录读取原 `T-S2.input.bin`；T02 使用既有 `tasks/T02.in` 与 `tasks/T02.out`。源码与 I/O 文件逐个记录身份，执行后确认没有变化。T-C5 的三个版本按原 `check-pass` 契约由固定语料扫描验证。

```sh
python tools/ci/selfhost_patterns.py --bootstrap-output compiler/target/ci/selfhost --originals
python tools/ci/selfhost_patterns.py --bootstrap-output compiler/target/ci/selfhost --case patterns53_drop_before_body
```

两条命令读取通过的 bootstrap receipt，使用真实 stage2 与同一锁定 Zig；报告分别在 `patterns53/originals/receipt.json` 和 `patterns53/focused/receipt.json`。常规 bootstrap 已运行全部语义用例，这两条只补原稿运行与析构时序。
