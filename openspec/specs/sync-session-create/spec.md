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

### Requirement: 创建成功直接投影活跃

`session.create` 成功建立会话后的摘要 SHALL 立即包含 `localActivity: active`，不得先发布 `idle` 再以生命周期事件切到 `active`。该取值 SHALL 反映已建立且尚未确认释放的运行资源。

#### Scenario: 创建终态没有空闲闪动

- **WHEN** 客户端创建会话且 Agent 成功建立原会话身份
- **THEN** 创建完成结果中的会话摘要直接为 `localActivity: active`，客户端不观察到先空闲后活跃的投影

### Requirement: 空会话在保留期内完整可用

`session.create` 成功后形成的空会话 SHALL 立即出现在目录、会话清单和权威读取中；在删除前，它可以进入、建立查看租约、激活并接收第一条消息。系统 MUST NOT 因其尚无内容而在保留期结束前提前隐藏或删除。

#### Scenario: 新建后立即进入并发送首条消息

- **WHEN** Web 创建成功并进入新会话，用户在保留期内激活并提交第一条消息
- **THEN** 目录和列表已经包含该会话，首条消息可正常受理，会话随后不再满足空会话条件

#### Scenario: 宽限期内刷新目录

- **WHEN** 一个尚未产生内容的会话仍处于完整保留期
- **THEN** 快照和会话清单仍返回该会话，客户端可再次进入和激活
