# sync-snapshot-scope Specification

## Purpose

定义 Sync 快照承载的资源范围与正文回源路径：快照只承载可离线渲染的清单类资源，会话明细一律由 `session.read` 按复合游标分页在线获取，使快照体积只随会话条数线性增长而与会话消息总量无关。

## Requirements

### Requirement: 快照只承载清单类资源

系统 SHALL 在 `sync.snapshot_*` 中只下发 `sessions`、`workspaces` 与 `agents` 三类资源；`messages`、`turns`、`pending_interactions`、`config_options` 与 `capabilities` 这五类会话明细资源 MUST NOT 进入快照，无论会话的 `origin.kind` 是 `local` 还是 `remote`。`snapshot_begin` 与 `snapshot_end` 的 `chunkCount` MUST 与实际下发的资源数量一致。系统 MUST NOT 以「本次没有明细」为由改变 `sessions` 摘要的字段集合或可用性。

#### Scenario: 本地会话的明细不进入快照

- **WHEN** 一个设备在协商 `core.local-catalog.v1` 后完成一次初始同步，且本节点存在本地会话及其历史消息
- **THEN** 快照只包含 `sessions`、`workspaces` 与 `agents` 三类资源，`chunkCount` 为三类资源实际占用的块数，且客户端从快照中得不到任何 `messages`、`turns`、`pending_interactions`、`config_options` 或 `capabilities` 元素

#### Scenario: 未协商目录 feature 时快照缩为单类

- **WHEN** 一个未协商 `core.local-catalog.v1` 的设备完成一次初始同步
- **THEN** 快照只包含 `sessions` 一类资源，既不含目录资源，也不含任何会话明细资源

#### Scenario: 明细缺席不被表述为完整

- **WHEN** 客户端在快照中找不到某会话的明细资源
- **THEN** 该会话在客户端侧的明细视为「尚未加载」而非「为空」，客户端 MUST NOT 据此向用户呈现该会话没有任何消息或没有任何待处理交互

### Requirement: 会话明细经分页读取

系统 SHALL 支持按会话分页读取明细：`session.read` 的 payload SHALL 接受可选的 `before` 游标与 `limit`，省略 `before` 时返回该会话最新的一页。结果 SHALL 携带本次返回的元素与一个指示是否仍有更早内容的布尔值。当 `limit` 省略或超过服务端上限时，服务端 MUST 使用其配置上限而非报错。同一会话在明细未变化时以同一 `before` 重复读取 MUST 返回相同结果。

#### Scenario: 省略游标时返回最新一页

- **WHEN** 客户端对一个有 60 条历史消息的会话发起不带 `before`、且 `limit` 为 20 的 `session.read`
- **THEN** 返回该会话最新 20 条消息，且结果指示仍有更早内容可读

#### Scenario: 带上游标时返回更早一页

- **WHEN** 客户端在上一次结果的最早一条消息上设置 `before` 后再次以相同 `limit` 读取同一会话
- **THEN** 返回紧接在该条之前的 20 条消息，不与上一次结果重叠，也不跳条

#### Scenario: 到达最早一条时不再指示更多

- **WHEN** 客户端以该会话最早一条消息为 `before` 读取，且该会话没有更早的消息
- **THEN** 结果指示没有更早内容，且不返回空占位

#### Scenario: 超出上限的 limit 被收敛

- **WHEN** 客户端请求的 `limit` 超过服务端为该会话配置的明细条数上限
- **THEN** 服务端返回不超过该上限的一页并指示仍有更早内容，MUST NOT 因该请求失败

#### Scenario: 到达保留窗口之前时显式不可读

- **WHEN** 客户端请求的 `before` 早于该节点对与会话消息的保留窗口，且该节点已按保留策略清理了更早的消息
- **THEN** 服务端返回明确的保留期已过类错误，MUST NOT 静默返回更少的元素并让客户端误以为已到最早一条

### Requirement: 游标由时间与标识复合构成

`session.read` 的 `before` SHALL 是由消息创建时间与消息标识复合而成的游标，MUST NOT 只使用其中一项，也 MUST NOT 以消息标识的数值或字典序作为排序依据。服务端 MUST 按创建时间升序返回一页，并在同一创建时间的多个消息之间以稳定且可重复的次序排列，使相邻两页不重不漏。

#### Scenario: 同一时刻的多条消息不重不漏

- **WHEN** 一个会话有多条创建时间完全相同的消息，且客户端连续翻页跨越该时刻
- **THEN** 每条消息在全部页面中恰好出现一次，客户端按游标拼接后得到与全量一致的序列

#### Scenario: 游标不可由标识前缀构造

- **WHEN** 客户端构造 `before` 时只提供消息标识而未提供创建时间
- **THEN** 服务端拒绝该请求并返回参数类错误，MUST NOT 猜测或补全缺失的排序分量

#### Scenario: 不按标识排序

- **WHEN** 一个会话的消息标识的数值次序与创建时间次序不一致
- **THEN** 服务端仍按创建时间返回，并在同一创建时间内以稳定次序排列，结果不依赖标识的数值大小

### Requirement: 分页只作用于客户端同步面

`session.read` 的分页参数 SHALL 只在 Sync 传输面上生效。Node Link 传输面上的同名命令 MUST NOT 因 Sync 新增的分页参数而改变其行为，也 MUST NOT 因客户端未提供这些参数而失败。

#### Scenario: 节点协议侧不受影响

- **WHEN** 一个 Node Link 侧的调用方按既有形状发起 `session.read` 且不携带任何分页参数
- **THEN** 其行为与本次变更前一致，返回该会话的明细而不因缺少分页参数被拒绝

#### Scenario: 同步面未提供参数时使用默认页

- **WHEN** 一个客户端发起的 `session.read` 只提供 `include` 而未提供 `before` 与 `limit`
- **THEN** 服务端按默认页大小返回最新一页，MUST NOT 要求客户端必须显式给出页大小
