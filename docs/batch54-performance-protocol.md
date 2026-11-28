# 第 54 批：独立 runner 的性能复测与体积归因

**同一独立 runner 上，52→53 后端自编译 P50 为 6.394→6.645 秒（+3.92%），前端为 43.422→44.785 ms（+3.14%）。两基线及当前 54 的 12 个 PCH arm 全部达到本组条件下 P50 ≤200 ms。C 文本缩短 16.02%，ELF 增长 4.53%；增长的 97.56% 来自 `.text`，纯字符串拼写对照生成完全相同的二进制。**

结果依据：[正式结果与输入身份](../compiler/validation/batch54-paired-performance-2026-11-28.json)、[原始报告，原字节保存](../compiler/validation/batch54-paired-performance-raw-2026-11-28.json)、[历史 C/ELF 静态证据](../compiler/validation/batch54-static-size-2026-11-28.json)。[性能 producer run 46132191802](https://github.com/msly/yavo/actions/runs/46132191802)，head `cec3862644c477d1d3fb1e8e4bba8ad9636abbf8`，job `140939620862` 于 2026-11-28 09:53:50 UTC 成功完成。

## 实际测量：52→53

独立 Ubuntu 24.04 hosted VM，Intel Xeon 6975P-C、4 vCPU、固定 affinity `[0,1,2,3]`，Rust 1.99.1、Zig 0.17.1。52/53 在同一 VM 交替执行；计时和 RSS 来自低内存 C supervisor 的真实 `wait4`。199 条记录全部复核，包含 3 条计量控制、24 条预热和 172 条性能样本；117 份源码身份逐一对照精确 Git blob，512 项归档文件身份匹配。每条采样前后都保留负载，1 分钟 loadavg 范围 1.067–1.817；未观察到外来已识别构建进程或 affinity 变化。50 ms 轮询的名称和短进程局限见后文隔离合同。

| 指标 | 每版保留样本 | 52 P50 / P95 | 53 P50 / P95 | 两组 P50 变化 | 峰值 RSS 52→53 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 自编译前端 | 9 | 43.422 / 46.008 ms | 44.785 / 54.699 ms | +3.14% | 26.777 → 27.250 MiB |
| 自编译后端 | 7 | 6.394 / 6.435 s | 6.645 / 6.779 s | +3.92% | 195.812 → 200.996 MiB |

7 对后端样本全部是 53 较慢，逐对差值为 +0.208–0.404 秒；中位配对差值 +0.264 秒，中位配对比 +4.15%。前端 9 对中有 8 对较慢，中位配对差值 +2.501 ms、中位配对比 +5.97%。配对比和两组 P50 的比值属于不同统计量，报告保留两者。P95 是 7/9 个样本的描述性次序统计量。

计量控制中，`/bin/true` 在 Python 父进程持有 64 MiB 前后的峰值 RSS 都为 1,324 KiB；已知 32 MiB child 分配得到 42,844 KiB，低内存 supervisor 没有把 Python 报告内存变成编译器 RSS 下限。

## 四例编辑后编译：本 runner 全部达标

下表单位均为 ms；plain 与 PCH 都要求 build-cache miss，同轮输入字节一致，PCH 首轮 miss、保留样本持续命中同一 PCH 身份。每个 arm 为 1 次预热和 7 次保留样本。

| 用例 | 52 plain P50 | 52 PCH P50 | 53 plain P50 | 53 PCH P50 | 当前 54 PCH P50 | 当前 54 PCH P95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| integer_recurrence | 143.864 | 115.080 | 142.821 | 113.388 | 115.713 | 116.738 |
| vec_transform | 154.431 | 126.148 | 153.328 | 124.890 | 123.287 | 124.199 |
| mutref_scatter | 153.592 | 126.004 | 155.489 | 126.054 | 122.200 | 122.833 |
| deque_ring | 156.938 | 128.153 | 156.439 | 128.936 | 125.971 | 126.707 |

52/53 中 PCH 相对对应 plain 节省 27.50–29.44 ms，约 17.58%–20.61%；同轮二进制完全一致，最终输出均与独立 Rust oracle 相同。当前 54 在主配对结束后单独观察，以上四个值用于检查当前目标；没有据此宣称借用来源缓存带来对 53 的因果加速。

12 个 PCH arm 都未超过 200 ms，按预先固定的条件没有执行追加阶段诊断，实际新增 phase 命令为 0。旧主机 205.947–221.043 ms 的超标原因仍未被该实验分解；不能把它与新 Xeon runner 的差值归因于 cordis-rs 并发或本批优化。这里确认的是该 runner、四个小程序、Release/native、热 PCH 和编辑后 miss 路径下的目标，正式的一般性能目标仍未标记完成。

## 体积：源码拼写缩短，实际代码增加

历史共享服务器和本次 CI 的 ELF/C 是两组不同身份的产物；旧记录只有 Linux/x86_64，没有 CPU 型号，不能补用当前机器型号。两组保留各自源码绝对路径、产物 SHA 和编译 argv；同为 Zig 0.17.1、`-std=c17 -fno-strict-aliasing -DYV_ALLOC_CHECK -O2 -s`，仍不把跨主机绝对字节或时间拼接成一条趋势。

| 产物组 | C 字节 52→53 | C 变化 | ELF 字节 52→53 | ELF 变化 | `.text` 变化 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 历史服务器 | 1,780,336→1,494,217 | −286,119（−16.07%） | 733,152→766,512 | +33,360（+4.55%） | +32,560 |
| 本次独立 CI | 1,778,752→1,493,832 | −284,920（−16.02%） | 739,576→773,080 | +33,504（+4.53%） | +32,688 |

本次 CI 的文件增长由 `.text +32,688`、`.rodata +384`、`.eh_frame +376`、`.eh_frame_hdr +64`、`.data.rel.ro −16` 和节外布局 `+8` 构成。对 unstripped ELF 重新解析后，loadable 节与 strip 基线逐字节一致；稳定函数名映射核对了全部定义、参数数、实际入口及九个模块的函数体锚点。

| `.text` 归属 | 53−52 字节 |
| --- | ---: |
| `codegen` | +28,609 |
| `types` | +1,817 |
| `parser` | +1,628 |
| 生成 helper | +353 |
| `main` | +184 |
| 未被符号 size 覆盖的 padding/布局 | +97 |
| **合计** | **+32,688** |

主要函数或相关函数组如下；完整逐符号差值见正式结果。

| 函数 / 函数组 | 53−52 字节 |
| --- | ---: |
| `codegen.declarations` | +7,275 |
| `codegen.enumValue` | +6,421 |
| `codegen.patternsComplete` | +4,360 |
| `codegen.bindPattern` | +3,558 |
| `codegen.patternConstructor` | +2,654 |
| `codegen.patternCondition` | +2,321 |
| `codegen.printValue` 与 `outputValue` 合计 | +1,687 |
| `codegen.optionalLifecycle` | +1,351 |
| `codegen.cString` | +910 |
| `parser.pattern` 与 `matchExpression` 合计 | +749 |
| `codegen.standardErrorValue` 与 `declareErrorBinding` 独立符号合计 | −3,944 |

`printValue` 在 52 源码中已经存在，只是其独立 ELF 符号当时被内联消除；不能把 53 新出现的 8,018 字节符号全部记成新增逻辑。上表将它与收缩的 `outputValue` 合并，剩余函数按实际编译 caller 归属。

统一为全八进制 C 字符串表示后，52/53 的 C 长度为 1,779,238/1,856,574 字节，统一表示后的 C 文本增加 77,336 字节。实际 53 的紧凑拼写比其全八进制版本少 362,742 字节；同一 53 程序、路径、header、flags 和工具链下，两种拼写都产生 773,080 字节 ELF，SHA256 均为 `0099d37771484484c6ee44ac3766dae59663e4a501feacc4e994b8365289a175`。字符串表示优化解释了 C 文件变小，却没有抵消 enum、通用模式与类型处理所需的机器代码增长。具体 LLVM pass 的耗时未在本实验分解，不能把某个函数的字节增量直接等同于其编译耗时增量。

## 资格与保留记录

原始报告 SHA256：`ff3043ba74db9a6970b2f628dc138e52d6b4c90998b185cf58f44531d566f421`；artifact `15257974188`，6,128,882 字节，归档 SHA256 `a3f8b7194673ed4ce19223954e84819b316f5b538f335df1bc3f9390a731afce`。只下载该性能归档，未下载三平台 native ZIP。

首次 run `46132017263` 因 job 级 `env` 不能使用 `runner.temp` 而在解析阶段失败，未启动作业或样本。修正环境变量所在步骤后，本次性能 job 独立成功；同一 producer run 的三平台功能 job 因旧 `subset_features` 断言失败，工作流整体结论仍为 failure。两类结论分别保存，不改写历史。

后续仅修改测试、policy 或报告的提交，可在核对编译器、runtime、selfhost、benchmark 和测量脚本输入不变后引用这份资格；引用保留原 producer/head/report SHA，不伪称在后续 head 重新测量。旧服务器 12.221/13.948 秒、87.668/87.992 ms、RSS 和 205.947–221.043 ms 保留原始值并标记 `unqualified`；这些数值与本次 Xeon 结果不是同机配对比较。

## 精确基线和历史口径

| 对象 | 固定身份 / 原口径 | 原数据能说明什么 |
| --- | --- | --- |
| 第 52 批 | `5fa6401db5726c635a34f8d4ce5c6cc32abc88cc` | 对应源码与历史 C/ELF 字节身份 |
| 第 53 批 | `f0183c07195c658de0d0fc859d79406b53893655` | 最终交付提交；编译器实现来自 `cc81bca`，后续工具修复和 Full 预算修复均保留 |
| 后端 12.221→13.948 秒 | 每版一次 `zig cc -std=c17 -fno-strict-aliasing -DYV_ALLOC_CHECK -O2 -s`；新 C 路径，保留已有 OS/Zig 缓存 | 单次观察，不是冷构建分布或已证实回归 |
| 前端 P50/RSS | stage2 对编译器本身 `emit-c`；分配计数开启；1 次预热、5 次样本 | 曾修复 Python Full receipt 引起的 RSS 污染，但未补齐工作负载隔离证据 |
| 小程序 PCH P50 205.947–221.043 ms | Rust stage0，四个固定原稿，Release、默认 native target；逐轮增加注释使生成 C 与 build-cache key 改变；PCH 必须 hit；每分支1次预热、7次样本 | 四例旧主机观察；不能用同源缓存命中的 3–5 ms 替代编辑后编译目标 |

旧记录并非完全没有负载数据：52 配对重测启动 loadavg 为 `[0.07421875, 0.3623046875, 0.76708984375]`；53 corrected 为 `[0.005859375, 0.4248046875, 0.822265625]`；更早 RSS 污染样本为 `[0.9111328125, 1.095703125, 1.125]`。这些单点无法证明全部样本期间的隔离，也不能据此认定 cordis-rs 与某个样本实际重叠。旧数值及原始记录保留，资格降为 `unqualified`；时间差不直接归因于 cordis-rs。

## 专项 job 合同

接线由 `.github/workflows/ci.yml` 的独立 Linux job 完成，仅第 54 批性能资格专项触发，不进入每批常规 Core/Full 测量矩阵。Ubuntu GitHub-hosted runner、45 分钟预算、`fetch-depth: 0`；脚本拒绝本机和 self-hosted runner，没有绕过资格检查的参数。

```sh
python3 tools/ci/paired_performance_test.py
python3 tools/ci/paired_performance.py \
  --output compiler/target/ci/perf54 \
  --backend-samples 7 --frontend-samples 9 --compile-samples 7 \
  --observe-current
```

依赖仅 Python 标准库、Git、Cargo/Rust 和 runner 的 C 编译器。准备阶段先显式执行 `rustup toolchain install $RUSTUP_TOOLCHAIN --profile minimal`；该变量必须是精确的 `x.y.z`，随后记录 `rustc -Vv` 和 Cargo 版本，不依赖新 VM 预装或隐式安装。Yavo 使用两基线共同的 `compiler/toolchain.lock` 安装并验证 Zig 版本，记录锁文件和实际 Zig 可执行文件 SHA256。只有不可变的 `YAVO_HOME/toolchains` 下载缓存可恢复；Yavo 构建缓存、PCH、Zig 编译缓存和 Cargo target 均不从其他 job 恢复。

所有快照、Cargo target、PCH/Zig 缓存与重复输出均放在独占的 `RUNNER_TEMP/yavo-paired-perf-*`。`compiler/target/ci/perf54/**` 只包含报告、逐轮原始 usage/环境、有限 stdout/stderr、输入身份和归因需要的少量 C/ELF/映射；不上传工作树或缓存。

job 成功表示 **身份、隔离、正确性与样本数合同有效完成**。`≤200 ms` 逐例单列 `target_p50_le_200ms`；超标不使功能 CI 失败，也不改变工作负载或挑选更快样本。身份漂移、其他构建干扰、固定点失败、PCH 回退、样本不足或计量控制失败才使专项失败。

## 测量矩阵

所有准备步骤顺序完成后才开始性能采样。52/53 各自从精确提交构建 Rust stage0，分别完成最小 stage1→stage2→stage3 的 C 和二进制固定点；不运行完整功能用例门。当前 54 只准备 stage0，单独观察，不与 52/53 合并为同一趋势。

| 测量 | 保留样本 | 缓存与顺序 | 输出校验 |
| --- | ---: | --- | --- |
| 自编译前端 | 每版 1 预热 + 9 样本 | 52/53 与 53/52 逐轮交替；同一固定源码；前端缓存关闭；OS 缓存不清空 | 每次 C 必须等于该版固定点 C |
| 自编译后端 | 每版 1 预热 + 7 样本 | 每版独立 Zig caches；预热记录单列；固定 C 路径，每轮追加无语义注释，保证新的翻译单元内容 | 每次 strip 二进制必须等于该版固定点二进制 |
| 52/53 四例 PCH/direct | 每条件 1 预热 + 7 样本 | 固定四条件顺序与其逆序交替；同轮源码字节相同；build-cache 必须 miss；PCH 首轮必须 miss，保留样本必须以同一 key/路径/字节数/SHA hit；direct 不得出现任何 PCH trace | 同轮 PCH/direct 二进制逐字节相同；独立 Rust oracle 核对输出 |
| 当前 54 四例 PCH | 每例 1 预热 + 7 样本 | 主配对结束后单列；记录当前精确 `GITHUB_SHA`；不增加 direct/冷热矩阵 | 同样要求 build-cache miss、PCH 首轮 miss/后续同身份 hit 和 oracle 正确 |

后端预热的空 Zig cache 只形成一次观察，不声称 cold-build 分布；没有清 OS page cache，没有新增 Debug 目标，也没有将 native 编译延迟和 musl 静态程序体积混为同一测量。

报告保留每对原始差值、比值、各 arm 的 P50/P95、最小/最大、CPU user/system time 与 RSS。7/9 个样本的 P95 为描述性次序统计量，不据其声称稳定尾延迟或统计显著性。

## 真实 usage 与负载证据

`perf_wait4.c` 是轻量 C supervisor，被测进程由它 fork/exec。计时从 fork 前的 `CLOCK_MONOTONIC` 到 `wait4` 返回；Python 外层观察时长单列。每条记录保留 wait status、exit、signal、timeout、完整 Linux `rusage`（CPU、RSS、缺页、I/O block、上下文切换等）及 supervisor 自身数据。`ru_maxrss` 不是同时刻进程树 RSS 之和。

计量器先做三个机制控制：真实退出码与原始 stdout 字节、Python 父进程持有 64 MB 时 `/bin/true` 的 RSS 不抬升、超时只结束被测进程组；专项另有已知 child 分配控制。不会加载 Full receipt。Python 报告对象后续增长不会成为被测编译器的 fork 来源。

每次采样前后记录 UTC/Unix 时间、loadavg、各 CPU `/proc/stat` 原始计数、CPU affinity 与可读取的频率；主机元数据包含 CPU 型号/特性、governor、cgroup 限额、runner/image/run/head 和工具版本。采样期间每 50 ms 检查外来 cargo/rustc/cc/Zig/Yavo 等构建进程，覆盖目标前缀和版本后缀的 GCC/Clang/ld、rust-lld、collect2；保留观察者自身 CPU 开销。准备命令仅在启动前检查一次。检测到干扰则保留并判无效，不停止其他进程，不静默删除或重跑样本。独立 VM job 是主要隔离边界；进程检查是附加证据；极短或未识别名称的进程可能逃过轮询，也不能代表物理宿主机完全没有噪声。此范围同时写入机器可读 `report.isolation`。

## 超过 200 ms 时的阶段诊断

任何 PCH arm 的 P50 超过 200 ms，会对同轮冻结输入补测 `stage0 check --emit-c`，要求 C 与实际 driver cache 中该轮产物逐字节相同；然后使用该轮真正的 runtime PCH、等价 Release/native flags 编译冻结 C，要求二进制与对应 driver 构建逐字节相同。后端诊断有独立的预热和缓存根，保留样本不复用预热翻译单元。

每个超标 case/revision 最多执行 8 次 frontend 和 8 次 frozen-C/PCH-backend（各 1 次预热、7 次保留样本）。52/53 最多 8 个 case/revision，即 128 次阶段命令；当前 54 的四例观察最多新增 64 次阶段命令，另有原本的 32 次完整 driver 构建；不扩展 direct/冷热矩阵。

报告分列 full-driver、frontend-check/emit 和 frozen-C/PCH-backend 的分布，以及 full-minus-backend 等残差。这些是独立受控对照，残差包含 driver/cache/启动和交互影响，不假称进程内可精确相加的 profiler；以实际记录决定是否存在本批可小范围修复的瓶颈。

## ELF 与字符串表示的归因方法

独立 CI 用同一 flags/header/工具链构建 unstripped 对照，验证 `.text/.rodata/.data/.data.rel.ro` 与对应 strip 基线一致，再读取 ELF 符号。`sh_fn_数字` 按现行 `main.compile` 的 breadth-first 模块顺序及 `parser.parseFile` 的声明顺序映射到稳定 `module.function`；全部生成 C 声明与定义索引、逐函数参数数、真正 C `main` 对 `main.main` 的调用均须一致；另以源函数与 C 函数中均唯一的长字符串解码值/大整数常量核对函数体，要求每个有函数的模块至少一个独立锚点。历史 52/53 的现存源码与 C 已静态核对：300/316 个函数、477/478 个唯一函数体锚点，九个有函数的模块全部覆盖；无重新编译或性能采样。源字符串使用严格 Yavo 转义，C 字符串使用严格 C 转义；原始 `${...}` 插值不冒充字面量锚点。此机制避免新增函数造成的编号漂移被误判为整批新代码。精确地址区间别名去重；内联代码归入实际 caller；未被符号 size 覆盖的 padding 单列。

字符串对照使用同一 53 程序、同一输入/输出文件名、相同 header 与 flags，只改变普通 C 字符串 token 的等价字节拼写。预处理行、注释、字符常量不改；无法证明的 escape/prefix 拒绝。只有实际 strip 二进制 SHA256 相同后，报告才把该对照记为通过。
