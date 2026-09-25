<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 DU1；主 Agent。核实目标基线与工具：`git rev-parse refs/heads/main`、`git status --porcelain`、Node/Rust 版本（`rust-toolchain.toml`）。完成条件：命令、退出码与结果写入 `verification.md` 的运行时基线（规划时刻背景值 main = `84a8a05` 仅作参考）。
- [x] 1.2 DU1；主 Agent。确认契约边界：增量规范 `specs/local-agent-host/spec.md`（1 条 ADDED Requirement）；写入范围 = 仅 `crates/agent-host/src/process.rs` 的 `abort_agent` 函数与其注释 + 本变更目录。完成条件：写入范围记入 `verification.md`；实施中若发现需要改动其它函数/文件，先回主 Agent 更新计划。
- [x] 1.3 DU1；主 Agent。资源安排：PV2 三连跑窗口内不并行其它 `cargo test`（被测场景就是全量并行负载本身）；`target/` 串行共享。完成条件：隔离方式写入 `verification.md`；确认不涉及数据库、容器、端口或外部账号。
- [x] 1.4 DU1；主 Agent。无上游代码依赖（单工作包、首个交付单元）。完成条件：该依据写入 `verification.md` 的 Dependency Handoffs。

## 2. Implementation

- [x] 2.1 WP1；前置：1.1–1.4；实现 Agent（coder）。按 design D1 把 `crates/agent-host/src/process.rs` 的 `abort_agent` 调整为「`exit.mark(...)` → drain pending 并投递 Oversize 错误 → `tree.terminate()`」，并同步函数注释为实际顺序；按 D2 核实两条不变量并在交接报告引用代码行（`ExitState::mark` 的幂等/覆写语义、`wait_loop` 空 drain 与 status 覆写、`abort_agent` 内 `terminate` 无条件执行、无「依赖 is_running() 为真才清理」的反向逻辑）。完成条件：新顺序与注释一致；`wait_loop` 与其它 `exit.mark` 调用点零改动；交接报告含 D2 核实记录。
- [x] 2.2 WP1；前置：2.1；实现 Agent（coder）。局部验证：`cargo test -p agent-host`（重点 `oversize_frame` 用例）+ `cargo fmt --all -- --check` + `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`。完成条件：三项全绿，命令与退出码写入 `reports/wp1-local-checks.log`；交接报告写入 `reports/wp1-handoff.md`。

## 3. Branch Validation

- [ ] 3.1 DU1；前置：2.2；主 Agent。交付前验证：[PV1] `npm run verify`（EXIT=0，日志 `reports/final-verify.log`）+ [PV2] `cargo test --locked --workspace --all-features` 连跑 3 次（串行窗口，日志 `reports/stress-runs.log`）。完成条件：3 次均 EXIT=0 且 `oversize_frame` 用例每轮全绿；任一轮红即 FAIL 并按 plan.md 的 Failure and Recovery 归因处理，不得以重跑掩盖。
- [ ] 3.2 DU1；前置：2.2，可与 3.1 并行；独立 reviewer（fresh、只读子 Agent）。[RV1] 检视 WP1 全部 diff + 相关上下文，关注点与阻断标准见 `plan.md` Code Review。完成条件：报告 `reports/rv1-wp1.md`；发现修复后由新子 Agent 复核。

## 5. Integration Readiness

- [ ] 5.1 仅一次，不随单元复制；主 Agent。创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、本计划、候选版本与证据、`refs/heads/main` 及合入条件。完成条件：记录集成 Agent 实际 ID 及上下文方式；主 Agent 不兼任。
- [ ] 5.2 DU1；主 Agent。复核 DU1 预定模式（independent）与组成（WP1，无 TP），核对 [PV1]/[PV2]/[RV1] 的有效证据。完成条件：证据清单核对一致；任何变化先同步计划再合入。

## 6. Merge Unit

- [ ] 6.1 DU1；前置：5.2；主 Agent。核实目标仓库及主分支当前提交（`git rev-parse refs/heads/main` + `git status --porcelain`）。完成条件：准确引用与核实证据写入 `verification.md`；无法确认时保持 BLOCKED。
- [ ] 6.2 DU1；前置：6.1；集成 Agent。基于已核实基线构造候选（分支 `feat/agent-host-oversize-exit-ordering`），固定基线与候选提交。完成条件：候选提交、基线提交与构建结果（`cargo build --locked --workspace` EXIT=0）写入 `verification.md`。
- [ ] 6.3 DU1；前置：6.2；主 Agent。候选轮 [PV1] + [PV2]（三连跑）全绿。完成条件：版本、范围、退出码与日志路径关联到 `verification.md`。
- [ ] 6.4 DU1；前置：6.2，可与 6.3 并行；独立 reviewer。只读检视固定候选（范围同 [RV1]）。完成条件：报告写入 `verification.md`；修复后由新子 Agent 复核。
- [ ] 6.5 DU1；前置：6.3、6.4；主 Agent。Main E2E 为 not-applicable：核对降级理由、依据与 `downgrade_approval` 记录完整，并核对替代检查 [PV1]/[PV2] 的结果证据齐备。完成条件：核对结论写入 `verification.md`。
- [ ] 6.6 DU1；前置：6.3、6.4、6.5 且 premerge 门 PASS；集成 Agent。本地合入 `refs/heads/main`（优先 `--ff-only`），不推送远端。完成条件：合入提交与树哈希记录到 `reports/integrator.md` 与 `verification.md`。
- [ ] 6.7 DU1；前置：6.6；集成 Agent。主分支回归 [PV1]。完成条件：EXIT=0，日志 `reports/main-verify.log`。
- [ ] 6.8 DU1；前置：6.7；主 Agent。合入差异审查：相对已验收候选无新增差异则引用既有 review，否则补审。完成条件：结论与依据写入 `verification.md`。

## 7. Final E2E

- [ ] 7.1 DU1；主 Agent。not-applicable 替代验证清单核对：PV1/PV2 证据路径可读，`reports/stress-runs.log` 的三连跑记录完整可复现。完成条件：核对结论写入 `verification.md`。
- [ ] 7.2 DU1；主 Agent。资源清理核实：无本次新增 `acpr-*` 残留、无遗留测试进程与多余 worktree。完成条件：清理结果写入 `verification.md`。
- [ ] 7.3 [e2e-owned] 全变更；前置：7.2；扩展（openspec-agentic）。运行 `npx --quiet --no-install openspec-agentic e2e check --change agent-host-oversize-exit-ordering`，确认「不适用判据已按计划固化」（降级批准可追溯、替代检查清单齐备）。完成条件：由该检查在 PASS 时自动勾选本行、非 PASS 时自动回退；主 Agent 不得手勾或手动回退。本行只检查门禁，不执行测试、不汇总结果。

## 8. Final Verification

- [ ] 8.1 [final-verification] 全变更；前置：7.3；主 Agent。使用 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收，核对用户意图（PRO-4 登记、本会话选项 A 授权与降级批准原话）、需求覆盖、设计一致性、计划与任务完成度、最终主分支证据；记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change agent-host-oversize-exit-ordering --stage final --json`，全部通过才完成。
