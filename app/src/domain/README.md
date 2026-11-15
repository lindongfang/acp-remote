# 会话内层模型

当前实施依据是 [implementation-contract.md](../../../openspec/changes/deliver-codex-session-loop/implementation-contract.md) 与当前八份规格。

`input_queue` 是新版节点共享队列的纯内存状态机：最多三个等待项，挂起项和发送准备仍占名额，活动交接单独管理。编辑/取消通过项版本协调；可靠失败挂起旧项，新输入按条件优先，显式继续才恢复旧项。正文是借用/移动的泛型结构，无正文大小预算或期限。准备阶段可撤销，最后交接复核版本及资源代际。它不认证、不持久登记、不发送协议，也不证明真实前台已经结束；应用仍须按原生目标复用唯一实例并验证这些事实。精确边界见[共享队列合同](../../../openspec/changes/deliver-codex-session-loop/shared-input-queue-contract.md)。

下述 `requests` 签名期限及 `views` 内容预算是此前实现及其历史验证范围。Q-001 已取代这些冲突规则，不能将旧预算、期限或派发再验原actor接入新队列；迁移和真实节点接线仍待独立验收。

`native` 定义显式接入来源、原生目标及各类代际；`activity` 协调单个原生目标的前台、待确认输入和有效权限请求引用；`requests` 定义产品请求意图及保守时间观测。它们不证明操作者已认证、原生事件可信、资源已实际关闭或事务已经提交，应用所有者必须分别验证这些事实。

`views` 是节点持有的可丢弃内存窗口：可靠来源键替换保留位置，读取句柄按容器和资源/读取代际隔离，旧快照及候选替换始终计费。它仅保存调用方交付的结构字节，不解释协议chunk、不产生执行事实、不检查网络操作者权限，也不写入SQLite。使用边界见[窗口合同](../../../openspec/changes/deliver-codex-session-loop/view-window-contract.md)。

结构构建使用 `item_lease`、`stage_workspace`、`stage_insert/stage_replace` 和消费式 `publish`：工作区、旧值租约和完整候选共用原账本，发布重验旧对象身份；`window_status` 无需申请快照即可查询失败后的淘汰水位。未发布候选丢弃会将其原读取标为未完整，旧代际不会污染新读取。合同和分配边界见[预算构建合同](../../../openspec/changes/deliver-codex-session-loop/view-budget-port-contract.md)；这组端口已通过[独立验收](../../../testing/runs/deliver-codex-session-loop/2026-10-31-view-budget-port-acceptance.md)，不代表ACP结构适配已实现。

`with_window` 同步借用保留条目，不申请正文租约，因此不占用额外条目名额。回调拿到冻结状态；窗口的发布、淘汰及首次失败统一推进读取版本，回调返回后版本变化则拒绝结果。返回的stamp只绑定调用者原句柄，不持有正文或授予权限，也不保证未来发送时仍最新。回调只构建可丢弃的私有输出，成功返回后再由调用方按权限与版本协议交付；不能在回调内发布需要撤销的外部副作用。合同见[借用与读取版本](../../../openspec/changes/deliver-codex-session-loop/domain-borrow-revision-contract.md)，端口已通过[独立验收](../../../testing/runs/deliver-codex-session-loop/2026-10-31-domain-borrow-revision-acceptance.md)，网络输出尚未接线。

`history`、旧 `approvals` 来自先前部分实现，当前只在测试构建中提供；旧输入根值保留用于历史回归，正常构建不再提供旧应用服务与正文存储工厂。不能把永久正文、历史强刷或必须活动 Run 的旧语义接回新会话入口。新增模型和管理服务尚未装配成运行节点/Web会话闭环。
