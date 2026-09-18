# Sync Protocol v1 Schemas

本目录是 [`docs/SYNC_PROTOCOL.md`](../../../docs/SYNC_PROTOCOL.md) 的机器可验证表达，使用 JSON Schema Draft 2020-12。

入口：

- `message.schema.json`：所有 WSS message 的联合入口。
- `pairing.schema.json`：HTTPS pairing request/response。
- `common.schema.json`：共享值对象。
- `auth.schema.json`：WSS 认证消息。
- `sync.schema.json`：订阅、快照和 ACK。
- `event.schema.json`：持久化事件信封。
- `event-views.schema.json`：`event.payload.view` 的按事件类型 `$defs` 库（每个 `$defs` 只约束该事件的最低必填字段，`additionalProperties` 保持开放）。它没有顶层 `oneOf`，只由 `fixtures/sync/v1/manifest.json` 的 `viewSchema`/`viewDef` 指针引用。
- `command.schema.json`：命令及结果。
- `error.schema.json`：连接级错误和 heartbeat。

规则：

- schema `$id` 是稳定逻辑标识，不表示项目依赖公网 schema registry。
- 测试解析 `$ref` 时必须映射到本目录，禁止构建过程访问网络。
- 控制 DTO 默认是 closed object；事件 `view` 和错误 `details` 是明确的开放扩展点。
- 本目录不能单独改变协议语义；修改时必须同步 `docs/SYNC_PROTOCOL.md` 和 `fixtures/sync/v1/`。

JSON Schema 只检查单条消息的结构。以下规则必须由 Rust/TypeScript 契约测试和运行时状态机检查：

- base64url 解码后的真实字节长度和 P-256 曲线点有效性。
- transcript、签名、HMAC、hash 和 `rawJson.byteLength`。
- connection sequence、event sequence、cursor、ACK 和 snapshot chunk 的跨消息顺序。
- feature 列表上限、scope、Origin、撤销状态和命令授权。
- request 幂等、Session Actor 串行化和 command terminal event 唯一性。

仓库自带的检查（在仓库根运行）：

```text
npm run check
```

它依次执行 `scripts/check-schema-fixtures.mjs`（ajv Draft 2020-12 逐条校验 `fixtures/sync/v1/manifest.json` 的正反样例与事件视图）、`scripts/check-command-catalog.mjs`（命令名在 `compatibility/commands/v1/commands.json`、`command.schema.json`、`docs/SYNC_PROTOCOL.md` §11.5 与 `docs/SECURITY_DESIGN.md` §10.2 四处一致）、`scripts/check-contract-assets.mjs`（`$ref` 目标、`$id` 唯一性、manifest 文件、`rawJson` hash/长度、transcript SHA-256/HMAC/签名重算）与 `scripts/check-acp-compatibility.mjs`。Rust 实现必须在**自己的 wire DTO** 上形成独立判定，而不是复用本目录的校验结果：`crates/*/tests/` 用同一份 `manifest.json` 驱动——每条 valid 夹具必须解析成功并往返一致，每条 invalid 夹具必须在声明的层被拒绝；`type` 常量、错误码与视图登记表另有漂移门禁与 schema/registry 逐条比对。分工是：ajv 判「schema 是否自洽、夹具是否合 schema」，DTO 判「实现是否忠实于 schema」；将来若需要 Rust 侧独立复算 schema 本身，可以另加通用 Draft 2020-12 校验器作为额外一层，但它不能替代 DTO 层。
