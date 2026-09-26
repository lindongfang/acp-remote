# RV2-WP5 Review 报告（recheck：WP5 catalog 与 resource 的 fix1）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## 结论

**判定：PASS**（Target Revision `27ad3f800c7821846b8991ae7d61b67f59e8ba24`；`watchdog_diff stat` 确认 reviewer 启动时 HEAD 即该提交且工作树无改动）。无 CRITICAL/MAJOR；1 条 MINOR（报告项）+ 1 条 SUGGESTION；本轮不覆盖 [PV5]/E2E/候选门禁。

## 逐项复核结论（按原问题 ID）

| 原 ID | 结论 | 复核依据（target `27ad3f8`） |
|---|---|---|
| RV1-WP5-F1 | **已解决** | `catalog.rs:600-730` 三条路由级用例均非空壳（过滤/两帧稳定/空集）；6 条红向 panic 行号与目标文件逐一吻合 |
| RV1-WP5-F3 | **部分解决**（残余见 RV2-WP5-F1） | `subscribers()` 遍历中登记 ended 并 remove；红向证据成立。但「attach 后未订阅即断开」的条目仍无回收路径 |
| RV1-WP5-F4 | **已解决** | attach → `export.not_found` 且不签发；ack → `sequence_invalid` 且不记账；与 §12.4 line 559 逐字一致；主 Agent 裁决 (A) 与文档字面一致；回显恒为本机 id |
| RV1-WP5-F5 | **已解决** | 四处全部有结构化 `warn!`（fanout_payload_invalid / snapshot_pending_interaction_unmappable + snapshot_items_omitted / replay_event_*）；字段仅标识与计数，无 payload、无秘密 |
| RV1-WP5-F6 | **已解决** | §4 改为「三条端口 seam 与它们的四个用例入口」，计数与代码逐条对齐 |

## Findings（本轮新发现）

| ID | 级别 | 位置 | 问题 | 主 Agent 处理 |
|---|---|---|---|---|
| RV2-WP5-F1 | MINOR（P2） | `resource.rs:235-265`、`:373-390` | F3 修复的回收只覆盖「有本会话订阅」的 key；「attach 后未订阅即断开」与「re-attach 清订阅后断开」的条目仍无回收路径 | **修复（wp5-fix2）**：`subscribers()` 遍历 `states` 时凡 key 不在 `registry.handles()` 即登记回收（放在订阅判定之前） |
| RV2-WP5-F2 | SUGGESTION | `resource.rs:762` | `let Some(attachment) = ... else {...}; let _ = attachment;` 绑定未使用 | **修复（wp5-fix2）**：改为 `is_none()` 提前返回形态 |

### 非问题核对（摘要）

- `ResourceRoute::new` 三参签名无其它调用者；`node_id` 与握手 `ownerNodeId` 同源（`authority.local_node()`）。
- 注册表 `register/unregister` 只在认证后与关闭路径调用，「不在注册表 = 已结束」判据安全。
- 各测试断言前提（空快照、`DecimalString` 序列化、`NoContentCache`、`GrantSet` 字典序）逐一在协议/核心代码核实。

## Assessment 与待核对证据

- [PV3]：wp5-fix1 轮次自洽（fmt/clippy/server 248/npm run check 全绿 + 6 条红向）；reviewer 未复跑；候选门禁前由 Project Verify 在 `27ad3f8` 重跑。
- [PV5]：PENDING（归 2.22/3.9），不影响本轮代码判断。
- 限制（L1）：无 Git/shell，diff 级证明未做（以目标文件内容 + 行号对齐 + 全仓符号搜索替代）。

## 残余风险（交主 Agent 记录）

1. RV2-WP5-F1 的残留增长（wp5-fix2 处理）。
2. `ResourceRoute::new` 第三参数必须传本机 node id——WP7 集成期风险（传错则全部 attach/ack 被拒），列入 WP7 派发输入。
3. F5 的四处 `warn!` 无自动化回归保护（v1 无日志捕获设施）——登记。
4. `resource.ack` 无限流、扇出队列满即丢为既有登记项。
5. 可选补强：`catalog.subscribe` 的 `knownRevision` 非空分支无路由级用例（实现已无条件返回完整快照，属覆盖增强）——列入 WP6 观察项。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.13", role: reviewer, phase: recheck, stage: work-package, target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24", evidence_type: REVIEW, evidence_id: RV2-WP5, report_path: "reports/rv2-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "F1 三条路由级用例逐条核对为真实断言（含红向行号吻合）；F6 文档计数句与 §5 seam 逐条对齐", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.14", role: reviewer, phase: recheck, stage: work-package, target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24", evidence_type: REVIEW, evidence_id: RV2-WP5, report_path: "reports/rv2-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "F4 两个错误码与 §12.4 line 559 逐字一致；新签名无其它调用者", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.15", role: reviewer, phase: recheck, stage: work-package, target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24", evidence_type: REVIEW, evidence_id: RV2-WP5, report_path: "reports/rv2-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "F5 四处 warn 落地且不含正文/秘密；F3 回收在正常结束路径成立（部分解决，残余记 RV2-WP5-F1）", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.16", role: reviewer, phase: recheck, stage: work-package, target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24", evidence_type: CHECK, evidence_id: PV3, report_path: "reports/wp5-catalog-resource.log", result: PASS, evidence_status: REUSED, applicability_basis: "coder 日志自洽且行号吻合；reviewer 未复跑；候选门禁前由 Project Verify 在 27ad3f8 重跑", source_evidence: { id: "PV3", report_path: "reports/wp5-catalog-resource.log", target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24" } }
  - { task_id: "2.13/2.14/2.15 (R51–R61 的 [PV5] 轮次)", role: reviewer, phase: recheck, stage: work-package, target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24", evidence_type: CHECK, evidence_id: PV5, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "本轮无 [PV5] 证据；待补门禁：2.22 / 3.9 候选合入前", source_evidence: NOT_APPLICABLE }
```
