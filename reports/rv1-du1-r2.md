# DU1 修复轮独立复核（tasks.md `6.4` 的「修复后独立复核」；Review ID `DU1-R2`）

## 固定字段

- **task_id**：`6.4` [DU1]（复核轮，Review ID `DU1-R2`，Review Type `recheck`；被复核的上一轮为 `DU1-R1`）
- **role**：独立代码检视子 Agent（本轮实例 `Du1Recheck`）。未参与实现、未参与实现讨论，也未参与 `DU1-R1`
- **phase**：候选准备（候选已固定 `601c8ae`，**未合入 main**、未推送、未开 PR）
- **agent_context**：本会话为新建子 Agent，由调度者以不继承父 Agent 实现/修复对话的方式创建（`fork_turns=none` 的等效隔离），上下文只有调度者交办的本模板与仓库只读访问。只声明收到的输入与自身限制，不声称能证明宿主未注入其它上下文。**只读边界**：未修改任何仓库文件（本报告除外）、未构建、未运行 `cargo`/`npm`、未 `commit`/`merge`/`push`/`checkout`/`worktree add`/`stash`
- **target_revision**：`601c8aebc262d0a0d8a694a4d950490c02ad9bf1`（修复对照版本 `aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`）
- **base / 目标主分支**：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`
- **scope**：`git diff aed9fb5..601c8ae`（13 文件 / +2757 −29）＋受影响上下文（`UseCases::put_export`/`add_import` 的调用面与消费侧、`ExportStore` 的 Import 读路径、`TrustStore::node`/`put_node` 的存储实现、`UseCases::settle_pairing`、`ExportRecord`/`ImportRecord`/`NodeRecord` 的不变量、`broker.rs` 的 `test_support` 三个 fake）＋原问题所在的原有代码（`CORE_PORTS_AND_STORAGE.md` §2/§5.1/§7.4/§11.2/§11.6/§11.7、`LOCAL_ADMIN_PROTOCOL.md` §5.4/§5.5/§6、`SECURITY_DESIGN.md` §14.2）
- **changes**：只新增 `reports/rv1-du1-r2.md`
- **checks**：未执行任何构建或测试（角色只读边界）；静态追踪 + 两处只读探针（`git archive 601c8ae` 解出仅含被跟踪文件的临时树后运行 `node scripts/check-doc-links.mjs`；`node:sqlite` 只读打开夹具的临时副本）。调度者日志（`reports/verify-du1-fixes.log`、`reports/clean-tree-check.log`）只作旁证，采信情况见 Assessment
- **issues**：原 6 项（`DU1-R1` 的 F1–F6）全部复核为**已解决**；新增 5 条非阻断发现（3 × MINOR、2 × SUGGESTION），无 CRITICAL/MAJOR
- **result**：**PASS**（Target Revision `601c8ae`）——无未解决 CRITICAL/MAJOR
- **evidence_paths**：`reports/rv1-du1.md`、`reports/du1-pv1.log`、`reports/verify-du1-fixes.log`、`reports/clean-tree-check.log`、`openspec/changes/admin-state-persistence-v2/verification.md`、本报告；只读探针位于系统临时目录，已删除
- **resource_cleanup**：仓库内未新增探针文件；夹具的只读打开发生在系统临时目录的副本上，`fixtures/storage/v2/` 未留下 `-wal`/`-shm`、字节未变；未创建 worktree、无后台进程；`git status --porcelain` 检视前后一致（只有宿主未跟踪项 `.omp/`、`.pi/prompts/opsx-verify.md`、`.pi/settings.json`、`.pi/skills/openspec-verify-change/` 与 `reports/parked-idle-identity-retention.patch`）

## Review Context

- **读取的规则与需求**：`openspec/schemas/agentic/roles/reviewer.md`（全文）；`openspec/changes/admin-state-persistence-v2/{proposal,design,plan,tasks,verification}.md`；`specs/**/spec.md`（5 份，重点 `admin-state-persistence` 与 `storage-schema-v2-migration`）；`AGENTS.md` §1/§10/§11/§12；`docs/CORE_PORTS_AND_STORAGE.md` §2/§3.1/§3.7/§5.1/§5.3/§7.3/§7.4/§9/§11.2/§11.6/§11.7；`docs/LOCAL_ADMIN_PROTOCOL.md` §5.4/§5.5/§6；`docs/MODULE_ARCHITECTURE.md` §4.1/§4.7；`docs/SECURITY_DESIGN.md` §14.2；`docs/IDENTITY_AND_AUTH_CONTRACT.md`；`docs/NODE_LINK_PROTOCOL.md` §10/§12 相关段
- **实际检视的目标版本范围（全文或相关范围）**：`crates/core/src/{use_cases.rs,broker.rs,ports.rs,model/{export.rs,identity.rs,config.rs,tests.rs}}`；`crates/storage-sqlite/src/{admin/{export.rs,trust.rs,mod.rs},migrate.rs}`；`crates/storage-sqlite/tests/migration.rs`；`scripts/check-doc-links.mjs`、`scripts/check-contract-drift.mjs`；`AGENTS.md`、`docs/CORE_PORTS_AND_STORAGE.md`；只读复核 `fixtures/storage/v2/from-v1.sqlite3` 的实际字节；`openspec/changes/admin-state-persistence-v2/{tasks,verification}.md` 的实际改动
- **未复检范围**：`DU1-R1` 已在 `aed9fb5` 上判定的同范围项（写集一事务提交与失败关闭、12-step 重建、`p256` 收窄与 allow-list、`enum_coverage` 覆盖面）；本轮只在「修复是否成立」与「修复是否引入回归」上重开结论。`identity-auth`/`identity-keystore`/`server`/`app`/`node-link-client`/`agent-host` 等未实现 crate 的运行时行为仍不作判断
- **验证证据与限制**：本轮**未运行** `cargo`/`npm`，不宣称 fmt/clippy/测试通过；调度者的 `reports/verify-du1-fixes.log`（工作区实跑）与 `reports/clean-tree-check.log` 作为旁证逐项核对，其中后者的标签问题单列为 `DU1-R2-F4`。CI 侧未接触

## Findings

### 原问题复核（沿用 `DU1-R1` 的问题 ID）

| ID | 本轮严重度 | 位置（Target） | 复核依据 / 证据 | 处理建议 | 复核状态 |
| --- | --- | --- | --- | --- | --- |
| F1 | 无（原 MAJOR） | `AGENTS.md:224` | `git ls-tree -r 601c8ae --name-only -- .omp` 计数 **0**；`.pi/prompts/commit.md` 在候选树里（`git ls-tree -r 601c8ae -- .pi` 第 1 行）；新文本把 `.omp/commands/commit.md` 写成行内代码而非方括号链接，方括号链接只留给 `.pi/prompts/commit.md`，「两份随仓库提交」的表述改成「宿主安装目录里的同名模板…**不进版本库**，因此这里不写链接」；本机 `test -f .omp/commands/commit.md` = YES、`git check-ignore -v` 退出 1（既未跟踪也未忽略），故「本机路径 + 不进版本库」两条陈述与仓库事实一致；**干净检出实测**（`git archive 601c8ae` → 仅被跟踪文件、87 个 `.md`）运行 `node scripts/check-doc-links.mjs` → `doc links OK: 366 relative links, 2226 section refs across 87 markdown files`、`EXIT=0`，而 `DU1-R1` 在同一方法下（`aed9fb5`、84 个 `.md`）得到 `1 problem(s)`/`EXIT=1` | 无需再改；`6.6` 合入前仍按干净检出复核一次 | **已解决** |
| F2 | 无（原 MINOR） | `crates/core/src/use_cases.rs:654-671` | 别名循环在读时钟与写集**之前**，逐项 `self.config.workspace(entry.alias())`，缺失返回 `InvalidRequest`；`ExportRecord::try_new` 已把 `default_workspace_alias` 与每个 `ExportTemplate.workspace_alias` 约束在 `workspace_aliases` 内（`model/export.rs:272-297`），因此只遍历 `workspace_aliases()` 不会漏别名；Agent 校验用 `AgentCatalog::agents()` 做全量存在性判定；`put_export` 的写入口全仓只有该用例（`ExportStore::put_export` 的其余调用点都在 `storage-sqlite` 测试里），无绕过路径 | 覆盖判别力不足见 `DU1-R2-F1`；错误码见 `DU1-R2-F3` | **已解决**（含 1 条非阻断新发现） |
| F3 | 无（原 MINOR） | `crates/core/src/use_cases.rs:718-736` | 用 `TrustStore::node(record.owner_node_id(), NodeKind::Owner)` 取行并要求 `state() == NodeState::Paired`；存储侧 `node()` 的 SQL 确实带 `AND kind = ?`（`admin/trust.rs:202`）、`state` 经 `NodeState` 解析（`admin/trust.rs:90`），`NodeState` 取值 `pending/paired/revoked`（`model/identity.rs:277`），故「已配对 + owner 角色」两半都能被真实存储表达与判定；写集仍在单个 `import.add` 端口调用里提交 | 覆盖判别力不足见 `DU1-R2-F2`；错误码见 `DU1-R2-F3` | **已解决**（含 1 条非阻断新发现） |
| F4 | 无（原 MINOR） | `docs/CORE_PORTS_AND_STORAGE.md:1390`（§11.6 第 4 条） | 新文本「按目标族写『信任建立』审计——**设备批准写 `pairing.approved`、节点批准写 `node.paired`**，两族拒绝都写 `pairing.rejected`」与实现逐值一致：`use_cases.rs:606-611` 的 `match (settlement.is_approved(), record.target())` 给出 `PairingRejected`/`PairingApproved`/`NodePaired`；`SECURITY_DESIGN.md:451` 的最小集合确含 `node.paired`；`verification.md` 第 30 条与用例 `pairing_settlement_carries_the_target_family_audit` 同口径；新增版本行 0.9（`CORE_PORTS_AND_STORAGE.md:14`）如实描述本行与 §2 枚举；`scripts/check-contract-drift.mjs` 不解析版本行，§5.3/§7 文本未动 | 无需再改 | **已解决** |
| F5 | 无（原 MINOR） | `fixtures/storage/v2/from-v1.sqlite3`；`crates/storage-sqlite/tests/migration.rs:280-289`、`:905` | 夹具字节只读实测（`node:sqlite`，`user_version=1`）：`imported_import` 单行 `endpoint_ref = wss://owner.invalid`、`imported_audit` 行数 2、`sqlite_sequence` 为 `owned_audit=7`/`imported_audit=3`（`grep` 也命中二进制页里的 `wss://owner.invalid`）；生成器 `:905` 已同步；新增断言经 `ExportStore::imports()` 读回：`len()==1`、`owner_endpoint()`、`export_ids()==[export-fixture]`，而 `admin/export.rs:219-243` 的 `import_from_row` 正是把关联表结果交给同一 `ImportRecord::try_new`（非 `wss://` 会失败），`display`/`owner`/`added_at` 另有既有原始 SQL 断言；`grants_json` 空数组经 `GrantSet::try_from_iter`（`model/identity.rs:40-50`，空集合为 `Ok`）可读回 | 无需再改 | **已解决** |
| F6 | 无（原 P3） | `openspec/changes/admin-state-persistence-v2/verification.md:63`、`:102`、`:199` | 三处旧描述都加了取代指针：第 16 条→「已被第 25/30 条取代」、第 12 条→「序列值已被第 33 条修正为 `seq = 7`/`seq = 3`」、Failures 表 F1 行→「第 33 条修正后为：序列 7、新写入得 `audit_id = 8`」；所指的第 25/30/33 条在 `## Check Plan Changes` 里存在且内容相符（25 = 节点批准的角色推导、30 = `node.paired` 写入方、33 = 夹具序列与判别力），未新增悬空引用；`doc-links` 在干净树上退出 0（§-引用判定通过） | 第 16 条保留句见 `DU1-R2-F5` | **已解决**（含 1 条非阻断新发现） |

#### F2 / F3 的回归用例判别力（逐条假想回退 → 断言结果）

`export_and_import_preconditions_are_enforced`（`use_cases.rs:1475-1607`）的每个断言在假想回退下的表现：

| 假想回退（对 Target 的实现） | 用例行为 | 是否拦住 |
| --- | --- | --- |
| 删除整个别名循环（`use_cases.rs:654-661`） | 第一个用例仍返回 `InvalidRequest`——因为此刻 `world.catalog_agents` 为空（`FakeWorld` 派生 `Default`，`broker.rs:3062-3063`）而 `agent_ids = ["codex"]`，Agent 校验必然先失败；步 2/步 3 同理不依赖别名循环 | **否** |
| 把别名判定反向（`is_none()` → `is_some()`） | 步 3（别名已登记 + Agent 已知）会返回 `InvalidRequest`，`.expect("a fully resolvable export must be accepted")` panic | 是 |
| 删除 Agent 存在性校验 | 步 2（别名已登记、目录为空）不再报错，`.expect_err("an unknown agent must be refused")` panic | 是 |
| `self.trust.node(...)` 的 `kind` 过滤改成 `nodes_for`（只看状态） | 步「access-role node」不再报错，`.expect_err("an access-role node must not own an import")` panic | 是 |
| 删掉 `node.state() == NodeState::Paired`（改成 `Some(_) => {}`） | 三例全过：缺失→`None` 报错；access→`node(id, Owner)` 无行报错；`Owner + Paired`→接受 | **否** |
| 把状态判定放宽成「非 `Revoked` 即可」 | 同上，无 `Pending`/`Revoked` 用例可触发 | **否** |
| 把 `import.add` 的 owner 前置整体删除 | 步「absent owner node」不再报错，`.expect_err(...)` panic | 是 |

### 新发现（本轮 Review ID `DU1-R2`）

| ID | 严重度 | 位置 | 触发 / 证据 | 影响 | 建议 |
| --- | --- | --- | --- | --- | --- |
| DU1-R2-F1 | MINOR | `crates/core/src/use_cases.rs:1517-1524` | 见上表第 1 行：新用例的「别名未登记 → 拒绝」断言只判 `matches!(error, PortError::InvalidRequest(_))`，而同一输入在别名校验被删掉后仍会因 Agent 校验返回同型错误 | 该回归用例无法拦住「workspace alias 前置校验被删除」这一回退（F2 的核心行为）；`F2` 的修复因此没有独立的回归网 | 让首个用例只剩一个失败原因：把 `catalog_agents` 的 push 提到该用例之前（`world.catalog_agents` 里放 `agent_ref`），使别名缺失成为唯一拒绝原因；或在该断言上补 `assert!(error.to_string().contains("workspace alias"))` 之类的具名判定 |
| DU1-R2-F2 | MINOR | `crates/core/src/use_cases.rs:1588-1600` | 见上表第 5/6 行：用例只造了 `Owner + Paired`（接受）与 `Access + Paired`（拒绝），没有任何 `Owner + Pending`/`Owner + Revoked` 行 | 「已配对」这半条前置（`LOCAL_ADMIN_PROTOCOL.md` §5.5「必须是已配对且 `kind = owner` 的节点」）没有用例覆盖：把 `state() == NodeState::Paired` 删掉或放宽都不会让任何断言失败 | 在现有 helper 上补一例：`fixture.world.nodes.lock()…push(node_record(&owner, NodeKind::Owner, NodeState::Pending));` 后断言 `add_import` 仍返回 `InvalidRequest`（helper 已按状态成对填 `revoked_at`，`Pending` 行可构造） |
| DU1-R2-F3 | SUGGESTION | `crates/core/src/use_cases.rs:657`、`:668`、`:730` | 两条新前置的失败一律是 `PortError::InvalidRequest(<英文消息>)`；而 `LOCAL_ADMIN_PROTOCOL.md` §6 的映射表把「引用的 workspace alias 未在 `owned_workspace` 建立」与「目标 `nodeId` 不存在」都定为 `local.not_found`（`:561`），§5.5 也要求区分「不存在」与「参数非法」（`:461` 的 exportIds 两分支、`:440` 的 `local.not_found`）；同族代码的既有约定相反：未知配对由 core 返回 `NotFound(EntityRef::Pairing(id))`（`verification.md` 第 30 条、`use_cases.rs:604`），而 `EntityRef::Node` 变体本就存在（`model/ids.rs:498`） | 适配器落地时无法按变体映射到 `local.not_found`，只能匹配消息文本（易随文案漂移）；两条新行为也未登记进 `verification.md` 的 `## Check Plan Changes`（对比第 30/31 条同类修复都有条目） | 建议「owner 节点不存在」分支改用 `NotFound(EntityRef::Node(record.owner_node_id().clone()))`（角色不符仍用参数类错误）；别名缺失受限于 `EntityRef` 无 workspace/node 变体，可在 `verification.md` 的 Check Plan Changes 里登记「由适配器按消息映射为 `local.not_found`」这一义务 |
| DU1-R2-F4 | MINOR | `reports/verify-du1-fixes.log:545`（本轮新增文件） | `reports/clean-tree-check.log` 自述「`git worktree add --detach <tmp> 601c8ae`，只有被跟踪文件」，却报 `across **104** markdown files`；本机工作区（含 17 个未跟踪 `.md`：`.omp/**` 15 + `.pi/**` 2）实跑同为 `104`，而 `git archive 601c8ae` 的仅被跟踪文件树是 **87** —— 干净的 `git worktree add` 检出不可能含未跟踪宿主文件，故该日志的数字与自己的标签矛盾；`reports/verify-du1-fixes.log` 本身也没有任何 revision/树状态标签（首行直接是 `### cargo fmt …`） | `DU1-R1` 的 F1 明确要求「修复后必须在干净检出上重跑 `npm run check` 才能采信」；若后续记录引用 `clean-tree-check.log` 作为干净检出证据，等于复用一条无依据的证据（结论本身为真——见 F1 行本轮实测，但证据产物不可采信） | 在真实干净检出（`git worktree add --detach …` 或 `git archive` 解出的目录）重跑并覆盖该日志，写上「树状态 + revision + `.md` 计数」标签；`verify-du1-fixes.log` 补一行说明它是在含未跟踪 `.omp/**` 的工作区执行的 |
| DU1-R2-F5 | SUGGESTION | `openspec/changes/admin-state-persistence-v2/verification.md:63` | 该行在插入「已被第 25/30 条取代」的同时保留了原文「批准节点配对必须先经 `UseCases::put_node`（§11.6 第 2 条把『写节点角色行与身份材料』定在那里）」；而同文件第 25 条与实现相反：节点批准的节点行由 `settle_pairing` 的 `approve_node` 自己写（`admin/trust.rs:962-996`，`NodeKind::Access`），不需要先经 `put_node` | 同一条记录里留下与第 25 条相反的句子，按第 16 条阅读会得出错误的实现顺序假设（`identity-auth` 落地时可能多写一次 `put_node` 并撞指纹/角色约束） | 把该句一并划掉或改成「当时以 `put_node` 预写节点的做法已被第 25 条的角色推导取代」 |

## Assessment

### 检视结论（Target Revision `601c8ae`）：PASS

- 原 6 项（`DU1-R1` 的 F1–F6）全部复核为已解决，其中 F1 是上一轮唯一的阻断项，本轮用与上一轮相同的只读方法在**仅含被跟踪文件**的树上实测 `node scripts/check-doc-links.mjs` 退出 0，正面对照上一轮的 `1 problem(s)`。
- 本轮未发现产品代码的 CRITICAL/MAJOR 缺陷，也未见修复引入的语义回归；新发现 5 条均为非阻断（3 × MINOR + 2 × SUGGESTION），集中在「回归用例判别力」「错误码映射与登记」「验证证据标签」三处。

### 检查计划核对（逐 ID）

- **PV1（候选 project verify）**：`reports/du1-pv1.log` 是**上一轮**候选 `aed9fb5` 的日志（R1 已指出它在含未跟踪 `.omp/**` 的工作区执行，因而发现不了 F1）。本轮 `601c8ae` 自身的 PV1 尚未产出：**待返回**，但按任务安排 `6.4` 的修补复核不依赖它；它影响 `6.6` 合入门禁，不影响本轮的代码判断。
- **PV2（合同漂移）**：本轮修改了 `docs/CORE_PORTS_AND_STORAGE.md` 的版本行与 §11.6 第 4 条。静态核对：§5.3 的 rust 块与 §7 的 DDL 块未被触碰（`git diff` 仅 2 行文档改动），`scripts/check-contract-drift.mjs` 不解析版本行，`verify-du1-fixes.log:536` 仍报 `§7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …` —— 与「签名、判据与 DDL 未变」的版本行陈述一致。日志按证据采信（本轮未复跑）。
- **PV3（依赖边界）**：本轮未改 `Cargo.toml`/allow-list，`verify-du1-fixes.log:540` 报 `crate boundaries OK: 6 个 crate …`；`DU1-R2-F4` 的标签问题不涉及本项。
- **PV4（fmt/clippy/test）**：本轮改动了 `crates/core`（新增前置校验与用例）与 `crates/storage-sqlite/tests/migration.rs`。`verify-du1-fixes.log` 记录 `core` 78 passed（上一轮 77，与新增用例 `export_and_import_preconditions_are_enforced` 一致）、`storage-sqlite` 各目标 ok（含 `migration` 7 passed / 1 ignored）。**待返回**：候选自身的 PV4；本轮不重复采信，但新增断言的静态一致性（断言目标、构造器约束、夹具实际字节）已逐条核对，未发现会导致其失败的矛盾。
- **check:docs**：本轮唯一必须复跑的项（`DU1-R1` F1）。已在本轮的干净树上实测通过；同时发现调度者「干净检出」日志的标签问题（`DU1-R2-F4`）。
- **E2E（`7.x`）**：本变更 `mode = not-applicable`（用户批准的替代验证安排），本轮不涉及 E2E 用例或配置，不重复判定。
- **证据复用与计划变更**：`verify-w1w2-closure.log`（标签为 `HEAD 5404610 + 工作区`）与 `du1-pv1.log` 均**不**被本轮当作候选证据；`verify-du1-fixes.log` 只作旁证（其内容与 Target 树一致，但缺 revision 标签，见 `DU1-R2-F4`）。

### 系统与回归面向（第 5 项交办）

- **调用方影响**：`UseCases::put_export`/`add_import` 在仓库内没有其它生产调用方（`server::local_admin` 未实现）；`storage-sqlite` 的测试直接调端口写集，不受用例层前置影响。`UseCases::put_node`、`settle_pairing`、`node.*` 读面未改。
- **测试替身改动**：`TestCatalog` 由单元结构体改为持 `Arc<FakeWorld>`（默认空目录 → 语义不变）；`FakeTrust` 的 `node`/`nodes_for`/`put_node` 由恒空/空操作改为真实读写，但 `put_node` 在 core 侧没有既有用例调用、`node`/`nodes_for` 在用例层除本次新增断言外无人读，因此对既有用例无语义影响（`verify-du1-fixes.log` 的 `core` 78 passed 与之一致）。
- **未登记的行为变化**：代码侧新增的两条前置校验只登记在 `verification.md` 的 Check Plan Changes 之外——对比第 30/31 条同类修复都有条目，本处缺条目；已在 `DU1-R2-F3` 中作为非阻断项提出，建议随本轮记录回填一并补上。
- **合同义务的下移核对**：`agent_ids` 的 `available` 标志未参与判定、`§5.5` 的「首切片 template 不得声明参数」与 `import.add` 的 catalog 快照/`grants` 子集校验仍属未实现的适配器；合同未把这几项要求写进 core（§11.2 第 4/5 条的「验证 Agent、workspace、模板与默认引用」中，模板与默认引用由 `ExportRecord::try_new` 的结构不变量兜住），因此**不计**为缺陷；`NODE_LINK_PROTOCOL.md` §10/§12.3 的相关义务按 `verification.md` 的记载视为下移项。

### 未验证内容与限制

1. 未运行任何构建、测试或写入型命令（只读边界）。执行类动作只有两处只读探针：`git archive 601c8ae` 解出的临时树跑 `doc-links`（不写仓库、不构建）、以及在系统临时目录的夹具副本上用 `node:sqlite` 只读查询。未复跑 `npm run check`/`cargo test`/`check:drift`/`check:boundaries`，这些结论按 `verify-du1-fixes.log` 采信并标注为旁证。
2. 未复核「夹具两次重生成 SHA-256 完全相同」（需跑 `#[ignore]` 生成器）；本轮只复核了提交夹具的实际字节（`endpoint_ref`、`user_version`、`sqlite_sequence`）与新断言自洽。
3. 未复核 `DU1-R1` 原报告中对 `aed9fb5` 的非代码结论（例如 `enum_coverage` 26 条 `cases` 的逐条内容）；本轮只确认它们未被本次修复改动。
4. 本报告不代替 Project Verify：PASS 只针对 `601c8ae` 的静态检视与门禁失败的复现已消除；`6.6` 合入前的候选 PV1（干净检出）仍须补齐。
5. 未更新任何任务状态，也未代替调度者宣称本变更可归档。

### 待补证据与门禁

| 待补项 | 是否影响本轮判断 | 应在哪个门禁补齐 |
| --- | --- | --- |
| 候选 `601c8ae` 的 PV1（`npm run verify`，干净检出） | 否（本轮已用只读方法覆盖 `check:docs` 这一关键子门禁） | `6.6` 合入前 |
| 候选自身的 PV4 日志（fmt/clippy/`cargo test --workspace`） | 否 | `6.6` 合入前 |
| 真实干净检出上的 `check:docs` 日志（替换标签不符的 `reports/clean-tree-check.log`，见 `DU1-R2-F4`） | 否（结论已由本轮实测取代） | `6.6` 合入前 |
| `verification.md` 的 DU1 轮记录回填（两条新前置的 Check Plan Changes 条目与错误码映射义务，见 `DU1-R2-F3`） | 否 | 本轮记录回填时 |

上一轮报告 `reports/rv1-du1.md` 及其结论保留；本轮不对 `aed9fb5` 的判定做修改，只声明其在 `601c8ae` 上的处置结果。
