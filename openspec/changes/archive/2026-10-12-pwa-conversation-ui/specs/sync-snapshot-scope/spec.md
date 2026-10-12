# Spec Delta

## MODIFIED Requirements

### Requirement: 快照只承载清单类资源

系统 SHALL 在 `sync.snapshot_*` 中只下发 `sessions`、`workspaces` 与 `agents` 三类资源；`messages`、`turns`、`pending_interactions`、`config_options`、`capabilities` 与 `history` 这六类会话明细资源 MUST NOT 进入快照，无论会话的 `origin.kind` 是 `local` 还是 `remote`。`snapshot_begin` 与 `snapshot_end` 的 `chunkCount` MUST 与实际下发的资源数量一致。系统 MUST NOT 以「本次没有明细」为由改变 `sessions` 摘要的字段集合或可用性。

#### Scenario: 本地会话的明细不进入快照

- **WHEN** 一个设备在协商 `core.local-catalog.v1` 后完成一次初始同步，且本节点存在本地会话及其历史消息
- **THEN** 快照只包含 `sessions`、`workspaces` 与 `agents` 三类资源，`chunkCount` 为三类资源实际占用的块数，且客户端从快照中得不到任何 `messages`、`turns`、`pending_interactions`、`config_options`、`capabilities` 或 `history` 元素

#### Scenario: 未协商目录 feature 时快照缩为单类

- **WHEN** 一个未协商 `core.local-catalog.v1` 的设备完成一次初始同步
- **THEN** 快照只包含 `sessions` 一类资源，既不含目录资源，也不含任何会话明细资源

#### Scenario: 明细缺席不被表述为完整

- **WHEN** 客户端在快照中找不到某会话的明细资源
- **THEN** 该会话在客户端侧的明细视为「尚未加载」而非「为空」，客户端 MUST NOT 据此向用户呈现该会话没有任何消息或没有任何待处理交互

#### Scenario: 结构化历史条目不进入快照

- **WHEN** 一个会话已有工具调用、终端输出或计划变化等结构化历史条目，设备完成一次初始同步
- **THEN** 快照里没有任何历史条目元素，这些条目只在客户端按会话在线读取时下发

### Requirement: 分页只作用于客户端同步面

`session.read` 的分页参数 SHALL 只在 Sync 传输面上生效。Node Link 传输面上的同名命令 MUST NOT 因 Sync 新增的分页参数而改变其行为，也 MUST NOT 因客户端未提供这些参数而失败；这一口径同样适用于按会话读取结构化历史条目所引入的游标与页标记。

#### Scenario: 节点协议侧不受影响

- **WHEN** 一个 Node Link 侧的调用方按既有形状发起 `session.read` 且不携带任何分页参数
- **THEN** 其行为与本次变更前一致，返回该会话的明细而不因缺少分页参数被拒绝

#### Scenario: 同步面未提供参数时使用默认页

- **WHEN** 一个客户端发起的 `session.read` 只提供 `include` 而未提供 `before` 与 `limit`
- **THEN** 服务端按默认页大小返回最新一页，MUST NOT 要求客户端必须显式给出页大小

#### Scenario: 历史条目的游标不泄漏到节点协议面

- **WHEN** 一个 Node Link 侧的调用方发起 `session.read`，其 `include` 里含 Sync 面新增的取值或携带 Sync 面新增的游标字段
- **THEN** 该面的行为不因这些取值与字段改变，既 MUST NOT 返回与既有形状不同的结果，也 MUST NOT 因它们失败
