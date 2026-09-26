# RV2-WP7 Review 报告（WP7 fix1 修复复核，工作包交付前）

> 说明：本轮 reviewer 子 Agent 只读检视（无 shell/Git/写文件工具），报告由主 Agent 按其返回正文原样持久化。

## 结论

**判定：FAIL**（Target Revision `690b91722f6ef1306660fde55ce1afd0d4607f4b`，工作树干净）。

逐项复核：
- **RV1-WP7-F1（vacuous 用例）→ 已解决**（debug 级 + 阳性对照，判据非恒真）。
- **RV1-WP7-F2（停 accept 时机与日志时机）→ 已解决**（同步 `trigger_shutdown` + 真实探针用例 + 红向证据）。
- **RV1-WP7-F3（非终态批失败静默丢弃）→ 主形态已解决，但新机制引入一条相邻可达路径（RV2-WP7-F1，P1）**：`turn.failed`/`command.uncertain`/已落盘 delta 的 `completed` 同批提交、被放弃 turn 的后续事件丢弃、与 §6 第 9 条一致、无第二个持久权威写入者——这些都对；但被放弃 turn 的**迟到终态在「下一个 turn 已开始运行」时会被记到下一个 turn 上**，可让下一个命令被误报 `completed` 且其正文被丢弃，与同一提交写下的 §6 第 9 条「后续事件（含终态）不再提交」矛盾。
- **RV1-WP7-F4（复刻私有常量）→ 已解决**（`identity.key().clone()`；`NODE_KEY_LABEL` 全仓只剩 `app/src/identity.rs` 一处）。
- **回归**：既有用例计数只增不减（core 113→115、daemon_lifecycle 10→11，其余逐条同值，含 2 处 `1 ignored` 不变）。

## Findings

| ID | 级别 | Location | 问题 | 主 Agent 裁决 / 处理 |
|---|---|---|---|---|
| RV2-WP7-F1 | **P1（本 diff 引入的可达路径）** | `core/src/broker.rs:1606-1616`（归属兜底：先 running 后 abandoned.last()）+ `:1993`（abandon 里 `finish_turn()`）+ `:1544-1551`（pump 后立即 dispatch） | 被放弃 turn 的迟到终态在「下一个 turn 已在跑」时被记到下一个 turn：T2 可被误报 `completed` 且正文被丢弃；违反「一个状态只能有一个权威写入者」与 §6 第 9 条新承诺 | **修复（wp7-fix2，按建议 (a)）**：被放弃的 turn 继续占住会话槽直到其终态被端点观测到（`dispatch_one` 在此前不派发下一个排队 turn；其事件一律按 abandoned 归属丢弃）；配兜底（端点取消/会话关闭/超时）避免会话永久卡住；补 core 回归用例（T1 放弃 → 派发 T2 → 迟到 turn.completed → T2 不得出现 completed 且其事件不被丢弃） |
| RV2-WP7-F2 | P2 | `app/tests/support/owner.rs:275-278` | 测试替身注释与修复后语义不符（终态批不再被提交） | 修复（wp7-fix2：改写注释） |
| RV2-WP7-F3 | P2 | `docs/CORE_PORTS_AND_STORAGE.md:828`（§6 第 19 条） | §19 的「无归属降级」与第 9 条新增的「归属到被放弃 turn 并丢弃」对同一事件类给不同后果 | 修复（wp7-fix2：§19 补交叉引用 + 第 9 条点明有意更强处置 + 补放弃场景用例） |

### 报告性观察（摘要）

1. 被放弃 turn 的交互请求会被丢弃（permission/elicitation）——建议在端点取消/会话关闭路径上收敛或合同登记（随 wp7-fix2 的兜底设计一并考虑）。
2. 放弃提交自身再次 Unavailable 时内存领先于库——与既有 record_uncertain 同口径，不构成新的权威冲突。
3. e2e 新断言有 500 ms 时间窗（负载高时敏感）——候选门禁重跑 [PV5] 时留意。
4. NetIngress 关闭语义无矛盾（触发提前、预算不变）。

## Assessment（摘要）

- F1/F2/F4 已解决且无回归；F3 主形态正确；RV2-WP7-F1（P1）使 §6 第 9 条的无条件承诺不成立。
- 回归核对：逐 test binary 计数只增不减（core +2、daemon_lifecycle +1）。
- [PV5] 在 target 上只有 workspace 并行轮次（约定形式未重跑）——PENDING，候选门禁前补齐。
- [PV1] 在 target 上 npm run check EXIT=0（10 道门禁，日志证据）。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.21", role: reviewer, phase: recheck, stage: work-package, target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b", evidence_type: REVIEW, evidence_id: RV2-WP7, report_path: "reports/rv2-wp7.md", result: FAIL, evidence_status: NEW, applicability_basis: "F1/F2/F4 已解决且无回归；F3 主形态落地但发现 RV2-WP7-F1（P1）", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b", evidence_type: REVIEW, evidence_id: RV2-WP7, report_path: "reports/rv2-wp7.md", result: FAIL, evidence_status: NEW, applicability_basis: "R61 断言改钉新语义后仍覆盖「提交失败 → 连接不出现该事件」；F3 的相邻路径无用例覆盖", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b", evidence_type: CHECK, evidence_id: PV5, report_path: "reports/wp7-integration.log", result: BLOCKED, evidence_status: PENDING, applicability_basis: "[PV5] 约定形式未在 690b917 重跑；候选门禁前必须补齐", source_evidence: { id: "PV5", report_path: "reports/wp7-integration.log", target_revision: "a3109c8a80ec806cd7899b72709ec07e579cec28" } }
  - { task_id: "2.21/2.22", role: reviewer, phase: recheck, stage: work-package, target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b", evidence_type: CHECK, evidence_id: PV1, report_path: "reports/wp7-integration.log", result: PASS, evidence_status: NEW, applicability_basis: "target 上 npm run check EXIT=0（10 道门禁）", source_evidence: NOT_APPLICABLE }
```
