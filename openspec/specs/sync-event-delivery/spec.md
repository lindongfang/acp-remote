# sync-event-delivery Specification

## Purpose
定义 Sync 的事件投递面：订阅与游标判定、增量重放与实时投递的无缝衔接、快照的清单资源组装与完整性校验、设备可见会话清单的范围，以及累计 ACK 的单调性与恢复位置口径。它让客户端能在断线后按持久化游标继续，既不丢事件也不重复呈现。

## Requirements

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

### Requirement: 游标序号的域上限是协议错误

序号超出 v1 上界（`2^63-1`）或超出 64 位承载范围的 cursor 是**协议错误**，MUST 返回 `protocol.schema_invalid`（`SYNC_PROTOCOL.md` §3.3），MUST NOT 折叠为 `sync.cursor_invalid` 的任一 `reason`；「无法表示」与「大于 head」是两类不同条件，`sync.ack` 路径同样适用。

#### Scenario: 超出 v1 上界的序号是协议错误而非游标判定

- **WHEN** 客户端提交的 cursor 序号超出 v1 上界 `2^63-1`（含超出 64 位承载范围的取值，例如 `"99999999999999999999999"`）
- **THEN** 服务端返回 `protocol.schema_invalid`，MUST NOT 返回 `sync.cursor_invalid`（含 `details.reason = "beyond_head"`），`sync.ack` 路径同样按协议错误处理
- **AND** 序号**可表示**但大于当前 head 时仍回 `details.reason = "beyond_head"`，两类条件 MUST NOT 混用

### Requirement: 增量重放与实时投递的衔接

cursor 有效且仍在保留窗口内时，服务端 SHALL 从该 cursor 之后读取事件，按全局序号升序投递 `event`，随后发送 `sync.caught_up` 携带屏障游标。重放期间到达的实时事件 MUST 排在屏障之后投递，MUST NOT 越过序号更早的已持久化事件。设备方向不再有按授权的可见性过滤，因此投递集合是「该 cursor 之后的全部已持久化事件」；仍可能出现的序号空洞只来自保留策略（已压缩或已清理的事件），客户端 MUST NOT 因此报错，并可对 `sync.caught_up` 的屏障游标发 ACK。

#### Scenario: 从游标续传

- **WHEN** 客户端以有效 cursor 订阅，且该 cursor 之后存在可读事件
- **THEN** 服务端按序号升序投递这些事件，随后发送 `sync.caught_up`，其后继续实时投递新事件

#### Scenario: 重放期间的实时事件不越过屏障

- **WHEN** 重放进行中，新的实时事件被提交并发布
- **THEN** 该事件在 `sync.caught_up` 之后才投递，MUST NOT 出现在序号早于它的重放事件之前

#### Scenario: 保留策略造成的空洞不报错

- **WHEN** 某段全局序号在保留窗口内但因保留策略已压缩或清理而不可读
- **THEN** 客户端不因这段空洞报错，按屏障游标继续，并可对其发 ACK

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

### Requirement: 事件帧的来源身份字段经 feature 协商启用

`origin` 的身份字段与来源取值经 feature `core.event-origin.v1` 协商启用。协商了该 feature 的客户端 SHALL 收到含 `nodeId` 键的 `origin`，且 `kind` 取 `device` 时 `deviceId` SHALL 非空、取 `node` 时 `nodeId` SHALL 非空、其余取值时两者 SHALL 为 `null`。未协商该 feature 的客户端 MUST NOT 收到 `nodeId` 键、MUST NOT 收到 `kind` 为 `node` 的事件，其 `origin` 形状 SHALL 与本 feature 引入前完全一致。服务端 MUST NOT 因客户端版本而伪造或降级 `kind`。

#### Scenario: 协商后收到带身份字段的来源

- **WHEN** 客户端协商了 `core.event-origin.v1`
- **THEN** 它收到的每条 `event` 帧的 `origin` 都含 `nodeId` 键，且由另一个节点经 Node Link 命令产生的事件其 `kind` 为 `node`、`nodeId` 非空、`deviceId` 为 `null`

#### Scenario: 未协商时形状不变且不投递节点来源事件

- **WHEN** 客户端未协商 `core.event-origin.v1`，而事件库中存在由另一个节点命令产生的事件
- **THEN** 服务端 MUST NOT 投递该事件、MUST NOT 在该连接的 `origin` 里出现 `nodeId` 键，其余事件的 `origin` 形状与本 feature 引入前一致

### Requirement: 会话清单从权威在线组装

快照 SHALL 覆盖本节点自有清单及当前可用且完成端到端协商的远程清单；远程条目从 Owner 在线获取，MUST NOT 取自 Access 磁盘。远程条目 SHALL 标明来源与可用性，来源不可达 MUST NOT 被表达为成功读取了空清单。任何已认证设备可见全部本节点自有会话，远程范围取决于节点授权及实际能力，不新增设备权限维度。

#### Scenario: 两类来源在同一快照中共存
- **WHEN** 本节点同时有自有会话及当前在线可访问的 Owner 会话
- **THEN** 已协商客户端收到正确来源的两类摘要，远程资料不落盘

#### Scenario: 未协商来源 feature 时过滤外来会话
- **WHEN** 设备不具备所需远程来源及无持久中转能力
- **THEN** 不下发远程条目和新消息形状，显式说明远程功能不支持，本地能力保留

#### Scenario: 已认证设备看到全部自有会话
- **WHEN** 一台已认证设备请求本节点清单
- **THEN** 全部自有会话按原范围返回，不因远程 Owner 不可用而被隐藏

#### Scenario: 未打开会话的标题变化
- **WHEN** Owner 上一个未被当前客户端打开的会话摘要发生变化
- **THEN** 在线客户端获知该来源清单变化并重新读取，列表最终显示新值，不依赖打开该会话才能更新

### Requirement: 自有和远程事件分开恢复

既有本地全局游标、增量重放与累计 ACK SHALL 只覆盖当前节点自有日志。远程事件 SHALL 使用独立来源流，以 Owner 节点、会话、epoch 和来源序号定位，MUST NOT 冒用 Access 本地全局或会话序号。远程进度只在当前运行期保存，Access 重启后由客户端显式提供来源位置或重新读取，不能从本地恢复。

#### Scenario: 本地游标不因 Access 重启失效
- **WHEN** 同时访问本地与远程会话的 Access 重启
- **THEN** 本地持久 epoch 与本地游标规则保持，远程单独重新建立来源流

#### Scenario: 来源流有序衔接
- **WHEN** 远程来源位置有效且 Owner 正在重放并产生实时事件
- **THEN** 先按来源序号重放至已捕获屏障，再接实时事件，不重复应用稳定事件且不静默漏掉区间

#### Scenario: 来源流位置无效
- **WHEN** 来源 epoch 不匹配、位置越界或已过期
- **THEN** 拒绝并明确重置原因；客户端重建 Owner 当前状态，不能钳制到 head 假称追平

### Requirement: 远程确认只表达运行期处理进度

远程来源流的累计确认 SHALL 只表示该客户端当前已连续处理至给定来源位置，MUST NOT 写入 Access 数据库、推进 Owner head、表示用户已阅读或使 Owner 提前删除权威数据。确认 MUST 绑定当前连接、来源与临时订阅，超过已发最高位置或跨来源确认 MUST 拒绝；回退确认 SHALL 忽略并诊断。

#### Scenario: 重启不恢复旧确认
- **WHEN** Access 收到远程确认后重启
- **THEN** 不存在可读取的远程确认记录，新订阅只使用调用方显式提交的位置

#### Scenario: 多个来源不能互相确认
- **WHEN** 客户端把来源 A 的游标提交给来源 B 的订阅
- **THEN** 明确拒绝，不更新任何来源进度

