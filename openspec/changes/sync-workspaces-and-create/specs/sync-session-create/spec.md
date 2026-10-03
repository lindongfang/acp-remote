<!-- 新增能力：Sync 面 `session.create` 的设备级授权、载荷约束、失败分类与终态契约。 -->

## Purpose

定义 Sync 面 `session.create` 命令的设备级授权语义、载荷约束、失败分类与终态契约，使 PWA 能在本机已登记 workspace 与已配置 Agent 的范围内创建会话，同时不接受任何绝对路径、目录追加或凭据输入。

## ADDED Requirements

### Requirement: 命令登记与双层授权

`session.create` SHALL 同时经 Node Link 与 Sync 暴露，归设备授权包 `pack.create-session` 与 `grant.remote-work`。Sync 侧发起时 MUST 同时满足设备 scope `session.create` 与 Owner 侧 `grant.remote-work`；未满足时 MUST 以授权拒绝错误（`authorization.scope_denied`）终止且无副作用——不启动进程、不写会话行、不分配会话标识。

#### Scenario: 未持 scope 的设备被拒且无副作用

- **WHEN** 一台持 observe 与 interact 但不持 `session.create` 的设备提交 `session.create`
- **THEN** 返回 `authorization.scope_denied`，本机不产生会话行、不启动任何 Agent 进程

#### Scenario: 持 scope 的设备成功创建

- **WHEN** 一台持 `session.create` 的设备提交本机已登记的 workspace 别名与已配置的 Agent 标识
- **THEN** 请求先被接受，随后以 `completed` 终态返回新会话的标识与会话摘要

### Requirement: 载荷形状与拒绝

`session.create` 的 payload MUST 只允许 `workspaceAlias` 与 `agentId` 两个键。出现任何其它键（含 `cwd`、`exportId`、`templateParams`、绝对路径、MCP 配置或凭据字段）时 MUST 以 schema 校验错误（`protocol.schema_invalid`）拒绝，MUST NOT 创建会话、MUST NOT 部分应用参数、MUST NOT 回显提交内容。系统 MUST NOT 接受客户端提供的任何绝对路径、目录追加或凭据。

#### Scenario: 携带 cwd 被拒且不创建

- **WHEN** payload 携带 `cwd` 或任何绝对路径取值
- **THEN** 返回 `protocol.schema_invalid`，不创建会话、不启动进程，错误细节不回显该取值

#### Scenario: 只接受已登记引用

- **WHEN** payload 的 `workspaceAlias` 与 `agentId` 都是本机已登记/已配置的取值
- **THEN** 请求通过形状校验并进入授权与解析流程

### Requirement: 设备级授权语义与副作用

`session.create` 对设备的授权判定 SHALL 以「该资源当前是否已登记（workspace）或已配置（Agent）」为准，而不是以配对时的快照为准。一次授予 MUST 因此覆盖该节点当时及此后新增的全部已登记 workspace 与已配置 Agent；该副作用 MUST 在权威安全文档中显式记录，MUST NOT 仅隐含在 scope 名称之下。撤销该 scope 或撤销设备 MUST 立即使后续创建被拒。

#### Scenario: 新登记的目录自动进入已授权范围

- **WHEN** 一台设备配对时被授予 `session.create`，此后本机新登记了一个 workspace
- **THEN** 该设备无需重新授权即可在该 workspace 中创建会话

#### Scenario: 撤销后立即失效

- **WHEN** 该设备的 `session.create` scope 被撤销，或设备本身被撤销
- **THEN** 其后续 `session.create` 一律被拒且无副作用

### Requirement: 引用失败与解析失败的分类

引用的别名或 Agent 不在本机登记/配置集合内时，系统 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；别名已登记但本机解析失败（目录被删除、被替换为文件、规范化失败或权限不足）时 MUST 以服务端不可用类错误拒绝。两类失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录或替代 Agent。

#### Scenario: 未登记别名是参数类错误

- **WHEN** payload 引用一个本机从未登记的 workspace 别名
- **THEN** 返回授权/参数类错误（`authorization.scope_denied`），且不访问文件系统

#### Scenario: 已登记但解析失败是服务端错误

- **WHEN** 别名已登记，但其目录已被删除或已不是目录
- **THEN** 返回服务端不可用类错误（`internal.unavailable`），不创建会话也不启动进程

### Requirement: 终态与幂等

`session.create` 是有副作用的 mutation：系统 SHALL 先接受（`accepted`，`result` 与 `terminalEventId` 均为 `null`）再执行，最终以 `completed`（携带新会话标识与会话摘要）、`failed` 或 `uncertain` 之一终结。同一稳定 `requestId` 重复提交 MUST 返回首次结果，MUST NOT 产生第二个会话或第二次 Agent 启动；已接受但副作用无法确认的崩溃窗口 MUST 落 `uncertain` 终态，MUST NOT 自动重试。

#### Scenario: 重复 requestId 不产生第二个会话

- **WHEN** 同一设备用同一 `requestId` 重发一次已完成的 `session.create`
- **THEN** 返回首次的终态结果，本机不新增会话、不新增 Agent 进程

#### Scenario: 崩溃窗口进 uncertain

- **WHEN** 命令已被接受，但在确认 Agent 启动结果之前进程崩溃
- **THEN** 恢复后该命令的终态为 `uncertain`，且不自动重试创建
