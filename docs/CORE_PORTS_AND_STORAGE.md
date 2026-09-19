# core 端口与 SQLite 持久化 v1 合同

> 状态：编码前契约（v1），实现前冻结；`core`/`storage-sqlite` 的 L1 切片已开始实现（`core::model` 与 `core::ports` 已落地，见 §10）
> 版本：0.3（v0.2 的 L1 实现期修订：补齐交互**创建**路径、正文读取入口 `ReadView::event_payload`、非会话级事件的 `origin_*` 列改为可空、`owned_attachment` 增加 id 列、`find_remote_request` 改名、Windows ACL 判定说明；并把 §5.2 里不存在的 `resolve_interaction` 方法改为 §6 第 13 条）
> 版本：0.4（2026-09-18，L1 收尾：补齐四条会阻塞「一个真实 turn」的缺口——§6 第 14 条 `agent.message.completed` 的生成、第 15 条 delta 压缩与 `turn.delta_compacted` 收据、第 16 条启动恢复 `RecoverUnsettled`、第 17 条 `session.mode.list` 的候选来源；新增 `SessionEndpoint::modes()`、`SessionStore::unsettled_commands`、`OwnedCommit.compacted`、`MessageId`/`ModeState`，并给 §9 加判据 18–21）
> 版本：0.5（2026-09-18：§5.4 的 `EventSink` 更正为包装结构，与 §5.1 的 `[决定]` 和 `ports.rs` 一致；新增 `scripts/check-contract-drift.mjs`，把 §7/§5 与实现的漂移变成 `npm run check` 的第八个门禁）
> 日期：2026-09-18
> 上位文档：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.1/§4.7/§5/§6/§7/§8/§10、[INITIAL_DESIGN.md](./INITIAL_DESIGN.md) §5/§10、[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §3/§9/§10/§11/§14、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §6/§7/§12/§15、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §13/§14/§15、[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §4/§5/§6、[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5
> 作用：冻结 `core::model` 值对象、`core::use_cases` 用例面、`core::ports` 端口签名、broker 事务顺序与 `storage-sqlite` 的 v1 表结构、保留/清理与 migration。**本文件是这些内容的唯一权威来源**；`MODULE_ARCHITECTURE.md` §4.1/§4.7 只保留职责边界。
> 标记约定：`[决定]` = 本合同新定且不改变既有协议语义；`[待确认]` = 触及协议或产品语义，需用户确认；`[open]` = 明确留到实现阶段。

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
pub enum ConflictKind { VersionMismatch, AlreadyResolved, AlreadyClaimed, Expired, Consumed, IdempotencyConflict }
pub enum UnavailableKind { Busy, StorageFull, IoError, RemoteUnavailable, OwnerOffline, ExportRevoked }
```

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
| `EntityRef` | enum `Session(SessionId) ｜ Turn(TurnId) ｜ Interaction(InteractionId) ｜ Command{ session: Option<SessionId>, request: RequestId } ｜ Pairing(PairingId) ｜ Device(DeviceId) ｜ Node(NodeId) ｜ Export(ExportId) ｜ Import(ImportId)` | 本合同（错误定位） |

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
| `Actor` | enum `Device { device: DeviceId, scopes: ScopeSet } ｜ Node { node: NodeId, access_node: NodeId } ｜ LocalCli`；`ScopeSet` 为 scope 名集合 | `MODULE_ARCHITECTURE.md` §5；`SECURITY_DESIGN.md` §10.2 |
| `DeviceRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3（含 `deviceId`、指纹、`scopes`、状态、时间戳） | 同左 |
| `NodeRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.4（含 `nodeId`、指纹、`grants`、`state`、`ownerEndpoint`） | 同左 |
| `PairingRecord` | `{ id: PairingId, target: PairingTarget(Device｜Node), state: PairingState, display_name: Option<String(1..=128)>, requested_scopes: ScopeSet, requested_grants: GrantSet, secret_digest: Digest, created_at, expires_at, claimed_at: Option, approved_at: Option, terminal_at: Option }`；构造校验：`claimed_at` 非空 ⟺ 状态不是 `created`；`approved_at` 非空 ⟺ `approved`/`consumed`；`terminal_at` 非空 ⟺ `rejected`/`expired`/`consumed`；**设备配对不得带 grants、节点配对不得带 scopes**。`secret_digest` 是 pairing secret 的 SHA-256——明文只存在于创建方内存（`SECURITY_DESIGN.md` §13.1） | 本合同（**首个实现已冻结**，见 `crates/core/src/model/identity.rs`）；方法形状见 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 |
| `PairingState` | enum `Created｜Claimed｜PendingConfirmation｜Approved｜Rejected｜Expired｜Consumed`；终态 = `Rejected｜Expired｜Consumed`；`Claimed` **不对外可见**（只在服务端事务与审计里出现） | `SYNC_PROTOCOL.md` §7.0 |
| `PeerIdentity` | enum `Device(DeviceId)｜Node(NodeId)`；`kind()` 给出 "device"/"node" | 本合同 |
| `PairingPeer` | `{ id: PeerIdentity, display_name: String(1..=128), public_key_fingerprint: Fingerprint, client_nonce: Nonce }` | `SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2 |
| `PairingClaim` | `{ pairing: PairingId, peer: PairingPeer, requested_scopes: ScopeSet, requested_grants: GrantSet }`——HMAC/proof 由**调用方**验证，进入本类型时只剩已核对的事实；设备配对不对带 grants、节点配对不得带 scopes | 本合同（`TrustStore::claim_pairing` 的输入） |
| `PairingSettlement` | enum `Approved { granted_scopes: ScopeSet, granted_grants: GrantSet }｜Rejected { reason: Option<String(≤256)> }`；`granted_*` 是**用户确认的最终集合**（不是请求值）；`reason` 是简短原因，不进审计正文 | 本合同（`TrustStore::settle_pairing` 的输入） |
| `ExportRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 `ExportView`（`exportId`、`agentIds`、`workspaceAliases`、`defaultWorkspaceAlias`、`templates`、**`scopes`**、`cachePolicy`、`createdAt`、`revokedAt`） | 同左（字段名已按本次决定与 Node Link 对齐） |
| `ImportRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 `ImportRecord`（`importId`、`ownerEndpoint`、`ownerNodeId`、`exportIds`、`grants`） | 同左 |
| `AuditRecord` | `{ at: Timestamp, action: AuditAction, actor: Actor, via_node: Option<NodeId>, local_principal_ref: Option<String(≤128)>, target: EntityRef, outcome: AuditOutcome, detail_digest: Option<Digest> }`；**不含任何内容** | `SECURITY_DESIGN.md` §14.2 |
| `AuditAction` | 闭合枚举：`PairingCreated｜PairingClaimed｜PairingApproved｜PairingRejected｜PairingExpired｜DeviceAuthenticated｜DeviceAuthFailed｜DeviceRevoked｜DeviceScopesChanged｜NodePaired｜NodeTrustRevoked｜NodeIdentityChanged｜AuthorizationDenied｜RateLimitTriggered｜StorageIntegrityFailed` | `SECURITY_DESIGN.md` §14.2 |
| `AuditOutcome` | enum `Success｜Denied｜Failed` | 本合同 |

### 3.6 后端与能力

| 类型 | 形状 | 出处 |
|---|---|---|
| `AgentDescriptor` | `{ agent: AgentRef, available: bool, origin: ResourceOrigin }` | `MODULE_ARCHITECTURE.md` §4.1 |
| `Capability` / `CapabilitySet` | `Capability { kind: String(1..=128), detail: Option<String(≤256)> }`；`CapabilitySet` 为去重集合 | `ACP_COMPATIBILITY_MATRIX.md` §4 |
| `CreateSessionRequest` | `{ agent: AgentRef, workspace_alias: Option<WorkspaceAlias>, template: Option<TemplateSelection>, origin: ResourceOrigin }` | `NODE_LINK_PROTOCOL.md` §12.7 |
| `TemplateSelection` | `{ template_id: String(1..=128), params: Vec<(String, ConfigValue)> }` | `NODE_LINK_PROTOCOL.md` §12.3 |
| `PromptRequest` | `{ content: Vec<PromptContentBlock> }`（形状见协议 crate 的 `promptContentBlock`） | `SYNC_PROTOCOL.md` §11.5 |
| `EndpointEvent` | `{ kind: EventKind, event_type: EventType, payload: EventPayload, turn: Option<TurnId>, causation: Option<RequestId>, at: Timestamp }`（`SessionEndpoint` 的输出流元素） | 本合同 |

## 4. `core::use_cases` 用例面

`[决定]` **每个**入口的第一个参数都是 `actor: &Actor`：授权只在 core 判定，`server` 不得传入“已授权”标志，也不得代替 core 判定（`AGENTS.md` §3/§4、`SECURITY_DESIGN.md` §8）。全部 mutation 只同步接受。

| 用例族 | 入口 | 调用方 |
|---|---|---|
| `SessionCommands` | `submit_command(actor, ClientCommand) -> CommandReceipt` | `server::sync`、`server::node_link`、`server::acp_facade` |
| `SessionQueries` | `list_sessions(actor, SessionQuery) -> Vec<SessionSummary>`、`read_session(actor, ReadQuery) -> HistoryPage` | 同上 |
| `ConfigCommands` | `set_mode(actor, SessionReference, ModeId) -> Version`、`set_config(actor, SessionReference, ConfigOptionId, ConfigValue) -> Version` | 同上 |
| `PermissionCommands` | `resolve_interaction(actor, SessionReference, InteractionId, InteractionResolution) -> Resolution` | 同上 |
| `SubscriptionQueries` | `replay(actor, SessionReference, Option<GlobalCursor>, ReplayLimit) -> ReplayBatch`、`retention_window(actor, SessionId) -> Option<(Sequence, Sequence)>` | `server::sync`、`server::node_link` |
| `DeviceManagement` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 `device.*`/`node.*` | `server::local_admin`、`app::cli` |
| `ExportManagement` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 的 `export.*`/`import.*` 写方法 | 同上 |
| `RemoteCatalogQueries` | 见 §5.5 读取方法与 `NODE_LINK_PROTOCOL.md` §12.3 catalog 投影 | 同上、`server::node_link` |

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
- `[决定]` `read_history` 的分流：owned 由 `storage-sqlite` 从事件日志回答；imported 由 `node-link-client` 在线回源 Owner，Owner 不可达返回 `PortError::Unavailable(RemoteUnavailable)`。

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
- `[决定]` `origin_epoch` 由 **core** 在创建会话时用 `IdGenerator` 生成并传入（响应审查：存储层返回它会让无创建需求的提交也必须回读）；存储层只校验“该会话已有 epoch 时必须一致”。
- `[决定]` 幂等命中返回 `CommitOutcome::replayed`，不追加事件、不改状态。
- `[决定]` 交互的创建与解析规则见 §6 第 13 条。`SessionStore` **没有** `resolve_interaction` 方法：解析是 `OwnedCommit.state.interaction` 的一部分；`SessionEndpoint::resolve_interaction` 是后端（Agent）侧入口，不落盘。
- `[决定]` `retention_window` 返回该会话仍可重放的 `session_sequence` 下界/上界；broker 据此决定 `sync.reset_required`（`reason` 枚举 `initial_sync|epoch_mismatch|cursor_expired|cache_incompatible`，`SYNC_PROTOCOL.md` §9.4）；cursor 的四种拒绝原因：格式非法 → `malformed`（协议层）、`serverEpoch` 与 `meta.server_epoch` 不符 → `epoch_mismatch`、超出 `head()` → `beyond_head`、低于窗口下界 → `cursor_expired`（`SYNC_PROTOCOL.md` §9.2）。

### 5.3 信任、Export、审计与附件

```rust
#[async_trait]
pub trait TrustStore: Send + Sync {
    async fn upsert_device(&self, record: DeviceRecord, at: Timestamp) -> Result<(), PortError>;
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError>;
    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError>;
    async fn revoke_device(&self, id: &DeviceId, at: Timestamp, reason: RevokeReason) -> Result<(), PortError>;
    async fn upsert_node(&self, record: NodeRecord, at: Timestamp) -> Result<(), PortError>;
    async fn node(&self, id: &NodeId) -> Result<Option<NodeRecord>, PortError>;
    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError>;
    async fn revoke_node(&self, id: &NodeId, at: Timestamp, reason: RevokeReason) -> Result<(), PortError>;
    async fn create_pairing(&self, pairing: PairingRecord) -> Result<(), PortError>;
    /// 单一事务的原子认领：存在、未过期、仍为 created、host/origin 匹配（HMAC 由调用方验证）。
    async fn claim_pairing(&self, claim: PairingClaim, at: Timestamp) -> Result<PairingClaimOutcome, PortError>;
    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError>;
    async fn settle_pairing(&self, id: &PairingId, settlement: PairingSettlement, at: Timestamp) -> Result<TrustRecordRef, PortError>;
    async fn expire_pairings(&self, at: Timestamp) -> Result<u64, PortError>;
}

#[async_trait]
pub trait ExportStore: Send + Sync {
    async fn upsert_export(&self, export: ExportRecord, at: Timestamp) -> Result<(), PortError>;
    async fn export(&self, id: &ExportId) -> Result<Option<ExportRecord>, PortError>;
    async fn exports(&self) -> Result<Vec<ExportRecord>, PortError>;
    async fn revoke_export(&self, id: &ExportId, at: Timestamp) -> Result<(), PortError>;
    async fn upsert_import(&self, import: ImportRecord, at: Timestamp) -> Result<(), PortError>;
    async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError>;
    async fn imports(&self) -> Result<Vec<ImportRecord>, PortError>;
    async fn remove_import(&self, id: &ImportId, at: Timestamp) -> Result<(), PortError>;
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append(&self, record: AuditRecord) -> Result<(), PortError>;
    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditRecord>, PortError>;
}

#[async_trait]
pub trait AttachmentStore: Send + Sync {
    /// 写入内容寻址的附件字节，返回其 sha256 与相对路径。
    async fn put(&self, bytes: &[u8], media_type: &str, at: Timestamp) -> Result<AttachmentRef, PortError>;
    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError>;
    async fn link(&self, session: &SessionId, attachment: &AttachmentId, generation: AttachmentGeneration) -> Result<(), PortError>;
    /// 按 LRU 清理到给定字节预算以下；返回被删除的附件。
    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError>;
    /// §7.5 的孤儿回收（§6 第 18 条）：删除 `storage.attachment_dir` 下**不在 `owned_attachment` 表里**
    /// 且 `mtime` 早于 `at` 的文件，每次最多 `limit` 个，返回实际删除数。
    async fn sweep_orphans(&self, at: Timestamp, limit: u32) -> Result<u32, PortError>;
}
```

约束：

- `[决定]` 凭据（Provider、Node key、pairing secret）**不经**任何上述端口：只存平台 keystore，表里最多存引用与指纹（`SECURITY_DESIGN.md` §13.1）。
- `[决定]` `AuditRecord.action` 是 §3.5 的闭合枚举；审计行不得包含内容（`SECURITY_DESIGN.md` §14.2），并由 §9.13 的机器检查兜底。

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
5. `[决定]` 授权调用点：所有用例入口先按 `actor` 的 scope/grant 与 Export 交集判定；core 不信任 adapter 的“已授权”标志。
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

## 7. `storage-sqlite` v1 表结构

### 7.1 文件、PRAGMA、连接与权限

- 单文件 `<data_dir>/acp-remote.sqlite3`；附件目录 `<data_dir>/attachments`（`[决定]` 新增配置键 `storage.attachment_dir`，默认 `<data_dir>/attachments`，语义与权限同数据库目录）。
- `[决定]` 启动设置：`journal_mode = WAL`、`synchronous = FULL`、`foreign_keys = ON`、`busy_timeout = 5000`、`wal_autocheckpoint = 1000`。
- `[决定]` 连接模型：1 个写连接（串行化写事务）＋ N 个只读连接；`read_view()` 打开一个显式只读事务（`BEGIN`）供快照使用。
- `[决定]` 权限：Unix 目录 `0700`、数据库/WAL/`-shm`/附件 `0600`；启动时检查，正式模式失败关闭（`SECURITY_DESIGN.md` §13.2）。
- `[决定]` **Windows**：ACL 读取不可用（workspace 禁 `unsafe`，也没有 ACL 封装依赖），判定返回 `Unverifiable` 且**不**失败关闭——只记录一次结构化警告；数据目录默认位于用户 profile 下并继承用户专属 ACL。Unix 仍按真实模式位判定并失败关闭（§9 判据 12 的合成视图单测在两平台都跑）。
- `[决定]` 启动 `PRAGMA quick_check`；失败进入只读失败关闭（写路径返回 `PortError::Corrupt`）。
- `[决定]` WAL 检查点与备份：关闭时执行一次 `wal_checkpoint(TRUNCATE)`；「直接拷贝主库文件而不带 `-wal`/`-shm` 不安全」的运维提醒落在 `CONFIG_REFERENCE.md` §5 的 storage 段，`SECURITY_DESIGN.md` §13.2 用一行链接指向它（两份文档各自只保留一处正文）。

### 7.2 Migration

- `[决定]` `PRAGMA user_version` = 文件格式版本（v1 = 1）；`meta` 保存两族 schema 版本：`owned_schema_version`、`imported_schema_version`。
- `[决定]` 两族 migration 分开维护；单事务、可重复执行、失败整体回滚；发现版本高于本二进制已知版本 → 拒绝启动。
- `[决定]` 迁移测试资产：`fixtures/storage/v1/empty.sqlite3`（`user_version = 1`，两族版本 1）与 `fixtures/storage/v1/too-new.sqlite3`（`user_version = 2`）。

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
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
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
  export_id     TEXT NOT NULL,
  display_name  TEXT,
  endpoint_ref  TEXT,
  cache_policy  TEXT NOT NULL CHECK (cache_policy = 'no-content-cache'),
  owner_server_epoch TEXT,          -- Owner 的 serverEpoch；变化即说明对方重建了事件库
  created_at    TEXT NOT NULL,
  removed_at    TEXT,
  UNIQUE (owner_node_id, export_id)
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
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id     TEXT NOT NULL,
  owner_node_id TEXT, export_id TEXT, session_id TEXT,
  request_id   TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX imported_audit_at ON imported_audit(at);
```

约束与由来（含本轮修订）：

- 字段集合 = `NODE_LINK_PROTOCOL.md` §6 的无正文索引 + cursor/ACK/requestId/命令终态引用/local sequence 映射；`imported_session` 只承载 `session.list`/快照所需的摘要元数据（`SYNC_PROTOCOL.md` §9.6 的「快照只含元数据」）。
- `[决定]` 白名单归属：`imported_import` → import 配置与 owner/origin 引用；`imported_session` → cursor/ACK、local sequence 映射、摘要元数据、当前连接的 attachment 凭据；`imported_delivery_index` → owner/origin 引用、event type/digest、local sequence 映射；`imported_command_ref` → requestId 与命令终态引用；`imported_audit` → 不含内容的审计元数据（已确认：`SECURITY_DESIGN.md` §13.4 的白名单已显式加入该项）。
- `[决定]` `imported_*` 禁止出现正文语义列；机器检查用**黄金列清单**逐表比对（§9.6），而不是子串黑名单（子串黑名单会被 `snapshot_json` 之类的新列绕过）。
- `[决定]` `imported_delivery_index`/`imported_command_ref` 对 `imported_session` 建复合外键：会话行不存在时 `commit_receipt` 返回 `PortError::NotFound`，不得静默推进 `next_local_sequence`。
- `[决定]` `attachment_id`/`attachment_generation` 断开即清空；`owner_server_epoch` 变化时清空该 import 的交付索引（对方重建了事件库，cursor 失效）。
- `[决定]` `drop_import` 只删交付索引与命令引用，**不删** `imported_audit`（审计保留义务）。

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

`[决定]` **②/③ 的谓词必须排除仍被引用的事件行**：`owned_interaction.request_event` 是 `INTEGER REFERENCES owned_event(global_sequence)` 且 `foreign_keys = ON`，所以 ② 必须带 `AND NOT EXISTS (SELECT 1 FROM owned_interaction i WHERE i.request_event = owned_event.global_sequence)`，并且 ②′ 必须排在 ② 之前。否则超限时 `enforce_capacity` 里的 ② 会撞外键、把 `FOREIGN KEY constraint failed` 当成 `PortError::Backend` 抛给上层（而不是 §7.5⑥ 的 `StorageFull`），`prune` 的整个事务也会回滚、连 ① 的 delta 清理都做不成。

`[决定]` **幂等/终态行不参与 TTL 清理**：`owned_command` 的 mutation 行与 `status='uncertain'` 行保留到「目标会话被删除」或「actor 被撤销」，与 `SYNC_PROTOCOL.md` §11.2 的「至少保留到设备被撤销且目标会话被删除」一致。

`[决定]` 清理不得破坏承诺期内的重放：窗口前移后 `retention_window` 必须反映新下界，broker 据此要求客户端重建 snapshot。

## 8. 失败关闭

- `[决定]` `Corrupt` 或权限检查失败 → 写路径全部拒绝、只读查询继续、`server` 以 `internal.unavailable` 返回并保持明确日志（`SECURITY_DESIGN.md` §15）。
- `[决定]` 错误消息不得包含 SQL 文本、完整 prompt、密钥或 ACP raw；日志字段 allowlist 由 `SECURITY_DESIGN.md` §14.1 决定。
- `[决定]` 失败关闭必须有测试：用损坏的数据库文件（或注入 `integrity_ok = false`）打开存储 → 写路径全部返回 `PortError::Corrupt`、只读查询仍可用（§9 未单列判据，但 §8 的两条规则都要有可观察用例）。

## 9. 验收判据（实现该合同的测试）

1. **migration**：用 `fixtures/storage/v1/empty.sqlite3` 启动 → 版本不变、无错误；连续两次启动后 `PRAGMA user_version`、`meta.*_schema_version` 与 `sqlite_master` 里每条 SQL 文本**逐字节相同**（幂等）；打开 `fixtures/storage/v1/too-new.sqlite3` → 返回具名错误且不写入任何行。
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
13. **端口纯度**：`cargo tree -p core --edges normal` 的输出与冻结 allow-list 逐行相等（黄金文件）；`cargo public-api -p core` 快照不得出现 §3/§5 之外的类型（该半条需要 nightly toolchain + 外部 `cargo-public-api`，未安装时应记录为**未执行**并说明替代判据，不得声称已通过）。
14. **审计**：`action` 只能取 §3.5 的枚举（表级 CHECK + 用例层枚举，**每个取值都要有写入用例**）；`owned_audit` 与 `imported_audit` 都要有「黄金列清单」测试（逐列 `PRAGMA table_info` 比对，新增内容列即失败）。
15. **交互创建**：一次 `commit` 写入 `interaction` 事件 + `PendingInteractionWrite` 后，`owned_interaction` 恰好一行且 `request_event` 等于**配对事件**的 `global_sequence`（列类型见 §7.3；§9 判据 10 的悬空引用为 0）；装配方传入的任何占位值都不得入库；同一提交里 `interactions` 与 `state.interaction` 同时出现 → `InvalidRequest`；随后 `HistoryInclude.pending_interactions` 读回的行 `options` 为空，而按事件流取到配对事件的 `id` 后 `ReadView::event_payload(id)` 能还原出非空 `options`。
16. **正文读取**：`ReadView::event_payload` 返回的 `view` 文本与库内 `payload_json` **逐字节**相同（含未知字段与嵌套），`AcpRaw::Available.raw_json` 与写入时逐字节相同；原文被清理过的行返回 `AcpRaw::Unavailable`，且 `reason`、`byte_length`、`sha256` 三者都必须与行内列一致（**摘要不得因为原文被清理而丢失**——`acp_sha256` 与 `acp_raw_json` 只有在没有不可用原因时才同有同无）；不存在的事件 id 返回 `None`。
17. **非会话级事件**：`session: None` 的提交落库后 `session_sequence`/`origin_epoch`/`origin_sequence` 三列都是 NULL，且该行仍出现在 `replay` 流里；`session: Some` 的事件三列都非 NULL（成对 CHECK 不得被绕过）。
18. **消息收尾**（§6 第 14 条）：正常结束、取消、失败三种终态各一例；同一 turn 内两个 `messageId` 各自收尾；**没有** delta 的 turn 不产生 `agent.message.completed`；`content` 的文本等于该消息 delta 文本按 `deltaIndex` 的拼接；`completed` 与该 turn 的终态事件在**同一次提交**（用装饰 store 断言批次）；`agent.thought.delta` 不产生 `completed`。
19. **压缩**（§6 第 15 条）：`persist_deltas = false` 时 turn 终态后出现恰好一条 `turn.delta_compacted`，被压行的 `compacted_into` 等于该行的 `global_sequence`，重放里不再出现那些 delta（summary 只带 `turnId`/`deltaCount`）；turn 未终态时**不**压缩；`persist_deltas = true` 时永不压缩；`compacted` 里塞入非 delta 行或跨会话 cursor → `InvalidRequest` 且零写入。
20. **启动恢复**（§6 第 16 条）：造 `status='accepted'` 且无终态事件的 mutation 行 + 未终态 turn → 恢复后该行 `status='uncertain'`、`terminal_event_id` 非空且指向存在的 `command.uncertain` 事件，对应 turn 为 `failed`；`unsettled_commands` 之后返回空；广播发生在提交之后（`EventPublisher` 不得先于 `commit`）。
21. **`session.mode.list`**（§6 第 17 条）：端口返回的候选列表原样出现在结果里；`currentModeId` 与会话 `current_mode` 一致；`version` 等于会话版本；端口返回空列表时结果为空（不伪造）。
22. **附件事务边界与孤儿回收**（§6 第 18 条）：`prune_lru` 删行后崩溃（模拟：删行后不执行文件删除）→ 库内无悬空行；`sweep_orphans(启动时刻, n)` 删掉表外文件、**不删** `mtime ≥ 启动时刻` 的文件、遵守 `limit`、返回实际删除数；回收失败不阻止启动（有警告）。

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
- `[已裁定]` `owned_interaction.elicitation_action` 的 CHECK 补 `'decline'`（原值是 `submit|cancel` 两值，会让 ACP 早就有的 `decline` 在解析事务里撞约束、交互永远停在 `pending`）；同时要求 DDL 里每个 `IN (...)` 枚举字面量与 core 枚举的 wire 值逐条一致，并有一条「每个枚举值都能写入并读回」的测试。
- `[已裁定]` id 分配收敛为两处权威（§3.1）：`SessionId`/`EventId` 归存储层事务内分配，`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId` 归 core 的 `IdGenerator`，`AttachmentId` 归 `AttachmentStore::put`。`IdGenerator` 因此删掉 `session_id()`/`attachment_id()`——原 §3.1 与 §5.4 的写法互相矛盾，实现者只能二选一。
- `[已裁定]` `SessionBackendFactory::create` 增加 `session: &SessionId` 参数：§5.1 原先承诺「core 把分配好的会话传给 `create`」，而 §3.6 的 `CreateSessionRequest` 字段表里没有 `session`，后端因此拿不到 id、无法构造 `SessionEndpoint::reference()`。
- `[已裁定]` `templateId` 的权威是 `schemas/node-link/v1/common.schema.json` 的 `^[A-Za-z0-9._-]{1,128}$`（§3.6 也写的 1..=128）：`docs/LOCAL_ADMIN_PROTOCOL.md` §5.5 与 core 的 `is_template_id` 原写 64，按 wire 取 128（否则跨节点会拒收 schema 合法的模板 id）。
- `[已裁定]` `payload_digest` 由**存储层**用 `acpr-wire` 的 ACPR-CJ1 实现从 `payload_json` 重算（§7.3/§9 判据 9）：原实现直接哈希库内字节，而 `payload_json` 按判据 16 原样保留调用方字节（不重排键），两者只有在调用方恰好传了规范 JSON 时才相等——跨节点复算与 imported 去重都会失配。ACPR-CJ1 是跨 Sync/Node Link 的共用机制，因此实现放 `acpr-wire`（叶子 crate），并与 JS 参考实现在同一批 fixture 上互校。
- `[已裁定]` 清理顺序补 ②′（终态交互行必须先删）且 ②/③ 谓词必须排除仍被 `owned_interaction.request_event` 引用的行；容量度量补 `imported_*` 家族分量（否则 Access-only 节点度量恒 0、永不回收）；⑤ 的审计清理对 `owned_audit` 与 `imported_audit` 都生效。三条都是实测缺陷：外键会让 `prune` 整事务回滚、并让超限提交报 `Backend` 而不是 `StorageFull`。
- `[已裁定]` `PendingEvent` 增加必填 `origin: EventOrigin`（§3.4）：原写入形状没有 origin，存储层只能按「有没有会话」猜出 `agent`/`daemon`，等于伪造 `origin.kind`。
- `[open]` 交互解析的崩溃窗口（「行已终态、`*.resolved` 事件未落盘」）的补偿机制：当前按 §6 第 13 条接受；若将来要消除，需要「解析意向」行或两阶段提交。
- `[已裁定]` **附件文件删除的事务边界与孤儿回收**（§6 第 18 条）：行删除与事务同提交、文件删除在提交之后；孤儿回收由组合根启动时调用一次 `AttachmentStore::sweep_orphans(启动时刻, 1000)`，只删「不在表里且 mtime 早于本次启动」的文件，失败不阻止启动。新增该端口方法（§5.3）。
- `[open]` 未来加密离线正文缓存（必须新 feature + Owner 明示授权 + ADR；本合同不预留任何静默开关）。
