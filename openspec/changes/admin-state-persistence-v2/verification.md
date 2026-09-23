# admin-state-persistence-v2 — 执行记录

## Target

- 变更：`admin-state-persistence-v2`（schema `agentic`）
- 仓库：`D:\Project\acp-remote`
- 主分支引用：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（`git rev-parse`）
- 实现分支：`feat/admin-state-persistence-v2`，起点 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`（= `refs/heads/docs/pre-implementation-contracts` HEAD，main 的后代）
- **W0 基线提交**：本记录所在的那个提交（分支 `feat/admin-state-persistence-v2`；其父提交是 `28f8cb97c7297d83b9df7b15b16d0b059e2dec78`）。W0 的完成条件（合同漂移门禁绿 + 全部本地门禁绿）就在该状态上成立，W1 的三条轨道以此为固定基线。
- 本轮证据对应的版本：**W0 基线提交**（不再是未提交工作区）；该提交包含 core 的 WP1–WP3、`storage-sqlite` 的 v2 DDL/migration、合同并入与全部 W0 证据日志。
- **W1/W2 闭合轮交付提交**：`86ae8b4`（父提交 = 第四个提交 `5404610`）；四份独立 RV1 报告的固定被检视 revision 是 `5404610` + 工作区。
- **DU1 候选（固定）**：`62ef2649ae6d35e65930df505b2cf41858a19d26`（父提交链 `601c8ae → aed9fb5`），集成分支/主 worktree 按 `plan.md` 的 Merge Strategy 为 `feat/admin-state-persistence-v2`；**未合入 main**（缺授权）。
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
| PV1 / 关联文档同步（WP6 落地后，2.2 补充） | `5404610` + 工作区 | `docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/DEVELOPMENT_PLAN.md`、`README.md`、`docs/CONFIG_REFERENCE.md` 的「仍待实现/目标形状」陈述与实现对齐 | 主 Agent | `npm run check`；`cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked --workspace --all-features` | rustc 1.98.1（`rust-toolchain.toml`）、Node 24.19.0 | **PASS**：四条全部退出 0；`check:docs` 367 links / 1528 refs、`check:drift` 仍 36 条 DDL / 15 trait / 87 方法（合同块未改） | `reports/verify-doc-sync.log` |
| PV1+PV2+PV3+PV4 / W1–W2 review 闭合轮 | `5404610` + 工作区 | 四份 RV1 报告驱动的修复：core 落定审计动作按目标族、授权先于 workspace 解析、`p256` 最小 feature 与 allow-list、夹具序列与确定性、升级回滚用例、v2 枚举列覆盖；文档 §-引用与 2.2 关联同步 | 主 Agent | `cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked --workspace --all-features`；`node scripts/check-contract-drift.mjs`；`node scripts/check-crate-boundaries.mjs`；`npm run check` | rustc 1.98.1、Node 24.19.0 | **PASS**：全部退出 0；`core` 77 passed；`storage-sqlite` 各目标 ok（`migration` 7 passed / 1 ignored、`enum_coverage` 2、`admin_store` 28）；`check:drift` 仍 36 条 DDL / 15 trait / 87 方法（合同块未改） | `reports/verify-w1w2-closure.log` |
| PV1+PV3 / DU1 候选与修复轮 | `601c8ae`→`62ef264` + 工作区 | DU1 三份独立报告（集成/PV1/review）驱动的修复：干净检出的文档门禁、`put_export`/`add_import` 前置校验、夹具 `wss://` 端点与升级读回断言、证据标签更正 | 主 Agent | `npm run verify` 口径四条 + `check:contract-drift` + `check:boundaries` + `check:docs` + `npm run check`；另在只含被跟踪文件的检出上跑 `check:docs` | rustc 1.98.1、Node 24.19.0 | **PASS**：全部退出 0；`core` 78 passed；`storage-sqlite` 全目标 ok（`migration` 7/1 ignored、`enum_coverage` 2、`admin_store` 28）；干净检出（88 个 `.md`、无 `.omp/`）`check:docs` 退出 0 | `reports/verify-du1-fixes.log`、`reports/verify-du1-fixes-2.log`、`reports/clean-tree-check.log` |

| 6.6 / W3 合入 | `62ef264`→`86f282b`（`refs/heads/main`） | 以条件更新（fast-forward）合入 DU1 交付单元；`62ef264..86f282b` 对代码零差异 | 主 Agent（按用户本轮授权） | `git update-ref refs/heads/main 86f282bf… 37a398e9…`（旧值 CAS） | local | **PASS**：退出 0；`refs/heads/main` = `86f282bf545ea7839e961e09481d0784005ce420`；合并前后 `main` 是分支尖端 | `verification.md` 的 Merge History、`reports/du1-integration.md` |
| 6.7 [PV1] / main | `86f282b`（worktree `D:/Project/acp-remote-main`，`git status` 为空） | 在合入后的 `main` 固定版本上重跑 `npm run verify`，逐子项确认无失败/无零用例/无全跳过 | 主 Agent（独立检查执行者 `Du1Check` 的候选轮证据按同一提交复用） | `npm run verify`（`CARGO_TARGET_DIR` 复用主 worktree 的 `target/`） | rustc 1.98.1、Node 24.19.0、npm 12.0.2；不联网 | **PASS**：退出 0；`npm run check` 十道门禁 + `cargo fmt --check` + `clippy -D warnings` + `cargo test --workspace --all-features` 全部通过 | `reports/du1-main-verify.log` |
| 6.8 [RV1] / 合入差异 | `601c8ae..86f282b` | 合入新增差异（唯一带代码提交 `62ef264`：`add_import` 前置三分支与用例判别力） | 独立 reviewer 子 Agent `RvDu1Merge`（不继承实现对话） | 只读复核 + 实跑 `cargo test -p core --all-features export_and_import_preconditions_are_enforced` | rustc 1.98.1 | **correct**：无 P1/P2；`DU1-R1-F1`（干净检出 `check:docs`）未回归（`62ef264` 88 个 `.md` 与 `86f282b` 89 个 `.md` 均退出 0）；2 条 P3 记录债（合入后未回填 `target_commit`、Final Assessment 散文行与机器块矛盾）本轮闭合 | `reports/rv1-du1-merge.md` |
| 7.1/7.2 替代验证 | `86f282b`（同上 worktree） | not-applicable 的四项 `alternative_checks`：变更 crate 行为/保留性、项目级 `npm run verify`、契约漂移、依赖边界，外加 v1→v2 重放与资源清理 | 主 Agent | `cargo test --locked -p core -p storage-sqlite --all-features`；`npm run verify`；`node scripts/check-contract-drift.mjs`；`node scripts/check-crate-boundaries.mjs`；`cargo test -p storage-sqlite --test migration -- --nocapture`（v1→v2 重放） | rustc 1.98.1、Node 24.19.0 | **PASS**：全部退出 0；`core` 78、`migration` 7/1 ignored、`admin_store` 28、`enum_coverage` 2；三份夹具 SHA-256 前后不变、`target/alt-replay-*` 已清理 | `reports/alt-final-verification.md`、`reports/alt-7.1-run.log` |

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
16. **节点配对的批准路径失败关闭**：冻结的 `PairingSettlementWrite` 只携带 `pairing` + `settlement`，没有节点角色（`NodeKind`，由 `node.pair.begin` 的 `mode` 决定，`PairingRecord` 也不含它），因此 `settle_pairing` 对 `PairingTarget::Node` 的 `Approved` 返回 `InvalidRequest`（零写入）。**（本条已被第 25/30 条取代）**：A 路闭合后节点批准会写 `owned_node` + `owned_peer_key` + `node.paired`；此处保留为当时的决定记录。~~批准节点配对必须先经 `UseCases::put_node`~~ **（该从句已被第 25 条取代）**：A 路闭合后批准由 `settle_pairing` 的节点分支自己写 `owned_node`（`NodeKind::Access`）+ `owned_peer_key` + `node.paired`，`put_node` 不是前置；拒绝路径不受影响（它不创建信任）。
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
28. **撤销身份的「重新配对」语义（review `WP6B-3`，用户决定：保留合同文本、改实现）**：`specs/peer-identity-material` 与 §11.2 第 2 条的措辞是「已撤销身份不能经**普通 upsert** 自动激活，只能按协议重新配对」——即协议路径**可以**恢复同一身份。原实现让 `approve_device`/`approve_node` 也拒绝 `revoked` 行，比合同更严，本轮据用户决定把实现对齐到合同：①`put_device`/`put_node`（普通写入）继续拒绝 `revoked` 行；②两条 `approve_*`（`settle_pairing` 的批准路径，对端已出示配对 secret 的 HMAC/proof 且本机用户确认）允许复活——状态回到 `active`/`paired`、`revoked_at`/`revoke_reason` 清空，而撤销审计行保留（§11.3 的 tombstone 语义）；③指纹仍必须与既有身份材料一致（同一 id 不得换绑公钥，§11.6 第 1 条）。文档同步：§11.2 第 2 条点明唯一恢复入口、§11.6 第 1/4 条补例外与清空语义。回归用例：`only_a_fresh_pairing_can_lift_a_revocation`、`only_a_fresh_node_pairing_can_lift_a_node_revocation`（两者都先断言普通写入仍被拒）。

29. **（2026-09-23）WP6 落地后的关联文档同步（tasks.md 2.2 的补充）**：管理 store 落盘实现提交后，五份关联文档里「管理状态仍待实现 / 目标形状」的陈述与实现脱节，本轮统一改为同一口径——**core 端口 + SQLite 三个管理 store（含 v2 DDL 与写集原子性）已落地；仍未实现的是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`，在它们完成前不得声称配对、撤销或本地配置已端到端可用**。改动面：`docs/CORE_PORTS_AND_STORAGE.md`（文档头 0.8 版本行与修订行、§5.3「实现状态」段、§9 判据 14、§10 两条 `[已裁定]`、§11 开头段、§11.6 第 7 条）、`docs/MODULE_ARCHITECTURE.md` §4.1/§4.7、`docs/DEVELOPMENT_PLAN.md` §2/§4、`README.md`（core 依赖行、`storage-sqlite` 行、状态段）、`docs/CONFIG_REFERENCE.md`（标题去掉「，待实现」+ 0.7 修订记录）。同时修掉两处指向已迁走规则的 §-引用：`§11.8 第 7 条` → `§7.2` 的第 ② 步与 §7.3 的 `action` CHECK（§11.8 现在只保留设计理由与指针）。**§5.3 的 rust 代码块、§7 的 sql 代码块与 §9 判据正文一字未改**：`check:drift` 仍报 §7 36 条 DDL、§5 15 trait / 87 方法，`contractDigest` 不变。`docs/IDENTITY_AND_AUTH_CONTRACT.md` 的 `[待实现]` keystore 端口保持不动（`identity-keystore` 确实尚未实现）。

30. **（阻断项闭合）`node.paired` 此前没有写入方**：`UseCases::settle_pairing` 两族共用一个动作，存储层只落库写集携带的审计，而 `SECURITY_DESIGN.md` §14.2 的最小审计集合含 `node.paired`——审计因此永远无法回答「某节点是否经配对批准」。现在先经 `TrustStore::pairing` 读出目标族，再按族选动作（设备 `PairingApproved`、节点 `NodePaired`、拒绝两族 `PairingRejected`）；未知配对由 core 返回与存储层同形的 `NotFound(EntityRef::Pairing(id))`。回归用例 `pairing_settlement_carries_the_target_family_audit`（`crates/core/src/use_cases.rs`）。

31. **（授权顺序）授权先于 workspace 解析**：`UseCases::create_session` 原先先读 `LocalConfigStore` 并对结果做 `fs::metadata`/`canonicalize`、之后才授权，与 `broker.rs` 的「所有用例入口第一步先授权」约定相悖，且让未授权调用方能区分「别名已登记但目录缺失」（`Unavailable(IoError)`）与「授权拒绝」——本机登记状态与文件系统成了预言机。现在先 `broker.authorize(actor, "session.create", …)`（拒绝路径只写一条 `authorization.denied`；本地 actor 的内层授权恒成功，不重复写）。回归用例 `create_session_authorizes_before_resolving_the_workspace`。

32. **（依赖面）`core` 的 `p256` 收窄到 `arithmetic`**：原以 `p256.workspace = true` 继承 workspace 的 `ecdsa` feature，把 `ecdsa`/`rfc6979`/`hmac`/`signature`/`pkcs8`/`spki`/`pem-rfc7468`/`base64ct` 拖进 core 的冻结依赖闭包，与本次写入 `AGENTS.md` §12 与 `MODULE_ARCHITECTURE.md` §3.1 的「`hmac`/`base64` 不在 core」相反、也不符 tasks 2.3 的「按需最小 feature」。现在 workspace 的 `p256` 改为 `default-features = false, features = ["arithmetic"]`，`PeerPublicKey` 改用曲线级 `p256::elliptic_curve::PublicKey::<p256::NistP256>::from_sec1_bytes`（core 既不签名也不验签）；`CORE_ALLOWED_CLOSURE` 与 §9 判据 13/`AGENTS.md` §12/`MODULE_ARCHITECTURE.md` §3.1 的措辞同步，闭包 **38 → 29 项**。`identity-auth` 落地时用 `{ workspace = true, features = ["ecdsa"] }` 增量开启。

33. **（夹具与判据判别力）`from-v1` 夹具的审计序列原先并没有领先**：`owned_audit` 留 `{1,2,5}` 且 `sqlite_sequence = 5`（`max == seq`），于是 12-step 重建「按列拷贝全部行」本身就把序列置成 5、`restore_audit_sequences` 成为空操作，判据 28 的「序列不回退」失去判别力（真实场景：审计 365 天 TTL 清掉尾部行后 `seq > max`）。生成器现在写 `audit_id = 1..=7` 再删 `3/4/6/7`（`owned_audit`：`seq = 7`；`imported_audit` 同款：`1/2` 与 `seq = 3`），断言改为升级后 `seq = 7`、新插入得 `audit_id = 8`、`imported_audit` 行数 2。**同时**：夹具生成器把 `meta.server_epoch` 钉成字面量（`open` 默认写随机 UUID），三个夹具因此逐字节可复现（连续两次重生成 SHA-256 相同）。**DDL 条数修正**：交办文件写的「改前 25 条 / 改后 37 条」各多算一条，实际是 **36 条 = owned 29（18 表 + 11 索引）+ imported 7**（与漂移门禁输出一致）。

34. **（新增判据）升级路径与 v2 枚举列的判据补齐**：新增 `a_failed_upgrade_rolls_back_to_v1`（预建占位表 `imported_import_v2` 使第二段升级脚本失败 → `open` 返回错误且库仍是完整 v1：无 `owned_device`/`imported_import_export`、`owned_audit` DDL 无 `provider.configured`、`imported_import` 仍含 `export_id`、`user_version = 1`；去掉注入后重开升级成功）；升级用例补「读视图给出三条事件且 `event_payload` 可还原」（读视图持有读池连接，必须在 `close()` 前 `drop`）与「升级库的 6 张 `imported_*` 表列集合与新建库逐项相等」；升级库的两张审计表 DDL 断言覆盖 `AuditAction::ALL` 每个取值；`enum_coverage.rs` 的 `cases` 由 16 扩到 **26**（v2 管理表 8 个枚举列 + 两张审计表 `actor_kind`）。**残留（不在本变更内）**：`revoke_reason`（`owned_device`/`owned_node`）与 `cache_policy`（`owned_export`/`imported_import`）的 CHECK 不是 `IN (...)` 形状，core 侧也没有对应 `ALL`/`as_str`（`RevokeReason` 是 `core::ports` 的朴素枚举、文本映射在 `admin/trust.rs`），因此这两列仍只由行为用例覆盖。

35. **（DU1-R1 阻断项）干净检出下 `check:docs` 会红**：`AGENTS.md` §8 用相对链接指向 `.omp/commands/commit.md`，而 `.omp/` 在仓库里**没有任何被跟踪文件**（宿主安装目录），本机因为文件存在才一直显绿；`git archive`/CI 上 `check:docs` 退出 1，`checks` 这个必需检查会红。修法：只链接随版本控制的 `.pi/prompts/commit.md`，宿主路径写成纯文本并注明不进版本库（`AGENTS.md`）。证据：`reports/clean-tree-check.log`（只含被跟踪文件的 88 个 `.md` 树上 `check-doc-links.mjs` 退出 0）。

36. **（DU1-R1 F2/F3）`export.create` 与 `import.add` 的引用前置校验**：`UseCases::put_export` 现在校验每个 `workspace_aliases[].alias` 已在本机 `owned_workspace` 登记、每个 `agent_ids[]` 在本机目录里可用；`UseCases::add_import` 校验 `owner_node_id` 是**已配对**且 `kind = owner` 的节点（§5.1/§11.2 第 4/5 条、§11.7、`LOCAL_ADMIN_PROTOCOL.md` §5.5）。错误分层：**缺席**报 `NotFound(EntityRef::Node(..))`（适配器映射 `local.not_found`），**存在但角色或状态不对**报参数类错误——alias 一侧 core 没有对应 `EntityRef` 变体，适配器需按 §5.5 映射为 `local.not_found`/`local.invalid_params`（记录在案的适配器义务）。回归用例 `export_and_import_preconditions_are_enforced`（先把 Agent 放进目录，使「别名未登记」只能被别名前置拦下；含 access 角色与 `Owner + pending` 两个反例）。

37. **（DU1-R1 F4）`from-v1` 夹具端点与升级读回**：`imported_import.endpoint_ref` 由 `https://owner.invalid` 改为 `wss://owner.invalid`（`ImportRecord` 只接受 `wss://`；原值不可能来自真实 v1 库，且会让升级后的读路径直接报错），重生成夹具；升级用例补经 `ExportStore::imports()` 的读回断言（1 行、端点正确、`export_ids` 与 `imported_import_export` 一致），覆盖「Import 管理行 + 关联行在升级后经端口可用」。

38. **（DU1-R2 F4）证据标签更正**：早前版本的 `reports/clean-tree-check.log` 声称来自干净 worktree，实际在**仓库工作目录**执行（报告 104 个 `.md`，而只含被跟踪文件的树是 88 个）。已重做：`git archive 62ef264 → tar -xf` 解出的树内运行该树自己的 `scripts/check-doc-links.mjs`，日志写明 revision、取树命令、`.omp/` 缺失与 `.md` 计数。

39. **（W3 合入与验收机制，2026-09-23）**：用户在本轮给予**合并授权**，因此 W3 按以下方式落地——① 合入用**条件更新**而非 PR 流程：`git update-ref refs/heads/main 86f282bf… 37a398e9…`（旧值 CAS，快进；`AGENTS.md` §8 的 PR + 必需检查路径是常规入口，本轮由用户显式授权的合并是 `plan.md` 的「按当前授权」分支）；未推送：本轮授权范围是**本地** `refs/heads/main`，`origin` 未刷新、未 push。② 主分支复验与最终验收在**只含被跟踪文件、且不含宿主安装目录的干净 worktree**（`D:/Project/acp-remote-main`，`git worktree add … main`）内执行，并显式传 `--planning-root D:/Project/acp-remote`——这同时满足 `workflow-check.md` 的「路径相对 changeDir」与 `acceptance.md` 的 worktree 要求，避免把宿主安装的 `.omp/`、`.pi/settings.json` 等未跟踪文件误判成「变更目录之外有未提交改动」。③ 验收期间记录（`verification.md`/`tasks.md`）保持在变更目录内**未提交**：检查器的 `evaluateRecordFreshness` 只统计变更目录**之外**的未提交改动，因此「记录先于最后一次提交」是设计允许的状态；记录在本轮末尾以一次仅供文档的提交落库。

### W0 执行期的偏差与决定（2026-09-23，本轮落地）

8. **WP4 的 2.14–2.18 在 W0 内一次闭合（执行波次调整）**：把三个版本常量推进到 2 必然让既有 `tests/migration.rs`（`empty`/`too-new` 夹具假定）与 `tests/commit.rs` 的 `user_version == 1` 变红，删列还会让 `session_store.rs` 的四处 `imported_import` 查询与 `tests/imported.rs` 的黄金列清单失效。因此 W0 实际执行了 tasks.md 的 **2.14 + 2.15 + 2.16 + 2.17 + 2.18** 与这些连带修改；`plan.md` 的 W1「WP4 剩余（2.15–2.18）」轨道因此为空，W1 只剩 WP6（2.19–2.22）与独立 review（3.2/3.4）。任务编号与要求不变。
9. **`imported_import` 的 v2 形状新增 `grants_json TEXT NOT NULL`**：依据 §11.1 目标表的「grant 集合」与 §11.6 的 `ImportWrite { record: ImportRecord, .. }`——Import 的 grants 必须落在管理行上，而 v1 没有这一列。迁移对旧行一律写 `'[]'`（无可信来源，不默认放权），该 Import 因此保持不可用。
10. **`RemoteDeliveryStore::drop_import` 去掉写 `removed_at` 的语句**：合同只把「连接级清空交付索引与命令引用」授予它（§5.2、§11.6 的「两处删除权威不得重叠」），移除标记属 `ImportRemoval` 写集；且本 crate 不读系统时间，旧实现把 `owner_node_id` 绑进 `removed_at` 是一个随重写消失的缺陷。查询改走 `imported_import_export`（一个 Import 可关联多个 Export）。
11. **§11 标题改名连带的引用同步**：`§11.` 由「（待实现）」改为「（形状已并入 §3/§5/§7）」，因此同步更新了 `README.md`、`docs/DEVELOPMENT_PLAN.md` 的锚点与文案，以及 `AGENTS.md` §1/§10 的措辞；`docs/CORE_PORTS_AND_STORAGE.md` 新增 §3.7（本地配置与凭据值对象）以承接从 §11.6 删除的 DTO 形状。
12. **夹具生成方式**：`fixtures/storage/v2/{empty,from-v1,too-new}.sqlite3` 由 `crates/storage-sqlite/tests/migration.rs` 的 `#[ignore]` 生成器 `regenerate_v2_fixtures` 产出（`cargo test -p storage-sqlite --test migration -- --ignored regenerate_v2_fixtures`）；`from-v1.sqlite3` 以 `fixtures/storage/v1/empty.sqlite3` 为基底插入 v1 形状的行，并刻意留下 `owned_audit.audit_id = 1,2,5`（`sqlite_sequence.seq = 5`）以覆盖「序列领先于 `max(audit_id)`」。**（序列值已被第 33 条修正为 `owned_audit` `seq = 7`、`imported_audit` `seq = 3`——原先 `seq == max` 时判据没有判别力。）**

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
| RV1-BLOCKED（W0） | `5404610` + 工作区 | `RvWp1`/`RvWp23`/`RvWp4`/`RvWp5`（四个不继承实现对话的 reviewer 子 Agent，`agent://RvWp1` 等） | WP1–WP3、WP4、WP5 全部改动 | **已闭合**：四份报告落盘、发现全部处理（见下方 W1/W2 闭合轮表） | — | `reports/rv1-wp{1,23,4,5}.md` |
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
| WP6B-3 | 同上 | 同上 | `crates/storage-sqlite/src/admin/trust.rs:962` | 提示：撤销身份的「重新配对」语义在 spec（可恢复）与 `SECURITY_DESIGN`/§11.3（改身份）之间方向相反；当时实现取保守口径（比合同更严） | **已闭合（2026-09-23，用户决定保留「按协议重新配对才能恢复」）**：`put_device`/`put_node` 仍拒绝 `revoked` 行，两条 `approve_*` 允许复活并清空撤销时间/原因、保留撤销审计；文档与用例同步（Check Plan Changes 第 28 条） | `reports/wp6-admin-store-tests.log`（28 passed） |

**W1/W2 闭合轮（2026-09-23）**：四个独立 reviewer 子 Agent 各检视一个 WP 切片，报告见 `reports/rv1-wp{1,23,4,5}.md`。结论：`rv1-wp1`/`rv1-wp4`/`rv1-wp5` = `correct`，`rv1-wp23` = `incorrect`（1 条阻断）。全部发现及处理：

| ID | Reviewer | 位置 | 严重度 / 影响 | 处理 | 证据 |
| --- | --- | --- | --- | --- | --- |
| `WP23-2` | `RvWp23` | `crates/core/src/use_cases.rs`（`settle_pairing`） | **阻断**：`node.paired` 无任何写入方，而 `SECURITY_DESIGN.md` §14.2 的最小集合含它；现存用例自造写集，门禁无法发现 | **已修**（Check Plan Changes 第 30 条）：按目标族选动作 + 回归用例 | `reports/rv1-wp23.md`；`reports/verify-w1w2-closure.log` |
| `WP23-1` | `RvWp23` | `use_cases.rs`（`create_session`） | 高：授权在学习别名解析之后，未授权方可探测本机登记状态/文件系统 | **已修**（第 31 条）：先授权 + 回归用例 | 同上 |
| `WP23-3` | `RvWp23` | `use_cases.rs:755/:820`、合同 §3.1 | 信息：两处 §-引用指向已迁走的规则 | **已修**：`§11.7` → `§7.3`；`（§3.1/§11.6）` → `（§3.1）`；§3.1 的 `Provider` 指针改指 §3.7 | `reports/verify-w1w2-closure.log` |
| `WP4-1` | `RvWp4` | `tests/migration.rs`、`fixtures/storage/v2/from-v1.sqlite3` | 高：夹具审计序列未真正领先 → 判据 28 无判别力 | **已修**（第 33 条）：`seq = 7` / `imported_audit seq = 3`，断言与新插入续号同步 | 同上 |
| `WP4-2` | `RvWp4` | `tests/migration.rs` | 高：spec 的「升级中途失败整体回滚」无用例 | **已修**（第 34 条）：`a_failed_upgrade_rolls_back_to_v1` | 同上 |
| `WP4-3` | `RvWp4` | `tests/enum_coverage.rs`、`tests/migration.rs` | 中：v2 管理表枚举列与升级库 DDL 取值集无逐值判据 | **已修**（第 34 条）：`cases` 16 → 26 + 升级库 DDL 覆盖 `AuditAction::ALL`；`revoke_reason`/`cache_policy` 列为登记残留 | 同上 |
| `WP4-4` | `RvWp4` | `tests/migration.rs` | 中：升级库无重放与黄金列断言 | **已修**（第 34 条）：读视图三条事件 + 6 张 `imported_*` 表列等价 | 同上 |
| `WP4-5` | `RvWp4` | `verification.md` | 信息：交办文件的 DDL 条数硬判据多算一条 | **已登记**（第 33 条末段）：实际 36 条 = owned 29 + imported 7 | — |
| `WP1-1` | `RvWp1` | `crates/core/Cargo.toml`、`Cargo.toml` | 中：`p256` 继承 `ecdsa` 把签名/编码栈拖进 core 闭包，与 `AGENTS.md` §12/`MODULE_ARCHITECTURE.md` §3.1 措辞及 tasks 2.3 冲突 | **已修**（第 32 条）：`default-features = false, features = ["arithmetic"]` + 曲线级校验 + allow-list/文档同步（闭包 38 → 29） | `reports/rv1-wp1.md`；`reports/verify-w1w2-closure.log` |
| `WP1-2` / `WP5-F4` | `RvWp1` / `RvWp5` | 合同 §3.5、`IDENTITY_AND_AUTH_CONTRACT.md` §3、`design.md` §D3 | 中：合同声称的 `PeerPublicKey::FromStr` 在代码里不存在（core 也不允许引入 base64/hex 解析） | **已修**：删掉 `/FromStr`，入口统一为 `try_from_bytes`（`TryFrom<&[u8]>` 委托它）；`design.md` 同步为曲线级校验 | 同上 |
| `WP1-3` | `RvWp1` | `crates/core/src/model/config.rs:102/:288` | 信息：两处注释指向已迁走的 §11.7 列清单 | **已修**：改为 §7.3 | 同上 |
| `WP1-4` | `RvWp1` | `crates/core/src/model/tests.rs`、`tests/admin_store.rs` | 信息：`EntityRef::Provider` 的 kind/target_id 落库映射无用例 | **已修**：core 断言 `kind/target_id/Display`；存储侧断言审计行 `target_kind = 'provider'`、`target_id` = 引用 id | 同上 |
| `WP1-5` | `RvWp1` | `crates/core/src/model/config.rs`、`tests.rs` | 信息：若干构造校验分支无用例（重复绑定、command/arg 的 NUL 与超长、`configured_fields` 重复/非法、Provider id 超长、env 名超长） | **已修**：`local_config_values_enforce_their_invariants` 逐条补齐 | 同上 |
| `WP5-F1` | `RvWp5` | 合同 §2 | 中：§2 的错误枚举与 `error.rs` 不一致，而两处断言声称已同步 | **已修**：§2 补 `AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership`/`KeystoreUnavailable` 与 `local.conflict`/`local.unavailable` 的映射义务 | `reports/rv1-wp5.md` |
| `WP5-F2` | `RvWp5` | 合同 §10 | 中：写集映射义务与 `port_error_public` 误指 §5.1 | **已修**：改指 §5.3 | 同上 |
| `WP5-F3` | `RvWp5` | 合同 §9 判据 14、§10、§11.6 第 7 条 | 中：审计 `action` CHECK 只引 §7.3，漏掉 imported 侧的 §7.4 | **已修**：三处统一为「§7.3 与 §7.4」 | 同上 |
| `WP5-F5` | `RvWp5` | `scripts/check-crate-boundaries.mjs` | 信息：注释写「§2：五个」而实际四个、且权威是 §9 判据 13 | **已修**：改为「§9 判据 13：四个」 | 同上 |

四份报告结论均为「无未解决阻断项」（`rv1-wp23` 的唯一阻断项已在同一轮闭合）。WP6 的两份报告（`rv1-wp6.md`/`rv1-wp6b.md`）及其 WP6-1..4、WP6B-1..3 的闭合见上表。

**DU1（集成与候选轮，2026-09-23）**：`5.1`/`5.2`/`6.1`–`6.5` 完成——独立集成 Agent `Du1Integrator`（不继承实现对话）核对包含关系并构建候选（`reports/du1-integrator.md`）、独立检查执行者 `Du1Check` 在固定候选上跑 PV1 全绿（`reports/du1-pv1.log`）、独立 reviewer `Du1Review` 检视候选（`reports/rv1-du1.md`，1 条阻断 + 5 条非阻断），修复落 `601c8ae`；复核 `Du1Recheck` 结论 `correct` 并给出 5 条非阻断（`reports/rv1-du1-r2.md`），再修复落 `62ef264`。`refs/heads/main` 当时为 `37a398e`；**合入 `6.6` 与 `6.7`/`6.8`/`7.x`/`8.1` 因缺合并授权未执行**（候选与证据保留）。**（该状态已被下方 W3 段取代。）**

**DU1 合入与最终轮（W3，2026-09-23，用户授权合并）**：`RvDu1Merge`（独立 reviewer 子 Agent，不继承实现对话）复核 `601c8ae..86f282b`（唯一带代码提交 `62ef264`）——结论 `correct`，无 P1/P2，`DU1-R1` 的阻断项 `F1` 未回归（干净树上 `62ef264` 的 88 个 `.md` 与合入后 `86f282b` 的 89 个 `.md` 均使 `check:docs` 退出 0，与 `aed9fb5` 的 1 problem/EXIT=1 形成正反对照）；给出 2 条 P3 记录债（① 合入后未回填实际合入提交与 `target_commit`；② `Final Assessment` 的散文行与机器块互相矛盾），两条均在本轮闭合（本条与 `Merge History`、`agentic-assessment` 的更新）。报告：`reports/rv1-du1-merge.md`。

## Merge History

W0 在门禁全绿的状态上落了**基线提交**（分支 `feat/admin-state-persistence-v2`；父提交 `28f8cb9`），供 W1 的轨道作为固定基线。

**W3 合入（2026-09-23，用户授权）**：DU1 交付单元以条件更新（fast-forward）合入本地 `refs/heads/main`——`37a398e9dbafa368bdb15853e1c1d9b40f19b28d` → `86f282bf545ea7839e961e09481d0784005ce420`（命令 `git update-ref refs/heads/main 86f282bf… 37a398e9…`，旧值不符即失败）。合入的提交链：`86ae8b4`（W1/W2 review 阻断项闭合）→ `aed9fb5`（W1/W2 记录）→ `601c8ae`（DU1-R1 修复）→ `62ef264`（DU1-R2 修复；**唯一带代码的合入差异**）→ `0b5fa80`、`86f282b`（DU1 记录）。`62ef264..86f282b` 对代码零差异，因此候选轮证据按同一代码内容复用，`6.7` 在 `main` 上重跑了完整 `npm run verify`，`6.8` 对 `601c8ae..86f282b` 做了独立复核（结论 `correct`）。**未推送**：本轮授权针对**本地** `refs/heads/main`；`origin/main` 未刷新，推送与 PR 不在授权范围内。

W1·WP6 在独立 review（`reports/rv1-wp6.md`）的修复与全部本地门禁全绿的状态上落**第二个提交**（父提交 = W0 基线 `013f2b9`），内容为三个管理 store、`tests/admin_store.rs` 与记录回填。

WP6 闭合轮（WP6-2 节点批准 / WP6-4 本机绑定，用户批准 A 路）在复验（`reports/rv1-wp6b.md`，结论 `correct`）与全部门禁全绿的状态上落**第三个提交**（父提交 = 第二个提交 `1baea5b`），内容为 core 模型的 `host_binding`、storage 的绑定写/校验与节点批准、文档措辞与记录回填。

复验提出的 `WP6B-3`（撤销身份的重新配对语义）按用户决定「按协议重新配对才能恢复」落**第四个提交**（即本记录所在的提交；父提交 = 第三个提交 `f43f7a7`）：`approve_*` 允许经配对批准复活已撤销身份、普通写入仍拒绝，文档与两条回归用例同步。W1/W2 闭合轮落**第五个提交** `86ae8b4`（`fix(core): 闭合 W1/W2 独立 review 的阻断项、依赖面与判据缺口`；父提交 = 第四个提交 `5404610`）：四份独立 review 的阻断项与全部 P2/P3 发现闭合（Check Plan Changes 30–34）、`core` 的 `p256` 收窄到 `arithmetic` 与 allow-list/合同措辞同步、`from-v1` 夹具的序列与确定性修正、升级中途失败回滚与 v2 枚举列判据、以及 WP6 落地后的关联文档同步（tasks.md 2.2 的补充）。**未合入 main**、未推送、未开 PR。

## Test Design and Authoring

`mode = not-applicable`，无 TP 分组。各工作包自带行为测试：

- `core`（WP1–WP3）：`crates/core/src/model/tests.rs`（`PeerPublicKey` 正负例、本地配置值对象不变量）与 `crates/core/src/use_cases.rs` 的 `#[cfg(test)] mod tests`（workspace 解析四类输入、未登记别名、缺失目录、`remove_import` 单写集、撤销设备审计随写集）。
- `storage-sqlite`（WP6）：`tests/admin_store.rs`（28 用例）逐条覆盖 `admin-state-persistence`/`peer-identity-material`/`local-agent-config`/`storage-schema-v2-migration` 的场景——并发认领只有一个成功、拒绝/过期不建信任、重启终结过期配对且不动已批准信任、双角色共享身份材料与角色指纹不一致被拒、按节点撤销覆盖两角色并在重启后生效、已撤销/换钥身份不可经普通写入复活、Import 归属冲突与写集分歧、连接级清空与完整移除都不删审计、默认 profile 唯一与切换、非法绑定拒写、空种子标记与重复打开不重导、Provider 引用版本递增、workspace 记录本机归属与 `created_at` 保留、库内无秘密材料、审计写失败与落定中途失败的整事务回滚（SQLite 触发器注入）、损坏库管理写路径全拒、超限拒绝新写入而不删信任。
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
| F1 / 2.15 | `cargo test --workspace`：`storage-sqlite --test enum_coverage` FAILED（审计 DDL 字面量 vs `AuditAction::ALL`，新增 5 个取值） | 主 Agent | 在 `migrate.rs` 的两条审计 CHECK 中补 5 个新取值，并为**既有库**补 12-step 表重建（`V2_UPGRADE_OWNED`/`V2_UPGRADE_IMPORTED`），重建后按旧 `sqlite_sequence` 回填序列 | 独立 review 未做（3.6 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `cargo test --locked -p storage-sqlite --all-features` → 11 个目标全 `ok`；`v1_fixture_upgrades_to_v2_and_preserves_rows` 断言升级库的 `owned_audit` DDL 含 `provider.configured`、`audit_id` 保留 `[1,2,5]`、序列仍为 5、新写入得 `audit_id = 6`（**第 33 条修正后为：序列 7、新写入得 `audit_id = 8`**） | **已解决**（新建库与升级库两侧一致；证据 `reports/wp4-migration-tests.log`） |
| F2 / 2.1·2.2·2.14 | `node scripts/check-contract-drift.mjs`：§5.3 `TrustStore`/`ExportStore` 方法集、§7.3/§7.4 审计 CHECK 不一致 | 主 Agent | §5.3 逐字转录 `ports.rs` 的 §5.3 区域；§7.3 追加 9 张管理表 + 2 个索引、§7.4 改 `imported_import` 并追加 `imported_import_export`、两张审计表 CHECK 扩宽；§7.2/§3/§9/§11 与关联文档同步 | 独立 review 未做（3.8 属 W2，缺隔离上下文时记 BLOCKED） | 重跑 `node scripts/check-contract-drift.mjs` → 退出 0（`§7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …`）；`npm run check` 退出 0 | **已解决**（证据 `reports/wp5-contract-drift-before-after.md`、`reports/w0-npm-check.log`） |
| F3 / 2.1 附带 | `node scripts/check-doc-links.mjs`：`README.md`/`docs/DEVELOPMENT_PLAN.md` 的 `#11-管理状态持久化合同待实现` 锚点失效；`design.md` 里指向 `CORE_PORTS_AND_STORAGE.md` 的 §5.3/§3.5 被误归因到身份合同；本文件的 Required Follow-up 行里的 §12 被误归因到配置参考 | 主 Agent | 按合同 §11 新标题更新两处锚点与文案；把 design.md 的引用改成指名 `CORE_PORTS_AND_STORAGE.md`；重写本文件的 Required Follow-up 行使其不含歧义 §-引用 | 无（门禁自证） | 重跑 `node scripts/check-doc-links.mjs` → 退出 0；`npm run check` 退出 0 | **已解决**（证据 `reports/w0-npm-check.log`） |

| F4 / 自审（无任务号，会话内自审发现） | 未提交工作区出现**未登记的合同扩张**：`IDLE_IDENTITY_RETENTION_DAYS` 常量 + `TrustStore::cleanup_idle_identities` 端口方法 + `IdleCleanupWrite`/`IdleCleanupReport` + `IdentityPruned`（`identity.pruned`）审计动作 + 两张审计表 CHECK 新取值 + §5.3/§7 的对应正文（漂移门禁当时仍判定绿，即两侧「一致地」越出了合同）。它不在 `specs/**`、`design.md`、`plan.md`、`tasks.md` 的任何要求内；`docs/**` 除该切片自带的 §5.3 文本外查不到规则，且与 §11.3「活动信任、授权与配置不按聊天 TTL 清理；撤销 tombstone 不因容量压力被删除，防止旧身份恢复」方向相反（按 90 天闲置删除信任行与身份材料、已撤销行也一并删除） | 主 Agent | 整体回退该切片（不属于本变更，且会改写 W0 冻结的 §5.3/§7 与已登记 `contractDigest`）：`git restore -- crates/core crates/storage-sqlite docs/CORE_PORTS_AND_STORAGE.md fixtures/storage/v2`；完整 diff（含二进制夹具）另存为 `reports/parked-idle-identity-retention.patch`（37.6 KB，未跟踪）以便按正规流程重启 | 该切片无 spec/plan 依据，不进入本变更；如确需闲置身份保留，须先补规格要求与 §11.3/§11.6 的权威规则并经用户确认后才能改合同 | 回退后复跑：`node scripts/check-contract-drift.mjs` 退出 0（`§7 的 36 条 DDL 与 migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 ports.rs 一致`）、`node scripts/check-crate-boundaries.mjs` 退出 0、`cargo test --locked -p storage-sqlite --all-features` 全绿、`npm run check` / `cargo fmt --all -- --check` / `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` / `cargo test --locked --workspace --all-features` 全部退出 0（`reports/verify-5404610.log`）；`workflow check --stage plan --json` 的 `contractDigest` 仍为 `sha256:1191945887f7001486e328786ad7b0395999317def7c1f0e51054c225951ec61`（与回退前记录一致） | **已回退**：工作区内容等于 `5404610`，`git status --porcelain` 只剩未跟踪的 `reports/parked-idle-identity-retention.patch` 与宿主安装项 |

## Final Assessment

```agentic-assessment
target_commit: "afedea3a58b31df01087d28b1f28802ac8f96739"
assessment_id: "admin-state-persistence-v2-w3-merge-final"
contract_digest: "sha256:92c48c1af83d60043705d7a14837dd00aa350c41f6d8184cc5886e40cb7d6cc9"
result: PASS
evidence:
  - path: reports/wp6-admin-store-tests.log
    sha256: "sha256:87b993be501a576a5d45a80dc5e9372fb3dd6b35123a06a682f0bcc895d9997f"
  - path: reports/wp5-contract-drift-before-after.md
    sha256: "sha256:386a808109d07ff84894da9c366a631b9a38bc9d6a538c08ff3c12e9978b8d76"
  - path: reports/wp1-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp3-core-tests.log
    sha256: "sha256:f294a5a7aa12becdd4a1ff7c32f47a8bd03cf0c4b7b5609989f515ef0438e7b9"
  - path: reports/wp4-migration-tests.log
    sha256: "sha256:ff8405540caefe49c44b5fc63a7d50beafa4c8d6a686af1074153b169b88d949"
  - path: ../../../reports/du1-integrator.md
    sha256: "sha256:2d77ab4e6b1b2578cb828e84c33626fd2673cc9b3992f5c2b809fc116eb325b1"
  - path: ../../../reports/du1-pv1.log
    sha256: "sha256:65895455952f9c5abbe69b2745f59ae3312c6a791639b93d5ac958889ea0c490"
  - path: ../../../reports/du1-checker.md
    sha256: "sha256:29032902bb0e75d2a0f9e159987af1bad895afa0e20d357fbdd8c9bacc0af9b2"
  - path: ../../../reports/rv1-du1.md
    sha256: "sha256:5327b5f1795d10d9672f244aa37640605df40a2346b7fae4d5d1a4060ac67e84"
  - path: ../../../reports/rv1-du1-r2.md
    sha256: "sha256:56b0f2de8887c6cb06848604de65e25601f6fd43e47788caa4db8daf5187c5ed"
  - path: ../../../reports/rv1-du1-merge.md
    sha256: "sha256:b88a6c9ee334b109d454183466f11a3a77ab454bca4b9554131dc7fa7cd83c99"
  - path: ../../../reports/du1-integration.md
    sha256: "sha256:96a6aefebf773ea9a6adbc5949daf88d7182899c151d119abe3f1584e91899e5"
  - path: ../../../reports/clean-tree-check.log
    sha256: "sha256:4fc7c99868a4dfb69a0bc125ab1d3bcb99f6ef88df3f15546ea5c8ffecca68ea"
  - path: ../../../reports/verify-du1-fixes.log
    sha256: "sha256:847eb9ccb867ad217f8c700b7c0493219a3ddf274dbc1a16164964860351899e"
  - path: ../../../reports/verify-du1-fixes-2.log
    sha256: "sha256:e48e9f824da99f3e28c02a142760ab46d5ecce011ebd5b00c90cfc819fcd74ca"
  - path: ../../../reports/verify-w1w2-closure.log
    sha256: "sha256:7328b2fa38923eaa3fd8c949b3e6a8fe0fe4d73cc7d19ec5fe786d8f9d667798"
  - path: ../../../reports/verify-5404610.log
    sha256: "sha256:71553bbeb1bed817891833e662306fdf321f8342c68a5f2693b4a133d750ae85"
  - path: ../../../reports/verify-doc-sync.log
    sha256: "sha256:f283018cb4503cda3ee08890fcf0bb9f32b48474c1c0bb7a182db0e3aa7ab556"
  - path: reports/verify-final-round1.log
    sha256: "sha256:eec21eaf1fc5fdd41a4a3c247f87989bbf78f12e1fc6ab6fca432160455c28cf"
  - path: ../../../reports/alt-7.1-run.log
    sha256: "sha256:97e7b6716487b0a2f74b1fa65317490915bbb6078e2ce30b1488ed9f95b7d2cd"
  - path: ../../../reports/du1-main-verify.log
    sha256: "sha256:007b8812ed318f570a2c0ebf8ac691cf8d415b42a94e8930822c409377083909"
  - path: reports/alt-final-verification.md
    sha256: "sha256:cb4e598d474899fb3a2d5167b5ead92ba193a8b7fd1c6701ab5a550156f8d810"
  - path: ../../../reports/rv1-wp1.md
    sha256: "sha256:b2f42a93b47e467f3d09cf8954f8e3d24e0fcb5e2a286bb6790d5d1efa95a39b"
  - path: ../../../reports/rv1-wp23.md
    sha256: "sha256:4a01b4f54bb1bf0a8437f6624557f9ba43fcfc2395f3fbf50283db892ca79e98"
  - path: ../../../reports/rv1-wp4.md
    sha256: "sha256:83f61641c088cbe5c9d25dd16eda40b19b02dff7d7859f4d3e8cdef22c4156b8"
  - path: ../../../reports/rv1-wp5.md
    sha256: "sha256:7c9c0d1c4151fa1a7c239d6f4c30b19a5a9838db81860eb0042abcfa224e6fc0"
  - path: ../../../reports/rv1-wp6.md
    sha256: "sha256:678c200f65937121017f9f4de8a14adaebb1a50df22503e302fae3d5546e2c4a"
  - path: ../../../reports/rv1-wp6b.md
    sha256: "sha256:428f6b5a9151cf345c231571382d1833b57b6f75c249330897c2088747fb631f"
  - path: reports/w0-npm-check.log
    sha256: "sha256:90410d8c4e39b80e398c1581c598d9e71b9faeceb762eb621a509bce27c953ea"
  - path: reports/w0-verify-rust.log
    sha256: "sha256:2c51ea13ed78c41323425d62aa65c90df00a00df3153474e1f54f5a58c0bd402"
  - path: reports/wp5-boundaries.log
    sha256: "sha256:71f677d60f7f23043139a2cb433b0b30e0670a37eef02e6b6968fb8a6f0a12b1"
```

**（该机器块已在 W3 轮次 2 中重写；以下段落是轮次 1/W1-W2 时的历史说明。）** `target_commit` 当时是本轮证据的**被检视目标** = DU1 候选 `62ef264`（集成/检查/review/recheck 子 Agent 的固定输入）；`refs/heads/main` 仍为 `37a398e`，合入未获授权。`contract_digest` **已变**（`sha256:1191…ec61` → `sha256:38c5…4eab` → 本轮 `5492db84…6043`）：先是把 `p256` 收窄到 `arithmetic` 并同步 §9 判据 13/`AGENTS.md` §12/`MODULE_ARCHITECTURE.md` §3.1 与 allow-list，随后是 §11.6 写集语义第 4 条的落定审计措辞与合同版本 0.9；由 `npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` 在回填后重跑得到（`result: PASS`）。

- **（已被轮次 2 取代）** Assessment ID / Time: `admin-state-persistence-v2-w1w2-review-closure`，2026-09-23（W1/W2 闭合轮收尾时）
- Target / Task: 见 `Target` 的「W1·WP6 闭合轮」与 `Merge History` 末段；本轮闭合 `3.2`/`3.4`/`3.6`/`3.8`（四份独立 review）与它们驱动的全部修复，含唯一阻断项 `WP23-2`；最终验收任务 `8.1` 未开始
- CLI State: `openspec status` = 5/5 artifacts complete；`npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage plan --json` = PASS（`contractDigest` 见上方评估块）。CLI 状态不表示实现完成
- Audit / Evidence: 四份 RV1 报告与本地门禁全绿（`reports/verify-w1w2-closure.log`：fmt/clippy/`cargo test --workspace`/`check:drift`/`check:boundaries`/`check:docs`/`npm run check` 全部退出 0）；`core` 77 用例、`storage-sqlite` 的 `migration` 7 / `enum_coverage` 2 / `admin_store` 28 全绿。仍未做：`5.1`–`6.5`（集成就绪与候选构造）、`6.6` 合入（无授权）、`7.x` 替代验证、`8.1` 最终验收
- **（已被轮次 2 取代）** Result / Open Issues: **BLOCKED** —— 当轮未完成任务：`5.1`、`5.2`、`6.1`–`6.8`、`7.1`–`7.3`、`8.1`；其中 `6.6` 起的合入与主分支验证当时依赖合并授权（本轮（W3）已获授权并执行）
- 阻断项状态：**无未闭环阻断项**。本变更历史阻断项（WP6-1..4、WP6B-1..3、WP23-2）全部闭合；`rv1-wp1`/`rv1-wp4`/`rv1-wp5` 结论 `correct`，`rv1-wp23` 的 `incorrect` 仅由 `WP23-2` 引起且已在本轮闭合
### 最终验收轮次 1（2026-09-23，执行者：主 Agent）— 结论 **BLOCKED**

**验收 ID / 时间**：`admin-state-persistence-v2-final-round-1`，2026-09-23（W1/W2 + DU1 闭合轮之后）。

**目标与核实**：本轮验收的是**本地**主分支 `refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（本地 `git rev-parse`）。远端 `origin` 存在（`git@github.com:lindongfang/acp-remote.git`），本轮**未刷新** `origin/main`，按 `workflow-check.md` 的说明，远端引用只是缓存、不构成实时状态，因此验收目标是本地主分支引用。；交付候选为本分支 `62ef2649ae6d35e65930df505b2cf41858a19d26`，**尚未合入 main**（`6.6` 未执行——缺合并授权，`plan.md` 的 Merge Strategy 明确合并/推送受当前会话限制）。

**各审计组结论**（按 `procedures/acceptance.md` 的分组）：

| 审计组 | 结论 | 依据 |
| --- | --- | --- |
| Contracts and Coverage | **PASS（候选上）** | proposal 的意图/硬约束/非目标与交付方向一致；`plan.md` 的 Coverage Index 逐项有任务与证据（`verification.md` 的 Checks 表 + `openspec/changes/.../reports/*.log`）；specs/design 无后续澄清悬空。 |
| Delivery and Versions | **BLOCKED** | 候选构造/PV1/review/recheck 全部完成（`reports/du1-integrator.md`、`du1-pv1.log`、`rv1-du1.md`、`rv1-du1-r2.md`），但合入 `6.6` 与主分支检查 `6.7`/`6.8` 未执行（缺授权），因此不存在「实际合入结果与候选一致性」证据。 |
| Project Checks and Resources | **PASS（候选上）** | `npm run verify` 口径四条 + `check:drift` + `check:boundaries` + `check:docs` + `npm run check` 在候选上全部退出 0（`reports/verify-du1-fixes.log`、`reports/verify-du1-fixes-2.log`），并在**只含被跟踪文件的干净检出**上复跑 `check:docs`（`reports/clean-tree-check.log`，88 个 `.md`、无 `.omp/`）；构建目录按执行者隔离（`target/du1-*`），无共享资源污染记录。 |
| Independent Reviews | **PASS（候选上）** | 六个独立子 Agent（不继承实现对话）：`RvWp1`/`RvWp23`/`RvWp4`/`RvWp5`（WP 切片）、`Du1Review`（候选）、`Du1Recheck`（修复复核，结论 `correct`）；全部 CRITICAL/MAJOR（含 `WP23-2` 阻断项与 `DU1-R1` F1 阻断项）均已闭环并留复核依据，非阻断项有处理结论。 |
| E2E Design and Execution | **PASS（不适用判据已固化）** | mode `not-applicable` 有 reason/basis/非空 alternative_checks（`plan.md`，附用户降级批准）；`e2e check` 本轮返回 BLOCKED（原因见下），因 `[e2e-owned]` 行要求变更完成才判定；替代验证是 `7.1` 的独立任务，待在最终主分支版本上执行。 |
| Issue Closure and Evidence Validity | **PASS（候选上）** | 历史阻断项 WP6-1..4/WP6B-1..3/WP23-2/DU1-R1-F1 全部闭合；F4（未登记切片）已回退并留 patch；W0/WP6 期间的报告在契约块未变的部分按原 ID 复用（见 Checks 与 Check Plan Changes 的说明）。 |

**CLI 原始状态**（独立记录，未被证据结论改写）：

- `openspec status --change admin-state-persistence-v2 --json`：5/5 artifacts complete（`planningHome` = 本仓库）。
- `npx --quiet --no-install openspec-agentic e2e check --change admin-state-persistence-v2 --json` → 退出码 1，`result: BLOCKED`，reason: `任务未全部完成（除最终 E2E 行与最终验收行外仍有待办）；单变更检查要求变更完成后判定，未 PASS 即阻断`。
- `npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --stage final --json` → 退出码 1，`result: FAIL`，errors 10 条：`6.6`/`6.7`/`6.8`/`7.1`/`7.2`/`7.3` 未完成、`当前代码 HEAD 与计划目标引用不一致`、`验收结论的 target_commit 已失效`、`当前证据验收结论不是 PASS`、`目标代码状态不能验收：执行后工作区仍有未提交改动`（含宿主未跟踪项 `.omp`/`.pi/**`）。前 6 条是合并授权缺失的直接后果；后 4 条同源于「候选未合入 main + 记录尚未提交 + 验收结论按设计为 BLOCKED」。

**记录侧修复（本轮）**：`agentic-assessment` 的 `evidence` 路径改为**相对 `changeDir`** 解析（`workflow-check.md` 的规定；根目录材料写 `../../../reports/...`），并把 `plan.md` 引用的 `reports/wp6-admin-store-tests.log` 放入 `changeDir/reports/`，使最终阶段不再有「证据不可读取」类错误。

**证据与复用判断**：本轮 `agentic-assessment` 的 `evidence` 覆盖全部被引用报告（路径相对 `changeDir`，原始文件同时保留在仓库根 `reports/`）；W0/WP6/W1-W2 期间的报告在对应契约块与用例未变的前提下复用，凡改动过的部分（§11.6 措辞、`p256` 依赖面、夹具序列与端点）都有本轮新增或复跑的证据（Check Plan Changes 30–38）。

**未解决项**：`6.6`（合入）、`6.7`/`6.8`（主分支复验与复核）、`7.1`–`7.3`（替代验证与 `[e2e-owned]` 门禁）、`8.1`（最终验收）——全部只缺**合并授权**；另有两条已登记的实现残留（`revoke_reason`/`cache_policy` 的 `IN (...)` 逐值断言，见 Check Plan Changes 第 34 条）。

**提交后复跑**（记录已提交为 `0b5fa80` 之后，`reports/verify-final-round1.log` 末段）：`workflow check --stage final` 仍为 `FAIL`、错误仍为 10 条：其中 `目标代码状态不能验收：执行后工作区仍有未提交改动` 一条仍在，但清单已从「本变更文件 + 宿主文件」变为**仅宿主安装的未跟踪项**（`.omp`、`.pi/prompts/opsx-verify.md`、`.pi/settings.json`）——本变更自身的文件已全部提交；其余 9 条（6 条未完成任务 + `HEAD 与计划目标引用不一致` + `target_commit 已失效` + `证据结论不是 PASS`）**全部同源于「候选未合入 main、合并门禁未执行」**；`git status --porcelain` 里只剩宿主安装的未跟踪项（`.omp/`、`.pi/**`），不属于本变更的交付物。

**结论**：**BLOCKED** —— 没有已确认的产品缺陷或未闭环阻断项；但变更未合入目标主分支、合并门禁任务未完成、`workflow check --stage final` 非 PASS，因此**不具备归档条件**，`8.1` 保持待办，不做归档、合并、推送或发布。
- Required Follow-up: ① 四份独立 review（`3.2`/`3.4`/`3.6`/`3.8`）**已全部闭合**（`rv1-wp{1,23,4,5}.md`；唯一阻断项 WP23-2 与其余发现全部处理，见 Review Findings 的 W1/W2 闭合轮表）→ ② W3：`5.1`（集成 Agent 交接记录）/`5.2`（DU1 就绪）→ `6.1`–`6.5`（核实 `refs/heads/main`、构造候选、候选 PV1、候选 review、E2E 不适用核对）；③ `6.6` 合入与 `6.7`/`6.8`、`7.x`、`8.1` **依赖合并授权**，本轮不授权 → ④ W4 的替代验证与最终验收待授权后执行

### 最终验收轮次 2（W3，2026-09-23，执行者：主 Agent）— 结论 **PASS**

**验收 ID / 时间**：`admin-state-persistence-v2-w3-merge-final`，2026-09-23（合入、主分支复验、替代验证与 `[e2e-owned]` 门禁完成之后）。

**目标与核实**：用户在本轮给予**合并授权**；交付单元已按条件更新合入**本地** `refs/heads/main`，验收目标 = `afedea3a58b31df01087d28b1f28802ac8f96739`（`git rev-parse refs/heads/main`）。验收在**只含被跟踪文件的干净 worktree** `D:/Project/acp-remote-main`（`git worktree add … main`；执行 `git status --porcelain` 为空、`HEAD` 等于目标提交）内执行，`openspec` 与扩展命令均以 `--planning-root D:/Project/acp-remote` 指向权威规划根——这同时规避了宿主安装目录（`.omp/`、`.pi/settings.json` 等未跟踪文件）造成的「变更目录之外有未提交改动」误判。远端 `origin` 存在但本轮**未推送、未刷新**（授权范围是本地主分支）。

**合入结果**：`37a398e9…` → `86f282bf…`（`git update-ref refs/heads/main 86f282bf… 37a398e9…`，旧值 CAS，退出 0）；链条 `86ae8b4` → `aed9fb5` → `601c8ae` → `62ef264`（唯一带代码差异的提交）→ `0b5fa80` → `86f282b`；其后 `a9de891`（6.7/6.8/7.x 记录）、`afedea3`（7.3 回写）为**仅文档/证据**提交。`62ef264..86f282b` 对代码零差异，故候选轮证据按同一代码内容复用并在 `reports/alt-final-verification.md` 写明适用性。

**各审计组结论**：

| 审计组 | 结论 | 依据 |
| --- | --- | --- |
| Contracts and Coverage | **PASS** | proposal 意图/硬约束/非目标与交付一致；`plan.md` Coverage Index 的任务与证据在最终版本上仍成立；`contract_digest` 随 tasks.md 的执行记录更新为 `sha256:92c48c1a…6cc9`（预期变化，见 `Check Plan Changes` 39）。 |
| Delivery and Versions | **PASS** | 6.6 合入（CAS 快进）+ 6.7 在 `main` 上重跑 `npm run verify`（退出 0，`reports/du1-main-verify.log`）+ 6.8 独立复核（`correct`，`reports/rv1-du1-merge.md`）；候选与最终版本同一代码内容，包含关系可核。 |
| Project Checks and Resources | **PASS** | `npm run verify` 口径四条 + `check:drift`（§7 36 条 DDL、§5 15 trait/87 方法逐条一致）+ `check:boundaries`（6 crate 矩阵一致）+ `npm run check` 全部退出 0；验收使用的干净 worktree 与 `CARGO_TARGET_DIR` 复用方式记录在 `Check Plan Changes` 39；无共享可变资源污染。 |
| Independent Reviews | **PASS** | 七个独立 reviewer/检查子 Agent（不继承实现对话）：`RvWp1`/`RvWp23`/`RvWp4`/`RvWp5`/`Du1Review`/`Du1Recheck`/`RvDu1Merge`；全部阻断项（`WP23-2`、`DU1-R1-F1`）闭合且未回归；本轮合入差异复核结论 `correct`。 |
| E2E Design and Execution | **PASS** | mode `not-applicable`（reason/basis/非空 alternative_checks + 用户降级批准）；`e2e check --planning-root …` = **PASS** 并已按 `[e2e-owned]` 自动勾选 7.3；替代验证四项在最终版本上逐项执行（`reports/alt-final-verification.md`）。 |
| Issue Closure and Evidence Validity | **PASS** | 历史阻断项全部闭合；`RvDu1Merge` 的 2 条 P3 记录债（合入后未回填、散文行与机器块矛盾）本轮闭合；证据在最终版本上逐项复核（夹具 SHA-256 前后不变、临时目录已清理）。 |

**CLI 原始状态**（独立记录，未被证据结论改写）：`openspec status` = 5/5 artifacts complete；`openspec-agentic e2e check --change admin-state-persistence-v2 --planning-root D:/Project/acp-remote --json` → 退出 0、`result: PASS`（`mode: not-applicable`、`approval: true`、`marked: true`）；`openspec-agentic workflow check --change admin-state-persistence-v2 --planning-root D:/Project/acp-remote --stage final --json` → 见下方「最终阶段检查」一行。

**未解决的剩余项**：无未闭环 FAIL/BLOCKED。已登记的实现残留仅一条（`revoke_reason`/`cache_policy` 两列缺 `IN (...)` 逐值断言，`Check Plan Changes` 34 尾段），其在最终版本上仍由行为用例覆盖，不影响本次结论。

**结论**：**PASS** —— 目标为**本地** `refs/heads/main` 的 `afedea3a…`，交付单元已合入且在该版本上完成主分支复验、独立复核、替代验证与门禁；据此勾选 `8.1`。本轮**未**推送、未归档；归档前若目标版本或证据再变化，须重新验收。

**最终阶段检查**：`npx --quiet --no-install openspec-agentic workflow check --change admin-state-persistence-v2 --planning-root D:/Project/acp-remote --stage final --json` → 退出码 `0`、`result: PASS`（错误列表为空）。

**验收后的记录提交**：勾选 `8.1` 与本块的落库提交（`afedea3` 之后的一次 docs-only 提交，仅动 `verification.md`/`tasks.md`）会让 `HEAD` 前移；由于检查器对 `assessment.target_commit` 与 `refs/heads/main` 采用严格相等比较，任何在其后重跑 `workflow check --stage final` 的场合都需要把 `target_commit` 更新为当时的 `refs/heads/main`（`evaluateRecordFreshness` 允许变更目录内的未提交记录，因此这是记录侧的机械更新，不是重新验收）。
