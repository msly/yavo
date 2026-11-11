# Yavo 测试集（冻结候选 v0.5）

本版以用户提供的 76 项 v0.4 测试为基础，保留原有 ID，修订相关预期并新增 53 项。全部 Yavo 程序尚未解析、类型检查或运行；`tools/check_package.py` 只验证包的结构和元数据，不能替代编译器验收。

## 目录与数量

| 类别 | 数量 | 阶段 |
| --- | ---: | --- |
| check | 76 | P1 |
| fix | 7 | P1 |
| run | 21 | P2 |
| concurrency | 21 | P3 |
| report | 4 | P3 |

总计 **129** 项；P1 83，P2 21，P3 25。`*.fixed.yv` 和 report/fixtures 是输入或期望文件，不另计测试。

## 已关闭的设计缺口

原 README 与语义表的两张清单合并后共 11 项，详见主目录《决议与变更》：标量解引用、可选模式、Vec 切片借用、Drop、并发库、库细节、临时切片线程参数、禁止部分移动、常量失败、catch 通配符、fix 命名。额外固定普通遍历只读、显式 & 可变遍历。

## 格式和判定

注释头保留 id/kind/phase/rule/summary。主诊断仍写在同一行 `//~ ERROR E0303` 或 `//~ WARN W0001`。MANIFEST 同时记录行号、severity、源码哈希、预期输出、候选、固定文件、平台、夹具哈希和分离的 Sanitizer 配置。运行器应先检查头与清单一致，避免执行过时预期。

| kind | 必须满足 |
| --- | --- |
| check-pass | 能解析，check 无错误；允许无关警告 |
| check-fail | 必须先能解析；只产生指定的主错误码及行号，相关 secondary 不算额外错误 |
| fix-auto | 原诊断匹配；自动补丁结果与 fixed 文件按 token 比较一致；新文件 check 通过；第二次 fix 不再改变 |
| fix-candidate | 不自动修改；候选 id 全部存在并附影响；每个候选应用到独立副本后重新 check，不得提供缺少必要修改的半补丁 |
| run-pass | 退出 0、stdout 精确匹配；CRLF 统一为 LF，不裁剪其他空白 |
| run-panic | 已成功生成程序，非零退出，stderr 的 panic 类别匹配；普通构建失败或任意崩溃不算成功 |
| run-exit | 用平台原生方式传入 args-hex；非零退出并带指定启动错误码，main 未执行、stdout 为空 |
| run-allowed | 仅接受 allow-outcomes 列出的 stdout/指定 panic；超时、未知崩溃和 Sanitizer 错误始终失败 |
| report | 按 JSON setup 配置场景；JSON 字段满足 expect；场景本身未执行不能算通过 |

比较 fixed 文件不得用“简单正则删掉 // 后面的内容”，因为字符串也可以包含 //。使用词法 token 比较或真实 formatter。标注可出现在代码后的注释中，主诊断列允许实现选择该行的合理 token；源哈希保证它没有对错版本。

## 特殊运行要求

- `sanitize: address+undefined; thread` 是两个不同的构建和运行，不能组合 address 与 thread。每个结果带自己的工件哈希和状态。工具不可用记 unsupported 或 not_run，不是 passed。
- 每个程序独立子进程，默认 5 秒或注释 timeout-ms；超时清理子进程树。concurrency 不能只检查编译成功。
- `fault-alloc-at: 1` 只用于测试运行时：在 main 开始后第 1 次用户堆分配失败，启动参数等运行时准备不计入。生产构建没有该入口。注入器未实现时标 not_run，不得真的耗尽宿主内存。
- 表达式失败即使常量可知也须在 debug/release 保留执行路径和 panic 类别；字面量本身超出目标类型才是编译错误。
- `fixture` 是单纯相对路径，fixture-hex 是独立的字节校验；`cwd: test-dir` 明确工作目录。
- report 的 available_runners 和辅助工件是测试运行器受控场景，不假设机器真的有或没有某平台。不能通过伪造通过报告来满足场景。

## 覆盖与尚未取得的证据

COVERAGE.json 是规则引用清单，不是穷尽性证明；本轮新增了 O3、K2、K3、S1/S2/S4、M1/M2、M3 分配查询等用例。仍需实现期扩充模糊测试、FFI、Arena 代数溢出、调度压力和任意用户回调的规则。v1 只支持库已知 noescape 回调，不开放未声明效果的用户高阶接口。

包内没有 Yavo 编译器，也没有伪装成编译器的校验器。所有 Yavo 执行证据保持 not_run。
