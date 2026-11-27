# Yavo：按需 API 索引

先读 [agent-start.md](agent-start.md)，遇到缺失的类型、方法或规则再按下表取片段。完整 [cheatsheet.md](cheatsheet.md) 记录当前实现；[semantics.md](semantics.md) 和 [stdlib_contract.md](stdlib_contract.md) 给出安全规则与精确契约，后者也包含尚未实现的设计，不能仅凭契约认定 API 已实现。`yavo doc` 尚未实现，直接读取文件。

行号对应 `f54b5fb` 的原始文档，均为闭区间；实验应冻结并核对源文件哈希。文档变化后先按章节标题重新定位，不沿用旧行号。

| 主题键 | 先读当前写法 | 需要精确规则时 |
| --- | --- | --- |
| `syntax` | cheatsheet.md 5–39：骨架、插值、返回、数值 | semantics.md 136–187：数值、比较与隐式操作 |
| `types` | cheatsheet.md 40–133：结构体、枚举、接口、泛型 | semantics.md 27–85：Copy 与借用来源 |
| `errors` | cheatsheet.md 134–200：可选、结果、try/catch/match | semantics.md 44–55、127–133：匹配与错误丢弃 |
| `ownership` | cheatsheet.md 201–290：九条规则、树与图 | semantics.md 7–115：权限、移动、来源、逃逸、并发 |
| `order` | semantics.md 116–135：V1–V14 求值与析构 | decisions.md 72–91：字段默认值、索引赋值与 V2a |
| `io` | cheatsheet.md 291–300、318–361：读取、文件、参数、错误处理 | stdlib_contract.md 77–105：UTF-8 与系统输入 |
| `strings` | cheatsheet.md 301–307：切分、字节、子串、查找、转换 | stdlib_contract.md 77–105 |
| `vec` | cheatsheet.md 308–309、312：读写、排序、遍历 | stdlib_contract.md 17–32、61–76 |
| `map-set` | cheatsheet.md 310–312、314：借用键、更新和 Set | stdlib_contract.md 33–60 |
| `heap-deque` | cheatsheet.md 313–314：collections 导入与操作 | 查看当前写法中的限制；无 Deque 下标/遍历 |
| `arena` | cheatsheet.md 267–289、315：Arena/Id 示例及操作 | semantics.md 136–167：跨 Arena/过期 Id 行为 |
| `math` | cheatsheet.md 316：math 模块 | 排序浮点同时读 `vec`；f64 不满足 Ord |
| `drop` | cheatsheet.md 363–375：自定义 Drop | semantics.md 116–135；stdlib_contract.md 106–111 |
| `modules` | cheatsheet.md 378–409：use/pub、项目根与模块名 | 每文件单独导入；导入不传递；多文件 fix 不支持 |
| `threads` | cheatsheet.md 410–444：scope/spawn、Mutex/Channel | semantics.md 94–115；stdlib_contract.md 112–141 |
| `tools` | cheatsheet.md 445–460：check/build/fix/verify/allocations | stdlib_contract.md 142–148：查询输出契约 |
| `diagnostics` | cheatsheet.md 461–494：常见错误和修法 | 报错对应的 `ownership/order/errors/threads` 主题 |
| `unsupported` | cheatsheet.md 511–513：当前缺口 | 不按 Rust 方法名猜测；必要时构造最小程序 check |

## 查找纪律

- 只取需要的主题；接口仍不明确时再扩大范围。实现用法和规则冲突时保留诊断并报告，不自行放宽所有权。
- `Map.update` 仅支持不携带借用的 Copy 值；`sortBy/update` 的标准库回调可不逃逸借用，用户高阶函数没有同等支持；`Heap.by` 保存比较闭包，不能捕获借用。
- `for k, v in &m` 的键仍只读；`Set` 仅只读遍历；Arena 和 Deque 不可遍历。不要把容器接口互相类推。
- 实验记录需要保存实际返回的原文字节、读取主题、次数和耗时；文档大小不等于模型输入 token。每组初始卡和按需检索必须采用预先固定的分页规则。
