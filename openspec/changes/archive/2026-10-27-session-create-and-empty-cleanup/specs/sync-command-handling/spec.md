# Spec Delta

## ADDED Requirements

### Requirement: 会话删除是幂等 mutation

`session.delete` SHALL 作为 owned 会话的可重试 mutation 登记在 Sync 面。它遵守稳定 `requestId`、先接受后持久终态及同请求最多一次删除；重复请求 MUST 返回首次结果。该命令只删除 ACP Remote 元数据与历史，MUST NOT 被表述为对 Agent 内部数据的 ACP `session/delete`。

#### Scenario: 首次删除返回完成

- **WHEN** 一个无人查看且满足空会话清理条件的会话收到首次 `session.delete`
- **THEN** 请求先被接受，资源确认释放后以持久化 `completed` 终态返回

#### Scenario: 重试不重复删除

- **WHEN** 同一设备以同一 `requestId` 重发语义相同的 `session.delete`
- **THEN** 服务端返回首次终态，不产生第二次资源回收或第二组历史清理

### Requirement: 删除后的激活稳定返回不存在

`session.resume`、查看声明或发送消息若在 `session.delete` 提交后到达，MUST 以 `session.not_found` 拒绝，MUST NOT 由删除事件或旧缓存重建会话。若删除因新的查看者或其他保护条件未提交，目标操作 SHALL 正常按当前会话状态处理。

#### Scenario: 删除后迟到激活

- **WHEN** 删除已提交后才到达一个此前排队的 `session.resume`
- **THEN** 激活返回 `session.not_found`，客户端不保留可见会话并回到目录投影

#### Scenario: 删除后旧会话请求重试

- **WHEN** 删除已提交后同一设备重试一条删除前创建的非删除命令
- **THEN** 服务端返回 `session.not_found`，不得回放该命令并重建已删除会话

#### Scenario: 删除被保护条件取消

- **WHEN** 删除检查时发现新增查看租约或未完成轮次
- **THEN** 若命令尚未受理则不产生命令；若已受理则写入 `failed` 终态，不删除目标会话，并保持其可读取及可继续激活

### Requirement: 删除终态与领域事件同事务

删除的领域事件、正式会话历史移除和 `command.completed` SHALL 在同一权威事务中提交，序号上先写 `session.deleted` 再写命令终态。存储失败 MUST 回滚记录删除和两个事件，命令不得被报告为完成。

已受理的删除命令 MUST always 取得 `completed`、`failed` 或 `uncertain` 之一。释放失败、结果未知或释放后出现新的查看租约、任务、交互等保护条件时，系统 MUST 持久化相应失败或不确定终态并保留正式会话与历史；后续清理使用新的稳定 requestId。

#### Scenario: 删除事务失败

- **WHEN** 删除事件或命令终态任一写入失败
- **THEN** 正式会话、历史、事件序号和容量状态保持删除前结果，客户端不会收到伪造的成功终态

#### Scenario: 受理后出现新查看者

- **WHEN** 删除命令已受理并完成资源释放，但最终事务发现新的查看租约或未结束任务
- **THEN** 系统提交失败终态，不发布 `session.deleted`、不删除历史，并在保护条件解除后由新的请求重试
