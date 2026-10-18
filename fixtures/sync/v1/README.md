# Sync Protocol v1 Fixtures

- `valid/`：必须通过相应 schema 的正向样例。
- `invalid/`：必须失败的反向样例。
- `transcripts/`：跨语言密码学和 transcript 固定向量。
- `device-qr/`：新设备短二维码（`https://<origin>/pair#q2=…`）的 Rust/TypeScript 共享固定向量。
- `manifest.json`：fixture、schema 和预期结果的映射。

`manifest.json` 的字段约定：

- 每条 case 至少包含 `fixture`、`schema`、`valid`；`invalid/` 的 case 追加 `expectedKeyword`，声明该 fixture 必须因哪个关键字失败（由 `scripts/check-schema-fixtures.mjs` 用 ajv 断言）。
- 事件 fixture 可以追加 `viewSchema` 与 `viewDef`，表示还必须用 `schemas/sync/v1/event-views.schema.json` 中对应事件类型的 `$defs` 校验 `body.payload.view`。
- `deviceQrVectors` 列出设备短二维码的共享向量（`device-qr/vectors.json`）。它们不是 JSON Schema 样例，但同样必须在 manifest 登记；`crates/sync-protocol/tests/device_qr_vectors.rs` 与 `clients/app/src/protocol/device-qr.test.ts` 读取**同一文件**，覆盖规范 65 字节布局、旧 `#data=`、版本/长度/编码拒绝、主机公钥 SHA-256 指纹与 134 字节 / 58×29 预算。`valid/` 与 `invalid/` 里不再有任何旧设备完整资料（`#data=`）样例；节点二维码仍由 `fixtures/node-link/v1/` 独立覆盖。
- `transcriptVectors` 列出全部固定向量（含 `transcripts/invalid/` 下的拒绝样例）；每个正向向量包含 `codec: "acpr-transcript-v1"`、`input` 与 `expected`。`expected` 必含 `transcriptBase64url`、`transcriptSha256Hex`；HMAC 用途追加 `hmacKeyBase64url`/`hmacSha256`，签名用途追加 `publicKey`/`p1363Signature`/`privateJwk`。
- 拒绝样例形如 `{ "codec": "acpr-transcript-v1", "domain": "…", "malformedTranscriptBase64url": "…", "expectedError": "…" }`，或把 `malformedTranscriptBase64url` 换成 `malformedPublicKeyBase64url` 用于公钥校验。transcript 家族的 `expectedError` 取 `bad_magic`/`bad_codec_version`/`truncated_domain`/`truncated_field`/`field_order`/`duplicate_tag`/`unknown_tag`/`length_mismatch`/`trailing_bytes`，公钥家族取 `bad_base64url`/`bad_length`/`bad_point`/`not_on_curve`。
- `scripts/check-contract-assets.mjs` 用 `compatibility/transcripts/v1/transcripts.json` 的 tag 表从 `codec`/`domain`/`input` 重新编码 transcript，与 `expected.transcriptBase64url` 逐字节比较：tag 编号、字段宽度或顺序写错都会失败，不能只比较常量。

fixture 中的密钥、签名、ID 和内容全部是公开测试数据，禁止用于真实 Host 或 Device。
