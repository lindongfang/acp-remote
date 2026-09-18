# Sync Protocol v1 Fixtures

- `valid/`：必须通过相应 schema 的正向样例。
- `invalid/`：必须失败的反向样例。
- `transcripts/`：跨语言密码学和 transcript 固定向量。
- `manifest.json`：fixture、schema 和预期结果的映射。

`manifest.json` 的字段约定：

- 每条 case 至少包含 `fixture`、`schema`、`valid`；`invalid/` 的 case 追加 `expectedKeyword`，声明该 fixture 必须因哪个关键字失败（由 `scripts/check-schema-fixtures.mjs` 用 ajv 断言）。
- 事件 fixture 可以追加 `viewSchema` 与 `viewDef`，表示还必须用 `schemas/sync/v1/event-views.schema.json` 中对应事件类型的 `$defs` 校验 `body.payload.view`。
- `transcriptVectors` 列出全部固定向量；每个向量包含 `codec: "acpr-transcript-v1"`、`input` 与 `expected`。`expected` 必含 `transcriptBase64url`、`transcriptSha256Hex`；HMAC 用途追加 `hmacKeyBase64url`/`hmacSha256`，签名用途追加 `publicKey`/`p1363Signature`/`privateJwk`。

fixture 中的密钥、签名、ID 和内容全部是公开测试数据，禁止用于真实 Host 或 Device。
