# RV1-WP3 Review 报告（变更 `node-link-owner` / WP3 配对 HTTP 全量）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告按其返回正文原样持久化（含其 handoff_index 与全部发现）。

**结论：PASS**（Target Revision `d127a802b75d63cd14789ea3ad504d89ce57f3f3`）。无 CRITICAL/MAJOR；5 项非阻断发现（4 × P2 + 1 × P2 report-only）；附带复核项 RV2-WP2FIX-F1/F3、RV1-WP3C-F2/F3/F5 全部判定**已解决**。

## Shared Report

- **task_id**：WP3 全量（2.25、2.26、2.27、2.7、2.8、2.9）+ DU1
- **role**：reviewer（独立只读检视；未参与实现、未参与修复对话）；**phase**：review；**stage**：work-package
- **agent_context**：任务级 reviewer 子 Agent（ba4cb688-1799-4772-b203-825fd70117c5；隔离上下文启动，未继承任何实现对话）；工作目录 `D:\Project\acp-remote-wt\node-link-owner`；未修改任何文件、未切换分支、未提交、未创建端口/进程/临时目录
- **base / target**：`041aeb043d7b585b43faaaf1f40a330854b80784` / `d127a802b75d63cd14789ea3ad504d89ce57f3f3`
- **版本稳定性**：`watchdog_diff` 报工作区相对 `d127a802` 无任何改动 → 读到的文件内容 = target revision
- **scope（实际检查范围）**：`crates/server/src/node_link/{mod,pairing,tests}.rs`（逐行）、`transport/net/{route,listener,ratelimit,http,proxy,mod}.rs` 与 `net/tests.rs`、`crates/identity-auth/src/{pairing,types,error}.rs` 与 `tests/pairing.rs`、`crates/core/src/use_cases.rs`、`crates/core/src/model/identity.rs`、`crates/core/src/broker.rs`、`crates/storage-sqlite/src/admin/trust.rs`、`migrate.rs` 与 `tests/migration.rs`、`crates/app/src/compose.rs`；文档 `docs/CORE_PORTS_AND_STORAGE.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md` §4.9、`docs/NODE_LINK_PROTOCOL.md` §2.5/§9/§13；规划/证据 specs/design/plan/tasks/verification 与各报告
- **checks**：本轮未执行任何命令（只读检视）。[PV3] 的 coder 日志逐项静态核对（两轮：84678e4 与 d127a802）；[PV5] 本 WP 无 cfg(windows) 分支；[PV1]/[PV2] 属 3.5/候选门禁
- **issues**：0 × CRITICAL、0 × MAJOR；5 条非阻断。未发现 specs 需求违反、依赖方向破坏、秘密进日志/错误/Debug、unsafe、证据造假
- **result**：**PASS**（只对应 d127a802；不代表 PV1/PV2/PV5 正式轮次、WP4/WP7 接线、E2E 或合并已完成）
- **evidence_paths**：本报告 + `reports/wp3-pairing-http.log`、`reports/wp3-handoff.md`、`reports/wp3-fix1-handoff.md`、`reports/wp3-contract.log`、`reports/rv1-wp3c.md`、`reports/rv2-wp2fix.md`
- **resource_cleanup**：无资源创建、无写入、无端口/进程/临时目录；未安装依赖、未改锁文件

## handoff_index

```yaml
handoff_index:
  - { task_id: "2.27", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "在 target 上核对 identity-auth 两个新公开入口与 core 绑定只读入口的行为与登记（§5.1/CORE_PORTS §4/§10）；发现 2 项文档口径失配（F2/F3），不阻断", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.7", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "在 target 上逐条核对 claim 端点：限流→Content-Type→解码→只读→410/409→403→verify_claim→写集/幂等→201 的顺序与 §13.4/R19-R25 一致；401 路径单一形状；storage 事务内重做守卫", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.8", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "status 一律 200 + 五状态、401 收敛、nonce 重放缓存、两份限流器语义核对（同窗口语义）；终态/secret 已清除读法与 spec 文本的张力记为 F1", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.25", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: REUSED, applicability_basis: "core 侧整体检视复用 RV1-WP3C（原 target 13f0a16b）；13f0a16→d127a802 的增量未改任何端口签名、DDL、枚举与状态机代码；本轮另独立重读 require_pairing_access、pairing_channel_view、consume_pairing 与 storage claim 守卫", source_evidence: {id: "RV1-WP3C", report_path: "reports/rv1-wp3c.md", target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"} }
  - { task_id: "2.26", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: REUSED, applicability_basis: "v2→v3 迁移与 §5/§7 合同一致性复用 RV1-WP3C；增量是 §7.2 步骤 4 精度改写（F3 修复），本轮独立核对 migrate.rs:954-988 与文档新措辞逐字一致", source_evidence: {id: "RV1-WP3C", report_path: "reports/rv1-wp3c.md", target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"} }
  - { task_id: "2.9", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "交付形状与证据链静态核对：WP3 冻结公开面与代码一致；coder 日志两轮计数与红向判别力实验自洽；本轮不执行测试", source_evidence: NOT_APPLICABLE }
  - { task_id: "2.9", role: reviewer, phase: review, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: CHECK, evidence_id: PV3, report_path: NOT_AVAILABLE, result: BLOCKED, evidence_status: PENDING, applicability_basis: "[PV3] 的正式独立轮次归 3.5/3.10 与候选门禁 6.x；reviewer 只读、未执行命令", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV2-WP2FIX-F1", role: reviewer, phase: recheck, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "饱和用例存在、按生产常量填满池（不写死槽位数）、注入 500ms 超时并写明 5s vs 32s 的判别力 → 已解决", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV2-WP2FIX-F3", role: reviewer, phase: recheck, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "accept 的 None 分支已写明不可达依据，drop(permit) 承重注释在 → 已解决", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP3C-F2", role: reviewer, phase: recheck, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "CORE_PORTS §3.5 已把 Actor::Node.node 钉死为对端节点 id（与 via_node、ImportRecord.owner_node_id 同口径）→ 已解决（WP4 仍须按此接线）", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP3C-F3", role: reviewer, phase: recheck, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "§7.2 步骤 4 已改为不单独落盘；migrate.rs:954-988 两段重建同事务、末尾统一写 3 → 已解决", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP3C-F5", role: reviewer, phase: recheck, stage: work-package, target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3", evidence_type: REVIEW, evidence_id: RV1-WP3, report_path: "reports/rv1-wp3.md", result: PASS, evidence_status: NEW, applicability_basis: "migration.rs 头注释已改为「用 empty.sqlite3 的临时副本判定」 → 已解决", source_evidence: NOT_APPLICABLE }
```

## Findings

| ID | 级别 | 位置（target `d127a802`） | 触发 / 证据 | 影响 | 建议（最小修复） | Recheck |
|---|---|---|---|---|---|---|
| RV1-WP3-F1 | P2（report-only） | `node_link/pairing.rs:399-408` vs spec R26/R28 | status 在本机已不持有 secret 时：终态回 200 + 状态、非终态回 401——R26「一律 200」与 R28「proof 无效 401」在该情形下无法同时字面成立 | 行为是唯一可行读法；风险是归档后主规范与实现不一致 | 主 Agent 已选 (a)：spec R26 补 secret 生命周期分支文本 + R28 收窄 + 新增场景（Coverage R88）；`NODE_LINK_PROTOCOL.md` §13.3 同步一句澄清（wp3-fix2） | 已随 spec 修订闭环 |
| RV1-WP3-F2 | P2 | `IDENTITY_AND_AUTH_CONTRACT.md:351` vs `:352`（自相矛盾；design.md D12 同款） | 「只能在 proof 验证成功后构造」与「verify_claim 之前可先用绑定 claimant 只读」并存；实现是先读后验 | 合同内部矛盾；后续切片会挑错口径 | 修复（wp3-fix2：:351 收窄 + :352 末句改为 status 先读后验 + design.md 同步） | wp3-fix2 后复核 |
| RV1-WP3-F3 | P2 | `IDENTITY_AND_AUTH_CONTRACT.md:304-307`、`:316` vs `pairing.rs:404` | `has_secret` 登记为「测试/诊断」，但 status 端点生产路径用它决定 secret 生命周期分支 | 公开面登记与事实不符 | 修复（wp3-fix2：§5.1 既有条目补一句生产用途） | wp3-fix2 后复核 |
| RV1-WP3-F4 | P2 | `node_link/tests.rs:967-978` 与文件头覆盖表 | 用例注释声称覆盖 7 个状态码，实际只发 4 个请求且不断言状态码 | 注释与断言名不副实（实际需求覆盖未丢失：各状态码专测均调 `assert_security_headers()`） | 修复（wp3-fix2：注释改如实 + 补 4 个状态码断言） | wp3-fix2 后复核 |
| RV1-WP3-F5 | P2（report-only） | `node_link/pairing.rs:243-253` | 同内容并发 claim 的极窄窗口内后到者撞 `Conflict(AlreadyClaimed)` → 409；再次同内容重试自愈（Repeat → 201） | 极窄窗口内客户端可能误读 409 | 修复（wp3-fix2：补自愈语义注释） | wp3-fix2 后复核 |

report-only note：

- **N-RV1-WP3-1**：`CORE_PORTS_AND_STORAGE.md:1338` 现状句与 `MODULE_ARCHITECTURE.md:363` 口径不同步——并入 WP8 收口清单。
- **N-RV1-WP3-2**：`pairing_channel_view` 是 3 次独立查询的读时拼接，不承诺跨行快照——未发现缺陷，提示后续切片可在文档写明。

## Assessment 摘要

- 派单重点逐项结论：401 不泄露差异 ✔（单一拒绝形状 + correlationId 随机）；幂等重试不建第二条记录 ✔；四个安全头覆盖 201/400/401/403/404/405/409/410/413/429 ✔（处理器内与接入层默认头同源）；secret 生命周期与日志边界 ✔；限流键与固定速率 ✔（两份实现逐行等价，均满足 §2.5）；claim 原子性 ✔（BEGIN IMMEDIATE 内重做守卫，并发第二个得 409）；endpoint host 校验 ✔；claimant 全部使用点绑定配对 ✔；无 unsafe/秘密进日志 ✔。
- coder 日志（两轮）内部自洽，含红向判别力实测（空跑默认头 seam → 3 个行为用例 FAILED → 还原后全绿）。
- 待补证据（不影响本轮判断）：PV3 独立轮次（3.5 执行中）与 PV5 正式轮次；cargo-deny/gitleaks 只在 CI。
- 限制：L1 无 shell/Git（diff 未独立重算）；L2 未执行命令；L3 平台（Linux cfg(unix) 由 CI 覆盖）；L4 CI 专属；L5 E2E not-applicable；L6 2.25/2.26 深检复用 RV1-WP3C（增量不含端口/DDL/枚举/状态机代码，复用依据成立）。

## 结论

- **判定：PASS**（target `d127a802`）。无未解决 CRITICAL/MAJOR；5 项 P2 不阻断 WP3 交付；F1 已由主 Agent 以 spec 修订闭环，F2/F3/F4/F5 列入 wp3-fix2。
- 本报告不代替主 Agent 写入 verification.md，也不宣称候选验证、E2E 或合并已完成。
