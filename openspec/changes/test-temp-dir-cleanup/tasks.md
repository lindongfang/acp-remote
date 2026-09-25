<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 DU1；主 Agent。核实目标基线与工具：`git rev-parse refs/heads/main`、`git status --porcelain`、Node/Rust 版本（`rust-toolchain.toml`）。完成条件：命令、退出码与结果写入 `verification.md` 的运行时基线（规划时刻的背景值 main = `ad29199` 仅作参考，不作验收依据）。
- [x] 1.2 DU1；主 Agent。确认契约边界：`.openspec.yaml` 已置 `skip_specs: true`，无契约资产改动；写入范围 = 仅 `crates/*/tests/**` 与 `crates/*/src/**` 的 `#[cfg(test)]` 模块 + 本变更目录。完成条件：写入范围记入 `verification.md`；实施中若发现必须触碰产品代码/契约，先回到主 Agent 更新计划。
- [x] 1.3 DU1；主 Agent。资源安排：PV2 计数窗口内系统临时目录为独占观察资源（串行执行，不并行其它 `cargo test`）；`target/` 串行共享。完成条件：隔离方式与释放方法写入 `verification.md`；确认本变更不涉及数据库、容器、端口或外部账号。
- [x] 1.4 DU1；主 Agent。无上游代码依赖（单工作包、首个交付单元，无代码交接）。完成条件：该依据写入 `verification.md` 的 Dependency Handoffs。

## 2. Implementation

- [x] 2.1 WP1；前置：1.1–1.4；实现 Agent（coder）。全量盘点：以 `grep -rn 'temp_dir()' crates/ --include='*.rs'` 的实际输出建核对表（当前约 25 个创建点），逐点标记「已有守卫 / 无泄漏（仅拼路径未创建）/ 待修复」，必须覆盖 `core`（`acpr-ws-*`）、`agent-host`（`acpr-agent-host-*.txt`）、`server`（`audit.rs`/`params.rs`）、`identity-keystore`（`acpr-keystore-modes-*`/`acpr-keystore-atomic-*`）、`app`（`acpr-wp4b-input-*` 等）。完成条件：核对表写入 `reports/inventory.md` 且与 grep 输出逐行一致，无未分类点。
- [x] 2.2 WP1；前置：2.1；实现 Agent（coder）。修复 `crates/storage-sqlite/tests/support/mod.rs` 的 `temp_dir()`：改为返回 `Drop` 守卫（`Deref<Target = Path>` + `#[must_use]`，design D1/D3），按 D2 准则审计全部调用点（约 100 处），inline 临时值写法一律改为先绑定再使用。完成条件：`cargo test -p storage-sqlite` 全绿；核对表中 storage-sqlite 的全部创建点状态为「已守卫」。
- [x] 2.3 WP1；前置：2.1；实现 Agent（coder）。按盘点结论修复其余泄漏点（identity-keystore 单测、app `cli/input.rs` 测试、agent-host 的 txt 文件等），同一守卫准则；判定「无泄漏」的点必须在 `reports/inventory.md` 写明依据。完成条件：各受影响 crate 的 `cargo test -p <crate>` 全绿；核对表全部条目闭合。
- [x] 2.4 WP1；前置：2.2、2.3；实现 Agent（coder）。WP1 收尾：`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 全绿。完成条件：交接报告写入 `reports/wp1-handoff.md`（含改动文件清单与盘点核对表引用）。

## 3. Branch Validation

- [ ] 3.1 DU1；前置：2.4；主 Agent。交付前验证：[PV1] `npm run verify`（EXIT=0，日志 `reports/final-verify.log`）+ [PV2] 临时目录计数验收（完整测试运行前后 `acpr-*` 条目数差为 0，原始命令与输出写入 `reports/temp-count-final.log`）。完成条件：两项全绿；核实无本次新增资源残留；若 PV2 有残余，按名称前缀归因到用例并回到 WP1 修复后重跑。
- [ ] 3.2 DU1；前置：2.4，可与 3.1 并行；独立 reviewer（fresh、只读子 Agent）。[RV1] 检视 WP1 全部 diff，关注点与阻断标准见 `plan.md` Code Review（盘点无漏点、守卫满足 design D1–D4、未触碰产品代码/契约资产、未弱化断言）。完成条件：报告 `reports/rv1-wp1.md`；发现修复后由新子 Agent 复核。

## 5. Integration Readiness

- [ ] 5.1 仅一次，不随单元复制；主 Agent。创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、本计划、候选版本与证据、`refs/heads/main` 及合入条件。完成条件：记录集成 Agent 实际 ID 及上下文方式；主 Agent 不兼任。
- [ ] 5.2 DU1；主 Agent。复核 DU1 预定模式（independent）与组成（WP1，无 TP），核对 [PV1]/[PV2]/[RV1] 的有效证据。完成条件：证据清单核对一致；任何变化先同步计划再合入。

## 6. Merge Unit

- [ ] 6.1 DU1；前置：5.2；主 Agent。核实目标仓库及主分支当前提交（`git rev-parse refs/heads/main` + `git status --porcelain`）。完成条件：准确引用与核实证据写入 `verification.md`；无法确认时保持 BLOCKED。
- [ ] 6.2 DU1；前置：6.1；集成 Agent。基于已核实基线构造候选（分支 `feat/test-temp-dir-cleanup`），固定基线与候选提交。完成条件：候选提交、基线提交与构建结果写入 `verification.md`。
- [ ] 6.3 DU1；前置：6.2；主 Agent。候选轮 [PV1] + [PV2] 全绿。完成条件：版本、范围、退出码与日志路径关联到 `verification.md`。
- [ ] 6.4 DU1；前置：6.2，可与 6.3 并行；独立 reviewer。只读检视固定候选（范围同 [RV1]）。完成条件：报告写入 `verification.md`；修复后由新子 Agent 复核。
- [ ] 6.5 DU1；前置：6.3、6.4；主 Agent。Main E2E 为 not-applicable：核对降级理由、依据与 `downgrade_approval` 记录完整，并核对替代检查 [PV1]/[PV2] 的结果证据齐备。完成条件：核对结论写入 `verification.md`。
- [ ] 6.6 DU1；前置：6.3、6.4、6.5 且 premerge 门 PASS；集成 Agent。本地合入 `refs/heads/main`（优先 `--ff-only`），不推送远端。完成条件：合入提交与树哈希记录到 `reports/integrator.md` 与 `verification.md`。
- [ ] 6.7 DU1；前置：6.6；集成 Agent。主分支回归 [PV1]。完成条件：EXIT=0，日志 `reports/main-verify.log`。
- [ ] 6.8 DU1；前置：6.7；主 Agent。合入差异审查：相对已验收候选无新增差异则引用 [RV1] 既有审查，否则补审。完成条件：结论与依据写入 `verification.md`。

## 7. Final E2E

- [ ] 7.1 DU1；主 Agent。not-applicable 替代验证清单核对：PV1/PV2 证据路径可读，`reports/temp-count-final.log` 的计数差为 0 可复现。完成条件：核对结论写入 `verification.md`。
- [ ] 7.2 DU1；主 Agent。资源清理核实：无本次新增 `acpr-*` 残留、无遗留测试进程与多余 worktree。完成条件：清理结果写入 `verification.md`。
- [ ] 7.3 [e2e-owned] 全变更；前置：7.2；扩展（openspec-agentic）。运行 `npx --quiet --no-install openspec-agentic e2e check --change test-temp-dir-cleanup`，确认「不适用判据已按计划固化」（降级批准可追溯、替代检查清单齐备）。完成条件：由该检查在 PASS 时自动勾选本行、非 PASS 时自动回退；主 Agent 不得手勾或手动回退。本行只检查门禁，不执行测试、不汇总结果。

## 8. Final Verification

- [ ] 8.1 [final-verification] 全变更；前置：7.3；主 Agent。使用 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收，核对用户意图（本会话「测试临时目录清理」授权与降级批准原话）、需求覆盖、设计一致性、计划与任务完成度、最终主分支证据；记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change test-temp-dir-cleanup --stage final --json`，全部通过才完成。
