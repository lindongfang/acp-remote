# ACP v1 compatibility fixtures

这些 fixture 由 `compatibility/acp/v1/matrix.json` 引用，供 Rust、TypeScript 和结构检查器共同使用。

- 文件内容本身是测试输入；raw round-trip 测试必须读取原始 bytes，不能先 parse 再重新序列化。
- fixture 由 `compatibility/acp/v1/matrix.json` 的 `invariants[].fixture` 引用；`scripts/check-acp-compatibility.mjs` 对它们只做 JSON 解析与路径存在性检查，语义断言（`byte_exact`/`structured_not_text`/`explicit_unsupported`/`truthful_negotiation`）由对应模块的契约测试承担。
- `invariants` 条目不带 `layers`/`delivery`：它们是跨阶段不变量，不是某个交付阶段的功能项。
- `future-session-update.json` 故意包含当前 schema 未知的 discriminator，用于验证“可见降级或明确不支持”，不是有效 ACP v1 fixture。
- fixture 可以包含虚构路径和内容，但不得包含真实 prompt、凭据或用户数据。

