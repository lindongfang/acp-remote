# admin-state-persistence-v2 — 执行记录

## Target

- 变更：`admin-state-persistence-v2`（schema `agentic`）
- 仓库：`D:\Project\acp-remote`
- 主分支引用：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（`git rev-parse`）
- 实现分支：`feat/admin-state-persistence-v2`，起点 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`（= `refs/heads/docs/pre-implementation-contracts` HEAD，main 的后代）
- **W0 基线提交**：本记录所在的那个提交（分支 `feat/admin-state-persistence-v2`；其父提交是 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`）。W0 的完成条件（合同漂移门禁绿 + 全部本地门禁绿）就在该状态上成立，W1 的三条轨道以此为固定基线。
- 本轮证据对应的版本：**W0 基线提交**（不再是未提交工作区）；该提交包含 core 的 WP1–WP3、`storage-sqlite` 的 v2 DDL/migration、合同并入与全部 W0 证据日志。
- 版本确认负责人：主 Agent（`git rev-parse`/`git status`/`git worktree list`）

### W1·WP6（管理 store）

- 上游基线：`013f2b93a77a31cec3e1a18f589c5e984f013128`（W0 基线提交，`git rev-parse HEAD`）
- 本轮改动（工作区 → 见 `Merge History`）：`crates/storage-sqlite/src/admin/**`（新增）、`tests/admin_store.rs`（新增）、`src/{lib.rs,error.rs,session_store.rs}`、`Cargo.toml`/`Cargo.lock`（`storage-sqlite` 新增 `serde`/`serde_json` 直接依赖，版本不变）
- 任务：`1.4`（上游交接核对）、`2.19`–`2.22`（三个管理 store 与失败关闭）、`3.9`（交付前 project verify）、`3.10`（独立 review）

### W1·WP6 闭合轮（WP6-2 / WP6-4）

- 上游基线：`1baea5b`（WP6 首版提交）+ 工作区改动（本轮闭合，见下与 `Merge History`）
- 改动面：`crates/core/src/model/{identity,tests}.rs`（`PairingRecord`/`PairingPeer` 增 `host_binding`）、`crates/storage-sqlite/src/admin/trust.rs`（写真实绑定、认领比对、节点批准 `approve_node`/`approve_device`、`pairing_peer` 回填）、`tests/admin_store.rs`（25 用例）、`docs/CORE_PORTS_AND_STORAGE.md`（§3.5/§11.1/§11.2/§11.6 措辞）
- **未改动**：§5.3 与 `crates/core/src/ports.rs` 的写集签名、§7.3 的 DDL（`owned_pairing.host_binding` 列在 v2 里本就存在）、任何 wire schema 与 fixture —— 因此漂移门禁的判据面（§7 36 条 DDL、§5 15 trait/87 方法）保持不变

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PV4 / WP1 交付前 | `28f8cb9` + 工作区 | `core` 的 fmt/clippy/测试（值对象、端口、用例） | 主 Agent（coder 角色） | `cargo fmt --all -- --check`；`cargo clippy --locked -p core --all-targets --all-features -- -D warnings`；`cargo test --locked -p core --all-features` | rustc 1.98.1（`rust-toolchain.toml`），Node 24.19.0 | **PASS**：fmt 退出 0；clippy 无输出（无 lint）；`75 passed; 0 failed` | `reports/wp1-core-tests.log` |
| PV4 / WP2+WP3 交付前 | `28f8cb9` + 工作区 | 端口目标签名、写集 DTO、`UseCases` 写集路径与 workspace 解析 | 主 Agent（coder 角色） | 同上命令（同一固定版本） | 同上 | **PASS**：`75 passed; 0 failed`（含新增 5 个用例） | `reports/wp3-core-tests.log` |
| PV3 / WP5 | `28f8cb9` + 工作区 | 依赖方向矩阵 + `core` 普通依赖闭包 allow-list | 主 Agent | `node scripts/check-crate-boundaries.mjs` | 本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | **PASS**：`crate boundaries OK`（6 个 crate 已登记） | `reports/wp5-boundaries.log` |
| PV1（部分）/ 全工作区 | `28f8cb9` + 工作区 | `cargo test --locked --workspace --all-features` | 主 Agent | 同左 | 同上 | **PASS**：37 个测试目标全部 `ok`（含 `storage-sqlite` 的既有 migration/retention/enum_coverage 等） | 本轮终端输出（未落盘；PV1 完整证据待 6.3/6.7） |
| PV2 / 合同漂移 | W0 基线提交（父 `28f8cb9`） | §5.3/§7.3/§7.4 与 `ports.rs`/`migrate.rs` 逐条一致 | 主 Agent | `node scripts/check-contract-drift.mjs` | 只读脚本 | **PASS**：`contract drift OK: §7 的 36 条 DDL 与 crates/storage-sqlite/src/migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates/core/src/ports.rs 一致`（改前红→改后绿两段都留档） | `reports/wp5-contract-drift-before-after.md` |
| PV3 / WP5（W0 复跑） | W0 基线提交 | 依赖方向矩阵 + `core` 普通依赖闭包 allow-list | 主 Agent | `node scripts/check-crate-boundaries.mjs` | 本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | **PASS**：`crate boundaries OK`（W0 复跑段追加在文件末尾） | `reports/wp5-boundaries.log` |
| PV4 / WP4 交付前（含 2.15–2.18） | W0 基线提交 | `storage-sqlite` 的 fmt/clippy/测试（v2 DDL、12-step 重建、Import 拆分迁移、v2 夹具与升级保留断言） | 主 Agent | `cargo fmt --all -- --check`；`cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings`；`cargo test --locked -p storage-sqlite --all-features` | rustc 1.98.1（`rust-toolchain.toml`），Node 24.19.0 | **PASS**：fmt 退出 0；clippy 退出 0（无 lint）；11 个测试目标全部 `ok`（`migration` 6 passed / 1 ignored 为夹具生成器、`imported` 9、`retention` 8、`enum_coverage` 2、`permissions` 4、`commit` 15、`contract_v03` 5、`compaction_recovery` 3、`attachments` 5、lib 单测 3） | `reports/wp4-migration-tests.log` |
| PV1（部分）/ 工作区 Rust（W0） | W0 基线提交 | `cargo fmt` / `cargo clippy --workspace` / `cargo test --locked --workspace --all-features` | 主 Agent | 同左 | 同上 | **PASS**：fmt 与 clippy 退出 0；workspace 全部测试目标 `ok`、0 failed（37 个目标；`core` 75 用例不变） | `reports/w0-verify-rust.log` |
| PV1 / `npm run check`（W0） | W0 基线提交 | 十道合同门禁全绿（含 `check:docs` 与 `check:drift`） | 主 Agent | `npm run check` | Node 24.19.0、仓库内 `node_modules`，不联网 | **PASS**：退出码 0（完整输出留档） | `reports/w0-npm-check.log` |
| PV4 / WP6 交付前（2.19–2.22） | `013f2b9` + 工作区（**独立 review 修复后**） | `storage-sqlite` 的三个管理 store（TrustStore/ExportStore/LocalConfigStore）、容量门与失败关闭映射；`core` 与 workspace 回归 | 主 Agent | `cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked -p core -p storage-sqlite --all-features`；`cargo test --locked --workspace --all-features` | rustc 1.98.1（`rust-toolchain.toml`） | **PASS**：fmt 退出 0；workspace clippy 退出 0；`core` 75 passed；`storage-sqlite` 12 个测试目标全 `ok`、workspace 全部目标 `ok`，其中新增 `admin_store` **23 passed; 0 failed**（并发认领、落定只一次、拒绝/过期不建信任、双角色与角色指纹不一致、撤销重启、时间戳只推进、Import 归属与完整移除、默认唯一、非法绑定、种子幂等、版本递增、无秘密、审计与落定回滚注入、损坏库写路径全拒、容量拒绝新写入） | `reports/wp6-admin-store-tests.log` |
| PV3 / WP6（复跑） | `013f2b9` + 工作区 | 依赖方向矩阵 + `core` 闭包 allow-list（`storage-sqlite` 新增 `serde`/`serde_json` 直接依赖后复跑） | 主 Agent | `node scripts/check-crate-boundaries.mjs` | 本地 `cargo`，不联网 | **PASS**：`crate boundaries OK`（输出并入 WP6 日志） | `reports/wp6-admin-store-tests.log` |
| PV1 / `npm run check`（WP6） | `013f2b9` + 工作区 | 十道合同门禁全绿（`check:drift` 仍报 §7 36 条 DDL、§5 15 trait/87 方法一致） | 主 Agent | `npm run check` | Node 24.19.0、仓库内 `node_modules`，不联网 | **PASS**：退出码 0 | `reports/wp6-admin-store-tests.log` |
| PV4+PV2+PV3+PV1 / WP6 闭合轮 | `1baea5b` + 工作区（含 `RV1` 的 WP6-2/WP6-4 闭合） | 节点批准与绑定校验落地后的 core + storage；契约文本改动后重跑四类门禁 | 主 Agent | `cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked -p core -p storage-sqlite --all-features`；`cargo test --locked --workspace --all-features`；`node scripts/check-contract-drift.mjs`；`node scripts/check-crate-boundaries.mjs`；`npm run check` | rustc 1.98.1、Node 24.19.0 | **PASS**：全部退出 0；`core` 75 passed；`admin_store` **25 passed**（新增节点批准与绑定不一致拒绝两例）；`check:drift` 仍 36 条 DDL / 15 trait / 87 方法 | `reports/wp6-admin-store-tests.log`（闭合轮复跑段） |

## Check Plan Changes

本轮实现相对原计划的三处偏差，均已同步到代码注释与 `tasks.md`，需要在任务 2.1 写入合同时一并反映：

1. **`EntityRef` 新增 `Provider(String)` 变体**（`crates/core/src/model/ids.rs`）：§11.6 要求 `provider.configure` 的写集必须携带 `provider.configured` 审计，而 `PendingAudit.target` 是 `EntityRef`，原枚举没有任何可表达 Provider 引用的变体。新增变体后 `kind()` = `"provider"`、`target_id()` 为 Provider 引用 id。影响：§3.1 的 `EntityRef` 行、§7.3/§7.4 审计表的 `target_kind` 取值（存储侧实现时）需一并说明；不影响 wire（`EntityRef` 不出现在协议载荷里）。
2. **`UseCases::create_session` 增加 `workspace_alias: Option<WorkspaceAlias>` 参数**：§3.6 的目标形状把 `CreateSessionRequest.workspace` 改成 `Option<ResolvedWorkspace>`，因此 alias 必须在进入该结构之前解析；解析归 core（§11.9），Export 侧「alias 是否声明」的校验由 `server::node_link` 负责。
3. **`ImportWrite.exports` 与 `record.export_ids()` 必须相等**：§11.6 的 DTO 同时含有 `record.export_ids`（既有模型）与 `exports`，两者分歧会让管理行与关联行不一致；端口文档明确要求存储层返回 `InvalidRequest`。
4. **`owned_audit`/`imported_audit` 的 CHECK 只在 `migrate.rs` 常量的**新建路径**中扩宽（`+5` 个取值）**：这是为了让 `crates/storage-sqlite/tests/enum_coverage.rs` 恢复绿。**v1 既有库的 12-step 表重建（任务 2.15）与非空 DB 的取值可用性仍未实现**——本轮结束时「新建库可写新审计动作、升级库不可写」的不一致状态**不可发布**，必须在任务 2.15/2.17 完成后复核。
5. `UseCases::audit_action` 暂时保留 `#[allow(dead_code)]`：管理写集不再使用它（审计随 `WriteContext` 提交），但保留给尚无关联状态变更的审计入口。
6. **执行模型变更（2026-09-23，本轮暂停时记录）**：原计划的 W1「WP5+WP1 并行」在单会话下被合并为一段串行实现，2.1/2.2（合同并入）被推到实现之后。后续改用多执行者后重新定为 **W0 串行冻结（2.14 → 2.1 → 2.2 + 基线提交）→ W1 三轨道并行（WP4 剩余 ∥ WP6 ∥ WP1–WP3 的独立 review）→ W2/W3/W4**，见 `plan.md` 的 Execution Waves 与多 Agent 分派规则。该重排不改变任何任务编号与要求。
7. **本轮的 2.14 半成品归属**：`migrate.rs` 两条审计 CHECK 的扩宽（为恢复 `enum_coverage`）并入 W0 的 2.14，必须与 12-step 重建语句一起定稿；在此之前该状态**不可发布**（F1）。

### WP6 执行期的决定（2026-09-23，W1 的 WP6 轨道）

13. **`expire_pairings` 对从未被认领的配对回填 `claimed_at`**：`owned_pairing` 的 `CHECK ((state = 'created') = (claimed_at IS NULL))`（§7.3，冻结）不允许「非 `created` 但 `claimed_at` 为空」的行，而 §11.6 第 6 条要求终结**未确认**且已过期的配对（含从未被认领的 `created` 行）。因此扫描在写 `state = 'expired'`/`terminal_at` 的同时以 `COALESCE(claimed_at, at)` 落值：对这一类行，`claimed_at` 的语义是「离开 `created` 的时刻」。不这样做只能让这类行永远停在 `created`（「已过期」就不再是可见终态）。
14. **种子标记落在 `meta.local_config_seeded_at`**：§11.7 的表清单里没有承载「本地配置已初始化」的列/表，而 §11.8 第 4 条要求种子与标记**同事务**提交。`meta` 是既有的库级键值表（§7.2/§7.3），新增一个键不改变表结构；`mark_seeded` 在标记已存在时**零写入**返回（spec 的「重复打开不重导种子」）。
15. **容量门加在「会新增行」的管理写路径上**：`admin::enforce_capacity_gate` 复用 `session_store::enforce_capacity`（同一份 ①→②→③ 顺序与 `StorageFull` 判据），调用点是 `put_device`/`put_node`/`create_pairing`/`claim_pairing`/`settle_pairing`/`put_export`/`add_import`/`put_profile`/`put_workspace`/`put_provider_ref`/`mark_seeded` 的提交前。撤销（`revoke_*`）、完整移除（`remove_import`）与过期扫描**不加**：它们不增长库且是安全动作，容量不足时仍必须可用（§11.2 第 4/5 条要求撤销先提交再阻断访问）。
16. **节点配对的批准路径失败关闭**：冻结的 `PairingSettlementWrite` 只携带 `pairing` + `settlement`，没有节点角色（`NodeKind`，由 `node.pair.begin` 的 `mode` 决定，`PairingRecord` 也不含它），因此 `settle_pairing` 对 `PairingTarget::Node` 的 `Approved` 返回 `InvalidRequest`（零写入）。批准节点配对必须先经 `UseCases::put_node`（§11.6 第 2 条把「写节点角色行与身份材料」定在那里）；拒绝路径不受影响（它不创建信任）。
17. **`ExpiryWrite.context.audit` 只作为出处，不再整体追加**：`pairing.expired` 由存储层**按配对**写入（`insert_expiry_audit`，actor 取 `context.audit` 首条）；若同时把 `context.audit` 整体追加，一次扫描会为同一条配对留下两条同动作审计。`UseCases::expire_pairings` 传空集合（core 的文档把 `pairing.expired` 的写入定在存储层），此时不产生额外审计行。
18. **`storage-sqlite` 新增 `serde`/`serde_json` 直接依赖**：管理表的集合字段是「有类型的 JSON 数组文本」（§11.1），编解码只属于本适配器（数据库 record 不进 core），因此依赖落在这里而不是 core；`Cargo.lock` 只增加 `storage-sqlite` 依赖项的 2 行，没有版本变化。
19. **BLOB 列的读取**：`owned_peer_key.public_key`/`owned_device.public_key` 是 BLOB，新增共用 `blob()` 读取（读不到 BLOB 即 `ColumnValue` 损坏），公钥一律经 `PeerPublicKey::try_from_bytes` 还原，指纹由 `PeerPublicKey::fingerprint()` 从同一份字节派生（不读 `fingerprint` 列作判据）。

### WP6 独立 review（RV1，2026-09-23）驱动的修复与记录

20. **`settle_pairing` 补 `pending_confirmation` 状态守卫**（review `WP6-1`，已修）：原实现只排除终态（`rejected|expired|consumed`），而 `approved` 不是终态，于是「重复批准」会改写首次 `approved_at` 并重写信任行、「批准后再拒绝」会撞 `owned_pairing` 的 `(state IN ('approved','consumed')) = (approved_at IS NOT NULL)` CHECK 并把约束失败落进未具名的 `PortError::Backend`。现在落定前用正向谓词（`state = 'pending_confirmation'`，与 `claim_pairing` 的 `state = 'created'` 同款）判定：终态 → `terminal_conflict`，其余（`approved`/不可达的 `claimed`）→ `Conflict(Consumed)`；两条 UPDATE 也改用正向谓词并对 `rows_affected == 0` 失败关闭。回归用例：`a_pairing_can_be_settled_only_once`（重复批准与批准后拒绝都是 `Consumed`，首次 `approved_at` 与审计不变）。
21. **`last_seen_at`/`last_connected_at` 只推进不抹掉**（同一 review 的附带发现，已修）：`upsert_device`/`upsert_node` 改用 `COALESCE(excluded.*, *)`，一次不带新时间戳的写入不得让「最近一次认证成功/连接时间」回到未知。回归用例：`timestamps_are_advanced_never_erased`。（同一处的教训：**SQL 字面量里不得写 `--` 注释**——`\` 续行会把后续赋值吞进注释，本处实现过程中一度踩到，已改为 Rust 注释。）
22. **`host_binding` 缺口在记录里闭环**（review `WP6-4`）：实现写空串且 claim/settle 无绑定可校验，属**未闭环的合同缺口**（§11.1 把该列定义为「本节点的 identity/origin 或 endpoint 绑定」，§11.2 第 1 条要求认领时校验「本机绑定一致」，而冻结的 `PairingWrite`/`PairingClaimWrite` 都不携带该事实）。review 判定「必须修（合同侧）」；本轮只把它从代码注释提升为**记录在案的缺口**（本节与 `Review Findings`），修复面在合同/端口侧（把绑定并入写集，或改述 §11.1/§7.3 为「调用方校验、不持久化」），需与 `WP6-2` 一并决定。
23. **`ExpiryWrite.context.audit` 的语义加固建议未采纳但已记录**（review 对自述缺口 6 的建议）：可对「非空且非 `pairing.expired`」的 `context.audit` 返回 `InvalidRequest`，或把「本写集忽略 `context.audit`」写进 §11.6；本轮保持现状（core 的唯一调用方传空集合），留作合同措辞的一次选择。

### WP6 闭合轮的决定（2026-09-23，用户批准 A 路）

24. **`PairingRecord`/`PairingPeer` 增 `host_binding: String`（闭合 WP6-4，模型层改动）**：绑定在本机登记时写入 `owned_pairing.host_binding`，认领时要求对端逐字回显。**不需要 DDL 或端口改动**——该列在 v2 DDL 里本就存在（§7.3），而 `PairingWrite`/`PairingClaimWrite` 的形状不变（绑定随 record/peer 走）。`owned_pairing_peer` **不**存该列：认领时已校验两侧逐字相等，因此对端行的绑定恒等于配对行的值，读取时从配对行回填（这让「两条绑定不一致」在库内不可能存在）。
25. **节点批准的角色推导（闭合 WP6-2）**：节点配对行只由 `node.pair.begin --mode owner` 创建，而 `LOCAL_ADMIN_PROTOCOL.md` §5.4 明确「`--mode access` 本机没有 `confirm` 调用」（claim 的 `nodeKind` 也固定为 `access`），因此批准时的对端角色恒为 `NodeKind::Access`、`owner_endpoint` 为 `None`——角色**可推导**，无须把它并入写集，也就不必改 §5.3/`ports.rs`/DDL，更不会让 W0 的契约与夹具证据作废（§11.2 第 2 条已写入这条推导）。
26. **绑定的比较语义定在存储层「逐字相等」**：比 `NODE_LINK_PROTOCOL.md` §13.2 的 endpoint **host** 级匹配更严（对端必须回显登记时宣告的那个值）。理由是登记值与回显值同源（QR 里的 `canonicalOrigin`/`endpoint` 被逐字带进 claim），逐字相等既能覆盖 host 级检查，也能发现「claim 指向了另一次配对/另一台机器」；host/Origin 级的协议检查仍由 `SYNC_PROTOCOL.md` §7.2 与 Node Link 的 403 路径负责，文档已写明两者分工。

27. **闭合轮独立 review（`Wp6ClosureReview`）的两处修复**（结论 `correct`、无阻断项）：① `pairing_from_row` 把「库里 `host_binding` 为空」按 `Corrupt` 具名报告，不再落进通用 `InvalidRequest("value does not satisfy its domain shape")`——该状态只可能来自外部改写或本轮之前写空绑定的构建，分类必须指向库内状态（回归用例 `an_empty_stored_binding_is_reported_as_corrupt`）；② §11.1 的 `owned_pairing_peer` 行改为只描述真实列（不存绑定与角色），消除与 §3.5/实现相反的陈述。
28. **待决（不阻断本次提交）：撤销身份的「重新配对」语义**（review `WP6B-3`）：`specs/peer-identity-material` 的场景写「只有按协议完成重新配对才能恢复」，而 §11.2 第 2 条与本轮实现的守卫（`put_device`/`put_node`/`approve_*` 对 `revoked` 行一律 `Conflict(IdentityMismatch)`）使**同一 id** 无法经重新配对恢复；`SECURITY_DESIGN.md` §「重新配对为新设备/新身份」与 §11.3 的「撤销 tombstone 不因容量删除」支持「改身份」口径。两处文本方向相反，需在最终验收前二选一：① 与实现/SECURITY_DESIGN 对齐，把 spec 场景与 §11.2 措辞改成「只能以**新身份**重新配对」；② 改为允许同一 id 经重新配对恢复，则为两条 `approve_*` 增加 revocation-aware 分支与回归用例。本轮按保守口径（① 的方向）实现并记录，等主决策后再改文本。

### W0 执行期的偏差与决定（2026-09-23，本轮落地）

8. **WP4 的 2.14–2.18 在 W0 内一次闭合（执行波次调整）**：把三个版本常量推进到 2 必然让既有 `tests/migration.rs`（`empty`/`too-new` 夹具假定）与 `tests/commit.rs` 的 `user_version == 1` 变红，删列还会让 `session_store.rs` 的四处 `imported_import` 查询与 `tests/imported.rs` 的黄金列清单失效。因此 W0 实际执行了 tasks.md 的 **2.14 + 2.15 + 2.16 + 2.17 + 2.18** 与这些连带修改；`plan.md` 的 W1「WP4 剩余（2.15–2.18）」轨道因此为空，W1 只剩 WP6（2.19–2.22）与独立 review（3.2/3.4）。任务编号与要求不变。
9. **`imported_import` 的 v2 形状新增 `grants_json TEXT NOT NULL`**：依据 §11.1 目标表的「grant 集合」与 §11.6 的 `ImportWrite { record: ImportRecord, .. }`——Import 的 grants 必须落在管理行上，而 v1 没有这一列。迁移对旧行一律写 `'[]'`（无可信来源，不默认放权），该 Import 因此保持不可用。
10. **`RemoteDeliveryStore::drop_import` 去掉写 `removed_at` 的语句**：合同只把「连接级清空交付索引与命令引用」授予它（§5.2、§11.6 的「两处删除权威不得重叠」），移除标记属 `ImportRemoval` 写集；且本 crate 不读系统时间，旧实现把 `owner_node_id` 绑进 `removed_at` 是一个随重写消失的缺陷。查询改走 `imported_import_export`（一个 Import 可关联多个 Export）。
11. **§11 标题改名连带的引用同步**：`§11.` 由「（待实现）」改为「（形状已并入 §3/§5/§7）」，因此同步更新了 `README.md`、`docs/DEVELOPMENT_PLAN.md` 的锚点与文案，以及 `AGENTS.md` §1/§10 的措辞；`docs/CORE_PORTS_AND_STORAGE.md` 新增 §3.7（本地配置与凭据值对象）以承接从 §11.6 删除的 DTO 形状。
12. **夹具生成方式**：`fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 由 `crates/storage-sqlite/tests/migration.rs` 的 `#[ignore]` 生成器 `regenerate_v2_fixtures` 产出（`cargo test -p storage-sqlite --test migration -- --ignored regenerate_v2_fixtures`）；`from-v1.sqlite3` 以 `fixtures/storage/v1/empty.sqlite3` 为基底插入 v1 形状的行，并刻意留下 `owned_audit.audit_id = 1,2,5`（`sqlite_sequence.seq = 5`）以覆盖「序列领先于 `max(audit_id)`」。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2/WP3 | WP1 | `28f8cb9` + 工作区；`reports/wp1-core-tests.log` | 变更分支起点 | `cargo test -p core` 通过（75 用例） | WP1 值对象或端口形状变化 → WP2/WP3 复验 |
| WP4 | WP5（2.1 的 DDL 定稿） | W0 基线提交的 `migrate.rs` DDL 常量与 §7.3/§7.4 逐条一致（`reports/wp5-contract-drift-before-after.md`） | 变更分支起点 | `node scripts/check-contract-drift.mjs` 退出 0 且报 36 条语句 | §7 DDL 变化 → WP4/WP6 与夹具复验 |
| WP6 | WP2、WP4 | `013f2b9`；`ports.rs` 的写集签名 + `migrate.rs` 的 v2 常量与升级语句；`reports/wp4-migration-tests.log` | W0 基线提交 `013f2b9` | **1.4 包含关系核对（2026-09-23）**：① `storage-sqlite` 实际编译于 `HEAD = 013f2b93a77a31cec3e1a18f589c5e984f013128` 的 `core`/`migrate`；② `impl TrustStore/ExportStore/LocalConfigStore for SqliteStore` 的方法数（16 / 8 / 10）与漂移门禁断言的 §5 方法集合逐条对应（Rust 不允许缺方法，多出的都会编译失败）；③ `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION` 均为 `2`，管理 store 使用的表（`owned_device`/`owned_node`/`owned_peer_key`/`owned_pairing`/`owned_pairing_peer`/`owned_export`/`imported_import`/`imported_import_export`/`owned_agent_profile`/`owned_workspace`/`owned_provider_ref`）全部来自 v2 常量，列类型按 §11.7（`public_key` BLOB、`is_default` INTEGER）；④ 相关既有测试在 v2 夹具上复跑全绿（`migration` 6 / `imported` 9 / `enum_coverage` 2 / `retention` 8 / `commit` 15 / `permissions` 4 / `contract_v03` 5 / `compaction_recovery` 3 / `attachments` 5）。 | 端口签名或 DDL 变化 → WP6 复验 |
| WP5 | WP1 | core 新增 `p256`/`sha2`（直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`） | 变更分支起点 | `cargo tree -p core --edges normal` 与 allow-list 逐项相等（`reports/wp5-boundaries.log` 的 W0 复跑段） | 依赖版本变化 → allow-list 与 PV3 复验 |

## Runtime Resources

- 共享设施：Rust 构建目录 `target/`（唯一），W0 串行使用，无并发分片，故未设 `CARGO_TARGET_DIR`；W1 的三条并行轨道必须各自设置 `CARGO_TARGET_DIR`（见 `plan.md` 的多 Agent 分派规则）。
- SQLite：W0 的 storage 测试各自使用独立临时目录；夹具 `fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 已生成并只读使用（生成器为 `#[ignore]` 用例，不覆盖 `fixtures/storage/v1/`）。
- 无数据库服务、容器、端口、账号或外部网络资源。

## Review Findings

W0 期的独立 review（任务 3.2 / 3.4）**未执行**：当时没有可用的独立 reviewer 隔离上下文，按 apply instruction「无独立 reviewer 不得以自审替代」保持待办。WP6 起改用子 Agent（不继承实现对话的隔离上下文）执行 RV1。

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-BLOCKED（W0） | `28f8cb9` + 工作区 | 无（当时缺隔离上下文） | WP1–WP3 全部改动 | 阻断：交付前独立 review 未做 | 待独立 reviewer 完成后回填 | 无 |
| WP6-1 | `013f2b9` + 工作区 | `Wp6Review`（独立 reviewer 子 Agent，`agent://Wp6Review`） | `crates/storage-sqlite/src/admin/trust.rs:717` | 高：`settle_pairing` 缺 `pending_confirmation` 守卫——重复批准改写首次 `approved_at`；批准后拒绝撞 CHECK 且以未具名 `Backend` 失败（reviewer 已用只读 SQLite 复现） | **已修**：落定前正向状态守卫 + 两条 UPDATE 正向谓词 + `rows_affected` 失败关闭；附带修 `last_seen_at`/`last_connected_at` 的 `COALESCE`；新增用例 `a_pairing_can_be_settled_only_once`、`timestamps_are_advanced_never_erased` | `reports/wp6-admin-store-tests.log`（23 passed） |
| WP6-2 | 同上 | 同上 | `trust.rs:784` | **阻断：节点配对批准在存储层不可达**（`node.pair.confirm` 无法持久化信任；根因是写集不携带节点角色） | **已闭合（2026-09-23，A 路）**：角色由 `node.pair.begin --mode owner` 推导为 `NodeKind::Access`（§5.4 明确 `--mode access` 本机没有 confirm；§11.2 第 2 条已写明），`settle_pairing` 的节点分支写 `owned_node` + `owned_peer_key` + `approved` + `node.paired` 审计；未改 §5.3/`ports.rs`/DDL。用例 `node_pairing_approval_persists_access_trust` | `reports/rv1-wp6b.md`、`reports/wp6-admin-store-tests.log`（25 passed） |
| WP6-3 | 同上 | 同上 | `crates/storage-sqlite/tests/admin_store.rs:2083` | 中：落定回滚的故障注入落在写集的**第一个**写之前，无法区分「整事务回滚」与「尚未写入」 | **已修**：注入改为 `owned_device`（首个写）与 `owned_audit`（最后一个写）两处，并断言设备行、身份材料、配对状态（`approved_at` 为空）与审计四个面都回到调用前内容 | `reports/wp6-admin-store-tests.log` |
| WP6-4 | 同上 | 同上 | `trust.rs:529` | 中：`owned_pairing.host_binding` 写空串、claim/settle 无绑定可校验；该偏差只写在代码注释里，记录中查不到 | **已闭合（2026-09-23）**：登记时写真实绑定（设备 canonical origin、节点 endpoint），认领时要求逐字回显（不一致 → `Conflict(IdentityMismatch)`，零推进），`pairing_peer` 从配对行回填；模型与文档同步（Check Plan Changes 第 24/26 条）。用例 `claim_with_a_foreign_host_binding_is_rejected` | `reports/rv1-wp6b.md`、`reports/wp6-admin-store-tests.log`（25 passed） |

review 报告全文：`reports/rv1-wp6.md`（含逐条核对结论、规格场景覆盖表、自述缺口判断与复现方法）。

**闭合轮复验**（`Wp6ClosureReview`，`reports/rv1-wp6b.md`）：结论 `overall_correctness = correct`，WP6-1/2/3/4 四处**全部闭合**，无阻断项；角色推导（`Node ⇒ Access`）取得三条独立证据、未找到可达反例。其三条提示级项：

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| WP6B-1 | `1baea5b` + 工作区 | `Wp6ClosureReview` | `crates/core/src/model/identity.rs:476` | 提示：首版提交写过的库里，存量空 `host_binding` 行读取时落进通用 `InvalidRequest` 文案（分类误导排障） | **已修**：`pairing_from_row` 对空绑定返回具名 `Corrupt`；回归用例 `an_empty_stored_binding_is_reported_as_corrupt` | `reports/wp6-admin-store-tests.log`（26 passed） |
| WP6B-2 | 同上 | 同上 | `docs/CORE_PORTS_AND_STORAGE.md:1298` | 提示：§11.1 的 `owned_pairing_peer` 行宣称表内有「角色和非秘密 endpoint 引用」，与实际列相反 | **已修**：该行改为只列真实列，并写明绑定与角色都不在该表 | `docs/CORE_PORTS_AND_STORAGE.md`（本提交） |
| WP6B-3 | 同上 | 同上 | `crates/storage-sqlite/src/admin/trust.rs:962` | 提示（需合同侧二选一）：撤销身份的「重新配对」语义在 spec（可恢复）与 `SECURITY_DESIGN`/§11.3（改身份）之间方向相反；当前实现取保守口径 | **待决**：见 Check Plan Changes 第 28 条；在最终验收（`8.1`）前需主决策后改文本或用例 | 本文件 |

## Merge History

W0 在门禁全绿的状态上落了**基线提交**（分支 `feat/admin-state-persistence-v2`；父提交 `28f8cb9`），供 W1 的轨道作为固定基线。

W1·WP6 在独立 review（`reports/rv1-wp6.md`）的修复与全部本地门禁全绿的状态上落**第二个提交**（父提交 = W0 基线 `013f2b9`），内容为三个管理 store、`tests/admin_store.rs` 与记录回填。

WP6 闭合轮（WP6-2 节点批准 / WP6-4 本机绑定，用户批准 A 路）在复验（`reports/rv1-wp6b.md`，结论 `correct`）与全部门禁全绿的状态上落**第三个提交**（即本记录所在的提交；父提交 = 第二个提交 `1baea5b`），内容为 core 模型的 `host_binding`、storage 的绑定写/校验与节点批准、文档措辞与本次记录回填。**未合入 main**、未推送、未开 PR。

## Test Design and Authoring

`mode = not-applicable`，无 TP 分组。各工作包自带行为测试：

- `core`（WP1–WP3）：`crates/core/src/model/tests.rs`（`PeerPublicKey` 正负例、本地配置值对象不变量）与 `crates/core/src/use_cases.rs` 的 `#[cfg(test)] mod tests`（workspace 解析四类输入、未登记别名、缺失目录、`remove_import` 单写集、撤销设备审计随写集）。
- `storage-sqlite`（WP6）：`tests/admin_store.rs`（21 用例）逐条覆盖 `admin-state-persistence`/`peer-identity-material`/`local-agent-config`/`storage-schema-v2-migration` 的场景——并发认领只有一个成功、拒绝/过期不建信任、重启终结过期配对且不动已批准信任、双角色共享身份材料与角色指纹不一致被拒、按节点撤销覆盖两角色并在重启后生效、已撤销/换钥身份不可经普通写入复活、Import 归属冲突与写集分歧、连接级清空与完整移除都不删审计、默认 profile 唯一与切换、非法绑定拒写、空种子标记与重复打开不重导、Provider 引用版本递增、workspace 记录本机归属与 `created_at` 保留、库内无秘密材料、审计写失败与落定中途失败的整事务回滚（SQLite 触发器注入）、损坏库管理写路径全拒、超限拒绝新写入而不删信任。
- `storage-sqlite`（WP4）：`tests/migration.rs` 的 v2 夹具幂等、过新拒绝零写入、`v1_fixture_upgrades_to_v2_and_preserves_rows`（序号/origin cursor/`requestId`/`audit_id` 与序列保留、12-step 重建的 DDL 文本与新取值可用、Import 关联迁移与不补 grants）与 `second_open_of_an_upgraded_database_rewrites_nothing`；`tests/imported.rs` 的 6 表黄金列清单与无正文行为；`tests/enum_coverage.rs`、`tests/commit.rs`、`tests/retention.rs`、`tests/permissions.rs` 的既有判据在 v2 上复跑。

## Candidate E2E

无（`mode = not-applicable`）。

## Main E2E

- 项目开关：`npx --quiet --no-install openspec-agentic e2e --json` → `enabled: true`、`command: ""`、`maxAttempts: 3`。
- `plan.md` 的 mode：`not-applicable`，附用户降级批准原话（2026-09-23，本会话）。
- E2E 结论：**NOT_APPLICABLE**（未执行任何 E2E）。
- 替代检查：`7.1` 的替代验证（cargo 测试 + `npm run verify` + 漂移门禁 + `from-v1` 夹具重放）**尚未作为最终验证执行**——W0 已在基线上让漂移门禁与 `npm run verify` 的两半都可通过（`npm run check` 与 `cargo fmt/clippy/test` 全绿），`fixtures/storage/v2/from-v1.sqlite3` 也已生成并在 `tests/migration.rs` 中重放；按计划，替代验证仍须在 **W4 的最终主分支版本**上重跑并写入 `reports/alt-final-verification.md`。
- 另注：W0 的 `storage-sqlite` 夹具重放（v1 → v2 升级的行数/序号/`audit_id` 断言）已随 `cargo test -p storage-sqlite --all-features` 执行，证据见 `reports/wp4-migration-tests.log`。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| F1 / 2.15 | `cargo test --workspace`：`storage-sqlite --test enum_coverage` FAILED（审计 DDL 字面量 vs `AuditAction::ALL`，新增 5 个取值） | 主 Agent | 在 `migrate.rs` 的两条审计 CHECK 中补 5 个新取值，并为**既有库**补 12-step 表重建（`V2_UPGRADE_OWNED`/`V2_UPGRADE_IMPORTED`），重建后按旧 `sqlite_sequence` 回填序列 | 独立 review 未做（3.6 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `cargo test --locked -p storage-sqlite --all-features` → 11 个目标全 `ok`；`v1_fixture_upgrades_to_v2_and_preserves_rows` 断言升级库的 `owned_audit` DDL 含 `provider.configured`、`audit_id` 保留 `[1,2,5]`、序列仍为 5、新写入得 `audit_id = 6` | **已解决**（新建库与升级库两侧一致；证据 `reports/wp4-migration-tests.log`） |
| F2 / 2.1·2.2·2.14 | `node scripts/check-contract-drift.mjs`：§5.3 `TrustStore`/`ExportStore` 方法集、§7.3/§7.4 审计 CHECK 不一致 | 主 Agent | §5.3 逐字转录 `ports.rs` 的 §5.3 区域；§7.3 追加 9 张管理表 + 2 个索引、§7.4 改 `imported_import` 并追加 `imported_import_export`、两张审计表 CHECK 扩宽；§7.2/§3/§9/§11 与关联文档同步 | 独立 review 未做（3.8 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `node scripts/check-contract-drift.mjs` → 退出 0（`§7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …`）；`npm run check` 退出 0 | **已解决**（证据 `reports/wp5-contract-drift-before-after.md`、`reports/w0-npm-check.log`） |
| F3 / 2.1 附带 | `node scripts/check-doc-links.mjs`：`README.md`/`docs/DEVELOPMENT_PLAN.md` 的 `#11-管理状态持久化合同待实现` 锚点失效；`design.md` 里指向 `CORE_PORTS_AND_STORAGE.md` 的 §5.3/§3.5 被误归因到身份合同；本文件的 Required Follow-up 行里的 §12 被误归因到配置参考 | 主 Agent | 按合同 §11 新标题更新两处锚点与文案；把 design.md 的引用改成指名 `CORE_PORTS_AND_STORAGE.md`；重写本文件的 Required Follow-up 行使其不含歧义 §-引用 | 无（门禁自证） | 重跑 `node scripts/check-doc-links.mjs` → 退出 0；`npm run check` 退出 0 | **已解决**（证据 `reports/w0-npm-check.log`） |

## Final Assessment

```agentic-assessment
assessment_id: "admin-state-persistence-v2-w1-wp6-closure"
target_commit: "1baea5bd7d4dc824443b4e52e916d97f7bec3c44"
contract_digest: "sha256:1191945887f7001486e328786ad7b0395999317def7c1f0e51054c225951ec61"
result: BLOCKED
evidence:
  - path: reports/wp6-admin-store-tests.log
    sha256: "sha256:d5051e77ec177d46267dcdbaf678a3a48cfd03075dbe47ea9174920083ace2f9"
  - path: reports/rv1-wp6.md
    sha256: "sha256:678c200f65937121017f9f4de8a14adaebb1a50df22503e302fae3d5546e2c4a"
  - path: reports/rv1-wp6b.md
    sha256: "sha256:428f6b5a9151cf345c231571382d1833b57b6f75c249330897c2088747fb631f"
  - path: reports/wp1-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp3-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp4-migration-tests.log
    sha256: "sha256:ff8405540caefe49c44b5fc63a7d50beafa4c8d6a686af1074153b169b88d949"
  - path: reports/wp5-contract-drift-before-after.md
    sha256: "sha256:386a808109d07ff84894da9c366a631b9a38bc9d6a538c08ff3c12e9978b8d76"
  - path: reports/wp5-boundaries.log
    sha256: "sha256:71f677d60f7f23043139a2cb433b0b30e0670a37eef02e6b6968fb8a6f0a12b1"
  - path: reports/w0-verify-rust.log
    sha256: "sha256:2c51ea13ed78c41323425d62aa65c90df00a00df3153474e1f54f5a58c0bd402"
  - path: reports/w0-npm-check.log
    sha256: "sha256:90410d8c4e39b80e398c1581c598d9e71b9faeceb762eb621a509bce27c953ea"
```

`target_commit` 是闭合轮证据产生的**起点**（WP6 首版提交 `1baea5bb`）；本记录所在的提交即闭合轮交付提交，其父提交是它。`contract_digest` 与 W0 相同（本轮未改契约资产），由 `npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` 在回填后重跑得到（`result: PASS`）；任何契约内容再变化都会使该值失效，必须重跑。

- Assessment ID / Time: `admin-state-persistence-v2-w1-wp6`，2026-09-23（W1·WP6 收尾时）
- Target / Task: 见 `Target` 的「W1·WP6」与「W1·WP6 闭合轮」段；本轮完成 `1.4`、`2.19`–`2.22`、`3.9`、`3.10`（含闭合轮复验）；最终验收任务 `8.1` 未开始
- CLI State: `openspec status` = 5/5 artifacts complete；`npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` = PASS（`contractDigest` 见上方评估块）。CLI 状态不表示实现完成
- Audit / Evidence: W0 完成了 `storage-sqlite` 的 v2 DDL、两张审计表的 12-step 重建（含 `sqlite_sequence` 回填）、`imported_import` 拆分迁移与 v2 三件夹具（`cargo test -p storage-sqlite --all-features` 全绿），合同并入（`check:drift`/`check:boundaries`/`check:docs` 全绿）与合同/关联文档同步；`npm run check`、`cargo fmt`、`cargo clippy --workspace`、`cargo test --workspace` 全部退出 0。仍未做：WP6 的三个管理 store、独立 review（`3.2`/`3.4`/`3.6`/`3.8`/`3.10`）、候选与合入、替代验证与最终验收
- Result / Open Issues: **BLOCKED** —— 未完成任务：`3.2`/`3.4`/`3.6`/`3.8`（W0/WP4/WP5 的独立 review 仍缺隔离上下文）、`5.x`、`6.x`、`7.x`、`8.x`；WP6 的 review（`3.10`）已执行并留证（`reports/rv1-wp6.md`）
- 阻断项状态：WP6-1/WP6-2/WP6-3/WP6-4 均已闭合（闭合轮复验 `correct`）；唯一待决项是 **WP6B-3**（撤销身份的「重新配对」语义在 spec 与 `SECURITY_DESIGN`/§11.3 之间方向相反，见 Check Plan Changes 第 28 条）——它不阻断本提交，但需在 `8.1` 最终验收前由主决策后统一文本或用例
- Required Follow-up: ① 先决定 WP6-2/WP6-4 的合同侧修复（把节点角色并入配对落定写集或 `PairingRecord`、把本机绑定并入配对写集；或改述 §11.6/§5.4/§11.1/§7.3 的对应措辞），随后按合同变更流程更新 §5.3/§7.3 + `crates/core/src/ports.rs` + 写集与用例，并重跑 PV2/PV3/PV4 与 WP6 复验
- ② WP1–WP3 与 WP4/WP5 的独立 review（`3.2`/`3.4`/`3.6`/`3.8`：用不继承实现对话的子 Agent，方式同 `Wp6Review`）→ ③ W3（`5.x`/`6.x`）→ ④ W4（`7.x`/`8.x`）
