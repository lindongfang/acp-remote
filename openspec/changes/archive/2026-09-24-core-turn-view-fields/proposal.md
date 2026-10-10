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

## Capabilities

### New Capabilities
- `core-event-view-identity`: core 对事件 view 的 turn 归属与会话版本字段的注入、冲突 fail-closed 语义、版本规则的漂移检测，以及注入不改变 ACP 保真与未知字段的约束。

### Modified Capabilities
（无：本次不改变任何既有能力的行为要求。`local-agent-host` 的适配器行为不变——它继续产出 ACP 派生字段，turn/version 由 core 补齐）

## Impact

- **代码**：`crates/core`（`core::broker` 的提交/广播路径、`core::model::json` 的文本工具、`core::ports` 的注释与 `TurnAccepted` 语义说明）；`crates/agent-host` 不改行为（可能仅新增/调整断言 core 补齐行为的测试）。
- **文档**：`docs/CORE_PORTS_AND_STORAGE.md` §5.1（`prompt` 返回值语义）、§6（提交顺序/版本规则/注入时机）、§9（判据补充）；`docs/MODULE_ARCHITECTURE.md` §4.1/§4.7（broker 职责与 view 组装的归属）。
- **合同资产**：不新增/不修改 `schemas/**`、`fixtures/**`、`compatibility/**`（`SYNC_PROTOCOL.md` §10.3 已是权威来源，本变更不改它）；依赖集不变（不新增依赖）。
- **系统行为**：持久化与广播的事件 view 更完整；Sync 切片可以按 §10.3 直接校验与重放，无需再动 core。
