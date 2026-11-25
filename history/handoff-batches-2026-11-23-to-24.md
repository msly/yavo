# Yavo 交接记录：第二十七至第三十批（2026-11-23 至 11-24）

从 HANDOFF.md 原第 11–14 节原样移出。

## 11. 第二十七批：引导子集与自举固定点（2026-11-23）

- 实现提交 `67794d9`。改写 `selfhost/` 全部编译器源码（support/lexer/model/parser/types/codegen/main，3,600 行）；删除 `diagnostic.yv`，旧 `model.yv`、`diagnostic.yv` 冻结到 `selfhost/tests/prefixes/` 供三个历史 prefix 工作负载使用。`lexer_test.yv` 改为检查 `Lexed` 返回值，由 stage0 构建。
- bootstrap.py：`COMPILER_PARTS` 拼接 → stage0 构建 stage1 → stage1 check/emit 自身得 stage2.c → Zig `-O2 -DYV_ALLOC_CHECK` 构建 stage2 → stage2 emit 得 stage3.c → 比较 C 与 stage3/stage2 二进制；每个用例另跑 stage2 check（及可执行例的 emit）并要求与 stage1 完全一致。原 `self_compilation_not_yet_supported` 检查删除，receipt 改为 `achieved: self-hosted-fixed-point`、`self_hosted: true`。
- 新增 14 例：run `inout_params`、`text_slices`、`take_places`、`std_catch_args`、`text_slice_owned_print`（由 `unsupported_text_slice` 原样晋升）；panic `text_slice_boundary`、`text_slice_bounds`；reject `inout_missing_borrow`(E0001)、`inout_later_argument_use`(E0102)、`inout_immutable_local`(E0014)、`inout_move_out`(E0401)、`catch_falls_through`(E0601)；unsupported `unsupported_string_view_binding`、`unsupported_catch_binding`；shared_unsupported `unsupported_inout_index`。错误码全部先用 stage0 实测。
- 行为差异只有一处：原前端在循环重检（replayBody）里为对齐 stage0 吞掉非 E0402 错误，首错即退出后不再吞掉。全部旧用例未出现差异；若以后遇到 stage0 接受而子集在循环重检中报错的程序，先查这里。
- 性能：stage1 编译自身约 0.03 s，生成 C 574 KB；Zig 构建约 5 s。完整门约 2 分 40 秒（4 核服务器）。
- CI：`67794d9` 的 [run 45147299836](https://github.com/msly/yavo/actions/runs/45147299836) 中 Linux x86-64（含 Rust 全量与独立 ASan/UBSan 作业）、Windows x86-64 及其余三项通过；macOS arm64 的 selfhost receipt 显示固定点成立（stage2.c = stage3.c，574,333 字节；335/335），唯一失败是 `stage3_binary_reproduces_stage2`：Mach-O 的 ad-hoc 签名写入输出文件名，stage2/stage3 用了不同文件名。修正为两代都输出为各自目录下的 `yavo-selfhost`（`4739ffc`），其 [run 45150424099](https://github.com/msly/yavo/actions/runs/45150424099) **6/6 jobs 成功**：Linux x86-64、Windows x86-64、macOS arm64 原生 selfhost 步骤全部通过，固定点在三平台原生确认。下载 macOS 归档用的是本机 git 凭据调用 artifacts API。
- 第二十七批当时未覆盖：stage2 本身未跑 GCC ASan/UBSan（sanitizer 作业仅覆盖 stage1 生成的程序 C）。第二十八批补齐，见第 12 节。

## 12. 第二十八批：真实 CLI 与 stage2 内存检查（2026-11-23）

- 首个应用 `examples/test-summary/` 包含 main/parser/stats/report 四模块；SPEC 明确日志语法、重复项、汇总不一致、错误码、路径转义与多文件输出原子性。真实输入来自第四轮 Yavo/Rust 验证日志。无需更改编译器或运行时。
- 作者首次完整源码检查、Debug 和 Release 构建全部通过；完整三次调用、源码快照、诊断和工具身份在 `examples/test-summary/evidence/`，后续验收或修复追加记录，不替换首次结果。真实 usage 不可用，token/成本保持未知。
- 独立验收由 `examples/test-summary/tests/` 的固定期望与独立 Python oracle 组成，冻结后再执行应用。72 场景中每平台 71 适用，另外 1 项是目标系统不支持的原生文件名形式；三次独立空缓存构建，Debug/Release 逐例输出比较，Release C/二进制身份比较。此处计数是单应用场景，不是 71 个独立应用。
- 应用最终 Linux 门为 213/213（Debug/Release 142，加退出检查辅助产物 71）；原始 C/runtime 不改写，通过独立 forced-include shim 在显式退出前检查堆块。clean/leak 两个真实 String 探针先验证辅助检查有效。runtime、shim、probe、生成 C 均从全新目录的哈希绑定副本编译；三次原生构建的源副本、C、binary 及辅助产物运行后全部复验，避免同名头遮蔽或工件在执行中变化。见 `compiler/validation/test-summary-2026-11-23.json`。
- `tools/ci/selfhost_sanitize.py` 保留原 126 个生成程序门；新增 `selfhost_compiler_sanitize.py` 只额外 GCC 编译一次实际 stage2 C，随后执行自编译、335 checks、126 emits 与 8 个坏输入压力用例。当前 stage2 C 与 runtime 复制到全新构建目录并逐字节核对，防止同目录同名头遮蔽已绑定 runtime。
- 独立 GCC 14.2 本机验证通过，详细范围及来源绑定见 `compiler/validation/stage2-sanitizer-2026-11-23.json`；正常编译器退出有分配检查，首错 `os.exit` 路径不声称泄漏检查。LSan、TSan 和其他平台 sanitizer 不在本批范围。
- 初始提交 `aa8eb56` 的 run 45172624540 中，macOS 全部测试（含应用 213/213）成功，唯独 artifact 上传拒绝测试有意构造的 CR/LF 文件名。修正提交 `2a64808` 增加 `package_test_summary.py`：原始名字与字节打入 tar.gz，保留空目录，仅省略可重建缓存；workflow 排除直接上传原目录并上传容器。应用与 oracle 未改，本机封装检查覆盖 1,324 个文件及 453 个目录。初始失败保留在 `compiler/validation/batch28-ci-initial-2026-11-23.json`。
- 修正后的 [run 45174913978](https://github.com/msly/yavo/actions/runs/45174913978) **6/6 jobs 成功**。三平台应用各 213/213，自举各 335/335、11/11；Linux 完整 stage2 sanitizer adapter 通过。三份原生 ZIP 已下载并匹配 GitHub 大小/SHA-256，离线复核实际应用输出、源副本、Release 产物、固定点 C/二进制与 Linux stage2 命令流；runner 原始 stage0/Zig/GCC 库未包含在 ZIP 中的身份仅按回执记录。最终来源与结果在 `compiler/validation/batch28-native-2026-11-23.json`。SH-4 目前完成 1/3 个独立应用，Rust stage0 继续承担日常编译。

## 13. 第二十九批：Result 与真实词法错误传播（2026-11-24）

- 基线 `aecd7c3`。`TypeRef`/Registry 为 Result 的成功与错误类型建立独立池；C 表示按活动分支移动与析构。`try` 在清理临时值前保留错误返回值，Vec 字面量在首元素类型确定时登记准确类型，长度只计已初始化元素。
- `lex` 返回 `Vec[Token] ! Diagnostic`，`compile` 用 `try`，`main` 捕获后沿用首错格式与退出码。Diagnostic 自有 code/message，不借用 source/path。parser/types/codegen 仍是首错进程退出，尚无诊断累积。
- 新增 37 项：14 run、18 reject、4 subset unsupported、1 shared unsupported；总计 112 run、28 panic、190 reject、33 unsupported、9 shared。原用例源码保留；`fs.Error` 的有名 catch 不因用户 Result 功能而晋升。独立 stage0 正例执行 28/28，早期 stage1 Debug 实际执行 14/14；原始失败尝试和 6 个首错位置差异保留在 `selfhost/tests/evidence/batch29-result-oracle/`。
- 审查修复两个边界：Result 的聚合比较报 E0501；把拥有 String 的 transient view 包装到 Result 报 U0001，避免丢失借用来源。静态 str 成功载荷仍可用；arrow catch、Vec[Result]、Result 字段、嵌套 Result 保留各自明确边界。
- 修复 Rust 与旧 subset 的 catch-break 缺陷：break 隐藏在表达式中不能把可退出 loop 误判为永久循环。`compiler/tests/p0/errors_catch_break_return.yv` 覆盖 18 个 E0013 与 6 个合法边界；`compiler/validation/catch-break-return-2026-11-24.json` 记录旧 C 的可达落空路径，未执行缺少返回的程序。
- 真实 lexer 单元现由 stage0 与 selfhost 构建。原断言保留，三个 for 改为 while 以满足已声明子集；预期坏输入被捕获，16 轮 32 个拥有 token 后的 escape 失败保留第一错误，source/path 释放后 Diagnostic 仍可读。测试成功路径正常返回，stage1/stage2 C 一致，Debug/Release 堆块检查通过；同一源码和 C 作为第 141 个 Linux GCC sanitizer 程序通过。
- Rust 完整 Debug/Release 各 145/0/1、冻结 124/3/2 通过；本机自举 372/372、12/12、0 skip 记录在 `compiler/validation/selfhost-result-2026-11-24.json`。原摘要的 implementation_commit=null 及 native pending 保留原时点语义，后续 `e355cb3` 的 run 45226925936 六项成功与归档核验写入 `compiler/validation/batch29-native-2026-11-24.json`，685 个本机测试输入路径已与该提交逐项对照。
- Windows 归档中 36 对 reject-new/reject-existing 记录内容（含耗时 0.0）相同。离线审计器原“记录内容唯一”假设误报，改为 3,140 条嵌入/保存记录多重集完全一致、逐份核验实际流；首失败与完整证明已保留于最终索引。三平台最终审计通过；未改编译器或重跑 CI 来处理这项审计器问题。
- SH-3 仍进行中，SH-4 为 1/3；当批提出的 Optional/pop 已在第三十批完成，后续范围见第 10 节。

## 14. 第三十批：Copy Optional 与编译器原地退栈（2026-11-24）

- 基线 `e3222d8`。Copy Optional 使用 has/val C 布局；支持 int、byte、bool、非拥有型 struct 和无载荷 enum。pop 对可写 local/field/inout place 定位一次，沿用 readable/writable/预留锁，再按空或非空返回；结果不保留 Vec 借用。
- 普通值 `??` 右结合，RHS 惰性求值且不隐式包装；分支内清理新增临时值，合并移动状态。私有 Hint 表示上下文中的额外 Optional 层，仅检查类型，禁止虚拟值进入 C。`let x:?int=none??1` 为 E0001；`let x:?int=some(1)??2` 合法；实际嵌套 Optional 为 U0001。
- 分离 raw context 与 fit，修正原 subset 对 Result 字面量的误接受：`?byte=1`、`byte ! E` 的 `return 1`、`Vec[int] ! E` 的 `return []` 均按 Rust 报 E0001；some/ok 构造器和显式转换提供相应直接上下文。Rust 实现未改。
- 删除五个 trunc helper，14 处改为 while + pop。`cleanTemps`、`cleanLocals`、`readable`、`writable`、`takeValue`、`refPlace`、`recordExit` 与基线逐函数字节相同；锁栈保留外层 mark，循环状态先复制再缩短。消除保留前缀的重复复制，尚无性能测量结论。
- 正式语料 439：127 run、28 panic、218 reject、52 unsupported、14 shared。净新增 67 例，原 unsupported_vec_pop 原字节晋升为 vec_pop_basic；本批覆盖 68 项，15 个 run 的独立 Rust Debug/Release 共 30 次执行通过。首轮输入修正与诊断位置差异保留在独立 oracle 中，不按 selfhost 输出修改预期。
- 拥有型/String/Vec/str/unit Optional、嵌套/字段/容器/Result 组合、owned pop、if let、match、跳转式 ?? 均继续明确 U0001；Optional 比较和直接打印与 stage0 一样为 U0001。
- `7297e3b` 的 run 45345384747 六项成功；三平台原生与归档审计通过，详见 [batch30-native-2026-11-24.json](compiler/validation/batch30-native-2026-11-24.json)。原本机回执的 implementation_commit=null/native pending 保留原时点语义。SH-3 未接替、SH-4 保持 1/3，下一步见第 10 节。
