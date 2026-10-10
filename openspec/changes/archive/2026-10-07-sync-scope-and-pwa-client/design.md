<!-- 说明实现方案与决策理由；行为以 specs 为准，
-->

## Context

前端工程（`clients/app/`）尚不存在，本机没有任何 `.ts`/`.tsx` 文件。协议侧的合同在 `sync-workspaces-and-create`（2026-10-03 归档）中已全部就位：`session.create`、目录资源、命令目录、feature 登记、固定向量都已冻结。

对账发现的核心事实是：**34 类 Sync 事件里有三类「合同登记了、fixture 有了、但全仓找不到生产者」**——`file.changed`、`agent.connected`/`agent.disconnected`、以及 `session.info.changed` 的生产路径。它们在 `crates/sync-protocol/src/views.rs` 有 DTO 与分派表，在 `fixtures/sync/v1/valid/` 有用例，在 `docs/SYNC_PROTOCOL.md` §10.3 有最低字段表，但在 `core`、`storage-sqlite`、`agent-host` 里都没有产生它们的地方。

同时快照的资源范围有一个真实的不一致：`NODE_LINK_PROTOCOL.md` §12.4 已经写明「正文绝不入快照」，而 `SYNC_PROTOCOL.md` §9.4 只对 imported 会话作此豁免，本地会话的明细仍在快照范围内。按「单条消息 1 MiB、快照 digest 要求整份在客户端拼完才能验证」的约束，本地会话正文进入快照会让首次连接时间随节点上历史消息总量线性劣化。

## Goals / Non-Goals

**Goals:**

- 让 `file.changed` 与 `agent.connected`/`agent.disconnected` 具备生产者，并使 `agent.*` 的 `state` 成为封闭词表。
- 让会话标题可由 Agent 的 `session_info_update` 单向写入，且客户端无重命名入口。
- 把 Sync 快照的资源范围收窄到 `sessions`/`workspaces`/`agents`，与 Node Link 的既有口径一致。
- 为 `session.read` 建立可翻页的明细读取路径，且不改变 Node Link 侧同名命令的行为。
- 交付 `clients/app/` 的 Web/PWA 第一版：能配对、能浏览目录、能打开会话、能对话。

**Non-Goals:**

- `server::sync` 入站适配层与 `/ui` 静态托管。前端第一版以 `fixtures/sync/v1/` 为数据源开发，这两者是独立变更。
- CI 门禁与 npm workspaces（用户明确选择不改）。
- 前端状态库与表单库选型（`FRONTEND_DESIGN.md` §11 暂缓项）。

## Decisions

### D1 快照只承载清单类资源

**决定**：`sync.snapshot_*` 只下发 `sessions`、`workspaces`、`agents`；五类 session-scoped 明细资源不进快照，对本地会话同样适用。`chunkCount` 随之收缩（全量为 3，未协商目录 feature 时为 1）。

**理由**：这不是性能优化，而是与既有决策对齐——`NODE_LINK_PROTOCOL.md` §12.4 已经写死「正文绝不入快照」，Sync 是唯一的例外。而且这个例外在结构上无法自洽：`snapshotDigest` 要求客户端把整份快照拼完才能验证，而明细资源的体量与会话消息总量成正比，无界。

**被否决的替代**：

| 方案 | 否决理由 |
| --- | --- |
| 快照只对 imported 会话豁免明细（现状） | 与 Node Link 不一致；本地会话仍是无界来源 |
| 快照带最近 N 个会话的明细做首屏加速 | 首屏加速在目录页用不上（目录页只要摘要）；正文去重要按 `messageId` 引入第二套幂等路径 |
| 快照带全部会话的明细 | 与 Node Link 冲突；体量无界 |
| 给快照加条数上限与截断信号 | 需要在 `snapshot_end` 增加截断语义与 UI 表达，而「明细一律现取」让这条路径根本不会发生 |

**连带影响**：客户端**没有**「待处理授权请求」的离线计数来源以外的东西——实际上有：`SessionSummary.state` 含 `waiting_permission`/`waiting_input`，目录页的「待处理 N」徽标由摘要得出（原型 `:1379`）。交互细节经 `session.read { include: ["pending_interactions"] }` 按需取得。

### D2 `session.read` 分页游标用 `(createdAt, messageId)` 复合键

**决定**：`session.read` 的 payload 增加可选 `before`（复合游标）与 `limit`；结果增加一个指示是否仍有更早内容的布尔值。默认页为 20 条用户输入对应的范围。

**理由**：`snapshotItem.messages` 的字段是 `{messageId, sessionId, role, content, status, createdAt, turnId}`——**没有序号字段**，所以拿不到单调序号做游标。而 `SYNC_PROTOCOL.md` §3 明令「排序不得依赖 UUID」，因此即便 `messageId` 是 UUIDv7 也不能拿来排序。`createdAt` 是 Daemon 持久化时间（同文：「不使用客户端时间决定顺序」），可信；`messageId` 作 tie-breaker 保证同一时刻的多条不重不漏。

**「20 条用户输入」这个口径**：用户输入（`role: "user"`）与 turn 边界天然接近，是用户能数得清的量。服务端按此计数取页，使一页的体积对用户可预期。

**被否决的替代**：

| 方案 | 否决理由 |
| --- | --- |
| 单用 `before: messageId` | 无序号字段支撑；违反「排序不得依赖 UUID」 |
| 单用 `before: createdAt` | 同一毫秒的多条会漏或重 |
| 偏移量分页（`offset`/`limit`） | 新消息插入时页边界漂移，重翻页会重复或跳条 |
| 引入 `sessionSequence` 到消息上 | 要改 `owned_message` 表与 `snapshotItem.messages` 的必填集合，改动面远大于收益；事件侧已有 `sessionSequence`，消息侧复用它需要重新论证权威来源 |

### D3 分页只加 Sync 侧

**决定**：`session.read` 的分页参数只在 Sync 传输面生效；`crates/node-link-protocol` 的命令 DTO 与 schema 不变。

**理由**：`sync-protocol` 与 `node-link-protocol` 是两份独立的 crate 与两份独立的 schema，`commands.json` 里的 `transport: ["sync","node_link"]` 只表示「这条命令在两个面上都存在」，不要求载荷同形。而 Node Link 侧的正文有 `no-content-cache` 硬约束（`NODE_LINK_PROTOCOL.md` §12.4），Access 侧根本不保留正文，实际不需要分页。

**代价**（明确记录）：同一个命令名在两个协议下的载荷形状不同。缓解措施是在两份协议文档各自的命令表里写明适用面，`SESSION_PROTOCOL` 的表加一行「分页参数仅 Sync 侧生效」。

### D4 `file.changed` 的行数用行级 diff，不用行数差

**决定**：新增 `addedLines`/`deletedLines` 两个可选 `decimalString` 字段，由 core 对 ACP `Diff` 的 `oldText`/`newText` 跑行级 Myers diff 得出。

**理由**：行数差是错的且错得很隐蔽——`oldText` 10 行换成 `newText` 10 行，行数差为 0，但实际可能是「删 2 加 2」，UI 会显示「无改动」。Myers 是 O(ND)（D 为编辑距离），典型编辑的 D 很小。

**为什么用 `decimalString` 而不是 JSON number**：与 `terminal.output.chunkIndex`、`agentContentBlock.byteLength` 一致；`AGENTS.md §3` 对超过 JavaScript safe integer 的数字有明确要求，计数沿用十进制字符串可以彻底绕开这个边界。

**判定不出时省略而非填零**：若 Diff 缺少 `newText`（schema 上是 required，但 `x-deserialize-default-on-error` 允许降级路径），则省略这两个字段。原型那句「不会用 0 冒充」在这里同样成立。

### D5 `displayPath` 相对化，越界显式标记

**决定**：新增可选布尔字段 `outsideWorkspace`。路径在会话 workspace 根之下 → 下发相对形式；经规范化后不在其下 → 置 `outsideWorkspace: true` 且只下发文件名称。

**理由**：ACP `Diff.path` 的上游文档写的是「The absolute file path being modified」，而 `SECURITY_DESIGN.md` §12.3 与 `CORE_PORTS_AND_STORAGE.md` §3.6 禁止本机路径出现在任何对端可见输出中。`crates/acp-protocol/src/content.rs` 的注释明确「Agent 报什么就是什么，本 crate 不解析、不规范化」——所以相对化必须发生在 core。

**越界不静默降级的原因**：Agent 完全可能报 `C:\Windows\...` 或 `../../etc/passwd`。若静默剥成 basename，用户看到 `config.ts` 却不知道是哪儿的；若下发 `../../Windows/...` 则等于泄漏目录结构。所以显式标记，让 UI 能说清「这个文件在工作区之外」。

**前缀判定必须用规范化结果**：字符串前缀相同但不在其下的情况（`/work/api` vs `/work/api-tools`）必须判为越界。

**被否决的替代**：静默剥 basename（信息丢失且不可区分）；下发带 `../` 的相对形式（泄漏目录结构）。

### D6 Agent 连接状态是节点级，列表来自快照、状态来自事件

**决定**：`agent.connected`/`agent.disconnected` 是节点级事件（`sessionId: null`），由 `agent-host` 的 profile 进程建立/退出产生。`state` 收成封闭词表（连接事件取值 `connected`，断开事件取值 `disconnected`）。客户端的 Agent 列表来自 `agents` 快照资源，事件只作覆盖层。

**投递通道的现状（Round 1 独立审查 F5 的发现）**：这两个事件是**节点级**（会话标识为空），而当前唯一实现的入站适配器 `crates/server/src/node_link/resource.rs` 按会话归属投递——`event.session.is_none()` 时直接返回，`fan_out` 同样先判 `Some(session)`，注释写明「非会话级事件没有会话级 origin cursor，也不属于任何 attachment」。因此**本次变更不要求这两类事件到达任何客户端**：本变更交付的是产生与持久化，客户端的覆盖层在同步入站面落地前一律呈现「状态未知」。让 `node_link` 投递节点级事件需要改 `NODE_LINK_PROTOCOL` 的事件语义，超出本次范围（已由用户 2026-10-03 裁定为「甲」方案）。
**理由**：`crates/agent-host/src/host.rs` 的既有注释「复用到的既有进程可能正在服务其它会话」确认了进程按 profile 复用，所以生命周期归属 profile 而非会话。

**列表必须来自快照，不能来自事件**：`storage.sync_event_retention_days` 默认 7 天，事件过期后若以事件为准推导列表，同一会话在不同时间打开会显示不同的 Agent 集合——这违反「一个状态只能有一个权威写入者」的精神。以快照为本体、事件为覆盖层后，事件缺失只导致「状态未知」，列表始终可复现。

**`state` 保留而不是删除**：它在两个 fixture 里已存在且取值恒定，但删除它要改 schema + fixture + `views.rs` + §10.3 表格四处，而保留并收窄词表只改同样四处却保留了未来承载更多取值的空间。词表收窄必须同时登记 `views.rs` 的 `VIEW_ENUMS`（`tests/schema_drift.rs::view_enums_match_schema` 双向门禁：登记的必须等于 schema，schema 里的每个 enum 都必须被登记）。

**不表达「运行中/空闲」**：那是会话的活跃度（`SessionSummary.state` 的七个取值），不是 profile 进程的状态。混进来会让一个被 5 个会话复用的进程无法定义「忙」。

**DDL 已就位**：`owned_event` 的 `session_id` 可空，且有三条 CHECK 保证节点级事件的字段组合自洽（`session_id IS NULL` ⟺ `session_sequence IS NULL` ⟺ `origin_epoch IS NULL` ⟺ `origin_sequence IS NULL`），**不需要 migration**。

### D7 会话标题单向由 Agent 写入

**决定**：`SessionUpdate` 增加标题字段，唯一来源是 ACP `session_info_update` 的投影；不新增重命名命令。

**理由**：ACP 上游 `SessionInfoUpdate` 的文档写明「Agents send this notification to update session information like title ... This allows clients to display dynamic session names」，这正是目录页标题的原生来源。而当前 `core` 的 `SessionUpdate` 结构体里没有标题字段、`create` 路径恒写 `None`（`broker.rs` 的 `StateChange::Create(NewSession { title: None, .. })`），所以目录页现在只能显示「未命名会话」。

**partial update 语义**：ACP 的 `SessionInfoUpdate` 是「所有字段可选以支持部分更新」。因此标题字段必须是两层可选——外层缺席表示不改，内层显式为空表示清空。否则只收到 `updatedAt` 的通知会误清空标题。

**`updatedAt` 不采用 Agent 自报值**：排序必须确定（`session_store.rs:2048` 已是 `ORDER BY s.updated_at DESC, s.session_id ASC`），采用 Agent 自报时间会允许任意夸大或写错。视图里的 `updatedAt` 字段照常转发，但会话的权威更新时间取 Daemon 持久化时间——这与 `SYNC_PROTOCOL.md` §10.2 的「`createdAt` 是 Daemon 持久化时间，不使用客户端时间决定顺序」同源。

**无重命名入口**：`commands.json` 不新增命令；`session.info.changed` 是单向事件，客户端构造它不成立。

### D8 前端分七层，数据来源单一

**决定**：`clients/app/` 按 `FRONTEND_DESIGN.md` §3 的七层组织。数据入口只有一条——打开会话页时若本地无该会话明细则发 `session.read`，向上滚动按游标续取。

**理由**：D1 定下「正文一律现取」之后，前端不再需要处理「快照里的明细」与「读回来的明细」如何合并去重。这消除了前端最大的一块状态机复杂度，也与 `AGENTS.md §3` 的事件去重要求解耦（事件侧仍按 `eventId` 去重，明细侧不需要第二套幂等）。

**设备身份**：`CryptoKey` 不可导出，持久化到 IndexedDB；这是「WebCrypto 能否与 Rust 侧 P1363 固定向量对齐」的唯一风险点，所以 `fixtures/sync/v1/transcripts/device-proof.json` 的对拍必须在第一刀完成，不能留到最后。

**前端对 fixtures 开发的风险控制**：`src/protocol/` 严格 typed，生成或显式定义 DTO 后不做宽容解析；不做「猜测服务端语义」的兼容分支。`AGENTS.md` §5 要求「Rust 和 TypeScript 必须消费同一 manifest」，因此前端测试直接读 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`，不另建一套样例。

## Risks / Trade-offs

- **[节点级 Agent 事件在本变更内没有投递通道]** → `crates/server/src/node_link/resource.rs` 的会话级投递路径会丢弃会话标识为空的事件，而 `server::sync` 未落地。已由用户 2026-10-03 裁定为「甲」方案：本变更只交付产生与持久化，`core-derived-events` 的对应需求据此收窄为「持久化 + 不被会话级路径误收」；客户端覆盖层在事件通道就绪前一律呈现「状态未知」，`pwa-web-client` 的对应需求同步收窄。恢复完整交付的前置条件是同步入站面落地，届时需按 `NODE_LINK_PROTOCOL` 决定是否扩展节点级事件语义。
- **[路径相对化会读到会话的工作目录，而会话可能未登记 workspace]** → 解析失败时按越界处理（`outsideWorkspace: true` + 只给 basename），不静默下发绝对路径；这条口径在 specs 的场景「工作区之外的路径被显式标记」中被断言。
- **[`session.read` 分页只在 Sync 侧生效，同名命令两载荷不同形]** → 在两份协议文档的命令表各自写明适用面，并在 `core` 的 `required_grant` 镜像处不引入分页相关的授权判定。
- **[Myers diff 在病态输入上的成本]** → 单文件行数在千级时 D 很小；若实测出现大 D，退化为按块的直方图 diff 并在 design 记录口径变化（行数是提示性字段，不影响 ACP 原文保真）。
- **[快照收窄后，「快照只有清单」这条规则没有机器强制]** → 由 `server::sync` 实现时的组装逻辑保证；本变更把它写进 specs 的场景「本地会话的明细不进入快照」，并在固定向量层保留一个只含清单资源的快照向量。
- **[前端在没有 `server::sync` 的情况下开发 UI，语义可能与真实服务端有偏差]** → 协议层严格 typed + 直接消费 fixtures + 不做宽容解析；真正的不一致在接入 `server::sync` 时暴露，届时由合同门禁兜底。
- **[前端工程的依赖审计不在 CI 覆盖内]** → 用户已明确选择不改门禁；该权衡记录在 proposal 的 `non_goals`，前端依赖审计留给其自身流程。
- **[`state` 收窄成枚举后，既有 fixture 的取值必须与 schema 一致]** → `schema_drift.rs` 的双向门禁会在 `npm run check` 阶段直接失败，不会漏到运行时。

## Migration Plan

无数据迁移。`owned_session.title` 列已存在，`owned_event` 的 DDL 已支持节点级事件，不推进文件格式版本、不新增列、不改 `CHECK` 约束。

协议侧是 v1 内的形状收窄，快照与 `session.read` 尚未有实现（`server::sync` 未落地），因此**不存在需要兼容的既有部署**；若此前已有本地消费 `file.changed` 的实验性代码，随本次字段扩展一并对齐即可。

回滚条件：若 D1 或 D2 在实现中出现无法在本变更内解决的阻塞（例如 `session.read` 分页与 `command.uncertain` 的交互需要重新裁决），按能力边界回滚——`sync-snapshot-scope` 与 `core-derived-events` 是两个独立能力，可单独撤回而不影响前端工程的目录页与对话页骨架。