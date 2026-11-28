# 第54批：表达式结果的借用来源与清理边界

stage0 现于表达式求值时保存借用来源，在 `match` arm、`catch` handler 和 `??` 右侧的生成 C 清理边界前核验结果。拥有绑定或分支临时值的视图即使直接传给 `println(...)` 也会被 E0305 拒绝；合法参数来源、外层活跃 owner 和完整语句内的临时借用保持可用。本批修改集中于 `compiler/src/check.rs`，没有扩大 selfhost 的借用语义范围。

[专项审计与逐项证据](../compiler/validation/batch54-borrow-provenance-2026-11-28.json)记录源码身份、34 种 AST 表达式、9 类 C 清理边界、修改操作的返回来源和实际诊断；整合结果见[本批验证索引](../compiler/validation/batch54-delivery-2026-11-28.json)。

## 修复机制

| 机制 | 约束与结果 |
| --- | --- |
| 求值来源快照 | `HExpr.result_loans` 保存该次求值的来源，后续状态恢复、分支合流和其他操作数求值不再重解释已求出的值。 |
| 活分支合流 | `match` 在 `pop_scope` 前保存结果；只有可到达且产出值的 arm 贡献结果来源和后续流状态。 |
| 内容来源与访问限制分开 | Copy 模式绑定的 `value_loans` 表示复制内容来源，既有 `.loans` 继续限制来源 place 的访问；既不把局部描述符误作数据 owner，也不提前解除借用。 |
| 清理前检查 | 共用 `branch_result` 仅核验 Match arm、catch handler、`??` RHS；结束作用域的透明 `RefMut` 描述符归一为底层来源，拥有存储、guard、临时值和仍活跃的权限约束保留。 |
| 修改操作返回旧值 | `replace` 在新值求值及写入前保存旧来源；take/pop/remove 保守保留接收者来源，swap 合并双方来源。 |
| inout 字段地址 | `InoutPlace` 即使 HIR 类型是 String 等非借用目标类型，也必须传播其地址来源；独立审查发现并补齐了这一早退路径。 |

`LocalId` 单调追加，退出作用域只删除名字可见性，不复用局部槽位。Tuple、Optional、Vec、Map、Result 中的借用由类型递归规则和结果快照保留。`??` 左值与 catch 输入的临时 owner 在分支清理标记前求值，仍活到外层完整语句结束，不与 RHS/handler 内部创建的临时 owner 混为一类。

## 全部表达式的审计范围

下表覆盖 `ast.rs::ExprKind` 的全部 34 个变体；专项 JSON 按源码顺序逐项列出，自动核对集合和顺序相等。

| AST 变体 | 来源规则或现有边界 |
| --- | --- |
| Int、Float、Byte、Bool | 不产出借用。 |
| Str | 普通字面量为 static；插值走打印/format 专门检查，format 返回拥有 String。 |
| Ident、Paren | 读取当前内容来源并冻结；括号保留原快照。 |
| Call、Generic | 函数/方法/接口依 `ret_of` 传播实参来源，内建视图追溯接收者；Generic 单独作为值仍拒绝。 |
| Field、Index、SliceRange、Borrow、Unary | 区分复制内容与借用存储，保留根 owner、指针底层来源和 Temp；数值/逻辑一元运算不产出借用。 |
| VecLit、MapLit、Tuple | 逐元素快照并合并递归借用来源，Map 包含键和值。 |
| StructLit、Case、Named | 普通结构/枚举字段递归含借用仍 E0301；Named 仅用于枚举载荷；内建空容器没有初始来源。 |
| Binary | 算术、比较、逻辑及拥有字符串结果不含借用；短路 RHS 清理后只返回 bool。 |
| None、Unit | none 无载荷来源；Unit 仅在既有合法上下文使用，不产出借用。 |
| Coalesce、Catch、Match | 保存活分支来源，按各自实际 C 清理边界检查。 |
| Return、Break、Continue | return 在退出前核验 `of`；跳转分支不贡献表达式结果，循环状态保持已有合流规则。 |
| Closure | 独立闭包值仍 U0001；直接回调按既有返回约束检查，保存的闭包捕获借用仍 E0307。 |
| Err、Try | Result 成功值传播来源；错误类型含借用仍 E0301，隐式错误返回不能绕过来源检查。 |
| Spawn | 线程结果须 Send 且拥有数据，借用结果仍 E0701；scope 保存参数借用。 |
| Block | 仅用于必须跳转的 match/catch 分支；真实语句块内 return 按正常来源规则检查。 |

`if` 表达式不在 AST 中，独立块值也未实现，两者是 E0020 语法边界，未计作借用安全负例。已支持的 if 语句和语句块分别有“局部拥有值视图逃逸”负例及参数来源返回正例。普通结构/枚举的借用字段、Box/共享载荷中的借用等现有语义边界也单列。

## 验证与合同迁移

| 验证 | 结果 |
| --- | --- |
| 新增 `borrow_provenance.rs` | 7/7 测试组；有效参数、外层 owner、拥有结果转移、重借用和临时值正例按 Debug/Release 实际运行，分配检查开启，stdout/stderr 严格匹配。 |
| 既有 `mutref_returns.rs` | 6/6 测试组，覆盖引用返回、转发、移动、重新绑定、复杂接收者、字段、guard 和独占访问。 |
| 相关 P0 诊断 | 17 个入口的 code、行、列及警告严格一致；新增文件仅 `errors_expression_loans.yv`，run/panic 数量及 `p0.rs` 不变。 |
| patterns53 参考检查 | 59/59，当前 32 run / 27 reject。 |
| 阻塞分析合同 | 旧断言仍要求接受 escaping_view，在 CI 被正确的 E0305 拒绝触发失败；三条原例改为严格拒绝合同，合法 borrow_return 保留结构模型不完整性正控制，定向复验15/15。 |
| 本机 Core | 90/90 用例、13/13 检查；固定点与二进制重现通过；校准 399 passed / 0 failed / 3 skipped。 |

第53批保留的三个 stage0 漏检原例本批从 `unsupported` 安全重分类为 `reject`：

| 原例 | stage0 实际诊断 | selfhost 实际诊断 | 说明 |
| --- | --- | --- | --- |
| `escaping_view` | E0305 | U0001 | stage0 在 arm 拥有绑定释放前拒绝结果；selfhost 仍为保守边界。 |
| `escape_local_view` | E0303 | U0001 | stage0 拒绝返回指向函数局部 String 的视图；selfhost 仍为保守边界。 |
| `borrow_wrong_source` | E0303 | E0303 | 返回来源未列入 `of`，双方拒绝。 |

三个原文件与上批源码逐字节一致，历史回执保持原样；两份清单显式记录独立的 `reference_diagnostic`。固定原语料仍为 622 路径、stage0 接受 394、selfhost 接受 135；完整模块图仍为 224/68。与 batch53-final 逐路径比对，stage0 与 selfhost 接受集合的新增/移除均为空。这三条模式夹具重分类不改变原语料分母，也不计作功能覆盖收益。
