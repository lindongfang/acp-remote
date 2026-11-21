# 展示快照协调实现

本文件说明 `deliver-session-transcript-cache` 的混合自适应展示快照协调、原生恢复状态机与删除擦除实现，存储结构与 30 天 / 10 GiB 淘汰规则见 [独立展示快照存储实现](transcript-cache-storage.md)。

## 职责边界

- **协调状态机与单飞校准**（[coordinator.rs](../../app/src/transcript_cache/coordinator.rs)）：
  - `TranscriptCacheCoordinator` 协调内存实时视图、离线本地展示快照、`AcpTranscriptTap`、单飞（Singleflight）后台重放校准以及 `ProtectedSessionsPort`（活跃 Native Slot 或在途分块租约 `SelectionLeases > 0` 保护）。
  - 实时流 tap 在后台有界通道持久化每条 `session/update`；满额或底层 SQLite I/O 失败时切入 `CacheHealth::DegradedWriteWarning`，向客户端推送 `cacheWarning = true` 状态记录，不阻断实时会话执行、输入排队与严格审批。
- **三档混合读取决策树与零放行门禁**（[native_views.rs](../../app/src/node/native_views.rs)、[read_source.rs](../../app/src/application/native_dispatch/read_source.rs)）：
  - **步骤 0（纯元数据门禁）**：所有冷/热打开请求均先执行客户端信任校验与纯元数据 `verify_cold_target_read` / `verify_cold_source_read`，在任何快照查询、子进程拉起或名额占用前拒绝篡改或过期的选择凭证。
  - **分支 1（已加载活跃会话）**：直接绑定当前共享实时视图，不发起重复重放、不消耗额外名额（标记 `transcriptSource = "live"` 或 `"hybrid_resumed"`）。
  - **分支 2（未加载冷会话 + 命中有效未过期快照 + 非显式刷新）**：零等待离线挂载 `SharedViewOwner::open_cached_target`（0 子进程、0 主会话名额、不续期 30 分钟空闲计时，置 `cacheRecoverable = true`），并按 `capability_profile` / `native_recovery_state` 决定后续行为：
    - `ReplayCapable` / `Unknown` 时按 `NativeSessionKey` 单飞在后台先经 `initialize` 探测能力（绝不因纯查看触发 `session/resume`），确认标准重放后申请 50 名额（含满额空闲候选回收）执行一次最新重放，通过内容摘要比对写入并触发 `stream_rebound` 无感重绑至实时流（`syncState = "synced"`）；
    - 容量全满或传输不可达时保留缓存可读并追加 `RecordKind::Status` 轻提示（`degraded_capacity_full` / `degraded_unreachable`）；
    - 确认远端原生缺失时持久化并推送 `native_unrecoverable`；`ResumeOnly` / `PureMemory` 则诚实标注只读边界与是否可继续接管。
  - **分支 3（无有效快照或显式刷新）**：走标准原生重放通道重建快照；失败时明确报错，不从 `node.sqlite3` 拼装聊天历史。
- **上下文恢复预载与原生删除同步擦除**：
  - `NativeCoordinator::load_resource` 仅向对端发送原生 `Strategy::resume`；若成功且未执行全量重放，先将未过期快照预载为前序历史段（`transcriptSource = "hybrid_resumed"`），再追加后续实时更新。若对端明确拒绝恢复，立即持久化 `native_unrecoverable` 并返回 `ProtocolRejected`。
  - `purge_deleted_native_session(&NativeSessionKey)` 同步终止在途单飞、封存内存缓存代际并立即清零擦除 `transcript-cache.sqlite3` 与 WAL 明文页。

## 历史分页的来源边界

静态历史页在同一个 SQLite 读事务中选择代际、轮次、原始帧和工具补丁。离线翻页期间，读流持有选择租约。命中快照即可在原生进程停止后读取，也可在原生会话不能重放时读取上次留存的内容。快照淘汰后，重新定位需要当前内存视图或原生重放能力。实时流收到无法归入轮次的后续记录时，以 `history_unprovable` 通知客户端重开完整视图。

首次重放仍须通过主会话名额准入。只有发现记录和平台注册目标、缺少已验证根身份的外部会话，在快照缺失时会收到 `session_tree_unverified`。已验证根会话以及根树下已有关系证明的子目标分别沿用原有准入路径。
