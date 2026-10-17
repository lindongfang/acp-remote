# Sync Workspace Catalog 增量

## MODIFIED Requirements

### Requirement: 目录与 Agent 目录随快照下发

系统 SHALL 在 `sync.snapshot_*` 中提供两个资源：`workspaces`（元素为 `{ directoryId, path }`）与 `agents`（元素为 `{ agentId, displayName, default }`）。`workspaces` 的条目集合 MUST 等于本节点记录过的目录（即曾成功创建过会话的目录），MUST NOT 需要任何人工登记步骤。客户端 MUST 能仅凭一次快照渲染目录页与创建选择器，MUST NOT 需要额外的在线查询命令。目录路径只出现在 `workspaces` 资源这一处，会话摘要的目录引用 MUST NOT 重复携带它——需要路径的客户端按 `directoryId` 在同一份快照内查表。

#### Scenario: 首次同步获得完整目录

- **WHEN** 一个已认证设备完成首次订阅并接收完整快照
- **THEN** 快照包含本节点记录过的全部目录与配置中的全部 Agent，且元素只含本条列出的字段

#### Scenario: 空目录仍出现在快照中

- **WHEN** 一个已记录目录下没有任何会话
- **THEN** 它仍出现在 `workspaces` 资源中，使客户端能渲染该目录并说明其中没有会话

#### Scenario: 离线渲染不依赖在线查询

- **WHEN** 客户端断线并按 cursor 重放、在重连完成前渲染本地缓存
- **THEN** 目录与 Agent 目录来自上一次成功替换的快照，渲染不需要任何在线查询命令

#### Scenario: 路径只在目录资源里出现一次

- **WHEN** 检查一次完整快照
- **THEN** 规范化路径只出现在 `workspaces` 的条目上，会话摘要的目录引用里 MUST NOT 出现它

#### Scenario: 目录由会话自动出现

- **WHEN** 在一个此前从未使用过的目录里成功创建了一个会话
- **THEN** 该目录以 `{ directoryId, path }` 出现在下一次快照（或对应的增量替换）中，无需任何登记命令

## REMOVED Requirements

### Requirement: 会话摘要携带目录归属引用

**Reason**: 目录别名体系随人工登记一并删除；摘要里的目录引用由 `{ alias, displayName }` 改为 `{ directoryId }`，场景集合（别名重指向、别名被删除）随之不再成立。

**Migration**: 无可部署的历史客户端，不保留兼容分支；新要求「会话摘要携带目录标识引用」承接同一组不变量（创建时确定、不按路径反查、缺失时不猜测）。

### Requirement: 目录资源的协商与下发范围

**Reason**: 该要求以「路径字段另受 `core.workspace-path.v1` 门控」为核心，而本变更撤销该 feature（路径成为目录条目的内容），因此整条要求的场景集合（未协商路径能力时键缺席）不再成立。

**Migration**: 由新要求「目录资源的协商与路径成对下发」承接；`core.local-catalog.v1` 的门控语义与「未协商时不发送资源」「摘要字段 SKIP」两条场景原样保留。

### Requirement: 目录路径只下发给协商该能力的设备

**Reason**: 路径成为目录条目的内容（客户端需要它来呈现目录名与消歧），不再由 feature `core.workspace-path.v1` 单独门控；该 feature 撤销。

**Migration**: 由新增要求「目录路径只随目录条目下发」承接；「客户端提交的路径不被回显」「目录浏览只返回当层条目」两条约束不变。

## ADDED Requirements

### Requirement: 会话摘要携带目录标识引用

系统 SHALL 在 Sync 的会话摘要投影中携带 `workspace` 字段，取值为 `{ directoryId }` 或 `null`（`null` 表示「未分组」）。`directoryId` MUST 是目录资源条目的同一个不透明标识；该引用 MUST 在会话创建时按当时解析得到的目录 id 确定并随会话持久化，投影 MUST NOT 依据当前规范化路径反查目录表、拼接或推导该引用。客户端 MUST NOT 解析、拼接或由目录 id 反推出路径。

#### Scenario: 已在已登记目录中创建的会话

- **WHEN** 一个会话创建时使用了一个本机已记录的目录（`directoryId`）
- **THEN** 其会话摘要的 `workspace` 为 `{ directoryId }`，且与 `workspaces` 资源里的同一条目一致

#### Scenario: 没有目录归属的会话

- **WHEN** 一个会话没有持久化的目录 id（例如目录解析失败，或它早于本能力落地且没有可用路径）
- **THEN** 其会话摘要的 `workspace` 为 `null`，系统不为其猜测、补齐或反查任何目录

#### Scenario: 目录记录被清理后归属仍稳定

- **WHEN** 会话的持久化 `directoryId` 在本机目录表中已不存在
- **THEN** `workspace` 仍是该 `directoryId`，不变成 `null`，也不按路径反查或替换为其它目录

#### Scenario: imported 会话的归属

- **WHEN** 读取一条 imported（远程来源）会话的摘要
- **THEN** 其 `workspace` 为 `null`，直到提供该会话的 Owner 侧投影携带目录引用为止；系统 MUST NOT 用本机目录集合为它猜测归属

### Requirement: 目录资源的协商与路径成对下发

两个资源 SHALL 受 feature `core.local-catalog.v1` 门控：客户端未协商该 feature 时 MUST NOT 收到它们，也 MUST NOT 收到空数组占位；`body.requiredFeatures` 含该 feature 而对方未协商时，接收方 MUST 返回 `protocol.feature_required`。会话摘要的 `workspace` 字段 SHALL 是同一 feature 门控下的可选字段：未协商该 feature 的客户端 MUST NOT 在会话摘要中收到该字段（SKIP），且它 MUST NOT 出现在摘要的必填集合中。目录条目里的路径字段 MUST 始终与 `directoryId` 一起下发——路径不再由独立 feature 门控（`core.workspace-path.v1` 已撤销）。目录资源不再按设备授权过滤：任何已认证设备都收到本节点完整记录的目录与配置中的 Agent 集合。

#### Scenario: 未协商时不发送该资源

- **WHEN** 客户端未在握手协商中声明 `core.local-catalog.v1`
- **THEN** 快照中既不出现 `workspaces` 与 `agents` 资源，也不出现空数组替代

#### Scenario: 未协商时摘要不含目录字段

- **WHEN** 一台未协商 `core.local-catalog.v1` 的旧客户端接收会话摘要
- **THEN** 摘要中不出现 `workspace` 字段（不是 `null`，而是该字段缺席），整条消息仍能通过该客户端自己的 schema 校验

#### Scenario: 路径与目录标识成对出现

- **WHEN** 一台协商了 `core.local-catalog.v1` 的客户端接收快照
- **THEN** 每个 `workspaces` 条目同时含 `directoryId` 与 `path` 两个键，不存在只有其中一个的形态

#### Scenario: 已认证设备收到完整目录

- **WHEN** 一台已认证设备协商了 `core.local-catalog.v1`
- **THEN** 它收到的快照含本节点记录过的全部目录与配置中的全部 Agent，不因设备维度而缺少任何条目

### Requirement: 目录路径只随目录条目下发

规范化路径 MUST NOT 出现在除 `workspaces` 资源条目以外的一切对端可见输出中，包括会话摘要、事件载荷、错误 `details`、`command.result` 与审计记录的前像。条目里的 `path` SHALL 是已记录目录的规范化权威值。客户端自己提交的目录 MUST NOT 被回显为任何对端可见字段。目录浏览只返回它必然要给客户端的那一层条目（见 `sync-workspace-browse`），不得借该命令泄漏其它层的路径。

#### Scenario: 事件与错误都不含路径

- **WHEN** 一次会话创建成功或失败后检查广播事件、错误载荷与审计记录
- **THEN** 任何字段（含摘要计算的输入）都不包含本机规范化路径明文，客户端自己提供的目录也不被回显

#### Scenario: 客户端提交的目录不被回显

- **WHEN** 一次 `session.create` 携带客户端提供的绝对目录并成功
- **THEN** 快照里出现的只是该目录被记录后的条目，命令结果与错误载荷里不出现被提交的路径原文

#### Scenario: 错误响应不回显路径

- **WHEN** 一次目录相关请求因形状或解析失败被拒
- **THEN** 错误 `details` 只含 `directoryId` 或字段名，不含任何路径明文
