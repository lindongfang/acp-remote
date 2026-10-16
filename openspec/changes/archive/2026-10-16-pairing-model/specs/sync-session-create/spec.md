# Spec Delta

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: 命令登记与双层授权

**Reason**：设备配对不再授予授权（`pairing-model` 删除设备权限层），"设备 scope `session.create`" 这一半前提消失；授权边界改为「已认证 + 节点侧的 Export grant」。

**Migration**：见本 delta 的新增要求「命令登记与授权边界」；Node Link 侧的 `grant.remote-work` 判定不变。

### Requirement: 载荷形状与拒绝

**Reason**：原要求禁止客户端提供任何目录（只允许 `workspaceAlias`），而本变更允许 Sync 面由客户端指定目录，原文与新增的目录字段直接冲突。

**Migration**：见本 delta 的新增要求「载荷形状与目录来源」与「目录校验与失败分类」；「不得回显提交内容」「不得部分应用参数」两条约束已并入新要求。

### Requirement: 设备级授权语义与副作用

**Reason**：该要求整条描述设备 scope 的授权判定与「新登记资源自动进入已授权范围」这一副作用，设备权限层删除后不再成立。

**Migration**：无需替代——已认证设备对本机已登记/已配置资源可创建会话；对未登记别名的拒绝由「目录校验与失败分类」承担。
