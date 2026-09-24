## Why

上一变更（`acp-boundary-and-agent-host`，已归档）在验收时登记了 `Check Plan Change 3`：`SessionEndpoint::prompt(request, at)` 的签名不携带 core 的 `TurnId`，会话 `version` 也由 core 掌握，因此适配器产出的 view 缺 `docs/SYNC_PROTOCOL.md` §10.3 规定的最低字段（`turn.*`/`agent.message.*`/`tool.call.*`/`permission.requested` 等需要 `turnId`；`session.mode.changed`/`session.config.changed` 需要 `version`）。

本地路径今天不坏：core 的 broker 广播时会自己补 `event.turn`，且只从 view 读 `interactionId`/`messageId`/`deltaIndex`/`text`/`block`。但**持久化的 view 本身不合规**，而 Sync 切片正是按 view 逐字段校验并重放的——它一旦开始实现就会立刻撞上这个缺口。用户已裁定收口方式为 **core 侧注入**（不改 `SessionEndpoint` 端口签名），本变更把该裁定落地，使 view 在持久化与广播两条路径上都满足 §10.3。

## What Changes

- **core 注入 `turnId`**：broker 在事件提交**之前**，把它已解析出的 turn 归属以顶层字段 `turnId` 写入 view 文本；owned（`SessionStore::commit` 前）与 imported（`RemoteDeliveryStore` 收据后广播前）两条路径一致。没有 turn 归属的事件（`session.*`、`agent.connected` 等）**不伪造**该字段。
- **core 注入 `version`**：对需要 `version` 的两类 view（`session.mode.changed`、`session.config.changed`）注入**提交后**的会话版本（十进制字符串）。
- **版本规则显式化并 fail-closed**：core 在提交前按「带状态变更 `version + 1`、纯事件提交版本不变」推导预期版本，提交后与 `CommitOutcome.version` 比对；不一致时显式失败（不发布、不静默降级）。这样存储层规则与 core 假设的漂移会变成可观察错误，而不是静默的字段不一致。
- **view 字段冲突 fail-closed**：view 已带顶层 `turnId`/`version` 时，取值必须与 core 权威值一致；不一致即拒绝该次提交，不覆盖、不保留两份。
- **JSON 文本工具**：`core::model::json` 新增顶层字段插入能力（纯文本，保持未知字段原样），因为 core 的直接依赖被 allow-list 锁定，**不得引入 `serde_json`**。
- **`TurnAccepted.turn` 语义收口**（`RV-DU1-F6`）：明确该字段是适配器侧占位/审计值，core **一律不采信**，turn 归属始终用 core 自己分配的 `TurnId`；并把该行为固定为回归断言。
- **文档同步**：`docs/CORE_PORTS_AND_STORAGE.md` §6（提交顺序与版本规则）、§9（判据）、§5.1（`prompt` 返回值的语义）与 `docs/MODULE_ARCHITECTURE.md` §4.1/§4.7 的职责描述。
- **不包含**：ACP raw 保真语义、Sync/Node Link 切片、`SessionEndpoint` 端口签名、storage DDL/migration、前端。

## Intent and Constraints

```agentic-intent
sources:
  - "用户 2026-09-24 会话裁定（上一变更 `acp-boundary-and-agent-host` 的验收提问第 1 项）：「同意采用 core 侧收口——由 core 在生成/提交事件时把 turnId（以及会话 version）注入 view，而不是改 SessionEndpoint 端口签名」，原文「1. 同意」"
  - "用户 2026-09-24 会话（本变更的启动请求）：「先做core 侧 turnId/version 收口」"
  - "用户 2026-09-24 会话（本变更的范围确认）：「1. 同意本变更 Main E2E 记 not-applicable」「2. 同意」——第 2 项同意把 RV-DU1-F6（`TurnAccepted.turn` 占位 id）纳入本变更"
  - "openspec/changes/archive/2026-09-24-acp-boundary-and-agent-host/verification.md 的 `Check Plan Change 3` 与「仍未处理」清单（含 RV-DU1-F6）"
  - "docs/SYNC_PROTOCOL.md §10.3 的事件 view 最低字段表"
constraints:
  - "core 的直接依赖固定为 async-trait/thiserror/p256/sha2（AGENTS.md §12 与 docs/CORE_PORTS_AND_STORAGE.md §9 判据 13）：不得引入 serde_json 或新的 JSON 解析依赖"
  - "core 不得出现 serde_json::Value（docs/CORE_PORTS_AND_STORAGE.md §2）；view 是「已验证的 JSON 文本」，须原样保真、不得重排键或丢弃未知字段"
  - "不得改 ACP raw 三要素：acp.rawJson 的字节、acp_sha256、acp_byte_length 在注入前后必须完全不变"
  - "不得改 `SessionEndpoint::prompt` 端口签名（用户已裁定）；端口签名与 §5 矩阵不得漂移"
  - "不改 storage-sqlite 的 DDL/migration，不新增表或列；`docs/CORE_PORTS_AND_STORAGE.md` §7 与 migrate.rs 的 drift 门禁必须继续一致"
  - "字段形状必须符合 docs/SYNC_PROTOCOL.md §10.3：`turnId` 为字符串，`version` 为十进制字符串（不得用 JSON number，避免跨语言精度问题）"
  - "Main E2E 记 not-applicable，替代验证四项必须逐项留证；cargo-deny 与 gitleaks 仍只在 CI 判定，本地不得声称通过"
non_goals:
  - "不改 Sync/Node Link 的 wire schema、fixtures 或协议文档（§10.3 已是权威要求，本变更只让 core 满足它）"
  - "不实现 Sync/Node Link server 切片，不接线 Daemon/CLI/前端"
  - "不改 agent-host 的适配器行为（它继续产出 ACP 派生字段；turn/version 由 core 补齐）；若需要测试调整，仅限断言适配器未产出这两个字段时 core 仍补齐"
  - "不给 core 引入 serde_json、不在 core 里解析 ACP 原文"
  - "不顺手收口上一变更登记的其他「接线 app 前」项（ensure_runtime 跨 await 持锁、回收/关闭交错、UnknownProfile 用例等），它们仍在原登记处"
success_criteria:
  - "任一归属到 turn 的事件，其**持久化** view 与**广播** view 都含顶层 `turnId`，取值等于 core 为该事件解析出的权威 turn 标识"
  - "`session.mode.changed` 与 `session.config.changed` 的 view 含顶层 `version`（十进制字符串），且等于该次提交后存储层返回的会话版本"
  - "view 里原有的未知字段、字段顺序与 ACP raw 三要素在注入前后逐字节不变（有回归断言）"
  - "view 已带不一致的 `turnId`/`version` 时提交显式失败，且不写入任何行、不发布任何帧"
  - "适配器 `prompt` 返回任意占位 `turn` 时，core 的 turn 记录、事件归属与 view 的 `turnId` 仍使用 core 分配的权威值"
decision_bounds:
  - "可自主决定：注入的代码落点（broker 的哪一阶段）、JSON 文本插入的具体算法与校验强度、测试用例划分、内部错误类型选择、文档措辞"
  - "需要用户决策：任何 `SessionEndpoint`/`SessionStore`/`RemoteDeliveryStore` 端口签名变化、DDL 或 §7 表结构变化、core 依赖集变化、Sync/Node Link 语义变化、把版本权威从存储层移到 core"
assumptions:
  - "存储层的版本规则是「带 state 的提交 `version = version + 1`，纯事件提交不变」——已由 crates/storage-sqlite/src/session_store.rs 的 `UPDATE owned_session SET version = version + 1 … RETURNING version` 与无状态分支读回原值确认；本变更把它写成 core 侧的 fail-closed 断言，若实现期发现不成立则以显式失败暴露并回来改设计"
  - "适配器可能已经自行产出 `turnId`，也可能没有；两种输入都必须收敛到同一结果（一致则保留，缺失则补，冲突则失败）"
  - "imported 路径不重放正文（no-content-cache），但它同样广播 view，因此同样需要注入；其 `turnId` 归属来自 Owner 的事件（已随事件到达）"
```

## Capabilities

### New Capabilities
- `core-event-view-identity`: core 对事件 view 的 turn 归属与会话版本字段的注入、冲突 fail-closed 语义、版本规则的漂移检测，以及注入不改变 ACP 保真与未知字段的约束。

### Modified Capabilities
（无：本次不改变任何既有能力的行为要求。`local-agent-host` 的适配器行为不变——它继续产出 ACP 派生字段，turn/version 由 core 补齐。）

## Impact

- **代码**：`crates/core`（`core::broker` 的提交/广播路径、`core::model::json` 的文本工具、`core::ports` 的注释与 `TurnAccepted` 语义说明）；`crates/agent-host` 不改行为（可能仅新增/调整断言 core 补齐行为的测试）。
- **文档**：`docs/CORE_PORTS_AND_STORAGE.md` §5.1（`prompt` 返回值语义）、§6（提交顺序/版本规则/注入时机）、§9（判据补充）；`docs/MODULE_ARCHITECTURE.md` §4.1/§4.7（broker 职责与 view 组装的归属）。
- **合同资产**：不新增/不修改 `schemas/**`、`fixtures/**`、`compatibility/**`（`SYNC_PROTOCOL.md` §10.3 已是权威来源，本变更不改它）；依赖集不变（不新增依赖）。
- **系统行为**：持久化与广播的事件 view 更完整；Sync 切片可以按 §10.3 直接校验与重放，无需再动 core。
