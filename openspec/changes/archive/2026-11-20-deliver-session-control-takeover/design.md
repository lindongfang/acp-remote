# 单会话客户端控制权接管与只读隔离技术设计

## Context

动机与用户确认依据见 [proposal.md](proposal.md)，行为规则见 [session-control-takeover](specs/session-control-takeover/spec.md)、[session-execution](specs/session-execution/spec.md)、[permission-approvals](specs/permission-approvals/spec.md) 增量规格及 `openspec/specs/` 长期规格。前端基于 `refactor-web-workbench` 模块化架构（`clients/web/src/management/session-protocol/*` 与 `clients/web/src/workbench/*`）演进，后端基于 `app/src/node/`、`app/src/application/` 与 `app/src/management_ws/` 既有主执行树与请求准入端口扩展。

## Goals / Non-Goals

**Goals:**
- 在保持节点级设备信任（`client-trust`）与外部原生会话纳管（`native-session-import`）正交的前提下，以主执行树根身份（`NativeSessionKey(source_id, root_native_id)`）为唯一粒度，建立运行期单客户端互斥写控制权与进程活跃期内严格单调递增的控制代际（`controlEpoch`）。
- 支持新建主会话自动授予初始控制权、无主会话显式接管、已受控会话顶号式显式抢占接管、当前主控主动释放控制权及实时管理通道广播（`session.control.changed`）。
- 在队列写操作（`input.register` / `input.consume`）与权限审批裁决（`approval.register` / `approval.decide`）入口统一实施主树控制权与控制代际核验，保证顶号、主动释放、信任撤销、会话资源释放及节点重启时在途 `Run`、已受理队列项与待决审批不丢失、不误中断。

**Non-Goals:**
- 引入多用户团队 RBAC 角色体系，或将客户端控制权持久化到 SQLite 跨重启保留。
- 修改 `management-input-upload` 的连接私有分块暂存通道（`input.upload.*` 与 `ACPI` 二进制帧，上传完成不等于队列受理）。
- 阻塞未接管可信客户端的节点级操作（`node.info`、`device.list`、`device.revoke`、`catalog.*`、`workspace.register`、`target.register`、`session.capacity.inspect`、`native.release.*`、`source.session.*`）或只读会话查阅（`view.open`、`view.open_source`、`input.queue`、`input.body.open`、`approval.list/read/status`）。

## Decisions

### 1. 职责分层、单实例归属、专用控制权根解析与门禁作用面

1. **双层准入链路与单实例协调器归属**：
   - **节点级设备信任层**：继续由 `ApplicationPolicy::require_current_actor` 在 `TrustStore::client_gate(client_id)` 保护下核验客户端 `ClientStatus::Trusted`、公钥与 `auth_generation`；扩展 `AuthenticatedActor`（或通过 `TrustStore` 受门保护查询）携带客户端 `display_name`，供控制权快照展示与广播使用。
   - **主会话控制权准入层（进程内唯一协调器实例）**：新增运行期内存协调器 `SessionControlCoordinator`，在 `Storage` / `ApplicationPolicy` / `NodeRuntime` 组装层创建进程内唯一 `Arc<SessionControlCoordinator>` 实例，统一注入 `TrustStore`、`NodeRequestAuthority`（同时覆盖 `NodeRuntime::request_authority()`、`NodeRuntime::approval_owner()` 及 `start_control_only_for_test` 组件级入口）、`OwnedNodeSessionServices` 与 `ManagementService`，杜绝多实例状态分裂。
   - **专用控制权主树根解析（`resolve_control_root`）**：在保持 `NativeCoordinator::ensure_read_resource` / `reserve_capacity` 容量加载根验证（`SessionTreeRepository::resolve_session_tree` / `require_root`）100% 不变的前提下，为控制权操作与队列/审批写准入定义专用主树根解析规则：
     - 目标寻址操作：先经 `NodeRequestAuthority::resolve_target(target_id)` 查询 `product_targets` 取得 `native: NativeSessionKey`（不存在返回 `invalid_target`），再沿 `node_session_parents` 上溯祖先链（环路返回损坏状态错误）：
       1. **存在父链记录（`!ancestors.is_empty()`，目标系子会话）**：强制要求顶层祖先在 `node_session_capacity` 中具备 `root_verified = 1`，否则拒绝并返回 `session_tree_unverified`；验证通过后收敛至该顶层祖先 `root: NativeSessionKey`（保持子会话防伪造新根安全边界，不设立子会话独立锁）。
       2. **不存在父链记录（`ancestors.is_empty()`，顶层登记目标）**：若 `node_session_capacity` 中存在该 `(source_id, native_id)` 记录但仅有 `root_verified = 0`（无 `root_verified = 1`，如遗留升级未验证绑定），返回 `session_tree_unverified`；否则（已验证 `root_verified = 1`，或经 CLI `acp-remote target register`、Web `discover -> target.register` 及测试夹具登记于 `product_targets` 且尚未进入 `node_session_capacity` 的独立顶层目标）直接以该 `native` 自身作为控制权根键 `NativeSessionKey`。
     - 审批寻址操作：`NativeApprovalService`（`approval.register` / `approval.decide`）经 `ensure_approval_association` 定位 `ApprovalSlot` 关联 `target: EntityId`（未关联返回 `invalid_approval`），在 `ApprovalAuthority::authorize_decision` 中经同一 `resolve_control_root` 解析至 `root: NativeSessionKey` 校验控制权。
2. **精确门禁作用面**：
   - **受主会话控制权保护的写命令（未持有控制权或代际失配统一返回 `session_not_controlled`）**：
     - 队列与执行变更：`"input.register"`（覆盖 `QueueMutation` 全部 5 类动作：`submit`、`edit`、`cancel`、`prioritize` 即中断并优先发送、`resume` 即显式继续）与 `"input.consume"`。两步操作均在准入与提交变更点核验控制权与登记时绑定的 `controlEpoch`，防止客户端在 `register` 后被顶号仍通过 `consume` 落库或派发。
     - 权限审批裁决：`"approval.register"` 与 `"approval.decide"`。在登记与提交裁决事务点均核验所属主会话当前控制权与 `controlEpoch`；若裁决已在控制权变更前可靠提交至 `product_approvals`（`state = 'decided'`），则后续交付由节点按既有托管规则继续完成，不受原审批者断线、释放控制权、被顶号或被撤销影响。
     - 显式释放控制权：`"session.control.release"`（仅当前主控客户端可释放；无主或由其他客户端控制时返回 `session_not_controlled`）。
   - **不受主会话控制权阻塞的命令**：
     - 连接私有上传暂存：`"input.upload.begin"`、`"input.upload.finish"`、`"input.upload.status"`、`"input.upload.release"` 及 `ACPI` 二进制分块保持纯连接内存暂存语义不变（零 SQLite/队列/Harness 副作用），避免顶号瞬间因 `ACPI` 错误触发 `4400` 断开原客户端管理连接；当后续 `"input.register"` 或 `"input.consume"` 返回 `session_not_controlled` 时，前端 `submitSessionText` / `editSessionText` 沿既有异常分支自动调用 `"input.upload.release"` 回收暂存槽位。
     - 只读观察与历史查看：`"input.queue"`、`"input.request"`、`"input.body.open"`（按需读取排队项正文）、`"approval.list"`、`"approval.read"`、`"approval.status"`、`"view.open"`、`"view.open_source"`、`"view.close"`、`"view.rebind"`。
     - 节点级管理与容量协调：`"node.info"`、`"device.list"`、`"device.revoke"`、`"catalog.*"`、`"workspace.register"`、`"target.register"`、`"session.capacity.inspect"`、`"source.session.*"`（含 `discover` 与新建主会话 `create`）、`"native.release.register"` / `"native.release"` / `"native.release.status"`（全局容量手动释放遵循既有活动与待决审批安全保护，不因会话无主或原主控离线而死锁名额释放）以及 `"session.control.inspect"` / `"session.control.takeover"`。

### 2. 控制权状态机、线上协议契约与请求幂等

1. **内存状态结构、锁纪律与进程内代际严格单调不变量**：
   `SessionControlCoordinator` 使用同步短临界区 `std::sync::Mutex`（**严禁持锁跨 `.await` 或在锁内执行任何 SQLite I/O**）维护：
   - `roots: HashMap<NativeSessionKey, RootControlRecord>`，其中 `RootControlRecord { controller_client_id: Option<EntityId>, controller_display_name: Option<String>, control_epoch: u64, root_target_id: Option<EntityId>, unloaded_claimed_at: Option<Instant> }`；同时维护协调器级水位线 `evicted_epoch_floor: u64`（初始为 `0`）。
   - **进程内 `controlEpoch` 单调不减铁律**：在同一 `NodeRuntime` 进程活跃期内，任何根会话的 `controlEpoch` **绝不倒退或重置为 `0`**。`controlEpoch: "0"` 仅表示当前 `NodeRuntime` 进程启动以来从未被授予或接管过控制权的初始无主态（包含节点进程重启后的全部历史会话）。进程内任何释放/回收/撤销事件仅置空 `controller_client_id = None` 并递增 `control_epoch += 1`；若协调器为限制内存淘汰长期无主的空闲条目，必先更新 `evicted_epoch_floor = evicted_epoch_floor.max(record.control_epoch)`，使任何后续新建/接管记录从 `evicted_epoch_floor.saturating_add(1)` 起算，彻底消除同进程跨释放周期的 `controlEpoch` ABA 复用与前端乱序覆盖竞态。
   - `RootControlRecord.root_target_id` 在 `target.register`、`session.control.inspect`、`session.control.takeover`、`session.control.release` 已解析出根 `product_targets` 行时同步刷新缓存，使 `TrustStore::revoke` 或 `capacity_closed` 在纯内存锁内即可直接组装 `session.control.changed` 帧而无需反查 SQLite。
   - `receipts: VecDeque/HashMap<(EntityId, Id128), CachedControlReceipt>`（有界请求幂等表，绑定进程活跃期内存，避免节点重启后从持久化命令表重放出过期的重启前 `controlEpoch`）。
2. **新增会话管理命令（`SessionAction` 与 Web `SHAPES`）**：
   - `"session.control.inspect"`：请求 `{ "targetId": "<id128>" }`，返回 `SessionControlSnapshot`：
     ```json
     {
       "targetId": "<id128>",
       "rootTargetId": "<id128 | null>",
       "sourceId": "<id128>",
       "rootNativeSessionId": "<string>",
       "controllerClientId": "<id128 | null>",
       "controllerDisplayName": "<string | null>",
       "controlEpoch": "<u64 decimal string>"
     }
     ```
     其中 `rootTargetId` 通过 `product_targets(source_id, native_id)` 反查根会话已登记的 `targetId` 并回填缓存（若仅注册了子目标而根目标尚未单独注册则为 `null`）。
   - `"session.control.takeover"`：请求 `{ "targetId": "<id128>" }`，返回更新后的 `SessionControlSnapshot`。
     - 先按 `(actor.client_id, requestId)` 核对幂等缓存：同身份且同 `("session.control.takeover", targetId)` 重试直接返回原快照（不重复递增 `controlEpoch`）；同 `requestId` 但动作或参数不同返回 `"request_conflict"`。
     - 若当前主控已为 `actor.client_id`，刷新展示名与 `root_target_id` 并返回当前快照（保持 `controlEpoch` 稳定）并写入幂等缓存；若当前无主或由另一客户端持有，则原子更新 `controller_client_id = Some(actor.client_id)`、`controller_display_name = Some(actor.display_name)`、`control_epoch = record_prev_epoch_or_floor.saturating_add(1)`（首次接管且无淘汰水位时由 `0` 升至 `1`，顶号或释放后再接管均在前值/水位基础上严格递增 `+1`），记录幂等收据与安全审计，并向在线认证连接广播 `session.control.changed`。
   - `"session.control.release"`：请求 `{ "targetId": "<id128>", "expectedControlEpoch"?: "<u64 decimal string>" }`，返回更新后的 `SessionControlSnapshot`。
     - 先按 `(actor.client_id, requestId)` 核对幂等缓存（同参数重试返回原释放快照，异参数返回 `"request_conflict"`）。
     - 核验当前 `controller_client_id == Some(actor.client_id)` 且可选 `expectedControlEpoch` 与当前 `control_epoch` 一致；任一不满足立即返回 `"session_not_controlled"` 且零状态变更。
     - 校验通过后原子置空 `controller_client_id = None`、`controller_display_name = None`、清除 `unloaded_claimed_at` 并递增 `control_epoch = control_epoch.saturating_add(1)`，写入幂等收据与安全审计，广播 `session.control.changed`。
3. **可选期望代际（`expectedControlEpoch`）闭合解析契约与两步提交点代际绑定**：
   - **闭合解析规则**：在 `"input.register"`、`"input.consume"`、`"approval.register"`、`"approval.decide"` 与 `"session.control.release"` 中扩展可选字段 `expectedControlEpoch?: DecimalU64`。Rust（`app/src/management_ws/session_protocol.rs` 配合 `#[serde(default, skip_serializing_if = "Option::is_none")]`、非空值反序列化器与闭集可选键白名单）与 Web（`clients/web/src/management/session-protocol/primitives.ts` 与 `wire-codec.ts`）两侧严格要求该字段**仅允许“省略该键”或“合法 canonical 十进制 `u64` 字符串”**；显式传入 JSON `null`（`"expectedControlEpoch": null`）、非字符串、负值/溢出数字字符串或任何未知额外字段，一律确定性拒绝为 `invalid_command`（线上 `invalid_request`）。
   - **两步提交点复核与已落库幂等重放兼容**：当客户端携带 `expectedControlEpoch` 时，协调器同时要求 `controller_client_id == Some(actor.client_id)` 且 `control_epoch == expected`；当省略时，协调器除校验 `controller_client_id == Some(actor.client_id)` 外，在 `input.register` 与 `approval.register` 受理时绑定当时的 `registered_epoch`。在 `QueueService::consume_inner`（`OwnerCell` 入队/队列变更提交点）与 `NativeApprovalService::decide`（`ApprovalSlot` 裁决落库提交点）：若该 `(actor.client_id, requestId)` 对应的变更此前已提交落库（`SqliteRequestRegistry` 已存在终态 `RequestFact` 或 `product_approvals` 已记录该请求的 `StoredApprovalDecision`），直接幂等返回既有已落库事实以保障断线重连重试语义；仅对尚未提交的两步执行原子复核 `controller_client_id == Some(actor.client_id) && current.control_epoch == registered_epoch`（防止 `A(epoch=1) -> B(epoch=2) -> A(epoch=3)` 回切或释放重接管窗口放行旧代际未完成登记）。
4. **管理通道实时广播与闭合错误码同步**：
   - 新增服务端推送帧 `{ "v": 1, "type": "session.control.changed", "rootTargetId": "<id128 | null>", "sourceId": "<id128>", "rootNativeSessionId": "<string>", "controllerClientId": "<id128 | null>", "controllerDisplayName": "<string | null>", "controlEpoch": "<u64 decimal string>" }`。在 `app/src/management_ws.rs` 的 `run_authenticated_loop` 中通过 `tokio::sync::broadcast` 监听并经 `enqueue_authenticated` 复核未撤销状态后直接下发给全部已认证管理连接（无需客户端在握手后额外发送新 `subscribe` 命令，兼容既有连接建链流程）。
   - 错误结果严格保持 `{ "code": "session_not_controlled" }` 单键结构（兼容 `clients/web/src/management/protocol.ts` 的 `parseCommandResult` 闭合键校验），同步将 `"session_not_controlled"` 加入 `ManagementErrorCode`、`knownManagementError()` 及 `clients/web/src/management/error-messages.ts` 的 `REQUEST_ERROR_MESSAGES` 中文映射。

### 3. 生命周期钩子、自动归零与审计存储边界

1. **新建主会话自动授权**：
   - 在 `SourceSessionService::drive_admitted`（`app/src/application/native_dispatch/source_sessions/operations.rs`）中，当 `SourceRequestKind::Create` 确认返回 `SourceResponseKind::Created` 并完成 `bind_created_capacity`（写入 `root_verified = 1`）时，以创建操作的 `ActorContext` 原子初始化根 `NativeSessionKey` 的控制权记录（`controller_client_id = Some(actor.client_id)`，`control_epoch` 由首次无水位状态升为 `1`，若曾有协调器淘汰水位则取 `evicted_epoch_floor.saturating_add(1)`）并广播 `session.control.changed`。此后客户端执行 `target.register` 绑定 `targetId` 时即可直接映射到该初始控制记录并补齐 `root_target_id` 缓存。
   - 既有历史会话经 `OriginalAdmission::reserve` 或 `ReadCapacityAdmission::reserve` 执行只读重放或恢复加载时，不触发自动授权，保持无主只读态直至客户端显式调用 `session.control.takeover`。
2. **设备信任撤销即时归零与单连接 SQLite 防死锁纪律（覆盖 WSS 与 CLI 命名管道双入口）**：
   - 在 `TrustStore::revoke`（`app/src/trust.rs`，同时服务于 WSS `"device.revoke"` 与本机 CLI `acp-remote device revoke` 命名管道入口）撤销成功时，调用共享 `SessionControlCoordinator` 将所有 `controller_client_id == Some(revoked_id)` 的主会话在内存锁内原子置为无主态（`controller_client_id = None`、`controller_display_name = None`、`unloaded_claimed_at = None`、`control_epoch = control_epoch.saturating_add(1)`），利用已缓存的 `root_target_id` 直接向广播通道发送 `session.control.changed`。
   - **单连接 SQLite 防死锁硬约束**：因 `SqliteRequestRegistry` 底层使用单连接池（`max_connections(1)`）且 `TrustStore::revoke` 持有 `trust_owned_connection` 直至 `connection.retain_for_trust()`，写入 `'session.control.revoke_clear'` 的 `security_audit` 记录**严禁在归还 `trust_owned_connection` 前另行申请新的 SQLite 连接**——必须在 `TrustStore::revoke` 同一事务提交前复用该事务连接一并写入，或在 `connection.retain_for_trust()` 归还连接之后再执行异步审计落库。同时在协调器验权入口结合 `TrustStore` 当前信任状态做兜底复核。
3. **主会话资源释放、预接管超时与节点重启自动归零**：
   - 在 `NativeCoordinator::capacity_closed`（`app/src/application/native_dispatch/capacity.rs`，统一覆盖手动 `native.release`、`release_target`、30 分钟 `IdleTimeout` 自动回收及 `CapacityPressure` 满额淘汰）及 `SourceSlot` 失效收敛路径中：若对应根 `NativeSessionKey` 当前存在主控客户端（`controller_client_id.is_some()`），原子置空 `controller_client_id = None`、`controller_display_name = None`、`unloaded_claimed_at = None` 并**单调递增 `control_epoch = control_epoch.saturating_add(1)`**，以新 `controlEpoch` 广播 `session.control.changed`（若已处于无主态则保持当前 `control_epoch` 不变，绝不在同一进程内将 `controlEpoch` 倒退回 `"0"`）。
   - 若客户端对当前尚未加载 `CapacityResource` 的冷会话提前执行了 `session.control.takeover`，记录 `unloaded_claimed_at`；一旦会话实际加载即清除该标记交由 `capacity_closed` 托管，若连续超过 `DEFAULT_IDLE_TIMEOUT`（30 分钟）仍未加载使用则在协调器扫描/访问时同样置空 `controller_client_id = None` 并递增 `control_epoch += 1` 广播归零。
   - 节点进程重启后，`SessionControlCoordinator` 内存表全新初始化为空，所有历史会话重新打开均处于初始无主只读态（`controllerClientId = null, controlEpoch = "0"`）。
4. **SQLite 审计与冻结表约束**：
   - 严禁修改受 `validate_management_audit` 精确 DDL 校验保护的 `node_management_audit` 表，严禁创建 `LEGACY_SESSION_TABLES` 封禁的 `session_commands` 表。
   - 会话控制权生命周期事件（`'session.control.initial'`、`'session.control.takeover'`、`'session.control.release'`、`'session.control.revoke_clear'`）统一追加写入既有 `security_audit` 表（`actor_id`、`action`、`target_id` 为根会话稳定键、`outcome` 记录前后控制端与 `controlEpoch` JSON 摘要、`occurred_at`），零破坏性 Schema 迁移。

### 4. Web 工作台模块化设计与只读交互不变量

1. **单向分层、单调快照收敛与 650 行文件预算控制**：
   - 在协议层新增 `clients/web/src/management/session-protocol/control.ts`（定义 `SessionControlSnapshot`、`SessionControlChangedFrame` 及闭合解析器），由 `session-protocol.ts`（保持 `<= 120` 行）纯 barrel 重导出，并在 `wire-codec.ts`（当前 237 行）与 `primitives.ts`（新增仅接受省略或 canonical 十进制 `u64` 字符串、显式拒绝 `null` 的可选 `DecimalU64` 字段校验辅助）注册 `"session.control.inspect"`、`"session.control.takeover"`、`"session.control.release"` 及可选 `expectedControlEpoch`。
   - 在工作台层新增独立控制器 `clients/web/src/workbench/session-control-controller.ts`，集中管理当前目标与根会话的控制权快照缓存（在同一管理连接会话内按单调不减 `BigInt(controlEpoch)` 合并并发 `inspect` 响应与 `session.control.changed` 推送，重连后重置本地连接缓存以接纳节点重启后的 `controlEpoch = "0"`）、状态横幅与操作按钮渲染（本机主控态显示“当前由本设备接管 · 代际 N”与“释放控制权”；他人主控态显示“只读观察 · 当前由 `<displayName>` 接管”与“顶号接管会话”；无主态显示“只读观察 · 当前无人接管”与“接管会话”）。`source-session-controller.ts`（当前 637 行）、`session-queue-controller.ts`（当前 604 行）、`workspace-catalog-controller.ts`（当前 604 行）与 `management-workbench.ts`（当前 599 行）仅通过轻量回调委托对接，确保 `src/workbench/*.ts` 与 `src/management/session-protocol/*.ts` 全部文件严格 `<= 650` 行且零循环依赖。
2. **只读控件禁用、未发草稿保留与浏览器夹具兼容**：
   - 当目标主会话处于非本机控制态（无主或由其他设备控制），或收到 `session.control.changed` 顶号推送 / 写请求返回 `session_not_controlled` 时：立即禁用全部会话写动作 `<button>`（“发送输入”、“恢复暂停队列”、“优先执行”、“编辑”、“取消”、“保存编辑”、“提交审批决策”），但在审批面板与会话面板提供一键“接管会话 / 顶号接管”入口。
   - **草稿与焦点选区不变量**：`#session-input-body` 文本框保持可聚焦（不设置阻断焦点的 `disabled = true`，仅禁用提交按钮并保留用户输入内容），在被其他设备顶号或收到 `devices.changed` / `session.control.changed` 触发面板重绘时，继续通过 `captureActiveControlSelection` 与 `restoreActiveControlSelection` 完整保留本地未发送草稿文本与光标选区（同时保证既有独立 DOM 用例 `WV-15` 焦点选区契约不变）。
   - 同步在 `clients/web/tests/browser/independent-dom/management-peer.ts`（及对应 `FIXTURE-SHA256.md` 清单）补齐对 `session.control.inspect` 的模拟响应，确保既有 `WV-13`–`WV-16` 独立浏览器回归全绿。

## Risks / Trade-offs

- **[双端网络延迟下旧主控写请求与顶号/释放重接管并发交错]** → 同一 `NodeRuntime` 进程内 `controlEpoch` 严格单调递增（释放/回收亦递增而不回退 `0`），所有队列与审批写入口在提交点核验最新 `controller_client_id` 与两步登记/请求携带的 `controlEpoch`，迟到请求确定性返回 `session_not_controlled` 且零队列/SQLite/Harness 副作用，前端捕获后刷新控制权状态并保留本地草稿。
- **[主控设备离线挂起而会话停留在待审批或暂停队列]** → 其他在线可信设备可在只读视图实时查看待审批详情与暂停原因，点击“顶号接管”即可原子接手并完成审批裁决或恢复队列，无死锁风险；全局容量面板手动释放亦不因无主或主控离线而受阻。
- **[既有单客户端测试基线受 BREAKING 默认只读策略影响]** → 以前直接向 `product_targets` 注入顶层目标后立即发起 `input.register` / `approval.register` 的集成测试，经 `resolve_control_root` 识别为独立顶层目标后在默认无主只读新规则下会收到 `session_not_controlled`（而不会误报 `session_tree_unverified`）；需在对应测试准备阶段显式调用 `session.control.takeover`（或经 `source.session` `create` 自动取得初始控制权），同时保留专项拒绝用例验证 BREAKING 门禁与未验证子会话 `session_tree_unverified` 拦截生效。

## Migration Plan

控制权状态纯驻留节点运行期内存，无需迁移既有客户端身份、工作区、配置、请求登记或审批数据；`security_audit` 复用既有表结构追加控制权事件，管理协议新增闭合错误码 `"session_not_controlled"` 与推送帧 `"session.control.changed"` 由同源内嵌 Web 与 Rust 节点在单二进制构建中原子同步发布。
