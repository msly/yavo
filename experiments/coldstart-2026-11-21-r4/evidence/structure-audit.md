# 第四轮代码结构人工审计

审计对象为 source-manifest.json 中冻结的最终受试源码；本文件不代表逐次运行转录。

| 要求 | Yavo 证据 | Rust 证据 | 判断 |
| --- | --- | --- | --- |
| T04 至少两个源文件，入口只负责 IO、参数、输出 | T04/main.yv 调用 config.parse/query；解析与展开在 config.yv | T04/main.rs 调用 Config::parse/query；解析与展开在 config.rs | 满足 |
| T05 至少三个源文件 | main.yv、parse.yv、plan.yv | main.rs、parse.rs、plan.rs | 满足 |
| T06 四线程计算规定连续区段 | T06.yv 用 n/4、n%4 构造五个边界，连续创建四个 spawn，再 join | T06.rs 构造四个连续切片，先 collect 四个 scoped handles，再 join | 满足 |
| T10 三工作线程动态领取，结果按输入顺序 | T10.yv 的三个 worker 共用 jobs Channel，循环 recv；主线程按 idx 写回 res | T10.rs 的三个 scoped workers 共用 AtomicUsize，fetch_add(1) 取索引；汇总按索引写回 | 有效计算任务动态分发，无预先均分 |

Yavo T10 在主线程先解析输入，只将有效数字发送给 worker；Collatz 运算由三个 worker 动态处理。Rust 线程领取原始行后各自解析。两者均未预先把计算任务均分成三个固定区段，工作组织方式并不完全相同。

T08 Rust 最终源码与 tasks/T08_broken.rs 的差异仅覆盖必要修复：longer 添加来源生命周期；saved 克隆 names；first 转为独立拥有的 String；top 克隆元素字符串；先收集待插入的键，再修改 Map。未重写整体算法。LOG 只记录已修版本的一次 build；没有完整工具转录，不能证明此前未做过未记录的检查。

本审计不以“包含 spawn 关键字”替代线程逻辑核对，也不将结构满足等同于所有合法输入都正确。T10 前导零的独立失败见 README 和 t10-leading-zero.*。
