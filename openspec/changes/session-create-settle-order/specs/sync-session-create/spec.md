# Spec Delta

## MODIFIED Requirements

### Requirement: 终态与幂等

`session.create` 是有副作用的 mutation：系统 SHALL 先接受（`accepted`，`result` 与 `terminalEventId` 均为 `null`）再执行，最终以 `completed`（携带新会话标识与会话摘要）、`failed` 或 `uncertain` 之一终结。同一稳定 `requestId` 重复提交 MUST 返回首次结果，MUST NOT 产生第二个会话或第二次 Agent 启动；已接受但副作用无法确认的崩溃窗口 MUST 落 `uncertain` 终态，MUST NOT 自动重试。

受理 MUST 只持久化该命令的幂等记录，MUST NOT 因此创建会话行；会话行、ACP 会话标识与归属信息的落盘 MUST 与终态在同一次提交内完成；后端失败时 MUST NOT 留下会话行。终结类别 SHALL 按「能否证明 Agent 侧未产生会话」判定：可证明时 `failed`，无法证明时 `uncertain`。

#### Scenario: 重复 requestId 不产生第二个会话

- **WHEN** 同一设备用同一 `requestId` 重发一次已完成的 `session.create`
- **THEN** 返回首次的终态结果，本机不新增会话、不新增 Agent 进程

#### Scenario: 崩溃窗口进 uncertain

- **WHEN** 命令已被接受，但在确认 Agent 启动结果之前进程崩溃
- **THEN** 恢复后该命令的终态为 `uncertain`，且不自动重试创建；本机没有该会话的会话行

#### Scenario: 受理不创建会话行

- **WHEN** 一条 `session.create` 已被接受，但后端尚未返回结果
- **THEN** 本机只有该命令的幂等记录（会话标识为空），没有对应的会话行；同 `requestId` 的重复提交返回受理态而不再派发一次创建

#### Scenario: 可证明未产生会话时以 failed 终结且零会话行

- **WHEN** 后端因进程未能启动，或 Agent 明确对会话建立请求返回错误（含请求未发出即失败）而未能建立会话
- **THEN** 该命令以 `failed` 终结并携带服务端不可用类错误，本机不产生会话行，同 `requestId` 重发返回该终态

#### Scenario: 无法证明时以 uncertain 终结

- **WHEN** 请求已发出但结果无法证明（短请求超时、发出后连接断开，或会话已建立但后续落盘/存储失败）
- **THEN** 该命令以 `uncertain` 终结并标记可能已到达 Agent，MUST NOT 自动重试
