# Local Admin Channel v1 Schemas

本目录是 [`docs/LOCAL_ADMIN_PROTOCOL.md`](../../../docs/LOCAL_ADMIN_PROTOCOL.md) 的机器可验证表达，使用 JSON Schema Draft 2020-12。

入口：

- `envelope.schema.json`：channel `0x01` 的管理载荷（请求、成功响应、失败响应）的联合入口，并携带 framing 常量、方法名枚举与本地错误码枚举。

边界：

- 本目录**只**覆盖管理载荷。channel `0x02` 的 ACP 字节流不在此表达——它只有分帧与上限是合同（文档 §3.1），ACP 语义见 [ACP_COMPATIBILITY_MATRIX.md](../../../docs/ACP_COMPATIBILITY_MATRIX.md)。
- 各方法的 `params`/`result` 形状仍以文档 §5 为权威；本 schema 把它们当作明确的开放容器（`type: object`），不复制字段表，避免第二份定义。
- 本地错误码**不进入** `compatibility/errors/v1/errors.json`：那份 registry 只登记出现在 WSS wire 上的错误码（文档 §6）。这里的枚举是本地错误码的机器定义，文档 §6 的表格是它的说明，二者由 `scripts/check-local-admin-contract.mjs` 断言一致。
- 本目录不能单独改变语义；修改时必须同步文档与 `fixtures/local-admin/v1/`。

固定向量位于 [`fixtures/local-admin/v1/`](../../../fixtures/local-admin/v1/)；`npm run check` 的 `check:schemas` 逐条校验，`check:local-admin` 断言方法集与错误码在文档与 schema 之间一致。
