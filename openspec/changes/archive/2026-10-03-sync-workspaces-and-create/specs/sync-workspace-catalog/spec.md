<!-- 新增能力：Sync 面设备可见的本机目录（workspace 与 Agent）与会话的目录归属引用。 -->

## Purpose

定义 Sync v1 中设备可见的本机 workspace 目录、Agent 目录与会话摘要里的目录归属引用，使客户端配对后即可渲染目录页与创建选择器、断线时仍能离线渲染，同时保证规范化路径不进入任何对端可见输出。

## ADDED Requirements

### Requirement: 会话摘要携带目录归属引用

系统 SHALL 在 Sync 的会话摘要投影中携带 `workspace` 字段，取值为 `{ alias, displayName }` 或 `null`（`null` 表示原型的「未分组」）。该引用 MUST 在会话创建时按当时解析使用的别名确定并随会话持久化；投影 MUST NOT 依据当前规范化路径反查解析表、拼接或推导该引用。

#### Scenario: 已在已登记目录中创建的会话

- **WHEN** 一个会话创建时使用了本机已登记的 workspace 别名
- **THEN** 其会话摘要的 `workspace` 为该别名当前的 `{ alias, displayName }`，与登记值一致

#### Scenario: 没有目录归属的会话

- **WHEN** 一个会话没有可用的持久化别名（创建时未解析出别名，或它早于本能力落地）
- **THEN** 其会话摘要的 `workspace` 为 `null`，系统不为其猜测、补齐或反查任何别名

#### Scenario: 目录重指向后归属不漂移

- **WHEN** 同一别名被重新指向另一个本机目录后，读取此前用该别名创建的会话摘要
- **THEN** `workspace` 仍是该别名的 `{ alias, displayName }`，不变成 `null`，也不出现任何路径信息

#### Scenario: 别名已从本机删除时归属仍稳定

- **WHEN** 会话的持久化别名在本机 workspace 登记表中已不存在
- **THEN** `workspace.alias` 仍为该别名，`displayName` 回退为该别名本身；归属不变成 `null`，也不因缺少登记而反查路径

#### Scenario: imported 会话的归属

- **WHEN** 读取一条 imported（远程来源）会话的摘要
- **THEN** 其 `workspace` 为 `null`，直到提供该会话的 Owner 侧投影携带目录引用为止；系统 MUST NOT 用本机 workspace 集合为它猜测归属

### Requirement: 目录与 Agent 目录随快照下发

系统 SHALL 在 `sync.snapshot_*` 中提供两个新增资源：`workspaces`（元素为 `{ alias, displayName }`）与 `agents`（元素为 `{ agentId, displayName, default }`）。客户端 MUST 能仅凭一次快照渲染目录页与创建选择器，MUST NOT 需要额外的在线查询命令才能获得这两份目录。

#### Scenario: 首次同步获得完整目录

- **WHEN** 一个已授权设备完成首次订阅并接收完整快照
- **THEN** 快照包含当前全部已登记的 workspace 与已配置的 Agent，且元素只含上述字段

#### Scenario: 空目录仍出现在快照中

- **WHEN** 一个已登记 workspace 下没有任何会话
- **THEN** 它仍出现在 `workspaces` 资源中，使客户端能渲染空目录而不是把它当作不存在

#### Scenario: 离线渲染不依赖在线查询

- **WHEN** 客户端断线并按 cursor 重放、在重连完成前渲染本地缓存
- **THEN** 目录与 Agent 目录来自上一次成功替换的快照，渲染不需要任何在线查询命令

### Requirement: 目录资源的协商与授权过滤

两个新增资源 SHALL 受 feature `core.local-catalog.v1` 门控：客户端未协商该 feature 时 MUST NOT 收到它们，也 MUST NOT 收到空数组占位；`body.requiredFeatures` 含该 feature 而对方未协商时，接收方 MUST 返回 `protocol.feature_required`。会话摘要的 `workspace` 字段 SHALL 是同一 feature 门控下的可选字段：未协商该 feature 的客户端 MUST NOT 在会话摘要中收到该字段（SKIP），且它 MUST NOT 出现在摘要的必填集合中，以免旧客户端因未知字段拒绝整条消息。资源 SHALL 按设备授权过滤：设备缺少目录读取授权时 MUST NOT 下发，未授权条目 MUST NOT 出现在快照中。

#### Scenario: 未协商时不发送该资源

- **WHEN** 客户端未在握手协商中声明 `core.local-catalog.v1`
- **THEN** 快照中既不出现 `workspaces` 与 `agents` 资源，也不出现空数组替代

#### Scenario: 未协商时摘要不含目录字段

- **WHEN** 一台未协商 `core.local-catalog.v1` 的旧客户端接收会话摘要
- **THEN** 摘要中不出现 `workspace` 字段（不是 `null`，而是该字段缺席），整条消息仍能通过该客户端自己的 schema 校验

#### Scenario: 缺少目录读取授权的设备不下发

- **WHEN** 一台设备的授权集合不含目录读取能力
- **THEN** 它收到的快照不含这两个资源，且不因缺少资源而收到部分或占位数据

### Requirement: 目录相关输出不泄漏规范化路径

目录相关的一切对端可见输出 MUST 只含 `alias` 与 `displayName`，包括快照资源与快照 item、会话摘要、事件载荷、错误 `details`、`command.result` 与审计记录的前像。规范化路径、其父级片段与末段 MUST NOT 出现在这些输出中的任何字段（含摘要计算的输入）。

#### Scenario: 快照与会话摘要只含引用

- **WHEN** 检查一次完整快照与全部会话摘要
- **THEN** 其中任何字段都不含本机规范化路径明文或它的分段

#### Scenario: 错误响应不回显路径

- **WHEN** 一次引用目录的操作失败（别名未登记或本机解析失败）
- **THEN** 错误 `details` 只含别名或字段名，不含任何路径明文
