# WP4 fix1b Handoff — RV1-WP4-F2 的存储层用例缺口（`node-link-owner` / WP4 握手与连接生命周期）

## Shared Report

- **task_id**: `RV1-WP4-F2`（wp4-fix1 交付时**自己登记**的缺口：`storage-sqlite` 侧没有 `record_node_connected` / `advance_node_connected_at` 的集成用例；见 `reports/wp4-fix1-handoff.md` 的「残留风险与下游注意」第 2 条）
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP4`
- **agent_context**: 任务级 coder 子 Agent（fix 轮次的尾巴），**不继承** WP4 实现、wp4-fix1 或 review 会话；只按主 Agent 的派单（补存储层用例）作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：diff、门禁、四条红向证据均由本 Agent 亲自执行。派单**无清单外编辑**。
- **base / target_revision**: base = `2917401cdbfaa21dc97786d0a792c1081ebb805a`（开工时 `git rev-parse HEAD` 与之一致、`git status --porcelain` 为空）→ target = **`59bf1058851deb5b83c875a02f8046c815a9c4f0`**。提交后 `git status --porcelain` 为空、暂存区为空。
- **scope**（严格按派单允许写入面）：
  - 唯一改动文件：`crates/storage-sqlite/tests/admin_store.rs`（+442 / −3）。
  - **未动**：`crates/storage-sqlite/src/admin/trust.rs` 等任何生产代码（556d170e 的实现与 2917401 的 HEAD 逐字保持）、`docs/**`、`openspec/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、`Cargo.toml`/`Cargo.lock`、`core` 的替身（`core/src/broker.rs` 的 `FakeTrust` 仍按 wp4-fix1 的 `unreachable!` 留在原状）。
- **changes**（1 个提交，1 个文件，4 条新用例，用例数 36 → 40）：
  1. `record_node_connected_advances_the_access_row_once_forward`（§11.6 第 9 条 / spec `node-link-owner-server`「节点双向认证与凭据状态」的收尾副作用）：`(node, access)` 行 `last_connected_at` 首次写入 NULL → 本次时间；写入后除该列外**逐字段**与写入前相同（用 `NodeRecord::try_new` 重建期望行做整行相等断言，`owner_endpoint`/`state`/`grants`/`created_at`/`revoked_at` 一起锁住「只 `UPDATE` 一个时间列、不重写整行」）；`node.authenticated` 与时间同事务落库且归因**逐列**一致（`at|action|actor_kind|actor_id|via_node_id|local_principal_ref|target_kind|target_id|outcome|detail_digest`）；重启后保持；更早时间不倒退、更晚时间推进；三次认证留下三行审计（时间列是否推进不改变留痕义务）。
  2. `consume_pairing_advances_the_node_row_of_a_node_peer`（§11.6 第 8 条，`advance_node_connected_at` 的另一条调用路径）：首次认证在配对消费的同一写集里推进对端 `access` 行；同一对端的重复消费幂等且**不重复推进**；`Actor::Device` 的配对消费前后 `owned_node` 都为空（设备配对没有节点行）。
  3. `record_node_connected_refuses_missing_rows_and_wrong_roles`（失败关闭）：未知对端、只有 `access` 行的对端被要求写 `owner`、只有 `owner` 行的对端被要求写 `access`——三种都是 `PortError::NotFound(EntityRef::Node(该 id))`，且零审计、零行推进；命中角色的那一侧（`owner` 行 + `kind = Owner`）同一入口成功推进，证明被拒的原因是角色行不存在而不是入口本身不可用。
  4. `a_failed_audit_write_leaves_last_connected_at_untouched`（§11.2 第 6 条 / §9 判据 23）：`owned_audit` 上装 `RAISE(ABORT)` 触发器后调用 → `PortError::Backend(_)`，未认证过的行仍为 NULL（首次写入失败不留半状态）；移除触发器后同一写集成功；再装触发器推进已有值（`at(3)` → `at(5)`）失败 → 已有值保持 `at(3)`；最终只有成功的那一次留下审计行（1 行）。
  - 附带测试设施（同一文件内，非新文件）：`approve_node`（节点配对批准，复用既有 `node_pairing`/`node_claim` 夹具）、`node_actor`/`node_authenticated`/`node_connected`（§5.3 的 Node Link 归因形状）、`last_connected`（读面）、`audit_lines`（按列核对审计，返回逐行文本）。
  - 模块头用例索引表补一行（标明这一行来自 `node-link-owner` 的 spec，不属 admin-state-persistence-v2 的场景表）；`use acp_core::ports::NodeConnectedWrite` 一行。
- **checks**（原始输出见 `wp4-handshake.log` 的「wp4-fix1b 轮次」）：
  - `cargo fmt --all -- --check` exit=0（先由 `cargo fmt --all` 收敛两处换行）。
  - `cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked -p storage-sqlite --all-features` exit=0（14 个 test binary 全 ok、0 failed；`tests/admin_store.rs` 40 passed，其中本轮新增 4 条；2 个 `ignored` 是仓库声明过的 `crash_child` 与 `regenerate_v1_fixture`）。
  - `cargo test --locked --workspace --all-features` exit=0（旁证：83 条 `test result: ok` 行、885 passed / 0 failed / 2 ignored，无非通过 result 行）。
  - `npm run check` exit=0（16/16；`check:drift` 仍报「§7 的 36 条 DDL 与 `migrate.rs` 逐条一致；§5 的 15 个 trait / 90 个方法签名与 `ports.rs` 一致」——本轮未改文档，门禁只是随动确认）。
  - 提交前钩子（`.husky/pre-commit`）三步全通过（`cargo fmt --check` → `npm run check` → workspace clippy `-D warnings`），commitlint 通过；`gitleaks` 本机未安装（本地跳过，CI 的 `secrets` job 仍判定）。
  - **红向证据（四条，均「单点回退生产代码 → 同一条用例 FAIL → `git checkout --` 恢复」；恢复后 `git diff` 对该生产文件为 0 行）**：
    1. 回退 `advance_node_connected_at` 的「只前进」比较守卫（`if false`）→ 用例 1 FAIL：`更早的认证时间不得让已存值倒退`，`left: 2026-09-18T00:02:00.000Z` vs `right: …T00:03:00.000Z`。
    2. 删掉 `record_node_connected` 里的 `insert_audit_rows(...)` → 用例 1 FAIL（`认证成功的审计行必须与时间同事务、归因逐列一致`，`left: []`）+ 用例 4 FAIL（`expect_err` 拿到 `Ok`，审计写失败不再让写集失败）。
    3. 把 `advance_node_connected_at` 的行缺失分支从 `NotFound(EntityRef::Node)` 改成 `Ok(())` → 用例 3 FAIL（`an unknown node must be refused` 拿到 `Ok`）。
    4. 删掉 `consume_pairing` 写集里的 `if let Actor::Node { .. }` 推进段 → 用例 2 FAIL（消费后 `last_connected_at` 仍是 `None`）。
  - **用例计数**：`crates/storage-sqlite/tests/admin_store.rs` 的 `#[tokio::test]` 36 → 40（+4，无删除、无改写既有用例）。
- **issues**: 派单的 4 项要求全部落地；无阻断、无新增不确定项；**没有发现生产缺陷**（四条红向证据都是人为回退，不是现场 bug）。派单允许写入面内的每一处改动都只影响测试。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表**独立复核（RV2-WP4）、`[PV5]`、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp4-fix1b-handoff.md`
  - 被补的缺口登记：`openspec/changes/node-link-owner/reports/wp4-fix1-handoff.md` §「残留风险与下游注意」第 2 条
  - 被复核的 review：`openspec/changes/node-link-owner/reports/rv1-wp4.md`（F2 P1）
  - 本轮原始输出（含四条红向证据）：`openspec/changes/node-link-owner/reports/wp4-handshake.log` 的「wp4-fix1b 轮次」
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`。临时文件只写在系统临时目录（`/tmp/wp4fix1b/`，非仓库内）；提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `59bf1058851deb5b83c875a02f8046c815a9c4f0` | `test(storage): 认证收尾写集补存储层用例` | `crates/storage-sqlite/tests/admin_store.rs`（1 file, +442/−3） |

（经 `.husky/pre-commit` 三步与 commitlint；`git status --porcelain` 为空。）

## 逐条落点（reviewer 复检索引）

| 派单条目 | 断言的东西 | 用例 |
|---|---|---|
| ① 首次写入：NULL → 给定时间戳，审计同事务落库 | `last_connected_at` 从 `None` 变 `Some(at(3))`；`audit_lines` 逐列等于 `2026-09-18T00:03:00.000Z\|node.authenticated\|node\|99999999-…/99999999-…\|99999999-…\|∅\|node\|99999999-…\|success\|∅` | `record_node_connected_advances_the_access_row_once_forward` |
| ② 只前进不倒退 | `at(3)` 之后写 `at(2)` 仍是 `at(3)`、写 `at(4)` 变 `at(4)`；两次的审计行都写（共 3 行） | 同上（红向 ①） |
| ③ 失败关闭/边界：未知节点、角色不符 | 未知对端 / `Access` 行冒充 `Owner` / `Owner` 行冒充 `Access` 都是 `NotFound(EntityRef::Node)`，零审计、`last_connected_at IS NOT NULL` 计数为 0；命中角色的一侧成功 | `record_node_connected_refuses_missing_rows_and_wrong_roles`（红向 ③） |
| ④ 其他不变式：同一事务性（§11.2 第 6 条） | 审计写失败 → `Backend(_)` 且未认证过的行仍为 NULL、已有值不推进；移除触发器后成功；只有成功的那次留审计 | `a_failed_audit_write_leaves_last_connected_at_untouched`（红向 ②） |
| 补充：`advance_node_connected_at` 的另一条调用路径（§11.6 第 8 条） | 首次认证在 `consume_pairing` 写集里推进 `access` 行；重复消费不重复推进；设备配对不产生节点行 | `consume_pairing_advances_the_node_row_of_a_node_peer`（红向 ④） |

## 语义对照（本轮的断言都取自既有权威文本，未新增语义）

1. `docs/CORE_PORTS_AND_STORAGE.md` §11.6 第 9 条：`record_node_connected` 把 `(node, kind)` 行的 `last_connected_at` 推进到 `context.at`、值只前进不倒退、`context.audit` 同一事务、行不存在 → `NotFound(EntityRef::Node)`、不设容量门。
2. 同文档 §11.6 第 8 条：`consume_pairing` 在同一事务推进对端节点行的 `last_connected_at`；`Actor::Device` 的配对没有节点行；已是 `consumed` 的重复提交不重复推进。
3. 同文档 §11.2 第 6 条 / §9 判据 23：审计写失败 → 整事务回滚（本用例用 `owned_audit` 上的 `RAISE(ABORT)` 触发器注入，与既有 `a_failed_audit_write_leaves_the_pairing_approved` 同一手法）。
4. 同文档 §5.3 的归因形状（`actor`/`via_node` 都是该对端、`target = Node(对端)`、`localPrincipalRef = null`、`detailDigest = null`）；`specs/node-link-owner-server/spec.md` 的「节点双向认证与凭据状态」要求认证成功的收尾副作用随写集在同一事务提交。
5. **没有**为通过测试放宽或扩展任何语义：失败分支全部按生产代码现状断言（角色不符即 `NotFound`），未给生产代码加状态门、未断言未写明的行为；`NodeKind::Owner` 那侧的成功断言只用来证明「角色是谓词的一部分」（Node Link 恒传 `access`，用例注释已写明这一点）。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP4-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0"
    evidence_type: CHECK
    evidence_id: RV1-WP4
    report_path: "reports/wp4-fix1b-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 2917401 → 59bf105 上只改 crates/storage-sqlite/tests/admin_store.rs（+442/−3，36 → 40 条用例）：record_node_connected_advances_the_access_row_once_forward（首次写入非空、整行只动一列、审计归因逐列一致、更早不倒退/更晚推进、三次认证三行审计、重启保持）、consume_pairing_advances_the_node_row_of_a_node_peer（§11.6 第 8 条路径 + 幂等不重复推进 + 设备配对无节点行）、record_node_connected_refuses_missing_rows_and_wrong_roles（未知对端与两个方向角色不符均 NotFound(Node)，零审计零推进）、a_failed_audit_write_leaves_last_connected_at_untouched（审计写失败整事务回滚）。四条红向证据为单点回退生产代码后同一用例 FAIL（更早时间倒退 / 审计缺失且失败不再冒泡 / 未知对端静默成功 / 消费侧不推进），恢复后 git diff 对该文件 0 行；cargo fmt、clippy -p storage-sqlite -D warnings、cargo test -p storage-sqlite（14 binary 全绿，admin_store 40 passed）与 npm run check（16/16）均 exit 0"
    source_evidence: NOT_APPLICABLE
```

## 残留风险与下游注意

1. **待独立复核**：本报告是 coder 自检（`[PV3]` 范围），不代替 RV2-WP4 的复检、`[PV5]`（3.7/3.9）或候选门禁；建议 reviewer 在 `59bf105` 上核对四条用例与四条红向证据，并确认生产代码确实未被本轮触碰（`git diff 2917401 59bf105 --stat` 只有 1 个测试文件）。
2. **wp4-fix1-handoff 第 2 条缺口的处置**：该条已由本轮关闭；同一段里的第 3 条（`core` 的 `FakeTrust` 只给 `unreachable!`、不做真实镜像）**仍未处置**——它不是本轮派单项，`core` 的替身行为不变。
3. **未覆盖的相邻面（明确登记）**：`put_node`/`revoke_node` 路径对 `owned_node` 的 `revoke_reason` 列（DDL 有该列，`node_from_row` 不读它）不在本轮范围内；`record_node_connected` 对 `revoked` 状态行的行为**没有任何权威文本**，因此本轮有意不对它断言（若 reviewer 认为「撤销后的认证收尾」需要明确语义，应先在 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 补一句，再补用例——那属于新决定，不是本轮 fix）。
4. **`context.audit` 为空的处理**：§11.6 第 9 条说「`context.audit` 必须非空」（安全动作），但该约束的**执行点**在 `UseCases::record_node_connected`（总会带一条），存储层 `record_node_connected` 对空集合是「只推进时间、不写审计」。本轮按生产代码现状未断言空集合（避免把「存储层不校验」固化成期望语义）；若需要存储边界也失败关闭，同样属于新决定。
5. **换行风格的一次性差异**：`admin_store.rs` 编辑后被写成 LF 工作区文件（仓库其余检出为 CRLF，`.gitattributes` 只对 pin 的文件固定 `eol=lf`）。提交前后 `git show HEAD:<file>` 与 `HEAD~1:<file>` 都是 LF、`git diff --numstat` 为 `442 3`（无整文件重写），仓库内容与 `.editorconfig` 的 LF 口径一致；下次 `git checkout` 会按 `core.autocrlf=true` 转成 CRLF。
6. **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 生效，本地无等价物（提交钩子已提示「本地密钥扫描已跳过」）；`npm run verify` 的等价面本轮已全跑（`npm run check` + fmt + clippy + test），上述两类 CI 判定仍未在本地执行。
