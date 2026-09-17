# Sync Protocol v1 Schemas

本目录是 [`docs/SYNC_PROTOCOL.md`](../../../docs/SYNC_PROTOCOL.md) 的机器可验证表达，使用 JSON Schema Draft 2020-12。

入口：

- `message.schema.json`：所有 WSS message 的联合入口。
- `pairing.schema.json`：HTTPS pairing request/response。
- `common.schema.json`：共享值对象。
- `auth.schema.json`：WSS 认证消息。
- `sync.schema.json`：订阅、快照和 ACK。
- `event.schema.json`：持久化事件信封。
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
- feature 列表合计上限、scope、Origin、撤销状态和命令授权。
- request 幂等、Session Actor 串行化和 command terminal event 唯一性。

仓库自带的轻量检查：

```text
node scripts/check-contract-assets.mjs
```

它检查 JSON 可解析性、`$ref` 目标、manifest 文件、`rawJson` hash/长度和固定签名向量，不代替 Draft 2020-12 validator。Rust 与 TypeScript 工程建立后，双方都必须运行 manifest 中的正反 fixture。
