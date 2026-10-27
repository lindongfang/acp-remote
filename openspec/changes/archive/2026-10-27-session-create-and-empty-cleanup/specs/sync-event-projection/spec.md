# Spec Delta

## ADDED Requirements

### Requirement: 会话删除事件具有类型化最小视图

`session.deleted` 的 `payload.view` SHALL 至少包含被删除会话的稳定 `sessionId`。视图 MUST 表达权威记录已删除，MUST NOT 借标题、关闭状态或普通摘要更新掩盖删除语义。

#### Scenario: 删除视图携带会话标识

- **WHEN** 服务端投影一条 `session.deleted`
- **THEN** 客户端可从 `payload.view.sessionId` 唯一确定要移除的会话，无需自行解析 ACP 原文

### Requirement: 删除投影是静默数据事实

客户端 SHOULD 以删除事件更新本地权威投影，但该投影 MUST NOT 被产品解释为向用户发送“会话已关闭”或“空会话已删除”的通知。事件本身不得包含用户草稿、会话正文或删除原因文案。

#### Scenario: 删除当前空会话

- **WHEN** 当前页面所在会话收到 `session.deleted`
- **THEN** 客户端移除目录、摘要缓存和当前会话投影并返回目录，但不显示删除通知

### Requirement: 删除投影可重复应用

重复投递或重放同一 `session.deleted` SHALL 对客户端投影保持幂等。目标已从本地投影移除时，客户端 MUST 将重复事件视为已应用且不产生第二次导航或其它副作用。

#### Scenario: 删除事件重放

- **WHEN** 客户端因重连再次收到同一删除事件
- **THEN** 会话继续保持移除状态，不发生重复通知或二次跳转
