<!-- 已有能力（node-link-owner-server）的增量：省略 ## Purpose，只写需求变化。 -->

## MODIFIED Requirements

### Requirement: 命令授权、幂等与终态

`command.submit` 的 `command` SHALL 限于 `compatibility/commands/v1/commands.json` 的 13 个命令名；有效权限 MUST 是 Export grant、该 Access 信任记录 grant 与实际 capability 的交集，越权命令 MUST 以 `command.rejected`（`nodelink.export.not_granted`）拒绝且无副作用。可重试 mutation 的幂等键 MUST 至少含 `(ownerNodeId, accessNodeId, requestId)`：同一键重复提交 MUST 返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后 `payload` 语义不同 MUST 返回 `nodelink.command.idempotency_conflict`。`command.accepted` 的 `acceptedAt` 永远非 `null`；`command.rejected` 不携带 `acceptedAt`；`command.terminal` MUST 携带提交时的 `command` 名，`status = "completed"` 时 `terminal.result` MUST 存在且为 object（`session.create` 必须是 `SessionCreateResult`；`session.resume` 必须是 `SessionResumeResult`；其余命令可为空对象 `{}`——§12.5 的「非空」原意是「非 null」，与该节末句的 session.create 动机一致），其余 status MUST 给出 `error`；无法确认副作用时 MUST 写入 `uncertain` 终态。`command.status` 的两种等价形式（独立消息或作为 `command.submit` 的 `command`）MUST 产生同形回复：已终结 mutation → `command.terminal`（`terminalEventId` 非 `null`）；未终结 mutation → 同形 `command.accepted`；查询命令的 requestId 不落持久记录，重查 MUST 返回 `nodelink.command.not_found`。被接受 mutation 的终态 MUST 由提交方连接上有所有者的 watcher（连接断开即取消）推送 `command.terminal`。幂等行落盘前失败（如存储写失败等瞬态原因）的 `session.create` 不作为持久首次结果：同 `requestId` 重查 MUST 返回 `nodelink.command.not_found`，同键重试可得到不同结果（只有写入持久记录后的重复提交才保证返回首次结果）。session-scoped 命令 MUST 携带当前 `attachmentId`/`attachmentGeneration`；`session.mode.set`/`session.config.set` 的 `expectedVersion` MUST 存在。单连接 in-flight 命令不超过协商上限，命令速率超过 120 次/分钟时返回 `nodelink.resource.rate_limited`（`details.retryAfterMs` 存在时 Access 必须遵守），连续超限以 4429 关闭。

`session.resume` 与 `session.create` 一样按 `grant.remote-work` 判定授权，不新增授权维度；授权判定 MUST 发生在读取该会话行与触碰文件系统之前，未授权时 MUST NOT 因会话是否存在而产生可区分的响应。

#### Scenario: 相同 requestId 重试不重复派发

- **WHEN** Access 以相同 `(requestId, command, payload)` 重复提交同一 mutation
- **THEN** Owner 返回首次的 accepted/terminal 结果，副作用只发生一次

#### Scenario: 同键不同语义被拒绝

- **WHEN** Access 以相同 `requestId` 但不同 `payload` 提交命令
- **THEN** Owner 返回 `nodelink.command.idempotency_conflict`，不执行第二次副作用

#### Scenario: 越权命令被拒绝

- **WHEN** Access 的 Export 只含 `grant.observe`，却提交 `session.prompt`
- **THEN** Owner 以 `command.rejected`（`nodelink.export.not_granted`）拒绝，会话无变化，审计记录该次拒绝

#### Scenario: 越权恢复被拒绝且先于本机读取

- **WHEN** 持 `grant.observe` 的 Access 提交 `session.resume`
- **THEN** Owner 以 `command.rejected`（`nodelink.export.not_granted`）拒绝，不读取该会话行、不触碰文件系统、不启动 Agent 进程，且响应不因会话是否存在而不同

#### Scenario: 崩溃窗口进入 uncertain

- **WHEN** 命令已 accepted 但 Owner 在确认副作用前发生崩溃窗口，恢复后 Access 以 `command.status` 重查
- **THEN** Owner 返回 `status = "uncertain"` 的 `command.terminal`，不猜测成功或失败

#### Scenario: 命令限流

- **WHEN** 单连接一分钟内命令数超过 120
- **THEN** 超限命令收到 `link.error`（`nodelink.resource.rate_limited`，`retryable = true`）；持续超限时连接以 4429 关闭

## ADDED Requirements

### Requirement: `session.resume` 的 payload 与结果契约

`session.resume` SHALL 要求 `grant.remote-work`；其 `payload` MUST 是空对象 `{}`，出现任何键时 MUST 以 `command.rejected`（`nodelink.command.unsupported_field`）拒绝，`details.field` 给出被拒字段名，且不得启动 Agent 进程或部分应用参数（恢复所需的 Agent 标识、ACP 会话标识与创建时目录一律取自 Owner 自身的持久化记录，不接受客户端提供）。其 `expectedVersion` MUST 为 `null`；`sessionRef`/`attachmentId`/`attachmentGeneration` MUST 为指向该 Export 可见会话的非 `null` 值。

`session.resume` 是带副作用的 mutation：Owner 先回 `command.accepted`（`result = null`），恢复完成后发 `command.terminal`；`completed` 时 `terminal.result` MUST 是 `SessionResumeResult`（含 `remoteSessionRef` 与 `sessionMeta`），`failed` 时 MUST 给出错误；目标 Agent 未宣告 `sessionCapabilities.resume` 时 MUST 以 `nodelink.command.unsupported` 终态失败，MUST NOT 静默降级为新建会话。

#### Scenario: 正常恢复并回传会话引用

- **WHEN** 持 `grant.remote-work` 的 Access 对一条已导出、归属于该 Export 且带持久化恢复数据的会话提交 `session.resume`（payload 为空对象）
- **THEN** Owner 先回 `accepted`（`result = null`），随后 `terminal.result` 给出含 `remoteSessionRef` 与 `sessionMeta` 的 `SessionResumeResult`，该会话随后可接受 `session.prompt`

#### Scenario: 携带字段被拒绝且不启动进程

- **WHEN** `session.resume` 的 `payload` 携带任意键（例如 `cwd` 或 `agentId`）
- **THEN** Owner 返回 `command.rejected`（`nodelink.command.unsupported_field`，`details.field` 指明字段），不启动 Agent 进程、不改会话状态

#### Scenario: Agent 不支持恢复时终态失败

- **WHEN** 目标 Agent 未宣告 `sessionCapabilities.resume`
- **THEN** `session.resume` 以 `command.terminal`（`status = "failed"` 或 `uncertain`）与 `nodelink.command.unsupported` 结束，不创建新会话、不报告成功
