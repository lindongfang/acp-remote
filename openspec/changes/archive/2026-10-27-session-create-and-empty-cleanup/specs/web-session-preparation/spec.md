# Spec Delta

## REMOVED Requirements

### Requirement: 后端提前分配固定会话身份

**Reason**: Web 新会话改为直接调用 `session.create`，会话 ID 由正式创建结果返回，不再存在准备阶段预分配身份。

**Migration**: 客户端从创建完成结果取得正式会话 ID 并导航到 `/session/<id>`。

### Requirement: 未提交准备不进入持久目录

**Reason**: 未落库的准备阶段被移除；正式创建成功即成为可同步会话。

**Migration**: 空会话在保留期内立即出现在目录和列表中，到期后按空会话清理规则删除。

### Requirement: 已正式提交的身份返回明确结果

**Reason**: 不再有准备 ID 的 `committed` 恢复分支。

**Migration**: 创建结果以 `session.create` 的持久终态为准；响应丢失时使用原请求标识查询。

### Requirement: 真实模式与配置在提交前可调整

**Reason**: 模式和配置在正式会话页中读取和调整，不再需要提交前临时 Agent。

**Migration**: 创建成功后进入正式会话，由 Agent 实际宣告的控制面提供模式与配置。

### Requirement: 准备操作不得绕过设备认证

**Reason**: 准备控制帧和准备凭据被移除；创建使用既有已认证 Sync 命令。

**Migration**: 删除准备凭据、控制帧和错误码，沿用设备认证、撤销与命令幂等边界。

### Requirement: 刷新恢复完整准备草稿

**Reason**: 不再持久化准备草稿、设置或未提交附件。

**Migration**: 正式会话使用普通页面内存草稿；刷新由权威快照和会话历史重建，不承诺恢复未提交输入。

### Requirement: 标签页隔离与主动离开清除

**Reason**: 准备槽位、准备归属和临时资源均由正式会话生命周期替代。

**Migration**: 删除浏览器准备存储与清理逻辑，正式会话统一由查看租约和保留期管理。

### Requirement: 更换 Agent 或目录重新开始

**Reason**: 正式会话创建后不再提供准备阶段的原位换绑。

**Migration**: 需要不同 Agent 或目录时显式创建新会话；旧会话按正常生命周期保留或清理。

### Requirement: 临时资源可回收且能沿原 ID 重建

**Reason**: 空会话本身就是正式资源，使用正式资源回收、失败重试和宽限期删除规则。

**Migration**: 保留期内可激活正式空会话；到期且 Agent 确认释放后按空会话删除。

### Requirement: 陌生地址仅在权威不存在后跳转

**Reason**: 地址解析不再先检查当前标签页准备上下文。

**Migration**: 所有 `/session/<id>` 都通过权威存在性检查；仅明确不存在或删除事实才跳目录。

### Requirement: 首条消息完整持久受理先于派发

**Reason**: 正式会话与首条消息沿用普通会话提交路径，不再由准备转正事务实现。

**Migration**: 使用既有 `session.prompt` 的稳定请求、持久受理和派发围栏；会话已经由 `session.create` 落库。

### Requirement: 首条输入只展示一次且保持保真

**Reason**: 首条输入不再需要准备 ID 回显关联或转正去重。

**Migration**: 使用普通消息稳定身份和既有 ACP 原文保真规则。

### Requirement: 首次提交的幂等与并发互斥

**Reason**: 不再存在多个请求竞争同一准备 ID 的首条提交。

**Migration**: 首条消息使用普通 `session.prompt` 的设备级稳定 `requestId` 幂等。

### Requirement: 持久派发边界决定故障后的重试

**Reason**: 首条消息不再由 `session.start` 的专用派发围栏处理。

**Migration**: 沿用普通任务派发的可证明未派发、结果不确定与禁止自动重派规则。

### Requirement: 附件完整可用才接受首条消息

**Reason**: 准备附件的预上传与转正语义被移除。

**Migration**: 正式会话按普通消息附件流程在点击发送后上传；全部附件成功后才提交消息。

### Requirement: 兼容边界与旧会话不变

**Reason**: 产品明确不兼容旧准备式新建，要求直接替换。

**Migration**: 客户端不再协商 `core.session-preparation.v1`；服务端移除准备帧和 `session.start`，升级迁移清理旧准备数据并保留正式会话。
