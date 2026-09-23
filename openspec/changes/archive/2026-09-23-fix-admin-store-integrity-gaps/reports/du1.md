All phases verified. Final report:

---

# DU1 集成/合并报告（isolated integrator）

## 固定字段

| 字段 | 值 |
|---|---|
| `task_id` | DU1 / `fix-admin-store-integrity-gaps` |
| `role` | integrator（独立集成/合并子 Agent，非实现者、非 reviewer、非 tester） |
| `phase` | A 候选构造（5.2）→ B 合入（5.6）→ C 主分支验证（5.7），全部执行完毕 |
| `agent_context` | 隔离上下文（新会话，未参与 WP1/WP2/WP3 实现与 RV1/RV2 检视；宿主未提供 Agent ID 字符串，本报告以「DU1-integrator (isolated)」标识） |
| `target_revision` | 本地 `refs/heads/main`：`1ef6640431079f9ca22b409c42071de2d9a79fa2` → `78e1db1b432289a5d67f1d7498704884d9cc7e44` |
| `scope` | 集成候选的构造、候选 Project Verify、本地 fast-forward 合入、主分支 Project Verify；**不含** push / PR / tag / 发布 / 归档 |
| `changes` | 3 个本地提交：`c7c7ade`（产品+文档+变更目录）、`1af5af4`（候选验证证据）、`78e1db1`（主分支验证证据）；产品代码 6 文件 +556/−69，与已检视修订逐字节一致 |
| `checks` | A1/A3 现场核对、A4 `candidate-pv`/`candidate-lc1`、B5 防竞态复核、B6 ff-only 合入、C7 `main-pv`/`main-lc1`：全部 exit 0、无 `FAILED`、无 `error[`（详见下表） |
| `issues` | 1 项已升级并获决定（`*.log` 被忽略）；1 项操作插曲（并发 git 调用导致 `index.lock`，已自然释放，未用 `--no-verify`）；2 项仅 CI 可执行的检查未在本地运行；1 处对 supervisor 理由的事实更正 |
| `result` | **A PASS / B PASS / C PASS** — 已本地合入 `main`，未推送 |
| `evidence_paths` | `openspec/changes/fix-admin-store-integrity-gaps/reports/`（`candidate-pv.log`、`candidate-lc1.log`、`main-pv.log`、`main-lc1.log`、`branch-diff.patch`、`round1/`、`rv1-fix.md`、`rv2-fix-recheck.md`），全部已入库 |
| `resource_cleanup` | `/tmp/du1_commit_msg.txt`（仓库外临时文件，可弃）；本地分支 `fix/admin-store-integrity-gaps` @ `1af5af4` **保留**（见残留风险 4）；无 worktree 新增；无 `index.lock` 残留 |

## 集成细节

- **分支/worktree**：`D:/Project/acp-remote`（唯一 worktree），临时集成分支 `fix/admin-store-integrity-gaps`（从基线切出），目标分支 `main`。
- **提交链（base → main）**：`1ef6640` → `c7c7ade` → `1af5af4` → `78e1db1`（`git rev-list --count base..HEAD` = 3）。
- **依赖包含关系**：DU1 为 integrated 唯一单元，WP1/WP2/WP3 三条轨道已合入同一候选；合入为 `--ff-only`，**无 merge commit、无冲突、无冲突解决差异**（预期无冲突，实测一致）。
- **候选内容标识复核（关键）**：`git diff 1ef6640 HEAD -- <6 个产品/文档文件> | sha256sum` = `945d1d509b428a8b84e8d2168d980600098a14d57086d8bd2e4c6957b21e6170`，与交付输入给的 `tracked_diff_sha256` 逐字节相同（合入前后均相同）；`reports/branch-diff.patch` sha256 = `da7cbcdba0ae4c7cda0e9bc0c36e6645e85bf389ae02c20f84326f06337576bd`。⇒ **RV2 的 PASS 与 PV1–PV5 证据对本候选继续适用**（产品修订零漂移）。
- **防竞态**：A1 开工前与 B5 合入前各核对一次 `git rev-parse refs/heads/main`，两次均为 `1ef6640…`，未发生基线变化。
- **授权依据**：用户 2026-09-23 本会话回应「(b) 现在就授权本地合入 `main`」，原话「A同意，B a」与「A同意，B  b」，按更晚的更正记为选 **(b)**；据此仅执行本地合入。执行中因 `*.log` 忽略规则向 supervisor 请求决定，获回复 **(a) 强制入库证据日志，不改 `.gitignore`、不跳过第 2/第 3 个提交**，已按此执行。

### 检查表（ID / 命令 / 退出码 / 日志）

| ID | 完整命令 | 退出码 | 日志/证据 |
|---|---|---|---|
| A1 现场核对 | `git rev-parse refs/heads/main`；`git status --porcelain`；`git diff \| sha256sum` | 0 | base 相符；仅 6 个 `M` + `?? openspec/changes/fix-admin-store-integrity-gaps/`；sha256 = `945d1d50…` ✓ |
| A2 候选提交 | `git switch -c fix/admin-store-integrity-gaps`；`git add -A`；`git commit -F …`（husky 钩子真实执行） | 0 | `c7c7ade`，20 文件 +3330/−69；pre-commit 3 步（gitleaks 未安装已跳过 / `cargo fmt --check` / `npm run check` / `cargo clippy -D warnings`）+ commit-msg commitlint 均通过；**未使用 `--no-verify`** |
| A4 candidate-pv | `npm run verify` | **0** | `reports/candidate-pv.log`（38 个 `test result: ok`，0 `FAILED`，0 `error[`，含 `export_revocation_is_terminal_for_plain_writes`、`timestamps_are_advanced_never_erased`、`receipt_without_imported_session_is_rejected`、`late_callbacks_after_a_full_removal_cannot_rebuild_the_index` 等新用例） |
| A4 candidate-lc1 | `cargo test --locked -p storage-sqlite --test admin_store --test imported` | **0** | `reports/candidate-lc1.log`（admin_store 32 passed / imported 10 passed，0 failed） |
| A4 第二提交 | `git add -f …/reports/`；`git commit -m 'docs(storage): 记录 fix-admin-store-integrity-gaps 候选验证证据'` | 0 | `1af5af4`，18 文件 +3220（含 `candidate-*.log` 与 `round1/*.log`、`pv*.log`、`lc1*.log`，非空提交 ✓） |
| B5 防竞态 | `git rev-parse refs/heads/main` | 0 | `1ef6640…` 未变 ✓ |
| B6 合入 | `git switch main`；`git merge --ff-only fix/admin-store-integrity-gaps` | 0 | **fast-forward 成功**，`main` = `1af5af4`；`git status --porcelain` 为空；6 文件 +556/−69 + 变更目录；`origin/main` 仍为 `1ef6640…` |
| C7 main-pv | `npm run verify` | **0** | `reports/main-pv.log`（38 个 `test result: ok`，0 `FAILED`，0 `error[`；`contract drift OK: §7 的 36 条 DDL…§5 的 15 个 trait / 87 个方法签名`） |
| C7 main-lc1 | `cargo test --locked -p storage-sqlite --test admin_store --test imported` | **0** | `reports/main-lc1.log`（32 + 10 passed） |
| C7 第三提交 | `git add -f …/reports/`；`git commit -m 'docs(storage): 记录 fix-admin-store-integrity-gaps 主分支验证证据'` | 0 | `78e1db1`，2 文件 +639（`main-pv.log`、`main-lc1.log`）；提交后 `git status --porcelain` 为空、无暂存项 |
| 最终核对 | `git diff --quiet 1af5af4 78e1db1 -- crates docs` | 0 | 第三提交只新增 2 个日志文件，产品与文档树逐字节不变 ⇒ C7 结果覆盖最终 main 树 |

### 处理过的插曲与更正

1. **`index.lock` 插曲（已解决，非失败）**：我最初把「只读核对」与「`git add -A` + `git commit`」放进同一批并行调用，`git status` 的索引刷新与提交争抢 `D:/Project/acp-remote/.git/index.lock`，`git commit` 报 `fatal: Unable to create … index.lock: File exists`。随即确认无 `git.exe` 进程且锁已消失（临时锁，未手工删除任何锁文件），重新提交一次即成功。此后所有 git 命令改为串行。**未使用 `--no-verify`，未改写任何历史。**
2. **对 supervisor 决定理由的事实更正**：回复中提到「已归档的 `openspec/changes/archive/*/reports/` 里也保留了 `.log`（如 storage-ddl-constraint-coverage 的 `lc1-*.log`/`pv*.log`）」——实测这些文件是 **未跟踪且被忽略**（`git status --ignored` 显示为 `!!`），该项归档目录只提交了 `MU1.md` 与 `branch-diff.patch`。真实先例是 `564d076` 提交的顶层 `reports/*.log`（9 个，`.gitignore` 当时已含 `*.log`，即 `git add -f`）。**决定 (a) 的结论不受影响**，我按 (a) 执行并已如实记录本项更正。

## 阶段结论

- **A（候选构造）= PASS**：候选 `c7c7ade`（产品修订）内容与已检视修订逐字节一致，`candidate-pv`/`candidate-lc1` 全绿。
- **B（合入）= PASS**：防竞态复核通过后 `--ff-only` 快进合入本地 `main`，无冲突、无 merge commit、无远端动作。
- **C（主分支验证）= PASS**：`main-pv`/`main-lc1` 全绿并已用第三提交记录；最终 `main` = `78e1db1b432289a5d67f1d7498704884d9cc7e44`，工作区干净、无暂存项、`origin/main` 未移动。
- **整体结果：已本地合入，未推送。** 候选 PASS 不等于最终验收 PASS；主 Agent 仍需按 `.agents/skills/agentic-verify/SKILL.md` 执行验收后才可报告可归档。

## 未解决项与残留风险

1. **仅 CI 可执行的检查未在本地运行，不得声称通过**：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）无本地等价物；本机 `gitleaks` 未安装，pre-commit 已按其设计「跳过并提示」。`commits` job 的提交范围校验同样只在 CI 运行。
2. **本地 `main` 领先 `origin/main` 3 个提交，尚未推送**：`origin/main` = `1ef6640…`（未越权）。CI 判定因此尚未生效。
3. **强制入库的归档文件与忽略规则的既有偏差**：26 个 `openspec/changes/fix-admin-store-integrity-gaps/reports/` 文件已跟踪，其中 20 个 `*.log` 是 `git add -f` 入库（`.gitignore:15` 未改）。这是 supervisor 决定 (a) 的显式选择；副作用是这些文件此后对忽略规则「免疫」，`*.log` 规则对它们不再生效。
4. **保留的本地资源**：分支 `fix/admin-store-integrity-gaps` @ `1af5af4` 未删除（内容已全部在 `main` 中，仅 `main` 多出第三提交的 2 个日志）。我按「不执行未明确要求的破坏性动作」保留它，供后续检视锚点；如需清理，删除该本地分支即可，无需改写历史。
5. **本报告文件本身**：`reports/du1.md` 由宿主在本轮结束后写入，届时工作区会把它显示为未跟踪（`.md` 不被忽略）。它不是任何提交的一部分，我未自行 `git add`；是否入库由主 Agent 决定（若入库，建议单独一个 `docs(storage): …` 提交，不要混入已完成的三个提交）。
6. **C7 的计时说明（已消除歧义）**：`main-pv`/`main-lc1` 在 `main` 树 = `1af5af4` 时执行；第三提交只新增 `main-*.log`，`git diff --quiet 1af5af4 78e1db1 -- crates docs` 退出 0，故对最终 `main` 树同样成立。
7. 变更目录中的 `verification.md`/`tasks.md` 的「已合入」状态与实际一致（`main` 已含候选），但我**未修改**任何规划文件。

**推荐下一步**：主 Agent 取用本报告的 SHA 链与日志证据，按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收；在授权范围内决定是否推送 `main` 触发 CI 五 job（`commits`/`deps`/`advisories`/`secrets` 尚未运行）、是否把 `reports/du1.md` 单独入库，以及是否删除已合入的本地分支 `fix/admin-store-integrity-gaps`。