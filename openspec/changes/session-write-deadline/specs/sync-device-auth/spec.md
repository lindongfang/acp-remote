## ADDED Requirements

### Requirement: 写路径有期限且写卡住时判据仍被检查

Sync 连接会话 SHALL 与 Node Link 同口径：出站写有期限，「对端不读、socket 缓冲被填满」不会让会话任务无限期停在写路径上；写未被推进的期间，心跳、静默超时与慢消费者看门狗 SHALL 仍按各自窗口被检查并生效。超时的处置 SHALL 与既有分级一致（先按判据处置，最终断开），MUST NOT 静默丢弃事件或让连接无限期存活。

#### Scenario: 对端不读时连接被有界收尾

- **WHEN** Sync 对端停止读取、socket 缓冲被写满
- **THEN** 连接 SHALL 在有界时间内被收尾，MUST NOT 一直保持在「已连接」状态

#### Scenario: 写卡住期间判据仍被检查

- **WHEN** Sync 会话的写路径长时间未被推进
- **THEN** 心跳、静默判定与慢消费者看门狗 SHALL 仍按窗口被检查并生效
