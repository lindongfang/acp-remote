# ACP v1 compatibility fixtures

这些 fixture 由 `compatibility/acp/v1/matrix.json` 引用，供 Rust、TypeScript 和结构检查器共同使用。

- 文件内容本身是测试输入；raw round-trip 测试必须读取原始 bytes，不能先 parse 再重新序列化。
- fixture 由 `compatibility/acp/v1/matrix.json` 的 `invariants[].fixture` 引用，并由本目录 `manifest.json` 驱动 ajv 校验（schema 指向 vendored 固定快照 `schemas/acp/v1/upstream/schema.json`）；`scripts/check-acp-compatibility.mjs` 只做路径存在性与 JSON 解析检查，语义断言（`byte_exact`/`structured_not_text`/`explicit_unsupported`/`truthful_negotiation`/`visible_degradation`）由对应模块的契约测试承担。
- `invariants` 条目不带 `layers`/`delivery`：它们是跨阶段不变量，不是某个交付阶段的功能项。
- 当前 fixture 集：`initialize-capabilities.json`（`truthful_negotiation`）、`tool-call-with-diff.json`（`structured_not_text`）、`unknown-fields-and-large-integer.json`（`byte_exact`，被 Sync 与 Node Link 两个不变量复用）、`future-session-update.json`（`visible_degradation`，故意包含固定快照中未知的 discriminator）、`meta-and-unknown-fields.json`（`byte_exact`，覆盖消息级与 content 级 `_meta` 及未知字段）、`extension-method.json`（`explicit_unsupported`，下划线扩展方法）。
- `future-session-update.json` 故意包含固定快照中未知的 discriminator，用于验证「可见降级并保留 `acp.rawJson`」（`expectation = visible_degradation`），不是有效 ACP v1 fixture。
- 六个消息 fixture 都是固定快照下的合法 ACP 消息（上游 schema 允许未知字段与 `_meta`，因此保真类 fixture 也能通过结构校验）；两个片段 fixture（`valid/tool-call-location.json`、`invalid/tool-call-location-negative-line.json`）用 `manifest.json` 的 `schemaPointer: "#/$defs/ToolCallLocation"` 指向具体定义，用来证明上游对 `line` 的 `uint32` 约束（含负值拒绝）真的生效。新增文件必须同时登记进 `manifest.json`，否则 `check:schemas` 的 orphan 检查会失败。
- 为什么负例要指向 `$defs`：上游快照顶层是对 request/response/notification 的宽松 `anyOf`，一份不合法的 tool_call 文档仍可能被其他消息种类的分支接受，所以约束级负例不能靠整份文档失败。
- fixture 可以包含虚构路径和内容，但不得包含真实 prompt、凭据或用户数据。

