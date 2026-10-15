# Tasks

## 1. 事件视图与原文装配（WP1）

- [x] 1.1 新建 `crates/server/src/sync/event/`，在 `crates/server/src/sync/mod.rs` 追加模块声明；模块文档写明它只调用 `core::use_cases` 与 `sync-protocol` 的公开面，不 import `storage-sqlite`、不调用 `server::node_link`（算法口径相同但类型各自定义）。完成判据：`npm run check:boundaries` 通过。
- [x] 1.2 实现视图投影：从持久化事件取 `payload_json` 的**原样文本**作为 `view`，调 `sync-protocol::views::project` 做形状判定；判定失败时记一条含 `event_type` 与缺失字段的结构化日志，但仍按原样下发该 `view`（不得丢弃事件、不得造序号空洞）。完成判据：单元测试覆盖「形状正确时按类型投影」「形状缺失时事件仍下发且日志可见」两条路径。
- [x] 1.3 实现 `payload.acp` 装配：按 `acp.raw-payload.v1` 协商结果决定是否携带；`rawJson` 解码后的字节 MUST 与原始文档逐字节一致（**不得先解析再重新序列化**）；不可用时按已登记原因标记并给出字节长度。完成判据：单元测试用含超 64 位整数字面量与未识别字段的原文驱动，断言下发文本与输入逐字节相同。
- [x] 1.4 实现不可用标记的三种原因与内嵌上限：超过 256 KiB 或 media type 非 `application/json` 时降级为不可用（原因区分 `size_limit` 与 `storage_failure`），保留期原因对应 `retention_expired`；未协商能力时不下发原文容器。完成判据：单元测试逐条覆盖三种原因与「未协商时不携带容器」，且断言未发生截断。
- [x] 1.5 实现未登记内容的降级路径：权威协议未登记的事件类型与 ACP 判别子仍下发并保留原文或不可用标记，MUST NOT 静默丢弃、MUST NOT 降级为普通文本、MUST NOT 阻断同会话后续事件。完成判据：单元测试覆盖「未登记判别子仍有事件」「同会话后续事件投递不受影响」。
- [x] 1.6 事件轨道与投递队列衔接：把投影后的事件交给前置变更实现的有界出站队列（按其水位处理），**不另建第二条队列**。完成判据：单元测试断言事件经由既有队列路径入队，且水位触发时投递层被告知停止读取重放批次。

## 2. 命令管线与查询命令（WP2）

- [x] 2.1 新建 `crates/server/src/sync/command/` 与命令解码分派：按 `sync-protocol::command` 的 DTO 解码，封闭词表外与 `session.resume` 一律按 `command.unsupported` 拒绝。完成判据：单元测试断言 `session.resume` 在 Sync 面被拒且未在本机执行任何操作。
- [x] 2.2 实现授权前置：每条业务命令在读取会话行、触碰文件系统、启动进程**之前**按设备 scope 判定；被拒时回 `authorization.scope_denied` 且零副作用，且不因会话是否存在改变错误类型。完成判据：单元测试断言「未授权设备对不存在的会话派发命令也只得授权拒绝」，并用一个记账替身证明未发生任何 core 读取调用。
- [x] 2.3 实现 5 条查询命令：`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list` 同步返回一次 `completed` 或 `rejected`，结果形状与协议登记一致。完成判据：单元测试逐条断言结果形状，并断言任一查询命令都不会先回 `accepted`。
- [x] 2.4 实现结果超限的显式失败：投递前计算编码后字节数，超过单条消息上限时回 `resource.result_too_large`，MUST NOT 截断后当完整结果返回。完成判据：单元测试用一个超大结果驱动，断言返回该错误码而非被截断的结果。
- [x] 2.5 实现命令级限流：按设备标识与命令族分别计数，默认 120/分钟；超限回 `resource.rate_limited`（可重试，带 `retryAfterMs`）；连续超限达阈值时以 `4429` 关闭连接。完成判据：单元测试覆盖「单次超限只回可重试错误」「连续超限关闭连接」「不同命令族的额度互不挤占」。
- [x] 2.6 实现查询类命令的连接期去重（不落盘）与命令 `messageId` 重复不重放业务结果。完成判据：单元测试断言同一查询在不同连接上各自独立、同一连接内重复 `messageId` 不产生第二次业务读取。

## 3. mutation 与终态（WP3）

- [x] 3.1 实现 7 条 mutation 的接受路径（`session.prompt`、`session.cancel`、`permission.resolve`、`elicitation.respond`、`session.mode.set`、`session.config.set`、`session.create`）：先持久化接受并回 `accepted`（`result` 与 `terminalEventId` 均为 `null`），MUST NOT 在同一条响应里给出业务结果。完成判据：单元测试逐条断言首响应为 `accepted`，且接受行已持久化。
- [x] 3.2 实现 `(deviceId, requestId)` 幂等：重复提交返回首次的已知接受或终态，MUST NOT 二次派发；查询类之外的 mutation 幂等由持久记录承担，MUST NOT 只放内存。完成判据：单元测试断言「同 requestId 重发只产生一个业务副作用」，并断言该性质跨「重新打开存储」仍成立。
- [x] 3.3 实现冲突判定：比较 `command`、`sessionId`、`expectedVersion` 与**解码后 payload 的语义**（用 ACPR-CJ1 摘要比较，不得依赖 JSON 键顺序），不一致回 `command.idempotency_conflict`。完成判据：单元测试覆盖「键顺序不同但语义相同 → 不判冲突」与「值不同 → 判冲突」两条。
- [x] 3.4 实现 `expectedVersion` 冲突：模式与配置设置必须携带版本，冲突时回 `state.version_conflict` 并给 `currentVersion`，且零副作用。完成判据：单元测试断言冲突时返回当前真实版本，且会话状态与版本未变。
- [x] 3.5 实现终态观察者：按 `(deviceId, requestId)` 持有、连接断开即取消（不留 detached task）、终态从事件扇出匹配后入队该连接；MUST NOT 由适配层重新推导业务结果。完成判据：单元测试覆盖「提交方收到自己的终态」「断开后观察者被取消且无遗留任务」，并断言重连后可用 `command.status` 查到终态。
- [x] 3.6 实现 `uncertain` 的处理：持久化 `command.uncertain` 后广播、MUST NOT 自动重派；`command.status` 对已终结 mutation 返回终态与原 `terminalEventId`。完成判据：单元测试断言「uncertain 不触发第二次派发」与「status 返回原终态标识」。

## 4. 文档同步（本变更自带）

- [x] 4.1 更新 `docs/SYNC_PROTOCOL.md` §10.3/§11 的实绩写回（命令面与事件面已落地），保持「计划」类措辞只用于仍未实现的部分。**并包含 §6.2 与 §6.3 的降级口径**（已裁决：本期不补 core 读面）：§9.4 的资源表与 §11.5 的 `session.read` 行要写明 —— 本期服务端对 `turns`/`pending_interactions`/`capabilities` 与 imported 会话的明细资源一律回 `capability.unsupported_by_broker`，客户端 MUST NOT 把该拒绝当成「会话无内容」或「已到最早一条」。完成判据：`npm run check:docs`。
- [x] 4.2 更新 `docs/MODULE_ARCHITECTURE.md` §3/§4.9 的 `[现状]` 与 `README.md` 的「仓库当前状态」与 `server` 行。**并更新 `docs/FRONTEND_DESIGN.md`**：写明 PWA 本期**不展示** `turns`/`pending_interactions`/`capabilities` 三类明细资源与 imported 会话明细（§6.2/§6.3 的已裁决降级），并说明它不影响其余三类明细（`messages`/`config_options`/`history`）。完成判据：`npm run check:docs`。

## 5. 协议级端到端验收（WP4）

- [x] 5.1 在 `crates/app/tests/support/sync.rs` 追加命令相关方法（派发命令、读取 `command.result`、等待终态事件、读取事件视图）。完成判据：客户端能对真实 listener 派发一条查询命令并收到 `completed`。
- [x] 5.2 在 `crates/app/tests/sync_commands_e2e.rs` 实现主链路：派发 `session.prompt` 收到 `accepted` → 观察同一轮次的领域事件（含视图最低字段断言）→ 收到 `command.completed` 且 `terminalEventId` 与终态事件一致。完成判据：用例断言各阶段可观察结果，并断言事件视图含协议要求的最低字段。
- [x] 5.3 覆盖边界：未持 scope 的设备派发 mutation 得 `authorization.scope_denied` 且零副作用（用存储查询证明未写入事件、未启动进程）；同 `requestId` 重发只产生一个副作用；`session.resume` 在 Sync 面得 `command.unsupported`。完成判据：三条各有独立断言，且每条都能在实现错误时变红。
- [x] 5.4 覆盖限流与原文保真：超限得到 `resource.rate_limited`（含 `retryAfterMs`）且被拒命令无副作用；一条含未识别字段的 ACP 原文经事件下发后逐字节可比。完成判据：两条各有独立断言。**落地口径**：限流一半在新用例 `exceeding_the_command_quota_is_retryable_and_side_effect_free` 里走真实 listener（用生产额度 120/分钟/设备/命令族发满一轮，断言 `resource.rate_limited` 可重试且含 `retryAfterMs`）；**原文保真一半不在该文件**——它由 WP1 的事件装配用例（`sync::event` 的逐字节断言，含超 64 位整数字面量与未识别字段）与既有快照/历史投递用例覆盖，脚本化后端不产出带 ACP 原文的事件，因此在端到端层无法构造「未识别字段的 ACP 原文」而不伪造后端行为。
- [x] 5.5 **（不适用——本次无隔离区）** 本条约束的是**工作包**的报告：交付的若只是用例代码，就不得声称已在隔离区通过，真实运行由主 Agent 合入后执行。本次 WP4 由主 Agent 在**主检出**直接实现并运行（没有派发工作包、不存在隔离区），因此该约束的对象不存在，据实登记为不适用而不是「已完成」。实际证据：`cargo test -p app --test sync_commands_e2e` 在本机主检出 6/6 通过（真实 loopback listener + 真实 SQLite + 脚本化后端），输出见本轮交付。

## 6. 执行中发现、必须登记的缺口

- [x] 6.1 **（路由接线，plan 未覆盖）** `CommandRoute` 已实现并导出，但**尚未注册进组合根**：`crates/app/src/daemon.rs` 的 `/sync/v1` 端点只接受**一条** `MessageRoute`（`SyncConn::with_route`），当前注册的仍是 `SubscribeRoute`。**需要**：在 `server::sync` 里加一个组合路由（按 `RouteOutcome::Unclaimed` 链式委托 `SubscribeRoute` 与 `CommandRoute`，并把 `on_authenticated` 的连接上下文交接转发给两者），再由组合根注册该组合。完成判据：一条真实 `command.status` 经 `/sync/v1` 得到 `completed`（由 5.1 端到端用例覆盖）。**背景**：由 WP2 的作者在执行中发现并如实上报；它超出该包写范围（`plan.md` 的四个 WP 没有一个覆盖 `crates/app/src/**` 或 `server::sync` 的路由组合），故登记为缺口。

- [x] 6.2 **（core 读面缺口：`session.read` 的三类明细资源无法表达）** 协议（`docs/SYNC_PROTOCOL.md` §9.4 的资源表与 §11.5 的 `session.read` 行）登记了六类明细资源，其中三类在当前 core 读面上**给不出协议要求的字段**：
  - `turns`：`core::model::Turn`（`crates/core/src/model/session.rs:423`）有 `id/session/state/queue_index/causation/started_at/ended_at`，**没有协议必填的 `createdAt`**，也没有可选的 `terminalError`；
  - `capabilities`：`CapabilitySet`（`crates/core/src/model/backend.rs:71`）是一个 `BTreeSet<Capability>`，给不出协议要求的 `agentCapabilities` 与 `brokerAdditions` 两个字段；
  - `pending_interactions`：`PendingInteraction` 没有协议要求的 `schema` 对象。

  当前处置（WP2）：请求这三者一律显式回 `capability.unsupported_by_broker`（已登记的错误码），**不静默省略、不伪造字段** —— 符合 `AGENTS.md` §4「无法安全处理的能力必须明确返回不支持」。**背景**：由 WP2 的作者在执行中核实并上报（主代理已独立复核 `Turn` 与 `CapabilitySet` 的字段）。

  **已裁决（2026-10-15，用户确认走 (b)）**：本期**不**补齐 core 读面 —— 确认「PWA 本期不展示 `turns`/`pending_interactions`/`capabilities`」，由文档如实写明该降级；「补齐 core 读面」作为后续输入登记（见下）。理由：补齐 `Turn.createdAt` 不只是加字段，而是要先决定它的产品语义（turn 的 `createdAt` 取什么时刻），且会把变更② 的范围从「命令面」扩到 core 模型与 wire 值对象。

  因此本条的**完成判据改为**（由 §4.1 的文档包执行，不再是「二选一」）：
  - `docs/SYNC_PROTOCOL.md` 在 §9.4 的资源表与 §11.5 的 `session.read` 行写明：本期服务端对 `turns`/`pending_interactions`/`capabilities` 一律回 `capability.unsupported_by_broker`，客户端 MUST NOT 把该拒绝当成会话无内容；
  - `docs/FRONTEND_DESIGN.md` 写明 PWA 本期不展示这三类资源（以及 imported 会话明细，见 6.3），并说明它不影响其余五类；
  - 后续输入登记：新增 core 读面（`Turn.createdAt` 的产品语义、`CapabilitySet` 的 `agentCapabilities`/`brokerAdditions` 拆分、`PendingInteraction.schema`）属**独立变更**，需先定语义再动模型。

- [x] 6.3 **（WP2 检视指出的第三处缺口：imported 会话的明细资源未回源）** `docs/SYNC_PROTOCOL.md:1368` 规定 imported 会话的明细资源应**回源 Owner**，而当前实现（`crates/server/src/sync/command/query.rs:408-412`）对非自有引用一律回 `capability.unsupported_by_broker`。与 6.2 不同的是：6.2 是 core 给不出形状，本条是**回源通道缺失**（需要 `node_link` 的出站客户端，而 `node-link-client` 尚未落地，`server::sync` 也不得调用 `server::node_link`）。完成判据：与 4.9 同口径登记 —— 要么在出站客户端落地后接通，要么在产品层面确认本期 PWA 不展示 imported 会话明细并在文档写明。**背景**：由 WP2 的独立检视发现（主代理确认它未进 §6 原有清单）。

- [x] 7.2 **（WP3 三次派发全部因宿主外部故障中断，零产出）** 时间线：`SyncCmdWP3` 401 Upstream unavailable（1h09m）→ `SyncCmdWP3b` 合并后发现 762 行实现从未编译通过、30 条用例无一处断言 `accepted`，已退回（见 §7.1）→ `SyncCmdWP3c` 启动后即被中断，未创建任何工作区、零产出（`agent://SyncCmdWP3c` 已 parked，消息被拒）。三次失败均为上游服务侧（401/429/parked），非任务设计问题。**可用的发现**：`UseCases::submit_command(actor, ClientCommand) -> CommandReceipt` 是现成入口，`crates/server/src/node_link/command.rs:427-465` 就是同款范本（组 `ClientCommand` → 提交 → `command_status` 取持久 record → 回 `accepted` 带 `turn` 并挂终态观察）。完成判据：派发窗口恢复后重派，或由主代理自行实现；无论哪种，硬门槛不变 —— 交付前必须有一条用例断言 `session.prompt` 首响应 `status == "accepted"`。

## 8. 交付登记（2026-10-15，主 Agent 在主检出完成）

本轮把变更② 从「WP1/WP2 已落地、WP3 零产出」推到**全部任务完成**，并补齐了 plan.md 没有覆盖的接线。逐项证据：

- **WP1/WP2（1.1–1.6、2.1–2.6）**：既有实现，逐条判据在 `crates/server/src/sync/event/` 与 `sync/command/` 的用例里核对后勾选（`session.resume` 的 `command.unsupported`、授权先于读取的记账替身、五条查询命令的形状、结果超限的显式失败、限流三态、连接期去重）。
- **3.1（接受路径）**：硬门槛用例 `a_prompt_mutation_is_accepted_without_any_business_result` 断言首响应 `status == "accepted"` 且 `result`/`terminalEventId` 均为 `null`；`acceptedAt` 由 `an_accepted_mutation_carries_the_acceptance_time` 断言非空。**`session.create` 也已接线**（不再是占位拒绝）：走 core 的 `UseCases::create_session`，feature 门控 `core.session-create.v1`，未登记别名与无权限同样回 `authorization.scope_denied`（不泄露本机登记状态），终态投影失败以 `uncertain` 收口。
- **3.2/3.3（幂等与冲突）**：**落在 core 已有的持久路径上，服务端不重复实现**。判据是 `the_same_request_id_dispatches_only_once_even_after_a_reopen`（同一 requestId 重发只派发一次，且跨「重新打开存储」成立）与 `the_same_request_id_with_a_different_payload_is_a_conflict`，语义比较由 `the_payload_fingerprint_ignores_key_order` 直接断言 ACPR-CJ1 规范化。
  - **修掉一处我自己引入的缺陷**：最初在服务端提交前用 `command_status` 做幂等闸门，那需要 `grant.observe`，会**误拒只持 `grant.interact`** 的设备。改为「提交后按 `(actor, requestId)` 读回持久记录决定回复形状」——这也顺带满足了 §11.2「重复提交已终结的 request 时返回当前终态及原 terminal event ID」。
- **3.4（版本冲突）**：`a_stale_expected_version_is_rejected_with_the_current_version` 断言 `state.version_conflict` + `details.currentVersion` 为**读到的真实版本** + `commits()` 未变（零副作用）。这条要求 `PortError` 之外的一次版本读取，映射写在命令面。
- **3.5（终态观察者）**：`sync::command` 内的有界轮询观察者（250ms，`WATCH_MAX_AGE` 600s），取消路径三条（连接离开注册表 / 到达终态 / 超龄）；`Shutdown` 触发即返回。用例：`the_submitter_receives_its_own_terminal_state`（含终态只推一次）与 `a_disconnected_submitter_leaves_no_watcher_behind`。**接线缺一不可**：组合根与测试 host 都必须 spawn `CommandRoute::dispatch`——漏掉它时 `accepted` 之后永远收不到终态（E2E 首次运行即因此变红，补上后通过）。
- **3.6（uncertain）**：`an_uncertain_command_is_never_dispatched_again` 断言重放回已终结的终态、`prompt_calls` 不增加、`command.status` 给出原 `terminalEventId`。
- **3.2 的持久事实靠替身建模**：测试替身的 `commit` 现在把幂等行落成 `accepted` 的 `CommandRecord`（此前恒为空表，导致接受路径根本无法被验证——这正是前三次派发「30 条用例无一处断言 accepted」的根因）。
- **6.1（组合路由）**：`server::sync::router` 的组合路由 + 组合根注册，另补终态观察者的 spawn。
- **6.2/6.3（明细资源降级）**：按裁决写进 `docs/SYNC_PROTOCOL.md` §9.4/§11.5 与 `docs/FRONTEND_DESIGN.md` §4.2/§6，并同步 §10.3、README、`docs/MODULE_ARCHITECTURE.md` §4.9。
- **7.2**：硬门槛已达成（见 3.1）。

**门禁**：`cargo test -p server` 465 通过、`cargo clippy -p server --all-targets` 无警告、`cargo test -p app --test sync_commands_e2e` 6/6 通过、`npm run check` 10/10 全绿。

**已知未覆盖**：`session.create` 的端到端（需要 workspace 别名 + agent profile + 真实创建链路，本次只做了单元层：接受、feature 门控、错误映射）；`sync_commands_e2e.rs` 不覆盖带 ACP 原文的事件投递（理由见 5.4）。
