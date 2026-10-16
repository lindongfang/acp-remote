# Spec Delta

## MODIFIED Requirements

### Requirement: mutation 先接受后终态

mutation 类命令 SHALL 首次提交只返回 `accepted`（此时 `result` 与 `terminalEventId` 均为 `null`）或 `rejected`；`accepted` MUST NOT 表示业务完成。已接受 mutation 的终态 MUST 只通过一个持久化的 `command.completed`、`command.failed` 或 `command.uncertain` 事件表达，且同一请求 MUST 最多产生一个终态事件。领域状态变化与终态事件 SHALL 在同一事务提交，序号上先写领域事件、再写终态事件。客户端 MUST 只在看到终态事件或 `command.status` 的终态结果后才把命令标为完成。

对会创建资源的 mutation（当前为 `session.create`），受理 MUST 只持久化该命令的幂等记录，MUST NOT 因此创建资源行；资源行与其终态 MUST 在同一次提交内落盘；后端失败时 MUST NOT 留下资源行，MUST NOT 停留在只有受理记录、既无终态又无资源的状态。

#### Scenario: 派发后先收到接受

- **WHEN** 客户端派发一条 `session.prompt` 且授权通过
- **THEN** 首次响应为 `accepted`，`result` 与 `terminalEventId` 为空，MUST NOT 在同一条响应里给出业务结果

#### Scenario: 终态经持久化事件表达

- **WHEN** 被接受的 prompt 对应的轮次正常结束
- **THEN** 该请求的终态以一条 `command.completed` 事件下发，`terminalEventId` 指向它，且该事件与领域状态变化在同一事务内提交

#### Scenario: 崩溃窗口进入结果不确定

- **WHEN** Daemon 恢复时发现某个已接受的 mutation 可能已到达外部 Agent，但无法证明其结果
- **THEN** 服务端先持久化 `command.uncertain` 再向客户端广播，MUST NOT 自动重新派发该命令

#### Scenario: 创建类命令受理时没有资源行

- **WHEN** 一条创建资源的 mutation 已被接受、但后端尚未返回结果
- **THEN** 本机只有该命令的幂等记录（资源标识为空），没有对应的资源行

#### Scenario: 创建类命令失败后不留资源行

- **WHEN** 创建资源的 mutation 因后端失败而终结
- **THEN** 本机没有该资源行，且该命令已有终态（不是只有受理记录）

## ADDED Requirements

### Requirement: 命令级结构化日志

服务端 SHALL 对每个被接受的业务命令与它的终态各产生一条结构化日志，字段至少包含主体标识（哪个设备或节点在控制）、命令名、资源标识（若有）、终态类别与失败成因类别。日志 MUST NOT 包含路径、凭据或完整 prompt；成因类别 SHALL 使用封闭词表，未登记的取值不得凭空写入。

#### Scenario: 受理与终态各有一条

- **WHEN** 一条业务命令被接受并随后终结
- **THEN** 日志里能分别看到受理与终态两条记录，且都带主体标识与命令名

#### Scenario: 失败成因可读且不含敏感内容

- **WHEN** 一条业务命令以失败或不确定终结
- **THEN** 其日志记录给出可区分的成因类别，且不含路径、凭据或 prompt 正文
