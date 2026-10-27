# Spec Delta

## ADDED Requirements

### Requirement: 删除事实进入持久事件流

空会话删除成功后，Owner SHALL 先持久化 `session.deleted`，再向在线已认证客户端投递。该事件 SHALL 使用正常全局事件序号、重放和去重规则，使断线客户端在游标仍有效时也能获知删除。

#### Scenario: 在线客户端收到删除

- **WHEN** 一个已认证客户端在线且订阅覆盖删除事务提交的游标
- **THEN** 它按全局序号收到对应的 `session.deleted` 事件

#### Scenario: 断线客户端恢复

- **WHEN** 客户端在删除发生前断开，并以仍有效的 cursor 重连
- **THEN** 重放包含 `session.deleted`，客户端处理完后从会话投影移除目标

### Requirement: 删除后快照与读取不再包含会话

删除提交后，新快照、`session.list`、`session.read` 和摘要查询 SHALL 都不再返回该 owned 会话。游标过期而要求重建快照时，客户端 MUST 通过不含该会话的新快照收敛，不得依赖已过保留窗口的删除事件。

#### Scenario: 删除后首次同步

- **WHEN** 新客户端在删除提交后首次取得快照
- **THEN** 快照的 `sessions` 不包含已删除会话

#### Scenario: 删除事件已过保留窗口

- **WHEN** 客户端因 cursor 过期而重建快照
- **THEN** 快照直接省略已删除会话，客户端不保留旧目录项或摘要缓存

### Requirement: 删除事件不以普通会话更新替代

删除结果 MUST 使用专用 `session.deleted` 事实表达，MUST NOT 用 `session.updated` 的空标题、关闭状态或本地隐藏规则代替。客户端无需猜测字段组合即可确定会话已从权威存储删除。

#### Scenario: 收到删除投影

- **WHEN** 客户端收到 `session.deleted`
- **THEN** 它按会话标识执行删除，不等待额外更新事件或查询来确认
