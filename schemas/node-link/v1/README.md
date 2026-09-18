# Node Link v1 Schemas

本目录是 [`docs/NODE_LINK_PROTOCOL.md`](../../../docs/NODE_LINK_PROTOCOL.md) 的机器可验证表达，使用 JSON Schema Draft 2020-12。

入口：

- `message.schema.json`：所有 Node Link WSS message 的联合入口。
- `handshake.schema.json`：`node.hello`、`node.challenge`、`node.proof`、`node.ready`。
- `catalog.schema.json`：`catalog.*`、`export.revoked`、`node.trust.revoked`、`node.rotate-key.*`。
- `resource.schema.json`：`resource.attach/attached/detach`、`resource.subscribe`、`resource.snapshot_*`、`resource.event`、`resource.ack`。
- `command.schema.json`：`command.submit/accepted/rejected/terminal/status` 与全部 payload 形状。
- `error.schema.json`：`link.error`、`link.ping`、`link.pong`、`link.backpressure`。
- `pairing.schema.json`：节点配对 HTTPS payload（二维码、claim、status、HTTP 错误）。

规则：

- schema `$id` 是稳定逻辑标识，前缀统一为 `https://acp-remote.dev/schemas/node-link/v1/`，不表示项目依赖公网 schema registry。
- 测试解析 `$ref` 时必须映射到本目录，禁止构建过程访问网络。
- 控制 DTO 默认是 closed object（`additionalProperties: false`）。明确的开放扩展点只有五处：`resource.event.payload.view`、`resource.event.payload.acp`、错误 `details`、`elicitation.respond.payload.values`、`session.create.payload.templateParams`；它们用 `additionalProperties: true` 并带 `description` 标注。
- 消息名与 `$defs` 的对照表见 `docs/NODE_LINK_PROTOCOL.md` §12.8；新增或改名消息必须同时更新该表、本文档目录和 `fixtures/node-link/v1/`。
- `common.schema.json` 的 `$defs/transcriptCodec`、`transcriptDomain`、`transcriptFieldTag` 是 §9.1–§9.3 冻结表在 schema 中的机器可读登记（codec 名、六个 domain、`1..16` 字段 tag），fixture 校验器与 Rust/TypeScript 测试应当直接读取，避免各自硬编码。
- 本目录不能单独改变协议语义；修改时必须同步 `docs/NODE_LINK_PROTOCOL.md` 与 `fixtures/node-link/v1/`。

JSON Schema 只检查单条消息的结构。以下规则必须由 Rust/TypeScript 契约测试和运行时状态机检查：

- base64url 解码后的真实字节长度、P-256 曲线点有效性、SEC1 与 P1363 编码。
- transcript、签名、HMAC、`payloadDigest`、`snapshotDigest`、ACP `rawJson.byteLength`/`sha256`。
- `connectionSequence`、`originSequence`、`originEpoch`、ACK cursor、snapshot chunk 的跨消息顺序。
- `attachmentId`/`attachmentGeneration` 的当前性与旧 generation 拒绝。
- feature 列表上限、`grant.*` 授权、Export 可见性、撤销状态。
- `requestId` 幂等、`expectedVersion` 并发控制、`command.terminal` 唯一性与 `uncertain` 语义。

仓库自带的检查（在仓库根运行）：

```text
npm run check
```

其中 `scripts/check-schema-fixtures.mjs` 用 ajv（Draft 2020-12）逐条校验 `fixtures/node-link/v1/manifest.json` 的正反样例；`scripts/check-contract-assets.mjs` 校验 `$ref` 目标、`$id` 唯一性、manifest 文件、`rawJson` hash/长度与 transcript 固定向量（SHA-256/HMAC/P1363 实际重算）。Rust 与 TypeScript 工程建立后，双方仍必须各自运行 manifest 中的正反 fixture 与 `transcripts/` 向量，形成独立于本脚本的判定。
