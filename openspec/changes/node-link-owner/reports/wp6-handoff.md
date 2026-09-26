# WP6 Handoff — 命令管线与撤销传播（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.17（命令管线与终态映射）、2.18（`session.create` 硬约束与命令限流）、2.19（撤销传播）、2.20（交付前局部验证）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。本轮为**恢复轮次**：上一轮在 30 分钟上限处被宿主终止（非代码问题），工作区改动完整保留，本轮从「重读目标区域 → 分三批提交 → 全量门禁 → 报告」继续。
- **target_revision**：`8bc2ff3`（HEAD，含本 WP 全部提交）；起点 `5e2d17c7ac4074b1d473dffab664eed48f48f21e`（WP5 之后）。同批次提交按依赖顺序：
  - `3638ebd` **fix(core)**：Owner 侧节点命令的授权判定修正 + `record_node_link_auth` 接受动作集扩宽 + `docs/CORE_PORTS_AND_STORAGE.md` §4/§6 第 5 条/§10 同步（5 文件）；
  - `bbf042e` **feat(server)**：Node Link 命令管线 `node_link/command.rs` + 25 条行为用例 + `resource.rs` 的两个 additive 公开方法 + `mod.rs` 接线（4 文件）；
  - `4cbdf8a` **feat(server)**：`export.revoke` 接进 `ConnectionCloser` 通知缝（`pairing.rs`/`router.rs`/`test_support.rs`/`app/compose.rs`）；
  - `8bc2ff3` **test(node-link)**：按 R45 的下调实例（limit = 8）覆盖 in-flight 上限的用例实例调整。
  - **提交边界为何是这样**：`BrokerDeps` 新增必需字段、`ConnectionCloser` 的 trait 扩展各自是**编译单元**（core 的字段与 `app`/`test_support` 的构造点必须同批；trait 扩方法必须与 `NoConnections`/`RecordingCloser`/`RevocationCloser` 三个实现同批）。因此按「core 授权面 → 命令管线 → 撤销缝」切三批，而不是按任务号切；每批都在**提交前**跑过 fmt + `npm run check` + clippy（pre-commit）与相应测试，三批的树各自可编译。
- **scope**：
  - 新增：`crates/server/src/node_link/command.rs`（约 2 100 行，含模块契约注释）、`crates/server/src/node_link/command/tests.rs`（25 条用例）；
  - 修改：`crates/server/src/node_link/mod.rs`（`pub mod command` + `pub use command::CommandRoute`）、`crates/server/src/node_link/resource.rs`（两个 additive 公开方法 + `session_meta` 改 `pub(crate)`）、`crates/server/src/local_admin/{pairing.rs,router.rs,test_support.rs}`（撤销缝）、`crates/core/src/{broker.rs,use_cases.rs}`、`crates/app/src/compose.rs`（**编译连带**，见专节）、`docs/CORE_PORTS_AND_STORAGE.md`；
  - **未改**：`openspec/**`（不改权威规划工件）、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/server/src/transport/**`、`node_link/conn/**`（WP4 冻结形状未动——终态推送只用 `ConnectionHandle::send`/`request_close`）、`local_admin` 其它方法的既有行为、`storage-sqlite`（本 WP 未触及存储合同）。
- **changes**：见「改动与需求映射（R66–R78、R82/R83）」与「冻结的公开形状（供 WP7）」。
- **checks**：`[PV3]`（本机轮次，两轮记录都在日志里；最终修订 `8bc2ff3` 一轮为准）——
  - `cargo fmt --all -- --check` 退 0；
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 退 0；
  - `cargo test --locked --workspace --all-features` 退 0：83 个测试目标、0 failed、0 全跳过（`server` lib 276 通过，其中 `node_link::command::tests` 25、`local_admin::router::tests` 35；`core` lib 108 通过）；两处 `1 ignored` 是 `storage-sqlite` 既有的 `#[ignore]`（`commit.rs:1092` 的子进程探针、`migration.rs:970` 的夹具生成器），与本 WP 无关；
  - `rg -n "unsafe" crates/server/src` 0 命中（workspace 级 `unsafe_code = "forbid"` 亦由 clippy 兜底）；
  - `npm run check` 退 0（含合同漂移门禁：§5 的 `core::ports` 签名与 §7 的 v1 表结构逐条一致——本 WP 未动 §5/§7）。
  - 原始输出：`reports/wp6-command.log`。
- **issues**：无阻断项。有 7 条需要主 Agent / reviewer 知晓的事实，逐条列在「需要主 Agent / reviewer 知晓的设计事实」。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.12）、`[PV5]`/E2E（3.11、2.22）或合并已完成。
- **evidence_paths**：`reports/wp6-command.log`、本文件。
- **resource_cleanup**：用例不起 listener、不绑端口、不起后台进程、不用数据库服务/容器；端口替身（`CommandStore`/`FakeBackends`/`FakeEndpoint`/`TestCatalog`）与 `TestWorld` 的 fake 端口都在用例结束即丢弃；`ConnectionHandle` 由 `pub(crate)` 构造器直接构造并随 `Fixture` 释放；命令终态观察循环（`CommandRoute::dispatch`）由**组合根** spawn（WP7），用例直接调 `poll_pending()`，因此没有 detached task；`session.create` 的 workspace alias 只读 `std::env::temp_dir()` 的元数据、不创建目录；`target/` 作为缓存保留（被 git 忽略）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.17"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "8bc2ff3"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp6-command.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = 8bc2ff3（git status 为空）上的本机轮次：cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0；cargo test --locked --workspace --all-features 83 目标 0 failed（server lib 276，其中 node_link::command::tests 25）；rg unsafe crates/server/src 0 命中；npm run check 退 0。R66–R70 的逐条覆盖见本文件「改动与需求映射」。R66/R67/R69 的 [PV5] 轮次（跨实现 + 本机轮次）由 3.11/2.22 执行，本行只声称 [PV3]。core 侧授权判定（node_allowed）的用例随 3638ebd 一并提交（core lib 108 通过，含新增的 owner_side_node_commands_without_a_session_use_the_trust_record_and_a_covering_export 与扩宽后的 node_link_auth_audit_is_narrow）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.18"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "8bc2ff3"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp6-command.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮检查。node_link::command::tests 覆盖 R71（the_command_rate_limit_replies_rate_limited_and_then_closes：120 次内按越权拒绝、第 121 次回 nodelink.resource.rate_limited + retryable=true + details.retryAfterMs、首次超限不关连接、连续三次超限以 4429 关闭）、R72（四键白名单与绝对路径在严格解码前判定、sessionRef/attachmentId/attachmentGeneration/expectedVersion 必为 null、零参数 template、accepted(result=null) → terminal(SessionCreateResult)）、R73（session_create_returns_accepted_then_the_composite_result：Owner 生成的 sessionId + sessionMeta）、R74（禁带字段 → unsupported_field + details.field，commit 调用数为 0）、R75（未知 workspaceAlias → not_granted + details.parameter，未导出 agent 同判）。R45 后半的 in-flight 上限实例随 8bc2ff3 调整（limit=8 → 第 9 个被拒）。R72/R73 的 [PV5] 轮次由 3.11/2.22 执行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.19"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "8bc2ff3"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp6-command.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮检查。R76（local_admin::router::tests::export_revoke_notifies_the_connection_closer_after_the_persisted_commit：提交后才通知、重试不重复通知；node_revocation_notifies_then_closes_with_4410；export_revocation_clears_attachments_and_rejects_later_commands）、R77（撤销后清内存 attachment 与订阅、推 export.revoked、该 Export 上的新命令按持久化记录被拒）、R78（node.trust.revoked 推送 + 4410 关闭 + 观察表随连接消失）。既有 local_admin 用例全绿（server lib 276 通过，含 35 条 router 用例），缝扩展不改变 close_device/close_node 的行为。R76/R77/R78 的 [PV5] 轮次由 3.11/2.22 执行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.20"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "8bc2ff3"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp6-command.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "2.20 的完成条件是「[PV3] 全绿且无零用例/全跳过」：命令与完整输出见日志；83 个目标 0 failed，两处 1 ignored 是 storage-sqlite 既有的 #[ignore]（探针与夹具生成器），没有任何目标全跳过；npm run check 未因本变更变红。工作区在提交后 git status 为空。"
    source_evidence: NOT_APPLICABLE
```

## 改动与需求映射（R66–R78、R82/R83）

用例全部在 `crates/server/src/node_link/command/tests.rs`（除注明者）。

| 需求 | 覆盖点（用例名） |
|---|---|
| **R66** 命令授权、幂等与终态 | `the_error_registry_maps_core_codes_without_inventing_new_ones`（12 条命令 → `required_grant` 全覆盖、词表外不发新错误码）；`an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`（授权交集与 `command.rejected(nodelink.export.not_granted)`、无副作用）；`the_request_fingerprint_is_acpr_cj1_over_the_decoded_payload`（幂等键的语义面）；`command_status_replies_the_terminal_of_the_mutation`（两种 `command.status` 形式产生相同形状、`terminalEventId` 非空、`command` 名取被查询 mutation）；`command_status_replies_accepted_for_an_unfinished_mutation`（未终结 → 同形 `command.accepted`，`acceptedAt` 非 null）；`terminal_mapping_keeps_the_command_name_and_the_terminal_contract`（`completed` 带非空 `result`、其余 status 带 `error`、`rejected` 无终态事件）；`the_watcher_pushes_the_terminal_once_the_record_is_terminal`（终态经观察表推送）；`session_create_null_rules_and_expected_version_are_enforced_at_the_wire_boundary`（session-scoped 命令必须携带代际、`session.mode.set`/`session.config.set` 必须带 `expectedVersion`）；`the_in_flight_limit_is_enforced_per_connection`（in-flight 上限 + 越限的 `retryAfterMs`）；`the_command_rate_limit_replies_rate_limited_and_then_closes`（120/分钟 + 4429）；`export_revocation_clears_attachments_and_rejects_later_commands`（撤销后 `command.submit` 被拒）。core 侧：`use_cases::tests::owner_side_node_commands_without_a_session_use_the_trust_record_and_a_covering_export`。 |
| **R67** 相同 requestId 重试不重复派发 | `a_repeated_request_id_replays_the_first_terminal`（mutation：命中 core 幂等 → 回首次终态，`commit` 调用数为 0）；`a_repeated_session_create_request_replays_the_first_result`（`session.create`：回首次终态且只创建一次，`commit` 调用数为 1）。 |
| **R68** 同键不同语义被拒绝 | `the_same_request_id_with_a_different_payload_conflicts`（mutation：`nodelink.command.idempotency_conflict`、不再派发）；`a_repeated_session_create_request_replays_the_first_result` 的第三段（`session.create`：同 requestId 换合法 alias → `idempotency_conflict`）。 |
| **R69** 越权命令被拒绝且留痕 | `an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`：`grant.observe` 的节点提交 `session.prompt` → `command.rejected(nodelink.export.not_granted)`、`commit` 调用数为 0；审计表里恰好一条 `authorization.denied`（`outcome = denied`、`via_node = 该对端`、`localPrincipalRef = null`）。 |
| **R70** 崩溃窗口进入 uncertain | `an_uncertain_terminal_is_passed_through`：持久化记录 `uncertain` → `command.terminal{status: uncertain}`，`error.code = nodelink.command.uncertain`、`result` 为 null，不猜成功或失败。 |
| **R71** 命令限流 | `the_command_rate_limit_replies_rate_limited_and_then_closes`：前 120 次正常（按越权拒绝）、第 121 次 `link.error(nodelink.resource.rate_limited, retryable = true, details.retryAfterMs)`、首次不关连接、连续三次超限 `request_close(4429)`。 |
| **R72** `session.create` 硬约束与结果契约 | `forbidden_session_create_fields_are_recognised_before_the_typed_decode` + `absolute_path_shapes_cover_posix_windows_and_unc`（白名单/绝对路径/超长键→`<unknown>`）；`session_create_with_template_parameters_is_rejected`（非空 `templateParams` → `unsupported_field`，空对象与省略都放行）；`session_create_null_rules_and_expected_version_are_enforced_at_the_wire_boundary`（四个字段必为 null）；`session_create_returns_accepted_then_the_composite_result`（`accepted(result = null)` → `completed(SessionCreateResult)`）。 |
| **R73** 正常创建并回传复合引用 | `session_create_returns_accepted_then_the_composite_result`：`remoteSessionRef.ownerNodeId` 是本机 id、`exportId` 是请求的 Export、`sessionId` 由 Owner 在提交事务内生成（≠ 任何请求带来的 id）、`sessionMeta{state: idle, version: "1"}`；`commit` 调用数为 1。 |
| **R74** 禁带字段被拒绝且不创建会话 | `session_create_forbidden_fields_are_rejected_without_creating_a_session`：`cwd`/`mcpServers` 各一次 → `command.rejected(nodelink.command.unsupported_field)` + `details.field` 精确等于被拒键；两次都断言 `commit` 调用数为 0。 |
| **R75** 未知 workspaceAlias 被拒绝 | `session_create_with_an_unknown_workspace_alias_is_rejected_with_the_parameter`：未知 alias → `nodelink.export.not_granted` + `details.parameter = "workspaceAlias"`；未导出 agent → 同码 + `details.parameter = "agentId"`；两者都不创建会话。 |
| **R76** 撤销的即时传播（Owner 共同面） | `local_admin::router::tests::export_revoke_notifies_the_connection_closer_after_the_persisted_commit`（提交成功后才通知、重试不再通知）；`node_revocation_notifies_then_closes_with_4410`；`export_revocation_clears_attachments_and_rejects_later_commands`。 |
| **R77** Export 撤销即断资源 | `export_revocation_clears_attachments_and_rejects_later_commands`：受影响的连接收到 `export.revoked{exportId, revokedAt}`（时间取持久化记录）、`current_attachment_session_ref` 变 `None`（内存订阅已清）、此后该 Export 上的 `command.submit` 按当次持久化记录被 `nodelink.export.not_granted` 拒绝（推送丢失也不影响判定）。 |
| **R78** 节点撤销即关连接 | `node_revocation_notifies_then_closes_with_4410`：`node.trust.revoked{revokedNodeId, revokedAt, reason}` + `request_close(4410)`；观察表随连接消失；重连在凭据状态检查处被拒属 WP4 的既有用例（`conn` 的 `nodelink.auth.node_revoked`）。 |
| **R82** 审计与日志边界（命令侧） | `an_unauthorized_command_is_rejected_audited_and_has_no_side_effect`（审计行只记 `viaNodeId`，`localPrincipalRef` 为 null）；`the_error_registry_maps_core_codes_without_inventing_new_ones`（错误码只取 registry 词表，未登记细分回落 `nodelink.internal.unavailable` 并只进结构化日志）；`forbidden_session_create_fields_are_recognised_before_the_typed_decode`（`details.field` 只放字段名；超过 128 字符的键回 `<unknown>`，不把输入回显进错误体）。 |
| **R83** 拒绝留痕且不含秘密 | 同上；本 WP 的日志字段只有 `access_node_id`/`connection_id`/`request_id`/`command`/错误码与 `PortError` 的枚举形状，没有 prompt 正文、凭据值、私钥或 `pairingSecret`（命令内容只在 `core_payload` 的入参里，不落日志）。 |

不在本 WP 范围、但被本 WP 的文件改动触及的既有需求：R60–R65（WP5 的 resource/扇出）只用 `resource.rs` 的两个 additive 方法，WP5 用例全绿即回归证据；`local_admin` 的方法行为（R14–R18 面）不变，35 条 router 用例全绿。

## 冻结的公开形状（供 WP7）

```rust
// crates/server/src/node_link/command.rs —— 组合根装配与生命周期
impl CommandRoute {
    pub fn new(
        core: Arc<UseCases>,
        registry: Arc<ConnectionRegistry>,
        resource: Arc<ResourceRoute>,
        authority: Arc<Authority>,
    ) -> Self;
    pub fn node_id(&self) -> &NodeId;

    /// 终态观察循环：组合根 spawn，`Shutdown` 参与关闭序列。
    pub async fn dispatch(self: Arc<Self>, shutdown: Shutdown);

    /// `node.revoke` **持久提交成功后**调用；返回被通知/关闭的连接数。
    pub async fn node_revoked(&self, node: &NodeId) -> usize;
    /// `export.revoke` **持久提交成功后**调用；返回收到 `export.revoked` 的连接数。
    pub async fn export_revoked(&self, export: &ExportId) -> usize;
}

// 固定常量（§2.5；不可配置）
COMMANDS_PER_MINUTE = 120;      COMMAND_WINDOW = 60s;
MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS = 3;   // 连续超限 → 4429
TERMINAL_POLL_INTERVAL = 250ms; WATCH_MAX_AGE = 600s;
MAX_TRACKED_SESSION_CREATES = 1024;          // session.create 进程内幂等表上限

// crates/server/src/node_link/mod.rs
pub mod command;
pub use command::CommandRoute;

// crates/server/src/node_link/resource.rs —— additive 公开方法（D7/D8 的缝）
impl ResourceRoute {
    /// session-scoped 命令复核「当前 attachment」；不匹配一律返回 None。
    pub fn current_attachment_session_ref(
        &self, handle: &ConnectionHandle, attachment_id: &Uuid, generation: u64,
    ) -> Option<RemoteSessionRef>;
    /// 撤销 Export 后清除内存 attachment/订阅，返回受影响的 connectionId（有序）。
    pub fn revoke_export(&self, export: &ExportId) -> Vec<String>;
}

// crates/server/src/local_admin/pairing.rs —— 撤销通知缝（D7）
#[async_trait::async_trait]
pub trait ConnectionCloser: Send + Sync {
    async fn close_device(&self, device: &DeviceId);
    async fn close_node(&self, node: &NodeId);
    /// `export.revoke` 提交后通知持有该 Export 的活跃连接；推送失败不回滚已提交的撤销。
    async fn export_revoked(&self, export: &ExportId);
}
impl PairingSessions {
    pub async fn export_revoked(&self, export: &ExportId);
}

// crates/core/src/broker.rs —— 授权面（`3638ebd`）
pub struct BrokerDeps {
    /* 既有字段… */
    pub trust: Arc<dyn TrustStore>,   // 新增且**必需**：Owner 侧授权不能只靠 Export 记录
}
```

**WP7 接线清单**（三条，缺任一条本 WP 的行为都不会对 Access 生效）：

1. **路由**：把命令路由挂进 `node_link::Routes`，并把 `Routes` 交给 `NodeLinkConn::with_route`：
   `Routes::new(vec![Arc::new(CatalogRoute::new(core.clone())), resource.clone(), command.clone()])`
   （`Routes` 按序询问、第一个认领的胜出；`CommandRoute` 只认领 `command.submit` 与 `command.status`，顺序不敏感）。
2. **终态推送**：`tokio::spawn(Arc::clone(&command).dispatch(shutdown.clone()))`，并把该任务纳入关闭序列（`Shutdown` 触发即退出，无 detached task）。
3. **撤销缝**：`app` 的 `ConnectionCloser` 实现改为持有 `Arc<CommandRoute>` 与连接注册表，在
   `close_node(node)` 里 `command.node_revoked(&node).await`、在新的 `export_revoked(export)` 里
   `command.export_revoked(&export).await`（当前 `RevocationCloser` 只记日志，见专节）。
   注意 `local_admin` 已经保证只在持久提交**之后**调用这两个方法。

## Owner 侧无会话命令的授权规则（主 Agent 要求单列）

`session.list`/`command.status`/`session.create` 没有目标会话，因此 `Broker::node_allowed` 不能靠「该会话的 agent 属于某个 Export」来判定（这正是修正前的死结：`session = None` 一律 `false`，三条命令对 Owner 侧节点恒拒）。`3638ebd` 把 Owner 侧判定定为两级：

1. **信任记录**：该对端存在 `access` 角色信任行、`state == Paired` 且 `grants` 含 `required_grant(command)`（命令 → grant 的映射表见 `crates/core/src/broker.rs::required_grant`：`session.list`/`session.read`/`command.status`/`session.mode.list`/`session.config.list` → `grant.observe`，`session.prompt`/`session.cancel`/`elicitation.respond` → `grant.interact`，`session.mode.set`/`session.config.set` → `grant.configure-session`，`permission.resolve` → `grant.approve`，`session.create` → `grant.remote-work`）。缺行、未配对或 grants 不含该值 → 失败关闭。
2. **Export 覆盖**：存在**未撤销**的 `ExportRecord`，其 `scopes` 含该 grant，且 `scopes ∩ 该信任行 grants ≠ ∅`（与 Node Link 可见性 `catalog::visible_exports` 同源）。会话命令（`session.*` 中需要会话的那些、`permission.resolve`、`elicitation.respond`）另外要求该 Export 覆盖目标会话的 agent（会话不存在也失败关闭）；**无会话命令只要前两条**。

因此有效权限仍是「Export grant ∩ 信任记录 grant」，只是无会话命令没有第三支可比对。为此 `BrokerDeps` 新增必需的 `Arc<dyn TrustStore>`（授权判定必须能读信任行，不能只看 Export 记录），`docs/CORE_PORTS_AND_STORAGE.md` §4/§6 第 5 条/§10 已同步（§5 的端口签名与 §7 的表结构未动，漂移门禁仍绿）。

**本层不做的事**（主 Agent 裁定的边界）：`session.list` 的结果过滤（哪些会话对这个节点可见）与 `session.create` 的参数校验（`agentId`/`exportId`/`workspaceAlias` 是否属于该 Export）**仍由 `server::node_link` 承担**——core 只判授权，不投影结果、不做参数级授权。这正是下面 §「session.list 的 adapter 过滤」的实现依据。

## session.list 的 adapter 过滤（主 Agent 要求单列）

`core::use_cases::list_sessions` 返回本机**全部** owned 摘要（存储层只按 `query` 取行，授权过滤在用例层完成，而用例层只知道「这个节点能不能调 `session.list`」，不知道「这条会话属于哪个 Export」）。因此可见性过滤落在适配层，且**复用同一个判定点**：

- `CommandRoute::visible_sessions(node, summaries)`：
  1. 取 `core.node_link_catalog_view(node)`；
  2. 用 `catalog::visible_exports(view.node, view.exports)`（**唯一的可见性判定点**，D14）收集可见 Export 覆盖的 `agentId` 集合；
  3. 只保留 `summary.agent().agent_id()` 在该集合内的会话，其余**静默从结果里去掉**——它们不是错误，只是这个节点看不到（不过滤会泄露未导出 agent 的会话元数据）；
  4. 按 `sessionId` 升序排序（`session.list` 没有顺序合同；固定顺序让对端与用例都可复现）；
  5. 投影成 Sync 的 `sessionListResult`（§11.4 共享合同，Node Link 不另立形状）；投影失败的会话只记结构化日志并跳过，不伪造字段。
- 与其它路径的一致性：同一份 `visible_exports` 同时决定 `catalog.snapshot`（WP5）、`resource.attach`（WP5 的 `export_is_visible`）与命令管线的授权交集，因此「catalog 里出现的 Export」与「`session.list` 里出现的会话」不可能出现不同结论。
- 已知精度边界：过滤只比 `agentId`（Export 的清单），不比 `exportIds` 维度——后者是已登记的后续阶段待办（D14 的用户裁决 (b) 口径），与 WP5 的 catalog 完全同源。

## `crates/app/src/compose.rs` 的连带改动（正式属 WP7）

`4cbdf8a` 改了这个文件，但**它的正式写范围是 WP7（2.21/2.23）**。原因只有一条：`ConnectionCloser` 是服务端定的端口，实现写在组合根，trait 一扩方法，`app` 不补实现就编不过。改动本身刻意做到最小且不改变行为：

- `RevocationCloser` 新增 `export_revoked`：与既有的 `close_node` 同款——只记一条结构化事件
  （`daemon.export_revoked_notification`，`target_kind = "export"`、`notified_connections = 0`）并明确「本切片没有按 Export 持有的连接表」，不假装通知成功；
- `close_node` 的日志措辞随动（「Node Link 未落地」→「未接线」），语义不变；
- 另加 `ExportId` 的 import 与 `BrokerDeps.trust` 一行（`trust_store` 是组合根已有的字段）。

**WP7 必须替换它**：撤下这条「没有连接表」的日志实现，改为持有 `Arc<CommandRoute>` 并调用
`command.node_revoked`/`command.export_revoked`（见上一节的接线清单第 3 条），否则撤销只会留日志、不会真的推送与关闭连接。若主 Agent 认为该文件本阶段不应出现 WP7 之外的改动，可在 2.21 里整体重写该实现，本文件不依赖当前措辞。

## 需要主 Agent / reviewer 知晓的设计事实

1. **未终结的 mutation 回 `command.accepted`，不回 `command.terminal`**：wire 的
   `command.terminal.status` 词表只有 `completed|failed|rejected|uncertain`（Sync 的 `CommandStatus` 多一个 `accepted`），把它当 `uncertain` 会把对端钉在一个假终态上（`uncertain` 是终态、Access 不得重试）。因此：
   - 首次提交 → `command.accepted`（`acceptedAt` 取该记录的持久化接受时间，缺失时取应答时刻）+ 挂终态观察；
   - `command.status` 重查未终结的 record → 同形 `command.accepted`；
   - 记录已终结 → `command.terminal`（`terminalEventId` 取持久化终态事件，因此 mutation 的重查回复非 null）。
2. **`command.status` 重查"查询命令"的 requestId 会得 `nodelink.command.not_found`**：core 的 `submit_command` 只对 mutation 落 `owned_command` 行，查询命令没有记录。R66 的「重查回复 `terminalEventId` 非 null」在 mutation 上逐字满足（用例断言）；查询命令的同步回复里 `terminalEventId` 为 null（它没有终态事件，core 的存储合同也禁止给查询记录写 `terminal_event`）。
3. **`session.create` 的幂等表是进程内的**：core 的 `create_session` 不写 `owned_command`，所以 CLI/节点重启后同一 requestId 会再创建一次会话（不谎报幂等）。表上限 1024 条、按键序淘汰最旧一条；跨实现的多节点幂等不在 v1 的承诺里。
4. **本切片显式不支持三个查询**：`session.read`/`session.mode.list`/`session.config.list` 回 `command.rejected(nodelink.command.unsupported)`（`queries_without_a_v1_projection_are_rejected_explicitly`）。原因：它们的 wire 结果形状（`sessionReadResult`/`modeListResult`/`configListResult`）需要把正文/活体元数据投影成 Sync 登记的视图，`session.read` 的 `messages` 还要事件正文的聚合——属 Access facade（切片 6）的正文路径，本切片用 `resource.*` 交付事件正文。**不返回被裁剪的结果**（AGENTS §3：不得静默丢弃正文）。R66–R78 没有一条要求它们；若 review 认为必须在本切片内提供，需要先裁决结果形状的归属。
5. **两条留痕缺口（无 core 缝，只有结构化日志）**：`rate_limit.triggered` 与「幂等冲突」没有 core 的写入口（`record_node_link_auth` 只接受 `node.authenticated`/`node.auth_failed`/`authorization.denied`），因此限流与 `idempotency_conflict` 只写结构化日志；R82 列出的「命令幂等冲突」如果要求审计行，需要主 Agent 裁决是否再扩 `record_node_link_auth` 的接受集合（本轮只按 Q2(a) 扩到 `authorization.denied`）。
6. **in-flight 上限的拒绝形状**：注册表里没有专门的「在途超限」错误码，用的是 `nodelink.resource.rate_limited`（`retryable = true`）+ `details.retryAfterMs = 1000`。上限值取 `node.ready.limits.maxInFlightCommands`，计数口径是「已接受但未终结的 mutation」（= 终态观察表的条数），判定前会先摘掉已经终结的条目；读记录失败时保持占用（失败关闭）。
7. **授权交集的 capability 支停在 core/agent 边界**：`compatibility/acp/v1/matrix.json` 里没有「ACP capability → 命令」的映射，因此「实际 capability」这一支无法在适配层判定；当前交集是 Export grant ∩ 信任记录 grant，capability 由 Agent 后端在派发路径上如实回报。若 review 需要显式的一支，需要先补矩阵。

## 测试手法说明（一处刻意的选择）

- **命令 mutation 的落盘路径不在本 WP 的用例里**：`CommandStore::commit` 只支持 create，其余分支 `unreachable!`——这条 `unreachable!` 就是「不得发生第二次派发」的**断言**（命中幂等时代码必须提前返回）。`session.prompt` 走完 `submit_prompt` 需要后端 turn 事件与 turn 表语义，那属 [PV5] 的受控路径全链路测试（2.22 用真实 SQLite + fake Access 客户端覆盖 R66/R67/R69/R72/R73/R76/R77/R78 的本机轮次）。本 WP 的用例覆盖的是**适配层自己的判定与映射**（wire 边界、授权交集、幂等命中/冲突的映射、终态形状、限流与在途、撤销推送）。
- 用例用**同一份解码结果**算幂等指纹（`serde_json::from_value::<CommandSubmit>` 后调 `fingerprint_of`），因此断言的是适配器真实使用的那条路径，而不是测试自造的摘要。
- 端口替身只实现被触及的方法，其余 `unreachable!`（与 WP5 的用例同口径）；`FakeTrust`/`FakeExports`/`FakeAudit`/`FakeConfig` 复用 `local_admin::test_support`，其中 `FakeTrust` 的写路径刻意照抄真实存储的可观察语义。
