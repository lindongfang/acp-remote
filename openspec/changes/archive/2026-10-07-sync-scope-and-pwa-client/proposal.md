<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

PWA 设计原型 `prototypes/acp-remote-pwa.html` 已被确立为前端第一版的验收基准，但把它逐条对照已冻结的 Sync 合同后，发现 **16 处对不上的地方**，其中 4 类是硬冲突：合同登记了字段却没有生产者、合同禁止的东西原型画了、合同无法表达原型要的语义、状态机自相矛盾。合同层已经全部就位（`sync-workspaces-and-create` 已冻结目录与 `session.create`），前端一行代码都还没有。现在做这次对账，是为了让前端工程建立在与合同一致的形状上，而不是建起来之后边写边改协议。

## What Changes

**一、合同改动**

- **快照只承载「清单」**：`sync.snapshot` 的 session-scoped 明细资源（`messages`/`turns`/`pending_interactions`/`config_options`/`capabilities`）不再进入快照，对**本地会话**同样适用；正文一律经 `session.read` 在线取。这与 `NODE_LINK_PROTOCOL.md` §12.4 既有的「正文绝不入快照」对齐（Sync 当前是唯一例外）。
- **`session.read` 加分页**：payload 增加可选游标与条数，结果增加「是否还有更多」。游标为 `(createdAt, messageId)` 复合键——`snapshotItem.messages` 没有序号字段，且 `SYNC_PROTOCOL.md` §3 明令排序不得依赖 UUID。**只加 Sync 侧**，Node Link 的命令 DTO 与 schema 不变。
- **`file.changed` 增加可选字段**：`addedLines`、`deletedLines`、`outsideWorkspace`。ACP 上游 `$defs/Diff` 已提供类型化的 `path`/`oldText`/`newText`，无需解析 `rawInput`。
- **`agent.connected`/`agent.disconnected` 的 `state` 收成 2 值封闭枚举**（`connected`/`disconnected`），与既有 fixture 一致。

**二、core 生产者**

- **`file.changed` 事件**：由 `agent-host` 转发的 ACP `tool_call`/`tool_call_update` 中 `ToolCallContent::Diff` 派生。行数对 `oldText`/`newText` 跑行级 diff 得出（**不能用行数差**：10 行换成 10 行也可能是删 2 加 2）；`displayPath` 由 `Diff.path`（绝对路径）相对化到会话 workspace 根，越界时置 `outsideWorkspace` 且只给 basename。
- **`agent.connected`/`agent.disconnected` 事件**：节点级（`sessionId: null`），由 `agent-host` 的 profile 进程建立/退出产生。`owned_event` 的 DDL 已支持 `session_id IS NULL`，**不需要 migration**。
- **会话标题接通**：ACP `session_info_update` → `session.info.changed` → 更新 `owned_session.title`。当前 `SessionUpdate` 结构体没有 title 字段、`create` 时恒写 `None`，因此**目录页现在只能显示「未命名会话」**。标题唯一来源是 Agent 通知，**客户端不得修改标题**（无 `session.rename` 命令）。

**三、前端工程**

- 新建 `clients/app/`（Expo/React Native 通用工程，首个交付只构建 Web/PWA），按 `FRONTEND_DESIGN.md` §3 的七层单向依赖组织：`platform/` → `protocol/` → `sync-client/` → `state/` → `features/` → `components/` → 页面。
- 目录页（以目录为主角）、目录详情、对话页、配对页、主机与连接面板、降级卡片、诊断抽屉。
- 「Agent 未上报上下文」态保留，数据源是「本会话尚未收到过 `session.usage.changed`」——不是合同缺口，是事件缺席。

**本次不包含**：`server::sync` 与 `/ui` 静态托管（前端第一版对 fixtures 开发）、`node-link-client` 与 `server::acp_facade`（切片 6）、CI 门禁改动、状态库/表单库选型（`FRONTEND_DESIGN.md` §11 暂缓项）。

## Capabilities

### New Capabilities
- `sync-snapshot-scope`: 快照承载范围与正文回源——快照只含清单资源，`session.read` 按 `(createdAt, messageId)` 复合游标分页取会话明细，客户端滚动加载且不重复不跳条。
- `core-derived-events`: core 派生的结构化事件——`file.changed` 的行数统计与相对化 `displayPath`、`agent.connected`/`agent.disconnected` 的节点级生命周期，以及会话标题经 `session.info.changed` 的单向更新。
- `pwa-web-client`: `clients/app` 的 Web/PWA 客户端——七层单向依赖架构、WebCrypto 不可导出 P-256 设备身份、Sync Client、连接 12 态（8 常规态 + 4 阻断态）与命令 6 态状态机、目录与对话页面、以及 imported 资源的 no-content-cache 离线边界。（Round 10 的 F44：原写「连接 9 态」，取自原型下拉项数，与 `specs/pwa-web-client/spec.md`「连接状态是互斥状态机」枚举的 12 态及 `docs/FRONTEND_DESIGN.md` §5 的状态图不符；本行是 `requirementsDigest` 的输入，与 spec 保持同一口径）

### Modified Capabilities
- `workspace-resolution`: 新增「派生事件展示路径必须相对化到会话 workspace 根」与「越界时显式标记 `outsideWorkspace` 且不泄漏路径结构」。
- `local-agent-host`: 新增「Agent profile 进程的建立与退出产生节点级 `agent.connected`/`agent.disconnected` 事件」与「ACP `tool_call` 中的 `ToolCallContent::Diff` 是 `file.changed` 的唯一来源」。

## Impact

- **机器合同**：`schemas/sync/v1/{common,command,sync,event-views}.schema.json`、`compatibility/` 的封闭词表（若 `limits` 或命令 payload 描述变更）、`fixtures/sync/v1/**`（新增/更新用例与 `manifest.json`）。
- **Rust**：`crates/core`（`ports.rs` 的 `SessionUpdate` 与事件提交面、`broker.rs` 的派生与注入）、`crates/storage-sqlite`（title 列读写）、`crates/sync-protocol`（`views.rs` 的 `FileChanged` 字段与 `VIEW_ENUMS` 登记、`sync.rs` 的 `SessionRead` 结果形状）、`crates/agent-host`（进程生命周期与 Diff 转发的上报）。
- **权威文档**：`docs/SYNC_PROTOCOL.md`（§9.4 资源范围、§10.3 视图最低字段、§11.5 `session.read`）、`docs/NODE_LINK_PROTOCOL.md`（若需说明两侧一致性）、`docs/FRONTEND_DESIGN.md`（§3 落地状态、§4.1、§5、§9 验收标准、§11 暂缓项结论）、`docs/ACP_COMPATIBILITY_MATRIX.md`（`file.changed` 与 agent 事件的 `pwa` 层交付状态）、`docs/SECURITY_DESIGN.md`（若展示路径口径影响信任边界）、`docs/CORE_PORTS_AND_STORAGE.md`（§5 端口签名、§7 表结构说明）。
- **前端**：`clients/app/` 全新工程。
- **存储**：title 列已存在，不新增列、不推进文件格式版本。
- **依赖**：不新增任何 crate 或第三方依赖；行级 diff 在 core 内自实现。
- **门禁**：新增或调整的机器合同必须让 `npm run check` 十道脚本全绿；CI 五个 job 的定义不变。