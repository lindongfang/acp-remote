# Proposal

## Why

用户发现同一会话先思考后回复，页面却显示回复在前。真实数据库证明事件序号正确，但同批消息时间相同，消息页按随机 ID 排序丢失先后；按通道聚合还会把穿插工具后的文字合并回前面。

## What Changes

- **BREAKING** 消息正文片段及消息页按 Owner 固定的发生顺序读取与展示，不以时间或消息 ID 推算发生顺序；不保留旧版本排序兼容。
- 相邻同消息文字连续呈现，跨其他可见条目必须分段；已有工具卡更新不移动。
- Owner 持久保存消息片段的会话与来源起始序号，Access 不持久化远程内容或序号映射。
- 同步实时、历史、分页、重连与远程来源的展示规则；时间只作辅助。
- 旧数据依据现存证据恢复；不能证明的保留并标记不完整，不删除或猜测。

## Capabilities

### New Capabilities
- `conversation-order`: 对话输出的权威顺序、片段边界、读取分页与未知历史的处理。

### Modified Capabilities

无；本能力明确覆盖早期消息页的时间排序约定，结构化历史独立分页接口继续存在。

## Impact

影响 agent-host、core 消息记录、storage-sqlite 迁移、server 读取与 Node Link 转发、Sync schema/fixture/Rust/TypeScript、PWA 时间线。无新依赖、不改变认证、授权和 Access 持久化边界。既有未提交前端修改作为本次基线保留。成功标准为同毫秒、交替、分页、刷新、清理后恢复和远程序号不同域的验证均通过。
