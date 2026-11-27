# Yavo 自举路线与实用验收

2026-11-27 · 第四十七批加入 Set[String] 与独立 CSV 校验 CLI；最终959/959、18/18与三平台CI 6/6通过，正式CLI交付2/3，SH-3尚未接替

近期主目标改为：**用 Yavo 实现 Yavo 编译器，完成可重放的自举闭环，再让它承担日常编译工作。** 自举用于暴露真实工程中的语言和工具链缺口；实用验收继续独立执行。

本文件修订 [v0.5 规划](plan.md)中“编译器用 Rust 实现”的后续实施方向。现有 Rust 编译器保留为引导编译器、差分参照和回退实现；C17 后端、锁定的 Zig、无 GC 和所有权语义继续保留。[语义表](semantics.md)、[标准库契约](stdlib_contract.md)、[语义修订](decisions.md)以及冻结测试包仍是语言行为依据，不随移植方便而放宽。

## 自举证明什么

| 证据 | 可以证明的范围 | 仍需独立验证 |
| --- | --- | --- |
| Yavo 编写的前端能编译外部程序 | 实现了该输入子集的词法、语法、检查和代码生成 | 编译器自身是否属于可接受的输入子集 |
| Yavo 编译器能编译自己的完整源码 | 语言能表达这套编译器所需的数据结构、算法和资源管理 | 未在编译器源码中出现的语言特性、错误输入与其他应用 |
| 连续两代重建结果稳定 | 固定输入和工具链下，自举链收敛且可重放 | 共享实现缺陷、独立正确性和跨平台行为 |
| 独立程序、负向用例、平台与资源检查通过 | 在已声明应用范围内具备可测的实用性 | 未覆盖的业务领域与生态需求 |

编译器工作负载集中在文本处理、符号表、图遍历、诊断和文件生成。网络、数据库、GUI、第三方库集成不会因为编译器能编译自己而自动具备。近期实用范围先定位为**本地命令行开发工具和批处理程序**，以后按真实应用扩展。

## 编译器代际与完成定义

代际表示“由谁构建”，不表示功能版本：

| 名称 | 实现与来源 |
| --- | --- |
| stage0 | 当前 Rust 实现的 Yavo 编译器 |
| stage1 | stage0 编译 Yavo 实现源码得到的编译器 |
| stage2 | stage1 编译同一份 Yavo 实现源码得到的编译器 |
| stage3 | stage2 再编译同一份 Yavo 实现源码得到的编译器 |

最小自举闭环必须真正产出 stage2，并由 stage2 产出 stage3；之后用 stage2 和 stage3 编译独立测试程序。仅由 stage0 编译一个 Yavo 编写的前端、只扫描自身源码、输出固定程序或保留预生成 C 文件，都不构成自举。

初期允许 Python 或 shell 驱动阶段构建，并调用锁定的 Zig 编译生成的 C。它们只负责调度、文件准备、执行和结果记录；Yavo 源码的解析、语义检查和 C 生成必须由对应阶段的 Yavo 编译器完成。stage1 以后不得把未支持的输入转交给 Rust 前端。保留 C 运行时与 Zig 后端符合本项目的自举定义；完全重写后端不是当前目标。

## 实施里程碑

| 里程碑 | 交付范围 | 退出条件 | 当前状态 |
| --- | --- | --- | --- |
| SH-1：可用的子集编译器 | 在 `selfhost/` 中实现真实词法器、解析器、最小语义检查和 C 生成；公布精确输入子集 | stage0 构建成功；外部正向程序运行结果正确；负向程序在前端拒绝；未实现语法明确报错；支持的行为与 stage0 差分一致 | 已完成：子集覆盖编译器自身源码；第三十一批语料扩展到 490 例，当前验收见下节 |
| SH-2：最小自举闭环 | 输入子集覆盖编译器全部 Yavo 源码；可由外部驱动构建 | 在独立目录、清空产物的条件下完成 stage0 → stage1 → stage2 → stage3；stage2/3 对固定输入生成的 C 一致，独立程序结果一致；记录所有源码、运行时、工具链和构建参数的身份 | **已闭合（引导子集）**：stage2.c 与 stage3.c 逐字节一致，stage2/stage3 二进制一致，stage2 在全部用例上与 stage1 输出相同；三平台原生固定点基线及第三十一批扩展均已确认，当前证据见下节 |
| SH-3：主编译器接替 | 逐步覆盖当前 Rust 前端支持的语义、模块与诊断；承接正常构建入口 | 现有回归与冻结规格报告无未解释退步；独立错误输入与资源测试通过；Linux、Windows、macOS 原生执行相应验收；日常开发默认用自举编译器 | 尚未接替，Rust stage0 仍是主编译器 |
| SH-4：声明范围内的实用交付 | 以非编译器应用验证语言、库、工具和分发 | 下节中的应用、诊断、可重放构建、资源和平台门槛通过，明确支持范围与仍缺能力 | 当前2/3：test-summary与csv-check均完成三平台独立验收；CSV两份C的Linux sanitizer通过，第三个多文件依赖/索引项目待交付 |

SH-1 的结果只标为“子集编译器”。SH-2 可以标为“子集自举完成”，同时列出尚未移植的语言能力。覆盖全部当前语言能力并完成 SH-3 后，才把自举实现作为主编译器交付。阶段编号不换算为项目完成百分比。

截至转向自举前的 `bc366c6`，主编译器仍是 Rust，`compiler/src/*.rs` 共 20,776 物理行，`check.rs` 占 8,982 行；C 运行时 1,931 行。这是移植规模基线，不是成本或进度模型。

## 当前进度

### 第四十七批：Set[String] 与独立 CSV 校验 CLI

本批基线 `84cd8ef`，第二个正式独立应用 [csv-check](../examples/csv-check/README.md) 为main/parser/validator/report四模块、271行。先冻结[SPEC](../examples/csv-check/SPEC.md)并用stage0完整实现，首次check成功，仅移除一条未使用导入后冻结源码，Debug/Release均0错误、0警告；没有行为修复。第46批stage2对原文的唯一实际阻塞是 `validator.yv:15:20` 的Set构造U0001。补Set后候选stage1/stage2直接check/emit-c成功；八次完整编译尝试的源码、工具和结果见[开发记录](../examples/csv-check/evidence/DEVELOPMENT.md)。

`Set[String]`支持拥有局部、参数、返回、结构体字段及Result两侧载荷，接入正常与return/try/catch/break/continue清理。局部空构造为 `[]`、`[:]`、`Set[String]{}`，字段默认仅 `[]`；add消费String并返回Unit，重复键释放传入所有者，has/remove借用str或String查询，remove释放删除的存储键并返回bool，len/clone可用。C复用Map[String,bool] helper，语言类型仍区分Set/Map。保留接收者预留与借用检查；Set遍历/take、其他键、直接Vec[Set[String]]、拥有Optional及查询参数修改自身接收者仍U0001。独立审查修正遍历错误分类、非空初始化分类及字段默认 `[:]` 误接受三处边界。

CSV支持完整UTF-8验证、严格引号/双引号转义、逗号、LF/CRLF和跨行字段；表头非空且按解码内容精确唯一，记录列宽一致。先完整解析当前记录再立即结构校验，保持同记录词法优先、早记录结构错误先于后记录词法错误；位置按UTF-8字节，结构错指向记录起始行第1列。裸CR、开头BOM、除TAB外的ASCII控制及DEL被拒绝。stdin/多文件按顺序处理，失败时stdout保持空，稳定诊断分别以CSV错误1、用法2、读取/UTF-8错误3退出。拥有资源在execute返回时释放，再由main调用os.exit。

139条手工合同预期与独立字节oracle在应用执行前一致，见[冻结记录](../examples/csv-check/tests/oracle-freeze.json)；cases SHA256 `a0abf46feaf6185864573641cfc4df6f64776f7b734907dec2f2d4862929f257`，oracle SHA256 `eff38afb971cd0064430cc0623c3e453696b57788700cecc868e2ead7edd82f9`。POSIX适用138，Windows适用137，平台不适用项保留明确状态。

| 验收路径 | POSIX执行数 | Windows执行数 | 当前证据 |
| --- | ---: | ---: | --- |
| bootstrap第18项csv_check_application：stage0/stage2各Debug、Release、Release exit-shim | 828 | 822 | 最终本机828/828；三平台原生门均通过 |
| native：Debug、Release、exit-shim；另建独立Release副本做复现 | 414 | 411 | 本机414/414；Linux/macOS414、Windows411均通过 |
| 独立GCC ASan/UBSan：stage0 C、最终stage2重新emit的C | Linux 276 | 不适用 | clean/UAF/overflow能力控制先行；本机无GCC，unavailable/null；Linux CI required已通过276次 |

普通及退出清理产物均验证精确输出和退出码，清洁/泄漏控制校准exit-shim；SPEC、oracle、固定输入、应用、验收脚本、运行时、锁定工具链和产物首尾身份受检查。native使用预安装锁定Zig，在含空格/非ASCII的独立目录内建两个空缓存Release副本，生成C与二进制逐字节相同：应用C46,702字节，SHA256 `6d32d6dd002adda1377bd3eba57d22a49f3cca98dbe2a0b10d77123b18c56a27`；Release产物30,456字节，SHA256 `bd53883ea38843a63561a22bae375298462df64b5743117bdc9087aefdcbb996`。回执为 `compiler/target/ci/csv-check-batch47-rss/attempt-xyp1f13q/receipt.json`，3次native构建、6次计时构建均通过。

| 模式 | 构建条件 | 单次耗时（秒） | wait4报告RSS（字节） | 缓存 |
| --- | --- | ---: | ---: | --- |
| Debug | 冷构建 | 22.284126 | 411,893,760 | miss |
| Debug | 不变重建 | 0.170244 | 30,924,800 | hit |
| Debug | 注释编辑后 | 0.486040 | 84,553,728 | miss |
| Release | 冷构建 | 4.942010 | 219,435,008 | miss |
| Release | 不变重建 | 0.020308 | 31,305,728 | hit |
| Release | 注释编辑后 | 0.825170 | 90,292,224 | miss |

这些条件各一次观察，P50/P95仍为null。新runner全部431条命令均测得RSS，整体最大417,312,768字节，414次应用执行最大26,951,680字节。Linux将ru_maxrss的KiB转为bytes，macOS使用bytes；Windows无法观测时留null与原因。该值是OS对waited child的报告，不是同时进程树RSS，也不对不同命令的峰值求和。正常exit0、非零exit7、两层子进程超时及注入构建失败清缓存控制均通过：超时1.005秒返回−9，三PID均终止，Popen.returncode一致；native五棵临时缓存已清除。控制证据在 `compiler/target/batch47/rss-controls-7x7tqsxm/receipt.json`。

Set新增34例、无旧例提升，矩阵959=242run+32panic+524reject+123unsupported+38shared_unsupported。无Rust/runtime改动，Rust Debug/Release各145/0/1、冻结124/3/2。首轮完整自举 **959/959、18/18、0 skip**，stage2/3 C **1,057,613字节**、二进制 **490,144字节**逐字节相同；C SHA256 `694a454c2e1ba59f4b19fd2bd1e4a641472433e74e53f461baf8b576b4416ded`，binary SHA256 `7dfc1373b1e0ab8ae510c61565abdc93e813748d8869905124a89e255675bcbe`。首轮回执位于 `compiler/target/ci/selfhost-batch47/run-wc7teq0m/receipt.json`。

新runner的最终目录 `compiler/target/ci/selfhost-batch47-final/run-zukpunvi` 已通过最终完整959/959、18/18、0 skip，六项应用门与输入身份检查全部通过；stage2/3 C与二进制逐字节一致。较长输出目录进入源诊断路径，本次C为 **1,057,805字节**，SHA256 `6b88386c7a52c839e6c937345250a7125e187d61686cdab9f0c766dcd5ab8107`；二进制 **490,192字节**，SHA256 `9d3e68bce7b96e59ddd25281a1787d2b006b3aeae8e99c66955b66344d5a5cbb`。前述首轮身份保留为该次运行历史。

| 覆盖输入集合 | 文件数 | stage0接受 | 最终stage2接受 | 按入口字节去重 |
| --- | ---: | ---: | ---: | ---: |
| 第46批冻结原集合，路径及字节未改 | 613 | 392 | 112 | 52/222 |
| 加入CSV四模块后的集合 | 617 | 393 | 113 | 53/223 |

最终stage2扫描退步/越界/超时均0，回执在 `compiler/target/batch47/final-stage2-coverage.json`。首轮自举后补齐了可观测RSS和失败路径清缓存，验收器输入已改变，新runner的最终完整自举已独立重跑通过；[验证索引](../compiler/validation/batch47-csv-set-2026-11-27.json)记录新回执与产物身份。实现提交 `0373fb8` 的 [run 45937402633](https://github.com/msly/yavo/actions/runs/45937402633) **6/6成功**；三平台原生bootstrap、CSV、证据归档/上传以及Linux两份C sanitizer均通过。验证索引保存精确提交与job/step结果，CI归档内容未另行下载。**SH-4正式交付更新为2/3**。SH-3仍只有check/emit-c，正常build/run、结构化诊断与默认接替另计。下一批目标为第三个独立多文件依赖/索引CLI。

### 第四十六批：Map 遍历、String 排序与原始原子账本 CLI（历史记录）

支持只读Map[String,int/bool/byte]两绑定遍历：按插入顺序、跳过删除空槽，借用只读String键和Copy值；循环期间冻结Map或字段所有者。键可clone为拥有值，不能直接移动或修改；完整临时Map活到循环结束，临时结构体的Map字段按stage0报E0305。Vec[String].sort()复用稳定原地归并排序和UTF-8字节比较，描述符移动不新增所有者；接收者、借用和提前退出清理规则保持原约束。引用别名、键视图逃逸、可变遍历及更广Map值类型仍在子集外。

原始r5/yavo-b/T01未改，第17项atomic_ledger_application保留原judge14组并补18个有效输入边界，共32场景、783命令、517条batch支付行。覆盖回滚后仍消费剩余输入、提交、错误优先级、自转账、Map扩容、ASCII键排序、batch0/100、200命令、99,000,000,000单户余额和200,000,000,000总额。独立差额日志与clone账本模型输出一致，双方Debug/Release共128次正常返回分配检查执行全部通过，8项输入首尾绑定。

新增28例，原样提升indexed_map和sort45_string，合计925；完整本机自举 **925/925、17/17、0 skip**。C **1,039,982字节**和Linux二进制 **484,496字节**在stage2/3间逐字节一致；五项应用门和输入不变检查通过。Rust未改，Debug/Release各145/0/1，冻结124/3/2。实现提交 `bc8aefa` 的 [run 45826605112](https://github.com/msly/yavo/actions/runs/45826605112) **6/6成功**；三平台原生Rust、自举/应用门、Linux selfhost ASan/UBSan及独立作业全部通过。

全仓613个源路径和392个stage0接受成员未变，最终stage2覆盖 **106→112/392**，退步/越界/超时均0，221个stage0拒绝输入无放行。新增六路径对应两个账本实现各三份快照；入口文件字节去重后为 **52/222份不同源码**，尚有170份未接受。每个首拒只表示当前第一个阻塞，语料覆盖不换算为项目完成率。SH-3还需覆盖前端语义并接替当前只有check/emit-c之外的正常build/run、诊断和工具入口；SH-4正式交付仍1/3。下一批优先第二个正式结构化文本校验项目，用实际依赖推动拥有Optional等能力。

### 第四十五批：整数 Vec 排序与原始窗口中位数 CLI（历史记录）

支持 Vec[int].sort()：无参、稳定原地升序、返回 Unit，接收者为可写变量、inout 或嵌套字段；复用现有归并排序和不会通过相减溢出的整数比较器。移动/借用/循环冻结/调用预留均保持原检查，括号裸变量和临时接收者按 stage0 保持 U0001；byte/str/String 排序、sortBy、拥有元素下标 place 仍未开放。本批不改 Rust；Debug/Release 全量各 145/0/1，冻结规格 124/3/2。

原始 r5/yavo-b/T11 未改，新增第 16 项 sliding_median_application。28 固定场景保留原 judge 14 组，补 K=25、长度 200、奇偶下中位数、重复、空序列、混合空白和窗口和 ±25,000,000,000。独立预期覆盖 1,405 个输入值、1,112 个窗口，双方 Debug/Release 共 112 次正常返回分配检查执行全部通过；8 项源/契约/场景/验收器输入绑定。所有场景均在公开有效 ASCII/LF 输入域内。

新增 22 例，无提升，合计 897。全仓 613 个源文件和 392 个 stage0 接受成员均不变，最终 stage2 覆盖 **100→106/392**，退步 0、越界 0；新增六个路径来自 yavo-a/yavo-b T11 各三份快照，只有两种入口源码。四项 bootstrap 应用回归不改变 SH-4 正式交付仍为 1/3 的状态。完整本机自举 **897/897、16/16、0 skip**；stage2/stage3 C 1,027,977 字节、Linux 二进制 479,152 字节逐字节一致，四项应用门及输入不变检查全部通过。实现提交 `67407bb` 的 [run 45794313520](https://github.com/msly/yavo/actions/runs/45794313520) **6/6 成功**；三平台原生自举/应用门、Linux ASan/UBSan 与独立作业全部通过。

### 第四十四批：整数 min/max 与原始文本 diff CLI（历史记录）

支持同型 int/byte 的 min/max，保留双向数字推断、Optional/Result 包装边界、用户同名函数优先；两个实参各求值一次，先保存左值再执行右侧副作用。Rust stage0 在本地/字段首参上原先违反此顺序，现已修复并加 P0 回归。完整 Rust Debug/Release 各 145/0/1，冻结规格保持 124/3/2；其他数值类型仍在 selfhost 子集外。

原始 r4/yavo/T11.yv 未改，新增第 15 项 bootstrap 应用门：35 固定场景、双方 Debug/Release 共 140 次执行，精确核对差异行、LCS 平局先删除、汇总、stdout/stderr 和 0/1/2 退出码。32 次正常返回完成分配检查，108 次按原程序 os.exit 路径核对结果；契约与 API 扩展边界分开记录。三项 bootstrap 应用回归不改变 SH-4 正式交付仍为 1/3 的状态。

新增 28 用例，原样提升 builtin_max 一例，合计 875。原有 391 个 stage0 接受程序的覆盖 98→99，仅新增 T11；加上新 P0 后最终 stage2 全仓扫描为 100/392，退步 0、越界 0，221 个 stage0 拒绝输入均未被接受。完整本机自举 **875/875、15/15、0 skip**；stage2/stage3 C 1,018,462 字节、Linux 二进制 476,112 字节逐字节一致，三项应用门及输入不变检查全部通过。实现提交 `1c8207d` 的 [run 45783874254](https://github.com/msly/yavo/actions/runs/45783874254) **6/6 成功**，三平台原生自举/应用门、Linux ASan/UBSan 与独立作业全部通过。

### 第四十三批：Copy 元组与原始单位换算 CLI（历史记录）

支持由 int/byte/bool/非拥有结构体/无载荷枚举组成的平坦 Copy 元组，包括构造、数字投影、局部值、参数/返回、整体赋值和 inout；Optional 元组可用 some/none、隐式包装、两类 ?? 和 if-let。类型签名使用有界的无碰撞整数编码，超出容量为 U0001；嵌套、拥有或借用元素、元组容器/字段、解构仍未开放。整数 abs 保留 byte 上下文并检查 int 最小值溢出。定标发现 Rust stage0 直接打印元组会触发 codegen panic，已在检查期以 E0001 拒绝，新增 P0 回归。

原始 `r5/runs/yavo-b/tasks/T04/final` 的 main.yv 与 conversion.yv 未改动，新增 bootstrap 第 14 项应用门，以 20 个固定场景、330 条输入、Debug/Release 下 stage0/selfhost 共 80 次分配检查执行核对输出。公开契约、固定场景、两份源码和验收器均绑定哈希。该门验证真实程序兼容性，SH-4 正式应用交付计数仍为 1/3。

新增 55 例，合计 847；最终 stage2 全仓扫描 **92 → 98 / 391**，新增 6 个路径来自两份单位换算实现各 3 份快照，退步 0、越界 0，221 个 stage0 拒绝输入均未被接受。本机完整自举 **847/847、14/14、0 skip**；stage2/stage3 C 1,010,808 字节、Linux 二进制 472,064 字节逐字节一致，两个应用门全部通过。实现提交 `7a2d08f` 的 [run 45771997599](https://github.com/msly/yavo/actions/runs/45771997599) **6/6 成功**；三平台原生自举/应用门、Linux ASan/UBSan 和独立作业全部通过。下一批针对原始 `experiments/coldstart-2026-11-21-r4/yavo/T11.yv` 的文本 diff CLI 补整数 min/max，并核对差异文本和退出码。

### 第四十二批：带下标遍历与 Copy Optional if let（历史记录）

`for index, item in values` 支持既有 Vec/只读切片及 os.args()；索引只读 int，元素继续按 Copy 或只读借用处理，临时集合与控制流清理保持原规则。`if let name = value` 支持现有 int/byte/bool/非拥有结构体/无载荷枚举 Optional，支持 else、else-if-let 与 `_`；具名绑定在 then 作用域内保守保持匹配源的借用，`_` 不保留该借用；拥有型 Optional 尚未开放。编译器内类型查找和限定类型解析已分别迁移到两种语法。

新增 36 个用例，原样提升 2 个，合计 792；stage2 全仓覆盖 **91 → 92 / 391**，0 子集越界，220 个 stage0 拒绝输入均未被放行。本机完整自举 **792/792、13/13、0 skip**；stage2/stage3 C 967,346 字节、Linux 二进制 448,416 字节一致，test-summary 的 71 个本机场景通过。实现提交 `819ea1e` 的 [run 45763574200](https://github.com/msly/yavo/actions/runs/45763574200) **6/6 成功**，三平台原生门与 Linux ASan/UBSan 通过。基线 91 来自 `3ce5d36` 无缓存重建，旧记录 88 未能复现，已在 [HANDOFF 第 22 节](../HANDOFF.md) 校正。本批新增接受罗马数字转换 CLI、无退步。下一批以原样编译 r5/yavo-b/T04 的双模块单位换算 CLI 为目标：Copy `(int, int)`、函数返回/局部值、`.0/.1`、`?(int, int)` 与 `abs()`；不将只读诊断实验中的改写版算作应用通过。元组类型的 34 个路径包含重复源码，另 12 个“元组表达式”首诊断实际来自 sortBy 闭包，不能据此预测覆盖增量。

### 第四十一批：std/io、视图绑定与 str 方法（历史记录）

`use std/io` 后可以写 `let text = io.readAll() catch _ { ... }`；str/String 新增 `trim`、`lines`、`fields`。指向拥有局部的视图现在可以绑定（`let lines = text.lines()`），视图在作用域内时其所有者不能修改或移动；视图不能离开所有者的作用域（不能返回、赋给外层变量或放进其他容器）。指向只读 String 参数的视图可以经 `of` 返回。`for` 可以遍历临时 Vec。当时记录全仓覆盖从 70 升到 88，仍无子集越界；第四十二批重建其最终源码基线为 91，校正见上节。新增 23 例，提升 11 例。

### 第四十批：Map 与 test-summary（历史记录）

`Map[String, V]`（V 为 int、bool 或 byte）可以作局部、参数、返回值、Result 载荷和结构体字段；`[:]` 需要期望类型。`set` 消费 String 键，`has`/`getCopy`/`remove` 接受 str 或 String 查询，`getCopy`/`remove` 返回 `?V`，另有 `len` 与 `clone`。插入顺序与重复键处理同 stage0。selfhost 现在能编译原样的 examples/test-summary，构建出的程序在 71 个本机场景上与人工期望和 stage0 构建一致；这项检查是 bootstrap 的第 13 项。同时修复了块语句、带消息的 assert、解引用和数值类型转换在子集中报错误 E 码的问题，并用 `selfhost/subset_sweep.py` 确认全仓无子集越界。新增 40 例。

这达到了第三十四批起追踪的“编译真实程序”判据，但 SH-3 的正式接替条件（覆盖既有回归与冻结规格、日常默认使用）尚未满足。

### 第三十九批：遍历拥有元素（历史记录）

`for x in v` 现在可以遍历 `Vec[String]` 与拥有型结构体元素：`x` 是元素的只读视图，不被释放也不能移动，循环期间 `v` 不能修改或移动。移动元素报 E0404，修改元素或 `&x` 报 E0014，移动元素的字段报 E0401，循环内修改 `v` 报 E0102。`let y = x`、遍历临时 Vec、返回元素视图仍是 U0001。同时修复了跳转式 `??` 回退中拥有型临时值被重复释放（生成的 C 无法编译）的问题。把 test-summary 的 Map 临时换成 Vec 后，selfhost 已能编译整个程序，70 个场景输出与 stage0 构建一致。新增 18 例并提升 2 个旧 unsupported 用例。

### 第三十八批：str 查询方法（历史记录）

str 与 String 接收者支持 `startsWith`/`endsWith`/`contains`（bool）、`find`/`parseInt`（`?int`，可配合 `??` 与 `?? return`）和 `split`（`Vec[str]`）。`split` 的结果指向接收者：str 接收者时继承其来源（可做 `-> Vec[str] of text` 返回值，元素同样），String 或临时接收者时只能在当前语句内使用，绑定为局部仍是 U0001。实参个数错 E0012，实参类型错 E0001，`let i: int = s.find(x)` 报 E0001。实参求值期间锁住接收者：拥有型接收者被 inout 修改报 E0102；同时修复了 Rust stage0 在这里漏检导致的释放后使用。新增 12 例（1 run、7 reject、2 unsupported、2 shared_unsupported）。

### 第三十七批：只读 []T 参数（历史记录）

`[]T`（T 为 int/byte/bool/str 或 Copy 命名类型）可以做参数、由参数复制来的局部和带 `of` 的返回值；C 中与 Vec 同为 `yv_vec` 描述符，不拥有存储、携带借用（参数带自己的来源位，元素读出的 str 继承切片来源）。调用时 `Vec[T]` 实参自动转为视图（不按切片元素类型推断字面量，同 stage0），`os.args()` 视为 `[]str`。`len`、下标读（越界 P-INDEX）、`for` 可用；写元素、`push`、给参数赋值报 E0014，`var` 切片局部写元素报 E0101，比较 E0501，打印/插值 E0001，字段 E0301，返回缺 `of` 报 E0302。把 `Vec` 实参的视图作为结果绑定、`let ys: []T = v`、返回 `Vec` 当切片、子切片 `xs[a..b]`、`toVec`、元素为拥有类型的切片仍是 U0001。编译器自身的 `contains`、来源与状态辅助函数、parser 的 `store` 改为 `[]T` 参数。新增 26 例（1 run、1 panic、16 reject、7 unsupported、1 shared_unsupported）。

### 第三十六批：跳转式 ??（历史记录）

`x ?? return [值]`、`?? break`、`?? continue` 用于 Copy Optional。与 stage0 相同，只能作为 `let` 的整个初始值或 `=` 赋值的右侧，可以经过 `a ?? b ?? return` 的右结合链；加括号、放进参数或运算式、复合赋值都报 U0001。跳转分支复用 `return`/`break`/`continue` 语句的生成与清理，分支之后的代码看到分支之前的 moved 与来源状态；`return` 值的类型与 E0001/E0015 规则同语句形式。顺带修复第三十四批遗留的子集差异：`catch _ { … }` 块执行后只恢复了 moved 状态，没有恢复来源状态，块里 `t = a; return a` 会让后续 `return t` 被误接受（stage0 报 E0303）。编译器自身的 `types.importedFile` 改返回 `?int`，`qualifiedType` 用 `?? return -1` 代替哨兵判断。新增 14 例（1 run、9 reject、4 shared_unsupported），旧的三个 `unsupported_optional_coalesce_*` 原字节晋升为 run 用例。

### 第三十五批：Vec[str]（历史记录）

`Vec[str]` 可以做局部、参数和带 `of` 的返回值。容器的来源是元素来源的并集：`push` 与下标赋值合并，整体赋值替换，移动、`take`、`clone` 保留；读出的元素与 `for` 绑定继承容器来源，之后改动容器不影响它们。字段或错误类型里放 `Vec[str]` 报 E0301；往 `inout Vec[str]` 存非静态文本报 E0306，存它自己的元素报 E0403。`pop`（`?str`）、拥有 String 的视图、`Vec[str] ! E` 仍是 U0001。写 `inout` 参数时若仍有局部变量指向它，子集报 U0001（stage0 按活跃性可能接受或报 E0102），这也关掉了上一批 `inout str` 读后写被子集接受而 stage0 拒绝的差异。

定标时发现 Rust stage0 的另一处缺陷：调用点对 `of` 实参只取元素借用，`fn first(v: Vec[String]) -> str of v`、`fn head(v: Vec[int]) -> []int of v` 返回的视图实际指向实参的存储，实参随后被重新赋值时结果悬空（实测打印乱码）。修复后拥有堆内存的实参按“指向其存储”记录；只有元素为 Copy 的 Vec 且返回类型的借用全是 `str` 时保持只取元素借用，所以 `first(words)` 之后仍可改动 `words`。新增 p0 `errors_owned_view_args.yv`（3 处，旧版只报出结构体那 1 处）与 `run_text_vector_views.yv`。selfhost 新增 25 例（1 run、18 reject、6 unsupported），旧的 `unsupported_vec_str_element` 原字节晋升为 `vec_str_literal`。编译器自身没有为绕开 `Vec[str]` 写的变通代码，本批无迁移。

### 第三十四批：借用返回（历史记录）

函数可以写 `-> str of a`、`of (a, b)`、`of static`、`of (s, static)`；未写 `of` 的 str 返回报 E0302，名字不是参数报 E0010，返回值指向未列出的参数、拥有局部或临时值报 E0303，`inout str` 参数只能接收静态文本（E0306）。子集里持久的 str 只指向参数或静态数据，所以每个 str 局部记录一个参数位掩码；赋值替换、分支在可落下的路径上合并、循环重放到入口状态不再增长，与 stage0 的 loans 同构。调用方按被调函数的 `of` 合并实参来源；实参是拥有 String 时结果是语句内视图，不能绑定（U0001）。

定标时发现 Rust stage0 的缺陷：loans 按“最后一次写入”记录，分支后不合并，循环也不把后面写入的来源带回下一轮，`if c { t = b } else { t = a }` 或循环里先 `return t` 后 `t = b` 都被接受，运行时读到已释放内存。修复后 if/match/catch 处按路径合并，循环迭代到不动点，重放中发现的 E0303/E0305/E0306/E0102/E0703 也报告；新增 p0 `errors_loan_paths.yv`（9 处）与 `run_loan_paths.yv`。30 个新用例（3 run、22 reject、5 unsupported），其中 7 个拒绝只有在修复后的 stage0 上才成立。编译器自身的 `integerOperation` 改为两个 `-> str of static` 查表函数，不再为选运行时函数名分配 String。

### 第三十三批：源模块（历史记录）

`use a`、`use a/b`、`use a/{b, c}` 以入口目录为根加载，按模块路径去重，允许循环与自导入；`pub` 决定导出，导入逐文件、不传递；函数调用写 `module.f(...)`，公开类型可用短名或 `module.T`，二义短名 E0010。诊断与运行时 panic 均指向原文件行列（与 stage0 相同的源文件表）。21 个多文件用例（4 run、1 panic、15 reject、1 unsupported），错误码均先由 stage0 实测。本机 **568/568、12/12、0 skip**，stage2/stage3 C 与二进制一致。随后编译器自身也改为以 `main.yv` 为入口的 `use` 多文件（[决定 28](decisions.md)），bootstrap 不再拼接源码，只交入口文件。

### 第三十二批：插值与 format（历史记录）

`${expr}` 用于 print/println/eprintln/format 的直接字符串参数，类型限 int/byte/bool/str/String/无载荷 enum；format 返回拥有 String，用户自定义的 format 函数仍覆盖 prelude。所有片段按源码顺序各求值一次，全部成功后才写出外层消息；后续片段 try/catch 退出时释放先前的拥有临时值，不留下输出前缀。片段内词法错误延迟到解析时报告，以保持与 Rust 相同的报错先后。

真实迁移为 support.report 的完整诊断行与 codegen 的 temporaryName、localName、lineColumn；生成 C 与诊断字节不变。独立对照发现 Rust 检查器没有在后续片段求值期间持有前面 String/切片片段的借用（可导致释放后使用），已在 `print_parts` 修复并加 P0 回归。

语料 **547 cases**；本机 **547/547、12/12、0 skip**，stage2/stage3 C（727,751 字节）与 Linux 二进制（339,512 字节）分别一致；Rust Debug/Release 各 145/0/1。从本批起验收按 [decisions.md 第九节](decisions.md) 收敛。

### 第三十一批：for 与真实遍历迁移（历史记录）

支持 int/byte 半开区间，以及稳定具名 Vec 局部/普通参数/inout 参数的单绑定 Copy 元素只读遍历；允许名称外括号。range 端点左到右只求值一次，循环变量只读，continue 执行步进，有限循环保留零次执行路径。遍历期间根锁禁止修改或转移容器；头部 catch 的跳转仍归属外层循环，拥有临时值在正确作用域清理。

真实迁移为 lexer_test 的 32 次 token 构造、512 个前导零、16 轮 Diagnostic 验证，以及 parser.store 的 Vec[int] 复制。四处转换以基线函数原文核验，保留全部输入与断言。拥有元素、可变借用、字段/临时容器、双绑定集合遍历、slice/Map/Set 保持 U0001。

独立对照发现并修复 Rust range 左端读取延迟：右端可改写原变量，原实现直到循环初始化才读左端。修复只增加一次 typed snapshot；新 P0 回归的 local/field/index/byte/cache 五项在 Debug/Release 均为预期6，旧失败证据原样保留。语言运行时和冻结规格未修改。

本批52项独立定标，17个运行程序共34次Debug/Release通过；总表 **490 cases、172个可执行输入**（净新增51，旧for原样晋升）。[本机回执](../compiler/validation/selfhost-for-2026-11-24.json)记录 **490/490、12/12、0 skip**，stage2/stage3 C（685,990字节）和Linux二进制（321,368字节）分别一致，真实lexer两模式正常返回堆检查通过。完整Rust每模式145/0/1，冻结规格124/3/2；[oracle](../selfhost/tests/evidence/batch31-for-oracle/oracle-index.json)绑定源码和修前/修后参考身份。

实现 `046ccc6` 的 [run 45354873591](https://github.com/msly/yavo/actions/runs/45354873591) **6/6 jobs 成功**。三平台各490/490、12/12、0 skip，平台内C/二进制固定点、真实lexer两模式及CLI各213次执行通过。Linux GCC ASan/UBSan通过173程序，stage2自编译、490 checks、172 emits和8个坏输入。三份归档核验44,165文件、25,476条嵌入流，837个本机输入绑定与提交一致，见[原生索引](../compiler/validation/batch31-native-2026-11-24.json)；本机回执历史pending字段保留。

### 第三十批：Copy Optional 与编译器原地退栈（历史记录）

`?T` 支持 int、byte、bool、Copy struct 和无载荷 enum；显式类型与局部推断、`some`、有上下文的 `none`、边界处隐式包装、`isSome`/`isNone`、普通值形式的惰性 `??` 已接入。`Vec.pop()` 只处理 Copy 元素，接收者必须是可写 local、字段或 inout place；空 Vec 返回 none，非空 Vec 取末元素并减长度，保留容量。

编译器的 **14 个截断调用点** 已使用原地 pop，删除 `truncInts`、`truncBools`、`truncLocals`、`truncTemps`、`truncLoops`。锁栈只删除 mark 后的记录；局部/临时值先生成清理代码再删除分析记录；循环退出状态先复制再退栈。`cleanTemps`/`cleanLocals` 本身继续只生成清理代码，以保留其它控制流分支需要的分析状态。改动消除了截断时复制保留前缀的工作；性能收益尚未测量。

| 范围 | 第三十批边界 |
| --- | --- |
| `??` | 右结合、惰性 RHS；RHS 必须精确匹配载荷类型；fallback 临时值在分支内清理，移动状态合并到后续路径 |
| 上下文与包装 | 数字只接受直接整数上下文，Vec 字面量只接受直接 Vec 上下文；`some` 根据实参实际类型构造。`?byte = some(1)` 合法，`?byte = 1` 和 `byte ! E` 函数的 `return 1` 为 E0001 |
| Hint 与实际类型 | `??` 可能临时产生嵌套或拥有型 Optional 提示；仅用于推导与精确错型诊断，不进入 C 布局或资源管理 |
| U0001 | 拥有型/String/Vec/str/unit 载荷、嵌套 Optional、Optional 字段/容器、Optional/Result 组合、拥有元素 pop、`if let`、`match`、跳转式 `??`；Optional 比较和直接打印与 stage0 一样为 U0001 |

正式语料为 **439 cases、155 个可执行输入**：127 run、28 panic、218 reject、52 unsupported、14 shared_unsupported。净新增 67 例，原 `unsupported_vec_pop` 按原字节晋升；本批 68 项均先由独立 Rust oracle 确定结果，15 个运行程序的 Debug/Release 共 30 次执行通过。

本机完整门 **439/439 cases、12/12 checks、0 skip**，stage2.c = stage3.c（668,302 字节），Linux stage2/stage3 二进制逐字节一致（308,504 字节）；真实 lexer 的两代 C 与 Debug/Release 正常返回堆检查通过。Rust 完整 Debug/Release 各 145/0/1，冻结 124/3/2。来源、命令与哈希见[本机验收摘要](../compiler/validation/selfhost-optional-pop-2026-11-24.json)和[独立 oracle](../selfhost/tests/evidence/batch30-optional-oracle/oracle-index.json)。

`7297e3b` 的 [CI run 45345384747](https://github.com/msly/yavo/actions/runs/45345384747) **6/6 jobs 成功**。Linux、Windows、macOS 各通过 439 cases、12 checks、0 skip，平台内 stage2/stage3 C 与二进制相同，真实 lexer 两模式及 CLI 各 213 次执行通过。Linux GCC ASan/UBSan 通过 156 程序（155 fixture + 1 lexer），stage2 自编译、439 checks、155 emits 和 8 个坏输入。三份归档已按 GitHub 大小/摘要、提交源码与实际输出核验，共 40,102 个文件、22,892 条嵌入流；783 个本机输入绑定与提交一致，见[第三十批原生索引](../compiler/validation/batch30-native-2026-11-24.json)。

### 第二十九批：Result/try 与词法错误传播（历史记录）

用户函数已支持 `T ! E`、上下文确定类型的 `ok(value)`/`err(error)`、成功值自动包装、`try`，以及有名或匿名的终止式 `catch` 块。成功载荷覆盖现有非 Result 类型，包括 `String`、`Vec[Token]` 和拥有型结构体；错误载荷可以是拥有型结构体。`try` 要求调用方返回相同错误类型，传播前转移错误载荷，再清理仍存活的局部值、临时值和容器已初始化前缀。`() ! E` 函数允许空 return、`return ()` 和正常落空为成功。

真实编译器已迁移为 `lex → compile → main`：`lex` 返回 `Vec[Token] ! Diagnostic`，`compile` 用 `try` 返回 `String ! Diagnostic`，`main` 捕获拥有型 `Diagnostic`，按原格式打印后退出。Diagnostic 拥有 code/message 字符串和 byte/line/column 位置；解析、类型检查、C 生成仍在首个错误直接退出，多错误恢复未进入本批。

| 范围 | 第二十九批边界 |
| --- | --- |
| Result 的存储与消费 | 函数签名、局部注解和局部推断；拥有型 Result 在绑定、赋值、return、try/catch 时移动，借用参数不可搬出（E0401），已移动值不可重用（E0402） |
| catch | `catch name { ... }` 转移错误载荷，`catch _ { ... }` 释放错误；处理块必须终止，落空为 E0601；标准库 fs/String 错误仍使用既有 catch `_` 路径 |
| 借用 | 局部 Result 可携带静态 `str`；拥有 String 的视图装入 Result 明确 U0001，函数返回借用 str 仍不支持；借用错误载荷为 E0301 |
| 未扩展能力 | arrow catch、Result 字段、`Vec[Result]`、嵌套 Result、Optional、载荷枚举和多错误恢复；Result 比较为 E0501 |

本机扩展用例已通过 **372/372 cases、140 个可执行输入的 C 生成**：112 run、28 panic、190 reject、33 unsupported、9 shared_unsupported。Rust Debug/Release 各 **145 通过、0 失败、1 个既有忽略**。本批同时修复 Rust 和自举端的返回完备性缺陷：catch 表达式中的 `break` 必须算作目标循环的退出；18 个缺少返回的拒绝样本均报 E0013，6 个合法循环/闭包边界保留。

真实词法器单元已纳入自举门：直接拼接当前 `support.yv + lexer.yv + lexer_test.yv`，比较 stage1/stage2 C，在 Zig Debug/Release 执行正常退出的堆检查。最终本机门 **372/372 cases、12/12 checks、0 skip**，固定点 C 为 623,204 字节，两代 Linux 二进制均为 293,640 字节；420 份用例 C 与 560 个模式二进制均按回执复核哈希，见[本机验收摘要](../compiler/validation/selfhost-result-2026-11-24.json)。

`e355cb3` 的 [CI run 45226925936](https://github.com/msly/yavo/actions/runs/45226925936) **6/6 jobs 成功**：Linux x86-64、Windows x86-64、macOS arm64 各通过 372 cases、12 checks、0 skip，两代 C/二进制一致，真实 lexer 两模式正常返回检查通过。Linux GCC ASan/UBSan 通过 **141 程序（140 fixture + 1 lexer）**，stage2 自编译、372 checks、140 emits、8 个坏输入；三个平台各通过既有 CLI 的 213 次执行。三份原生归档已按 API 大小/哈希、提交源码与实际执行字节完成核验，见[第二十九批原生索引](../compiler/validation/batch29-native-2026-11-24.json)。

### 第二十七批固定点（历史记录）

**SH-2 在引导子集上闭合（2026-11-23，第二十七批，实现提交 `67794d9`）。** 按[决定 20–24](decisions.md)，`selfhost/` 的编译器源码改写为编译器自己实现的引导子集，不再要求原样编译改写前的源码。bootstrap.py 拼接 `COMPILER_PARTS` 得到 134,212 字节、3,600 行的单一源文件：

| 代际 | 结果 |
| --- | --- |
| stage0 → stage1 | Rust 编译器构建 Yavo 编写的编译器 |
| stage1 → stage2 | stage1 检查并生成自身的 C（574,149 字节，0.03 s），锁定 Zig `-O2` 构建 stage2（约 5 s） |
| stage2 → stage3 | stage2 生成的 C 与 stage2.c 逐字节一致，构建出的 stage3 与 stage2 二进制哈希一致；stage2 带 `YV_ALLOC_CHECK` 编译自身后正常退出，没有泄漏 |
| stage2 的外部行为 | 全部 335 个用例的检查退出码、stdout、stderr 与 stage1 相同，126 个可执行输入生成的 C 与 stage1 逐字节相同 |

完整本机门：**335/335 cases、11/11 checks、0 skip**（98 run、28 panic、172 reject、29 unsupported、8 shared_unsupported），记录在[固定点验收摘要](../compiler/validation/selfhost-fixpoint-2026-11-23.json)。原 321 个用例全部保留，`unsupported_text_slice` 原样晋升为正例，新增 14 例覆盖 inout、take、文本切片、`std/{fs, os}` 与 `catch _`，以及与 stage0 一致的拒绝码（E0001、E0102、E0014、E0401、E0601）。Rust 编译器、运行时和冻结规格本批未改，Rust 套件沿用上一批结果。

引导子集在原有能力（标量、Copy 聚合、无载荷枚举、String、Copy/拥有元素 Vec、表达式内只读索引、byte 与文本字节读取）之外新增：

- `inout` 参数，实参为 `&变量` 或 `&变量.字段`；同一调用中后续实参不得再使用该根（E0102），下标元素作实参与 stage0 一样为 U0001。
- `take(&place)`，限 String 与 Vec。
- 表达式内文本切片 `s[a..b]`（int 端点，P-INDEX/P-STR-BOUNDARY）；拥有字符串的视图不能绑定或赋值（U0001）。
- `use std/{fs, os}`、`os.args()`、`os.exit`；`fs.readText`、`fs.writeNewText`、`String.fromUtf8` 只配不落空的 `catch _` 块。

编译器内部做法随之改变：AST 改为 Copy 结构加列表池；解析、检查、生成的首个诊断直接退出，词法错误放在返回值里；多文件由驱动拼接，不实现模块。改写前的 `model.yv`、`diagnostic.yv` 冻结在 `selfhost/tests/prefixes/`，原有 prefix 工作负载继续运行。

### 第二十八批原生验收（历史记录）

第二十八批的 [run 45174913978](https://github.com/msly/yavo/actions/runs/45174913978) 在 `2a64808` 上 6/6 jobs 成功：首个 CLI 每平台 71 场景、213 次执行，三平台固定点各 335 cases、11 checks；Linux 的 stage2 自身 ASan/UBSan 完整门通过。三份原生归档均已下载和核验，见[批次索引](../compiler/validation/batch28-native-2026-11-23.json)。

## 下一步

| 顺序 | 工作 | 退出条件 |
| --- | --- | --- |
| 已完成 | 三平台确认固定点 | `4739ffc` 的 run 45150424099 六项成功，三平台原生固定点通过；见[独立 CI 索引](../compiler/validation/selfhost-fixed-point-native-2026-11-23.json)。原历史本机回执保留原样 |
| 已完成 | 第一个非编译器 CLI（SH-4 线） | [test-summary](../examples/test-summary/README.md) 使用 stage0，四模块首次 check/Debug/Release 通过，0 编译修复；独立输出、资源、可复现及三平台门已通过 |
| 已完成 | Result/try 与真实词法器验收 | `e355cb3` 的 run 45226925936 六项成功；三平台各372/12、零跳过，真实 lexer 与 Linux stage2 sanitizer 通过，归档已核验 |
| 已完成 | Copy Optional 与 `Vec.pop` | `7297e3b` 的 run 45345384747 六项成功；三平台各439/12，14 个真实截断点已迁移并删除五个 helper；拥有型 Optional/pop 保持 U0001 |
| 已完成 | `for` 与真实遍历迁移 | `046ccc6` 的 run 45354873591 六项成功；三平台各490/12、四处真实迁移、Rust端点快照修复及原生归档均已核验 |
| 已完成 | 插值与 `format` | 第三十二批：547 例，四处真实迁移，诊断与 C 字节不变；修复 Rust 输出片段借用缺陷 |
| 已完成 | 源模块 | 第三十三批：568 例；文件身份、导入图、逐文件名字/可见性、原文件诊断与 panic 位置 |
| 已完成 | 编译器自身改为多文件 | 跨文件调用加模块限定、补 `pub`/`use`，bootstrap 只交入口文件；固定点保持 |
| 已完成 | 借用返回 | 第三十四批：598 例；参数来源位掩码与路径合并；修复 Rust loans 在分支/循环中丢失的释放后使用 |
| 已完成 | `Vec[str]` | 第三十五批：623 例；容器来源合并；修复 Rust 对拥有型实参返回视图的检查 |
| 已完成 | 跳转式 `??` | 第三十六批：637 例；`?? return/break/continue`；修复 catch 块后来源未恢复的子集差异 |
| 已完成 | 只读 `[]T` 参数 | 第三十七批：663 例；`Vec[T]` 自动视图、len/下标/for |
| 已完成 | str 查询方法 | 第三十八批：675 例；startsWith/endsWith/contains/find/split/parseInt；修复 Rust 接收者锁 |
| 已完成 | 遍历拥有元素 | 第三十九批：693 例；`for x in Vec[String]`/结构体元素；Map 以外的 test-summary 已可编译 |
| 已完成 | Map 与 test-summary | 第四十批：733 例；selfhost 原样编译 test-summary，71 场景与 stage0 一致，纳入 bootstrap |
| 已完成 | std/io、视图绑定 | 第四十一批：756 例；`io.readAll`、trim/lines/fields、视图绑定、遍历临时 Vec；原记录 88/391，第四十二批重建基线为 91 |
| 已完成 | 带下标遍历与 if let | 第四十二批：本机 792/792、13/13、零跳过；三平台 CI 6/6；覆盖 91→92/391 |
| 已完成 | Copy 元组与单位换算 CLI | 第四十三批：本机 847/847、14/14、0 skip；原始双模块程序 20 场景全部通过；覆盖 92→98/391、0 越界；三平台 CI 6/6 |
| 已完成 | 整数 min/max 与文本 diff CLI | 第四十四批：本机 875/875、15/15、0 skip；三项应用回归；原有覆盖98→99/391，含新 P0 共100/392、0越界；三平台CI 6/6 |
| 已完成 | 整数 Vec 排序与窗口中位数 CLI | 第四十五批：本机897/897、16/16、0 skip；第四项应用回归28场景/112执行；覆盖100→106/392、0越界；三平台CI 6/6 |
| 已完成 | Map遍历、String排序与原子账本CLI | 第四十六批925/925、17/17、0 skip；第五应用32场景/128执行；覆盖106→112/392、0越界；三平台CI 6/6 |
| 已完成 | Set[String]及第二个正式独立应用csv-check | 第47批最终959/959、18/18，三平台CI 6/6；139固定案例，POSIX六产物828次/native414次、Windows822/411、Linux双份C sanitizer276次；SH-4更新为2/3 |
| 下一步 | 第三个独立多文件依赖/索引CLI | 冻结真实文件格式、跨文件依赖、模块及缺失/冲突/循环等失败语义；独立oracle与stage0自然实现，保持应用原文按实际阻塞扩充selfhost，再完成资源/复现/三平台门 |
| 接替阶段 | SH-3接替 | 完成普通check/build/run、稳定及结构化诊断，保持既有回归/冻结规格/资源/三平台门，再默认使用selfhost；cache/fix/verify按依赖迁移 |

固定点现在是回归门：任何修改 `selfhost/` 的提交都必须保持 stage2.c = stage3.c 与 stage1/stage2 全用例一致。

## 实现次序与约束

1. **先建立可运行的编译路径。** 从少量函数、基本值、局部变量、控制流与调用开始；解析到真实内部表示，完成检查后再生成 C。字符串、容器、结构体和模块按编译器自身需求扩展。
2. **让自举源码约束下一步。** Token 保存源码范围；AST 可使用拥有型节点加索引或 `Arena`/`Id`。避免通过长期借用普通字段等不受支持的表示方式绕过语言规则。先记录真实阻塞，再决定补库、改实现或修编译器。
3. **保持输入子集的语义一致。** 数值溢出、除零、求值顺序、可变性、作用域、返回路径和字符串边界继续按既定语义处理。不能把 C 编译通过当成 Yavo 语义检查通过，也不能静默接受不支持的构造。
4. **再覆盖编译器自身所需的所有权能力。** 集合、拥有型字段、借用与修改、确定析构、结果类型和模块逐项移植；每项都有外部正向与负向程序。检查器既要允许合法的编译器源码，也要拒绝对应非法程序。
5. **自举后再接替完整工具。** `check` 的稳定诊断和普通构建优先；缓存、修复、验证、包管理等按依赖关系迁移。Rust 的 JSON、哈希、进程驱动依赖无需在最小前端闭环之前全部重写。
6. **持续验证结果传播的提前退出清理。** Vec 字面量临时值在元素类型确定后立即更新为准确类型；后续元素内 `try` 提前退出时，只析构已初始化元素并调用对应析构函数。第二十九批已接入该路径，后续 Optional/pop 等扩展继续复用独立输出与资源检查。

需要改动 stage0 时，在 `compiler/` 中独立修复并加入针对性回归，不通过降低自举目标或修改冻结用例掩盖缺陷。通用闭包、泛型枚举、独立原生后端和完整包生态都不是预设的自举前置清单；只有实际实现证明必要时才升为阻塞项。

## 每次自举验收的记录

- 源码提交和完整输入清单，含运行时与模块；stage0 二进制哈希、工具链锁、目标、编译模式和有效参数。
- 每一代的构建命令、退出码、诊断、生成 C 和二进制哈希，以及构建耗时、峰值资源的测量条件。
- 单独的阶段输出目录，禁止从上一轮缓存或预生成文件冒充新阶段。自举验收使用可证明执行了前端的构建路径。
- stage2/3 生成 C 的逐字节比较。若有路径或时间差异，先消除非确定输入；必须规范化时，公布规则并保留原始文件。二进制在相同后端、路径约束和确定性配置下比较，不把哈希一致解释为语义正确性证明。
- 与阶段构建分开的外部正向、负向、边界和故障用例；与 stage0 的差分结果，以及独立确定的预期结果。两个编译器行为相同仍可能共享错误。
- 按平台列出已执行和未执行的项目。交叉编译成功不替代目标系统上的运行验证。

## 与自举并行的实用验收

| 维度 | 近期交付要求 |
| --- | --- |
| 非编译器程序 | 至少三个独立命令行项目，分别覆盖结构化文本校验、日志/数据统计、多文件依赖或索引处理；具有真实输入格式、模块划分和失败路径，不只是编译器内部单元测试 |
| 正确性与安全 | 覆盖输出、退出状态、数值边界、非法 UTF-8、I/O 失败、所有权冲突和资源释放；在可用的独立 C 工具链上执行 sanitizer 能力探针与应用测试 |
| 诊断与迭代 | 错误能定位到原始文件与范围，稳定错误码可供 agent 使用；不支持的能力、用户程序错误、编译器内部失败可区分；开发者能通过公开命令重现失败 |
| 构建与分发 | 锁定工具链，离线构建，空缓存重建可重放；独立目录与含空格/非 ASCII 路径可用；从干净检出构建编译器和示例有明确入口 |
| 平台 | Linux、Windows、macOS 原生运行声明支持的程序和诊断用例，分别公布范围；跨平台差异不得只靠 Linux 交叉编译掩盖 |
| 成本与性能 | 记录实际应用及编译器自身的耗时、内存与产物大小，区分冷构建、编辑重建与缓存命中；继续使用原规划的冻结性能门槛，不以自举结果替代 |
| 工程补缺 | 由真实项目推动测试入口、文件/进程 API、依赖锁定与互操作；最小闭环暂由外部驱动补位的部分，在工具交付中显式列明 |

上述门槛证明的是选定命令行工作负载的实用性，不要求先建立网络、数据库或 GUI 生态。自举之后优先消除这些真实应用反复遇到的阻塞，而不是继续以特性数量作为进度指标。

## 工作优先级

1. SH-1、SH-2 已完成；保持三平台固定点及已建立的 Linux stage2 编译器自身 ASan/UBSan 回归门。
2. 用stage0实现第三个独立多文件依赖/索引CLI；保持应用原文，按实际依赖扩充自举子集，延续既有回归、冻结规格和原生平台检查。
3. 逐步完成 SH-3 日常编译与诊断接替，并独立完成 SH-4 声明范围内的实用验收。
4. 既有 agent 成本实验和文档对照实验保留为后续产品证据；有授权的可计量环境后继续，不再作为自举开发的前置步骤。已有结果与未执行状态保持原样。

原规划的无 GC、接近 C 的性能、跨平台和 agent 交付目标不变。自举改变实现优先级，并增加一个大型真实程序作为持续验证对象。
