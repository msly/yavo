# 第53批：通用枚举、模式与生成 C 体积

本批将标准 Error 的专用 match 合并为共同模式实现，交付用户枚举的多字段 Copy/拥有载荷、Optional/Result 嵌套匹配，以及 `?String`、拥有型 Named Optional。原622路径、394个stage0接受入口的分母不变；覆盖与最终资源数据以[验证索引](../compiler/validation/batch53-delivery-2026-11-28.json)为准。

## 共同实现与边界

| 层 | 实现 | 验收重点 |
| --- | --- | --- |
| 语法与类型 | Pattern节点树；名义枚举和统一变体字段描述；标准Error、用户enum、Optional、Result共用查询接口 | 多字段、具名/位置构造、跨名义类型拒绝、递归布局与借用字段拒绝 |
| 模式检查 | `matchValue`唯一分支入口；递归模式类型/元数检查；模式矩阵按构造器专化 | 嵌套sum与多字段乘积的穷尽性、67变体、空enum、scalar wildcard/bind；遮蔽分支依stage0仍接受 |
| 生命周期 | 已有place借用；拥有临时逐字段搬出并置零，未绑定部分在进入arm前析构 | 未绑定字段即时释放、单次求值、return/break/continue清理、分支移动状态合流 |
| 返回来源 | 模式别名追溯只读参数；匹配结果保留参数来源bit并受`of`约束 | 参数来源返回成功、错误来源拒绝、局部与临时视图保守拒绝 |
| C布局 | 零载荷enum保留整数；有载荷enum使用tag+union；Optional/Result/标准Error通过布局投影适配 | 原Error ABI/errno载荷保持，Utf8Error仍为结构体；同一模式检查与分支过程 |

当前aggregate字段中的Optional、Tuple、Result、Heap组合、嵌套Optional类型、通用引用与可变模式绑定仍有U0001边界。借用载荷和按值递归保持stage0诊断。泛型、接口、方法尚未在本批实现。已有place的整个模式作用域保持loan，尚未引入精确最后使用分析。

59项新模式用例由独立手写预期和stage0核验建立；旧12项U0001用例用原源码提升，未修改夹具以规避语义。分配计数检查正常产品，独立C观察器还检查三个arm输出时刻的活跃块数，避免仅凭进程退出无泄漏漏掉过迟析构。

## 原稿收益

初扫135/394，上一批125/394；完整模块图68/224，上一批62/224。新增10路径/6模块图包括：

- `compiler/tests/p0/run_enums.yv`：多字段/拥有枚举、容器、嵌套Optional匹配、显示与借用。
- `experiments/coldstart-2026-11-21/yavo/T02.yv`：拥有Optional与匹配。
- `T-C5`当前及v0.4/v0.5原检查稿：拥有Optional。
- `T-GR4`当前及v0.5：String.fromUtf8的拥有成功值和Utf8Error。
- `T-S2`当前及v0.4/v0.5：文件读取Result及嵌套错误模式。

运行稿使用原源码和原输入协议；T-C5保持原check任务性质。路径重复和模块图去重分别记录。上批“Optional/Result模式”单项模型候选5路径兑现为5条；组合加入拥有Optional和用户enum后再增加5条，不能将全部10条归因于单一模式语法。

batch53 profile保留51/52回测；新增的结构组合仍如实阻塞。raw profile误报由校准发现，不能因为checker接受而清空分析标记。259个未接受入口仍保留结构、流敏感语义和容量unknown；完整语法遍历不等于全部U0001已穷尽。

## 生成 C 压缩

`cString`将可打印ASCII直接写入C字面量，双引号、反斜杠和问号转义；其余字节保留固定三位八进制。问号转义阻断C17 trigraph，固定三位防止后继数字并入转义。

同一冻结编译器源码在该局部变换前后，生成C从1,856,352降至1,493,898字节，减少362,454字节（19.525%）。两份C将字符串字面量解码规范化后逐字节相同。真实C17回环对比全部ASCII、控制字节/NUL、九种trigraph、转义后接数字及UTF-8，256字节stdout完全一致。该数据衡量C文本表示，不把它外推为可执行文件同幅缩小；正式趋势采用最终自举产物。[专项证据](../compiler/validation/batch53-cstring-2026-11-28.json)。

## 已发现的问题与验收安排

stage0既有`loans_of`未传播Match结果的借用，部分由arm内拥有String返回的视图、局部String视图及错误`of`来源会被接受。已保存原例和生成C证据；selfhost分别保守U0001或依据来源拒绝，不把stage0接受视为安全性证明。完整修复需要在arm作用域退出前保留与归一来源，本批没有改写参考编译器来迁就测试。

旧diagnostics50/52冻结输入保留。diagnostics53显式提升原ownedOptional诊断，旧CLI生成C产品由三项可审计模板变换升级；加载器同时校验旧freeze和新版本，不从candidate输出学习预期。

本批新增拥有枚举布局和析构，按分层策略标记ownership milestone，运行一次Full；普通批次仍为Core，周期Full仍在第55批。三平台范围与Windows/macOS既有缺口继续单列。新成本对比仍受真实agent usage获取阻塞，未把程序重放计为新受试实验。
