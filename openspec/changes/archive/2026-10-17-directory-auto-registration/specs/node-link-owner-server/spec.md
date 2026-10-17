# Node Link Owner Server 增量

## MODIFIED Requirements

### Requirement: `session.create` 的 payload 白名单

`session.create` SHALL 要求 `grant.remote-work`；其 `payload` 只允许 `agentId`/`exportId`/`directoryId`/`templateParams` 四个键，出现 `cwd`、`mcpServers`、任何绝对路径或凭据字段时 MUST 以 `command.rejected`（`nodelink.command.unsupported_field`）拒绝，`details.field` 给出被拒字段名，且不得创建会话或部分应用参数。

#### Scenario: 禁带字段被拒绝且不创建会话

- **WHEN** `session.create` 的 `payload` 携带 `cwd` 或 `mcpServers`
- **THEN** Owner 返回 `command.rejected`（`nodelink.command.unsupported_field`，`details.field` 指明字段），Owner 侧不创建任何会话

#### Scenario: 任何路径字段都被拒绝

- **WHEN** `session.create` 的 `payload` 携带任意绝对路径字段（含自造的目录路径）
- **THEN** 按同一错误码拒绝，且不触碰文件系统

## REMOVED Requirements

### Requirement: `session.create` 的参数解析与零参数 template

**Reason**: 目录引用由 `workspaceAlias` 改为 `directoryId`，参数解析的校验对象随之改变；template 的零参数约束不变。

**Migration**: 由新要求「`session.create` 的目录标识解析与零参数 template」承接；`agentId`/`exportId` 的 Export 归属校验与「非空 templateParams 被明确拒绝」保持不变。

## ADDED Requirements

### Requirement: `session.create` 的目录标识解析与零参数 template

`agentId`/`exportId` 必须同时存在于该 Access 可见 Export 与授权范围内，否则返回 `nodelink.export.not_granted`（可带 `details.parameter`）；`directoryId` 必须同时存在于该 Export 的目录集合与本机目录表内，否则同样返回 `nodelink.export.not_granted`。首切片 template MUST 零参数：`templateParams` 必须省略或为空对象，声明了参数的 Export/template 必须明确拒绝而非静默忽略。

#### Scenario: 未知 directoryId 被拒绝

- **WHEN** `session.create` 引用不在该 Export 目录集合内、或不在本机目录表内的 `directoryId`
- **THEN** Owner 返回 `nodelink.export.not_granted`（`details.parameter` 指明被拒参数），不创建会话、不触碰文件系统

#### Scenario: 非空 templateParams 被明确拒绝

- **WHEN** `session.create` 的 `templateParams` 非空或引用的 Export/template 声明了参数
- **THEN** Owner 明确拒绝该请求，不静默忽略 `templateParams`
