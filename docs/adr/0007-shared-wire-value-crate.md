# ADR-0007：跨协议 wire 值对象下沉为叶子 crate `acpr-wire`

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：protocol crate 边界、依赖矩阵、workspace crate 数量（十二 → 十三）

## 背景

`node-link-protocol` 要实现 v1 全部消息 body 时，需要与 `sync-protocol` **逐字节相同**的一组值对象与校验
机制。机器事实（`schemas/{sync,node-link}/v1/common.schema.json` 的 `$defs` 逐条比较）：

- 相同的 `$defs`：`uuid`、`decimalString`、`timestamp`、`base64url`、`base64url16`、`base64url32`、
  `base64url64`、`base64url65`、`featureId`、`featureList`、`rawAcp`（11 项，schema 文本逐字节相同）；
- 不同的只有 `errorCode`（两协议各自一份封闭词表，本就不同）与 `publicError`（差别仅是引用各自的 `errorCode`）。

这 11 项对应的 Rust 侧还包括跨协议共用的机制：`Nullable<T>`（`required` 且可 null 的键存在性语义）、
`RawObject`/`ExtraFields`（开放扩展点的字节/值保真）、`Text`/`NonEmptyText`/`BoundedU64`、`ValueError`、
`deserialize_optional_non_null`。`AGENTS.md` §4 规定协议 crate 彼此不直接依赖，平级模块共享的底层实现下沉为
叶子 crate；按该规则照字面做，就要在 `node-link-protocol` 里复制约 1000 行安全相关校验代码。

## 决策

1. 新增第 13 个 crate **`acpr-wire`**，作为叶子 crate：只拥有跨协议共用的 wire 值对象与校验机制——
   `Uuid`、`DecimalString`、`Timestamp`、`Base64Url<N>`、`FeatureId`、`FeatureList`、`RawAcp`、
   `Text<N>`、`NonEmptyText<N>`、`BoundedU64<MIN,MAX>`/`UIntAtLeast<MIN>`/`ProtocolVersionV1`、
   `Nullable<T>`、`RawObject`、`ExtraFields`、`ValueError`、`deserialize_optional_non_null`，以及泛型的
   `PublicError<Code>`。
2. **不拥有任何协议语义**：不含 domain/tag 取值、命令名、grant、错误码词表或会话状态机规则。协议专属的值对象
   （Sync 的 `cursor`/`sessionSummary`/`originBlock`，Node Link 的 `originCursor`/`sessionMeta`/`exportEntry` 等）
   继续留在各自协议 crate，不因"看着像"而下沉。
3. **依赖方向**：`sync-protocol` 与 `node-link-protocol` 正常依赖 `acpr-wire`；`acpr-wire` 依赖
   `acpr-transcript`（`Base64Url` 复用其规范化 base64url 解码），因此依赖链是 `协议 crate → acpr-wire →
   acpr-transcript`，无环。协议 crate 之间仍然互不依赖。
4. **`PublicError` 泛型化**：`acpr_wire::PublicError<Code>`，各协议用
   `pub type PublicError = acpr_wire::PublicError<ItsErrorCode>;` 具体化。错误码词表必须由各自的
   `compatibility/errors/v1/errors.json` 列表驱动，不能共用一份。
5. **命名空间**：协议 crate 的 `common` 模块继续以自己的路径再导出这些类型（例如
   `sync_protocol::common::Uuid` 不变），避免为了搬家而大范围改调用点；这是"命名空间再导出"，不是兼容 shim。

## 结果

- 安全关键的字段级校验（uuid/decimalString/timestamp/base64url 规范形式/feature 列表/开放对象保真）只有一份实现，
  不会在两个协议之间漂移；`Nullable` 的"缺键即错"语义也只维护一处。
- `sync-protocol` 的公开路径不变，既有夹具驱动与漂移门禁（56 条合法消息往返、33 个视图投影、错误码与 registry
  逐条相等）继续作为这次搬家的回归判据。
- crate 数从 ADR-0006 的十二变为十三：`MODULE_ARCHITECTURE.md` §2/§3/§4/§5、`AGENTS.md` §4 与架构图需要同步；
  架构图（`docs/diagrams/`）是生成产物，必须重新生成而不是手工改。
- 新增依赖审查：`acpr-wire` 只依赖 `serde`、`serde_json`、`thiserror`、`base64` 与 `acpr-transcript`，全部纯
  Rust、无平台依赖、Apache-2.0/MIT 许可，符合 `AGENTS.md` §7 的依赖准入。
- 后续：若 `acp-protocol` 出现同一批值对象，同样复用它；若某类型只有两个协议中的一个在用，不下沉。
