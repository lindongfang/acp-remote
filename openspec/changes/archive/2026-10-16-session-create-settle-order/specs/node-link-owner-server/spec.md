# Spec Delta

## MODIFIED Requirements

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

判定「该命令有没有产生业务副作用」MUST 以资源行是否已提交为准（对 `session.create` 即会话行），MUST NOT 以「该 `requestId` 是否存在持久记录」代替——受理记录在受理阶段就已经存在，用它当替身会把可证明的失败误报成 `uncertain`。

#### Scenario: command.status 同形回复

- **WHEN** Access 以 `command.status`（独立消息形式）重查一个已终结的 mutation
- **THEN** Owner 回复与提交时同形的 `command.terminal`，其 `terminalEventId` 非 `null`

#### Scenario: 未登记 requestId 重查为 not_found

- **WHEN** Access 以 `command.status` 重查一个从未提交过的 `requestId`
- **THEN** Owner 返回 `nodelink.command.not_found`

#### Scenario: 受理记录的存在不构成副作用证据

- **WHEN** 一条已受理的 `session.create` 在后端阶段失败，Access 以同一 `requestId` 重查
- **THEN** Owner 按失败是否可证明返回 `failed` 或 `uncertain` 的终态，MUST NOT 因为"受理记录存在"就一律回 `uncertain`
