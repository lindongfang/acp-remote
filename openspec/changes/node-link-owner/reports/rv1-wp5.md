# RV1-WP5 Review 报告（branch：WP5 catalog 与 resource）

> 说明：本轮 reviewer 子 Agent 只有只读工具，报告由主 Agent 按其返回正文原样持久化。所有行号均针对 Target Revision `6b1f0b9`。

## 结论

- **判定：PASS**（Target `6b1f0b9cb100638eb9e15ca414d5013a86aebaf1`；`watchdog_diff` 确认工作树干净且稳定在该目标提交）。
- **无 P0/P1**：未发现违反 specs R51–R65、依赖方向破坏、unsafe、秘密进日志或证据造假的证据。
- **6 条 P2/MINOR**（全部 report-only 或不阻断的改进，见 Findings）。
- 附带复核：**RV2-WP4-F1/F2 的两处文档修正已在 target 生效**；WP5 handoff 自报的「tasks.md 2.13 仍写 exportIds」已由主 Agent 同步完毕。
- 本 PASS 不覆盖 WP5 的 `[PV5]`、E2E 与候选门禁；R53 的用例缺口见 F1。

## Shared Report

- **task_id**：2.13 / 2.14 / 2.15 / 2.16（WP5）；**role / phase**：reviewer / review。
- **Review ID / Type / Stage**：RV1-WP5 / branch / 工作包交付前（WP5）。
- **agent_context**：新建独立 reviewer 子 Agent（b05e7d1b-9990-48f5-8b70-7fb302beecda），未参与 WP5 实现或修复；只读工具集（无 shell、无 Git、无写文件），不继承实现对话。
- **Repository**：`D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。
- **base / target**：`2937850f607ea54792a55649a82b7f6878808830` / `6b1f0b9cb100638eb9e15ca414d5013a86aebaf1`。
- **scope（实际检查）**：`node_link/{catalog.rs,resource.rs,resource/tests.rs,mod.rs}`、`conn/registry.rs`（`send_with_frame`）、`core/{use_cases,ports,broker}.rs` 的 Node Link seam 与发布顺序、`storage-sqlite/src/{session_store.rs,admin/audit.rs,migrate.rs}`、`conn/{session.rs,limits.rs}`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/NODE_LINK_PROTOCOL.md` §8.2/§12.3/§12.4、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1、`schemas/node-link/v1/common.schema.json`、specs R51–R65、design D4/D5/D6/D14、plan.md、tasks.md、verification.md、`reports/wp5-handoff.md`、`reports/wp5-catalog-resource.log`、`reports/pv5-windows-nodelink.log`（3.7 轮）、`reports/rv1-wp4.md`、`reports/rv2-wp4.md`。
- **result**：PASS（含 6 条 P2）；**resource_cleanup**：无写入、无资源占用。

## 逐条需求符合性（R51–R65）

| 需求 | 结论 | 依据（target） |
|---|---|---|
| R51 catalog 投影 | 实现符合，**用例缺口**（F1） | `catalog.rs:47-96`（每次现算、失败关闭）、`:103-166`（chunks 切分、批次夹在 [MIN,500]）、`:325-405`（全字段 + 零参数 template 失败关闭）；`knownRevision` 只记日志 |
| R52 按信任记录过滤（grant 交集） | 符合 | 唯一判定点 `catalog.rs:260-283`（`visible_exports`：Paired + 未撤销 + 交集、按 exportId 升序）；catalog 与 resource 共用；用例存在 |
| R53 超大批次稳定切分 | **未验证**（F1） | 切分逻辑存在但无路由级用例 |
| R54/R55 attachment 与 generation 隔离 | 符合 | `resource.rs:304-390`、`current_attachment` 精确匹配；用例 `resource/tests.rs:579-623` |
| R56 未导出会话不可 attach | 符合 | `resource.rs:311-330`（not_granted/not_found）+ use_cases 归属前置 |
| R57/R58 快照无正文 + digest 原始字节 + 并发 1 | 符合 | items 只有两类元数据；digest 用 `send_with_frame` 取实际入队帧文本；并发 1 由单循环内联分派保证 |
| R59 epoch 不符 → sequence_invalid | 符合 | `use_cases.rs:895-937` + attach_fault 映射；用例 `resource/tests.rs:809-880` |
| R60 事件保真 + origin 三元组 + ACPR-CJ1 digest | 符合 | `resource.rs:909-975`；`acpr_wire::cj1::canonicalize` 唯一实现 |
| R61 先持久化后发布 | 结构成立，端到端待 WP7（F2） | broker 只在 commit 成功后 publish；本层不产生事件 |
| R62 未登记事件类型保留 raw | 符合 | view/acp 原样转发，不丢弃不文本化（登记判定属生产者侧） |
| R63 超大内嵌显式降级 | 符合 | >256 KiB → `rawUnavailable(size_limit)` 带 byteLength/sha256，不截断 |
| R64/R65 ACK 归属 + epoch + 单调 | 符合 | `resource.rs:663-712`：归属 → epoch → 单调（回退拒绝且不改写水位） |

## Findings

| ID | 级别 | 位置 | 问题 | 主 Agent 裁决 / 处理 |
|---|---|---|---|---|
| RV1-WP5-F1 | P2 | `catalog.rs` 路由零路由级用例 | R51 的路由行为与 R53 的切分无测试（完成条件「PV3 覆盖 R51–R53」对 R53 不成立） | **修复（wp5-fix1）**：补 3 条路由级用例（E2 不相交只见 E1、批次=2 可见 3 条→两帧稳定、空集一帧空快照） |
| RV1-WP5-F2 | P2 report-only | `resource.rs:147-212` | R61 只有结构保证（手工构造的已持久化事件），端到端「提交失败→连接收不到」未行为化 | verification 的 2.15/R61 记为**部分覆盖**；端到端断言列入 WP7（2.21/2.22）派发输入 |
| RV1-WP5-F3 | P2 | `resource.rs:97,222-256` + `conn/session.rs:932-934` | `states` 表对正常结束的连接不回收（只在扇出失败时 forget）→ 表随时间增长、每事件扫描成本线性 | **修复（wp5-fix1）**：`subscribers()` 内顺带回收不在 `registry.handles()` 的 key |
| RV1-WP5-F4 | P2 report-only | `resource.rs:304-390,977-1013` | `ownerNodeId` 未校验且被原样回显（复合身份的 owner 分量可由对端任意指定） | **修复（wp5-fix1）**：attach/ack 比较 `remote.owner_node_id` 与本机 node id，不等即 `export.not_found` |
| RV1-WP5-F5 | P2 report-only | `resource.rs:193-195,465-467,580-591,638-644` | 四处映射失败静默跳过，无结构化日志 | **修复（wp5-fix1）**：补 `warn!`（字段与既有 `node_link.fanout_event_invalid` 对齐） |
| RV1-WP5-F6 | P2 doc | `CORE_PORTS_AND_STORAGE.md:217-219` | 「三条窄 seam」与随后四个用例入口计数不一致 | **修复（wp5-fix1）**：改写为「三条端口 seam 与它们的四个用例入口」 |

## 已核验为「非问题」的点（摘要）

1. `send_with_frame` 是纯加法（`send` 委托同一实现；digest 前像 = 对端收到的字节）。
2. ACPR-CJ1 无第二实现。
3. `snapshotDigest` 规则正确（只对 chunk 帧、按序连接、数量校验后才发 end）。
4. R58 单快照并发=1 由单循环内联分派证明。
5. core seam 窄授权与零写入（node 绑定在用例层；无通用读放开）。
6. 文档与门禁同步齐备（§4 两条 [决定] 含 watermark 精度边界、§5.2/§5.3 签名）。
7. watermark 用 `sqlite_sequence`（重建时刻意回填，跨重启与清理仍单调）；握手两侧同源。
8. `RawAcp::Complete` 原文逐字节可还原；`media_type != application/json` 分支生产不可达。
9. `ensure_digestable` 的降级是防御性的（写入时已施加 CJ1），降级摘要仍与发出字节一致。
10. 无 unsafe / 无秘密进日志 / 唯一 `expect` 构造期不可达且有说明。
11. 平级调用纪律（catalog/resource 不互调；Routes 只按序询问；check:boundaries 通过）。
12. RV2-WP4-F1/F2 附带复核已闭环。
13. handoff 自报的 tasks.md 不同步已过时（主 Agent 已同步）。
14. `SliceStore` 替身不比真实存储宽容。

## Assessment 与待补证据

- [PV3]：coder 证据已核（日志自洽）；reviewer 未复跑；交付前在新提交重跑。
- [PV5]：PENDING（WP5 尚无轮次；归 2.22/3.9，候选合入前）。
- R53 行为覆盖：PENDING（= F1）。
- R61 端到端顺序断言：PENDING（= F2，归 2.21/2.22）。

## 残余风险（交主 Agent 记录）

1. 扇出队列满即丢（EVENT_QUEUE_CAPACITY=1024 全连接共享）：在线订阅者在不重连情况下会静默缺一段事件（v1 不发 link.backpressure）——D6/G4 既定取舍，WP7 集成测试显式记录。
2. `resource.ack` 无频率限制（每次 ACK 触发一次读）——合同无 ACK 限流条款，仅登记。
3. F3 的状态表增长与 F4 的 owner 分量回显属本层设计边界，WP6 复用形状时会一并继承。
4. L1：本轮无 diff 工件；如需 diff 级形式化核对需另派有 Git 权限的执行者。

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.13", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: REVIEW, evidence_id: RV1-WP5, report_path: "reports/rv1-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "R51/R52 的 grant 交集口径、唯一判定点与 R53 用例缺口（F1）已逐条 grounded；[PV5] 记 PENDING", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.14", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: REVIEW, evidence_id: RV1-WP5, report_path: "reports/rv1-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "R54–R59 逐条代码定位 + 用例名核对；R58 并发=1 由单循环内联分派证明；send_with_frame 加法的不变性已复核", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.15", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: REVIEW, evidence_id: RV1-WP5, report_path: "reports/rv1-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "R60/R62/R63/R64/R65 代码与用例逐条核对；R61 仅结构保证（F2，端到端归 2.21/2.22）", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.16", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: REVIEW, evidence_id: RV1-WP5, report_path: "reports/rv1-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "[PV3] 日志自洽性已核（83 目标、server lib 242、fmt/clippy/npm check 退 0、unsafe 0）；reviewer 未复跑", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV2-WP4-F1/F2", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: REVIEW, evidence_id: RV1-WP5, report_path: "reports/rv1-wp5.md", result: PASS, evidence_status: NEW, applicability_basis: "附带复核：CORE_PORTS §4 三条窄入口与 IDENTITY §5.1 的 record_node_connected 均在 target 生效，原 P2 已闭环", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.13/2.14/2.15 (R51–R61 的 [PV5] 轮次)", role: reviewer, phase: review, stage: work-package, target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1", evidence_type: CHECK, evidence_id: PV5, report_path: "NOT_AVAILABLE", result: BLOCKED, evidence_status: PENDING, applicability_basis: "WP5 的 [PV5] 轮次尚不存在；待补门禁：2.22 / 3.9 候选合入前", source_evidence: NOT_APPLICABLE }
```
