# RV2-WP4 Review 报告（recheck：WP4 握手与连接生命周期 fix1 + fix1b）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## 结论

- **判定：PASS**（Target Revision `59bf1058851deb5b83c875a02f8046c815a9c4f0`，工作树干净）。
- 逐项复核：**F1/F2（含 fix1b 存储层用例）/F3/F4/F5/F6 全部已解决**；无未解决 CRITICAL/MAJOR，无回归证据。
- 新发现 2 条 P2（文档口径，不阻断）：RV2-WP4-F1（CORE_PORTS §4「两条窄入口」与三条入口自相矛盾）、RV2-WP4-F2（身份合同 §5.1 收尾写集指针未含 `record_node_connected`）。**主 Agent 已当场修复**（`2937850f`，npm run check 16/16 绿），RV1-WP5 附带复核。
- 本 PASS 不覆盖 [PV5]、E2E、候选门禁或 workflow check；日志中的门禁输出是 coder 证据（reviewer 未复跑，无 shell）。

## Shared Report

- **task_id**：RV1-WP4-F1..F6（复核对象见 `reports/rv1-wp4.md`）
- **role**：reviewer；**phase**：recheck；**stage**：work-package；**evidence_id**：RV2-WP4
- **agent_context**：独立 reviewer 子 Agent（d9dfbb2a-7753-4833-ba72-df1457bfcdbe），未参与 WP4 实现或修复；无 shell/Git/写文件工具。
- **base / target**：`319c77b715eb218ee81696aeec8257d268e0930a` / `59bf1058851deb5b83c875a02f8046c815a9c4f0`
- **scope**：`conn/{registry,session,wire,tests,mod}.rs`、`core/{ports,use_cases,broker}.rs`、`storage-sqlite/src/admin/trust.rs`、`storage-sqlite/tests/admin_store.rs`、`server/src/local_admin/test_support.rs`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1、`node-link-protocol/src/envelope.rs`、specs。
- **result**：PASS；**evidence_paths**：本报告 + `reports/rv1-wp4.md`、`reports/wp4-fix1-handoff.md`、`reports/wp4-fix1b-handoff.md`、`reports/wp4-handshake.log`。
- **resource_cleanup**：无资源创建、无写入。

## handoff_index

```yaml
handoff_index:
  - { task_id: "RV1-WP4-F1", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "OutboundState/send/consumed 逐行重读 + 红向证据（修前 ['1','3'] vs 修后 ['1','2']）；序号、容量预算与 try_send 同处一个临界区且仅入队成功才提交", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F2", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "两条路径同事务推进且不倒退；§5.3 与 ports.rs 逐字一致；存储层四条新用例含红向证据；check:drift 90 签名（coder 日志证据）", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F3", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "配额随握手释放；tests.rs:1109 断言 active==5 且 in_flight==0，红向证据为修前第 5 条被 rate_limited", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F4", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "两条准入闸门各有行为用例（错误码/details/close code/零副作用），确定性靠 #[cfg(test)] in_flight_handshakes() 观测口", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F5", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "未知 type 与已知路径同口径记账；第 ⑧ 段改写的红向证据见 wp4-handshake.log", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F6", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "不可达判定独立复核为真；注释而非删除（on_proof 守卫承担解构），无 panic 引入", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV1-WP4-F2（清单外编译连带：crates/core/src/broker.rs）", role: reviewer, phase: recheck, stage: work-package, target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0", evidence_type: REVIEW, evidence_id: RV2-WP4, report_path: "reports/rv2-wp4.md", result: PASS, evidence_status: NEW, applicability_basis: "FakeTrust 是 TrustStore 的第 4 个 impl，缺方法是编译错误；record_node_connected 无 core 测试调用路径，不会 panic", source_evidence: NOT_APPLICABLE }
```

## Findings 摘要（原问题复核）

- **F1 已解决**：`OutboundState{sequence,pending_messages,pending_bytes}` 全程持锁；`try_send` 仅 Ok 才提交序号；Full/HighWater 不提交；锁内无 await、无锁序反转（先 drop 再取 saturated_since）。
- **F2 已解决**（含 fix1b）：首次认证 = consume_pairing 写集内 advance；重复 = `record_node_connected`（BEGIN IMMEDIATE → advance → 审计 → commit）；`>=` 守卫不倒退；行缺失 → NotFound(Node)；§5.3 与 ports.rs 逐字一致；幂等分支不重复推进；失败整事务回滚。
- **F3 已解决**：配额只覆盖握手阶段；已认证连接不受限。
- **F4 已解决**：两条闸门各一条行为用例（4429 + rate_limited + retryAfterMs + 零副作用）。
- **F5 已解决**：未知 type 在信封可解析（版本/形状过 Wire 解析、connectionId 匹配、序号等于期望）时记账；pre-auth 不记账。
- **F6 已解决**：两处守卫补不可达依据注释。
- **F7**（主 Agent 已修 design.md D3，见 rv1-wp4.md 的裁决注记）。

### 新发现（RV2-WP4-F<n>，均已当场闭环）

| ID | 级别 | 位置 | 问题 | 处理 |
|---|---|---|---|---|
| RV2-WP4-F1 | P2 report-only | CORE_PORTS §4（「两条窄入口」与三条入口矛盾） | 文档计数失配 | 主 Agent 修复于 `2937850f`（「两条」→「三条」） |
| RV2-WP4-F2 | P2 report-only | IDENTITY_AND_AUTH_CONTRACT §5.1（收尾写集只点名 consume_pairing） | 重复认证路径未登记 | 主 Agent 修复于 `2937850f`（补 record_node_connected 一句） |

### 判定为非问题的点（摘要）

- broker.rs 的 `unreachable!` 确为编译连带（#[cfg(test)] 替身、无生产路径）。
- 新临界区不含 await、无锁序反转。
- F5 记账不会「多记」（未知 type 必经完整 Wire 解析）。
- F2 新增失败面（4500）对合法对端不可达（凭据状态判定先于写集）。

### Regression 复核

- conn/tests.rs 只增不减（19→24，+5 与 handoff 一致，17 条 WP4 轮次名称逐条仍在）。
- F5 第 ⑧ 段改写是加强而非弱化（红向证据）。
- admin_store.rs 36→40 只增不减。
- 限制：net/pairing 用例计数未与 base 对照（L3）；未独立重算 diff（L1）；未执行命令（L2）。

### 限制与残余风险

- L1 无 shell/Git；L2 未执行命令；L3 net/pairing 计数未对照；L4 WP5/WP6/WP7 不在本轮。
- L5 残余风险登记：① `core` 的 `FakeTrust` 不给 `record_node_connected` 真实镜像（未来 core 用例走到该路径会 panic，需改成同款镜像——列入 WP6 观察项）；② `record_node_connected` 对 revoked 行的语义无权威文本（有意不断言）；③ F3 之后不再有「同源已认证连接数」上限（TLS 池 64 槽是唯一兜底，主 Agent 已裁定的取舍）。
- [PV5] 与 workflow check 待补（门禁：候选合入前）。
