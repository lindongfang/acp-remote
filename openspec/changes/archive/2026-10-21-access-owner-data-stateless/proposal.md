# Proposal

## Why

用户已确认：Access 不保存任何 Owner 资源数据、命令状态或事件转发进度，只保留连接信息；先完成这项改造，再交付自定义标题，离线页面独立随后交付。现行 imported 索引及“收据落盘后广播”的设计与该要求冲突，不能只删除写库调用。

## What Changes

- **BREAKING** 将 Access 从持久化无正文索引改为在线转发：远程清单、标题、状态、正文、命令追踪、来源游标与交付映射均不落盘。
- 明确允许保存的连接资料；保留本节点自有资源的原有权威日志与恢复保证，节点同时承担两种角色时按资源归属分别处理。
- 远程同步使用 Owner 的来源事件身份与游标；不再伪造 Access 本地持久序号。客户端当前运行期间可用原请求编号回源查询，刷新后重新读取现状，不自动重发。
- 增加协商后的独立远程事件通道与纯内存路由，旧远程通道明确降级，绝不偷偷落盘兼容。
- 清除 Access 历史 imported 资源、命令及投递数据，测试 owned 数据、连接信任和身份不受影响。
- 本轮只准备文档；不实现离线页、不自动生成标题、不取消 Owner 命令去重、不引入云存储或新身份模型。
- 成功判据：完整跨节点操作、断网与重启后 Access 存储仅保留连接资料；Owner 可恢复历史，原请求不会再次执行，无法恢复时明确报错。

## Capabilities

### New Capabilities

- `stateless-access`: Access 连接资料白名单、远程运行期数据生命周期、无本地持久游标的恢复与迁移。

### Modified Capabilities

- `node-link-access-session`: 替换无正文收据为 Owner 回源的在线交付，明确命令状态不在 Access 保存。
- `sync-event-delivery`: 自有事件保留本地游标，远程事件另用来源游标；远程清单在线产生。
- `sync-session-history`: 移除 Access 历史索引与收据前提，保留 Owner 先提交和按来源读取。
- `sync-command-handling`: 远程命令状态回源，Owner 承担持久去重，Access 日志不记录远程请求或资源身份。
- `node-link-owner-server`: 新 ACK 不再承诺 Access 落盘，增加明确追平屏障，保留 Owner 幂等和授权模型。
- `storage-schema-v2-migration`: 替换 imported 白名单为远程资源不落盘的升级与清理验收。
- `pwa-web-client`: 远程资料与恢复进度只在当前运行内使用，断线清除远程展示数据，不提供离线元数据视图。

## Impact

涉及已有 core、storage-sqlite、node-link-client、server、app、两个 wire 协议及 PWA；需替换 core 端口、移除 imported 表的资源用途、增加协议 schema/fixture/feature、更新产品/模块/协议/安全/存储权威文档及 AGENTS 不变量。本规划不修改当前实现或权威行为合同。背景决策见 [ADR-0013](../../../docs/adr/0013-stateless-access-owner-data.md)。

实施顺序：本变更 → [custom-session-title](../custom-session-title/proposal.md) → [scoped-offline-page](../scoped-offline-page/proposal.md)。不依赖标题改名命令即可独立验证。
