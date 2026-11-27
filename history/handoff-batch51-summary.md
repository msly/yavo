# 第五十一批交接首页（历史记录）

## 1. 一句话状态

**第五十一批交付四种名义标准 Error、prelude Utf8Error、已有十个 fallible 调用的完整 Result，以及命名 catch 和标准 Error 的真实载荷 match。** 错误可保存、传参/返回、显示、匹配及 try 传播；相同载荷布局的 fs/io/os/process Error 仍互不转换。一般用户载荷枚举、普通 match、错误容器、qualified payload 构造及未声明API继续保持边界。

本机完整自举 **1,136/1,136、20/20、0 skip**；标准错误 **48场景 × 3实际前端 × 2模式 = 288/288**，另6项oracle控制、4项清理控制和3项原词法边界检查通过。stage2/stage3 C **1,774,302字节**、Linux二进制 **729,968字节**，两代分别逐字节相同；JSON/text/supplement **196/160/16**、SHA256 **276**、普通driver **114/114、9/9**通过。实现提交 `76915bc` 的 [run 46028799189](https://github.com/msly/yavo/actions/runs/46028799189) **6/6 成功**；三平台标准错误六产物门分别通过 Linux/macOS 288、Windows 276 次，Linux 原 stage0 与实际最终 stage2 C 的独立 ASan+UBSan 通过96次。三份原生 ZIP 的 GitHub 大小/哈希、原始回执、源输入和产物身份已复核。 首次 [run 46018585973](https://github.com/msly/yavo/actions/runs/46018585973) 为5/6：Windows新错误门因原生反斜杠与冻结POSIX路径比较不等，在发射C和执行场景前退出。该处已改用 `.as_posix()`，路径正负控8/8、本机错误门288/288复验通过；首次失败和原始归档保留。

新增73项、10项旧例原源码提升，清单共1136：run319、panic34、reject600、unsupported141、shared_unsupported42。73个新增语义用例中，一项最初使用 `['a', 0xE2, 0x82]`，触及既有非十进制词法 U0001 边界。按显式唯一 amendment 新增等值 `['a', 226, 130]` 源文件，并只替换该 manifest 项的 file；原稿、原 freeze、原 manifest 快照和首次候选失败全部保留。旧 stage0 对原/新源 Debug/Release 的4次执行均输出 `1\n`，预期未改；正式 stage0 接受原稿，stage1/2仍报 U0001。 原622份语料字节与stage0分类不变，覆盖 **119→125/394**、入口字节去重 **56→62/224**，零退步/越界/超时；6个新接受原入口另经12场景、48次Debug/Release差分运行。未修改Rust、生产runtime或工具链锁。本批未重跑本机 Rust 全量；当前 CI 实际日志为Linux完整 Debug/Release各151/0/1，macOS原生选定 Debug/Release各90/0/1，Windows原生选定 Debug/Release各87/0/1。

**SH-2保持固定点，SH-4正式CLI保持3/3，SH-3仍未默认接替Rust。** 下一批优先 Copy 元组 Vec：当前首阻塞共25路径/11份入口字节，其中Copy为22/8，另3/3是包含String的拥有型元组。先冻结空/非空构造、push/pop、字段及参数/返回、索引读写/遍历与边界诊断，再用原文暴露后续限制。Map值首阻塞27/14混合Counter、元组、String、Vec和Set，不统一归入Copy。首阻塞计数不等于预期覆盖增量。 交叉目标、缓存/PCH、安装、其他工具入口、更广语义及丰富诊断仍待迁移；正式性能P50与相对Rust的交付成本另验。[完整验证索引](../compiler/validation/batch51-standard-errors-2026-11-27.json)。
