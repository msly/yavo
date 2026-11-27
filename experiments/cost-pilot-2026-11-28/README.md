# 交付成本审计与十二题轻量复测（2026-11-28）

**Yavo 与 Rust 已做过对照，尚无证据证明 Yavo 的交付成本更低。本次新受试会话为 0，token 计量仍阻塞。** 本目录补齐历史证据分级、可直接执行的十二题 CLI 计量入口和当前失败回执；没有把准备工作或既有程序回放写成新实验。

## 已有结果

| 轮次 | 题目/独立运行 | 正确性 | 编译次数 Yavo / Rust | token Yavo / Rust | 证据强度 |
| --- | --- | --- | --- | --- | --- |
| R3 | 12 题，各 1 次 | 冻结运行用例双方 18/18；非修复题首次正确 3/11、11/11 | 23 / 14 | 报告值 165,176 / 124,860，比例 1.3229 | 任务级 LOG 可重算编译次数；无原始 provider usage |
| R4 | 同一 12 题，各 1 次 | 冻结用例双方 18/18；首次正确 8/11、11/11 | 16 / 12 | 报告值 162,464 / 127,801，比例 1.2712 | 同上；封卷后 T10 前导零抽查 Yavo 失败、Rust 通过 |
| R5 | 12 题，每语言 2 次 | 双方 24/24，含隐藏判定和结构审查 | 24 / 24，失败均 0 | 未取得 | 原始操作日志、源码快照、独立裁判可核对；不是 24 个独立题目 |
| R6 | 准备态 | 新受试会话 0 | 不适用 | 未取得 | API/CLI 计量资格未通过 |

R3/R4 的额外构建为 11/2、4/0，包含环境错误、输出文件冲突和运行结果修正，不能全部记成编译器诊断后的代码修复。R5 两语言的修复轮数均为 0；受控操作 138/132，用时 671.2876/557.0373 秒。代码与文档字节数、工具次数、耗时都不能补算缺失 token。

[historical-audit.json](evidence/historical-audit.json) 从 R3/R4 任务 LOG 重算逐题构建次数，从 R5 `events.jsonl` 和既有裁判重算已记录成本。旧 token 数字明确列在 `reported_token_totals`，未声称本次取得原始计量。

## 本次执行阻塞

服务器没有 Codex/Claude CLI，未提供 API key。本地已有 `codex-cli 0.172.0-alpha.2`，正常执行 `codex exec --ephemeral --sandbox read-only --json` 后在初始化 app-server 时失败：`Read-only file system (os error 30)`，没有模型输出或 `turn.completed`。没有改变访问控制、移动凭据或重设 HOME/CODEX_HOME。

实际入口探针见 [cli-probe/probe.json](evidence/cli-probe/probe.json) 及 [stderr.txt](evidence/cli-probe/stderr.txt)。失败发生在受试运行之前；当前无法生成真实新 token 对照结果。需要能正常执行现有登录会话的 Codex CLI 主机，或已授权且能返回 usage 的模型执行接口。这是执行环境缺口，不依赖 SH-3。

追加的有限诊断使用[官方配置项](https://learn.chatgpt.com/docs/config-file/config-reference) `sqlite_home` 与 `log_dir` 指向本任务临时目录：状态库和日志库实际创建成功，初始化仍在 5.45 秒后报 EROFS，事件和 usage 仍为空。此次新建日志中没有对应的 EROFS 错误记录；系统禁止 ptrace，未取得具体失败文件操作，不能把剩余失败唯一归因于默认 `state_5.sqlite` 路径。诊断至此停止，十二题协议不变；仅保留[脱敏配置探针回执](../../compiler/validation/batch52-cli-config-probe-2026-11-28.json)，未归档数据库、凭据或无关日志。

## 轻量复测口径

`pilot.py` 沿用 R5 已验证的十二份规格及隐藏用例，标记为同题复测。使用当前 Yavo stage0 对照 Rust，回答语言交付成本；selfhost 的兼容覆盖另行计量。每题/每语言单独上下文；同一请求模型和推理强度，按题交替 Yavo→Rust、Rust→Yavo，最多四轮；修复轮携带此前完整源码与公开失败，所有重复输入计入实际 token。

模型只返回源码 JSON，控制器负责编译、公开测试和反馈。Yavo 提供当前完整速查卡，文档输入计入 token；Rust 使用模型已有知识。两个语言都不能查阅仓库、参考程序或隐藏答案。该实验测量的是受控编程循环，不覆盖开放工具 Agent 的全部工作流。

每次模型调用保存原始 JSONL、最终源码和 `turn.completed.usage`；记录 input/output/cached token、实际编译次数、失败次数、修复轮数。cached 是 input 的子集，不重复加总；没有价格表时货币成本保持 null。CLI 只报告请求模型标识，未报告的服务模型标识保持 null。该先导比较接受 CLI 的真实回合计数，不要求 R6 的逐 provider 请求身份链。

编译、公开测试失败保留，并作为下一轮反馈；十二题双方全部封卷后才运行隐藏测试。T04 多文件职责、T06 真实线程结构须审查最终源码，在 `contract-reviews.json` 记录通过状态、理由及匹配的 `source_manifest` 后重算汇总。未经审查只报告行为正确率，完整契约正确率保持 null。

输入与编译器身份记录一次；不增加 ZIP、六产物或 sanitizer 门。4 项离线检查只验证计量缺失不记零、缓存不重复计数、源码路径边界以及精确输出/进程超时。它们没有发生模型调用。

## 执行入口

在有可用 Codex CLI 和两套编译器的同一主机，从仓库根目录执行；`MODEL` 和 `EFFORT` 使用两组相同且已确定的配置，`OUT` 必须是新目录，建议放在 `compiler/target/`，避免生成的实验源码混入原语料覆盖分母。

```sh
python3 experiments/cost-pilot-2026-11-28/pilot.py probe --out "$OUT/probe"
python3 experiments/cost-pilot-2026-11-28/pilot.py run \
  --out "$OUT/paired" --model "$MODEL" --effort "$EFFORT" \
  --yavo "$YAVO_COMPILER" --rustc "$RUST_COMPILER"
```

若 Rust 依赖固定 Zig 链接器，添加 `--rust-linker "$RUST_LINKER"`。`run` 自带同配置探针，只有它取得完整 CLI usage 才启动受试任务。`--max-attempts` 默认 4；请求参数、编译器和任务文件身份写入 `run.json`。缺失 usage 或 CLI 失败立即保存回执并停止，不继续积累不可计量样本。

```sh
python3 experiments/cost-pilot-2026-11-28/audit_history.py \
  --out experiments/cost-pilot-2026-11-28/evidence/historical-audit.json
python3 experiments/cost-pilot-2026-11-28/pilot.py summarize --out "$OUT/paired"
```

汇总同时给总 token、每份行为正确交付的 token、最终正确率、编译失败和修复轮数。失败任务的消耗仍计入总成本。小样本用于找成本差异与机制，不验收“至少低 20%”的正式统计目标。
