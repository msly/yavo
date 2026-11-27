# 自举验收分层（第 52 批起）

每批保留三平台真实固定点、核心语义和全原语料分类；重复产品矩阵与归档审计移至完整门。策略由 [`validation-policy.json`](../tools/ci/validation-policy.json) 驱动，当前批次 52，基线 `61b17c4`，间隔 **N=5**。

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
