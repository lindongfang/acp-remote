# core 端口与 SQLite 持久化合同

> 状态：编码前契约（v1），实现前冻结；`core`/`storage-sqlite` 的 L1 切片已开始实现（`core::model` 与 `core::ports` 已落地，见 §10）
> 版本：0.3（v0.2 的 L1 实现期修订：补齐交互**创建**路径、正文读取入口 `ReadView::event_payload`、非会话级事件的 `origin_*` 列改为可空、`owned_attachment` 增加 id 列、`find_remote_request` 改名、Windows ACL 判定说明；并把 §5.2 里不存在的 `resolve_interaction` 方法改为 §6 第 13 条）
> 版本：0.4（2026-09-18，L1 收尾：补齐四条会阻塞「一个真实 turn」的缺口——§6 第 14 条 `agent.message.completed` 的生成、第 15 条 delta 压缩与 `turn.delta_compacted` 收据、第 16 条启动恢复 `RecoverUnsettled`、第 17 条 `session.mode.list` 的候选来源；新增 `SessionEndpoint::modes()`、`SessionStore::unsettled_commands`、`OwnedCommit.compacted`、`MessageId`/`ModeState`，并给 §9 加判据 18–21）
> 版本：0.5（2026-09-18：§5.4 的 `EventSink` 更正为包装结构，与 §5.1 的 `[决定]` 和 `ports.rs` 一致；新增 `scripts/check-contract-drift.mjs`，把 §7/§5 与实现的漂移变成 `npm run check` 的第八个门禁）
> 日期：2026-09-18
> 上位文档：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.1/§4.7/§5/§6/§7/§8/§10、[INITIAL_DESIGN.md](./INITIAL_DESIGN.md) §5/§10、[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §3/§9/§10/§11/§14、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §6/§7/§12/§15、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §13/§14/§15、[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §4/§5/§6、[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5
> 作用：冻结 `core::model` 值对象、`core::use_cases` 用例面、`core::ports` 端口签名、broker 事务顺序与 `storage-sqlite` 的 v2 表结构、保留/清理与 migration。**本文件是这些内容的唯一权威来源**；`MODULE_ARCHITECTURE.md` §4.1/§4.7 只保留职责边界。
> 标记约定：`[决定]` = 本合同新定且不改变既有协议语义；`[待确认]` = 触及协议或产品语义，需用户确认；`[open]` = 明确留到实现阶段。
> 修订（2026-09-23）：§11.5–§11.9 的形状已并入 §3.5/§3.6/§3.7/§5.1/§5.3/§7.2/§7.3/§7.4，§11 改为「形状已并入」索引（只留设计理由）；`scripts/check-contract-drift.mjs` 对新增端口签名与 DDL 逐条断言。`storage-sqlite` 的管理 store 落盘实现已落地（`crates/storage-sqlite/src/admin/`，写集一事务提交、失败关闭与容量纳入）。**Daemon/CLI 接线与 `server`/`app` 的入站适配器已随切片 4 `daemon-cli-and-local-admin` 落地**（`server` 的本地管理与 `app` 组合根/CLI，见 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9/§4.10；`identity-auth`/`identity-keystore` 两个身份 crate 已于 2026-09-24 落地，见 §4.8/§4.12）；仍未实现的是 `server::sync`/`server::node_link`/`server::acp_facade` 与 `node-link-client`。在远程与网络适配器落地前，不能把合同检查或存储测试通过解释为配对、撤销或本地配置已经端到端可用。
> 版本：0.6（2026-09-23：§11 从「表设计 + 要求」补成可实现合同——新增 §11.5 身份值对象与读取形状、§11.6 管理写入 DTO 与端口签名（目标形状）、§11.7 管理表 DDL（目标形状）、§11.8 版本常量/migration/fixture 约定。**§11.5–§11.8 是 `[待实现]` 的目标形状**：它们不写入 §5/§7，因为 `scripts/check-contract-drift.mjs` 把 §5 的 ```rust 块与 `crates/core/src/ports.rs`、§7 的 ```sql 块与 `crates/storage-sqlite/src/migrate.rs` 逐条绑定；实现变更必须把这些形状并入 §5/§7 并让漂移门禁断言，在此之前不得只加表就声明管理状态可用）
> 版本：0.7（2026-09-23：§11.5–§11.9 落地——§5.3 换为写集端口（`WriteContext`/`PendingAudit` + 全部写集 DTO + `TrustStore`/`ExportStore` 新签名 + `LocalConfigStore`/`CredentialResolver`），§3.5/§3.6/§3.7 补 `PeerPublicKey`、`ResolvedWorkspace` 与本地配置值对象，§7 升级为 v2 表结构（9 张管理表 + 2 个索引 + `imported_import` 拆分），§7.2 新增 v1 → v2 迁移规则与 v2 夹具，§9 增补判据 23–29，§11 改为索引）
> 版本：0.9（2026-09-23：§2 的错误枚举补齐 `ConflictKind::{AlreadyExists, IdentityMismatch, DuplicateOwnership}` 与 `UnavailableKind::KeystoreUnavailable` 及到 `local.conflict`/`local.unavailable` 的映射义务；§11.6 写集语义第 4 条按目标族区分落定审计（设备 `pairing.approved` / 节点 `node.paired`）；签名、判据与 DDL 未变）
> 版本：0.8（2026-09-23：管理 store 的落盘实现落地（`crates/storage-sqlite/src/admin/`）后，把 §5.3/§9/§11 与关联文档里「仍待实现」的陈述改为与实现一致；合同形状、判据与 DDL 未变）
> 版本：0.10（2026-09-23：补齐管理存储的**终态与单调性守卫**——§5.2 补 imported 写路径的归属前置（`upsert_session`/`commit_receipt` 先验 `(ownerNodeId, exportId)` 归属，否则 `NotFound(Export)` 且零写入）、§5.3 补 `put_export` 撤销终态 / 身份材料读取核对同行指纹 / 活动时间只前进（显式 `CASE`）、§7.4 补迟到回调拒绝与「重导入后无法区分新旧连接」的已知边界、§9 新增判据 30。**§5/§7 的代码块、端口签名与 DDL 未变**，漂移门禁继续逐条成立；wire 协议、封闭词表与本地管理方法集未变）
> 版本：0.11（2026-09-24：§10.3 的 view 收口落地——§5.1 写明 `TurnAccepted.turn` 是适配器侧占位/审计值（turn 归属由 core 定稿），§6 新增第 19 条（提交前注入 `turnId` 与会话 `version`、冲突与漂移失败关闭、imported 路径保留 Owner 取值），§9 新增判据 31；端口签名与 DDL 均未变）
> 版本：0.12（2026-09-26，`node-link-owner` 变更 WP5：为 Node Link 的资源读面补三条窄 seam——`ReadView::node_link_slice`（会话摘要 + origin head + 未决交互含创建事件 id + `after` 之后的事件含正文，同一次只读事务）、`ReadView::session_event_payload`（按会话归属取正文）、`AuditStore::watermark`（管理写集水位 = `catalogRevision` 的唯一来源）；`NodeLinkHandshakeView` 增 `catalog_revision` 字段（`catalogRevision` 不再取 `store.head()`）。§4 补两条 `[决定]`（`catalogRevision` 口径含已登记的精度边界、三条资源读 seam 的绑定规则），§5.2/§5.3 的 trait 签名同步；DDL 未变，漂移门禁继续逐条成立）

> 版本：0.13（2026-09-26，`node-link-owner` 变更 WP6 修复轮次（RV1-WP6-F1）：`session.create` 的幂等与终态落进 `owned_command`——§4 补一条用例入口（`create_session` / `settle_session_create`）、§5.1 的 `[决定]` 同步签名与指纹义务、§6 新增第 20 条（创建提交回填 `session_id`、终态提交、崩溃窗口走第 16 条恢复、重试与冲突）。**§5/§7 的代码块与 DDL 未变**（未新增端口方法与表列），漂移门禁继续逐条成立）

## 1. 范围与非目标

范围：core 的值对象/用例/端口/错误类型、broker 的提交与发布契约、`storage-sqlite` 的 PRAGMA/文件布局/migration/表结构/索引/保留与容量/崩溃恢复、以及实现该合同的验收判据。

非目标：不写实现代码；不定义 ACP DTO（`acp-protocol`）与 Sync/Node Link wire DTO（各自协议 crate）；不定义 daemon 生命周期、CLI、前端；不设计 `identity-auth` 内部状态机（只定义它需要的持久化端口面）；不做应用层数据库加密（`SECURITY_DESIGN.md` §13.1 既定决策）。

## 2. 依赖、运行时与错误类型

- `core` 不依赖 Tokio、Axum、SQLite、WebSocket、子进程、ACP DTO、任何 wire protocol（`AGENTS.md` §4、`MODULE_ARCHITECTURE.md` §4.1）。
- `[决定]` 端口用可 dyn 化的异步 trait：`#[async_trait]`（纯 proc-macro，不引入 runtime）＋ `Send + Sync + 'static`；组合根持有 `Arc<dyn Port>`。不用 async fn in trait（v1 的 rustc 不支持 trait object）。
- `[决定]` 端口签名只使用 §3 定义的 `core::model` 类型。禁止出现 `sqlx`、`serde_json::Value`、HTTP、JSON-RPC、wire discriminator 或数据库列名。
- `[决定]` 时间与 ID 由端口注入：所有持久化写方法与 `prune` 都接收 `at: Timestamp`（由调用方从 `Clock` 取），`storage-sqlite` 内部**不得**读系统时间。
- `[决定]` 统一错误类型（`MODULE_ARCHITECTURE.md` §8 的 `PortError`）：

```rust
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("not found: {0}")] NotFound(EntityRef),
    #[error("conflict: {0}")] Conflict(ConflictKind),
    #[error("invalid request: {0}")] InvalidRequest(&'static str),
    #[error("unavailable: {0}")] Unavailable(UnavailableKind),
    #[error("corrupt: {0}")] Corrupt(&'static str),
    #[error("backend failure: {0}")] Backend(Box<dyn std::error::Error + Send + Sync>),
}
pub enum ConflictKind {
    VersionMismatch, AlreadyResolved, AlreadyClaimed, Expired, Consumed, IdempotencyConflict,
    AlreadyExists, IdentityMismatch, DuplicateOwnership,
}
pub enum UnavailableKind {
    Busy, StorageFull, IoError, RemoteUnavailable, OwnerOffline, ExportRevoked, KeystoreUnavailable,
}
```

`ConflictKind::{AlreadyExists, IdentityMismatch, DuplicateOwnership}` 与 `UnavailableKind::KeystoreUnavailable` 是管理写集引入的取值（§11.6），与本表同批落地（`crates/core/src/model/error.rs` 的 `ALL`/`as_str` 逐项一致）；本地管理适配器把它们映射为 `LOCAL_ADMIN_PROTOCOL.md` §6 的 `local.conflict`/`local.unavailable`，`port_error_public` 必须显式覆盖这四个取值，不得落进通配臂。

`sqlx::Error`（或任何适配器错误）必须在适配器内映射成上表之一后才可进入 core（`MODULE_ARCHITECTURE.md` §8）。

## 3. `core::model` 值对象（全量冻结）

### 3.1 标识与引用

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `NodeId` / `DeviceId` / `SessionId` / `EventId` / `RequestId` / `TurnId` / `InteractionId` / `PairingId` / `AttachmentId` / `MessageId` | newtype over canonical 小写 UUID 文本（`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`） | `SYNC_PROTOCOL.md` §3.2 |
| `ExportId` | newtype over `^[A-Za-z0-9._-]{1,128}$` 的字符串，**不是 UUID**；`ExportId` 与 `workspaceAlias` 是不同字段、不同模式 | `NODE_LINK_PROTOCOL.md` §12.3；`schemas/node-link/v1/common.schema.json#/$defs/exportId` |
| `ImportId` | newtype over `^[A-Za-z0-9._-]{1,128}$`，Access 本地为主键 | `LOCAL_ADMIN_PROTOCOL.md` §5.5 |
| `WorkspaceAlias` | `^[a-z0-9][a-z0-9._-]{0,63}$` | `NODE_LINK_PROTOCOL.md` §12.3 |
| `AgentRef` | `{ agentId: String(1..=128), name: String(1..=128) }` | `schemas/sync/v1/common.schema.json#/$defs/sessionSummary` |
| `OwnedSessionRef` | `{ sessionId }` | `MODULE_ARCHITECTURE.md` §4.1 |
| `RemoteSessionRef` | `{ ownerNodeId, exportId, sessionId }` | `NODE_LINK_PROTOCOL.md` §7 |
| `OriginEventRef` | `{ ownerNodeId, originEpoch, originEventId }` | `NODE_LINK_PROTOCOL.md` §7 |
| `SessionReference` | enum `Owned(OwnedSessionRef) ｜ Remote(RemoteSessionRef)`；`SessionBackendFactory` 与路由用它 | `MODULE_ARCHITECTURE.md` §4.1/§4.5/§4.6 |
| `EntityRef` | enum `Session(SessionId) ｜ Turn(TurnId) ｜ Interaction(InteractionId) ｜ Command{ session: Option<SessionId>, request: RequestId } ｜ Pairing(PairingId) ｜ Device(DeviceId) ｜ Node(NodeId) ｜ Export(ExportId) ｜ Import(ImportId) ｜ Provider(String)`；`Provider` 承载 `ProviderRef.id`（不是凭据、不是 UUID），用于 `provider.configured` 审计与仓储行定位；`kind()` 给出稳定 token，审计/仓储的 `target_kind` 就取它：`session`/`turn`/`interaction`/`command`/`pairing`/`device`/`node`/`export`/`import`/`provider` | 本合同（错误定位；`Provider` 承载 `ProviderRef.id`，形状见 §3.7） |

`[决定]` **id 的分配只有两处权威**：`SessionId` 与 `EventId` 由 `SessionStore::commit` 在创建/提交事务内分配（前者经 `CommitOutcome.session_id` 回传，后者写进 `owned_event.event_id`）——它们必须与落盘同一时刻产生；`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId`/**`MessageId`** 由 **core** 在调用前用 `IdGenerator` 分配并随写入形状传入（`MessageId` 由生产 delta 的适配器在该消息第一条 delta 提交前分配，见 §6 第 14 条）（`NewTurn.turn`、`PendingInteractionWrite.interaction.id`、`OwnedCommit.origin_epoch` 等），存储层只校验一致性、**不得**另行编号；`AttachmentId` 由 `AttachmentStore::put` 分配并返回（§7.3）。

### 3.2 序号、游标与时间

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `Sequence` | newtype over `u64`；线上为无前导零十进制字符串；`[决定]` v1 上界 `2^63−1`，超出即协议错误（见 §7.3 存储规则） | `SYNC_PROTOCOL.md` §3.2 |
| `Version` | newtype over `u64`，会话乐观并发版本 | `MODULE_ARCHITECTURE.md` §4.1 |
| `AttachmentGeneration` | newtype over `u64` | `NODE_LINK_PROTOCOL.md` §7 |
| `ServerEpoch` | newtype over UUID；事件库首次创建时生成，普通升级与重启保持不变 | `SYNC_PROTOCOL.md` §9.1 |
| `OriginEpoch` | newtype over UUID；**会话级** | `NODE_LINK_PROTOCOL.md` §7 |
| `GlobalCursor` | `{ serverEpoch, globalSequence }` | `SYNC_PROTOCOL.md` §9.1 |
| `OriginCursor` | `{ originEpoch, originSequence }` | `NODE_LINK_PROTOCOL.md` §6/§15 |
| `LocalCursor` | newtype over `u64`（Access 本地投递序号，只服务本节点客户端） | `NODE_LINK_PROTOCOL.md` §6 |
| `Timestamp` | newtype over 文本，固定 `%Y-%m-%dT%H:%M:%S%.3fZ`（UTC、毫秒、`Z`）；TEXT 排序即时间序，因此该宽度是硬约束 | `SYNC_PROTOCOL.md` §3.2 |
| `Digest` | newtype over base64url 32 字节（SHA-256） | `schemas/sync/v1/common.schema.json#/$defs/base64url32` |

### 3.3 会话、turn、命令与交互

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `ResourceOrigin` | enum `Local ｜ Remote { ownerNodeId, exportId, originEpoch, online }`；owned/imported 在类型上可区分 | `MODULE_ARCHITECTURE.md` §4.1；`schemas/sync/v1/common.schema.json#/$defs/originBlock` |
| `Session` | `{ id: SessionId, reference: OwnedSessionRef, title: Option<String(≤512)>, agent: AgentRef, state: SessionState, origin: ResourceOrigin, current_mode: Option<ModeRef>, version: Version, created_at, updated_at, closed_at: Option<Timestamp> }` | `SYNC_PROTOCOL.md` §9.6；`common.schema.json#/$defs/sessionSummary` |
| `SessionSummary` | `Session` 的可投影子集（wire 形状见协议 crate）；owned 与 imported 用同一形状 | `SYNC_PROTOCOL.md` §9.6 |
| `SessionSnapshot` | `Session` + `origin_epoch: OriginEpoch` + `head: GlobalCursor` | 本合同（供 `commit`/`load` 返回） |
| `SessionState` | enum `Idle｜Queued｜Running｜WaitingInput｜WaitingPermission｜Failed｜Closed` | `common.schema.json#/$defs/sessionSummary` |
| `ModeState` | `{ current_mode: Option<ModeRef>, available: Vec<ModeRef> }`——`session.mode.list` 的端口侧形状；`version` 属于会话（用例层补），不在本类型里 | `SYNC_PROTOCOL.md` §11.5/§10.2 |
| `TurnId`/`Turn` | `Turn { id, session: SessionId, state: TurnState, queue_index: u32, causation: Option<RequestId>, started_at/ended_at }` | `SYNC_PROTOCOL.md` §11.6 |
| `TurnState` | enum `Queued｜Running｜WaitingInput｜WaitingPermission｜Completed｜Failed｜Cancelled` | 同上 |
| `ModeRef` / `ModeId` | `{ modeId: String(1..=256), displayName: String(1..=256) }`；`ModeId` 是 `modeId` 的 newtype | `common.schema.json#/$defs/modeRef` |
| `ConfigOptionId` | newtype over `String(1..=256)` | `SYNC_PROTOCOL.md` §11.5 |
| `ConfigValue` | enum `Text(String(≤4096)) ｜ Boolean(bool) ｜ Select(String(1..=256))` | `common.schema.json#/$defs/configOptionView` |
| `ConfigOption` | `{ id, name, description: Option<String(≤1024)>, category: Option<String(≤128)>, kind: Select｜Boolean, current: ConfigValue, options }` | 同上 |
| `CommandKind` | enum `Query ｜ Mutation`（取自 `commands.json` 的分类） | `compatibility/commands/v1/commands.json` |
| `ClientCommand` | `{ actor: Actor, request: RequestId, command: String(命令名), kind: CommandKind, session: Option<SessionId>, expected_version: Option<Version>, payload: CommandPayload }` | `SYNC_PROTOCOL.md` §11.5 |
| `CommandPayload` | enum，按命令名一对一：`SessionList{} ｜ SessionRead{ include } ｜ CommandStatus{ target_request: RequestId } ｜ ModeList{} ｜ ConfigList{} ｜ Prompt{ content } ｜ Cancel{turn: Option<TurnId>} ｜ ModeSet{mode} ｜ ConfigSet{id,value} ｜ PermissionResolve{interaction, option_id} ｜ ElicitationRespond{interaction, action, values}` | `SYNC_PROTOCOL.md` §11.5、§12.7 |
| `CommandReceipt` | enum `Accepted{ request: RequestId, turn: Option<TurnId> } ｜ Rejected{ error: PublicError }`（同步接受，不含终态） | `SYNC_PROTOCOL.md` §11.2 |
| `CommandStatus` | enum `Accepted｜Completed｜Failed｜Rejected｜Uncertain` | `SYNC_PROTOCOL.md` §11.2 |
| `CommandRecord` | `{ session: Option<SessionId>, request: RequestId, command: String, kind: CommandKind, actor: Actor, accepted_at: Option<Timestamp>, status: CommandStatus, terminal_at: Option<Timestamp>, terminal_event: Option<EventId>, result: Option<CommandResult>, error: Option<PublicError>, expected_version: Option<Version>, request_fingerprint: Digest }` | `SYNC_PROTOCOL.md` §11.2/§11.4 |
| `CommandTerminalRecord` | `CommandRecord` 的终态子集（`status`、`terminal_at`、`terminal_event`、`result`、`error`） | 同上 |
| `InteractionId` / `InteractionKind` | `InteractionKind ∈ {Permission, Elicitation}` | `SYNC_PROTOCOL.md` §10.3 |
| `InteractionOption` | `{ optionId: String(1..=128), label: String(1..=512), kind: String(1..=64) }` | `common.schema.json#/$defs/interactionOption` |
| `PermissionDecision` | `{ option_id: String(1..=64), kind: AllowOnce｜AllowAlways｜RejectOnce｜RejectAlways }`；`option_id` **必须**是 Agent 在未解决请求里给出的原始值并原样回传，`kind` 仅用于 UI/审计分类 | `SYNC_PROTOCOL.md` §11.5；`schemas/acp/v1/upstream/schema.json` 的 `PermissionOptionId` |
| `ElicitationAction` | enum `Submit ｜ Decline ｜ Cancel`（对齐 ACP `accept`/`decline`/`cancel`；`decline` 是 ACP 已有而我们 v1 原先缺失的动作，见 §10） | `SYNC_PROTOCOL.md` §11.5；`schemas/acp/v1/upstream/schema.json` 的 `CreateElicitationResponse` |
| `ElicitationValues` | `[决定]` **已冻结**（依据 ACP `ElicitationContentValue`）：有序映射 `键 → 值`，值域为 `Text \| Integer(i64) \| Number(f64) \| Boolean \| TextArray \| Unknown(raw JSON value 文本)`；`Unknown` 原样保留、原样转发，不解释（ACP 要求未知 action/模式保真）。边界：序列化后 ≤64 KiB、深度 ≤16。`Submit` 时可为 `Some(空映射)`（= ACP `content: {}`）或 `None`（= ACP `content: null`），两者可区分；`Decline`/`Cancel` 时必须为 `None` | 本合同；ACP `CreateElicitationResponse` 的 `ElicitationContentValue`（五种线格式） |
| `InteractionResolution` | enum `Permission(PermissionDecision) ｜ Elicitation { action: ElicitationAction, values: ElicitationValues }` | 本合同（统一关联端口的唯一解析入口） |
| `Resolution` | enum `Resolved ｜ AlreadyResolved` | `SYNC_PROTOCOL.md` §11.5 |
| `PendingInteraction` | `{ id, kind, session, created_at, options: Vec<InteractionOption> }`；`options` 在**存储读视图**里恒为空——真值只存在于 `request_event` 指向的事件 payload（§5.2、§6 第 13 条） | `NODE_LINK_PROTOCOL.md` §12.4 |
| `PendingInteractionWrite` | `{ interaction: PendingInteraction, turn: Option<TurnId> }`——新建 pending 交互行的写入形状。**不携带 `request_event`**：`owned_event.event_id` 由存储层在提交事务内分配（§3.1），装配方无法预知；`owned_interaction.request_event` 是 `INTEGER REFERENCES owned_event(global_sequence)`（§7.3），由存储层按「同一提交 + 事件 payload 的 `interactionId` 等于 `interaction.id()`」配对后写入该事件的 `global_sequence`，配不到即整事务 `InvalidRequest` | 本合同（§5.2、§6 第 13 条） |

### 3.4 事件

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `EventId` | 事件稳定标识；imported 事件的 `event_id` 必须等于 origin 事件的 id | `SYNC_PROTOCOL.md` §9.6/§10.1 |
| `EventType` | newtype over `^[a-z0-9_.-]{1,128}$`；未登记取值按未知事件降级处理，**不是**闭合枚举 | `SYNC_PROTOCOL.md` §10.1 |
| `EventKind` | enum `State｜Delta｜FinalMessage｜Structured｜Interaction｜Summary` | `INITIAL_DESIGN.md` §10.2/§10.3 |
| `EventOrigin` | enum `Agent｜Device｜Daemon｜LocalCli` | `SYNC_PROTOCOL.md` §10.1 |
| `EventPayload` | `{ view: ViewJson, acp: Option<AcpRaw> }`；`view` 为公共视图（原样保存），`acp` 为 ACP 原文 | `SYNC_PROTOCOL.md` §10.3 |
| `AcpRaw` | enum `Available { media_type: String, raw_json: String, byte_length: u64, sha256: Digest } ｜ Unavailable { reason: RawUnavailableReason, byte_length: u64, sha256: Option<Digest> }` | `SYNC_PROTOCOL.md` §10.3；`common.schema.json#/$defs/rawAcp` |
| `RawUnavailableReason` | enum `SizeLimit｜RetentionExpired｜StorageFailure` | `common.schema.json#/$defs/rawAcp` |
| `PersistencePolicy` | enum `Durable｜ShortTerm｜Ephemeral`；`Ephemeral` **不得**进入任何提交 | `INITIAL_DESIGN.md` §10.2 |
| `StoredPolicy` | enum `Durable｜ShortTerm`（`OwnedCommit` 只接受它，使 `Ephemeral` 不可表达） | 本合同 |
| `PendingEvent` | `{ kind: EventKind, event_type: EventType, policy: StoredPolicy, payload: EventPayload, origin: EventOrigin, turn: Option<TurnId>, causation: Option<RequestId> }`；`origin` 由 broker 按产生者填（agent 事件 / 设备命令 / daemon / local CLI），存储层只原样写入 `owned_event.origin_kind`——**不得**按「有没有会话」推断（否则每条会话事件都被记成 `agent`，而 `device.revoked` 被记成 `daemon`，`SYNC_PROTOCOL.md` §10.1 的 `origin.kind` 就失去意义） | 本合同（§7.3） |
| `CommittedEvent` | `{ id: EventId, event_type: EventType, session: Option<SessionId>, session_sequence: Option<Sequence>, global_sequence: Sequence, origin_epoch: Option<OriginEpoch>, origin_sequence: Option<Sequence>, created_at: Timestamp }`；**非会话级事件两者均为 `None`**（§3.4、§7.3 的成对 CHECK、§9 判据 17）。`event_type` 是**必填**的：历史（`HistoryPage`）与重放（`CommittedDelivery::Owned`）都必须能在 wire 上报出事件的类型，不能靠调用方另查一次；`kind` **不**进本类型（它只是存储层的保留/压缩维度，wire 不需要） | `SYNC_PROTOCOL.md` §9/§10 |
| `CommittedDelivery` | enum `Owned(CommittedEvent) ｜ Imported { session: RemoteSessionRef, origin: OriginEventRef, origin_sequence: Sequence, local_sequence: LocalCursor, event_type: EventType, payload_digest: Digest, payload: Option<EventPayload> }`（imported 的 `payload` 只在内存中存在） | `MODULE_ARCHITECTURE.md` §7 |

`[决定]` owned 事件同时携带两种会话级序号：`session_sequence`（该会话内递增）与 `origin_sequence`（该会话的 origin cursor 分量）；对 owned 会话两者在同一事务内各自 +1，但**不得**互相替代（`NODE_LINK_PROTOCOL.md` §7 要求跨节点只认 origin cursor）。非会话级事件（`device.revoked` 等）两者均为 `None`，只分配 `global_sequence`（`SYNC_PROTOCOL.md` §10.1）。

### 3.5 身份、信任、审计

| 类型 | 形状 | 出处 |
|---|---|---|
| `Actor` | enum `Device { device: DeviceId, scopes: ScopeSet } ｜ Node { node: NodeId, access_node: NodeId } ｜ LocalCli ｜ PairingClaimant { pairing: PairingId }`；`ScopeSet` 为 scope 名集合。`Node` 的 `node` 是**对端（claimant／连接对端）节点 id**（本轮钉死）：Owner 侧即 claim 里声明的 `accessNodeId`，Access 侧即本地 Import 的 `ownerNodeId`（`NODE_LINK_PROTOCOL.md` §12.5）；配对消费与握手的对端比对（`TrustStore::consume_pairing`、`PairingConsumption`）只比它，`AuditRecord.via_node` 与 broker 判定 Import 授权时的 `ImportRecord.owner_node_id` 也是同一口径；`access_node` 的既有含义不变（该连接的 Access 端点）。`PairingClaimant` 是配对 HTTP 通道的认领方（design D12，构造规则见 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1）：**绑定一个配对**，只在该配对为本次调用目标时才被用例面接受，且永不被 broker 授予任何命令 | `MODULE_ARCHITECTURE.md` §5；`SECURITY_DESIGN.md` §10.2 |
| `DeviceRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3（含 `deviceId`、指纹、`scopes`、状态、时间戳） | 同左 |
| `NodeRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.4（含 `nodeId`、指纹、`grants`、`state`、`ownerEndpoint`）；同一 `nodeId` 可同时存在 `access`/`owner` 两行，读取与撤销按 `(NodeId, NodeKind)` 取行，两种角色共享一条身份材料（§11.5/§5.3）。`lastConnectedAt` 是**最近一次认证成功时间**（§11.6 第 9 条）：每次成功认证的收尾写集都推进它，值只前进不倒退（§5.3 约束） | 同左 |
| `PairingRecord` | `{ id: PairingId, target: PairingTarget(Device｜Node), state: PairingState, display_name: Option<String(1..=128)>, requested_scopes: ScopeSet, requested_grants: GrantSet, secret_digest: Digest, host_binding: String(1..=2048), created_at, expires_at, claimed_at: Option, approved_at: Option, terminal_at: Option }`；构造校验：`claimed_at` 非空 ⟺ 状态不是 `created`；`approved_at` 非空 ⟺ `approved`/`consumed`；`terminal_at` 非空 ⟺ `rejected`/`expired`/`consumed`；`host_binding` 非空（空串 → `InvalidValue::Empty`）；**设备配对不得带 grants、节点配对不得带 scopes**。`secret_digest` 是 pairing secret 的 SHA-256——明文只存在于创建方内存（`SECURITY_DESIGN.md` §13.1）。`host_binding` 是**登记方**宣告的绑定（设备为 canonical origin，节点为本机在该配对中的 endpoint），落 `owned_pairing.host_binding`，认领时必须被对端逐字回显（§11.2 第 1 条、§7.3） | 本合同（**首个实现已冻结**，见 `crates/core/src/model/identity.rs`）；方法形状见 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 |
| `PairingState` | enum `Created｜Claimed｜PendingConfirmation｜Approved｜Rejected｜Expired｜Consumed`；终态 = `Rejected｜Expired｜Consumed`；`Claimed` **不对外可见**（只在服务端事务与审计里出现） | `SYNC_PROTOCOL.md` §7.0 |
| `PeerIdentity` | enum `Device(DeviceId)｜Node(NodeId)`；`kind()` 给出 "device"/"node" | 本合同 |
| `PeerPublicKey` | `65 字节 SEC1 未压缩 P-256 公钥`：私有字段 + `try_from_bytes`（`TryFrom<&[u8]>` 委托同一实现）；构造顺序固定（长度 == 65 → 首字节 == `0x04` → `p256::PublicKey::from_sec1_bytes` 成功），任一步失败 → `InvalidValue`；**长度断言必须先于解析**（`from_sec1_bytes` 接受 33 字节压缩点，不先断言就会绕过「SEC1 uncompressed」合同）；`fingerprint()` 是唯一指纹入口 = `SHA-256(65 字节原始公钥)` 的 64 字符小写 hex，适配器不得各自现算；私钥、keystore handle、pairing secret 明文不进本类型 | `crates/core/src/model/identity.rs`；`IDENTITY_AND_AUTH_CONTRACT.md` §3 |
| `PairingPeer` | `{ id: PeerIdentity, display_name: String(1..=128), public_key: PeerPublicKey, host_binding: String(1..=2048), client_nonce: Nonce }`；指纹不再单独存放，由 `public_key.fingerprint()` 派生（`LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 wire 字段 `publicKeyFingerprint` 语义不变，值变成派生结果）；`host_binding` 是 claim 里回显的绑定（设备为 `canonicalOrigin`、节点为 `endpoint`），必须与登记的 `PairingRecord.host_binding` 逐字相等，`owned_pairing_peer` 不单独存该列（读取时从配对行回填） | `SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2 |
| `PairingClaim` | `{ pairing: PairingId, peer: PairingPeer, requested_scopes: ScopeSet, requested_grants: GrantSet }`——HMAC/proof 由**调用方**验证，进入本类型时只剩已核对的事实；设备配对不对带 grants、节点配对不得带 scopes | 本合同（`TrustStore::claim_pairing` 的输入） |
| `PairingSettlement` | enum `Approved { granted_scopes: ScopeSet, granted_grants: GrantSet }｜Rejected { reason: Option<String(≤256)> }`；`granted_*` 是**用户确认的最终集合**（不是请求值）；`reason` 是简短原因，不进审计正文 | 本合同（`TrustStore::settle_pairing` 的输入） |
| `PairingConsumption` | `{ pairing: PairingId, actor: Actor, context: WriteContext }`；`actor` 是本次认证的主体，只接受与该配对**已批准对端**一致的 `Actor::Node`/`Actor::Device`，`context.audit` 必须携带与它同主的审计行（`node.authenticated`/`device.authenticated`） | 本合同（`TrustStore::consume_pairing` 的输入） |
| `ExportRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 `ExportView`（`exportId`、`agentIds`、`workspaceAliases`、`defaultWorkspaceAlias`、`templates`、**`scopes`**、`cachePolicy`、`createdAt`、`revokedAt`） | 同左（字段名已按本次决定与 Node Link 对齐） |
| `ImportRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 `ImportRecord`（`importId`、`ownerEndpoint`、`ownerNodeId`、`exportIds`、`grants`） | 同左 |
| `AuditRecord` | `{ at: Timestamp, action: AuditAction, actor: Actor, via_node: Option<NodeId>, local_principal_ref: Option<String(≤128)>, target: EntityRef, outcome: AuditOutcome, detail_digest: Option<Digest> }`；**不含任何内容** | `SECURITY_DESIGN.md` §14.2 |
| `AuditAction` | 闭合枚举：`PairingCreated｜PairingClaimed｜PairingApproved｜PairingRejected｜PairingExpired｜DeviceAuthenticated｜DeviceAuthFailed｜DeviceRevoked｜DeviceScopesChanged｜NodePaired｜NodeAuthenticated｜NodeAuthFailed｜NodeTrustRevoked｜NodeIdentityChanged｜ExportCreated｜ExportRevoked｜ImportAdded｜ImportRemoved｜ProviderConfigured｜AuthorizationDenied｜RateLimitTriggered｜StorageIntegrityFailed` | `SECURITY_DESIGN.md` §14.2 |
| `AuditOutcome` | enum `Success｜Denied｜Failed` | 本合同 |

### 3.6 后端与能力

| 类型 | 形状 | 出处 |
|---|---|---|
| `AgentDescriptor` | `{ agent: AgentRef, available: bool, origin: ResourceOrigin }` | `MODULE_ARCHITECTURE.md` §4.1 |
| `Capability` / `CapabilitySet` | `Capability { kind: String(1..=128), detail: Option<String(≤256)> }`；`CapabilitySet` 为去重集合 | `ACP_COMPATIBILITY_MATRIX.md` §4 |
| `CreateSessionRequest` | `{ agent: AgentRef, workspace: Option<ResolvedWorkspace>, template: Option<TemplateSelection>, origin: ResourceOrigin }`——alias → 路径的解析在 `UseCases::create_session` 内完成（§5.1），后端只收已解析路径 | `NODE_LINK_PROTOCOL.md` §12.7 |
| `ResolvedWorkspace` | `{ alias: WorkspaceAlias, canonical_path: String }`；`canonical_path` 是本机规范化绝对路径，只交给后端，不得进事件、错误 `details`、审计 `detail_digest` 的前像或 Node Link catalog（解析、校验与失败分类见 §5.1） | 本合同 |
| `TemplateSelection` | `{ template_id: String(1..=128), params: Vec<(String, ConfigValue)> }` | `NODE_LINK_PROTOCOL.md` §12.3 |
| `PromptRequest` | `{ content: Vec<PromptContentBlock> }`（形状见协议 crate 的 `promptContentBlock`） | `SYNC_PROTOCOL.md` §11.5 |
| `EndpointEvent` | `{ kind: EventKind, event_type: EventType, payload: EventPayload, turn: Option<TurnId>, causation: Option<RequestId>, at: Timestamp }`（`SessionEndpoint` 的输出流元素） | 本合同 |

### 3.7 本地配置与凭据值对象

| 类型 | 形状 | 出处 |
|---|---|---|
| `AgentProfile` | `{ id: AgentId, display_name: String(1..=128), command: String, args: Vec<String>, env_allowlist: Vec<String>, env: Vec<ProviderEnvBinding>, default: bool, created_at: Timestamp, updated_at: Timestamp }`；`command` 是可执行文件路径或名字、**不经 shell 拼接**；`env_allowlist` 是环境变量名白名单（上限而非提示）；`env` 为空表示不注入任何凭据；至多一条 `default = true`（§7.3 的部分唯一索引） | 本合同（`crates/core/src/model/config.rs`）；写入路径 `LocalConfigStore::put_profile`（§5.3） |
| `ProviderEnvBinding` | `{ provider_id: String, field: String, name: String }`；不变式：`name` 必须同时出现在 `env_allowlist` 里（绑定不能越过白名单）；`(provider_id, field)` 在同一条 profile 内不重复；`name` 不得是保留名（如 `ACP_REMOTE_*`）；`provider_id`/`field` 必须在 `owned_provider_ref` 的已配置字段内 | 本合同（`agent.configure` 写入时校验，运行期失效只影响新启动的进程） |
| `WorkspaceRecord` | `{ alias: WorkspaceAlias, display_name: String(1..=128), canonical_path: String, created_at, updated_at }`；`canonical_path` 只在本节点可读，不进 Node Link catalog（§11.1） | 本合同（`crates/core/src/model/config.rs`） |
| `ProviderRef` | `{ id: String(^[A-Za-z0-9._-]{1,64}$), kind: ProviderRefKind, display_name: String(1..=128), configured_fields: Vec<String>, keystore_ref: String, version: u64, updated_at: Timestamp }`；只记字段名、keystore 引用与版本，**永不记凭据值**；换绑必须递增 `version`（§11.2 第 7 条） | `LOCAL_ADMIN_PROTOCOL.md` §5.2；`SECURITY_DESIGN.md` §13.1 |
| `ProviderRefKind` | enum `Provider｜Mcp` | 同左 |
| `SeedState` | `{ seeded: bool, seeded_at: Option<Timestamp> }`；`seeded = false` 时启动流程才能导入种子，且该标记与种子 profile 同一事务提交（`LocalConfigStore::mark_seeded`，§5.3） | `CONFIG_REFERENCE.md`「配置与管理状态的权威」 |
| `SecretValue` | 不透明凭据值（私有字段）：只经端口进出，**不实现** `Debug`/`Serialize`，不得落盘、进事件、进日志或进错误消息 | `IDENTITY_AND_AUTH_CONTRACT.md` §7 |

## 4. `core::use_cases` 用例面

`[决定]` **每个**入口的第一个参数都是 `actor: &Actor`：授权只在 core 判定，`server` 不得传入“已授权”标志，也不得代替 core 判定（`AGENTS.md` §3/§4、`SECURITY_DESIGN.md` §8）。全部 mutation 只同步接受。

| 用例族 | 入口 | 调用方 |
|---|---|---|
| `SessionCommands` | `submit_command(actor, ClientCommand) -> CommandReceipt` | `server::sync`、`server::node_link`、`server::acp_facade` |
| `SessionLifecycle` | `create_session(actor, RequestId, Digest, CreateSessionRequest, Option<WorkspaceAlias>) -> SessionId`、`settle_session_create(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` | `server::node_link`（§6 第 20 条） |
| `SessionQueries` | `list_sessions(actor, SessionQuery) -> Vec<SessionSummary>`、`read_session(actor, ReadQuery) -> HistoryPage` | 同上 |
| `ConfigCommands` | `set_mode(actor, SessionReference, ModeId) -> Version`、`set_config(actor, SessionReference, ConfigOptionId, ConfigValue) -> Version` | 同上 |
| `PermissionCommands` | `resolve_interaction(actor, SessionReference, InteractionId, InteractionResolution) -> Resolution` | 同上 |
| `SubscriptionQueries` | `replay(actor, SessionReference, Option<GlobalCursor>, ReplayLimit) -> ReplayBatch`、`retention_window(actor, SessionId) -> Option<(Sequence, Sequence)>` | `server::sync`、`server::node_link` |
| `DeviceManagement` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 `device.*`/`node.*` | `server::local_admin`、`app::cli` |
| `ExportManagement` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 的 `export.*`/`import.*` 写方法 | 同上 |
| `RemoteCatalogQueries` | 见 §5.5 读取方法与 `NODE_LINK_PROTOCOL.md` §12.3 catalog 投影 | 同上、`server::node_link` |
| `PairingChannel` | `claim_pairing(actor, PairingClaim) -> PairingClaimOutcome`、`pairing(actor, PairingId) -> Option<PairingRecord>`、`pairing_channel_view(actor, PairingId) -> Option<PairingChannelView>`、`consume_pairing(actor, PairingId) -> PairingRecord` | `server::node_link`（及未来 `server::sync`）；前两项另由 `server::local_admin` 以 `Actor::LocalCli` 调用 |
| `NodeLinkHandshake` | `node_link_handshake_view(accessNodeId) -> NodeLinkHandshakeView`、`record_node_link_auth(accessNodeId, AuditAction, AuditOutcome) -> ()`、`record_node_connected(accessNodeId) -> ()` | `server::node_link`（及未来 `server::sync` 的同形入口） |

`[决定]` 配对通道的 actor 规则（design D12）：`claim_pairing`/`pairing`/`pairing_channel_view` 在 `Actor::LocalCli` 之外**只**接受 `Actor::PairingClaimant`，且 `actor.pairing` 必须等于本次调用的目标配对，否则与其它 actor 一样得到 `authorization.scope_denied`（同一拒绝形状，不区分「不是本机入口」与「绑定了别的配对」）；`consume_pairing` **只**接受与该配对已批准对端一致的 `Actor::Node`/`Actor::Device`，把 `approved` 推进到 `consumed`（`terminal_at` + 审计同一写集），已是 `consumed` 且对端一致时幂等成功（不覆盖首次 `terminal_at`，也不重复写审计）。

`[决定]`（2026-09-26，seam 补全）`pairing_channel_view` 是配对 HTTP 端点的**唯一**只读入口：它一次带回记录、已认领的对端行与已批准后的对端节点行（`grant.*` 的唯一来源，`PairingRecord` 不带 `granted_*`），不含任何写入，也不返回秘密材料（pairing secret 只在状态机内存）。认领路径允许在 proof 校验**之前**读取（端点必须先拿到记录才能校验 HMAC），但状态推进仍只能经 `claim_pairing` 的写集、且只在 proof 通过后提交；拒绝仍收集在同一类 `authorization.scope_denied`。入口绑定式读取（claimant 只能读自己那个配对的三类行）是硬约束：不允许放开为通用只读。

`[决定]`（2026-09-26，WSS 握手 seam 补全；同一方向的第二次补全）`NodeLinkHandshake` 是 WSS 握手准入的三条窄入口，服务 `design.md` D3 与 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1：

- `node_link_handshake_view(access_node)`：用例面**唯一**没有 `actor` 的入口——握手完成前不存在已验证主体，`node.hello` 里的 `accessNodeId` 只是待验证的自报身份。授权面因此收窄到「单个自报 node id」：只读该 id 自己的 `access` 信任行、已绑定验签公钥、最近一次配对（`TrustStore::pairing_for`）与本机全局水位 `store.head()`；未知 id 返回**空视图**而不是错误（`NODE_LINK_PROTOCOL.md` §12.2：不得用错误区分节点是否存在），零写入、零审计、不含秘密材料。`Revoked`/`Unknown` 由调用方按信任行状态映射为凭据状态（§5.1：不是握手失败）。视图形状 `NodeLinkHandshakeView { node: Option<NodeRecord>, public_key: Option<PeerPublicKey>, pairing: Option<PairingRecord>, head: GlobalCursor, catalog_revision: u64 }`：五项都是握手本次调用需要且只需要的持久事实；`head` 是 `serverEpoch` 的来源，`catalog_revision` 是 `node.challenge.catalogRevision` / `node.ready.catalogRevision` 的来源。
- `record_node_link_auth(access_node, action, outcome)`：只接受 `node.authenticated`/`node.auth_failed`/`authorization.denied`（其余 → `authorization.scope_denied`），归因 `actor`/`via_node` 都是该对端 id、`target = Node(对端)`、`localPrincipalRef` 为 `None`（§8.3 节点级信任）。首次认证成功（已批准配对 → `consumed`）的那条 `node.authenticated` 由 `consume_pairing` 的写集提交，适配器不得再补一条。`authorization.denied` 服务命令管线：`server::node_link` 的可见性/授权交集比 `Broker::authorize` 的 Owner 侧判定更严，被它拒的命令不会到达 broker，拒绝留痕只能经本入口（`design.md` D8 第 2 步）。
- `record_node_connected(access_node)`：**重复认证**（没有待消费配对）的收尾写集（§11.6 第 9 条）——把 `(access_node, access)` 行的 `last_connected_at` 推进到本次时间（只前进不倒退）并把 `node.authenticated` 在同一事务提交；归因与上一条逐字一致。首次认证的同一职责由 `consume_pairing` 的写集承担，两者不得互相替代（否则 `lastConnectedAt` 会停在首次认证的时刻）。

`[决定]`（2026-09-26，`node-link-owner` 变更 D4/G5 的实现期结论）**`catalogRevision` 的唯一来源是 [`AuditStore::watermark`]**（本机审计自增序列，`storage-sqlite` 取 `sqlite_sequence.owned_audit`），不再用 `store.head()`：全局水位会随保留期裁剪回退，而 `catalogRevision` 要跨重启与清理单调。`node.challenge`/`node.ready`/`catalog.snapshot` 三个字段同源（实测不变式：同一次握手的挑战与 `node.ready` 必然相等，因为两者用同一份视图快照）。**已登记的精度边界**：该水位随每一行审计前进，包括 `node.auth_failed`/`authorization.denied`/`rate_limit.triggered` 这类不改变导出目录的动作，因此「revision 未变 ⇒ 目录未变」并不成立（反向「目录变了 ⇒ revision 必变」成立）。v1 不依赖前者：`knownRevision` 非空时仍回完整快照（`catalog.changed` 属 `post_mvp`）。

`[决定]`（2026-09-26，同一变更）**Node Link 资源读面的三条端口 seam（`ReadView::node_link_slice`/`ReadView::session_event_payload`/`AuditStore::watermark`）与它们的四个用例入口（`node_link_catalog_view`/`node_link_session_view`/`node_link_replay`/`node_link_event_payload`）**（见 `server::node_link` 的 catalog/resource 模块）：与 `NodeLinkHandshake` 同一模式——无 `actor`、按已认证 `accessNodeId` 绑定、零写入、零审计；会话级读必须同时给出 `(access_node, exportId, session)`，由 core 硬校验「对端是已配对的 `access` 行 + Export 存在且未撤销 + 会话的 Agent 属于该 Export」，**不得**复用 `Broker::authorize` 的 `Actor::Node` 分支（那是 Access 侧本地客户端的语义）。「这个 Export 是否对该节点可见」是适配器的**单点可见性策略**（D14：未撤销且 `export.scopes ∩ 节点 grants ≠ ∅`），不在 core 重复实现。

四个用例入口都不放宽既有 `LocalCli` 路径的行为，也不新增任何对端可见的能力。

## 5. `core::ports` 出站端口

`[决定]` 下列签名是**冻结形状**：实现可增加私有辅助方法，但不得改变既有方法的语义与参数；新增公共方法必须先改本合同。

### 5.1 会话后端

```rust
#[async_trait]
pub trait AgentCatalog: Send + Sync {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError>;
    async fn agent_capabilities(&self, agent: &AgentRef) -> Result<CapabilitySet, PortError>;
}

#[async_trait]
pub trait SessionBackendFactory: Send + Sync {
    /// `session` 是 core 已在存储里创建好的 `SessionId`（来自 `CommitOutcome.session_id`），后端**必须**用它
    /// 构造 `SessionEndpoint::reference()`；`request` 携带完整目标（agent/workspace/模板/origin）；`sink` 用于
    /// 交付后端事件。
    async fn create(&self, session: &SessionId, request: CreateSessionRequest, sink: EventSink) -> Result<Box<dyn SessionEndpoint>, PortError>;
    async fn open(&self, reference: SessionReference, sink: EventSink) -> Result<Box<dyn SessionEndpoint>, PortError>;
}

#[async_trait]
pub trait SessionEndpoint: Send + Sync {
    fn reference(&self) -> SessionReference;
    async fn prompt(&self, request: PromptRequest, at: Timestamp) -> Result<TurnAccepted, PortError>;
    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError>;
    /// 模式的只读枚举（`session.mode.list` 的唯一来源，§6 第 17 条）：候选列表来自 ACP 的
    /// `SessionModeState.availableModes`；core 不用它做授权或状态迁移，适配器不得凭当前模式编造候选。
    async fn modes(&self) -> Result<ModeState, PortError>;
    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError>;
    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError>;
    async fn set_config(&self, id: &ConfigOptionId, value: ConfigValue) -> Result<(), PortError>;
    /// 权限与 elicitation 共用的唯一解析入口。
    async fn resolve_interaction(&self, interaction: &InteractionId, resolution: InteractionResolution) -> Result<(), PortError>;
    async fn read_history(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;
    async fn close(&self) -> Result<(), PortError>;
}
```

- `[决定]` 后端事件通道：`create`/`open` 接收 `EventSink`（`[决定]` 定义为 `Arc<dyn Fn(EndpointEvent) + Send + Sync>` 的包装类型，由 core 提供有界队列的发送端）；`EventSink` 的调用顺序即提交顺序，broker 按该顺序组装 `OwnedCommit`（§6 第 1/3 条）。**不**在 `SessionEndpoint` 上暴露 `next_event`，以免后端自己持有排序权。
- `[决定]` **`TurnAccepted.turn` 是适配器侧占位/审计值，不是 turn 归属的权威来源**：turn 归属一律由 core 在提交前用自己的 `TurnId`（`IdGenerator::turn_id`）定稿并写入 `owned_event.turn_id` 与事件 view 的 `turnId`（§6 第 19 条）；适配器返回的值**不得**参与归属决策、不得产生第二个 turn 行，也不得影响事件顺序（§9 判据 31）。
- `[决定]` `read_history` 的分流：owned 由 `storage-sqlite` 从事件日志回答；imported 由 `node-link-client` 在线回源 Owner，Owner 不可达返回 `PortError::Unavailable(RemoteUnavailable)`。
- `[决定]` **workspace 解析归 core**（§3.6 的 `CreateSessionRequest.workspace` 是 `Option<ResolvedWorkspace>`）：`UseCases::create_session(actor, requestId, requestFingerprint, request, workspace_alias)` 在调用 `SessionBackendFactory::create` **之前**完成 alias → 规范化绝对路径的解析与校验，后端只收到 `ResolvedWorkspace`，**不得**自己查存储、也不得按约定拼路径。`requestId` 与 `requestFingerprint` 由适配层传入（Node Link 的幂等键是 `(ownerNodeId, accessNodeId, requestId)`，指纹是 ACPR-CJ1 之后的解码 payload 摘要）：core **不得**自造 requestId 或指纹，否则同一次重试会得到第二个幂等键（§6 第 20 条）。终态由 `UseCases::settle_session_create(actor, requestId, status, result, error)` 提交（没有持久记录或记录已终结时是幂等 no-op），`completed` 的 `result` 由适配层投影（Node Link 的 `SessionCreateResult` 原文）。校验（与 `SECURITY_DESIGN.md` §12.3 同口径）：必须是绝对路径、必须存在、必须是目录；`canonicalize`（解析 symlink/junction/大小写/`.` 与 `..`）的结果作为权威值，拒绝相对路径与含 `..` 的输入。失败分类：alias 未在该 Export 中声明 → 参数类错误（`NODE_LINK_PROTOCOL.md` §12.7 的 `nodelink.export.not_granted`）；alias 已声明但**本机**解析失败（目录被删/不是目录/`canonicalize` 失败）→ `PortError::Unavailable(UnavailableKind::IoError)`，在线映射为服务端错误（`nodelink.internal.unavailable`），**不得**降级为参数错误。别名命名空间：Export 的 `workspace_aliases[].alias` 就是本机 `owned_workspace.alias`，Export 不复制路径，`export.create` 必须校验每个 alias 已存在。`canonical_path` 只出现在该调用入参里：不进事件、错误 `details`、审计 `detail_digest` 的前像或 Node Link catalog。UNC/网络路径允许解析且不改变授权模型，是否记结构化警告由 `server` 层决定（core 不引入日志依赖）。

### 5.2 持久化端口

```rust
pub struct OwnedCommit {
    pub session: Option<SessionId>,          // None = 非会话级事件（如 device.revoked）
    pub at: Timestamp,
    pub expected_version: Option<Version>,
    pub state: Option<StateChange>,
    pub turns: Vec<TurnChange>,
    pub events: Vec<PendingEvent>,
    /// 新建 pending 交互行（权限 / elicitation 请求）。每条都必须与**同一提交**里那条
    /// `interaction` 事件一一对应；解析既有行走 `state.interaction`，两条路径互斥（§6 第 13 条）。
    pub interactions: Vec<PendingInteractionWrite>,
    /// 本提交里的 summary 事件**替代**的既有事件（`kind='delta'`、同一会话、尚未被压缩），
    /// 由存储层把它们的 `compacted_into` 置为该 summary 行的 `global_sequence`（§6 第 15 条）。
    pub compacted: Vec<GlobalCursor>,
    pub idempotency: Option<IdempotencyRecord>,   // 含指纹与 expected_version
    pub command_terminal: Option<CommandTerminalRecord>,
    pub origin_epoch: Option<OriginEpoch>,        // 新建会话时由 core 生成并传入
}

pub struct CommitOutcome {
    pub session_id: Option<SessionId>,   // 新建会话时返回分配到的 id
    pub origin_epoch: Option<OriginEpoch>,
    pub version: Version,
    pub appended: Vec<CommittedEvent>,
    pub replayed: Option<IdempotentReplay>,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError>;
    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError>;
    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError>;
    async fn head(&self) -> Result<GlobalCursor, PortError>;
    /// 一致性读视图：`sync.snapshot_*` 必须在本方法返回的视图内完成（barrier 依据）。
    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError>;
    async fn find_request(&self, request: &RequestId, actor: &Actor) -> Result<Option<CommandRecord>, PortError>;
    /// 启动恢复（§6 第 16 条）：`status='accepted'` 且 `terminal_event_id IS NULL` 的 mutation 行，
    /// 按 `accepted_at` 升序；走 §7.3 的 `owned_command_status` 索引。
    async fn unsettled_commands(&self, limit: ReplayLimit) -> Result<Vec<CommandRecord>, PortError>;
    async fn retention_window(&self, session: &SessionId) -> Result<Option<(Sequence, Sequence)>, PortError>;
    async fn prune(&self, policy: RetentionPolicy, at: Timestamp) -> Result<PruneReport, PortError>;
    async fn health(&self) -> Result<StoreHealth, PortError>;
}

#[async_trait]
pub trait ReadView: Send + Sync {
    async fn head(&self) -> Result<GlobalCursor, PortError>;
    async fn replay(&self, after: Option<GlobalCursor>, limit: ReplayLimit) -> Result<ReplayBatch, PortError>;
    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;
    /// 取回一条 owned 事件的持久化正文——重放、`sync.snapshot_*` 与 `PendingInteraction.options`
    /// 补全的唯一入口（复用 core 既有 `EventPayload`/`AcpRaw`，不新增类型）。
    ///
    /// `view` 必须是库里 `payload_json` 的**原样文本**（不得经通用 JSON 解析后重新序列化）；`acp`
    /// 可用时 `raw_json` 必须字节保真，不可用时按行里的 `acp_raw_unavailable_reason` 还原为
    /// `AcpRaw::Unavailable`。事件不存在返回 `None`。
    async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError>;
    /// Node Link 的会话读视图（`NODE_LINK_PROTOCOL.md` §12.4，`design.md` D6）：`after` 之后的会话事件
    /// **含持久化正文**，以及快照需要元数据（`after` 的 epoch 是否一致由调用方判定）。
    async fn node_link_slice(&self, session: &SessionId, after: Option<OriginCursor>, limit: ReplayLimit) -> Result<NodeLinkSlice, PortError>;
    /// 指定会话内某条 owned 事件的正文（事件 id 与会话必须同时匹配，否则 `None`）；还原规则同 `event_payload`。
    async fn session_event_payload(&self, session: &SessionId, event: &EventId) -> Result<Option<EventPayload>, PortError>;
}

pub struct DeliveryReceipt {
    pub session: RemoteSessionRef, pub origin: OriginEventRef, pub origin_sequence: Sequence,
    pub event_type: EventType, pub payload_digest: Digest, pub at: Timestamp,
}

#[async_trait]
pub trait RemoteDeliveryStore: Send + Sync {
    async fn commit_receipt(&self, receipt: DeliveryReceipt) -> Result<ReceiptOutcome, PortError>;
    async fn local_replay(&self, after: Option<LocalCursor>, limit: ReplayLimit) -> Result<Vec<DeliveryIndexEntry>, PortError>;
    async fn ack(&self, session: &RemoteSessionRef, cursor: OriginCursor, at: Timestamp) -> Result<AckOutcome, PortError>;
    async fn load_ack(&self, session: &RemoteSessionRef) -> Result<Option<OriginCursor>, PortError>;
    async fn upsert_session(&self, record: ImportedSessionRecord, at: Timestamp) -> Result<(), PortError>;
    async fn list_sessions(&self, query: ImportedSessionQuery) -> Result<Vec<ImportedSessionRecord>, PortError>;
    /// `SessionStore::find_request` 的 imported 对应物。**改名过**（原名与前者同名，会迫使每个
    /// 同时持有两个 trait 的调用点写 UFCS）。
    async fn find_remote_request(&self, session: &RemoteSessionRef, request: &RequestId) -> Result<Option<RemoteCommandRef>, PortError>;
    /// 删除该 import 名下的交付索引与命令引用；**不删除审计行**（`SECURITY_DESIGN.md` §11.5 要求保留审计）。
    async fn drop_import(&self, import: &ImportId) -> Result<DropReport, PortError>;
    async fn prune(&self, policy: RetentionPolicy, at: Timestamp) -> Result<PruneReport, PortError>;
}
```

约束：

- `commit` / `commit_receipt` 是各自家族的**唯一**写入口；禁止可分别调用的 repository/journal/deduper（`MODULE_ARCHITECTURE.md` §4.1）。
- `[决定]` imported 写路径的**归属前置**（§11.2 第 5 条）：`upsert_session` 与 `commit_receipt` 都必须在同一写事务内先确认 `(owner_node_id, export_id)` 仍归属某个 Import（`imported_import_export` 有行），否则返回 `NotFound(EntityRef::Export(exportId))` 且零写入——不重建 `imported_session`、不写 `imported_delivery_index`/`imported_command_ref`，也不推进 `local_sequence`。关联行缺失即「该 Import 已被完整移除或从未添加」；同一 `(ownerNodeId, exportId)` 被重新导入后无法区分新旧连接（需导入实例标识或连接代际，见 §7.4）。
- `[决定]` `origin_epoch` 由 **core** 在创建会话时用 `IdGenerator` 生成并传入（响应审查：存储层返回它会让无创建需求的提交也必须回读）；存储层只校验“该会话已有 epoch 时必须一致”。
- `[决定]` 幂等命中返回 `CommitOutcome::replayed`，不追加事件、不改状态。
- `[决定]` 交互的创建与解析规则见 §6 第 13 条。`SessionStore` **没有** `resolve_interaction` 方法：解析是 `OwnedCommit.state.interaction` 的一部分；`SessionEndpoint::resolve_interaction` 是后端（Agent）侧入口，不落盘。
- `[决定]` `retention_window` 返回该会话仍可重放的 `session_sequence` 下界/上界；broker 据此决定 `sync.reset_required`（`reason` 枚举 `initial_sync|epoch_mismatch|cursor_expired|cache_incompatible`，`SYNC_PROTOCOL.md` §9.4）；cursor 的四种拒绝原因：格式非法 → `malformed`（协议层）、`serverEpoch` 与 `meta.server_epoch` 不符 → `epoch_mismatch`、超出 `head()` → `beyond_head`、低于窗口下界 → `cursor_expired`（`SYNC_PROTOCOL.md` §9.2）。

### 5.3 信任、Export、审计与附件

实现状态：SQLite 已实现附件端口、§7 的 v2 表结构与三个管理 store 的**落盘实现**（管理写集的一事务提交、失败关闭与容量纳入，见 `crates/storage-sqlite/src/admin/` 与 §9 判据 23–29）；core 侧同时保留测试替身。下面列出的签名与 §5.1/§5.2 一样是**冻结形状**，由 `scripts/check-contract-drift.mjs` 与 `crates/core/src/ports.rs` 逐条绑定。

```rust
/// 撤销原因。v1 的 `device.revoke`/`node.revoke` 只传 id，因此调用方填 `UserRequested`；
/// `node.identity_changed` 填 `KeyChanged`；带外撤销（丢失/泄露）填 `Compromised`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeReason {
    UserRequested,
    KeyChanged,
    Compromised,
}

/// 原子认领的结果：认领后的记录（状态已推进到 claimed 一档）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingClaimOutcome {
    pub pairing: PairingRecord,
}

/// 配对落定后创建的信任记录引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustRecordRef {
    Device(DeviceId),
    Node(NodeId),
}

/// 待写入的审计行（§11.6）：与状态变更同事务；审计写失败则整事务失败（§11.2 第 6 条）。
///
/// 字段与 `AuditRecord` 完全一致，只是没有 `at`（`at` 由 [`WriteContext`] 提供）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAudit {
    pub action: AuditAction,
    pub actor: Actor,
    pub via_node: Option<NodeId>,
    pub local_principal_ref: Option<String>,
    pub target: EntityRef,
    pub outcome: AuditOutcome,
    pub detail_digest: Option<Digest>,
}

/// 管理写集的公共上下文（§11.6）。`at` 由调用方从 `Clock` 取（§2：存储层不读系统时间）。
///
/// `audit` 为空只允许用于不作为 `AuditAction` 已登记安全动作的操作（例如 `workspace.select`、
/// `agent.configure`）；涉及安全动作的写集必须至少带一条成功或失败审计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteContext {
    pub at: Timestamp,
    pub audit: Vec<PendingAudit>,
}

/// 写入一个设备记录（§11.6）：同 ID 不得换绑公钥，也不得把 `revoked` 改回 `active`。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceWrite {
    pub record: DeviceRecord,
    pub context: WriteContext,
}

/// 写入一个节点角色行并绑定身份材料（§11.6）：同一 `nodeId` 的两种角色必须指纹一致。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeWrite {
    pub record: NodeRecord,
    pub public_key: PeerPublicKey,
    pub context: WriteContext,
}

/// 撤销一个设备（§11.6）：单事务写撤销时间、状态与审计；提交后才由组合根关闭连接。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRevocation {
    pub device: DeviceId,
    pub reason: RevokeReason,
    pub context: WriteContext,
}

/// 按 NodeId 撤销一个节点（§11.6）：同一事务令两种角色一起进入 `revoked`。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRevocation {
    pub node: NodeId,
    pub reason: RevokeReason,
    pub context: WriteContext,
}

/// 登记一次性配对（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingWrite {
    pub record: PairingRecord,
    pub context: WriteContext,
}

/// 原子认领（§11.6）：单事务内检查「存在、未过期、仍为 `created`、本机绑定一致」，插入唯一 peer 行
/// 并推进到 `pending_confirmation`；HMAC/proof 由调用方验证。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingClaimWrite {
    pub claim: PairingClaim,
    pub context: WriteContext,
}

/// 落定配对（§11.6）：单事务完成状态/过期检查、固定 peer 与最终 scopes/grants 校验、创建信任记录、
/// 更新配对状态并写审计。peer 公钥从配对的对端行读回，不由调用方重复提供。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingSettlementWrite {
    pub pairing: PairingId,
    pub settlement: PairingSettlement,
    pub context: WriteContext,
}

/// 过期扫描（§11.6）：只终结未确认且已过期的配对，返回终结行数。
#[derive(Debug, Clone, PartialEq)]
pub struct ExpiryWrite {
    pub context: WriteContext,
}

/// 消费一个已批准的配对（§11.6 第 8 条）。
///
/// `actor` 是本次认证的主体：只接受与该配对**已批准对端**一致的 `Actor::Node`/`Actor::Device`（存储层
/// 在同一事务内与 `owned_pairing_peer` 比对）；`context.audit` 必须携带与 `actor` 同主的审计行
/// （`node.authenticated`/`device.authenticated`，`SECURITY_DESIGN.md` §14.2）。写集还推进对端节点行的
/// `last_connected_at`（§11.6 第 9 条）：`actor` 为 `Actor::Node` 时该行是 `(actor.node, access)`。
#[derive(Debug, Clone, PartialEq)]
pub struct PairingConsumption {
    pub pairing: PairingId,
    pub actor: Actor,
    pub context: WriteContext,
}

/// 认证成功的收尾写集（§11.6 第 9 条）：`(node, kind)` 行的 `last_connected_at` 与 `context.audit`
/// （`node.authenticated` 成功行）同一事务提交；服务「认证成功但没有待消费配对」的重复认证。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeConnectedWrite {
    pub node: NodeId,
    pub kind: NodeKind,
    pub context: WriteContext,
}

/// 写入/更新一个 Export（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportWrite {
    pub record: ExportRecord,
    pub context: WriteContext,
}

/// 撤销一个 Export（§11.6）：先提交再发送 `export.revoked`，发送失败不撤销数据库决定。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRevocation {
    pub export: ExportId,
    pub context: WriteContext,
}

/// 添加一个 Import（§11.6）：管理行 + 全部关联行一次提交。
///
/// `exports` 是本次写入的关联集合，**必须**等于 `record.export_ids()`（两处不得分歧，否则存储层返回
/// `InvalidRequest`）；同一 `(owner_node_id, export_id)` 只能属于一个 Import，冲突返回
/// `PortError::Conflict(DuplicateOwnership)`。
#[derive(Debug, Clone, PartialEq)]
pub struct ImportWrite {
    pub record: ImportRecord,
    pub exports: Vec<ExportId>,
    pub context: WriteContext,
}

/// 完整移除一个 Import（§11.6）：同一事务删除管理行、关联行、`imported_session` 及其级联
/// （交付索引、命令引用），**审计保留**。提交后由组合根停止连接/重连并清空内存正文。
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRemoval {
    pub import: ImportId,
    pub context: WriteContext,
}

/// 写入一个 Agent profile（§11.6）。`put_profile` 是唯一写入默认 profile 的入口：至多一个
/// `default = true`，切换默认必须是一次调用的原子写集。
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileWrite {
    pub profile: AgentProfile,
    pub context: WriteContext,
}

/// 写入一个 workspace 记录（§11.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceWrite {
    pub record: WorkspaceRecord,
    pub context: WriteContext,
}

/// 写入一个 Provider 引用（§11.6）：只记字段名、keystore 引用与版本。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRefWrite {
    pub reference: ProviderRef,
    pub context: WriteContext,
}

/// 种子导入（§11.6）：`profiles` 与「已初始化」标记在同一事务里提交（空列表也写标记）。
#[derive(Debug, Clone, PartialEq)]
pub struct SeedWrite {
    pub profiles: Vec<AgentProfile>,
    pub context: WriteContext,
}

/// 信任存储（§5.3/§11.6）：**读**面按角色/身份材料取值，**写**面一调用一个事务一个完整写集。
#[async_trait]
pub trait TrustStore: Send + Sync {
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError>;

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError>;

    /// 按 `(NodeId, NodeKind)` 取值；同一对端可同时存在两种角色（禁止「取第一行」）。
    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError>;

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError>;

    /// 该对端的全部角色行。
    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError>;

    /// 已绑定的身份材料（验签公钥的唯一来源）。
    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError>;

    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError>;

    /// 已认领的对端行（确认事务从它读回公钥，§11.5）。
    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError>;

    /// 该对端**最近一次**配对（`design.md` D3：Node Link 握手准入手里的绑定与待消费配对来源）。
    ///
    /// 配对行本身是「登记方宣告的 `host_binding`」的唯一权威（`PairingRecord`，§11.2 第 1 条），
    /// 而节点信任行不带该列；同一对端先后可能有多条配对行，因此这里按 `created_at`、`pairing_id`
    /// 降序取一条，结果确定（同一次读取的输入决定同一次读取的输出）。没有任何配对行时返回 `None`。
    async fn pairing_for(&self, peer: &PeerIdentity) -> Result<Option<PairingRecord>, PortError>;

    async fn put_device(&self, write: DeviceWrite) -> Result<(), PortError>;

    async fn put_node(&self, write: NodeWrite) -> Result<(), PortError>;

    async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError>;

    async fn revoke_node(&self, write: NodeRevocation) -> Result<(), PortError>;

    async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError>;

    async fn claim_pairing(
        &self,
        write: PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError>;

    async fn settle_pairing(
        &self,
        write: PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError>;

    async fn expire_pairings(&self, write: ExpiryWrite) -> Result<u64, PortError>;

    /// 消费一个已批准的配对（§11.6 第 8 条）：单事务把状态推进到 `consumed` 并写 `terminal_at`、追加
    /// `context.audit`；已是 `consumed` 且 `actor` 与对端一致时幂等成功（不覆盖首次 `terminal_at`，也
    /// 不重复写审计）。写集还推进对端节点行的 `last_connected_at`（见 [`PairingConsumption`]）。
    async fn consume_pairing(&self, write: PairingConsumption) -> Result<PairingRecord, PortError>;

    /// 认证成功的收尾写集（§11.6 第 9 条）：单事务把 `(node, kind)` 行的 `last_connected_at` 推进到
    /// `context.at`（只前进不倒退、不抹掉已存值）并追加 `context.audit`；该行不存在 →
    /// `NotFound(EntityRef::Node)`。
    async fn record_node_connected(&self, write: NodeConnectedWrite) -> Result<(), PortError>;
}

/// Export/Import 存储（§5.3/§11.6）：读取面不变，写入面全部走写集。
#[async_trait]
pub trait ExportStore: Send + Sync {
    async fn export(&self, id: &ExportId) -> Result<Option<ExportRecord>, PortError>;

    async fn exports(&self) -> Result<Vec<ExportRecord>, PortError>;

    async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError>;

    async fn imports(&self) -> Result<Vec<ImportRecord>, PortError>;

    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError>;

    async fn revoke_export(&self, write: ExportRevocation) -> Result<(), PortError>;

    async fn add_import(&self, write: ImportWrite) -> Result<(), PortError>;

    /// 完整移除（§11.6）：管理行 + 关联行 + 交付索引 + 命令引用；审计保留。
    ///
    /// 与 `RemoteDeliveryStore::drop_import`（连接级清空交付索引）不是同一件事，两者不得串联充当完整删除。
    async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError>;
}

/// 本地配置存储（§11.6）：profile、workspace、Provider 引用与首次初始化标记。
#[async_trait]
pub trait LocalConfigStore: Send + Sync {
    async fn profiles(&self) -> Result<Vec<AgentProfile>, PortError>;

    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError>;

    async fn put_profile(&self, write: ProfileWrite) -> Result<(), PortError>;

    async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, PortError>;

    async fn workspace(&self, alias: &WorkspaceAlias)
    -> Result<Option<WorkspaceRecord>, PortError>;

    async fn put_workspace(&self, write: WorkspaceWrite) -> Result<(), PortError>;

    async fn provider_refs(&self) -> Result<Vec<ProviderRef>, PortError>;

    async fn put_provider_ref(&self, write: ProviderRefWrite) -> Result<(), PortError>;

    /// 首次初始化标记：`seeded = false` 时启动流程才能导入种子。
    async fn seed_state(&self) -> Result<SeedState, PortError>;

    /// 种子导入与「已初始化」标记同一事务提交（空列表也写标记）。
    async fn mark_seeded(&self, write: SeedWrite) -> Result<(), PortError>;
}

/// 凭据解析（§11.6）：把 profile 的凭据绑定解析成子进程环境变量。
///
/// 由组合根用平台 keystore 实现并注入 `agent-host`（`agent-host` 不依赖 `identity-auth`）。
#[async_trait]
pub trait CredentialResolver: Send + Sync {
    /// 解析启动子进程所需的全部环境变量。只允许解析 profile 的 `env` 绑定与 `env_allowlist` 的交集；
    /// 未绑定、未列入白名单或引用失效（keystore 不可用 / 字段不存在）→
    /// `Unavailable(KeystoreUnavailable)`，**失败关闭**：不得静默跳过该变量后继续启动。
    async fn resolve_env(
        &self,
        profile: &AgentProfile,
    ) -> Result<Vec<(String, SecretValue)>, PortError>;
}

/// 审计查询条件；`actions` 为空 = 不按动作过滤。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditQuery {
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub actions: Vec<AuditAction>,
    pub actor: Option<Actor>,
    pub target: Option<EntityRef>,
    /// 返回行数上限；`None` = 不限。
    pub limit: Option<u32>,
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append(&self, record: AuditRecord) -> Result<(), PortError>;

    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditRecord>, PortError>;

    /// 管理写集的变更水位（`NODE_LINK_PROTOCOL.md` §12.3 的 `catalogRevision`）：只增不减的审计自增序号，
    /// 不得随清理回退（Export/信任写集各追加一行审计，因此它正是这些写集的变更点）。
    async fn watermark(&self) -> Result<u64, PortError>;
}

/// 内容寻址附件的引用。`id` 由存储层分配（`AttachmentStore::put` 不接受 id 参数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRef {
    pub id: AttachmentId,
    pub sha256: Digest,
    pub byte_length: u64,
    pub media_type: String,
    pub relative_path: String,
}

#[async_trait]
pub trait AttachmentStore: Send + Sync {
    /// 写入内容寻址的附件字节，返回其 sha256 与相对路径。
    async fn put(
        &self,
        bytes: &[u8],
        media_type: &str,
        at: Timestamp,
    ) -> Result<AttachmentRef, PortError>;

    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError>;

    async fn link(
        &self,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
    ) -> Result<(), PortError>;

    /// 按 LRU 清理到给定字节预算以下；返回被删除的附件。
    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError>;

    /// §7.5 的孤儿回收（§6 第 18 条）：删除 `storage.attachment_dir` 下**不在 `owned_attachment` 表里**
    /// 且 `mtime` 早于 `at` 的文件，每次最多 `limit` 个，返回实际删除数（`limit = 0` 不删）。
    ///
    /// 一律在事务之外运行（文件删除只在行事务提交之后）；失败不阻止启动。
    async fn sweep_orphans(&self, at: Timestamp, limit: u32) -> Result<u32, PortError>;
}
```

约束：

- `[决定]` 凭据（Provider、Node key、pairing secret）**不经**任何上述端口：只存平台 keystore，表里最多存引用与指纹（`SECURITY_DESIGN.md` §13.1）。
- `[决定]` `AuditRecord.action` 是 §3.5 的闭合枚举；审计行不得包含内容（`SECURITY_DESIGN.md` §14.2），并由 §9.13 的机器检查兜底。
- `[决定]` `put_export` 的**撤销终态**（§11.1 的「撤销记录保留」、§11.2 第 4 条）：传入记录 `revoked_at` 非空 → `InvalidRequest`（撤销只能经 `revoke_export`，与 `put_device` 拒绝 `revoked` 记录同款）；库内该 `export_id` 已撤销而传入记录未撤销 → `Conflict(AlreadyExists)`（失败关闭，不得静默保留后返回成功）；`DO UPDATE` 的该列取「已存非空值优先」，重复写入不清除首次撤销时间。`put_export` **不是** `export.update`（`LOCAL_ADMIN_PROTOCOL.md` §5.5 只登记 `export.create`/`export.list`/`export.revoke`）。
- `[决定]` **身份材料读取必须核对同行指纹**（§3.5、§9 判据 25/30）：`owned_peer_key`、`owned_pairing_peer` 与 `owned_device`（含设备记录的**单读与列表读**）的读取路径都用 `PeerPublicKey::fingerprint()` 比对同一行的 `fingerprint` 列，不一致 → `PortError::Corrupt` 且**不返回该材料**。DDL 只有列级长度/字符集 CHECK，跨列一致性只能在读取期判定（SQL 算不了 SHA-256）。
- `[决定]` `owned_device.last_seen_at` 与 `owned_node.last_connected_at` **只前进**（§9 判据 30）：旧值为空时写入新值、新值为空时保留旧值、两者非空取较大者，比较按固定宽度 UTC 毫秒文本的字典序（§3.2）；MUST 用显式 `CASE`，不得用标量 `max(a, b)`（任一参数为 NULL 时返回 NULL，会把「旧值为空时的首次写入」丢成 NULL）。

### 5.4 发布与基础设施

```rust
/// `Arc<dyn Fn(EndpointEvent) + Send + Sync>` 的包装类型：值语义 + `Clone`，因此
/// `SessionBackendFactory::create`/`open` 可按值接收并交给后端持有；`send` 是它的固有方法，
/// 调用顺序即提交顺序（§6 第 1/3 条）。**不是** trait——§5.1 的 `[决定]` 与实现一致。
#[derive(Clone)]
pub struct EventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);
pub trait EventPublisher: Send + Sync { fn publish(&self, delivery: CommittedDelivery); }
pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }

pub trait IdGenerator: Send + Sync {
    fn turn_id(&self) -> TurnId;
    fn message_id(&self) -> MessageId;
    fn interaction_id(&self) -> InteractionId;
    fn pairing_id(&self) -> PairingId;
    fn origin_epoch(&self) -> OriginEpoch;
    fn request_id(&self) -> RequestId;
}
```

- `[决定]` `event_id`：由存储层在提交事务内分配（`SessionStore::commit`），**不**由 `IdGenerator` 提供，避免出现“先生成后提交”导致的两个来源。
- `[决定]` `IdGenerator` **不提供** `session_id()`/`event_id()`（由存储层在事务内分配，§3.1）与 `attachment_id()`（由 `AttachmentStore::put` 分配并返回，§7.3）：同一个 id 只能有一个来源。
- `[决定]` `publish` 不返回结果，且不得回滚已提交事务；订阅注册与背压属于 `server`（它持有注册表与有界队列），`app` 把同一个 hub 交给 core 与 server（`MODULE_ARCHITECTURE.md` §6）。

## 6. Broker 的顺序与事务契约

1. owned：`EventSink` → broker 组装 `OwnedCommit` → `SessionStore::commit` → `EventPublisher::publish`（顺序不可交换）。
2. imported：`node-link-client` → broker 组装 `DeliveryReceipt` → `RemoteDeliveryStore::commit_receipt` → 发布（正文只在内存）。
3. `[决定]` 每会话串行：一个会话一个 actor，命令/后端事件/终态在同一串行队列内处理；不同会话并行。
4. `[决定]` active turn：同一会话最多一个非终态 turn。`sessions.queue_policy = queue` 时按持久化接受顺序排队，超过 `sessions.max_queued_turns`（16）返回 `session.busy`；`= reject_busy` 时直接返回 `session.busy`（`CONFIG_REFERENCE.md` §4）。
5. `[决定]` 授权调用点：所有用例入口先按 `actor` 的 scope/grant 与 Export 交集判定；core 不信任 adapter 的“已授权”标志。`Actor::Node` 的判定分两侧：**Access 侧**（本节点是对方的客户端）要求本地 `ImportRecord.grants` 覆盖该命令；**Owner 侧**（本节点是导出方）要求该对端的 `access` 信任行已配对且 `grants` 含该命令所需的 grant（`required_grant`），**并且**存在一个未撤销的 `ExportRecord`——它的 `scopes` 含该 grant、与本节点信任记录的 grants 有交集（`export.scopes ∩ node.grants ≠ ∅`，与 Node Link 的可见性口径同源）、且会话命令还需覆盖目标会话的 agent。**无会话命令**（`session.list`/`command.status`/`session.create`）没有目标会话可比对 agent，因此只需前两条（信任 grant + 存在满足条件的 Export）；它的结果过滤（`session.list` 只列该节点可见 Export 内的会话）与参数校验（`session.create` 的 `agentId`/`exportId`/`workspaceAlias`）仍由 `server::node_link` 按其单点可见性策略完成，本层只判授权。为使该判定成立，`BrokerDeps` 必须注入 `TrustStore`（授权不能只靠 Export 记录）；`session.create` 的 `CreateSessionRequest` 不携带 `exportId`（Export 归属由适配层在调用前校验），因此本层对它的 Owner 侧判定按上述“存在满足条件的 Export”执行。
6. `[决定]` 幂等键是**协议维度**的 `(actor, requestId)`：Sync 侧 actor 由 `deviceId` 决定（`SYNC_PROTOCOL.md` §11.1），Node Link 侧由 `(ownerNodeId, accessNodeId)` 决定（`NODE_LINK_PROTOCOL.md` §12.5）。重复提交时必须比对 `command`/`kind`/`session`/`expected_version`/`request_fingerprint`：任一不同 → `command.idempotency_conflict`；全同 → 返回首次结果（`CommitOutcome::replayed`），**不得**二次派发。
7. `[决定]` 权限仲裁：`resolve_interaction` 的 first-writer-wins；`interaction.already_resolved` 之后到达的应答必须被拒绝且不覆盖既有结果。
8. `[决定]` 模型/配置切换只在 turn 边界生效；非终态 turn 期间的 `set_mode`/`set_config` 排队到边界或返回 `state.version_conflict`。
9. `[决定]` 磁盘写失败：`commit` 返回 `PortError::Unavailable` → 命令显式失败或 `uncertain`；**不得**发布对应事件（`SECURITY_DESIGN.md` §15）。
10. `[决定]` `storage.flush_interval_ms = 250` 允许把同一会话短窗口内的 delta 合并为一次 `commit`；合并不得改变顺序、不得跨 turn 边界、不得延迟终态事件。
11. `[决定]` `OwnedCommit.events` 只接受 `StoredPolicy`；`Ephemeral` 由 broker 在组装前过滤（内存转发），`commit` 收到 `Ephemeral` 视为 `InvalidRequest`（类型上已不可表达）。
12. `[决定]` 快照与重放必须来自 `SessionStore::read_view()` 返回的同一读视图，`sync.snapshot_begin`/`sync.snapshot_end` 与之后的事件游标都以该视图的 `head()` 为 barrier（`SYNC_PROTOCOL.md` §9.3/§9.4）。
13. `[决定]` **交互（权限 / elicitation）的创建与解析**：
    - 创建：Agent 的请求由 broker 组装成一条 `kind = interaction` 的事件**加上** `OwnedCommit.interactions` 里的 `PendingInteractionWrite`，**同一次 `commit`**（§9 判据 15）。写入形状**不带** `request_event`（`event_id` 由存储层分配、装配方预知不了）：存储层在同一提交里找 `kind = 'interaction'` 且 payload 的 `interactionId` 等于 `interaction.id()` 的那条事件，把该事件的 `global_sequence` 写进 `owned_interaction.request_event`（列类型是 `INTEGER REFERENCES owned_event(global_sequence)`，不是 `event_id`）；配不到（事件缺失或被 Ephemeral 过滤）→ 整事务 `InvalidRequest`，**不得**写入悬空引用。
    - 解析：走 `OwnedCommit.state.interaction`（`InteractionResolved { interaction, resolution, resolved_by }`）——**不是** `SessionStore` 的独立方法，因此「行更新 + `*.resolved` 事件 + 命令终态」仍在 `commit` 的单次事务边界内。`SessionEndpoint::resolve_interaction` 只负责把决定交给后端。
    - 仲裁顺序（不可交换）：先条件更新落盘仲裁 → 再派发后端 → 最后 flush 后端发出的 `*.resolved` 事件。受影响行数为 0 时存储层在同一事务内回读：行存在且 `state <> 'pending'` → `PortError::Conflict(AlreadyResolved)`；无行 → `PortError::NotFound`。
    - 崩溃窗口：上面三步之间崩溃会留下「交互行已终态、`*.resolved` 事件未落盘」。重试同一条命令返回 `AlreadyResolved` 且**不**二次派发；UI 以交互行（`HistoryInclude.pending_interactions`）恢复，不依赖事件流补齐。该窗口是本轮接受的取舍（消除它需要「解析意向」行或两阶段提交，见 §10）。
    - `options` **不落库**（`owned_interaction` 无该列）：读视图里 `PendingInteraction.options` 恒为空。还原路径是事件流（`replay`/`read_session` 返回的 `CommittedEvent` 带 `id`），再用 `ReadView::event_payload(id)` 取那条 `interaction` 事件的正文并解析候选项（§5.2）；不要拿 `request_event`（它是 `global_sequence`）直接当 `event_payload` 的参数。
14. `[决定]` **助手消息的收尾正文**（`SYNC_PROTOCOL.md` §10.2 的 `agent.message.completed`）：turn 进入终态（`completed`/`cancelled`/`failed`）时，broker 必须为**该 turn 内出现过 `agent.message.delta` 的每个 `messageId`** 生成恰好一条 `agent.message.completed`，并与该 turn 的终态事件在**同一次 `commit`** 内提交（因此它对重放与快照恒可见）。规则：
    - `content` 由该消息的 delta **按 `deltaIndex` 升序**折叠而成：连续的文本 delta 合并为**一个** `{type:"text", text}` 块（文本按序拼接）；带 `block` 字段的 delta（非文本内容，由适配器投影，见 `SYNC_PROTOCOL.md` §10.2）按顺序插入对应块。
    - 正文**只来自 delta 的 view**：broker 不解析 ACP 原文（core 不依赖 ACP DTO，§2），非文本内容的投影是**生产端（适配器）**的义务；`block` 缺失时该 delta 只贡献文本，**不得**由 broker 猜测类型。
    - `deltaIndex` 有空洞不构成错误、不触发补写；顺序一律以现存的 `deltaIndex` 升序为准。
    - `agent.thought.delta` 的流**不**产生 `completed`：思考是短保留期内容，其保真只存在于 delta 事件与它们的 `acp` 原文里（这是**显式**的短保留期降级，不违反 `SYNC_PROTOCOL.md` §10.1 的「不得静默丢弃」——它是登记在案的语义，不是丢弃）。
    - `MessageId` 由生产端（适配器）用 `IdGenerator::message_id()` 在**该消息第一条 delta 提交前**分配，该消息的所有 delta 与其 `completed` 复用同一个 id。
15. `[决定]` **delta 压缩**（`storage.persist_deltas = false` 时的收尾动作）：turn 终态提交之后，broker 可以对**该 turn 已终结**的 `kind='delta'` 事件做一次压缩——在下一次 `commit` 里提交一条 `kind='summary'` 的 `turn.delta_compacted` 事件，并用 `OwnedCommit.compacted` 列出被它替代的 `global_sequence`；存储层把这些行的 `compacted_into` 置为该 summary 行的 `global_sequence`。规则：
    - **执行中不得压缩**（turn 未终态就不写 `compacted_into`）；`storage.persist_deltas = true` 时永不压缩。
    - 压缩是**元数据替换**：summary 的 view 只承载收据（`turnId`、`deltaCount`），**不复制正文**；终态正文来自第 14 条的 `agent.message.completed`。**不带** delta 摘要：「是否丢过 delta」由 §9 判据 4 的会话内 `session_sequence` 稠密性判定，而 ACPR-CJ1 摘要需要 `acpr-wire`，core 的依赖闭包不允许它（§2、§9 判据 13）。
    - `compacted` 里的每个 cursor 必须属于本提交的会话、对应行必须是 `kind='delta'` 且 `compacted_into IS NULL`；任一不满足 → 整事务 `InvalidRequest`（存储层校验，失败关闭）。
    - 重放里 summary 事件**替代**被压的 delta：客户端据此知道该段历史已压缩；「是否丢过 delta」由会话内 `session_sequence` 的稠密性判定（§9 判据 4），不由 summary 承载。
16. `[决定]` **启动恢复**（`SYNC_PROTOCOL.md` §11：「Daemon 恢复时发现外部 Agent 副作用无法确认，必须先持久化 `command.uncertain`，再向客户端广播」）：组合根在取得单实例锁、开始监听**之前**，必须以 `LocalCli` actor 调用 `RecoverUnsettled` 用例；它对 `SessionStore::unsettled_commands` 返回的每条 `accepted` 命令，在该会话的串行门内终结为 `uncertain`——写终态事件 `command.uncertain { requestId, reason, mayHaveReachedAgent: true }` 并更新幂等行（`status`/`terminal_event_id`），同时把该命令对应的未终态 turn 终结为 `failed`（`PublicError { code: "command.uncertain" }`）。**不得**自动重放副作用，也不得让 `accepted` 行静默存活。`uncertain` 行不参与 TTL 清理（§7.5）。
    - 同一趟恢复还要补写**缺失的收尾事件**：若某个 turn 已有已提交的 `agent.message.delta` 却没有 `agent.message.completed`（进程在终态提交前崩溃），必须用 `ReadView::event_payload` 从库里按第 14 条的规则重建该事件并在**同一次恢复提交**内落盘，否则那段已经广播出去的文本永远没有终态记录。
17. `[决定]` **`session.mode.list` 的应答**：结果形状是 `ModeState { currentModeId, availableModes: ModeRef[], version }`（`SYNC_PROTOCOL.md` §11.5/§10.2）；`availableModes` **只能**来自 `SessionEndpoint::modes()`（§5.1），`version` 取该会话当前版本。core 不得凭 `current_mode` 编造候选列表，端口返回空列表时结果就是空列表（不伪造）。
18. `[决定]` **附件文件与行的事务边界**：行的删除与它所在的事务一起提交，**文件删除一律在提交之后**（崩溃只会留下无人引用的孤儿文件，绝不会留下悬空行）；`AttachmentStore::prune_lru` 与 `sweep_orphans` 因此都在事务之外运行。
    - **孤儿回收**由组合根在启动时调用一次 `sweep_orphans(启动时刻, 1000)`（紧跟 §6 第 16 条的恢复之后）：只删「不在 `owned_attachment` 里**且** `mtime` 早于本次进程启动时刻」的文件——第二条规则保护正在写入、行还没提交的新附件。回收失败**不阻止启动**，记一次结构化警告，剩余孤儿留到下次启动。
19. `[决定]` **提交前的 view 收口（`SYNC_PROTOCOL.md` §10.3 的身份与会话版本）**：owned 提交在 `commit_owned` 漏斗内、调用 `SessionStore::commit` **之前**完成两件事，因此落盘 view、重放 view 与广播所依据的 payload 同源：
    - **turn 归属**：事件类型属于 §10.3 要求 `turnId` 的集合（`turn.*`、`user.message.delta`、`agent.message.delta`、`agent.message.completed`、`agent.thought.delta`、`tool.call.started`/`updated`/`completed`、`permission.requested`、`elicitation.requested`）且该事件已被归属到某个 turn 时，view 顶层必须有 `turnId`，取值等于 core 已定稿的权威 turn；**不得**向其它事件类型添加未协商字段，无归属的事件不得出现该字段。适配器已给出同名字段时：取值一致 → 保留原字节；取值不同或值不是字符串 → 显式 `InvalidRequest`、不写任何行、不发布任何帧。归属只在批组装时定稿一次，注入是它的唯一消费者（不重新推导）。
    - **会话版本**：事件类型属于 §10.3 要求 `version` 的集合（`session.mode.changed`、`session.config.changed`）时，view 顶层必须有十进制字符串 `version`，取值等于该次提交后的会话版本。推导规则与存储层一致：含 `StateChange` 的提交为当前版本 + 1，否则不变；提交后必须与 `CommitOutcome.version` 比对，不一致 → `PortError::Corrupt`、不发布该批、不得报告成功（比对发生在存储返回之后，已落盘的行不由 core 撤销）。幂等命中（`replayed`）时不比对：返回的是首次提交的结果，第二次提交的 view 不得被重写。imported 路径**不**注入这两个字段（`turnId`/`version` 由拥有该会话的节点注入，`payloadDigest` 覆盖 Owner 给出的视图字节），只保留其取值。
    - **两个已登记的边界**：① 失败关闭（`turnId` 冲突或版本漂移）发生在 `flush` 组装之后，该批适配器事件**不再重投**（调用方按本条 ① 的失败语义——与 §6 第 9 条同口径——决定是否把 turn 判为失败），不得重试时假装该批从未到达；② 无状态变更的提交里存储层**不**校验 `expected_version`（§5.2 只对 `Update` 校验），因此 core 的推导/比对就是该组合的失败关闭点，且可能发生在落盘之后。
    - **无归属的降级**：turn 终结后晚到的、类型属于 §10.3 `turnId` 集合的事件（适配器异步尾巴）没有权威 turn，**不**注入（`owned_event.turn_id` 与 view 同时为 NULL），宁可缺字段也不伪造；该降级必须有用例固定，并留给 Sync 切片裁定是否拒绝。
20. `[决定]` **`session.create` 的幂等与终态落盘**（`node-link-owner` 的 WP6 修复轮次 RV1-WP6-F1）：创建走**两次提交**，幂等键是第 6 条的 `(actor, requestId)`（Owner 侧 `actor` 是该 `access` 对端，`kind = mutation`，`expected_version = None`，`command = "session.create"`）：
    - **创建提交**：`StateChange::Create` 与幂等行在**同一事务**里落盘。幂等行的 `session = None`（装配期尚无目标会话），`session_id` 由存储层在事务内分配后**回填该列**——终态提交与第 16 条的启动恢复都按 `(session_id, request_id)` 定位该行，回填前那两处都定位不到它。`request_fingerprint` 取 ACPR-CJ1 之后的解码 payload 摘要，由适配层计算并作为参数传入（core 不依赖 `acpr-wire`）。
    - **幂等比对里的 `session` 分量**：`IdempotencyRecord.session = None` 的语义是「装配期无目标会话」（目前只有 `session.create`），存储层**不**用行里存储层的创建结果与它比对；`command`/`kind`/`expected_version`/`request_fingerprint` 四项仍然恒比（第 6 条）。命中且四项相同 → 返回行里记的首次 `sessionId`（`CommitOutcome.replayed`，不创建第二个会话、不开第二个后端端点）；任一不同 → `PortError::Conflict(IdempotencyConflict)`。
    - **终态提交**：一条 `command.completed`/`command.failed`/`command.uncertain` 事件（`causation = requestId`）+ `CommandTerminalRecord`。`completed` 的 `result` 是适配层投影的结果原文（Node Link 的 `SessionCreateResult`）、`completed` 必须有 `terminal_event_id`（§7.3 的 CHECK）；`failed`/`uncertain` 携带结构化错误。没有持久记录（创建在幂等行落盘前就失败）或记录已终结时，终态提交是**幂等 no-op**，不覆盖首次结果。
    - **落盘前失败不构成持久首次结果**（`node-link-owner` 的 WP6 修复轮次 RV2-WP6-F1）：幂等行落盘前失败（如存储写失败）的 `session.create` 没有持久记录，因此同 `requestId` 重查回 `nodelink.command.not_found`，且重试可以创建出另一个会话、得到与首次尝试不同的结果——`failed` 终态只适用于适配层在同一 `requestId` 上能复现的确定类失败（授权拒绝、本机 workspace 解析失败），不得把落盘失败也描述成「重试结果确定」。
    - **`settle_session_create` 只终结 `session.create` 的记录**（`node-link-owner` 的 WP6 修复轮次 RV2-WP6-F2）：该 `(actor, requestId)` 的持久记录 `command != "session.create"` 时返回 `InvalidRequest` 且零写入（适配层误用，wire 不可达），不得把别的命令的幂等行改写成创建的终态。
    - **崩溃窗口**：两次提交之间崩溃留下 `accepted` 行 + 已创建的会话；第 16 条的启动恢复把它终结为 `uncertain`（`command.uncertain` 事件 + `terminal_event_id`），**不**重放副作用、也不猜测创建是否成功。该行不是无会话命令：`owned_command.session_id` 已回填，恢复走「有会话」分支。

## 7. `storage-sqlite` v3 表结构

### 7.1 文件、PRAGMA、连接与权限

- 单文件 `<data_dir>/acp-remote.sqlite3`；附件目录 `<data_dir>/attachments`（`[决定]` 新增配置键 `storage.attachment_dir`，默认 `<data_dir>/attachments`，语义与权限同数据库目录）。
- `[决定]` 启动设置：`journal_mode = WAL`、`synchronous = FULL`、`foreign_keys = ON`、`busy_timeout = 5000`、`wal_autocheckpoint = 1000`。
- `[决定]` 连接模型：1 个写连接（串行化写事务）＋ N 个只读连接；`read_view()` 打开一个显式只读事务（`BEGIN`）供快照使用。
- `[决定]` 权限：Unix 目录 `0700`、数据库/WAL/`-shm`/附件 `0600`；启动时检查，正式模式失败关闭（`SECURITY_DESIGN.md` §13.2）。
- `[决定]` **Windows**：ACL 读取不可用（workspace 禁 `unsafe`，也没有 ACL 封装依赖），判定返回 `Unverifiable` 且**不**失败关闭——只记录一次结构化警告；数据目录默认位于用户 profile 下并继承用户专属 ACL。Unix 仍按真实模式位判定并失败关闭（§9 判据 12 的合成视图单测在两平台都跑）。
- `[决定]` 启动 `PRAGMA quick_check`；失败进入只读失败关闭（写路径返回 `PortError::Corrupt`）。
- `[决定]` WAL 检查点与备份：关闭时执行一次 `wal_checkpoint(TRUNCATE)`；「直接拷贝主库文件而不带 `-wal`/`-shm` 不安全」的运维提醒落在 `CONFIG_REFERENCE.md` §5 的 storage 段，`SECURITY_DESIGN.md` §13.2 用一行链接指向它（两份文档各自只保留一处正文）。

### 7.2 Migration

- `[决定]` `PRAGMA user_version` = 文件格式版本（当前 **v3 = 3**）；`meta` 保存两族 schema 版本：`owned_schema_version`、`imported_schema_version`（当前都为 3）。三个常量是 `crates/storage-sqlite/src/migrate.rs` 的 `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION`。
- `[决定]` 两族 migration 分开维护；单事务、可重复执行、失败整体回滚；文件格式版本**或**任一表结构版本高于本二进制已知版本 → 拒绝启动，不降级写入。
- `[决定]` **升级判据**：库内已有 schema（`owned_session` 存在）且 `user_version < 3` 时，在同一 `BEGIN IMMEDIATE` 事务内按版本执行对应的升级段（v1 库走 v2 段再走 v3 段，v2 库只走 v3 段）；`user_version = 3` 的库**跳过**全部升级步骤，因此第二次打开不重写 `sqlite_master`、不写任何行（§9.1）；空目录新建的库直接由 §7.3/§7.4 的 DDL 建成 v3 形状（此时 `user_version` 是 0，不能只按版本号判断）。
- `[决定]` v1 → v2 升级步骤（顺序固定，都在同一事务内）：
  1. 执行 §7.3/§7.4 的 DDL 常量：`CREATE ... IF NOT EXISTS` 建出新增的管理表与 `imported_import_export`，既有表不动；
  2. `owned_audit` 与 `imported_audit` 走 **12-step 表重建**（SQLite 不能修改既有 CHECK）：新建带完整 `action` CHECK 的表 → 按列拷贝**全部行（含 `audit_id`）** → `DROP` 旧表 → `RENAME` → 重建索引；之后按升级前的 `sqlite_sequence` 回填序列，**AUTOINCREMENT 不得回退**（审计有 365 天 TTL，尾部行被清理后 `seq` 会领先于 `max(audit_id)`）；
  3. `imported_import` 重建：去掉 `export_id` 与 `UNIQUE (owner_node_id, export_id)`，新增 `grants_json`；原行的 `(owner_node_id, export_id)` 与 `created_at` 迁为一条 `imported_import_export` 关联行（`added_at` = 原 `created_at`）。旧行**没有可信的 grants 来源**，因此 `grants_json` 一律写 `'[]'`——**不得凭空补齐或默认放权**，这类 Import 保持不可用，等本地重新授权；
  4. 本段**不单独落盘版本**：`meta.owned_schema_version`/`meta.imported_schema_version` 与 `PRAGMA user_version` 都由同一事务内紧随其后的 v2 → v3 段在**全部**表重建结束后统一写为 `'3'`/`3`（见下一条），因此不存在「已写 v2 版本号、表仍是 v1 形状」的中间落盘。
- `[决定]` **v2 → v3 升级步骤**（与 v2 段在同一事务内、顺序在后）：两张审计表同样走 12-step 表重建，**列集合与列顺序逐字不变**，只扩宽 CHECK——`actor_kind` 增 `'pairing_claimant'`（配对认领方的审计归因）、`action` 增 `'node.authenticated'`/`'node.auth_failed'`（节点握手留痕）；之后按本次升级前的 `sqlite_sequence` 回填序列（两段重建各自 `DROP` 过审计表，因此序列只在**全部**重建结束后回填一次），并置两个 schema 版本为 `'3'`、`PRAGMA user_version = 3`。`owned_command` **不重建**：认领方永不提交命令，它的 `actor_kind` CHECK 保持 `('device','node','cli')`（design D12）——因此 v3 库上两张审计表接受四值、`owned_command` 只接受三值。
- `[决定]` 保留不变量：`server_epoch`、会话 `origin_epoch` 与事件 `global_sequence`/`session_sequence`、`requestId` 与幂等行、命令终态、全部既有审计都逐行保留，**不得重新编号**。管理表初始为空；profile 种子与「已初始化」标记在同一事务里提交（`LocalConfigStore::mark_seeded`，§5.3），不从聊天或审计内容推断信任。
- `[决定]` 管理表纳入 §7.5 的容量度量（TEXT 列 + 附件字节）与清理顺序；撤销 tombstone 不因容量压力被删除，空间不足时拒绝新写入而不是删活动信任或未到期审计。
- `[决定]` 迁移测试资产：`fixtures/storage/v2/` 三件套在 v3 之后是**冻结的历史升级输入**——`empty.sqlite3`（v2 形状的空库，v2 → v3 用例的输入）、`from-v1.sqlite3`（含会话/事件/cursor/幂等/审计数据的 v1 库，`owned_audit` 故意留下 `audit_id = 1,2,5` 的空洞以覆盖「序列领先于 `max(audit_id)`」；v1 → v2 → v3 连续升级用例的输入）与 `too-new.sqlite3`（`user_version = 3`，v3 之后不再「过新」，保留为历史资产）；`fixtures/storage/v1/` 的两个文件是更早的历史资产。当前二进制不再能生成 v2 形状的空库，因此 `from-v1.sqlite3` 的生成器（`crates/storage-sqlite/tests/migration.rs` 的 `regenerate_v1_fixture`，默认 `#[ignore]`）只重建它；「版本过新拒绝启动」用例改在临时副本上把 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`；「已是最新版则不重写」用例改在空目录新建的当前版本库上判定。
- `[决定]` 回滚：v3 库不能被旧二进制打开（版本过新拒绝启动），因此回滚 = 恢复升级前的数据库备份 + 回退二进制；本合同**不提供**自动降级迁移。

### 7.3 `owned_*` 表

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
-- 必需键：server_epoch、owned_schema_version、imported_schema_version、created_at、last_prune_at

CREATE TABLE owned_session (
  session_id        TEXT PRIMARY KEY,
  title             TEXT,
  agent_id          TEXT NOT NULL,
  agent_name        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('idle','queued','running','waiting_input','waiting_permission','failed','closed')),
  origin_epoch      TEXT NOT NULL,
  current_mode_id   TEXT,
  current_mode_name TEXT,
  version           INTEGER NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  closed_at         TEXT
) STRICT;

CREATE TABLE owned_turn (
  turn_id     TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  state       TEXT NOT NULL CHECK (state IN ('queued','running','waiting_input','waiting_permission','completed','failed','cancelled')),
  queue_index INTEGER NOT NULL,
  causation   TEXT,
  started_at  TEXT,
  ended_at    TEXT,
  UNIQUE (session_id, queue_index)
) STRICT;

CREATE TABLE owned_event (
  global_sequence  INTEGER PRIMARY KEY AUTOINCREMENT,   -- 库内严格递增且唯一；非会话级事件也有值
  session_id       TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,  -- 非会话级事件为 NULL
  session_sequence INTEGER,                             -- 非会话级事件为 NULL
  origin_epoch     TEXT,                                -- 非会话级事件为 NULL
  origin_sequence  INTEGER,                             -- 非会话级事件为 NULL
  event_id         TEXT NOT NULL UNIQUE,
  turn_id          TEXT REFERENCES owned_turn(turn_id),
  event_type       TEXT NOT NULL,
  kind             TEXT NOT NULL CHECK (kind IN ('state','delta','final_message','structured','interaction','summary')),
  policy           TEXT NOT NULL CHECK (policy IN ('durable','short_term')),
  origin_kind      TEXT NOT NULL CHECK (origin_kind IN ('agent','device','daemon','local_cli')),
  causation        TEXT,
  payload_json     TEXT NOT NULL,
  payload_digest   TEXT NOT NULL,
  acp_media_type   TEXT,
  acp_raw_json     TEXT,
  acp_byte_length  INTEGER,
  acp_sha256       TEXT,
  acp_raw_unavailable_reason TEXT CHECK (acp_raw_unavailable_reason IN ('size_limit','retention_expired','storage_failure')),
  created_at       TEXT NOT NULL,
  expires_at       TEXT,
  compacted_into   INTEGER,
  CHECK (session_id IS NOT NULL OR session_sequence IS NULL),
  CHECK ((session_id IS NULL) = (origin_epoch IS NULL)),   -- 非会话级事件：origin 两列必须与 session_id 同时为空/同时非空
  CHECK ((session_id IS NULL) = (origin_sequence IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NOT NULL          -- 原文与摘要同有同无；**不可用原因行除外**——
         OR (acp_raw_json IS NULL) = (acp_sha256 IS NULL)),   -- 「摘要算得出来但原文已清」是模型的合法状态（`AcpRaw::Unavailable { sha256: Some(_) }`），必须可入库
  CHECK (acp_raw_unavailable_reason IS NULL OR acp_raw_json IS NULL),
  CHECK (acp_raw_json IS NOT NULL OR acp_raw_unavailable_reason IS NOT NULL OR acp_sha256 IS NULL)
) STRICT;
CREATE UNIQUE INDEX owned_event_session ON owned_event(session_id, session_sequence);   -- 会话内序号稠密且唯一（§9 判据 4）
CREATE UNIQUE INDEX owned_event_origin ON owned_event(session_id, origin_epoch, origin_sequence) WHERE session_id IS NOT NULL;
CREATE INDEX owned_event_expiry ON owned_event(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX owned_event_turn ON owned_event(session_id, turn_id, kind);

CREATE TABLE owned_command (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_kind    TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id      TEXT NOT NULL,
  request_id    TEXT NOT NULL,
  session_id    TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,   -- 非会话命令为 NULL
  command       TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('query','mutation')),
  expected_version TEXT,
  request_fingerprint TEXT NOT NULL,
  accepted_at   TEXT,
  status        TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  terminal_at   TEXT,
  terminal_event_id TEXT,
  result_json   TEXT,
  error_code    TEXT, error_message TEXT, error_details_json TEXT, retryable INTEGER,
  UNIQUE (actor_kind, actor_id, request_id),                -- 协议维度的幂等键
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  CHECK (status <> 'rejected' OR result_json IS NULL),
  CHECK (status <> 'completed' OR error_code IS NULL),
  CHECK (status IN ('accepted','completed') OR error_code IS NOT NULL),
  CHECK (kind <> 'query' OR terminal_event_id IS NULL),      -- 查询命令不得带 terminalEventId
  CHECK (status <> 'completed' OR kind <> 'mutation' OR terminal_event_id IS NOT NULL),
  CHECK (status NOT IN ('failed','uncertain') OR terminal_event_id IS NOT NULL)
) STRICT;
CREATE INDEX owned_command_status ON owned_command(status) WHERE status IN ('uncertain','accepted');
CREATE INDEX owned_command_session ON owned_command(session_id, accepted_at);

CREATE TABLE owned_interaction (
  interaction_id  TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('permission','elicitation')),
  request_event   INTEGER REFERENCES owned_event(global_sequence),
  state           TEXT NOT NULL CHECK (state IN ('pending','resolved','expired')),
  created_at      TEXT NOT NULL,
  resolved_at     TEXT,
  decision_option_id TEXT,          -- permission：Agent 的原始 optionId
  decision_kind   TEXT CHECK (decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
  elicitation_action TEXT CHECK (elicitation_action IN ('submit','decline','cancel')),   -- 与 `ElicitationAction`（ACP 的 accept/decline/cancel）逐值一致
  elicitation_values_json TEXT,     -- elicitation 的 values（原样保存）
  resolved_by_kind TEXT, resolved_by_id TEXT
) STRICT;
CREATE INDEX owned_interaction_pending ON owned_interaction(session_id, state) WHERE state = 'pending';

CREATE TABLE owned_audit (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.authenticated','node.auth_failed','node.trust_revoked','node.identity_changed',
                 'export.created','export.revoked','import.added','import.removed','provider.configured',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli','pairing_claimant')),
  actor_id     TEXT NOT NULL,
  via_node_id  TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX owned_audit_at ON owned_audit(at);
CREATE INDEX owned_audit_action ON owned_audit(action, at);

CREATE TABLE owned_attachment (
  attachment_id TEXT NOT NULL UNIQUE,                 -- 由 `AttachmentStore::put` 分配并随 `AttachmentRef.id` 返回；`get` 走它做 O(1) 命中
  sha256      TEXT PRIMARY KEY,
  byte_length INTEGER NOT NULL,
  media_type  TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  last_used_at TEXT NOT NULL
) STRICT;

CREATE TABLE owned_attachment_link (
  session_id   TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  attachment_id TEXT NOT NULL REFERENCES owned_attachment(attachment_id),
  generation   INTEGER NOT NULL,
  sha256       TEXT NOT NULL REFERENCES owned_attachment(sha256),
  PRIMARY KEY (session_id, attachment_id)
) STRICT;

CREATE TABLE owned_device (
  device_id     TEXT PRIMARY KEY,
  display_name  TEXT NOT NULL,
  public_key    BLOB NOT NULL,                 -- 65 字节 SEC1 未压缩 P-256
  fingerprint   TEXT NOT NULL,                 -- 64 字符小写 hex = SHA-256(public_key)
  scopes_json   TEXT NOT NULL,                 -- 展开后的 scope（= 命令名）数组，空集合写 '[]'
  state         TEXT NOT NULL CHECK (state IN ('pending','active','revoked')),
  created_at    TEXT NOT NULL,
  last_seen_at  TEXT,
  revoked_at    TEXT,
  revoke_reason TEXT CHECK (revoke_reason IN ('user_requested','key_changed','compromised')),
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
  CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoke_reason IS NOT NULL))
) STRICT;

CREATE TABLE owned_node (
  node_id        TEXT NOT NULL,
  kind           TEXT NOT NULL CHECK (kind IN ('access','owner')),
  display_name   TEXT NOT NULL,
  fingerprint    TEXT NOT NULL,
  grants_json    TEXT NOT NULL,                -- LOCAL_ADMIN_PROTOCOL.md §5.4 的 grants[]
  state          TEXT NOT NULL CHECK (state IN ('pending','paired','revoked')),
  owner_endpoint TEXT,                         -- 仅 kind = 'owner' 非空
  created_at     TEXT NOT NULL,
  last_connected_at TEXT,
  revoked_at     TEXT,
  revoke_reason  TEXT CHECK (revoke_reason IN ('user_requested','key_changed','compromised')),
  PRIMARY KEY (node_id, kind),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
  CHECK ((kind = 'owner') = (owner_endpoint IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoke_reason IS NOT NULL))
) STRICT;
CREATE INDEX owned_node_role ON owned_node(kind, state);

-- Node 双角色共享一条身份材料：主键不含 kind。
CREATE TABLE owned_peer_key (
  peer_kind   TEXT NOT NULL CHECK (peer_kind IN ('device','node')),
  peer_id     TEXT NOT NULL,
  public_key  BLOB NOT NULL,
  fingerprint TEXT NOT NULL,
  bound_at    TEXT NOT NULL,
  PRIMARY KEY (peer_kind, peer_id),
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*')
) STRICT;

-- 只存 pairing secret 的摘要与绑定；明文、HMAC、QR URL、完整认证 payload 都不落库。
CREATE TABLE owned_pairing (
  pairing_id          TEXT PRIMARY KEY,
  target_kind         TEXT NOT NULL CHECK (target_kind IN ('device','node')),
  state               TEXT NOT NULL CHECK (state IN ('created','claimed','pending_confirmation','approved','rejected','expired','consumed')),
  display_name        TEXT,
  requested_scopes_json TEXT NOT NULL,
  requested_grants_json TEXT NOT NULL,
  secret_digest       TEXT NOT NULL,
  host_binding        TEXT NOT NULL,           -- 设备：canonical origin；节点：owner endpoint
  created_at          TEXT NOT NULL,
  expires_at          TEXT NOT NULL,
  claimed_at          TEXT,
  approved_at         TEXT,
  terminal_at         TEXT,
  CHECK ((state = 'created') = (claimed_at IS NULL)),
  CHECK ((state IN ('approved','consumed')) = (approved_at IS NOT NULL)),
  CHECK ((state IN ('rejected','expired','consumed')) = (terminal_at IS NOT NULL)),
  -- 设备配对不对带 grants、节点配对不得带 scopes（§3.5）；空集合固定写 '[]'
  CHECK (target_kind <> 'device' OR requested_grants_json = '[]'),
  CHECK (target_kind <> 'node'   OR requested_scopes_json = '[]')
) STRICT;

-- 每个配对最多一个 peer；claim 之后不能换人（配对行条件更新与唯一主键共同保证）。
CREATE TABLE owned_pairing_peer (
  pairing_id   TEXT PRIMARY KEY REFERENCES owned_pairing(pairing_id) ON DELETE CASCADE,
  peer_kind    TEXT NOT NULL CHECK (peer_kind IN ('device','node')),
  peer_id      TEXT NOT NULL,
  display_name TEXT NOT NULL,
  public_key   BLOB NOT NULL,
  fingerprint  TEXT NOT NULL,
  client_nonce TEXT NOT NULL,
  claimed_at   TEXT NOT NULL,
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*')
) STRICT;

CREATE TABLE owned_export (
  export_id        TEXT PRIMARY KEY,
  display_name     TEXT NOT NULL,
  agent_ids_json   TEXT NOT NULL,              -- Export 内 Agent selector；首切片恰好 1 项
  aliases_json     TEXT NOT NULL,              -- [{alias,displayName}]
  default_alias    TEXT NOT NULL,
  templates_json   TEXT NOT NULL,              -- ExportTemplate[]
  default_template TEXT NOT NULL,
  scopes_json      TEXT NOT NULL,              -- grant.* 子集
  cache_policy     TEXT NOT NULL CHECK (cache_policy = 'no-content-cache'),
  created_at       TEXT NOT NULL,
  revoked_at       TEXT
) STRICT;

CREATE TABLE owned_agent_profile (
  agent_id           TEXT PRIMARY KEY,
  display_name       TEXT NOT NULL,
  command            TEXT NOT NULL,
  args_json          TEXT NOT NULL,
  env_allowlist_json TEXT NOT NULL,
  provider_env_json  TEXT NOT NULL,             -- ProviderEnvBinding[]，空数组写 '[]'
  is_default         INTEGER NOT NULL CHECK (is_default IN (0,1)),
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL
) STRICT;
-- 至多一个默认 profile。
CREATE UNIQUE INDEX owned_agent_profile_default ON owned_agent_profile(is_default) WHERE is_default = 1;

-- 路径只在本节点可读；不进入 Node Link catalog（§11.1）。
CREATE TABLE owned_workspace (
  alias          TEXT PRIMARY KEY,
  display_name   TEXT NOT NULL,
  canonical_path TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
) STRICT;

-- 不含凭据值：只有字段名、keystore 引用与版本。
CREATE TABLE owned_provider_ref (
  provider_id            TEXT NOT NULL,
  kind                   TEXT NOT NULL CHECK (kind IN ('provider','mcp')),
  display_name           TEXT NOT NULL,
  configured_fields_json TEXT NOT NULL,
  keystore_ref           TEXT NOT NULL,
  version                INTEGER NOT NULL,
  updated_at             TEXT NOT NULL,
  PRIMARY KEY (provider_id, kind)
) STRICT;
```

约束与由来（含本轮修订）：

- `[决定]` **非会话级事件**（`device.revoked`、`storage.integrity_failed` 等）必须能落库并参与 `global_sequence` 重放：`owned_event` 以 `global_sequence` 为代理主键（`AUTOINCREMENT`，唯一且严格递增），`session_id`/`session_sequence` 可空（`SYNC_PROTOCOL.md` §10.1 要求非会话级事件的 `sessionId`/`sessionSequence` 为 `null`）。
- `[决定]` `origin_sequence` 是 owned 事件的**独立**会话级序号（跨节点只认它），与 `session_sequence` 同事务各自递增。
- `[决定]` 非会话级事件的 `origin_epoch`/`origin_sequence` 与 `session_sequence` 一样为 NULL（§3.4、§9 判据 17）；成对 CHECK 让「要么全有要么全无」成为表级约束——node 作用域的事件没有会话级 origin cursor，**不得**用 `meta.server_epoch` 或 `global_sequence` 伪造。
- `[决定]` `owned_command` 的幂等键是 `(actor_kind, actor_id, request_id)`（协议维度）；`request_fingerprint` 是对解码后 payload 按 ACPR-CJ1 取的摘要，`expected_version` 原样保存——二者是跨重启冲突判定（`command.idempotency_conflict`）的唯一依据（`SYNC_PROTOCOL.md` §11.1/§11.2）。
- `[决定]` `completed` 的 mutation 必须带 `terminal_event_id`；查询命令固定为 NULL；`failed`/`uncertain` 必须有终态事件（`SYNC_PROTOCOL.md` §11.2）。`result_json` 对 mutation 可空（Sync 侧允许 `result: null`，Node Link 侧要求非空对象——由用例层按协议判定，不在表级强制）。
- `[决定]` `uncertain` 行与 mutation 幂等行**不按 TTL 清理**（见 §7.5）。
- `[决定]` 附件：字节按 sha256 内容寻址存放（`owned_attachment`），会话归属与 generation 由 `owned_attachment_link` 记录，因此单会话清理与全局 LRU 都可实现。
- `[决定]` `compacted_into` 指向**替代它的 summary 事件**的 `global_sequence`（与 `request_event` 同一惯例）；`compacted_into IS NOT NULL` 的行是容量清理 ③ 的第一批候选，且重放里由 summary 事件替代（§6 第 15 条）。
- `[决定]` `owned_attachment.attachment_id` 是写入时由 `AttachmentStore::put` 分配（实现可选随机 UUID，也可选由 sha256 派生的确定性 id）、持久化并随 `AttachmentRef.id` 返回的显式列；`get` 由该列 O(1) 命中，不再按 sha256 前缀重算后扫描。sha256 仍承担内容去重；`IdGenerator` 不参与（同一个 id 只能有一个来源）。
- `[决定]` `acp_raw_unavailable_reason` 表达「保留期/容量清理掉原文但保留 view」的合法状态（`SYNC_PROTOCOL.md` §10.1 的 `rawUnavailable.reason`）。
- `[决定]` `payload_digest` 必须等于对 `payload_json` 施 **ACPR-CJ1**（`SYNC_PROTOCOL.md` §3.3）后的 SHA-256（§9 判据 9）。**前像不是库里那串字节**：`payload_json` 按 §9 判据 16 原样保留调用方字节（不重排键），因此摘要必须由写入方**先规范化再哈希**；唯一写入者是存储层，`payload_digest` 由存储层用共享实现 `acpr-wire` 的 ACPR-CJ1 从 `payload_json` 重算，**不接受调用方提供**（否则一个非法摘要会被落库，跨节点复算与 imported 去重都会失配）。`acpr-wire` 的实现必须与 `scripts/check-contract-assets.mjs` 的参考实现在同一批 fixture 上逐值一致。

### 7.4 `imported_*` 表（无正文）

```sql
CREATE TABLE imported_import (
  import_id     TEXT PRIMARY KEY,
  owner_node_id TEXT NOT NULL,
  display_name  TEXT,
  endpoint_ref  TEXT,
  cache_policy  TEXT NOT NULL CHECK (cache_policy = 'no-content-cache'),
  owner_server_epoch TEXT,          -- Owner 的 serverEpoch；变化即说明对方重建了事件库
  created_at    TEXT NOT NULL,
  removed_at    TEXT,
  grants_json   TEXT NOT NULL       -- grant.* 子集；无可信来源时写 '[]'（该 Import 保持不可用）
) STRICT;

CREATE TABLE imported_session (
  owner_node_id  TEXT NOT NULL,
  export_id      TEXT NOT NULL,
  session_id     TEXT NOT NULL,
  title          TEXT,
  agent_id       TEXT, agent_name TEXT,
  state          TEXT,
  version        INTEGER,
  created_at     TEXT,
  last_origin_epoch   TEXT,
  last_origin_sequence INTEGER,
  acked_origin_epoch  TEXT,
  acked_origin_sequence INTEGER,
  next_local_sequence INTEGER NOT NULL DEFAULT 1,
  attachment_id TEXT,                 -- 当前连接上的 attachment（断开即清空；不跨重启保留）
  attachment_generation INTEGER,
  updated_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, export_id, session_id)
) STRICT;

CREATE TABLE imported_delivery_index (
  owner_node_id   TEXT NOT NULL,
  export_id       TEXT NOT NULL,
  session_id      TEXT NOT NULL,
  origin_event_id TEXT NOT NULL,
  origin_epoch    TEXT NOT NULL,
  origin_sequence INTEGER NOT NULL,
  local_sequence  INTEGER NOT NULL,
  event_type      TEXT NOT NULL,
  payload_digest  TEXT NOT NULL,
  received_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, export_id, session_id, origin_event_id),
  UNIQUE (owner_node_id, export_id, session_id, local_sequence),
  UNIQUE (owner_node_id, export_id, session_id, origin_epoch, origin_sequence),
  FOREIGN KEY (owner_node_id, export_id, session_id)
    REFERENCES imported_session(owner_node_id, export_id, session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE imported_command_ref (
  owner_node_id  TEXT NOT NULL, export_id TEXT NOT NULL, session_id TEXT NOT NULL,
  request_id     TEXT NOT NULL,
  command        TEXT NOT NULL,
  status         TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  accepted_at    TEXT,
  terminal_at    TEXT, terminal_event_id TEXT,
  error_code     TEXT, retryable INTEGER,
  PRIMARY KEY (owner_node_id, export_id, session_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  FOREIGN KEY (owner_node_id, export_id, session_id)
    REFERENCES imported_session(owner_node_id, export_id, session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE imported_audit (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.authenticated','node.auth_failed','node.trust_revoked','node.identity_changed',
                 'export.created','export.revoked','import.added','import.removed','provider.configured',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli','pairing_claimant')),
  actor_id     TEXT NOT NULL,
  owner_node_id TEXT, export_id TEXT, session_id TEXT,
  request_id   TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX imported_audit_at ON imported_audit(at);

-- v1 的 imported_import.export_id 与 UNIQUE (owner_node_id, export_id) 移除，改为关联表：
-- 一个 Import 可关联多个 Export，但同一 (owner_node_id, export_id) 只归一个 Import。
CREATE TABLE imported_import_export (
  import_id     TEXT NOT NULL REFERENCES imported_import(import_id) ON DELETE CASCADE,
  owner_node_id TEXT NOT NULL,
  export_id     TEXT NOT NULL,
  added_at      TEXT NOT NULL,
  PRIMARY KEY (import_id, export_id),
  UNIQUE (owner_node_id, export_id)
) STRICT;
```

约束与由来（含本轮修订）：

- 字段集合 = `NODE_LINK_PROTOCOL.md` §6 的无正文索引 + cursor/ACK/requestId/命令终态引用/local sequence 映射；`imported_session` 只承载 `session.list`/快照所需的摘要元数据（`SYNC_PROTOCOL.md` §9.6 的「快照只含元数据」）。
- `[决定]` 白名单归属：`imported_import` → import 配置与 owner/origin 引用；`imported_session` → cursor/ACK、local sequence 映射、摘要元数据、当前连接的 attachment 凭据；`imported_delivery_index` → owner/origin 引用、event type/digest、local sequence 映射；`imported_command_ref` → requestId 与命令终态引用；`imported_audit` → 不含内容的审计元数据（已确认：`SECURITY_DESIGN.md` §13.4 的白名单已显式加入该项）。
- `[决定]` `imported_*` 禁止出现正文语义列；机器检查用**黄金列清单**逐表比对（§9.6），而不是子串黑名单（子串黑名单会被 `snapshot_json` 之类的新列绕过）。
- `[决定]` `imported_delivery_index`/`imported_command_ref` 对 `imported_session` 建复合外键：会话行不存在时 `commit_receipt` 返回 `PortError::NotFound`，不得静默推进 `next_local_sequence`。
- `[决定]` `attachment_id`/`attachment_generation` 断开即清空；`owner_server_epoch` 变化时清空该 import 的交付索引（对方重建了事件库，cursor 失效）。
- `[决定]` `upsert_session`/`commit_receipt` 都必须先确认 `(owner_node_id, export_id)` 仍在 `imported_import_export` 里（§11.2 第 5 条、§5.2 约束），否则 `NotFound(Export(exportId))` 且零写入：`remove_import` 之后失去 Import 的在途回调必须被拒绝，不能重建已删除的会话行与交付索引。**已知边界**：同一 `(owner_node_id, export_id)` 被重新导入后关联行会再次存在，仅凭这对 ID 无法区分新旧连接/新旧导入；区分需要**导入实例标识**或**连接代际**，需与 `node-link-client` 的 attachment generation 语义一并设计，v1 未实现。
- `[决定]` `drop_import` 只删交付索引与命令引用，**不删** `imported_audit`（审计保留义务）。
- `[决定]` v2 起 Import 与 Export 的归属由 `imported_import_export` 表达（一个 Import 可关联多个 Export）：`owner_node_id` 指向 owned 家族的节点记录，因此**只存值、不用跨族外键**（`MODULE_ARCHITECTURE.md` §4.7），存在性由用例层在写集内校验；同一 `(owner_node_id, export_id)` 只归一个 Import 由该表的 `UNIQUE` 保证，`imported_import` 自身不再假定恰好一个 Export。

### 7.5 保留、清理与容量

| 配置键 | 默认 | 落到 schema 的行为 |
|---|---:|---|
| `storage.transcript_retention_days` | 90 | 清理 `kind ∈ {final_message, structured, summary}` 的 `owned_event`；`kind='state'` 一并纳入该窗口（`AGENTS.md` §6 的「关键结构化事件与状态变化」以 90 天为默认承诺期）；`owned_interaction` 的终态行同期清理 |
| `storage.sync_event_retention_days` | 7 | 清理 `kind='delta'` 的 `owned_event`；cursor 低于窗口下界 → `sync.reset_required`（`reason = cursor_expired`） |
| `storage.max_total_size_bytes` | 2 GiB | 全库上限；度量 = `owned_*` 家族所有 `TEXT` 列的 `length()` 之和 + `owned_attachment.byte_length` 之和 + **`imported_*` 家族所有 `TEXT` 列的 `length()` 之和**（v1 用字符数而非字节数，作为有界增长的代理指标；同一口径同时用于提交前的容量检查与两侧 `prune`）。Access-only 节点上 `owned_*` 为空，因此没有 imported 分量时度量恒为 0、`prune` 永不回收 |
| `storage.max_session_size_bytes` | 100 MiB | 单会话上限；度量同上并按 `session_id` 归集 |
| `storage.persist_deltas` | false | `false` 时允许 turn 完成后把 delta 压缩为 `summary`（写 `compacted_into`），执行中不得压缩 |
| `storage.flush_interval_ms` | 250 | delta 合并提交窗口（§6 第 10 条） |
| `storage.attachment_dir` | `<data_dir>/attachments` | 附件字节目录（`[决定]` 新增键，权限同数据库目录） |
| `storage.attachment_max_file_bytes` | 20 MiB | 单附件上限，超出显式拒绝 |
| `storage.attachment_max_total_bytes` | 1 GiB | 附件总量，超出按 `last_used_at` LRU 清理 |
| `terminal.max_output_per_command_bytes` | 1048576 | 单命令终端上限 |
| `terminal.keep_head_bytes` / `terminal.keep_tail_bytes` | 131072 / 917504 | 截断时保留的头/尾（必须带截断标识） |
| `storage.audit_retention_days` | 365 | 审计保留期（已确认）；到期后按 `at` 清理 `owned_audit`/`imported_audit`。容量压力下审计排在清理顺序**最后**（先清 delta → 正文/状态事件 → 已压缩批次 → 附件，再考虑审计），超限时宁可拒绝新写入也不静默丢证据 |

`[决定]` 清理顺序：① 过期 delta → ② 过期正文/结构化/状态事件 → ②′ **终态交互行**（`owned_interaction.state <> 'pending'`）→ ③ 该会话最旧的已压缩 delta 批次 → ④ 附件 LRU → ⑤ 到期的审计（`storage.audit_retention_days`，365 天；`owned_audit` 与 `imported_audit` 都要清）→ ⑥ 仍超限则拒绝新写入（`PortError::Unavailable(StorageFull)`），不静默丢弃、不继续广播。TTL 清理（①②⑤、以及 ②′）由 `prune` 驱动；容量清理（②′③④）只在超限时触发。

`[决定]`（2026-09-23）**清理任务的驱动者与周期**（此前只写了“由 `prune` 驱动”，没写谁调、多久调一次，等于默认永不执行）：

1. 组合根在取得单实例锁、migration 完成**之后**、开始监听**之前**先执行一次初清理：`SessionStore::prune` + `RemoteDeliveryStore::prune`，加上 `TrustStore::expire_pairings`（§11.6）与 `AttachmentStore::sweep_orphans(启动时刻, 1000)`（§6 第 18 条）。
2. 启动之后按**固定 60 s**周期重复同一批调用（固定 v1 常量，不给配置键：这是保证保留策略真的生效，不是可调业务参数）。
3. 周期任务与写入共用同一个写连接（§7.1 的单写连接），必须**分批并让出**：单次 `prune` 到上限即返回，不得抦住写事务阻塞会话提交；一轮超时或出错只记结构化日志，不中断 daemon。
4. 关闭顺序（以 `SECURITY_DESIGN.md` §12.1 为准）：**停接入层**（不再接受新连接，并排空在途连接至宽限上限）→ **取消周期任务与信号监听**（必须先于停止 Agent）→ 停 Agent 进程 → 最后做一次 `wal_checkpoint(TRUNCATE)` 并清理 endpoint/释放单实例锁。周期任务的取消位置是为了让它们在存储关闭前停止写入，不是「先于接入层」。
5. `MODULE_ARCHITECTURE.md` §4.10 的后台任务清单必须与本节一致（prune / expire_pairings / sweep_orphans / 心跳与重连）。

`[决定]` **②/③ 的谓词必须排除仍被引用的事件行**：`owned_interaction.request_event` 是 `INTEGER REFERENCES owned_event(global_sequence)` 且 `foreign_keys = ON`，所以 ② 必须带 `AND NOT EXISTS (SELECT 1 FROM owned_interaction i WHERE i.request_event = owned_event.global_sequence)`，并且 ②′ 必须排在 ② 之前。否则超限时 `enforce_capacity` 里的 ② 会撞外键、把 `FOREIGN KEY constraint failed` 当成 `PortError::Backend` 抛给上层（而不是 §7.5⑥ 的 `StorageFull`），`prune` 的整个事务也会回滚、连 ① 的 delta 清理都做不成。

`[决定]` **幂等/终态行不参与 TTL 清理**：`owned_command` 的 mutation 行与 `status='uncertain'` 行保留到「目标会话被删除」或「actor 被撤销」，与 `SYNC_PROTOCOL.md` §11.2 的「至少保留到设备被撤销且目标会话被删除」一致。

`[决定]` 清理不得破坏承诺期内的重放：窗口前移后 `retention_window` 必须反映新下界，broker 据此要求客户端重建 snapshot。

## 8. 失败关闭

- `[决定]` `Corrupt` 或权限检查失败 → 写路径全部拒绝、只读查询继续、`server` 以 `internal.unavailable` 返回并保持明确日志（`SECURITY_DESIGN.md` §15）。
- `[决定]` 错误消息不得包含 SQL 文本、完整 prompt、密钥或 ACP raw；日志字段 allowlist 由 `SECURITY_DESIGN.md` §14.1 决定。
- `[决定]` 失败关闭必须有测试：用损坏的数据库文件（或注入 `integrity_ok = false`）打开存储 → 写路径全部返回 `PortError::Corrupt`、只读查询仍可用（§9 未单列判据，但 §8 的两条规则都要有可观察用例）。

## 9. 验收判据（实现该合同的测试）

1. **migration**：空目录新建的当前版本库连续两次启动后 `PRAGMA user_version`、`meta.*_schema_version`、`sqlite_master` 里每条 SQL 文本与全部表的行集**逐字节相同**（幂等）；把 `fixtures/storage/v2/empty.sqlite3`（v2 形状）的临时副本的 `user_version` 顶到 `FILE_FORMAT_VERSION + 1` → 返回具名错误且不写入任何行；`fixtures/storage/v2/from-v1.sqlite3` 的 v1 → v2 → v3 连续升级按判据 28 断言保留性。
2. **单事务提交**：用一个装饰 `SessionStore` 的测试替身统计 `commit` 调用次数，并对第二次调用注入失败；断言
(a) 每个 mutation 恰好一次**接受提交**（幂等行那一次；重试与 `Ephemeral` 过滤都不新增提交，见 §6 第 6/11 条）——一次 mutation 天然还会产生终态提交与 delta 合批提交，本条只约束接受语义不得重复；
(b) 失败后 `owned_session.version`、`owned_turn`、`owned_event`、`owned_command` 与调用前快照逐行相同；
(c) 进程被强杀（Windows 用 `TerminateProcess`、Unix 用 `SIGKILL`）后 `PRAGMA integrity_check = ok`，且**每条曾进入 `accepted` 的 mutation 行**到达终态时 `terminal_event_id` 非空并指向存在的 `owned_event` 行（原措辞「每条 `status <> 'accepted'` 的行都有终态事件」与 §7.3 自己的 CHECK 冲突——查询命令的 `terminal_event_id` 被强制为 NULL、`rejected` 也允许为空，因此不可判据）。
3. **幂等**：同一 `(actor, requestId)` 且指纹相同 → 返回首次结果、事件数不变；指纹或 `expected_version` 不同 → `command.idempotency_conflict`；不同 actor 用同一 `requestId` → 视为两条独立命令。**`expected_version` 不同这一支必须有独立用例**（`None` 与 `Some` 两个方向都要有，否则该分支等于没测）。
4. **序列**：`global_sequence` 唯一且严格递增（唯一索引即为约束）；会话内 `session_sequence` 无空洞；`origin_sequence` 与会话 `origin_epoch` 组合唯一；非会话级事件两者为 NULL 且仍出现在重放流中。
5. **imported 去重与失效**：同一 `origin_event_id` 重发只产生一行、不新增 `local_sequence`；`drop_import` 后该 import 名下交付索引与命令引用为空而**审计行仍在**；`owner_server_epoch` 变化后该 import 的索引被清空。
6. **无正文机器检查（黄金列清单）**：逐张 `imported_*` 表用 `PRAGMA table_info` 取列名集合，与合同冻结的列名集合**逐项相等**（新增列即失败）；并做行为断言：投递一条携带 `payload.view` 的 `resource.event` 后，`local_replay()` 只返回 origin 字段/`event_type`/`payload_digest`/`local_sequence`，且库中任何列都不包含该 view 的文本。
7. **保留与窗口**：`expires_at` 到期行被清理；`kind='state'` 按 90 天窗口清理；清理后 `retention_window` 前移并由 broker 产生 `sync.reset_required`；承诺期内 delta 仍可重放。
8. **容量**：用极小配置值驱动 `max_total_size_bytes`/`max_session_size_bytes`，断言清理顺序（先 delta、再正文/状态、再附件）与被删行集合；**库中存在已过期但仍被 `owned_interaction.request_event` 引用的交互事件时**，容量清理仍必须完成并给出 `Unavailable(StorageFull)`（而不是外键错误映射的 `Backend`）；仍超限时 `commit` 返回 `Unavailable(StorageFull)` 且 `EventPublisher` 收到零次调用。
9. **摘要自洽**：对每条 `owned_event`，用 ACPR-CJ1 重新规范化 `payload_json` 后取 SHA-256，必须等于 `payload_digest`。
10. **引用完整性**：`owned_event.turn_id`、`owned_interaction.request_event`、`owned_attachment_link.sha256` 的悬空行数必须为 0（外键 + 断言）；`owned_command` 的 `(actor_kind, actor_id, request_id)` 唯一。
11. **权限仲裁**：两个**并发**解析请求（单写连接会串行化它们，判据看的是结果而不是交错）恰好一个 `Resolved`；已解析后再次应答 → `AlreadyResolved` 且 `resolved_at`/`decision_option_id` 不变；对不存在的 `interactionId` → `PortError::NotFound`。
12. **ACL 判定**：把平台 ACL 读取抽象成纯函数（输入为合成的权限视图），单测覆盖「组/其他可写」「非当前用户可读写」等视图 → 判定为宽松；正式模式下宽松即失败关闭。真实第二账号的端到端检查作为可选集成测试。
13. **端口纯度**：`cargo tree -p core --edges normal` 的输出与冻结 allow-list 逐行相等（黄金文件）——`core` 的直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`——其中 `p256` **只开 `arithmetic`**（core 只做曲线级点校验，不签名也不验签），因此 `ecdsa`/`rfc6979`/`hmac`/`signature`/`pkcs8`/`spki`/`pem-rfc7468` 等签名与编码栈**不在**闭包内；完整普通依赖闭包与 allow-list 逐项登记在 `scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`；`cargo public-api -p core` 快照不得出现 §3/§5 之外的类型（该半条需要 nightly toolchain + 外部 `cargo-public-api`，未安装时应记录为**未执行**并说明替代判据，不得声称已通过）。
14. **审计**：`action` 只能取 §3.5 的枚举（表级 CHECK + 用例层枚举，**每个取值都要有写入用例**，含该合同 §7.3 与 §7.4 的 `action` CHECK 里 `export.*`/`import.*`/`provider.configured` 五类，以及 v3 新增的 `node.authenticated`/`node.auth_failed`）；`actor_kind` 的取值集在两张审计表上与 `ActorKind` 逐值相等，但在 `owned_command` 上**只有三值**（无 `pairing_claimant`，design D12）；`owned_audit` 与 `imported_audit` 都要有「黄金列清单」测试（逐列 `PRAGMA table_info` 比对，新增内容列即失败）。
15. **交互创建**：一次 `commit` 写入 `interaction` 事件 + `PendingInteractionWrite` 后，`owned_interaction` 恰好一行且 `request_event` 等于**配对事件**的 `global_sequence`（列类型见 §7.3；§9 判据 10 的悬空引用为 0）；装配方传入的任何占位值都不得入库；同一提交里 `interactions` 与 `state.interaction` 同时出现 → `InvalidRequest`；随后 `HistoryInclude.pending_interactions` 读回的行 `options` 为空，而按事件流取到配对事件的 `id` 后 `ReadView::event_payload(id)` 能还原出非空 `options`。
16. **正文读取**：`ReadView::event_payload` 返回的 `view` 文本与库内 `payload_json` **逐字节**相同（含未知字段与嵌套），`AcpRaw::Available.raw_json` 与写入时逐字节相同；原文被清理过的行返回 `AcpRaw::Unavailable`，且 `reason`、`byte_length`、`sha256` 三者都必须与行内列一致（**摘要不得因为原文被清理而丢失**——`acp_sha256` 与 `acp_raw_json` 只有在没有不可用原因时才同有同无）；不存在的事件 id 返回 `None`。
17. **非会话级事件**：`session: None` 的提交落库后 `session_sequence`/`origin_epoch`/`origin_sequence` 三列都是 NULL，且该行仍出现在 `replay` 流里；`session: Some` 的事件三列都非 NULL（成对 CHECK 不得被绕过）。
18. **消息收尾**（§6 第 14 条）：正常结束、取消、失败三种终态各一例；同一 turn 内两个 `messageId` 各自收尾；**没有** delta 的 turn 不产生 `agent.message.completed`；`content` 的文本等于该消息 delta 文本按 `deltaIndex` 的拼接；`completed` 与该 turn 的终态事件在**同一次提交**（用装饰 store 断言批次）；`agent.thought.delta` 不产生 `completed`。
19. **压缩**（§6 第 15 条）：`persist_deltas = false` 时 turn 终态后出现恰好一条 `turn.delta_compacted`，被压行的 `compacted_into` 等于该行的 `global_sequence`，重放里不再出现那些 delta（summary 只带 `turnId`/`deltaCount`）；turn 未终态时**不**压缩；`persist_deltas = true` 时永不压缩；`compacted` 里塞入非 delta 行或跨会话 cursor → `InvalidRequest` 且零写入。
20. **启动恢复**（§6 第 16 条）：造 `status='accepted'` 且无终态事件的 mutation 行 + 未终态 turn → 恢复后该行 `status='uncertain'`、`terminal_event_id` 非空且指向存在的 `command.uncertain` 事件，对应 turn 为 `failed`；`unsettled_commands` 之后返回空；广播发生在提交之后（`EventPublisher` 不得先于 `commit`）。
21. **`session.mode.list`**（§6 第 17 条）：端口返回的候选列表原样出现在结果里；`currentModeId` 与会话 `current_mode` 一致；`version` 等于会话版本；端口返回空列表时结果为空（不伪造）。
22. **附件事务边界与孤儿回收**（§6 第 18 条）：`prune_lru` 删行后崩溃（模拟：删行后不执行文件删除）→ 库内无悬空行；`sweep_orphans(启动时刻, n)` 删掉表外文件、**不删** `mtime ≥ 启动时刻` 的文件、遵守 `limit`、返回实际删除数；回收失败不阻止启动（有警告）。
23. **管理写集的原子性**（§11.2 第 1/2/6 条、§5.3）：一次管理端口调用 = 一个事务 = 状态 + 引用 + 审计；注入审计写失败、唯一键/CHECK 冲突、磁盘满 → 整事务回滚，断言既不留「提交成功但没有持久信任」，也不留半条授权。
24. **配对、撤销与重启恢复**（§11.2 第 3 条、§11.3）：两个并发 `claim_pairing` 恰好一个成功；拒绝或过期不创建信任；已撤销身份不能经普通写入复活；重启后撤销仍有效、未确认配对不再可用、同一 `nodeId` 的两种角色行共享一条身份材料并被一并撤销。
25. **配对公钥与信任材料**（§11.5、§5.3）：认领并重启后仍能经 `TrustStore::peer_key` 取到验签公钥；指纹与公钥不一致的写集无法落库（表级 CHECK + 用例层构造校验）。
26. **Export/Import 归属与完整移除**（§11.6、§7.4）：`ImportWrite.exports` 必须等于 `record.export_ids()`（分歧 → `InvalidRequest`）；同一 `(owner_node_id, export_id)` 归属冲突 → `Conflict(DuplicateOwnership)`；`remove_import` 与连接级 `drop_import` 的删除权威不重叠，完整移除后审计行仍在。
27. **本地配置与凭据边界**（§11.6、§5.3）：至多一个默认 profile 且切换默认是一次原子写集；Provider 引用只存字段名/keystore 引用/版本，换绑递增版本；种子 profile 与「已初始化」标记同事务提交、空种子也标记、重复打开不重导；`CredentialResolver::resolve_env` 只返回 `env_allowlist` ∩ `env` 绑定，引用失效 → `Unavailable(KeystoreUnavailable)` 失败关闭，日志只记变量名与数量。
28. **旧库升级的保留与幂等**（§7.2）：升级保留 `server_epoch`、事件 `global_sequence`/`session_sequence` 与 origin cursor、`requestId` 与幂等行、命令终态与全部既有审计；`audit_id` 与其 `AUTOINCREMENT` 序列不回退（v1 → v2 → v3 连续升级也要保持），审计表的新取值在升级库上可写、`owned_command` 的 `actor_kind` 不接受 `pairing_claimant`；`imported_import` 不再有 `export_id`，Export 关联迁入 `imported_import_export` 且 `added_at` 取原 `created_at`，无可信来源的 grants 保持 `'[]'`（该 Import 不可用）；升级后第二次打开 `sqlite_master`/`meta`/行集逐字节不变。
29. **管理状态纳入容量与失败关闭**（§7.5、§8）：容量度量包含管理表的 TEXT 列；超限时拒绝新写入而不删除活动信任、撤销记录或未到期审计；损坏库或宽松权限下**管理写路径**与 owned 写路径一样全部被拒，只读查询仍可用。
30. **管理记录的终态与单调性**（§11.1、§11.2 第 4/5 条、§5.2/§5.3 约束、§7.4）：① `put_export` 对已撤销的 Export 不得清除 `revoked_at`——传入未撤销记录 → `Conflict(AlreadyExists)` 且该行逐列不变，传入 `revoked_at` 非空记录 → `InvalidRequest` 且零写入（含零审计）；② 完整移除 Import 后，携带该 `(ownerNodeId, exportId)` 的 `upsert_session` 与 `commit_receipt` 都返回 `NotFound(Export)`，`imported_session`/`imported_delivery_index`/`imported_command_ref` 保持为空且不推进 `local_sequence`；③ `owned_peer_key`/`owned_pairing_peer`/`owned_device` 中任一行 `fingerprint` 与同行 `public_key` 的派生值不一致时，对应读取路径（设备记录含单读与列表读）返回 `PortError::Corrupt` 且不返回材料；④ `last_seen_at`/`last_connected_at` 在「旧值为空」「新值更早」「新值为空」三种边界下都不丢值、不倒退，且不使调用失败或丢弃同写集的其他字段。
31. **§10.3 的 view 身份与版本**（§6 第 19 条）：① 适配器视图不含 `turnId` 时，落盘 view 与 `owned_event.turn_id` 都等于 core 的权威 turn，且该 view 除新增的**一个前置成员**外逐字节不变（含未知字段、嵌套结构；ACP 原文 `raw_json`/`sha256`/`byte_length` 不变）；② 会话级或无归属事件不出现 `turnId`；未列入 §10.3 的类型（如 `terminal.output`）不新增该字段；③ 视图已带 `turnId` 且取值一致 → 字节不变且只出现一次，取值不一致 → `InvalidRequest` 且该批零落盘、零发布、turn 状态不变；④ 含 `session.mode.changed`/`session.config.changed` 的提交：无 `StateChange` 时注入当前版本且不递增，含 `StateChange` 时注入递增后的版本，两者都必须等于存储层返回值；存储返回不一致 → 不发布且不报成功；⑤ 幂等重放的 view 与首次落盘逐字节相同且不二次注入；⑥ 适配器 `prompt` 返回任意值（含全零占位）都不产生第二个 turn 行，也不改变归属。

## 10. 未决项

- `[已裁定]` `ElicitationValues` 的形状：见 §3.3——值域为 ACP `ElicitationContentValue` 的五种线格式（`Text`/`Integer(i64)`/`Number(f64)`/`Boolean`/`TextArray`）**加**一个「未知形状原样保留」变体；边界 ≤64 KiB、深度 ≤16；`Submit` 的空映射与 `None` 可区分（分别对应 ACP `content: {}` 与 `content: null`）；`Decline`/`Cancel` 必须为 `None`。同时补齐 ACP 已有而我们 v1 原先缺失的 `decline` 动作（已同步 `SYNC_PROTOCOL.md` §11.5/§10.2 与 `NODE_LINK_PROTOCOL.md` §12.7）。
- `[已裁定]` Sync 与 Node Link 对 `completed` 的 `result` 要求不同（Sync 允许 `null`，Node Link 要求非空对象）：**认定差异是有意的**——Sync 的 mutation 完成后可能没有可返回的数据，Node Link 必须让 `session.create` 回传 `SessionCreateResult`（§12.7）。core 保持 `Option<CommandResult>`，「非空」是 `server::node_link` 的映射期义务并带契约测试；两侧协议文档各加一句说明（`SYNC_PROTOCOL.md` §11.2、`NODE_LINK_PROTOCOL.md` §12.5）。
- `[确认]` 序号上界 `2^63−1`（已批准）：随本次改动写入 `SYNC_PROTOCOL.md` §3.2。
- `[已裁定]` `PairingRecord`/`PairingState`/`PeerIdentity`/`PairingPeer`/`PairingClaim`/`PairingSettlement` 的记录级字段**已冻结**（§3.5）：首实现（`crates/core/src/model/identity.rs`）在合同标 `[open]` 期间落地了形状与构造校验，本轮把它们**采纳为合同形状**（未发布、未实现发布语义，采纳不影响任何已交付行为），`[open]` 据此关闭。
- `[已裁定]` **`session.mode.list` 的候选来源**：新增 `SessionEndpoint::modes()`（§5.1）；用例层只补 `version`，不得编造候选（§6 第 17 条）。
- `[已裁定]` **`agent.message.completed` 的生成**归 **broker**（与该 turn 的终态事件同一次提交），正文只从 delta 的 view 折叠；非文本内容的投影归生产端适配器（`agent.message.delta` 增加可选 `block` 字段），core 不解析 ACP 原文。思考流不产生收尾事件（登记在案的短保留期降级）。
- `[已裁定]` **启动恢复**：组合根在开始监听前调 `RecoverUnsettled`，把 `accepted` 且无终态的 mutation 终结为 `uncertain`（对应 turn → `failed`），不重放副作用；新增 `SessionStore::unsettled_commands`（§6 第 16 条）。
- `[已裁定]` **delta 压缩**：broker 在 turn 终态之后提交一条 `kind='summary'` 的 `turn.delta_compacted` 事件（只带收据 `turnId`/`deltaCount`，不复制正文），`OwnedCommit.compacted` 列出被替代的行，存储层写 `compacted_into`；执行中与 `persist_deltas = true` 时都不压缩（§6 第 15 条）。
- `[已裁定]` 交互**创建**路径：`OwnedCommit.interactions`（`PendingInteractionWrite`，与同提交的 `interaction` 事件成对）；交互**解析**路径：`OwnedCommit.state.interaction`。原 §5.2 约束里「`resolve_interaction` 条件更新」引用的方法在端口集里并不存在，已改为 §6 第 13 条；`SessionEndpoint::resolve_interaction` 只面向后端。
- `[已裁定]` `owned_event` 的 `acp_sha256` CHECK 修正为「有不可用原因时不要求与 `acp_raw_json` 同有同无」：模型允许 `AcpRaw::Unavailable { sha256: Some(_) }`（摘要算得出来、原文已被保留期/容量清掉），原 CHECK 会让这个合法状态整行写不进去（实测 SQLite 275）。修正后 `reason`/`byte_length`/`sha256` 三者都能保真。
- `[已裁定]` `ReadView::event_payload(&EventId) -> Option<EventPayload>`：正文只有这一个读取入口（复用 core 既有 `EventPayload`/`AcpRaw`/`RawUnavailableReason`，不新增类型）；`PendingInteraction.options` 不落库，由该入口还原。
- `[已裁定]` 非会话级事件：`owned_event.origin_epoch`/`origin_sequence` 与 `session_sequence` 一样可空 + 成对 CHECK；`owned_event_session` 升为唯一索引并新增 `owned_event_origin` 部分唯一索引——原 §7.3 的 `NOT NULL` 与 §3.4、§9 判据 4 直接冲突，按后者修正（node 作用域事件没有会话级 origin cursor）。
- `[已裁定]` §7.5 的度量**实现**：语句在 `SqliteStore::open` 时由 `sqlite_master` + `pragma_table_info` 现读两家族全部 TEXT 列拼出并缓存（不手写列清单，新增列不会被漏掉），并经 `StoreHealth.total_bytes` 暴露给调用方与测试；测试助手按同一算法计算，`tests/retention.rs::capacity_measure_matches_the_store` 断言两端相等（单写两处口径曾导致一次误诊）。
- `[已裁定]` `RemoteDeliveryStore::prune` 的 ⑤（到期审计）**不得**被「未超容量就提前返回」跳过：TTL 由 `prune` 驱动，容量才是有条件的那一半。
- `[已裁定]` `owned_attachment` 增加 `attachment_id` 唯一列（`AttachmentStore::get` 由「重算 id 后扫描」改为 O(1) 命中）；`owned_attachment_link.attachment_id` 同样引用它。
- `[已裁定]` `RemoteDeliveryStore::find_request` 改名 `find_remote_request`（与 `SessionStore::find_request` 同名会迫使每个调用点写 UFCS）。
- `[已裁定]` Windows 的 ACL 判定返回 `Unverifiable` 且不失败关闭（平台限制，见 §7.1）；Unix 仍失败关闭。
- `[open]` **`session.mode.changed`/`session.config.changed` 的 `version` 语义**（2026-09-24 登记，来源：变更 `core-turn-view-fields` 的 RV1-WP1-F2）：§10.3 只要求「十进制字符串的会话版本」，而 §6 第 19 条的推导口径是「该次提交后的会话版本」。真实模式/配置切换流程中，`session.*.changed` 事件（来自适配器）与状态变更是**两次提交**（§6 第 8 条的 turn 边界语义），因此注入的是**变更前**版本，变更后的版本只出现在后续 `command.completed` 的 result 里。本变更不改变批形状（不合并两次提交）；在 Sync 切片前必须裁定：要么把两者合并为同一提交（使事件承载变更后版本），要么在 §10.3/Sync 文档里明确「mode/config 事件承载变更前版本」并让客户端不以它为乐观并发基准。**未裁定前 Sync 切片不得假设事件里的 `version` 等于会话最终版本。**
- `[open]` **`command.completed.result.turnId` 承载的是会话版本而非 turn 标识**（2026-09-24 登记，来源同上，RV1-WP1-F3）：`view_command_completed` 把 `apply_state` 传入的 `Some(version)` 渲染成 `result.turnId`（十进制字符串），而 `SYNC_PROTOCOL.md` §11.5 的 `result.turnId` 示例是 UUID。本变更未触碰该函数（属变更外既有缺陷）；应在 Sync/CLI 切片把它改为 `result.version`（或 `null`）并同步协议文档与 fixture。
- `[已裁定]` `owned_interaction.elicitation_action` 的 CHECK 补 `'decline'`（原值是 `submit|cancel` 两值，会让 ACP 早就有的 `decline` 在解析事务里撞约束、交互永远停在 `pending`）；同时要求 DDL 里每个 `IN (...)` 枚举字面量与 core 枚举的 wire 值逐条一致，并有一条「每个枚举值都能写入并读回」的测试。
- `[已裁定]` id 分配收敛为两处权威（§3.1）：`SessionId`/`EventId` 归存储层事务内分配，`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId` 归 core 的 `IdGenerator`，`AttachmentId` 归 `AttachmentStore::put`。`IdGenerator` 因此删掉 `session_id()`/`attachment_id()`——原 §3.1 与 §5.4 的写法互相矛盾，实现者只能二选一。
- `[已裁定]` `SessionBackendFactory::create` 增加 `session: &SessionId` 参数：§5.1 原先承诺「core 把分配好的会话传给 `create`」，而 §3.6 的 `CreateSessionRequest` 字段表里没有 `session`，后端因此拿不到 id、无法构造 `SessionEndpoint::reference()`。
- `[已裁定]` `templateId` 的权威是 `schemas/node-link/v1/common.schema.json` 的 `^[A-Za-z0-9._-]{1,128}$`（§3.6 也写的 1..=128）：`docs/LOCAL_ADMIN_PROTOCOL.md` §5.5 与 core 的 `is_template_id` 原写 64，按 wire 取 128（否则跨节点会拒收 schema 合法的模板 id）。
- `[已裁定]` `payload_digest` 由**存储层**用 `acpr-wire` 的 ACPR-CJ1 实现从 `payload_json` 重算（§7.3/§9 判据 9）：原实现直接哈希库内字节，而 `payload_json` 按判据 16 原样保留调用方字节（不重排键），两者只有在调用方恰好传了规范 JSON 时才相等——跨节点复算与 imported 去重都会失配。ACPR-CJ1 是跨 Sync/Node Link 的共用机制，因此实现放 `acpr-wire`（叶子 crate），并与 JS 参考实现在同一批 fixture 上互校。
- `[已裁定]` 清理顺序补 ②′（终态交互行必须先删）且 ②/③ 谓词必须排除仍被 `owned_interaction.request_event` 引用的行；容量度量补 `imported_*` 家族分量（否则 Access-only 节点度量恒 0、永不回收）；⑤ 的审计清理对 `owned_audit` 与 `imported_audit` 都生效。三条都是实测缺陷：外键会让 `prune` 整事务回滚、并让超限提交报 `Backend` 而不是 `StorageFull`。
- `[已裁定]` `PendingEvent` 增加必填 `origin: EventOrigin`（§3.4）：原写入形状没有 origin，存储层只能按「有没有会话」猜出 `agent`/`daemon`，等于伪造 `origin.kind`。
- `[open]` 交互解析的崩溃窗口（「行已终态、`*.resolved` 事件未落盘」）的补偿机制：当前按 §6 第 13 条接受；若将来要消除，需要「解析意向」行或两阶段提交。
- `[已裁定]` **附件文件删除的事务边界与孤儿回收**（§6 第 18 条）：行删除与事务同提交、文件删除在提交之后；孤儿回收由组合根启动时调用一次 `AttachmentStore::sweep_orphans(启动时刻, 1000)`，只删「不在表里且 mtime 早于本次启动」的文件，失败不阻止启动。新增该端口方法（§5.3）。
- `[已裁定]`（2026-09-23）管理写集需要的 `ConflictKind` 新取值（`AlreadyExists`、`IdentityMismatch`、`DuplicateOwnership`）与 `UnavailableKind::KeystoreUnavailable`：枚举本体、`ALL`/`as_str` 与 §2 的取值表已同批落地（`crates/core/src/model/error.rs`）；写集侧映射义务与 `port_error_public` 不得用通配臂吞掉新取值的陷阱见 §5.3 与 §11.6 末段。
- `[已裁定]`（2026-09-23）`AuditAction` 追加 `ExportCreated`/`ExportRevoked`/`ImportAdded`/`ImportRemoved`/`ProviderConfigured`（`SECURITY_DESIGN.md` §14.2）：落库靠 §7.3 与 §7.4 两张审计表 `action` CHECK 的扩宽；既有 v1 库走 §7.2 的第 ② 步 12-step 表重建（保留全部行与 `audit_id`、`AUTOINCREMENT` 序列不回退）。
- `[已裁定]`（2026-09-23）首切片 workspace template 必须零参数（`NODE_LINK_PROTOCOL.md` §10）；有参 template 属 `post_mvp`，启用前必须定义值的来源与用途。
- `[已裁定]`（2026-09-26）配对通道的 actor 与用例面扩展（design D12，用户裁定方案 A）：`core::model` 新增 `Actor::PairingClaimant { pairing: PairingId }`（`ActorKind::PairingClaimant` = `'pairing_claimant'`）与两个节点审计动作；`claim_pairing`/`pairing` 另接受绑定该配对的认领方，新增 `consume_pairing(actor, PairingId)` 与 `TrustStore::consume_pairing`（`PairingConsumption`）；存储走 **v2 → v3** 迁移（两张审计表的 `actor_kind`/`action` CHECK 扩宽，12-step 表重建），`owned_command` 不动——`pairing_claimant` 永不可提交命令。涉及 §3.5/§4/§5.3/§7.2/§7.3/§7.4 与 `SECURITY_DESIGN.md` §14.2、`IDENTITY_AND_AUTH_CONTRACT.md` §5.1，漂移门禁随动。
- `[已裁定]`（2026-09-26）D12 的 seam 补全（用户裁定 A 的实现细化，不改变方向）：§4 新增只读入口 `pairing_channel_view(actor, PairingId) -> Option<PairingChannelView>`（记录 + 已认领对端行 + 已批准后的对端 `access` 节点行，绑定式读取、零写入）；`identity-auth` 新增 `Authority::verify_node_link_pairing_status`（用本机内存里的 pairing secret 校验 `node-link-pairing-status/v1` 的 HMAC）与 `Authority::pairing_request_material`（返回 claim 响应与 Owner 证明需要的**非秘密** `serverNonce`/`pairingRequestId`，与 secret 同生同灭）；claim 路径的读取顺序与拒绝口径写在 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1。
- `[已裁定]`（2026-09-26）WSS 握手 seam 补全（`node-link-owner` 的 WP4/任务 2.28，主 Agent 裁定方案 A，与 D12 同方向）：§4 新增 `NodeLinkHandshake` 两项——只读入口 `node_link_handshake_view(accessNodeId) -> NodeLinkHandshakeView`（用例面唯一无 `actor` 的入口：按单个自报 node id 返回该节点的 `access` 信任行、已绑定验签公钥、最近一次配对与本机水位 `store.head()`；未知 id 返回空视图而不是错误，零写入、零审计）与窄写入口 `record_node_link_auth(accessNodeId, action, outcome)`（只接受 `node.authenticated`/`node.auth_failed`，其余 → `authorization.scope_denied`）；§5.3 新增 `TrustStore::pairing_for(peer) -> Option<PairingRecord>`（该对端最近一次配对：登记方宣告的 `host_binding` 与「待消费配对」的唯一来源——节点信任行不带绑定列）。既有 `LocalCli` 路径的行为不变，不新增对端可见能力；`identity-auth` 的 `HandshakeFailure::audit()` 对 NodeLink 从 `None` 改为 `Some(node.auth_failed)`（原注释所称的「词表缺口」已由上述两个动作补上）。
- `[已裁定]`（2026-09-26）认证收尾的 `last_connected_at`（`node-link-owner` 的 wp4-fix1 轮次 / RV1-WP4-F2，主 Agent 裁定选项①）：认证收尾必须在写集里推进 `owned_node.last_connected_at`（「最近一次认证成功时间」）——首次认证在 `consume_pairing` 的写集里（§11.6 第 8 条），重复认证在新增的 `TrustStore::record_node_connected`（`NodeConnectedWrite`，§5.3/§11.6 第 9 条）里；两条路径的时间与 `node.authenticated` 各自同事务，值只前进不倒退（§3.5/§5.3 约束）。原实现没有任何生产调用方推进该列（`put_node`/`upsert_node` 只被配对确认与测试使用），管理视图 `lastSeenAt` 因此恒空。涉及 §3.5/§4/§5.3/§11.6 与 `crates/{core,storage-sqlite,server}`，漂移门禁随动。
- `[已裁定]`（2026-09-26）Owner 侧无会话节点命令的授权与两条 seam 的收口（`node-link-owner` 的 WP6/任务 2.17–2.19，主 Agent 裁定方案 (1)）：`Broker::node_allowed` 原先在 `session = None` 时恒返回 `false`，因此 `UseCases::{create_session,command_status,list_sessions}` 对 Owner 侧 `Actor::Node` 恒拒，而 `NODE_LINK_PROTOCOL.md` §12.5/§12.7 要求这些命令可用。修正：Owner 侧判定改为「该对端 `access` 信任行已配对且 `grants` 含 `required_grant`」+「存在未撤销且覆盖该命令的 Export（`scopes` 含该 grant、`scopes ∩ grants ≠ ∅`）」，会话命令另需该 Export 覆盖目标会话的 agent；无会话命令（`session.list`/`command.status`/`session.create`）只要求前两条。为此 `BrokerDeps` 新增必需的 `Arc<dyn TrustStore>`（授权不能只靠 Export 记录）；`record_node_link_auth` 的接受动作集扩到 `authorization.denied`（适配层比 broker 更严的越权拒绝路径需要留痕，R69）；`session.list` 的会话过滤与 `session.create` 的参数校验仍由 `server::node_link` 承担。涉及 §4/§6 第 5 条/§10 与 `crates/{core,server,app}`，漂移门禁（§5/§7）不受影响。
- `[已裁定]`（2026-09-23）`identity-keystore` 的 Windows 第一档位与 Linux 失败关闭：Windows 用 DPAPI（当前用户）包裹私钥 + 进程内签名，Linux 维持失败关闭；持久化 fallback、CNG/TPM 不可导出档位均需单独 ADR，wrapper 选型与 MSRV 约束见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20。实现前合同见 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md) §7/§9。
- `[open]` 未来加密离线正文缓存（必须新 feature + Owner 明示授权 + ADR；本合同不预留任何静默开关）。

## 11. 管理状态持久化合同（形状已并入 §3/§5/§7）

`[已并入]`（2026-09-23）本节收口的设计**形状**已经落地：值对象在 §3.5/§3.6，写入 DTO、端口签名与 workspace 解析规则在 §5.1/§5.3，管理与 Import 关联表的 DDL 在 §7.3/§7.4，版本常量与 v1 → v2 迁移规则在 §7.2，验收判据在 §9 判据 23–29；`scripts/check-contract-drift.mjs` 把 §5/§7 与 `crates/core/src/ports.rs`、`crates/storage-sqlite/src/migrate.rs` 逐条绑定，因此本节不再保留签名与 DDL 正文（第二份副本必然漂移）。

本节剩下的是**设计理由**与到上述段落的指针。`storage-sqlite` 的**管理 store 落盘实现**（`TrustStore`/`ExportStore`/`LocalConfigStore` 的事务、失败关闭与容量纳入）已落地；Daemon/CLI 接线与 `server` 的**本地**入站适配器（`server::local_admin`/`server::transport::local`）与 `app`（组合根、CLI、`acp-stdio`）已随切片 4 `daemon-cli-and-local-admin` 落地；仍未实现的是 `server::sync`/`server::node_link`/`server::acp_facade` 与 `node-link-client`（`identity-auth`/`identity-keystore` 已落地）——在这些适配器完成之前，不得把这些存储测试通过解释为配对、撤销或本地配置已经端到端可用。配置与管理状态的来源优先级以 `CONFIG_REFERENCE.md` 的「配置与管理状态的权威」为准。

### 11.1 数据归属与表设计

管理数据与会话数据共用 §7.1 的数据库、串行写事务和文件权限；不新增独立数据库或通用配置服务。下表是逻辑表与其约束的索引，可执行 DDL 在 §7.3/§7.4 并由漂移门禁逐条断言。

| 表 / 记录 | 主键与内容 | 约束 |
|---|---|---|
| `owned_device` | `device_id`；§3.5 `DeviceRecord` 的全部非秘密字段 | 状态与 `revoked_at` 一致；同 ID 不得被普通 upsert 换绑公钥或恢复已撤销权限 |
| `owned_node` | `(node_id, kind)`；§3.5 `NodeRecord` 的全部非秘密字段 | 同一对端可同时承担 Owner/Access；`node_id` 的身份指纹必须一致；按 NodeId 撤销影响两种角色 |
| `owned_peer_key` | `(peer_kind, peer_id)`；65-byte SEC1 未压缩 P-256 公钥、SHA-256 指纹 | 公钥是非秘密认证材料；配对验证后与信任一起提交，重启验签从此读取；不保存私钥或完整认证请求 |
| `owned_pairing` | `pairing_id`；§3.5 `PairingRecord` 的状态、目标、请求权限、secret digest、`host_binding`、时间戳 | `host_binding` 是本机在该配对里宣告的绑定（设备 canonical origin、节点本机 endpoint），认领时被逐字回显（§11.2 第 1 条）；不存 secret、HMAC、QR URL 或完整认证 payload |
| `owned_pairing_peer` | `pairing_id`（主键）；claim 后的对端身份（`peer_kind`/`peer_id`）、名称、公钥、指纹、`client_nonce`、`claimed_at` | 每个配对最多一个 peer；与配对目标一致；claim 之后不能换人；确认时公钥转入信任材料。**不存绑定与角色**：绑定只在 `owned_pairing.host_binding`（认领时已校验两侧逐字相等，读取时回填），对端角色由配对方向推导（§11.2 第 2 条） |
| `owned_export` | `export_id`；§3.5 `ExportRecord` 的全部字段 | `cache_policy` 固定；默认 alias/template 必须属于本 Export；撤销记录保留 |
| `owned_agent_profile` | `agent_id`；本地 `agent.configure` 的名称、命令、参数、环境白名单、default 及创建/更新时间 | 至多一个默认 profile；字段不含 Provider/MCP 凭据；参数不经 shell 拼接 |
| `owned_workspace` | `alias`；本地 `workspace.select` 的名称、规范化路径及创建/更新时间 | 路径只在本节点可读；建立/使用时校验目录与路径边界；不进入 Node Link catalog |
| `owned_provider_ref` | `(provider_id, kind)`；名称、已配置字段名、keystore 引用及版本 | 不含凭据值；引用失效时失败关闭，不回落到配置文件 |
| `imported_import`（升级） | `import_id`；`ImportRecord` 的 endpoint、owner ID、grant 集合、时间戳 | 管理记录自身不再假定恰好一个 Export；不存在内容列 |
| `imported_import_export`（新增） | `(import_id, export_id)`；外键到 Import，关联 Owner | 同一 `(owner_node_id, export_id)` 只归一个 Import，避免两套权限与删除权威；一个 Import 可关联多个 Export |
| `owned_audit` / `imported_audit` | 复用 §7 已有审计表与 §3.5 `AuditRecord` | 不另建聊天式审计正文；跨表查询稳定排序，过滤时间与类别 |

集合字段在 SQLite adapter 内编码为有类型的 JSON 数组（scopes、grants、Agent selector、alias/template/params、args、envAllowlist），读写都按领域构造器及现有协议约束校验；不把管理 DTO 或任意 JSON 对象直接塞进 core。身份、状态、时间、唯一键与外键使用显式列，不能只靠 JSON blob 保证仲裁。Export 的 workspace 引用在本机解析，不将原始路径复制到 Export。

可执行 DDL 与索引在 §7.3/§7.4，读取面与写集在 §5.3；下面这张表只是「哪张表承载哪类事实」的索引。

当前 `NodeRecord` 的读取面已经带角色维度（`node(&NodeId, NodeKind)`、`nodes_for(&NodeId)`，§5.3），禁止隐式取第一行；配对 DTO 已经携带公钥本身（`PairingPeer.public_key`，§3.5），指纹由它派生——WSS 握手不能假定对端会重新发送公钥，也不能由指纹反推公钥。公钥绑定由信任事务唯一写入，Node 双角色共享一条身份材料；改变绑定必须遵循身份变化/重新配对规则（§11.2 第 2 条）。

### 11.2 原子提交与失败处理

1. **认领**：验证 HMAC/proof 后，单事务检查配对存在、未过期、仍为 `created` 及**本机绑定一致**，插入唯一 peer 并转到 `pending_confirmation`；并发 claim 只有一个成功。`claimed` 仅为事务内过渡，不形成可重新认领的中间提交。「本机绑定一致」由 `PairingPeer.host_binding`（claim 的回显值：设备 `canonicalOrigin`、节点 `endpoint`）与 `PairingRecord.host_binding`（登记时宣告的值）**逐字相等**判定，不一致 → `Conflict(IdentityMismatch)` 且不推进任何状态；host/Origin 级的协议检查（`SYNC_PROTOCOL.md` §7.2 的 Host/Origin 匹配、`NODE_LINK_PROTOCOL.md` §13.2 的 403 路径）由协议层负责，不替代这里的相等判定。
2. **确认**：单事务完成状态/过期检查、固定 peer 与最终 scopes/grants 校验、创建信任记录、更新配对为 approved、写入对应审计。失败时全回滚，绝不能返回成功却没有持久信任。拒绝或过期不创建信任；已撤销身份不能经普通 upsert 自动激活，只能按协议重新配对——`put_device`/`put_node` 一律拒绝 `revoked` 行，而 `settle_pairing` 的批准路径（对端已出示配对 secret 的 HMAC/proof 且本机用户确认）是**唯一**的恢复入口，复活保留撤销审计（§11.3 的 tombstone 语义）。落定只接受 `pending_confirmation`（已落定/已终态 → `Conflict(Consumed)`，不重复改写首次批准时间）。节点配对的批准创建 `owned_node` 的 **`access`** 行（`owner_endpoint` 为空）：只有 `node.pair.begin --mode owner` 会创建节点配对行，而 `LOCAL_ADMIN_PROTOCOL.md` §5.4 明确「`--mode access` 本机没有 `confirm` 调用」，因此对端角色可推导、不需要在写集里携带；`owner` 角色行由该节点自己被信任时经 `put_node` 写入。
3. **撤销**：单事务记录撤销时间、撤销状态与对应审计；提交后阻断新命令/订阅并关闭适用连接，再回答管理调用。提交失败返回失败，不把内存撤销当作持久成功；连接清理失败也不回滚已提交的撤销，阻断后续访问并报告失败。重启先加载撤销状态，再允许连接。
4. **Export**：创建前验证 Agent、workspace、模板与默认引用；创建/撤销各为一个事务。撤销先提交再发送 `export.revoked`；发送失败不撤销数据库决定。授权从最新记录计算，不能仅信任旧连接缓存。
5. **Import**：`import.add` 的管理行与全部 Export 关联行一次提交，重复 ID 或重复 Owner/Export 归属显式冲突；`import.remove` 同事务删除管理行、关联行及对应 `imported_session`/delivery/command 引用，审计保留。提交后停止连接/重连、清空内存正文；失去 Import 的在途回调必须被拒绝，不能重建已删除的索引。端口级落点：`upsert_session`/`commit_receipt` 在同一写事务内的归属前置（关联行缺失 → `NotFound(Export(exportId))`、零写入，§5.2/§7.4）。**已知边界**：同一 `(ownerNodeId, exportId)` 被重新导入后该前置会再次成立，区分旧连接/旧导入的迟到回调需要导入实例标识或连接代际，`node-link-client` 落地前不实现。
6. **审计**：涉及已登记安全动作的管理 mutation，其成功审计与状态同事务提交；审计写入失败则整事务失败。被拒绝的请求只尝试追加失败/拒绝审计，不能因审计不可写而继续执行。适配器记录固定错误类型，不记录凭据。独立 `AuditStore::append` 用于没有关联状态变更的审计，不用于伪造跨端口原子性。
7. **凭据引用**：SQLite 与 keystore 不做分布式事务。先写新的、带版本的 keystore 条目，再提交 SQLite 引用；失败时旧引用继续有效，未引用条目作为孤儿回收。新引用提交后才能清理旧条目；重启发现引用缺失则明确不可用，不能静默生成新身份。

实现入口由 core 用例组织授权和状态决定，storage-sqlite 在单个端口调用内提交完整管理写集；连接关闭等外部效果在提交后通过端口/组合根装配执行。管理写入 DTO/端口已按本节扩展为携带审计上下文与完整写集（§5.3）；禁止用“两次 await 共用一个连接池”宣称同一事务。`UseCases::remove_import` 已从「先 `drop_import` 再 `remove_import`」撤回为一次 `ImportRemoval`，mutation 后独立追加审计的路径也已替换为写集携带审计。

### 11.3 重启、保留与升级

- pairing secret 仍只存在内存；Daemon 重启不能凭 secret digest 恢复它。启动时终结未确认且无法继续验密的配对，客户端重新发起配对；已批准的信任记录保留，不要求正常重连重新配对。原有效期不能延长；临时 nonce、SAS、HMAC 和 QR URL 不落入普通数据库。
- pairing 终态记录至少保留到原 `expires_at`，随后可清理；安全审计按既有 365 天策略保留。活动信任、授权与配置不按聊天 TTL 清理；撤销 tombstone 不因容量压力被删除，防止旧身份恢复。
- 管理表纳入现有总容量度量；空间不足时拒绝新写入，不能删活动信任或未到期审计腾空间。Import 关联表与交付表继续执行无正文黄金列清单检查。
- 文件格式 v2 现在承载管理表与 Import 归属升级：owned/imported 家族版本各推进到 2；`server_epoch`、会话 origin、事件序号、requestId 与已有审计全部保留（§7.2）。旧二进制因版本过新拒绝打开，不能降级写入。
- v1 的单 Export Import 行已迁为一个管理行加一条关联行；没有可信来源的 grants 未补齐也未默认放权（写作 `'[]'`），该 Import 保持不可用、待本地重新授权。管理表初始为空；profile 种子与初始化标记一起提交，不从聊天或审计内容推断信任。
- migration 在取得单实例锁后、监听前完成，单事务失败全回滚；可重复打开且不重导配置。§7、DDL 常量、版本常量、夹具与漂移门禁已在同一变更内同步，「过新」用例使用高于新版本的值（`user_version = 3`）。

### 11.4 实现验收清单

- v1 → v2 升级保留事件、cursor、幂等与审计；升级中途失败回滚，重复打开不改业务记录，过新数据库拒绝打开。
- 两个并发 claim 只有一个成功；确认与过期竞争、拒绝、身份不匹配均不产生多余信任；任一步写入失败不留下半条授权。
- 重启后撤销仍有效、未确认配对不再可用、已配对节点正常重新认证；Node 同时承担两种角色时身份一致且撤销覆盖两者。
- profile 首次导入/空种子/冲突/后续忽略种子，管理修改重启后保留；旧配置不能复活撤销记录。
- 一个 Import 对应多个 Export 的增删原子性、重复关联拒绝、删除后在途收据被拒，审计不随删除消失。
- 注入审计写失败、磁盘满、keystore 写失败、引用提交失败、连接清理失败，验证上述提交边界与可恢复结果。
- 检查所有管理/交付表、日志与错误不含 secret、QR payload 或 imported 正文；本机 workspace 路径不出现在远程 catalog。

### 11.5 身份值对象与读取形状（并入 §3.5/§5.3）

`[已并入]` 值对象在 §3.5（`PeerPublicKey`、`PairingPeer.public_key`），读取面在 §5.3（`device`/`devices`/`node`/`nodes`/`nodes_for`/`peer_key`/`pairing`/`pairing_peer`）。保留的设计理由：

- 构造顺序固定，任一步失败 → `InvalidValue`（§3 的错误边界）：字节长度 == 65 → 首字节 == `0x04` → `p256::PublicKey::from_sec1_bytes` 成功（曲线点有效、非无穷远点）。
- 长度断言**必须**在解析之前：`from_sec1_bytes` 接受 33 字节压缩点，不先断言长度就会绕过「SEC1 uncompressed」合同（`INITIAL_DESIGN.md` §16 第 6 条实测约束）。
- `fingerprint()` 是唯一计算入口：`SHA-256(65 字节原始公钥)` 的 **64 字符小写 hex**（不是 base64url；wire 上的 `base64url32`/`base64url65` 是另一回事，`schemas/node-link/v1/common.schema.json`）。禁止适配器各自现算指纹。
- 私钥、keystore handle、pairing secret 明文**不进** `core::model`（`SECURITY_DESIGN.md` §13.1）；本类型是公开可传输材料。

`[决定]` **`PairingPeer` 携带 `public_key: PeerPublicKey`**（形状见 §3.5）：

- 指纹不再单独存放：`public_key_fingerprint` 由 `public_key.fingerprint()` 派生，避免「指纹与公钥不一致」这一可落库的非法状态。`LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 `publicKeyFingerprint` 字段语义不变（wire 形状不改），只是来源变成派生值。
- 认领事务把公钥写进 `owned_pairing_peer`；确认事务在**同一事务内**把它读出来写入 `owned_peer_key`（§7.3）。因此 WSS 握手不依赖对端重发公钥（`SECURITY_DESIGN.md` §9.4）。
- 负例已固化为常驻测试（`crates/core/src/model/tests.rs`）：33 字节压缩点被拒、长度/前缀/曲线点非法被拒、指纹只能由公钥派生。

`[决定]` **节点角色**复用 `core::model` 已冻结的 `NodeKind`（`crates/core/src/model/identity.rs`，token 为 `"access"`/`"owner"`，与 `LOCAL_ADMIN_PROTOCOL.md` §5.4 的 `NodeRecord.kind` 同名同义）；**不要**新增同义词 `NodeRole`。

- 读取面（§5.3）：`nodes()` 返回两种角色；`node(&NodeId, NodeKind)` 按角色取值；`nodes_for(&NodeId)` 返回该对端的全部角色行。**禁止**「找不到就取第一行」的隐式实现，也禁止用读取顺序表达权威。
- `node.trust.revoked` 按 NodeId 撤销时，同一事务让两种角色一起进入 `revoked`（§11.1 的「按 NodeId 撤销影响两种角色」）；两个角色共享 `owned_peer_key` 的一条身份材料，指纹不一致即 `PortError::Conflict`（§11.6 的 `IdentityMismatch`）。

### 11.6 管理写入 DTO 与端口签名（并入 §5.3）

`[已并入]` DTO（`PendingAudit`/`WriteContext` 与全部写集）与 `TrustStore`/`ExportStore`/`LocalConfigStore`/`CredentialResolver` 的签名在 §5.3；`UseCases` 的管理入口已改为「一次端口调用 = 一个写集（含审计）」。保留的设计理由：

**提交模型**：一次端口调用 = 一个事务 = 一个完整写集（状态 + 引用 + 审计）。禁止用「两次 `await` 共用一个连接池」宣称同一事务（§11.2）。任一约束失败（CHECK、唯一键、外键、条件更新）→ 整事务回滚，映射为 §2 的错误类型后返回；**不得**先提交状态再补审计，也不得在提交后才发现审计写失败。

- `WriteContext.audit` 为空**只允许**用于不作为 `AuditAction` 枚举中已登记安全动作的操作（例如 `workspace.select`、`agent.configure`）；涉及安全动作的写集必须至少带一条成功或失败审计（§9 判据 14 的「每个取值都要有写入用例」）。`provider.configure` 属于已登记安全动作（`provider.configured`），**必须**带审计。
- 被拒绝的请求只尝试追加失败/拒绝审计，不能因审计不可写而继续执行（§11.2 第 6 条）。
- 写集里的集合字段按 §11.1 编码为有类型的 JSON 数组文本；空集合固定写 `[]`（表级 CHECK 依赖这一点）。

`[决定]` **写入 DTO 与端口签名**：形状见 §5.3（`DeviceWrite`/`NodeWrite`/`DeviceRevocation`/`NodeRevocation`/`PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`/`ExpiryWrite` 与 `TrustStore` 的新签名）。读面保留原状并补角色维度，写面把 `upsert_*`/`revoke_*`（携带 `at`）/`claim_pairing`/`settle_pairing` 全部替换为写集。

写集语义（每条都要有对应测试，§11.4 已列验收项）：

1. `put_device`：同 ID 不得换绑公钥（`fingerprint` 与已存行不一致 → `Conflict(IdentityMismatch)`），不得把 `revoked` 改回 `active`（→ `Conflict(IdentityMismatch)`；唯一的复活路径是协议重新配对，见第 4 条）；`scopes` 变化写 `device.scopes_changed`，撤销写 `device.revoked`。
2. `put_node`：写 `owned_node` 行与 `owned_peer_key` 的绑定；同一 NodeId 的两种角色必须指纹一致；撤销过的身份只能按协议重新配对，不能经普通 upsert 激活。
3. `claim_pairing`：并发只有一个成功（唯一 peer 行 + 条件更新）；过期/已终态 → `Conflict(Expired/Consumed)`；本机绑定不一致 → `Conflict(IdentityMismatch)`（§11.2 第 1 条）；插入 `owned_pairing_peer` 与状态推进、`pairing.claimed` 审计同一事务。
4. `settle_pairing`：拒绝或过期**不创建**信任；`Approved` 时创建信任行、把 peer 公钥转入 `owned_peer_key`、更新配对为 `approved`，并按目标族写「信任建立」审计——**设备批准写 `pairing.approved`、节点批准写 `node.paired`**，两族拒绝都写 `pairing.rejected`（`SECURITY_DESIGN.md` §14.2 的最小集合；与 `device.revoked`/`node.trust_revoked` 同款对称）；任一步失败全回滚，绝不出现「返回成功但没有持久信任」。设备配对写下设备行（`active`）；节点配对写下 `owned_node` 的 `access` 行（§11.2 第 2 条的角色推导）；两条路径都要求指纹与既有绑定/角色行一致（同一对端不得换绑公钥，第 1 条），并**允许复活已撤销的身份**：这是 §11.2 第 2 条指定的唯一恢复入口，撤销审计不因复活消失（`revoked_at`/`revoke_reason` 随 `active`/`paired` 清空，历史留在 `device.revoked`/`node.trust_revoked` 审计行里）。
5. `revoke_device`/`revoke_node`：单事务写撤销时间、状态与审计；**提交后**才由组合根关闭适用连接并对管理调用作答（§11.2 第 3 条）。连接清理失败不回滚已提交的撤销。
6. `expire_pairings`：只终结「未确认且 `expires_at <= at`」的行，写 `pairing.expired`；已批准信任不受影响。
7. `put_export`/`revoke_export`/`add_import`/`remove_import`：审计取值分别用 `export.created`/`export.revoked`/`import.added`/`import.removed`（§7.3 与 §7.4 的 `action` CHECK），与状态同事务（§11.2 第 4/5 条）。
8. `consume_pairing`（design D12）：`actor` 必须与 `owned_pairing_peer` 的对端**同类同 id**（不一致 → `Conflict(IdentityMismatch)`），且写集里的主体与每一条 `context.audit` 的归因必须一致（分歧 → `InvalidRequest`，接线错误失败关闭）；只有 `approved` 能推进到 `consumed`（条件更新 `AND state = 'approved'`，`terminal_at` 取本次 `at`，`node.authenticated`/`device.authenticated` 与状态同事务），并在同一事务推进对端节点行的 `last_connected_at`（第 9 条；`actor` 为 `Actor::Node` 时该行是配对批准写下的那个 `access` 行，`Actor::Device` 的配对没有节点行）；已是 `consumed` 且对端一致时幂等成功（**不**覆盖首次 `terminal_at`、**不**重复写审计，也不重复推进 `last_connected_at`——那是同一次消费的重复提交）；其余状态具名拒绍（`expired` → `Expired`、`rejected` → `Consumed`、未批准 → `InvalidRequest`），配对行或对端行缺失分别是 `NotFound`/`Corrupt`。该写集不设容量门：它只推进一个状态、不新增管理行，而且是认证收尾的安全动作（与 `revoke_*`/`expire_pairings` 同类）。
9. `record_node_connected`（`NodeConnectedWrite`，认证成功的收尾写集；新增于 2026-09-26 的 `node-link-owner` 修复轮次）：重复认证（没有待消费配对）把 `(node, kind)` 行的 `last_connected_at` 推进到 `context.at`，并把 `context.audit`（该对端的 `node.authenticated` 成功行）在**同一事务**提交；值只前进不倒退（与 `upsert_node` 的三分支 `CASE` 同口径，§5.3 约束），行不存在 → `NotFound(EntityRef::Node)`。首次认证的同一职责在 `consume_pairing` 的写集里（第 8 条）：两条路径都不得省——`lastConnectedAt` 的语义是「最近一次认证成功时间」，只在首次认证写会让它对后续连接说谎。不设容量门（只更新一个时间列、不新增管理行）；`context.audit` 必须非空（认证成功是安全动作，§11.6 的写集约束）。

`[决定]` **Export/Import 写集**：形状见 §5.3（`ExportWrite`/`ExportRevocation`/`ImportWrite`/`ImportRemoval`）。`ExportStore` 的写面把 `upsert_export`/`revoke_export`/`upsert_import`/`remove_import`（携带 `at`）替换为 `put_export`/`revoke_export`/`add_import`/`remove_import`，读取面不变。

- **两处删除权威不得重叠**：`ImportRemoval` 负责「用户移除 Import」的完整删除；`RemoteDeliveryStore::drop_import`（§7.4）保留「连接级清空投递索引」的既有语义，只删交付索引与命令引用（`UseCases::remove_import` 已撤回为一次 `ImportRemoval`，不再串联两次调用）。
- `ImportWrite.exports` 必须等于 `record.export_ids()`，分歧由存储层返回 `InvalidRequest`（DTO 里两处集合不得各自漂移）。
- `import.add` 的关联行与既有 Import 冲突（重复 ID 或重复 Owner/Export 归属）必须显式冲突，不能静默合并。

`[决定]` **本地配置写集（新端口）**：值对象（`AgentProfile`/`ProviderEnvBinding`/`WorkspaceRecord`/`ProviderRef`/`ProviderRefKind`/`SeedState`/`SecretValue`）在 §3.7，写集 DTO 与 `LocalConfigStore` 的签名在 §5.3。`owned_agent_profile`/`owned_workspace`/`owned_provider_ref` 在此之前**没有任何端口**，因此建表与端口必须同时落地（§11 开头）。保留的设计理由：

- 凭据值只经平台 keystore 的端口（`IDENTITY_AND_AUTH_CONTRACT.md` §7），`ProviderRef` 里只有字段名、引用与版本（§5.3 约束、`SECURITY_DESIGN.md` §13.1）。
- `put_profile` 是唯一写入默认 profile 的入口：至多一个 `default = true`（§7.3 的部分唯一索引），切换默认必须是**一次调用的原子写集**（两条行一起改），不能两次调用。
- 种子语义：`seed_state` 报告「未初始化/已初始化」；`mark_seeded` 与种子 profile 同事务提交（空种子也提交标记）。初始化完成后数据库是唯一权威，不再重导配置。
- 端口纯度不变：`LocalConfigStore` 只使用 §3 的 `core::model` 类型，不出现 `serde_json::Value`/SQL/HTTP（§2）。

`[决定]` **凭据如何到达 Agent 子进程（新端口）**：`provider.configure` 只把凭据写进 keystore，profile 只描述「哪个 Provider 字段注入哪个环境变量」；把二者接起来的是 `CredentialResolver`（签名见 §5.3），由组合根用 keystore 实现并注入 `agent-host`（`agent-host` 不依赖 `identity-auth`，`MODULE_ARCHITECTURE.md` §5）。保留的设计理由：

- 调用时机：只在 `SessionBackendFactory::create`/进程启动前解析；不得把解析结果缓存到磁盘、写进 `owned_*` 表或事件 payload。
- 注入边界：最终传给子进程的环境变量 = `env_allowlist` ∩ `profile.env` 的 `name` 集合，加必要的进程环境（如 `PATH`）；Node/Device 私钥、pairing secret 永不注入（`SECURITY_DESIGN.md` §12.2）。
- `resolve_env` 的返回值不得被 `Debug` 打印：请求/响应日志只记录变量名与数量，不记录值。
- 校验时机：`agent.configure` 在写入 profile 时就校验 `provider_id`/`field`/`name` 的存在性与子集关系（`local.invalid_params`）；运行期再失效只影响**新启动**的进程，不改变已运行会话。

`[已裁定]`（2026-09-23）新增 `ConflictKind` 取值 `AlreadyExists`、`IdentityMismatch`、`DuplicateOwnership`，以及 `UnavailableKind` 取值 `KeystoreUnavailable`（`CredentialResolver` 在 keystore 不可用或引用失效时失败关闭用）：它们是 `core::model` 的公开枚举，已与 §2、`crates/core/src/model/error.rs`（含 `ALL` 与 `as_str`）、`crates/core/src/model/tests.rs` 的 `ALL.len()` 断言同批落地。**必须显式处理的隐性陷阱**：`crates/core/src/broker.rs` 的 `port_error_public` 有 `PortError::Conflict(_) => ("internal.unavailable", …)` 与 `PortError::Unavailable(_) => ("internal.unavailable", …)` 两条通配臂，新增取值不会引发编译错误而会静默落入 `internal.unavailable`；因此本地管理适配器必须把管理冲突**显式**映射为 `local.conflict`、把 keystore 不可用映射为 `local.unavailable`（`LOCAL_ADMIN_PROTOCOL.md` §6），不得依赖那两条通配臂（core 侧已给出显式分支）；将来某个新取值可能出现在命令路径时，同样要在 `port_error_public` 里补显式分支。

### 11.7 管理表 DDL（并入 §7.3/§7.4）

`[已并入]` 九张 `owned_*` 管理表（`owned_device`/`owned_node`/`owned_peer_key`/`owned_pairing`/`owned_pairing_peer`/`owned_export`/`owned_agent_profile`/`owned_workspace`/`owned_provider_ref`）与两个索引在 §7.3，`imported_import` 的新形状与 `imported_import_export` 在 §7.4：两者各自把 `owned_schema_version`/`imported_schema_version` 推进到 2，并由漂移门禁逐条断言。保留的设计理由：

- **禁止跨族外键**（`MODULE_ARCHITECTURE.md` §4.7）：`imported_import_export.owner_node_id` 指向的是 owned 家族的节点记录，因此只存值、不用 FK；存在性由用例层在写集内校验（`import.add` 的前置条件，`LOCAL_ADMIN_PROTOCOL.md` §5.5）。
- **不新增 catalog 投影表**（`[决定]`）：Access 侧的 Node Link catalog 是**连接期内存数据**，不进 SQLite（`NODE_LINK_PROTOCOL.md` §12.3）。`SECURITY_DESIGN.md` §13.4 的可持久化白名单是封闭的，catalog 不在其中；因此 `imported_*` 家族不因它增加新表，也不得为「离线也能改 Import」再加一张。
- **跨两族的列类型约定**：`public_key` 是 STRICT 表的 **BLOB** 列：adapter 必须绑定字节（`Vec<u8>`/`&[u8]`），绑定 hex 或 base64 文本会被 STRICT 直接拒绝。这层拒绝是特性（阻止「同一份材料两种编码」的静默分叉），不是障碍。
- 管理表的集合字段（`*_json`）由 SQLite adapter 按领域构造器校验后编码；身份、状态、时间、唯一键与外键用显式列，不用 JSON blob 承担仲裁（§11.1）。
- `imported_import` 自身升级后仍**不含任何正文列**，并继续纳入 §9 判据 6 的无正文黄金列清单检查。
- `owned_audit`/`imported_audit` 复用既有列集合、不新增内容列；v2 → v3 升级只扩宽 `actor_kind`/`action` 的 CHECK（12-step 重建），因此列清单测试（§9 判据 14）的期望值不变。
- 收口前的一次性现场校验（在 SQLite 上实际执行这 12 条语句并对 16 条约束做正反断言）只是探针惯例（`INITIAL_DESIGN.md` §16），**未入库、也不构成实现完成**；这些断言现在由常驻测试承载：`tests/enum_coverage.rs`（枚举逐值）、`tests/imported.rs` 与 `tests/commit.rs`（黄金列清单）、`tests/migration.rs`（升级保留、幂等、`audit_id`/序列、Import 归属迁移与缺 grants 行为）。

### 11.8 版本常量、migration 与 fixture（并入 §7.2）

`[已并入]` 三个版本常量、升级判据、四步升级过程、保留不变量与夹具清单在 §7.2，可执行 DDL 在 §7.3/§7.4。保留的设计理由：

- **为什么必须做 12-step 重建**：SQLite 不能修改既有 CHECK，而 `owned_audit`/`imported_audit` 的 `action` CHECK 必须接纳 `export.created`/`export.revoked`/`import.added`/`import.removed`/`provider.configured`（`SECURITY_DESIGN.md` §14.2）；重建在同一事务里保留全部行与 `audit_id`，并按升级前的 `sqlite_sequence` 回填序列（审计有 TTL，`seq` 可能领先于 `max(audit_id)`）。列集合不变，因此「黄金列清单」测试（§9 判据 14）的期望值不动。
- **为什么不补 grants**：v1 没有存储 Import 的 grants 来源，凭猜测补齐等于凭空放权；迁移写 `'[]'`（该 Import 保持不可用），把重新授权交回本地管理入口。
- **为什么不提供降级**：v2 库对 v1 二进制是「版本过新」，只能拒绝打开；回滚靠备份 + 回退二进制，不写自动降级迁移（§11.3）。

### 11.9 workspace 解析（并入 §5.1/§3.6）

`[已并入]` `ResolvedWorkspace` 的形状在 §3.6，解析时机、校验、失败分类、别名命名空间与不泄漏规则在 §5.1。保留的设计理由：**alias → 实际路径的解析归 core，不属于任何后端**——`agent-host`/`node-link-client` 都拿不到 `owned_workspace`（`MODULE_ARCHITECTURE.md` §5 矩阵：后端只依赖 `core` + 协议 crate），而 `SECURITY_DESIGN.md` §12.3 要求原始路径只能由 Owner 本地管理入口选择、不得经 wire 传输。UNC/网络路径允许解析但不改变授权模型；其结构化告警落在 `server` 层（core 不引入日志依赖）。
