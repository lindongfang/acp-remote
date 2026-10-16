# Spec Delta

## MODIFIED Requirements

### Requirement: 载荷形状与目录来源

`session.create` 的 payload MUST 只允许 `agentId`、目录引用（`workspaceAlias` 或客户端提供的目录，二者之一）与一个可选的「目录不存在则先创建」开关三类字段。出现任何其它键（含 `exportId`、`templateParams`、`mcpServers`、MCP 配置或凭据字段）时 MUST 以 schema 校验错误（`protocol.schema_invalid`）拒绝，MUST NOT 创建会话、MUST NOT 部分应用参数、MUST NOT 回显提交内容。该开关 MUST NOT 与已登记别名同时出现——别名解析出的目录必然已存在，两者同时给出时 MUST 以 schema 校验错误拒绝。凭据 MUST NOT 经本命令传入。

#### Scenario: 未登记的键被拒且不创建

- **WHEN** payload 携带 `exportId`、`templateParams`、MCP 配置或凭据类字段
- **THEN** 返回 `protocol.schema_invalid`，不创建会话、不启动进程，错误细节不回显该取值

#### Scenario: 目录引用二选一

- **WHEN** payload 同时给出别名与一个客户端提供的目录
- **THEN** 请求被拒绝，不创建会话；系统 MUST NOT 猜测以哪一个为准

#### Scenario: 建目录开关不与别名同用

- **WHEN** payload 同时给出 `workspaceAlias` 与「不存在则先创建」开关
- **THEN** 返回 `protocol.schema_invalid`，不解析别名、不触碰文件系统

### Requirement: 目录校验与失败分类

本机 SHALL 对生效的目录来源做与既有口径一致的校验：别名按登记表解析，客户端提供的目录 MUST 是绝对路径、并以 `canonicalize` 的结果作为权威值。默认（未给出「不存在则先创建」开关，或该开关为假）时，客户端提供的目录 MUST 已存在且为目录。形状不合法（相对路径、含 `..`、或别名未登记）时 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；形式合法但无法解析（目录被删除、被替换为文件、规范化失败、权限不足）时 MUST 以服务端不可用类错误拒绝。开关为真时目录可以不存在：系统 SHALL 先创建它（连同缺失的父层）再启动 Agent；路径上已存在但不是目录的成分、或创建失败（父层不可达、无写权限）MUST 以服务端不可用类错误拒绝。所有失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录。

#### Scenario: 形状不合法是参数类错误

- **WHEN** payload 给出一个相对路径、含 `..` 的路径，或一个本机未登记的别名
- **THEN** 返回参数类错误，且不访问文件系统、不启动进程

#### Scenario: 无法解析是服务端错误

- **WHEN** 形式合法的目录已被删除、已不是目录，或规范化结果与预期不同，且未给出建目录开关
- **THEN** 返回服务端不可用类错误（`internal.unavailable`），不创建会话也不启动进程

#### Scenario: 开关为真且目录不存在时先创建

- **WHEN** payload 给出一个绝对、形状合法的目录并置建目录开关为真，而该目录不存在
- **THEN** 系统先在主机上创建该目录（含缺失的父层），再启动 Agent；创建失败时返回服务端不可用类错误，不启动 Agent、不留会话行

#### Scenario: 开关为真但路径上已存在非目录

- **WHEN** 开关为真，而目标路径上已存在一个文件，或其某个父层成分是文件
- **THEN** 返回服务端不可用类错误，不覆盖、不删除已有内容，也不启动 Agent

#### Scenario: 创建出的目录不成为已登记别名

- **WHEN** 系统按开关创建了一个目录并据此成功创建会话
- **THEN** 该目录不在本机 workspace 登记表中，该会话的目录归属为「未分组」，系统 MUST NOT 为它补出一个别名

#### Scenario: 后续失败不删除已创建的目录

- **WHEN** 目录已按开关创建，但随后的 Agent 启动或落盘失败，该命令以失败或不确定终结
- **THEN** 已创建的目录仍留在主机上，系统 MUST NOT 把它删除或回滚
