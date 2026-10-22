# core 端口与 SQLite 持久化合同

> 版本：0.20（2026-10-17，`retire-export-import-and-node-grants`：删除 Export/Import 整族与 `grant.*` 授权词汇；`owned_export`/`imported_import`/`imported_import_export` 删除，`owned_node` 去掉 `grants_json`/`export_ids_json`，`imported_*` 按 `(owner_node_id, session_id)` 重建；存储版本 v11 / imported v4。**升级不可逆**，回滚只能靠升级前备份。
> 状态：编码前契约（v1），实现前冻结；`core`/`storage-sqlite` 的 L1 切片已开始实现（`core::model` 与 `core::ports` 已落地，见 §10）
> 版本：0.3（v0.2 的 L1 实现期修订：补齐交互**创建**路径、正文读取入口 `ReadView::event_payload`、非会话级事件的 `origin_*` 列改为可空、`owned_attachment` 增加 id 列、`find_remote_request` 改名、Windows ACL 判定说明；并把 §5.2 里不存在的 `resolve_interaction` 方法改为 §6 第 13 条）
> 版本：0.4（2026-09-18，L1 收尾：补齐四条会阻塞「一个真实 turn」的缺口——§6 第 14 条 `agent.message.completed` 的生成、第 15 条 delta 压缩与 `turn.delta_compacted` 收据、第 16 条启动恢复 `RecoverUnsettled`、第 17 条 `session.mode.list` 的候选来源；新增 `SessionEndpoint::modes()`、`SessionStore::unsettled_commands`、`OwnedCommit.compacted`、`MessageId`/`ModeState`，并给 §9 加判据 18–21）
> 版本：0.5（2026-09-18：§5.4 的 `EventSink` 更正为包装结构，与 §5.1 的 `[决定]` 和 `ports.rs` 一致；新增 `scripts/check-contract-drift.mjs`，把 §7/§5 与实现的漂移变成 `npm run check` 的第八个门禁）
> 日期：2026-09-18
> 上位文档：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.1/§4.7/§5/§6/§7/§8/§10、[INITIAL_DESIGN.md](./INITIAL_DESIGN.md) §5/§10、[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §3/§9/§10/§11/§14、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §6/§7/§12/§15、[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §13/§14/§15、[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §4/§5、[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5
> 作用：冻结 `core::model` 值对象、`core::use_cases` 用例面、`core::ports` 端口签名、broker 事务顺序与 `storage-sqlite` 的 v2 表结构、保留/清理与 migration。**本文件是这些内容的唯一权威来源**；`MODULE_ARCHITECTURE.md` §4.1/§4.7 只保留职责边界。
> 标记约定：`[决定]` = 本合同新定且不改变既有协议语义；`[待确认]` = 触及协议或产品语义，需用户确认；`[open]` = 明确留到实现阶段。
> 修订（2026-09-23）：§11.5–§11.9 的形状已并入 §3.5/§3.6/§3.7/§5.1/§5.3/§7.2/§7.3/§7.4，§11 改为「形状已并入」索引（只留设计理由）；`scripts/check-contract-drift.mjs` 对新增端口签名与 DDL 逐条断言。`storage-sqlite` 的管理 store 落盘实现已落地（`crates/storage-sqlite/src/admin/`，写集一事务提交、失败关闭与容量纳入）。**Daemon/CLI 接线与 `server`/`app` 的入站适配器已随切片 4 `daemon-cli-and-local-admin` 落地**（`server` 的本地管理与 `app` 组合根/CLI，见 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9/§4.10；`identity-auth`/`identity-keystore` 两个身份 crate 已于 2026-09-24 落地，见 §4.8/§4.12）；仍未实现的是 `server::sync`/`server::acp_facade` 与 `node-link-client`（`server::transport::net` 与 `server::node_link` 的 Owner 侧入站面已随切片 5 `node-link-owner` 落地，见 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9）。在远程与网络适配器落地前，不能把合同检查或存储测试通过解释为配对、撤销或本地配置已经端到端可用。
> 版本：0.6（2026-09-23：§11 从「表设计 + 要求」补成可实现合同——新增 §11.5 身份值对象与读取形状、§11.6 管理写入 DTO 与端口签名（目标形状）、§11.7 管理表 DDL（目标形状）、§11.8 版本常量/migration/fixture 约定。**§11.5–§11.8 是 `[待实现]` 的目标形状**：它们不写入 §5/§7，因为 `scripts/check-contract-drift.mjs` 把 §5 的 ```rust 块与 `crates/core/src/ports.rs`、§7 的 ```sql 块与 `crates/storage-sqlite/src/migrate.rs` 逐条绑定；实现变更必须把这些形状并入 §5/§7 并让漂移门禁断言，在此之前不得只加表就声明管理状态可用）
> 版本：0.7（2026-09-23：§11.5–§11.9 落地——§5.3 换为写集端口（`WriteContext`/`PendingAudit` + 全部写集 DTO + `TrustStore`/`ExportStore` 新签名 + `LocalConfigStore`/`CredentialResolver`），§3.5/§3.6/§3.7 补 `PeerPublicKey`、`ResolvedWorkspace` 与本地配置值对象，§7 升级为 v2 表结构（9 张管理表 + 2 个索引 + `imported_import` 拆分），§7.2 新增 v1 → v2 迁移规则与 v2 夹具，§9 增补判据 23–29，§11 改为索引）
> 版本：0.9（2026-09-23：§2 的错误枚举补齐 `ConflictKind::{AlreadyExists, IdentityMismatch, DuplicateOwnership}` 与 `UnavailableKind::KeystoreUnavailable` 及到 `local.conflict`/`local.unavailable` 的映射义务；§11.6 写集语义第 4 条按目标族区分落定审计（设备 `pairing.approved` / 节点 `node.paired`）；签名、判据与 DDL 未变）
> 版本：0.8（2026-09-23：管理 store 的落盘实现落地（`crates/storage-sqlite/src/admin/`）后，把 §5.3/§9/§11 与关联文档里「仍待实现」的陈述改为与实现一致；合同形状、判据与 DDL 未变）
> 版本：0.10（2026-09-23：补齐管理存储的**终态与单调性守卫**——§5.2 补 imported 写路径的归属前置（`upsert_session`/`commit_receipt` 先验 `(ownerNodeId, exportId)` 归属，否则 `NotFound(Export)` 且零写入）、§5.3 补 `put_export` 撤销终态 / 身份材料读取核对同行指纹 / 活动时间只前进（显式 `CASE`）、§7.4 补迟到回调拒绝与「重导入后无法区分新旧连接」的已知边界、§9 新增判据 30。**§5/§7 的代码块、端口签名与 DDL 未变**，漂移门禁继续逐条成立；wire 协议、封闭词表与本地管理方法集未变）
> 版本：0.11（2026-09-24：§10.3 的 view 收口落地——§5.1 写明 `TurnAccepted.turn` 是适配器侧占位/审计值（turn 归属由 core 定稿），§6 新增第 19 条（提交前注入 `turnId` 与会话 `version`、冲突与漂移失败关闭、imported 路径保留 Owner 取值），§9 新增判据 31；端口签名与 DDL 均未变）
> 版本：0.12（2026-09-26，`node-link-owner` 变更 WP5：为 Node Link 的资源读面补三条窄 seam——`ReadView::node_link_slice`（会话摘要 + origin head + 未决交互含创建事件 id + `after` 之后的事件含正文，同一次只读事务）、`ReadView::session_event_payload`（按会话归属取正文）、`AuditStore::watermark`（管理写集水位 = `catalogRevision` 的唯一来源）；`NodeLinkHandshakeView` 增 `catalog_revision` 字段（`catalogRevision` 不再取 `store.head()`）。§4 补两条 `[决定]`（`catalogRevision` 口径含已登记的精度边界、三条资源读 seam 的绑定规则），§5.2/§5.3 的 trait 签名同步；DDL 未变，漂移门禁继续逐条成立）

> 版本：0.13（2026-09-26，`node-link-owner` 变更的修复轮次：`session.create` 的幂等与终态落进 `owned_command`——§4 补一条用例入口（`create_session` / `settle_session_create`）、§5.1 的 `[决定]` 同步签名与指纹义务、§6 新增第 20 条（创建提交回填 `session_id`、终态提交、崩溃窗口走第 16 条恢复、重试与冲突）。**§5/§7 的代码块与 DDL 未变**（未新增端口方法与表列），漂移门禁继续逐条成立）

> 版本：0.14（2026-09-27，`node-trust-export-ids` 变更：信任记录增 `exportIds` 收窄型白名单——§3.5 给 `NodeRecord`/`PairingSettlement::Approved` 加清单字段，§4 的单点可见性策略与 §6 第 5 条的 Owner 侧授权判定改为三条件（未撤销 ∧ `export.scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`），§7 升级到 v4（`owned_node` 末尾追加 `export_ids_json`，`ALTER TABLE ADD COLUMN`，既有行置空、不得默认放权），§7.2 版本常量改 4/4/3，§9 新增判据 32 并同步判据 1/28 的版本链与 owned 列清单断言，§10 给历史裁定条目补当前口径提示，§11.3 的「过新」用例取值改为 5，§11.6 第 4 条补清单校验与审计不新增，§11.7/§11.8 补集合列说明与「为什么只加列不重建」。**本次未改任何端口签名：§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新**，漂移门禁继续逐条成立）

> 版本：0.15（2026-09-30，`session-resume` 变更：会话恢复（`session/resume`）所需的端口与持久化形状——§2 的 `UnavailableKind` 增 `BackendUnsupported`（后端不支持该操作 = 未宣告能力，或该会话没有恢复数据；映射既有 `command.unsupported`/`nodelink.command.unsupported`/`local.unavailable`，不新增错误码），§3.1 增 `AgentSessionId`，§3.3 注明 `session.create` 与 `session.resume` 均走专用路径、不在 `CommandPayload` 枚举中，§3.6 增 `ResumeSessionRequest`（`workspace_cwd` 为持久化原文、不带别名）与 `SessionRecoveryRecord`（两列**不进** `Session`/`SessionSummary`），§4 的 `SessionLifecycle` 增 `resume_session`/`settle_session_resume`，§5.1 增 `SessionBackendFactory::resume`/`SessionEndpoint::agent_session_id` 与恢复用例顺序（授权先于一切本机读取）的 `[决定]`，§5.2 增 `SessionStore::load_recovery` 窄读取与 `StateChange::Update` 两列（`None` = 不改该列、写入后只读）的 `[决定]`，§7 升级到 v5（`owned_session` 末尾追加 `agent_session_id`/`workspace_cwd` 两列，`ALTER TABLE ADD COLUMN`、可空无默认值、既有会话行得 `NULL` 且 `NULL` 不得被补齐或按别名重解析），§7.2 版本常量改 5/5/3 并新增 v4 → v5 升级段，§9 同步判据 1/28 的版本链与 owned 列清单/旧行保留/读回不推导断言，§11.3 的「过新」用例取值改为 6。**§5 的 rust 块由本次变更的 core 侧（`session-resume` WP3）同批更新、§7 的 SQL 块由本条目同批更新**，漂移门禁继续逐条成立）
> 版本：0.16（2026-10-02，`sync-workspaces-and-create` 变更 WP3：会话目录归属在 core 侧的形状——§3.1 新增 `WorkspaceRef`（`{ alias, displayName }`，无路径字段），§3.6 注明 `workspace_alias` 是第三列但**不是**恢复列（不进 `SessionRecoveryRecord`、不进 `Session` 聚合，恢复既不读也不改），§5.2 把 `StateChange::Update` 的窄写入列从两个扩为三个（新增 `workspace_alias`，与 `workspace_cwd` 同一次提交、创建时写一次），并写死「目录归属的投影来源只有持久化的别名、不得按 `workspace_cwd` 反查」的 `[决定]`。`SessionSummary.workspace` 是可投影字段（`None` = 未分组），`workspace_cwd` 仍不进可投影形状。**§5/§7 的代码块与 DDL 之外未变**（未新增端口方法与表列），漂移门禁继续逐条绑定）
> 版本：0.17（2026-10-03，`sync-workspaces-and-create` 变更 WP4：目录归属列在存储层的落盘与投影——§7 升级到 **v6**（`owned_session` 末尾追加 `workspace_alias TEXT`，`ALTER TABLE ADD COLUMN`、可空无默认值、既有行置 `NULL` 且**不得**按 `workspace_cwd` 反查补齐），§7.2 版本常量改 6/6/3（imported 家族仍为 3）并新增 v5 → v6 升级段与「不反查」的 `[决定]`，§7.3 的 `owned_session` DDL 同步，§9 判据 1/28 的版本链与列清单断言改为 v6，§11.3 的「过新」用例取值改为 `user_version = 7`。展示名由存储层在读投影里按别名 `LEFT JOIN owned_workspace` 取得（取不到回退为别名本身，该 JOIN 不上浮到 core）；`load_recovery` 仍不读该列。**§5 的 rust 块与端口签名未变**，§7 的 SQL 块已同批更新，漂移门禁继续逐条成立）

> 版本：0.18（2026-10-04，`sync-scope-and-pwa-client` 变更 WP3：core 派生事件的生产者与两种写入形状——§5.2 的 `StateChange::Update` 增**两层可选**标题字段（`Option<Option<String>>`：`None` = 不改该列、`Some(None)` = 显式置空、`Some(Some(text))` = 写入；唯一来源是 Agent 的 `session_info_update` 经 broker 投影的 `session.info.changed`，**不存在**客户端重命名入口），§5.4 增**节点级**事件通道 `NodeEventSink`（与 `EventSink` 同形的包装类型，但语义是「不属于任何会话的事件」，使「节点级事件不得进入会话槽位」在类型上可见），§6 新增第 21 条（`file.changed` 的派生源与行级差异统计、展示路径的规范化前缀相对化、节点级 `agent.*` 事件的产生与落库、会话标题的单向更新与 `updatedAt` 取 Daemon 持久化时间）。**§7 的 SQL 块与 DDL 未变**（节点级事件的 `owned_event` 三列 CHECK 已就位，不新增 migration；标题列 `owned_session.title` 早已存在），漂移门禁继续逐条成立）

> 版本：0.19（2026-10-14，`sync-server-connection` 变更的任务 **1.7 + 1.6**：事件的**来源**与**因果**如实落库，并把它们读回 Sync 读面。先说 1.7 的前置——§3.4 的 `EventOrigin` 增第五个取值 `node`（另一个节点经 Node Link 的命令；跨节点跳数仍由 `remoteOrigin` 表达），`PendingEvent.origin`/`causation` 的填法改为「命令类事件按**诱发命令的执行者**分类、`causation` 取诱发命令的 `requestId`」，**没有诱发命令时记 `daemon`**（原文「缺 actor 时按 `device` 记账」删除——那会把 CLI 驱动的命令类事件误记成设备、并让 `causation` 恒为 `NULL`，违反 `SYNC_PROTOCOL.md` §10.1）；§7 因此升级到 **v7**（`owned_event.origin_kind` 的 CHECK 从四值扩到五值，12-step 表重建 + `owned_interaction` 的过渡表协同，见 §7.2 的 v6 → v7 段；§7.2 版本常量改 7/7/3，§9 判据 1/28 的版本链同步）。再说 1.6 的读缝——§10.1 的 `event` 帧把 `origin.kind`/`origin.deviceId`/`causationRequestId` 列为必需，而这三者只存在于 `owned_event` 的 `origin_kind`/`causation` 两列（§7.3 早已就位、`EVENT_COLUMNS` 早已 SELECT），`CommittedEvent`/`EventPayload`/`OwnedEventRecord` 都不承载——于是 §5.2 新增 `ReadView::sync_replay` 与条目类型 `SyncEventRecord`/`SyncReplaySlice`（形如 `node_link_slice` 的既有做法：条目带持久化正文 + `after`/`limit` + 同一次只读事务），条目另带 `origin_node_id`（`node` 类的 `nodeId`，与 `deviceId` 同源回查）；§4 登记用例入口 `UseCases::sync_replay`（设备 scope `session.list`，授权先于任何读取）。**未新增表列、未改 `CommittedEvent`/`OwnedEventRecord` 的形状**；§7 的 SQL 块只有 v6 → v7 那一段是新增的，漂移门禁逐条成立（§5 的 rust 块已同批更新）

> 版本：0.20（2026-10-16，`pairing-model` 变更 WP2 的文档收尾：**删除设备权限层**——设备通过认证即可执行命令、配对不授予授权，设备记录不再有授权集合。core 侧：§3.5 的 `Actor::Device` 去掉 `scopes` 字段（`Actor::scopes()` 已删除）、`DeviceRecord` 不含 `scopes`（`try_new` 7 参）；§6 第 5 条明确设备方向**没有命令级判定**（节点/本地方向不变，仍按 scope/grant 与 Export 交集判定）；§4 的两条认证收尾入口（`record_device_connected`/`record_device_auth_failure`）不再按设备 scope 归因；§11.6 第 1 条去掉 `put_device` 的「scopes 变化」分支。持久化侧：§7 升级到 **v8**（`owned_device` 删除 `scopes_json` 列，`ALTER TABLE ... DROP COLUMN`，只作用于升级前已存在的表——新建库由 DDL 常量直接建成当前形状），§7.2 版本常量改 8/8/3（imported 家族仍为 3）并新增 v7 → v8 升级段。`device.scopes_changed` 作为**历史取值**保留在两张审计表的 `action` CHECK 里（本二进制不再产生它——升级前的库可能仍有这类审计行，收缩 CHECK 会让历史行写不回去、读不出来），理由见 §7.2/§11.6 第 1 条；§9 判据 1/28 的版本链与断言同步到 v8。**§5 的 rust 块与端口签名未变**，漂移门禁继续逐条成立）
> 版本：0.21（2026-10-16，`session-create-settle-order`：`session.create` 的落账时序改为**三笔**——受理只落幂等记录（`owned_command.session_id` 为 `NULL`、无会话行）→ 执行（`SessionBackendFactory::create`）→ 落账提交（`StateChange::SettleCreate`：建会话行 + 回填幂等行 + 终态，**同一次事务**），失败零会话行且必达终态（可证明未产生会话 ⇒ `failed`、复用既有码 `internal.unavailable`；说不清 ⇒ `uncertain`）。随之：`SessionId` 改由 core 的 `IdGenerator::session_id` 分配、存储层采用（§3.1/§5.4）；§5.1 新增 `SessionBackendFactory::terminate_agent`（窗口期事件队列超限的失败关闭——结束该 Agent 运行时）；§6 第 20 条改写为三笔时序与「窗口期有界队列 + 失败关闭后围栏」，第 16 条补**无会话**恢复分支；§9 判据 20 补无会话崩溃窗口。§2 新增 `UnavailableKind::SessionNotEstablished`。**§7 的 DDL 与 migration 未变**，漂移门禁逐条成立）

> 版本：0.22（2026-10-17，`sync-path-browse-and-diff-delivery` 变更 WP2：**目录浏览端口与目录资源投影**——§5 新增 §5.5 的 `DirectoryBrowser`（`roots`/`directories`，只列直接子目录、只读、顺序不作要求）与 `DirectoryListing { name, path }`；§3.7 新增 `WorkspaceCatalogEntry { alias, display_name, path }`（`path` 为 `None` = 该键缺席，取值是规范化结果剥掉 verbatim 前缀后的形式）；§4 新增 `WorkspaceBrowse`（`workspace_browse`）与 `SyncSnapshotQueries` 的 `sync_workspace_catalog`，`session.create` 的目录来源增 `WorkspaceSource::PathWithCreate`（显式建目录开关，与别名在类型上互斥、失败不删除已建目录）；§11.1/§11.9 的路径口径改写为「**已登记**目录的路径只在协商 `core.workspace-path.v1` 时随 `workspaces` 条目下发，客户端自己提交的目录不被回显」。**§7 的 DDL 与 migration 未变**（路径取自既有 `owned_workspace.canonical_path`，无回填、无新列），漂移门禁逐条成立）

> 版本：0.23（2026-10-20，`session-activity-three-states` 变更 WP2c：会话主状态收敛为 **`idle | running | pending`** 三态并由 `SessionActivity::reduce(pending, unfinished_turns)` 唯一归约；`Session` 删除 `closed_at`。§3.3 记录归约规则并声明 `TurnState`/`CommandStatus` 词表不变；§5.2 新增 `session_activity`/`queued_turns`/`sessions_with_pending_activity` 三条读缝，`SessionUpdate` 增 `confirmation`（`confirmation` 与仲裁同事务）；`InteractionResolved` 带 `request_version`，`InteractionAuthority` 收敛为读 + `invalidate_pending`（删除 `resolve_effective`/`EffectiveResolution`）。§6 把仲裁与确认拆为两阶段、turn 终态按 `turn_id` 失效本轮交互、恢复先失效再结算并重水化持久队列、`session.updated` 最多一条。§7 升级到 **v13 / imported v5**：`owned_session` 三态 CHECK 且无 `closed_at`，`owned_interaction` 增 `confirmed_at`/`turn_id`，新增 v12 → v13 段（旧七态暂存映射、按创建事件回填 turn 归属、imported 归一化、同事务版本落盘）。§9 判据 11/28/36 改写并新增 37–39。**§7 的 SQL 块与漂移门禁逐条一致**，`npm run check:drift` 通过。

> 版本：0.24（2026-10-21，`custom-session-title` 变更 WP2b：**Owner 两类标题与原子标题命令**——§3.3 `Session` 增 `custom_title: Option<SessionTitle>`（Agent 标题仍是 `title`，有效标题 = 自定义优先否则 Agent 否则未命名）、新增 `SessionTitle` 值对象（trim 后 1..=512 标量值）、`SessionSummary.title` 只下发有效值、`CommandPayload` 增 `TitleSet{title}`；§5.2 `SessionUpdate` 增 `custom_title`（两层：`None` = 不改、`Some` = 写入，无清除/恢复语义），并声明 `OwnedCommit.idempotency` 与 `command_terminal` 可同为 `Some`（受理与终态同批、一次事务写成终态并回填 `terminal_event_id`）；§5.9 `RemoteCommandRecord` 增 `terminal_event_id`，新增 `RemoteTitleWrite`（Access 只转发、不落盘）；§7 升级到 **v15**：`owned_session` 末尾追加可空 `custom_title TEXT`（v14 → v15 只 `ALTER TABLE ADD COLUMN`，旧行 `NULL`），新建库与升级库列顺序逐项相等）

> 版本：0.25（2026-10-22，`session-activation-lifecycle` 变更 **WP1（合同先行，实现待 WP2/WP3/WP4）**：**本机生命周期的纯类型与端口**——§3.3 新增 `LocalActivity`（`Idle｜Active｜Pending`，唯一归约）、`ResourceGeneration` 与 `ViewLease`（纯运行期，**不新增持久表**）；§4 新增 `LocalSessionLifecycle` 用例族（激活/合并、查看声明与续租、容量准入与候选选择、逐会话释放确认、当前生命周期查询），时钟经既有 `Clock` 端口注入；§5 新增 **§5.10** 的计划端口（用 `rust-planned` 围栏，避免 §5 的常规 rust 块把它当成已实现形状）；§6 新增第 **23** 条：`session.activity.changed` 先持久化后发布、启动收敛不信旧 ready、查看者/租约不持久化、owned 当前查询与快照读同一代目录、激活与回收不交错、`uncertain` 不自动重派；§7 明确**不改 DDL**（`owned_*` 家族与 migration 未变），运行期目录不落盘，若实现阶段确需列变更必须版本化迁移 + 旧库升级测试；§9 新增判据 40–42。**§5 的既有 rust 块、§7 的 SQL 块与漂移门禁逐条不变**）


> 版本：0.26（2026-10-22，`web-session-preparation` 变更 **WP1（合同先行，实现待 WP2–WP7）**：**首条提交与准备附件的合同冻结**——§5 新增 **§5.11** 计划端口（`FirstSubmission`、`PreparationAttachmentStaging` 及全部纯类型，`rust-planned` 围栏）；新增顶层 **§12** 用 `sql-planned` 围栏冻结 `owned_first_submission` 的 DDL（`session_id` 主键、`first_submission_id`/`user_message_id` 唯一、完整输入与摘要、阶段与 `ever_dispatched`、当前请求/结果），**不**写进 §7；§7 的 DDL 与 v15 版本断言逐字未变。配套 wire 合同（feature `core.session-preparation.v1`、准备控制帧、`session.start`、三个 `preparation.*` 错误码、`acp-remote/session-preparation/v1` 签名域、附件准备 target）见 `SYNC_PROTOCOL.md` §3.4/§6.3/§11.5/§11.9/§12.2。WP2 以 v16 落地后把计划 DDL 移入 §7、把已实现的端口移入 §5 常规块并删除 §5.11/§12 的计划小节。

> 版本：0.27（2026-10-22，`web-session-preparation` 变更 **WP2**：**首条提交与准备附件暂存落地**——`owned_first_submission` 进入 §7 的 `sql` 块（v16）；`owned_message.channel` 的 CHECK 扩到 `user`（首条用户消息无需 Agent 回显即可读取）；§5.11 的两个端口移入常规 `rust` 块并与 `crates/core/src/ports.rs` 逐条对齐，§12 计划小节删除。准备运行期目录（`PreparationDirectory`：`allocated → preparing → ready → released`、30 秒租约/10 秒续租、容量经 `CapacityAdmission`）与派发围栏/启动收敛/回显关联的纯类型见 §3.9/§3.10。旧会话不回填、不被识别成准备。

> 版本：0.28（2026-10-22，`web-session-preparation` 变更 **WP2 检视修复**：终态投影与事件口径——`session.start` 的 `command.completed` 现在携带 §11.9 的六字段 `result` 并持久化 `result_json`（重放返回同一终态与 `terminal_event_id`）；结算只在首条 turn 仍非终态时写 `turn.*` 领域事件（CAS，随后写命令终态），已由 broker 收敛的 turn 不再重复终态/`ended_at`/会话版本，非 `accepted` 行按 `Conflict(Consumed)` 拒绝；终态事件与首条初始事件都按既有 `expires_at` 策略写 TTL；`FirstSubmissionRecord` 增加 `mode_id`/`config` 供显式继续复用记录的选择；受理事务回滚只把**本次认领**真的搬走的附件字节移回封存路径（不碰更早会话已提交的同内容字节）且同 ID 可重试；准备目录在释放/提交成功后移除条目，只留 `PREPARATION_RELEASED_TOMBSTONES` 条 tombstone。§7 的 DDL 与 v16 版本断言逐字未变。

## 1. 范围与非目标

范围：core 的值对象/用例/端口/错误类型、broker 的提交与发布契约、`storage-sqlite` 的 PRAGMA/文件布局/migration/表结构/索引/保留与容量/崩溃恢复、以及实现该合同的验收判据。

非目标：不写实现代码；不定义 ACP DTO（`acp-protocol`）与 Sync/Node Link wire DTO（各自协议 crate）；不定义 daemon 生命周期、CLI、前端；不设计 `identity-auth` 内部状态机（只定义它需要的持久化端口面）；不做应用层数据库加密（`SECURITY_DESIGN.md` §13.1 既定决策）。

## 2. 依赖、运行时与错误类型

- `core` 不依赖 Tokio、Axum、SQLite、WebSocket、子进程、ACP DTO、任何 wire protocol（`AGENTS.md` §5、`MODULE_ARCHITECTURE.md` §4.1）。
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
    BackendUnsupported, SessionNotEstablished,
}
```

`ConflictKind::{AlreadyExists, IdentityMismatch, DuplicateOwnership}` 与 `UnavailableKind::KeystoreUnavailable` 是管理写集引入的取值（§11.6），与本表同批落地（`crates/core/src/model/error.rs` 的 `ALL`/`as_str` 逐项一致）；本地管理适配器把它们映射为 `LOCAL_ADMIN_PROTOCOL.md` §6 的 `local.conflict`/`local.unavailable`，`port_error_public` 必须显式覆盖这四个取值，不得落进通配臂。

`[决定]` `UnavailableKind::SessionNotEstablished`（`session-create-settle-order` 变更）：「后端**可证明**没有建立本次会话」——进程未能启动（spawn / profile / 凭据 / 环境变量解析失败）、请求未发出即失败（进程不在），或 Agent 对会话建立请求给出明确错误答复。它是 `session.create`/`session.resume` 的**终态判据输入**：命中它 ⇒ `failed`（可证明零副作用），其余不可用取值（`Busy`/`IoError`/…）都「说不清」⇒ `uncertain`。`agent-host` 只在会话建立入口（`create`/`resume`）把可证明的一类收敛成它（`HostError::to_establishment_error`），其余操作沿用既有映射；`port_error_public` 取既有码 `internal.unavailable`（成因由终态 `message` 承担），**不新增**任何 wire 错误码。

`[决定]` `UnavailableKind::BackendUnsupported`（「后端不支持该操作」）语义上**不是临时故障**：目标 Agent 未宣告该操作所需的能力，或该会话没有该操作所需的持久化数据（§3.6 的两列 `NULL`）时使用它。`port_error_public` 取既有码 `command.unsupported`（不可重试）；Node Link 适配器映射为既有 `nodelink.command.unsupported`，本地管理适配器映射为既有 `local.unavailable`——**不新增**任何错误码、feature ID 或 `local.*` 取值。它与 `IoError`/`StorageFull` 等「存储或后端不可用」类取值必须可区分（同一输入不得有两个错误码）。

`sqlx::Error`（或任何适配器错误）必须在适配器内映射成上表之一后才可进入 core（`MODULE_ARCHITECTURE.md` §8）。

## 3. `core::model` 值对象（全量冻结）

### 3.1 标识与引用

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `NodeId` / `DeviceId` / `SessionId` / `EventId` / `RequestId` / `TurnId` / `InteractionId` / `PairingId` / `AttachmentId` / `SubmissionId` / `MessageId` | newtype over canonical 小写 UUID 文本（`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`） | `SYNC_PROTOCOL.md` §3.3 |


| `WorkspaceAlias` | `^[a-z0-9][a-z0-9._-]{0,63}$` | `NODE_LINK_PROTOCOL.md` §12.3 |
| `AgentRef` | `{ agentId: String(1..=128), name: String(1..=128) }` | `schemas/sync/v1/common.schema.json#/$defs/sessionSummary` |
| `WorkspaceRef` | `{ directoryId: DirectoryId }`——**会话目录归属在可投影形状里的唯一载体**：只有目录标识，**没有路径字段**（路径按 `directoryId` 在同一份快照的 `workspaces` 条目里查表；§3.7/§5.1）。`directoryId` 由本机规范化路径派生、是稳定主键，**不作为界面文本呈现**。`SessionSummary.workspace` 是它的 `Option` 包装，`None` = 未分组 | `design.md`；`schemas/sync/v1/common.schema.json#/$defs/workspaceRef`；`SECURITY_DESIGN.md` §12.3 |
| `AgentSessionId` | **Agent（ACP）侧会话标识**：非空、≤512 字符、不含 NUL（不经过任何 wire，因此上限是本机约束，不引入 schema）；与 core 的 `SessionId`（本机 UUID）不是同一个东西。由 [`SessionEndpoint::agent_session_id`] 交给 core 落盘（§3.6/§5.2），core **不得**在未取得标识时编造取值（`crates/core/src/model/ids.rs`） | 本合同（§5.1/§5.2） |
| `OwnedSessionRef` | `{ sessionId }` | `MODULE_ARCHITECTURE.md` §4.1 |
| `RemoteSessionRef` | `{ ownerNodeId, exportId, sessionId }` | `NODE_LINK_PROTOCOL.md` §7 |
| `OriginEventRef` | `{ ownerNodeId, originEpoch, originEventId }` | `NODE_LINK_PROTOCOL.md` §7 |
| `SessionReference` | enum `Owned(OwnedSessionRef) ｜ Remote(RemoteSessionRef)`；`SessionBackendFactory` 与路由用它 | `MODULE_ARCHITECTURE.md` §4.1/§4.5/§4.6 |
| `EntityRef` | enum `Session(SessionId) ｜ Turn(TurnId) ｜ Interaction(InteractionId) ｜ Command{ session: Option<SessionId>, request: RequestId } ｜ Pairing(PairingId) ｜ Device(DeviceId) ｜ Node(NodeId) ｜ Export(String) ｜ Import(String) ｜ Provider(String)`；`Export`/`Import` 两个分支只用于读取升级前的历史审计行，本二进制不再产生；`kind()` 给出稳定 token | 本合同 |

`[决定]` **id 的分配只有两处权威**：`SessionId` 由 **core** 在调用后端**之前**用 `IdGenerator::session_id` 分配、落账提交（`StateChange::SettleCreate`）由存储层**采用**（经 `CommitOutcome.session_id` 原样回传）——后端要拿它构造引用，而会话行那时才落盘，因此存储层不得再自己现分配；`EventId` 仍由 `SessionStore::commit` 在提交事务内分配（写进 `owned_event.event_id`）——它必须与落盘同一时刻产生；`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId`/**`MessageId`** 由 **core** 在调用前用 `IdGenerator` 分配并随写入形状传入（`MessageId` 由生产 delta 的适配器在该消息第一条 delta 提交前分配，见 §6 第 14 条）（`NewTurn.turn`、`PendingInteractionWrite.interaction.id`、`OwnedCommit.origin_epoch` 等），存储层只校验一致性、**不得**另行编号；`AttachmentId` 由 `AttachmentStore::put` 分配并返回（§7.3）。

### 3.2 序号、游标与时间

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `Sequence` | newtype over `u64`；线上为无前导零十进制字符串；`[决定]` v1 上界 `2^63−1`，超出即协议错误（见 §7.3 存储规则） | `SYNC_PROTOCOL.md` §3.3 |
| `Version` | newtype over `u64`，会话乐观并发版本 | `MODULE_ARCHITECTURE.md` §4.1 |
| `AttachmentGeneration` | newtype over `u64` | `NODE_LINK_PROTOCOL.md` §7 |
| `ServerEpoch` | newtype over UUID；事件库首次创建时生成，普通升级与重启保持不变 | `SYNC_PROTOCOL.md` §9.1 |
| `OriginEpoch` | newtype over UUID；**会话级** | `NODE_LINK_PROTOCOL.md` §7 |
| `GlobalCursor` | `{ serverEpoch, globalSequence }` | `SYNC_PROTOCOL.md` §9.1 |
| `OriginCursor` | `{ originEpoch, originSequence }` | `NODE_LINK_PROTOCOL.md` §6/§15 |
| `LocalCursor` | newtype over `u64`（Access 本地投递序号，只服务本节点客户端） | `NODE_LINK_PROTOCOL.md` §6 |
| `Timestamp` | newtype over 文本，固定 `%Y-%m-%dT%H:%M:%S%.3fZ`（UTC、毫秒、`Z`）；TEXT 排序即时间序，因此该宽度是硬约束 | `SYNC_PROTOCOL.md` §3.3 |
| `Digest` | newtype over base64url 32 字节（SHA-256） | `schemas/sync/v1/common.schema.json#/$defs/base64url32` |

### 3.3 会话、turn、命令与交互

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `ResourceOrigin` | enum `Local ｜ Remote { ownerNodeId, exportId, originEpoch, online }`；owned/imported 在类型上可区分 | `MODULE_ARCHITECTURE.md` §4.1；`schemas/sync/v1/common.schema.json#/$defs/originBlock` |
| `Session` | `{ id: SessionId, reference: OwnedSessionRef, title: Option<String(≤512)>, custom_title: Option<SessionTitle>, agent: AgentRef, state: SessionState, origin: ResourceOrigin, current_mode: Option<ModeRef>, version: Version, created_at, updated_at }`；`title` 是 **Agent 标题**、`custom_title` 是用户设置的覆盖值（trim 后 1..=512 标量值，`None` = 从未设置）；**有效标题** = `custom_title` 否则 `title` 否则未命名（`Session::effective_title`，唯一投影口径）。**没有 `closed_at`**：会话没有关闭动作，也没有终态（`session-activity` 变更删除该字段） | `SYNC_PROTOCOL.md` §9.6；`common.schema.json#/$defs/sessionSummary` |
| `SessionTitle` | newtype over `String`：`SessionTitle::parse` 校验**输入** 1..=512 个 Unicode 标量值且 trim 后非空，**保存值是 trim 后的文本**（不截断、不改写中间内容）；空串/纯空白/超长/null 一律 `InvalidValue`。core 自己的值对象，不从 wire crate 借用 | 本合同；`custom-session-title` D1/D2 |
| `SessionSummary` | `Session` 的可投影子集（wire 形状见协议 crate），**只有一个 `title` 且它是有效值**（自定义优先、否则 Agent 标题；`design.md` D1 不新增 wire 字段）；owned 与 imported 用同一形状 | `SYNC_PROTOCOL.md` §9.6 |
| `SessionSnapshot` | `Session` + `origin_epoch: OriginEpoch` + `head: GlobalCursor` | 本合同（供 `commit`/`load` 返回） |
| `SessionState` | enum `Idle｜Running｜Pending`——**只有活跃程度，没有终态**。`TurnState`（`Queued｜Running｜WaitingInput｜WaitingPermission｜Completed｜Failed｜Cancelled`）与 `CommandStatus`（`Accepted｜Completed｜Failed｜Rejected｜Uncertain`）词表**不因本收敛而改变**：任务失败/取消与命令结果仍如实保留，只是不再把会话压成 `Failed`/`Closed` | `common.schema.json#/$defs/sessionSummary` |
| `SessionActivity` | 零大小类型 + 唯一归约函数 `SessionActivity::reduce(pending: bool, unfinished_turns: bool) -> SessionState`：`pending` 优先 → `Pending`；否则 `unfinished_turns` → `Running`；否则 `Idle`。辅助判定 `SessionActivity::turn_is_unfinished(turn: TurnState) -> bool` = `!turn.is_terminal()`（`queued`/`running`/`waiting_*` 都算未结束）。所有写入方（broker、存储层恢复/失效路径）只按此函数取值，不得各自维护转换表；有效未解决交互的细节留在 `SessionActivityFacts`（§5.2），等待原因留在有效交互上，不新增互斥的单值原因字段 | `crates/core/src/model/session.rs`（`session-activity` D1） |
| `LocalActivity` | enum `Idle｜Active｜Pending`——**本机展示**的活跃程度（`core.local-session-lifecycle.v1`，`SYNC_PROTOCOL.md` §9.8），与 `SessionState` 是两个维度：`Pending` 优先（有效未解决交互）；否则未结束任务（含排队）、已确认 ready 的运行资源、或从 ready 开始回收但**尚未确认释放**的资源 → `Active`；否则 `Idle`。首次 activating 不提前变 `Active`；`uncertain`/关闭失败/回收中保持 `Active` 与容量占位，只有确认释放后才转 `Idle`。纯类型，不含时间与 I/O；`SYNC_PROTOCOL.md` §9.8 与 `common.schema.json#/$defs/localActivity` 同义 | 本合同（§6 第 23 条） |
| `ResourceGeneration` | newtype over `u64`，**会话运行的资源代**：每次成功建立/恢复会话运行资源时自增，回收、关闭失败或重新恢复都会换新代。用途是隔离迟到事件与旧命令结果——旧代的事件/收口结果不得绑定到新代资源。只在运行期目录内存在，**不落库、不进快照** | 本合同（§5.10） |
| `ViewLease` | `{ connection: ConnectionId, session: SessionId, view_version: u64, expires_at: Timestamp }`——**查看租约**：某个已认证连接当前正在查看某会话的声明与到期时刻。每连接最多一个当前查看者；同一 `view_version` 的重试幂等，过期版本不复活。租约只存在于内存，断开/撤销/连接被替换时立即清除，不持久化、不进快照或事件 | 本合同（`SYNC_PROTOCOL.md` §11.8） |
| `ModeState` | `{ current_mode: Option<ModeRef>, available: Vec<ModeRef> }`——`session.mode.list` 的端口侧形状；`version` 属于会话（用例层补），不在本类型里 | `SYNC_PROTOCOL.md` §11.5/§10.2 |
| `TurnId`/`Turn` | `Turn { id, session: SessionId, state: TurnState, queue_index: u32, causation: Option<RequestId>, started_at/ended_at }` | `SYNC_PROTOCOL.md` §11.6 |
| `TurnState` | enum `Queued｜Running｜WaitingInput｜WaitingPermission｜Completed｜Failed｜Cancelled` | 同上 |
| `ModeRef` / `ModeId` | `{ modeId: String(1..=256), displayName: String(1..=256) }`；`ModeId` 是 `modeId` 的 newtype | `common.schema.json#/$defs/modeRef` |
| `ConfigOptionId` | newtype over `String(1..=256)` | `SYNC_PROTOCOL.md` §11.5 |
| `ConfigValue` | enum `Text(String(≤4096)) ｜ Boolean(bool) ｜ Select(String(1..=256))` | `common.schema.json#/$defs/configOptionView` |
| `ConfigOption` | `{ id, name, description: Option<String(≤1024)>, category: Option<String(≤128)>, kind: Select｜Boolean, current: ConfigValue, options }` | 同上 |
| `CommandKind` | enum `Query ｜ Mutation`（取自 `commands.json` 的分类） | `compatibility/commands/v1/commands.json` |
| `ClientCommand` | `{ actor: Actor, request: RequestId, command: String(命令名), kind: CommandKind, session: Option<SessionId>, expected_version: Option<Version>, payload: CommandPayload }` | `SYNC_PROTOCOL.md` §11.5 |
| `CommandPayload` | enum，按命令名一对一：`SessionList{} ｜ SessionRead{ include } ｜ CommandStatus{ target_request: RequestId } ｜ ModeList{} ｜ ConfigList{} ｜ Prompt{ content } ｜ Cancel{turn: Option<TurnId>} ｜ ModeSet{mode} ｜ ConfigSet{id,value} ｜ PermissionResolve{interaction, option_id} ｜ ElicitationRespond{interaction, action, values} ｜ TitleSet{title: SessionTitle}`。`session.create` 与 `session.resume` **均不在其中**：两者各有专用路径（分别经 `CreateSessionRequest` 与 `ResumeSessionRequest`、由 §4 的 `SessionLifecycle` 入口受理），增删这两个命令不改本枚举 | `SYNC_PROTOCOL.md` §11.5、§12.7 |
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

| `MessageChannel` | enum `Reply｜Thought`（wire `reply/thought`）；回复与思考分别持久化，`Thought` 只在 Agent 明确提供时产生，`agent.thought.delta` 不产生 `agent.message.completed` | `SYNC_PROTOCOL.md` §10.4 |
| `PartialResult` | `{ state: Completed｜Partial｜Failed｜Cancelled, reason: Option<PartialReason> }`；`Partial` 表示保留已收到内容但上游未完成，MUST NOT 折叠成成功 | 同上 |
| `PartialReason` | enum `AgentStopped｜AgentFailed｜TurnCancelled｜ConnectionLost｜RetentionExpired` | 同上 |
| `MediaKind` | enum `Image｜Audio｜File` | 同上 |
| `MediaAvailability` | enum `Available｜Expired｜NotFetched｜Unsupported`；不可用是显式事实，不得用空字节或空 URL 冒充 | 同上 |
| `MediaReference` | `{ attachment: AttachmentId, session: SessionId, kind: MediaKind, media_type: String(1..=255), byte_length: u64, sha256: Digest, display_name: Option<String(1..=255)>, availability: MediaAvailability }`；**不含** URL、路径或 bearer 凭据 | 同上 |
| `AttachmentSubject` | enum `Device(DeviceId)｜Node { access_node: NodeId }`；传输与清单的主体绑定，每一帧重复校验 | `SYNC_PROTOCOL.md` §3.4；`NODE_LINK_PROTOCOL.md` §12.9 |
| `AttachmentTransferState` | enum `Receiving｜Sealed｜Aborted｜Expired` | 同上 |
| `AttachmentTransfer` | `{ transfer: AttachmentId, session: SessionId, subject: AttachmentSubject, total_length: u64, received_length: u64, chunk_size: u32(32768), state: AttachmentTransferState, sealed: Option<SealedAttachment>, created_at, updated_at }`；`chunk_size` 是固定 v1 常量，不是客户端可协商值 | 本合同（§5.8/§7.6） |
| `SealedAttachment` | `{ attachment: AttachmentId, length: u64, sha256: Digest, sealed_at: Timestamp }`；只在长度与流式摘要校验通过后产生 | 同上 |
| `AttachmentSubmission` | `{ submission: SubmissionId, session: SessionId, subject: AttachmentSubject, attachments: Vec<AttachmentId>, digest: Digest, sealed_at: Timestamp }`；`attachments` 顺序即清单顺序，封存后不可追加 | 同上 |
| `EffectiveInteraction` | `{ interaction: InteractionId, session: SessionId, kind: InteractionKind, request_version: Version, expires_at: Option<Timestamp>, options: Vec<InteractionOption>, schema: Option<ElicitationSchema> }`；`request_version` 每次 pending 写入递增，resolve 必须原样回传，不符即 `interaction.version_conflict` 且不消费请求 | `SYNC_PROTOCOL.md` §11.7 |

`workspace.mkdir` 是无会话变更命令：受理和终态复用 `CommandRecord`，不构造 session/workspace 行。独立创建目录端口与一次性执行凭证见 §5.6；终态必须使用 §5.2 的主体绑定提交入口，避免不同主体使用相同 requestId 时发生歧义。

### 3.4 事件

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `EventId` | 事件稳定标识；imported 事件的 `event_id` 必须等于 origin 事件的 id | `SYNC_PROTOCOL.md` §9.6/§10.1 |
| `EventType` | newtype over `^[a-z0-9_.-]{1,128}$`；未登记取值按未知事件降级处理，**不是**闭合枚举 | `SYNC_PROTOCOL.md` §10.1 |
| `EventKind` | enum `State｜Delta｜FinalMessage｜Structured｜Interaction｜Summary` | `INITIAL_DESIGN.md` §10.2/§10.3 |
| `EventOrigin` | enum `Agent｜Device｜Daemon｜LocalCli｜Node` | `SYNC_PROTOCOL.md` §10.1 |
| `EventPayload` | `{ view: ViewJson, acp: Option<AcpRaw> }`；`view` 为公共视图（原样保存），`acp` 为 ACP 原文 | `SYNC_PROTOCOL.md` §10.3 |
| `AcpRaw` | enum `Available { media_type: String, raw_json: String, byte_length: u64, sha256: Digest } ｜ Unavailable { reason: RawUnavailableReason, byte_length: u64, sha256: Option<Digest> }` | `SYNC_PROTOCOL.md` §10.3；`common.schema.json#/$defs/rawAcp` |
| `RawUnavailableReason` | enum `SizeLimit｜RetentionExpired｜StorageFailure` | `common.schema.json#/$defs/rawAcp` |
| `PersistencePolicy` | enum `Durable｜ShortTerm｜Ephemeral`；`Ephemeral` **不得**进入任何提交 | `INITIAL_DESIGN.md` §10.2 |
| `StoredPolicy` | enum `Durable｜ShortTerm`（`OwnedCommit` 只接受它，使 `Ephemeral` 不可表达） | 本合同 |
| `PendingEvent` | `{ kind: EventKind, event_type: EventType, policy: StoredPolicy, payload: EventPayload, origin: EventOrigin, turn: Option<TurnId>, causation: Option<RequestId> }`；`origin` 是**产出者**、`causation` 是**诱发它的命令**，两件事不得互相替代。填法（`crate::broker::event_origin` 是唯一实现）：命令类事件（`permission.resolved`/`elicitation.resolved`/`turn.*`/`command.*`/`session.mode.changed`/`session.config.changed`）的 `origin` 取**诱发命令的执行者**——本机 CLI → `local_cli`、本机已认证 Sync 设备 → `device`、另一个节点经 Node Link 的命令 → `node`；daemon 自身事件（`device.revoked`/`session.origin.online_changed`/`storage.integrity_failed`）→ `daemon`；其余由 Agent 产出 → `agent`。**没有诱发命令时记 `daemon`**，`MUST NOT` 默认 `device`、也 `MUST NOT` 按「有没有会话或 actor」反推产出者。`causation` 取诱发命令的 `requestId`（**按事件类型**分流：turn 生命周期事件与 turn 内由 Agent 产出的事件取该 turn 的 `owned_turn.causation`；四类**响应类**事件——`permission.resolved`/`elicitation.resolved`/`session.mode.changed`/`session.config.changed`——取提交时在会话作用域的那条命令，**即使它落在某个正在跑的 turn 里**，会话作用域无命令时退回该 turn 的命令，`MUST NOT` 按事件归属的 turn 取命令——权限/elicitation 的解析按定义发生在 prompt turn 的 `WaitingPermission` 期间，按归属取会把解析记到那条 prompt 上；其余不属于任何 turn 的**命令类**事件取会话作用域命令；其余不属于任何 turn 的事件为 `null`）；**没有命令直接导致的事件为 `null`**，而 turn 内由 Agent 产出的事件**照样带**该命令的 `causation`。存储层只原样写 `owned_event.origin_kind`/`owned_event.causation`（否则每条会话事件都被记成 `agent`，而 `device.revoked` 被记成 `daemon`，`SYNC_PROTOCOL.md` §10.1 的 `origin.kind` 就失去意义） | 本合同（§7.3） |
| `CommittedEvent` | `{ id: EventId, event_type: EventType, session: Option<SessionId>, session_sequence: Option<Sequence>, global_sequence: Sequence, origin_epoch: Option<OriginEpoch>, origin_sequence: Option<Sequence>, created_at: Timestamp }`；**非会话级事件两者均为 `None`**（§3.4、§7.3 的成对 CHECK、§9 判据 17）。`event_type` 是**必填**的：历史（`HistoryPage`）与重放（`CommittedDelivery::Owned`）都必须能在 wire 上报出事件的类型，不能靠调用方另查一次；`kind` **不**进本类型（它只是存储层的保留/压缩维度，wire 不需要） | `SYNC_PROTOCOL.md` §9/§10 |
| `CommittedDelivery` | enum `Owned(CommittedEvent) ｜ Imported { session: RemoteSessionRef, origin: OriginEventRef, origin_sequence: Sequence, local_sequence: LocalCursor, event_type: EventType, payload_digest: Digest, payload: Option<EventPayload> }`（imported 的 `payload` 只在内存中存在） | `MODULE_ARCHITECTURE.md` §7 |

`[决定]` owned 事件同时携带两种会话级序号：`session_sequence`（该会话内递增）与 `origin_sequence`（该会话的 origin cursor 分量）；对 owned 会话两者在同一事务内各自 +1，但**不得**互相替代（`NODE_LINK_PROTOCOL.md` §7 要求跨节点只认 origin cursor）。非会话级事件（`device.revoked` 等）两者均为 `None`，只分配 `global_sequence`（`SYNC_PROTOCOL.md` §10.1）。

### 3.5 身份、信任、审计

| 类型 | 形状 | 出处 |
|---|---|---|
| `Actor` | enum `Device { device: DeviceId } ｜ Node { node: NodeId, access_node: NodeId } ｜ LocalCli ｜ PairingClaimant { pairing: PairingId }`；设备与节点分支都不带授权集合，配对认领方永远不授权。`AuditRecord.via_node` 与节点身份的 id 文本同一口径 | 本合同 |
| `DeviceRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3（含 `deviceId`、指纹、状态、时间戳；**不含 `scopes`**——设备不再有授权集合，构造器 `try_new` 取 7 参：`deviceId`/`displayName`/`publicKeyFingerprint`/`state`/`createdAt`/`lastSeenAt`/`revokedAt`） | 同左 |
| `NodeRecord` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.4（含 `nodeId`、指纹、`state`、`ownerEndpoint`）；同一 `nodeId` 可同时存在 `access`/`owner` 两行，读取与撤销按 `(NodeId, NodeKind)` 取行，两种角色共享一条身份材料（§11.5/§5.3）。信任记录不含任何授权集合或 Export 清单。`lastConnectedAt` 是**最近一次认证成功时间**：每次成功认证的收尾写集都推进它，值只前进不倒退 | 同左 |
| `PairingRecord` | `{ id: PairingId, target: PairingTarget(Device｜Node), state: PairingState, display_name: Option<String(1..=128)>, secret_digest: Digest, host_binding: String(1..=2048), created_at, expires_at, claimed_at: Option, approved_at: Option, terminal_at: Option }`；构造校验：`claimed_at` 非空 ⟺ 状态不是 `created`；`approved_at` 非空 ⟺ `approved`/`consumed`；`terminal_at` 非空 ⟺ `rejected`/`expired`/`consumed`；`host_binding` 非空。配对不携带任何授权集合。`secret_digest` 是 pairing secret 的 SHA-256 | 本合同 |
| `PairingState` | enum `Created｜Claimed｜PendingConfirmation｜Approved｜Rejected｜Expired｜Consumed`；终态 = `Rejected｜Expired｜Consumed`；`Claimed` **不对外可见**（只在服务端事务与审计里出现） | `SYNC_PROTOCOL.md` §7.0 |
| `PeerIdentity` | enum `Device(DeviceId)｜Node(NodeId)`；`kind()` 给出 "device"/"node" | 本合同 |
| `PeerPublicKey` | `65 字节 SEC1 未压缩 P-256 公钥`：私有字段 + `try_from_bytes`（`TryFrom<&[u8]>` 委托同一实现）；构造顺序固定（长度 == 65 → 首字节 == `0x04` → `p256::PublicKey::from_sec1_bytes` 成功），任一步失败 → `InvalidValue`；**长度断言必须先于解析**（`from_sec1_bytes` 接受 33 字节压缩点，不先断言就会绕过「SEC1 uncompressed」合同）；`fingerprint()` 是唯一指纹入口 = `SHA-256(65 字节原始公钥)` 的 64 字符小写 hex，适配器不得各自现算；私钥、keystore handle、pairing secret 明文不进本类型 | `crates/core/src/model/identity.rs`；`IDENTITY_AND_AUTH_CONTRACT.md` §3 |
| `PairingPeer` | `{ id: PeerIdentity, display_name: String(1..=128), public_key: PeerPublicKey, host_binding: String(1..=2048), client_nonce: Nonce }`；指纹不再单独存放，由 `public_key.fingerprint()` 派生（`LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 wire 字段 `publicKeyFingerprint` 语义不变，值变成派生结果）；`host_binding` 是 claim 里回显的绑定（设备为 `canonicalOrigin`、节点为 `endpoint`），必须与登记的 `PairingRecord.host_binding` 逐字相等，`owned_pairing_peer` 不单独存该列（读取时从配对行回填） | `SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2 |
| `PairingClaim` | `{ pairing: PairingId, peer: PairingPeer }`——HMAC/proof 由**调用方**验证，进入本类型时只剩已核对的事实 | 本合同（`TrustStore::claim_pairing` 的输入） |
| `PairingSettlement` | enum `Approved ｜ Rejected { reason: Option<String(≤256)> }`；配对不携带任何授权集合 | 本合同（`TrustStore::settle_pairing` 的输入） |
| `PairingConsumption` | `{ pairing: PairingId, actor: Actor, context: WriteContext }`；`actor` 是本次认证的主体，只接受与该配对**已批准对端**一致的 `Actor::Node`/`Actor::Device`，`context.audit` 必须携带与它同主的审计行（`node.authenticated`/`device.authenticated`） | 本合同（`TrustStore::consume_pairing` 的输入） |


| `AuditRecord` | `{ at: Timestamp, action: AuditAction, actor: Actor, via_node: Option<NodeId>, local_principal_ref: Option<String(≤128)>, target: EntityRef, outcome: AuditOutcome, detail_digest: Option<Digest> }`；**不含任何内容** | `SECURITY_DESIGN.md` §14.2 |
| `AuditAction` | 闭合枚举：`PairingCreated｜PairingClaimed｜PairingApproved｜PairingRejected｜PairingExpired｜DeviceAuthenticated｜DeviceAuthFailed｜DeviceRevoked｜DeviceScopesChanged｜NodePaired｜NodeAuthenticated｜NodeAuthFailed｜NodeTrustRevoked｜NodeIdentityChanged｜ExportCreated｜ExportRevoked｜ImportAdded｜ImportRemoved｜ProviderConfigured｜AuthorizationDenied｜RateLimitTriggered｜StorageIntegrityFailed` | `SECURITY_DESIGN.md` §14.2 |
| `AuditOutcome` | enum `Success｜Denied｜Failed` | 本合同 |

`[决定]`（`access-owner-data-stateless` D1）`NodeId`/`DeviceId` 及其公钥、指纹、地址与信任/撤销决定属于
**连接资料**；由它们派生出的 Owner 会话摘要、目录、来源游标、命令状态与结果引用**不是**连接资料，不得
持久保存。运行期目录与在线回源的纯类型接口见 §5.9。

### 3.6 后端与能力

| 类型 | 形状 | 出处 |
|---|---|---|
| `AgentDescriptor` | `{ agent: AgentRef, available: bool, origin: ResourceOrigin }` | `MODULE_ARCHITECTURE.md` §4.1 |
| `Capability` / `CapabilitySet` | `Capability { kind: String(1..=128), detail: Option<String(≤256)> }`；`CapabilitySet` 为去重集合 | `ACP_COMPATIBILITY_MATRIX.md` §4 |
| `CreateSessionRequest` | `{ agent: AgentRef, workspace: Option<ResolvedWorkspace>, template: Option<TemplateSelection>, origin: ResourceOrigin }`——alias → 路径的解析在 `UseCases::create_session` 内完成（§5.1），后端只收已解析路径 | `NODE_LINK_PROTOCOL.md` §12.7 |
| `ResolvedWorkspace` | `{ alias: WorkspaceAlias, canonical_path: String }`；`canonical_path` 是本机规范化绝对路径，只交给后端，不得进事件、错误 `details`、审计 `detail_digest` 的前像或 Node Link catalog（解析、校验与失败分类见 §5.1） | 本合同 |
| `ResumeSessionRequest` | `{ agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }`——恢复的输入**全部取自 Owner 自身的持久化记录**（客户端不得提供，Node Link 的 `session.resume` payload 是空对象）。`workspace_cwd` 是持久化的「创建时 canonical path」**原文**，不带 workspace 别名（别名指向可被改写或删除，恢复一律以持久化取值为权威、不得按别名重解析）；构造校验与 `ResolvedWorkspace::canonical_path` 同口径（非空、≤4096 字符、无 NUL、绝对路径形状），存在性/目录性/`canonicalize` 一致性由恢复用例在调用后端**之前**复校验（§5.1） | 本合同（§5.1） |
| `SessionRecoveryRecord` | `{ agent: AgentRef, agent_session_id: Option<AgentSessionId>, workspace_cwd: Option<String> }`——`owned_session` 两列在 core 侧的读取形状：`None` 就是 `NULL`，语义为「该会话没有可用于恢复的数据」，恢复必须显式失败，**不得**推导、补齐或用别名重解析。两列**不进** `Session`/`SessionSummary`（`workspace_cwd` 是本机规范化路径，进入可投影形状会违反「路径只出现在目录资源条目上」的边界，§3.7/§11.1），只经窄读取 `SessionStore::load_recovery` 进出（§5.2）。`owned_session.workspace_alias` 是**第三列但不是恢复列**：它不进本类型、也不进 `Session` 聚合，只出现在 `SessionSummary.workspace`（§3.1 的 `WorkspaceRef`）里；恢复路径既不读它也不改它 | 本合同（§5.2） |
| `TemplateSelection` | `{ template_id: String(1..=128), params: Vec<(String, ConfigValue)> }` | `NODE_LINK_PROTOCOL.md` §12.3 |
| `PromptRequest` | `{ content: Vec<PromptContentBlock> }`（形状见协议 crate 的 `promptContentBlock`） | `SYNC_PROTOCOL.md` §11.5 |
| `EndpointEvent` | `{ kind: EventKind, event_type: EventType, payload: EventPayload, turn: Option<TurnId>, causation: Option<RequestId>, at: Timestamp }`（`SessionEndpoint` 的输出流元素） | 本合同 |

`[决定]`（`access-owner-data-stateless` D2/D6）`SessionSummary` 的 `ResourceOrigin::Remote` 在 Access 侧
**只**由运行期目录组装：`owner_node_id`/`origin_epoch` 来自当前已认证连接的 Owner 目录，`online` 是连接期
事实而不是持久字段。运行期目录不可用、来源已断开/撤销时不得把该来源表达成空清单成功；来源状态通过
连接期通知（`sync.remote_source_state`）与 `origin.online` 表达。端口形状见 §5.9。

### 3.7 本地配置与凭据值对象

| 类型 | 形状 | 出处 |
|---|---|---|
| `AgentProfile` | `{ id: AgentId, display_name: String(1..=128), command: String, args: Vec<String>, env_allowlist: Vec<String>, env: Vec<ProviderEnvBinding>, default: bool, created_at: Timestamp, updated_at: Timestamp }`；`command` 是可执行文件路径或名字、**不经 shell 拼接**；`env_allowlist` 是环境变量名白名单（上限而非提示）；`env` 为空表示不注入任何凭据；至多一条 `default = true`（§7.3 的部分唯一索引） | 本合同（`crates/core/src/model/config.rs`）；写入路径 `LocalConfigStore::put_profile`（§5.3） |
| `ProviderEnvBinding` | `{ provider_id: String, field: String, name: String }`；不变式：`name` 必须同时出现在 `env_allowlist` 里（绑定不能越过白名单）；`(provider_id, field)` 在同一条 profile 内不重复；`name` 不得是保留名（如 `ACP_REMOTE_*`）；`provider_id`/`field` 必须在 `owned_provider_ref` 的已配置字段内 | 本合同（`agent.configure` 写入时校验，运行期失效只影响新启动的进程） |
| `DirectoryRecord` | `{ directory_id: DirectoryId, canonical_path: String }`；目录由成功的会话创建自动登记；`canonical_path` **始终**随 Sync 快照的 `workspaces` 条目下发给已认证设备（撤销 `core.workspace-path.v1` 门控），其余对端可见输出（会话摘要、事件载荷、错误 `details`、审计）都不含它，也不进 Node Link catalog（§11.1、`SYNC_PROTOCOL.md` §9.4） | 本合同（`crates/core/src/model/config.rs`） |
| `WorkspaceCatalogEntry` | `{ directory_id: DirectoryId, path: String }`；`DirectoryRecord` 的**投影**（Sync 快照 `workspaces` 的资源条目）。`path` **始终在场**，是已登记目录的规范化路径**剥掉平台 verbatim 前缀**后的形式（`design.md` D2）；形状约束与 `DirectoryRecord.canonical_path` 相同（1..=4096 字符、不含 NUL、绝对路径形状） | 本合同（`crates/core/src/model/config.rs`）；投影入口 `UseCases::sync_workspace_catalog`（§4） |
| `ProviderRef` | `{ id: String(^[A-Za-z0-9._-]{1,64}$), kind: ProviderRefKind, display_name: String(1..=128), configured_fields: Vec<String>, keystore_ref: String, version: u64, updated_at: Timestamp }`；只记字段名、keystore 引用与版本，**永不记凭据值**；换绑必须递增 `version`（§11.2 第 7 条） | `LOCAL_ADMIN_PROTOCOL.md` §5.2；`SECURITY_DESIGN.md` §13.1 |
| `ProviderRefKind` | enum `Provider｜Mcp` | 同左 |
| `SeedState` | `{ seeded: bool, seeded_at: Option<Timestamp> }`；`seeded = false` 时启动流程才能导入种子，且该标记与种子 profile 同一事务提交（`LocalConfigStore::mark_seeded`，§5.3） | `CONFIG_REFERENCE.md`「配置与管理状态的权威」 |
| `SecretValue` | 不透明凭据值（私有字段）：只经端口进出，**不实现** `Debug`/`Serialize`，不得落盘、进事件、进日志或进错误消息 | `IDENTITY_AND_AUTH_CONTRACT.md` §7 |

`[决定]`（`access-owner-data-stateless` D1/D2）本节的值对象只承载本机配置与凭据：远程 Owner 的会话摘要、
目录 revision/generation、来源游标与命令状态**不得**写进这些记录或任何本地配置表。它们的运行期形状见
§5.9，生命期止于当前认证连接。

### 3.8 会话文件与 Git 只读视图

`session-file-browser` 的 core 值对象（`crates/core/src/model/session_files.rs`）。它们只承载根内相对文件事实；**绝对根不进值对象**（根只作为 [`SessionFiles`] 端口的入参由用例层从 `SessionRecoveryRecord.workspace_cwd` 取得）。

| 类型 | 形状 / 不变量 | 出处 |
|---|---|---|
| `SessionFilePath` | newtype over `String`；非空、≤4096 字符；拒绝前导 `/`/`\`（绝对路径与 UNC）、任意 `:`（盘符与 Windows ADS）、NUL/控制字符、`.`/`..` 分量、空路径分量（重复/尾随分隔符）以及任一分量为 `.git`（大小写不敏感）。构造即校验，失败为 `PortError::InvalidRequest` | 本合同；`SYNC_PROTOCOL.md` §11.5 |
| `SessionListPath` | `Option<SessionFilePath>`：`None` = 会话根（wire 空串），`Some` = 根内相对目录；`as_wire()` 给出请求/结果回显的原文 | 同上 |
| `SessionFileEntryKind` | enum `File｜Directory｜Link｜Other`（wire `file/directory/link/other`） | 同上 |
| `SessionFileRestrictedReason` | enum `OutsideRoot｜Unreadable｜Unsupported`（wire `outside_root/unreadable/unsupported`） | 同上 |
| `SessionFileEntry` | `{ path: SessionFilePath, name: String(1..=256), kind, openable: bool, restricted: Option<SessionFileRestrictedReason> }`；不变式：`openable ⇔ restricted.is_none()`；根外链接可列出但不可打开 | 同上 |
| `SessionFilePage` | `{ path: SessionListPath, git_state: GitState, entries: Vec<SessionFileEntry>(≤128), next: Option<SessionPageToken> }` | 同上 |
| `GitState` | enum `Available｜NotRepository｜Unavailable`（wire `available/not_repository/unavailable`）；非仓库与 Git 缺失是两种不同的成功状态 | 同上 |
| `SessionPageToken` | newtype over `String`；1..=256 个 `[A-Za-z0-9_-]`；不透明翻页位置，不授予读取权限 | 同上 |
| `ContentVersion` | newtype over `String`；1..=128 个 `[A-Za-z0-9_-]`；连接期不透明内容版本，绑定连接/设备/会话/路径 | 同上 |
| `ContentSnapshot` | `{ path: SessionFilePath, kind: SessionFileContentKind, purpose: FileReadPurpose, size_bytes: u64, version: ContentVersion, preview_truncated: bool }`；`purpose = download` 时 `size_bytes ≤ 64 MiB`，**预览不设原始大小上限**（只按文本 256 KiB / 图片 4 MiB 的预览预算截断并置 `preview_truncated`）；只在内存中，不落临时正文文件 | 同上 |
| `SessionFileContentKind` | enum `Text｜Image｜Binary`（wire `text/image/binary`） | 同上 |
| `FileReadPurpose` | enum `Preview｜Download`（wire `preview/download`） | 同上 |
| `SessionFileChunk` | `{ path, kind, size_bytes, version, offset, data: Vec<u8>, eof, preview_truncated }`；`data.len() ≤ 32 KiB` 且 `offset + data.len() ≤ size_bytes` | 同上 |
| `GitFileStatus` | enum `Added｜Modified｜Deleted｜Renamed｜Untracked`（wire `added/modified/deleted/renamed/untracked`） | 同上 |
| `GitStatusEntry` | `{ path, status, previous_path: Option<SessionFilePath>, conflicted: bool, binary: bool, line_counts: Option<(u32, u32)> }`；`previous_path` 只允许与 `Renamed` 同时出现；`line_counts` 为相对 HEAD 的（新增行数，删除行数），空基准按全量新增，二进制/冲突/无法完整统计时为 `None` | 同上 |
| `GitStatusPage` | `{ git_state: GitState, entries: Vec<GitStatusEntry>(≤128), next: Option<SessionPageToken> }` | 同上 |
| `GitDiffBase` | enum `Head｜Empty`（无 HEAD 时为空基准） | 同上 |
| `GitDiffKind` | enum `Text｜Binary｜Conflict` | 同上 |
| `GitDiffUnavailableReason` | enum `NotRepository｜GitUnavailable｜TooLarge` | 同上 |
| `GitDiff` | `{ path, base, kind, lines: Vec<String>, unavailable: Option<GitDiffUnavailableReason>, truncated: bool, comparison: Option<(String, String)> }`；`lines ≤ 400` 行、合计 ≤64 KiB、单行 ≤4096 字符；`unavailable.is_some()` 时 `lines` 必须为空。`comparison` 为（HEAD 原文，工作文件原文），仅成功文本结果可附，每侧 UTF-8 ≤64 KiB，必须完整，空侧表示新增/删除，不持久化；`truncated` 只描述补丁行 | 同上 |

固定 v1 常量（`SYNC_PROTOCOL.md` §14）：`SESSION_FILE_PAGE_LIMIT = 128`、`SESSION_FILE_PATH_MAX_CHARS = 4096`、`SESSION_FILE_CHUNK_MAX_BYTES = 32 KiB`、`SESSION_FILE_TEXT_PREVIEW_MAX_BYTES = 256 KiB`、`SESSION_FILE_IMAGE_PREVIEW_MAX_BYTES = 4 MiB`、`SESSION_FILE_DOWNLOAD_MAX_BYTES = 64 MiB`、`GIT_DIFF_MAX_LINES = 400`、`GIT_DIFF_MAX_BYTES = 64 KiB`、`GIT_DIFF_LINE_MAX_CHARS = 4096`。

### 3.9 首条提交、派发围栏与回显关联（WP2 已实现）

纯类型在 `crates/core/src/model/first_submission.rs`：`FirstSubmissionStage`（`saved_not_dispatched | dispatch_started | terminal`）、`RequestOutcome`、`FirstSubmissionInput`（有序内容块、保真 JSON 数组与同源 SHA-256 摘要）、`FirstSubmissionConfig`、`SubmissionManifestClaim`、`FirstSubmissionWrite`、`FirstSubmissionRecord`、`RequestTerminal`、`FirstSubmissionAdmission`、`DispatchFence`/`DispatchAdmission`、`FirstSubmissionSettlement`、`FirstSubmissionConvergence`、`FirstUserMessage`、`DispatchContext`/`UserEchoLink`/`UserEchoEvent`/`EchoAssociation` 与唯一关联规则 `associate_user_echo`。`FirstSubmissionId`（服务端生成、与会话一对一）与 `RuntimeEpoch`（进程运行期）在 §3.1。回显只按派发上下文匹配，文本/Agent `messageId` 不参与。

### 3.10 准备运行期与暂存（WP2 已实现）

纯类型在 `crates/core/src/model/preparation.rs`：`PreparationState`（`allocated → preparing → ready → released`）、`PreparationFailureReason`、`PreparationOwner`、`AllocatePreparation`/`AllocatePreparationOutcome`、`PreparationView`、`PreparationDecision`、`PreparationStaleReason`、`PreparationReleaseOutcome`、`PreparationFreeze`/`PreparationGrant`、`PreparationBinding` 与暂存形状（`StagedTransferBegin/State`、`StagedChunk/Outcome`、`StagedSeal/Outcome`、`SealedFacts`、`StagedManifestBegin/Page/Seal/State`、`StagedClaim`、`ClaimedAttachment`）。运行期目录 `core::preparation::PreparationDirectory`：30 秒租约（`PREPARATION_LEASE_MS`）、10 秒续租节奏、每次重建新资源代、释放失败/未知继续占位、首次提交冻结资源代与 Agent 身份（成功后名额转移给正式会话）。释放成功或首次提交成功的条目**立即从目录移除**，只留 `PREPARATION_RELEASED_TOMBSTONES` 条有界的释放事实（`(session, generation, released_at)`），因此长期运行的目录不会为每个已结束的准备保留内存项，旧代操作仍得到 `stale(released)`。全部运行期，不落库、不发事件、不写准备目录。

## 4. `core::use_cases` 用例面

`[决定]` **每个**入口的第一个参数都是 `actor: &Actor`：授权只在 core 判定，`server` 不得传入“已授权”标志，也不得代替 core 判定（`AGENTS.md` §4/§5、`SECURITY_DESIGN.md` §8）。全部 mutation 只同步接受。

| 用例族 | 入口 | 调用方 |
|---|---|---|
| `SessionCommands` | `submit_command(actor, ClientCommand) -> CommandReceipt` | `server::sync`、`server::node_link`、`server::acp_facade` |
| `SessionLifecycle` | `create_session(actor, RequestId, Digest, CreateSessionRequest, Option<WorkspaceSource>) -> SessionCreateAdmission`、`settle_session_create(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool`、`resume_session(actor, RequestId, Digest, SessionId) -> SessionId`、`settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` | `server::node_link`（§6 第 20 条；`resume_session`/`settle_session_resume` 见 §5.1） |
| `LocalSessionLifecycle`（计划，WP2 实施；用例面**恰好由 §5.10 的五个端口组合**：`SessionActivation`/`ViewRegistry`/`CapacityAdmission`/`SessionRelease`/`LifecycleQuery`，方法名与签名以 §5.10 为唯一权威） | 每个入口先取 `actor`，再委托同名端口方法：`activate_session(actor, ActivationRequest) -> ActivationAdmission`（→ `SessionActivation::activate`；同会话并发激活合并为一次建立，先查幂等记录，再判 ready 复用或启动恢复）、`settle_session(actor, SessionId, ResourceGeneration, ActivationOutcome)`（→ `SessionActivation::settle`）、`declare_view(actor, ConnectionId, Option<SessionId>, view_version, now) -> ViewDecision`（→ `ViewRegistry::declare`）、`renew_view(actor, ConnectionId, view_version, now) -> ViewDecision`（→ `ViewRegistry::renew`）、`release_view(actor, ConnectionId, now)`（→ `ViewRegistry::release`；离页/断开/撤销）、`reserve_capacity(actor, SessionId, now) -> Admission`（→ `CapacityAdmission::reserve`）、`release_capacity(actor, SessionId, ResourceGeneration, now)`（→ `CapacityAdmission::release`）、`reclaim_candidate(actor, now) -> Option<ReclaimCandidate>`（→ `CapacityAdmission::reclaim_candidate`；满额时的安全候选，**没有候选就是 `None`**）、`release_session(actor, SessionId, ResourceGeneration, now) -> ReleaseOutcome`（→ `SessionRelease::release_session`；只有 `Released` 交还名额）、`current_activity(actor, SessionId) -> LocalActivity`（→ `LifecycleQuery::local_activity`）、`lifecycle_snapshot(actor) -> Vec<SessionSummary>`（→ `LifecycleQuery::snapshot`；owned 当前查询，与快照同一代目录） | `server::sync`（激活命令、查看控制帧、快照/事件投影）；`app::daemon` 只按返回的候选与配置驱动回收，不做业务判断 |
| `SessionQueries` | `list_sessions(actor, SessionQuery) -> Vec<SessionSummary>`、`read_session(actor, ReadQuery) -> HistoryPage` | 同上 |
| `ConfigCommands` | `set_mode(actor, SessionReference, ModeId) -> Version`、`set_config(actor, SessionReference, ConfigOptionId, ConfigValue) -> Version` | 同上 |
| `PermissionCommands` | `resolve_interaction(actor, SessionReference, InteractionId, InteractionResolution) -> Resolution` | 同上 |
| `SubscriptionQueries` | `replay(actor, SessionReference, Option<GlobalCursor>, ReplayLimit) -> ReplayBatch`、`retention_window(actor, SessionId) -> Option<(Sequence, Sequence)>`、`sync_replay(actor, Option<GlobalCursor>, ReplayLimit) -> SyncReplaySlice`（设备主体恒通过、节点按 `grant.observe`；§5.2） | `server::sync`、`server::node_link` |
| `DeviceManagement` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4 的 `device.*`/`node.*` | `server::local_admin`、`app::cli` |

| `RemoteCatalogQueries` | 见 `LOCAL_ADMIN_PROTOCOL.md` §5.5 读取方法与 `NODE_LINK_PROTOCOL.md` §12.3 catalog 投影 | 同上、`server::node_link` |
| `PairingChannel` | `claim_pairing(actor, PairingClaim) -> PairingClaimOutcome`、`pairing(actor, PairingId) -> Option<PairingRecord>`、`pairing_channel_view(actor, PairingId) -> Option<PairingChannelView>`、`consume_pairing(actor, PairingId) -> PairingRecord` | `server::node_link`（及未来 `server::sync`）；前两项另由 `server::local_admin` 以 `Actor::LocalCli` 调用 |
| `PairingMaintenance` | `pairings(actor) -> Vec<PairingRecord>`、`expire_pairings(actor) -> u64`、`expire_pairings_by_id(actor, &[PairingId]) -> u64` | `app::daemon`（启动恢复与周期维护，`Actor::LocalCli`；`pairings` 只读、`expire_pairings_by_id` 只终结未确认标识） |
| `NodeLinkHandshake` | `node_link_handshake_view(accessNodeId) -> NodeLinkHandshakeView`、`record_node_link_auth(accessNodeId, AuditAction, AuditOutcome) -> ()`、`record_node_connected(accessNodeId) -> ()` | `server::node_link`（及未来 `server::sync` 的同形入口） |
| `SyncDeviceHandshake` | `sync_device_handshake_view(deviceId) -> SyncDeviceHandshakeView`、`record_device_connected(deviceId) -> ()`、`record_device_auth_failure(deviceId) -> ()` | `server::sync` |
| `SyncSnapshotQueries` | `sync_visible_sessions(actor, SyncSessionQuery, owner_reachable) -> Vec<SessionSummary>`（远程部分来自运行期目录，见 §5.9；`owner_reachable` 为保留的装配参数，不再决定在线状态）、`sync_workspace_catalog(actor) -> Vec<WorkspaceCatalogEntry>`（快照的 `workspaces` 资源：每条都带目录标识与规范化路径；`core.workspace-path.v1` 已撤销，没有协商参数） | `server::sync`（快照的 `sessions`/`workspaces` 资源） |
| `RemoteSessionQueries` | `read_imported_session(actor, &SessionId, ImportedReadQuery) -> Option<ImportedRead>`（旧 wire 原文路径；先经运行期目录校验来源身份/在线）、`read_remote_source(actor, &RemoteSessionRef, RemoteReadQuery) -> RemoteReadResult`（D4 的类型化回源，含 `readCursor`/`messageCoverage`）、`remote_command_status(actor, RemoteCommandTarget, RequestId) -> RemoteCommandLookup`（D5：按原 requestId 直接向 Owner 查询，不读本地请求表） | `server::sync`、`server::node_link`（旧入口保留到 WP4 切换） |
| `WorkspaceBrowse` | `workspace_browse(actor, Option<&str>) -> WorkspaceBrowse`（`workspace.browse`：`None` = 根列表；条目只含名称、绝对路径与是否已登记，按名称升序、单层上限 128 并置截断标记；受 feature `core.workspace-browse.v1` 门控，门控在适配层） | `server::sync` |
| `SessionFilesQueries` | `list_session_files(actor, &SessionId, &str, bool, Option<&str>) -> SessionFilePage`、`read_session_file(actor, &SessionId, &str, FileReadPurpose, u64, Option<&str>) -> SessionFileChunk`、`session_git_status(actor, &SessionId, Option<&str>) -> GitStatusPage`、`session_git_diff(actor, &SessionId, &str) -> GitDiff`（授权 → 来源 → 实际 cwd → 形状 → `SessionFiles` 端口；imported 在端口前拒绝，查询无终态/无事件） | `server::sync`（feature 门控在适配层） |

`[决定]` 配对通道的 actor 规则（design D12）：`claim_pairing`/`pairing`/`pairing_channel_view` 在 `Actor::LocalCli` 之外**只**接受 `Actor::PairingClaimant`，且 `actor.pairing` 必须等于本次调用的目标配对，否则与其它 actor 一样得到 `authorization.scope_denied`（同一拒绝形状，不区分「不是本机入口」与「绑定了别的配对」）；`consume_pairing` **只**接受与该配对已批准对端一致的 `Actor::Node`/`Actor::Device`，把 `approved` 推进到 `consumed`（`terminal_at` + 审计同一写集），已是 `consumed` 且对端一致时幂等成功（不覆盖首次 `terminal_at`，也不重复写审计）。

`[决定]`（2026-09-26，seam 补全）`pairing_channel_view` 是配对 HTTP 端点的**唯一**只读入口：它一次带回记录、已认领的对端行与已批准后的对端节点行（`grant.*` 的唯一来源，`PairingRecord` 不带 `granted_*`），不含任何写入，也不返回秘密材料（pairing secret 只在状态机内存）。认领路径允许在 proof 校验**之前**读取（端点必须先拿到记录才能校验 HMAC），但状态推进仍只能经 `claim_pairing` 的写集、且只在 proof 通过后提交；拒绝仍收集在同一类 `authorization.scope_denied`。入口绑定式读取（claimant 只能读自己那个配对的三类行）是硬约束：不允许放开为通用只读。

`[决定]`（2026-09-26，WSS 握手 seam 补全；同一方向的第二次补全）`NodeLinkHandshake` 是 WSS 握手准入的三条窄入口，服务 `design.md` D3 与 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1：

- `node_link_handshake_view(access_node)`：用例面**第一条**没有 `actor` 的入口——握手完成前不存在已验证主体，`node.hello` 里的 `accessNodeId` 只是待验证的自报身份。授权面因此收窄到「单个自报 node id」：只读该 id 自己的 `access` 信任行、已绑定验签公钥、最近一次配对（`TrustStore::pairing_for`）与本机全局水位 `store.head()`；未知 id 返回**空视图**而不是错误（`NODE_LINK_PROTOCOL.md` §12.2：不得用错误区分节点是否存在），零写入、零审计、不含秘密材料。`Revoked`/`Unknown` 由调用方按信任行状态映射为凭据状态（§5.1：不是握手失败）。视图形状 `NodeLinkHandshakeView { node: Option<NodeRecord>, public_key: Option<PeerPublicKey>, pairing: Option<PairingRecord>, head: GlobalCursor, catalog_revision: u64 }`：五项都是握手本次调用需要且只需要的持久事实；`head` 是 `serverEpoch` 的来源，`catalog_revision` 是 `node.challenge.catalogRevision` / `node.ready.catalogRevision` 的来源。
- `record_node_link_auth(access_node, action, outcome)`：只接受 `node.authenticated`/`node.auth_failed`/`authorization.denied`（其余 → `authorization.scope_denied`），归因 `actor`/`via_node` 都是该对端 id、`target = Node(对端)`、`localPrincipalRef` 为 `None`（§8.3 节点级信任）。首次认证成功（已批准配对 → `consumed`）的那条 `node.authenticated` 由 `consume_pairing` 的写集提交，适配器不得再补一条。`authorization.denied` 服务命令管线：`server::node_link` 的可见性/授权交集比 `Broker::authorize` 的 Owner 侧判定更严，被它拒的命令不会到达 broker，拒绝留痕只能经本入口（`design.md` D8 第 2 步）。
- `record_node_connected(access_node)`：**重复认证**（没有待消费配对）的收尾写集（§11.6 第 9 条）——把 `(access_node, access)` 行的 `last_connected_at` 推进到本次时间（只前进不倒退）并把 `node.authenticated` 在同一事务提交；归因与上一条逐字一致。首次认证的同一职责由 `consume_pairing` 的写集承担，两者不得互相替代（否则 `lastConnectedAt` 会停在首次认证的时刻）。

`[决定]`（2026-09-26，`node-link-owner` 变更 D4/G5 的实现期结论）**`catalogRevision` 的唯一来源是 [`AuditStore::watermark`]**（本机审计自增序列，`storage-sqlite` 取 `sqlite_sequence.owned_audit`），不再用 `store.head()`：全局水位会随保留期裁剪回退，而 `catalogRevision` 要跨重启与清理单调。`node.challenge`/`node.ready`/`catalog.snapshot` 三个字段同源（实测不变式：同一次握手的挑战与 `node.ready` 必然相等，因为两者用同一份视图快照）。**已登记的精度边界**：该水位随每一行审计前进，包括 `node.auth_failed`/`authorization.denied`/`rate_limit.triggered` 这类不改变导出目录的动作，因此「revision 未变 ⇒ 目录未变」并不成立（反向「目录变了 ⇒ revision 必变」成立）。v1 不依赖前者：`knownRevision` 非空时仍回完整快照（`catalog.changed` 属 `post_mvp`）。

`[决定]`（2026-09-26，同一变更）**Node Link 资源读面的三条端口 seam（`ReadView::node_link_slice`/`ReadView::session_event_payload`/`AuditStore::watermark`）与它们的四个用例入口（`node_link_catalog_view`/`node_link_session_view`/`node_link_replay`/`node_link_event_payload`）**（见 `server::node_link` 的 catalog/resource 模块）：与 `NodeLinkHandshake` 同一模式——无 `actor`、按已认证 `accessNodeId` 绑定、零写入、零审计；会话级读只给出 `(access_node, session)`，由 core 硬校验「对端是已配对的 `access` 行 + 会话存在」，**不得**复用 `Broker::authorize` 的 `Actor::Node` 分支（那是 Access 侧本地客户端的语义）。「这个 Export 是否对该节点可见」是适配器的**单点可见性策略**（`D14`；2026-09-27 起为三条件：未撤销 ∧ `export.scopes ∩ 节点 grants ≠ ∅` ∧ `exportId ∈ 节点信任记录的 exportIds`，权威定义见 `NODE_LINK_PROTOCOL.md` §8.2 与本文件 §3.5），不在 core 重复实现；core 的 Owner 侧授权判定（§6 第 5 条）必须与它同口径。

四个用例入口都不放宽既有 `LocalCli` 路径的行为，也不新增任何对端可见的能力。

`[决定]`（2026-10-14，`sync-server-connection` 变更 WP1）`SyncDeviceHandshake` 是 **Sync** 侧 WSS 握手准入与认证收尾的三条窄入口，与 `NodeLinkHandshake` 逐条同形（`design.md` D2；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1）：

- `sync_device_handshake_view(device_id)`：**未认证**的只读入口，与 `node_link_handshake_view` 同一模式（不带 `actor`）——`sync.hello` 里的 `deviceId` 只是待验证的自报身份，授权面收窄到「单个自报 device id」：只读该 id 自己的设备信任行（`TrustStore::device`）、已绑定验签公钥（`TrustStore::peer_key`，`verify_proof` 唯一接受的密钥来源）、最近一次配对（`TrustStore::pairing_for`）与本机全局水位（`SessionStore::head()`，`serverEpoch`/`globalSequence` 的来源）。**未知 id 返回结构完整的空视图**（`device`/`public_key`/`pairing` 都是 `None`，`head` 照常给出）而不是错误：`SYNC_PROTOCOL.md` §8.2/§8.3 与 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1 都要求未知对端照常签发挑战、对端可见的失败映射推迟到 proof 阶段，用错误区分设备是否存在会把它变成设备 id 预言机。零写入、零审计、不含秘密材料；视图形状 `SyncDeviceHandshakeView { device: Option<DeviceRecord>, public_key: Option<PeerPublicKey>, pairing: Option<PairingRecord>, head: GlobalCursor }`——比 `NodeLinkHandshakeView` 少一项 `catalog_revision`（那是 Node Link 的目录投影源，设备握手不用它）。`revoked` 由调用方按 `device.state()` 映射为凭据状态，不是读取失败；认证收尾不在本入口（见下一条）。
- `record_device_connected(device_id)`：认证**成功**的收尾写集（`SYNC_PROTOCOL.md` §8.4；`SECURITY_DESIGN.md` §14.2 的 `device.authenticated`）——把该设备的 `last_seen_at` 推进到本次时间，并把该设备的 `device.authenticated` 成功行在**同一事务**提交（`WriteContext` 同时携带两者，§11.2 第 6 条）。本入口没有 `actor` 参数：归因由 `device_id` 唯一确定，审计行的 actor 取 `Actor::Device { device: 该 id }`（设备不再有授权集合，因此没有 scopes 可归因，也不得取对端自报值）。`last_seen_at` 的**只前进**三分支（旧值为空 → 写入新值、新值为空 → 保留旧值、两者非空取较大者）与「`revoked` 记录不得复活」都由存储层的 `put_device`/`upsert_device` 判定，本层只读出既有行、置 `Some(now)` 并把**其余每一列原样写回**（不得用 `None` 抹掉已存值），也不预先判定撤销。**设备行不存在 → `NotFound(EntityRef::Device)` 且零写入**（`DeviceWrite` 携带的是整行，编造一行等于伪造信任）。`owned_device.last_seen_at` 的**唯一生产者是本入口**——首次认证（有待消费配对）也必须经它推进。
- `record_device_auth_failure(device_id)`：认证**失败**的留痕（`SECURITY_DESIGN.md` §14.2 的 `device.auth_failed`；`SYNC_PROTOCOL.md` §8 的握手失败路径）。动作**恒为** `AuditAction::DeviceAuthFailed`（不接受动作参数，因此结构上写不出别的动作）、目标恒为 `Device(device_id)`、结果恒为 `AuditOutcome::Failed`，`via_node`/`local_principal_ref`/`detail_digest` 均为空（细分失败原因只进结构化日志，§14.1/§14.2 的 no-content 取舍）。**已知设备**与**未知设备**写出**形状相同**的审计行（都只按 `device_id` 归因，不带任何授权集合），**不得返回 `NotFound`**——未配对设备发起握手并失败是最常见的失败场景，正是本入口存在的主要理由，而「未知 → 错误 / 已知 → 成功」的响应差异会变成设备存在性预言机（这条与 `record_device_connected` 刻意不同：那个入口的整行写集要求行存在）。两种情形都返回 `Ok(())` 且各写一行形状相同的审计。本入口没有关联状态变更，故直接用 `AuditStore::append` 追加、不新增端口方法（§11.2 第 6 条的后半句）；成功路径的 `device.authenticated` 由 `record_device_connected` 承担，两者**不得互相替代**。

`[决定]`（2026-10-14，同一变更；2026-10-21，`access-owner-data-stateless` WP2 改写来源）`SyncSnapshotQueries` 的 `sync_visible_sessions(actor, query, owner_reachable)` 一处（另一端是 `sync_workspace_catalog`，见本节的目录资源条）：Sync 快照 `sessions` 资源的来源（`SYNC_PROTOCOL.md` §9.6；spec `sync-event-delivery`「设备可见的会话清单范围」、`stateless-access`「远程数据只用于当前在线处理」）。它先按 `session.list` 走 `Broker::authorize`——与 `list_sessions` **逐字同口径**（同一命令名、同一 broker 判定、同一 `authorization.scope_denied`；未授权时不读会话/目录、也不返回任何会话）；通过后取自有会话（`SessionStore::list`，条目的 `workspace` 取自该行**持久化**的 `workspace_alias`，不得按 `workspace_cwd` 反查），并在 `include_imported` 为真时向 §5.9 的 `RemoteRuntimeDirectory::list` 取当前已认证连接期的远程条目，原样纳入其已在目录中组装好的 `SessionSummary`。**远程条目不再来自 `RemoteDeliveryStore::list_sessions`**：持久 imported 索引在本用例里一次都不读；`SyncSessionQuery` 的 `owned`/`imported` 只分别收窄自有列表与运行期目录结果，`include_imported` 是 feature `resource.remote-origin.v1` 的**协商结果**（core 不读 feature 表、也不缓存它）——为假时一行远程摘要都不得出现，连空占位也不给（`Default` 因此取 `false`）。顺序是自有在前、远程在后，本层不重排（`SessionSummary` 没有排序语义字段）。`owner_reachable` 是**保留的装配兼容参数**（WP4 会从 `server` 移除）：在线状态以目录条目的 `source`/`origin.online` 为唯一权威，本层不再用闭包覆盖它；目录未装配时远程部分为空，来源断开/撤销时其整代目录已被 `invalidate` 清除，因此某个不可达 Owner 不会被表达成「成功读到空清单」——来源状态经连接期通知与 `origin.online` 表达。合并必须发生在 core：授权只在 core 判定、`server` 不得代替 core 判定（本节开头的 `[决定]`），适配层分别取行再自行拼接会形成第二条可见性判定路径。

`[决定]`（2026-10-17，`sync-path-browse-and-diff-delivery` 变更 WP2）**目录浏览、目录资源与建目录开关的三条口径**（spec `sync-workspace-browse`、`sync-workspace-catalog` 与 `sync-session-create`；`design.md` D1–D4/D7）：

- **`workspace_browse(actor, Option<&str>)`**（`workspace.browse`）：先按 `workspace.browse` 走 `Broker::authorize`（`required_grant` 把它归入 `grant.observe`，§6 第 5 条的命令表），再按 §5.1 的**同一五步顺序**校验父路径——非绝对 → 含 `..` → 存在 → 是目录 → `canonicalize`；前两步是参数类错误（`InvalidRequest`）且判定在**任何文件系统访问之前**（端口一次都不会被调用，这条有测试断言），后三步失败是服务端不可用类（`Unavailable(IoError)`）；两类都不返回条目，也不回退到父层或根列表。顺序上「先拒 `..` 再 `canonicalize`」不可交换：Windows 的 `canonicalize` 不归一化 `..`。结果只含名称、绝对路径与是否已登记：结果的 `path` **逐字回显请求的父路径原文**（`None` = 根列表；请求字段与结果回显字段逐字相同是 `SYNC_PROTOCOL.md` §11.5 与 `command.schema.json#/$defs/workspaceBrowsePath` 的硬要求，规范化结果因此只用于调用端口与 `registered` 判定），每个**条目**的 `path` 取**下发形式**（`canonicalize` 的结果剥掉平台 verbatim 前缀），`registered` 由本层把条目的下发路径与已登记目录的下发路径比较得出（登记表的路径不下发），条目按名称的 Unicode 码点升序排列（名称相同再按路径，保证同一层两次调用同序），超过**固定**上限 128（§14）时只返回上限内条目并置 `truncated`——不静默丢弃，也不因超限整次失败。只读：不登记、不缓存、不写会话状态、不产生审计动作；命令本身受 feature `core.workspace-browse.v1` 门控，门控在适配层且必须在触碰文件系统之前（`SYNC_PROTOCOL.md` §11.5）。`None` 的父路径请求根列表（走端口的根列举），条目格式与某一层一致。
- **`sync_workspace_catalog(actor)`**：Sync 快照 `workspaces` 资源的投影源；每条都带 `directory_id` 与 `path`，取值是**已登记目录**的 `canonicalize` 结果**剥掉平台 verbatim 前缀**后的形式（`design.md` D2，复用唯一的 `strip_verbatim`）。`core.workspace-path.v1` 已撤销，core 不再读 feature 表、也没有 `include_path` 协商参数。路径只出现在这些条目上：会话摘要的 `workspace` 引用只有 `{ directoryId }`，客户端按目录标识在同一份快照内查表。
- **`create_session` 的目录来源增 `WorkspaceSource::PathWithCreate`**（客户端提供的目录 + 显式建目录开关，`design.md` D4）：开关与目录标识**互斥**在类型上成立（目录标识指向的目录必然已存在），因此没有「目录标识 + 开关」的传参形状，也不需要运行时判定。开关为真时目录可以不存在：本层先 `create_dir_all`（缺失父层一并创建）再 `canonicalize` 并把结果作为权威值，**然后**才调 `SessionBackendFactory::create`；路径上已有成分不是目录、父层不可达、无写权限或随后的规范化失败一律是服务端不可用类（`Unavailable(IoError)`），**不覆盖、不删除**既有内容。被创建的目录与成功的会话在**同一次提交**里登记（目录因此出现在下一次快照的 `workspaces` 里）；后端启动或落账提交失败时**不删除、不回滚**已建目录——删除要区分「哪些层是这次建的」，误删用户目录的代价远高于留下一个空目录。

## 5. `core::ports` 出站端口

`[决定]` 下列签名是**冻结形状**：实现可增加私有辅助方法，但不得改变既有方法的语义与参数；新增公共方法必须先改本合同。

### 5.1 会话后端

```rust
#[async_trait]
pub trait AgentCatalog: Send + Sync {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError>;
    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError>;
    async fn agent_capabilities(&self, agent: &AgentRef) -> Result<CapabilitySet, PortError>;
}

#[async_trait]
pub trait SessionBackendFactory: Send + Sync {
    /// `session` 是 core 已在存储里创建好的 `SessionId`（来自 `CommitOutcome.session_id`），后端**必须**用它
    /// 构造 `SessionEndpoint::reference()`；`request` 携带完整目标（agent/workspace/模板/origin）；`sink` 用于
    /// 交付后端事件。
    async fn create(&self, session: &SessionId, request: CreateSessionRequest, sink: EventSink) -> Result<Box<dyn SessionEndpoint>, PortError>;
    async fn open(&self, reference: SessionReference, sink: EventSink) -> Result<Box<dyn SessionEndpoint>, PortError>;
    /// 进程不在的会话恢复（`session.resume`）：按持久化取值重新拉起 Agent 子进程并发送
    /// `session/resume { sessionId, cwd }`；能力门控（未宣告时**不得发送**该请求、并在返回前回收本次拉起的
    /// 子进程）与「同一会话只有一条活跃绑定」由实现负责。
    async fn resume(&self, session: &SessionId, request: ResumeSessionRequest, sink: EventSink) -> Result<Box<dyn SessionEndpoint>, PortError>;
    /// 结束该会话所属的 Agent 运行时（进程树与映射一并回收），此后该运行时**不得再被复用**：唯一调用方是
    /// 窗口期事件队列超限的失败关闭（§6 第 20 条）。没有任何运行时时是成功的空操作。
    async fn terminate_agent(&self, session: &SessionId) -> Result<(), PortError>;
}

#[async_trait]
pub trait SessionEndpoint: Send + Sync {
    fn reference(&self) -> SessionReference;
    /// 本次绑定对应的 Agent 侧会话标识（创建后由 core 落盘，恢复得到的端点同样可读）；未取得时为 `None`，
    /// 实现**不得**编造占位值。
    fn agent_session_id(&self) -> Option<&AgentSessionId>;
    async fn prompt(&self, request: PromptRequest, at: Timestamp) -> Result<TurnAccepted, PortError>;
    async fn validate_prompt(&self, request: &PromptRequest) -> Result<(), PortError>;
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
- `[决定]` **workspace 解析归 core**（§3.6 的 `CreateSessionRequest.workspace` 是 `Option<ResolvedWorkspace>`）：`UseCases::create_session(actor, requestId, requestFingerprint, request, Option<WorkspaceSource>)` 在调用 `SessionBackendFactory::create` **之前**完成 alias → 规范化绝对路径的解析与校验，后端只收到 `ResolvedWorkspace`，**不得**自己查存储、也不得按约定拼路径。`requestId` 与 `requestFingerprint` 由适配层传入（Node Link 的幂等键是 `(ownerNodeId, accessNodeId, requestId)`，指纹是 ACPR-CJ1 之后的解码 payload 摘要）：core **不得**自造 requestId 或指纹，否则同一次重试会得到第二个幂等键（§6 第 20 条）。终态由 `UseCases::settle_session_create(actor, requestId, status, result, error)` 提交（没有持久记录或记录已终结时是幂等 no-op），`completed` 的 `result` 由适配层投影（Node Link 的 `SessionCreateResult` 原文）。校验（与 `SECURITY_DESIGN.md` §12.3 同口径）：必须是绝对路径、必须存在、必须是目录；`canonicalize`（解析 symlink/junction/大小写/`.` 与 `..`）的结果作为权威值，拒绝相对路径与含 `..` 的输入。失败分类：alias 未在该 Export 中声明 → 参数类错误（`NODE_LINK_PROTOCOL.md` §12.7 的 `nodelink.export.not_granted`）；alias 已声明但**本机**解析失败（目录被删/不是目录/`canonicalize` 失败）→ `PortError::Unavailable(UnavailableKind::IoError)`，在线映射为服务端错误（`nodelink.internal.unavailable`），**不得**降级为参数错误。别名命名空间：Export 的 `workspace_aliases[].alias` 就是本机 `owned_workspace.alias`，Export 不复制路径，`export.create` 必须校验每个 alias 已存在。`canonical_path` 只出现在该调用入参里：不进事件、错误 `details`、审计 `detail_digest` 的前像或 Node Link catalog。UNC/网络路径允许解析且不改变授权模型，是否记结构化警告由 `server` 层决定（core 不引入日志依赖）。
- `[决定]` **`resume_session` 的用例顺序与终态归属**（§4 的 `SessionLifecycle`；`session.resume` 与 `session.create` 同形：`CommandPayload` 里没有对应变体，`accepted` 行与幂等行由该用例在**同一事务**里自建，幂等行的 `session` 指向目标会话）：① 授权（`grant.remote-work`，与 `session.create` 同口径）——授权先于一切本机读取与文件系统访问，未授权时不得读取会话行、不得区分会话是否存在；② `SessionStore::load_recovery` 窄读取；两列为 `NULL`（或不存在恢复数据）时与「后端不支持该操作」走**同一条路径**（`Unavailable(BackendUnsupported)`），**不启动进程**、不降级为新建会话；③ cwd 复校验（同创建口径：绝对、存在、是目录，且 `canonicalize` 的结果与持久化取值**逐字相同**）——失败返回 `Unavailable(IoError)`（服务端不可用类），**不**回退到按别名重新解析、也不改用「最接近」的目录；④ `SessionBackendFactory::resume`（唯一可能 spawn 的副作用）；⑤ 只返回 `SessionId`（会话重新可交互），**不**投影、**不**写终态。终态由适配层用与 `session_create_result` 同源的映射投影（`remoteSessionRef.exportId` 只有适配器有）后经 `UseCases::settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` 提交；该入口与 `settle_session_create` **同形**，但**只**终结 `command == "session.resume"` 的持久记录（该 `(actor, requestId)` 记着别的命令时 `InvalidRequest` 且零写入），**没有持久记录或记录已终结时是幂等 no-op**（返回 `false`），落盘失败不报成功（行仍 `accepted`，由启动恢复按 §6 第 16 条终结为 `uncertain`）；投影失败时适配层按既有 `session.create` 模式结 `uncertain`（不用 `failed` 说谎）。

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

pub struct HistoryInclude {
    pub messages: bool,
    pub turns: bool,
    pub pending_interactions: bool,
    pub config_options: bool,
    pub capabilities: bool,
    /// 结构化历史条目（`SYNC_PROTOCOL.md` §11.5 的 `history` 取值）：开启时 `HistoryPage.history` 给出
    /// 该会话的一页条目，关闭时它为空。空只由本开关决定，不代表「该会话没有历史」。
    pub history: bool,
}

pub struct HistoryQuery {
    pub session: SessionId,
    pub include: HistoryInclude,
    /// `messages` 面的分页起点：只返回该 cursor 之后的事件；`None` = 从头。
    pub after: Option<GlobalCursor>,
    /// 结构化历史条目的分页游标：**会话内**事件序号（§11.5 的 `beforeHistory`），只返回会话内序号
    /// **严格小于**它的条目；`None` = 最新一页。取值只能来自上一页最早条目的 `session_sequence`；
    /// 早于该会话仍可读条目的下界（行已被压缩/清理）时实现返回 `InvalidRequest`。
    pub before_history: Option<Sequence>,
    /// 消息正文页的游标：`(createdAt, messageId)` 复合游标（`SYNC_PROTOCOL.md` §11.5 的
    /// `beforeMessages`），只返回 `(created_at, message_id)` **严格早于**它的 `owned_message` 行；
    /// `None` = 最新一页。与 `before_history` 各自独立分页。
    pub before_messages: Option<MessageCursor>,
    /// 消息正文页大小；`0` = 本请求不取 `HistoryPage.messages`。
    pub messages_limit: ReplayLimit,
    pub limit: ReplayLimit,
}

/// 会话消息的分页游标：`owned_message` 没有会话内序号，正文页按「创建时间 + 标识」稳定排序。
pub struct MessageCursor {
    pub created_at: Timestamp,
    pub message_id: MessageId,
}

/// owned 事件 + 其持久化正文：定位与正文严格同源（同一条行 / 同一次读视图），调用方不需要逐条再取一次。
pub struct OwnedEventRecord {
    pub event: CommittedEvent,
    pub payload: EventPayload,
}

/// Sync 事件读面的条目（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧）：定位 + 持久化正文 + 三个**只有存储
/// 行能回答**的来源字段。`OwnedEventRecord`/`CommittedEvent` 的形状不变（它们有大量现存调用方），
/// `origin_kind`/`origin_device_id`/`origin_node_id`/`causation` 由本条目承载。
///
/// `remoteOrigin`（同属 §10.1 的 required 清单）没有载体：本读面只读 `owned_event`，imported 事件的
/// 跳数与来源由投递层的 imported 路径另行投影，因此调用方按「本读面返回的都是 owned 事件 ⇒
/// `remoteOrigin` 恒为 `null`」投影（§9.6）。
pub struct SyncEventRecord {
    pub event: CommittedEvent,
    pub payload: EventPayload,
    /// `origin.kind`：取 `owned_event.origin_kind`，**五个**取值（`agent`/`device`/`daemon`/
    /// `local_cli`/`node`）原样往返；词表外取值是列值损坏 → `Corrupt`。**不**得按「这条事件有没有
    /// `session_id`」推断来源。
    pub origin_kind: EventOrigin,
    /// `origin.deviceId`：**仅** `origin_kind = device` 时非空。取值来自 `owned_event.causation`
    /// （= 诱发命令的 requestId）到 `owned_command.actor_id`（`actor_kind = 'device'`）的回查；
    /// 得不出唯一取值时**整批读失败关闭**（`Corrupt`），见下面 §5.2 的 `[决定]`。
    pub origin_device_id: Option<DeviceId>,
    /// `origin.nodeId`（feature `core.event-origin.v1`）：**仅** `origin_kind = node` 时非空。与
    /// `origin_device_id` 同源同口径，只是回查 `actor_kind = 'node'` 且取复合键
    /// `"{ownerNodeId}/{accessNodeId}"` 里的主 principal `ownerNodeId`。
    pub origin_node_id: Option<NodeId>,
    /// `causationRequestId`：`owned_event.causation` 原样；没有命令直接导致的事件为 `None`。
    pub causation: Option<RequestId>,
}

/// Sync 事件读面的一个批次（`ReadView::sync_replay` 的返回值）：`after` 之后的一页事件 + 生成这批数据
/// 时的一致性视图 head（`sync.caught_up` 的屏障游标），同一次只读事务内取得。
pub struct SyncReplaySlice {
    pub events: Vec<SyncEventRecord>,
    pub head: GlobalCursor,
}

pub struct HistoryPage {
    pub session: SessionSummary,
    /// `messages` 面的原料：`after` 之后的一页事件（**含持久化正文**），按 `global_sequence` 升序。
    pub events: Vec<OwnedEventRecord>,
    /// 结构化历史条目（**含持久化正文**），按 `session_sequence` 升序；`include.history` 为假时为空。
    pub history: Vec<OwnedEventRecord>,
    /// `history` 之前是否仍有更早条目（§11.5 的 `historyHasEarlier`）；关闭 `history` 时恒为假。
    pub history_has_earlier: bool,
    /// 该会话的权威消息正文（`owned_message`）：按 `(created_at, message_id)` 升序的**一页**，最多
    /// `HistoryQuery::messages_limit` 条；`include.messages` 为假或 `messages_limit = 0` 时为空。
    /// 媒体引用只查本页消息（`IN (…)`），不做整会话扫描。
    pub messages: Vec<MessageRecord>,
    /// `messages` 之前是否仍有更早消息；不是消息页时恒为假。
    pub messages_has_earlier: bool,
    pub turns: Vec<Turn>,
    pub interactions: Vec<PendingInteraction>,
    pub config: Vec<ConfigOption>,
    pub capabilities: Option<CapabilitySet>,
    /// 本页数据对应的一致性视图 head（barrier）。
    pub head: GlobalCursor,
    /// 续读游标（只服务 `events`）；`None` = 已到 `head`。
    pub next: Option<GlobalCursor>,
}

pub struct SubmissionBinding {
    pub submission: SubmissionId,
    pub attachments: Vec<AttachmentId>,
    /// 认证主体：存储层在受理事务内与清单行逐项比较（直接调用 `commit_with_submission` 也绕不过）。
    pub subject: AttachmentSubject,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError>;
    /// 与 `commit` 同一原子提交，并在同一事务里绑定 prompt 的附件清单（§5.8/§6 第 22 条）。
    async fn commit_with_submission(&self, commit: OwnedCommit, binding: Option<SubmissionBinding>) -> Result<CommitOutcome, PortError>;
    /// 仅 workspace.mkdir 的无会话终态；默认实现拒绝。按主体与 requestId 精确定位受理行。
    async fn commit_workspace_mkdir_terminal(&self, actor: &Actor, commit: OwnedCommit) -> Result<CommitOutcome, PortError>;
    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError>;
    /// 会话列表。每个 `SessionSummary.workspace` **必须**取自该会话行持久化的
    /// `owned_session.workspace_alias`（§3.1 的 `WorkspaceRef`）：展示名按别名关联目录记录，取不到时回退为
    /// 别名本身；别名为 `NULL` 时就是 `None`（未分组），MUST NOT 按 `workspace_cwd` 反查别名补齐。
    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError>;
    /// 批量读取会话的 origin epoch（`owned_session.origin_epoch`）。唯一生产调用方是 Node Link 的
    /// catalog 投影（`NODE_LINK_PROTOCOL.md` §12.3 的 `catalog.session.originEpoch`）；不存在的标识
    /// 不出现在结果里，调用方按「未知」处置、不补值。只读、无正文、不改 cursor。
    async fn origin_epochs(&self, sessions: &[SessionId]) -> Result<Vec<(SessionId, OriginEpoch)>, PortError>;
    /// 会话活跃程度归约的全部输入（`SessionActivityFacts`）：有效未解决交互数、已仲裁未确认的标识、
    /// 按 `turn_id` 分组的未解决交互数，以及未结束 turn 数。读口径见本节 `[决定]`。
    async fn session_activity(&self, session: &SessionId) -> Result<SessionActivityFacts, PortError>;
    /// 该会话的持久队列（`owned_turn.state = 'queued'`），按 `queue_index` 升序。
    async fn queued_turns(&self, session: &SessionId) -> Result<Vec<Turn>, PortError>;
    /// 启动恢复用：仍有未结束 turn 或有效未解决交互的会话，最多 `limit` 个。
    async fn sessions_with_pending_activity(&self, limit: ReplayLimit) -> Result<Vec<SessionId>, PortError>;
    async fn head(&self) -> Result<GlobalCursor, PortError>;
    /// 一致性读视图：`sync.snapshot_*` 必须在本方法返回的视图内完成（barrier 依据）。
    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError>;
    async fn find_request(&self, request: &RequestId, actor: &Actor) -> Result<Option<CommandRecord>, PortError>;
    /// 该会话的恢复数据（§3.6 的 `SessionRecoveryRecord`）：`agent` 取自会话行，两个 `Option` 直接对应
    /// `owned_session` 的 `agent_session_id`/`workspace_cwd`。**窄读取**：这两列不进
    /// `Session`/`SessionSummary`；会话行不存在、或任一列为 `NULL`（没有可用于恢复的数据）时返回 `Ok(None)`
    /// ——`NULL` 不是错误，也不得被推导或补齐。
    /// 本方法**不读 `workspace_alias`**：恢复以持久化 cwd 原文为权威，目录归属是投影期的事。
    async fn load_recovery(&self, session: &SessionId) -> Result<Option<SessionRecoveryRecord>, PortError>;
    /// 启动恢复（§6 第 16 条）：`status='accepted'` 且 `terminal_event_id IS NULL` 的 mutation 行，
    /// 按 `accepted_at` 升序；走 §7.3 的 `owned_command_status` 索引。
    async fn unsettled_commands(&self, limit: ReplayLimit) -> Result<Vec<CommandRecord>, PortError>;
    async fn retention_window(&self, session: &SessionId) -> Result<Option<(Sequence, Sequence)>, PortError>;
    async fn prune(&self, policy: RetentionPolicy, at: Timestamp) -> Result<PruneReport, PortError>;
    async fn health(&self) -> Result<StoreHealth, PortError>;
}

/// 一次带来源位置的会话读取（`ReadView::read_session_origin`）：页、来源位置 R 与逐消息覆盖。
#[derive(Debug, Clone)]
pub struct OriginReadPage {
    pub page: HistoryPage,
    pub read_cursor: OriginCursor,
    pub coverage: Vec<MessageCoverage>,
}

pub trait ReadView: Send + Sync {
    async fn head(&self) -> Result<GlobalCursor, PortError>;
    async fn replay(&self, after: Option<GlobalCursor>, limit: ReplayLimit) -> Result<ReplayBatch, PortError>;
    /// Sync 增量重放的事件来源（§10.1 的 `event` 帧）：`after` 之后的 owned 事件（定位 + 持久化正文 +
    /// 三个来源字段），按 `global_sequence` 升序，并给出这批数据的 head。与 `replay` 的分工见 §5.2 的
    /// `[决定]`：`replay` 服务本机客户端的全局重放，本方法只服务入站设备，且不做 cursor 解释；identity
    /// 类事件得不出唯一身份时失败关闭（历史行处置，见 §5.2 `[决定]` 的对应条目）。
    async fn sync_replay(&self, after: Option<GlobalCursor>, limit: ReplayLimit) -> Result<SyncReplaySlice, PortError>;
    /// 会话的一页读（`session.read`）：`messages` 面的事件原料与结构化历史条目各自按 `include` 开关分页
    /// （`after` 与 `before_history` 互不影响）。两个集合都必须**带持久化正文**、且正文与定位来自同一条
    /// 行——实现一次取行（`payload_json` 与 ACP 列同批取出），MUST NOT 为逐条事件再取一次正文
    /// （`design.md` D4）。结构化历史条目按**会话内序号**分页（省略游标 = 最新一页；只跳过已压缩的行；
    /// 游标早于仍可读条目的下界时返回 `InvalidRequest`，不是更少的条目）。
    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError>;
    /// Owner 侧原子读取：页、来源位置 R 与逐消息覆盖来自同一只读事务（design D4）；无来源位置必须显式不支持。
    async fn read_session_origin(&self, query: HistoryQuery) -> Result<OriginReadPage, PortError>;
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
```

约束：

- `commit` / `commit_receipt` 是各自家族的**唯一**写入口；禁止可分别调用的 repository/journal/deduper（`MODULE_ARCHITECTURE.md` §4.1）。
- `[决定]` imported 写路径不再有 Import/Export 归属前置：`upsert_session`/`commit_receipt` 只按 `(owner_node_id, session_id)` 写入；v11 已删除 `imported_import` 与 `imported_import_export`。
- `[决定]` `origin_epoch` 由 **core** 在创建会话时用 `IdGenerator` 生成并传入（响应审查：存储层返回它会让无创建需求的提交也必须回读）；存储层只校验“该会话已有 epoch 时必须一致”。
- `[决定]` 幂等命中返回 `CommitOutcome::replayed`，不追加事件、不改状态。
- `[决定]`（`custom-session-title` 变更）**`OwnedCommit` 允许 `idempotency` 与 `command_terminal` 同为 `Some`**：这是「受理与终态同批」的提交形状（`session.title.set` 用它，`design.md` D2）。存储层在**同一事务**里写幂等行（`status` 直接取终态，`terminal_event_id` 回填为同批 `causation = requestId` 的那条 `command.completed`/`failed`/`uncertain` 事件的 id）、其余状态/事件，并**不再**定位既有 `accepted` 行（该行本次才建）；任一步失败整事务回滚。终态事件的 `causation` 必须等于幂等行的 `requestId`，否则整事务 `InvalidRequest`。`command_terminal` 的 `terminal_event` 字段在该形状下由 core 留 `None`（事件 id 由存储层在事务内分配）。
- `[决定]` **`StateChange::Update` 新增三个可空列**（§3.6 的 `agent_session_id`/`workspace_cwd` 与承载 §3.1 `WorkspaceRef` 的 `workspace_alias`）：`None` = 不改该列。`session.create` **不再经这条列写入**（`session-create-settle-order` 变更）：三列随会话行在**落账提交**（`StateChange::SettleCreate`）里一次写入（§6 第 20 条）——`agent_session_id` 取 `SessionEndpoint::agent_session_id()`，`workspace_cwd` 取 core 自己已解析的 `ResolvedWorkspace::canonical_path()`，`workspace_alias` 取**同一次解析**用掉的别名原文 `ResolvedWorkspace::alias()`（三者都不依赖后端回报）；`agent_session_id()` 为 `None` 时三列都不写，该会话不被当作可恢复会话，目录归属也就是「未分组」。落账提交把会话行与终态放在**同一次**提交里（新建行的 `version` 因此只有一个取值），不再有「写列一次、终态再一次」的两笔。写入之后这三列**只读**：恢复流程只经 `load_recovery` 读其中两列，MUST NOT 覆写，MUST NOT 读或写 `workspace_alias`（`design.md` D1 的归属来源 + D2 的契约订正）。**`title` 是两层可选**（`sync-scope-and-pwa-client` 变更 WP3 新增）：`Option<Option<String>>` —— `None` = 不改该列、`Some(None)` = **显式置空**（写入 `NULL`）、`Some(Some(text))` = 写入该标题（上限 512 字符）。上面「`None` = 不改该列」的单层口径**覆盖不了**置空这第三个状态，因此存储层的 `CASE WHEN ?x IS NULL` 之外必须再叠一条**哨兵参数**判定来表达「本次提交要置空」；置空与其余列共用**同一条** `UPDATE`，故版本递增（`version = version + 1`）与 `updated_at = commit.at` 对两条路径一致。该字段的语义、唯一来源（Agent 的 `session_info_update` 经 broker 投影出的 `session.info.changed`，**不存在**客户端重命名入口）与失败关闭口径见 §6 第 21 条。
- `[决定]` **目录归属的投影来源只有持久化的别名**：`SessionStore::list`（以及 `ReadView` 中返回摘要的 `read_session`/`node_link_slice`）给出的 `SessionSummary.workspace` 必须由 `owned_session.workspace_alias` 关联 `owned_workspace` 的展示名得出——该 JOIN 留在同时拥有两张表的存储实现内部，不上浮到 core。MUST NOT 按 `owned_session.workspace_cwd` 反查别名解析表来补出归属：`NULL` 就是未分组，「不反查」才能保证同一别名重指向后老会话的归属不漂移、投影期不引入对别名解析表的隐式耦合（`design.md` D1/D6，`workspace-resolution` 的创建时持久化要求）。
- `[决定]` 交互的创建与解析规则见 §6 第 13 条。`SessionStore` **没有** `resolve_interaction` 方法：解析是 `OwnedCommit.state.interaction` 的一部分；`SessionEndpoint::resolve_interaction` 是后端（Agent）侧入口，不落盘。
- `[决定]`（`session-activity` 变更 + `custom-session-title` 变更）**`StateChange::Update` 的形状**（`crates/core/src/ports.rs` 的 `SessionUpdate`）：`state: Option<SessionState>`（`None` = 不改状态）、`title: Option<Option<String>>`（Agent 标题，两层可选，见上）、`custom_title: Option<SessionTitle>`（`owned_session.custom_title`；`None` = 不改该列，`Some` = 写入 trim 后的 1..=512 标量值——没有清除/恢复分支，Agent 通知永远不碰它）、`mode: ModeChange`、`interaction: Option<InteractionResolved>`（仲裁）、`confirmation: Option<InteractionId>`（**确认**）、`agent_session_id`/`workspace_cwd`/`workspace_directory_id`（`None` = 不改列）。**没有 `closed_at`**。`InteractionResolved { interaction, resolution, resolved_by, request_version: Option<Version> }`：`request_version` 为 `Some` 时必须等于行内 `owned_interaction.request_version`，不符 → 整事务 `Conflict(VersionMismatch)` 且不消费请求。仲裁只把行推进到 `state='resolved'` 并写 `resolved_at`，**不写 `confirmed_at`**：在端点确认处理结果前该行仍是有效待处理（归约到 `pending`）。`confirmation` 是唯一把 `confirmed_at` 写入的路径；行不存在、不是 `resolved` 或已确认时是无操作（不产生第二次副作用），且确认与会话摘要、`session.updated` 同事务。
- `[决定]` **`SessionActivityFacts` 的读口径**（§5.2 的 `session_activity`）：`unresolved_interactions` = 未失效（`invalidated_at IS NULL`）且 `state='pending'` 或（`resolved` 且 `confirmed_at IS NULL`）的行数；`unconfirmed_interactions` = 其中已仲裁未确认的行标识；`unresolved_by_turn` = 按 `turn_id` 分组的未解决计数（`turn_id IS NULL` 的历史行不出现，供 turn 终态提交计算同事务失效数）；`unfinished_turns` = `owned_turn.state IN ('queued','running','waiting_input','waiting_permission')` 的行数。调用方据此取 `SessionActivity::reduce(unresolved_interactions > 0, unfinished_turns > 0)`。
- `[决定]`（`pwa-conversation-ui` D2/D4）**会话历史读的元素带持久化正文、按会话内序号分页**：`HistoryPage.events`（`messages` 面的原料，`after`/`next` 的全局 cursor 升序）与 `HistoryPage.history`（结构化历史条目，`before_history`/`history_has_earlier`）都由既有 `OwnedEventRecord` 承载——**正文与定位来自同一条行**，实现在一次取行里一并取出 `payload_json` 与 ACP 五列（`acp_media_type`/`acp_raw_json`/`acp_byte_length`/`acp_sha256`/`acp_raw_unavailable_reason`），MUST NOT 为逐条事件再调 `ReadView::event_payload`（N+1）。原文不可用时按行里的 `acp_raw_unavailable_reason` 还原为 `AcpRaw::Unavailable`（与 `event_payload` 同规则，摘要不因原文被清掉而丢失）。历史条目按 `owned_event.session_sequence` 取行，走的正是已有的唯一索引 `owned_event_session(session_id, session_sequence)`——**本次不新增索引、不改 §7**；省略游标 = 最新一页、带游标 = 严格更早的一页、只跳过 `compacted_into IS NOT NULL` 的行（已压缩事件不产生条目、也不占位），且**只取条目集合内的事件类型**——集合是 `SYNC_PROTOCOL.md` §10.2.1 的 17 类（该表同时登记了不进入的 17 类：消息正文与其收尾、摘要/仪表类、命令终态类、连接与审计类），由存储查询以 `event_type IN (...)` 与 `limit`/`history_has_earlier` 在**同一条语句**里施加：事后过滤会让页大小与页标记按未过滤的行计数，客户端会拿到空页或短页却被 `historyHasEarlier` 告知「更早还有条目」，而它按「上一页最早条目的会话内序号」续读时没有游标可用。集合只定义一次（`HISTORY_ENTRY_EVENT_TYPES`），`storage-sqlite` 与 core 的读视图替身共用它，不各写一份字面量。游标早于该会话仍可读**条目**的下界时返回 `InvalidRequest`（保留窗口之前显式不可读，MUST NOT 静默少返回让调用方误以为已到最早一条）。`HistoryInclude.history` 只决定该集合是否下发，两个集合在同一次请求里各自分页、互不影响（`SYNC_PROTOCOL.md` §11.5）。
- `[决定]`（`sync-server-connection` 变更 WP1 的 1.6，2026-10-14）**Sync 事件读面（`ReadView::sync_replay` + `SyncEventRecord`/`SyncReplaySlice` + 用例入口 `UseCases::sync_replay`）**：`SYNC_PROTOCOL.md` §10.1 的 `event` 帧把 `origin.kind`/`origin.deviceId`、`origin.nodeId` 与 `causationRequestId` 列为必需字段，而这几项只存在于 `owned_event` 的两个列（`origin_kind`/`causation`，§7.3），`CommittedEvent` 与 `EventPayload` 都不承载——因此新条目类型承载它们，**不**改 `CommittedEvent`/`OwnedEventRecord` 的形状（两者有大量现存调用方）。口径逐条如下：
  - **来源的取值只有一处**：`origin.kind` 取 `owned_event.origin_kind`，**五个**取值（`agent`/`device`/`daemon`/`local_cli`/`node`）原样往返（§7.3 的 CHECK 与 wire 词表同集，`node` 由 1.7 的前置补入）；词表外取值是**列值损坏**（实现返回 `Corrupt`），不得静默降级或挑近义取值。MUST NOT 用「这条事件有没有 `session_id`」推断来源（与 §5.2 关于 `PendingEvent.origin` 的条款同口径）。
  - **两个身份字段都不是第三处事实来源**：`origin.deviceId`/`origin.nodeId` 只能由 `owned_event.causation`（= 诱发该事件的命令 `requestId`）回查 `owned_command` 的 `actor_id` 得到，且**仅**在同类 `origin_kind`（`device`/`node`）时可能出现；前者要求 `actor_kind = 'device'`，后者要求 `actor_kind = 'node'` 并取复合键 `"{ownerNodeId}/{accessNodeId}"` 里的主 principal `ownerNodeId`（§6 第 6 条的幂等键；§10.1 的 `nodeId` 要的是产生该事件的另一个节点）。回查与事件读 MUST 在同一次只读事务内完成，且 MUST NOT 逐条事件各查一次（一条批量 `IN (…)` 查询覆写两类 actor），device 事件只认设备行、node 事件只认节点行，两类身份因此不可互换。
  - **identity 类事件得不出唯一身份 ⇒ 整批读失败关闭（`Corrupt`），MUST NOT 以 `None` 交差**：`origin.kind = device` 的语义要求是「事件来自已认证 Sync 设备经命令产生 ⇒ 带非空 `deviceId`」（§10.1 的 kind 表），`kind = node` 则被 schema 与 DTO 的配对规则强制要求非空 `nodeId`；而 schema 侧的 `{kind:"device", deviceId:null}` 又**合法**（frozen v1 形状只有 `{kind, deviceId}` 两个键，§16.2 不允许借扩展收紧必填性）——于是**发送方** MUST NOT 产生那种帧，读面也就不得把它登记成合法取值：给 `None` 等于把「投递层无法组装合法帧」伪装成一条合法数据，逼它静默违反 §10.1。四类得不出唯一取值的情形（前三类是**升级前写入的历史行**，正是变更 4.8 要求显式处置的对象）：**① `causation` 为空**——1.7 之前 agent-host 产出的每条 `EndpointEvent` 都不带 `causation`（适配器唯一的生产构造点固定 `None`）、broker 又原样落盘，设备驱动事件因此**全部**落在这一类，这是主流历史形态；**② 对应命令行不在本机**——`owned_event` 与 `owned_command` 的写入本来就允许「事件在本节点、命令由别处发起」（跨节点重放同一 requestId），保留期清理/不在本机历史窗口同样导致回查不到；**③ 命令行的 `actor_kind` 与本事件的 `origin.kind` 不符**——同一 requestId 上先由本机 CLI（或另一节点）发起、后被设备重发（`owned_command` 的唯一键是 `(actor_kind, actor_id, request_id)`，跨 actor 复用同一 requestId 在 DDL 上合法），或 1.7 之前 `event_origin` 按 actor 瞬时状态分类留下的错配行；**④ 同一 requestId 在同一 actor 类别下命中多行**（跨设备/跨节点复用，唯一键允许）。MUST NOT 从旁路（会话归属、actor 瞬时状态……）推断 id：编造 id 比失败关闭更糟——它把归属判错给另一台设备并让调用方无法察觉。
  - **`workspace.mkdir` 终态的精确关联**：该命令允许不同设备使用相同 requestId；终态事件优先通过 `owned_command.terminal_event_id = owned_event.event_id` 精确关联命令主体，不使用仅含类别与 requestId 的歧义匹配。关联仍在同一次只读事务内批量完成，不新增表列；其它既有事件继续遵守上述回查与歧义失败规则。
  - **列值损坏一律 `Corrupt`**：词表外的 `origin_kind`、`actor_kind = 'device'` 行上不是 canonical UUID 的 `actor_id`、`actor_kind = 'node'` 行上不是 `node/access-node` 复合键的 `actor_id`，都与 `find_request`/`command_record_from_row` 对同一列的既有口径一致（同列、同为 fail-closed），**不**降级成「没有身份 id」（那会把列值损坏藏起来）。这几条在生产上不可达（DDL 与写入侧都钉死了形状），属读取侧纵深防御。
  - **窗口**：严格在 `after` 之后（`None` = 从头）、按 `global_sequence` 升序、最多 `limit` 条；已压缩的 delta 不出现（与 `replay` 同口径，§6 第 15 条）。`after.server_epoch` 与当前 epoch 是否一致、以及四类 `sync.cursor_invalid` 原因与 `sync.reset_required`（§9.2）**不在本方法的判定范围内**：那是投递层的判定，本方法只按 `global_sequence > after` 取行、并给出这条快照的 head（`sync.caught_up` 的屏障游标）。
  - **授权与投影**：用例入口 `UseCases::sync_replay(actor, after, limit)` 先按 `session.list` 走 `Broker::authorize`（设备主体恒通过、节点按 `grant.observe`），授权先于读取；它也不在 `require_local` 之外另设守卫（入口自带「为什么在 `require_local` 之外」的理由注释，与 `sync_visible_sessions` 同源）。core 不投影，owned 事件的字段由条目承载（`MODULE_ARCHITECTURE.md` §8），`remoteOrigin` 按 §9.6 恒为 `null`（本读面只返回 owned 事件，imported 的跳数由投递层的 imported 路径另行投影）；投递层不再按授权过滤事件。
  - **本条不新增表列、不改 §7**：`owned_event.origin_kind`/`causation` 与 `owned_command` 的 `actor_kind`/`actor_id` 早已就位，本次只是把它们读出（`EVENT_COLUMNS` 一直在 SELECT 它们，缺的只是组装 §10.1 形状的读面）。
- `[决定]` `retention_window` 返回该会话仍可重放的 `session_sequence` 下界/上界；broker 据此决定 `sync.reset_required`（`reason` 枚举 `initial_sync|epoch_mismatch|cursor_expired|cache_incompatible`，`SYNC_PROTOCOL.md` §9.4）；cursor 的四种拒绝原因：格式非法 → `malformed`（协议层）、`serverEpoch` 与 `meta.server_epoch` 不符 → `epoch_mismatch`、超出 `head()` → `beyond_head`、低于窗口下界 → `cursor_expired`（`SYNC_PROTOCOL.md` §9.2）。

**Access 侧 imported 回源端口（2026-10-20，`conversation-area-complete` WP5E5-read；2026-10-21，`access-owner-data-stateless` WP2 切换来源）**：`RemoteSessionReads::read_imported(session, ImportedReadQuery) -> ImportedRead` 是旧 wire 路径对 imported 会话的读面。`ImportedRead.document` 是 Owner `session.read` 完成结果的**原文**（`SessionReadResult` 形状），core 不解释其正文；实现（`node-link-client::RemoteSession`，组合根经 `AccessManager` 注入）MUST 在 Owner 不可达时返回 `UnavailableKind::OwnerOffline`，MUST NOT 返回空历史，也 MUST NOT 把正文写入数据库、临时文件或日志。`UseCases::read_imported_session` 先按 `session.read` 授权，再按 **`RemoteRuntimeDirectory::find_by_session`** 判定来源身份与在线状态（不再读 `RemoteDeliveryStore::find_imported_session`）：目录没有该标识返回 `None`；多个 Owner 复用同一 `SessionId` 明确拒绝；离线/撤销返回 `Unavailable`；读取期间代际改变则丢弃结果。未装配端口返回 `UnavailableKind::BackendUnsupported`。D4 的类型化回源（含 `readCursor`/`messageCoverage`）走 §5.9 的 `RemoteSourceReads`。

### 5.3 信任、Export、审计与附件

实现状态：SQLite 已实现附件端口、§7 的 v2 表结构与三个管理 store 的**落盘实现**（管理写集的一事务提交、失败关闭与容量纳入，见 `crates/storage-sqlite/src/admin/` 与 §9 判据 23–29）；core 侧同时保留测试替身。下面列出的签名与 §5.1/§5.2 一样是**冻结形状**，由 `scripts/check-contract-drift.mjs` 与 `crates/core/src/ports.rs` 逐条绑定。

`TrustStore` 的读面新增一条只读列表（`pairing-code-web-entry` 的 WP3，2026-10-18）：`pairings()` 返回全部 `PairingRecord`，只服务 daemon 的过期/重启维护（`Authority::due_pairings`/`unrecoverable_after_restart` 需要完整快照）。它不返回 secret、不做业务写、不新增表或列（摘要仍是唯一的持久形态）；业务读面继续走 `pairing`/`pairing_for`。配套的 `ExpiryWrite` 增加 `only: Option<Vec<PairingId>>`：`None` 保持原有「未确认且已到期」的时间扫描，`Some(ids)` 只终结列出的未确认标识（重启后已无法验密的配对早于 `expires_at` 也必须终结）；两种形态都不改写已批准/已终结的行，DDL 不变。

```rust
pub trait TrustStore: Send + Sync {
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError>;

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError>;

    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError>;

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError>;

    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError>;

    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError>;

    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError>;

    async fn pairings(&self) -> Result<Vec<PairingRecord>, PortError>;

    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError>;

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

    async fn consume_pairing(&self, write: PairingConsumption) -> Result<PairingRecord, PortError>;

    async fn record_node_connected(&self, write: NodeConnectedWrite) -> Result<(), PortError>;
}
```

```rust
pub trait LocalConfigStore: Send + Sync {
    async fn directories(&self) -> Result<Vec<DirectoryRecord>, PortError>;

    async fn directory(
        &self,
        directory_id: &DirectoryId,
    ) -> Result<Option<DirectoryRecord>, PortError>;
}
```

约束：

- `[决定]` 凭据（Provider、Node key、pairing secret）**不经**任何上述端口：只存平台 keystore，表里最多存引用与指纹（`SECURITY_DESIGN.md` §13.1）。
- `[决定]` `AuditRecord.action` 是 §3.5 的闭合枚举；审计行不得包含内容（`SECURITY_DESIGN.md` §14.2），并由 §9.13 的机器检查兜底。

- `[决定]` **身份材料读取必须核对同行指纹**（§3.5、§9 判据 25/30）：`owned_peer_key`、`owned_pairing_peer` 与 `owned_device`（含设备记录的**单读与列表读**）的读取路径都用 `PeerPublicKey::fingerprint()` 比对同一行的 `fingerprint` 列，不一致 → `PortError::Corrupt` 且**不返回该材料**。DDL 只有列级长度/字符集 CHECK，跨列一致性只能在读取期判定（SQL 算不了 SHA-256）。
- `[决定]` `owned_device.last_seen_at` 与 `owned_node.last_connected_at` **只前进**（§9 判据 30）：旧值为空时写入新值、新值为空时保留旧值、两者非空取较大者，比较按固定宽度 UTC 毫秒文本的字典序（§3.2）；MUST 用显式 `CASE`，不得用标量 `max(a, b)`（任一参数为 NULL 时返回 NULL，会把「旧值为空时的首次写入」丢成 NULL）。

### 5.4 发布与基础设施

```rust
/// `Arc<dyn Fn(EndpointEvent) + Send + Sync>` 的包装类型：值语义 + `Clone`，因此
/// `SessionBackendFactory::create`/`open` 可按值接收并交给后端持有；`send` 是它的固有方法，
/// 调用顺序即提交顺序（§6 第 1/3 条）。**不是** trait——§5.1 的 `[决定]` 与实现一致。
#[derive(Clone)]
pub struct EventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);
/// **节点级**事件的交付通道（`design.md` D6、R8）：与 `EventSink` 同形（同一个 `Clone` + `send`
/// 契约），但语义不同——它承载的是**不属于任何会话**的事件（`agent.connected`/`agent.disconnected`）。
/// 单独成一个类型而不是复用 `EventSink`，是为了让「节点级事件不得进入会话槽位」在类型上可见：
/// 持有一个 `EventSink` 意味着「这是某个会话的事件」，而本类型的消费者只能是
/// `Broker::commit_node_event`。
#[derive(Clone)]
pub struct NodeEventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);
pub trait EventPublisher: Send + Sync { fn publish(&self, event: CommittedEvent); }
pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }

pub trait IdGenerator: Send + Sync {
    fn session_id(&self) -> SessionId;
    fn turn_id(&self) -> TurnId;
    fn message_id(&self) -> MessageId;
    fn interaction_id(&self) -> InteractionId;
    fn pairing_id(&self) -> PairingId;
    fn origin_epoch(&self) -> OriginEpoch;
    fn request_id(&self) -> RequestId;
}
```

- `[决定]` `event_id`：由存储层在提交事务内分配（`SessionStore::commit`），**不**由 `IdGenerator` 提供，避免出现“先生成后提交”导致的两个来源。
- `[决定]` **`session` 标识由 core 分配、存储层采用**（`session-create-settle-order` 变更）：`session.create` 的落账提交（`StateChange::SettleCreate`）在事务内用 core 早已分配的 `SessionId` 建行，存储层**不再**现分配；`IdGenerator` 因此提供 `session_id()`，而 `IdGenerator` **不提供** `event_id()`（由存储层在事务内分配）与 `attachment_id()`（由 `AttachmentStore::put` 分配并返回，§7.3）——同一个 id 仍然只有一个来源。`StateChange::Create`（存储层现分配标识的旧形状）只服务不经 core 用例的提交方，`session.create` 不走它。
- `[决定]` `publish` 不返回结果，且不得回滚已提交事务；订阅注册与背压属于 `server`（它持有注册表与有界队列），`app` 把同一个 hub 交给 core 与 server（`MODULE_ARCHITECTURE.md` §6）。
- `[决定]` **节点级事件不走 `EventSink`**（`sync-scope-and-pwa-client` 变更 WP3、`design.md` D6）：`agent.connected`/`agent.disconnected` 的会话标识为空，而 `EventSink` 是**会话槽位**的入口（`Broker::sink(&SessionId)` 把事件塞进该会话的 pending 队列），走它会把节点级事件归到某个会话上。节点级事实的产生点在适配器（只有它知道「本次是新 spawn 还是复用」与「进程真的退出了」），因此 `agent-host` 用 `NodeEventSink` 交付**已构造好的 `EndpointEvent`**，core 侧的唯一提交入口是 `Broker::commit_node_event`（校验事件类型/类别/turn/ACP 原文/`agentId`/`state` 词表，失败即 `InvalidRequest`），提交形状是 `session: None` 的 `OwnedCommit`。这两类事件当前**没有投递通道**（唯一的入站适配器按会话归属投递），本切片只交付产生与持久化。

### 5.5 目录浏览

`[决定]`（2026-10-17，`sync-path-browse-and-diff-delivery` 变更 WP2）目录浏览是 `workspace.browse` 的**只读、逐层**目录列举端口（spec `sync-workspace-browse`；§11.9）：类型上只有「名称 + 绝对路径」两个事实，因此**没有**承载文件条目、文件内容、大小、时间戳或权限位的字段——不给可能性比运行时过滤更可靠。顺序不作要求（确定顺序由用例层按名称施加），不可读/不存在/不是目录对调用方是同一类事实（`Unavailable(IoError)`），不用于区分「这一层不存在」与「这一层不可读」。端口是 dyn 化的异步 trait，由组合根注入；core 侧随 crate 提供一个本机文件系统实现作为默认值（组合根可用 `UseCases::with_directory_browser` 换掉）。

默认本机实现 `SystemDirectories` 在 Windows 上不列出带 `FILE_ATTRIBUTE_HIDDEN` 或 `FILE_ATTRIBUTE_SYSTEM` 的子目录，过滤发生在投影与条目上限截断之前。按属性判断，不按目录名黑名单或点前缀判断；其它平台的列举行为不变。属性只在实现内部读取，不加入端口值对象或 wire 字段。该规则仅控制目录浏览列表，不新增 `session.create` 的禁止路径规则；创建仍按既有授权、路径形状、规范化与文件系统访问结果判定。

```rust
/// 目录浏览端口（§5.5/§11.9）：`workspace.browse` 的只读、逐层目录列举。
///
/// 三条约束：只返回**直接子目录**（不含文件、内容、大小、时间戳与权限）；只读（不写任何登记、
/// 缓存或会话状态）；顺序不作要求（确定顺序由用例层施加）。`parent` 是**已规范化**的绝对路径，
/// 形状校验与存在性/目录性判定由用例层在调用前完成。
#[async_trait]
pub trait DirectoryBrowser: Send + Sync {
    /// 本机的根列表（父路径为 `null` 的分支）：每个条目是一个根挂载点/盘符。
    async fn roots(&self) -> Result<Vec<DirectoryListing>, PortError>;

    /// `parent` 的**直接**子目录。
    async fn directories(&self, parent: &str) -> Result<Vec<DirectoryListing>, PortError>;
}

/// 目录浏览端口的一条子目录：只含名称与该目录的绝对路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListing {
    /// 该子目录的名称（单层，不含任何路径分隔符）。
    pub name: String,
    /// 该子目录的绝对路径（实现侧的规范化结果）。
    pub path: String,
}
```


### 5.6 独立创建目录

`DirectoryCreator::create_directory(&self, parent: &str, name: &str) -> Result<(), PortError>` 只创建既有父目录下的一层目录；`DirectoryBrowser` 保持只读。`SystemDirectories` 默认实现此端口，`UseCases::with_directory_creator` 允许注入。不得新增 core 普通依赖。

`UseCases::admit_workspace_mkdir(&self, actor: &Actor, request: &RequestId, fingerprint: &Digest, parent_path: &str, name: &str) -> Result<WorkspaceMkdirAdmission, PortError>` 返回 `Accepted(WorkspaceMkdirExecution)`、`InFlight` 或 `Settled(Box<CommandRecord>)`。执行凭证字段私有且不可 Clone，绑定受理时的主体、请求、父路径和名字；`execute_workspace_mkdir(&self, execution: WorkspaceMkdirExecution) -> Result<(), PortError>` 消耗此凭证，防止未受理或重复执行。

复用 SessionStore 的 OwnedCommit(session=None)，先持久接受，再执行 DirectoryCreator，再同一事务提交终态与无会话全局事件；不得伪造 owned_session/owned_workspace 行。成功结果固定 `{ created: true }`，事件、错误、审计不含路径/名字。重放按稳定 (actor, requestId) 返回原状态，内容指纹冲突拒绝；未终结受理由既有 unsettled_commands 恢复为 uncertain，禁止重做。终态提交使用 `SessionStore::commit_workspace_mkdir_terminal(actor, commit)`，只给本命令的结算与启动恢复使用，按 `(actor, requestId)` 精确定位。该入口默认返回 `InvalidRequest`（unsupported），只接受 `session=None`、有 `command_terminal` 且无 idempotency/state/turn/interactions/compaction/origin_epoch 的提交；按终态事件 causation 获取 requestId，再按完整主体键查行并校验命令为 `workspace.mkdir`、session 为空，更新时同样限定主体。SQLite 内部 `commit_owned` 仅在此路径传入 `terminal_actor`，旧 `commit` 路径行为保持不变。无需新增表或迁移。

```rust
#[async_trait]
pub trait DirectoryCreator: Send + Sync {
    async fn create_directory(&self, parent: &str, name: &str) -> Result<(), PortError>;
}
```

### 5.7 会话文件与 Git 只读端口

`[决定]`（2026-10-19，`session-file-browser` WP1）`SessionFiles` 是 `session.files.list` / `session.files.read` / `session.git.status` / `session.git.diff` 的唯一出站端口。core 只做授权、会话来源判定、根读取、纯形状校验与错误分类；文件句柄、目录枚举、Git 子进程、取消与进程树清理属于 `app::session_files` 的原生实现（WP2），不进入 core（`AGENTS.md` §5：core 不依赖文件系统/子进程/Tokio）。

- **根**：`root` 是用例层从 `SessionStore::load_recovery().workspace_cwd` 取得的规范化绝对路径；实现 MUST NOT 按 workspace 别名反查、猜测或回退其它目录。`SessionFilePath` 已拒绝绝对/父引用/`.git` 等不安全形状，实现仍以实际打开的对象验证根边界（D3）。
- **内存生命周期**：`open_content` 建立连接绑定、内存有界的只读快照；`read_content` 只按同一 `ContentVersion` 续读；`release_content` 在连接断开、取消、设备撤销与 60 秒空闲到期时释放。每连接最多一个快照、每节点最多两个；不写临时正文文件，不把快照版本当 bearer。
- **预算**：单块 32 KiB、文本预览 256 KiB、图片预览 4 MiB、下载 64 MiB（§3.8 的常量 / `SYNC_PROTOCOL.md` §14）。预算按 `purpose` 分开：预览只受预览预算约束，原文件更大时如实返回 `size_bytes` 并置 `preview_truncated`，**不因超过 64 MiB 而拒绝预览**；只有 `download` 在 `size_bytes > 64 MiB` 时明确失败，不截断冒充完整。这条失败必须是**可识别**的：`open_content` MUST 返回 `PortError::InvalidRequest(core::model::RESULT_TOO_LARGE)`（文本 `resource.result_too_large`），服务端据此映射为同名 wire 错误码；不得落进通用形状错误。
- **会话来源**：四条查询的 imported 判定不能只看 `SessionStore::load`——远程会话只存在于当前运行期目录里。`load` 返回 `None` 后再按 **`RemoteRuntimeDirectory::find_by_session`** 查一次：命中即 `capability.unsupported_by_broker`（发生在任何文件/Git 端口调用之前），仍未命中才是 `session.not_found`。不得回退到本机文件路径或持久 imported 索引。
- **Git**：只读；实现不得运行仓库配置指定的 external diff/textconv/hook/fsmonitor/pager，也不修改索引或工作树。非仓库与 Git 缺失分别是 `git_state = not_repository` 与 `unavailable` 的成功状态。
- **无副作用**：端口实现不写会话状态、登记表、缓存或审计；查询不产生命令终态事件。

```rust
#[async_trait]
pub trait SessionFiles: Send + Sync {
    async fn list(&self, root: &str, path: Option<&SessionFilePath>, hide_ignored: bool, after: Option<&SessionPageToken>) -> Result<SessionFilePage, PortError>;
    async fn open_content(&self, root: &str, path: &SessionFilePath, purpose: FileReadPurpose) -> Result<ContentSnapshot, PortError>;
    async fn read_content(&self, version: &ContentVersion, offset: u64) -> Result<SessionFileChunk, PortError>;
    async fn release_content(&self, version: &ContentVersion) -> Result<(), PortError>;
    async fn git_status(&self, root: &str, after: Option<&SessionPageToken>) -> Result<GitStatusPage, PortError>;
    async fn git_diff(&self, root: &str, path: &SessionFilePath) -> Result<GitDiff, PortError>;
}
```

组合根通过 `UseCases::with_session_files(Arc<dyn SessionFiles>)` 注入；core 的默认值 `UnavailableSessionFiles` 一律返回 `Unavailable(IoError)`（失败关闭，不是空成功）。`UseCaseDeps` 的必填字段不变，既有字面量构造点无需改动。

### 5.8 分块附件与有效交互

本节是 `conversation-area-complete` WP2 已实现的端口，与 `crates/core/src/ports.rs` 逐项绑定。
上传只接受单块（`ATTACHMENT_CHUNK_SIZE = 32768`），清单用 `begin_submission`/`append_submission_page`
分批登记（每页最多 64 项）后由 `finalize_submission` 整批校验；`prompt` 的受理经
`SessionStore::commit_with_submission` 把清单绑定放进同一事务（§6 第 22 条）。

```rust
#[async_trait]
pub trait AttachmentTransferStore: Send + Sync {
    async fn begin_transfer(&self, write: TransferBegin) -> Result<AttachmentTransfer, PortError>;
    async fn append_chunk(&self, write: TransferChunk) -> Result<ChunkOutcome, PortError>;
    async fn seal_transfer(&self, write: TransferSeal) -> Result<SealedAttachment, PortError>;
    async fn transfer(&self, transfer: &AttachmentId) -> Result<Option<AttachmentTransfer>, PortError>;
    async fn abort_transfer(&self, transfer: &AttachmentId, at: Timestamp) -> Result<AttachmentTransfer, PortError>;
    async fn begin_submission(&self, write: SubmissionBegin) -> Result<SubmissionProgress, PortError>;
    async fn append_submission_page(&self, write: SubmissionPage) -> Result<SubmissionProgress, PortError>;
    async fn finalize_submission(&self, write: SubmissionSeal) -> Result<AttachmentSubmission, PortError>;
    async fn submission(&self, submission: &SubmissionId) -> Result<Option<AttachmentSubmission>, PortError>;
    async fn read_chunk(&self, request: &AttachmentRead) -> Result<AttachmentChunk, PortError>;
    async fn prune_transfers(&self, at: Timestamp, limit: u32) -> Result<u32, PortError>;
}
```

```rust
pub struct TransferBegin {
    pub transfer: AttachmentId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub total_length: u64,
    pub media_type: String,
    pub file_name: String,
    pub at: Timestamp,
}

pub struct TransferChunk {
    pub transfer: AttachmentId,
    pub offset: u64,
    pub data: Vec<u8>,
    pub at: Timestamp,
}

pub struct ChunkOutcome {
    pub next_offset: u64,
    pub duplicate: bool,
}

pub struct TransferSeal {
    pub transfer: AttachmentId,
    pub length: u64,
    pub sha256: Digest,
    pub at: Timestamp,
}

pub struct SubmissionBegin {
    pub submission: SubmissionId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub attachment_count: u64,
    pub at: Timestamp,
}

pub struct SubmissionPage {
    pub submission: SubmissionId,
    pub start_position: u64,
    pub attachments: Vec<AttachmentId>,
    pub at: Timestamp,
}

pub struct SubmissionProgress {
    pub submission: SubmissionId,
    pub registered: u64,
    pub attachment_count: u64,
    pub sealed: bool,
}

pub struct SubmissionSeal {
    pub submission: SubmissionId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub attachments: Vec<AttachmentId>,
    pub digest: Digest,
    pub at: Timestamp,
}

pub struct AttachmentRead {
    pub attachment: AttachmentId,
    pub session: SessionId,
    pub offset: u64,
    pub max_length: u64,
}

pub struct AttachmentChunk {
    pub offset: u64,
    pub data: Vec<u8>,
    pub total_length: u64,
    pub eof: bool,
}
```

```rust
#[async_trait]
pub trait InteractionAuthority: Send + Sync {
    async fn effective(&self, session: &SessionId) -> Result<Vec<EffectiveInteraction>, PortError>;
    async fn invalidate_pending(&self, session: Option<&SessionId>, at: Timestamp) -> Result<u32, PortError>;
}
```

约束（实现必须满足，测试见 §9 判据 34–36）：

- `append_chunk` 单次写入的 `data.len() ≤ 32768`；按期望偏移做条件更新，重复同内容返回
  `ChunkOutcome { duplicate: true }`，不同内容返回 `Conflict(AlreadyExists)`，非期望偏移返回
  `InvalidRequest`（wire 适配器据此填 `attachment.offset_mismatch` 的 `expectedOffset`/`actualOffset`）。
- `seal_transfer` 在追加过程中维护流式 SHA-256 与已接收长度；校验失败不产生 `SealedAttachment`，
  也不把已写入的临时内容标成可引用附件。
- `begin_submission`/`append_submission_page` 分批登记清单（同位置同值幂等、不同值冲突，
  越过声明数量拒绝）；`finalize_submission` 只在整批已封存、同一会话/主体、顺序与登记一致、
  数量与摘要相符且无重复项时成功，封存后不可追加。
- `commit_with_submission` 在受理事务内复核清单（已封存、会话、**认证主体**）与每个附件文件；任一
  失败整事务回滚，命令不被受理，Agent 不会被派发。成功绑定后**删除清单行**（条目级联删除）：清单被
  消费后不再是附件 GC 根，附件改由会话绑定与消息媒体引用保护。幂等命中先于绑定校验，因此重试返回首次
  结果。
- `read_chunk` 返回 `data.len() ≤ min(max_length, 32768)`；跨会话/主体读取返回 `NotFound`，不泄露内容。
  附件文件一律经 `attachment_file_path` 映射（`attachments/<name>` → `storage.attachment_dir/<name>`），
  因此自定义 `attachment_dir` 在封存、清单复核、绑定与读取之间一致生效。
- `prune_transfers` 清理两类闲置对象（同一实现也被 `SessionStore::prune` 调用）：① `open` 超过 24 小时
  或 `sealed` 超过 24 小时仍未绑定的清单（24 小时从 `created_at`/`sealed_at` 算，条目随外键级联删除）；
  ② `state = Receiving` 且 `updated_at` 早于 24 小时窗口、且未被**活跃**清单引用的临时传输。删除文件在
  事务提交之后；seal/提交与清理用条件更新互斥。传输临时文件位于 `attachment_dir/.transfers/`，不属于
  孤儿回收扫描的顶层，因此活跃上传不会周期清理误删。
- `InteractionAuthority` **只有读与失效**：应答的 first-writer-wins 仲裁走
  `SessionStore::commit` 的 `SessionUpdate.interaction`，确认走 `SessionUpdate.confirmation`，因此交互阶段、
  会话摘要与 `session.updated` 永远在同一事务里；旧端口方法 `resolve_effective` 与其输入
  `EffectiveResolution` 已删除，**不存在**可单独调用的解析写入口。仲裁的条件更新同时校验
  版本/状态/失效标记：版本不符 → `Conflict(VersionMismatch)`，已处理 → `Resolution::AlreadyResolved`，
  失效 → `Conflict(Expired)`，失败校验不推进版本、不消费请求。
  `invalidate_pending` 把仍 `pending`（或 `resolved` 但未确认）的行同时置为 `state = 'expired'` 并写
  `invalidated_at`；失效行因此不出现在任何 pending 读面（`effective`、`read_session` 的
  `pending_interactions`、Node Link 快照），旧解析路径（`StateChange::Update.interaction`）也返回
  `Conflict(Expired)` 而不消费它，且不可复活。
- `permission.resolve`/`elicitation.respond` 的版本、类别、表单与上游存活校验先于命令受理；受理之后的
  竞态失败（另一设备抢先解析、窗口内失效、后端写失败）必须落一条 `command.failed` 终态，而不是把命令
  留在 `accepted`。

### 5.9 远程运行期目录与在线回源

`[决定]`（2026-10-21，`access-owner-data-stateless` WP2）**远程资源只存在于已认证连接期**：
Access 不持久保存 Owner 的会话摘要、目录、来源游标、命令状态或结果引用。以下端口由 `node-link-client`
提供有界内存实现、由组合根装配；core 只定义纯类型接口，不实现内存结构。断开、撤销或 Owner epoch
改变时实现必须调用 `invalidate` 整代清除，并让未完成读取以 `Unavailable` 结束；所有类型只在
`crate::model`/`crate::ports` 内建模，不使用 `serde_json::Value`。

```rust
pub enum RemoteSourceState {
    Online,
    Offline,
    Revoked,
}

pub struct RemoteGeneration(u64);

pub struct RemoteCatalogRevision(u64);

pub struct RemoteSessionEntry {
    pub summary: SessionSummary,
    pub generation: RemoteGeneration,
    pub source: RemoteSourceState,
}

pub struct RemoteDirectorySnapshot {
    pub owner_node_id: NodeId,
    pub generation: RemoteGeneration,
    pub revision: RemoteCatalogRevision,
    pub source: RemoteSourceState,
    pub sessions: Vec<RemoteSessionEntry>,
}

#[async_trait]
pub trait RemoteRuntimeDirectory: Send + Sync {
    async fn list(&self) -> Result<Vec<RemoteSessionEntry>, PortError>;
    async fn snapshot(&self, owner: &NodeId) -> Result<Option<RemoteDirectorySnapshot>, PortError>;
    async fn lookup(&self, session: &RemoteSessionRef) -> Result<Option<RemoteSessionEntry>, PortError>;
    async fn find_by_session(&self, session: &SessionId) -> Result<Vec<RemoteSessionEntry>, PortError>;
    async fn owner_state(&self, owner: &NodeId) -> Result<RemoteSourceState, PortError>;
    async fn generation(&self, owner: &NodeId) -> Result<Option<RemoteGeneration>, PortError>;
    async fn invalidate(&self, owner: &NodeId, generation: RemoteGeneration) -> Result<(), PortError>;
}

pub struct MessageCoverage {
    pub message_id: MessageId,
    pub through: OriginCursor,
    pub last_delta_index: Option<u64>,
}

pub struct RemoteHistoryRecord {
    pub origin: OriginEventRef,
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload: EventPayload,
    pub created_at: Timestamp,
}

pub struct RemoteReadResult {
    pub summary: SessionSummary,
    pub read_cursor: OriginCursor,
    pub messages: Vec<MessageRecord>,
    pub history: Vec<RemoteHistoryRecord>,
    pub coverage: Vec<MessageCoverage>,
    pub messages_has_earlier: bool,
    pub history_has_earlier: bool,
}

pub struct RemoteReadQuery {
    pub include: HistoryInclude,
    pub before_history: Option<Sequence>,
    pub before_messages: Option<MessageCursor>,
    pub messages_limit: ReplayLimit,
    pub limit: ReplayLimit,
}

pub struct RemoteSourceBarrier {
    pub cursor: OriginCursor,
    pub generation: RemoteGeneration,
}

#[async_trait]
pub trait RemoteSourceReads: Send + Sync {
    async fn read(&self, session: &RemoteSessionRef, generation: RemoteGeneration, query: RemoteReadQuery) -> Result<RemoteReadResult, PortError>;
    async fn replay_barrier(&self, session: &RemoteSessionRef, generation: RemoteGeneration, after: Option<OriginCursor>) -> Result<RemoteSourceBarrier, PortError>;
}

pub struct RemoteCommandTarget {
    pub owner_node_id: NodeId,
    pub session_id: Option<SessionId>,
}

pub struct RemoteCommandRecord {
    pub target: RemoteCommandTarget,
    pub request: RequestId,
    pub status: CommandStatus,
    pub accepted_at: Option<Timestamp>,
    pub terminal_at: Option<Timestamp>,
    pub terminal_event_id: Option<EventId>,
    pub result: Option<CommandResult>,
    pub error: Option<PublicError>,
}

pub enum RemoteCommandLookup {
    Accepted(RemoteCommandRecord),
    Terminal(RemoteCommandRecord),
    NotFound,
    Uncertain(RemoteCommandRecord),
}

#[async_trait]
pub trait RemoteCommandStatus: Send + Sync {
    async fn query(&self, target: &RemoteCommandTarget, request: &RequestId) -> Result<RemoteCommandLookup, PortError>;
}

#[async_trait]
pub trait RemoteTitleWrite: Send + Sync {
    async fn set_title(&self, session: &RemoteSessionRef, generation: RemoteGeneration, request: &RequestId, title: &SessionTitle) -> Result<RemoteCommandRecord, PortError>;
}

pub enum RemoteStreamResetReason {
    EpochMismatch,
    CursorExpired,
    CacheIncompatible,
}

pub struct RemoteStreamEvent {
    pub owner_node_id: NodeId,
    pub session_id: SessionId,
    pub origin_event_id: EventId,
    pub origin_epoch: OriginEpoch,
    pub origin_sequence: Sequence,
    pub event_type: EventType,
    pub payload_digest: Digest,
    pub created_at: Timestamp,
    pub payload: EventPayload,
}

pub enum RemoteStreamItem {
    Event(RemoteStreamEvent),
    CaughtUp(RemoteSourceBarrier),
    Reset {
        reason: RemoteStreamResetReason,
        cursor: Option<OriginCursor>,
    },
    Unavailable(PublicError),
}

pub enum RemoteSourceNotice {
    CatalogChanged {
        owner_node_id: NodeId,
        revision: RemoteCatalogRevision,
    },
    State {
        owner_node_id: NodeId,
        state: RemoteSourceState,
    },
}

#[async_trait]
pub trait RemoteStreamReceiver: Send {
    async fn next(&mut self) -> Option<RemoteStreamItem>;
}

#[async_trait]
pub trait RemoteNoticeReceiver: Send {
    async fn next(&mut self) -> Option<RemoteSourceNotice>;
}

#[async_trait]
pub trait RemoteEventStream: Send + Sync {
    async fn subscribe(&self, session: &RemoteSessionRef, generation: RemoteGeneration, after: Option<OriginCursor>) -> Result<Box<dyn RemoteStreamReceiver>, PortError>;
    async fn unsubscribe(&self, session: &RemoteSessionRef, generation: RemoteGeneration);
    async fn notices(&self) -> Result<Box<dyn RemoteNoticeReceiver>, PortError>;
}
```

约束：

- **生命期**：四个 trait 都只服务当前已认证连接。连接断开、信任撤销或 Owner epoch 改变时，实现取消该来源
  的读写任务并让未完成调用以 `Unavailable` 结束；目录与结果不写数据库、临时文件或日志，也不跨重启恢复。
- **有界**：目录、快照与回源页沿用协议已有的目录/帧/队列/超时限额，不新增无界缓存；慢客户端不得阻塞
  其他来源。超出协议限额按既有 `result_too_large`/backpressure 失败关闭，不静默扩容。
- **来源一致性**：`RemoteSessionEntry.summary` 必须是 `ResourceOrigin::Remote`，`online` 必须与 `source`
  一致；core 在 `sync_visible_sessions` 与回源读取前校验，不一致按 `Corrupt` 失败关闭。不同 Owner 的
  同名 `SessionId` 不是同一会话：`find_by_session` 的多条命中由 core 明确拒绝，不得挑选任意一条。
- **generation**：`invalidate(owner, generation)` 只在代际与当前值相等时清除，旧连接的迟到失效不得误删
  新连接建立的目录。回源读取结果在返回 core 前/后都要与当前代际比对；跨代结果丢弃并返回
  `Unavailable(OwnerOffline)`。
- **D4 读取边界**：`RemoteReadResult.read_cursor` 与该次 `summary.origin.origin_epoch` 必须一致；
  `coverage[*].through` 必须全部等于 `read_cursor`；`coverage` 只能覆盖本次返回的消息，每条消息至多一项，
  且**每条返回消息必须恰有一项**（漏报按 `Corrupt` 失败关闭，否则客户端会把已返回的正文再拼一次）。
  `UseCases` 暴露纯函数 `validate_remote_read_result`、`classify_covered_event`、`decide_remote_page`
  供适配器与客户端测试共用合并判定：覆盖窗口内的事件不再拼接，未返回消息与覆盖之后的增量必须保留，旧页
  不得回滚新内容（同一 `through` 下 `lastDeltaIndex` 更少、或当前已拼入增量而新页为 `null`，都算更旧），
  epoch 不同只能 reset/重建。
- **D5 命令**：`RemoteCommandStatus` 按原 `(target, requestId)` 直接查询 Owner；`NotFound` 与
  `Uncertain` 是结果分类，Owner 不可达是 `Unavailable`。core 不写本地请求表、不去重、不换号；同一
  `requestId` 的幂等与参数冲突完全由 Owner 判定。`session_id = None` 仅用于尚未建成会话的
  `session.create`，其余操作必须先经运行期目录校验该会话属于 `owner_node_id`；目录条目本身损坏
  （摘要不是远程来源）按 `Corrupt` 失败关闭，不得折叠成 `NotFound`。
- **旧端口**：`RemoteDeliveryStore`/`RemoteSessionReads` 已在 v14 迁移中删除，core 不再从任何持久存储读取远程清单、来源身份或命令状态。

### 5.10 本机生命周期端口（计划，WP2 实施）

> **实现状态：尚未实现。** 本节用 `rust-planned` 围栏声明目标形状（`scripts/check-contract-drift.mjs` 不把它当作已实现签名；实现落地后可直接移入 §5 的常规 rust 块）。core 仍不得依赖 Tokio、SQLite、文件系统、wire DTO 或具体 Agent；时间一律由既有 `Clock` 端口注入，租约与资源代只存在于内存。

```rust-planned
/// 激活/恢复的合并入口：同一会话的并发激活共享一次建立过程。
#[async_trait]
pub trait SessionActivation: Send + Sync {
    /// 先查稳定 requestId 的持久记录，再判 ready 复用或启动恢复；`uncertain` 不自动换号重派。
    async fn activate(&self, request: ActivationRequest) -> Result<ActivationAdmission, PortError>;
    /// 完成一次激活：成功才把资源代置为 ready；`Failed`（可证明未建立）释放容量预占并收敛为 `Idle`，
    /// `Uncertain` 保留占位与 `Active` 事实。
    async fn settle(&self, session: SessionId, generation: ResourceGeneration, outcome: ActivationOutcome) -> Result<(), PortError>;
}

/// 查看声明、续租与释放。每连接最多一个当前查看者，租约不持久化。
#[async_trait]
pub trait ViewRegistry: Send + Sync {
    async fn declare(&self, connection: ConnectionId, session: Option<SessionId>, view_version: u64, now: Timestamp) -> Result<ViewDecision, PortError>;
    async fn renew(&self, connection: ConnectionId, view_version: u64, now: Timestamp) -> Result<ViewDecision, PortError>;
    async fn release(&self, connection: ConnectionId, now: Timestamp) -> Result<(), PortError>;
    async fn viewers(&self, session: SessionId, now: Timestamp) -> Result<Vec<ConnectionId>, PortError>;
}

/// 容量准入与安全候选选择：创建与恢复共用，预占也计入上限。
#[async_trait]
pub trait CapacityAdmission: Send + Sync {
    async fn reserve(&self, session: SessionId, now: Timestamp) -> Result<Admission, PortError>;
    async fn release(&self, session: SessionId, generation: ResourceGeneration, now: Timestamp) -> Result<(), PortError>;
    /// 只在「未受保护」的候选中选最久未使用者；同一时刻以稳定会话标识打破并列。
    async fn reclaim_candidate(&self, now: Timestamp) -> Result<Option<ReclaimCandidate>, PortError>;
}

/// 逐会话释放确认：只有适配器确认真实释放后才能交还名额。
#[async_trait]
pub trait SessionRelease: Send + Sync {
    async fn release_session(&self, session: SessionId, generation: ResourceGeneration, now: Timestamp) -> Result<ReleaseOutcome, PortError>;
}

/// 当前生命周期的只读查询：owned 当前查询与快照必须读同一代目录。
#[async_trait]
pub trait LifecycleQuery: Send + Sync {
    async fn local_activity(&self, session: SessionId) -> Result<LocalActivity, PortError>;
    async fn snapshot(&self) -> Result<Vec<SessionSummary>, PortError>;
}

// —— 纯类型：上面五个端口的输入/输出形状（最小字段 + 不变量）——

/// 激活请求。四个字段都是稳定值：同一 `(actor, request_id)` 的重试必须逐字相同，
/// 指纹不一致即 `command.idempotency_conflict`；激活路径不得自造 requestId 或指纹。
pub struct ActivationRequest {
    pub actor: Actor,
    pub request_id: RequestId,
    pub request_fingerprint: Digest,
    pub session: SessionId,
}

/// 受理三态，互斥。受理 ≠ 建立成功：`Prepared`/`InFlight` 之后仍要 `settle`。
pub enum ActivationAdmission {
    /// 本次调用获得建立权：容量名额已预占，仍须 `settle` 收口。
    Prepared,
    /// 同会话已有激活在建立中：不重复启动后端，等同一份终态。
    InFlight,
    /// 已有持久终态（幂等重试，或先前调用已收口）：原样返回记录，不再派发、不新建资源代。
    Settled(CommandRecord),
}

/// 建立结果三态。只有 `Ready` 让资源代进入 ready；`Uncertain` 保留容量占位与 `LocalActivity::Active`
/// （外部资源可能存在）；`Failed` 表示可证明没有建立资源：实现释放预占并把本机展示收敛为 `Idle`。
pub enum ActivationOutcome {
    Ready,
    /// 可证明未建立（能力不支持、身份丢失等）：释放预占，展示收敛为 `Idle`。
    Failed { error: PublicError },
    /// 结果不确定（超时、连接中断、进程状态未知）：保留占位与 `Active`，须经释放确认才收敛。
    Uncertain { error: PublicError },
}

/// 查看声明/续租的判定：`Accepted` 必须给出未来时刻，`Stale` 必须没有租期。
pub enum ViewStatus {
    Accepted,
    Stale,
}

pub struct ViewDecision {
    pub status: ViewStatus,
    /// 只有 `Accepted` 才为 `Some`，且必须晚于调用方的 `now`。
    pub lease_expires_at: Option<Timestamp>,
}

/// 容量准入：只有 `Reserved` 占住名额；`CapacityExceeded` 映射 `session.capacity_exceeded`（retryable）。
pub enum Admission {
    Reserved,
    CapacityExceeded,
}

/// 安全回收候选。「没有安全候选」是 `None`，不是空占位、哨兵会话或「随便挑一个」。
pub struct ReclaimCandidate {
    pub session: SessionId,
    /// 该会话最后一次仍受保护（查看租约/未结束任务/有效交互）的时刻；回收按此值最久优先。
    pub last_protected_at: Timestamp,
}

/// 逐会话释放确认：只有 `Released` 交还名额；`Failed`/`Uncertain` 继续占位并保持 `Active`。
pub enum ReleaseOutcome {
    Released,
    Failed,
    Uncertain,
}
```

- **纯类型**：`ActivationRequest`/`ActivationAdmission`/`ActivationOutcome`/`ViewDecision`/`Admission`/`ReclaimCandidate`/`ReleaseOutcome` 都只含 §3 的值对象与枚举（`ResourceGeneration`、`ViewLease`、`LocalActivity`、`Timestamp`、`SessionId`），不得出现 wire/DB/进程类型。字段与变体以 `rust-planned` 块为准，最小不变量如下：
  - **`ActivationRequest`**：`request_id`/`request_fingerprint`/`session`/`actor` 四者都是稳定输入——重试逐字复用，core 不得自造（否则一次重试会得到第二个幂等键，§6 第 6/20 条）。
  - **`ActivationAdmission`**：三态互斥，且**受理不是建立成功**。`Prepared` 已预占名额但仍须 `settle`；`InFlight` 表示本次不启动后端、等同一份终态；`Settled(CommandRecord)` 只装**终态**记录（`completed`/`failed`/`rejected`/`uncertain`），`accepted` 不得出现在这里（那是 `Prepared`/`InFlight`）。
  - **`ActivationOutcome`**：只有 `Ready` 让资源代就绪。`Failed` 表示适配器**可证明**没有建立资源（能力不支持、原身份不存在等）：实现 MUST 释放容量预占并把 `localActivity` 收敛为 `Idle`，同时如实报告失败、保留历史——不得用「新建会话」伪装成功。`Uncertain` 表示无法证明外部资源是否存在（超时、连接中断、进程状态未知）：实现 MUST 保留容量占位与 `Active` 事实，只有经 `SessionRelease::release_session` 确认释放后才能收敛。说不清的一律用 `Uncertain`，MUST NOT 用 `Failed` 冒充已释放。`settle` 对同一 `(session, generation)` 重复调用是幂等无操作（历史重试与崩溃恢复都会重放它）。
  - **`ViewDecision`**：`Accepted` 的 `lease_expires_at` 必须晚于调用方的 `now`；`Stale` 必须为 `None`。判定口径与 `SYNC_PROTOCOL.md` §11.8 逐条一致：同版本同载荷幂等接受；同版本但载荷已变化（含 `Some(A)`→`None`、`A`→`B`）与低于版本的离页都回 `Stale`，且 `Stale` 既不替换现有查看者/租期，也不建立或刷新查看保护。
  - **`Admission`**：只有 `Reserved` 占住名额；`CapacityExceeded` 不建立任何资源、不产生会话行，由适配层映射为 `session.capacity_exceeded`（`retryable = true`）。
  - **`ReclaimCandidate`**：「没有安全候选」是 `None`——不得用空 `SessionId`、哨兵值或「随便挑一个受保护会话」占位。`last_protected_at` 是最后一次仍受保护的时刻，`reclaim_candidate` 只在该值最久的**未受保护**会话里选，并列按稳定会话标识打破。
  - **`ReleaseOutcome`**：只有 `Released` 交还名额并允许 `Active → Idle`；`Failed`/`Uncertain` 继续占位、保持 `Active`，不得因「已经调用过释放」就提前放出名额。
- **固定锁顺序**：每会话既有串行门 → 节点容量准入；全局只短暂预占/确认名额，**不得**持全局锁等待 Agent I/O。关闭中/结果不确定的资源仍占位，禁止恢复与回收交错产生两个绑定。
- **适配器边界**：`agent-host` 提供有语义的单会话释放并执行真实 ACP close（§4 的「Agent 托管」）；`server::sync` 声明/续租/释放查看者，认证撤销与断开时主动通知；`app` 只负责扫描调度、任务所有权与关闭顺序，不做回收业务判断。
- **不落盘**：本节的目录、租约、资源代都不进 `storage-sqlite`，不需要 migration（§7）。

### 5.11 首条提交与准备附件暂存（WP2 已实现）

> 实现状态：**已实现**（`crates/storage-sqlite/src/first_submission.rs` / `staging.rs`；纯类型在 `core::model`，运行期目录在 `core::preparation`）。下面两个 trait 与 `ports.rs` 的方法集逐条相等；输入/输出纯类型见 §3.9（首条提交/派发/回显）与 §3.10（准备运行期与暂存）。core 仍不得依赖 Tokio、SQLite、文件系统、wire DTO 或具体 Agent。

```rust
#[async_trait]
pub trait FirstSubmission: Send + Sync {
    async fn commit_first_submission(&self, write: FirstSubmissionWrite) -> Result<FirstSubmissionAdmission, PortError>;
    async fn first_submission(&self, session: SessionId) -> Result<Option<FirstSubmissionRecord>, PortError>;
    async fn first_user_message(&self, session: SessionId) -> Result<Option<FirstUserMessage>, PortError>;
    async fn begin_dispatch(&self, fence: DispatchFence) -> Result<DispatchAdmission, PortError>;
    async fn settle_first_submission(&self, settlement: FirstSubmissionSettlement) -> Result<FirstSubmissionRecord, PortError>;
    async fn converge_first_submission(&self, session: SessionId, at: Timestamp) -> Result<FirstSubmissionConvergence, PortError>;
}

#[async_trait]
pub trait PreparationAttachmentStaging: Send + Sync {
    async fn begin(&self, begin: StagedTransferBegin) -> Result<StagedTransferState, PortError>;
    async fn append(&self, chunk: StagedChunk) -> Result<StagedChunkOutcome, PortError>;
    async fn seal(&self, seal: StagedSeal) -> Result<StagedSealOutcome, PortError>;
    async fn status(&self, transfer: AttachmentId) -> Result<Option<StagedTransferState>, PortError>;
    async fn abort(&self, transfer: AttachmentId, reason: StagedAbortReason) -> Result<(), PortError>;
    async fn begin_manifest(&self, begin: StagedManifestBegin) -> Result<StagedManifestState, PortError>;
    async fn append_manifest_page(&self, page: StagedManifestPage) -> Result<StagedManifestState, PortError>;
    async fn seal_manifest(&self, seal: StagedManifestSeal) -> Result<StagedManifestState, PortError>;
    async fn claim(&self, claim: StagedClaim) -> Result<Vec<ClaimedAttachment>, PortError>;
    async fn release_orphans(&self, before: Timestamp) -> Result<usize, PortError>;
}
```

- **受理 ≠ 派发**：`commit_first_submission` 只写 `saved_not_dispatched`；`begin_dispatch` 是唯一把阶段推进到 `dispatch_started` 的入口，且必须在调用 Agent 之前提交。`AlreadyCommitted` 不重建准备、不追加首条、不重传附件。
- **初始事件**：受理事务同时写首条用户消息进入事件流的初始事件（`user.message.delta`，`turn_id`/`causation` 指向本次受理，`source = "owned_first_submission"`）；正文权威仍是 `owned_message.content_json`，事件只做可见性与来源标注（D7），其 `text` 是按 §10.3 单条 message 文本上限（262144 字符）截断的文本块拼接。它的 sequence 早于该命令的终态事件。
- **同一事务**：正式会话行、完整首条输入（`owned_message` 的 `user` 通道）、命令受理、turn、`owned_first_submission` 与附件正式引用全在一个 `BEGIN IMMEDIATE`；任一步失败全回滚。
- **终态只写一处**：`settle_first_submission`/`converge_first_submission` 复用 owned 提交路径的同一个终态 writer（`apply_terminal`）与同一个事件 writer（`write_session_event`）：先写**领域事件**（`turn.completed`/`turn.failed`），再写命令终态；`owned_command` 的 `result_json`/`terminal_event_id` 与事件同源。turn 的领域事件只用 CAS 推进**仍非终态、且 `causation = first_request_id`** 的首条 turn：turn 已由 `agent_host::finish_turn` + broker `commit_chunk` 收敛时，结算不再改 `ended_at`、不再提升 `owned_session.version`、也不再写第二个 `turn.*`（`session.updated` 仍只由 core 生成，§6 第 1 条，存储层不伪造）。
- **冻结结果投影**：`command.completed` 的 `view.result` 与 `owned_command.result_json` 是 `SYNC_PROTOCOL.md` §11.9 的六字段投影（`stage`/`firstSubmissionId`/`firstRequestId`/`currentRequestId`/`everDispatched`/`canContinue`）。同一 `requestId` 的重放（含并发）返回原终态与原 `terminal_event_id`，不追加第二条 `command.*`、不改 `current_request_id`；已非 `accepted` 的行按 `Conflict(Consumed)` 拒绝。
- **读投影带记录的选择**：`FirstSubmissionRecord.mode_id`/`config` 原样暴露受理时记录的模式与配置选择（`config` 是记录原文 object，`{}` = 未选择），供 `kind:"continue"` 缺省 `mode`/`configOptions` 时复用（D8.1）。存储层不把文本值反推成 `Text`/`Select`。
- **暂存无 owned 外键行**：`PreparationAttachmentStaging` 只维护运行期元数据与 `<attachment_dir>/.preparation/` 中的字节；`claim` 只核验已封存事实，首次提交把字节 `rename` 到同一内容寻址路径，不复制、不全量重哈希。受理事务回滚时 `finish_claim(committed = false)` **只把本次认领真的 `rename` 过**的字节移回封存路径（正式路径上原本就存在的同内容字节属于更早已提交的会话，绝不移回），且 `begin_claim` 也接受已在正式路径的字节，因此同 ID 可原样重试（D8 第 2 行）；提交成功的清单与其传输记录一并从运行期目录清除。
- **连接绑定**：传输/清单记录绑定连接标识；旧连接迟到帧返回 `ConnectionMismatch`，重绑只转移连接归属、不重置偏移。

## 6. Broker 的顺序与事务契约

1. owned：`EventSink` → broker 组装 `OwnedCommit` → `SessionStore::commit` → `EventPublisher::publish`（顺序不可交换）。
    - `[决定]`（`session-activity-three-states`）`session.updated` 的**唯一生产者是 core**：状态变化时 broker 生成一条 `EventKind::State` 事件，payload 是提交后的完整会话摘要、版本嵌在 `session.version`（与 `sessionSummary` 同形）。状态不变不生成；同一次提交最多一条；仲裁、确认、turn 终态与恢复路径都不得各自另发，也不得由适配器/存储层伪造。
2. imported：`node-link-client` → broker 组装 `DeliveryReceipt` → `RemoteDeliveryStore::commit_receipt` → 发布（正文只在内存）。
3. `[决定]` 每会话串行：一个会话一个 actor，命令/后端事件/终态在同一串行队列内处理；不同会话并行。
4. `[决定]` active turn：同一会话最多一个非终态 turn。`sessions.queue_policy = queue` 时按持久化接受顺序排队，超过 `sessions.max_queued_turns`（16）返回 `session.busy`；`= reject_busy` 时直接返回 `session.busy`（`CONFIG_REFERENCE.md` §4）。
5. `[决定]` 授权调用点：设备与节点方向**没有命令级判定**——认证通过即可执行业务命令；配对认领方永远不授权。core 不信任 adapter 的“已授权”标志。
6. `[决定]` 幂等键是**协议维度**的 `(actor, requestId)`：Sync 侧 actor 由 `deviceId` 决定（`SYNC_PROTOCOL.md` §11.1），Node Link 侧由 `(ownerNodeId, accessNodeId)` 决定（`NODE_LINK_PROTOCOL.md` §12.5）。重复提交时必须比对 `command`/`kind`/`session`/`expected_version`/`request_fingerprint`：任一不同 → `command.idempotency_conflict`；全同 → 返回首次结果（`CommitOutcome::replayed`），**不得**二次派发。
7. `[决定]` 权限仲裁与确认是**两个阶段**：仲裁（`SessionUpdate.interaction`）沿用 first-writer-wins，只把行推进到 `state='resolved'` 并写 `resolved_at`——它阻止其它设备重复应答，但在端点确认前该交互仍计入有效待处理、会话摘要仍是 `pending`；确认（`SessionUpdate.confirmation`）才写 `confirmed_at` 并把它移出有效集合。`interaction.already_resolved` 之后到达的应答必须被拒绝且不覆盖既有结果；重复确认是无操作，不产生第二次副作用。两个阶段都与会话摘要、`session.updated` 同事务。
8. `[决定]` 模型/配置切换只在 turn 边界生效；非终态 turn 期间的 `set_mode`/`set_config` 排队到边界或返回 `state.version_conflict`。
9. `[决定]` 磁盘写失败：`commit` 返回 `PortError::Unavailable` → 命令显式失败或 `uncertain`；**不得**发布对应事件（`SECURITY_DESIGN.md` §15）。
    - **非终态批次失败不得只丢弃**（`node-link-owner` 的修复轮次）：失败批次携带在跑 turn 的事件时，broker 必须用一次提交把该 turn 终结为 `failed`、把命令置为 `uncertain`，并把该 turn **已落盘** delta 的 `agent.message.completed`（第 14 条）与它们同批提交——缺的那一块不伪造。
    - 被终结的 turn 进入「已放弃」集合：它后续到达的事件（含终态）不再提交，因此库里不会出现「报完成但正文缺失」的记录；适配器不带 turn 标识时，无在跑 turn 的迟到事件按最后被放弃的 turn 归属（归属集合见第 19 条的 `turnId` 集合），而不是被记到下一个 turn 上。
    - **被放弃的 turn 继续占住会话槽**（`node-link-owner` 的修复轮次）：在它的终态被端点观测到之前，`dispatch_one` **不得**提升下一个排队 turn。理由是归属只能靠「有在跑 turn 时归给它」推断（第 19 条）：一旦提前派发，下一个 turn 就成了那个「在跑的 turn」，上一个 turn 的迟到事件（含终态）会被记到它头上——客户端看到下一个命令 `completed`，而它的正文被按上一个 turn 的收尾折叠。占住期间它的迟到事件（含 permission/elicitation 请求）一律按它归属并丢弃：不落库、不写交互行、不广播——这是相对第 19 条「无归属时照常落库（`turnId` 为 NULL）」的**有意更强的处置**，只在有可归属的被放弃 turn 时适用；收敛这个已经终结为 `uncertain` 的 turn 是端点的职责。占住期间该会话按**仍有 active turn** 处理（`TurnQueue.running` 未清空），客户端可见后果是：`session.mode.set`/`session.config.set` 返回 `state.version_conflict`（第 8 条），`session.prompt` 按 `sessions.queue_policy` 受理排队或返回 `session.busy`（第 4 条）。
        - 释放占位有两条路径：①该 turn 的终态事件到达（按上面丢弃，但视为「端点已观测到它结束」）；②兜底——占位消耗的驱动轮次达到 core 常量 `ABANDONED_TURN_HOLD_ROUNDS`（`1200` 轮；组合根按 `storage.flush_interval_ms` 驱动活动会话，默认 250 ms 一轮 ≈ 5 分钟）。core 不读时钟、不设定时器，因此兜底按轮次而不按墙钟计时：端点永不收敛时优先让会话继续可用。
        - 仍有排队 turn 时，这次放弃提交把会话状态归约为 `running`（占位 turn 已失败，但仍有未结束 turn）：三态下没有 `queued` 主状态，组合根的合并窗口按未结束 turn/持久队列驱动，而不是只筛旧状态。
        - 释放之后无归属事件回到「归给在跑的 turn」的既有规则（第 19 条/§10.3），因此兜底是最后手段：它只在端点确实不再收敛时触发。**它的代价要显式登记**：占位一旦释放，该被放弃 turn 若还有迟到事件（含它终态之后的尾巴），会按在跑的 turn 归属并照常提交——兜底把窗口收窄到有界轮次，不是消除它；这是「提前派发（每个被放弃的 turn 都会发作）」与「会话永久卡住」之间的取舍，彻底消除需要适配器为 `EndpointEvent` 给出权威 turn 标识（本切片不要求）。
    - 该路径不压缩已落盘的 delta（第 15 条是可选动作），也不向调用方冒泡错误（与带终态的批次失败时的既有口径一致）：可观测信号就是上面两条持久事件（core 不含日志依赖，`AGENTS.md` §8）；失败批次没有在跑 turn 可归属时（例如会话级事件）仍然只有「不发布」这一个效果，没有命令可终结。
10. `[决定]` `storage.flush_interval_ms = 250` 允许把同一会话短窗口内的 delta 合并为一次 `commit`；合并不得改变顺序、不得跨 turn 边界、不得延迟终态事件。
11. `[决定]` `OwnedCommit.events` 只接受 `StoredPolicy`；`Ephemeral` 由 broker 在组装前过滤（内存转发），`commit` 收到 `Ephemeral` 视为 `InvalidRequest`（类型上已不可表达）。
12. `[决定]` 快照与重放必须来自 `SessionStore::read_view()` 返回的同一读视图，`sync.snapshot_begin`/`sync.snapshot_end` 与之后的事件游标都以该视图的 `head()` 为 barrier（`SYNC_PROTOCOL.md` §9.3/§9.4）。
13. `[决定]` **交互（权限 / elicitation）的创建与解析**：
    - 创建：Agent 的请求由 broker 组装成一条 `kind = interaction` 的事件**加上** `OwnedCommit.interactions` 里的 `PendingInteractionWrite`，**同一次 `commit`**（§9 判据 15）。同一事务里还要按归约重算摘要（新增交互把会话推到 `pending`），状态变化时附带一条 `session.updated`；三件事（交互行、事件、摘要/`session.updated`）要么全落盘要么全不落盘。写入形状**不带** `request_event`（`event_id` 由存储层分配、装配方预知不了）：存储层在同一提交里找 `kind = 'interaction'` 且 payload 的 `interactionId` 等于 `interaction.id()` 的那条事件，把该事件的 `global_sequence` 写进 `owned_interaction.request_event`（列类型是 `INTEGER REFERENCES owned_event(global_sequence)`，不是 `event_id`）；配不到（事件缺失或被 Ephemeral 过滤）→ 整事务 `InvalidRequest`，**不得**写入悬空引用。
    - 解析：走 `OwnedCommit.state.interaction`（`InteractionResolved { interaction, resolution, resolved_by, request_version }`）——**不是** `SessionStore` 的独立方法，因此「行更新 + 摘要 + `session.updated` + `*.resolved` 事件 + 命令终态」仍在 `commit` 的单次事务边界内。`SessionEndpoint::resolve_interaction` 只负责把决定交给后端。`request_version` 是可选版本校验：不符 → 整事务 `Conflict(VersionMismatch)`，不消费请求。
    - 仲裁顺序（不可交换）：先条件更新落盘仲裁（`state='resolved'`、`resolved_at`，**`confirmed_at` 保持 `NULL`**）→ 再派发后端 → 最后 flush 后端发出的 `*.resolved` 事件。受影响行数为 0 时存储层在同一事务内回读：行存在且 `state <> 'pending'` → `PortError::Conflict(AlreadyResolved)`；无行 → `PortError::NotFound`。
    - 确认：`OwnedCommit.state.confirmation`（`Some(InteractionId)`）把**已 `resolved` 且未确认**的行写入 `confirmed_at`；行不存在/仍 `pending`/已确认时是无操作（不覆盖、不重复写）。确认提交同时重算摘要：若状态因此从 `pending` 变为 `running`/`idle`，`session.updated` 与该确认**同一次提交**；状态未变时只落确认、不重复生成 `session.updated`。未确认的已仲裁行仍计入有效待处理（§5.2 的 `SessionActivityFacts`）。
    - **turn 终态失效**：turn 进入 `completed`/`failed`/`cancelled`（含派发失败、放弃与启动恢复的终结）时，存储层在同一次提交里把该 `turn_id` 下仍未失效的交互置为 `expired` 并写 `invalidated_at`（`pending` 与「`resolved` 未确认」都失效），broker 用 `SessionActivityFacts.unresolved_for_turn(turn)` 扣减待处理数并重算摘要——只失效**本 turn** 的行，绝不误伤其它 turn。终态提交里的 `session.updated` 最多一条。
    - 崩溃窗口：仲裁与确认之间崩溃会留下「行已 `resolved`、`confirmed_at` 未写」；重启恢复按 §6 第 16 条先调用 `invalidate_pending` 把仍有效的行失效（不可复活），因此不会永久停在待处理。重试同一条应答返回 `AlreadyResolved` 且**不**二次派发；UI 以交互行（`HistoryInclude.pending_interactions`）恢复，不依赖事件流补齐。
    - `options` **不落库**（`owned_interaction` 无该列）：读视图里 `PendingInteraction.options` 恒为空。还原路径是事件流（`replay` 返回的 `CommittedEvent` 带 `id`；`read_session` 返回的 `events`/`history` 元素直接带持久化正文），再用 `ReadView::event_payload(id)` 取那条 `interaction` 事件的正文并解析候选项（§5.2）；不要拿 `request_event`（它是 `global_sequence`）直接当 `event_payload` 的参数。
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
16. `[决定]` **启动恢复**（`SYNC_PROTOCOL.md` §11：「Daemon 恢复时发现外部 Agent 副作用无法确认，必须先持久化 `command.uncertain`，再向客户端广播」）：组合根在取得单实例锁、开始监听**之前**，必须以 `LocalCli` actor 调用 `RecoverUnsettled` 用例；它对 `SessionStore::unsettled_commands` 返回的每条 `accepted` 命令，在该会话的串行门内终结为 `uncertain`——写终态事件 `command.uncertain { requestId, reason, mayHaveReachedAgent: true }` 并更新幂等行（`status`/`terminal_event_id`），同时把该命令对应的未终态 turn 终结为 `failed`（`PublicError { code: "command.uncertain" }`）。有会话与无会话两类都要覆盖：`session.create` 在受理与落账之间崩溃留下的 `accepted` 行是**无会话**的（`session_id` 为 `NULL`、没有会话行，第 20 条），它不进任何会话串行门、也没有可终结的 turn，恢复只为它写一条 `session` 为空（`None`）的 `command.uncertain` 提交——本机因此不会出现该会话的会话行。**不得**自动重放副作用，也不得让 `accepted` 行静默存活。`uncertain` 行不参与 TTL 清理（§7.5）。
    - 同一趟恢复还要补写**缺失的收尾事件**：若某个 turn 已有已提交的 `agent.message.delta` 却没有 `agent.message.completed`（进程在终态提交前崩溃），必须用 `ReadView::event_payload` 从库里按第 14 条的规则重建该事件并在**同一次恢复提交**内落盘，否则那段已经广播出去的文本永远没有终态记录。
    - **交互与队列的恢复顺序**（`session-activity` D3）：恢复开始时先调用 `InteractionAuthority::invalidate_pending(None, now)` 把仍有效的交互（含已仲裁未确认）失效——重启后上游 ACP 请求已不存在，不得再被当作可应答，也不得复活；然后按 `unsettled_commands` 终结 `accepted` 命令（对应 turn → `failed`，同事务失效该 turn 的交互），最后用 `SessionStore::sessions_with_pending_activity(limit)` 补齐仍有未结束 turn/有效交互的会话，逐个 `pump` 并 `refresh_activity` 重算摘要。恢复可重复执行：已终结的行不会被再次终结，已失效的交互不会被重新计数。
    - **持久队列的恢复**：`SessionStore::queued_turns` 按 `queue_index` 升序给出遗留 `queued` turn；`Session.prompt` 正文不在 `owned_turn` 里（只由 Agent 回显的 `user.message.delta` 留下），因此**不得**重放它们的 Agent 副作用。内存队列为空且没有在跑 turn 时，broker 把这些遗留项显式终结为 `failed`（`internal.unavailable`，不静默删除）、`accepted` 命令由恢复结算为 `uncertain`，然后 FIFO 从后续项继续；同一会话最多一个 active turn 的不变量不变。**不确定的工作不重派**：`uncertain` 命令、被放弃的 turn 与无法确认停止的执行都不自动重试，后者保留占位与运行阻塞，不提前 `idle`、不提升后续 turn。
    - **摘要收敛**：同一趟恢复里每个会话的摘要只按 `SessionActivity::reduce` 重算一次，状态未变不写行、不生成 `session.updated`；变化时摘要更新与 `session.updated` 同一次提交，不得重复生成。
17. `[决定]` **`session.mode.list` 的应答**：结果形状是 `ModeState { currentModeId, availableModes: ModeRef[], version }`（`SYNC_PROTOCOL.md` §11.5/§10.2）；`availableModes` **只能**来自 `SessionEndpoint::modes()`（§5.1），`version` 取该会话当前版本。core 不得凭 `current_mode` 编造候选列表，端口返回空列表时结果就是空列表（不伪造）。
18. `[决定]` **附件文件与行的事务边界**：行的删除与它所在的事务一起提交，**文件删除一律在提交之后**（崩溃只会留下无人引用的孤儿文件，绝不会留下悬空行）；`AttachmentStore::prune_lru` 与 `sweep_orphans` 因此都在事务之外运行。
    - **孤儿回收**由组合根在启动时调用一次 `sweep_orphans(启动时刻, 1000)`（紧跟 §6 第 16 条的恢复之后）：只删「不在 `owned_attachment` 里**且** `mtime` 早于本次进程启动时刻」的文件——第二条规则保护正在写入、行还没提交的新附件。它只扫描 `storage.attachment_dir` 的**顶层**条目；传输临时文件在 `attachment_dir/.transfers/` 子目录里，因此周期维护（每 60 s、以 `now` 为界）不会误删正在上传的文件。回收失败**不阻止启动**，记一次结构化警告，剩余孤儿留到下次启动。
19. `[决定]` **提交前的 view 收口（`SYNC_PROTOCOL.md` §10.3 的身份与会话版本）**：owned 提交在 `commit_owned` 漏斗内、调用 `SessionStore::commit` **之前**完成两件事，因此落盘 view、重放 view 与广播所依据的 payload 同源：
    - **turn 归属**：事件类型属于 §10.3 要求 `turnId` 的集合（`turn.*`、`user.message.delta`、`agent.message.delta`、`agent.message.completed`、`agent.thought.delta`、`tool.call.started`/`updated`/`completed`、`permission.requested`、`elicitation.requested`）且该事件已被归属到某个 turn 时，view 顶层必须有 `turnId`，取值等于 core 已定稿的权威 turn；**不得**向其它事件类型添加未协商字段，无归属的事件不得出现该字段。适配器已给出同名字段时：取值一致 → 保留原字节；取值不同或值不是字符串 → 显式 `InvalidRequest`、不写任何行、不发布任何帧。归属只在批组装时定稿一次，注入是它的唯一消费者（不重新推导）。
    - **会话版本**：事件类型属于 §10.3 要求 `version` 的集合（`session.mode.changed`、`session.config.changed`）时，view 顶层必须有十进制字符串 `version`，取值等于该次提交后的会话版本。推导规则与存储层一致：含 `StateChange` 的提交为当前版本 + 1，否则不变；提交后必须与 `CommitOutcome.version` 比对，不一致 → `PortError::Corrupt`、不发布该批、不得报告成功（比对发生在存储返回之后，已落盘的行不由 core 撤销）。幂等命中（`replayed`）时不比对：返回的是首次提交的结果，第二次提交的 view 不得被重写。imported 路径**不**注入这两个字段（`turnId`/`version` 由拥有该会话的节点注入，`payloadDigest` 覆盖 Owner 给出的视图字节），只保留其取值。
    - **两个已登记的边界**：① 失败关闭（`turnId` 冲突或版本漂移）发生在 `flush` 组装之后，该批适配器事件**不再重投**（调用方按本条 ① 的失败语义——与 §6 第 9 条同口径——决定是否把 turn 判为失败），不得重试时假装该批从未到达；② 无状态变更的提交里存储层**不**校验 `expected_version`（§5.2 只对 `Update` 校验），因此 core 的推导/比对就是该组合的失败关闭点，且可能发生在落盘之后。
    - **无归属的降级**：turn 终结后晚到的、类型属于 §10.3 `turnId` 集合的事件（适配器异步尾巴）没有权威 turn，**不**注入（`owned_event.turn_id` 与 view 同时为 NULL），宁可缺字段也不伪造；该降级必须有用例固定，并留给 Sync 切片裁定是否拒绝。**例外**：被放弃的 turn（§6 第 9 条）的迟到事件按那一条归属到它并被丢弃，不走本降级——第 9 条对同一事件类有意给出比本条更强的处置（丢弃 > 无归属落库），因为「正文已经缺块的 turn」不得再往库里写尾巴；两条的适用边界就是「有没有可归属的被放弃 turn」。
20. `[决定]` **`session.create` 的幂等与终态落盘**（`node-link-owner` 的修复轮次；`session-create-settle-order` 变更改为**三笔时序**）：走**受理 → 执行 → 落账**，幂等键是第 6 条的 `(actor, requestId)`（Owner 侧 `actor` 是该 `access` 对端，`kind = mutation`，`expected_version = None`，`command = "session.create"`）：
    - **受理提交**：幂等行**单独**落盘（`StateChange` 为 `None`、`session_id` 为 `NULL`、无会话行）——受理因此不产生任何资源；`session_id` 由 **core** 用 `IdGenerator::session_id` 在调用后端**之前**分配，落账提交时才建行并**回填该列**（`StateChange::SettleCreate`，见下一条）。`request_fingerprint` 取 ACPR-CJ1 之后的解码 payload 摘要，由适配层计算并作为参数传入（core 不依赖 `acpr-wire`）。
    - **幂等比对里的 `session` 分量**：`IdempotencyRecord.session = None` 的语义是「装配期无目标会话」（目前只有 `session.create`），存储层**不**用行里记的会话标识与它比对；`command`/`kind`/`expected_version`/`request_fingerprint` 四项仍然恒比（第 6 条）。命中且四项相同 → 返回既有记录（`CommitOutcome.replayed`，不创建第二个会话、不开第二个后端端点）：仍是 `accepted` 时它是**窗口期重发**（core 回受理态 `SessionCreateAdmission::InFlight`，不再派发创建），已终结时回首次结果（`Settled`）；任一不同 → `PortError::Conflict(IdempotencyConflict)`。
    - **落账提交**（`session-create-settle-order` 变更）：`StateChange::SettleCreate` + `command_terminal` 在**同一事务**里完成——用 core 早已分配的标识建会话行（含 `agent_session_id`、core 自己解析的 `workspace_cwd` 与目录别名）、**回填**幂等行的 `session_id`、写终态事件（`command.completed`/`command.uncertain`）。此后会话行与终态同批可见；窗口期（会话行落账之前）到达的后端事件在会话槽位的**有界**队列里排队（上限 `WINDOW_EVENT_QUEUE_LIMIT = 256`），落账成功后按到达顺序落盘再广播。队列**超限**走失败关闭：先结束该会话所属的 Agent 运行时（`SessionBackendFactory::terminate_agent`——先把运行时从运行目录摘掉使 `is_running()` 为假且不再被复用、让未完成请求以明确结果结束，再回收进程树与子任务，即 `local-agent-host` 的「超限结束的失败关闭顺序」），再作废那些没有归属的事件，最后把命令终结为 `uncertain`；失败关闭之后该槽位被**围栏**，此后到达的事件直接丢弃（不再缓冲——该会话不会有会话行，组合根的合并窗口枚举不到它，因此没有消费者，缓冲只会变成永久滞留）。没有窗口期状态时的终态提交（后端失败那条路径的兜底）按 `request_id + session_id IS NULL` 定位「尚无会话行」，`completed` 在那条形状上不可表达。
    - **终态提交的形状**：一条 `command.completed`/`command.failed`/`command.uncertain` 事件（`causation = requestId`）+ `CommandTerminalRecord`。`completed` 的 `result` 是适配层投影的结果原文（Node Link 的 `SessionCreateResult`）、`completed` 必须有 `terminal_event_id`（§7.3 的 CHECK）；`failed`/`uncertain` 携带结构化错误。没有持久记录（创建在幂等行落盘前就失败）或记录已终结时，终态提交是**幂等 no-op**，不覆盖首次结果。
    - **落盘前失败不构成持久首次结果**（`node-link-owner` 的修复轮次）：幂等行落盘前失败（如存储写失败）的 `session.create` 没有持久记录，因此同 `requestId` 重查回 `nodelink.command.not_found`，且重试可以创建出另一个会话、得到与首次尝试不同的结果——`failed` 终态只适用于适配层在同一 `requestId` 上能复现的确定类失败（授权拒绝、本机 workspace 解析失败），不得把落盘失败也描述成「重试结果确定」。
    - **`settle_session_create` 只终结 `session.create` 的记录**（`node-link-owner` 的修复轮次）：该 `(actor, requestId)` 的持久记录 `command != "session.create"` 时返回 `InvalidRequest` 且零写入（适配层误用，wire 不可达），不得把别的命令的幂等行改写成创建的终态。
    - **崩溃窗口**：受理与落账之间崩溃留下 `accepted` 行（`session_id` 仍为 `NULL`）+ **没有**会话行；第 16 条的启动恢复把它终结为 `uncertain`（`command.uncertain` 事件 + `terminal_event_id`），**不**重放副作用、也不猜测创建是否成功。这一行是**无会话**命令：恢复走「尚无会话」分支（按 `request_id + session_id IS NULL` 定位），本机不会出现该会话的会话行。
21. `[决定]` **core 派生的三类生产者**（`sync-scope-and-pwa-client` 变更 WP3；`design.md` D4/D5/D6/D7）：协议早已登记而 core 长期没有生产者的三类事件在本条落地。它们都是**投影**（读适配器给的 `view`/ACP 原文），不新增端口、不新增表、不新增 migration。
    - **`file.changed` 的派生源唯一**（R5）：只来自 ACP 工具调用内容里的**类型化 Diff 元素**——适配器（`agent-host`）把它逐字节投影成 `tool.call.*` 事件 view 的 `diff` 键（形式为元素数组，元素含 `path`/`oldText`/`newText`/逐字节 `raw`），broker 按该键派生。**MUST NOT** 从 `rawInput` 等自由形状字段推断文件改动；同一个工具调用可能在 `tool.call.started` 与 `tool.call.updated` 里重复携带同一个 Diff 元素，因此派生的去重键是 `(toolCallId, 工具调用给出的**原始 `path` 原文**)`——**不是**展示路径（越界时按 R7 收窄为文件名，是有损的，两个不同目录下的同名越界文件会碰撞而让其一被静默丢弃），且只登记**已提交**的键（落盘失败的批次不得吃掉重试的派生）。派生**不改动**该次 ACP 原文的字节、其摘要与字节长度（`payload.acp` 三要素在派生前后逐字不变）。
    - **行数用行级差异统计**（R6）：`addedLines`/`deletedLines` 由 `oldText`/`newText` 之间的**行级 Myers diff** 得出（`D = (N−lcs)+(M−lcs)`，`added = M−lcs`、`deleted = N−lcs`，只求编辑距离即可，因此实现是线性空间）；**MUST NOT** 用两段文本的行数相减代替——行数相等而内容不同时必须仍报出非零改动。`oldText` 缺席 = 新建文件（全部行计新增、删除为零）；`newText` 缺席或超出工作预算 = **省略这两项**（`MUST NOT` 以零代替未知）。省略时那两个键在 view 里整个缺席。
    - **展示路径相对化**（R7）：`displayPath` 是相对该会话**工作目录根**（`owned_session.workspace_cwd`，经 `SessionStore::load_recovery` 窄读取取得）的形式，不下发绝对路径、不含工作目录根的任何片段或回退层级；经**规范化后**的组件前缀关系判定不在其下（含工作目录根未登记、报告路径是绝对路径而根不存在、相对形式带回退层级逃出根）时置 `outsideWorkspace: true` 且只下发文件名称。字符串前缀相同但不在其下（`/work/api` 与 `/work/api-tools`）不得被判为工作区内。
    - **节点级 Agent 事件**（R8）：`agent.connected`/`agent.disconnected` 由适配器经 `NodeEventSink` 交付**已构造好的 `EndpointEvent`**，core 侧唯一入口是 `Broker::commit_node_event`——校验后按 `session: None` 的 `OwnedCommit` 落库（`owned_event` 的 `session_id`/`session_sequence`/`origin_epoch`/`origin_sequence` 四列由既有 CHECK 约束为同时为空），**不新增 migration**。`state` 是封闭词表且与该事件类型一一对应；这些事件**不表达**任何会话的活跃程度，也不得经会话级投递路径归属到任意会话。
    - **会话标题有两个来源，有效标题是投影**（R9；`custom-session-title` 的 design D1/D2）：`owned_session.title` 仍只承载 **Agent 标题**，来源是 ACP `session_info_update` 投影出的 `session.info.changed`；用户自定义标题由新命令 `session.title.set`（feature `core.session-title.v1`）写入**实现轮次新增的可空列** `owned_session.custom_title`（migration 与 §7.3 的 DDL 必须同时更新，drift 门禁要求两者逐条相同）。有效标题 = `custom_title` 优先 → 否则 `title` → 否则未命名，由 core 的摘要投影在**一处**计算并复用既有 `SessionSummary.title` 下发，不新增 `customTitle` 键。`StateChange::Update.title` 仍是**两层可选**（`None` = 不改该列、`Some(None)` = 显式置空、`Some(Some(text))` = 写入），且只写 Agent 列：判定「通知有没有携带标题」读的是 **ACP 原文**（`payload.acp` 的 `params.update.title` 键是否存在）——公共 view 的 `title` 是 `SYNC_PROTOCOL.md` §10.3 的 required 字段（`string|null`），键永远存在，表达不了「缺席 vs 显式 null」；ACP 原文不可用时退回 view 并把 `null` 当作「不改」（最保守解释，绝不把「不改」误判成「清空」）。Agent 通知与显式清空 MUST NOT 触碰 `custom_title`。标题超长（> 512 字符）→ 失败关闭（`InvalidRequest`），不写入违反值对象不变量的取值。**`updatedAt` 取 Daemon 持久化时间**（`commit.at`，由存储层写进 `owned_session.updated_at`）：view 里照常转发 Agent 自报值，但决定排序的权威时间不是它。标题写入按 design D2 的约束落账：`expectedVersion` 为 `None`、`kind = mutation`、`command = "session.title.set"`，状态写入 + 版本推进 + 完整 `session.updated` + 命令终态在**同一次提交**完成（先提交再广播），同一 `requestId` 重试返回首次结果、参数不同报幂等冲突；尚无 `custom_title` 时即使保存文字等于 Agent 标题也**必须**建立覆盖值，已有相同覆盖值时不重复变更业务状态但命令仍明确终结。客户端与 Access 都不得自行推导优先级。

22. `[决定]` **附件传输与消息绑定的提交顺序**（计划，WP2 实施；端口见 §5.8、表见 §7.6）：
    - 上传：`begin` 只建传输行与临时文件；每个 `chunk` 按期望偏移做条件更新并追加到临时文件（重复同内容返回原确认，不同内容拒绝）；`seal` 在同一事务内校验总长度与流式 SHA-256 后把传输置为 `sealed` 并记录摘要。文件先写临时名，校验通过后 rename，**再**允许后续业务事务引用；跨资源原子性只保证「最坏留下孤儿文件，不留下悬空引用」。
    - 提交：附件的引用绑定与 `session.prompt` 的幂等记录、消息行、清单引用在**同一业务事务**内完成；接受前重新校验清单 `sealed`、身份、会话、主体与 Agent 能力。上传成功不提前派发 Agent。
    - 清理：先提交引用/行删除，再在事务之外删除文件（§6 第 18 条）；`prune_transfers`/`prune` 先把超过
      24 小时的 `open`/`sealed` 清单过期（条目级联删除，释放其对附件的引用），再清 `receiving` 且
      `updated_at` 早于 24 小时窗口的临时传输，与 seal/提交用条件更新互斥，不误删进行中的传输或仍被
      引用的内容。传输临时文件位于 `attachment_dir/.transfers/` 子目录，孤儿回收只扫顶层，因此周期
      维护不会误删活跃上传。成功绑定后清单行立即删除，未绑定清单最多保留 24 小时。
    - 撤销：设备/节点撤销在事务内把相关 `receiving` 传输置为 `aborted` 并拒绝后续块/读取；已提交附件不因撤销被删除，按消息保留策略处理。
    - 失败：磁盘满、摘要不符、预算耗尽都不得留下「可被 prompt 引用的半成品」；失败关闭路径与 §8 一致。
23. `[决定]` **本机生命周期的归约、事件与收敛**（`session-activation-lifecycle` 的合同；实现待 WP2，端口见 §5.10）：
    - **归约唯一**：`LocalActivity` 只由 §3.3 的规则归约（有效交互优先 `Pending` → 未结束任务/ready 资源/回收未确认 `Active` → 否则 `Idle`）。首次 activating 不提前显示 `Active`；失败不覆盖真实任务或交互；`uncertain`、关闭失败与回收中保持 `Active` 与容量占位，**只有确认资源释放后**才转 `Idle`。
    - **先持久化后发布**：生命周期变化写 owned 持久事件 `session.activity.changed`（`view = { session }`，完整摘要），与该次状态变化在**同一次 `commit`** 内落盘，落盘成功后才经 `EventPublisher` 广播；`commit` 失败不得广播。事件保存当时事实，不作为重启后资源仍存活的凭据。
    - **启动收敛**：组合根在开始监听前完成既有 uncertain/任务恢复，确认旧进程停止并清空**所有**运行资源、资源代与查看租约；随后为「曾发布 `Active` 但现已失效」的 owned 会话提交一次收敛事件（`localActivity` 变为真实值）。旧历史里的 `Active` MUST NOT 被当作资源已恢复；重放与快照沿既有屏障与去重规则收敛。
    - **owned 当前查询与快照同代**：`localActivity`、快照 `sessions` 与 `session.read` 的摘要必须读**同一代**运行期目录与任务事实；不得把租约、查看者、资源代或进程句柄写入任何快照、事件或 wire 字段。
    - **不交错**：激活与回收由每会话串行门互斥；关闭中/结果不确定的资源保留容量占位，禁止「回收未确认时又启动恢复」产生两个绑定。容量驱逐只选未受保护（无查看者、无未结束/排队任务、无有效交互、无停止未确认执行）的候选，`sessions.idle_timeout_ms = 0` 只关闭按时间回收，不关闭容量策略。
    - **命令合并**：同一会话的并发激活共享一次建立过程；先查稳定 `requestId` 的持久记录，再判 ready 复用或启动恢复。旧命令重试返回历史结果**不**等于当前资源 ready；`uncertain` 的恢复不得换号自动重试，必须先核实状态。查看者只按最新声明集合计算，不因旧命令结果重新添加。
    - **不持久化**：查看者、租约与资源代只存在于运行期（§5.10），不新增表、不新增列；`session.activity.changed` 复用既有 owned 事件表。

## 7. `storage-sqlite` v16 表结构（仅 owned 家族与连接资料）

### 7.1 文件、PRAGMA、连接与权限

- 单文件 `<data_dir>/acp-remote.sqlite3`；附件目录 `<data_dir>/attachments`（`[决定]` 新增配置键 `storage.attachment_dir`，默认 `<data_dir>/attachments`，语义与权限同数据库目录）。
- `[决定]` `session-file-browser` **不新增持久字段、表或 migration**：会话文件的根仍取既有 `owned_session.workspace_cwd`（§3.6 的窄读取），枚举/内容/Git 结果只在内存与 wire 中存在；不保存内容快照、文件正文、Git 差异或翻页位置。查询不改变 `owned_*`/`imported_*` 的任何行，也不推进 `global_sequence`。
- `[决定]` `session-activation-lifecycle` **本阶段不改 DDL**（当前为 **v16 = 16**，v16 只由 `web-session-preparation` 的首条表与 `owned_message.channel = user` 引入）：本机生命周期的资源代、查看租约、当前活跃目录都是运行期状态（§5.10、§6 第 23 条），`session.activity.changed` 复用既有 `owned_event` 表与既有列，`localActivity` 是投影而非新列。**若实现阶段确需新增列**（例如为区分任务/资源事实而必须持久化新事实），必须先在本节登记版本化 migration、提供旧库升级测试并同步 §7.2 的版本常量与 §9 判据；禁止只加列不改门禁，也禁止把租约、查看者或进程句柄写进任何持久表。
- `[决定]` 启动设置：`journal_mode = WAL`、`synchronous = FULL`、`foreign_keys = ON`、`busy_timeout = 5000`、`wal_autocheckpoint = 1000`。
- `[决定]` 连接模型：1 个写连接（串行化写事务）＋ N 个只读连接；`read_view()` 打开一个显式只读事务（`BEGIN`）供快照使用。
- `[决定]` 权限：Unix 目录 `0700`、数据库/WAL/`-shm`/附件 `0600`；启动时检查，正式模式失败关闭（`SECURITY_DESIGN.md` §13.2）。
- `[决定]` **Windows**：ACL 读取不可用（workspace 禁 `unsafe`，也没有 ACL 封装依赖），判定返回 `Unverifiable` 且**不**失败关闭——只记录一次结构化警告；数据目录默认位于用户 profile 下并继承用户专属 ACL。Unix 仍按真实模式位判定并失败关闭（§9 判据 12 的合成视图单测在两平台都跑）。
- `[决定]` 启动 `PRAGMA quick_check`；失败进入只读失败关闭（写路径返回 `PortError::Corrupt`）。
- `[决定]` WAL 检查点与备份：关闭时执行一次 `wal_checkpoint(TRUNCATE)`；「直接拷贝主库文件而不带 `-wal`/`-shm` 不安全」的运维提醒落在 `CONFIG_REFERENCE.md` §5 的 storage 段，`SECURITY_DESIGN.md` §13.2 用一行链接指向它（两份文档各自只保留一处正文）。

### 7.2 Migration

- `[决定]` `PRAGMA user_version` = 文件格式版本（上一版为 v14 = 14；当前 **v16 = 16**）；`meta` 只保存 `owned_schema_version`（当前 16）。两个常量是 `crates/storage-sqlite/src/migrate.rs` 的 `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`；Access 不再有 imported schema 版本键。
- `[决定]` 两族 migration 分开维护；单事务、可重复执行、失败整体回滚；文件格式版本**或**任一表结构版本高于本二进制已知版本 → 拒绝启动，不降级写入。
- `[决定]` **升级判据**：库内已有 schema（`owned_session` 存在）且 `user_version < 16` 时，在同一 `BEGIN IMMEDIATE` 事务内按版本执行对应的升级段（v1 库依次走 v2…v16 段，…，v13 库走 v14/v15/v16 三段，v14/v15 库只走 v15/v16 或 v16 段）；`user_version = 16` 的库**跳过**全部升级步骤，因此第二次打开不重写 `sqlite_master`、不写任何行（§9.1）；空目录新建的库直接由 §7.3/§7.4 的 DDL（三态 `owned_session`（含 `custom_title`）、`owned_interaction` 的 `confirmed_at`/`turn_id` 与 v12 的五张表）建成当前形状（此时 `user_version` 是 0，不能只按版本号判断）。每段各自带 `file_version < N` 的守卫，因此「文件格式已比该段新、但仍低于最新版」的库不会重跑那一段。
- `[决定]` v1 → v2 升级步骤（顺序固定，都在同一事务内）：
  1. 执行 §7.3/§7.4 的 DDL 常量：`CREATE ... IF NOT EXISTS` 建出新增的管理表与 `imported_import_export`，既有表不动；
  2. `owned_audit` 与 `imported_audit` 走 **12-step 表重建**（SQLite 不能修改既有 CHECK）：新建带完整 `action` CHECK 的表 → 按列拷贝**全部行（含 `audit_id`）** → `DROP` 旧表 → `RENAME` → 重建索引；之后按升级前的 `sqlite_sequence` 回填序列，**AUTOINCREMENT 不得回退**（审计有 365 天 TTL，尾部行被清理后 `seq` 会领先于 `max(audit_id)`）；
  3. `imported_import` 重建：去掉 `export_id` 与 `UNIQUE (owner_node_id, export_id)`，新增 `grants_json`；原行的 `(owner_node_id, export_id)` 与 `created_at` 迁为一条 `imported_import_export` 关联行（`added_at` = 原 `created_at`）。旧行**没有可信的 grants 来源**，因此 `grants_json` 一律写 `'[]'`——**不得凭空补齐或默认放权**，这类 Import 保持不可用，等本地重新授权；
  4. 本段**不单独落盘版本**：`meta.owned_schema_version` 与 `PRAGMA user_version` 都由同一事务内紧随其后的升级段在**全部**段结束后统一按当前常量写入（当前为 `'16'` 与 `16`，见下面几条），因此不存在「已写 v2 版本号、表仍是 v1 形状」的中间落盘。
- `[决定]` **v2 → v3 升级步骤**（与 v2 段在同一事务内、顺序在后）：两张审计表同样走 12-step 表重建，**列集合与列顺序逐字不变**，只扩宽 CHECK——`actor_kind` 增 `'pairing_claimant'`（配对认领方的审计归因）、`action` 增 `'node.authenticated'`/`'node.auth_failed'`（节点握手留痕）；之后按本次升级前的 `sqlite_sequence` 回填序列（两段重建各自 `DROP` 过审计表，因此序列只在**全部**重建结束后回填一次）；版本键与 `PRAGMA user_version` 的落盘不在本段，见下一条。`owned_command` **不重建**：认领方永不提交命令，它的 `actor_kind` CHECK 保持 `('device','node','cli')`（design D12）——因此 v3 库上两张审计表接受四值、`owned_command` 只接受三值。
- `[决定]` **v3 → v4 升级步骤**（与 v2/v3 段在同一事务内、顺序在后）：唯一差异是给 §7.3 的 `owned_node` 在**列清单末尾**追加一列 `export_ids_json TEXT NOT NULL DEFAULT '[]'`，用 `ALTER TABLE owned_node ADD COLUMN` 实现——**不**走 12-step 表重建（重建只用于改既有 CHECK，见 §11.8），`NOT NULL` 由默认值满足，因此既有行得到 `'[]'`（**不得默认放权**，§11.8）；`ALTER TABLE ADD COLUMN` 同样把新列追加在列清单末尾，所以升级库与新建库的 `pragma table_info` 列顺序逐项相等（§9 判据 28）。版本键与 `PRAGMA user_version` 的落盘不在本段，见下一条。
- `[决定]` **v4 → v5 升级步骤**（与 v2/v3/v4 段在同一事务内、顺序在后）：唯一差异是给 §7.3 的 `owned_session` 在**列清单末尾**追加两列 `agent_session_id TEXT` 与 `workspace_cwd TEXT`，用 `ALTER TABLE owned_session ADD COLUMN` 实现——**不**走 12-step 表重建（重建只用于改既有 CHECK）；两列都可空、**无默认值**，因此既有会话行得到 `NULL`——`NULL` 的语义是「该会话没有可用于恢复的标识或目录」，`MUST NOT` 被任何读取路径补全、推导或替换为别名解析结果（`NOT NULL DEFAULT ''` 做不到这一点：空串无法与「未取得」区分，会让「可恢复」判定被默认值蒙蔽），§9 判据 28 逐行断言。`ALTER TABLE ADD COLUMN` 同样把新列追加在列清单末尾，所以升级库与新建库的 `pragma table_info` 列顺序逐项相等（§9 判据 18/28）。之后由全部升级段结束后的统一落盘把 `meta.owned_schema_version` 置为 `'13'`、`PRAGMA user_version` 置为 `13`，`meta.imported_schema_version` 置为 `'5'`（见 v12 → v13 段）。
- `[决定]` **v5 → v6 升级步骤**（与 v2/v3/v4/v5 段在同一事务内、顺序在后）：唯一差异是给 §7.3 的 `owned_session` 在**列清单末尾**追加一列 `workspace_alias TEXT`（会话的目录归属），用 `ALTER TABLE owned_session ADD COLUMN` 实现——**不**走 12-step 表重建（重建只用于改既有 CHECK）；该列可空、**无默认值**，因此既有会话行得到 `NULL`——`NULL` 的语义是「该会话没有目录归属（未分组）」，`MUST NOT` 被任何读取路径按 `workspace_cwd` 反查 `owned_workspace` 补齐，也 `MUST NOT` 用空串或占位别名代替（`NOT NULL DEFAULT ''` 做不到这一点：空串无法与「未分组」区分，会让「已分组」判定被默认值蒙蔽，与 v5 两列同一理由），§9 判据 28 逐行断言。写入侧与 v5 两列同属会话创建的那一次窄写提交（`CASE WHEN ?x IS NULL THEN col ELSE ?x END`，`None` = 不改该列），因此恢复流程既不读也不改它。`ALTER TABLE ADD COLUMN` 同样把新列追加在列清单末尾，所以升级库与新建库的 `pragma table_info` 列顺序逐项相等（§9 判据 18/28）。之后由全部升级段结束后的统一落盘把 `meta.owned_schema_version` 置为 `'13'`、`PRAGMA user_version` 置为 `13`，`meta.imported_schema_version` 置为 `'5'`（见 v12 → v13 段）。
- `[决定]` **v6 → v7 升级步骤**（与 v2/v3/v4/v5/v6 段在同一事务内、顺序在后）：唯一差异是 `owned_event.origin_kind` 的 CHECK 从四值扩到五值（新增 `'node'`，§3.4/§10.1），列集合、列顺序、其余 CHECK 与索引**逐字不变**。SQLite 不能修改既有 CHECK，因此走 **12-step 表重建**（新建 `owned_event` 新形状 → 按列拷贝**全部行（含 `global_sequence`）** → `DROP` 旧表 → `RENAME` → 重建四个索引）。**子表必须一起挪开**：`owned_interaction.request_event` 是 `owned_event` 的子外键且是 `NO ACTION`，`foreign_keys = ON` 时 `DROP TABLE` 的隐式 DELETE 会直接违反它（实测 `FOREIGN KEY constraint failed`），而官方流程要求的 `PRAGMA foreign_keys = OFF` 在事务内是 no-op、与本段「单事务」的要求冲突；因此照 v1 → v2 段处理 `imported_import` 的同款手法，先把 `owned_interaction` 的行搬进一张**无外键的过渡表**并 `DROP` 该子表（它是叶子，删除永远安全），父表重建后按 §7.3 的**规范 DDL** 把它建回并回填行——过渡表不进 `sqlite_manifest` 的最终形状，升级库与新建库的 `owned_interaction` 逐列（名称/顺序/类型/`NOT NULL`/默认值）相等。`owned_event` 的 `AUTOINCREMENT` 序列（`sqlite_sequence`）与两张审计表一起在**全部**重建之前取一次、之后回填一次（`global_sequence` 是跨节点排序与 cursor 的基准，`MUST NOT` 因保留期清理过尾部行而回退）。行级保留要求见 §9 判据 28。
- `[决定]` **v7 → v8 升级步骤**（与 v2/v3/v4/v5/v6/v7 段在同一事务内、顺序在后）：唯一差异是从 §7.3 的 `owned_device` **删除一列** `scopes_json`（设备不再有授权集合，§3.5、§6 第 5 条），用 `ALTER TABLE owned_device DROP COLUMN` 实现——**不**走 12-step 表重建（重建只用于改既有 CHECK，见 §11.8）：该列既不在任何 CHECK/索引/外键/生成列里，也没有表引用 `owned_device`。本段**只作用于升级前就存在**的 `owned_device`（v1 库没有这张表，它由 §7.3 的 DDL 常量按**当前形状**直接建出、已不含该列，再 `ALTER TABLE ... DROP COLUMN` 会因缺列失败，因此与 v4 段同款加 `owned_device` 存在性守卫）；删列**不动** `owned_audit`/`imported_audit` 的 `action` CHECK——`'device.scopes_changed'` 作为**历史取值**保留（升级前的库可能已有这类审计行，收缩 CHECK 会让历史行写不回去、读不出来），只是本二进制不再产生它（§11.6 第 1 条）。删列后升级库与新建库的 `owned_device` 列清单（列名/顺序/类型/`NOT NULL`/默认值）逐项相等（§9 判据 28）。之后由全部升级段结束后的统一落盘把 `meta.owned_schema_version` 置为 `'13'`、`PRAGMA user_version` 置为 `13`，`meta.imported_schema_version` 置为 `'5'`（见 v12 → v13 段）。
- `[决定]` **v11 → v12 升级步骤**（与 v2…v11 段在同一事务内、顺序在后）：五张新表由 §7.3 的 `CREATE ... IF NOT EXISTS` 建出；既有 `owned_interaction` 追加 `request_version INTEGER NOT NULL DEFAULT 1` 与 `invalidated_at TEXT` 两列（`ALTER TABLE`，先探测 `column_exists`，因此新建库与升级库同形）。本段只追加列，不改既有行；旧 pending 请求得到版本 1。
- `[决定]` **v12 → v13 升级步骤**（与 v2…v12 段在同一事务内、顺序在后；`session-activity` 变更）：
  1. `owned_interaction` 追加两列（先 `column_exists` 探测，新建库与升级库同形）：`confirmed_at TEXT`（可空；`NULL` = 仍计入有效待处理）与 `turn_id TEXT REFERENCES owned_turn(turn_id)`（可空）。`turn_id` 只按**创建该交互的事件**回填——`turn_id = (SELECT e.turn_id FROM owned_event e WHERE e.global_sequence = owned_interaction.request_event)`，`request_event` 为空的旧行保持 `NULL`；**不得**按时间相邻或会话内最近 turn 猜测归属。旧 `state='resolved'` 行在旧模型里已经退出待处理集合，因此回填 `confirmed_at = resolved_at`，不制造一批「已仲裁未确认」。
  2. `owned_session` 走 **12-step 表重建**（SQLite 不能修改既有 CHECK）：建三态新表（`state IN ('idle','running','pending')`，**无 `closed_at`**）→ 拷贝全部行并按旧七态写迁移暂存值（idle→idle、queued/running→running、waiting_input/waiting_permission→pending、failed/closed→idle）→ `DROP` 旧表 → `RENAME` 为规范名。子表全部以 `ON DELETE CASCADE` 引用 `owned_session`，因此 `migrate()` 在 `BEGIN` **之前**关闭 `PRAGMA foreign_keys`（事务内该 pragma 是 no-op）、迁移结束后恢复；否则 `DROP TABLE` 会级联删除子表行。重建保留列、行主键、`version`、时间列与恢复列；索引由规范 DDL 重建。
     - 映射值是**暂存值**，不是权威摘要：迁移不发布摘要、不广播事件；开放业务接入前的启动恢复用 `SessionActivity::reduce` 按有效交互与未结束 turn 重算（§6 第 16 条），旧标签不得覆盖重算结果，也不得据此伪造任务结果。
  3. 归一化 `imported_session.state` 的旧七态（imported 表没有 CHECK，因此只改值、不重建、不加正文列）：`queued`/`running` → `running`、`waiting_input`/`waiting_permission` → `pending`、`failed`/`closed` → `idle`，只作用于这七个旧取值。
  4. 版本只在三件事全部成功后统一落盘（与其它段同一事务）：`meta.owned_schema_version = '13'`、`meta.imported_schema_version = '5'`、`PRAGMA user_version = 13`。失败整体回滚，重开可重试；`sqlite_sequence`、既有索引、外键与事件/审计字节保持不变（§9 判据 37）。
- `[决定]` **v13 → v14 升级步骤**（`access-owner-data-stateless`；与既有段同一事务、顺序最后）：按外键顺序 `DROP TABLE IF EXISTS imported_delivery_index` → `imported_command_ref` → `imported_session` → `imported_audit` → `imported_import_export` → `imported_import`；删除前按明确来源谓词清理 `owned_command` 与 `owned_audit` 中可辨认的远程代理记录（只命中属于旧 imported 会话、且不属于 `owned_session` 的记录），归属无法安全判定时整段失败且不写入版本；删除 `meta.imported_schema_version`；最终写入 `meta.owned_schema_version = '14'` 与 `PRAGMA user_version = 14`。owned 会话/事件/序列/epoch/幂等/信任与既有 owned 审计不删除。
- `[决定]` **v14 → v15 升级步骤**（`custom-session-title`；与既有段同一事务、顺序最后）：唯一差异是给 §7.3 的 `owned_session` 在**列清单末尾**追加一列 `custom_title TEXT`（用户设置的自定义标题），用 `ALTER TABLE owned_session ADD COLUMN` 实现——**不**走 12-step 表重建；该列可空、**无默认值**，因此既有会话行得到 `NULL`（= 从未设置自定义标题），Agent 标题列 `title` 与事件/epoch/幂等/审计一律不动。`column_exists` 守卫让重开/重跑幂等（已加过列不再 `ALTER`）；`ALTER TABLE ADD COLUMN` 把新列追加在列清单末尾，新建库的 DDL 也把它写在末尾，因此升级库与新建库的 `pragma table_info` 列顺序逐项相等（§9 判据 28）。最终写入 `meta.owned_schema_version = '15'` 与 `PRAGMA user_version = 15`；失败整体回滚到 v14 形状。
- `[决定]` **v15 → v16 升级步骤**（`web-session-preparation` WP2；与既有段同一事务、顺序最后）：给 `owned_message.channel` 的 CHECK 增加 `'user'`（首条用户消息，不是 Agent 消息），因 SQLite 不能修改既有 CHECK 而走 **12-step 表重建**（拷贝全部行与正文、重建索引）；`owned_first_submission` 由 `CREATE TABLE IF NOT EXISTS` 建出。**不回填历史会话**、不扫描空正文改状态；旧会话没有本表行，也永不被推断成准备。失败整体回滚，第二次打开因 `user_version = 16` 跳过本段。

- `[决定]` **目录归属的读取投影**（`design.md` D1/D6）：`SessionSummary.workspace` 取自 `owned_session.workspace_alias`，展示名由 `storage-sqlite` 在同一条读查询里 `LEFT JOIN owned_workspace ON w.alias = s.workspace_alias` 取得；**关联不上时回退为别名本身**（目录被删除不丢分组，也不让规范化路径进入摘要——路径只出现在快照的 `workspaces` 条目上，§11.1）。该 JOIN 留在 `storage-sqlite` 内部（它同时拥有两张表），**不上浮到 core**。实现 `MUST NOT` 存在任何「按 `owned_session.workspace_cwd` 反查别名」的查询——`NULL` 就是未分组，这是「别名重指向后既有会话归属不漂移」的前提。
- `[决定]` 保留不变量：`server_epoch`、会话 `origin_epoch` 与事件 `global_sequence`/`session_sequence`、`requestId` 与幂等行、命令终态、全部既有审计都逐行保留，**不得重新编号**。管理表初始为空；profile 种子与「已初始化」标记在同一事务里提交（`LocalConfigStore::mark_seeded`，§5.3），不从聊天或审计内容推断信任。
- `[决定]` 管理表纳入 §7.5 的容量度量（TEXT 列 + 附件字节）与清理顺序；撤销 tombstone 不因容量压力被删除，空间不足时拒绝新写入而不是删活动信任或未到期审计。
- `[决定]` 迁移测试资产：`fixtures/storage/v2/` 三件套在 v3 之后是**冻结的历史升级输入**——`empty.sqlite3`（v2 形状的空库，v2 → … → v12 连续升级用例的输入）、`from-v1.sqlite3`（含会话/事件/cursor/幂等/审计数据的 v1 库，`owned_audit` 故意留下 `audit_id = 1,2,5` 的空洞以覆盖「序列领先于 `max(audit_id)`」；v1 → … → v12 连续升级用例的输入）与 `too-new.sqlite3`（`user_version = 3`，v3 之后不再「过新」，保留为历史资产）；`fixtures/storage/v1/` 的两个文件是更早的历史资产。当前二进制不再能生成 v2 形状的空库，因此 `from-v1.sqlite3` 的生成器（`crates/storage-sqlite/tests/migration.rs` 的 `regenerate_v1_fixture`，默认 `#[ignore]`）只重建它；v3、v4 与 v5 形状的库同样不由夹具提供，而由用例现场造（新建库写行 → `DROP` 掉换代才有的列、并把换代才删的列按旧声明补回 → 降版本键），`v3_database_upgrades_to_v6_by_appending_the_export_id_and_recovery_columns_only`、`v4_database_upgrades_to_v6_by_appending_the_recovery_columns_only` 与 `v5_database_upgrades_to_v6_by_appending_a_null_workspace_alias_column`（`tests/workspace_alias.rs`）分别覆盖三段；「版本过新拒绝启动」用例改在临时副本上把 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`；「已是最新版则不重写」用例改在空目录新建的当前版本库上判定。
- `[决定]` 回滚：v13 库不能被 v12 或更早的二进制打开（版本过新拒绝启动），因此回滚 = 恢复升级前的数据库备份 + 回退二进制；本合同**不提供**自动降级迁移（需要把库先降回 v12 形状的降级工具必须单独授权）。

### 7.3 `owned_*` 表

```sql

CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_session (
  session_id        TEXT PRIMARY KEY,
  title             TEXT,
  agent_id          TEXT NOT NULL,
  agent_name        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('idle','running','pending')),
  origin_epoch      TEXT NOT NULL,
  current_mode_id   TEXT,
  current_mode_name TEXT,
  version           INTEGER NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  agent_session_id  TEXT,                      -- Agent（ACP）侧会话标识；NULL = 该会话没有可用于恢复的标识，不得被补齐/推导
  workspace_cwd     TEXT,                      -- 创建时解析出的规范化绝对路径；NULL = 同上（不得按别名重解析来填上）
  workspace_directory_id TEXT,                 -- 创建时解析出的目录标识；NULL = 未分组（不得按 workspace_cwd 反查补齐）
  custom_title      TEXT                       -- 用户设置的自定义标题（trim 后 1..=512 标量值）；NULL = 从未设置。与 title 互不覆盖
) STRICT;

CREATE TABLE IF NOT EXISTS owned_turn (
  turn_id     TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  state       TEXT NOT NULL CHECK (state IN ('queued','running','waiting_input','waiting_permission','completed','failed','cancelled')),
  queue_index INTEGER NOT NULL,
  causation   TEXT,
  started_at  TEXT,
  ended_at    TEXT,
  UNIQUE (session_id, queue_index)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_event (
  global_sequence  INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id       TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
  session_sequence INTEGER,
  origin_epoch     TEXT,
  origin_sequence  INTEGER,
  event_id         TEXT NOT NULL UNIQUE,
  turn_id          TEXT REFERENCES owned_turn(turn_id),
  event_type       TEXT NOT NULL,
  kind             TEXT NOT NULL CHECK (kind IN ('state','delta','final_message','structured','interaction','summary')),
  policy           TEXT NOT NULL CHECK (policy IN ('durable','short_term')),
  origin_kind      TEXT NOT NULL CHECK (origin_kind IN ('agent','device','daemon','local_cli','node')),
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
  CHECK ((session_id IS NULL) = (origin_epoch IS NULL)),
  CHECK ((session_id IS NULL) = (origin_sequence IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NOT NULL
         OR (acp_raw_json IS NULL) = (acp_sha256 IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NULL OR acp_raw_json IS NULL),
  CHECK (acp_raw_json IS NOT NULL OR acp_raw_unavailable_reason IS NOT NULL OR acp_sha256 IS NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_session ON owned_event(session_id, session_sequence);
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_origin ON owned_event(session_id, origin_epoch, origin_sequence) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_expiry ON owned_event(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_turn ON owned_event(session_id, turn_id, kind);

CREATE TABLE IF NOT EXISTS owned_command (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_kind    TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id      TEXT NOT NULL,
  request_id    TEXT NOT NULL,
  session_id    TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
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
  UNIQUE (actor_kind, actor_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  CHECK (status <> 'rejected' OR result_json IS NULL),
  CHECK (status <> 'completed' OR error_code IS NULL),
  CHECK (status IN ('accepted','completed') OR error_code IS NOT NULL),
  CHECK (kind <> 'query' OR terminal_event_id IS NULL),
  CHECK (status <> 'completed' OR kind <> 'mutation' OR terminal_event_id IS NOT NULL),
  CHECK (status NOT IN ('failed','uncertain') OR terminal_event_id IS NOT NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_command_status ON owned_command(status) WHERE status IN ('uncertain','accepted');
CREATE INDEX IF NOT EXISTS owned_command_session ON owned_command(session_id, accepted_at);

CREATE TABLE IF NOT EXISTS owned_interaction (
  interaction_id  TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('permission','elicitation')),
  request_event   INTEGER REFERENCES owned_event(global_sequence),
  state           TEXT NOT NULL CHECK (state IN ('pending','resolved','expired')),
  created_at      TEXT NOT NULL,
  resolved_at     TEXT,
  decision_option_id TEXT,
  decision_kind   TEXT CHECK (decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
  elicitation_action TEXT CHECK (elicitation_action IN ('submit','decline','cancel')),
  elicitation_values_json TEXT,
  resolved_by_kind TEXT, resolved_by_id TEXT,
  request_version INTEGER NOT NULL DEFAULT 1,
  invalidated_at  TEXT,
  confirmed_at    TEXT,                        -- 端点确认处理结果的时刻；NULL = 仍计入有效待处理
  turn_id         TEXT REFERENCES owned_turn(turn_id)  -- 所属 turn（由创建事件回填）；NULL = 历史行无法归属
) STRICT;
CREATE INDEX IF NOT EXISTS owned_interaction_pending ON owned_interaction(session_id, state) WHERE state = 'pending';

CREATE TABLE IF NOT EXISTS owned_audit (
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
CREATE INDEX IF NOT EXISTS owned_audit_at ON owned_audit(at);
CREATE INDEX IF NOT EXISTS owned_audit_action ON owned_audit(action, at);

CREATE TABLE IF NOT EXISTS owned_attachment (
  attachment_id TEXT NOT NULL UNIQUE,
  sha256      TEXT PRIMARY KEY,
  byte_length INTEGER NOT NULL,
  media_type  TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  last_used_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_link (
  session_id   TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  attachment_id TEXT NOT NULL REFERENCES owned_attachment(attachment_id),
  generation   INTEGER NOT NULL,
  sha256       TEXT NOT NULL REFERENCES owned_attachment(sha256),
  PRIMARY KEY (session_id, attachment_id)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_transfer (
  transfer_id       TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  subject_kind      TEXT NOT NULL CHECK (subject_kind IN ('device','node')),
  subject_id        TEXT NOT NULL,
  total_length      INTEGER NOT NULL,
  received_length   INTEGER NOT NULL DEFAULT 0,
  chunk_size        INTEGER NOT NULL CHECK (chunk_size = 32768),
  state             TEXT NOT NULL CHECK (state IN ('receiving','sealed','aborted','expired')),
  media_type        TEXT NOT NULL,
  file_name         TEXT NOT NULL,
  sealed_sha256     TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK ((state = 'sealed') = (sealed_sha256 IS NOT NULL))
) STRICT;
CREATE INDEX IF NOT EXISTS owned_attachment_transfer_stale
  ON owned_attachment_transfer(updated_at) WHERE state = 'receiving';

CREATE TABLE IF NOT EXISTS owned_attachment_submission (
  submission_id     TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  subject_kind      TEXT NOT NULL CHECK (subject_kind IN ('device','node')),
  subject_id        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('open','sealed')),
  attachment_count  INTEGER NOT NULL,
  manifest_digest   TEXT,
  created_at        TEXT NOT NULL,
  sealed_at         TEXT,
  CHECK ((state = 'sealed') = (manifest_digest IS NOT NULL)),
  CHECK ((state = 'sealed') = (sealed_at IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_submission_item (
  submission_id     TEXT NOT NULL REFERENCES owned_attachment_submission(submission_id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  attachment_id     TEXT NOT NULL,
  PRIMARY KEY (submission_id, position)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_message (
  message_id        TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  turn_id           TEXT,
  channel           TEXT NOT NULL CHECK (channel IN ('reply','thought','user')), -- 'user' = 首条用户消息（owned_first_submission）；不是 Agent 消息
  state             TEXT NOT NULL CHECK (state IN ('partial','completed','failed','cancelled')),
  content_json      TEXT NOT NULL,
  partial_reason    TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK (state <> 'completed' OR partial_reason IS NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_message_session ON owned_message(session_id, created_at, message_id);

CREATE TABLE IF NOT EXISTS owned_media_ref (
  message_id        TEXT NOT NULL REFERENCES owned_message(message_id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  attachment_id     TEXT NOT NULL,
  kind              TEXT NOT NULL CHECK (kind IN ('image','audio','file')),
  PRIMARY KEY (message_id, position)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_media_ref_attachment ON owned_media_ref(attachment_id);

CREATE TABLE IF NOT EXISTS owned_first_submission (
  session_id          TEXT PRIMARY KEY REFERENCES owned_session(session_id) ON DELETE RESTRICT,
  first_submission_id TEXT NOT NULL UNIQUE,
  user_message_id     TEXT NOT NULL UNIQUE,
  first_request_id    TEXT NOT NULL,
  current_request_id  TEXT NOT NULL,
  actor_kind          TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id            TEXT NOT NULL,
  request_fingerprint TEXT NOT NULL,
  input_json          TEXT NOT NULL,
  input_digest        TEXT NOT NULL,
  mode_id             TEXT,
  config_json         TEXT NOT NULL,
  stage               TEXT NOT NULL CHECK (stage IN ('saved_not_dispatched','dispatch_started','terminal')),
  ever_dispatched     INTEGER NOT NULL DEFAULT 0 CHECK (ever_dispatched IN (0,1)),
  agent_session_id    TEXT,
  resource_epoch      TEXT,
  terminal_outcome    TEXT CHECK (terminal_outcome IN ('completed','failed','uncertain')),
  terminal_at         TEXT,
  error_code          TEXT, error_message TEXT, error_details_json TEXT, retryable INTEGER,
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  CHECK (stage = 'saved_not_dispatched' OR agent_session_id IS NOT NULL),
  CHECK (stage <> 'terminal' OR terminal_outcome IS NOT NULL),
  CHECK (ever_dispatched = 0 OR stage <> 'saved_not_dispatched'),
  CHECK (stage <> 'dispatch_started' OR ever_dispatched = 1),
  CHECK (terminal_outcome IS NULL OR terminal_outcome = 'completed' OR error_code IS NOT NULL)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_device (
  device_id     TEXT PRIMARY KEY,
  display_name  TEXT NOT NULL,
  public_key    BLOB NOT NULL,                 -- 65 字节 SEC1 未压缩 P-256
  fingerprint   TEXT NOT NULL,                 -- 64 字符小写 hex = SHA-256(public_key)
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

CREATE TABLE IF NOT EXISTS owned_node (
  node_id        TEXT NOT NULL,
  kind           TEXT NOT NULL CHECK (kind IN ('access','owner')),
  display_name   TEXT NOT NULL,
  fingerprint    TEXT NOT NULL,
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
CREATE INDEX IF NOT EXISTS owned_node_role ON owned_node(kind, state);

-- Node 双角色共享一条身份材料：主键不含 kind。
CREATE TABLE IF NOT EXISTS owned_peer_key (
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
CREATE TABLE IF NOT EXISTS owned_pairing (
  pairing_id          TEXT PRIMARY KEY,
  target_kind         TEXT NOT NULL CHECK (target_kind IN ('device','node')),
  state               TEXT NOT NULL CHECK (state IN ('created','claimed','pending_confirmation','approved','rejected','expired','consumed')),
  display_name        TEXT,
  secret_digest       TEXT NOT NULL,
  host_binding        TEXT NOT NULL,           -- 设备：canonical origin；节点：owner endpoint
  created_at          TEXT NOT NULL,
  expires_at          TEXT NOT NULL,
  claimed_at          TEXT,
  approved_at         TEXT,
  terminal_at         TEXT,
  CHECK ((state = 'created') = (claimed_at IS NULL)),
  CHECK ((state IN ('approved','consumed')) = (approved_at IS NOT NULL)),
  CHECK ((state IN ('rejected','expired','consumed')) = (terminal_at IS NOT NULL))
) STRICT;

-- 每个配对最多一个 peer；claim 之后不能换人（配对行条件更新与唯一主键共同保证）。
CREATE TABLE IF NOT EXISTS owned_pairing_peer (
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

-- 路径不进入 Node Link catalog（§11.1）：它只随 Sync 快照的 `workspaces` 条目下发（已记录目录的
-- 规范化路径），命令行日志、审计记录与错误 details 仍不含路径。
CREATE TABLE IF NOT EXISTS owned_directory (
  directory_id   TEXT PRIMARY KEY,
  canonical_path TEXT NOT NULL UNIQUE,
  created_at     TEXT NOT NULL,
  last_used_at   TEXT NOT NULL
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
- `[决定]`（`pwa-conversation-ui` WP2）**会话历史读复用本节的 `owned_event_session` 唯一索引，本次不改 §7**：按 `session_sequence` 取一页条目走的就是这个索引（`session_id = ? AND session_sequence < ?` + 反向扫描即可），因此不需要新增索引、不需要 migration，本节 DDL 与本轮之前的字节完全相同。查询形状与「一次取行连正文」的要求见 §5.2 的对应 `[决定]`。
- `[决定]` `payload_digest` 必须等于对 `payload_json` 施 **ACPR-CJ1**（`SYNC_PROTOCOL.md` §3.3）后的 SHA-256（§9 判据 9）。**前像不是库里那串字节**：`payload_json` 按 §9 判据 16 原样保留调用方字节（不重排键），因此摘要必须由写入方**先规范化再哈希**；唯一写入者是存储层，`payload_digest` 由存储层用共享实现 `acpr-wire` 的 ACPR-CJ1 从 `payload_json` 重算，**不接受调用方提供**（否则一个非法摘要会被落库，跨节点复算与 imported 去重都会失配）。`acpr-wire` 的实现必须与 `scripts/check-contract-assets.mjs` 的参考实现在同一批 fixture 上逐值一致。

### 7.4 远程资源表（v14 起不存在）

v14 起不再有 `imported_*` 表族（`imported_session`/`imported_delivery_index`/`imported_command_ref`/`imported_audit`）。Access 只保存连接与信任资料及本节点 owned 资源；远程目录、摘要、正文、游标、ACK、命令状态与请求引用只在已认证连接期存在。迁移在开始监听前删除旧表并清理可辨认的远程代理审计/命令记录（§7.2 的 v14 段与 §11.10），任何新表、替代表或持久视图都不得承载上述数据。

### 7.5 保留、清理与容量

| 配置键 | 默认 | 落到 schema 的行为 |
|---|---:|---|
| `storage.transcript_retention_days` | 90 | 清理 `kind ∈ {final_message, structured, summary}` 的 `owned_event`；`kind='state'` 一并纳入该窗口（`AGENTS.md` §7 的「关键结构化事件与状态变化」以 90 天为默认承诺期）；`owned_interaction` 的终态行同期清理 |
| `storage.sync_event_retention_days` | 7 | 清理 `kind='delta'` 的 `owned_event`；cursor 低于窗口下界 → `sync.reset_required`（`reason = cursor_expired`） |
| `storage.max_total_size_bytes` | 2 GiB | 全库上限；度量 = `owned_*` 家族所有 `TEXT` 列的 `length()` 之和 + `owned_attachment.byte_length` 之和 + **`imported_*` 家族所有 `TEXT` 列的 `length()` 之和**（v1 用字符数而非字节数，作为有界增长的代理指标；同一口径同时用于提交前的容量检查与两侧 `prune`）。Access-only 节点上 `owned_*` 为空，因此没有 imported 分量时度量恒为 0、`prune` 永不回收 |
| `storage.max_session_size_bytes` | 100 MiB | 单会话上限；度量同上并按 `session_id` 归集 |
| `storage.persist_deltas` | false | `false` 时允许 turn 完成后把 delta 压缩为 `summary`（写 `compacted_into`），执行中不得压缩 |
| `storage.flush_interval_ms` | 250 | delta 合并提交窗口（§6 第 10 条） |
| `storage.attachment_dir` | `<data_dir>/attachments` | 附件字节目录（`[决定]` 新增键，权限同数据库目录） |
| `storage.attachment_max_file_bytes` | 20 MiB | 单附件上限，超出显式拒绝 |
| `storage.attachment_max_total_bytes` | 1 GiB | 附件总量，超出按 `last_used_at` LRU 清理 |
| `storage.audit_retention_days` | 365 | 审计保留期（已确认）；到期后按 `at` 清理 `owned_audit`。容量压力下审计排在清理顺序**最后**（先清 delta → 正文/状态事件 → 已压缩批次 → 附件，再考虑审计），超限时宁可拒绝新写入也不静默丢证据 |

`[决定]` 清理顺序：① 过期 delta → ② 过期正文/结构化/状态事件 → ②′ **终态交互行**（`owned_interaction.state <> 'pending'`，含已失效的 `expired` 行）→ ③ 该会话最旧的已压缩 delta 批次 → ④″ **闲置清单与传输**（`open`/`sealed` 超过 24 小时的清单、`receiving` 超过 24 小时的传输）→ ④ 附件 LRU → ④′ 按引用 GC（无会话绑定/媒体引用/活跃清单且 `last_used_at` 早于 24 小时）→ ⑤ 到期的审计（`storage.audit_retention_days`，365 天；`owned_audit` 到期清理）→ ⑥ 仍超限则拒绝新写入（`PortError::Unavailable(StorageFull)`），不静默丢弃、不继续广播。TTL 清理（①②②′④″⑤）由 `prune` 驱动；容量清理（②′③④）只在超限时触发。④″ 必须排在 ④/④′ 之前，否则已过期清单的条目会在本轮继续把附件挡在 GC 根之外。

`[决定]` 消息保留期与附件的关联清理：`prune` 删除过期消息后，删除「不再被任何 `owned_media_ref` 引用、且附件的**最近一次使用**（`owned_attachment.last_used_at`）已早于正文保留窗口」的 `owned_attachment_link` 行。判定不按 `created_at`：内容寻址去重让旧字节复用同一个附件行并在行上保留**首次**上传的 `created_at`，按它判定会把「今天重发 90 天前的同一文件」刚建立的链接在下一轮清理里删掉（随后 `read_chunk` 失去授权、内容被 ④′ 回收）。因此 `bind_submission` 与 `AttachmentStore::link` 在建立链接时把该附件的 `last_used_at` 刷新为绑定时刻，保留期从最近一次绑定起算。附件因此随所属消息的保留期结束而失去会话绑定，随后由 ④′ 在闲置窗口后回收；仍被保留消息引用（或仍在保存期内）的内容不受影响。附件行被任一清理删除时，指向它的已封存传输在同一事务内置为 `expired` 并清空 `sealed_sha256`（CHECK 要求两列同有同无），不会出现「传输显示 sealed 但读不出附件」。

`[决定]`（2026-09-23）**清理任务的驱动者与周期**（此前只写了“由 `prune` 驱动”，没写谁调、多久调一次，等于默认永不执行）：

1. 组合根在取得单实例锁、migration 完成**之后**、开始监听**之前**先执行一次初清理：`SessionStore::prune` + `SqliteStore::run_startup_gate`（v14 远程数据清理与 WAL/自由页收尾），加上配对内存态的重启恢复（`Authority::reset_memory` → `UseCases::pairings` → `Authority::unrecoverable_after_restart` → `UseCases::expire_pairings_by_id`，见 §11.6 第 6 条）、`TrustStore::expire_pairings`（§11.6）与 `AttachmentStore::sweep_orphans(启动时刻, 1000)`（§6 第 18 条）。
2. 启动之后按**固定 60 s**周期重复同一批调用（固定 v1 常量，不给配置键：这是保证保留策略真的生效，不是可调业务参数）。
3. 周期任务与写入共用同一个写连接（§7.1 的单写连接），必须**分批并让出**：单次 `prune` 到上限即返回，不得抦住写事务阻塞会话提交；一轮超时或出错只记结构化日志，不中断 daemon。
4. 关闭顺序（以 `SECURITY_DESIGN.md` §12.1 为准）：**停接入层**（不再接受新连接，并排空**其余**在途连接，最坏仍受宽限上限约束；发出该 `daemon.stop` 的那条连接不计入——服务端在写回响应后自行结束它）→ **取消周期任务与信号监听**（必须先于停止 Agent）→ 停 Agent 进程 → 最后做一次 `wal_checkpoint(TRUNCATE)` 并清理 endpoint/释放单实例锁。周期任务的取消位置是为了让它们在存储关闭前停止写入，不是「先于接入层」。
5. `MODULE_ARCHITECTURE.md` §4.10 的后台任务清单必须与本节一致（prune / expire_pairings / sweep_orphans / 心跳与重连）。

`[决定]` **②/③ 的谓词必须排除仍被引用的事件行**：`owned_interaction.request_event` 是 `INTEGER REFERENCES owned_event(global_sequence)` 且 `foreign_keys = ON`，所以 ② 必须带 `AND NOT EXISTS (SELECT 1 FROM owned_interaction i WHERE i.request_event = owned_event.global_sequence)`，并且 ②′ 必须排在 ② 之前。否则超限时 `enforce_capacity` 里的 ② 会撞外键、把 `FOREIGN KEY constraint failed` 当成 `PortError::Backend` 抛给上层（而不是 §7.5⑥ 的 `StorageFull`），`prune` 的整个事务也会回滚、连 ① 的 delta 清理都做不成。

`[决定]` **幂等/终态行不参与 TTL 清理**：`owned_command` 的 mutation 行与 `status='uncertain'` 行保留到「目标会话被删除」或「actor 被撤销」，与 `SYNC_PROTOCOL.md` §11.2 的「至少保留到设备被撤销且目标会话被删除」一致。

`[决定]` 清理不得破坏承诺期内的重放：窗口前移后 `retention_window` 必须反映新下界，broker 据此要求客户端重建 snapshot。

### 7.6 临时传输、清单、消息与媒体引用的落盘语义

本节描述 §7.3 已含的五张表与两列的实际语义（DDL 见 §7.3，版本见 §7.2）；
`check-contract-drift` 已把它们与 `migrate.rs` 的 `OWNED_SCHEMA_V1` 逐条绑定。

- `owned_attachment_transfer.received_length` 与 `updated_at` 在每次接受块时条件更新；
  `state = receiving` 且最近活动不足 24 小时的传输不被清理。
- `sealed_sha256` 与 `state = sealed` 同有同无（CHECK）；封存后不可追加，临时文件先落盘并校验，再写引用。
- `owned_attachment_submission_item.position` 是清单顺序；`attachment_count`/`manifest_digest`/`sealed_at`
  与 `state = sealed` 同生同灭（CHECK）；`prompt` 受理前按 `(session_id, subject)` 复核整份清单，绑定事务内
  再复核一次主体并在成功后删除清单行（条目级联删除）。未绑定清单按 `created_at`/`sealed_at` 在 24 小时后
  过期，因此 `owned_attachment_submission_item` 只对活跃清单构成附件 GC 根。
- `owned_message` 是回复/思考/部分结果的权威正文表，`channel` 区分 `reply|thought`；
  `completed` 的 `partial_reason` 恒为 NULL；`failed`/`cancelled` 必须给出原因；`partial` 允许 NULL——它
  表示该 turn 仍在进行，turn 终态时由收尾逻辑改写为终态（未收到收尾则按 `failed` + `agent_failed` 落盘）。
  历史读取只从这里取正文，并只取 `(created_at, message_id)` 游标指定的一页。
- `owned_media_ref` 只存引用，不存媒体字节；附件的保留与回收按 `owned_media_ref`/`owned_attachment_link`/
  `owned_attachment_submission_item` 的引用判定。`owned_attachment_link` 只在附件仍处于正文保留窗口内、或
  仍被某条保留消息的媒体引用时保留；窗口按附件的 `last_used_at` 判定（建立链接时刷新为绑定时刻，见 §7.5），
  不按内容首次上传的 `created_at`——去重复用的旧内容因此不会刚绑定就被回收。清单条目只对未绑定/未过期的清单存在。
- `owned_interaction.request_version` 是当前有效请求版本（新行默认 1；本版本不递增——同一个 `pending` 请求
  只有一次创建，重发/重试沿用该值）；`invalidated_at` 记录失效时刻，失效同时把 `state` 置为 `expired`，
  所有 pending 读面按 `state = 'pending' AND invalidated_at IS NULL` 过滤，已终结请求不复活。失效行与其
  请求事件在保留期到期后可被回收（②′ 与 ② 的既有谓词）。
- `owned_interaction.confirmed_at` 是**端点确认处理结果**的时刻：仲裁只写 `resolved_at`，`confirmed_at`
  保持 `NULL`，该行仍计入有效待处理；`confirmation` 提交才写它。`turn_id` 是交互的轮次归属，取自创建
  它的交互事件（`request_event` 指向的 `owned_event.turn_id`），turn 终态按它失效本轮的未解决交互；
  `turn_id IS NULL` 的历史行不参与按 turn 的失效，但若仍有效（`pending` 或 `resolved` 未确认）仍计入
  会话摘要，由重启恢复统一失效。

## 8. 失败关闭

- `[决定]` `Corrupt` 或权限检查失败 → 写路径全部拒绝、只读查询继续、`server` 以 `internal.unavailable` 返回并保持明确日志（`SECURITY_DESIGN.md` §15）。
- `[决定]` 错误消息不得包含 SQL 文本、完整 prompt、密钥或 ACP raw；日志字段 allowlist 由 `SECURITY_DESIGN.md` §14.1 决定。
- `[决定]` 失败关闭必须有测试：用损坏的数据库文件（或注入 `integrity_ok = false`）打开存储 → 写路径全部返回 `PortError::Corrupt`、只读查询仍可用（§9 未单列判据，但 §8 的两条规则都要有可观察用例）。

## 9. 验收判据（实现该合同的测试）

1. **migration**：空目录新建的当前版本库连续两次启动后 `PRAGMA user_version`、`meta.*_schema_version`、`sqlite_master` 里每条 SQL 文本与全部表的行集**逐字节相同**（幂等）；把 `fixtures/storage/v2/empty.sqlite3`（v2 形状）的临时副本的 `user_version` 顶到 `FILE_FORMAT_VERSION + 1` → 返回具名错误且不写入任何行；`fixtures/storage/v2/from-v1.sqlite3` 的 v1 → v2 → … → 当前版本连续升级按判据 28 断言保留性。
2. **单事务提交**：用一个装饰 `SessionStore` 的测试替身统计 `commit` 调用次数，并对第二次调用注入失败；断言
(a) 每个 mutation 恰好一次**接受提交**（幂等行那一次；重试与 `Ephemeral` 过滤都不新增提交，见 §6 第 6/11 条）——一次 mutation 天然还会产生终态提交与 delta 合批提交，本条只约束接受语义不得重复；
(b) 失败后 `owned_session.version`、`owned_turn`、`owned_event`、`owned_command` 与调用前快照逐行相同；
(c) 进程被强杀（Windows 用 `TerminateProcess`、Unix 用 `SIGKILL`）后 `PRAGMA integrity_check = ok`，且**每条曾进入 `accepted` 的 mutation 行**到达终态时 `terminal_event_id` 非空并指向存在的 `owned_event` 行（原措辞「每条 `status <> 'accepted'` 的行都有终态事件」与 §7.3 自己的 CHECK 冲突——查询命令的 `terminal_event_id` 被强制为 NULL、`rejected` 也允许为空，因此不可判据）。
3. **幂等**：同一 `(actor, requestId)` 且指纹相同 → 返回首次结果、事件数不变；指纹或 `expected_version` 不同 → `command.idempotency_conflict`；不同 actor 用同一 `requestId` → 视为两条独立命令。**`expected_version` 不同这一支必须有独立用例**（`None` 与 `Some` 两个方向都要有，否则该分支等于没测）。
4. **序列**：`global_sequence` 唯一且严格递增（唯一索引即为约束）；会话内 `session_sequence` 无空洞；`origin_sequence` 与会话 `origin_epoch` 组合唯一；非会话级事件两者为 NULL 且仍出现在重放流中。
5. **imported 去重与失效**：同一 `origin_event_id` 重发只产生一行、不新增 `local_sequence`；`owner_server_epoch` 变化后该 import 的索引被清空。
6. **无正文机器检查（黄金列清单）**：逐张 `imported_*` 表用 `PRAGMA table_info` 取列名集合，与合同冻结的列名集合**逐项相等**（新增列即失败）；并做行为断言：投递一条携带 `payload.view` 的 `resource.event` 后，`local_replay()` 只返回 origin 字段/`event_type`/`payload_digest`/`local_sequence`，且库中任何列都不包含该 view 的文本。
7. **保留与窗口**：`expires_at` 到期行被清理；`kind='state'` 按 90 天窗口清理；清理后 `retention_window` 前移并由 broker 产生 `sync.reset_required`；承诺期内 delta 仍可重放。
8. **容量**：用极小配置值驱动 `max_total_size_bytes`/`max_session_size_bytes`，断言清理顺序（先 delta、再正文/状态、再附件）与被删行集合；**库中存在已过期但仍被 `owned_interaction.request_event` 引用的交互事件时**，容量清理仍必须完成并给出 `Unavailable(StorageFull)`（而不是外键错误映射的 `Backend`）；仍超限时 `commit` 返回 `Unavailable(StorageFull)` 且 `EventPublisher` 收到零次调用。
9. **摘要自洽**：对每条 `owned_event`，用 ACPR-CJ1 重新规范化 `payload_json` 后取 SHA-256，必须等于 `payload_digest`。
10. **引用完整性**：`owned_event.turn_id`、`owned_interaction.request_event`、`owned_attachment_link.sha256` 的悬空行数必须为 0（外键 + 断言）；`owned_command` 的 `(actor_kind, actor_id, request_id)` 唯一。
11. **权限仲裁与确认**：两个**并发**解析请求（单写连接会串行化它们，判据看的是结果而不是交错）恰好一个 `Resolved`；已解析后再次应答 → `AlreadyResolved` 且 `resolved_at`/`decision_option_id` 不变；对不存在的 `interactionId` → `PortError::NotFound`。胜出行的 `confirmed_at` 仍为 `NULL`，`session_activity` 仍计入有效待处理（摘要保持 `pending`）；随后 `confirmation` 提交才写 `confirmed_at` 并按归约转为 `running`/`idle`；重复确认是无操作、不产生第二次提交副作用。
12. **ACL 判定**：把平台 ACL 读取抽象成纯函数（输入为合成的权限视图），单测覆盖「组/其他可写」「非当前用户可读写」等视图 → 判定为宽松；正式模式下宽松即失败关闭。真实第二账号的端到端检查作为可选集成测试。
13. **端口纯度**：`cargo tree -p core --edges normal` 的输出与冻结 allow-list 逐行相等（黄金文件）——`core` 的直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`——其中 `p256` **只开 `arithmetic`**（core 只做曲线级点校验，不签名也不验签），因此 `ecdsa`/`rfc6979`/`hmac`/`signature`/`pkcs8`/`spki`/`pem-rfc7468` 等签名与编码栈**不在**闭包内；完整普通依赖闭包与 allow-list 逐项登记在 `scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`；`cargo public-api -p core` 快照不得出现 §3/§5 之外的类型（该半条需要 nightly toolchain + 外部 `cargo-public-api`，未安装时应记录为**未执行**并说明替代判据，不得声称已通过）。
14. **审计**：`action` 只能取 §3.5 的枚举（表级 CHECK + 用例层枚举，**每个取值都要有写入用例**，含该合同 §7.3 与 §7.4 的 `action` CHECK 里 `export.*`/`import.*`/`provider.configured` 五类，以及 v3 新增的 `node.authenticated`/`node.auth_failed`）；`actor_kind` 的取值集在两张审计表上与 `ActorKind` 逐值相等，但在 `owned_command` 上**只有三值**（无 `pairing_claimant`，design D12）；`owned_audit` 与 `imported_audit` 都要有「黄金列清单」测试（逐列 `PRAGMA table_info` 比对，新增内容列即失败）。
15. **交互创建**：一次 `commit` 写入 `interaction` 事件 + `PendingInteractionWrite` 后，`owned_interaction` 恰好一行且 `request_event` 等于**配对事件**的 `global_sequence`（列类型见 §7.3；§9 判据 10 的悬空引用为 0），`turn_id` 等于该配对事件的 `turn_id`（事件不带 turn 时为 `NULL`，不得按时间相邻猜测），`confirmed_at` 为 `NULL`；装配方传入的任何占位值都不得入库；同一提交里 `interactions` 与 `state.interaction` 同时出现 → `InvalidRequest`；会话摘要与 `session.updated`（状态变化时）和这一行在同一次提交；随后 `HistoryInclude.pending_interactions` 读回的行 `options` 为空，而按事件流取到配对事件的 `id` 后 `ReadView::event_payload(id)` 能还原出非空 `options`。
16. **正文读取**：`ReadView::event_payload` 返回的 `view` 文本与库内 `payload_json` **逐字节**相同（含未知字段与嵌套），`AcpRaw::Available.raw_json` 与写入时逐字节相同；原文被清理过的行返回 `AcpRaw::Unavailable`，且 `reason`、`byte_length`、`sha256` 三者都必须与行内列一致（**摘要不得因为原文被清理而丢失**——`acp_sha256` 与 `acp_raw_json` 只有在没有不可用原因时才同有同无）；不存在的事件 id 返回 `None`。
17. **非会话级事件**：`session: None` 的提交落库后 `session_sequence`/`origin_epoch`/`origin_sequence` 三列都是 NULL，且该行仍出现在 `replay` 流里；`session: Some` 的事件三列都非 NULL（成对 CHECK 不得被绕过）。
18. **消息收尾**（§6 第 14 条）：正常结束、取消、失败三种终态各一例；同一 turn 内两个 `messageId` 各自收尾；**没有** delta 的 turn 不产生 `agent.message.completed`；`content` 的文本等于该消息 delta 文本按 `deltaIndex` 的拼接；`completed` 与该 turn 的终态事件在**同一次提交**（用装饰 store 断言批次）；`agent.thought.delta` 不产生 `completed`。
19. **压缩**（§6 第 15 条）：`persist_deltas = false` 时 turn 终态后出现恰好一条 `turn.delta_compacted`，被压行的 `compacted_into` 等于该行的 `global_sequence`，重放里不再出现那些 delta（summary 只带 `turnId`/`deltaCount`）；turn 未终态时**不**压缩；`persist_deltas = true` 时永不压缩；`compacted` 里塞入非 delta 行或跨会话 cursor → `InvalidRequest` 且零写入。
20. **启动恢复**（§6 第 16 条）：造 `status='accepted'` 且无终态事件的 mutation 行 + 未终态 turn → 恢复后该行 `status='uncertain'`、`terminal_event_id` 非空且指向存在的 `command.uncertain` 事件，对应 turn 为 `failed`；**无会话**形状（`session.create` 在受理与落账之间崩溃：`accepted` 行 + `session_id` 为 `NULL` + 没有会话行）的恢复**不得**走「有会话」分支——该行终结为 `uncertain`、本机仍无该会话的会话行、`unsettled_commands` 之后返回空（`session-create-settle-order` 变更）。
21. **`session.mode.list`**（§6 第 17 条）：端口返回的候选列表原样出现在结果里；`currentModeId` 与会话 `current_mode` 一致；`version` 等于会话版本；端口返回空列表时结果为空（不伪造）。
22. **附件事务边界与孤儿回收**（§6 第 18 条）：`prune_lru` 删行后崩溃（模拟：删行后不执行文件删除）→ 库内无悬空行；`sweep_orphans(启动时刻, n)` 删掉表外文件、**不删** `mtime ≥ 启动时刻` 的文件、遵守 `limit`、返回实际删除数；回收失败不阻止启动（有警告）。
23. **管理写集的原子性**（§11.2 第 1/2/6 条、§5.3）：一次管理端口调用 = 一个事务 = 状态 + 引用 + 审计；注入审计写失败、唯一键/CHECK 冲突、磁盘满 → 整事务回滚，断言既不留「提交成功但没有持久信任」，也不留半条授权。
24. **配对、撤销与重启恢复**（§11.2 第 3 条、§11.3）：两个并发 `claim_pairing` 恰好一个成功；拒绝或过期不创建信任；已撤销身份不能经普通写入复活；重启后撤销仍有效、未确认配对不再可用、同一 `nodeId` 的两种角色行共享一条身份材料并被一并撤销。
25. **配对公钥与信任材料**（§11.5、§5.3）：认领并重启后仍能经 `TrustStore::peer_key` 取到验签公钥；指纹与公钥不一致的写集无法落库（表级 CHECK + 用例层构造校验）。
26. **imported 家族按 Owner 节点寻址**（§7.4）：`imported_session` 主键为 `(owner_node_id, session_id)`，交付索引与命令引用没有 `export_id` 列。
27. **本地配置边界**：Agent profile 只来自启动配置 `[agent]`，不写 SQLite；目录由成功的会话创建自动记录；v10 迁移删除 `owned_agent_profile` 与 `owned_provider_ref`，被删除的 profile 条数记入 meta 供启动提示；不再有 Provider 凭据解析端口。
28. **旧库升级的保留与幂等**（§7.2）：升级保留 `server_epoch`、事件 `global_sequence`/`session_sequence` 与 origin cursor、`requestId` 与幂等行、命令终态与全部既有审计；`audit_id` 与其 `AUTOINCREMENT` 序列不回退（v1 → v2 → … → v13 连续升级也要保持），审计表的新取值在升级库上可写、`owned_command` 的 `actor_kind` 不接受 `pairing_claimant`；`imported_import` 不再有 `export_id`，Export 关联迁入 `imported_import_export` 且 `added_at` 取原 `created_at`，无可信来源的 grants 保持 ''[]''（该 Import 不可用）；升级后第二次打开 `sqlite_master`/`meta`/行集逐字节不变；v4、v5 与 v6 的追加列不改变以上任何一条，v7 段重建的两张事件表也不改变以上任何一条（`owned_event` 的全部行与 `global_sequence` 逐行保留、`owned_interaction` 的行与外键逐行保留，且升级库的 `origin_kind` CHECK 接受五值——含新增的 `node`），v8 段删除的 `owned_device.scopes_json` 同样不改变以上任何一条（`owned_device` 的既有行逐列保留、删列后升级库与新建库的列清单逐项相等），并额外断言：① 升级库与新建库的 **owned** 家族列清单（列名/顺序/类型/`NOT NULL` 与默认值）逐项相等，`owned_node.export_ids_json` 与 `owned_session` 的 `agent_session_id`/`workspace_cwd`/`workspace_alias` 分别在各自表的末尾（按各自追加顺序）；② v3 及更早的库升级后既有节点行的 `export_ids_json` 为 ''[]''（即不得默认放权），这些行的其余列逐列不变；③ v4 及更早的库升级后既有会话行的 `agent_session_id`/`workspace_cwd` 为 `NULL`（不是空串、不是占位路径），其余列逐列不变，且这些会话不被任何路径当作可恢复会话（`SessionStore::load_recovery` 返回 `Ok(None)`）；④ 恢复两列写入后重开库读回的值与写入时逐字节相同，未写入两列的会话读回仍为 `NULL`（不出现空串、别名或占位路径），且恢复流程（`load_recovery` + `session.resume` 的 `accepted` 幂等行）不覆写它们、不改会话版本；⑤ v5 库升级后既有会话行的 `workspace_alias` 为 `NULL`（不是空串、不是任何占位别名），其余列逐列不变，且这些会话的 `SessionSummary.workspace` 为 `None`（未分组）——即使它的 `workspace_cwd` 恰好等于某个**已登记**目录的规范化路径，也**不得**反查补齐；⑥ 别名写入后重开库读回的值与写入时逐字节相同，未写入的会话读回 `None`；别名已登记时展示名取 `owned_workspace.display_name`，别名不再登记时回退为**别名本身**，同一别名重指向别的目录后既有会话的归属不变（`workspace_cwd` 也不被改写）；恢复流程前后该列逐字节不变；⑦ v13 段：升级库的 `owned_session` 无 `closed_at`、`state` CHECK 只接受三值，旧七态按迁移暂存映射写成 `idle`/`running`/`pending`（不发布、不据此伪造任务结果）；升级库的 `owned_interaction` 有 `confirmed_at` 与 `turn_id` 两列，`turn_id` 只等于创建事件（`request_event`）的 `turn_id`，旧 `resolved` 行 `confirmed_at = resolved_at`；`imported_session.state` 只留下三态；版本常量落为 13/5、`PRAGMA user_version = 13`，第二次打开逐字节不变；升级失败整体回滚、重开可重试，`sqlite_sequence`、既有索引、外键、事件与审计行逐字节保留。
29. **管理状态纳入容量与失败关闭**（§7.5、§8）：容量度量包含管理表的 TEXT 列；超限时拒绝新写入而不删除活动信任、撤销记录或未到期审计；损坏库或宽松权限下**管理写路径**与 owned 写路径一样全部被拒，只读查询仍可用。
30. **管理记录的终态与单调性**（§11.1、§11.2 第 4/5 条、§5.2/§5.3 约束、§7.4）：① `put_export` 对已撤销的 Export 不得清除 `revoked_at`——传入未撤销记录 → `Conflict(AlreadyExists)` 且该行逐列不变，传入 `revoked_at` 非空记录 → `InvalidRequest` 且零写入（含零审计）；② 完整移除 Import 后，携带该 `(ownerNodeId, exportId)` 的 `upsert_session` 与 `commit_receipt` 都返回 `NotFound(Export)`，`imported_session`/`imported_delivery_index`/`imported_command_ref` 保持为空且不推进 `local_sequence`；③ `owned_peer_key`/`owned_pairing_peer`/`owned_device` 中任一行 `fingerprint` 与同行 `public_key` 的派生值不一致时，对应读取路径（设备记录含单读与列表读）返回 `PortError::Corrupt` 且不返回材料；④ `last_seen_at`/`last_connected_at` 在「旧值为空」「新值更早」「新值为空」三种边界下都不丢值、不倒退，且不使调用失败或丢弃同写集的其他字段。
31. **§10.3 的 view 身份与版本**（§6 第 19 条）：① 适配器视图不含 `turnId` 时，落盘 view 与 `owned_event.turn_id` 都等于 core 的权威 turn，且该 view 除新增的**一个前置成员**外逐字节不变（含未知字段、嵌套结构；ACP 原文 `raw_json`/`sha256`/`byte_length` 不变）；② 会话级或无归属事件不出现 `turnId`；未列入 §10.3 的类型（如 `terminal.output`）不新增该字段；③ 视图已带 `turnId` 且取值一致 → 字节不变且只出现一次，取值不一致 → `InvalidRequest` 且该批零落盘、零发布、turn 状态不变；④ 含 `session.mode.changed`/`session.config.changed` 的提交：无 `StateChange` 时注入当前版本且不递增，含 `StateChange` 时注入递增后的版本，两者都必须等于存储层返回值；存储返回不一致 → 不发布且不报成功；⑤ 幂等重放的 view 与首次落盘逐字节相同且不二次注入；⑥ 适配器 `prompt` 返回任意值（含全零占位）都不产生第二个 turn 行，也不改变归属。
32. **节点信任不含授权清单**（§3.5/§7.2）：`owned_node` 只有身份、角色、状态与时间列；`exportIds`/`grants` 已随 v11 删除。
33. **会话历史读（带正文、按会话内序号分页）**（本文件 §5.2 的对应 `[决定]`、§7.3；`design.md` D2/D4，`SYNC_PROTOCOL.md` §11.5）：① `include.history` 打开时 `HistoryPage.history` 按 `session_sequence` **升序**给出一页条目，省略游标 = 最新一页、带游标 = 严格更早的一页：以上一页最早条目的序号为游标续读**不重不漏**（两页交集为空、并集连续），以仍可读的最早条目为游标时页标记 `historyHasEarlier` 为假且不返回空占位，同一游标 + 同一 `limit` 重复读结果**完全相同**；② `include.history` 关闭时该集合为空且页标记恒为假；`events` 仍只受 `include.messages` 与 `after` 控制，两者在同一请求里互不改变对方的返回内容；③ 条目**自带持久化正文**：`view` 与库内 `payload_json` 逐字节相同（不重新序列化）、ACP 可用时字节保真、不可用时按行内 `acp_raw_unavailable_reason` 与同行摘要还原；「只查一遍库」（页内每个元素的正文都来自同一次取行，逐条回查 `event_payload` 的实现不合格）由**调用路径检视**判定，行为用例固定的是正文与库内字节的逐字节相等；④ `compacted_into IS NOT NULL` 的行不出现在条目里（也不占位、不计入页标记）；⑤ 原文被清理过的行给出 `AcpRaw::Unavailable` 并带 `acp_raw_unavailable_reason` 已登记的原因与同行摘要；⑥ 游标早于该会话仍可读条目的下界时返回 `InvalidRequest`（`SYNC_PROTOCOL.md` §11.5 的「保留窗口之前显式不可读」），不得静默返回更少的条目。

34. **附件分块与封存（WP2 已实现）**（§5.8/§6 第 22 条/§7.6）：① 同偏移同内容重复提交返回原确认（`duplicate = true`）且不重复写入，同偏移不同内容返回冲突，非期望偏移返回带 `expected/actual` 的错误；② 处理 1 GiB 输入时进程驻留内存不随文件大小线性增长（断言单块工作集与 `chunk_size` 同阶）；③ 流式 SHA-256 与一次性摘要一致，长度/摘要不符不产生可引用附件；④ 跨会话、跨主体、跨设备的 `transferId`/`attachmentId` 一律 NotFound/InvalidRequest 且不泄露内容；⑤ 撤销在事务内中止 `receiving` 传输，已提交附件不被删除；⑥ 活动传输的临时文件（`attachment_dir/.transfers/`）在 `sweep_orphans(now)` 后仍存在且上传可继续；`prune_transfers` 不清理 `updated_at` 在 24 小时内的传输与仍被活跃消息/清单引用的内容；⑦ 已封存但未绑定的清单在 24 小时后过期，成功绑定则立即删除清单行（附件改由会话绑定/媒体引用保护）；⑧ 自定义 `storage.attachment_dir` 时封存、清单复核、绑定与 `read_chunk` 使用同一目录映射；⑨ 附件被 GC 后指向它的已封存传输变为 `expired`（`sealed: None` 不再出现）。
35. **消息通道与部分正文（WP2 已实现）**（§3.3/§6 第 14 条/§7.6）：回复与思考分别落盘（`channel` 区分）；停止、失败、断线三种情形都保留已收到部分并标记 `partial_result`；`thought` 不产生 `agent.message.completed`；`owned_media_ref` 只存引用、不含字节；历史读只从 `owned_message` 取正文，不从事件或快照重复拼接。消息正文按 `(created_at, message_id)` 复合游标分页：省略游标 = 最新一页、带游标 = 严格更早的一页，`messages_has_earlier` 逐页收敛，最多返回 `messages_limit` 条（`0` 表示不取正文），媒体引用只查本页消息；消息保留期到期后，附件随会话绑定失效并由 GC 回收。
36. **有效交互、仲裁与确认（WP2 已实现）**（§3.3/§5.8/§6 第 13 条/§7.6）：`request_version` 是当前有效请求版本（新行默认 1；本版本不递增，重试沿用同一值）；版本不符返回冲突且不消费请求，且校验先于命令受理（失败不留 `accepted` 行）；两个并发应答恰好一个胜出、另一个得到已处理事实，胜出行 `confirmed_at` 仍为 `NULL` 且继续计入 `session_activity`；只有 `confirmation` 写 `confirmed_at` 并把该交互移出有效集合，重复确认/未知交互/仍 `pending` 的确认都是无操作；turn 终态只失效该 `turn_id` 下未失效的交互（含已仲裁未确认），不误伤其它 turn；受理之后的竞态失败落 `command.failed` 终态；Daemon 重启后上游丢失的交互被置为 `expired`（不再出现在 `effective`/`read_session`/Node Link 快照，旧解析路径返回 `Conflict(Expired)`）且不可复活；表单值整数不经浮点往返，整数 `minimum`/`maximum` 用精确整数比较（2^53 边界用例）。
37. **三态归约**（§3.3/§5.2/§6 第 7/13/16 条）：`SessionActivity::reduce` 是唯一取值来源；有有效未解决交互 → `pending`（含已仲裁未确认）；无交互但有未结束 turn（`queued`/`running`/`waiting_*`）或「已派发但停止未确认」的占位 → `running`；否则 `idle`。失败/取消一个 turn 后会话不再进入终态，后续排队 turn 继续推进；状态未变的提交不重复生成 `session.updated`，变化时摘要事件与 `session.updated` 同一次提交且 `session.updated` 携带完整摘要与嵌套 `session.version`。
38. **持久队列恢复（§6 第 16 条/§4）**：`queued_turns` 按 `queue_index` 升序返回遗留排队项；重启后在内存队列为空且无在跑 turn 时，遗留项被显式终结为 `failed`（不静默删除）、`accepted` 命令结算为 `uncertain`，随后 FIFO 从后续项继续；`sessions_with_pending_activity` 返回仍有未结束 turn 或有效交互的会话（含刚被失效清空的会话在失效前的快照路径）；`uncertain` 命令与无法确认停止的执行**不**自动重试、不重放 Agent 副作用，后者保留占位、不提前 `idle`。
39. **`session.updated` 的唯一来源（§6）**：事件由 core 在状态变化时生成，一次提交最多一条；确认、仲裁、turn 终态与恢复路径都不得各自另发一条；payload 的 `session.version` 等于 `CommitOutcome.version`，漂移 → 失败关闭（§6 第 19 条）。
40. **本机生命周期归约与查看租约（计划，WP2 实现）**（§3.3/§5.10/§6 第 23 条）：① 假时钟下，纯 `session.view` 查看者让会话在保留期内保持 `Active` 且不启动 Agent；② 离页、断网、撤销与租约到期分别移除查看者，`control.ping` 不续租；③ 最后保护条件解除后才开始计时，重新出现保护条件即取消原计时、再次解除后从头计时；④ 未结束/排队任务、有效待处理交互与「停止未确认」都阻止回收；⑤ 同版本重试幂等、旧版本不复活、切换会话只替换同一连接的查看者。
41. **容量准入与安全候选（计划，WP2 实现）**（§5.10/§6 第 23 条）：① 创建与恢复共用准入，预占计入上限；可证明未建立的激活失败释放预占，结果不确定的激活不丢占位；② 满额时只在未受保护候选中选最久未使用者，同一时刻以稳定会话标识打破并列；③ 所有候选均受保护时返回明确容量拒绝（映射 `session.capacity_exceeded`），不中断受保护会话；④ `sessions.idle_timeout_ms = 0` 仍执行容量策略；运行期调低上限不强杀受保护会话，只拒绝新增直到计数回落。
42. **生命周期事件的提交与启动收敛（计划，WP2 实现）**（§6 第 23 条）：① `session.activity.changed` 与该次状态变化同一次 `commit` 落盘，落盘失败不广播；② 重启后不把旧事件里的 `Active` 当作资源已恢复：启动收敛完成前不开放入站查询/订阅，收敛事件在旧进程资源与租约清空后提交；③ owned 当前查询、快照 `sessions` 与 `session.read` 摘要来自同一代运行期目录，租约/查看者/资源代不出现在任何快照、事件或 wire 字段；④ 关闭中或结果不确定的资源保持 `Active` 与容量占位，直到适配器确认释放。

## 10. 未决项

- `[已裁定]` `ElicitationValues` 的形状：见 §3.3——值域为 ACP `ElicitationContentValue` 的五种线格式（`Text`/`Integer(i64)`/`Number(f64)`/`Boolean`/`TextArray`）**加**一个「未知形状原样保留」变体；边界 ≤64 KiB、深度 ≤16；`Submit` 的空映射与 `None` 可区分（分别对应 ACP `content: {}` 与 `content: null`）；`Decline`/`Cancel` 必须为 `None`。同时补齐 ACP 已有而我们 v1 原先缺失的 `decline` 动作（已同步 `SYNC_PROTOCOL.md` §11.5/§10.2 与 `NODE_LINK_PROTOCOL.md` §12.7）。
- `[已裁定]` Sync 与 Node Link 对 `completed` 的 `result` 要求不同（Sync 允许 `null`，Node Link 要求非空对象）：**认定差异是有意的**——Sync 的 mutation 完成后可能没有可返回的数据，Node Link 必须让 `session.create` 回传 `SessionCreateResult`（§12.7）。core 保持 `Option<CommandResult>`，「非空」是 `server::node_link` 的映射期义务并带契约测试；两侧协议文档各加一句说明（`SYNC_PROTOCOL.md` §11.2、`NODE_LINK_PROTOCOL.md` §12.5）。
- `[确认]` 序号上界 `2^63−1`（已批准）：随本次改动写入 `SYNC_PROTOCOL.md` §3.3。
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
- `[open]` **`session.mode.changed`/`session.config.changed` 的 `version` 语义**（2026-09-24 登记，来源：变更 `core-turn-view-fields` 的登记轮次）：§10.3 只要求「十进制字符串的会话版本」，而 §6 第 19 条的推导口径是「该次提交后的会话版本」。真实模式/配置切换流程中，`session.*.changed` 事件（来自适配器）与状态变更是**两次提交**（§6 第 8 条的 turn 边界语义），因此注入的是**变更前**版本，变更后的版本只出现在后续 `command.completed` 的 result 里。本变更不改变批形状（不合并两次提交）；在 Sync 切片前必须裁定：要么把两者合并为同一提交（使事件承载变更后版本），要么在 §10.3/Sync 文档里明确「mode/config 事件承载变更前版本」并让客户端不以它为乐观并发基准。**未裁定前 Sync 切片不得假设事件里的 `version` 等于会话最终版本。**
- `[open]` **`command.completed.result.turnId` 承载的是会话版本而非 turn 标识**（2026-09-24 登记，来源同上）：`view_command_completed` 把 `apply_state` 传入的 `Some(version)` 渲染成 `result.turnId`（十进制字符串），而 `SYNC_PROTOCOL.md` §11.5 的 `result.turnId` 示例是 UUID。本变更未触碰该函数（属变更外既有缺陷）；应在 Sync/CLI 切片把它改为 `result.version`（或 `null`）并同步协议文档与 fixture。
- `[已裁定]` `owned_interaction.elicitation_action` 的 CHECK 补 `'decline'`（原值是 `submit|cancel` 两值，会让 ACP 早就有的 `decline` 在解析事务里撞约束、交互永远停在 `pending`）；同时要求 DDL 里每个 `IN (...)` 枚举字面量与 core 枚举的 wire 值逐条一致，并有一条「每个枚举值都能写入并读回」的测试。
- `[已裁定]` id 分配收敛为两处权威（§3.1）：`SessionId` 归 **core** 的 `IdGenerator::session_id`（`session-create-settle-order` 变更改写——后端要在**落账之前**拿到它构造引用，而会话行那时才落盘，因此受理阶段只能留空的 `session_id`，由落账提交采用 core 给的那个值；存储层不得再现分配）、`EventId` 归存储层事务内分配、`TurnId`/`InteractionId`/`PairingId`/`OriginEpoch`/`RequestId` 归 core 的 `IdGenerator`、`AttachmentId` 归 `AttachmentStore::put`。`IdGenerator` 因此**不提供** `event_id()`/`attachment_id()`——原 §3.1 与 §5.4 的写法互相矛盾，实现者只能二选一。
- `[已裁定]` `SessionBackendFactory::create` 增加 `session: &SessionId` 参数：§5.1 原先承诺「core 把分配好的会话传给 `create`」，而 §3.6 的 `CreateSessionRequest` 字段表里没有 `session`，后端因此拿不到 id、无法构造 `SessionEndpoint::reference()`。
- `[已裁定]` `templateId` 的权威是 `schemas/node-link/v1/common.schema.json` 的 `^[A-Za-z0-9._-]{1,128}$`（§3.6 也写的 1..=128）：`docs/LOCAL_ADMIN_PROTOCOL.md` §5.5 与 core 的 `is_template_id` 原写 64，按 wire 取 128（否则跨节点会拒收 schema 合法的模板 id）。
- `[已裁定]` `payload_digest` 由**存储层**用 `acpr-wire` 的 ACPR-CJ1 实现从 `payload_json` 重算（§7.3/§9 判据 9）：原实现直接哈希库内字节，而 `payload_json` 按判据 16 原样保留调用方字节（不重排键），两者只有在调用方恰好传了规范 JSON 时才相等——跨节点复算与 imported 去重都会失配。ACPR-CJ1 是跨 Sync/Node Link 的共用机制，因此实现放 `acpr-wire`（叶子 crate），并与 JS 参考实现在同一批 fixture 上互校。
- `[已裁定]` 清理顺序补 ②′（终态交互行必须先删）且 ②/③ 谓词必须排除仍被 `owned_interaction.request_event` 引用的行；容量度量补 `imported_*` 家族分量（否则 Access-only 节点度量恒 0、永不回收）；⑤ 的审计清理对 `owned_audit` 与 `imported_audit` 都生效。三条都是实测缺陷：外键会让 `prune` 整事务回滚、并让超限提交报 `Backend` 而不是 `StorageFull`。
- `[已裁定]` `PendingEvent` 增加必填 `origin: EventOrigin`（§3.4）：原写入形状没有 origin，存储层只能按「有没有会话」猜出 `agent`/`daemon`，等于伪造 `origin.kind`。
- `[open]` 交互解析的崩溃窗口与补偿机制（`session-activity` 后为「仲裁后未确认」）：当前用 `confirmed_at` 区分阶段、启动恢复 `invalidate_pending` 终结仍有效的行（不可复活），因此不会永久停在待处理；若要连「已仲裁未确认」也精确补偿，需要「解析意向」行或两阶段提交。
- `[已裁定]` **附件文件删除的事务边界与孤儿回收**（§6 第 18 条）：行删除与事务同提交、文件删除在提交之后；孤儿回收由组合根启动时调用一次 `AttachmentStore::sweep_orphans(启动时刻, 1000)`，只删「不在表里且 mtime 早于本次启动」的文件，失败不阻止启动。新增该端口方法（§5.3）。
- `[已裁定]`（2026-09-23）管理写集需要的 `ConflictKind` 新取值（`AlreadyExists`、`IdentityMismatch`、`DuplicateOwnership`）与 `UnavailableKind::KeystoreUnavailable`：枚举本体、`ALL`/`as_str` 与 §2 的取值表已同批落地（`crates/core/src/model/error.rs`）；写集侧映射义务与 `port_error_public` 不得用通配臂吞掉新取值的陷阱见 §5.3 与 §11.6 末段。
- `[已裁定]`（2026-09-23）`AuditAction` 追加 `ExportCreated`/`ExportRevoked`/`ImportAdded`/`ImportRemoved`/`ProviderConfigured`（`SECURITY_DESIGN.md` §14.2）：落库靠 §7.3 与 §7.4 两张审计表 `action` CHECK 的扩宽；既有 v1 库走 §7.2 的第 ② 步 12-step 表重建（保留全部行与 `audit_id`、`AUTOINCREMENT` 序列不回退）。
- `[已裁定]`（2026-09-23）首切片 workspace template 必须零参数（`NODE_LINK_PROTOCOL.md` §10）；有参 template 属 `post_mvp`，启用前必须定义值的来源与用途。
- `[已裁定]`（2026-09-26）配对通道的 actor 与用例面扩展（design D12，用户裁定方案 A）：`core::model` 新增 `Actor::PairingClaimant { pairing: PairingId }`（`ActorKind::PairingClaimant` = `'pairing_claimant'`）与两个节点审计动作；`claim_pairing`/`pairing` 另接受绑定该配对的认领方，新增 `consume_pairing(actor, PairingId)` 与 `TrustStore::consume_pairing`（`PairingConsumption`）；存储走 **v2 → v3** 迁移（两张审计表的 `actor_kind`/`action` CHECK 扩宽，12-step 表重建），`owned_command` 不动——`pairing_claimant` 永不可提交命令。涉及 §3.5/§4/§5.3/§7.2/§7.3/§7.4 与 `SECURITY_DESIGN.md` §14.2、`IDENTITY_AND_AUTH_CONTRACT.md` §5.1，漂移门禁随动。
- `[已裁定]`（2026-09-26）D12 的 seam 补全（用户裁定 A 的实现细化，不改变方向）：§4 新增只读入口 `pairing_channel_view(actor, PairingId) -> Option<PairingChannelView>`（记录 + 已认领对端行 + 已批准后的对端 `access` 节点行，绑定式读取、零写入）；`identity-auth` 新增 `Authority::verify_node_link_pairing_status`（用本机内存里的 pairing secret 校验 `node-link-pairing-status/v1` 的 HMAC）与 `Authority::pairing_request_material`（返回 claim 响应与 Owner 证明需要的**非秘密** `serverNonce`/`pairingRequestId`，与 secret 同生同灭）；claim 路径的读取顺序与拒绝口径写在 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1。
- `[已裁定]`（2026-09-26）WSS 握手 seam 补全（`node-link-owner` 的 WP4/任务 2.28，主 Agent 裁定方案 A，与 D12 同方向）：§4 新增 `NodeLinkHandshake` 两项——只读入口 `node_link_handshake_view(accessNodeId) -> NodeLinkHandshakeView`（用例面唯一无 `actor` 的入口：按单个自报 node id 返回该节点的 `access` 信任行、已绑定验签公钥、最近一次配对与本机水位 `store.head()`；未知 id 返回空视图而不是错误，零写入、零审计）与窄写入口 `record_node_link_auth(accessNodeId, action, outcome)`（只接受 `node.authenticated`/`node.auth_failed`，其余 → `authorization.scope_denied`）；§5.3 新增 `TrustStore::pairing_for(peer) -> Option<PairingRecord>`（该对端最近一次配对：登记方宣告的 `host_binding` 与「待消费配对」的唯一来源——节点信任行不带绑定列）。既有 `LocalCli` 路径的行为不变，不新增对端可见能力；`identity-auth` 的 `HandshakeFailure::audit()` 对 NodeLink 从 `None` 改为 `Some(node.auth_failed)`（原注释所称的「词表缺口」已由上述两个动作补上）。
- `[已裁定]`（2026-09-26）认证收尾的 `last_connected_at`（`node-link-owner` 的修复轮次，主 Agent 裁定选项①）：认证收尾必须在写集里推进 `owned_node.last_connected_at`（「最近一次认证成功时间」）——首次认证在 `consume_pairing` 的写集里（§11.6 第 8 条），重复认证在新增的 `TrustStore::record_node_connected`（`NodeConnectedWrite`，§5.3/§11.6 第 9 条）里；两条路径的时间与 `node.authenticated` 各自同事务，值只前进不倒退（§3.5/§5.3 约束）。原实现没有任何生产调用方推进该列（`put_node`/`upsert_node` 只被配对确认与测试使用），管理视图 `lastSeenAt` 因此恒空。涉及 §3.5/§4/§5.3/§11.6 与 `crates/{core,storage-sqlite,server}`，漂移门禁随动。
- `[已裁定]`（2026-10-17 取代）Owner 侧节点命令不再做授权交集判定：已配对且未撤销即可执行本节点命令；原 `node_allowed`/`required_grant` 已删除。
- `[已裁定]`（2026-09-23）`identity-keystore` 的 Windows 第一档位与 Linux 失败关闭：Windows 用 DPAPI（当前用户）包裹私钥 + 进程内签名，Linux 维持失败关闭；持久化 fallback、CNG/TPM 不可导出档位均需单独 ADR，wrapper 选型与 MSRV 约束见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20。实现前合同见 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md) §7/§9。
- `[open]` 未来加密离线正文缓存（必须新 feature + Owner 明示授权 + ADR；本合同不预留任何静默开关）。

## 11. 管理状态持久化合同（形状已并入 §3/§5/§7）

`[已并入]`（2026-09-23）本节收口的设计**形状**已经落地：值对象在 §3.5/§3.6，写入 DTO、端口签名与 workspace 解析规则在 §5.1/§5.3，管理与 Import 关联表的 DDL 在 §7.3/§7.4，版本常量与 v1 → v2 迁移规则在 §7.2，验收判据在 §9 判据 23–29；`scripts/check-contract-drift.mjs` 把 §5/§7 与 `crates/core/src/ports.rs`、`crates/storage-sqlite/src/migrate.rs` 逐条绑定，因此本节不再保留签名与 DDL 正文（第二份副本必然漂移）。

本节剩下的是**设计理由**与到上述段落的指针。`storage-sqlite` 的**管理 store 落盘实现**（`TrustStore`/`LocalConfigStore` 的事务、失败关闭与容量纳入）已落地；Daemon/CLI 接线与 `server` 的**本地**入站适配器（`server::local_admin`/`server::transport::local`）与 `app`（组合根、CLI、`acp-stdio`）已随切片 4 `daemon-cli-and-local-admin` 落地；仍未实现的是 `server::sync`/`server::acp_facade` 与 `node-link-client`（`server::transport::net` 与 `server::node_link` 的 **Owner 侧入站面**已随切片 5 `node-link-owner` 落地，Access 侧出站属切片 6，见 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9；`identity-auth`/`identity-keystore` 已落地）——在这些适配器完成之前，不得把这些存储测试通过解释为配对、撤销或本地配置已经端到端可用。配置与管理状态的来源优先级以 `CONFIG_REFERENCE.md` 的「配置与管理状态的权威」为准。

### 11.1 数据归属与表设计

管理数据与会话数据共用 §7.1 的数据库、串行写事务和文件权限；不新增独立数据库或通用配置服务。下表是逻辑表与其约束的索引，可执行 DDL 在 §7.3/§7.4 并由漂移门禁逐条断言。

| 表 / 记录 | 主键与内容 | 约束 |
|---|---|---|
| `owned_device` | `device_id`；§3.5 `DeviceRecord` 的全部非秘密字段 | 状态与 `revoked_at` 一致；同 ID 不得被普通 upsert 换绑公钥或恢复已撤销权限 |
| `owned_node` | `(node_id, kind)`；§3.5 `NodeRecord` 的全部非秘密字段 | 同一对端可同时承担 Owner/Access；`node_id` 的身份指纹必须一致；按 NodeId 撤销影响两种角色 |
| `owned_peer_key` | `(peer_kind, peer_id)`；65-byte SEC1 未压缩 P-256 公钥、SHA-256 指纹 | 公钥是非秘密认证材料；配对验证后与信任一起提交，重启验签从此读取；不保存私钥或完整认证请求 |
| `owned_pairing` | `pairing_id`；§3.5 `PairingRecord` 的状态、目标、请求权限、secret digest、`host_binding`、时间戳 | `host_binding` 是本机在该配对里宣告的绑定（设备 canonical origin、节点本机 endpoint），认领时被逐字回显（§11.2 第 1 条）；不存 secret、HMAC、QR URL 或完整认证 payload |
| `owned_pairing_peer` | `pairing_id`（主键）；claim 后的对端身份（`peer_kind`/`peer_id`）、名称、公钥、指纹、`client_nonce`、`claimed_at` | 每个配对最多一个 peer；与配对目标一致；claim 之后不能换人；确认时公钥转入信任材料。**不存绑定与角色**：绑定只在 `owned_pairing.host_binding`（认领时已校验两侧逐字相等，读取时回填），对端角色由配对方向推导（§11.2 第 2 条） |

| ~~`owned_agent_profile`~~ | v10 已删除：Agent 声明改为配置文件 `[agent]` | 不再有 SQLite 写入路径 |
| `owned_workspace` | `alias`；本地 `workspace.select` 的名称、规范化路径及创建/更新时间 | 路径**只在本机持久化**，且只在客户端协商 `core.workspace-path.v1` 时随 Sync 快照的 `workspaces` 条目下发（只覆盖已登记目录；`design.md` D1/D2、`SYNC_PROTOCOL.md` §9.4）；客户端自己提交的目录不被回显，日志、审计与错误 `details` 也都不含路径；建立/使用时校验目录与路径边界；不进入 Node Link catalog |
| `owned_provider_ref` | `(provider_id, kind)`；名称、已配置字段名、keystore 引用及版本 | 不含凭据值；引用失效时失败关闭，不回落到配置文件 |


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

实现入口由 core 用例组织授权和状态决定，storage-sqlite 在单个端口调用内提交完整管理写集；连接关闭等外部效果在提交后通过端口/组合根装配执行。管理写入 DTO/端口已按本节扩展为携带审计上下文与完整写集（§5.3）；禁止用“两次 await 共用一个连接池”宣称同一事务。

### 11.3 重启、保留与升级

- pairing secret 仍只存在内存；Daemon 重启不能凭 secret digest 恢复它。启动时终结未确认且无法继续验密的配对，客户端重新发起配对；已批准的信任记录保留，不要求正常重连重新配对。原有效期不能延长；临时 nonce、HMAC 和 QR URL 不落入普通数据库。
- pairing 终态记录至少保留到原 `expires_at`，随后可清理；安全审计按既有 365 天策略保留。活动信任、授权与配置不按聊天 TTL 清理；撤销 tombstone 不因容量压力被删除，防止旧身份恢复。
- 管理表纳入现有总容量度量；空间不足时拒绝新写入，不能删活动信任或未到期审计腾空间。Import 关联表与交付表继续执行无正文黄金列清单检查。
- 文件格式 v2 现在承载管理表与 Import 归属升级：owned/imported 家族版本各推进到 2；`server_epoch`、会话 origin、事件序号、requestId 与已有审计全部保留（§7.2）。旧二进制因版本过新拒绝打开，不能降级写入。
- v1 的单 Export Import 行已迁为一个管理行加一条关联行；没有可信来源的 grants 未补齐也未默认放权（写作 `'[]'`），该 Import 保持不可用、待本地重新授权。管理表初始为空；profile 种子与初始化标记一起提交，不从聊天或审计内容推断信任。
- migration 在取得单实例锁后、监听前完成，单事务失败全回滚；可重复打开且不重导配置。§7、DDL 常量、版本常量、夹具与漂移门禁已在同一变更内同步，「过新」用例使用高于新版本的值（`user_version = 9`，即 `FILE_FORMAT_VERSION + 1`）。

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

`[已并入]` DTO（`PendingAudit`/`WriteContext` 与读写集）与 `TrustStore`/`LocalConfigStore` 的签名在 §5.3。

**提交模型**：一次端口调用 = 一个事务 = 一个完整写集（状态 + 引用 + 审计）。禁止用「两次 `await` 共用一个连接池」宣称同一事务（§11.2）。任一约束失败（CHECK、唯一键、外键、条件更新）→ 整事务回滚，映射为 §2 的错误类型后返回；**不得**先提交状态再补审计，也不得在提交后才发现审计写失败。

- `WriteContext.audit` 为空**只允许**用于不作为 `AuditAction` 枚举中已登记安全动作的操作（例如 `workspace.select`、`agent.configure`）；涉及安全动作的写集必须至少带一条成功或失败审计（§9 判据 14 的「每个取值都要有写入用例」）。`provider.configure` 属于已登记安全动作（`provider.configured`），**必须**带审计。
- 被拒绝的请求只尝试追加失败/拒绝审计，不能因审计不可写而继续执行（§11.2 第 6 条）。
- 写集里的集合字段按 §11.1 编码为有类型的 JSON 数组文本；空集合固定写 `[]`（表级 CHECK 依赖这一点）。

`[决定]` **写入 DTO 与端口签名**：形状见 §5.3（`DeviceWrite`/`NodeWrite`/`DeviceRevocation`/`NodeRevocation`/`PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`/`ExpiryWrite` 与 `TrustStore` 的新签名）。读面保留原状并补角色维度，写面把 `upsert_*`/`revoke_*`（携带 `at`）/`claim_pairing`/`settle_pairing` 全部替换为写集。

写集语义（每条都要有对应测试，§11.4 已列验收项）：

1. `put_device`：同 ID 不得换绑公钥（`fingerprint` 与已存行不一致 → `Conflict(IdentityMismatch)`），不得把 `revoked` 改回 `active`（→ `Conflict(IdentityMismatch)`；唯一的复活路径是协议重新配对，见第 4 条）；撤销写 `device.revoked`。（设备不再有授权集合，因此 `put_device` **没有**「scopes 变化」分支；`device.scopes_changed` 是**只读的历史取值**、本二进制不再产生它，理由见 §3.5/§7.2。）
2. `put_node`：写 `owned_node` 行与 `owned_peer_key` 的绑定；同一 NodeId 的两种角色必须指纹一致；撤销过的身份只能按协议重新配对，不能经普通 upsert 激活。
3. `claim_pairing`：并发只有一个成功（唯一 peer 行 + 条件更新）；过期/已终态 → `Conflict(Expired/Consumed)`；本机绑定不一致 → `Conflict(IdentityMismatch)`（§11.2 第 1 条）；插入 `owned_pairing_peer` 与状态推进、`pairing.claimed` 审计同一事务。
4. `settle_pairing`：拒绝或过期**不创建**信任；`Approved` 时创建信任行、把 peer 公钥转入 `owned_peer_key`、更新配对为 `approved`，并按目标族写「信任建立」审计——**设备批准写 `pairing.approved`、节点批准写 `node.paired`**，两族拒绝都写 `pairing.rejected`（`SECURITY_DESIGN.md` §14.2 的最小集合；与 `device.revoked`/`node.trust_revoked` 同款对称）；任一步失败全回滚，绝不出现「返回成功但没有持久信任」。设备配对写下设备行（`active`）；节点配对写下 `owned_node` 的 `access` 行（§11.2 第 2 条的角色推导）；两条路径都要求指纹与既有绑定/角色行一致（同一对端不得换绑公钥，第 1 条），并**允许复活已撤销的身份**：这是 §11.2 第 2 条指定的唯一恢复入口，撤销审计不因复活消失（`revoked_at`/`revoke_reason` 随 `active`/`paired` 清空，历史留在 `device.revoked`/`node.trust_revoked` 审计行里）。节点配对批准时，`granted_export_ids` 必须在**同一事务内**校验（与 `granted_grants ⊆ requested_grants` 同址）：每个 id 必须能在 `owned_export` 里查到且未撤销（否则 `NotFound(EntityRef::Export)`，适配层映射 `local.not_found`），其 `scopes` 必须与本次 `granted_grants` 有交集（否则 `InvalidRequest`，适配层映射 `local.invalid_params`）；空集合合法，此时不查任何 Export 行。任一条失败即整事务回滚：不创建信任行、不推进配对状态、不写审计。清单只能**收窄** Owner 侧的命令授权与 catalog 可见性（`NODE_LINK_PROTOCOL.md` §8.2），不得因清单放宽任何权限。批准仍沿用既有的 `node.paired` 审计动作，**不新增** `node.export_ids_changed` 之类的动作（没有配对后修改入口，就没有「授权变更」这种事件）。
5. `revoke_device`/`revoke_node`：单事务写撤销时间、状态与审计；**提交后**才由组合根关闭适用连接并对管理调用作答（§11.2 第 3 条）。连接清理失败不回滚已提交的撤销。
6. `expire_pairings`：只终结**未确认**（`created`/`claimed`/`pending_confirmation`）的行，写 `pairing.expired`，已批准信任不受影响。`ExpiryWrite.only = None` 是原有时间扫描（`expires_at <= at`）；`Some(ids)` 只处理列出的标识且不判到期（重启后已无法验密的未确认配对必须立即终结），重复提交与不在列表中的行都是零写入。周期维护的顺序是 `UseCases::pairings` → `Authority::due_pairings`（同时清除内存 secret 与配对码映射）→ `expire_pairings_by_id`；列表读取失败时退回时间扫描。
7. `put_export`/`revoke_export`/`add_import`/`remove_import`：审计取值分别用 `export.created`/`export.revoked`/`import.added`/`import.removed`（§7.3 与 §7.4 的 `action` CHECK），与状态同事务（§11.2 第 4/5 条）。
8. `consume_pairing`（design D12）：`actor` 必须与 `owned_pairing_peer` 的对端**同类同 id**（不一致 → `Conflict(IdentityMismatch)`），且写集里的主体与每一条 `context.audit` 的归因必须一致（分歧 → `InvalidRequest`，接线错误失败关闭）；只有 `approved` 能推进到 `consumed`（条件更新 `AND state = 'approved'`，`terminal_at` 取本次 `at`，`node.authenticated`/`device.authenticated` 与状态同事务），并在同一事务推进对端节点行的 `last_connected_at`（第 9 条；`actor` 为 `Actor::Node` 时该行是配对批准写下的那个 `access` 行，`Actor::Device` 的配对没有节点行）；已是 `consumed` 且对端一致时幂等成功（**不**覆盖首次 `terminal_at`、**不**重复写审计，也不重复推进 `last_connected_at`——那是同一次消费的重复提交）；其余状态具名拒绍（`expired` → `Expired`、`rejected` → `Consumed`、未批准 → `InvalidRequest`），配对行或对端行缺失分别是 `NotFound`/`Corrupt`。该写集不设容量门：它只推进一个状态、不新增管理行，而且是认证收尾的安全动作（与 `revoke_*`/`expire_pairings` 同类）。
9. `record_node_connected`（`NodeConnectedWrite`，认证成功的收尾写集；新增于 2026-09-26 的 `node-link-owner` 修复轮次）：重复认证（没有待消费配对）把 `(node, kind)` 行的 `last_connected_at` 推进到 `context.at`，并把 `context.audit`（该对端的 `node.authenticated` 成功行）在**同一事务**提交；值只前进不倒退（与 `upsert_node` 的三分支 `CASE` 同口径，§5.3 约束），行不存在 → `NotFound(EntityRef::Node)`。首次认证的同一职责在 `consume_pairing` 的写集里（第 8 条）：两条路径都不得省——`lastConnectedAt` 的语义是「最近一次认证成功时间」，只在首次认证写会让它对后续连接说谎。不设容量门（只更新一个时间列、不新增管理行）；`context.audit` 必须非空（认证成功是安全动作，§11.6 的写集约束）。





- `import.add` 的关联行与既有 Import 冲突（重复 ID 或重复 Owner/Export 归属）必须显式冲突，不能静默合并。

`[已删除]` 本地配置写集（Agent profile / Provider 引用 / 种子标记）：Agent 声明改为启动配置，写入路径与端口一并删除。

- 凭据值只经平台 keystore 的端口（`IDENTITY_AND_AUTH_CONTRACT.md` §7），`ProviderRef` 里只有字段名、引用与版本（§5.3 约束、`SECURITY_DESIGN.md` §13.1）。
- `put_profile` 是唯一写入默认 profile 的入口：至多一个 `default = true`（§7.3 的部分唯一索引），切换默认必须是**一次调用的原子写集**（两条行一起改），不能两次调用。
- 种子语义：`seed_state` 报告「未初始化/已初始化」；`mark_seeded` 与种子 profile 同事务提交（空种子也提交标记）。初始化完成后数据库是唯一权威，不再重导配置。
- 端口纯度不变：`LocalConfigStore` 只使用 §3 的 `core::model` 类型，不出现 `serde_json::Value`/SQL/HTTP（§2）。

`[已删除]` Provider 凭据注入：Agent 自己持有凭据，本系统不再解析或注入；子进程环境固定为必要进程环境。

- 调用时机：只在 `SessionBackendFactory::create`/进程启动前解析；不得把解析结果缓存到磁盘、写进 `owned_*` 表或事件 payload。
- 注入边界：最终传给子进程的环境变量 = `env_allowlist` ∩ `profile.env` 的 `name` 集合，加必要的进程环境（如 `PATH`）；Node/Device 私钥、pairing secret 永不注入（`SECURITY_DESIGN.md` §12.2）。
- `resolve_env` 的返回值不得被 `Debug` 打印：请求/响应日志只记录变量名与数量，不记录值。
- 校验时机：`agent.configure` 在写入 profile 时就校验 `provider_id`/`field`/`name` 的存在性与子集关系（`local.invalid_params`）；运行期再失效只影响**新启动**的进程，不改变已运行会话。

`[已裁定]`（2026-09-23）新增 `ConflictKind` 取值 `AlreadyExists`、`IdentityMismatch`、`DuplicateOwnership`，以及 `UnavailableKind` 取值 `KeystoreUnavailable`（设备/节点身份失败关闭用）；新增取值不会引发编译错误，必须同步检查 `port_error_public` 的映射。

### 11.7 管理表 DDL（并入 §7.3/§7.4）

`[已并入]` 管理表（设备/节点/配对/Export/目录）与 `imported_import` 的新形状在 §7.3/§7.4；v10 已删除 `owned_agent_profile` 与 `owned_provider_ref`。

- **禁止跨族外键**（`MODULE_ARCHITECTURE.md` §4.7）：`imported_import_export.owner_node_id` 指向的是 owned 家族的节点记录，因此只存值、不用 FK；存在性由用例层在写集内校验（`import.add` 的前置条件，`LOCAL_ADMIN_PROTOCOL.md` §5.5）。
- **不新增 catalog 投影表**（`[决定]`）：Access 侧的 Node Link catalog 是**连接期内存数据**，不进 SQLite（`NODE_LINK_PROTOCOL.md` §12.3）。`SECURITY_DESIGN.md` §13.4 的可持久化白名单是封闭的，catalog 不在其中；因此 `imported_*` 家族不因它增加新表，也不得为「离线也能改 Import」再加一张。
- **跨两族的列类型约定**：`public_key` 是 STRICT 表的 **BLOB** 列：adapter 必须绑定字节（`Vec<u8>`/`&[u8]`），绑定 hex 或 base64 文本会被 STRICT 直接拒绝。这层拒绝是特性（阻止「同一份材料两种编码」的静默分叉），不是障碍。
- 管理表的集合字段（`*_json`）由 SQLite adapter 按领域构造器校验后编码；身份、状态、时间、唯一键与外键用显式列，不用 JSON blob 承担仲裁（§11.1）。v11 之后节点信任不再有集合列。
- `imported_import` 自身升级后仍**不含任何正文列**，并继续纳入 §9 判据 6 的无正文黄金列清单检查。
- `owned_audit`/`imported_audit` 复用既有列集合、不新增内容列；v2 → v3 升级只扩宽 `actor_kind`/`action` 的 CHECK（12-step 重建），因此列清单测试（§9 判据 14）的期望值不变。
- 收口前的一次性现场校验（在 SQLite 上实际执行这 12 条语句并对 16 条约束做正反断言）只是探针惯例（`INITIAL_DESIGN.md` §16），**未入库、也不构成实现完成**；这些断言现在由常驻测试承载：`tests/enum_coverage.rs`（枚举逐值）、`tests/imported.rs` 与 `tests/commit.rs`（黄金列清单）、`tests/migration.rs`（升级保留、幂等、`audit_id`/序列、Import 归属迁移与缺 grants 行为）。

### 11.8 版本常量、migration 与 fixture（并入 §7.2）

`[已并入]` 三个版本常量、升级判据、四步升级过程、保留不变量与夹具清单在 §7.2，可执行 DDL 在 §7.3/§7.4。保留的设计理由：

- **为什么必须做 12-step 重建**：SQLite 不能修改既有 CHECK，而 `owned_audit`/`imported_audit` 的 `action` CHECK 必须接纳 `export.created`/`export.revoked`/`import.added`/`import.removed`/`provider.configured`（`SECURITY_DESIGN.md` §14.2）；重建在同一事务里保留全部行与 `audit_id`，并按升级前的 `sqlite_sequence` 回填序列（审计有 TTL，`seq` 可能领先于 `max(audit_id)`）。列集合不变，因此「黄金列清单」测试（§9 判据 14）的期望值不动。
- **为什么不补 grants**：v1 没有存储 Import 的 grants 来源，凭猜测补齐等于凭空放权；迁移写 `'[]'`（该 Import 保持不可用），把重新授权交回本地管理入口。
- **为什么 v3 → v4 只加列、不做 12-step 重建**：本次只给 `owned_node` 追加一个集合列（`export_ids_json`），不改任何 CHECK——重建的唯一理由是改 CHECK（v2 → v3 的先例）。`ALTER TABLE ADD COLUMN` 恰好把新列追加在列清单末尾，升级库与新建库的列清单因此逐项相等（§9 判据 28）。既有行由默认值得到空清单，口径与「为什么不补 grants」一致：**不得默认放权**——升级后这些配对要在 Owner 本机重新确认才能看到任何 Export（连接与握手仍然正常，`NODE_LINK_PROTOCOL.md` §8.2 的升级副作用）。
- **为什么不提供降级**：v2 库对 v1 二进制是「版本过新」，只能拒绝打开；回滚靠备份 + 回退二进制，不写自动降级迁移（§11.3）。

### 11.9 workspace 解析（并入 §5.1/§3.6）

`[已并入]` `ResolvedWorkspace` 的形状在 §3.6，解析时机、校验、失败分类、别名命名空间与路径下发口径在 §5.1。保留的设计理由：**alias → 实际路径的解析归 core，不属于任何后端**——`agent-host`/`node-link-client` 都拿不到 `owned_workspace`（`MODULE_ARCHITECTURE.md` §5 矩阵：后端只依赖 `core` + 协议 crate）。路径的边界口径（`SECURITY_DESIGN.md` §12.3、`SYNC_PROTOCOL.md` §9.4 与 [ADR-0009](./adr/0009-authenticated-workspace-path-and-browse.md)）是：**已登记**目录的规范化路径只在客户端协商了 feature `core.workspace-path.v1` 时随 Sync 快照的 `workspaces` 条目下发给已认证设备（取值剥掉平台 verbatim 前缀），**客户端自己提交的目录不得被回显**，而日志、审计、错误 `details`、事件载荷与会话摘要仍一律不含路径。UNC/网络路径允许解析但不改变授权模型；其结构化告警落在 `server` 层（core 不引入日志依赖）。

### 11.10 旧远程持久对象清理（`access-owner-data-stateless`，已落地）

运行期远程来源端口的最终定义在 §5.9（`RemoteRuntimeDirectory`、`RemoteSourceReads`、`RemoteCommandStatus` 及其值对象，代码在 `crates/core/src/ports.rs`）；本小节不重复端口形状。

v14 迁移已删除 `imported_delivery_index`/`imported_command_ref`/`imported_session`/`imported_audit` 及更早的 `imported_import*` 表，并按 §7.2 的来源谓词清理可辨认的远程代理 `owned_command`/`owned_audit` 记录。清理后启动门禁（`SqliteStore::run_startup_gate`）验证远程家族不存在、owned 事件/epoch/幂等与节点信任逐项保留，并完成 WAL/自由页收尾；失败则不开始监听，重开可安全重试。

