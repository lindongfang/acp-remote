# Spec Delta

## ADDED Requirements

### Requirement: 空会话按正式内容判定

空会话 SHALL 表示从未产生任何正式保存的会话内容。用户或 Agent 消息、轮次、工具调用、终端记录、结构化历史条目及有效待处理交互都算内容；标题、模式、配置、正式会话行、页面草稿和未提交附件都 MUST NOT 单独使会话非空。

#### Scenario: 只修改标题仍为空

- **WHEN** 一个新建会话只设置了自定义标题或模式，没有产生消息、轮次、工具记录、结构化历史或交互
- **THEN** 它仍属于空会话，可进入空会话清理流程

#### Scenario: 受理首条消息后不再为空

- **WHEN** 会话原子保存了第一条用户消息或对应轮次
- **THEN** 此后不得按空会话删除，即使消息正文或运行资源随后不可用

### Requirement: 空会话先回收再删除

保留期到期且空会话满足无查看者及无保护条件时，系统 SHALL 先真实回收运行资源。只有 Agent 明确确认 `session/close` 或受管进程已退出后，MUST 才在同一权威删除操作中移除正式会话记录及该会话的 ACP Remote 历史，并静默发布删除事实。

#### Scenario: Agent 确认关闭后删除

- **WHEN** 空会话满完整保留期且 Agent 明确确认原会话已关闭
- **THEN** ACP Remote 删除正式会话及其历史，宿主工作目录和 Agent 自身内部数据保持不承诺清理

#### Scenario: 未确认资源释放

- **WHEN** Agent 拒绝关闭、关闭结果不确定或无法确认受管进程已退出
- **THEN** 会话记录和历史保留，容量占位不释放，系统可在后续扫描中重试

### Requirement: 非空会话只回收资源

非空会话满足保留期回收条件时 SHALL 只释放运行资源并投影为 `localActivity: idle`，保留正式身份、正文、结构化历史及后续激活所需数据。

#### Scenario: 有历史时到期

- **WHEN** 一个包含用户消息或工具历史的会话满保留期且资源已确认释放
- **THEN** 会话仍出现在目录和历史中，重新进入时可恢复原 Agent 会话身份

### Requirement: 删除与激活在 Owner 上线性化

同一 owned 会话的删除提交与激活提交 SHALL 在 Owner 上按单一顺序生效。删除先提交时，迟到激活 MUST 返回 `session.not_found` 且不启动 Agent；激活先提交时，删除 MUST 重新检查查看租约、任务、排队输入、有效交互和停止确认条件。

#### Scenario: 删除先于激活

- **WHEN** 空会话删除已经提交，随后一个先前排队的激活请求到达
- **THEN** 激活返回 `session.not_found`，不产生新资源或重建已删除身份

#### Scenario: 激活先于删除

- **WHEN** 删除与激活竞争而激活先提交
- **THEN** 删除不删除该会话，直到查看租约及其他保护条件再次全部解除并经过一个新的完整保留期

### Requirement: 重启后的空会话重获完整保留期

Daemon 重启时，任何仍存在的空会话若按时间回收已启用，SHALL 从启动完成时重新获得完整 30 分钟保留期。系统 MUST NOT 使用持久化的旧截止时间在启动后立即删除，也 MUST NOT 自动发送或重放消息。

#### Scenario: 在保留期中重启

- **WHEN** 空会话已无人查看 20 分钟后 Daemon 重启
- **THEN** 重启完成时重新开始 30 分钟计时，会话仍可进入、激活和发送首条消息

### Requirement: 本机 owned 会话统一适用

空会话清理 SHALL 适用于所有本机 owned 会话，不因创建来源是 Web、Zed 或其他本机客户端而不同。Node Link imported 会话 MUST NOT 在 Access 节点产生本地删除记录或内容索引；Access 只消费 Owner 的权威删除事实。

#### Scenario: 非 Web 创建的空会话

- **WHEN** 一个由其他本机客户端创建的 owned 会话始终没有正式内容并满足回收条件
- **THEN** 它按同一空会话判定、资源回收和删除规则处理

#### Scenario: imported 会话被 Owner 删除

- **WHEN** Owner 删除一个空会话并通知 Access
- **THEN** Access 在连接期传播删除结果，且不在本地保存远程会话删除索引或正文
