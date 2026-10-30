# WP2 2.3 开发边界

合同单一来源：主仓库 openspec/changes/deliver-codex-session-loop/session-contract.md §4、§7、§8。本说明记录内层实现和开发验证范围，不替代合同或独立验收。

- ApprovalObservation 保存原始请求、选项、作用范围及协议标识；原生 request ID 的字符串与数值类型不同，与 tool ID 使用不同类型。工具关联限定平台 session/Run/turn、实例/连接代际、原生 session 和精确 tool ID；SHA256 仅用于索引，命中后比较原文。缺失引用、未知请求/作用范围/选项保留为 unsupported，不默认允许，也不等待未来事件自动补授权。
- prepare 在 actor/source fence 外强刷 BeforeApproval；commit 要求调用者已按 client_gate→owner 顺序持锁，再复核 owner 生命周期和观察版本，随后开始短 SQLite 事务。prepare 后任何已接收输出使旧检查点无效；检查点跨 attach 不复用。仓库接口是内层持久化端口，不承担网络身份或来源认证。
- 裁决 Command 与 input 共享 actor/request 唯一空间，裁决 CAS、关联、审计原子提交。裁决 accepted 不代表执行；单独的一次 claim 不可恢复为待发送。WriteCompleted 仅表示 transport write，绝不表示 peer 应用 ACK。未知、取消、终态、代际更换、重启后旧 claim 不再 current。失败的报告持久化不撤销已接受裁决、不允许新 claim。
- schema2 尚未发布，审批来源/选项/交付及 typed 工具元数据为草案布局的严格结构。早期中间 schema2 布局拒绝打开，不清库。schema1 仍由已有原子升级迁移，保留 identity、clients/revocation、security_audit 和 node_state。重启统一 recover_interrupted 使活跃输入 unknown，同时失效审批来源；已接受审批 Command 保持 accepted，交付状态另记。
- 正文和原生敏感 ID/标签不进入 security_audit 或 approval_audit_links，审计仅使用平台 ID、代际和固定动作/结果。错误保持语义型，不包含 SQLite 内容或正文。

开发测试采用任务独占临时 SQLite 数据库、有界 Tokio timeout 和真实延迟外键 COMMIT 故障；工具/输出 owner 是明确测试 fixture，没有真实 Harness。覆盖双池相反裁决竞争、重复与冲突、共享命名空间、原始结构、源关联、撤销/终态/取消/代际/重启、决策/审计/claim/report 回滚、强刷失败和准备后 dirty 变化。原34历史测试及 HIST-R001 屏障回归必须重跑。真实 private issuer、client_gate、owner fence 和原生 writer 最终复核仍属于 WP3；这些公共内部坐标或数据库状态不是外部授权。
