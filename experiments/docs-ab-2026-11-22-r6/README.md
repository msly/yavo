# 第六轮准备：短版文档、真实 usage 执行器与八题对照

**准备完成，真实计量资格仍为 `blocked`；受试会话 0、交付 0。** 本目录没有第六轮正确率、token 成本或语言效率结论。第五轮已完成结果继续保留在 [R5 报告](../coldstart-2026-11-22-r5/README.md)。

## 已落地

- 新增 [最小编写说明](../../docs/agent-start.md) 与 [按需 API 索引](../../docs/agent-api-index.md)。启动卡 134 行、8,936 字节；索引 33 行、3,607 字节。两份合计 12,543 字节，比原 32,889 字节完整卡少 61.9%；这只是文档字节数，尚未测量模型 token 或成功交付成本。
- 四个从 Markdown 原样提取的完整项目和八个针对性规则检查全部通过：共 8 个成功编译运行的程序、4 个按预期拒绝的反例。覆盖非 Copy 元素借用、克隆、V2/V2a、RHS 求值顺序、模块、线程和字面插值转义。[源码与验证回执](../../compiler/validation/agent-start-2026-11-22.json)。Linux CI 会直接重编文档示例并检查诊断。
- [Responses 执行器](../../tools/agent_eval/README.md) 逐请求保留 provider 请求/响应标识、原始 usage、服务返回的模型及配置、工具调用和实际输出。仅使用显式授权环境的 API 凭据；不读取 ChatGPT 浏览器凭据。未知计量保持 null，缺失时停止下一步。
- 八道新规格、16 个公开输入、96 个隐藏输入已准备；两次干净重建一致，另一份独立实现重算全部 112 个输入也一致。两个文档臂各计划两个空白会话，共 32 份交付；当前全部未启动。[任务](TASKS.md) · [协议](PROTOCOL.md) · [题包验证](evidence/packet-preflight.json) · [独立复核](evidence/oracle-review.json)。

## 对照时实际提供的文档

两组共用相同的 API 索引和十八个按需主题；只有初始 guide 不同。分页固定为最多 8192 个 UTF-8 字节，不切断字符，首次构建前必须读完 guide 与 index。

| 预先确定的输入 | full | compact |
| --- | ---: | ---: |
| guide 字节数 | 32,889 | 8,936 |
| guide 页数 | 5 | 2 |
| 共同实验索引字节数 | 3,847 | 3,847 |
| 必读 payload 字节总数 | 36,736 | 12,783 |
| 最少必读操作数 | 6 | 3 |

这是固定文档处理的大小和页数，不是受试者已发生的读取或 token 消耗；compact 的后续检索、修复和失败成本仍须全部计入。实验索引由原索引移除仅指向短卡的启动指令，并为两组添加同样的 lab 调用说明；转换与所有源码范围记录在 [文档清单](docs/document-manifest.json)。初始必读 payload 减少 65.2%，是否保持正确率并降低总成本尚待真实运行。

## 当前阻塞

新的官方 stdio app-server 初始化路径在打开 `/home/agent/.codex/state_5.sqlite` 时失败；没有绕过只读状态目录或移动凭据。当前环境没有 API key。现有 CLI 的 turn 汇总及 app-server 的 thread/turn token 通知，也不能单独提供本协议要求的逐 provider 请求身份链。[实际探针与原始输出](evidence/executor-probe.json)。

配置模板中的请求模型、预期服务模型和 runtime freeze 哈希保留 null。执行器实际运行该模板后，在网络请求前拒绝：provider 请求 0、受试会话 0。[启动资格回执](evidence/launch-qualification.json)。离线合成记录只验证契约，不会被接受为真实资格。

## 在具备计量凭据的 Linux 主机上执行

1. 提供已授权的 `OPENAI_API_KEY` 环境变量；在 `EXECUTOR_CONFIG.json` 显式选择请求模型、预期返回模型、参数和限额。此文件不存密钥。构建 Release 编译器并安装项目固定的 Zig 工具链。
2. 运行 `python3 experiments/docs-ab-2026-11-22-r6/tools/freeze.py`；它核对准备清单，绑定实际编译器、后端、文档、题包、prompt 和模型策略，创建不可覆盖的 runtime freeze，并回填配置中的 freeze 哈希。先提交这份冻结配置。当前准备清单不是已通过的模型资格。
3. 执行下面的资格链。它必须真实完成“模型请求 → 固定只读 probe → 模型续答”，两次响应都保存模型、usage、配置和唯一标识。只有 qualified 回执可以进入受试运行。

```sh
yavo_r6_packet=experiments/docs-ab-2026-11-22-r6
python3 tools/agent_eval/executor.py qualify \
  --config "$yavo_r6_packet/EXECUTOR_CONFIG.json" \
  --out "$yavo_r6_packet/executor-qualification"
python3 tools/agent_eval/executor.py verify \
  --config "$yavo_r6_packet/EXECUTOR_CONFIG.json" \
  --qualification "$yavo_r6_packet/executor-qualification/qualification.json"
python3 tools/agent_eval/executor.py run \
  --config "$yavo_r6_packet/EXECUTOR_CONFIG.json" --run full-a --prompt PROMPT.md \
  --qualification "$yavo_r6_packet/executor-qualification/qualification.json" \
  --out "$yavo_r6_packet/executor-runs/full-a"
```

其余会话按协议的两波顺序独立启动；四个会话全部封卷后才做隐藏裁判。运行器把原始请求、响应、源码和失败记录写入实验目录，交卷后统一核对并提交；程序与缓存排除在版本控制之外。结果汇总重新验证真实请求链，不直接相信 `run.json` 中填好的数字。

## CI 验证范围

新增的 Agent evaluation preparation 任务检查执行器、记录器和汇总契约、题包与文档可重建性，以及未获资格时确实拒绝初始化。该任务通过只表示准备材料和拒绝条件有效。Linux 的文档检查独立记录 12 项结果；既有完整 Cargo、Windows/macOS 选定原生套件、sanitizer 与 R5 证据检查继续保留。

本批没有改编译器、运行时、语言语义或 R5 冻结输入。之后仍需先跑这个有真实计量的文档对照，再进入 60 题 pilot；相对 Rust 的正式成本门和完整 v1 验收保持未完成。
