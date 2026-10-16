# sync-session-create Specification

## Purpose

定义 Sync 面 `session.create` 命令的认证前提、载荷约束、失败分类与终态契约，使 PWA 能在本机已登记目录或用户提供的目录里、用已配置的 Agent 创建会话，同时不接受凭据输入。

## Requirements

### Requirement: 命令登记与授权边界

`session.create` SHALL 同时经 Node Link 与 Sync 暴露。Sync 侧以「该连接已通过设备认证」为唯一前提，不再有设备级授权判定；Node Link 侧仍按 Owner 的 Export 与 `grant.remote-work` 判定。未通过认证时 MUST 拒绝且无副作用——不启动进程、不写会话行、不分配会话标识。

#### Scenario: 未认证的连接被拒且无副作用

- **WHEN** 一个尚未完成设备认证的连接提交 `session.create`
- **THEN** 返回认证类失败，本机不产生会话行、不启动任何 Agent 进程

#### Scenario: 已认证设备成功创建

- **WHEN** 一台已认证的设备提交已配置的 Agent 标识与一个合法目录
- **THEN** 请求先被接受，随后以 `completed` 终态返回新会话的标识与会话摘要
### Requirement: 载荷形状与目录来源

`session.create` 的 payload MUST 只允许 `agentId` 与目录引用（`workspaceAlias` 或客户端提供的目录，二者之一）两类字段。出现任何其它键（含 `exportId`、`templateParams`、`mcpServers`、MCP 配置或凭据字段）时 MUST 以 schema 校验错误（`protocol.schema_invalid`）拒绝，MUST NOT 创建会话、MUST NOT 部分应用参数、MUST NOT 回显提交内容。凭据 MUST NOT 经本命令传入。

#### Scenario: 未登记的键被拒且不创建

- **WHEN** payload 携带 `exportId`、`templateParams`、MCP 配置或凭据类字段
- **THEN** 返回 `protocol.schema_invalid`，不创建会话、不启动进程，错误细节不回显该取值

#### Scenario: 目录引用二选一

- **WHEN** payload 同时给出别名与一个客户端提供的目录
- **THEN** 请求被拒绝，不创建会话；系统 MUST NOT 猜测以哪一个为准
### Requirement: 目录校验与失败分类

本机 SHALL 对生效的目录来源做与既有口径一致的校验：别名按登记表解析，客户端提供的目录 MUST 是绝对路径、存在且为目录，并以 `canonicalize` 的结果作为权威值。形状不合法（相对路径、含 `..`、或别名未登记）时 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；形式合法但无法解析（目录被删除、被替换为文件、规范化失败、权限不足）时 MUST 以服务端不可用类错误拒绝。两类失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录。

#### Scenario: 形状不合法是参数类错误

- **WHEN** payload 给出一个相对路径、含 `..` 的路径，或一个本机未登记的别名
- **THEN** 返回参数类错误，且不访问文件系统、不启动进程

#### Scenario: 无法解析是服务端错误

- **WHEN** 形式合法的目录已被删除、已不是目录，或规范化结果与预期不同
- **THEN** 返回服务端不可用类错误（`internal.unavailable`），不创建会话也不启动进程
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

受理 MUST 只持久化该命令的幂等记录，MUST NOT 因此创建会话行；会话行、ACP 会话标识与归属信息的落盘 MUST 与终态在同一次提交内完成；后端失败时 MUST NOT 留下会话行。终结类别 SHALL 按「能否证明 Agent 侧未产生会话」判定：可证明时 `failed`，无法证明时 `uncertain`。

#### Scenario: 重复 requestId 不产生第二个会话

- **WHEN** 同一设备用同一 `requestId` 重发一次已完成的 `session.create`
- **THEN** 返回首次的终态结果，本机不新增会话、不新增 Agent 进程

#### Scenario: 崩溃窗口进 uncertain

- **WHEN** 命令已被接受，但在确认 Agent 启动结果之前进程崩溃
- **THEN** 恢复后该命令的终态为 `uncertain`，且不自动重试创建；本机没有该会话的会话行

#### Scenario: 受理不创建会话行

- **WHEN** 一条 `session.create` 已被接受，但后端尚未返回结果
- **THEN** 本机只有该命令的幂等记录（会话标识为空），没有对应的会话行；同 `requestId` 的重复提交返回受理态而不再派发一次创建

#### Scenario: 可证明未产生会话时以 failed 终结且零会话行

- **WHEN** 后端因进程未能启动，或 Agent 明确对会话建立请求返回错误（含请求未发出即失败）而未能建立会话
- **THEN** 该命令以 `failed` 终结并携带服务端不可用类错误，本机不产生会话行，同 `requestId` 重发返回该终态

#### Scenario: 无法证明时以 uncertain 终结

- **WHEN** 请求已发出但结果无法证明（短请求超时、发出后连接断开，或会话已建立但后续落盘/存储失败）
- **THEN** 该命令以 `uncertain` 终结并标记可能已到达 Agent，MUST NOT 自动重试
