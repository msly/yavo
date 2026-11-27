# Selfhost 子集选题：完整语法图、阻塞集合与兑现收益

批次：52。原语料冻结于第 51 批 `61b17c4`；最终扫描使用第52批核心自举的实际 stage2 和 batch52 profile。原语料保持 622 个路径；stage0 接受 394 个，selfhost 接受 125 个，覆盖未变。新分析命令不参与正式编译，也不改变 `check` 的首错及退出行为。

## 工具与证据

`subset_sweep.py` 对同一入口先执行 stage0/selfhost 的真实检查，再调用 stage0 的只读 `subset-features`。后者复用 `Project::load`、完整 AST 和已检查的 HIR；跨越 selfhost 的首个 U0001，访问全部声明、函数体、模块、模式和已实例化类型。未调用的普通函数也会检查；未实例化泛型函数的语法体仍访问，不虚构其尚未实例化的类型。

每个原入口记录：

- 原源码身份、完整模块图身份、实际 stage0/selfhost 结果。
- 所有**当前模型已识别能力**的集合及源文件/行列证据；模型未穷尽所有结构限制。
- `unknown` 中的未决条件；未建模的结构、借用/移动/来源追踪以及实现容量限制分别保留 unknown。
- 路径数、入口字节去重数、完整模块图去重数。现有样本后两者碰巧相同，代码仍分别计算；集成测试包含“入口相同、依赖不同”的反例。

**本批实现的是完整语法/类型图遍历和已识别阻塞集合，还没有做到所有 selfhost U0001 的穷尽诊断。** 269 个未接受入口保留结构与 flow-sensitive unknown。不得把 `ast_complete=true` 改写成“全部语义阻塞已知”；`semantic_complete` 仍为 false。8 个入口的首个阻塞本身就是未建模的视图来源或拥有元素下标限制，原诊断完整保留。下一步应逐类复用/抽取正式检查器的能力判定，不能维护一份会漂移的关键词清单。

收益排序采用当前模型的候选估计：设入口已识别特征为 B、计划能力为 F，只有 B ⊆ F 才进入 modeled_candidates。由于模型仍可能漏报或误报，这不是严格上界，更不是实际新增覆盖。只有用实现后的实际 selfhost 对原字节复验才能回填 confirmed_full_unlock；当前确认值全部为 0。即使 selfhost 已接受，raw 模型标记也保留并列入 profile_false_positives，不能清空来伪造校准成功。

运行方式：

```sh
python3 selfhost/subset_sweep.py \
  --selfhost PATH_TO_FINAL_STAGE2 \
  --stage0 PATH_TO_STAGE0 \
  --analyzer compiler/target/release/yavo \
  --corpus selfhost/tests/original-corpus.json \
  --json compiler/target/subset-sweep.json
```

默认 `--profile batch52` 包含十六进制/二进制字面量。回测第 51 批用 `--profile batch51`。新的只读 CLI 为 `yavo subset-features file.yv [--profile batch51|batch52]`。它加载源码而不执行用户程序、不生成 C、不修改输入。

622 路径的固定清单已入库；其中 30 份过去被 ignored 的 R5 原始副本纳入 git，保持原路径、原字节，clean CI 不再依赖服务器残留文件。新的成本实验样本不进入这个固定分母。

## Copy 元组 Vec 的真实选题范围

旧首阻塞统计为 22 路径 / 8 份入口。完整图的已知结构条件把单独实现 Copy 元组 Vec 的候选压到 **6 路径 / 2 份模块图**，均为 R5 的两份连通域程序；这是候选估计，尚不是已实现收益。

| 原始任务组 | 路径 / 图 | Copy Vec 之后仍存在的已知结构阻塞 |
|---|---:|---|
| R5 yavo-a、yavo-b T05 连通域 | 6 / 2 | 无其他已建模阻塞；借用等 unknown 仍需实现后验证 |
| R5 yavo-a、yavo-b T03 区间管理 | 6 / 2 | sortBy、闭包、序列 join |
| R5 yavo-a、yavo-b T06 并行编码 | 6 / 2 | scope、spawn、join 并发 |
| R4 T05 课程调度入口及其模块图 | 1 / 1 | 拥有型元组 Vec、sortBy、闭包、序列方法 |
| T-O5p 的三个历史版本 | 3 / 1 | 带捕获闭包与 sortBy |

另外三份首错为元组 Vec 的程序持有 String，属于拥有型元组，不计入 Copy 候选。拥有型字段的移动、部分消费、析构与借用不可由 Copy 路径直接外推。

| 待补能力组合 | 模型候选路径 / 完整图 | 解释 |
|---|---:|---|
| 泛型、接口、方法及显式泛型表达式 | 10 / 4 | 加上通用引用后更高；不是孤立泛型语法即可实现 |
| Optional/Result 模式 | 5 / 3 | 只含模式组合；拥有载荷另计 |
| 完整元组组合 | 7 / 3 | 包括 Vec、拆构、组合字段；闭包/排序仍排除 |
| Copy 元组 Vec | 6 / 2 | 上表两份 T05 |
| 用户 enum payload + 通用 match | 0 / 0 | 原语料同时使用方法、Optional/Result、拥有载荷等，必须按组合交付 |

这些数字限制“下一批可增加多少覆盖”的承诺，不改变通用特性的优先方向。先建立共用 enum/pattern 表示，再按组合验收用户 enum、Optional、Result；避免为了一个计数再扩标准 Error 的特殊分支。

## 标准 Error match 合流设计

当前语法已共用 `parser.matchExpression` 和 `MatchArm`，特殊逻辑集中在 `codegen.matchValue`：它只接受 `.StdError`，用 `standardVariantCount/standardVariant` 查询变体，并假定 payload 是单个 int，绑定调用 `declareErrorBinding`。通用化应在这条路径上扩展，不新增另一套 `match` 发射器。

1. 在类型注册表统一描述名义 enum 身份、变体标签、payload 类型和布局投影。fs/io/os/process Error 作为预定义 enum 描述符注册；Utf8Error 保持 Copy struct，作为普通结构载荷使用。保留既有 Error ABI 的 tag/payload 布局，但不保留独立的匹配算法。
2. `MatchArm` 的平面 payload token 列表改为模式节点树，统一表达 Wild、Bind、Case、Some/None、Ok/Err；所有模式经同一类型检查、覆盖检查及绑定计划。
3. 将 `declareErrorBinding` 替换为通用绑定计划的发射：Copy 值复制、只读已有 place 借用、拥有临时值移动。分支合流继续共用现有 moved/source state；未绑定拥有字段在规定作用域析构，防止双重释放和提前释放。
4. `matchValue` 接受共用判别形状，发射同一分支过程。Optional/Result 的 has/ok 表示通过标签/投影适配器进入同一过程；适配器只负责存储布局。
5. 首次通用 enum/match 交付必须让现有标准 Error 测试走共用路径；删去“仅 StdError”分支以及硬编码 payload int/arity 判断。核心验证覆盖原标准 Error 行为、用户 enum Copy/拥有载荷、名义类型不混用、穷尽性、单次求值及分支退出清理。重型内存门随该所有权里程碑运行。

## 本批验证与剩余工作

- 聚焦 Rust 集成测试 `subset_features`：11/11。覆盖跨函数继续、未实例化泛型体、完整模块图、源码不变、字符串/注释不误报、已支持数值辅助/Set、标准 Error 与用户 enum 分类、非法输入拒绝。
- 冻结原 622 路径：394 个 stage0 接受、125 个 selfhost 接受、228 个 stage0 拒绝；0 越界、0 超时、0 异常退出。
- 当前接受的 125 个入口：raw profile 误报 0。另对当前 run/panic 原样 fixtures 做只读校准，354/354 raw profile 无误报，3项需 source_prefix 组合，明确跳过；不执行C程序。
- 独立审查及校准修复了 Host/RemoveDir、some、Vec[int]{} 的误报，并补齐标准Error容器、byte/str排序、Set迭代、Vec[str] Result、Heap comparator调用等漏项的回归。
- 未决：正式 checker 的流敏感能力判定尚未可恢复收集，不能报告“全部 U0001 已穷尽”；ranking 仅作候选估计，不能作严格上界。
- 下一轮首先抽取来源/移动能力判断，按不可变分析状态收集所有约束；为每一项新增能力记录旧集合、实现后实际新接受集合及预测误差。若不能安全模拟，则保留 unknown，不改测试、不删源码、不继续损坏的 parser 状态。

逐入口集合和候选列表见 [批 52 阻塞索引](../compiler/validation/batch52-subset-blockers-2026-11-28.json)。CI 的实际最终 stage2 扫描会随核心自举产物一并保存为 `subset-sweep.json`。
