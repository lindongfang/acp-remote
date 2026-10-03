<!-- 说明实现方案与决策理由；行为以 specs 为准，协作安排写 plan.md。 -->

## Context

动机与范围见 `proposal.md`。以下是影响方案、且**本次变更前已核实**的现状事实：

- **会话摘要是九字段**：`core::model::session::SessionSummary`（`crates/core/src/model/session.rs:307-341`）只有 `session_id`/`title`/`agent`/`state`/`origin`/`current_mode`/`version`/`created_at`/`updated_at`；wire 侧 `schemas/sync/v1/common.schema.json#/$defs/sessionSummary`（`:223-249`）的 `required` 逐项对应，且 `additionalProperties: false`。
- **会话行只有路径、没有别名**：`owned_session`（`crates/storage-sqlite/src/migrate.rs:54-69`）在文件格式 v5（`FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION` 均为 5，`:19`/`:21`）追加了 `agent_session_id`、`workspace_cwd`；没有 workspace 别名列。`workspace_cwd` 被明确要求「不得进入可投影形状」（`crates/core/src/ports.rs:403-407`）。
- **别名与展示名已经存在**：`owned_workspace(alias PK, display_name, canonical_path, …)`（`migrate.rs:316-322`），由 `local.workspace.select` 写入（`crates/server/src/local_admin/params.rs:215-240`）。
- **创建用例已经解析别名**：`UseCases::create_session`（`crates/core/src/use_cases.rs:243`）在调用后端之前完成别名→规范化路径解析（`workspace-resolution` 能力即此契约），因此创建时别名**已知**，只是未落盘。
- **目录数据已有读取入口**：`UseCases::agents()`（`:401`）、`profiles()`（`:1323`）、`workspaces()`（`:1355`）。
- **快照资源固定六类**：`SnapshotResource::ALL`（`crates/sync-protocol/src/sync.rs:93-100`）与 `schemas/sync/v1/sync.schema.json:262` 的枚举一致；`SnapshotItems::parse` 按 `resource` 分派（`sync.rs:408-440`）。
- **Sync 命令枚举刻意不含 `session.create`**：`schemas/sync/v1/command.schema.json:13-25` 的 `commandName` 枚举只有 11 个，其 `description` 明写「不含只经 Node Link 接受的 session.create」；但同一文件的 `commandResult.body.allOf` 里已有一条把 `session.create` 归入「completed 必须带 terminalEventId」的臂（`:408-425`）——该臂目前在 `commandName` 收窄下不可达，本次使它可达。
- **授权镜像已就位**：`grant.remote-work` 已含 `session.create`/`session.resume`（`crates/identity-auth/src/authorization.rs:71` 与 `crates/core/src/broker.rs:130-131`）；`PACKS`（`authorization.rs:14-35`）没有 `pack.create-session`。
- **命令目录的 pack 判据**：含 `sync` transport 的命令必须有 pack，只有 Node Link 的命令允许 `pack: null`（`scripts/check-command-catalog.mjs`，`AGENTS.md` §10）。当前 `session.create` 是 `transport: ["node_link"]`、`pack: null`。
- **合同漂移门禁**：`scripts/check-contract-drift.mjs` 逐字比对 `docs/CORE_PORTS_AND_STORAGE.md` §7 ↔ `migrate.rs`、§5 ↔ `ports.rs`；`check:commands` 断言 `commands.json` ↔ 两份协议 schema ↔ `SYNC_PROTOCOL.md` §11.5 ↔ `SECURITY_DESIGN.md` §10.2 ↔ `broker::required_grant`。
- **路径禁令**：`docs/SECURITY_DESIGN.md` §12.3（`docs/SECURITY_DESIGN.md:391`）要求路径与规范化结果不得出现在对端可见输出；§4.3 把「用户可编辑的…路径…」列为**不可信输入**（需要转义渲染，而非禁止传输）——这是 B 口径能成立的分界线。

## Goals / Non-Goals

**Goals:**

- 让 Sync 面的目录、Agent 目录与会话归属成为**可协商、可过滤、离线可用**的合同，且不引入任何路径出网。
- 让 `session.create` 在 Sync 面与 Node Link 面共享同一套命令名、幂等与终态语义，差异只在 payload 与授权面。
- 让「设备级授权」的副作用成为文档里的一句显式规则，而不是从 scope 名称推断出来的隐含语义。

**Non-Goals:**

- 不实现 `server::sync`、快照组装、投影与命令处理（后续变更）；本次只冻结它们要消费的合同与端口形状。
- 不扩展 Node Link 的投影（imported 会话的 `workspace` 保持 `null`）。
- 不为目录提供在线查询命令；不改变 `local.workspace.select` 的参数形状。
- 不裁决 `sessions.queue_policy`（见 Open Questions）。

## Decisions

### D1 目录标识用 `{ alias, displayName }`，会话持久化别名

**取舍**：wire 上只传引用；会话行持久化创建时解析使用的别名。展示名在投影时从 `owned_workspace.display_name` 取，取不到时回退为别名本身。

理由：
- 别名是稳定主键，规范化路径是**派生权威值**（`SECURITY_DESIGN.md:391` 明令不得出网）；`display_name` 是 `local.workspace.select` 的用户输入，属 `§4.3` 的「不可信输入」类，可以出网并按不可信内容转义渲染。
- 持久化别名使归属不随「同一别名重指向新目录」漂移；若改用 `workspace_cwd` 反查 `owned_workspace`，重指向后老会话会掉进「未分组」，且投影期引入隐式耦合。
- 回退为别名本身保证「目录被删除」时不丢分组、也不泄漏路径；这与 `workspace-resolution` 的既有要求（不得按别名重解析、不得补全 `NULL`）相容。

**被否决的替代**：

| 方案 | 否决理由 |
| --- | --- |
| 下发 `canonical_path`（原型详情页原样显示真实路径） | 破 `SECURITY_DESIGN §12.3`；路径会进快照、前端缓存（`FRONTEND_DESIGN §7` 限 8 MiB）与诊断抽屉 |
| 下发路径末段（basename） | 仍是派生值，仍需开例外，收益仅是「少发几段」 |
| 只在会话行存 `workspace_cwd`、投影时反查 | 重指向后归属漂移；投影期隐式耦合；与「不得反查」的既有口径冲突 |
| 同时持久化 `alias` 与 `display_name`（两列） | 目录改名不会传播到既有会话；多一列冗余状态 |

### D2 目录与 Agent 目录走快照资源，并作为可选字段门控

两个新资源 `workspaces` / `agents` 加入 `SnapshotResource`（6 → 8），元素复用同一个 `$defs`：`workspaceRef = { alias, displayName }`、`agentCatalogEntry = { agentId, displayName, default }`。

- **门控**：两者受 feature `core.local-catalog.v1` 门控；未协商时服务端 MUST NOT 发送（含空数组）。`SessionSummary.workspace` 是同 feature 下的**可选字段**（`oneOf` 上 `null` 或缺席），MUST NOT 进 `required`——否则旧客户端会因 `additionalProperties: false` 拒绝整条摘要。这一点由 `SYNC_PROTOCOL §16.1`（「通过已协商 feature 增加仅发送给支持方的可选字段」）支持。
- **授权过滤**：资源按设备授权过滤；未授权条目不得出现。目录读取归在既有 `session.list` 授权面（`pack.observe`）之下，不新增 scope——目录页与会话列表是同一屏，拆成两种授权没有产品意义。
- **为什么不是查询命令**：原型在 `offline` 态仍渲染目录（`D:1412` 的「离线 · 2 分钟前同步」）与空目录，而 v1 的初始态载体只有快照；新增 `workspace.list`/`agent.list` 会让离线渲染依赖一次在线往返。`session.create` 的选择器因此也由快照供数。

**Rust 侧形状**：`SnapshotResource` 增加两个变体并同步 `ALL`、`as_str`、`from_str`、`SnapshotItems` 分派（`crates/sync-protocol/src/sync.rs:83-146`、`:390-452`），枚举与 schema 枚举由既有 `check:fixtures` 覆盖门禁双向断言。

### D3 `session.create` 的 Sync 面形状与 core 复用

| 项 | 取值 |
| --- | --- |
| 命令名 | `session.create`（与 Node Link 同名，不另造同义命令） |
| `kind` / `transport` | `mutation` / `["sync", "node_link"]` |
| `pack` / `grant` | `pack.create-session` / `grant.remote-work`（已有） |
| `delivery` | `mvp` |
| payload | `{ workspaceAlias, agentId }`，`additionalProperties: false` |
| 终态结果 | 新会话的 `sessionId` 与会话摘要（复用 `SessionSummary`） |

core 侧**不新增用例**：`UseCases::create_session`（`use_cases.rs:243`）已按「解析别名 → 调用后端 → 提交状态」实现，Node Link 侧就是在调用它。Sync 侧只需在其上补一层设备授权与 `{workspaceAlias, agentId}` 的解包。

**被否决的替代**：复用 Node Link 的 payload（含 `exportId`/`templateParams`）——那两个字段是跨节点 Export 概念，设备侧没有 Export，强加会造成「必须伪造 exportId」的荒谬输入。

### D4 错误映射：不新增任何错误码

| 情况 | wire 错误码 | 理由 |
| --- | --- | --- |
| 设备缺 `session.create` scope | `authorization.scope_denied` | 既有码；与 `commands.json` 的授权面一致 |
| payload 含未知字段（`cwd` 等） | `protocol.schema_invalid` | 服务端先按 `command` 分派再校验 payload 形状（`SYNC_PROTOCOL:150`） |
| 别名未登记 / `agentId` 未配置 | `authorization.scope_denied` | 设备授权目录不含该资源；与 Node Link 的 `nodelink.export.not_granted` 同义映射 |
| 已登记但本机解析失败 | `internal.unavailable` | 服务端不可用类，对应 `workspace-resolution` 的分类要求 |
| 已接受但副作用无法确认 | `command.uncertain` | 既有码，与 `session.prompt` 同口径 |

不新增错误码意味着 `compatibility/errors/v1/errors.json` 与两份协议 schema 的错误枚举不变，`check:errors` 无需调整。

### D5 设备级授权的副作用写进安全文档

`SECURITY_DESIGN §9`（`docs/SECURITY_DESIGN.md:290`）现写「Node Link 首个纵向切片必须实现它以支持 Zed `session/new`；Sync 首版不暴露该入口」——本次改为：Sync 面经设备 scope `session.create` 暴露，并**显式写出副作用**：「授予后覆盖该节点当时及此后新增的全部已登记 workspace 与已配置 Agent；新登记的资源自动进入已授权范围。」同时更新 `§10.2` 的命令/scope/pack/grant 表。

**被否决的替代**：把设备级授权改成资源级白名单（配对时勾选 alias/agentId 子集）。需要给 `DeviceRecord`、`owned_device`、配对 wire 增加资源字段并改配对 UI，且原型（`prototypes/acp-remote-pwa.html:723` 的五条 scope 列表）没有第二层选择；用户已明确选择设备级。

### D6 v6 迁移与合同漂移门禁

```
ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT;   -- 可空、无默认值、只追加
```

- 版本常量：`FILE_FORMAT_VERSION`、`OWNED_SCHEMA_VERSION` 5 → 6（`migrate.rs:19`/`:21`），新增 `V6_UPGRADE_OWNED` 段并挂到连续升级链；imported 家族保持 3。
- 同步 `docs/CORE_PORTS_AND_STORAGE.md` §7（`owned_session` DDL 与 §7.2 版本常量）——`check-contract-drift.mjs` 逐字比对，漏改即红。
- `Session`/`SessionSummary` 构造、`SessionUpdate` 写入字段（`workspace_alias`，与既有 `workspace_cwd` 同形的窄写入）、`SessionStore::list/load` 的读取路径同步。
- 读取时用 `owned_session.workspace_alias` LEFT JOIN `owned_workspace` 取 `display_name`；连接不上时回退别名。该 JOIN 留在 `storage-sqlite` 内部（它同时拥有两张表），不上浮到 core。

**回滚**：代码回滚到 v5 时，v6 库会被既有「过新版本拒绝打开」规则拦住（不静默降级、不丢数据）；生产回滚需先降级库形状，本次不提供降级工具。

### D7 契约原子点（必须在同一工作包/同一提交内改动）

`session.create` 的 `transport` 变化牵动同一集合的六处：`compatibility/commands/v1/commands.json`、`schemas/sync/v1/command.schema.json`、`schemas/node-link/v1/command.schema.json`、`docs/SYNC_PROTOCOL.md` §11.5、`docs/SECURITY_DESIGN.md` §10.2、`crates/core/src/broker.rs` 的 `required_grant`（已含映射，只需确认未漂移）。新增 pack 另需 `crates/identity-auth/src/authorization.rs` 的 `PACKS` 与既有验收测试。把它们拆到多个可并行工作包会让每个分支单独跑 `npm run check` 必失败。

**Feature 登记**：`core.local-catalog.v1`、`core.session-create.v1` 需同时进 `compatibility/features/v1/features.json`、`SYNC_PROTOCOL.md` §5.2 表与 fixture（`check:features` 三方一致）。

### D8 文档与机器资产同步清单

| 资产 | 改动 |
| --- | --- |
| `compatibility/commands/v1/commands.json` | `session.create` 的 `transport` 加 `sync`、`pack` 设 `pack.create-session`；新增该 pack（成员 `session.create`） |
| `compatibility/features/v1/features.json` | 新增两条 sync feature，`delivery: mvp`、`required: false` |
| `schemas/sync/v1/command.schema.json` | `commandName` 加 `session.create`；新增 payload/result def；修正 `description`；使既有 mutation-terminal 臂可达 |
| `schemas/sync/v1/common.schema.json` | 新增 `workspaceRef`；`sessionSummary` 加可选 `workspace` |
| `schemas/sync/v1/sync.schema.json` | `snapshotResource` 加两值；`snapshotChunk.allOf` 加两条 item 分支；新增 `snapshotItem.workspaces`/`snapshotItem.agents` |
| `schemas/node-link/v1/command.schema.json` | 仅当 `session.create` 的 pack 字段出现在该 schema 的约束中时才需同步（核查后决定） |
| `fixtures/sync/v1/**` | 新命令与两个新资源的正/负向量；`manifest.json` 覆盖门禁 |
| `docs/SYNC_PROTOCOL.md` | §5.2、§9.4（资源与 item 最低字段）、§10.3（`workspace` 与两个元素形状）、§11.3/§11.5（命令集合与计数）、§16.2 |
| `docs/SECURITY_DESIGN.md` | §9（设备 create 规则 + 副作用）、§10.2（命令/scope/pack/grant 表）、§11.2（展示名按不可信内容渲染） |
| `docs/FRONTEND_DESIGN.md` | §4.1（必须实现：目录页与创建）、§9 第 1 条（验收标准改口径）、§7（缓存含目录引用）、§2.2（产品边界） |
| `docs/CORE_PORTS_AND_STORAGE.md` | §3.1/§3.6（投影形状与窄写入字段）、§5（`SessionStore` 读取形状）、§7 标题/§7.2/§7.3（DDL 与版本） |
| `docs/DEVELOPMENT_PLAN.md`、`README.md` | 切片 7 的范围与仓库当前状态 |
| `crates/identity-auth/src/authorization.rs` + `tests/authorization.rs` | `PACKS` 新增 `pack.create-session` 与成员断言 |

### D9 与后续变更的接口边界

本次**只冻结**接口，不实现：`server::sync` 将消费 `SnapshotResource`/`SnapshotItems` 新变体、`SessionSummary.workspace` 与 `create_session`；`/ui` 托管与前端是另外两个变更。因此本次交付的验收证据落在合同门禁、cargo 测试（`sync-protocol`/`core`/`storage-sqlite`/`identity-auth`）与固定向量上，不需要（也拿不到）端到端运行证据。

## Risks / Trade-offs

- [设备级授权范围随登记集合增长] → 已在 D5 落成文档里的显式句子 + spec 场景「新登记的目录自动进入已授权范围」；撤销 scope/设备立即失效有独立场景。
- [`SessionSummary.workspace` 是 wire 上的新字段，旧客户端可能因 `additionalProperties: false` 拒绝] → 定为 feature 门控下的可选字段，未协商时字段缺席；spec 有专门场景断言「不是 `null` 而是缺席」。
- [会话行别名与 `owned_workspace` 脱节（目录被删除/改名）] → 投影期 JOIN + 回退别名；改名传播、删除不丢分组；有 spec 场景。代价是投影多一次 JOIN（同库同事务，无跨模块耦合）。
- [v6 迁移使旧二进制拒绝打开新库] → 既有设计（过新拒绝）；回滚需单独授权并先降级库形状，交付说明中写明。
- [`commandResult` 里那条引用 `session.create` 的既有 `allOf` 臂在本次之前不可达] → 本次使其可达，需在测试中确认它不会误伤其它 mutation（该臂的 `if` 用 `const` 匹配 `command`，语义独立）。
- [imported 会话的 `workspace` 恒为 `null`，与原型「只有本机会话」一致但与 `resource.remote-origin.v1` 的能力不对称] → 记为已知限制并写进 `SYNC_PROTOCOL` §9.6 注记；扩展 Node Link 投影属后续变更。

## Migration Plan

- **升级顺序**：`migrate` 在单实例锁之后、开始监听之前执行；v1…v5 库经既有连续升级段到 v6，单事务、失败整体回滚、可重复打开；既有会话行 `workspace_alias` 为 `NULL`（解释为「未分组」）。
- **兼容**：Sync 仍是 v1；新增资源与字段全部在 feature 门控之下，未协商的旧客户端行为不变。Node Link 侧命令集合不变（`session.create` 仍在其中，payload 形状不变）。
- **回滚条件**：需要回滚到 v5 二进制时必须先把库降回 v5 形状；本次不提供降级工具，回滚需单独授权。

## Open Questions

- **`sessions.queue_policy` 的口径**：原型在会话进行中拒绝发送（`prototypes/acp-remote-pwa.html:1868` 的 toast「会话进行中，完成后再发送」），却又渲染 `queued` 状态圆点；而 `CONFIG_REFERENCE` 的默认值是 `sessions.queue_policy = queue`（且 `crates/app/src/compose.rs:244-246` 目前把它当未接线段落取 v1 默认）。本变更不涉及该行为，`server::sync` 落地时必须先定口径。
