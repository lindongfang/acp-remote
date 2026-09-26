# RV1-WP7 Review 报告（WP7 app 接线 + 全链路集成测试，工作包交付前）

> 说明：本轮 reviewer 子 Agent 只读检视（无 shell/Git/写文件工具），报告由主 Agent 按其返回正文原样持久化。

## 结论

**判定：PASS**（Target Revision `a3109c8a80ec806cd7899b72709ec07e579cec28`）。

- **4 条硬门禁全部通过**：① `RevocationCloser` 桩已被真实 `NodeLinkCloser` 取代并注入 `local_admin`；② `CommandRoute::dispatch` 在 Daemon 中 spawn 且纳入关闭序列；③ `ResourceRoute::new` 第三参数取 `authority.local_node()` 且由 [PV5] 断言钉死；④ 组合根零业务规则。
- **附带回执**：RV2-WP6-F1/F2 **均已解决且未回归**。
- **新发现 4 条，全部非阻断**。无 CRITICAL/MAJOR。

## Shared Report

- **task_id**：2.21、2.22；**role / phase**：reviewer / review（work-package 交付前）
- **agent_context**：独立 reviewer 子 Agent（d2dccebc-a87c-4d9d-a83c-5f6751c43d18；新建、不继承实现对话、隔离上下文；只读、无 shell/Git/写文件工具）
- **Repository**：`D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）
- **base / target**：`1efd69ff886e72a932bf897e44003273217d670c` / `a3109c8a80ec806cd7899b72709ec07e579cec28`
- **scope**：`crates/app/src/{config,compose,daemon}.rs`、`crates/app/Cargo.toml`、`crates/app/tests/**`；跨 seam 核对 `server` 的 `resource.rs`/`command.rs`/`local_admin`/`transport::net`、`core/src/broker.rs`、各权威文档
- **result**：PASS（4 条非阻断发现）；**evidence_paths**：`reports/wp7-handoff.md`、`wp7-app-wiring.log`、`wp7-integration.log`、`wp7-workspace.log`、`pv5-windows-nodelink.log`（WP7 轮次）、`reports/rv2-wp6.md`、本文件
- **resource_cleanup**：无资源创建、无写入、无子进程
- **限制**：L1 无 Git/shell（target 全文 + 逐 seam 交叉核对，非行级 diff）；L2 未执行命令；L3 E2E 不适用

## Findings

| ID | 级别 | 位置 | 问题 | 主 Agent 裁决 / 处理 |
|---|---|---|---|---|
| RV1-WP7-F1 | MINOR | `app/tests/node_link_listener.rs:311-334` | unwired 收敛用例因日志级别（debug 事件 + info 配置）恒空循环 → vacuous（任何实现都通过） | 修复（wp7-fix1：改 debug 级 + 阳性对照（配 `instance_lock="ipc"` 断言事件确实出现）） |
| RV1-WP7-F2 | MINOR | `daemon.rs:1403-1417` | 网络 ingress 的 stop 只是构造 future，trigger 在 await 才执行；本地排空窗口内网络 listener 仍 accept，且「已停止接受新连接」日志早于事实 | 修复（wp7-fix1：`NetIngress::stop` 拆 trigger_shutdown + wait_stopped；日志移到真正停 accept 之后；补「stop 返回后再连被拒」用例） |
| RV1-WP7-F3 | MINOR（report-only，主 Agent 裁决） | `core/broker.rs:1768-1785` | 非终态批次写失败（Unavailable）静默丢弃 → 同 turn 终态批照常 completed → 正文缺失但报完成；与 CORE_PORTS §6 第 9 条字面冲突（既有代码，非本轮引入） | **主 Agent 裁决：选 (a) 修**——非终态批次 Unavailable 也使在跑 turn 进失败/uncertain（不留「completed 但正文缺失」）；含 core 端口层可观测信号与新用例（含红向）；wp7-fix1 执行 |
| RV1-WP7-F4 | SUGGESTION | `app/tests/support/owner.rs:139-148` | 测试 harness 复刻私有常量 `NODE_KEY_LABEL`（第二份口径） | 修复（wp7-fix1：改用 `identity.key().clone()`） |

## 报告性观察（不计入 findings）

- `node_link.*` 键的登记口径准确（`NetConfig::default()` 与 conn limits 默认值同值），但 `CONFIG_REFERENCE.md` §3 无「本切片未接线」注记——**WP8（2.23）必须完成项**（否则交付版本「文档说有、实际不生效」）。
- handoff 覆盖映射第 02 步把 claim 路径写作 `/node-link/v1/pair/claim`（应为 `/pairing/claim`）——报告文字笔误，不影响证据（测试用常量）。
- `crates/app` 无 `unsafe`；日志字段无秘密；测试侧 trace 默认关闭。

## Assessment（已核对要点）

1. 硬门禁 ①：NodeLinkCloser 真实调用 CommandRoute 的两个方法；`node.revoke`/`export.revoke` 都在持久提交之后调用；旧桩全仓无引用。
2. 硬门禁 ②：dispatch 与扇出由 NetIngress 持有、关闭序列第一步停止并 join/超时 abort。
3. 硬门禁 ③：生产唯一落点传 `authority.local_node()`；e2e 断言 ownerNodeId 一致且 attach/ack 真实跑通。
4. 启动/关闭顺序满足 R1 与 D11；F2 只涉及「停网络接入」的触发时机（非次序违规）。
5. 组合根零业务规则。
6. [PV4] 真实性：8 个用例真实二进制起停；F1 指出其中 1 个 vacuous，其余 7 个断言有效。
7. [PV5] 真实性：真实 listener/SQLite/Authority/LocalAdminRouter + 组合根同一批接线点；三条必备端到端断言与 TLS direct 轮次逐条核对；故障注入计数与帧级 4410 断言一致。
8. 隐蔽证据缺陷检查：空转 bug 已修；无 #[ignore]/全跳过；故障注入开关复位；OwnerNode::stop 显式释放存储。
9. `daemon.status` 形状与 §5.2 逐项一致；listen 真实可达。
10. RV2-WP6-F1/F2 附带回执已解决且未回归。
11. 依赖与边界：app 的新增 dev-dependencies 全 `workspace = true`，§5 矩阵 app 行允许。

## 待补检查证据

- [PV1]/[PV2]（WP7 提交上）：PENDING（handoff 未给 npm run check 原始日志）——3.13 在补。
- [PV4]/[PV5]：REUSED（reviewer 未复跑，候选门禁须重跑）。
- E2E：NOT_APPLICABLE。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.21", role: reviewer, phase: review, stage: work-package, target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28", evidence_type: REVIEW, evidence_id: RV1-WP7, report_path: "reports/rv1-wp7.md", result: PASS, evidence_status: NEW, applicability_basis: "只读检视 target 全文：四条硬门禁逐条落到代码；RV2-WP6-F1/F2 复核已解决；4 条非阻断发现", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.22", role: reviewer, phase: review, stage: work-package, target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28", evidence_type: REVIEW, evidence_id: RV1-WP7, report_path: "reports/rv1-wp7.md", result: PASS, evidence_status: NEW, applicability_basis: "真实 listener/SQLite/Authority/LocalAdminRouter；三条必备端到端断言与 tls_direct 轮次逐条核对；刻意替身与既有残余与代码一致", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.21", role: reviewer, phase: review, stage: work-package, target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28", evidence_type: CHECK, evidence_id: PV4, report_path: "reports/wp7-app-wiring.log", result: PASS, evidence_status: REUSED, applicability_basis: "8 passed/0 failed 与 target 版测试源码交叉核对（其中 1 个用例 vacuous，记 RV1-WP7-F1）", source_evidence: { id: "PV4", report_path: "reports/wp7-app-wiring.log", target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28" } }
  - { task_id: "2.22", role: reviewer, phase: review, stage: work-package, target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28", evidence_type: CHECK, evidence_id: PV5, report_path: "reports/wp7-integration.log", result: PASS, evidence_status: REUSED, applicability_basis: "2 passed/0 failed；与覆盖映射、故障注入计数与帧级 4410 断言核对一致；候选门禁须重跑", source_evidence: { id: "PV5", report_path: "reports/wp7-integration.log", target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28" } }
  - { task_id: "2.21/2.22", role: reviewer, phase: review, stage: work-package, target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28", evidence_type: CHECK, evidence_id: PV1, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "WP7 轮没有 npm run check 的原始日志；候选/主分支门禁前补齐（3.13 在补）", source_evidence: NOT_APPLICABLE }
```
