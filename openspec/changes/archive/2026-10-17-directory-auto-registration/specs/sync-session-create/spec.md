# Sync Session Create 增量

## MODIFIED Requirements

### Requirement: 目录校验与失败分类

本机 SHALL 对生效的目录来源做与既有口径一致的校验：`directoryId` 按本节点目录表解析，客户端提供的目录 MUST 是绝对路径、并以 `canonicalize` 的结果作为权威值。默认（未给出「不存在则先创建」开关，或该开关为假）时，客户端提供的目录 MUST 已存在且为目录。形状不合法（相对路径、含 `..`、或目录标识未记录）时 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；形式合法但无法解析（目录被删除、被替换为文件、规范化失败、权限不足）时 MUST 以服务端不可用类错误拒绝。开关为真时目录可以不存在：系统 SHALL 先创建它（连同缺失的父层）再启动 Agent；路径上已存在但不是目录的成分、或创建失败（父层不可达、无写权限）MUST 以服务端不可用类错误拒绝。所有失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录，也 MUST NOT 留下目录记录。

#### Scenario: 形状不合法是参数类错误

- **WHEN** payload 给出一个相对路径、含 `..` 的路径，或一个本节点没有记录的 `directoryId`
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
- **THEN** 该目录不获得任何别名，其目录记录与本次会话在同一次提交里写入，会话归属指向新记录的目录标识

#### Scenario: 后续失败不删除已创建的目录

- **WHEN** 目录已按开关创建，但随后的 Agent 启动或落盘失败，该命令以失败或不确定终结
- **THEN** 已创建的目录仍留在主机上，系统 MUST NOT 把它删除或回滚，也 MUST NOT 为它写入目录记录

## REMOVED Requirements

### Requirement: 载荷形状与目录来源

**Reason**: 目录引用由 `workspaceAlias` 改为 `directoryId`；「建目录开关不与别名同用」的场景随别名删除而改写为「不与目录标识同用」。

**Migration**: 由新要求「载荷形状与目录来源（目录标识）」承接；「只允许三类字段」「未知键即 schema 校验错误」「凭据不得经本命令传入」三条不变量不变。

### Requirement: 引用失败与解析失败的分类

**Reason**: 引用面从「未登记别名」变为「未记录目录标识」；两类失败（参数类 / 服务端不可用类）的语义保留，场景标题随输入面改写。

**Migration**: 由新要求「引用失败与解析失败的分类（目录标识）」承接；「未配置的 Agent 在受理之后失败」场景原样保留。

## ADDED Requirements

### Requirement: 载荷形状与目录来源（目录标识）

`session.create` 的 payload MUST 只允许 `agentId`、目录引用（`directoryId` 或客户端提供的目录，二者之一）与一个可选的「目录不存在则先创建」开关三类字段。出现任何其它键（含 `exportId`、`templateParams`、`mcpServers`、MCP 配置或凭据字段）时 MUST 以 schema 校验错误（`protocol.schema_invalid`）拒绝，MUST NOT 创建会话、MUST NOT 部分应用参数、MUST NOT 回显提交内容。该开关 MUST NOT 与 `directoryId` 同时出现——已记录的目录必然已存在，两者同时给出时 MUST 以 schema 校验错误拒绝。凭据 MUST NOT 经本命令传入。

#### Scenario: 未登记的键被拒且不创建

- **WHEN** payload 携带 `exportId`、`templateParams`、MCP 配置或凭据类字段
- **THEN** 返回 `protocol.schema_invalid`，不创建会话、不启动进程，错误细节不回显该取值

#### Scenario: 目录引用二选一

- **WHEN** payload 同时给出 `directoryId` 与一个客户端提供的目录
- **THEN** 请求被拒绝，不创建会话；系统 MUST NOT 猜测以哪一个为准

#### Scenario: 建目录开关不与别名同用

- **WHEN** payload 同时给出 `directoryId` 与「不存在则先创建」开关
- **THEN** 返回 `protocol.schema_invalid`，不查询目录表、不触碰文件系统

### Requirement: 引用失败与解析失败的分类（目录标识）

引用的 `directoryId` 不在本节点目录表内时，系统 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；引用的 Agent 在本机没有对应 profile 时，失败发生在**受理之后**，以 `failed` 终态收尾（判据见本文件「终态与幂等」）。目录已记录但本机解析失败（目录被删除、被替换为文件、规范化失败或权限不足）时 MUST 以服务端不可用类错误拒绝。三类失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录或替代 Agent。

#### Scenario: 未登记别名是参数类错误

- **WHEN** payload 引用一个本节点从未记录的 `directoryId`
- **THEN** 返回参数类错误（Sync 面为 `protocol.schema_invalid`），且不访问文件系统

#### Scenario: 已登记但解析失败是服务端错误

- **WHEN** `directoryId` 已记录，但其目录已被删除或已不是目录
- **THEN** 返回服务端不可用类错误（`internal.unavailable`），不创建会话也不启动进程

#### Scenario: 未配置的 Agent 在受理之后失败

- **WHEN** payload 的 `agentId` 在本机没有对应的 profile
- **THEN** 该命令先被受理，随后以 `failed` 终态收尾（错误码复用既有 `internal.unavailable`），本机不产生会话行
