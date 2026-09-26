# WP3 合同扩展交接报告（任务 2.25 + 2.26）

## Shared Report

- **task_id**：2.25、2.26（WP3 合同扩展，阶段 implement，交付单元 DU1）
- **role / phase**：coder / implement
- **agent_context**：worker 子 Agent（未继承主 Agent 对话；本机恢复运行，worktree 独占）。起草与提交在同一会话内完成，无并发写入者。
- **target_revision**：`13f0a16b55203a8224771a23fc432fac8b5184bb`（base `041aeb043d7b585b43faaaf1f40a330854b80784`）
- **scope**：`core`（model/use_cases/ports/broker 的授权分支 + 测试替身）、`storage-sqlite`（admin/audit 还原、admin/trust 落盘、migrate v3、session_store 还原、admin/mod 注释）、`server`（两个 TrustStore 测试替身）、三份权威文档；未触碰 `server::node_link`/`app`/协议资产/`compatibility/**`。
- **changes**：见下方「改动文件与需求映射」。
- **checks**：fmt / clippy(--workspace --all-targets --all-features) / `cargo test -p core -p storage-sqlite` / `npm run check`（10 道，含 `check:drift`）/ `unsafe` 扫描全部通过；原始输出 `reports/wp3-contract.log`。
- **issues**：无阻断项。三条需要主 Agent 知情的范围事实见「范围与断言偏离」：`crates/core/src/broker.rs`（1 行生产分支 + 测试替身）、`crates/server/src/local_admin/test_support.rs`（2 个替身）与 4 个测试文件的编辑是新增端口/枚举变体的**编译期必然连带**，不是可选项。
- **result**：PASS（本工作包检查全绿）。**不代表**独立 review、PV3 的正式执行（归 2.7/3.10）或候选/主分支验收。
- **evidence_paths**：`reports/wp3-contract.log`（逐命令原始输出）、`reports/wp3-contract-handoff.md`（本文件）。
- **resource_cleanup**：只用本机 worktree 的 `target/` 与系统临时目录（测试自建、由 `TempDir` 守卫清理）；未占用端口、未起后台进程、未改共享资源；`fixtures/**` 无二进制改动（实测生成器产物与既有资产 md5 一致）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.25"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp3-contract-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "core 合同扩展（Actor::PairingClaimant / ActorKind::PairingClaimant / node.authenticated / node.auth_failed / claim_pairing 与 pairing 的绑定校验 / consume_pairing 用例 / TrustStore::consume_pairing 与 PairingConsumption）在 base 041aeb0 + 本次改动 13f0a16 上实测：cargo test --locked -p core --all-features 100 passed / 0 failed；cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0；npm run check 退 0（10 道，check:drift 报告 §5 的 15 个 trait / 88 个签名与 ports.rs 一致）。日志 reports/wp3-contract.log。正式 [PV3] 执行归 WP3 交付任务 2.7/3.10，本轮未声称其 ID。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.26"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp3-contract-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "storage v2 → v3 迁移与 consume 落盘在 base 041aeb0 + 13f0a16 上实测：cargo test --locked -p storage-sqlite --all-features = migration 8 passed/1 ignored、admin_store 36 passed、enum_coverage 2 passed、commit 15 passed/1 ignored、其余套件全绿（零失败）；npm run check 退 0，其中 check:drift 报告 §7 的 36 条 DDL 与 migrate.rs 逐条一致；夹具生成器重跑产物与既有 fixtures/storage/v2/from-v1.sqlite3 的 md5 一致（70b62589355b81d93a2b959b13e8f6dd），未新增二进制资产。日志 reports/wp3-contract.log。"
    source_evidence: NOT_APPLICABLE
```

## 改动文件与需求映射

| 文件 | 对应任务点 | 内容 |
|---|---|---|
| `crates/core/src/model/identity.rs` | 2.25-1/2 | `ActorKind::PairingClaimant`（token `pairing_claimant`）、`Actor::PairingClaimant { pairing }`（`kind`/`id_text`/`pairing()`；`scopes`/`device_id`/`node_ids` 返回 `None`）、`AuditAction::NodeAuthenticated`/`NodeAuthFailed` |
| `crates/core/src/use_cases.rs` | 2.25-3/4 | `require_pairing_access`（LocalCli ∨ 绑定同一配对的认领方，拒绝形状与 `require_local` 逐字相同）、`claim_pairing`/`pairing` 换用该判定、新增 `consume_pairing`（主体类别 → 对端一致 → 状态 → 写集；`peer_matches_actor`/`terminal_pairing_conflict` 两个私有助手） |
| `crates/core/src/ports.rs` | 2.25-5 | `PairingConsumption { pairing, actor, context }` DTO + `TrustStore::consume_pairing(write) -> Result<PairingRecord, PortError>` |
| `crates/core/src/broker.rs` | 2.25-2 连带 | `authorize` 增 `Actor::PairingClaimant => false`（穷尽匹配 + 失败关闭，命中仍写 `authorization.denied`）；`test_support`：`FakeWorld.peers`、`FakeTrust::pairing_peer` 读它、`FakeTrust::consume_pairing`（审计只在真正推进状态的分支入账，镜像真实写集语义） |
| `crates/core/src/model/tests.rs` | 2.25-7 | `AuditAction::ALL.len()` 20 → 22、两个新动作的 `as_str`/`FromStr`、`pairing_claimant_actor_is_bound_to_one_pairing`（token/绑定/id_text/scopes/`ActorKind::ALL.len() == 4`） |
| `crates/core/src/use_cases.rs`（tests 模块） | 2.25-7 | 5 个新用例：绑定校验（匹配通过 + 不匹配/设备 actor 同一 `authorization.scope_denied` + 被拒认领零写集审计）、`consume_pairing` 推进与幂等（时间不覆盖、审计不重复）、节点路径 `node.authenticated`、对端/类别错配与写集审计归因分歧、未批准/过期/缺行/缺对端行的具名拒绝 |
| `crates/storage-sqlite/src/migrate.rs` | 2.26-1/3 | 三个版本常量 2 → 3；`OWNED_SCHEMA_V1`/`IMPORTED_SCHEMA_V1` 的两处 CHECK 扩宽；新增 `V3_UPGRADE_OWNED`/`V3_UPGRADE_IMPORTED`（12-step 重建）；`migrate()` 先按 `file_version < 2` 走 v2 段、再走 v3 段，序列在全部重建前取一次、之后回填一次；`mark_v2_schema_versions` → `mark_schema_versions`；`V2_UPGRADE_*` 注释改为「重建出 v2 形状」 |
| `crates/storage-sqlite/src/admin/trust.rs` | 2.26-2 | `consume_pairing`：写集主体与审计归因一致性守卫 → 读配对行/对端行（缺 peer → `Corrupt`）→ 对端同类同 id 比对 → `consumed` 幂等回读 / `approved` 条件更新 + 审计 / 其余具名拒绝；`peer_matches_actor` 助手；模块头「§11.6 第 1–8 条」 |
| `crates/storage-sqlite/src/admin/audit.rs` | 2.25-1 连带 | `actor_from_columns` 增 `PairingClaimant → Actor::PairingClaimant { pairing: decode(actor_id) }`（审计行可原样读回） |
| `crates/storage-sqlite/src/session_store.rs` | 2.25-1 连带 | `actor_from_columns` 增 `PairingClaimant → StorageError::ColumnValue`（`owned_command` 不接受该值，读到即按列值损坏处理） |
| `crates/storage-sqlite/src/admin/mod.rs` | 2.26-2 | 容量门注释把 `consume_pairing` 列入「不增长库且是安全动作」的免门路径 |
| `crates/server/src/local_admin/test_support.rs` | 2.25-5 连带 | `NotTouched` 与 `FakeTrust` 两个 `TrustStore` 替身各补 `consume_pairing`（`unreachable!("{NOT_TOUCHED}")`，与既有写面同款） |
| `crates/storage-sqlite/tests/migration.rs` | 2.26-4 | 新增 v2 → v3 升级用例（含「旧形状拒绝新词表」的红向断言、行/`audit_id`/序列保留、新词表可写、`owned_command` 拒绝认领方）；v1 → v3 连续升级用例；幂等/过新用例按新夹具角色改写；生成器缩为 `regenerate_v1_fixture` |
| `crates/storage-sqlite/tests/admin_store.rs` | 2.26-4 | 4 个新用例：推进一次 + 幂等 + 重启保持、对端/归因拒绝、未批准/已拒绝/已过期/缺行/缺对端行、审计写失败整写集回滚（触发器注入） |
| `crates/storage-sqlite/tests/enum_coverage.rs` | 2.26-4 | `owned_command.actor_kind` 期望值独立为字面三值（新增 `command_actor_kinds()`），审计两表仍按 `ActorKind::ALL` |
| `crates/storage-sqlite/tests/commit.rs` | 2.26-3 连带 | `health.user_version` 2 → 3 |
| `docs/CORE_PORTS_AND_STORAGE.md` | 2.25-6 / 2.26-3 | §3.5（`Actor` 变体 + 两个 `AuditAction` + `PairingConsumption` 行）、§4（`PairingChannel` 行 + actor 规则段）、§5.3（DTO + `consume_pairing` 签名，与 `ports.rs` 归一化后逐字一致）、§7 标题与 §7.2（版本、v3 步骤、夹具段落）、§7.3/§7.4（两处 CHECK）、§9 判据 1/14/28、§10（2026-09-26 裁定条目）、§11.6（写集语义第 8 条）、§11.7（CHECK 扩宽口径） |
| `docs/SECURITY_DESIGN.md` | 2.25-6 | §14.2 词表增两个 node 动作 + 一条 `[决定]`（握手留痕、`consumed` 不另设动作、认领方进审计、落库走 v3 重建） |
| `docs/IDENTITY_AND_AUTH_CONTRACT.md` | 2.25-6 | §5.1 增 `Actor::PairingClaimant` 构造规则；`Completion` 注释指向 `TrustStore::consume_pairing` |

## 冻结的新公开形状（供 WP3 HTTP 阶段与 WP4 消费）

```rust
// core::model（crates/core/src/model/identity.rs）
pub enum ActorKind { Device, Node, Cli, PairingClaimant }      // token: "pairing_claimant"
pub enum Actor {
    Device { device: DeviceId, scopes: ScopeSet },
    Node { node: NodeId, access_node: NodeId },
    LocalCli,
    PairingClaimant { pairing: PairingId },                     // Actor::pairing() -> Option<&PairingId>
}
pub enum AuditAction { /* … */ NodePaired, NodeAuthenticated, NodeAuthFailed, NodeTrustRevoked, /* … */ }
//   token: "node.authenticated" / "node.auth_failed"

// core::use_cases（crates/core/src/use_cases.rs）
pub async fn claim_pairing(&self, actor: &Actor, claim: PairingClaim) -> Result<PairingClaimOutcome, PortError>;
pub async fn pairing(&self, actor: &Actor, id: &PairingId) -> Result<Option<PairingRecord>, PortError>;
pub async fn consume_pairing(&self, actor: &Actor, id: &PairingId) -> Result<PairingRecord, PortError>;

// core::ports（crates/core/src/ports.rs）
pub struct PairingConsumption { pub pairing: PairingId, pub actor: Actor, pub context: WriteContext }
pub trait TrustStore {
    async fn consume_pairing(&self, write: PairingConsumption) -> Result<PairingRecord, PortError>;
}
```

适配器（`server::node_link` / 未来 `server::sync`）必须按以下口径接线：

- 认领方：`Actor::PairingClaimant { pairing }` 只能由配对 HTTP 端点在 claim/status 的 proof 验证成功后构造，且 `pairing` 必须是本次调用的目标配对；否则 `claim_pairing`/`pairing` 返回 `PortError::InvalidRequest("authorization.scope_denied")`（与「不是本机入口」同一个形状）。
- 握手收尾（D3）：`complete_auth` 返回的 `consume_pairing: Some(id)` 经 `UseCases::consume_pairing(&actor, &id)` 落盘，**同一事务成功后才发 `node.ready`**；`actor` 从 `IdentityFact` 构造（`Node`/`Device`），`node.authenticated` 的审计由用例面随写集提交，适配器不要再补一条。
- 错误分类：对端错配 → `PortError::Conflict(IdentityMismatch)`；未批准 → `InvalidRequest("pairing has not been approved")`；`expired` → `Conflict(Expired)`；`rejected`/已消费但非同一对端 → `Conflict(Consumed)`；配对缺失 → `NotFound(Pairing)`；配对在但对端行缺失 → `Corrupt`。
- 幂等：同一 `(pairing, 对端)` 的重复消费返回 `Ok`（首次 `terminal_at` 不变、不再写审计），适配器无需自己判重。

## 范围与断言偏离（逐条：原 → 新 → 理由）

### A. 测试断言/用例改写

1. `crates/core/src/model/tests.rs` — `assert_eq!(AuditAction::ALL.len(), 20)` → `22`。理由：`NodeAuthenticated`/`NodeAuthFailed` 是两个新取值；同文件补两个新动作的 `as_str`/`FromStr` 断言。
2. `crates/storage-sqlite/tests/enum_coverage.rs` — 案例 `("owned_command", "actor_kind", tokens(ActorKind::ALL, ActorKind::as_str))` → `("owned_command", "actor_kind", command_actor_kinds())`（字面 `["device","node","cli"]`，并新增该函数与理由注释）。理由：D12 明确 `owned_command` **不重建**、不接受 `pairing_claimant`，而 `ActorKind::ALL` 现在含它；两张审计表仍按 `ActorKind::ALL` 断言，因此新增一个变体时两处分别给出「审计表少一个值」与「命令表多收一个值」的具名失败。
3. `crates/storage-sqlite/tests/commit.rs` — `assert_eq!(health.user_version, 2)` → `3`。理由：`FILE_FORMAT_VERSION` 升到 3。
4. `crates/storage-sqlite/tests/migration.rs`
   - 删除 `v2_fixture_is_untouched_by_two_consecutive_starts`（断言夹具自身 `user_version == FILE_FORMAT_VERSION`，即「v2 夹具＝当前版本」）→ 新增 `current_version_database_is_untouched_by_two_consecutive_starts`（空目录新建的当前版本库，两轮 open 后比较 `user_version`/`schema`/`meta` **与全部表的行集**）。理由：v3 之后 v2 夹具是「升级输入」而不再是「当前形状」的代表；新用例还多覆盖了行集不变（原来只比 schema/meta）。
   - `fresh_directory_is_created_at_version_two` → `fresh_directory_is_created_at_the_current_version`；两处 `Some("2")` → 循环断言 `Some("3")`。理由：版本常量与函数名一致性。
   - `too_new_database_is_rejected_without_writing_rows`：夹具 `too-new.sqlite3` → `copy_fixture("empty.sqlite3")` + `PRAGMA user_version = FILE_FORMAT_VERSION + 1`；`assert_eq!(found, 3)` → `assert_eq!(found, future_version)`；末尾 `user_version` 断言随之。理由：用户裁决 A 不新增二进制夹具；v3 之后 `user_version = 3` 已不再「过新」。
   - `v1_fixture_upgrades_to_v2_and_preserves_rows` → `v1_fixture_upgrades_to_v3_and_preserves_rows`；`owned_schema_version/imported_schema_version == 2` → `== 3`；`meta.*` 断言 `Some("2")` → `Some("3")`（消息改为 "advanced to the current version"）。理由：v1 库现在走 v1 → v2 → v3 连续升级。
   - `a_failed_upgrade_rolls_back_to_v1` 的重试成功断言 2 → 3。理由同上；失败回滚断言（保持 v1、DDL 未换）不变。
   - 生成器 `regenerate_v2_fixtures` → `regenerate_v1_fixture`：只重建 `from-v1.sqlite3`（v1 形状）；删除「建 v2 空库」「造 uv=3 过新库」两步与其 `FIXTURE_SERVER_EPOCH` 常量。理由：当前二进制只能建出 v3 形状，无法再产出 v2 形状空库；实测重跑产物与既有 `from-v1.sqlite3` md5 一致（`70b62589…`）。
   - 新增 `v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`：先断言夹具是 v2 形状（DDL 不含 `pairing_claimant`/`node.authenticated`）且**旧 CHECK 会拒绝**这两个新词表（红向判别力），再断言升级后行/`audit_id`/`sqlite_sequence` 不变（7 > max=2）、新词表可写、`owned_command` 仍拒绝认领方。理由：2.26 的完成条件（v2 → v3 升级、新 CHECK 生效）。
5. `crates/storage-sqlite/tests/admin_store.rs` — 新增 4 个用例（推进+幂等+重启保持、对端/归因拒绝且零改动、未批准/已拒绝/已过期/缺行/缺对端行、审计写失败整写集回滚）；新增 `audit_for()`（可指定归因主体的审计行）与 `device_actor()` 两个测试助手；`use acp_core::ports` 增 `PairingConsumption`。理由：2.26 的完成条件（consume 写集原子性、重启后状态保持）。

### B. 声明写范围之外的**必需**文件

6. `crates/core/src/broker.rs` — 生产代码 `authorize` 增 `Actor::PairingClaimant => false`；`#[cfg(test)] test_support` 增 `FakeWorld.peers`、`FakeTrust::pairing_peer`（读该字段）与 `FakeTrust::consume_pairing`。理由：新增枚举变体让 `match actor` 不再穷尽（编译期强制），语义上认领方必须失败关闭；`use_cases` 的 `consume_pairing` 会读 `pairing_peer`，替身必须能返回已认领对端，否则坐标无法被用例覆盖。**没有替代写法**（trait 默认实现会破坏漂移门禁对 trait 体的逐行扫描，且违反「端口不提供默认实现」的既定口径）。
7. `crates/server/src/local_admin/test_support.rs` — 两个 `TrustStore` 替身（`NotTouched`、`FakeTrust`）各补 `consume_pairing`（`unreachable!("{NOT_TOUCHED}")`）。理由：新增端口方法的编译期必然连带，不补则 `cargo clippy --workspace --all-targets` 不过；两处实现体与既有写面（`create_pairing`/`settle_pairing`/`expire_pairings` 等）完全同款，不改变 `local_admin` 的任何行为。
8. `crates/storage-sqlite/src/session_store.rs`、`src/admin/audit.rs` — 各补一个 `ActorKind::PairingClaimant` 分支（前者按列值损坏失败关闭，后者还原为 `Actor::PairingClaimant`）。理由：同一个编译期穷尽性要求；两处语义分别对应「命令表不接受该值」与「审计行必须能原样读回」。

（用户裁决 A 已批准测试文件范围的扩大：`crates/core/src/model/tests.rs`、`crates/storage-sqlite/tests/{enum_coverage,migration,commit,admin_store}.rs`；第 6/7/8 条是同一类「新增端口/枚举变体的必要连带」，故一并在此逐条登记，供独立 reviewer 判定。）

## 未执行项与待留意

- **未执行 [PV3] 的正式轮次**：本任务不在 tasks.md 里绑定检查 ID，`[PV3]` 归 2.7/3.10（WP3 交付与交付前验证）；本轮跑的是同一命令集（fmt / clippy --workspace / 两个受影响 crate 的全量 test / `npm run check` / unsafe 扫描）并留了原始日志。
- **未跑 workspace 全量 `cargo test`**：其余 crate 未改动；workspace 级 `cargo clippy --all-targets` 已覆盖它们的编译面。若主 Agent 要求，可用 `cargo test --locked --workspace --all-features` 补一轮（`npm run verify` 的等价入口）。
- **`fixtures/storage/v2/too-new.sqlite3` 成为未引用的历史资产**（`user_version = 3` 在 v3 之后不再过新）；已按用户裁决 A 保留在仓库、在 §7.2 段落里说明其历史身份，未删除、未改动字节。
- **v1 → v3 升级会重建审计表两次**（v2 段一次、v3 段一次，各自保持自身目标形状可核算）。这是「v1 → v2 路径保持不变」的直接结果，只影响旧 v1 库的一次性启动成本。
- **幂等路径不写审计**是有意选择（同一消费只留一条留痕；WP4 若要为「每次握手」都留痕，走独立的 `AuditStore::append`），已在 §11.6 第 8 条、`SECURITY_DESIGN.md` §14.2 与代码注释里写明。
- **独立 review 尚未返回**（`reviewer` 角色负责）；本报告只声称实现侧检查 PASS。

## 复验指引（reviewer / 主 Agent）

```text
git -C D:/Project/acp-remote-wt/node-link-owner show --stat 13f0a16
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p core -p storage-sqlite --all-features
npm run check                      # 10 道，含 check:drift
rg -n "unsafe \{" crates/core/src crates/storage-sqlite/src   # 零命中
```

重点核对项：① `ports.rs` 与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的签名逐条一致（`check:drift`）；② `migrate.rs` 的 v3 DDL 与 §7.3/§7.4 的 CHECK 文本逐条一致（`check:drift`）；③ `owned_command` 的 CHECK 未被改动（三值）；④ `claim_pairing`/`pairing` 的 `LocalCli` 路径行为不变（既有 100 个 core 用例全绿）；⑤ `consume_pairing` 的幂等路径确实不追加审计（`admin_store.rs::consume_pairing_advances_an_approved_pairing_once` 断言 `device.authenticated` 行数恒为 1）。
