# sync-workspace-catalog Specification

## Purpose

定义 Sync v1 中设备可见的本机 workspace 目录、Agent 目录与会话摘要里的目录归属引用，使客户端配对后即可渲染目录页与创建选择器、断线时仍能离线渲染，同时把规范化路径的可见面收在受 feature 门控的目录资源条目上。

## Requirements

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

系统 SHALL 在 `sync.snapshot_*` 中提供两个新增资源：`workspaces`（元素为 `{ alias, displayName }`，协商 `core.workspace-path.v1` 时另含一个规范化路径字段）与 `agents`（元素为 `{ agentId, displayName, default }`）。客户端 MUST 能仅凭一次快照渲染目录页与创建选择器，MUST NOT 需要额外的在线查询命令才能获得这两份目录。目录路径只出现在 `workspaces` 资源这一处，会话摘要的目录引用 MUST NOT 重复携带它——需要路径的客户端按引用里的别名在同一份快照内查表。

#### Scenario: 首次同步获得完整目录

- **WHEN** 一个已认证设备完成首次订阅并接收完整快照
- **THEN** 快照包含当前全部已登记的 workspace 与已配置的 Agent，且元素只含本条列出的字段

#### Scenario: 空目录仍出现在快照中

- **WHEN** 一个已登记 workspace 下没有任何会话
- **THEN** 它仍出现在 `workspaces` 资源中，使客户端能渲染空目录而不是把它当作不存在

#### Scenario: 离线渲染不依赖在线查询

- **WHEN** 客户端断线并按 cursor 重放、在重连完成前渲染本地缓存
- **THEN** 目录与 Agent 目录来自上一次成功替换的快照，渲染不需要任何在线查询命令

#### Scenario: 路径只在目录资源里出现一次

- **WHEN** 检查一次协商了 `core.workspace-path.v1` 的完整快照
- **THEN** 规范化路径只出现在 `workspaces` 的条目上，会话摘要的目录引用里 MUST NOT 出现它

### Requirement: 目录资源的协商与下发范围

两个新增资源 SHALL 受 feature `core.local-catalog.v1` 门控：客户端未协商该 feature 时 MUST NOT 收到它们，也 MUST NOT 收到空数组占位；`body.requiredFeatures` 含该 feature 而对方未协商时，接收方 MUST 返回 `protocol.feature_required`。会话摘要的 `workspace` 字段 SHALL 是同一 feature 门控下的可选字段：未协商该 feature 的客户端 MUST NOT 在会话摘要中收到该字段（SKIP），且它 MUST NOT 出现在摘要的必填集合中，以免旧客户端因未知字段拒绝整条消息。`workspaces` 条目上的规范化路径字段 SHALL 另受 feature `core.workspace-path.v1` 门控，未协商时该键缺席（不是 `null`、不是空串）。目录资源不再按设备授权过滤：任何已认证设备都收到本节点完整的已登记目录与已配置 Agent 集合。

#### Scenario: 未协商时不发送该资源

- **WHEN** 客户端未在握手协商中声明 `core.local-catalog.v1`
- **THEN** 快照中既不出现 `workspaces` 与 `agents` 资源，也不出现空数组替代

#### Scenario: 未协商时摘要不含目录字段

- **WHEN** 一台未协商 `core.local-catalog.v1` 的旧客户端接收会话摘要
- **THEN** 摘要中不出现 `workspace` 字段（不是 `null`，而是该字段缺席），整条消息仍能通过该客户端自己的 schema 校验

#### Scenario: 未协商路径能力时键缺席

- **WHEN** 一台协商了 `core.local-catalog.v1`、但未协商 `core.workspace-path.v1` 的客户端接收快照
- **THEN** `workspaces` 的条目里不出现路径键（不是空串、不是 `null`），整条消息仍通过该客户端自己的 schema 校验

#### Scenario: 已认证设备收到完整目录

- **WHEN** 一台已认证设备协商了 `core.local-catalog.v1`
- **THEN** 它收到的快照含本节点全部已登记目录与已配置 Agent，不因设备维度而缺少任何条目

### Requirement: 目录路径只下发给协商该能力的设备

规范化路径 MUST NOT 出现在除 `workspaces` 资源条目以外的一切对端可见输出中，包括会话摘要、事件载荷、错误 `details`、`command.result` 与审计记录的前像。`workspaces` 条目上的路径字段 SHALL 只在客户端协商了 `core.workspace-path.v1` 时下发，且其取值 SHALL 是已登记目录的规范化权威值。客户端自己提交的目录 MUST NOT 被回显为任何对端可见字段。目录浏览只返回它必然要给客户端的那一层条目（见 `sync-workspace-browse`），不得借该命令泄漏其它层的路径。

#### Scenario: 未协商时不出现路径

- **WHEN** 一台未协商 `core.workspace-path.v1` 的客户端接收快照、事件与命令结果
- **THEN** 这些输出中的任何字段都不含本机规范化路径明文或它的分段

#### Scenario: 未登记的目录路径不出现在任何输出中

- **WHEN** 检查一次协商了路径能力的完整快照与其后的事件与命令结果
- **THEN** 只出现已登记目录的路径，客户端自己提交过的目录不被回显

#### Scenario: 错误响应不回显路径

- **WHEN** 一次目录相关请求因形状或解析失败被拒
- **THEN** 错误 `details` 只含别名或字段名，不含任何路径明文
