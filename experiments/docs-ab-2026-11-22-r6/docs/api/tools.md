# API 主题：tools

当前实现先看 cheatsheet；semantics/stdlib_contract 同时含设计契约，不能将尚未实现的项目当成可用 API。

## cheatsheet.md，第 445–460 行

### 命令行

`test` 块和 `yavo test` 还没有实现（写 `test "..." { }` 是语法错误）；要自测就写一个小程序或在 `main` 里用 `assert`。

| 命令 | 作用 |
| --- | --- |
| `yavo run main.yv` | 编译并运行 |
| `yavo build main.yv -o app` | 编译；`--release` 开优化，`--target x86_64-windows-gnu` 交叉编译 |
| `yavo check main.yv [--json]` | 只检查，不生成程序；`--json` 输出结构化诊断 |
| `yavo fix main.yv [--dry-run]` | 只自动应用不改变语义的修复（如未用变量改名 `_x`、补上缺的导入），其余只列候选；`--dry-run` 只预览 |
| `yavo allocations 函数名 --file main.yv` | 每个操作给出“不分配、可能分配、未知”之一 |
| `yavo verify main.yv [--stdin f] [--expect-stdout f]` | 构建、运行并输出验证报告；审计尚未实现，所以总体结论是 `not_verified`（退出码 5） |
| `yavo toolchain status` | 查看交叉编译工具链 |

退出码：`yavo` 自身 0 成功、1 编译失败、2 用法错误；`yavo run` 返回程序自己的退出码，panic 是 101，被信号杀死是 128+N；递归太深时程序以 `yavo panic [P-STACK]` 终止（退出码 101），没有源码位置（写作 `0:0`）。


## stdlib_contract.md，第 142–148 行

## 8. 诊断和分配查询

`yavo check --json <file>` 的主诊断用 code/severity/primary.line/primary.column；有关来源和冲突的位置放 secondary，不能因为测试只标主行而漏报关联位置。根因产生的级联错误标记 suppressed，不作为额外主错误。`yavo fix` 的每个候选包含稳定 id、完整编辑集合、影响说明、适用条件；应用任一候选后单独重跑 check。

`yavo allocations <function> --file <source> --json` 返回 `subject`、`status`、`operations`。status 是 `no_alloc`、`may_alloc`、`unknown` 之一；发现任何已知可能分配操作则 may_alloc，否则存在未知调用则 unknown，否则 no_alloc。parameters 和 callee 的操作分别记录。动态接口实现未知不能因接口方法体不可见就判 no_alloc。

查询和 verify 用例的 setup 是运行器场景要求，不是已经运行的日志。C 运行时锁定但未审计时标 trusted_pinned，不可改成 audited。
