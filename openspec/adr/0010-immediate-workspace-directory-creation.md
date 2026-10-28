# ADR-0010：独立创建主机目录

- 状态：Accepted
- 日期：2026-10-18
- 部分取代：[ADR-0009](0009-authenticated-workspace-path-and-browse.md) 中不提供独立建目录 mutation 的决定；其余路径可见面与信任边界不变。

## 决策

用户已批准文件夹选择器立即创建真实目录。新增仅 Sync 可用的 workspace.mkdir 变更命令，以 core.workspace-mkdir.v1 协商。它只创建既有父目录下的一层目录，不自动选择、不启动 Agent、不登记 workspace；用户可进入新目录继续创建下一级，最后单独选择。取消选择或创建会话不会删除已经创建的目录。

命令先持久受理，后执行文件系统副作用，再持久提交无会话终态与全局事件。成功结果仅 { created: true }，事件、错误和审计不含路径或目录名。崩溃窗口明确恢复为 uncertain，不自动重做，不根据目录存在推测成功。

沿用已认证设备能力面，不新增设备 scope。Node Link 不接受任意 Owner 路径或该命令。保留旧 session.create(createIfMissing) 行为与全部旧 fixture。

## 取舍

文件系统创建与 SQLite 终态不能原子提交，因此稳定 requestId 与 uncertain 是必要的。新客户端连接旧 Daemon 时明确显示不支持独立新建，不能用延迟创建冒充成功。
