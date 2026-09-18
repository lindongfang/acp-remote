# Sync Protocol v1 Fixtures

- `valid/`：必须通过相应 schema 的正向样例。
- `invalid/`：必须失败的反向样例。
- `transcripts/`：跨语言密码学和 transcript 固定向量。
- `manifest.json`：fixture、schema 和预期结果的映射。

`manifest.json` 的字段约定：

- 每条 case 至少包含 `fixture`、`schema`、`valid`；`invalid/` 的 case 追加 `expectedKeyword`，声明该 fixture 必须因哪个关键字失败（由 `scripts/check-schema-fixtures.mjs` 用 ajv 断言）。
- 事件 fixture 可以追加 `viewSchema` 与 `viewDef`，表示还必须用 `schemas/sync/v1/event-views.schema.json` 中对应事件类型的 `$defs` 校验 `body.payload.view`。
- `transcriptVectors` 列出全部固定向量（含 `transcripts/invalid/` 下的拒绝样例）；每个正向向量包含 `codec: "acpr-transcript-v1"`、`input` 与 `expected`。`expected` 必含 `transcriptBase64url`、`transcriptSha256Hex`；HMAC 用途追加 `hmacKeyBase64url`/`hmacSha256`（`pairing-sas` 再追加按 `SYNC_PROTOCOL.md` §7.2 算出的 6 位 `sas`），签名用途追加 `publicKey`/`p1363Signature`/`privateJwk`。
- 拒绝样例形如 `{ "codec": "acpr-transcript-v1", "domain": "…", "malformedTranscriptBase64url": "…", "expectedError": "…" }`，或把 `malformedTranscriptBase64url` 换成 `malformedPublicKeyBase64url` 用于公钥校验。transcript 家族的 `expectedError` 取 `bad_magic`/`bad_codec_version`/`truncated_domain`/`truncated_field`/`field_order`/`duplicate_tag`/`unknown_tag`/`length_mismatch`/`trailing_bytes`，公钥家族取 `bad_base64url`/`bad_length`/`bad_point`/`not_on_curve`。
- `scripts/check-contract-assets.mjs` 用 `compatibility/transcripts/v1/transcripts.json` 的 tag 表从 `codec`/`domain`/`input` 重新编码 transcript，与 `expected.transcriptBase64url` 逐字节比较：tag 编号、字段宽度或顺序写错都会失败，不能只比较常量。

fixture 中的密钥、签名、ID 和内容全部是公开测试数据，禁止用于真实 Host 或 Device。
