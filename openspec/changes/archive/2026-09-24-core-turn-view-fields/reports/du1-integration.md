# DU1 集成报告（`core-turn-view-fields`）

- 交付单元：DU1（`integrated`，覆盖 WP1–WP4）
- 基线：`1cd0416`（= `refs/heads/main` = `origin/main`，合并前）
- 候选：`6856a6e`（分支 `feat/core-turn-view-fields` 的 tip，前身为 `f582376` → `5ff2d0c` → `27c9155` → `6856a6e`）
- 目标：本地 `refs/heads/main`（快进合入；未推送、未发布、未回滚）
- 报告执行者：主 Agent（integrator 角色，tasks 5.1/5.2/6.1/6.6）+ 两个隔离的独立检查上下文（tasks 6.2/6.3/6.5 与 6.7/6.8）+ 一个隔离的独立 reviewer（tasks 6.4/6.8）

## 1. 集成就绪检视（tasks 5.1/5.2）

| 判据 | 事实 | 结论 |
| --- | --- | --- |
| 所有 WP 已完成且证据可复现 | WP1/WP2/WP3/WP4 的实现与测试均落在候选版本；`reports/wp1-contract-docs.log`、`wp2-core-injection.log`、`wp3-storage-version.log`、`wp4-agent-host-contract.log` 均 exit 0，且含命令原文与计数 | 满足 |
| Review 无未解决阻断项 | RV1（`reports/rv1-wp1.md`、`rv1-wp3.md`，被检 `f582376`）：0 BLOCKER / 0 MAJOR；RV1-REC（`reports/rv1-wp2-recheck.md`，复核 `f582376..27c9155`）：0 BLOCKER / 0 MAJOR，7 条记录类 MINOR 已闭合；RV-DU1（`reports/rv1-du1.md`，检视 `1cd0416..6856a6e`）：0 BLOCKER / 0 MAJOR | 满足 |
| 契约与边界未破坏 | `check:drift`（§7 的 36 条 DDL、§5 的 15 trait / 87 签名）、`check:boundaries`（8 个 crate）、`check:doc-links`、`check:agentic` 在候选与主分支两次复核中均 exit 0 | 满足 |
| 依赖与 schema 未变 | `1cd0416..6856a6e` 的 21 个文件中 **无** `Cargo.lock`/`Cargo.toml` 变化，无新依赖、无 DDL/端口/schema 变化（`session_store.rs` 仅 1 行 epoch 回填） | 满足 |
| 待办交接 | 无实现交接（RV-DU1 明确「不建议为实现目的启动实现交接」）；5.2 的就绪判据已由上述三份内外部证据满足 | 满足 |

## 2. 候选构建与统一入口（tasks 6.1/6.2/6.3/6.5）

- 固定版本：`HEAD == 6856a6e`，`refs/heads/main == 1cd0416`（合入前），工作区干净，无并发 cargo 进程。
- 区间事实：`1cd0416..6856a6e` = **4 个提交、0 merge**，`21 files changed, 2876 insertions(+), 35 deletions(-)`，文件全部落在 `crates/**`、`docs/**`、`openspec/changes/core-turn-view-fields/**`。
- 候选构建：`cargo build --locked --workspace --all-features` → exit 0（0 warning / 0 error）。
- 统一入口 PV4（独立执行者，另一个上下文）：`npm run verify` → **exit 0**（耗时 25s）；日志 `reports/du1-pv1.log`（头部含用途、执行者、时间 `2026-09-24T19:15:24+08:00`、固定提交 `6856a6e`、命令原文；末尾 `# npm-run-verify exit=0` 与补充的 `# cargo-fmt-check exit=0`）。
  - 十道合同门禁全部 OK（schemas 117 valid + 23 invalid + 39 views；commands 12；errors 58；features 11；assets 17 schemas / 155 fixture / 12+20 transcripts / 2 SAS；acp 71 rows；docs 367 links / 3058 refs / 156 md；boundaries 8；drift 36 DDL + 15 trait / 87 sigs；agentic 8 passed）。
  - workspace 测试：**398 passed / 0 failed / 2 ignored**（54 个 `test result:` 行）；`acp_core` 94、`agent-host` 53（含 `view_contract` 1）、`storage-sqlite` 97 + 2 ignored（含 `session_version_rule` 3）。
  - 两条 ignored 均为既有 `#[ignore]`（`storage-sqlite` 的 `crash_child`、`regenerate_v2_fixtures`），本变更未新增。
- Main E2E 判据（task 6.5）：`plan.md` 记录 `not-applicable`（申请理由与 `downgrade_approval` 见该文件），因此 6.5 的判定是「降级理由仍成立」：本变更无对外可观测的新入口（无 CLI 子命令、无 wire 方法、无 HTTP 路由），四项替代检查覆盖了行为面；`openspec-agentic e2e --json` 显示 `enabled=true, command=""`（未配置命令），故不执行产品级 E2E。

## 3. 本地合入（task 6.6）

```text
git switch main            # main == 1cd0416，工作区干净
git merge --ff-only feat/core-turn-view-fields
# => Fast-forward：main 由 1cd0416 前进到 6856a6e（4 个提交）
git rev-parse HEAD refs/heads/main   # 两者均为 6856a6e48e331a62501c020eeba4dfe0d7ec4e84
```

- 合入方式：**快进**（无合并提交、无冲突解决、无内容改写）。
- 操作边界遵守：仅本地引用变更；**未** `push`（`origin/main` 仍为 `1cd0416`）、未打 tag、未发布、未回滚。
- 回退路径：`git reset --hard 1cd0416`（分支 `feat/core-turn-view-fields` 仍指向 `6856a6e`，候选可复现）。

## 4. 主分支复核与合并差异检视（tasks 6.7/6.8）

- 独立执行者（第三个上下文）在 `main == 6856a6e` 上复核：
  - `git rev-parse HEAD` = `git rev-parse refs/heads/main` = `6856a6e…4e84`；`git branch --show-current` = `main`；`git status --porcelain` 为空（写入日志前后均空，`.log` 被 `.gitignore` 忽略）。
  - 一致性：`git diff --stat 6856a6e HEAD` 为空、`git diff --stat 6856a6e feat/core-turn-view-fields` 为空、`git merge-base --is-ancestor 6856a6e HEAD` = 0（⇒ 主分支 tree 与候选 tree 完全一致，快进未引入候选之外的任何差异）。
  - 统一入口 PV4 复跑：`npm run verify` → **exit 0**（耗时 29s）；日志 `reports/du1-main-verify.log`（头部含固定提交 `6856a6e`、前后置状态；末尾 `# exit=0`）。
  - 计数对照：两份日志同为 54 个 `test result:` 行、`398 passed / 0 failed / 2 ignored`；剔除耗时字段后逐目标序列 diff 为空（唯一差异是个别用例耗时抖动）。
- 合并差异 review（RV-DU1，独立 reviewer，`reports/rv1-du1.md`）：区间完整性、快进一致性、修复批「零生产代码行」、证据日志自洽均通过；4 条 MINOR + 2 条 SUGGESTION 全为记录/注释准确性，**0 BLOCKER / 0 MAJOR**。

## 5. 合入后修正（记录类，非代码）

RV-DU1 的 4 条 MINOR + 2 条 SUGGESTION 已在主分支上以记录/注释修正落地（见 `verification.md` 的 Review Findings 表 RV-DU1 行）：

- `verification.md` 的 PV1 doc-links 计数改为最终树实测 `367 links / 3058 refs / 156 md`；
- 3.2 自检的用例增量改为「13 条（10 broker + 3 model）」；
- F2 行的 Retest Evidence 注明「首轮 91 passed，日志已被修复批复跑覆盖为 94 passed」；
- `crates/agent-host/tests/view_contract.rs` 的 `NOT_EXERCISED` 每项补一条原因注释（并同步 `verification.md` 的措辞）；
- 「行号更正表」→「行号更正清单」；`docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条的引用改为「本条 ① 的失败语义（与 §6 第 9 条同口径）」。

**证据适用性**：`du1-pv1.log` 与 `du1-main-verify.log` pin 的提交是 `6856a6e`；上述修正在其后（仅 `docs/**`、`openspec/**`、一条测试注释），按变更记录的「验后影响判断」口径，后续替代验证（tasks 7.1/7.2）将在最终主分支修订上重新跑四项检查并另立日志。

## 6. 结论

- `DU1 集成：PASS`（候选构建、候选 PV4、主分支 PV4、合并一致性与合并差异 review 全部通过；0 BLOCKER / 0 MAJOR）。
- 未执行项及原因：`cargo-deny`、`gitleaks`、`commits` job 仅能在 CI 判定（本机无等价物），本轮不声称其通过；产品级 E2E 未配置命令，按 `not-applicable` 降级理由以四项替代检查覆盖。
