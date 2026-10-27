# Spec Delta

## ADDED Requirements

### Requirement: 创建成功直接投影活跃

`session.create` 成功建立会话后的摘要 SHALL 立即包含 `localActivity: active`，不得先发布 `idle` 再以生命周期事件切到 `active`。该取值 SHALL 反映已建立且尚未确认释放的运行资源。

#### Scenario: 创建终态没有空闲闪动

- **WHEN** 客户端创建会话且 Agent 成功建立原会话身份
- **THEN** 创建完成结果中的会话摘要直接为 `localActivity: active`，客户端不观察到先空闲后活跃的投影

### Requirement: 空会话在保留期内完整可用

`session.create` 成功后形成的空会话 SHALL 立即出现在目录、会话清单和权威读取中；在删除前，它可以进入、建立查看租约、激活并接收第一条消息。系统 MUST NOT 因其尚无内容而在保留期结束前提前隐藏或删除。

#### Scenario: 新建后立即进入并发送首条消息

- **WHEN** Web 创建成功并进入新会话，用户在保留期内激活并提交第一条消息
- **THEN** 目录和列表已经包含该会话，首条消息可正常受理，会话随后不再满足空会话条件

#### Scenario: 宽限期内刷新目录

- **WHEN** 一个尚未产生内容的会话仍处于完整保留期
- **THEN** 快照和会话清单仍返回该会话，客户端可再次进入和激活
