# node-link-owner-server Specification

## Purpose

定义 Owner 侧 Node Link WSS 服务端（`server::node_link`）的可观察行为：四步握手与 feature 协商、信封与双向 connectionSequence、按信任记录过滤的 Export catalog 投影、attachment generation 与快照/事件/ACK、命令的授权/幂等/终态与 `session.create` 硬约束、心跳与慢连接隔离、撤销即时传播与审计，使已授权 Access 节点能安全访问导出资源而未授权方无处入手。

## ADDED Requirements

### Requirement: 握手准入与版本/feature 协商

连接建立后的第一条消息 SHALL 是 `node.hello`，认证完成前收到任何业务消息 MUST 以 4401 关闭；整个握手（`node.hello` → `node.ready`）MUST 在 15 秒内完成，超时以 4408 关闭。Owner SHALL 在 `node.challenge` 中返回版本交集选择（v1 固定为 1），无法形成交集时返回 `link.error`（`nodelink.protocol.version_unsupported`）并以 4406 关闭。feature 协商 SHALL 只使用 `compatibility/features/v1/features.json` 的封闭词表：`selectedFeatures` 必须是 Access 声明集合的子集；Access 的 `requiredFeatures`（至少含 `node-link.core.v1`）未被全部选择时 MUST 返回 `nodelink.protocol.feature_required`，`details.features` 给出排序去重后未被选择的 feature ID，握手不得进入业务阶段。认证失败一律先返回 `link.error` 再按对应 close code 关闭，不得只关闭连接而不给出 `code`。

#### Scenario: 正常握手完成

- **WHEN** 已配对 Access 节点发起 `node.hello`（版本区间含 1、`requiredFeatures` 含 `node-link.core.v1`），并正确应答挑战
- **THEN** 连接依次收到 `node.challenge`/`node.ready`，进入业务阶段，`node.ready` 携带 `catalogRevision`、`limits` 与 `serverEpoch`

#### Scenario: 认证前发送业务消息

- **WHEN** 连接建立后第一条消息是 `command.submit` 而非 `node.hello`
- **THEN** Owner 以 4401 关闭连接，不产生任何命令副作用

#### Scenario: 必需 feature 未满足

- **WHEN** Access 的 `requiredFeatures` 包含 Owner 不认识的 feature ID
- **THEN** Owner 返回 `link.error`（`nodelink.protocol.feature_required`，`details.features` 列出未满足项）并关闭连接，不进入业务阶段

### Requirement: 节点双向认证与凭据状态

握手 SHALL 经 `identity-auth` 的 NodeLink 入口完成双向 challenge-response：Owner 签发连接挑战并签名（`node-link-challenge/v1`），Access 以 `node-link-proof/v1` 域签名应答；验签公钥 MUST 只来自持久化信任记录的当次快照，不得使用握手消息自报的公钥。proof 校验失败（签名、nonce、挑战一次性、绑定任一不成立）MUST 返回 `nodelink.auth.proof_invalid`；对端不在信任记录中 MUST 映射为 `nodelink.auth.node_unknown`，对端已撤销 MUST 映射为 `nodelink.auth.node_revoked` 并以 4410 关闭，不得降级为授权错误。每个新 WSS 连接 MUST 完整执行 challenge-response，不引入 bearer/session token；认证成功的收尾副作用（已批准配对转 `consumed`、`last_seen`、审计）MUST 随写集在同一事务提交。单 IP 新认证尝试 MUST 限流为 10 次/分钟。

#### Scenario: proof 无效被拒绝

- **WHEN** 对端提交签名错误或 nonce 不匹配的 `node.proof`
- **THEN** Owner 返回 `link.error`（`nodelink.auth.proof_invalid`）并关闭连接，追加认证失败审计，信任记录不受影响

#### Scenario: 已撤销节点连接被拒绝

- **WHEN** 信任记录已撤销的节点完成签名有效的 proof
- **THEN** Owner 返回 `nodelink.auth.node_revoked` 并以 4410 关闭，连接不得进入业务阶段

#### Scenario: 未知节点不泄露存在性之外的能力

- **WHEN** 从未配对的节点发起握手
- **THEN** Owner 照常签发挑战（不用错误区分节点是否存在），proof 阶段失败并映射为 `nodelink.auth.node_unknown`，连接关闭

### Requirement: `node.ready` 的 limits 下调语义

`node.ready.limits` SHALL 只把 `NODE_LINK_PROTOCOL.md` §2.5 表中标为可下调的值**调低**（`maxMessageBytes`、`catalogSnapshotBatchSize`、`resourceSnapshotBatchSize`、`maxInFlightCommands`、`maxPendingQueueBytes`、`maxPendingQueueMessages`、`heartbeatIntervalMs`），不得上调；未在 `node.ready` 中出现的 limits 使用 §2.5 默认值。握手超时 15 秒、心跳超时 90 秒、JSON 嵌套深度 64、单对象字段数 1024、单数组元素数 10000 等固定常量 MUST NOT 可通过任何配置或协商放宽。

#### Scenario: 下调生效

- **WHEN** 部署配置把 `node_link.max_in_flight_commands` 下调为 8
- **THEN** `node.ready.limits.maxInFlightCommands` 为 8，该连接第 9 个并发命令被按上限规则拒绝

#### Scenario: 固定常量不可协商

- **WHEN** 任何配置或握手载荷试图放宽握手超时、心跳超时或 JSON 嵌套深度
- **THEN** 这些常量保持 §2.5 的固定值，连接行为不因配置变化

### Requirement: 信封与 connectionSequence 校验

认证前消息（`node.hello`/`node.challenge`/`node.proof` 及握手阶段的 `link.error`）SHALL 省略 `connectionId`/`connectionSequence`，认证完成后两者 MUST 存在；两个方向各自从 `"1"` 开始严格加一，重复、回退、跳号或 `connectionId` 不匹配 MUST 返回 `nodelink.protocol.sequence_invalid`。控制信封与全部消息 body 是 closed object：未知字段或缺失必需字段 MUST 返回 `nodelink.protocol.schema_invalid`；未知 `type` 与 post_mvp 消息（`catalog.changed`、`resource.detach`、`node.rotate-key.request`/`result`、`link.backpressure`）MUST 返回 `nodelink.protocol.type_unsupported`，不得静默丢弃；`messageId` 重复不重放业务结果。消息 schema 校验 SHALL 消费 `schemas/node-link/v1/` 与 `fixtures/node-link/v1/manifest.json` 的正反用例。

#### Scenario: 序号回退被拒绝

- **WHEN** 已认证连接在序号为 `"5"` 之后发送序号为 `"4"` 的消息
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，该消息的业务语义不生效

#### Scenario: post_mvp 消息显式拒绝

- **WHEN** 已认证连接发送 `catalog.changed` 或 `link.backpressure`
- **THEN** Owner 返回 `nodelink.protocol.type_unsupported`，连接保持可用，消息不被静默忽略

#### Scenario: 未知字段被拒绝

- **WHEN** 已认证连接的消息 body 携带 schema 未登记的字段
- **THEN** Owner 返回 `nodelink.protocol.schema_invalid`，消息不生效

### Requirement: Export 过滤的 catalog 投影

`catalog.subscribe` 的 `knownRevision` 为 `null` 时表示首次获取；非空时 Owner SHALL 仍返回当前完整快照（`catalog.changed` 增量属 post_mvp，本切片不实现）。`catalog.snapshot` SHALL 从 Owner 自己的持久化 Export 记录投影，只包含该 Access **可见**的 Export——首阶段的可见性规则（2026-09-26 用户裁决，设计 D12/D13 同节注记）：未撤销且 `export.scopes ∩ 该节点信任记录的 grants ≠ ∅`（`NODE_LINK_PROTOCOL.md` §8.2 的 `exportIds` 字段推后，属后续阶段待办）。批次内条目顺序 MUST 稳定，批次大小不超过协商的 `catalogSnapshotBatchSize`（默认 500）。Export 条目 SHALL 携带 §12.3 登记的全部字段（含 `defaultWorkspaceAlias`、`templates`、`scopes`、`capabilityCeilingRef`、`cachePolicy = "no-content-cache"`、`revoked` 标记）；首切片发布的 template MUST 是零参数（`params = []`）。catalog 是绑定当前连接的内存投影，Owner 不得为 Access 额外持久化副本。

#### Scenario: 按信任记录过滤

- **WHEN** Owner 有两个 Export（E1、E2），E2 的 scopes 与该 Access 信任记录的 grants 不相交（E1 相交），完成握手后订阅 catalog
- **THEN** `catalog.snapshot` 只含 E1 的条目，E2 的任何字段不出现在响应中

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

### Requirement: 快照与 origin cursor 增量重放

`resource.subscribe` 的 `cursor` 为 `null` 时 SHALL 走快照流程（`snapshot_begin` → `snapshot_chunk*` → `snapshot_end`）：快照只承载 `session_meta` 与 `pending_interactions` 两类元数据，会话正文（消息、turn、diff、终端输出、附件、ACP raw）MUST NOT 出现在快照中；`snapshotDigest` MUST 按原始 chunk 字节规则计算（逐 chunk SHA-256 后按 `chunkIndex` 连接再求 SHA-256），不得通过重新序列化 JSON 得出。`cursor` 非空时 SHALL 从该 origin cursor 增量重放；`cursor.originEpoch` 与该会话的 origin epoch 不一致时 MUST 返回 `nodelink.protocol.sequence_invalid`。单个资源快照并发 MUST 为 1，快照批次条数不超过协商的 `resourceSnapshotBatchSize`。

#### Scenario: 首次订阅走快照

- **WHEN** Access 对某会话完成 attach 后以 `cursor = null` 订阅
- **THEN** Owner 依次发送 `snapshot_begin`、若干 `snapshot_chunk` 与带 `snapshotDigest` 的 `snapshot_end`，快照中不含任何会话正文，随后事件从快照结束点 cursor 继续

#### Scenario: epoch 不一致被拒绝

- **WHEN** Access 以属于该会话旧 epoch 的 cursor 订阅
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，不从错误位置重放

### Requirement: `resource.event` 的持久化顺序与保真

Owner SHALL 只在事件与命令终态持久化提交成功后才向 Node Link 连接发布 `resource.event`；每条事件 MUST 携带完整 origin 三元组（`originEventId`/`originEpoch`/`originSequence`）、`sessionRef`、`eventType`、`payloadDigest` 与 `createdAt`，`payload` 内 `view` 与 `acp` 至少其一。`payload.view` 的字段集合 SHALL 遵守 `NODE_LINK_PROTOCOL.md` §11.4 的共享合同（与 Sync 同一登记表）；未登记 `eventType` 或无法形成 view 时 MUST 保留 `payload.acp` 的 ACP raw document 逐字节保真，不得丢弃或文本化。`payloadDigest` SHALL 为 `base64url(SHA-256(ACPR-CJ1(payload)))`；单个内嵌 diff/document 超过 256 KiB 时 MUST 使用 `rawAcp.rawUnavailable`（`reason = "size_limit"`），不得截断后伪装完整。`resource.event.sessionRef` MUST 指向当前连接上 attachment 所属会话。

#### Scenario: 先持久化后发布

- **WHEN** Agent 产生一个会话事件，Access 连接已订阅该会话
- **THEN** 事件先提交到 Owner 的持久化事件日志，随后才作为 `resource.event` 发出；在提交失败的路径上连接收不到该事件

#### Scenario: 未登记事件类型保留 raw

- **WHEN** Owner 产生一个 eventType 未在登记表内的事件
- **THEN** `resource.event` 保留 `payload.acp` 的原始字节，不静默丢弃、不降级为普通文本

#### Scenario: 超大内嵌内容显式降级

- **WHEN** 事件内嵌的 diff 超过 256 KiB
- **THEN** `payload.acp` 使用 `rawAcp.rawUnavailable`（`reason = "size_limit"`），事件照常投递，不伪装完整内容

### Requirement: `resource.ack` 的单调性与归属

`resource.ack` SHALL 表达该会话的累计 ACK：其 `cursor` MUST 单调不减，回退 MUST 返回 `nodelink.protocol.sequence_invalid`；`sessionRef` MUST 属于当前连接上 attachment 所属会话，`cursor.originEpoch` 与该会话 origin epoch 不一致时同样返回 `sequence_invalid`。ACK 只推进 Access 侧恢复语义，MUST NOT 触发 Owner 删除仍在保留窗口内的事件。

#### Scenario: ACK 回退被拒绝

- **WHEN** 连接先 ACK 到 originSequence `"40"`，随后发送 cursor 为 `"35"` 的 `resource.ack`
- **THEN** Owner 返回 `nodelink.protocol.sequence_invalid`，已确认的 ACK 水位不回退

### Requirement: 命令授权、幂等与终态

`command.submit` 的 `command` SHALL 限于 `compatibility/commands/v1/commands.json` 的 12 个命令名；有效权限 MUST 是 Export grant、该 Access 信任记录 grant 与实际 capability 的交集，越权命令 MUST 以 `command.rejected`（`nodelink.export.not_granted`）拒绝且无副作用。可重试 mutation 的幂等键 MUST 至少含 `(ownerNodeId, accessNodeId, requestId)`：同一键重复提交 MUST 返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后 `payload` 语义不同 MUST 返回 `nodelink.command.idempotency_conflict`。`command.accepted` 的 `acceptedAt` 永远非 `null`；`command.rejected` 不携带 `acceptedAt`；`command.terminal` MUST 携带提交时的 `command` 名，`status = "completed"` 时 `terminal.result` MUST 存在且为 object（`session.create` 必须是 `SessionCreateResult`；其余命令可为空对象 `{}`——§12.5 的「非空」原意是「非 null」，与该节末句的 session.create 动机一致），其余 status MUST 给出 `error`；无法确认副作用时 MUST 写入 `uncertain` 终态。`command.status` 的两种等价形式（独立消息或作为 `command.submit` 的 `command`）MUST 产生同形回复：已终结 mutation → `command.terminal`（`terminalEventId` 非 `null`）；未终结 mutation → 同形 `command.accepted`；查询命令的 requestId 不落持久记录，重查 MUST 返回 `nodelink.command.not_found`。被接受 mutation 的终态 MUST 由提交方连接上有所有者的 watcher（连接断开即取消）推送 `command.terminal`。幂等行落盘前失败（如存储写失败等瞬态原因）的 `session.create` 不作为持久首次结果：同 `requestId` 重查 MUST 返回 `nodelink.command.not_found`，同键重试可得到不同结果（只有写入持久记录后的重复提交才保证返回首次结果）。session-scoped 命令 MUST 携带当前 `attachmentId`/`attachmentGeneration`；`session.mode.set`/`session.config.set` 的 `expectedVersion` MUST 存在。单连接 in-flight 命令不超过协商上限，命令速率超过 120 次/分钟时返回 `nodelink.resource.rate_limited`（`details.retryAfterMs` 存在时 Access 必须遵守），连续超限以 4429 关闭。

#### Scenario: 相同 requestId 重试不重复派发

- **WHEN** Access 以相同 `(requestId, command, payload)` 重复提交同一 mutation
- **THEN** Owner 返回首次的 accepted/terminal 结果，副作用只发生一次

#### Scenario: 同键不同语义被拒绝

- **WHEN** Access 以相同 `requestId` 但不同 `payload` 提交命令
- **THEN** Owner 返回 `nodelink.command.idempotency_conflict`，不执行第二次副作用

#### Scenario: 越权命令被拒绝

- **WHEN** Access 的 Export 只含 `grant.observe`，却提交 `session.prompt`
- **THEN** Owner 以 `command.rejected`（`nodelink.export.not_granted`）拒绝，会话无变化，审计记录该次拒绝

#### Scenario: 崩溃窗口进入 uncertain

- **WHEN** 命令已 accepted 但 Owner 在确认副作用前发生崩溃窗口，恢复后 Access 以 `command.status` 重查
- **THEN** Owner 返回 `status = "uncertain"` 的 `command.terminal`，不猜测成功或失败

#### Scenario: 命令限流

- **WHEN** 单连接一分钟内命令数超过 120
- **THEN** 超限命令收到 `link.error`（`nodelink.resource.rate_limited`，`retryable = true`）；持续超限时连接以 4429 关闭

### Requirement: `session.create` 的硬约束与结果契约

`session.create` SHALL 要求 `grant.remote-work`；其 `payload` 只允许 `agentId`/`exportId`/`workspaceAlias`/`templateParams` 四个键，出现 `cwd`、`mcpServers`、任何绝对路径或凭据字段时 MUST 以 `command.rejected`（`nodelink.command.unsupported_field`）拒绝，`details.field` 给出被拒字段名，且不得创建会话或部分应用参数。`agentId`/`exportId`/`workspaceAlias` 必须同时存在于该 Access 可见 Export 与授权范围内，否则返回 `nodelink.export.not_granted`（可带 `details.parameter`）。首切片 template MUST 零参数：`templateParams` 必须省略或为空对象，声明了参数的 Export/template 必须明确拒绝而非静默忽略。`session.create` 的 `sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` MUST 为 `null`；Owner 先回 `command.accepted`（`result = null`），创建完成后发 `command.terminal`，`completed` 时 `terminal.result` MUST 是含 `remoteSessionRef` 与 `sessionMeta` 的 `SessionCreateResult`，`sessionId` 由 Owner 生成并写入自身事件日志；`uncertain` 时表示无法确认会话是否已创建。

#### Scenario: 正常创建并回传复合引用

- **WHEN** 持 `grant.remote-work` 的 Access 提交 `agentId`/`exportId`/`workspaceAlias` 都合法的 `session.create`（`templateParams` 省略）
- **THEN** Owner 先回 `accepted`（`result = null`），随后 `terminal.result` 给出含 `ownerNodeId`/`exportId`/Owner 生成的 `sessionId` 的 `SessionCreateResult`，Access 可凭其发起 `resource.attach`

#### Scenario: 禁带字段被拒绝且不创建会话

- **WHEN** `session.create` 的 `payload` 携带 `cwd` 或 `mcpServers`
- **THEN** Owner 返回 `command.rejected`（`nodelink.command.unsupported_field`，`details.field` 指明字段），Owner 侧不创建任何会话

#### Scenario: 未知 workspaceAlias 被拒绝

- **WHEN** `session.create` 引用不在该 Export `workspaceAliases` 内的 alias
- **THEN** Owner 返回 `nodelink.export.not_granted`（`details.parameter` 指明被拒参数），不创建会话

### Requirement: 撤销的即时传播

本地 `export.revoke` 提交成功后 SHALL 立即：向持有该 Export 的活跃连接推送 `export.revoked`，拒绝该 Export 的新命令与新订阅，旧连接不得继续取资源。本地 `node.revoke` 提交成功后 SHALL 立即：向该节点推送 `node.trust.revoked` 并以 4410 关闭其连接；该节点后续连接按已撤销拒绝。撤销判定 SHALL 以持久化记录为准，进程内缓存不得让已撤销的 Export/节点继续可用；关闭/推送失败不得回滚已提交的撤销。

#### Scenario: Export 撤销即断资源

- **WHEN** Access 连接正在订阅某 Export 的会话，Owner 本地执行 `export.revoke` 并提交成功
- **THEN** 该连接收到 `export.revoked`，此后对该 Export 的 `resource.attach`/`command.submit` 被拒绝，内存中的订阅状态被清除

#### Scenario: 节点撤销即关连接

- **WHEN** Owner 本地执行 `node.revoke` 并提交成功
- **THEN** 该 Access 的连接收到 `node.trust.revoked` 并以 4410 关闭，重连握手在凭据状态检查处被拒绝

### Requirement: 心跳、超时与慢连接隔离

Owner SHALL 按协商的 `heartbeatIntervalMs`（默认 30 秒）发送 `link.ping`，`link.pong` MUST 原样回填 nonce；连续 90 秒无任何入站消息或 `link.pong` 时 MUST 以 4408 关闭连接。每连接的待发送队列 SHALL 有界（默认 8 MiB 或 2000 条，可经 limits 下调）：达到高水位后先停止读取新的快照批次；持续过慢时断开连接，由 origin cursor 重放补回，MUST NOT 丢弃 Owner 事件或阻塞其他连接，v1 不发送 `link.backpressure`。

#### Scenario: 心跳超时关闭

- **WHEN** 已认证连接 90 秒内没有任何入站消息与 `link.pong`
- **THEN** Owner 以 4408 关闭连接，信任记录与持久化状态不受影响

#### Scenario: 慢连接不影响其他连接

- **WHEN** 某 Access 连接消费缓慢、待发送队列达到高水位
- **THEN** Owner 先暂停向该连接读取新快照批次，持续过慢时断开该连接；同一 Owner 上其他 Access 连接的事件投递不受影响，断开的连接可凭 cursor 重放补齐

### Requirement: 审计与日志边界

握手失败、授权拒绝、命令幂等冲突、撤销与连接关闭 SHALL 按 `SECURITY_DESIGN.md` §14.2 落审计事件；Owner 侧命令审计行只记 `viaNodeId`，`localPrincipalRef` 为 `null`（节点级信任模型，§8.3）。日志与协议错误 MUST NOT 包含私钥、`pairingSecret`、签名/HMAC 输入以外的秘密材料、完整 prompt 或凭据值；close reason 不含敏感信息且不替代结构化错误。

#### Scenario: 拒绝留痕且不含秘密

- **WHEN** 发生认证失败、越权命令与节点撤销各一次
- **THEN** 三类事件都有对应审计记录，日志与错误消息中不出现私钥、`pairingSecret` 或凭据值
