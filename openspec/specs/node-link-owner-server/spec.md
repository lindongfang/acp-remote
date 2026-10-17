# node-link-owner-server Specification

## Purpose
定义 Owner 侧 Node Link WSS 服务端（`server::node_link`）的可观察行为：四步握手与 feature 协商、信封与双向 connectionSequence、按信任记录过滤的 Export catalog 投影、attachment generation 与快照/事件/ACK、命令的授权/幂等/终态与 `session.create` 硬约束、心跳与慢连接隔离、撤销即时传播与审计，使已授权 Access 节点能安全访问导出资源而未授权方无处入手。

## Requirements

### Requirement: 握手准入与超时

连接建立后的第一条消息 SHALL 是 `node.hello`，认证完成前收到任何业务消息 MUST 以 4401 关闭；整个握手（`node.hello` → `node.ready`）MUST 在 15 秒内完成，超时以 4408 关闭。认证失败一律先返回 `link.error` 再按对应 close code 关闭，不得只关闭连接而不给出 `code`。

#### Scenario: 正常握手完成

- **WHEN** 已配对 Access 节点发起 `node.hello`（版本区间含 1、`requiredFeatures` 含 `node-link.core.v1`），并正确应答挑战
- **THEN** 连接依次收到 `node.challenge`/`node.ready`，进入业务阶段，`node.ready` 携带 `catalogRevision`、`limits` 与 `serverEpoch`

#### Scenario: 认证前发送业务消息

- **WHEN** 连接建立后第一条消息是 `command.submit` 而非 `node.hello`
- **THEN** Owner 以 4401 关闭连接，不产生任何命令副作用

### Requirement: 握手版本交集协商

Owner SHALL 在 `node.challenge` 中返回版本交集选择（v1 固定为 1），无法形成交集时返回 `link.error`（`nodelink.protocol.version_unsupported`）并以 4406 关闭。

#### Scenario: 版本区间无交集被拒绝

- **WHEN** Access 在 `node.hello` 中声明的版本区间与 v1 没有交集
- **THEN** Owner 返回 `link.error`（`nodelink.protocol.version_unsupported`）并以 4406 关闭，握手不进入业务阶段

### Requirement: feature 协商的封闭词表

feature 协商 SHALL 只使用 `compatibility/features/v1/features.json` 的封闭词表：`selectedFeatures` 必须是 Access 声明集合的子集；Access 的 `requiredFeatures`（至少含 `node-link.core.v1`）未被全部选择时 MUST 返回 `nodelink.protocol.feature_required`，`details.features` 给出排序去重后未被选择的 feature ID，握手不得进入业务阶段。

#### Scenario: 必需 feature 未满足

- **WHEN** Access 的 `requiredFeatures` 包含 Owner 不认识的 feature ID
- **THEN** Owner 返回 `link.error`（`nodelink.protocol.feature_required`，`details.features` 列出未满足项）并关闭连接，不进入业务阶段

### Requirement: 双向 challenge-response 与验签公钥来源

握手 SHALL 经 `identity-auth` 的 NodeLink 入口完成双向 challenge-response：Owner 签发连接挑战并签名（`node-link-challenge/v1`），Access 以 `node-link-proof/v1` 域签名应答；验签公钥 MUST 只来自持久化信任记录的当次快照，不得使用握手消息自报的公钥。每个新 WSS 连接 MUST 完整执行 challenge-response，不引入 bearer/session token。

#### Scenario: 未知节点不泄露存在性之外的能力

- **WHEN** 从未配对的节点发起握手
- **THEN** Owner 照常签发挑战（不用错误区分节点是否存在），proof 阶段失败并映射为 `nodelink.auth.node_unknown`，连接关闭

#### Scenario: 验签只采用信任记录快照的公钥

- **WHEN** 对端在 `node.proof` 中自报与持久化信任记录不同的公钥
- **THEN** Owner 只用持久化信任记录当次快照中的公钥验签，自报公钥不参与验证

### Requirement: proof 校验失败与凭据状态映射

proof 校验失败（签名、nonce、挑战一次性、绑定任一不成立）MUST 返回 `nodelink.auth.proof_invalid`；对端不在信任记录中 MUST 映射为 `nodelink.auth.node_unknown`，对端已撤销 MUST 映射为 `nodelink.auth.node_revoked` 并以 4410 关闭，不得降级为授权错误。

#### Scenario: proof 无效被拒绝

- **WHEN** 对端提交签名错误或 nonce 不匹配的 `node.proof`
- **THEN** Owner 返回 `link.error`（`nodelink.auth.proof_invalid`）并关闭连接，追加认证失败审计，信任记录不受影响

#### Scenario: 已撤销节点连接被拒绝

- **WHEN** 信任记录已撤销的节点完成签名有效的 proof
- **THEN** Owner 返回 `nodelink.auth.node_revoked` 并以 4410 关闭，连接不得进入业务阶段

### Requirement: 认证成功的收尾副作用

认证成功的收尾副作用（已批准配对转 `consumed`、`last_seen`、审计）MUST 随写集在同一事务提交。

#### Scenario: 认证收尾副作用同事务提交

- **WHEN** Access 认证成功且该节点存在已批准的配对
- **THEN** 已批准配对转 `consumed`、`last_seen` 更新与审计在同一事务写集内提交，不存在只提交其中一部分的状态

### Requirement: 新认证尝试限流

单 IP 新认证尝试 MUST 限流为 10 次/分钟。

#### Scenario: 单 IP 认证尝试限流

- **WHEN** 同一 IP 一分钟内发起超过 10 次新认证尝试
- **THEN** 超出部分的认证尝试被限流拒绝

### Requirement: `node.ready` 的 limits 下调语义

`node.ready.limits` SHALL 只把 `NODE_LINK_PROTOCOL.md` §2.5 表中标为可下调的值**调低**（`maxMessageBytes`、`catalogSnapshotBatchSize`、`resourceSnapshotBatchSize`、`maxInFlightCommands`、`maxPendingQueueBytes`、`maxPendingQueueMessages`、`heartbeatIntervalMs`），不得上调；未在 `node.ready` 中出现的 limits 使用 §2.5 默认值。握手超时 15 秒、心跳超时 90 秒、JSON 嵌套深度 64、单对象字段数 1024、单数组元素数 10000 等固定常量 MUST NOT 可通过任何配置或协商放宽。

#### Scenario: 下调生效

- **WHEN** 部署配置把 `node_link.max_in_flight_commands` 下调为 8
- **THEN** `node.ready.limits.maxInFlightCommands` 为 8，该连接第 9 个并发命令被按上限规则拒绝

#### Scenario: 固定常量不可协商

- **WHEN** 任何配置或握手载荷试图放宽握手超时、心跳超时或 JSON 嵌套深度
- **THEN** 这些常量保持 §2.5 的固定值，连接行为不因配置变化

### Requirement: connectionSequence 的严格递增与存在性

认证前消息（`node.hello`/`node.challenge`/`node.proof` 及握手阶段的 `link.error`）SHALL 省略 `connectionId`/`connectionSequence`，认证完成后两者 MUST 存在；两个方向各自从 `"1"` 开始严格加一，重复、回退、跳号或 `connectionId` 不匹配 MUST 返回 `nodelink.protocol.sequence_invalid`。

#### Scenario: 序号回退被拒绝

- **WHEN** 已认证连接在序号为 `"5"` 之后发送序号为 `"4"` 的消息
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，该消息的业务语义不生效

#### Scenario: 认证前消息省略序号字段

- **WHEN** 连接在认证完成前发送 `node.hello`/`node.proof`
- **THEN** 这些消息省略 `connectionId`/`connectionSequence`，不因缺少两者被判为 `nodelink.protocol.sequence_invalid`

### Requirement: 信封与消息 body 的 closed object 校验

控制信封与全部消息 body 是 closed object：未知字段或缺失必需字段 MUST 返回 `nodelink.protocol.schema_invalid`；未知 `type` 与 post_mvp 消息（`catalog.changed`、`resource.detach`、`node.rotate-key.request`/`result`、`link.backpressure`）MUST 返回 `nodelink.protocol.type_unsupported`，不得静默丢弃。

#### Scenario: post_mvp 消息显式拒绝

- **WHEN** 已认证连接发送 `catalog.changed` 或 `link.backpressure`
- **THEN** Owner 返回 `nodelink.protocol.type_unsupported`，连接保持可用，消息不被静默忽略

#### Scenario: 未知字段被拒绝

- **WHEN** 已认证连接的消息 body 携带 schema 未登记的字段
- **THEN** Owner 返回 `nodelink.protocol.schema_invalid`，消息不生效

### Requirement: `messageId` 重复与 schema 校验语料

`messageId` 重复不重放业务结果。消息 schema 校验 SHALL 消费 `schemas/node-link/v1/` 与 `fixtures/node-link/v1/manifest.json` 的正反用例。

#### Scenario: 重复 messageId 不重放业务结果

- **WHEN** 已认证连接重复使用同一 `messageId` 发送消息
- **THEN** 该消息不产生第二次业务结果

### Requirement: catalog 批次切分与内存投影

批次内条目顺序 MUST 稳定，批次大小不超过协商的 `catalogSnapshotBatchSize`（默认 500）。catalog 是绑定当前连接的内存投影，Owner 不得为 Access 额外持久化副本。

#### Scenario: 超大批次稳定切分

- **WHEN** 可见 Export 条目数超过协商批次大小
- **THEN** snapshot 分成多条消息，批次内顺序稳定，接收方按序拼接后得到完整视图

### Requirement: attachment 申请与 generation 隔离

`resource.attach` SHALL 只为该连接可见且未撤销的 Export 内的会话签发新的 `attachmentId`/`attachmentGeneration`，`resource.attached` 返回两者与 `sessionMeta`；未知会话或不属于可见 Export 的组合 MUST 返回 `nodelink.export.not_found`/`nodelink.export.not_granted`。新 generation 生效后，旧 generation 的 frame（command 或事件投递）MUST 被拒绝为 `nodelink.resource.attach_generation_stale`，不得仅凭相同 `sessionId` 投递到新连接；Access 必须重新 attach 后再恢复订阅与命令。

#### Scenario: 重新 attach 使旧 generation 失效

- **WHEN** 某会话先 attach 得到 generation `"3"`，之后再次 attach 得到 generation `"4"`，随后旧连接延迟到达携带 generation `"3"` 的 `command.submit`
- **THEN** Owner 返回 `nodelink.resource.attach_generation_stale`，命令不派发，Access 重新 attach 后重试才可接受

#### Scenario: 未导出会话不可 attach

- **WHEN** Access 对不在其可见 Export 内的 `sessionId` 发起 `resource.attach`
- **THEN** Owner 返回 `nodelink.export.not_found` 或 `nodelink.export.not_granted`，不签发 attachment

### Requirement: 快照流程与元数据范围

`resource.subscribe` 的 `cursor` 为 `null` 时 SHALL 走快照流程（`snapshot_begin` → `snapshot_chunk*` → `snapshot_end`）：快照只承载 `session_meta` 与 `pending_interactions` 两类元数据，会话正文（消息、turn、diff、终端输出、附件、ACP raw）MUST NOT 出现在快照中。

#### Scenario: 首次订阅走快照

- **WHEN** Access 对某会话完成 attach 后以 `cursor = null` 订阅
- **THEN** Owner 依次发送 `snapshot_begin`、若干 `snapshot_chunk` 与带 `snapshotDigest` 的 `snapshot_end`，快照中不含任何会话正文，随后事件从快照结束点 cursor 继续

### Requirement: `snapshotDigest` 计算与快照批次上限

`snapshotDigest` MUST 按原始 chunk 字节规则计算（逐 chunk SHA-256 后按 `chunkIndex` 连接再求 SHA-256），不得通过重新序列化 JSON 得出。单个资源快照并发 MUST 为 1，快照批次条数不超过协商的 `resourceSnapshotBatchSize`。

#### Scenario: 摘要按原始 chunk 字节计算

- **WHEN** 快照由多个 chunk 组成并被发送
- **THEN** `snapshotDigest` 由逐 chunk 的 SHA-256 按 `chunkIndex` 连接后再求 SHA-256 得出，与重新序列化 JSON 的结果无关

### Requirement: origin cursor 增量重放

`cursor` 非空时 SHALL 从该 origin cursor 增量重放；`cursor.originEpoch` 与该会话的 origin epoch 不一致时 MUST 返回 `nodelink.protocol.sequence_invalid`。

#### Scenario: epoch 不一致被拒绝

- **WHEN** Access 以属于该会话旧 epoch 的 cursor 订阅
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，不从错误位置重放

### Requirement: `resource.event` 的持久化顺序

Owner SHALL 只在事件与命令终态持久化提交成功后才向 Node Link 连接发布 `resource.event`。

#### Scenario: 先持久化后发布

- **WHEN** Agent 产生一个会话事件，Access 连接已订阅该会话
- **THEN** 事件先提交到 Owner 的持久化事件日志，随后才作为 `resource.event` 发出；在提交失败的路径上连接收不到该事件

### Requirement: `resource.event` 的字段与 view 合同

每条事件 MUST 携带完整 origin 三元组（`originEventId`/`originEpoch`/`originSequence`）、`sessionRef`、`eventType`、`payloadDigest` 与 `createdAt`，`payload` 内 `view` 与 `acp` 至少其一。`payload.view` 的字段集合 SHALL 遵守 `NODE_LINK_PROTOCOL.md` §11.4 的共享合同（与 Sync 同一登记表）；未登记 `eventType` 或无法形成 view 时 MUST 保留 `payload.acp` 的 ACP raw document 逐字节保真，不得丢弃或文本化。`resource.event.sessionRef` MUST 指向当前连接上 attachment 所属会话。

#### Scenario: 未登记事件类型保留 raw

- **WHEN** Owner 产生一个 eventType 未在登记表内的事件
- **THEN** `resource.event` 保留 `payload.acp` 的原始字节，不静默丢弃、不降级为普通文本

### Requirement: `payloadDigest` 与超大内嵌降级

`payloadDigest` SHALL 为 `base64url(SHA-256(ACPR-CJ1(payload)))`；单个内嵌 diff/document 超过 256 KiB 时 MUST 使用 `rawAcp.rawUnavailable`（`reason = "size_limit"`），不得截断后伪装完整。

#### Scenario: 超大内嵌内容显式降级

- **WHEN** 事件内嵌的 diff 超过 256 KiB
- **THEN** `payload.acp` 使用 `rawAcp.rawUnavailable`（`reason = "size_limit"`），事件照常投递，不伪装完整内容

#### Scenario: payloadDigest 使用 ACPR-CJ1 计算

- **WHEN** Owner 发布一条携带 `payload` 的 `resource.event`
- **THEN** `payloadDigest` 为 `base64url(SHA-256(ACPR-CJ1(payload)))`

### Requirement: `resource.ack` 的单调性与归属

`resource.ack` SHALL 表达该会话的累计 ACK：其 `cursor` MUST 单调不减，回退 MUST 返回 `nodelink.protocol.sequence_invalid`；`sessionRef` MUST 属于当前连接上 attachment 所属会话，`cursor.originEpoch` 与该会话 origin epoch 不一致时同样返回 `sequence_invalid`。ACK 只推进 Access 侧恢复语义，MUST NOT 触发 Owner 删除仍在保留窗口内的事件。

#### Scenario: ACK 回退被拒绝

- **WHEN** 连接先 ACK 到 originSequence `"40"`，随后发送 cursor 为 `"35"` 的 `resource.ack`
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，已确认的 ACK 水位不回退

### Requirement: 命令白名单与授权交集

`command.submit` 的 `command` SHALL 限于 `compatibility/commands/v1/commands.json` 的 13 个命令名；有效权限 MUST 是 Export grant、该 Access 信任记录 grant 与实际 capability 的交集，越权命令 MUST 以 `command.rejected`（`nodelink.export.not_granted`）拒绝且无副作用。

#### Scenario: 越权命令被拒绝

- **WHEN** Access 的 Export 只含 `grant.observe`，却提交 `session.prompt`
- **THEN** Owner 以 `command.rejected`（`nodelink.export.not_granted`）拒绝，会话无变化，审计记录该次拒绝

### Requirement: 授权判定先于本机读取

`session.resume` 与 `session.create` 一样按 `grant.remote-work` 判定授权，不新增授权维度；授权判定 MUST 发生在读取该会话行与触碰文件系统之前，未授权时 MUST NOT 因会话是否存在而产生可区分的响应。

#### Scenario: 越权恢复被拒绝且先于本机读取

- **WHEN** 持 `grant.observe` 的 Access 提交 `session.resume`
- **THEN** Owner 以 `command.rejected`（`nodelink.export.not_granted`）拒绝，不读取该会话行、不触碰文件系统、不启动 Agent 进程，且响应不因会话是否存在而不同

### Requirement: mutation 幂等键与冲突

可重试 mutation 的幂等键 MUST 至少含 `(ownerNodeId, accessNodeId, requestId)`：同一键重复提交 MUST 返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后 `payload` 语义不同 MUST 返回 `nodelink.command.idempotency_conflict`。

#### Scenario: 相同 requestId 重试不重复派发

- **WHEN** Access 以相同 `(requestId, command, payload)` 重复提交同一 mutation
- **THEN** Owner 返回首次的 accepted/terminal 结果，副作用只发生一次

#### Scenario: 同键不同语义被拒绝

- **WHEN** Access 以相同 `requestId` 但不同 `payload` 提交命令
- **THEN** Owner 返回 `nodelink.command.idempotency_conflict`，不执行第二次副作用

### Requirement: 命令回复与终态契约

`command.accepted` 的 `acceptedAt` 永远非 `null`；`command.rejected` 不携带 `acceptedAt`；`command.terminal` MUST 携带提交时的 `command` 名，`status = "completed"` 时 `terminal.result` MUST 存在且为 object（`session.create` 必须是 `SessionCreateResult`；`session.resume` 必须是 `SessionResumeResult`；其余命令可为空对象 `{}`——§12.5 的「非空」原意是「非 null」，与该节末句的 session.create 动机一致），其余 status MUST 给出 `error`；无法确认副作用时 MUST 写入 `uncertain` 终态。能证明未产生副作用（进程未能启动、本机解析失败，或 Agent 明确返回错误）时 MUST 写入 `failed` 终态并给出结构化错误，MUST NOT 用 `uncertain` 代替。

#### Scenario: 崩溃窗口进入 uncertain

- **WHEN** 命令已 accepted 但 Owner 在确认副作用前发生崩溃窗口，恢复后 Access 以 `command.status` 重查
- **THEN** Owner 返回 `status = "uncertain"` 的 `command.terminal`，不猜测成功或失败

#### Scenario: 可证明未产生副作用时进入 failed

- **WHEN** 一条已接受的 `session.create` 因 Agent 进程未能启动、或 Agent 明确返回错误而未能建立会话
- **THEN** Owner 以 `status = "failed"` 的 `command.terminal` 终结该命令并给出结构化错误，不把可证明的失败报成 `uncertain`

### Requirement: `command.status` 查询与落盘前失败的幂等语义

`command.status` 的两种等价形式（独立消息或作为 `command.submit` 的 `command`）MUST 产生同形回复：已终结 mutation → `command.terminal`（`terminalEventId` 非 `null`）；未终结 mutation → 同形 `command.accepted`；查询命令的 requestId 不落持久记录，重查 MUST 返回 `nodelink.command.not_found`。幂等行落盘前失败（如存储写失败等瞬态原因）的 `session.create` 不作为持久首次结果：同 `requestId` 重查 MUST 返回 `nodelink.command.not_found`，同键重试可得到不同结果（只有写入持久记录后的重复提交才保证返回首次结果）。

#### Scenario: command.status 同形回复

- **WHEN** Access 以 `command.status`（独立消息形式）重查一个已终结的 mutation
- **THEN** Owner 回复与提交时同形的 `command.terminal`，其 `terminalEventId` 非 `null`

#### Scenario: 未登记 requestId 重查为 not_found

- **WHEN** Access 以 `command.status` 重查一个从未提交过的 `requestId`
- **THEN** Owner 返回 `nodelink.command.not_found`

### Requirement: 副作用判据以资源行为准

判定「该命令有没有产生业务副作用」MUST 以资源行是否已提交为准（对 `session.create` 即会话行），MUST NOT 以「该 `requestId` 是否存在持久记录」代替——受理记录在受理阶段就已经存在，用它当替身会把可证明的失败误报成 `uncertain`。

#### Scenario: 受理记录的存在不构成副作用证据

- **WHEN** 一条已受理的 `session.create` 在后端阶段失败，Access 以同一 `requestId` 重查
- **THEN** Owner 按失败是否可证明返回 `failed` 或 `uncertain` 的终态，MUST NOT 因为"受理记录存在"就一律回 `uncertain`

### Requirement: 终态推送、会话绑定字段与命令限流

被接受 mutation 的终态 MUST 由提交方连接上有所有者的 watcher（连接断开即取消）推送 `command.terminal`。session-scoped 命令 MUST 携带当前 `attachmentId`/`attachmentGeneration`；`session.mode.set`/`session.config.set` 的 `expectedVersion` MUST 存在。单连接 in-flight 命令不超过协商上限，命令速率超过 120 次/分钟时返回 `nodelink.resource.rate_limited`（`details.retryAfterMs` 存在时 Access 必须遵守），连续超限以 4429 关闭。

#### Scenario: 终态由 watcher 推送

- **WHEN** 一个已被 accepted 的 mutation 产生终态，且提交方连接仍在线
- **THEN** 该连接上属于它的 watcher 推送 `command.terminal`，连接断开时该 watcher 被取消

#### Scenario: 命令限流

- **WHEN** 单连接一分钟内命令数超过 120
- **THEN** 超限命令收到 `link.error`（`nodelink.resource.rate_limited`，`retryable = true`）；持续超限时连接以 4429 关闭

### Requirement: `session.create` 的 payload 白名单

`session.create` SHALL 要求 `grant.remote-work`；其 `payload` 只允许 `agentId`/`exportId`/`directoryId`/`templateParams` 四个键，出现 `cwd`、`mcpServers`、任何绝对路径或凭据字段时 MUST 以 `command.rejected`（`nodelink.command.unsupported_field`）拒绝，`details.field` 给出被拒字段名，且不得创建会话或部分应用参数。

#### Scenario: 禁带字段被拒绝且不创建会话

- **WHEN** `session.create` 的 `payload` 携带 `cwd` 或 `mcpServers`
- **THEN** Owner 返回 `command.rejected`（`nodelink.command.unsupported_field`，`details.field` 指明字段），Owner 侧不创建任何会话

#### Scenario: 任何路径字段都被拒绝

- **WHEN** `session.create` 的 `payload` 携带任意绝对路径字段（含自造的目录路径）
- **THEN** 按同一错误码拒绝，且不触碰文件系统

### Requirement: `session.create` 的结果契约

`session.create` 的 `sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` MUST 为 `null`；Owner 先回 `command.accepted`（`result = null`），创建完成后发 `command.terminal`，`completed` 时 `terminal.result` MUST 是含 `remoteSessionRef` 与 `sessionMeta` 的 `SessionCreateResult`，`sessionId` 由 Owner 生成并写入自身事件日志；`uncertain` 时表示无法确认会话是否已创建。

#### Scenario: 正常创建并回传复合引用

- **WHEN** 持 `grant.remote-work` 的 Access 提交 `agentId`/`exportId`/`workspaceAlias` 都合法的 `session.create`（`templateParams` 省略）
- **THEN** Owner 先回 `accepted`（`result = null`），随后 `terminal.result` 给出含 `ownerNodeId`/`exportId`/Owner 生成的 `sessionId` 的 `SessionCreateResult`，Access 可凭其发起 `resource.attach`

### Requirement: 撤销的即时传播

本地 `export.revoke` 提交成功后 SHALL 立即：向持有该 Export 的活跃连接推送 `export.revoked`，拒绝该 Export 的新命令与新订阅，旧连接不得继续取资源。本地 `node.revoke` 提交成功后 SHALL 立即：向该节点推送 `node.trust.revoked` 并以 4410 关闭其连接；该节点后续连接按已撤销拒绝。撤销判定 SHALL 以持久化记录为准，进程内缓存不得让已撤销的 Export/节点继续可用；关闭/推送失败不得回滚已提交的撤销。

#### Scenario: Export 撤销即断资源

- **WHEN** Access 连接正在订阅某 Export 的会话，Owner 本地执行 `export.revoke` 并提交成功
- **THEN** 该连接收到 `export.revoked`，此后对该 Export 的 `resource.attach`/`command.submit` 被拒绝，内存中的订阅状态被清除

#### Scenario: 节点撤销即关连接

- **WHEN** Owner 本地执行 `node.revoke` 并提交成功
- **THEN** 该 Access 的连接收到 `node.trust.revoked` 并以 4410 关闭，重连握手在凭据状态检查处被拒绝

### Requirement: 心跳、超时与慢连接隔离

Owner SHALL 按协商的 `heartbeatIntervalMs`（默认 30 秒）发送 `link.ping`，`link.pong` MUST 原样回填 nonce；连续 90 秒无任何入站消息或 `link.pong` 时 MUST 以 4408 关闭连接。待发送队列 SHALL 有界（默认 8 MiB 或 2000 条，可经 limits 下调）：达到高水位后先停读新的快照批次；**持续**过慢时断开连接，由 origin cursor 重放补回，MUST NOT 丢弃 Owner 事件或阻塞其他连接，v1 不发送 `link.backpressure`。

「持续过慢」的判定 SHALL 可恢复：队列被消费到低水位以下时饱和状态 MUST 被清除。**饱和 MUST NOT 短路心跳判定**：饱和段未超宽限时每周期照常判定；`link.ping` 因队列顶满被拒 SHALL 按一级处置推迟并在下一拍重试，MUST NOT 结束连接，也 MUST NOT 因此走进 90 秒静默关闭。高/低水位 SHALL 有明确常量并登记（低水位是迟滞阈值，不得等于高水位）。

#### Scenario: 心跳超时关闭

- **WHEN** 已认证连接 90 秒内没有任何入站消息与 `link.pong`
- **THEN** Owner 以 4408 关闭连接，信任记录与持久化状态不受影响

#### Scenario: 慢连接不影响其他连接

- **WHEN** 某 Access 连接消费缓慢、待发送队列达到高水位
- **THEN** Owner 先暂停向该连接读取新快照批次，持续过慢时断开该连接；同一 Owner 上其他 Access 连接的事件投递不受影响，断开的连接可凭 cursor 重放补齐

#### Scenario: 饱和后恢复的连接不被断开

- **WHEN** 某连接的待发送队列曾达到高水位，随后被消费到低水位以下
- **THEN** 该连接 MUST NOT 因那次饱和而被断开，且新快照批次与事件批次的读取恢复正常

#### Scenario: 饱和期间心跳判定不被短路

- **WHEN** 某连接处于饱和状态，且其饱和段尚未超过慢消费者宽限
- **THEN** 每个心跳周期 SHALL 照常判定是否该发 `link.ping`；`link.ping` 若因队列顶满而被拒，SHALL 按一级处置推迟并在下一拍重试，MUST NOT 结束连接，也 MUST NOT 让连接因「心跳未发出」而走进 90 秒静默关闭路径；是否断开只由慢消费者看门狗按宽限判定

#### Scenario: 真正持续过慢仍被断开

- **WHEN** 某连接在整个观察窗口内始终未把队列消费到低水位以下
- **THEN** Owner 断开该连接，同一 Owner 上其他连接不受影响，断开的连接可凭 origin cursor 重放补齐

### Requirement: 审计与日志边界

握手失败、授权拒绝、命令幂等冲突、撤销与连接关闭 SHALL 按 `SECURITY_DESIGN.md` §14.2 落审计事件；Owner 侧命令审计行只记 `viaNodeId`，`localPrincipalRef` 为 `null`（节点级信任模型，§8.3）。日志与协议错误 MUST NOT 包含私钥、`pairingSecret`、签名/HMAC 输入以外的秘密材料、完整 prompt 或凭据值；close reason 不含敏感信息且不替代结构化错误。

#### Scenario: 拒绝留痕且不含秘密

- **WHEN** 发生认证失败、越权命令与节点撤销各一次
- **THEN** 三类事件都有对应审计记录，日志与错误消息中不出现私钥、`pairingSecret` 或凭据值

### Requirement: `session.resume` 的 payload 与字段约束

`session.resume` SHALL 要求 `grant.remote-work`；其 `payload` MUST 是空对象 `{}`，出现任何键时 MUST 以 `command.rejected`（`nodelink.command.unsupported_field`）拒绝，`details.field` 给出被拒字段名，且不得启动 Agent 进程或部分应用参数（恢复所需的 Agent 标识、ACP 会话标识与创建时目录一律取自 Owner 自身的持久化记录，不接受客户端提供）。其 `expectedVersion` MUST 为 `null`；`sessionRef`/`attachmentId`/`attachmentGeneration` MUST 为指向该 Export 可见会话的非 `null` 值。

#### Scenario: 携带字段被拒绝且不启动进程

- **WHEN** `session.resume` 的 `payload` 携带任意键（例如 `cwd` 或 `agentId`）
- **THEN** Owner 返回 `command.rejected`（`nodelink.command.unsupported_field`，`details.field` 指明字段），不启动 Agent 进程、不改会话状态

### Requirement: `session.resume` 的结果契约

`session.resume` 是带副作用的 mutation：Owner 先回 `command.accepted`（`result = null`），恢复完成后发 `command.terminal`；`completed` 时 `terminal.result` MUST 是 `SessionResumeResult`（含 `remoteSessionRef` 与 `sessionMeta`），`failed` 时 MUST 给出错误；目标 Agent 未宣告 `sessionCapabilities.resume` 时 MUST 以 `nodelink.command.unsupported` 终态失败，MUST NOT 静默降级为新建会话。

#### Scenario: 正常恢复并回传会话引用

- **WHEN** 持 `grant.remote-work` 的 Access 对一条已导出、归属于该 Export 且带持久化恢复数据的会话提交 `session.resume`（payload 为空对象）
- **THEN** Owner 先回 `accepted`（`result = null`），随后 `terminal.result` 给出含 `remoteSessionRef` 与 `sessionMeta` 的 `SessionResumeResult`，该会话随后可接受 `session.prompt`

#### Scenario: Agent 不支持恢复时终态失败

- **WHEN** 目标 Agent 未宣告 `sessionCapabilities.resume`
- **THEN** `session.resume` 以 `command.terminal`（`status = "failed"` 或 `uncertain`）与 `nodelink.command.unsupported` 结束，不创建新会话、不报告成功

### Requirement: 每次消息级拒绝都推进入站期望序号

入站期望序号描述**对端已发出的消息**，本机是否接受它不影响它已被消耗。因此对**形状类**拒绝 —— 未登记的信封字段、缺 `messageId`、body 不是 JSON、**整帧超过 §2.5 的三个固定 JSON 结构上限（嵌套深度 64、单对象字段数 1024、单数组元素数 10000）** —— 与连接字段禁用类拒绝，服务端 MUST 照常推进期望序号；MUST NOT 让后续序号正常递增的合法消息被判 `sequence_invalid`，也 MUST NOT 以「静默丢弃后续消息直到静默超时」代替推进。

只有**读不出「本连接的下一个序号」**的帧才不推进：文本不是合法 JSON、连接字段不成对或缺席、`connectionId` 属于别的连接、序号非规范十进制 / 超 `2^63-1` / 不等于期望值，**以及嵌套深度超过解析器递归上限**（按文本重解析读不出序号）。最后一种是**已知残留**：实现 MUST 在其文档里如实登记「结构类超限的退化只被收窄、未被完全消除」，MUST NOT 声称已完全消除。

#### Scenario: 形状错误之后的一条合法消息

- **WHEN** 一条形状不合法的帧被拒绝（未登记信封字段 / 缺 `messageId` / body 非 JSON / 整帧超过 §2.5 的固定结构上限之一）
- **THEN** 其后一条序号正常递增的合法消息 MUST NOT 被判 `sequence_invalid`，并能得到正常处理

#### Scenario: 未知 type 与形状错误同口径

- **WHEN** 先后收到「词表外 `type`」与「形状不合法」两类被拒帧
- **THEN** 两类都必须推进期望序号，MUST NOT 出现「一类推进、另一类不推进」的差异

### Requirement: 写路径有期限且写卡住时判据仍被检查

连接会话 SHALL 为出站写设置期限，使「对端不读、socket 缓冲被填满」不会让会话任务无限期停在写路径上；在写未被推进的期间，心跳、静默超时与慢消费者看门狗 SHALL 仍按各自窗口被检查并生效。写超时的处置 SHALL 与慢消费者的既有分级一致（先按判据处置，最终断开），MUST NOT 静默丢弃事件或让连接无限期存活。

#### Scenario: 对端不读时连接被有界收尾

- **WHEN** 对端停止读取、socket 缓冲被写满（写路径挂住）
- **THEN** 连接 SHALL 在有界时间内被收尾（断开或按慢消费者分级处置），MUST NOT 一直保持在「已连接」状态

#### Scenario: 写卡住期间判据仍被检查

- **WHEN** 会话的写路径长时间未被推进
- **THEN** 心跳、90 秒静默判定与慢消费者看门狗 SHALL 仍按窗口被检查（例如日志里出现对应的超时/看门狗记录），而不是随写一起停摆

### Requirement: `session.create` 的目录标识解析与零参数 template

`agentId`/`exportId` 必须同时存在于该 Access 可见 Export 与授权范围内，否则返回 `nodelink.export.not_granted`（可带 `details.parameter`）；`directoryId` 必须同时存在于该 Export 的目录集合与本机目录表内，否则同样返回 `nodelink.export.not_granted`。首切片 template MUST 零参数：`templateParams` 必须省略或为空对象，声明了参数的 Export/template 必须明确拒绝而非静默忽略。

#### Scenario: 未知 directoryId 被拒绝

- **WHEN** `session.create` 引用不在该 Export 目录集合内、或不在本机目录表内的 `directoryId`
- **THEN** Owner 返回 `nodelink.export.not_granted`（`details.parameter` 指明被拒参数），不创建会话、不触碰文件系统

#### Scenario: 非空 templateParams 被明确拒绝

- **WHEN** `session.create` 的 `templateParams` 非空或引用的 Export/template 声明了参数
- **THEN** Owner 明确拒绝该请求，不静默忽略 `templateParams`

### Requirement: 全量 catalog 快照（无 Export 维度）

`catalog.subscribe` 的 `knownRevision` 为 `null` 时表示首次获取；非空时 Owner SHALL 仍返回当前完整快照。`catalog.snapshot` MUST 从本机目录、Agent 配置与会话记录直接投影，不得按 Export 或授权集合过滤：所有已配对且未撤销的节点看到同一份资源清单。`local.*` 能力仍不可远程调用。

#### Scenario: 已配对节点看到全部资源

- **WHEN** 一个已配对且未撤销的 Access 节点完成握手并订阅 catalog
- **THEN** `catalog.snapshot` 包含本机全部目录、Agent 与会话，不受任何 Export 或 grant 影响

#### Scenario: 撤销节点即时失去访问

- **WHEN** 本机撤销该节点后它尝试订阅 catalog
- **THEN** 握手或请求被拒绝，不再返回资源
