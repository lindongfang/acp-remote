# 单会话客户端控制权接管与只读隔离任务清单

## 1. 主会话控制权协调器、生命周期钩子与协议层（WP1）

- [x] 1.1 WP1 在 `Storage` / `ApplicationPolicy` / `NodeRuntime` 层组装进程内唯一 `Arc<SessionControlCoordinator>`（按根 `NativeSessionKey` 在不跨 `.await` 的短临界区内维护 `controllerClientId`、`controllerDisplayName`、进程活跃期严格单调递增 `controlEpoch`、`evicted_epoch_floor`、`root_target_id` 缓存、冷会话预接管时间戳及有界 `(actor_id, requestId)` 幂等收据缓存），扩展 `AuthenticatedActor` / `TrustStore` 透传客户端 `display_name`；完成判据：核心单调代际状态机、同请求重试幂等、同身份异参数 `request_conflict` 与非主控 `release` 返回 `session_not_controlled` 单元测试（`SCT-006`）全部通过。
- [x] 1.2 WP1 实现专用控制权主树根解析 `resolve_control_root`（子会话沿 `node_session_parents` 强制要求顶层祖先 `root_verified = 1`，无父链顶层目标拦截仅含 `root_verified = 0` 的未验证记录并兼容独立登记的 `product_targets` 顶层目标，保持 `NativeCoordinator` 容量 `require_root` 不变）及全部生命周期联动钩子——`SourceSessionService` 新建主会话 `bind_created_capacity` 自动授予初始控制权、`TrustStore::revoke`（覆盖 WSS `device.revoke` 与 CLI 命名管道，遵循单连接 SQLite 防死锁规则复用原事务或归还连接后写审计）即时归零、`NativeCoordinator::capacity_closed`（手动释放、30 分钟空闲回收、满额淘汰）与源关闭及冷会话预接管 30 分钟超时置空 `controllerClientId` 并单调递增 `controlEpoch`，向 `security_audit` 追加控制权审计记录（保持 `node_management_audit` 冻结表零改动且不创建 `session_commands`）；完成判据：控制权根解析、生命周期自动授权/单调代际归零与 SQLite 审计约束测试（`SCT-004`、`SCT-014`、`SCT-015`）通过。
- [x] 1.3 WP1 在 `app/src/management_ws/session_protocol.rs` 与 `app/src/management_ws.rs` 实现 `session.control.inspect`、`session.control.takeover`、`session.control.release`、严格仅允许省略或 canonical 十进制 `u64` 字符串（拒绝显式 `null` 与未知键）的可选 `expectedControlEpoch` 解析、单键错误 `"session_not_controlled"` 映射以及 `session.control.changed` 实时广播下发；完成判据：`cargo test --manifest-path app/Cargo.toml --lib --locked` 通过且协议闭合校验单测（`SCT-001` Rust 侧）全绿。

## 2. 队列与审批双步写准入门禁及节点集成回归（WP2）

- [x] 2.1 WP2 在 `NodeRequestAuthority::authorize`、`QueueService`（`input.register` 5 类 `QueueMutation` 与 `input.consume` 入队/变更提交点）及 `ApprovalAuthority::authorize_decision`、`NativeApprovalService`（`approval.register` 与 `approval.decide` 落库提交点）落地根会话控制权与登记代际一致性复核（先执行 Red Check 确认未持控与跨代际两步未提交请求被拒，同时放行已落库 `RequestFact` / `StoredApprovalDecision` 的断线重连幂等读取），保持连接私有 `input.upload.*`、只读视图/队列/排队项正文/审批读取及全局容量 `native.release.*` 不受阻塞，确保已落库审批决定继续由节点托管交付；完成判据：双步跨代际拦截、顶号移交、并发审批唯一裁决与托管交付测试（`SCT-003`–`SCT-015`）全部通过。
- [x] 2.2 WP2 适配受 BREAKING 默认无主只读规则影响的 `app/src/node/**` 与 `app/tests/**` 既有 Rust 集成测试夹具（显式准备 `session.control.takeover` 或 `create` 初始控制权），编写实现说明文档 `docs/implementation/session-control-takeover.md` 并同步更新 `docs/architecture/capability-map.md` 状态描述；完成判据：`npm run check:rust` 与 `npm run check:spec` 退出码 0 通过。

## 3. Web 管理协议、控制权工作台控制器与草稿保护（WP3）

- [x] 3.1 WP3 新增 `clients/web/src/management/session-protocol/control.ts`（由 `session-protocol.ts` 重导出并保持 `<= 120` 行），在 `primitives.ts`、`wire-codec.ts`、`protocol.ts`、`error-messages.ts`、`error-formatting.ts` 与 `client.ts` 接入 `session.control.*` 命令、`session.control.changed` 推送帧、严格拒绝显式 `null` 与未知键的可选 `expectedControlEpoch` 及 `session_not_controlled` 中文错误提示；完成判据：Web 协议闭合编解码、显式 `null`/未知字段拒绝与错误文案映射单测（`SCT-001` Web 侧）通过。
- [x] 3.2 WP3 新增 `clients/web/src/workbench/session-control-controller.ts` 管理目标控制权快照（按单调 `controlEpoch` 合并并发快照与推送，重连后重置连接缓存）、三态控制横幅与接管/顶号/释放按钮，对接队列、审批、来源会话、目录与容量释放控制器实现非主控态写动作 `<button>` 禁用门禁，同时保持 `#session-input-body` 可聚焦及顶号/重绘时本地草稿与光标选区无损保留，同步更新 `clients/web/tests/browser/independent-dom/management-peer.ts`（及 `FIXTURE-SHA256.md`）与 `docs/implementation/web-workbench.md`；完成判据：全部 `session-protocol/*.ts` 与 `workbench/*.ts` 单文件 `<= 650` 行且零循环依赖，`WV-13`–`WV-16` 兼容通过，`npm run check:web`、`npm run build:rust` 与 `npm run check` 全绿（`SCT-002`）。

## 4. 集成验收与交付检视门禁（WP4）

- [ ] 4.1 WP4 完成 `WP1`–`WP3`（含需入库的端到端自动化验收脚本）并锁定受跟踪文件干净的本地分支集成 `commit SHA`，并行执行独立 `tester` 正式验收（`SCT-001`–`SCT-016`，含真实 `acp-remote.exe` 双浏览器上下文闭环）与独立 `reviewer` 全量 diff 交付检视；完成判据：`PV1`–`PV5` 全部通过、所有阻断问题闭环、`plan.md` 中 `Code Review` 集成交付门禁登记为 `passed` 且 `Verification Results` 完成精简证据回填。

## Workflow follow-up

- 归档在同一本地提交内按原子顺序执行：① 将 `specs/session-control-takeover/spec.md`、`specs/session-execution/spec.md`、`specs/permission-approvals/spec.md` 同步合并至 `openspec/specs/<capability>/spec.md`（新建 `openspec/specs/session-control-takeover/spec.md` 并更新两份既有长期规格，不留增量头或断链） → ② 将变更目录移至 `openspec/changes/archive/YYYY-MM-DD-deliver-session-control-takeover/` 并补齐内部指向变更外相对链接的 `../` 层级，同步更新 `README.md` 与 `docs/` 引用 → ③ 新建主归属长期套件 `testing/suites/session-control-takeover/tests.md` 合入 `SCT-001`–`SCT-016` 用例正文并登记 `Source definitions`，同时更新 `testing/suites/session-execution/tests.md` 与 `testing/suites/permission-approvals/tests.md` 的四列 `## 需求关联` 表跨套件按 ID 引用新增/修订场景（关联套件不重复拷贝正文、不追加 `Source definitions`） → ④ 运行 `npm run check:spec` 校验全绿后提交。
