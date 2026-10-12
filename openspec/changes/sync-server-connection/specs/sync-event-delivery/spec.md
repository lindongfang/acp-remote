<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## Purpose

定义 Sync 的事件投递面：订阅与游标判定、增量重放与实时投递的无缝衔接、快照的清单资源组装与完整性校验、设备可见会话清单的范围，以及累计 ACK 的单调性与恢复位置口径。它让客户端能在断线后按持久化游标继续，既不丢事件也不重复呈现。

## ADDED Requirements

### Requirement: 订阅游标判定与重置

`sync.subscribe` 的 `cursor` 为 `null` 时 SHALL 进入首次同步流程。`cursor` 非空时服务端 SHALL 校验其格式与 epoch：格式非法、`serverEpoch` 与当前不匹配、序号大于当前 head 或已早于保留窗口时，MUST 返回 `sync.cursor_invalid`，其 `details.reason` 取值 SHALL 为 `malformed`、`epoch_mismatch`、`beyond_head`、`cursor_expired` 之一。服务端 MUST NOT 把越界 cursor 钳制到 head 或静默按 head 处理。需要客户端重建快照时 SHALL 由服务端主动发送 `sync.reset_required`，其 `reason` 取值 SHALL 为 `initial_sync`、`epoch_mismatch`、`cursor_expired`、`cache_incompatible` 之一。

#### Scenario: 首次订阅走快照

- **WHEN** 客户端发送 `cursor` 为 `null` 的 `sync.subscribe`
- **THEN** 服务端进入快照流程，不返回 `sync.cursor_invalid`

#### Scenario: epoch 不匹配回错误且不钳制

- **WHEN** 客户端提交的 cursor 的 `serverEpoch` 与当前事件库的 epoch 不同
- **THEN** 服务端返回 `sync.cursor_invalid` 且 `details.reason = "epoch_mismatch"`，MUST NOT 按当前 head 继续投递事件

#### Scenario: 超出 head 与早于保留窗口可区分

- **WHEN** 客户端提交的 cursor 序号大于当前 head，或该序号对应的事件已被保留策略清理
- **THEN** 两次分别返回 `details.reason` 为 `beyond_head` 与 `cursor_expired` 的 `sync.cursor_invalid`，两者 MUST NOT 混用同一取值

### Requirement: 增量重放与实时投递的无缝衔接

cursor 有效且仍在保留窗口内时，服务端 SHALL 从该 cursor 之后读取对该设备可见的事件，按全局序号升序投递 `event`，随后发送 `sync.caught_up` 携带屏障游标。重放期间到达的实时事件 MUST 排在屏障之后投递，MUST NOT 越过序号更早的已持久化事件。投递 MUST 只包含该设备 scope 允许可见的事件；因权限过滤造成的序号空洞 SHALL 是合法的，客户端可对 `sync.caught_up` 的屏障游标发 ACK。

#### Scenario: 从游标续传

- **WHEN** 客户端以有效 cursor 订阅，且该 cursor 之后存在若干可见事件
- **THEN** 服务端按序号升序投递这些事件，随后发送 `sync.caught_up`，其后继续实时投递新事件

#### Scenario: 重放期间的实时事件不越过屏障

- **WHEN** 重放进行中，新的实时事件被提交并发布
- **THEN** 该事件在 `sync.caught_up` 之后才投递，MUST NOT 出现在序号早于它的重放事件之前

#### Scenario: 权限过滤造成的空洞不报错

- **WHEN** 某段全局序号对当前设备全部不可见
- **THEN** 投递跳过该段且不返回错误，`sync.caught_up` 的屏障游标 SHALL 高于该段，客户端据此 ACK 而不被判为缺口

### Requirement: 快照资源范围

快照 SHALL 只承载 `sessions`、`workspaces` 与 `agents` 三类清单资源；会话明细资源（`messages`、`turns`、`pending_interactions`、`config_options`、`capabilities`、`history`）MUST NOT 进入快照，无论会话来源是本节点自有还是已导入。`workspaces` 与 `agents` SHALL 受 `core.local-catalog.v1` 门控：未协商该 feature 时 MUST NOT 下发这两个资源，也 MUST NOT 以空数组占位。设备无权读取的字段与资源 MUST NOT 进入快照。

#### Scenario: 未协商目录 feature 时不占位

- **WHEN** 设备未协商 `core.local-catalog.v1`
- **THEN** 快照只含 `sessions` 资源，`workspaces` 与 `agents` MUST NOT 以空数组形式出现，也不得出现其任何字段

#### Scenario: 会话明细不进快照

- **WHEN** 某会话存在大量消息、turn、待处理交互与结构化历史
- **THEN** 快照中不出现这些明细资源，快照体积只随会话条数增长，与消息总量无关

#### Scenario: 无权资源不进入快照

- **WHEN** 某资源存在但当前设备无权读取
- **THEN** 该资源不出现在快照中，且不因缺席被下游解释为空集合

### Requirement: 快照分块与块数一致

服务端 SHALL 先发送 `sync.snapshot_begin`（携带 `snapshotId`、快照游标、`schemaVersion` 与 `chunkCount`），随后按 `chunkIndex` 从 `0` 起严格递增、连续依次发送 `sync.snapshot_chunk`，最后发送 `sync.snapshot_end`（携带同一游标、同一 `chunkCount` 与 `snapshotDigest`）。`chunkCount` MUST 等于本次实际下发的 chunk 数，取值 SHALL 在 1 到 3 之间。`sessions` 的 chunk 恒发——会话集合为空时也发一个空数组，因此 `chunkCount: 0` 是形状错误。v1 MUST NOT 支持快照断点续传。

#### Scenario: 协商目录 feature 的完整快照

- **WHEN** 设备协商了 `core.local-catalog.v1`，订阅时无 cursor
- **THEN** 服务端发送 `snapshot_begin`、三个资源各一个 chunk（顺序与 `chunkIndex` 一致）与 `snapshot_end`，`chunkCount` 为 3

#### Scenario: 空会话集合仍下发一个 chunk

- **WHEN** 本机没有任何会话
- **THEN** 服务端仍发送一个 `sessions` chunk（空数组），`chunkCount` 为 1，MUST NOT 声明 `chunkCount: 0`

#### Scenario: 块数与实发数不符即失败

- **WHEN** `snapshot_end` 声明的 `chunkCount` 与实际收到的 chunk 数不一致，或 chunk 不连续、顺序错乱
- **THEN** 客户端丢弃整个暂存快照并在重连后重新请求，MUST NOT 提交部分快照

### Requirement: 快照摘要按原始帧字节计算

`sync.snapshot_end` 的 `snapshotDigest` SHALL 按「对每条 `sync.snapshot_chunk` 消息的**原始 UTF-8 文本字节**分别求 SHA-256，按 `chunkIndex` 顺序连接这些摘要后再求一次 SHA-256」计算，结果按无填充 base64url 表示。服务端 MUST NOT 通过对结构重新序列化来得出该值，MUST NOT 使用 key 排序之外的规范化；客户端重新序列化得到的字节与其原始帧文本不同，因此该摘要 MUST 覆盖将要发出的那一份字节。

#### Scenario: 摘要可由收到的帧复算

- **WHEN** 客户端逐字记录每条 `sync.snapshot_chunk` 的原始文本，并按序连接其 SHA-256 后再求一次 SHA-256
- **THEN** 复算结果与 `sync.snapshot_end` 的 `snapshotDigest` 逐字节相同

#### Scenario: 重新序列化不能替代原帧字节

- **WHEN** 服务端先解析 chunk 再重新序列化、并以重新序列化后的字节计算摘要
- **THEN** 客户端按收到的原始帧复算的摘要与声明值不一致，快照整体 MUST 被丢弃而不是部分提交

### Requirement: 设备可见的会话清单范围

快照 `sessions` 资源 SHALL 同时覆盖本节点自有会话与已导入会话：已导入会话 SHALL 受 `resource.remote-origin.v1` 门控（未协商时 MUST NOT 出现在快照中，其事件也 MUST NOT 带非空来源信息）。每个会话条目的 `origin` 字段 SHALL 区分本节点自有与来自其它节点；来自其它节点的条目 SHALL 携带其来源节点标识、来源导出标识、来源事件库 epoch 与可达性标记。设备无权读取的会话与字段 MUST NOT 进入结果，且该过滤 MUST 作为服务端判定执行，MUST NOT 依赖客户端的筛选。

#### Scenario: 两类来源在同一快照中共存

- **WHEN** 本节点既有一个自有会话、又通过 Import 持有另一个节点的会话，设备两条都有权读取且协商了 `resource.remote-origin.v1`
- **THEN** 快照的 `sessions` chunk 同时含两条，自有条目的来源标记为本节点，另一条的来源标记为其它节点并携带来源标识与可达性

#### Scenario: 未协商来源 feature 时过滤外来会话

- **WHEN** 设备未协商 `resource.remote-origin.v1`，本节点存在已导入会话
- **THEN** 快照中不出现任何来源为其它节点的会话，MUST NOT 出现空的来源标记或占位条目

#### Scenario: 无权会话不进入结果

- **WHEN** 某会话存在但当前设备的 scope 不允许读取它
- **THEN** 该会话不出现在快照与任何对端可见输出中，且不因缺席而被解释为空集合

### Requirement: 累计 ACK 的单调性与恢复位置

`sync.ack` SHALL 表达该设备**最高已连续处理**的可见事件游标，且 MUST 单调不减：回退值 MUST 被忽略并记诊断，MUST NOT 改变服务端已知的进度。ACK 超过本连接已发送的最高游标时 MUST 返回 `sync.cursor_invalid`，MUST NOT 用客户端 ACK 推进事件库 head。服务端 MAY 在连接期保存设备 ACK 的最大值用于保留策略与诊断，但**新连接的恢复位置 MUST 以客户端在 `sync.subscribe` 中显式提交的游标为准**，MUST NOT 由服务端保存的最大 ACK 擅自推进。ACK MUST NOT 表示用户已阅读，也 MUST NOT 改变业务状态。

#### Scenario: 重复投递的屏障可被 ACK

- **WHEN** 客户端对 `sync.caught_up` 的屏障游标发送 `sync.ack`
- **THEN** 服务端接受该 ACK，即使该段序号中有对它不可见的事件

#### Scenario: 回退 ACK 被忽略

- **WHEN** 客户端发送一个低于此前已确认值的 ACK
- **THEN** 服务端忽略它并记录诊断，后续投递与服务端进度不受影响

#### Scenario: 新连接不从服务端最大 ACK 起投

- **WHEN** 某设备此前 ACK 过一个较新的游标，随后以一条更旧的 cursor 重新订阅
- **THEN** 服务端从该 cursor 之后投递事件，MUST NOT 以服务端保存的最大 ACK 跳过这段区间
