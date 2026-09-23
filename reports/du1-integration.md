# du1-integration.md — DU1 集成就绪、候选与 E2E 处置

## 1. 就绪段（task 5.2）

- 交付单元：`DU1`，模式 `integrated`（`plan.md` 的 Delivery Units 表），组成 WP1–WP6。
- 目标主分支：`refs/heads/main` = `37a398e9dbafa368bdb15853e1c1d9b40f19b28d`（`git rev-parse refs/heads/main`，2026-09-23 三次核对未变：取证时、构建后、清理后）。
- 集成分支/worktree：`feat/admin-state-persistence-v2` / 主 worktree（`plan.md` 的 Merge Strategy 已定；未新建 worktree — 本次为单执行者串行）。
- 候选（固定）：`62ef2649ae6d35e65930df505b2cf41858a19d26`；其父链 `... 601c8ae → aed9fb5 → 86ae8b4 → 5404610 → f43f7a7 → 1baea5b → 013f2b9`，共 8 个提交构成本单元（分支继承的 `5d77f25`/`7cdff57`/`28f8cb9` 三个文档提交不算）。
- `3.x` 证据对当前候选仍然有效：`3.1`/`3.3`/`3.5`/`3.7`/`3.9` 的 PV4/PV1/PV2/PV3 证据在 `openspec/changes/admin-state-persistence-v2/reports/*.log` 与 `reports/wp6-admin-store-tests.log`（`check:drift` 的 36 条 DDL / 15 trait / 87 方法在候选上未变），`3.2`/`3.4`/`3.6`/`3.8`/`3.10` 的独立 review 报告为 `reports/rv1-wp{1,23,4,5,6}.md` + `rv1-wp6b.md`。
- 候选轮的新证据：`reports/du1-integrator.md`（6.2 包含关系与构建）、`reports/du1-pv1.log` + `reports/du1-checker.md`（6.3 独立 PV1）、`reports/rv1-du1.md`（6.4 候选 review，1 条阻断已修）、`reports/rv1-du1-r2.md`（修复轮 recheck，结论 `correct`）、`reports/clean-tree-check.log`（干净检出文档门禁）、`reports/verify-du1-fixes.log`、`reports/verify-du1-fixes-2.log`。
- 就绪结论：**候选侧 PASS**（构建/独立 PV1/独立 review/复核均通过）；合入侧 **BLOCKED**（缺合并授权，见 §3）。

## 2. E2E 段（task 6.5）

- 项目开关：`x-agentic.e2e.enabled = true`，但 `plan.md` 的 mode 为 **`not-applicable`**，附用户降级批准原话（2026-09-23，本会话）。
- 因此**没有**任何必须在候选或最终阶段运行的 E2E 任务：`[e2e-owned]` 门禁行（tasks `7.3`）只确认该不适用判据已按计划固化。
- 替代检查安排：`7.1` 的替代验证（`cargo test -p core -p storage-sqlite --all-features`、`npm run verify`、`check:drift`、`from-v1` 夹具重放）按计划在**最终主分支版本**上执行；候选轮已由 `6.3` 的独立 PV1 与 `6.4`/recheck 覆盖同等命令面。
- 结论：候选阶段 E2E = `NOT_APPLICABLE`，无 E2E 任务被跳过或降级。

## 3. 合入边界（task 6.6 起）

- `plan.md` 的 Merge Strategy 明确「合并与推送授权仍受当前会话限制」；本轮交办亦未授予合并/推送权限。因此 `6.6`（合入）、`6.7`/`6.8`（主分支复验与复核）、`7.x`（最终替代验证）与 `8.1`（最终验收）保持**未执行**，候选与全部证据保留在原地。
- 若获得授权：目标 `refs/heads/main` 当前是候选的祖先（快进关系），合入后 `HEAD` 预期等于候选 SHA；合入前须再校 `refs/heads/main` 未变，变化则候选证据整体失效并重建。