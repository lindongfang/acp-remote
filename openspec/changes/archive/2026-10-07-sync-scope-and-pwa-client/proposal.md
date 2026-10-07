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

## Intent and Constraints

```agentic-intent
sources:
  - "用户 2026-10-03 本会话原话：「我想落地前端工程」——确立本次目标"
  - "用户 2026-10-03 原话：「A1 那就保留吧」——保留「Agent 未上报上下文」态"
  - "用户 2026-10-03 原话：「A2 方案甲」——file.changed 的行数由 Broker 计算"
  - "用户 2026-10-03 原话：「A3 这是acp-remote识别的agent，owner侧可能有多个agent」——Agent 状态是节点级 profile 维度，不是会话维度"
  - "用户 2026-10-03 原话：「A4 方案乙」"
  - "用户 2026-10-03 原话：「会话信息是存储在owner侧吗？那为什么不能全量加载。只是默认加载一定量数据，然后动态加载，就可以」——确立「数据全在 Owner + 按需取」"
  - "用户 2026-10-03 原话：「清单吧」——快照只发清单，正文一律现取"
  - "用户 2026-10-03 原话：「3. 按用户输入条数来计算吧，默认展示最近20条用户输入，支持滚动加载」"
  - "用户 2026-10-03 原话：「3. 不允许修改标题」——会话标题只能由 Agent 产生，客户端不得修改"
  - "用户 2026-10-03 原话：「B 不需要」——不改 CI 门禁与 npm workspaces"
  - "用户 2026-10-03 原话：「E 甲」——保留 state 字段并收成 2 值枚举"
  - "用户 2026-10-03 原话：「丙」——本次覆盖合同 + core 生产者 + 前端工程"
  - "对账依据文件：prototypes/acp-remote-pwa.html（2012 行）、prototypes/IMPLEMENTATION-GAPS.md、prototypes/acp-remote-pwa-mobile.html"
  - "既有权威约束：docs/SYNC_PROTOCOL.md §9.4/§9.6/§10.3/§11.5/§14、docs/FRONTEND_DESIGN.md §2.2/§3/§4.1/§4.3/§5/§7/§9/§11、docs/SECURITY_DESIGN.md §12.3、docs/NODE_LINK_PROTOCOL.md §12.4、docs/CORE_PORTS_AND_STORAGE.md §3.6/§5/§7"
constraints:
  - "规范化路径 MUST NOT 出现在快照、摘要、事件 view 或任何对端可见输出中；Diff.path 是绝对路径，Broker 必须相对化，越界时显式标记而非静默降级"
  - "新增能力不得削弱既有不变量：先持久化后广播、每会话单 active turn、稳定 requestId 幂等、崩溃窗口进 uncertain"
  - "协议合同变更必须同步 schemas/、fixtures/、compatibility/ 与权威文档，并让 npm run check 全绿"
  - "core 端口签名变化会被 check:drift 逐字比对 docs/CORE_PORTS_AND_STORAGE.md §5 与 crates/core/src/ports.rs，两处必须同步改"
  - "视图封闭枚举变更必须同时登记 crates/sync-protocol/src/views.rs 的 VIEW_ENUMS 表，否则 schema_drift.rs 双向门禁失败"
  - "PWA 私钥不得存入 localStorage；页面不得直接操作 WebSocket/IndexedDB/CryptoKey"
  - "前端首个交付只构建 Web/PWA，不实现 Android/iOS 原生包"
  - "日志与错误页面不得出现敏感信息；ACP 原文按不可信文本保存"
  - "前端工程的依赖审计不在本变更范围（用户明确选择不改 CI 门禁）"
non_goals:
  - "不实现 server::sync 入站适配层与 /ui 静态托管——前端第一版对 fixtures/ 开发，这是独立变更"
  - "不实现 node-link-client 与 server::acp_facade（切片 6，与 PWA 无交集）"
  - "不改 CI 门禁、npm workspaces 与 package.json 的 check 脚本"
  - "不裁决前端状态库与表单库选型（FRONTEND_DESIGN §11 暂缓项）"
  - "不统一 AgentCatalogEntry.displayName 与 SessionSummary.agent.name 的命名差异"
  - "不为同名 workspace 目录设计除「展示名 + 别名」之外的新消歧信息"
success_criteria:
  - "目录页能显示 Agent 生成的会话标题；未生成时显示「未命名会话」且不泄漏其它状态"
  - "对话页在 Agent 运行 tool call 编辑文件后，能显示 +N −M 与相对化路径；文件在工作区之外时显示显式标记"
  - "主机面板能列出本节点所有已配置 Agent 及其当前连接状态，且该列表在断网时仍可渲染"
  - "点开任意会话能加载其对话正文，向上滚动能按游标加载更早的 20 条用户输入，不重复、不跳条"
  - "快照体积只与会话条数相关，与会话消息总量无关；10000 个会话的快照可被单次 digest 校验完成"
  - "npm run check 全绿，含 check:drift（端口签名）与 schema_drift（视图枚举）两道双向门禁"
  - "前端类型与协议 DTO 来自同一份 schemas/ 与 fixtures/，不复制测试样例"
decision_bounds:
  - "可自主决定：字段命名、目录内部组织、组件拆分、状态机实现形式、spec 场景与用例命名、游标的具体编码"
  - "需用户决策：改变安全模型或信任边界、扩大范围到 server::sync/前端门禁、裁决 FRONTEND_DESIGN §11 的暂缓项"
assumptions:
  - "阻塞性：会话标题接通后 SessionSummary 的真实字节数未知（title 上限 512 字符），因此本变更不对 sessions 快照设条数上限；快照仅含摘要，体积只随会话条数线性增长，量级可控"
  - "非阻塞：Myers diff 的实现复杂度在文件行数量级下可接受；若实测出现病态输入（超大单文件），改为有界算法并在 design 记录"
  - "非阻塞：客户端在无 server::sync 的情况下对 fixtures 开发，协议层严格 typed、不做宽容解析，可把语义偏差风险降到最低"
```

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