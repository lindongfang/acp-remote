<!-- 说明实现方案与决策理由；行为以 specs 为准，
-->

## Context

动机见 `proposal.md` 的 Why。影响方案的现状事实（均为本次变更前已核实的代码/合同事实）：

- `SessionBackendFactory`（`crates/core/src/ports.rs`）只有 `create` 与 `open`；`open` 只重新绑定**还活着**的进程内会话（`crates/agent-host/src/host.rs` 在 `supervisor.is_running()` 为假时返回 `SessionClosed`），因此进程不在时没有恢复入口。
- Agent 侧 ACP 会话标识只存在于 `agent-host` 的内存映射（`by_acp`/`by_core`，`crates/agent-host/src/session.rs` 的 `acp_session_id`），`owned_session`（`crates/storage-sqlite/src/migrate.rs` 的 `OWNED_SCHEMA_V1`）没有任何承载它的列，也没有创建时 cwd 的列。
- `session/resume` 在 `crates/acp-protocol/src/methods.rs` 已登记为已知方法（`Delivery::PostMvp`），`crates/acp-protocol/src/capability.rs` 的 `supports_session_resume()` 已按「`sessionCapabilities.resume` 省略或 `null` 即不支持，`{}` 即支持」实现；但没有任何类型化请求/响应 DTO。
- `compatibility/acp/v1/matrix.json` 中 `method.session_resume` 与 `cap.agent.session_resume` 仍为 `post_mvp`；`compatibility/commands/v1/commands.json` 没有 `session.resume`；Node Link 的 `CommandName`（`crates/node-link-protocol/src/command.rs`）固定 12 个命令。
- `server::node_link` 已落地 Owner 侧入站面并路由 `session.list`/`session.create`；`server::transport::net` 与 `app` 组合根已接线。
- `docs/SESSION_CONTINUITY_DESIGN.md` §12.1 的七条决策已定，其中与本次相关的是 B2、决策 2（不自动降级）、决策 3（复用 `grant.remote-work`）、决策 4（不读 Agent 原生历史）。

## Goals / Non-Goals

**Goals:**

- 让「进程不在的 owned 会话」有一条受授权、可幂等、可显式失败的恢复路径（B2）。
- 把恢复所需的全部状态落到 Owner 的权威存储里，且不引入新的授权维度或协议封闭词表条目。
- 让能力协商保持真实：不支持就是显式不支持，且不产生副作用。

**Non-Goals:**

- 不实现 A/C/D/D+（见 `proposal.md` 的 non_goals）。
- 不改变 `session/load` 的既有语义；不为恢复引入任何绕过 Export/配对/grant 的捷径。
- 不改变威胁模型、信任边界或 `local.*` 管理面。

## Decisions

### D1 触发通道与命令形状（对应设计文档 D1/D5；已定回引）

采用 B2（ACP `session/resume`）。恢复以新增的 Node Link 命令 `session.resume` 触发，原因：恢复是 Owner 本机副作用（拉起进程），必须走既有的「授权 → 幂等 → accepted → terminal」命令管线，而不是新增旁路。

**冻结的 wire 合同**：

| 项 | 取值 |
| --- | --- |
| 命令名 | `session.resume` |
| `kind` | `mutation` |
| `grant` | `grant.remote-work`（复用，不新增维度） |
| `pack` | `null`（与 `session.create` 同形） |
| `transport` | `["node_link"]`（Sync/PWA 面与 Access 侧客户端属后续切片） |
| `delivery` | `conditional_mvp`（取决于目标 Agent 的能力宣告） |
| payload | 空对象 `{}`；出现任何键 → `command.rejected` / `nodelink.command.unsupported_field` |
| `expectedVersion` | `null` |
| `sessionRef`/`attachmentId`/`attachmentGeneration` | 必须为指向该 Export 可见会话的非 `null` 值 |
| terminal.result（`completed`） | `SessionResumeResult`，字段形状与既有 `SessionCreateResult` 一致（`remoteSessionRef` + `sessionMeta`） |
| 能力不支持 | `command.terminal` 失败，错误码 `nodelink.command.unsupported` |

不新增错误码、不新增 feature ID：`nodelink.command.unsupported`、`nodelink.command.unsupported_field`、`nodelink.internal.unavailable` 都已在 `compatibility/errors/v1/errors.json` 中。

替代方案与不选原因：
- **让 `session.resume` 复用 `session.create` 的 payload**：拒绝。恢复的输入必须全部来自 Owner 的持久化记录；允许客户端传 `agentId`/`cwd` 会把「恢复」变成「带参新建」，并重新引入路径注入面。
- **在 `open()` 内部隐式尝试 resume**：拒绝。`open` 的语义是「重新绑定活着的会话」，把恢复藏进去会让失败分类与副作用（spawn）不可预期，也违反「显式优于隐式」。

### D2 持久化形状（对应设计文档 D3；本次冻结）

`owned_session` 追加两列（只追加、不触发表重建）：

```
agent_session_id TEXT -- 可空，无默认值；创建取得 ACP 会话标识后写入，此后只读
workspace_cwd TEXT -- 可空，无默认值；创建时解析出的规范化绝对路径，此后只读
```

- 文件格式版本与 owned 家族版本推进到 **v5**；imported 家族保持 **v3**。
- `NULL` 的语义是「该会话没有可用于恢复的数据」，恢复必须显式失败，MUST NOT 用别名重解析补齐。
- `workspace_cwd` 由 core 在创建时用自己已解析的 `ResolvedWorkspace::canonical_path()` 写入，不依赖后端回报（后端拿到的就是同一份解析结果）。
- `agent_session_id` 由 core 在 `create_session` 内、`factory.create` 成功返回后**紧接着提交的一次** `StateChange::Update` 中落盘（与 `workspace_cwd` 同一提交），**不**等适配层的终态提交；`settle_session_create` 的签名不变。取值：`agent_session_id` 来自 `endpoint.agent_session_id()`，`workspace_cwd` 来自 core 自己已解析的 `ResolvedWorkspace::canonical_path()`（**不依赖后端回报**）。按 spec R7，`agent_session_id()` 为 `None` 时两列都不写。
 - **可观察后果（DR1-F46）**：新建会话在 Create 提交（v1）之后、紧接着的 Update 提交（v2）之后，其**可见版本为 2**；`docs/NODE_LINK_PROTOCOL.md` §12.7 示例里的 `"version": "1"` 属**非规范示例**，不改写但记录此读法。且在两次提交之间的窄窗口内，本地客户端若先读到 v1 再带 `expectedVersion = 1` 提交写命令，会得到正常的 `state.version_conflict`（乐观并发的既有结果，非数据不一致）。
 - （**契约订正 2026-09-30，DR1 Round 9**）此处原写「写入 `session.create` 终态的同一次提交」，经 WP3 实施核对发现**在本架构下不可实现**：`session.create` 的终态由适配层提交，而适配层在 settle **之前**已用 `session_create_result` 投影 `sessionMeta`（`crates/server/src/node_link/command.rs` 先投影、后 `settle_session_create`），且 `StateChange::Update` 会 bump version；若在终态提交里写这两列，回给 Access 的 `sessionMeta.version` 会与落盘值错开，且 `settle_session_create` 拿不到 core 解析出的 cwd。改为上述提交点后：**spec R6 字面满足**（「core 可在同一会话创建流程的提交中把它与已解析的规范化目录一起写入该会话行」）、版本不自相矛盾、且崩溃窗口更稳（崩在 spawn 与终态之间时行里已有标识，可恢复），而 WP6 的调用点零改动。
- core 侧新增值对象 `AgentSessionId`（非空、有长度上限、无 NUL），不新增裸 `String` 传递。

替代方案与不选原因：
- **把 ACP 会话标识塞进 `owned_event`/元数据 JSON**：拒绝。恢复是热路径上的强类型读取，放在无 schema 的 JSON 里无法被门禁与迁移覆盖。
- **`NOT NULL DEFAULT ''`**：拒绝。空串无法与「未取得」区分，且会让「可恢复」判定被默认值蒙蔽。

### D3 端口与用例形状（对应设计文档 D6/D7）

```text
core::ports::SessionBackendFactory
 + async fn resume(&self, session: &SessionId, request: ResumeSessionRequest, sink: EventSink)
 -> Result<Box<dyn SessionEndpoint>, PortError>;

core::model::ResumeSessionRequest { agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }
// workspace_cwd = 持久化的「创建时 canonical path」原文；构造约束同 ResolvedWorkspace::canonical_path（非空、≤4096 字节、无 NUL、绝对路径形状）
// **不含 alias**（订正见下方 Round 13 注记）

core::ports::SessionEndpoint
 + fn agent_session_id(&self) -> Option<&AgentSessionId>; // 创建后供 core 落盘；恢复得到的端点同样可读
```

`resume_session` 用例的顺序（**授权先于一切本机读取**）：

```text
1. 授权：按 grant.remote-work 判定（与 session.create 同口径；未授权 → 无副作用拒绝）
2. 读取该会话行：agent_id / agent_session_id / workspace_cwd
 ├─ 会话不属于本节点或不可见 → 与「不存在」不可区分地拒绝
 └─ 任一恢复字段为 NULL → 显式不支持（`nodelink.command.unsupported`），不启动进程
3. cwd 复校验：对 `request.workspace_cwd`（持久化原文）执行同创建口径的校验（绝对、存在、是目录、`canonicalize` 结果与持久化值一致）→ 失败返回服务端不可用类错误
4. factory.resume(...)（可能 spawn 进程；这是唯一副作用）
5. `resume_session` 返回 `SessionId`（会话状态抬回可交互态）；**终态由适配层投影并结算**——适配层用与 `session_create_result` **同源**的映射（它独享 `remoteSessionRef.exportId`）构造 `SessionResumeResult`，再经 core 的 `settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` 落盘（与 `settle_session_create` 同形，**只**终结 `session.resume` 的持久记录）。投影失败时适配层按既有 `session.create` 模式结 `uncertain`（不用 `failed` 撒谎）。event/sequence 仍遵守「先持久化成功再广播」与「每会话最多一个 active turn」。
 > （**契约订正 2026-09-30，DR1 Round 10**）此处原写「core 用例把会话状态抬回可交互态**并写终态**」，经 Round 9（DR1-F41）核实**无可行承担者**：R27/R34 要求 `session.resume` 的 `completed` 终态结果为 `SessionResumeResult`，而 `remoteSessionRef.exportId` 只有适配层有（core 的 `ResumeSessionRequest` 无 export 信息，Export 可见性是适配器单点策略）；core 对外唯一的命令终态入口 `settle_session_create` 在 `crates/core/src/broker.rs` **显式拒绝** `command != "session.create"` 的记录，且 `use_cases.rs` 没有第二个通用 settle 入口。改为上面的「core 返回 SessionId + 适配层投影 + core 新增 `settle_session_resume` 结算」后，与既有 `session.create` 模式完全同形，且 core 不需要 export 信息。

> **`ResumeSessionRequest` 不含 alias（契约订正 2026-09-30，DR1 Round 13，回应 WP3 的开工实测）**：原冻结形状 `workspace: ResolvedWorkspace` 要求带 `WorkspaceAlias`，但**恢复路径上没有任何权威 alias 来源**——`owned_session` 的 DDL（v5 只加 `agent_session_id`/`workspace_cwd` 两列）**没有 workspace/alias 列**；alias↔path 的映射在 `owned_workspace` 表里，而它**可以被重指向或删除**（`specs/workspace-resolution` 与 TP1 的 SR-R26-1 正是在测「同一 alias 重指向另一目录后 resume 仍须用持久化路径」）；该规格又明令恢复**不得**按别名重新解析。故把 `workspace` 换成 `workspace_cwd: String`（持久化 canonical path 原文）：直接满足 SR-R26-1 与「不得按别名重解析」，`workspace_cwd` 逐字等于持久化值，D4 发送的 `cwd` 就是它。**不引入虚构值**：备选「反查 `owned_workspace` 取 alias、查不到则用保留标签」会在 alias 已重指向/删除时写入一个**不成立的断言**（声称「alias X 对应 path Y」），且反查结果不改变任何行为、只多一个端口调用与失败模式，故不采用；「再持久化 alias 为第三列」超出 D2 的「只加两列」与 WP4 写范围，亦不采用。

> **入口形状已钉死（契约订正 2026-09-30，DR1 Round 11，回应 DR1-F48）**：`resume_session` **完全镜像 `create_session`**——**core 不新增 `CommandPayload::SessionResume` 变体**、不改 `command_name`/`command_kind`/`family`、不加通用分发臂；它的 `accepted` 行与幂等行由 `resume_session` **自建**（与 `create_session` 同一事务形态）。适配层的 `core_payload()` 对 `WirePayload::SessionResume` **按 `SessionCreate` 的先例早退**（`return Err("… is dispatched by its own handler")`），不映射到任何 core payload。理由：① 与既有 `session.create` 完全同形，`docs/CORE_PORTS_AND_STORAGE.md` §3.3 的「`CommandPayload` 按命令名一对一；`session.create`/`session.resume` 均不在其中」保持为真；② 避开通用 `submit_mutation` 管线「先产生副作用再回 `accepted`」的顺序问题（`session.create` 是**先回 accepted 再工作**）；③ 不让 `settle_session_resume` 因通用 mutation 臂已提交终态而退化为幂等 no-op（那会使 `command.status` 读到非 `SessionResumeResult`，违反 R27/R34）。
>
> **F49 一并闭环（不改写范围）**：上条新入口的持久化语义（**只**终结 `session.resume` 记录；无记录/已终结时为幂等 no-op；崩溃窗口走 §6 第 16 条的 `recover_unsettled`）**只写在 §5.1 的 `[决定]` 段落**（该段落在 WP3 既有的 `docs/CORE_PORTS_AND_STORAGE.md` §5.1 写范围内），**不**改 §6 第 20 条。
>
> **F45 注释层闭环**：`crates/core/src/model/session.rs` 中「`CommandPayload` 按命令名一对一；`session.create` 不在其中」的既有注释**同时补上 `session.resume`**（该文件在 WP3 的 `crates/core/` 写范围内）。
```

幂等与不确定：沿用既有 mutation 幂等键 `(ownerNodeId, accessNodeId, requestId)`；`session.resume` 会 spawn 进程，因此在「已 accepted、尚未确认副作用」的窗口内崩溃 MUST 落 `uncertain` 终态，MUST NOT 自动重试。

失败到错误码的映射（全部使用既有码）：
- Agent 未宣告 `sessionCapabilities.resume` → core 侧以「后端不支持该操作」表达，Node Link 终态错误码 `nodelink.command.unsupported`。
- cwd 复校验失败 → core 侧 `PortError::Unavailable(..)`（服务端不可用类），Node Link 终态错误码 `nodelink.internal.unavailable`。
- 该会话没有持久化恢复数据（两列为 NULL）→ 与能力不支持同一条路径（`nodelink.command.unsupported`），**不降级为新建会话**。

> **错误映射的归属（契约订正 2026-09-30，DR1 Round 9）**：core 对上述两类「不支持」统一返回 `PortError::Unavailable(UnavailableKind::BackendUnsupported)`（新增取值，**不提供默认实现**）；**由 WP6** 在 `crates/server/src/node_link/command.rs` 的 `port_error_code` 新增一条臂，把该 kind 映射为既有 wire 码 `nodelink.command.unsupported`（不新增任何错误码或 feature ID）。`port_error_public` 对该取值的显式臂取既有码 `command.unsupported`，本地管理适配器按 D3 映射为既有 `local.unavailable`。选定理由：core 不应直接吐 wire 码（`InvalidRequest` 虽然是现成漏斗且有 `InvalidRequest("nodelink.origin_epoch_mismatch")` 先例，但那是越层表达）；且 WP6 已拥有整个 `crates/server/`。
>
> **恢复数据的读取形状（实现决定，已在 WP3 内裁定）**：两列**不**进 `Session`/`SessionSummary`（那会把本机规范化路径带进可投影形状，违反 §3.6 的「`canonical_path` 不进事件/错误 details/审计 detail 前像/Node Link catalog」边界）；改为 `SessionStore::load_recovery(&SessionId) -> Result<Option<SessionRecoveryRecord>, PortError>` 窄读取。代价：新增一个必需 trait 方法，**7 处实现 / 6 个文件**均需实现——`crates/core/src/broker.rs` 的 `FakeStore`（WP3）、`crates/storage-sqlite/src/session_store.rs` 的 `SqliteStore`（WP4）、`crates/app/tests/support/owner.rs` 的 `FlakySessionStore`、`crates/server/src/local_admin/test_support.rs` 的 **`NotTouched` 与 `FixedStore` 两个替身**、`crates/server/src/node_link/command/tests.rs` 的 `CommandStore`、`crates/server/src/node_link/resource/tests.rs` 的 `SliceStore`（后四者属 WP6 的整 crate 写范围）。其权威形状登记在 `docs/CORE_PORTS_AND_STORAGE.md` §3.6（WP3 写范围内），并注明「不进 `Session`/`SessionSummary`」。

> **契约修正（CR7-F2，2026-09-30）**：`specs/workspace-resolution/spec.md` 原先把「持久化取值为 `NULL`」归入「服务端不可用类错误」，与本条冲突（同一输入两个错误码）。已把规格修正为：`NULL` 走与能力不支持同一条路径（`nodelink.command.unsupported`），其余校验失败仍为服务端不可用类。并同步 `docs/CORE_PORTS_AND_STORAGE.md` §2 的错误枚举与 `ALL`/`as_str`、以及 `port_error_public` 的显式覆盖与本地管理映射（映射到已有的 `local.unavailable`）。这是 **core 内部**词表，不改变任何协议封闭词表。

### D4 后端恢复路径与能力门控（对应设计文档 D4/D9）

`agent-host` 的 `resume` 实现要点：

1. 按 `request.agent` 解析本机 profile；不可用时返回「后端不可用」类错误（`Unavailable(KeystoreUnavailable)` 等既有类别）。
2. 复用既有启动路径（`launch`/`process`/`platform`：stdio 分帧、stderr 有界采集、Windows Job Object / Unix 进程组清理、凭据注入边界）。
3. `initialize` 后读取协商能力：`!supports_session_resume()` → 立即关闭该子进程并返回「后端不支持」（**不发送** `session/resume`）。
4. 宣告支持时发送 `session/resume { sessionId: <持久化标识>, cwd: <持久化目录> }`；Agent 返回错误 → 明确失败并清理进程；成功 → 构造端点并登记到运行时映射，使同一 core 会话只有一条活跃绑定（旧绑定先让出）。

不选「先探测能力再决定是否 spawn」的原因：能力只有经 `initialize` 才能得知，而 `initialize` 本身就需要进程；因此门控必须发生在 spawn 之后、`session/resume` 之前，并由终态错误如实回报。**代价**：一次恢复失败会短暂启动并清理一个子进程；这是「不得虚报能力」的必要代价。

> **契约修正（CR7-F1，2026-09-30）**：本地规格原先写「未宣告时 MUST NOT 启动 Agent 进程」，与本决策不可兼得（能力只能经 `initialize` 得知）。已把规格修正为可满足的可观察保证：「MUST NOT 发送 `session/resume`」+「MUST 在返回前终止并回收本次为恢复而拉起的子进程」；设计侧不变。

| 资产 | 改动 |
| --- | --- |
| `compatibility/acp/v1/matrix.json` | `method.session_resume`、`cap.agent.session_resume` 的 `delivery` 提升为 `conditional_mvp`；`layers` 改为 `{ acp: native, broker: project_and_preserve, sync: explicit_unsupported, pwa: explicit_unsupported, facade: not_advertised }`（facade 尚未落地，不宣告）。**`broker` 必须是 schema 枚举内的取值**：`schemas/acp/compatibility-matrix.schema.json` 的 `$defs.layers.broker` 枚举为 `project_and_preserve \| local_service \| pass_through \| explicit_unsupported \| not_applicable`（无 `native`，且 `additionalProperties: false`，由 `check:acp` 用 ajv 强校验）；取 `project_and_preserve` 与 `method.session_prompt` 同值，语义为「core 建公共领域视图并保留 raw」 |
| `crates/identity-auth/` | `src/authorization.rs` 的 `GRANTS` 把 `session.resume` 加入 `grant.remote-work` 会员；既有 `tests/authorization.rs` 的会员与 `scopes.len()` 断言随之同步（该测试用 `include_str!` 直接读 `commands.json`，属封闭词表原子点的一部分） |
| `docs/IDENTITY_AND_AUTH_CONTRACT.md` | **no-op**（已核实：该文档不列举 grant 会员，只说明镜像存在）；在交付说明中记录该判定理由 |
| `compatibility/commands/v1/commands.json` | 追加 `session.resume`（见 D1）；node_link 子集 12 → 13 |
| `schemas/node-link/v1/`、`fixtures/node-link/v1/` | 新命令的 payload/结果与固定向量 |
| `crates/core/src/broker.rs` | `required_grant` 追加一条臂（封闭词表门禁的 Rust 镜像，见 D6） |
| `docs/SYNC_PROTOCOL.md` | §11.5 命令表与「node-link 全部 12 条」等计数文本（门禁按集合相等断言，必须同批改） |
| `docs/SECURITY_DESIGN.md` | §10.2 命令、scope、pack 与 grant 表格（同上） |
| `scripts/check-command-catalog.mjs` | `pack: null` 豁免的判据从「按命令名」改为「按 transport」（见 D6） |
| `AGENTS.md`、`README.md`、`.github/workflows/ci.yml` | 门禁规则调整后的四处说明同步（见 D6） |
| `docs/NODE_LINK_PROTOCOL.md` | §12.5/§12.7 命令表与 payload/结果、§15 顺序与重放（`session.resume` 的 uncertain 语义）、**§10 的 grant→命令表**（`grant.remote-work` 会员行现为 `session.create`，须加 `session.resume`；门禁不覆盖该表，故必须人工同步）。**DR1-F59 补齐（2026-10-01）**：另含 WP6 的两处——**§12.7 的 `:662` 收窄句**（Owner 只为 `session.list`/`session.create`/`session.resume` 投影 wire 结果）与 §12.7 示例的「可见版本为 2」非规范注记 |
| `docs/ACP_COMPATIBILITY_MATRIX.md` | §3.3/§3.4 layers 与 delivery 口径、§6 facade 映射、§7 Agent 实现差异 |
| `docs/CORE_PORTS_AND_STORAGE.md` | §5.1 端口签名（`resume`、`agent_session_id`）、§7.3 `owned_session` DDL 与 v5、§2 错误枚举新取值。**DR1-F59 补齐（资产级范围，2026-10-01）**：另含 §3.1/§3.3（`session.create`/`session.resume` 走专用路径）、§3.6（`ResumeSessionRequest`/`SessionRecoveryRecord`）、§4（`SessionLifecycle`）、§5.2、§7 标题与 §7.2、§9 判据 1/28 的版本链（v1→…→v5）、§11.3 的「过新」取值（随 v5 为 6）、文件头版本记录。**逐 WP 的具体区域以 计划 的 `## Shared File Ownership` 为准，本表只登记资产级范围** |
| `docs/SESSION_CONTINUITY_DESIGN.md` | 头部状态注记改为「B2 已实施」并标注未实施部分（A/C/D 的边界不变；§8 中 D3/D4/D6/D7/D9 指向本变更） |
| `README.md`、`docs/DEVELOPMENT_PLAN.md` | 仓库当前状态与切片状态 |

`session/load` 在 `matrix.json` 中保持 `post_mvp` 不变。

**原子性提示**：`commands.json`、两份协议 schema、`docs/SYNC_PROTOCOL.md` §11.5、`docs/SECURITY_DESIGN.md` §10.2 与 `crates/core/src/broker.rs` 的 `required_grant` 被 `scripts/check-command-catalog.mjs` 断言为**同一个集合**（双向相等）。因此这六处必须在同一工作包、同一次提交里改动；把它们拆到多个可并行交付的工作包，会让每个分支单独跑 `npm run check` 时必然失败（DR1-F1/F3 即此）。

### D6 封闭词表门禁的 `pack` 规则（对应 DR1-F2）

`scripts/check-command-catalog.mjs` 现以 `command.name !== "session.create"` 这一**按命令名**的豁免允许 `session.create` 声明 `pack: null`。D1 给 `session.resume` 也取了 `pack: null`，而它的语义理由与 `session.create` 完全相同：**该命令只经 Node Link 暴露给已配对节点，不属于任何设备 scope pack**（`pack.*` 是设备授权面）。

决策：把判据从「按命令名」改为「**按 transport**」——`transport` 不含 `sync` 的命令允许 `pack: null`，含 `sync` 的命令仍必须有 pack。理由：

- 语义正确：pack 是设备/配对面概念，Node Link-only 的远程工作命令本就无 pack；
- 反向改进：这**去掉**了 `session.create` 的硬编码特例，而不是往名单里再加一个名字；
- 收敛：未来新增 Node Link-only 命令不再需要改门禁脚本。

代价与配套：这是**调整门禁**，按 `AGENTS.md` §10 必须在同一改动里同步四处（`package.json` 的 `check` 脚本本身不变，但需同步本节说明、`README.md` 的「合同检查」②、`.github/workflows/ci.yml` 的注释）。不选「给 `session.resume` 指定或新增 pack」的原因：现有三个 pack 都不贴合恢复语义，新增 `pack.*` 会改变配对授予面与 `presets`（`identity-auth` 的授权词表展开），波及面远大于本次目标。

## Migration Plan

- **升级**：`owned_session` 追加两个可空列，单事务、`IF NOT EXISTS` 幂等；v1/v2/v3/v4 库经既有连续升级段到 v5。升级后既有行两列为 `NULL`，恢复对它们显式失败——**不产生**「升级后旧会话突然可恢复」的假象。
- **回滚**：代码回滚到 v4 时，v5 库会被「过新版本拒绝打开」的既有规则拦住（不静默降级、不丢数据）；如需在生产回滚，必须先把库降到 v4 形状（本次不提供降级工具，回滚需单独授权，交付说明中明确）。
- **兼容性**：Node Link 仍是 v1；新增命令是同一版本内的命令集合扩项，旧 Access 客户端不受影响（它不会发送未知命令）。`session.resume` 的 `delivery` 为 `conditional_mvp`，未宣告能力的 Agent 上该命令稳定返回「不支持」。

## Risks / Trade-offs

- [目标 Agent 是否真的宣告并实现 `sessionCapabilities.resume` 未知] → 门控按真实能力工作，不支持时显式失败、无副作用；`docs/SESSION_CONTINUITY_DESIGN.md` §10 的只读侦察列为独立验证任务（无真实 Agent 时记 BLOCKED 并说明），不阻塞代码交付，但**不得**在验收中把「B 在该 Agent 上可用」写成已验证。
- [恢复失败时已经 spawn 过进程，成本与「未启动进程」的验收措辞可能被误读] → 规格与 proposal 的验收措辞均已改为「未发送 `session/resume` + 已回收本次拉起的子进程」（DR1-F31/CR7-F1；本变更的措辞载体是 `proposal.md` 与 `specs/local-agent-host/spec.md`，不是 `docs/SESSION_CONTINUITY_DESIGN.md`）。
- [`agent_session_id` 的语义可能被后续 `session/load` 复用而取值不同] → 列名与值对象按「ACP 会话标识」通用命名，不绑死 resume；`session/load` 仍为 `post_mvp`，本次不改。
- [迁移把 `FILE_FORMAT_VERSION` 推进会造成旧二进制拒绝打开新库] → 这是既有设计（过新拒绝）；交付说明明确回滚需要单独授权与降级步骤。
- [新增 `UnavailableKind` 取值会触及 `port_error_public` 的显式覆盖与本地管理映射] → 在同一个 WP 内完成 core + 文档 §2 + 映射覆盖，并用既有本地错误码 `local.unavailable`，不新增 `local.*` 码（避免触碰本地管理封闭词表）。
