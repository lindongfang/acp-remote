# RV1/RV2 检视报告 · WP2（crates/acp-protocol）

- Review ID：`RV-WP2-R1`（检视 `4b0145e` → `64a7582`）
- Reviewer：独立 reviewer 子 Agent（新隔离上下文，只读）

## 结论：FAIL（1 个 MAJOR，其余 MINOR）

| ID | 级别 | 发现 | 状态 |
| --- | --- | --- | --- |
| WP2-F1 | MAJOR | `ContentBlock::Unknown` 带 `skip_serializing` → serde 重编码返回 Err，`mapper` 用 `unwrap_or(Value::Null)` 吞掉 → 未登记 content block 的 `block` 变 `null`（静默降级），且无用例 | 已修：`mapper` 直接用原始 payload；新增 `unknown_content_block_stays_structured` |
| WP2-F2 | MINOR | `MODULE_ARCHITECTURE.md` §8 写 `AcpCodecError`，实际是 `AcpError` | 已修：§8 两处类型名对齐实现 |
| WP2-F3 | MINOR | 任务要求「引用矩阵 row id」，实测以 `wireName`/`wireValue`/`path` 对齐；部分判别子样例是内联 payload 而非登记进 manifest | 记为已记录偏离：测试改为直接断言矩阵声明式字段 + `invariant_fixtures_exist` |
| WP2-F4 | MINOR | task 3.3 的「无零用例」判据被 verification 改写为「无 0 用例文件」；同文件测试计数自相矛盾 | 已修：计数改为实测值，判据措辞明确为「集成用例无跳过、无 0 用例文件」 |
| WP2-F5 | MINOR | 工作区不干净（`.pi/settings.json`）、无法独立核实提交哈希 | 已记录：`.pi/settings.json` 非本变更产物；日志已补 commit 字段 |
| WP2-F6 | MINOR | 出站 `id_value` 会把字符串 id 的转义规范化 | 记为已接受：本 crate 的保真承诺只覆盖收到的文档 |

Reviewer 明确认定的正确项（有代码依据）：raw 保真不经 `Value` 往返（含反例断言）、未知判别子与 `_` 方法的语义分离、结构化不文本化、1 MiB 上限不产生部分结果、capability 不虚报、矩阵表双向比对（25 方法/11 判别子/19 能力路径）。
