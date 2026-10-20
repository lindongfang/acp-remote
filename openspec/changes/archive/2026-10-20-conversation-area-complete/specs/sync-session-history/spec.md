# Spec Delta

## RENAMED Requirements

- FROM: `### Requirement: 历史条目只在客户端同步面生效`
- TO: `### Requirement: 历史读取按端到端协商支持节点回源`

## MODIFIED Requirements

### Requirement: 历史读取按端到端协商支持节点回源

历史读取 SHALL 支持 Sync 以及协商相应能力后的 Node Link 回源，保持事件标识、会话内序号、独立分页与原文可用性语义。未协商的新字段 MUST NOT 发往旧节点；旧节点按既有形状读取的行为 SHALL 不变。远程历史 SHALL 由 Owner 提供，Access MUST NOT 持久化正文；不可达或不支持 SHALL 明确说明。

#### Scenario: 节点协议侧行为不变
- **WHEN** 旧 Node Link 调用方按既有形状请求历史
- **THEN** 其既有行为不因新能力改变，不被强推新字段

#### Scenario: 协商后回源分页
- **WHEN** 已协商双方通过 Access 读取远程会话历史
- **THEN** 从 Owner 取得正确页边界与稳定标识，Access 只保存允许的无正文索引

## ADDED Requirements

### Requirement: 消息内容与部分输出可持久恢复

Owner SHALL 保留用户输入、Agent 回复、Agent 明确发送的思考及媒体引用，区分回复与思考，保持原始及扩展载荷保真。停止或失败时已接收内容 SHALL 保存为明确的部分结果，MUST NOT 当作成功完整回复。消息正文 SHALL 仅由消息资源提供，结构化历史 MUST NOT 重复承载同段正文；不得为流式恢复而把正文放入清单快照。

#### Scenario: 思考与部分回复恢复
- **WHEN** Agent 发送思考与部分回复后停止或失败，用户重新进入会话
- **THEN** 保留期内两种内容分别恢复并显示实际结果，不虚构未发送的思考

#### Scenario: 清理增量后读取权威内容
- **WHEN** 完成或中断的轮次流式增量已压缩
- **THEN** 消息仍能恢复对应保留内容，结构化历史没有重复消息正文

### Requirement: 流式事件先提交再交付

Owner SHALL 在带序号增量广播前持久化；Access SHALL 在本地广播前提交无正文收据。重放 SHALL 保持稳定事件标识和次序，消息内容压缩 SHALL 在权威内容提交后进行。客户端无法取得超出保留期的内容时 SHALL 明确不可用，MUST NOT 以空结果冒充完整历史。

#### Scenario: 提交失败与重连重复
- **WHEN** 增量提交失败，或已提交增量在重连后重复到达
- **THEN** 未提交增量不广播，已提交重复增量仅应用一次，历史与最终呈现一致
