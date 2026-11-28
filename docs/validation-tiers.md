# 自举验收分层（第 52 批起）

每批保留三平台真实固定点、核心语义和全原语料分类；重复产品矩阵与归档审计移至完整门。策略由 [`validation-policy.json`](../tools/ci/validation-policy.json) 驱动，当前批次 53，基线 `5fa6401`，间隔 **N=5**；本批新增拥有型枚举布局与清理，按里程碑执行 Full。

| 层级 | 触发 | 必跑项目 | 不计作本层通过的项目 |
| --- | --- | --- | --- |
| Core | 每批、本机及 Linux/Windows/macOS 原生 CI | stage0 构建；stage1→2→3；两代 C/二进制固定点；lexer；稳定核心用例＋本批新增/变更用例；原 622 份语料 check 分类与全部已识别阻塞；源码在运行中不变；自编译耗时/RSS/C 与二进制大小 | 六产物应用矩阵、独立 sanitizer、全量 Rust、归档成员审计 |
| Full | 第 55、60、65…批；版本 tag；手动 `tier=full`；策略中 `milestone=true` | Core 全部；所有语义用例/应用；三前端×两模式；独立 ASan/UBSan；原 SHA256 专项；完整诊断/驱动门；原生回归；归档与身份复核 | Windows/macOS 尚未移植的 Rust 套件及平台不适用场景仍单列 |

手动 `core` 不能覆盖批次周期、tag 或 milestone 的 Full 要求。所有权模型、析构/借用、ABI、运行时或 C 后端的大改在合入前将 `milestone` 置为 true；普通语法扩展不自动升级。每批更新 batch 和 baseline_commit；Full 失败必须修复或明确阻塞，不能用下一次 Core 成功代替。

`bootstrap.py` 默认 Core。`core_cases.py` 保留明确的语义骨架，并从 Git 基线比较 manifest、夹具和模块源码，把新增或改变预期的用例自动加入；未入选项目记为“不在本层”，不伪装通过。Full 使用全清单，保留既有重型入口。固定点的字节比较和最小来源标识成本很低，Core 仍保留；延后的是专门的 SHA256 场景矩阵与逐归档成员审计。

```sh
python3 selfhost/bootstrap.py --tier core
python3 tools/ci/subset_core.py --bootstrap-output compiler/target/ci/selfhost
python3 tools/ci/selfhost_metrics.py --bootstrap-output compiler/target/ci/selfhost
python3 selfhost/bootstrap.py --tier full
```

完整门的六产物、sanitizer 和打包由 CI 条件执行。每批报告只给交付、覆盖变化、失败/修正和下一批；命令、样本、平台差异及详细数字写入 `compiler/validation/` 索引。旧批次的 freeze、失败日志、ZIP 审计与原源码保持为历史证据。

## 资源口径

`selfhost_metrics.py` 对同一固定源码运行一次预热、五次真正 stage2 `emit-c`，逐次记录 wall time 和 OS wait4 RSS，输出 P50/P95、峰值、C 大小与 compiler 大小；另测一次后端构建。前端、后端分别报告，不用前端时间冒充完整构建时间。Windows 无 wait4 时 RSS 留 null 并写原因。已有 OS/Zig 缓存不清空，明确标记非冷构建；编译器固定点使用的分配计数设置保持不变。

小程序 P50≤200 ms、Linux strip 后≤250 KB、运行时相对 C 几何平均≤1.5 继续由 `docs/plan.md` 定义。自编译指标与小程序门槛分列；PCH、冷/热缓存、Debug/Release 必须分别建表。

## 第52批执行耗时

同一仓库前后两次实际原生作业耗时如下，包含工具链准备、缓存与网络；用于观察验收负担，不是编译器性能对照。

| 平台 | 第51批 Full | 第52批 Core |
| --- | ---: | ---: |
| Linux x86_64 | 20分41秒 | 3分1秒 |
| Windows x86_64 | 81分30秒 | 8分36秒 |
| macOS arm64 | 20分31秒 | 3分48秒 |

原始job/step时间及精确提交见[第52批索引](../compiler/validation/batch52-direction-2026-11-28.json)。本批没有下载或逐成员审计ZIP；完整门在约定周期执行。

第53批修正RSS启动器污染：Full回执在单独短命进程读取，测量进程只接收路径与身份，避免JSON展开的历史高水位进入编译器子进程统计。使用直接编译器wait4，仍保留Python启动下限；Windows无wait4仍null。旧记录不回写，跨方法比较以[同口径复测](../compiler/validation/batch53-metrics-correction-2026-11-28.json)为准。

第53批第三次CI的Windows全部执行、打包和上传步骤成功，总90分19秒导致90分钟job超时取消；完整上传归档已独立审计。自dca2d52起，Full原生job预算为120分钟，Core仍90分钟；触发规则和必跑范围保持本表定义。取消记录与修正后CI分别保留在本批索引。

该轮Windows前五项为bootstrap 37分58秒、csv-check 9分25秒、dep-index 9分22秒、Rust Debug/Release 6分29秒、test-summary 5分24秒，合计68分38秒，占90分19秒的76%。bootstrap与原生应用门确有同源stage0参考构建，但后者使用隔离源码快照、相对入口、空缓存并禁用PCH，当前不满足直接共享条件。后续先分析三项应用的参考构建契约能否统一；保留双前端、双模式、独立Release重建、exit-shim及缓存状态的区别。逐step时间尚未拆分构建与用例执行，不据此估计可节省分钟数。
