# WP4 fix1 Handoff — RV1-WP4 的 F1/F2（P1）与 F3/F4/F5/F6（P2）（`node-link-owner` / WP4 握手与连接生命周期）

## Shared Report

- **task_id**: `RV1-WP4-F1` / `RV1-WP4-F2` / `RV1-WP4-F3` / `RV1-WP4-F4` / `RV1-WP4-F5` / `RV1-WP4-F6`（对应 review 报告 `reports/rv1-wp4.md` 的 Findings；F7 已由主 Agent 当场修 design D3，不在本轮）
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP4`
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP4 实现或 review 会话，只按主 Agent 下发的 fix 清单与 `rv1-wp4.md` 的 Findings 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁、红向证据均由本 Agent 亲自执行。
- **base / target_revision**: base = `319c77b715eb218ee81696aeec8257d268e0930a`（开工时 HEAD 与之一致，`git status --short` 为空）。本轮两个提交：
  - `556d170e1130c6ff761b57162c3530ef1bc8af00`（F2 的 core/存储/文档）；
  - `2917401cdbfaa21dc97786d0a792c1081ebb805a`（F1/F3/F4/F5/F6 的 `server::node_link::conn`）。
  提交后 `git status --porcelain` 为空。
- **scope**（严格按派单允许写入面）：
  - `crates/server/src/node_link/conn/{registry.rs,session.rs,wire.rs,tests.rs}`：F1/F2 接线/F3/F4 用例/F5/F6。
  - `crates/core/src/{ports.rs,use_cases.rs}` + `crates/storage-sqlite/src/admin/trust.rs` + `crates/server/src/local_admin/test_support.rs`：F2 的窄入口与落盘（含测试替身镜像）。
  - `docs/CORE_PORTS_AND_STORAGE.md`：F2 的 §3.5/§4/§5.3/§11.6 同步与修订记录。
  - **清单外但经主 Agent 明确批准的一处编译连带**：`crates/core/src/broker.rs` 的测试替身 `FakeTrust` 补 `record_node_connected`（详见「清单外编辑」小节，供 reviewer 单独核对）。
  - **未做**：未改设计/规划文件（`design.md`/`plan.md`/`tasks.md`/`verification.md`/proposal/specs）、`openspec/specs/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、`node-link-protocol`/`identity-auth` 两 crate、其他文档与 app。
- **changes**（2 个提交，10 个文件，669 insertions / 114 deletions）：
  1. **F1（P1）出站序号不再因拒绝而消耗**：`ConnectionHandle` 把出站账本收敛为一个 `Mutex<OutboundState>`（`sequence` + `pending_messages` + `pending_bytes`）。`send` 在**同一临界区**内完成「容量预算（条数 + 字节双上限，`OutboundState::admits`）→ 候选序号 → 编码 → `try_send` → 提交序号与占用」；被高水位拒绝的投递不推进 `sequence`。原先 `enqueue` 的字节判定与 `try_send` 的条数判定都在序号分配之后，一次 `HighWater` 就永久烧掉一个序号（协议无重同步规则，对端只能一直 `sequence_invalid`）。同时删掉仅被 `send` 使用的 `encode`/`enqueue` 拆分（`encode` 改为私有并显式接收序号）。
  2. **F2（P1）认证收尾推进 `owned_node.last_connected_at`（主 Agent 裁定选项①）**：两条认证收尾路径都进写集——① 首次认证（已批准配对 → `consumed`）由 `TrustStore::consume_pairing` 的同一事务推进 `Actor::Node` 对应的 `(node, access)` 行（角色由配对批准唯一确定，`trust.rs` 模块头的机器事实 2）；② 重复认证（没有待消费配对）新增 `NodeConnectedWrite` + `TrustStore::record_node_connected`（`last_connected_at` 只前进不倒退 + `context.audit` 同一事务），由新的 `UseCases::record_node_connected(access_node)` 组装，`session.rs` 的 `None` 分支改调它。存储实现只 `UPDATE` 一个时间列（不走 `upsert_node`：后者会把 `revoked_at`/`revoke_reason` 一起写回）。
  3. **F3（P2）在途握手配额只在握手阶段持有**：`Session::run` 把 RAII 凭证绑定到 `handshake_phase()` 的返回值上并在进入业务阶段前 `drop`，配额因此限制「同源半开握手数」而不是「同源连接数」。
  4. **F4（P2）两条准入闸门补行为用例**：① 同源第 11 次建连 → `link.error(nodelink.resource.rate_limited, retryable=true, details.retryAfterMs>0)` + 4429 + 零副作用（无信任行/无审计/无注册）；② 同源并发 5 条未认证连接的第 5 条被拒（与 F3 的第二条用例同一份，见下）。为了让两条用例确定性（不靠轮询赌时机），`NodeLinkConn` 增 `#[cfg(test)] in_flight_handshakes()`（`HandshakeSlots::in_flight`），与既有 `with_test_windows` 同款测试观测口。
  5. **F5（P2）未知 type 与 post_mvp 同口径记账序号**：`wire::judge` 在 `EnvelopeError::UnknownType` 分支新增 `unknown_type_next_sequence`——认证后、`connectionId` 与本次连接相同、`connectionSequence` 是规范十进制串且不超过 v1 上界、正好是期望值时推进期望序号；任一不满足不记账。`wire.rs` 模块文档补一节「序号记账与 type 无关」。被钉死的用例第 ⑧ 段随之改为「信封合法 ⇒ 序号已被占用」，并给出红向证据。
  6. **F6（P2）两条不可达早退分支**：`session.rs` 的重复 `node.hello` 守卫与「没有 hello 的 proof」守卫各补注释，写明由 `wire::pre_auth_allowed` 保证不可达、保留为失败关闭（不 panic）。选择「注释」而不是「删除」，因为 `on_proof` 的守卫同时承担四个字段的解构，删掉就只能写 `unreachable!()`/`expect()`（违反「正常路径不得 panic」）。
- **checks**（原始输出见 `wp4-handshake.log` 的「wp4-fix1 轮次」）：
  - `cargo fmt --all -- --check` exit=0。
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth --all-features` exit=0（31 个 test binary 全 ok / 0 failed；`server` lib **225** 通过，其中 `node_link::conn` 24；`core` lib 104）。
  - `cargo test --locked --workspace --all-features` exit=0（83 个 `test result: ok`，无 FAILED）。
  - `npm run check` exit=0（`check:drift` 报「§5 的 15 个 trait / **90** 个方法签名与 `crates/core/src/ports.rs` 一致」——新增的 `record_node_connected` 已同步进 `docs/CORE_PORTS_AND_STORAGE.md` §5.3）。
  - 提交前钩子（`.husky/pre-commit`）两次提交各三步全通过（fmt → `npm run check` → workspace clippy），commitlint 通过。
  - **红向证据**（四条，均为「把修复前源文件取回工作区 → 同一用例 FAIL → 恢复」，原始输出见日志）：F1 `["1","3"] != ["1","2"]`；F2 重复认证后 `last_connected_at` 停在首次时间；F3 同源第 5 条已认证连接被 `rate_limited` 拒；F5 第 ⑧ 段之后的 `link.ping` 被判 `sequence_invalid`。
  - **用例计数**：`crates/server/src/node_link/conn/tests.rs` 的测试函数 19 → 24（+5，无删除）；唯一改写的是既有用例 `envelope_and_sequence_violations_are_rejected_without_closing` 的第 ⑧ 段（F5 的口径修订，红向证据在上）。
- **issues**: 6 项全部落地；无阻断，无新增不确定项。主 Agent 对 F2 形态的裁定（选项①）已按其四项要求执行（① §5.3 签名与 `ports.rs` 逐字一致、`check:drift` 通过；② §11.6 新增第 9 条写明「重复认证也推进、只前进不倒退」；③ 三个场景（首次非空 / 第二次推进 / 时钟回拨不倒退）都有断言；④ 清单外编辑单独列出）。三个场景写在同一条用例 `authentication_advances_the_node_last_connected_time` 的 ①②③ 三段里（同一个真实 loopback 世界、三次认证），没有拆成三条独立用例。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表**独立复核（RV2-WP4）、`[PV5]`、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp4-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp4.md`
  - 本轮原始输出（含四条红向证据）：`openspec/changes/node-link-owner/reports/wp4-handshake.log` 的「wp4-fix1 轮次」
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`。临时文件只写在系统临时目录（`/tmp/wp4fix1/`，非仓库内）；提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `556d170e1130c6ff761b57162c3530ef1bc8af00` | `feat(core): 认证收尾写集推进节点 last_connected_at` | `core/src/{ports,use_cases,broker}.rs`、`storage-sqlite/src/admin/trust.rs`、`server/src/local_admin/test_support.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（6 files, +224/−27） |
| `2917401cdbfaa21dc97786d0a792c1081ebb805a` | `fix(server): 出站序号不因拒绝而消耗、握手配额随握手释放` | `server/src/node_link/conn/{registry,session,wire,tests}.rs`（4 files, +445/−87） |

（两个提交均经 `.husky/pre-commit` 三步与 commitlint；`git status --porcelain` 为空。）

## 清单外编辑（主 Agent 已批准，单独列出供 reviewer 核对）

- 文件：`crates/core/src/broker.rs`（`#[cfg(test)] mod test_support` 内的 `FakeTrust`，第 ~4586 行）。
- 内容：新增一个 trait 方法实现
  `async fn record_node_connected(&self, _write: NodeConnectedWrite) -> Result<(), PortError> { unreachable!("本替身不实现 Node Link 认证收尾") }`
  以及测试用的 `use crate::ports::NodeConnectedWrite` 一行。
- 理由与影响：`TrustStore` 是 `core::ports` 的 trait，新增方法必须实现仓库里全部 4 个 impl（`storage-sqlite`、`server` 的两个替身、`core` 的 `FakeTrust`），否则不能编译。前三者都在派单允许写入面内；`core/src/broker.rs` 是本仓库唯一在清单外的 impl。该替身不被 Node Link 路径使用（`core` 的用例只走配对消费与审计），显式 `unreachable!` 而不是静默成功，避免「替身比真实存储更宽容」。**这是本轮唯一的清单外编辑，无行为改动**；主 Agent 在开工时以「与 WP3 合同扩展时 test_support 替身的同款连带」批准。
- 未在 `core` 侧给该替身写真实镜像（不写 `last_connected_at`），因此 `core` 的既有用例不受影响；认证收尾的真实行为由 `server` 的 loopback 用例与 `server`/`storage-sqlite` 的替身镜像覆盖。

## 各条落点（reviewer 复检索引）

| ID | 修复落点 | 用例 |
|---|---|---|
| F1 | `conn/registry.rs`：`OutboundState` + `ConnectionHandle::send` | `conn/tests.rs::a_rejected_delivery_does_not_consume_the_outbound_sequence`（红向：`["1","3"]` vs `["1","2"]`） |
| F2 | `core/ports.rs`（`NodeConnectedWrite`、`TrustStore::record_node_connected`、`PairingConsumption` 文档）、`core/use_cases.rs`（`record_node_connected`）、`storage-sqlite/admin/trust.rs`（`advance_node_connected_at` + `consume_pairing` 写集 + 新方法）、`server/node_link/conn/session.rs`（重复认证分支）、`server/local_admin/test_support.rs`（替身镜像） | `conn/tests.rs::authentication_advances_the_node_last_connected_time`（三段：首次非空 / 第二次推进 / 时钟回拨不倒退；并断言 3 行 `node.authenticated` 归因一致） |
| F3 | `conn/session.rs::Session::run`（凭证作用域）+ `conn/session.rs::HandshakeSlots::in_flight`（测试观测口） | `conn/tests.rs::authenticated_connections_do_not_hold_the_in_flight_handshake_quota`（红向：第 5 条被 `too many handshakes in flight` 拒） |
| F4 | 无生产改动（两条闸门本已实现，本轮补行为证据） | `conn/tests.rs::the_eleventh_authentication_attempt_from_one_address_is_rate_limited`、`conn/tests.rs::the_fifth_in_flight_handshake_from_one_address_is_refused` |
| F5 | `conn/wire.rs::judge` + `unknown_type_next_sequence` + 模块文档 | `conn/tests.rs::envelope_and_sequence_violations_are_rejected_without_closing` 第 ⑧ 段（红向：第 ⑧ 段之后的 ping 被判 `sequence_invalid`） |
| F6 | `conn/session.rs` 两处守卫注释 | 无（不可达分支，靠既有用例保证 `pre_auth_allowed` 行为不变：`a_business_message_before_hello_is_closed_with_4401`、`an_unknown_node_gets_a_challenge_and_fails_as_node_unknown`） |

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP4-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "2917401 上的本机轮次：outbound_sequence 的候选值、容量预算（条数+字节）与 try_send 已收进同一 Mutex<OutboundState> 临界区，HighWater 不提交序号；新用例 a_rejected_delivery_does_not_consume_the_outbound_sequence 由红（[1,3]）转绿（[1,2]），cargo test -p server 全绿、clippy -D warnings 退 0"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "两条认证收尾路径都进写集：consume_pairing（首次）与新增 TrustStore::record_node_connected（重复）；docs/CORE_PORTS_AND_STORAGE.md §3.5/§4/§5.3/§11.6 第 9 条已同步且 check:drift 退 0（§5 的 15 个 trait / 90 个方法签名一致）；用例 authentication_advances_the_node_last_connected_time 覆盖首次非空、第二次推进、时钟回拨不倒退（红向：重复认证后停在首次时间）"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "Session::run 的 HandshakeSlot 只在 handshake_phase() 期间持有，认证完成即 drop；同源 5 条已认证连接全部登记（registry.active()==5、in_flight_handshakes()==0），红向证据为修前第 5 条被 rate_limited 拒"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F4"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "两条准入闸门各有用例：同源第 11 次建连收到 nodelink.resource.rate_limited + retryable=true + details.retryAfterMs>0 + 4429（并断言无信任行/无审计/无注册），同源并发 5 条未认证连接的第 5 条被拒（details 为空、registry.active()==0）"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "wire::judge 的 UnknownType 分支与 post_mvp 同口径记账（unknown_type_next_sequence：认证后 + connectionId 匹配 + 规范十进制串 + ≤ v1 上界 + 正好是期望值），wire.rs 模块文档已注明；用例第 ⑧ 段改为『序号已被占用』并给出红向证据（修前紧随的 link.ping 被判 sequence_invalid），fixtures_are_consumed_by_the_handshake_and_error_layers 仍绿"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F6"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "两条早退分支保留失败关闭语义并注明『由 wire::pre_auth_allowed 保证不可达』；未删除 on_proof 的守卫是因为它同时承担四个字段解构，删除只能引入 unreachable!/expect；pre_auth_allowed 的行为由 a_business_message_before_hello_is_closed_with_4401 与 an_unknown_node_gets_a_challenge_and_fails_as_node_unknown 继续锁定，全量用例绿"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP4-F2（清单外编译连带：`crates/core/src/broker.rs`）"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "2917401cdbfaa21dc97786d0a792c1081ebb805a"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新增 trait 方法要求全部 impl 实现：清单外的 crates/core/src/broker.rs（test_support::FakeTrust）补一行 unreachable!（显式失败关闭，无行为改动）；经主 Agent 在开工时明确批准；cargo clippy --workspace --all-targets -D warnings 与全量测试退 0"
    source_evidence: NOT_APPLICABLE
```

## 残留风险与下游注意

1. **待独立复核**：本报告是 coder 自检（`[PV3]` 范围），不代替 RV2-WP4 的复检、`[PV5]`（3.7/3.9）或候选门禁；建议 reviewer 在 `2917401` 上按原 ID 逐条核对，尤其 F1 的临界区与 F2 的两条写集路径。
2. **存储层新 SQL 缺独立用例（明确登记的缺口）**：`storage-sqlite/src/admin/trust.rs` 新增的 `advance_node_connected_at`（`SELECT last_connected_at` + 条件 `UPDATE`）与 `consume_pairing` 写集里的那次调用，**没有** storage-sqlite 的集成用例覆盖——本批派单的允许写入面是 `crates/storage-sqlite/src/**`，不含 `crates/storage-sqlite/tests/`，因此本 Agent 未越界补测。当前证据链是：① `server` 的 loopback 用例 + 两个替身镜像（`local_admin::test_support::FakeTrust` 与 `storage-sqlite` 的实现同款语义）；② 「只前进」的比较口径与已被 `advance_last_seen_only_forward` 锁定的 `upsert_node` CASE 相同（本实现用显式比较表达，不重写整行）。建议由 RV2 或 `[PV5]` 判定是否需要补一条 storage 用例（在 `crates/storage-sqlite/tests/admin_store.rs` 里加用例会越出本批允许面）。
3. **`core/src/broker.rs` 的替身不给 `record_node_connected` 真实镜像**：若将来 `core` 的用例需要走这条写路径（例如新增一个「重复认证留痕」的用例层用例），需把 `unreachable!` 换成与 `storage-sqlite` 同款的镜像。
4. **F6 采用「注释」而非「删除」**：如果主 Agent 希望彻底删除死分支，`on_proof` 的守卫需要先重构状态机（把四个字段收进一个 `Option<IssuedChallenge>`），那会超出本轮「最小修复」的范围，故未做。
5. **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 生效，本地无等价物（提交钩子已提示「本地密钥扫描已跳过」）；本轮的 `npm run verify` 等价面已全跑（`npm run check` + fmt + clippy + test），上述两类 CI 判定仍未在本地执行。
6. **下游消费方**：F1 改变了 `ConnectionHandle::send` 的内部记账顺序（行为对调用方不变：仍是 `Ok`/`HighWater`/`Closed`/`Encoding` 四类），WP5/WP6 按 `saturated()`/`pending()` 停读快照批次的语义不变；F2 使 `node.list` 的 `lastSeenAt` 从此有值，WP7 的接线与前端展示需按「最近一次认证成功时间」理解该字段。
