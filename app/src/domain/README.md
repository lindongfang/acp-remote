# 会话内层领域模型

本目录承载平台会话控制面的纯内存值类型、不变量与状态机；所有类型均不自行认证网络调用方身份（`ActorContext` 仅为已由外层网关持门禁验证后的内部上下文）。对外行为规格以 [openspec/specs/](../../../openspec/specs) 下各能力规格为准，容量回收实现说明见 [session-capacity.md](../../../docs/implementation/session-capacity.md)，历史演进契约见归档变更 [implementation-contract.md](../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/implementation-contract.md)。

## 核心子模块职责与边界

- **`input_queue`**：节点共享输入队列的纯内存状态机（最多 3 个等待项，挂起项与发送准备仍占名额，活动交接单独管理）。通过项版本协调编辑与取消；可靠失败挂起旧项，新输入按条件优先，显式继续才恢复旧项；准备阶段可撤销，最终交接复核版本与资源代际。正文为泛型结构，不承担身份认证、持久登记或协议发送，也不单独证明真实前台已结束。详见归档 [shared-input-queue-contract.md](../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/shared-input-queue-contract.md)。
- **`native`、`native_catalog`、`native_dispatch`、`activity`**：`native` 与 `native_catalog` 定义显式接入来源、目录快照、原生目标及代际标识；`native_dispatch` 定义原生派发控制与收据边界；`activity` 协调单个原生目标的前台活动、待确认输入和有效权限请求引用。上述模型不替代应用层对操作者认证、原生事件可信性、资源物理关闭或事务已提交事实的独立校验。
- **`request_registry`、`requests`、`management_audit`**：定义产品幂等请求意图、登记状态、保守时间观测与管理审计值语义。注：`requests` 中保留的早期签名期限与 `views` 旧内容预算仅用于对应兼容边界，不得将旧预算、期限或派发再验原 actor 规则混入新共享队列。
- **`approval_owner`、`approval_records`、`approval_audit`**：定义权限审批的所有权状态机、持久化记录值模型与审计关联结构，保证并发客户端下的唯一裁决、失效收敛与敏感正文零落盘。
- **`shared_view`、`shared_projection`、`shared_view_log`**：定义面向多端只读附着与流控回放的共享会话视图、投影与序号日志模型，不将控制面变成长期聊天正文主数据库。
- **`session_capacity`、`source_sessions`**：定义节点会话主树容量（默认 50 主树）、空闲计时资格（默认 1800 秒）、前台/后台已知活动与待决审批保护及自动回收候选判定规则。详见 [session-capacity.md](../../../docs/implementation/session-capacity.md)。
- **`views`**：节点持有的可丢弃有界内存窗口——可靠来源键替换保留原位置，读取句柄按容器和资源/读取代际隔离，旧快照与候选替换统一计费，仅保存调用方交付的结构字节而不直接解析协议分片或写库（详见归档 [view-window-contract.md](../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/view-window-contract.md)）：
  - 结构构建通过 `item_lease`、`stage_workspace`、`stage_insert` / `stage_replace` 与消费式 `publish` 共享同一账本并在发布时重验旧对象身份；`window_status` 无需申请快照即可查询失败后的淘汰水位，未发布候选丢弃会将其原读取标为未完整（详见归档 [view-budget-port-contract.md](../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/view-budget-port-contract.md)）。
  - `with_window` 同步借用保留条目而不额外占用正文租约名额；窗口发布、淘汰及首次失败统一推进读取版本，回调返回后版本变化则拒绝结果，回调内不得发布不可撤销的外部副作用（详见归档 [domain-borrow-revision-contract.md](../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/domain-borrow-revision-contract.md)）。
