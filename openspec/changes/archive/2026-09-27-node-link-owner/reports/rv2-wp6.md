# RV2-WP6 Review 报告（WP6 修复轮次 wp6-fix1 复核，工作包交付前）

> 说明：本轮 reviewer 子 Agent 只读检视（无 shell/Git/写文件工具），报告由主 Agent 按其返回正文原样持久化。

## 结论

**判定：PASS**（Review ID `RV2-WP6`；Target Revision `f630859fec505773995adb5f6cf800c4c2da3115`；reviewer 启动时 HEAD 即该提交且工作树/暂存区均无改动）。

- **RV1-WP6-F1（原 P1 阻断）：已解决**。`session.create` 的幂等键与终态现在落在 `owned_command`（创建提交同事务回填 `session_id`），进程内幂等表整体删除；重启后同 requestId 重试回首次 sessionId、崩溃窗口由启动恢复终结为 `uncertain`、同键不同指纹 → `idempotency_conflict`、`command.status` 重查 `terminalEventId` 非空，均在 core / storage-sqlite / server 三层代码与用例中核实（含真实 SQLite 重开存储的「重启」路径）。
- **F2 / F3 / F6 / F8 / F10 / F11 / F12：已解决**。
- **回归**：既有用例计数只增不减（server lib 276→281、core lib 108→112、router 35→35、storage +2；合计 +11 与日志 953 一致）。
- **新发现**：4 条 P2（report-only，不阻断），无 P0/P1。
- **待补证据**：`[PV5]`/E2E（候选门禁 3.11 / 2.22）仍未返回——按计划属候选阶段，不影响本轮静态判断。

## Shared Report

- **task_id**：2.17 / 2.18 / 2.19 / 2.20（复核对象 RV1-WP6-F1/F2/F3/F6/F8/F10/F11/F12）；**role / phase**：reviewer / review（recheck）
- **agent_context**：独立 reviewer 子 Agent（1ccf40bc-d125-4523-955b-dd1418e82733；新建、不继承实现对话；只读、无 shell/Git、无写文件工具）
- **Repository**：`D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）
- **base / target**：`8bc2ff3e078549cebed2ab99b5981d5ed37e0581` / `f630859fec505773995adb5f6cf800c4c2da3115`
- **scope**：`crates/core/src/{broker.rs,use_cases.rs}`、`crates/storage-sqlite/src/session_store.rs` 与 `tests/commit.rs`、`crates/server/src/node_link/command.rs` 与 `command/tests.rs`、`crates/server/src/local_admin/{router.rs,test_support.rs}`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/NODE_LINK_PROTOCOL.md` §12.7
- **result**：**PASS**（0 条未解决阻断项；4 条 P2 report-only）
- **evidence_paths**：`reports/wp6-fix1-handoff.md`、`reports/wp6-command.log`（§wp6-fix1 轮次）、`reports/rv1-wp6.md`、目标版本源码
- **resource_cleanup**：无资源创建、无写入
- **限制**：L1 无 Git/shell（目标修订全文 + 与 RV1 基线逐条对照 + 修复交接说明，不是补丁行级 diff 复核）；L2 未执行任何命令（[PV3] 结论为日志核对与复用）；L3 用例计数按源码逐文件点数与日志交叉核对。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.17", role: reviewer, phase: review, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: REVIEW, evidence_id: RV2-WP6, report_path: "reports/rv2-wp6.md", result: PASS, evidence_status: NEW, applicability_basis: "只读复核目标修订：F1 的持久幂等/终态/崩溃窗口/重查 terminalEventId 与 F3 的终态 result 分量逐条落到代码与用例", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.18", role: reviewer, phase: review, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: REVIEW, evidence_id: RV2-WP6, report_path: "reports/rv2-wp6.md", result: PASS, evidence_status: NEW, applicability_basis: "session.create 四键/零参数 template/不创建会话语义未变；幂等与终态面（F1）已由持久记录承担", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.19", role: reviewer, phase: review, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: REVIEW, evidence_id: RV2-WP6, report_path: "reports/rv2-wp6.md", result: PASS, evidence_status: NEW, applicability_basis: "F10 的观察点改为通知当时回读持久撤销状态，顺序断言锚在持久事实上", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.20", role: reviewer, phase: review, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: CHECK, evidence_id: PV3, report_path: "reports/wp6-command.log", result: PASS, evidence_status: REUSED, applicability_basis: "日志 §wp6-fix1 最终门禁段记录 HEAD=f630859、工作区干净、全 exit=0；reviewer 未独立复跑，候选门禁前须由 Project Verify 在同一提交重跑", source_evidence: { id: "PV3", report_path: "reports/wp6-command.log", target_revision: "f630859fec505773995adb5f6cf800c4c2da3115" } }
  - { task_id: "2.17/2.18/2.19（R66–R78 的 [PV5] 轮次）", role: reviewer, phase: review, stage: work-package, target_revision: "f630859fec505773995adb5f6cf800c4c2da3115", evidence_type: CHECK, evidence_id: PV5, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "本轮仍无 [PV5]/E2E；待补门禁：候选合入前 3.11 / 2.22", source_evidence: NOT_APPLICABLE }
```

## Findings

### 复核结论（按原问题 ID）

| 原 ID | 结论 | 复核依据（target `f630859`） |
|---|---|---|
| RV1-WP6-F1（P1） | **已解决** | 幂等键落 `owned_command`（创建提交同事务回填 `session_id`）；进程内表整体删除（grep 零命中）；重放不二次创建；崩溃窗口 → `uncertain`（recover_unsettled 路径）；冲突 → `idempotency_conflict`；重查 `terminalEventId` 非空；红向证据（去回填 → 存储用例失败；短路幂等 → 重启后重复创建） |
| RV1-WP6-F1 附带（MAX_TRACKED 注释/淘汰语义、settle 原子性） | **已解决** | 常量随表删除；幂等行回填 + write_command + INSERT owned_session 同一 BEGIN IMMEDIATE；apply_terminal 条件更新 |
| RV1-WP6-F2 | **已解决** | `poll_pending` 首行 `forget_gone_connections()` 按注册表存活集合回收；红向用例存在 |
| RV1-WP6-F3 | **已解决（含已登记口径边界）** | 终态记录带收据分量（turnId/version）；accepted.result 透传收据 turn；session.create 恒 null 符合 schema if/then；重查未终结不带 turnId 已登记 |
| RV1-WP6-F6 | **已解决** | §12.7 末段新增三查询 unsupported 注记 + 修订记录 |
| RV1-WP6-F8 | **已解决** | 永久失败映射 `nodelink.command.unsupported`（retryable=false）、`command.uncertain` → `nodelink.command.uncertain`；正反用例锁定 |
| RV1-WP6-F10 | **已解决** | RecordingCloser 回读持久撤销状态并断言 |
| RV1-WP6-F11 | **已解决** | 死绑定删除，grep 零命中 |
| RV1-WP6-F12 | **已解决** | Shutdown 退出 + 超龄放弃两条用例（含红向） |

### 新发现（本轮，均 P2 report-only 不阻断）

| ID | 位置 | 问题 | 主 Agent 处理 |
|---|---|---|---|
| RV2-WP6-F1 | `command.rs:815-836` 注释 + spec R66 | 「幂等行落盘前失败」窗口的注释把瞬态写失败与确定类失败并列；该窗口下同键重试可得不同结果、重查回 not_found | **修复（wp6-fix2）**：注释收窄为确定类失败 + R66/§12.7 补一句该窗口语义 |
| RV2-WP6-F2 | `core/broker.rs:1343-1375` | `settle_session_create` 不校验 `record.command() == "session.create"`（仅适配层误用可达，wire 不可达） | **修复（wp6-fix2）**：加一行 guard（InvalidRequest） |
| RV2-WP6-F3 | `command.rs:1965-1970` | `state.version_conflict` 的 retryable 由 true 翻转为 false（RV1-WP6-F8 裁决的有意收窄） | 记录为已裁决取舍，不改码；切片 6 如需区分须增 registry 码（跨协议变更，另行裁决） |
| RV2-WP6-F4 | `crates/app/**` 无 `CommandRoute`/`.dispatch(` 引用 | 终态观察循环与回收在真实 Daemon 中尚未被 spawn（WP7 接线） | 列入 WP7 硬门禁（与 RV1-WP6-F9 同类） |

### Correct 与回归核对（摘要）

- 幂等键隔离正确；授权顺序未退化（先授权后幂等判定与创建）；终态形状未退化；恢复路径不重放副作用；DDL 约束与本修复一致；文档一致（§4/§5.1/§6 第 20 条、§12.7 注记）；无敏感信息泄漏；无 unsafe。
- 回归：各 crate 用例计数只增不减（Δ=+11 与日志 953 一致）。
- 残余风险登记：① F1 的窗口语义（随 wp6-fix2 文本化）；② F3 的「重查不带 turnId」边界；③ `view_command_completed` 把 version 写进 Sync 事件 view 的 `result.turnId` 的并存投影（交 Sync 切片裁定）；④ WP7 接线门禁（spawn dispatch + 替换 RevocationCloser）；⑤ 预持久化失败的 session.create 会留无端点会话行（既有行为，随 WP7/agent-host 复核）。
