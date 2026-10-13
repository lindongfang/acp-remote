<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## MODIFIED Requirements

### Requirement: 心跳、超时与慢连接隔离

Owner SHALL 按协商的 `heartbeatIntervalMs`（默认 30 秒）发送 `link.ping`，`link.pong` MUST 原样回填 nonce；连续 90 秒无任何入站消息或 `link.pong` 时 MUST 以 4408 关闭连接。每连接的待发送队列 SHALL 有界（默认 8 MiB 或 2000 条，可经 limits 下调）：达到高水位后先停止读取新的快照批次；**持续**过慢时断开连接，由 origin cursor 重放补回，MUST NOT 丢弃 Owner 事件或阻塞其他连接，v1 不发送 `link.backpressure`。

**「持续过慢」的判定 SHALL 是可恢复的**：当该连接的待发送队列被消费到低水位以下时，饱和状态 MUST 被清除，该连接 MUST NOT 仅因「曾经饱和过」而在其后被断开。**饱和期间心跳 MUST 继续按 `heartbeatIntervalMs` 发出**：饱和不是静默，MUST NOT 因心跳未发出而走进「连续 90 秒无入站消息」的关闭路径。高水位与低水位 SHALL 都有明确常量并在实现文档中登记（低水位是迟滞阈值，不得等于高水位）。

#### Scenario: 心跳超时关闭

- **WHEN** 已认证连接 90 秒内没有任何入站消息与 `link.pong`
- **THEN** Owner 以 4408 关闭连接，信任记录与持久化状态不受影响

#### Scenario: 慢连接不影响其他连接

- **WHEN** 某 Access 连接消费缓慢、待发送队列达到高水位
- **THEN** Owner 先暂停向该连接读取新快照批次，持续过慢时断开该连接；同一 Owner 上其他 Access 连接的事件投递不受影响，断开的连接可凭 cursor 重放补齐

#### Scenario: 饱和后恢复的连接不被断开

- **WHEN** 某连接的待发送队列曾达到高水位，随后被消费到低水位以下
- **THEN** 该连接 MUST NOT 因那次饱和而被断开，且新快照批次与事件批次的读取恢复正常

#### Scenario: 饱和期间心跳不停

- **WHEN** 某连接长时间处于饱和状态（待发送队列持续位于高水位之上）
- **THEN** `link.ping` 仍按 `heartbeatIntervalMs` 发出；连接不因「心跳未发出」而走进 90 秒静默关闭路径

#### Scenario: 真正持续过慢仍被断开

- **WHEN** 某连接在整个观察窗口内始终未把队列消费到低水位以下
- **THEN** Owner 断开该连接，同一 Owner 上其他连接不受影响，断开的连接可凭 origin cursor 重放补齐
