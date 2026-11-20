# 独立持久化展示快照库与混合自适应历史恢复技术设计

## Context

动机与用户确认依据见 [proposal.md](proposal.md)，行为规则见 [session-transcript-cache](specs/session-transcript-cache/spec.md)、[structured-history](specs/structured-history/spec.md)、[native-session-recovery](specs/native-session-recovery/spec.md)、[session-resource-policy](specs/session-resource-policy/spec.md) 增量规格及 `openspec/specs/` 长期规格。后端基于 `app/src/config.rs`、`app/src/data_dir.rs`、`app/src/application/shared_views/`、`app/src/application/native_dispatch/` 与 `app/src/node/native_views.rs` 现有 `ACPV` 分块流及 50 主会话容量模型演进；前端基于 `refactor-web-workbench` 模块化架构（`clients/web/src/views/*`、`clients/web/src/management/*` 与 `clients/web/src/workbench/*`）扩展。

## Goals / Non-Goals

**Goals:**
- 在节点 `data_dir` 下建立与核心控制管理库 `node.sqlite3`（`SCHEMA_VERSION = 15`）物理隔离的独立展示快照库 `transcript-cache.sqlite3`，忠实持久化经标准 ACP 观测到的原始帧与协商协议版本，复用 `AcpViewProjector` / `IndexedJournal` 离线保真还原结构化轮次、消息、思考、工具原位更新与子步骤引用。
- 实现冷会话“首屏秒开本地快照（零名额消耗、零子进程等待）+ 后台自动发起一次标准重放校准（Stale-While-Revalidate）”体验、50 名额全忙或对端不可达时的优雅降级与手动重试、`session/resume` 无重放上下文恢复衔接，以及无重放/纯内存会话只读历史兜底。
- 在 `acp-remote.toml` 提供可选 `[transcript_cache]` 配置（默认 **30 天**滑动保留、默认 **10 GiB** 容量上限），保证纯查看或无实质变更的后台重放不虚刷 30 天计时，保护活跃已加载会话与在途分块读取代际，并在确认原生删除时立即擦除对应快照。

**Non-Goals:**
- 将平台展示快照、命令、审批或审计拼接成文本重发给 Harness 伪造模型上下文。
- 解析 `~/.codex`、`~/.pi` 等任何厂商 CLI 私有磁盘目录或文件。
- 修改核心控制库 `node.sqlite3` 的表结构、引入新 Cargo SQLite 依赖，或将展示快照纳入核心管理事实备份包。
- 在本变更提前实现用户面向的原生删除授权协议（归属下游 `deliver-session-data-lifecycle`，本变更交付存储与协调器层的同步立即擦除端口及契约验证）。

## Decisions

### 1. 物理分库、配置契约、SQLite Schema 与故障/句柄隔离

1. **配置契约（`app/src/config.rs`）**：
   - 在 `ConfigFile`（保持 `#[serde(deny_unknown_fields)]`）新增可选子表 `transcript_cache: Option<TranscriptCacheConfigFile>`（内部同设 `#[serde(deny_unknown_fields)]`）：
     ```toml
     [transcript_cache]
     retention_days = 30          # 可选，默认 30，必须 >= 1 且毫秒换算不溢出
     max_bytes = 10737418240      # 可选，默认 10 GiB (10 * 1024^3)，必须 >= 1
     ```
   - 缺省 `[transcript_cache]` 时自动填充 `TranscriptCacheConfig { retention_days: 30, max_bytes: 10_737_418_240 }`，与全部既有 `acp-remote.toml` 配置文件 100% 兼容；传入 `0`、负数或未知键时返回明确 `AppError::Config`。

2. **物理隔离路径、既有 `sqlx` 复用、Windows ACL 与确定性关闭生命周期**：
   - `DataDirectory::transcript_cache_path(&self)` 定位于 `data_dir.join("transcript-cache.sqlite3")`（连同 `-wal` / `-shm` 附属文件）。不向 `node.sqlite3` 添加任何表，完全隔离 `app/src/session_storage/schema.rs` 的严格表白名单校验与控制面写锁争用。
   - **零新依赖复用 `sqlx::sqlite`**：直接复用仓库已带本地确定性关闭补丁（`vendor/sqlx-sqlite`，唯一链接 `libsqlite3-sys`）的既有 `sqlx` crate，为 `<data_dir>/transcript-cache.sqlite3` 建立物理独立的 `sqlx::sqlite` 连接池（独立单写者与有界读者，与 `node.sqlite3` 的 `SqlitePool` 完全分离），保持 `app/Cargo.toml` 与 `app/Cargo.lock` 在 `--locked` 门禁下零新增依赖。
   - 打开与 Checkpoint 时复用 `platform::protect_private_file` 对 `transcript-cache.sqlite3` 及其 `-wal`/`-shm` 施加当前进程 owner 独占 SDDL（拒绝符号链接/重解析点）。
   - 新增独立模块 `app/src/transcript_cache/`（含 `TranscriptCacheStore`、`TranscriptCacheWriter`、`TranscriptCacheCoordinator`），将关闭入口接入 `OwnedNodeSessionServices::shutdown`（`app/src/node/session_services.rs`）与 `NodeCloseOperation`（`app/src/node/lifecycle.rs`）：停止节点时先封存异步写入队列、等待后台刷盘与清理任务 join，再显式关闭独立 `sqlx::sqlite` 连接池并等待底层 SQLite worker 线程退出以释放全部 Windows 文件句柄，满足 `pool_close_tests` 临时目录即时无锁清理不变量。

3. **Schema 设计、协议版本记录与内容摘要防虚刷（`transcript-cache.sqlite3`）**：
   - 启用 `PRAGMA journal_mode = WAL`、`PRAGMA foreign_keys = ON` 与 `PRAGMA secure_delete = ON`（确保删除擦除时直接覆写 SQLite 数据页，不残留明文碎片）：
     - `transcript_cache_meta(id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL)`
     - `transcript_sessions`:
       - `source_id BLOB NOT NULL`（16 字节 `NativeSourceId`）、`native_session_id TEXT NOT NULL`，主键 `(source_id, native_session_id)`
       - `active_generation INTEGER NOT NULL`（单调递增快照代际 `>= 1`）
       - `protocol_version INTEGER NOT NULL CHECK (protocol_version IN (1, 2))`（捕获该代际时协商的真实 ACP 协议版本 `V1 = 1` / `V2 = 2`，供重启后无实时 `Connection` 时离线构造对应版本的 `AcpViewProjector`）
       - `content_digest BLOB NOT NULL`（32 字节 SHA-256，对 `protocol_version` 及有序规范化 `raw_json` 序列计算）
       - `last_updated_at_ms INTEGER NOT NULL`（30 天滑动过期基准时间：**仅在新增实时 `session/update` 落盘，或标准重放计算出 `new_digest != content_digest` 即存在实质内容新增/压缩/删改时刷新**；纯读取快照或重放确认 `new_digest == content_digest` 时严禁修改该列）
       - `last_synced_at_ms INTEGER`（最近一次完成原生标准重放校准的时间戳）
       - `capability_profile TEXT NOT NULL`（`'unknown' | 'replay_capable' | 'resume_only' | 'pure_memory'`）
       - `native_recovery_state TEXT NOT NULL`（`'normal' | 'native_unrecoverable'`）
       - `record_count INTEGER NOT NULL`、`byte_size INTEGER NOT NULL`（精确累计该会话当前代际原始帧与索引载荷字节数）
       - 索引：`idx_transcript_sessions_updated ON transcript_sessions(last_updated_at_ms ASC, source_id ASC, native_session_id ASC)`
     - `transcript_records`:
       - `source_id BLOB NOT NULL`、`native_session_id TEXT NOT NULL`、`generation INTEGER NOT NULL`、`ordinal INTEGER NOT NULL`、`connection_sequence INTEGER NOT NULL`、`raw_json BLOB NOT NULL`
       - 主键 `(source_id, native_session_id, generation, ordinal)`，外键级联关联 `transcript_sessions`
   - **离线版本感知还原（零重复投影逻辑）**：在 `app/src/adapters/acp/shared_view_projection.rs` 为 `AcpViewProjector` 提供基于存储 `Version`（`V1` / `V2`）的离线构造入口（不依赖存活 `Connection`），完整保留 `V1` 与 `V2` 在 `tool_call` vs `tool_call_content_chunk`、`state_update`、`_meta: null` 及 `messageId: null` 上的精确投影语义差异。读取快照时按 `ordinal ASC` 流式加载 `RetainedAcpRaw` 投入对应版本的 `AcpViewProjector` + `IndexedJournal` 构建只读 `TargetGeneration<RetainedAcpRaw>`，100% 复用既有 `OriginalRaw`、`ItemProjection`、`FieldEvidence`（9 槽工具原位更新、思考块、消息合并及子步骤原始结构引用）与前端 `RawJsonDocument` 不可信内容隔离边界。

4. **旁路故障隔离与告警降级**：
   - 实时 `session/update` 经有界非阻塞通道投递至 `TranscriptCacheWriter` 批量落盘；`SharedViewCapture`、队列派发（`QueueService`）、严格审批（`ApprovalOwner`）与容量账本（`SessionCapacity`）绝不等待快照磁盘锁。
   - 若 `transcript-cache.sqlite3` 初始化损坏、磁盘写满或运行时发生 SQLite I/O 错误：节点记录告警日志并将协调器置为 `CacheHealth::DegradedWriteWarning`，通过视图状态流向客户端推送持久化告警，同时继续保持内存 `SharedViews`、在途 `Run` 执行及权限审批裁决正常运转，绝不升级为 `SqliteStoreError` 控制面致命故障。

### 2. 三档混合自适应读取、后台校准与上下文恢复状态机

1. **统一客户端撤销门禁与来源元数据准入（`读取撤销`）**：
   - 无论是连接活跃内存 `SharedViews` 还是读取冷会话离线缓存视图（`CachedSharedViews`），`open`、`prepare`、`wait_change`、`rebind` 及 `QueuedSocketChunk::send` 均统一经由 `SocketViewAuthority` / `LocalViewAuthority` 在 `TrustStore::client_gate(client_id)` 锁保护下核验 `ApplicationPolicy::require_current_actor`。客户端一旦被撤销信任，实时视图与本地展示快照读取均立即返回 `ViewError::Revoked` 并以 `4410` 关闭连接。
   - **冷启动配置准入与 `NativeSourceId` 登记**：在未启动任何 Harness 子进程、占用 `0` 个主会话容量名额的前提下，冷会话打开（`view.open` 经 `QueueService::prepare_native_read` + `ReadConfigurationPort::resolve_read`，`view.open_source` 经 `SourceReadSelectionProof::mint` + `ReadConfigurationPort::resolve_source_read`）**必须先完成纯元数据级的目录、工作区、来源及 `profileRevision` 合法性核验**（拒绝过期或篡改的 `selection` 越权套取缓存快照），并将已验证的 `NativeSourceId` 注册至 `SocketViewAuthority` / `LocalViewAuthority` 的受权来源集合，确保零子进程冷启动场景下分块流读取正常通过 `current_guard` 校验。

2. **解耦冷快照直读与原生 `load_resource` / 50 名额分配**：
   - 重构 `NativeViewService`（`app/src/node/native_views.rs`）与 `NativeHistoryBridge`（`app/src/adapters/acp/native_views/replay.rs`）的打开决策树（同时覆盖 `view.open` 目标视图与 `view.open_source` 来源会话只读视图）：
     - **分支 1（节点已活跃加载该会话）**：若 `NativeCoordinator` 中对应 `NativeSessionKey` 已持有活跃 `ResourceSlot` / 实时 `SharedViews` 目标，直接绑定当前内存共享实时视图返回（`transcriptSource = "live"`），不重复发起重放，不额外消耗名额。
     - **分支 2（未加载冷会话 + 目录配置元数据核验通过 + 命中未过期本地快照 + `reason != ExplicitRefresh`）**：
       - **第 1 步（首屏零等待秒开）**：完成纯元数据 `resolve_read` / `resolve_source_read` 与 `Authority` 来源登记后，直接由 `TranscriptCacheCoordinator` 按快照 `protocol_version` 将 `transcript-cache.sqlite3` 当前代际离线装载为受管缓存发布槽（`ViewDescriptor::cache_recoverable = true`、`transcriptSource = "transcript_cache"`），**不启动 Harness 子进程、不调用 `OriginalAdmission` / `ReadCapacityAdmission` 占用 50 主会话名额、不调用 `CapacityActivity::observe` 刷新 30 分钟空闲计时**，立即向客户端返回 `view.opened` 并通过既有 `ACPV` 分块协议发送完整首屏快照与 `SnapshotEnd`。
       - **第 2 步（后台自动原生同步 / Stale-While-Revalidate）**：
         - 若已知该会话为 `pure_memory` 或 `native_unrecoverable`，不启动后台同步，直接标注只读不可继续边界；若已知为 `resume_only`，标注 `"展示平台已记录历史（外部终端直接产生的变更不可见）"` 且不因纯查看调用 `session/resume`。
         - 若对端支持标准重放（或首次未知需探测能力），协调器按 `NativeSessionKey` 单飞合并（Singleflight）在后台启动校准任务：连接源并检查协商 `Capabilities`（不执行 `load_resource` 无重放 `resume`）：
           - `capabilities.standard_replay == true`：按 `ReadCapacityAdmission` 申请主会话名额（若 49/50 以下直接分配；若 50 满额但存在已完成重放且无活跃执行/无待决审批的空闲候选，按 `CapacityPressure` 淘汰最旧空闲候选腾出名额；重放期间 `CapacityFreshness::History` 按本次实际接收时刻刷新 30 分钟空闲计时，但保持 `ObservedWork` 非活跃，重放结束后仍属可被满额淘汰的空闲候选）。
             - **重放成功**：计算 `new_digest` 原子写入 `transcript-cache.sqlite3` 新代际（仅当 `new_digest != old_digest` 时刷新 `last_updated_at_ms`），将最新重放视图发布为新 `ViewEpoch`，向已打开缓存流的客户端发出 `stream_rebound` 状态包触发平滑 `view.rebind` 及 `syncState = "synced"`。
             - **原生返回会话已删除/不存在或历史压缩**：若为压缩/删改后的有效重放，直接以最新权威重放替换快照代际；若对端明确返回会话不存在/不可恢复（`RemoteError`），更新 `native_recovery_state = 'native_unrecoverable'` 并向前端推送 `syncState = "native_unrecoverable"`，绝不用旧快照冒充最新已同步原生现状。
             - **容量 50 名额全被受保护会话占满（`SessionCapacityFull`）或 Harness 启动/传输暂不可达**：捕获错误而不关闭已秒开的缓存读取流，仅在当前缓存流追加一条 `RecordKind::Status`（`syncState = "degraded_capacity_full"` 或 `"degraded_unreachable"`），前端保持快照可读、轻提示未完成同步并开放手动重试按钮（`reason = "explicit_refresh"`）。
           - `!capabilities.standard_replay && capabilities.resume`：记录 `capability_profile = 'resume_only'`，推送状态更新告知仅展示平台历史快照且支持接管继续，不占用持久执行槽。
           - `!capabilities.standard_replay && !capabilities.resume`：记录 `capability_profile = 'pure_memory'`，推送状态更新标注连接结束后已不可继续恢复执行。
     - **分支 3（本地无有效快照 — 已过期/已淘汰/首次外部会话，或用户点击手动重试 `reason == ExplicitRefresh`）**：
       - 按标准原生读取路径申请名额并拉取历史，成功后重建 `transcript-cache.sqlite3` 快照；若无有效快照且原生重放失败（或无重放能力且快照已过期清除），明确返回读取错误或历史缺失状态，严禁读取 `node.sqlite3` 中的命令、队列、审批或审计表拼凑聊天记录。

3. **上下文恢复（`session/resume`）与本地快照无缝衔接**：
   - 当用户向未加载会话提交新输入（`QueueService` -> `NativeCoordinator::ensure_resource` -> `load_resource`）：
     - 若对端仅支持 `session/resume`（无重放），或支持重放但此前因满额降级尚未加载且当前通过轻量 `session/resume` 恢复上下文：节点严格仅调用原生 `Strategy::resume`（零快照文本回灌）。
     - 若原生 `session/resume` 返回错误（原生状态缺失），明确拒绝本次输入/恢复（`DispatchError::ProtocolRejected`），同时将 `native_recovery_state` 置为 `'native_unrecoverable'`，保留未过期的本地快照供只读查阅并标注原生不可恢复。
     - 若原生 `session/resume` 成功：在新建的实时 `TargetWorker` 初始化时，若 `transcript-cache.sqlite3` 存在该会话未过期快照，则先将缓存快照作为前序历史段（标记 `transcriptSource = "hybrid_resumed"`，诚实声明不含外部未观测变更）预载入 `TargetWorker`，再将后续新轮次产生的实时 `session/update` 连续追加到同一视图与快照库中，彻底消除回收恢复后的历史白屏；若对端同时支持标准重放，用户仍可随时通过 `explicit_refresh` 手动触发全量重放校准。

### 3. 30 天滑动过期、10 GiB 容量淘汰与原生删除同步擦除

1. **受保护会话判定（`ProtectedSessionsPort`）**：
   - 满足任一条件即受保护、跳过过期与容量淘汰：① `NativeCoordinator` 当前活跃持有该会话所属主会话容量槽（已加载活跃态）；② `TranscriptCacheCoordinator` 当前存在正在分块传输该会话快照代际的活跃读取租约（`SelectionLeases > 0`）。
2. **滑动过期与 LRU 容量淘汰执行时机**：
   - 节点启动完成时、后台周期清理定时器（每 5 分钟）以及每次快照新代际写入提交后异步检查：
     - **超期清理**：删除所有非受保护且 `now_ms.saturating_sub(last_updated_at_ms) > retention_days * 86_400_000` 的会话快照行及关联记录。
     - **10 GiB 水位淘汰**：若 `SUM(byte_size) > max_bytes`，按 `last_updated_at_ms ASC` 扫描 `transcript_sessions`，跳过当前受保护的活跃会话 X，整会话删除最旧的非活跃空闲会话 Y，直至总占用 `<= max_bytes`。
     - 所有自动清理仅操作 `transcript-cache.sqlite3`，绝不向 Harness 发送删除请求或触碰原生文件。
3. **原生删除即时同步擦除端口**：
   - 在 `TranscriptCacheCoordinator` 与 `NodeNativeViewsService` / `OwnedNodeSessionServices` 暴露 `purge_deleted_native_session(&NativeSessionKey)`：当确认原生会话删除成功时，同步执行事务删除 `transcript_records` 与 `transcript_sessions` 中对应 `(source_id, native_session_id)` 的全部数据（配合 `PRAGMA secure_delete = ON` 立即覆写清零底层页），同时立即封存并移除内存中该会话的任何缓存代际副本，不等 30 天到期。

### 4. 线上协议扩展与 Web 前缀保持无感补齐设计

1. **向后兼容的协议扩展（保持 `WV13`–`WV16` 独立浏览器夹具零改动）**：
   - 在 `clients/web/src/management/session-protocol/primitives.ts` 新增 `closedWithOptional(value, requiredShape, optionalShape)`（严格拒绝任何未声明的未知键，同时允许省略可选键）。
   - 在 `view-and-approval.ts` 的 `SessionViewOpened["status"]` 增加可选字段（Rust `opened_view_payload` 对缓存/混合状态按需输出，既有模拟对端省略时完全合法）：
     - `transcriptSource?: "live" | "transcript_cache" | "hybrid_resumed"`
     - `syncState?: "idle" | "revalidating" | "synced" | "degraded_capacity_full" | "degraded_unreachable" | "native_unrecoverable"`
     - `capabilityBoundary?: "unknown" | "replay_capable" | "resume_only" | "pure_memory"`
     - `continuable?: boolean`
     - `cacheWarning?: boolean`
   - 在 `ACPV` 二进制流的 `RecordKind::Status` 中新增状态记录类型 `{"type": "transcript_cache_state", "transcriptSource": "...", "syncState": "...", "capabilityBoundary": "...", "continuable": bool, "cacheWarning": bool}`，由 `clients/web/src/management/session-view-status.ts` 解析并实时更新视图横幅（无需断开重连即可将后台同步中、满额降级、能力边界或写盘告警即时呈现给用户）。

2. **跨代际/跨 `segmentId` 暂存对账与零闪烁前缀补齐（消除 `rebind` 双视图跳动）**：
   - 现有 `SharedViewController.rebindSharedView` 在收到 `stream_rebound` 时会向 `#session-view-cache` 挂载第二个 `<section>` 并在 `snapshot_end` 时销毁旧 `<section>`，且不同 `ViewEpoch` 会产生不同 `segmentId`（缓存流 `Seg_1` -> 实时重放流 `Seg_2`），直接替换会导致页面闪烁与滚动位置丢失。
   - **改进方案**：
     - 在 `ProjectionCollection`（`clients/web/src/views/projection-collection.ts`）、`ViewContentStore`（`content-store.ts`）与 `Timeline`（`timeline.ts`）实现跨 `segmentId` 代际原位重绑协调（`reconcileGeneration`）：
     - 当 `SessionViewDisplay` 触发 `stream_rebound` 重绑至新 `streamId` 时，新流在后台离屏暂存器（staging `ViewRecordAssembler` + staging `ViewContentStore`）中接收分块，**不挂载第二个 `<section>`**，保持当前已秒开的本地快照 DOM 与阅读滚动位置稳定不动。
     - 当新流收到 `snapshot_end` 时，将已挂载的主 `Timeline` 实例从旧缓存段 `Seg_1` 原位重绑至新权威段 `Seg_2`（更新 `timelineElement.dataset.segmentId = newSegmentId`、`section.dataset.streamId = reopened.streamId`，并替换 `SessionViewDisplay.#timelines` 映射，严格保持 `#session-view-cache > [aria-label='共享会话视图']` 唯一节点契约）：
       - **前序结构一致（外部 CLI 追加新对话）**：按 `(anchorId, firstPosition, kind)` 结构身份与解析后字段摘要（`bodyKey`）比对前序条目，未变更的前序 `<li class="acp-timeline__item">` DOM 节点引用（`===`）100% 保持原位不销毁、不重排（天然保持 `window.scrollY` 阅读位置与文本选区），仅在 `<ol class="acp-timeline__items">` 末尾追加外部新增轮次卡片；
       - **原生发生上下文压缩或删改**：原位更新变更项，并从 `ProjectionCollection` 与 `Timeline.#cards` / `<ol>` 中精确移除新权威快照已不存在的尾部或被压缩卡片，确保不残留过期旧快照内容。

3. **Web 分层方向合规与 650 行模块预算控制**：
   - 严格遵守单向无环分层 `protocol/views -> pairing/management -> workbench -> main.ts`：在 `management` 层新建纯 DOM 状态条辅助模块 `clients/web/src/management/transcript-cache-banner.ts`（零顶层 DOM 副作用、零 `innerHTML`），封装快照来源提示、满额/异常降级轻提示、`"重试同步"` 按钮（触发 `reason = "explicit_refresh"`）、无重放/纯内存/原生不可恢复边界提示及写盘告警渲染。
   - `SessionViewDisplay`（`src/management/session-view-display.ts`）与 `source-session-view.ts`（`src/management/source-session-view.ts`）在同层 `management/` 内直接调用 `transcript-cache-banner.ts`，上层 `shared-view-controller.ts`（当前 517 行）与 `source-session-controller.ts`（当前 637 行）仅透传轻量刷新回调，绝不产生 `management -> workbench` 反向依赖，保证全部 `src/workbench/*.ts`、`src/management/session-protocol/*.ts` 及新增模块严格 `<= 650` 行，且 `FIXTURE-SHA256.md` 6 个浏览器夹具字节级不变。

## Risks / Trade-offs

- **[打开支持重放的冷会话自动后台拉取可能唤醒子进程或竞争名额]** → 首屏由 `transcript-cache.sqlite3` 零等待直接响应；后台同步按 `NativeSessionKey` 单飞合并，完成后无活跃执行仍属可随时被 `CapacityPressure` 淘汰的空闲候选；50 名额全被受保护会话占满时自动优雅降级为纯快照只读浏览并保留手动重试。
- **[后台重放与纯查看意外续期导致冷快照永不过期]** → 在 `transcript_sessions` 存储规范化 `content_digest`，仅当实时新帧写入或原生重放检测到实质内容变化时才更新 `last_updated_at_ms`；纯打开快照与无变化后台校准绝不刷新 30 天滑动计时。
- **[跨流换代（`Seg_1` 快照流切入 `Seg_2` 重放流）引发 DOM 重建闪烁与阅读位置跳动]** → 采用离屏暂存直至新代际 `snapshot_end` 后对单个挂载主 `Timeline` 执行跨 `segmentId` 原位重绑与前缀保留对账，前序相同卡片 `<li>` 引用零变动、新增轮次平滑长出在末尾，压缩删改项精确裁剪。
- **[Windows 下独立 SQLite 连接池在节点关闭时残留句柄阻塞目录释放]** → 复用既有带确定性关闭补丁的 `sqlx::sqlite` 连接池并将 `TranscriptCacheStore::shutdown` 纳入 `OwnedNodeSessionServices::shutdown` 统一关闭序列，确保后台写任务与底层 SQLite worker 在节点退出前确定性释放全部文件句柄。

## Migration Plan

节点在 `data_dir` 下按需使用既有 `sqlx::sqlite` 自动初始化独立的 `transcript-cache.sqlite3`（应用 owner 独占 ACL），`acp-remote.toml` 缺省 `[transcript_cache]` 时自动启用 `30` 天滑动保留与 `10 GiB` 上限，对既有 `node.sqlite3` 核心管理库零 Schema 变更、零新依赖、零数据迁移风险；升级前已存在的历史原生会话在首次打开重放或继续对话时自然建立本地展示快照，回滚或删除 `transcript-cache.sqlite3` 不影响任何身份、审批、队列或原生会话权威数据。
