# DU1 候选独立 review（tasks.md 6.4，Review ID `DU1-R1`）

## 固定字段

- **task_id**：`6.4` [DU1]（独立 reviewer；本轮 Review ID `DU1-R1`，Review Type `integration`）
- **role**：独立代码检视子 Agent（`Du1Review`）。未参与实现、未参与实现讨论、未参与 W1/W2 四份 RV1 的检视
- **phase**：候选准备（候选已固定 `aed9fb5`，**未合入 main**、未推送、未开 PR）
- **agent_context**：本会话为新建子 Agent，由调度者以不继承父 Agent 实现对话的方式创建（`fork_turns=none` 的等效隔离），上下文只有调度者交办的本模板与仓库只读访问。只声明收到的输入与自身限制，不声称能证明宿主未注入其它上下文。**只读边界**：未修改任何文件（本报告除外）、未构建、未运行测试、未 `commit`/`merge`/`push`/`checkout`/`stash`
- **target_revision**：`aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`（分支 `feat/admin-state-persistence-v2`）
- **base / 目标主分支**：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（本单元组成提交：`013f2b9` → `1baea5b` → `f43f7a7` → `5404610` → `86ae8b4` → `aed9fb5`；`5d77f25`/`7cdff57`/`28f8cb9` 为分支继承的文档提交）
- **scope**：候选相对基线的完整 diff（`git diff 5d77f25 aed9fb5`，68 文件 / 约 15k 增行）并按 integration 侧重组合行为；含 `core::ports` 写集 DTO 与 `storage-sqlite` 三个管理 store 的组合、`UseCases` 写集路径与审计动作的对应、`migrate.rs` 与 `tests/{migration,enum_coverage,imported,commit}.rs` 的一致性、`Cargo.toml`/allow-list/三处文档的依赖描述、以及 `86ae8b4` 的四项新行为（`node.paired`、授权顺序、`p256` 收窄、夹具序列与升级回滚断言）
- **changes**：只新增 `reports/rv1-du1.md`；仓库其它文件零改动（`git status --porcelain` 检视前后一致，仅主 Agent 的 `tasks.md` 勾选改动）
- **checks**：未执行任何构建/测试（角色只读边界）；静态核对 + 只读工具复核（`git diff`/`git show`/`git ls-tree`/`node:sqlite` 只读读夹具/在 `git archive` 出的临时干净树上运行 `scripts/check-doc-links.mjs`）；候选 PV1（`reports/du1-pv1.log`，任务 6.3）已于检视期间返回且结论 PASS，但其运行环境含未跟踪的 `.omp/**`（见 Assessment），不影响本轮判断
- **issues**：6 条（1 × MAJOR 阻断、4 × MINOR、1 × 提示）；见 Findings
- **result**：**FAIL**（有 1 条已确认且未解决的 MAJOR：候选在干净检出上无法通过仓库自身的 `npm run check`，因此 `npm run verify` / PV1 与 CI `checks` 在该状态下必红；其余为非阻断项）
- **evidence_paths**：`reports/rv1-wp{1,23,4,5,6,6b}.md`、`reports/verify-w1w2-closure.log`、`reports/wp6-admin-store-tests.log`、`reports/du1-pv1.log`、`reports/du1-checker.md`、`reports/du1-integrator.md`、`openspec/changes/admin-state-persistence-v2/verification.md`、本报告；检视期的只读探针（`git archive` 干净树与夹具副本）位于系统临时目录且已删除
- **resource_cleanup**：仓库内未新增任何探针文件；只读打开夹具产生的 `fixtures/storage/v2/{empty,too-new}.sqlite3-{wal,shm}` 旁文件（0 字节 WAL）已删除，夹具本体字节未变（`git status` 未报 modified）；系统临时目录的探针目录与副本已删除；无数据库服务、无后台进程、未占用任何 `target/` 构建目录

## Review Context

- **读取的规则与需求**：`openspec/schemas/agentic/roles/reviewer.md`（全文）；`openspec/changes/admin-state-persistence-v2/{proposal,design,plan,tasks,verification}.md` 与 `specs/**/spec.md`（5 份）；`AGENTS.md` §1/§10/§11/§12；合同 `docs/CORE_PORTS_AND_STORAGE.md` §3.1/§3.5/§3.6/§3.7/§4/§5.1/§5.2/§5.3/§6/§7.2/§7.3/§7.4/§9（判据 1–29）/§11；`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md` §3.1/§4.1/§4.7/§5、`docs/CONFIG_REFERENCE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2–§5.5/§5.8/§6、`docs/SECURITY_DESIGN.md` §14.2。
- **实际检视的代码范围（目标版本全文/相关范围）**：`crates/core/src/{ports.rs,broker.rs,use_cases.rs,model/{config.rs,identity.rs,ids.rs,error.rs,backend.rs,export.rs,mod.rs,tests.rs}}`；`crates/storage-sqlite/src/{lib.rs,error.rs,migrate.rs,session_store.rs,admin/{mod.rs,trust.rs,export.rs,local_config.rs}}`；`crates/storage-sqlite/tests/{migration.rs,admin_store.rs,enum_coverage.rs,imported.rs,commit.rs,support/mod.rs}`；`Cargo.toml`、`crates/core/Cargo.toml`、`crates/storage-sqlite/Cargo.toml`、`scripts/check-crate-boundaries.mjs`；`AGENTS.md`、`docs/**` 与 `codex` 交付记录等文档 diff；只读复核 `fixtures/storage/v{1,2}/*.sqlite3` 的实际字节内容（`node:sqlite`）。
- **使用的验证证据及限制**：`reports/verify-w1w2-closure.log`（标签为 `HEAD 5404610 + 工作区`，**不是**候选提交本身）、`reports/wp6-admin-store-tests.log`、`openspec/changes/admin-state-persistence-v2/reports/*.log`、四份 RV1 报告；`verification.md` 的 `agentic-assessment` 中 6 个证据文件的 `sha256` 与磁盘实测**逐个一致**（证据完整性无异常）。不声称执行过任何测试；候选自身尚无 PV1 记录。
- **不复审范围**：四份 RV1（`3.2/3.4/3.6/3.8`）与 WP6 两份 RV1（`rv1-wp6*`）已闭合项的同范围结论；仅在发现「修复不成立」时重开（本轮重开了 `WP23-2` 的落点，见 F4）。

## Findings

### F1（MAJOR，阻断）`AGENTS.md` 引用了未入库的 `.omp/commands/commit.md`，干净检出上 `check:docs` 必红

- **位置**：`AGENTS.md:224`（`013f2b9` 引入；`git log -S` 确认该串只在该提交出现，main 上不存在）。
- **触发条件**：在**任何干净检出**（CI runner、新 clone、`git archive`、`git clean -xdf` 后的工作区）运行 `node scripts/check-doc-links.mjs`（属于 `npm run check` 的第七道门禁 `check:docs`，也由 `.husky/pre-commit` 触发）。该脚本对 markdown 相对链接（方括号紧跟圆括号的相对目标写法）做 `existsSync` 判定，缺失即 `error` + `exit 1`。（**主 Agent 注**：本条报告落库时按门禁要求把链接目标写成从 `reports/` 可解析的相对路径，正文语义未改。）
- **预期 / 实际**：预期 `npm run check` 退出 0（`verification.md` 与本单元记录均如此声称）。实际：`.omp/commands/commit.md` 在候选树里**未被跟踪**（`git ls-tree -r aed9fb5` 中 `^\.omp/` 计数为 0，main 同样为 0；`git check-ignore` 退出 1，即它既未跟踪也未忽略，只存在于本机工作区），因此文本里那句「两份随仓库提交且正文必须保持一致」不成立（`.pi/prompts/commit.md` 确实被跟踪，`.omp/` 侧一份都没有）。
- **证据（已在临时干净树上实测）**：`git archive aed9fb5 | tar -x -C %TEMP%\acp-doclink-probe`（仅跟踪文件、无 `.omp/`）后运行 `node scripts/check-doc-links.mjs` →
  `error: AGENTS.md:224: 链接目标不存在 -> .omp/commands/commit.md` / `doc link check failed: 1 problem(s)` / `EXIT=1`。同一文档的其余相对链接在干净树上全部解析成功（唯一失败项就是本条）。
- **影响**：候选作为一个提交**无法通过仓库自己的合同门禁**：CI `checks` job（必需检查）与本地 `npm run verify` 都会红；任务 `6.3`（候选 PV1）若按计划在干净 worktree 执行则必然失败，而本机之所以显示 `doc links OK: 367 relative links … across 101 markdown files`，是因为本机工作区存在 15 个未跟踪的 `.omp/**/*.md`（干净树为 84 个 `.md`）。这不影响运行时行为，但直接阻断交付门禁，并让文档对贡献者给出错误事实（会去编辑一个不在仓库里的文件）。
- **建议**：二选一并同批修（a）把 `.omp/commands/commit.md`（与其它被声明的宿主模板）纳入版本控制，使断言为真；（b）若确定 `.omp/` 不入库（当前计划如此），把该句恢复为只指 [`.pi/prompts/commit.md`](../.pi/prompts/commit.md) 并删掉 `.omp` 链接与「两份随仓库提交」的表述。修复后必须在**干净检出**上重跑 `npm run check` 才能采信。

### F2（MINOR）`UseCases::put_export` 缺契约要求的创建前校验（workspace alias 存在性等）

- **位置**：`crates/core/src/use_cases.rs:649-660`（`put_export`，本单元新写入集路径）。
- **触发条件**：一次 `export.create`/`export.update` 写入的 `ExportRecord.workspace_aliases[].alias`（或 `agent_ids`）在本机 `owned_workspace`（或 Agent 目录）中并不存在。
- **预期 / 实际**：合同 §5.1（`docs/CORE_PORTS_AND_STORAGE.md:240`）写「`export.create` 必须校验每个 alias 已存在」、§11.2 第 4 条（同文件 `:1326`）写「创建前验证 Agent、workspace、模板与默认引用」、`LOCAL_ADMIN_PROTOCOL.md:440` 把它定为 `local.not_found`/`local.invalid_params`。实际：`put_export` 只做 `require_local` + 取时钟 + 组装 `export.created` 审计后直接调 `ExportStore::put_export`，而存储层只做 DTO 与 SQL 约束；`LocalConfigStore`（本单元新增，`UseCases` 已持有 `self.config`）没有任何调用点。全仓库检索 `workspaces()`/`config.workspace(` 只有 `workspace.list`/`workspace.select` 与 `create_session` 的解析路径。
- **影响**：Export 可以指向不存在的别名与未知 Agent 并落到管理表，错误要到 `session.create` 运行时才以参数类错误暴露；合同里「配置期失败」的意图落空，且 `local.not_found` 无法被生产。
- **建议**：在 `put_export` 里对每个 `workspace_aliases().iter()` 调 `self.config.workspace(alias)`，缺失即返回参数类错误（映射为 `local.not_found`）；Agent/模板参数（首切片禁参数）校验同理。若这些校验被有意留给尚未落地的 `server::local_admin`，请在 `verification.md` 的 Check Plan Changes 里登记，避免与「实现已落地」的陈述冲突。

### F3（MINOR）`UseCases::add_import` 未校验 `ownerNodeId` 是「已配对且 `kind = owner`」的节点

- **位置**：`crates/core/src/use_cases.rs:695-710`（`add_import`）。
- **触发条件**：`import.add` 以任意（未配对、已撤销或角色为 `access`）的节点 id 作为 `owner_node_id` 写入 Import。
- **预期 / 实际**：合同 §11.7（`docs/CORE_PORTS_AND_STORAGE.md:1420`）明确「`imported_import_export.owner_node_id` 指向 owned 家族节点记录，因此只存值、不用 FK；**存在性由用例层在写集内校验**（`import.add` 的前置条件）」，`LOCAL_ADMIN_PROTOCOL.md:461` 写「前置条件：`ownerNodeId` 必须是已配对且 `kind = owner` 的节点」。实际：`add_import` 只校验 `record.export_ids()` 非空，随后一次写集落库；本单元**新加**的 `TrustStore::node(&NodeId, NodeKind)`/`nodes_for` 读面在生产代码里除 `UseCases::node` 透传外没有任何调用方。
- **影响**：Import 与「已配对 Owner 身份」之间的信任锚没有代码强制，管理记录可能指向不存在或角色不符的节点（该记录随后参与 `Broker::node_allowed` 的 Import 分支判定）；跨族无 FK，所以库层也不会拒绝。
- **建议**：在 `add_import` 开头用 `self.trust.node(record.owner_node_id(), NodeKind::Owner)` 判定存在且 `state = Paired`，不满足返回参数/不存在类错误；或在合同/记录里把该义务显式下移到 `server::local_admin` 并登记为残留。

### F4（MINOR）节点批准的审计动作与冻结合同 §11.6 第 4 条不一致（合同未同步）

- **位置**：`crates/core/src/use_cases.rs:604-611`（`settle_pairing` 的动作选择，`86ae8b4` 新增）。
- **触发条件**：`node.pair.confirm`（`PairingTarget::Node` + `Approved`）。
- **预期 / 实际**：合同 §11.6 第 4 条（`docs/CORE_PORTS_AND_STORAGE.md:1389`，本单元同期写入的文本）写「`Approved` 时创建信任行、把 peer 公钥转入 `owned_peer_key`、更新配对为 `approved`、**写 `pairing.approved`**」，节点分支只补了「写下 `owned_node` 的 `access` 行」，没有区分审计动作。实际实现按目标族分派：节点批准写 `node.paired`、设备批准写 `pairing.approved`、拒绝写 `pairing.rejected`（`SECURITY_DESIGN.md` §14.2 的最小集合含 `node.paired`，见 Check Plan Changes 第 30 条）。
- **影响**：修阻断项 `WP23-2` 时改了行为但没同步 §11.6 第 4 条，合同（「唯一权威来源」）与实现、`verification.md` 第 30 条三处说法不一致；后续 `identity-auth`/`local_admin` 若照合同实现会给节点批准写第二个动作，审计语义分叉。
- **建议**：在 §11.6 第 4 条节点分支补一句「节点批准写 `node.paired`（设备批准写 `pairing.approved`；两族拒绝都写 `pairing.rejected`）」，与 `verification.md` 第 30 条、用例 `pairing_settlement_carries_the_target_family_audit` 对齐。

### F5（MINOR）`from-v1` 夹具的 Import 端点不是 `wss://`，升级库的 Import 无法经新的读路径读回

- **位置**：`crates/storage-sqlite/tests/migration.rs:891-893`（`regenerate_v2_fixtures` 写入 `endpoint_ref = https://owner.invalid`；`fixtures/storage/v2/from-v1.sqlite3` 亦为本单元新增资产）。
- **触发条件**：打开升级后的 `fixtures/storage/v2/from-v1.sqlite3` 并经 `ExportStore::import`/`imports` 读取那条 Import。
- **预期 / 实际**：`ImportRecord::try_new` 要求 `owner_endpoint` 是 `wss://`（`crates/core/src/model/export.rs:390-393`，base 起即如此，因此真实 v1 库不可能出现该值）。实际：夹具该行是 `https://owner.invalid`，`admin/export.rs` 的 `import_from_row` 会把它交给同一构造器并失败。实测夹具字节（`node:sqlite` 只读）：`imported_import` 仅一行、`endpoint_ref = https://owner.invalid`、`user_version = 1`。迁移用例对 Import 只做原始 SQL 断言（`grants_json`、`imported_import_export` 内容、列集合、行数），读路径因此不覆盖。
- **影响**：记录里「升级后 Import 保留」的保证弱于陈述；夹具本身是领域模型无法读回的状态，任何复用它的读取用例/工具都会失败。
- **建议**：改为 `wss://owner.invalid` 并重生成三个夹具，同时在 `v1_fixture_upgrades_to_v2_and_preserves_rows` 补一条经 `ExportStore::imports()` 读回的断言（`grants` 为空、`export_ids` 来自关联表），顺带补上「升级后 Import 可读」这一判据。

### F6（P3 提示）`verification.md` 的 Check Plan Changes 仍留着被后续决定推翻的旧行为

- **位置**：`openspec/changes/admin-state-persistence-v2/verification.md:63`（第 16 条），同段第 12 条与 `Failures and Retests` 的 F1 行同类。
- **触发条件**：读者按记录理解候选行为。
- **预期 / 实际**：第 16 条写「`settle_pairing` 对 `PairingTarget::Node` 的 `Approved` 返回 `InvalidRequest`（零写入）」；第 12 条写夹具 `sqlite_sequence.seq = 5`；F1 的复测描述写「序列仍为 5、新写入得 `audit_id = 6`」。实际：`f43f7a7` 已实现节点批准（`approve_node` 写 `owned_node` 与 `owned_peer_key`，用例 `node_pairing_approval_persists_access_trust`），夹具实测 `owned_audit seq = 7`（行 `1/2/5`）、`imported_audit seq = 3`，断言为 `seq = 7` 与新插入 `audit_id = 8`（第 33 条）。
- **影响**：同一文件内自相矛盾，按第 16 条会得出「节点配对无法持久化信任」的错误结论；不阻断交付。
- **建议**：为第 12/16 条与 F1 行加「已被第 24/25/33 条与 `f43f7a7`/`86ae8b4` 取代」的指针，或改写为当前行为。

## Assessment

### 检视结论（对应 Target Revision `aed9fb5`）：FAIL

- 存在 1 条已确认且未解决的 **MAJOR（F1）**：候选作为一个提交在**干净检出**上无法通过 `node scripts/check-doc-links.mjs`（`npm run check` 的第七道门禁），因而 `npm run check`、`npm run verify`（任务 6.3 的候选 PV1）与 CI `checks` 在该状态下必红；`verification.md` 与本单元日志里「`npm run check` 退出 0」只在本机（工作区存在未跟踪的 `.omp/**`）成立。
- F2–F5 为非阻断 MINOR，F6 为提示。
- 本轮**未发现产品代码的 CRITICAL/MAJOR 缺陷**：写集一事务提交与失败关闭（`admin/{mod,trust,export,local_config}.rs` 的提交顺序、`rows_affected` 正向谓词、`into_conflict` 具名映射）、`settle_pairing` 的状态机与两族审计、`p256` 收窄与 allow-list/三处文档同步、12-step 审计表重建与 `imported_import` 拆分迁移（含 `sqlite_sequence` 回填与失败整体回滚）、容量门挂载点（只加在会新增行的写路径）经静态追踪均与合同 §5.3/§7.2/§7.3/§7.4/§9/§11 一致。

### 检查计划核对（逐 ID）

- **PV2（合同漂移）**：日志与记录一致（`check:drift` 始终报 §7 36 条 DDL、§5 15 trait / 87 方法）。静态复核 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的写集 DTO/三个 trait 签名与 `crates/core/src/ports.rs` 逐项对应；§7.3/§7.4 的 DDL 文本与 `migrate.rs` 的两条常量一致（含两张审计表 CHECK 的 5 个新取值与 `imported_import_export`）。
- **PV3（依赖边界）**：`CORE_ALLOWED_CLOSURE` 实测 29 项、不含 `ecdsa`/`hmac`/`signature`/`pkcs8`，`Cargo.toml` 的 `p256` 为 `default-features = false, features = ["arithmetic"]`，`AGENTS.md` §12、§9 判据 13、`MODULE_ARCHITECTURE.md` §3.1 三处措辞同步——一致。
- **PV4（fmt/clippy/测试）**：读日志可见 `core` 77 passed；`storage-sqlite` 各目标 ok（`migration` 7 passed / 1 ignored 为夹具生成器、`enum_coverage` 2、`admin_store` 28），无失败、无全跳过；`enum_coverage` 的 `cases` 实测 26（与第 34 条一致）、`imported.rs` 为 6 表黄金列清单。按日志采信，**未复跑**（只读边界）。
- **`check:docs`（PV1 的子门禁）**：本机日志为 `doc links OK … 101 markdown files`，干净树只有 84 个 `.md`；本项**不可在干净检出上复用**，见 F1。
- **候选 PV1（任务 6.3）已返回**（`reports/du1-pv1.log`，2026-09-23）：四条命令退出码 0、阶段结论 PASS。但该日志的 `doc links OK: 367 relative links, 2164 section refs across 101 markdown files` 表明它在**含未跟踪 `.omp/**` 的现有工作区**里执行（干净树只有 84 个 `.md`），因此它**无法**发现 F1、不改变本轮结论：候选在干净检出上仍不通过 `check:docs`（本轮已实测，见 F1）。建议 `6.6` 合入前的候选门禁按**干净检出**复跑一次 `npm run check`；`reports/du1-integration.md`（5.2/6.5）与本轮并行产出，不覆盖 6.4 的静态判断。
- **证据适用性**：`reports/verify-w1w2-closure.log` 的标签是 `HEAD 5404610 + 工作区`，即**候选的父提交加未提交改动**，不是候选提交本身；本报告只把它当作 W1/W2 修复的旁证，候选判定以对 `aed9fb5` 的只读检视与干净检出复跑为准。`verification.md` 的 `agentic-assessment` 中 6 个证据文件的 `sha256` 与磁盘实测逐个一致（证据完整性无异常）。
- **测试设计**：`mode = not-applicable` 与用户降级批准的替代验证安排同 `plan.md`/`verification.md` 一致；本单元不涉及 E2E 用例，不重复判定。

### 未验证内容与限制

1. 未运行任何构建、测试或需要写文件的检查（只读边界）。唯一的执行类动作是在 `git archive aed9fb5` 解出的**临时干净树**上运行 `scripts/check-doc-links.mjs`（不写仓库、不构建，见 F1）；其余结论来自静态阅读与只读的字节/元数据复核（`node:sqlite` 只读打开夹具）。
2. 未复核 `86ae8b4` 声称的「夹具两次重生成 SHA-256 完全相同」（需运行 `#[ignore]` 生成器）；只复核了当前夹具的字节内容与 `sqlite_sequence` 值（`owned_audit seq = 7`、行 `1/2/5`；`imported_audit seq = 3`、行 `1/2`；`user_version = 1`；`meta.*_schema_version = 1`），与第 33 条一致。
3. 未实测 `cargo tree -p core --edges normal` 的闭包（依赖 allow-list 的相等性以 `check:boundaries` 的日志为准）。
4. 未复核 W1/W2 四份 RV1 与 WP6 两份 RV1 的原始结论本身；本轮只核对「修复是否成立」，并据此重开了 `WP23-2` 的落点（F4）。
5. 对 `identity-auth`/`identity-keystore`/`server`/`app`/`agent-host`/`node-link-client` 等未实现 crate 的行为不作判断；合同里属于这些层的义务（「alias 是否在该 Export 内声明」、catalog 快照与 `grant.*` 子集校验、握手期身份变化审计）按 `verification.md` 的记载视为下移项，未计入缺陷。
6. 本报告不代替 Project Verify：`FAIL` 只针对候选版本的代码/交付面静态检视与已复现的门禁失败；候选 PV1（`reports/du1-pv1.log`）返回后仍须按干净检出复跑一次 `npm run check` 才能采信「门禁全绿」。
7. 未更新任何任务状态，也未代替调度者宣称本变更可归档或不可归档。



## 复核段（W3 合入轮，2026-09-23）

合入后的独立复核由另一名 reviewer 子 Agent `RvDu1Merge`（不继承实现对话）执行，范围 `601c8ae..86f282b`（唯一带代码提交 `62ef264`），结论 `correct`、无 P1/P2、本报告的唯一阻断项 `DU1-R1-F1` 未回归；报告见 `reports/rv1-du1-merge.md`。
