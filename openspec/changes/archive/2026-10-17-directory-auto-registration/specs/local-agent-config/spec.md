# Local Agent Config 增量

## REMOVED Requirements

### Requirement: workspace 记录的本机归属

**Reason**: 目录不再由本地配置维护：`owned_workspace` 表在 v9 删除，目录只能由成功的会话创建自动记录（见 `storage-schema-v2-migration` 的 v9 要求）。

**Migration**: 「使用不存在目录时明确失败」由 `workspace-resolution` 与 `sync-session-create` 的解析失败分类承接；「远程输出不含本机路径」由 `sync-workspace-catalog` 的路径可见面承接。
