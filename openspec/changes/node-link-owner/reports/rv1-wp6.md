# RV1-WP6 Review 报告（WP6 命令管线与撤销传播，工作包交付前）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## 结论

**判定：FAIL**（Target Revision `8bc2ff3e078549cebed2ab99b5981d5ed37e0581`；`watchdog_diff stat` 确认 reviewer 启动时 HEAD 即该提交且工作树无改动）。
**发现数：12**（P1/MAJOR 1 条 → 阻断；P2 8 条；SUGGESTION 3 条）。另有 2 项复核结论：RV2-WP5-F1/F2 **已修复**，RV2-WP4 残余①**仍开放但未被 WP6 触及**。

## Shared Report

- **task_id**：2.17 / 2.18 / 2.19 / 2.20；**role / phase**：reviewer / review（RV1 首次检视）
- **agent_context**：独立 reviewer 子 Agent（fbf5360b-2b36-47fb-a1da-c25bfa3be32c；新建、不继承实现对话；只读、无 shell/Git、无写文件工具）
- **Repository**：`D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）
- **base / target**：`5e2d17c7ac4074b1d473dffab664eed48f48f21e` / `8bc2ff3e078549cebed2ab99b5981d5ed37e0581`
- **scope**：`node_link/command.rs`（2128 行）与 `command/tests.rs`（25 条用例）、`node_link/mod.rs`、`resource.rs` 的 additive 方法、`local_admin/{pairing,router,test_support}.rs`、`core/src/{broker,use_cases}.rs`、`core/src/model/session.rs`、`storage-sqlite/src/session_store.rs`、`app/src/compose.rs`（RevocationCloser）、`schemas/node-link/v1/{command,common}.schema.json`、`compatibility/*/v1/*.json`
- **result**：**FAIL**（1 条未解决 MAJOR：RV1-WP6-F1）
- **evidence_paths**：`reports/wp6-command.log`、`reports/wp6-handoff.md`、`reports/rv2-wp5.md`、`reports/rv2-wp4.md`、本报告
- **resource_cleanup**：无资源创建、无写入
- **限制**：L1 无 Git/shell（diff 未独立重算）；L2 未执行命令；L3 目标级计数未逐条重算（按源码数出 command 用例恰 25 条，与 handoff 一致）

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.17", role: reviewer, phase: review, stage: work-package, target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581", evidence_type: REVIEW, evidence_id: RV1-WP6, report_path: "reports/rv1-wp6.md", result: FAIL, evidence_status: NEW, applicability_basis: "只读检视目标文件全文 + 契约文档逐条比对；F1（session.create 幂等只在进程内）阻断 R66/R70/§12.5 与 AGENTS §3 的重试不变量", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.18", role: reviewer, phase: review, stage: work-package, target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581", evidence_type: REVIEW, evidence_id: RV1-WP6, report_path: "reports/rv1-wp6.md", result: FAIL, evidence_status: NEW, applicability_basis: "四键白名单/绝对路径/零参数 template/不创建会话/SessionCreateResult 逐条核对为通过；F1 落在 session.create 的幂等面，故本任务同样阻断", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.19", role: reviewer, phase: review, stage: work-package, target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581", evidence_type: REVIEW, evidence_id: RV1-WP6, report_path: "reports/rv1-wp6.md", result: PASS, evidence_status: NEW, applicability_basis: "提交后通知、重试不重复通知、推送不回滚、4410 关闭、逐消息持久化复核逐条核对；仅余 F5 的口径收窄（P2）", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.20", role: reviewer, phase: review, stage: work-package, target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581", evidence_type: CHECK, evidence_id: PV3, report_path: "reports/wp6-command.log", result: PASS, evidence_status: REUSED, applicability_basis: "日志第二轮明确 HEAD=8bc2ff3 且工作区干净，全 exit=0；reviewer 未复跑，候选门禁前须由 Project Verify 在同一提交重跑", source_evidence: { id: "PV3", report_path: "reports/wp6-command.log", target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581" } }
  - { task_id: "2.17/2.18/2.19 (R66–R78 的 [PV5] 轮次)", role: reviewer, phase: review, stage: work-package, target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581", evidence_type: CHECK, evidence_id: PV5, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "本轮无 [PV5]/E2E 证据；待补门禁：3.11 / 2.22 候选合入前（受控路径全链路）", source_evidence: NOT_APPLICABLE }
```

## Findings（12 条）

| ID | 级别 | 位置 | 问题摘要 | 主 Agent 裁决 / 处理 |
|---|---|---|---|---|
| RV1-WP6-F1 | **P1 阻断** | `command.rs:169-179,599-744,812-877` + core `create_session` | `session.create` 幂等只在进程内：core 不写 owned_command、Access 的 requestId 不落盘 → 重启后同键重试双重创建、command.status 重查回 not_found 而非 uncertain | **修复（wp6-fix1，选项 a）**：create_session 携带 client requestId 并写持久幂等行 + 终态（core/存储合同同步 + 漂移门禁）；顺带修正 MAX_TRACKED_SESSION_CREATES 注释/淘汰语义 |
| RV1-WP6-F2 | P2 | `command.rs:1374-1463` | `states` 对「无 pending 项的连接」永不回收（同 RV2-WP5-F1 同类） | 修复（wp6-fix1：poll_pending 开头按 registry.handles() 回收 + 用例） |
| RV1-WP6-F3 | P2 | `command.rs:1724-1782` + core/storage | core 从不给终态记录写 result → completed 恒 `{}`；accepted.result 丢 turnId | 主 Agent 裁决：「非空」原意 = 非 null（spec R66 与 design D8 已写明口径）；core 收据有意义字段（turnId）透传进 accepted.result（wp6-fix1）+ 文档同步 |
| RV1-WP6-F4 | P2 | `command.rs:293-360` | 「两形式同形 + terminalEventId 非空」字面与唯一可行实现的冲突（未终结/查询两类） | 主 Agent 已同步 spec R66 与 design D8（未终结→同形 accepted；查询重查→not_found） |
| RV1-WP6-F5 | P2 | `command.rs:1552-1607` | export.revoked 推送只覆盖有 attachment/订阅的连接 | 主 Agent 已在 design D7 写明窄口径（catalog-only 连接靠实时投影与命令复核自愈） |
| RV1-WP6-F6 | P2 | `command.rs:493-562` | 三个查询回 command.unsupported 的收窄不在权威文档 | 修复（wp6-fix1：NODE_LINK_PROTOCOL.md §12.7 加注记） |
| RV1-WP6-F7 | P2 | `command.rs:953-1013` | 「三方交集」表述与实现（两支 + capability 下游显式失败）不一致 | 主 Agent 已在 design D8 写明（capability 支由后端显式失败承担） |
| RV1-WP6-F8 | P2 | `command.rs:1828-1884` | 未登记 core 错误码（capability.*/version_conflict）被映射为 retryable=true 的 internal.unavailable → 永久失败被标可重试 | 修复（wp6-fix1：已知永久失败映射为非 retryable 的语义最近登记码） |
| RV1-WP6-F9 | P2 | `compose.rs:737-776` RevocationCloser 只记日志 | WP7 若不替换实现，撤销只留日志 | 列入 WP7 硬门禁（2.21 派发输入 + RV1-WP7 必查项 + verification 跟踪） |
| RV1-WP6-F10 | SUGGESTION | `local_admin/router.rs` 撤销通知用例 | 「提交后才通知」的顺序断言实际不可观察 | 修复（wp6-fix1：RecordingCloser 回读存储断言已撤销） |
| RV1-WP6-F11 | SUGGESTION | `command/tests.rs:1704` | 死代码绑定 `let _ = node;` | 修复（wp6-fix1：删除） |
| RV1-WP6-F12 | SUGGESTION | `command.rs:218-239` dispatch 取消路径无用例 | watcher 的 Shutdown 退出/超龄放弃无覆盖 | 修复（wp6-fix1：补一条 Shutdown 退出用例） |

## Correct（通过项，含证据；摘要）

1. 幂等键完整性（除 session.create）：`(actor_kind, actor_id, request_id)` 合成正确；指纹 = ACPR-CJ1 后 SHA-256；冲突/重放语义有用例。
2. 越权拒绝无副作用 + 审计（record_node_link_auth 扩 authorization.denied；拒绝只写审计、commit 数 0）。
3. 越权优先于代际判定；非本机 ownerNodeId 一律 not_found（无预言机）。
4. terminal 形状契约（command 名取自持久记录、uncertain 不猜成败、两形式逐字节同形）。
5. session.create 四键白名单（严格解码之前判定、不回显输入、commit 数 0）；SessionCreateResult 的 sessionId 来自 core 提交事务。
6. 撤销传播（提交后通知、推送不回滚、4410、逐消息权威复核成立——推送丢失不影响「已撤销不可用」）。
7. 限流与在途上限（120/min、连续 3 次 4429、in-flight 按协商值）。
8. session.list 可见性复用唯一判定点。
9. 错误码全部在 registry 内、retryable 与 registry 一致、details 形状一致。
10. 无 unsafe/秘密进日志；依赖方向未破坏。
11. [PV3] 证据自洽（reviewer 未复跑）。

## 复核（RV2-WP5-F1/F2、RV2-WP4 残余①）

- RV2-WP5-F1/F2 **已解决**（resource.rs:228-268 / :763）。
- RV2-WP4 残余①仍开放但 WP6 未触及该路径（core 的 FakeTrust::record_node_connected 仍 unreachable!；WP6 用例走 local_admin 的真实镜像替身）。保留为观察项。

## 主 Agent 备注

- F1 的修复按选项 (a) 执行（切片内补持久化缝隙），原因：选项 (b)（文档化收窄）会违反 AGENTS.md §3 的最高优先级不变量，不可接受。
- F2/F3/F4/F5/F6/F7/F8 的处置见 Findings 表「主 Agent 裁决」列；F9 列入 WP7 硬门禁；F10/F11/F12 随 wp6-fix1。
- 残余风险登记（reviewer 原文）：F1 未决期间不得对外宣称 session.create 具备跨重启幂等；F9 接线遗漏风险；F2 长期增长；F3/F4 的客户端契约外溢（切片 6 依赖的三条事实应随 F1 裁决写进 §12.5/§12.7）；F6/F7 的切片 6 继承项。
