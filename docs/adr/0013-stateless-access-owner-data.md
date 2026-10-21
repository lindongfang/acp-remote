---
status: accepted
---

# Access 只保存连接资料，远程资源与恢复权威留在 Owner

用户已确认 Access 不保存 Owner 资源数据、命令状态或事件转发进度，只保存连接与信任资料。选择以 Owner 为唯一持久化权威、Access 在线转发，放弃依赖 Access 数据库的远程恢复机制；重启后重新建立来源连接，客户端当前仍持有原请求编号时向 Owner 查询，否则读取当前状态且不自动重发。

本决策的产品取舍已确认，技术合同与迁移已由 [access-owner-data-stateless](../../openspec/changes/archive/2026-10-21-access-owner-data-stateless/proposal.md) 实施（v14 迁移删除 imported 家族、Access 只保留连接资料、远程数据仅在运行期存在）。它计划替换 [ADR-0002](0002-node-link-first-owner-authority.md) 中允许 Access 无正文资源索引的部分，保持 Owner 权威与禁止再次导出等边界；本节点自有资源的持久化不受影响。

**边界（2026-10-21）**：该变更的 **wire 合同已冻结**——`resource.stateless-origin.v1`、`sync.remote_*` 帧族、`resource.caught_up`/`reset_required`/`unavailable`、收窄后的 ACK 语义、`readCursor`/`messageCoverage` 与 `command.status.remoteTarget` 已登记进 compatibility、schemas、fixtures 与两个协议文档（见 `SYNC_PROTOCOL.md` §9.7、`NODE_LINK_PROTOCOL.md` §11.5/§12.4）。修订：对应实现（服务端运行期端口、v14 迁移、Node Link/Sync 路由与 PWA 消费）已落地并通过仓库门禁；任何未实现部分不得被描述成已支持。

## Considered Options

- 保留无正文索引、命令引用与收据：可以本地恢复进度，但仍在 Access 保存 Owner 相关资源及操作记录，用户明确拒绝。
- 只删除正文与标题、保留进度：不满足不记录转发进度的要求。
- 选用纯在线远程资源通道：需要重建来源游标、ACK 与重连合同，但存储边界明确，符合用户确认。

## Consequences

Access 的 ACK 只表示当前运行中的流控确认，不再承诺可恢复落盘；Owner 不得据此提前清理承诺保留期内的数据。来源游标不映射为可持久恢复的 Access 序号，旧客户端不具备新远程通道时明确降级。本机和远程事件必须分域，避免把自有日志也变成临时数据。

需要同步 AGENTS、产品、安全、Sync、Node Link、core 端口与存储文档；代码和数据库未迁移之前，不得将本 ADR 视为当前实现事实。实施顺序固定为 Access 改造、自定义标题、独立离线页。
