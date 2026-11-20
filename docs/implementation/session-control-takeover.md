# 会话控制权接管与只读隔离实现说明

> 本文面向开发者记录 `deliver-session-control-takeover` 在节点内核、存储审计与 Web 工作台之间的工程结构与关键约束；权威产品规则见对应 OpenSpec 规格与变更文档（归档后以长期规格为准）。

## 1. 单实例协调器装配

- `SessionControlCoordinator` 在节点存储与运行时装配期创建唯一 `Arc` 实例，同时注入：
  - `Storage` 与 `TrustStore`（用于设备信任撤销 `TrustStore::revoke` 后同步清零持控状态并推送事件）
  - `ApplicationPolicy` 与 `NodeRequestAuthority` / `ApprovalAuthority`
  - `NativeCoordinator` 与 `SourceSessionService`（用于主会话新建授予初始控制权及容量关闭/回收清零）
  - `NodeManagementSessionServices` 与管理 WebSocket 服务端广播回路
- `NodeRuntime::request_authority()` 与 `NodeRuntime::approval_owner()` 复用同一协调器实例，确保无论经管理通道还是组件入口触发操作都执行同一控制权状态机。

## 2. 主树根解析与进程内单调代际

- **控制权专用根解析（`resolve_control_root`）**：
  - 保持 `NativeCoordinator` 容量加载层 `require_root` 不变；控制权侧从目标向上遍历 `node_session_parents`（含环路检测与步数上限）：
  - 存在父链时，强制要求顶层祖先 `node_session_capacity` 存在且 `root_verified = 1`，收敛至顶层根键 `(source_id, root_native_session_id)` 并携带可选 `rootTargetId`（仅注册子目标时为 `None`）；
  - 无父链顶层目标若在 `node_session_capacity` 中仅存在 `root_verified = 0` 未验证记录，返回 `session_tree_unverified`；
  - 经 `target.register` 等路径独立登记且无未验证容量记录的顶层目标，以自身 `(source_id, native_id)` 作为控制权根键，初始为无主只读态（`controllerClientId = null, controlEpoch = "0"`）。
- **单调代际与防 ABA 水位（`controlEpoch` & `evicted_epoch_floor`）**：
  - 同一 `NodeRuntime` 进程内，`controlEpoch` 从 `"0"` 起按主根键严格单调递增：首次授予/接管、顶号切换、主动释放、撤销清空、容量释放/30 分钟空闲回收与冷会话预接管 30 分钟超时均执行 `+1`；同一持控客户端重复 `takeover` 仅刷新 `controllerDisplayName` 而不递增代际。
  - 若空闲受控记录达到内存条目上限触发淘汰，仅允许淘汰无主空闲项，并将被淘汰项的最高 `controlEpoch` 并入全局 `evicted_epoch_floor`，阻止后续重入会话回退代际或发生跨周期 ABA 绕过。
  - 节点进程重启后协调器重新初始化为全空内存态，不从持久化表重放历史控制权，所有历史主会话均回归无主只读态（`controlEpoch = "0"`）。

## 3. 两步写提交点核验与幂等例外

- **管理通道入口**：`input.register`、`input.consume`、`approval.register`、`approval.decide` 与 `session.control.release` 支持严格可选字段 `expectedControlEpoch`（闭集仅允许省略或 canonical 十进制 `u64` 字符串，显式 `null` 与未知字段直接按格式错误拒绝）。
- **两步写原子复核**：
  - `NodeRequestAuthority::authorize` 与 `QueueService` 在 `input.register` 与 `input.consume` 真正进入队列消费提交前（`BeginInsert` 之前）共同核验当前 `controllerClientId`、`controlEpoch`（含与 `input.register` 阶段冻结代际的一致性）及可选 `expectedControlEpoch`，不满足即返回 `session_not_controlled`（不会占用未消费请求槽位或 3 个队列名额）。
  - `ApprovalAuthority::authorize_decision` 与 `NativeApprovalService` 在 `approval.register` 与 `approval.decide` 持久化提交前同步核验控制代际；若同一授权裁决已完成持久化（`RequestFact::Applying` / `Settled` 或 `StoredApprovalDecision`），断线重连查询或重复 `consume` / `decide` 按幂等读取分支放行，并由节点继续托管交付至底层 ACP 对端，同时保持连接私有 `input.upload.*` 与各类只读视图/队列/审批查询/容量释放入口不受阻塞。

## 4. 四类生命周期清零、单连接安全审计与 Web 对接

- **四类清零入口**：① 客户端显式 `session.control.release`；② `TrustStore::revoke`（覆盖 WSS `device.revoke` 与 CLI 命名管道）；③ `NativeCoordinator::capacity_closed`（覆盖手动 `native.release`、满额淘汰与 30 分钟空闲自动回收）；④ 冷会话预接管超过 30 分钟未加载时由 `sweep_expired()` 清理。
- **SQLite 单连接锁秩序与审计**：`TrustStore::revoke` 在提交设备撤销事务并归还单连接 SQLite 租约之后再写 `session.control.revoke_clear` 安全审计记录，且 `SessionControlCoordinator` 绝不持互斥锁跨越 `.await`；安全事件记入 `security_audit`（`session.control.initial`、`session.control.takeover`、`session.control.release`、`session.control.revoke_clear`），严格保持 `node_management_audit` 冻结 DDL 与零 `session_commands` 表约束。
- **Web 端契约**：协议层通过 [control.ts](../../clients/web/src/management/session-protocol/control.ts) 注册 `session.control.*`、`session.control.changed` 与 `session_not_controlled`；工作台通过 [session-control-controller.ts](../../clients/web/src/workbench/session-control-controller.ts) 维护三态控制横幅（`self` / `other` / `uncontrolled`）、只读写动作按钮禁用门禁，并在顶号切换与重绘时无损保留 `#session-input-body` 的本地草稿与光标选区（详见 [web-workbench.md](web-workbench.md)）。
