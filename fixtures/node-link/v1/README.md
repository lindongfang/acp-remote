# Node Link v1 Fixtures

- `valid/`：必须通过相应 schema 的正向样例。
- `invalid/`：必须失败的反向样例。
- `transcripts/`：跨语言密码学和 transcript 固定向量。
- `manifest.json`：fixture、schema 和预期结果的映射。

fixture 中的密钥、签名、nonce、ID 和内容全部是公开测试数据，禁止用于真实节点。

## 正反样例

| fixture | 绑定的 schema | 覆盖内容 |
|---|---|---|
| `valid/node-hello.json` | `message.schema.json` | 认证前信封（省略 `connectionId`/`connectionSequence`）、`role=access`、feature 列表 |
| `valid/node-challenge.json` | `message.schema.json` | Owner 版本选择、`connectionId`、`nodeProof` |
| `valid/node-ready.json` | `message.schema.json` | 认证后信封、`limits` 全字段、`serverEpoch` |
| `valid/catalog-snapshot.json` | `message.schema.json` | `exportEntry` 全字段（含 `grant.*` scopes 与 `no-content-cache`） |
| `valid/resource-attached.json` | `message.schema.json` | `attachmentId`/`attachmentGeneration`（十进制字符串）/`sessionMeta` |
| `valid/resource-event.json` | `message.schema.json` | origin 三元组、`payload.view` 开放扩展点、`payload.acp` 的 `rawJson`/`byteLength`/`sha256` |
| `valid/resource-ack.json` | `message.schema.json` | origin cursor 累计 ACK |
| `valid/command-submit-session-create.json` | `message.schema.json` | `session.create` 的四个合法键与 `templateParams` |
| `valid/command-submit-session-prompt.json` | `message.schema.json` | session 范围命令携带 `attachmentId`/`attachmentGeneration` |
| `valid/command-terminal.json` | `message.schema.json` | `terminal.status=completed` 时 `result` 为非空 object、`error=null` |
| `valid/export-revoked.json` | `message.schema.json` | Export 撤销通知 |
| `valid/pairing-claim-request.json` | `pairing.schema.json` | claim 请求与 HMAC `proof` 形状 |
| `valid/pairing-status-approved.json` | `pairing.schema.json` | `status=approved` 时返回 `node` 与 `owner` |
| `invalid/session-create-with-cwd.json` | `message.schema.json` | `session.create` payload 出现 `cwd`（`additionalProperties`） |
| `invalid/resource-event-missing-origin.json` | `message.schema.json` | 缺少 origin 三元组（`required`） |
| `invalid/stale-attachment-generation.json` | `message.schema.json` | `attachmentGeneration` 用数字而不是无前导零十进制字符串（`oneOf`） |
| `invalid/command-unknown-field.json` | `message.schema.json` | `command.submit` body 出现未登记字段（`unevaluatedProperties`） |

`invalid/` 意图说明：

- `session-create-with-cwd.json` 在 schema 层被 closed object 拒绝；运行时对 `cwd`、`mcpServers`、绝对路径或凭据字段必须返回更具体的 `nodelink.command.unsupported_field`（见 `docs/NODE_LINK_PROTOCOL.md` §2.4、§12.7）。
- `stale-attachment-generation.json` 只证明 generation 的 wire 编码规则（十进制字符串）；真正的“generation 陈旧”判定属运行时状态机，必须返回 `nodelink.resource.attach_generation_stale`，不能靠 schema 表达。

## transcript 固定向量

每个 `transcripts/*.json` 形如：

```json
{
  "description": "Public interoperability vector. Never use this key in production.",
  "domain": "acp-remote/node-link-challenge/v1",
  "codec": "acpr-transcript-v1",
  "input": {},
  "expected": {}
}
```

- `input` 是该 domain 的字段原始值；字段名与编码见 `docs/NODE_LINK_PROTOCOL.md` §9.3、§9.4。UUID 为字符串，`*NonceHex`/`*PublicKeyHex` 为十六进制字节，`catalogRevision`/`pairingExpiresAt` 为整数，`negotiatedFeatures` 为已排序数组（编码时以 `0x00` 连接）。
- `expected.transcriptBase64url` 是 codec 输出字节的无填充 base64url；`expected.transcriptSha256Hex` 是其真实 SHA-256。
- HMAC 用途（`pairing-proof`、`pairing-sas`、`pairing-status`）附带 `expected.hmacKeyBase64url` 与 `expected.hmacSha256`（HMAC-SHA256 输出，无填充 base64url）。
- 签名用途（`pairing-owner-proof`、`challenge`、`node-proof`）附带 `expected.publicKey`（SEC1 65 bytes）、`expected.p1363Signature`（P1363 64 bytes）与 `expected.privateJwk`（公开测试私钥，禁止用于真实节点）。
- SAS 由 `pairing-sas` 向量的 HMAC 输出前 4 bytes 按 u32be 解释后 `% 1_000_000`、左侧补零为 6 位十进制数得到（§9.4），并固化为 `expected.sas`，由检查脚本从 `expected.hmacSha256` 重算断言。
- 拒绝样例位于 `transcripts/invalid/`，每个文件只含 `codec`、`domain`（transcript 家族）与一个 `malformedTranscriptBase64url` 或 `malformedPublicKeyBase64url`，外加 `expectedError`。transcript 家族的取值是 `bad_magic`/`bad_codec_version`/`truncated_domain`/`truncated_field`/`field_order`/`duplicate_tag`/`unknown_tag`/`length_mismatch`/`trailing_bytes`，公钥家族的取值是 `bad_base64url`/`bad_length`/`bad_point`/`not_on_curve`；样例必须正好以声明的错误被拒绝。
- `manifest.json` 的 `transcriptVectors` 是全部向量（含拒绝样例）的路径清单；`scripts/check-contract-assets.mjs` 用 `compatibility/transcripts/v1/transcripts.json` 的 tag 表从 `codec`/`domain`/`input` 重新编码 transcript 并与 `expected.transcriptBase64url` 逐字节比较，再重算 SHA-256、HMAC、SAS 与签名，因此 tag 编号、字段宽度或顺序写错都会失败，不能只比较常量。

仓库自带的检查（在仓库根运行）：

```text
npm run check
```

其中 `scripts/check-schema-fixtures.mjs` 用 ajv（Draft 2020-12）逐条校验本目录 `manifest.json` 的正反样例（invalid 样例必须以声明的 `expectedKeyword` 失败），`scripts/check-contract-assets.mjs` 校验资产完整性、从向量 `input` 重新编码 transcript、拒绝全部拒绝样例，并实际重算 transcript SHA-256/HMAC/SAS/P1363。
