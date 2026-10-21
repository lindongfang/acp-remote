## REMOVED Requirements

### Requirement: 会话标题只由 Agent 通知更新

**Reason**: 用户要求支持最高优先级自定义标题，原要求禁止客户端改名，与新需求冲突。
**Migration**: 保留 Agent 标题和原始通知语义，以新增的双来源优先级要求替代；旧会话自定义标题初始为空。

## ADDED Requirements

### Requirement: Agent 标题独立更新并服从自定义标题优先级

系统 SHALL 将 Agent 会话信息通知中的标题保存为 Agent 标题，并以自定义标题优先、Agent 标题其次、未命名会话兜底的顺序产生有效标题。Agent 通知 MUST NOT 覆盖自定义标题。客户端 SHALL 通过专用命令设置或修改自定义标题，MUST NOT 伪造 Agent 会话信息通知。通知缺少标题时 MUST 保留既有 Agent 标题，显式空值时 SHALL 清空 Agent 标题；原始 ACP 内容 MUST 保持保真。

#### Scenario: 自定义后 Agent 更名
- **WHEN** 已保存自定义标题的会话收到新 Agent 标题
- **THEN** 有效标题保持自定义值，不提供清除或恢复操作

#### Scenario: 部分更新与显式清空
- **WHEN** Agent 通知没有标题字段或标题显式为空
- **THEN** 前者保持 Agent 标题，后者清空 Agent 标题，两者均不修改自定义标题

#### Scenario: 新会话没有任何标题
- **WHEN** 会话尚无自定义标题且 Agent 未提供标题
- **THEN** 显示未命名会话
