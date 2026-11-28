# SH-3 接替清单（2026-11-28）

SH-2 固定点已完成，SH-4 独立应用 3/3。SH-3 的结束条件是 selfhost 覆盖约定的 stage0 语义和日常工具链，并成为默认入口；135/394 是第53批语料入口覆盖，不能替代此验收。

下表是按完整工作包安排的估计；相互依赖允许并行，批次不是性能/功能承诺。每完成一包，用原语料阻塞集合重排剩余工作，不以首错路径数承诺增量。

| 剩余项 | 验收标准 | 估计批次 |
| --- | --- | --- |
| 用户 enum＋通用 match | 第53批交付用户多载荷枚举、表达式/语句及嵌套match、模式矩阵、名义类型、移动/借用/清理与Error合流；剩余组合字段、通用引用及精确借用收口 | 53–55 |
| Optional/Result 模式和拥有载荷 | 原语料需要的拥有/嵌套 payload、字段/容器、匹配、try/catch 与提前退出；正向原稿通过，错误码/资源拒绝用例无回退 | 54–57 |
| 完整元组（按 stage0 已支持范围） | Copy/拥有字段、任意已支持元数、容器元素、结构字段、参数/返回、字段投影及析构；Copy Vec 作为共同布局/生命周期的一部分 | 55–58 |
| 泛型与接口 | stage0 已有泛型函数/结构、约束与接口调用；单态化、所有权与 Send/Sync 规则一致；未实例化函数等既定边界明确 | 57–61 |
| 其余原语料语义与库缺口 | 按完整集合推进数值/Map/Set、方法/闭包、并发等；固定 394 个 stage0 接受入口逐个有结论；严格子集 0 越界、0 已支持退步；源码/模块图去重同时报告 | 59–64，按扫描结果再估 |
| 日常构建与分发 | 原生 build/run JSON、锁定后端安装/离线、交叉目标、缓存/PCH；干净目录独立构建，三平台原生执行；默认命令不偷偷委托 Rust 前端 | 60–64 |
| 现有工具入口与结构化诊断 | 对齐 stage0 已交付的 check/fix/verify/allocations/toolchain；明确错误码、源范围、多文件与 stdout 协议；outline/type-at/explain/doc/audit 仍按原产品路线另列，不能冒充 stage0 已有能力 | 62–65 |
| 默认接替与平台收口 | 干净检出由 selfhost 完成自身和既有应用的常规开发；完整门通过；回滚入口保留；Windows/macOS 各套件缺口逐项关闭或列明不适用 | 65–66 |

成本/性能与 SH-3 并行推进，不以 SH-3 未完成延后。12 题成本复测入口见 [`cost-pilot-2026-11-28`](../experiments/cost-pilot-2026-11-28/README.md)；真实模型 usage 可获得后立即启动，不以准备态声称完成。商业门槛仍是单位成功交付总成本比 Rust 低至少 20%，失败任务成本计入，且需要独立重复；当前没有达标证据。

## 标准 Error 并入通用 match

第53批已完成共同Pattern树、变体描述和唯一`matchValue`入口。用户enum、标准Error、Optional和Result共用模式类型检查、矩阵穷尽检查、绑定和清理；旧“仅标准Error”限制已删除。剩余是更广的组合类型、引用及精确借用生命周期，见[本批边界](batch53-patterns.md)。

通用实现采用统一变体描述：名义类型 ID、tag、字段类型、拥有性、借用来源和清理操作。`fs.Error`、`io.Error`、`os.Error`、`process.Error` 作为内建枚举登记，保留各自名义身份和实际 errno/载荷。用户 enum、Optional/Result 逐步使用相同描述。

穷尽/重复判断、载荷绑定、结果类型合并、分支状态合并和析构统一走 `matchValue`。后端可以按布局选 C 的 tagged struct 或 union，但不得保留“标准错误专用 match”与“用户枚举 match”两套语义检查。迁移验收要求删除 `match requires a standard-library error` 的专用限制，原标准错误行为/负例继续通过，并加入同构用户枚举的对照；同形载荷跨名义错误类型仍拒绝。

## 已知平台缺口及补齐安排

| 缺口 | 当前证据 | 处理 |
| --- | --- | --- |
| macOS/Windows Rust 原生回归为选定套件 | 第51批 Linux Debug/Release 各151/0/1；macOS90/0/1；Windows87/0/1。`build_cache`、`conformance`、`harness`、`p0_regressions`、`targets`、`verify` 未列入 native 清单 | 第55批完整门先逐套件试跑并分为可移植/平台前提；可移植项加入；POSIX假设改为显式平台测试，不伪称全量已通过 |
| Windows `env_invalid_utf8` | Win32 环境接口使用 Unicode，原 POSIX 非法 UTF-8 原始字节输入不适用 | 第55批补 Unicode/转换失败的 Windows 对应测试；不要求构造不存在的原始字节环境 |
| Windows `stdin_read_error` | 当前夹具用 POSIX 目录 FD 作为 stdin，Windows 未执行 | 第55批增加可确定的 Windows 句柄/管道读错夹具；验证实际 I/O 失败载荷和清理 |
| Windows 峰值 RSS | 当前 wait4 口径不可用，记录 null | 独立增加 Windows 进程内存采样后才填数，不从 Linux 推断 |

完整门仍需逐平台声明执行范围；三平台 CI 绿色只代表本次被选择的门。
