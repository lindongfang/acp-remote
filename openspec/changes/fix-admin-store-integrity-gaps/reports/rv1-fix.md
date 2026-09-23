## 固定字段

| 字段 | 值 |
| --- | --- |
| task_id | RV1 |
| role | reviewer（独立代码检视子 Agent；isolated，未参与实现，未继承实现对话） |
| phase | 工作包/WP 交付前（branch validation，候选合入之前） |
| agent_context | 新上下文子 Agent；只读检视；未修改任何文件、未切换分支、未提交、未勾任务、未写 `verification.md`；未执行任何 shell 命令 |
| target_revision | base `1ef6640431079f9ca22b409c42071de2d9a79fa2` + 工作区未提交改动（调度者声称 `tracked_diff_sha256 = 1341369ae898f0627ad9ac751bebb53ddb0e1ceec4483443d7d1c79cabbd80fc`；我按内容逐 hunk 比对确认，见 Assessment 的版本稳定性一节） |
| scope | 6 个被修改文件（`admin/export.rs`、`admin/trust.rs`、`session_store.rs`、`tests/admin_store.rs`、`tests/imported.rs`、`docs/CORE_PORTS_AND_STORAGE.md`）及其调用方与数据流：`put_export`/`revoke_export`/`add_import`/`remove_import`、`upsert_session`/`commit_receipt`/`ack`/`drop_import`/`prune`、`load_peer_key`/`pairing_peer_from_row`/`device_from_row`/`load_existing_device_key`/`device_identity`/`put_device`/`put_node`/`approve_device`/`approve_node`/`upsert_device`/`upsert_node`、§5.2/§5.3/§7.4/§9/§11.2 文档落点 |
| changes | 4 条守卫 + 4 组回归用例 + 文档同步；DDL/`migrate.rs`/`core` 端口与值对象/`schemas/`/`fixtures/`/`compatibility/`/`LOCAL_ADMIN_PROTOCOL.md` 零改动 |
| checks | LC1、PV1–PV5 证据逐项核对（读日志，未执行）；`cargo-deny`、`gitleaks` 记「本地未执行」 |
| issues | RV1-F1（MINOR/P2）、RV1-F2（SUGGESTION/P2）；无 CRITICAL/MAJOR |
| result | **PASS**（对应 Target Revision：base `1ef6640` + 上述工作区改动层） |
| evidence_paths | `openspec/changes/fix-admin-store-integrity-gaps/reports/{branch-diff.patch,branch-diff-stat.txt,lc1-admin-store.log,lc1-imported.log,pv1-fmt.log,pv2-clippy.log,pv3-cargo-test.log,pv4-check.log,pv5-verify.log,baseline-targeted-tests.log}`；`proposal.md`、`specs/admin-state-persistence/spec.md`、`specs/peer-identity-material/spec.md`、`design.md`、`plan.md`、`tasks.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`AGENTS.md` |
| resource_cleanup | 未创建或删除任何文件；未运行构建/测试，因此未产生 `target/`、临时 SQLite 或日志资源；无残留 |

### Review Context

- Review ID / Type / Stage：**RV1 / branch / 工作包（WP1、WP2、WP3）交付前**
- Repository：`D:/Project/acp-remote`（Windows，本地 `refs/heads/main`）
- 读取的规则与需求：`openspec/schemas/agentic/roles/reviewer.md`（全文先读）、`AGENTS.md` §3/§4/§6/§7/§9/§10/§12、`proposal.md`（含 `agentic-intent` 四条缺陷与四点复核意见）、两份 `specs/**/spec.md` 增量、`design.md`（D1–D6 与 Risks）、`plan.md`（RV1 关注点、Coverage Index R1–R12）、`tasks.md`（2.1–2.7、3.2）、`docs/CORE_PORTS_AND_STORAGE.md` §5.2/§5.3/§7.4/§9 判据 29–30/§11.2 第 5 条
- 实际检查范围：base→target 的全部 6 个文件的实际 diff（自行读取 `branch-diff.patch` 全文与工作区源码逐 hunk 比对），以及所有读写 `owned_export`、`imported_*`、`owned_device`/`owned_peer_key`/`owned_pairing_peer` 的调用点与数据流
- 使用的验证证据：只读核对 LC1/PV1–PV5 日志与 `baseline-targeted-tests.log`；**未由我执行任何命令**，不冒充执行
- 限制：无法自算 sha256、无法使用 git 校验 base/工作区关系；不执行 E2E；不执行构建；`cargo-deny`/`gitleaks` 本地无等价物（CI-only）

### Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-F1 | MINOR（P2，非阻断） | `crates/storage-sqlite/tests/imported.rs:314`（`receipt_without_imported_session_is_rejected`）；相关：`session_store.rs:2475`（新守卫）、`session_store.rs:2487`（被绕过的分支） | 该用例的夹具只 `open` store 后直接 `commit_receipt`，**没有** `add_import`（对比同文件 `seed_session` 已在 99–101 行先 `seed_import`）。新守卫 `ensure_import_owns` 在 `commit_receipt` 的第一条语句先返回 `NotFound(Export)`，因此 `session_store.rs:2487` 的 `NotFound(EntityRef::Session(..))` 分支不再被触达；断言是泛化的 `matches!(error, PortError::NotFound(_))`，仍通过但对「哪个 NotFound」不敏感，用例文档注释（「会话行不存在时…」）与 `design.md` D5 的「既有 `receipt_without_imported_session_is_rejected`（无会话行 → `NotFound(Session)`）」陈述均已与事实不符 | §7.4 `[决定]`（「会话行不存在时 `commit_receipt` 返回 `PortError::NotFound`，不得静默推进 `next_local_sequence`」）失去唯一的定向回归；该分支在生产上是活路径（先 `import.add`、会话来元数据尚未同步或从未同步的会话收到收据 → 归属行在、会话行不在）。行为本身仍正确（代码路径仍在），只是不再被证明 | 在该用例开头加 `seed_import(&store).await;`，并把断言收窄为 `matches!(error, PortError::NotFound(EntityRef::Session(_)))`（保留 `imported_delivery_index` 计数为 0 的断言）；「从未导入」场景已由新用例 `late_callbacks_after_a_full_removal_cannot_rebuild_the_index` 的 `NotFound(Export)` 路径覆盖 | 待实现/测试 Agent 修复后由新 reviewer 复核 |
| RV1-F2 | SUGGESTION（P2，仅报告） | `openspec/changes/fix-admin-store-integrity-gaps/design.md:10`（及 `tasks.md` 2.2 同措辞） | design 称 `trust.rs`「模块头注释（`:1-22`）与 `pairing_peer_from_row`（`:156`）都写着『指纹列不参与判定』」；工作区 `trust.rs:1-22` 并无该句（base→target 的 diff 在该区间也没有 hunk，故 base 同样没有）。全仓库检索「指纹列不参与」只剩被删的那一行与 openspec 文档 | 无代码/行为影响；只是把「模块头注释修正」写成了一条实际不存在的改动项，属变更内文档的事实精度问题 | 把该句改为指向实际位置（`load_peer_key` 的文档注释与 `pairing_peer_from_row`），或在 tasks 2.2 注明模块头无需改动 | 不适用（非阻断，建议随本次一并订正或明确不修） |

**未发现问题（已核对为正确，逐条列证据）**

1. 四条守卫都在既有写事务内、先失败关闭后写入，无「部分写入后再报错」路径：`put_export`（`export.rs:321-347`）两个前置检查都在 `INSERT` 之前；`commit_receipt`（`session_store.rs:2475`）与 `upsert_session`（`session_store.rs:2673`）的 `ensure_import_owns` 都是各自 `BEGIN IMMEDIATE` 事务的第一条语句；提前返回时 `Transaction` 被 drop → 回滚，且此前无任何写。
2. `put_export` 判定顺序正确且 `revoked_at` 不会被清空：`export.rs:330`（`revoked_at` 非空 → `InvalidRequest`）先于 `export.rs:337`（库内已撤销 → `Conflict(ConflictKind::AlreadyExists)`）；`stored_revoked: Option<Option<String>>` + `.flatten().is_some()` 正确区分「无行」与「行内为 NULL」（`export.rs:340-346`）；`DO UPDATE` 写成 `revoked_at = CASE WHEN owned_export.revoked_at IS NOT NULL THEN owned_export.revoked_at ELSE excluded.revoked_at END`（`export.rs:353-357`）。无旁路：全仓库只有 `export.rs:348` 一处 `INSERT INTO owned_export`，`revoke_export`（`export.rs:401`）用 `COALESCE(revoked_at, ?2)` 只保留首次撤销时间，`add_import`/`remove_import` 不触碰 `owned_export`；`core::use_cases::put_export`（`use_cases.rs:650-680`）原样传参，无读回再写的调用者。
3. imported 归属守卫正确且确实阻止 `local_sequence` 推进与会话行重建：`ensure_import_owns`（`session_store.rs:2448-2465`）以 `imported_import_export` 的 `UNIQUE (owner_node_id, export_id)`（`migrate.rs:430`）为判据，错误定位为 `EntityRef::Export(export_id)`；`remove_import`（`export.rs:489`）同事务删 `imported_session` 与 `imported_import`（后者对关联行 `ON DELETE CASCADE`，`migrate.rs:425`）。无关旁路：`ack` 只做 UPDATE（无行则 0 行、不重建），`drop_import` 有意保留关联行与会话行（连接级清空，重连后必须继续接受收据），`prune` 只删交付索引与审计。
4. 身份材料读取核对覆盖三条路径且错误为 `Corrupt`：`load_peer_key`（`trust.rs:271-288`，`owned_peer_key`）、`pairing_peer_from_row`（`trust.rs:169-186`，`owned_pairing_peer`）、`device_from_row`（`trust.rs:64-88`，`owned_device`，`DEVICE_COLUMNS` 已含 `public_key`，`device()`/`devices()`/`load_device` 共用）与写路径兜底 `load_existing_device_key`（`trust.rs:1152-1166`）口径一致，均由 `verify_identity_material`（`trust.rs:208-217`）映射为 `StorageError::Corrupt` → `PortError::Corrupt`（`error.rs:91-92`）。唯一只读 `fingerprint` 的 `device_identity`（`trust.rs:298-312`）是写路径辅助：`put_device` 的 `load_peer_key`/`load_existing_device_key` 与 `trust.rs:430-435` 的三方比对、`approve_device` 的 `trust.rs:971-995` 都使矛盾行无法被静默使用（不同口径判定均失败关闭）。`owned_node` 无 `public_key` 列，无遗漏读点。
5. 活动时间用显式 `CASE` 且三分支正确：`upsert_device`（`trust.rs:1061-1065`）与 `upsert_node`（`trust.rs:1098-1102`）四分支顺序为「新值为空 → 保留旧值」「旧值为空 → 写新值」「新值更晚 → 写新值」「否则保留旧值」，结果只可能是两者之一或 NULL，未使用标量 `max`/`COALESCE` 组合；`Timestamp` 固定宽度（`docs` §3.2）保证字典序即时间序。
6. 用例可证伪、断言针对可观察结果、未出现同源手抄期望值导致的空转：四条新/扩用例在 base 上都会失败（`peer_key`/`device`/`devices`/`pairing_peer` 在 base 返回材料 → `expect_err` panic；`put_export` 在 base 会成功写入或复活 → `assert_conflict` panic；更早时间戳在 base 会倒退 → 断言 panic；移除后回调在 base 会重建 → `expect_err` panic）。断言使用端口错误变体（`PortError::Corrupt`）与 raw pool 的行快照/计数（`table_snapshot`/`scalar_i64`），`export_revocation_is_terminal_for_plain_writes` 同时断言表逐行不变、行数为 1、`owned_audit` 为 0，`timestamps_are_advanced_never_erased` 覆盖空→Some（`approve_device` 落 NULL 后首次写入）、Some(晚)→Some(早)（并断言 `display_name` 仍更新）、Some→None 三方，设备与节点各一遍；`corrupted_identity_material_fails_closed_on_read` 含「一致时四条读取路径照常」的正向对照。`assert_invalid_request(..., "use export.revoke to revoke an export")` 断言完整消息，非空转。
7. 无越界改动：`watchdog_diff` 的 tracked 改动清单与 per-file 计数（6 文件、+549/−67）与 `branch-diff-stat.txt` 逐项一致，无 `crates/core/**`、`migrate.rs`、`schemas/`、`fixtures/`、`compatibility/`、`LOCAL_ADMIN_PROTOCOL.md` 改动；未新增 `ConflictKind`/错误码取值（只用既有 `AlreadyExists`），未新增端口方法或值对象。新增字符串仅为 `InvalidRequest`/`Corrupt` 的消息文本。
8. 文档同步落在 fenced 代码块之外且未扰动既有编号：§5.2 新增 `[决定]` 在 `RemoteDeliveryStore` 的 ```rust 块之后（`docs:328-330` 区间），§5.3 新增三条 `[决定]` 在 `AttachmentStore` 块之后（`docs:681-684`），§7.4 新增 `[决定]` 在 `CREATE TABLE imported_import_export` 的 ```sql 块之后，§9 判据 30 追加在 29 之后且 29 措辞未改，§11.2 第 5 条仅原地细化。交叉证据：`pv4-check.log:34` 报告 `contract drift OK: §7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …`，`pv4-check.log:27` 文档引用门禁 `367 relative links, 2487 section refs` 通过。
9. 用户四点复核意见真实落实，残留缺口如实登记未伪装闭合：`NotFound(Export)` 定位（`session_store.rs:2461`）、判定顺序（`export.rs:330` 先于 `337`）、设备记录单读+列表读纳入（`DEVICE_COLUMNS` + `device_from_row`）、`CASE` 而非 `MAX`（`trust.rs:1061`，文档同款说明见 `docs` §5.3）；残留缺口在 `proposal.md` 的 `non_goals`、`design.md` 的 Risks、`docs` §5.2/§7.4「已知边界」与 §11.2 第 5 条四处一致登记，并明确「需导入实例标识或连接代际、v1 未实现」。

### Assessment

**Target Revision：base `1ef6640431079f9ca22b409c42071de2d9a79fa2` + 工作区改动层（tracked diff 内容与 `branch-diff.patch` 逐 hunk 一致，per-file 计数与 `branch-diff-stat.txt` 一致）→ 检视结论 PASS。**

- **结论**：已按 plan.md 的 RV1 九项关注点完成约定范围检视，无未解决的 CRITICAL/MAJOR；两条非阻断项（RV1-F1 MINOR、RV1-F2 SUGGESTION）建议在合入前顺手订正，但不阻断。对应本仓库严重度口径即 **无阻断项**。
- **版本稳定性**：工作区读到的 6 个文件内容与 `branch-diff.patch` 的每个 hunk 逐一吻合；`watchdog_diff(stat)` 与 `branch-diff-stat.txt` 的 6 行计数完全相同（+549/−67）。我**无法自算 sha256**（无 shell/Git 权限），也无法独立证明日志与当前工作区字节级对应，仅记录该限制。未发现版本不稳定迹象。

**按检查 ID 的证据核对（我只读日志，未执行命令）**

| Check ID | 我核对的证据 | 结论 / 是否影响本轮判断 |
| --- | --- | --- |
| LC1 | `lc1-admin-store.log`：`running 32 tests`、`admin_store.rs` 的 `#[tokio::test]` 计数=32，含 `export_revocation_is_terminal_for_plain_writes`、`corrupted_identity_material_fails_closed_on_read`、`timestamps_are_advanced_never_erased`；`lc1-imported.log`：`running 10 tests`、10 passed，含 `late_callbacks_after_a_full_removal_cannot_rebuild_the_index`。与 `verification.md` 的「42 passed、0 failed」一致 | 已核对，一致；不改变本轮判断 |
| PV1 | `pv1-fmt.log`：`cargo fmt --all -- --check`，`# exit: 0` | 已核对，一致 |
| PV2 | `pv2-clippy.log`：`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，`# exit: 0` | 已核对，一致 |
| PV3 | `pv3-cargo-test.log`：共 **38** 条 `test result: ok` 行、0 failed（与「38 个 test result: ok」相符），其中 `admin_store` 32、`imported` 10，新用例名各自在列 | 已核对，一致 |
| PV4 | `pv4-check.log`：`schema fixtures OK`、`command catalog OK: 12`、`error registry OK: 58`、`feature registry OK: 11`、`contract assets OK`、`ACP matrix OK`、`doc links OK`、`crate boundaries OK`、`contract drift OK: §7 的 36 条 DDL…§5 的 15 个 trait / 87 个方法签名`、`check:agentic` PASS → 摘要「contract drift OK：§7 36 条 DDL、§5 15 trait/87 方法」属实 | 已核对，一致（同时印证新增文字确实落在 fenced 块之外） |
| PV5 | `pv5-verify.log`：`npm run verify → npm run check && npm run check:rust`，文件末尾 `# exit: 0` | 已核对，一致 |
| cargo-deny / gitleaks | 报告中如实记为「本地未执行」；我在 `reports/` 下未找到对应日志，未发现冒充执行的痕迹 | 属 CI-only，不属本阶段必要材料；不影响本轮代码判断，但合入/最终验收前仍需由 CI 补齐 |

**未验证内容（不影响本轮判断，但需在后续门禁前补齐）**

1. 我未执行 `cargo fmt/clippy/test`、`npm run check/verify` 或任何 git 命令；上述结论基于静态阅读与日志核对，**不构成**我本人运行通过的声明。
2. `cargo-deny`、`gitleaks` 本地无等价物，未执行（CI 覆盖）。
3. 未做运行时/动态验证与 E2E（本变更 `mode = not-applicable`，reviewer 也不执行 E2E）。
4. `server`/`app`/`node-link-client` 尚未实现：`core` 目前没有调用 `RemoteDeliveryStore::upsert_session` 的生产代码（`broker.rs` 只用 `commit_receipt`），`put_export` 现仅由未落地的本地管理适配器调用，因此四条守卫在当前树上主要是库级契约与后续适配器的前置约束，真实链路影响无法在本轮动态验证；本变更也未触碰 `compatibility/` 封闭词表与 `LOCAL_ADMIN_PROTOCOL.md` 方法集，故不影响其行为面。
5. RV1-F1 指出的覆盖率缺口使「会话行存在性 → `NotFound(Session)`」这一既有合同失去定向回归，属静态可判定的证据缺口，不改变「实现是否满足合同」的结论（实现仍满足）。

**残留风险（供主 Agent 决策）**

- 重导入后用 `(ownerNodeId, exportId)` 无法区分新旧连接的迟到回调：已按用户意见登记为 `non_goals`/Risks/文档「已知边界」，需导入实例标识或连接代际（属需用户决策项），本变更为其保留了显式注记而非伪装闭合。
- 既有库中若已存在指纹与公钥不一致的行，读取立即失败关闭且 `devices()` 会整体报错、不提供自动修复（`design.md` Risks 明示为有意行为）。
- RV1-F1 的定向覆盖缺口（P2）：若上游后续依赖该用例证明 `NotFound(Session)` 语义，会得到假阳性。