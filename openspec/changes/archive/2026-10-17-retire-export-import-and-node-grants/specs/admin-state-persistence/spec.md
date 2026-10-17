# Admin State Persistence 增量

## REMOVED Requirements

### Requirement: Export 与 Import 的归属与完整移除

**Reason**: Export 与 Import 整族删除（表、方法、CLI 命令），本要求描述的归属校验与完整移除不再有对象。

**Migration**: 跨节点可用性由节点信任记录直接表达：已配对节点即可访问本节点资源（`local.*` 除外）。存储 v11 删除 `owned_export`、`imported_import` 与 `imported_import_export`。

### Requirement: 撤销与删除后的写入不得复活资源

**Reason**: 该要求全部围绕 Export 撤销标记与 Import 完整移除后的迟到回调；两类资源删除后，相关写路径与错误码都不再存在。

**Migration**: 节点撤销的即时生效与"撤销后写入不得复活信任"仍由本能力的「撤销与重启恢复」承接；imported 家族的写入仍受「imported 家族保持无正文」约束。
