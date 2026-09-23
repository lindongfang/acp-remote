# DU1 集成报告（task 6.2）

## 固定字段

| 字段 | 值 |
| --- | --- |
| `task_id` | `6.2`（交付单元 DU1，模式 `integrated`） |
| `role` | 独立集成 Agent（不兼任实现者 / 测试 Agent / reviewer） |
| `phase` | Merge Unit：① 候选固定与包含关系取证（6.2 前半）② 候选完整工作区构建（6.2 后半）③ 合入阶段（本轮无授权，未执行） |
| `agent_context` | 子 Agent `Du1Integrator`（独立子 Agent，非主 Agent 兼任）。**上下文继承方式：不继承**实现者/测试/reviewer 的任何对话，仅接收主 Agent 显式交接的 `openspec/schemas/agentic/roles/integrator.md` 全文、计划 `local://admin-persistence-v2-review-fixes-plan.md`、变更目录 `openspec/changes/admin-state-persistence-v2/`、合同判据与固定候选/基线引用。工作目录 `D:\Project\acp-remote`（主 worktree） |
| `target_revision` | 候选（固定，未改写）：`aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`；目标主分支基线：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d` |
| `scope` | 仅允许写 `reports/du1-integrator.md` 与自身隔离构建目录 `target/du1-integrator/**`。未写任何产品代码、测试、夹具、规划文件（`plan.md`/`tasks.md`/`verification.md`）或其它执行者的报告 |
| `changes` | ① 新建 `reports/du1-integrator.md`（本文件）② `target/du1-integrator/` 下的 cargo 编译产物与构建日志 `du1-integrator-build.log`。仓库跟踪文件**零改动**；无新增 worktree、无分支创建、无合并 |
| `checks` | 见「检查记录」表（含完整命令、退出码、日志路径） |
| `issues` | ① 合入缺少授权（见「合入阶段结论」）② 工作区存在主 Agent 的未提交改动 `M openspec/changes/admin-state-persistence-v2/tasks.md`（6.x 执行记录回填），与本单元候选无冲突、未触碰 |
| `result` | **候选构造阶段 = PASS**；**合入阶段 = BLOCKED（缺合并授权，且 6.3/6.4/6.5 前置证据未齐）**。候选 PASS ≠ 已合入 ≠ 最终验收 PASS |
| `evidence_paths` | `reports/du1-integrator.md`、`target/du1-integrator/du1-integrator-build.log`（sha256 `55b23cc264a6b1c229caf7163574d849649a7fb0b118feff6820c61036a254ab`） |
| `resource_cleanup` | 无新建 worktree、无临时 SQLite 目录、未碰共享 `target/`。自建隔离构建目录 `target/du1-integrator/` 保留日志后清理编译产物（见文末） |

## 1. 交付单元核对

- **模式**：`integrated`（`plan.md` §Merge Strategy 第 665 行：`DU1 / integrated | WP1–WP6 | … | PV1、PV2、PV3、PV4；无关键 E2E（mode = not-applicable）| 唯一单元，先候选后合入`）。单一交付单元，无并行分片。
- **WP 组成**：WP1（core 值对象/依赖面）、WP2（ports 目标签名）、WP3（use_cases 写集与 workspace 解析）、WP4（SQLite v2 迁移与夹具）、WP5（合同并入与依赖边界）、WP6（三个管理 store）。
- **本单元组成提交**（`aed9fb5` 的 6 个提交；其余 3 个是分支继承的文档提交，不属本单元）：

| # | 提交 | 主题 | 归属 |
| --- | --- | --- | --- |
| 1 | `013f2b9` | `feat(core)!: 落地管理状态端口与 SQLite v2 存储合同` | 本单元（W0 基线：core 管理端口 + v2 合同/DDL/迁移） |
| 2 | `1baea5b` | `feat(storage): 实现管理状态的三个 SQLite store 与失败关闭` | 本单元（WP6） |
| 3 | `f43f7a7` | `feat(core): 配对记录携带本机绑定并落地节点配对的批准路径` | 本单元 |
| 4 | `5404610` | `feat(storage): 已撤销身份只能经协议重新配对恢复` | 本单元 |
| 5 | `86ae8b4` | `fix(core): 闭合 W1/W2 独立 review 的阻断项、依赖面与判据缺口` | 本单元（W1/W2 review 闭合） |
| 6 | `aed9fb5` | `docs(repo): 回填 W1/W2 闭合轮的交付提交与 Merge History` | 本单元（记录回填） |

分支继承、**不属**本单元：`5d77f25`、`7cdff57`、`28f8cb9`（文档提交）。

## 2. 依赖包含关系取证（只读命令与原始输出）

### 2.1 基线是候选的祖先（可直接快进，无分叉）

```text
$ git rev-parse --abbrev-ref HEAD
feat/admin-state-persistence-v2
$ git rev-parse HEAD
aed9fb5fc7c244883a12e08fe4c9c5b6540f6822
$ git rev-parse refs/heads/main
37a398e9dbafa368bdb15853e1c1d9b40f19b28d
$ git merge-base --is-ancestor 37a398e9dbafa368bdb15853e1c1d9b40f19b28d aed9fb5fc7c244883a12e08fe4c9c5b6540f6822
ANCESTOR_EXIT=0                     # 0 = main 是候选的祖先
$ git merge-base aed9fb5fc7c244883a12e08fe4c9c5b6540f6822 refs/heads/main
37a398e9dbafa368bdb15853e1c1d9b40f19b28d   # 唯一合并基点即 main 自身
$ git rev-parse aed9fb5fc7c244883a12e08fe4c9c5b6540f6822~1
86ae8b4c572d699e69fd6ccf21978c2496954cb8
$ git rev-list --count 37a398e9dbafa368bdb15853e1c1d9b40f19b28d..aed9fb5fc7c244883a12e08fe4c9c5b6540f6822
9                                   # 6 个单元提交 + 3 个分支继承文档提交
$ git log --merges --oneline 37a398e9dbafa368bdb15853e1c1d9b40f19b28d..aed9fb5fc7c244883a12e08fe4c9c5b6540f6822
                                    # 空：分支上无合并提交，历史线性
$ git worktree list
D:/Project/acp-remote aed9fb5 [feat/admin-state-persistence-v2]
                                    # 唯一 worktree，即主 worktree；未新建
$ git branch --contains 37a398e9dbafa368bdb15853e1c1d9b40f19b28d
  docs/pre-implementation-contracts
* feat/admin-state-persistence-v2
  main
```

结论：候选 `aed9fb5` 线性包含 `refs/heads/main`（`37a398e`）；`013f2b9` 亦为候选祖先（`git merge-base --is-ancestor 013f2b9 aed9fb5` 退出 0）。因此**无需构造合并提交**，集成基线 = 候选自身。

### 2.2 单元要素包含性（在候选 rev 上取证）

```text
$ git diff --name-status 013f2b9 aed9fb5fc7c244883a12e08fe4c9c5b6540f6822
A  crates/storage-sqlite/src/admin/export.rs
A  crates/storage-sqlite/src/admin/local_config.rs
A  crates/storage-sqlite/src/admin/mod.rs
A  crates/storage-sqlite/src/admin/trust.rs
M  crates/core/src/ports.rs   （经 crates/core 目录）
M  crates/storage-sqlite/src/migrate.rs   （经 crates/storage-sqlite 目录）
…

$ git ls-tree -r --name-only aed9fb5 crates/storage-sqlite/src/admin/
crates/storage-sqlite/src/admin/export.rs
crates/storage-sqlite/src/admin/local_config.rs
crates/storage-sqlite/src/admin/mod.rs
crates/storage-sqlite/src/admin/trust.rs

$ git grep -n -E 'pub struct (WriteContext|PendingAudit|DeviceWrite|NodeWrite|DeviceRevocation|NodeRevocation|PairingWrite|PairingClaimWrite|PairingSettlementWrite|ExpiryWrite|ExportWrite|ExportRevocation|ImportWrite|ImportRemoval|ProfileWrite|WorkspaceWrite|ProviderRefWrite|SeedWrite)' aed9fb5 -- crates/core/src/ports.rs
ports.rs:597:pub struct PendingAudit {
ports.rs:612:pub struct WriteContext {
ports.rs:619:pub struct DeviceWrite {
ports.rs:626:pub struct NodeWrite {
ports.rs:634:pub struct DeviceRevocation {
ports.rs:642:pub struct NodeRevocation {
ports.rs:650:pub struct PairingWrite {
ports.rs:658:pub struct PairingClaimWrite {
ports.rs:666:pub struct PairingSettlementWrite {
ports.rs:674:pub struct ExpiryWrite {
ports.rs:680:pub struct ExportWrite {
ports.rs:687:pub struct ExportRevocation {
ports.rs:698:pub struct ImportWrite {
ports.rs:707:pub struct ImportRemoval {
ports.rs:715:pub struct ProfileWrite {
ports.rs:722:pub struct WorkspaceWrite {
ports.rs:729:pub struct ProviderRefWrite {
ports.rs:736:pub struct SeedWrite {
                                    # 18 个写集 DTO 全部存在
$ git grep -n -E 'pub trait (TrustStore|ExportStore|LocalConfigStore|CredentialResolver)' aed9fb5 -- crates/core/src/ports.rs
ports.rs:743:pub trait TrustStore: Send + Sync {
ports.rs:789:pub trait ExportStore: Send + Sync {
ports.rs:812:pub trait LocalConfigStore: Send + Sync {
ports.rs:841:pub trait CredentialResolver: Send + Sync {
$ git grep -n -E 'fn (node|nodes_for|peer_key|pairing_peer|put_export|revoke_export|add_import|remove_import|resolve_env)' aed9fb5 -- crates/core/src/ports.rs
ports.rs:749:    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError>;
ports.rs:754:    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError>;
ports.rs:757:    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError>;
ports.rs:762:    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError>;
ports.rs:798:    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError>;
ports.rs:800:    async fn revoke_export(&self, write: ExportRevocation) -> Result<(), PortError>;
ports.rs:802:    async fn add_import(&self, write: ImportWrite) -> Result<(), PortError>;
ports.rs:807:    async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError>;
ports.rs:845:    async fn resolve_env(

$ git grep -n -E 'const (FILE_FORMAT_VERSION|OWNED_SCHEMA_VERSION|IMPORTED_SCHEMA_VERSION)' aed9fb5 -- crates/storage-sqlite/src/migrate.rs
migrate.rs:18:pub const FILE_FORMAT_VERSION: i64 = 2;
migrate.rs:20:pub const OWNED_SCHEMA_VERSION: i64 = 2;
migrate.rs:22:pub const IMPORTED_SCHEMA_VERSION: i64 = 2;
$ git grep -n -E 'imported_import_export|V2_UPGRADE|BEGIN IMMEDIATE' aed9fb5 -- crates/storage-sqlite/src/migrate.rs
migrate.rs:424:CREATE TABLE IF NOT EXISTS imported_import_export (
migrate.rs:434:/// §7.2 的 v1 → v2 升级：`owned_audit` 的 12-step 表重建（§11.8 第 7 条）。
migrate.rs:443:const V2_UPGRADE_OWNED: &str = r#"
migrate.rs:483:const V2_UPGRADE_IMPORTED: &str = r#"
migrate.rs:513:CREATE TABLE imported_import_export_pending AS
migrate.rs:536:INSERT INTO imported_import_export (import_id, owner_node_id, export_id, added_at)
migrate.rs:848:    let mut tx = write.begin_with("BEGIN IMMEDIATE").await?;
migrate.rs:885:        sqlx::raw_sql(V2_UPGRADE_OWNED).execute(&mut *tx).await?;
migrate.rs:886:        sqlx::raw_sql(V2_UPGRADE_IMPORTED).execute(&mut *tx).await?;
$ git grep -n 'admin' aed9fb5 -- crates/storage-sqlite/src/lib.rs
lib.rs:12:pub mod admin;

# W1/W2 阻断项闭合痕迹（候选内可见）
$ git grep -n -E 'NodePaired|settle_pairing' aed9fb5 -- crates/core/src/use_cases.rs
use_cases.rs:591:    pub async fn settle_pairing(
use_cases.rs:609:            (true, PairingTarget::Node) => AuditAction::NodePaired,
use_cases.rs:1461:        block_on(fixture.use_cases.settle_pairing(     # 回归用例
use_cases.rs:1483:                AuditAction::NodePaired,
```

单元 diff 规模（`git diff --shortstat 013f2b9 aed9fb5…`）：`41 files changed, 9436 insertions(+), 178 deletions(-)`（含夹具二进制与报告文件）。核心新增为三个管理 store（`trust.rs` +1104、`export.rs` +508、`local_config.rs` +421、`mod.rs` +194）与 `tests/admin_store.rs` +3113。

### 2.3 基线复核与防竞态

- 基线在取证与构建之间未变化：`git rev-parse refs/heads/main` 在取证时、构建后、清理后三次调用均为 `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`；同期 `git rev-parse HEAD` 恒为候选 `aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`，`git status --porcelain --untracked-files=no` 恒为单行 `M openspec/changes/admin-state-persistence-v2/tasks.md`。
- 因为 `main` 是候选的祖先且候选即当前 `HEAD`（工作区唯一改动是主 Agent 的 `tasks.md` 记录回填，不涉候选树），本轮不存在基线漂移 → **无需重建候选**。
- 防竞态机制：本轮不发生任何目标分支更新，故无需条件更新；若后续授权合入，因是快进关系，合入前须再次 `git rev-parse refs/heads/main` 比对 `37a398e…`，不等则本报告证据作废、重建候选。

## 3. 候选固定与构建

- **候选固定**：`aed9fb5fc7c244883a12e08fe4c9c5b6540f6822`，分支 `feat/admin-state-persistence-v2`，主 worktree `D:/Project/acp-remote`。未重建分支、未改写历史、**未新建 worktree**（符合 `plan.md` §Merge Strategy「Integration Branch / Worktree: `feat/admin-state-persistence-v2`（主 worktree…）」）。
- **集成分支决定**：沿用当前分支与主 worktree（`plan.md` 已定），本报告不产生任何 worktree 或分支。
- **构建**（隔离 `CARGO_TARGET_DIR`，独占，未触碰共享 `target/`）：

```text
$ set CARGO_TARGET_DIR=D:\Project\acp-remote\target\du1-integrator&& cargo build --locked --workspace --all-features 2>&1
   Compiling acpr-transcript v0.0.0 (D:\Project\acp-remote\crates\acpr-transcript)
   Compiling core v0.0.0 (D:\Project\acp-remote\crates\core)
   …
   Compiling sqlx-sqlite v0.8.6
   Compiling sqlx v0.8.6
   Compiling storage-sqlite v0.0.0 (D:\Project\acp-remote\crates\storage-sqlite)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.70s
退出码: 0       耗时: 17s（新目标目录冷构建）       输出: 140 行，无 warning、无 error
```

- 完整构建输出合并保存于 `target/du1-integrator/du1-integrator-build.log`（sha256 `55b23cc264a6b1c229caf7163574d849649a7fb0b118feff6820c61036a254ab`，4909 字节，含命令、退出码、候选 rev 头注）。

## 4. 检查记录

| Check ID | 完整命令 | 退出码 | 结论 | 日志 / 证据 |
| --- | --- | --- | --- | --- |
| DU1-C1（包含关系） | `git merge-base --is-ancestor 37a398e… aed9fb5…`、`git merge-base aed9fb5… refs/heads/main`、`git rev-list --count 37a398e…..aed9fb5…`、`git log --merges 37a398e…..aed9fb5…` | 0 | PASS（main 为祖先；9 提交 / 6 为本单元；无合并提交） | 本文件 §2.1 |
| DU1-C2（要素包含） | `git diff --name-status 013f2b9 aed9fb5…`、`git ls-tree …src/admin/`、`git grep … ports.rs / migrate.rs / lib.rs` | 0 | PASS（18 写集 DTO、4 个端口 trait、3 版本常量 = 2、v1→v2 升级语句、4 个 admin 模块文件均在） | 本文件 §2.2 |
| DU1-C3（候选构建，Project Verify 的前置构建门） | `set CARGO_TARGET_DIR=D:\Project\acp-remote\target\du1-integrator&& cargo build --locked --workspace --all-features 2>&1` | 0 | PASS | `target/du1-integrator/du1-integrator-build.log` |
| DU1-C4（工作区状态） | `git status --porcelain`、`git worktree list` | 0 | PASS（跟踪文件仅 `M openspec/changes/…/tasks.md`（主 Agent 记录回填）＋宿主未跟踪项 `.omp/`、`.pi/**`、`reports/parked-idle-identity-retention.patch`；未新建 worktree） | 本文件 §5、§6 |
| DU1-C5（PV1） | `npm run verify` | — | **未执行（非本任务）** | `6.3` 由独立检查执行者负责 → `reports/du1-pv1.log`（本报告落盘时该文件已存在于 `reports/`，由对等执行者生成，内容未由本 Agent 检视） |
| DU1-C6（候选独立 review） | — | — | **未执行（非本任务）** | `6.4` 由独立 reviewer 负责 → `reports/rv1-du1.md`（截至本报告落盘时该文件**不存在**） |

## 5. 冲突解决

**无冲突需要解决，且本轮未发生任何合并。** 依据：`git log --merges --oneline 37a398e…..aed9fb5…` 输出为空（分支历史线性、无合并提交）；`git merge-base` 唯一基点为 `37a398e`（即目标主分支自身），候选对 `main` 为快进关系，因此不存在需要人工裁决的 diff3 冲突、无「集成冲突解决差异」可交 reviewer。构建/测试中的任何失败均不得由我在本任务内改动产品代码，遇此应回退交付给实现者（本轮未出现此类失败）。

## 6. 候选阶段结论：PASS

依据：候选固定且含本单元全部要素（§2.2）；基线包含关系成立且基线未漂移（§2.1、§2.3）；候选完整工作区 `--locked --all-features` 构建退出 0（§3）；未改动任何产品/规划/他人报告文件；无未解决冲突。

**边界声明**：本 PASS 仅覆盖「候选固定 + 包含关系取证 + 候选构建」这一子集。`6.3`（`npm run verify` / PV1）与 `6.4`（候选独立 review）不属于本次交办，其证据到位前不得视为候选 PV 通过。

## 7. 合入阶段结论：BLOCKED

**缺失项**：

1. **合并授权缺失**——主要阻断。`plan.md` §Merge Strategy 明确「Authorization / Report Path：合并与推送授权仍受当前会话限制」，本轮交办亦声明「任何合并与推送没有授权」。故不做 `merge` / `push` / PR，保留候选与全部证据。
2. **前置证据未齐**：`6.3` 的 PV1 退出码 0（`reports/du1-pv1.log`）与 `6.4` 的独立 review 无未解决阻断项（`reports/rv1-du1.md`，当前缺失）；`6.5` 的 E2E `not-applicable` 理由核对结论（`reports/du1-integration.md` 的 E2E 段）由主 Agent 出具。
3. **防竞态复核未做**：合入前必须再次核对 `refs/heads/main` 仍等于 `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`；若变化则本报告全部候选证据作废并重建候选。

**下一步**：主 Agent 取得合并授权并收齐 6.3/6.4/6.5 证据后，按条件更新串行合入（因快进关系，合入后 HEAD 预期等于候选 `aed9fb5…`），再交 `6.7`/`6.8`。集成侧无未闭环缺陷需要修复。

## 8. 授权依据

- 执行授权范围：只读取证 + 隔离构建 + 写本报告；未获合并、推送、回滚、发布授权。
- 未执行项（如实记录）：`npm run verify`（属 6.3）、E2E（本变更 `not-applicable`）、`cargo-deny`/`gitleaks`（本地无等价物，仅 CI）、任何 git 写操作。

## 9. 证据有效性与复用

- 本报告证据绑定候选 `aed9fb5…` 与基线 `37a398e…`；两者任一变化即整体失效。
- 可复用于 `6.3`/`6.4` 的输入：候选提交引用、包含关系输出、构建命令与退出码（`6.3` 仍须自行跑 `npm run verify`，不以本报告替代）。
- 复用的上游证据（非本人产生）：`reports/rv1-wp{1,23,4,5,6,6b}.md`、`reports/verify-w1w2-closure.log`、`reports/wp6-admin-store-tests.log` —— 仅引用路径与结论，不复制其私有内容。

## 10. 资源清理

- 未创建 worktree、临时 SQLite 目录或后台进程。
- 隔离构建目录 `target/du1-integrator/`（865 MB）：保留构建日志 `du1-integrator-build.log`，其余编译产物（`debug/`、`.rustc_info.json`、`CACHEDIR.TAG`）在本报告落盘后删除，释放约 865 MB；共享 `target/` 全程未被本任务写入。
- 未触碰 `.omp/`、`.pi/**`、`reports/parked-idle-identity-retention.patch`、主 Agent 的 `tasks.md` 未提交改动。
