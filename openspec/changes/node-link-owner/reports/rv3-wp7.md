# RV3-WP7 复核报告（WP7 fix2，工作包交付前，recheck）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化。

## 结论

**PASS**（Target Revision `5c869160e8dab0361b248124f631beba239183c6`）。RV2-WP7 的 F1（P1）主形态、F2、F3 均已解决；无未解决 CRITICAL/MAJOR；新增 3 条 P2 报告项不阻断交付。占位机制在静态推理下不引入新死锁/永卡，也不产生第二个持久权威写入者，与 §6 第 9/15/16/19 条一致。

## 逐项复核（按原问题 ID）

- **RV2-WP7-F1（P1）→ 已解决**。占位机制：`TurnQueue.held: Option<HeldTurn>`；`abandon_failed_turn` 改为 `hold_abandoned_turn`（不再 `finish_turn()`）；`dispatch_one` 首部 `hold_blocks_dispatch()` 挡住唯一提升点；占位期事件按 abandoned 归属丢弃、终态额外释放占位（归属→丢弃→释放同一步）；兜底 `ABANDONED_TURN_HOLD_ROUNDS = 1200` 可达（会话投影 Queued 落在合并窗口内）；取消路径经 `submit_cancel` → 端点 `turn.cancelled` → 释放占位。两条新用例 + 红向证据（base `690b917` 上真实复现 P1 原症状：T2 命令被 T1 迟到终态报成 completed）。
- **RV2-WP7-F2 → 已解决**（owner.rs 注释与 §6 第 9 条逐句相符，行为零改动）。
- **RV3 回归核对**：core lib 115→118（+3 恰好等于新用例数），其余逐条同值，两处 `1 ignored` 不变。

## Findings（本轮新发现，均 P2 报告项，不阻断）

| ID | 位置 | 问题 | 主 Agent 处理 |
|---|---|---|---|
| RV3-WP7-F1 | `docs/CORE_PORTS_AND_STORAGE.md:797-801` | §6 第 9 条的无条件语气未点明代价：占位释放后该 turn 的迟到尾巴事件仍会按在跑 turn 归属并提交（窗口收窄而非消除） | 修复（wp7-fix3：兜底子项补显式代价句） |
| RV3-WP7-F2 | `broker.rs:843/986/1052` + §6 第 9 条 | 占位期「该会话有 active turn」的客户端可见后果（mode/config.set → `state.version_conflict`；RejectBusy 下 prompt → `session.busy`）未登记、无用例固定（本切片生产装配取 Queue 默认，RejectBusy 分支不可达） | 修复（wp7-fix3：§6 第 9 条补一句 + 可选 core 用例） |
| RV3-WP7-F3 | `broker.rs:255` + `pump` 注释 | 常量 1200 写进合同但无钉死断言；`pump` 注释未提占位期只落盘不派发 | 修复（wp7-fix3：用例补 `assert_eq!(ABANDONED_TURN_HOLD_ROUNDS, 1200)` + 注释补句） |

## Assessment 摘要

- PV1：target 上 `npm run check` EXIT=0（10 道门禁）+ fmt/clippy/workspace 全绿（coder 执行，reviewer 只读核对日志）。
- PV5：约定形式（`node_link_e2e --test-threads=1`）在 target 上已有 coder 的非正式 smoke（2 passed）；plan.md 定义的 PV5 命令与证据文件在本 target 未重新登记——候选门禁前必须补齐（RV2-WP7 记的 PENDING 仍然 PENDING；本轮改了 core 的会话槽语义，旧 PV5 证据对新语义不适用）。不影响本轮静态判断。
- 未执行且无法执行（如实记录）：cargo-deny、gitleaks、任何构建/测试。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.21", role: reviewer, phase: recheck, stage: work-package, target_revision: "5c869160e8dab0361b248124f631beba239183c6", evidence_type: REVIEW, evidence_id: RV3-WP7, report_path: "reports/rv3-wp7.md", result: PASS, evidence_status: NEW, applicability_basis: "RV2-WP7-F1/F2/F3 逐项复核：占位机制正确且无新永卡面，红向证据在 base 690b917 上真实复现 P1 原症状；新增 3 条 P2 报告项不阻断", source_evidence: { id: "RV2-WP7", report_path: "reports/rv2-wp7.md", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" } }
  - { task_id: "2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "5c869160e8dab0361b248124f631beba239183c6", evidence_type: REVIEW, evidence_id: RV3-WP7, report_path: "reports/rv3-wp7.md", result: PASS, evidence_status: NEW, applicability_basis: "R61 断言未在 fix2 改动；既有用例计数只增不减（core 115→118，其余逐条同值）", source_evidence: { id: "RV2-WP7", report_path: "reports/rv2-wp7.md", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" } }
  - { task_id: "2.21/2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "5c869160e8dab0361b248124f631beba239183c6", evidence_type: CHECK, evidence_id: PV1, report_path: "reports/wp7-integration.log", result: PASS, evidence_status: NEW, applicability_basis: "target 上 npm run check EXIT=0（10 道门禁）+ fmt/clippy/workspace 全绿；由 coder 执行、reviewer 只读核对日志", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "5c869160e8dab0361b248124f631beba239183c6", evidence_type: CHECK, evidence_id: PV5, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "PV5 约定形式在 target 只有非正式 smoke；候选门禁前必须按约定形式补齐；不影响本轮静态代码判断", source_evidence: { id: "PV5", report_path: "reports/wp7-integration.log", target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b" } }
```
