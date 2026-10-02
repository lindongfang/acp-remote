# U1 集成基线报告（第三轮：并入 WP4）（role: merger / phase: integrate / stage: candidate）

> 本报告只覆盖本轮「集成（integrate）」动作：把已验收上游 **WP4** 并入 U1 的固定集成基线，供下游 **WP5 / WP6** 从该基线开工。
>
> **本单元尚未做的步骤（明确不在本轮结论范围内）**：未构造最终候选（候选须基于**最新** `refs/heads/main`；本轮 `main` 未移动，本基线只是**集成基线**，不是候选）、未跑 `workflow check --stage premerge`、未固化 `agentic-premerge` 证据块、未合入 `refs/heads/main`、**未 push**、未做最终验收。
>
> **WP5 不在本轮范围**（正在由另一实例在 `D:/Project/acp-remote-wt/session-resume-wp5` 修 CR5-F1），其分支**未被**合入。

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | **5.1**（tasks.md：主 Agent 单独创建独立合入 Agent 并显式交接 `roles/merger.md` 全文、源提交及证据、复用的执行 worktree、`refs/heads/main` 及合入条件）；本轮落点是其中的「把 WP4 纳入 U1 集成基线」部分 |
| role | merger（独立合入角色，**merger-A2 本轮实例**；非任何 WP 的实现者 / 测试者 / reviewer） |
| phase | **integrate** |
| stage | **candidate**（`roles/_shared/role-report.md` 表：merger 的 `integrate` 与 `candidate` 绑同一候选提交；本轮的 target 是**集成基线**提交） |
| agent_context | 独立 merger 子 Agent，**全新上下文**（未继承任何实现 / review / 前轮集成对话）。`PI_SESSION_ID=01a0f62e-02d0-73f1-ab95-0d366104980f`，父会话 `PI_SUBAGENT_PARENT_SESSION=01a0f628-df10-76d3-99e2-df76bf766c9d`，会话文件 `C:\Users\zhang\.pi\agent\sessions\--D--Project-acp-remote--\2026-10-01T06-31-08-816Z_01a0f628-df10-76d3-99e2-df76bf766c9d\6cd53f8e-8e32-4991-b4e1-4e8beb290373\run-0\session.jsonl`；工作目录 `D:/Project/acp-remote-wt/session-resume-du1` |
| target_revision | **`b486c5324238e4e7a60ab24806c920e3706b43da`**（本轮新增的 U1 集成基线固定提交，分支 `integration/session-resume-du1`；短 sha `b486c53`） |
| scope | `D:/Project/acp-remote-wt/session-resume-du1`（**复用**本单元已有执行 worktree；未新建、未切换分支）。只做 WP4 的本地合入 + 计划内检查。**未**改任何 `crates/**` 产品代码、**未**改 `plan.md` / `tasks.md` / `verification.md` / `AGENTS.md` |
| changes | 分支 `integration/session-resume-du1` 新增 1 个 `--no-ff` 合并提交 `b486c53`（parents `8a08db8e…` + `4a3882d…`），9 个文件 +567/−50。**零冲突**，merger 未手工编辑任何文件（`git diff --stat 4a3882d HEAD` 为空） |
| checks | **PV1 阶段 1 PASS**（fmt exit 0；`clippy -p storage-sqlite` exit 0；`test -p storage-sqlite` exit 0，**131 passed / 0 failed / 2 ignored**，与 `wp4-coder.md` 逐字一致）；**PV2 PASS**（`npm run check` exit 0，十道门禁全绿）；**PV1 阶段 2（workspace 全量）NOT_APPLICABLE**（诊断性执行：exit 101，红窗口**恰好 10 条唯一诊断** = `E0046`×9 + `E0004`×1，**较上一轮基线减少 1 条**，即 WP4 收口的 `storage-sqlite` 那一条） |
| issues | 1 项**计划内、已登记、有主（WP5 / WP6）、有界**的红窗口（workspace 全量仍红），归属未变、范围**收窄** 1 条。除已登记原因外**未发现第二个成因**。另有 1 项**证据缺口**（`app` 自身错误被上游遮蔽，与上一轮相同，本轮同样不可断言） |
| result | **PASS**（本轮 integrate 的全部适用条件满足） |
| evidence_paths | 本报告 + `reports/merge-u1-integrate-wp4-PV1-stage1.log`、`-PV2.log`、`-workspace-clippy.log`、`-workspace-clippy-keepegoing.log`、`-workspace-test.log`、`-green-set-test.log`（均在 `openspec/changes/session-resume/reports/`；`reports/**/*.log` 已被 `.gitignore:27` 排除） |
| resource_cleanup | worktree **保留**（本单元候选阶段复用）；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` **保留**（本单元独占、复用缓存，未删除）；未新建分支 / worktree；`git status --porcelain` 空、`git diff --cached` 空；`refs/heads/main` 未移动；**未 push**；未执行 `npm install/ci/update`，未删除 `node_modules` 联接；未触碰 `wt/session-resume-wp5` 及其 target 目录 |

## 环境（本轮实测）

| 项 | 值 |
| --- | --- |
| 工作目录 | `D:/Project/acp-remote-wt/session-resume-du1`（复用，未新建、未切换） |
| 分支 | `integration/session-resume-du1` |
| 合入前 HEAD（= 上一轮集成基线） | `8a08db8e77ac67efee317363ce241261af05c008` |
| 合入后 HEAD（本轮新集成基线） | **`b486c5324238e4e7a60ab24806c920e3706b43da`**（短 `b486c53`） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（由 `rust-toolchain.toml` 固定，与 CI 同一编译器）；全部 cargo 命令带 `--locked` |
| Node / npm | `v24.19.0` / `12.0.2`（`npm run check` 要求 Node ≥ 22.12） |
| CARGO_TARGET_DIR | `D:/Project/acp-remote-target/session-resume-du1`（**每条 cargo 命令显式导出**；本单元独占，复用上一轮缓存，**未删除**） |
| node_modules | worktree 内为目录联接（指向主工作区）。本轮**只读使用**；**未**执行 `npm install/ci/update`，**未**对联接执行任何删除操作 |

## 开工核实（task 5.1 的「机械核实目标」）

派发要求的开工前置核实**逐条实测通过**，未做任何 `reset` / 切分支：

| 核实项 | 期望 | 实测 | 判定 |
| --- | --- | --- | --- |
| `git rev-parse HEAD` | `8a08db8` | `8a08db8e77ac67efee317363ce241261af05c008` | ✅ 一致 |
| `git status --porcelain` | 空 | **空**（无输出） | ✅ 干净 |
| `git -C D:/Project/acp-remote rev-parse refs/heads/main` | `81e350f` | `81e350ff340014265eb7c9251237c799d4357fee` | ✅ 逐字一致 |
| 当前分支 | `integration/session-resume-du1` | `integration/session-resume-du1` | ✅ 未切换 |

## 固定版本与依赖包含关系（证据）

| 角色 | 引用 | 完整 SHA |
| --- | --- | --- |
| base（合入前集成分支 HEAD） | `integration/session-resume-du1` | `8a08db8e77ac67efee317363ce241261af05c008` |
| 源：**WP4（本轮并入）** | `agentic/session-resume-wp4` | `4a3882dfb3bf95d76340e35332aa9a8dc491a2d9`（短 `4a3882d`，author `lindongfang`，2026-10-01 00:15:39 +0800，`feat(storage): v5 migration 追加恢复列并实现 load_recovery 窄读取`） |
| WP4 的 base | WP4 分支起点 | `8a08db8e…`（= 合入前 HEAD，故无第三方前置合入） |
| **candidate / 集成基线（本轮）** | `integration/session-resume-du1` HEAD | **`b486c5324238e4e7a60ab24806c920e3706b43da`**（parents: `8a08db8e…`, `4a3882d…`） |
| 目标主分支（本轮**未**合入、未移动） | `refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee` |

### `merge-base --is-ancestor` 逐条实测（退出码 0 = 是祖先）

**合入前**（在新基线建立前，于 `8a08db8e…` 上实测，确认四源已含）：

```text
$ git merge-base --is-ancestor 81e350f HEAD;  echo $?   →  0   # 原 base / refs/heads/main
$ git merge-base --is-ancestor 248d9b9 HEAD;  echo $?   →  0   # WP1
$ git merge-base --is-ancestor 32f71f5 HEAD;  echo $?   →  0   # WP2
$ git merge-base --is-ancestor 4f7a2355 HEAD; echo $?   →  0   # WP3
```

**合入后**（于新基线 `b486c53…` 上复测，五条全部为祖先）：

| 交付提交 | 归属 | 命令退出码 | 结论 |
| --- | --- | --- | --- |
| `81e350ff340014265eb7c9251237c799d4357fee` | 原 base / `refs/heads/main` | `0` | 基线包含主分支起点 |
| `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb` | WP1 | `0` | 基线包含 WP1 交付提交 |
| `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992` | WP2 | `0` | 基线包含 WP2 交付提交 |
| `4f7a23554f76915cf5f29cc23e292ce270e014c9` | WP3 | `0` | 基线包含 WP3 交付提交 |
| `4a3882dfb3bf95d76340e35332aa9a8dc491a2d9` | **WP4（本轮并入）** | `0` | 基线包含 WP4 交付提交 |

```text
$ git merge-base HEAD 4a3882d
4a3882dfb3bf95d76340e35332aa9a8dc491a2d9     # = WP4 tip 本身 ⇒ 快进型合并（--no-ff 保留来源与父提交）
$ git diff --stat 4a3882d HEAD
（空）                                        # 合并结果树 = WP4 树，merger 无第三方改动
$ diff <(git diff --stat 8a08db8 HEAD) <(git diff --stat 4a3882d^ 4a3882d)
（无差异）                                     # 本轮合并引入的文件集与 WP4 交付集逐项相同
```

## 已验收上游证据（引用，未重跑）

| 证据 | 路径 | 结论 |
| --- | --- | --- |
| WP4 实现报告 | `openspec/changes/session-resume/reports/wp4-coder.md` | PV1（`-p storage-sqlite`）131 passed / 0 failed / 2 ignored；PV2 exit 0 |
| WP4 独立 review | `openspec/changes/session-resume/reports/cr4-review.md` | **CR4 Round 1 PASS**（target `4a3882d`）：0×CRITICAL、0×MAJOR、1×MINOR（CR4-F1）+ 3×SUGGESTION（F2 命名、F3 判别式断言、F4 日志头），均非阻断 |
| Handoff Index 2.1–2.8 行 | `openspec/changes/session-resume/verification.md:33` 起 | 2.1 WP1 / 2.2 WP2 / 2.3 WP3 / 2.4 WP4 / 2.7 TP1 / 3.1–3.4 评审行的索引 |

**CR4-F1 的闭环状态**：CR4 裁定「§11.3 的『过新』用例取值随 `FILE_FORMAT_VERSION=5` 由 5 改为 6」是**必要连带修正**并要求补登记。plan.md 已完成补登记（`plan.md:121` 的 CR4-F1 行、`plan.md:326` 的 WP4 写范围、`plan.md:358` 的 Shared File Ownership 该行均含「§11.3 的『过新』用例取值」）。本轮实测该行现为 `docs/CORE_PORTS_AND_STORAGE.md:1479` 附近的「`user_version = 6`，即 `FILE_FORMAT_VERSION + 1`」，与 `FILE_FORMAT_VERSION = 5` 自洽。CR4-F2/F3/F4 为 SUGGESTION，未在 WP4 修复，**不阻塞**本轮合入（merger 无权改 `crates/**`）。

## 合入过程与冲突解决

```text
$ git merge --no-ff --no-verify agentic/session-resume-wp4 \
    -m "chore(repo): 集成 WP4（storage-sqlite v5 恢复列与 load_recovery 窄读取）到 U1 集成基线"
Merge made by the 'ort' strategy.          # exit=0
 crates/storage-sqlite/src/migrate.rs               |  42 +-
 crates/storage-sqlite/src/session_store.rs         |  66 ++-
 crates/storage-sqlite/tests/commit.rs              |  14 +
 crates/storage-sqlite/tests/contract_v03.rs        |   2 +
 crates/storage-sqlite/tests/enum_coverage.rs       |   2 +
 crates/storage-sqlite/tests/migration.rs           | 454 ++++++++++++++++++++-
 crates/storage-sqlite/tests/retention.rs           |   4 +
 crates/storage-sqlite/tests/session_version_rule.rs|   6 +
 docs/CORE_PORTS_AND_STORAGE.md                     |  27 +-
 9 files changed, 567 insertions(+), 50 deletions(-)
```

- **冲突解决：无。** `merge-base(HEAD, WP4) == 4a3882d`（WP4 tip 本身），且 `4a3882d` 的父提交正是合入前的 `8a08db8e…` ⇒ **快进型合并**（用 `--no-ff` 保留来源父提交）。工作区无冲突标记，merger 未手工编辑任何文件。
- **写范围核对**（对照 plan.md `## Shared File Ownership`）：WP4 改 `crates/storage-sqlite/`（8 个文件）+ `docs/CORE_PORTS_AND_STORAGE.md`（1 个文件），共 9 个，与 CR4 §8 复核一致。
- **唯一「双写者」的合入顺序核对**：`docs/CORE_PORTS_AND_STORAGE.md` 的 Writers = WP3, WP4；Merge Owner = **WP4**；Order = **WP3 → WP4**。本轮基线已含 WP3，故 WP4 的文档改动在 WP3 之上顺序应用。按 hunk 逐条核对区域不相交：

  | WP4 触碰的 hunk 位置 | 所属章节 | plan 登记的 WP4 写范围 |
  | --- | --- | --- |
  | 文件头（`:21` 附近，新增「版本：0.15」条目） | 文件头版本记录 | ✅ 登记（DR1-F52 + 既有惯例） |
  | `## 7.` 标题（v4 → v5） | §7 标题 | ✅ 登记 |
  | §7.2（`:860`、`:874` 附近：版本常量 5/5/3、v4→v5 升级段、迁移测试资产） | §7.2 | ✅ 登记 |
  | §7.3 `owned_*` 表（`:910` 附近：`owned_session` 末尾两列） | §7.3 DDL | ✅ 登记 |
  | §9 判据 1 / 判据 28（`:1374` 附近：版本链 → v5、owned 列清单、旧行 NULL 与读回断言） | §9 判据 1/28 | ✅ 登记（DR1-F52） |
  | §11.3（`:1471` 附近：「过新」取值 5 → 6） | §11.3 | ✅ 登记（CR4-F1 补登） |
  | `## 8.`（`:1344` 附近） | 上下文行位移，无内容改动 | — |

  WP3 写的是 §2 / §3.1 / §3.3 / §3.6 / §4 / §5.1 / §5.2，与上表**无区域重叠**。**无需求 / 接口取舍需要上报主 Agent。**
- **`--no-verify` 的理由**（与 plan.md DR1-F23 一致）：`.husky/pre-commit` → `scripts/pre-commit.mjs` 在涉及 Rust 的提交上跑 **workspace 全量** `cargo clippy -D warnings`，在已登记红窗口内必然被拒。按 `AGENTS.md` §8，该钩子只是「更早发现失败」而非门禁本体，故此时可用 `--no-verify`。**判定责任未因此下移**：下列所有计划内检查都在固定基线 `b486c53…` 上重新手工执行并落盘日志。
- 合入后立刻 `git status --porcelain` → 空；全部检查跑完后再次核对 → 仍为空；`git diff --cached` → 空。
- 基线提交 SHA **未** amend / rebase；`refs/heads/main` 仍为 `81e350f…`；**未 push**（`origin/main` 仍为 `81e350f`）。

## Check 记录（逐条命令 / 退出码 / 环境）

所有 cargo 命令：`cwd = D:/Project/acp-remote-wt/session-resume-du1`，`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1`，`HEAD = b486c5324238e4e7a60ab24806c920e3706b43da`，`rustc/cargo 1.98.1`，全部带 `--locked`。

### C1 — PV1 阶段 1：`cargo fmt`（**PASS**，exit 0）

```text
$ cargo fmt --all -- --check
exit=0（无输出）
```

日志：`merge-u1-integrate-wp4-PV1-stage1.log` §`[1/3]`。

### C2 — PV1 阶段 1：`cargo clippy -p storage-sqlite`（**PASS**，exit 0）

```text
$ cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings
   Checking core v0.0.0
   Checking acpr-wire v0.0.0
   Checking storage-sqlite v0.0.0
    Finished `dev` profile ... in 5.39s
exit=0
```

日志：`merge-u1-integrate-wp4-PV1-stage1.log` §`[2/3]`。**这是本轮最关键的一条**：`--all-targets` 覆盖 lib + lib test + 6 个集成测试目标，即 WP4 因提交形状变化而修过的 14 处 `SessionUpdate` 字面量全部编译通过。

### C3 — PV1 阶段 1：`cargo test -p storage-sqlite`（**PASS**，exit 0）

```text
$ cargo test --locked -p storage-sqlite --all-features
exit=0   # 131 passed / 0 failed / 2 ignored
```

| 测试目标 | 结果 |
| --- | --- |
| `unittests src/lib.rs` | 3 passed |
| `tests/admin_audit.rs` | 15 passed |
| `tests/admin_store.rs` | 43 passed |
| `tests/attachments.rs` | 5 passed |
| `tests/commit.rs` | 18 passed / 1 ignored |
| `tests/compaction_recovery.rs` | 3 passed |
| `tests/contract_v03.rs` | 5 passed |
| `tests/enum_coverage.rs` | 2 passed |
| `tests/imported.rs` | 10 passed |
| `tests/migration.rs` | 12 passed / 1 ignored |
| `tests/permissions.rs` | 4 passed |
| `tests/retention.rs` | 8 passed |
| `tests/session_version_rule.rs` | 3 passed |
| `Doc-tests storage_sqlite` | 0 passed |
| **合计** | **131 passed / 0 failed / 2 ignored** |

与 `wp4-coder.md` 的 PV1 小节、`cr4-review.md` 的 `Assessment` 小节记录的 **131 passed / 0 failed / 2 ignored** **逐字一致**（ignored 为 `commit::crash_child` 与 `migration::regenerate_v1_fixture`，均为既有 `#[ignore]`）。`migration.rs` 12 passed 覆盖 CR4 记述的「新增 2 条、改写 1 条」。

日志：`merge-u1-integrate-wp4-PV1-stage1.log` §`[3/3]`。

### C4 — PV2：`npm run check`（**PASS**，exit 0）

```text
$ npm run check          # 仓库根（worktree 内），node v24.19.0 / npm 12.0.2
exit=0
```

十道合同门禁逐条 PASS（完整输出见 `merge-u1-integrate-wp4-PV2.log`）：

```text
schema fixtures OK: 120 valid, 26 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 13 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 159 fixture files, 12 transcript vectors re-encoded from input,
                    20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types,
                    19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 399 relative links, 6782 section refs across 403 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
                   §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
agentic gate: toolchain / openspec / config / schema / verification skill / AGENTS.md 路由 / manifest 全 PASS，
              openspec validate 19/19，宿主入口检查 17 个文件 PASS
```

**PV2 为何不受红窗口影响**（与 plan.md PV2 行一致，且本轮由实测确认）：`check:boundaries` 只看 `cargo metadata` 的依赖边，`check:contract-drift` 只读 §5/§7 的**文本**与 `ports.rs` / `migrate.rs` 的**文本**，其余为资产校验——**没有任何一道门禁编译 workspace**。

**`check:contract-drift` 是本 WP 最关键的机械判据**：它证明 `docs/CORE_PORTS_AND_STORAGE.md` §7 的 36 条 DDL（含 WP4 新增的两列）与 `crates/storage-sqlite/src/migrate.rs` 的 DDL 常量**逐条一致**，即权威文档与实现无漂移。

**跨包一致性交叉核对**（防「两包各自绿、合并后漂移」）：

| 指标 | WP4 分支（`wp4-coder-PV2.log`） | 本基线 `b486c53…`（本轮） | 判定 |
| --- | --- | --- | --- |
| `contract drift` §7 DDL | 36 条一致 | **36 条一致** | ✅ 一致（WP4 的 v5 两列已进文档且与实现一致） |
| `contract drift` §5 trait / method | 15 / **96** | 15 / **96** | ✅ 一致（WP4 未动端口签名，与 CR4 §3 一致） |
| `doc links` | 6782 section refs | **6782 section refs** | ✅ 一致（相对上一轮基线 6766 的 +16 由 WP4 文档行解释） |
| `command catalog` | 13 commands | 13 commands | ✅ 一致 |
| `schema fixtures` | 120 valid / 26 invalid | 120 valid / 26 invalid | ✅ 一致 |
| `contract assets` | 17 schemas / 159 fixture files | 17 schemas / 159 fixture files | ✅ 一致 |
| `crate boundaries` | 12 个 crate 一致 | 12 个 crate 一致 | ✅ 一致（WP4 无新增依赖） |

证据状态为 **NEW**（在 `b486c53…` 上实跑，非复用 WP4 分支的 PV2）。

### C5 — PV1 阶段 2：workspace 全量（**NOT_APPLICABLE**，诊断性执行）

plan.md 的 PV1 行把阶段 2 定义为「**全部 WP 集成后的**集成基线、候选、主分支」的检查。本轮基线只含 W1（WP1/WP2）+ W2（WP3）+ W3（WP4），**前置条件未满足**，故本轮不适用。以下执行为**诊断性执行**，唯一目的是取出红窗口的确切形态并确认没有第二个成因。**merger 未修任何一处红。**

```text
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
exit=101

$ cargo clippy --locked --workspace --all-targets --all-features --keep-going -- -D warnings
exit=101        # --keep-going 取完整错误集（默认行为在首个失败后跳过 dependents）

$ cargo test --locked --workspace --all-features
exit=101        # 0 个测试二进制被执行（编译阶段即中止）
```

日志：`merge-u1-integrate-wp4-workspace-clippy.log`（默认）、`-workspace-clippy-keepegoing.log`（完整集）、`-workspace-test.log`。

### C6 — 绿集合独立复验（**PASS**，exit 0；证明红窗口未向绿集合泄漏）

```text
$ cargo test --locked --workspace --all-features \
    --exclude server --exclude app --exclude agent-host
exit=0   # 556 passed / 0 failed（无任何 "test result: FAILED"）
```

排除的 3 个 crate 即本轮实测的全部红窗口 crate（`storage-sqlite` 本轮已转绿，故**不**再排除——这是与上一轮基线的重要差别）。逐 crate 计数：

| crate | passed |
| --- | --- |
| `core`（lib 单测） | **136**（与上一轮基线**逐字相同**，无回归） |
| `acp-protocol`（7 个目标合计） | 40 |
| `node-link-protocol`（7 个目标合计） | 40 |
| `identity-auth`（6 个目标合计） | 81 + 2 doc-test |
| `sync-protocol`（8 个目标合计） | 48 |
| `acpr-transcript`（3 个目标合计） | 24 |
| `acpr-wire`（2 个目标合计） | 20 |
| `identity-keystore`（5 个目标合计） | 36 |
| `storage-sqlite`（13 个目标合计） | **131**（本轮新增进入绿集合） |
| **合计** | **556 passed / 0 failed** |

对照上一轮基线 `8a08db8e…` 的同类执行（`--exclude` 4 个 crate，425 passed）：425 + 131（`storage-sqlite` 转入绿集合）= **556**，**逐位吻合**——即 WP4 的合入**只把 `storage-sqlite` 从红转绿，没有在其它任何 crate 引入新失败或吞掉用例**。

日志：`merge-u1-integrate-wp4-green-set-test.log`。

## 红窗口的确切错误清单（已知、有主、有界；merger **未修**）

### 成因（三类，全部为 plan.md 预先登记的义务）

1. **WP3 新增三个必需 trait 方法**（均无默认实现，刻意保留「每个后端都必须作答」的能力诚实边界，见 DR1-F14）：`SessionBackendFactory::resume`、`SessionEndpoint::agent_session_id`、`SessionStore::load_recovery`。
2. **WP2 的既有成因（未闭环）**：`node_link_protocol::command::CommandPayload` 的第 13 个变体 `SessionResume`（`crates/node-link-protocol/src/command.rs:740`）未覆盖 `crates/server/src/node_link/command.rs:2191` 的穷尽匹配。**收口方式已由 plan.md DR1-F48 裁定**：`resume_session` 完全镜像 `create_session`，不加通用分发臂，WP6 只需为 `SessionResume` **补一条与 `SessionCreate` 同形的早退臂**。
3. **WP6 的 `port_error_code` 追加臂**（`UnavailableKind::BackendUnsupported` → `nodelink.command.unsupported`）——登记在 `plan.md` 的红窗口段，但因 `server` 编译在 `E0004` 处中止而**尚未浮现为诊断**。

### 完整错误清单（`cargo clippy --workspace --all-targets --all-features --keep-going -- -D warnings`，exit=101）

**汇总**：rustc 共报出 **10 条唯一诊断**（`E0046` ×9 + `E0004` ×1），cargo 报 **4 个失败编译单元**，全仓库**没有任何其它错误码**（`grep -oE "^error\[E[0-9]+\]" | sort | uniq -c` → `1 error[E0004]`、`9 error[E0046]`；`grep "^error" | grep -vE "^error\[E|^error: could not compile|^error: (aborting|Some errors)"` → **空**）。

| # | crate / 编译单元 | 错误码 | 位置 | 缺失/未覆盖 | 登记归属 |
| --- | --- | --- | --- | --- | --- |
| 1 | `agent-host` (lib) | `E0046` | `crates/agent-host/src/host.rs:449` `impl SessionBackendFactory for AgentHost` | `resume` | WP5 |
| 2 | `agent-host` (lib) | `E0046` | `crates/agent-host/src/session.rs:746` `impl SessionEndpoint for Endpoint` | `agent_session_id` | WP5 |
| 3 | `server` (lib) | **`E0004`** | `crates/server/src/node_link/command.rs:2191:25` `match &submit.payload` | `CommandPayload::SessionResume(_)` | WP2 遗留 → **WP6** |
| 4 | `server` (lib test) | `E0046` | `crates/server/src/local_admin/test_support.rs:817` `impl SessionStore for NotTouched` | `load_recovery` | WP6 |
| 5 | `server` (lib test) | `E0046` | `crates/server/src/local_admin/test_support.rs:961` `impl SessionStore for FixedStore` | `load_recovery` | WP6 |
| 6 | `server` (lib test) | `E0046` | `crates/server/src/local_admin/test_support.rs:1021` `impl SessionBackendFactory for NotTouched` | `resume` | WP6 |
| 7 | `server` (lib test) | `E0046` | `crates/server/src/node_link/command/tests.rs:145` `impl SessionStore for CommandStore` | `load_recovery` | WP6 |
| 8 | `server` (lib test) | `E0046` | `crates/server/src/node_link/command/tests.rs:437` `impl SessionBackendFactory for FakeBackends` | `resume` | WP6 |
| 9 | `server` (lib test) | `E0046` | `crates/server/src/node_link/command/tests.rs:484` `impl SessionEndpoint for FakeEndpoint` | `agent_session_id` | WP6 |
| 10 | `server` (lib test) | `E0046` | `crates/server/src/node_link/resource/tests.rs:265` `impl SessionStore for SliceStore` | `load_recovery` | WP6 |

```text
error: could not compile `agent-host` (lib) due to 2 previous errors
error: could not compile `agent-host` (lib test) due to 2 previous errors
error: could not compile `server` (lib) due to 1 previous error
error: could not compile `server` (lib test) due to 8 previous errors
Some errors have detailed explanations: E0004, E0046.
```

失败编译单元的错误总数之和 = 13（= 2+2+1+8），高于 10 是因为 rustc 对同一诊断在 `lib` 与 `lib test` 两个编译单元间**去重**（`lib test` 的摘要计入但不再重复打印）。`cargo test` workspace 版本报出**完全相同**的 4 个失败编译单元（0 个测试二进制被执行）。

**`E0004` 的完整文本与上一轮基线 / `reports/wp2-coder-red-window.log` 逐字节一致**（`command.rs:2191:25`、同一变体、同一 `note` / `help` 段）——即 WP2 的成因**未变化、未被 WP3/WP4 放大**（已用 `diff` 机械验证，输出 `IDENTICAL`）。

### 与上一轮基线的红窗口对比：本轮**收窄** 1 条

| 诊断 | 上一轮基线 `8a08db8e…` | 本轮基线 `b486c53…` | 变化 |
| --- | --- | --- | --- |
| `E0046` 总数 | 10 | **9** | **−1** |
| `E0004` 总数 | 1 | 1 | 不变 |
| 唯一诊断合计 | 11 | **10** | **−1** |
| 失败编译单元 | 6（`agent-host`×2、`storage-sqlite`×2、`server`×2） | **4**（`agent-host`×2、`server`×2） | **−2** |
| `storage-sqlite` | ❌ `E0046 missing: load_recovery` @ `session_store.rs:1942` | ✅ **已转绿**（`clippy -p storage-sqlite` exit 0；`session_store.rs:1953` 的 `impl SessionStore for SqliteStore` 已含 `:2059` 的 `async fn load_recovery`） | ✅ **WP4 义务完成** |

`storage-sqlite` 在 workspace clippy / test 的日志中**只出现为 `Checking storage-sqlite`，无任何 error/warning**（`grep "could not compile .storage-sqlite"` → 0 行）。

### 逐 crate 的红窗口形态与「只有已登记原因」的断言

| crate | 实测 | 登记归属 | 是否恰好只有已登记原因 |
| --- | --- | --- | --- |
| `core` | **全绿**（136 passed，与上一轮逐字相同） | — | ✅ 无红窗口 |
| `acp-protocol` / `node-link-protocol` / `identity-auth` | **全绿**（40 / 40 / 83 passed） | — | ✅ 无红窗口 |
| `sync-protocol` / `acpr-transcript` / `acpr-wire` / `identity-keystore` | **全绿**（48 / 24 / 20 / 36 passed） | — | ✅ 无红窗口 |
| **`storage-sqlite`** | ✅ **全绿**（clippy exit 0；test 131 passed / 0 failed / 2 ignored） | WP4（**本轮已收口**） | ✅ **红窗口已消除** |
| `agent-host` | exit=101；lib 与 lib test 各**恰好 2 个**错误：`E0046 missing: resume`（`host.rs:449`）、`E0046 missing: agent_session_id`（`session.rs:746`） | WP5 | ✅ **只有**缺 `resume` / `agent_session_id`；无第二个错误码 |
| `server` | exit=101；lib **恰好 1 个**（`E0004`，`command.rs:2191`）；lib test **恰好 8 个**（同一 `E0004` + 7 个 `E0046` 于 7 个替身实现点） | WP2 遗留 + WP6 | ✅ **只有** `E0004` + 缺 `load_recovery`（4 个替身）/ `resume`（2 个替身）/ `agent_session_id`（1 个替身）；无第二个错误码 |
| `app` | exit=101，但**全部错误来自其依赖**（`agent-host` / `server` 两个 crate 的编译失败）；`app` 自身**从未被编译**（日志中 `Checking app` = 0 行、`could not compile \`app\`` = 0 行） | WP6 | ⚠️ **不可判定**：`app` 自身的 3 个替身实现点（`crates/app/tests/support/owner.rs:416` `FlakySessionStore`、`:504` `ScriptedBackends`、`:619` `ScriptedEndpoint`）的错误**被上游失败遮蔽**，本轮**无法**断言它们「恰好」是已登记原因。归属已登记为 WP6，但**证据待 WP5/WP6 收口后重跑** |

**说明**：`app` 的遮蔽**不是** WP4 造成的。`app` 的依赖为 `core` / `storage-sqlite` / `identity-auth` / `identity-keystore` / `agent-host` / `server`，其中 `agent-host` 与 `server` 仍红（WP5 / WP6 未合入），故 `app` 仍被遮蔽。WP4 使 `storage-sqlite` 转绿这一事实**减少**了 `app` 的潜在遮蔽面，但不解除 `agent-host` / `server` 造成的遮蔽。

### 「除已登记原因外没有第二个成因」的论证（六条，可机械复现）

1. **错误码封闭**：整轮 workspace 编译只有 `E0046` 与 `E0004` 两种错误码，`error[E…]` 行数为 10（9 + 1），不存在 `E0433` / `E0599` / `E0277` / `E0061` 之类的其它断裂；过滤后 `grep "^error"` 的剩余形态为**空**。
2. **与实现点清单一一对应**：全仓库 `impl SessionStore for` = **7** 处、`impl SessionBackendFactory for` = **5** 处、`impl SessionEndpoint for` = **4** 处，共 **16** 处。其中 `core` 的 3 处（`broker.rs:4733` / `:5648` / `:5707`）**已实现**全部新方法（`:4871` `load_recovery`、`:5679` `resume`、`:5712` `agent_session_id`），**零错误**；`storage-sqlite` 的 1 处（`session_store.rs:1953`）**本轮已实现** `load_recovery`（`:2059`），**零错误**。剩余 9 条 `E0046` 恰好落在 `agent-host` 2 处 + `server` 7 处，**没有第 10 处**。
3. **`server` 自身完整**：`server` 的依赖（`core` / `identity-auth` / 三个协议 crate）**全部在绿集合内**（实测 `crates/server/Cargo.toml` 无 `storage-sqlite` / `agent-host` 依赖），因此 `server` 的编译是**独立且完整**的（未被上游遮蔽）——它的错误集（1 + 8 = 9 条）就是它的全部错误集，且恰好覆盖 `core_payload` 的 `E0004` 与 `SessionStore` / `SessionBackendFactory` / `SessionEndpoint` 的 7 个替身实现点。
4. **`agent-host` 自身完整**：`agent-host` 的依赖为 `core` / `acp-protocol`（均在绿集合内），故其 2 条 `E0046` 即全部错误集。
5. **绿集合独立证明**：9 个非红窗口 crate 的测试合计 **556 passed / 0 failed**（C6），且与上一轮基线的 425 + 131 **逐位吻合**——红窗口既没有向绿集合泄漏「第二种成因」，也没有吞掉任何既有用例。`-p storage-sqlite` 的 clippy（`--all-targets`）+ test 双双 exit 0，独立证明 WP4 自身的编译与用例无问题。
6. **无告警型失败**：`-D warnings` 已把 warning 提升为 error；9 个绿 crate 与 `storage-sqlite` 的 clippy 全部 exit 0，说明**不存在**被降级隐藏的 lint 型第二成因。

**本轮唯一「证据缺口」**：`app` 自身错误被遮蔽（上表倒数第二行的 ⚠️）。这**不改变**本轮对 `storage-sqlite`（已转绿）、`agent-host`、`server` 的精确断言，但意味着「`app` 的失败恰好是已登记原因」这句话本轮**不能**说。该缺口与上一轮**同源**（`agent-host` / `server` 未合入），**不是** WP4 合并引入的。

## 未执行项与原因

| 未执行项 | 原因 |
| --- | --- |
| 最终候选构造（基于最新 `refs/heads/main`） | **不在本轮授权范围**。plan.md 要求候选基于**最新**主分支；本轮 `refs/heads/main` 仍为 `81e350f…`（未移动），故 `b486c53…` 只是**集成基线**，不是候选 |
| 独立 reviewer 对「集成新增交互 / 冲突差异」的检视 | 属候选阶段，由主 Agent 调度；本轮**零冲突、零手工改动**，集成差异 = WP4 交付集本身（已由 CR4 在 `work-package` 阶段检视过） |
| `workflow check --stage premerge` 与 `agentic-premerge` 证据块固化 | 属候选阶段，须待红窗口由 WP5 / WP6 收口、候选 Project Verify 与独立 review 通过后由主 Agent 执行 |
| 合入 `refs/heads/main` | **不在本轮授权范围**；`main` 必须停在 `81e350f` |
| `git push` | **不在本轮授权范围**；本角色流程只允许本地合入，不授权推送或发布 |
| `cargo-deny` / `gitleaks` | 按 `AGENTS.md` §8 只在 CI 运行，本地无等价物；**不得**据本报告宣称其通过 |
| E2E | plan.md 记 not-applicable |
| 修复 `crates/server/` / `crates/app/` 的红 | **超出本角色写范围**（属 WP6）；merger 未改任何 `crates/**` 文件 |
| 修复 CR4-F2 / F3 / F4 | 三条 SUGGESTION，非阻断；merger 无权改 `crates/**`（若主 Agent 要收口，应派回 WP4 实现者或并入 WP6） |
| 合入 WP5 | **不在本轮范围**（WP5 正在由另一实例修 CR5-F1）；其分支 `agentic/session-resume-wp5` **未被**合入 |

## 资源释放情况

| 资源 | 处置 |
| --- | --- |
| worktree `D:/Project/acp-remote-wt/session-resume-du1` | **保留**（本单元候选阶段复用；`git status --porcelain` 空、`git diff --cached` 空） |
| 分支 `integration/session-resume-du1` | **保留**在 `b486c53…` |
| `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` | **保留**（本单元独占、后续集成/候选验证复用；**未删除**） |
| `wt/session-resume-wp5` 及其 target 目录 | **未触碰**（只读核对了它的 HEAD `1376e1b…` 以确认另一实例仍在工作；未执行任何写操作） |
| `refs/heads/main` | **未移动**（`81e350ff…`）；`origin/main` 亦为 `81e350f` |
| 新建 worktree / 分支 | **无** |
| `git push` | **未执行** |
| `npm install / ci / update`、`node_modules` 联接删除 | **未执行**（PV2 只读使用该 worktree 的联接） |
| 日志文件 | 5 份写入 `openspec/changes/session-resume/reports/merge-u1-integrate-wp4-*.log`；已核实被 `.gitignore:27`（`openspec/changes/**/reports/**/*.log`）排除，**不进入版本控制** |

## 下游开工判据（WP5 / WP6）

二者声明的 `code:WP3` / `code:WP4` 前置已由本轮机械证实：

```text
$ git -C D:/Project/acp-remote-wt/session-resume-du1 merge-base --is-ancestor 4f7a2355 b486c53   # WP3  → exit 0
$ git -C D:/Project/acp-remote-wt/session-resume-du1 merge-base --is-ancestor 4a3882d b486c53   # WP4  → exit 0
```

- **WP5**（`agent-host`）：可从 `b486c53…` 开工。其红窗口仅 2 条 `E0046`（`host.rs:449` 缺 `resume`、`session.rs:746` 缺 `agent_session_id`），写范围（`crates/agent-host/`）不依赖红窗口 crate，可在其 crate 子集上产出 PV1 阶段 1 证据。
- **WP6**（`crates/server/` + `crates/app/`）：可从 `b486c53…` 开工。收口义务共 10 条诊断（`server` 的 1 条 `E0004` + 7 条 `E0046`）+ `app` 的 3 个被遮蔽替身 + `port_error_code` 的追加臂。

## 待澄清问题

**无阻塞性待澄清项。** 本轮全部适用条件已满足，无需主 Agent 决策的开放问题。

两条**供主 Agent 参考**的非阻塞观察（均不要求 merger 行动）：

1. **CR4-F2 / F3 / F4 三条 SUGGESTION 尚未收口**。F2（`migration.rs:359` / `:659` 两条用例名仍写「to v3」但实际断言 v5）与 F3（v3 / v4 升级用例对会变的 `owned_session` / `owned_node` 只断言 `contains(...)`，缺一条能区分「ALTER 追加」与「12-step 重建」的判别式断言）都是**判别力**问题——CR4 已裁定非阻断，但 F3 恰好关系到 spec R13/R16「MUST NOT 触发重建」这条性质的测试强度。建议主 Agent 决定：并入 WP6 收尾、单独派回 WP4 实现者、或接受为已知残余。
2. **`app` 的遮蔽已连续两轮未被取全**。上一轮 merger 报告已把它列为「唯一证据缺口」；本轮同样。建议在 WP5 与 WP6 各自合入后，由下一轮 merger（或 WP6 自己）重跑 `--keep-going` 的 workspace clippy，把 `app` 的 3 个替身诊断取全，从而闭合「`app` 的失败恰好是已登记原因」这条断言。

## handoff_index

```yaml
handoff_index:
  - task_id: "5.1"
    work_package: "WP4"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b486c5324238e4e7a60ab24806c920e3706b43da"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      交付物 = 新集成基线提交 b486c5324238e4e7a60ab24806c920e3706b43da（分支 integration/session-resume-du1），
      单个 --no-ff 合并提交（parents 8a08db8e77ac67efee317363ce241261af05c008 +
      4a3882dfb3bf95d76340e35332aa9a8dc491a2d9），快进型（merge-base(HEAD, WP4) == 4a3882d 本身）、
      零冲突、零手工改动（git diff --stat 4a3882d HEAD 为空；diff(8a08db8..HEAD) 与
      diff(4a3882d^..4a3882d) 的文件集逐项相同），9 文件 +567/-50 全部落在 WP4 写范围内
      （crates/storage-sqlite/ 8 个 + docs/CORE_PORTS_AND_STORAGE.md 1 个）。
      唯一双写者 docs/CORE_PORTS_AND_STORAGE.md 的 Merge Owner = WP4、Order = WP3 → WP4，
      本轮按该顺序顺序应用，逐 hunk 核对区域不相交（WP4 触碰文件头/§7 标题/§7.2/§7.3/§9 判据 1 与 28/§11.3，
      WP3 写 §2/§3.1/§3.3/§3.6/§4/§5.1/§5.2），无需上报的取舍。
      五条 --is-ancestor 实测全部 exit 0：81e350ff…（原 base / refs/heads/main）、
      248d9b9f…（WP1）、32f71f5d…（WP2）、4f7a2355…（WP3）、4a3882df…（WP4，本轮并入）。
      合入前后 refs/heads/main 均为 81e350ff340014265eb7c9251237c799d4357fee（未移动）、未 push、
      未 amend/rebase、git status --porcelain 空、git diff --cached 空。
      上游已验收证据：reports/wp4-coder.md（PV1 131 passed / PV2 exit 0）、
      reports/cr4-review.md（CR4 Round 1 PASS，0×CRITICAL / 0×MAJOR / 1×MINOR + 3×SUGGESTION，
      target_revision 4a3882d，引用未重跑）、verification.md Handoff Index 2.1–2.8 行。
      本轮未合入 refs/heads/main、未构造最终候选、未跑 premerge —— 属后续步骤。
    source_evidence: NOT_APPLICABLE
  - task_id: "5.1"
    work_package: "WP4"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b486c5324238e4e7a60ab24806c920e3706b43da"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      PV1 阶段 1（分支/包级形式：fmt + 本包 crate 子集）在集成基线 b486c53 上实跑：
      cargo fmt --all -- --check exit 0；
      cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings exit 0
      （--all-targets 覆盖 lib + lib test + 6 个集成测试目标，即 WP4 修过的 14 处 SessionUpdate 字面量全部编译通过）；
      cargo test --locked -p storage-sqlite --all-features exit 0，131 passed / 0 failed / 2 ignored
      （3 lib + 15 + 43 + 5 + 18 + 3 + 5 + 2 + 10 + 12 + 4 + 8 + 3；ignored 为 commit::crash_child 与
      migration::regenerate_v1_fixture，均为既有 #[ignore]），与 wp4-coder.md 及 cr4-review.md 的 Assessment
      报告的 131/0/2 逐字一致。
      环境：cwd=D:/Project/acp-remote-wt/session-resume-du1、
      CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1（本单元独占，复用缓存）、
      rustc/cargo 1.98.1（rust-toolchain.toml 固定）、全部带 --locked。
      日志：merge-u1-integrate-wp4-PV1-stage1.log。
    source_evidence: NOT_APPLICABLE
  - task_id: "5.1"
    work_package: "WP4"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b486c5324238e4e7a60ab24806c920e3706b43da"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      集成基线 b486c53 上实跑 npm run check（Node v24.19.0 / npm 12.0.2），exit=0，十道合同门禁全 PASS。
      关键判据 check:contract-drift 报「§7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
      §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致」——即 WP4 的 v5 两列在权威文档与实现之间
      无漂移。PV2 不受红窗口影响（无门禁编译 workspace：check:boundaries 用 cargo metadata、
      check:drift 读文本、其余为资产校验），故本轮 PASS 是「真绿」而非「被跳过」。
      跨包一致性核对（与 WP4 分支 wp4-coder-PV2.log 逐项一致，无合并漂移）：§7 36 条 DDL、§5 15 trait/96 methods、
      13 commands、120 valid/26 invalid fixtures、17 schemas/159 fixture files、12 个 crate 边界矩阵、
      doc links 6782 section refs（相对上一轮基线 6766 的 +16 由 WP4 文档行解释）。
      日志：merge-u1-integrate-wp4-PV2.log。
    source_evidence: NOT_APPLICABLE
  - task_id: "5.1"
    work_package: "WP4"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b486c5324238e4e7a60ab24806c920e3706b43da"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md"
    result: NOT_APPLICABLE
    evidence_status: NEW
    applicability_basis: >-
      PV1 阶段 2（workspace 全量）在 plan.md 中的适用前提是「全部 WP 集成后的集成基线、候选、主分支」；
      本轮基线只含 W1(WP1/WP2) + W2(WP3) + W3(WP4)，WP5/WP6 未合入，前提未满足，故本轮不适用。
      诊断性执行：cargo clippy --locked --workspace --all-targets --all-features [--keep-going] -- -D warnings
      exit 101；cargo test --locked --workspace --all-features exit 101（0 个测试二进制被执行）。
      红窗口共 10 条唯一诊断（E0046 ×9 + E0004 ×1），4 个失败编译单元：
      agent-host(host.rs:449 缺 resume、session.rs:746 缺 agent_session_id；lib 与 lib test 各 2)、
      server(命令行 E0004 command.rs:2191 未覆盖 CommandPayload::SessionResume，WP2 遗留且与
      wp2-coder-red-window.log 逐字节一致；lib test 7 条 E0046 分布于 test_support.rs:817/:961/:1021、
      node_link/command/tests.rs:145/:437/:484、node_link/resource/tests.rs:265)。
      较上一轮基线 8a08db8e 的 11 条收窄 1 条：storage-sqlite 的 E0046（session_store.rs 缺 load_recovery）
      已由 WP4 消除，storage-sqlite 本轮在 workspace 日志中只出现为「Checking」且零诊断。
      归属：agent-host → WP5；server（E0004 早退臂 + 7 个替身 + port_error_code 追加臂）→ WP6。
      除已登记原因外无第二个成因：错误码封闭（仅 E0046/E0000x 两类，grep 剩余形态为空）、
      16 处 trait 实现点与 9 条 E0046 一一对应（core 3 处与 storage-sqlite 1 处已实现全部新方法、零错误）、
      server 与 agent-host 的依赖全在绿集合内故其错误集完整、9 个绿 crate 合计 556 passed/0 failed
      （= 上一轮 425 + 本轮转入绿集合的 storage-sqlite 131，逐位吻合）、-D warnings 下无告警型第二成因。
      证据缺口（如实声明）：app 从未被编译（Checking app = 0 行），其 3 个替身（app/tests/support/owner.rs:416
      FlakySessionStore、:504 ScriptedBackends、:619 ScriptedEndpoint）的错误被 agent-host/server 的失败遮蔽，
      本轮不能断言「恰好是已登记原因」；该缺口与上一轮同源，非 WP4 引入。
      绿集合复验 cargo test --locked --workspace --all-features --exclude server --exclude app
      --exclude agent-host exit 0（556 passed / 0 failed）。
      日志：merge-u1-integrate-wp4-workspace-clippy.log、-keepegoing.log、-workspace-test.log、
      -green-set-test.log。不得据此判定 PV1 阶段 2 通过。
    source_evidence: NOT_APPLICABLE
```

## 结论

- **result: PASS**（本轮 integrate 范围：把已验收的 WP4 并入 U1 集成基线）。
- **新集成基线 SHA：`b486c5324238e4e7a60ab24806c920e3706b43da`**（短 **`b486c53`**；分支 `integration/session-resume-du1`；base/父 `8a08db8e…`；WP4 `4a3882d…` 已并入；WP1 `248d9b9f…`、WP2 `32f71f5d…`、WP3 `4f7a2355…`、`refs/heads/main` = `81e350ff…` 均为其祖先）。
- **检查退出码摘要**：`cargo fmt --all -- --check` = **0**；`cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` = **0**；`cargo test --locked -p storage-sqlite --all-features` = **0**（131 passed / 0 failed / 2 ignored）；`npm run check`（PV2）= **0**（十道门禁全绿）；绿集合 `cargo test --workspace --all-features --exclude server --exclude app --exclude agent-host` = **0**（556 passed）；PV1 阶段 2 workspace `clippy`（含 `--keep-going`）= **101**（预期红）、workspace `test` = **101**（预期红）。
- **红窗口诊断条数**：**10 条唯一诊断**（`E0046` ×9 + `E0004` ×1），4 个失败编译单元。较上一轮基线的 11 条**收窄 1 条**（`storage-sqlite` 转绿，WP4 义务完成）。归属：`agent-host` 2 条 → **WP5**；`server` 8 条（`E0004` 早退臂 + 7 个替身）→ **WP6**；`app` 3 个替身被遮蔽 → **WP6**（待补证据）。**merger 未修任何一处红**（属 WP5 / WP6 写范围）。
- **本单元尚未做**：最终候选构造、`workflow check --stage premerge`、合入 `refs/heads/main`、`git push`、最终验收 —— 均为后续步骤，不在本轮授权内。
