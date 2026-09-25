# Tasks: agent-host-spawn-failure-coverage

<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 [全变更] 负责人：主 Agent；无独立采集需求，直接引用已有可信来源：仓库根 `D:\Project\acp-remote`、目标引用 `refs/heads/main`、工具链以 `rust-toolchain.toml` 为准、统一检查入口 `npm run verify`（AGENTS.md §8）；完成条件：verification.md 记录以上来源，无另行 recon。
- [x] 1.2 [WP1、WP2] 负责人：coder（deepseek/deepseek-flash）；依赖：无；确认契约（`specs/local-agent-host/spec.md` 增量、`design.md` 决策 1–4）与文件所有权：WP1 独占 `crates/agent-host/tests/catalog.rs`，WP2 独占 `crates/agent-host/src/error.rs`（仅追加 `#[cfg(test)]` 模块）；完成条件：记录编码起点（apply 开始时的 `refs/heads/main` 提交）；契约已冻结，允许 WP1 ∥ WP2 并行编码。
- [x] 1.3 [WP1、WP2] 负责人：主 Agent；无共享运行资源：fake ACP child 由 cargo 测试构建提供，测试临时文件由各测试自身 `temp_dir` 清理；仅 `target/` 为共享构建目录，本变更默认串行执行，并行时以各自 `CARGO_TARGET_DIR` 隔离；完成条件：verification.md 记录该依据。
- [x] 1.4 [WP1、WP2] 负责人：主 Agent；无代码依赖：WP1/WP2 互不依赖，也不依赖未落地 crate，共同只读引用 `crates/agent-host/src/error.rs` 现有映射表；完成条件：verification.md 记录不适用依据。

## 2. Implementation

<!-- 启动时完整读取并传入 roles/coder.md 及工作包输入；单 Agent 串行实现也使用相同指令。 -->

- [x] 2.1 [WP1] 负责人：coder；依赖：1.2；在 `crates/agent-host/tests/catalog.rs` 新增集成测试 `spawn_failure_is_explicit_unavailable_without_side_effects`：用 `profile_with` 构造指向不存在程序的 profile，直接调用 `create()`，断言 ① 返回 `Err(PortError::Unavailable(_))` 且 kind 为 `UnavailableKind::IoError`（精确匹配，区别于 `InvalidRequest`/`KeystoreUnavailable`/`Busy`），② 失败后无运行中的 runtime（`wait_until_not_running`）、`open()` 该会话报未知会话，③ 紧接着第二次 `create()` 仍是干净的 `Unavailable(IoError)`（重试不毒化）；局部自检：`cargo test --locked -p agent-host --all-features spawn_failure` 通过；完成条件：测试存在、通过且覆盖 specs 的 S1 三个可观察结果，不夹带任何产品代码改动；S1 的交付门禁由 [PV1]（任务 3.1）承载；若暴露实际行为与 S1 不符，停止编码并按 proposal decision_bounds 上报。
- [x] 2.2 [WP2] 负责人：coder；依赖：1.2；在 `crates/agent-host/src/error.rs` 追加 `#[cfg(test)] mod tests`：为 `HostError` 全部变体（当前 19 个，以代码为准）各构造实例，表驱动断言 `to_port_error()` 的 `PortError` 类别与 `ConflictKind`/`UnavailableKind`（含 `SpawnFailed → Unavailable(IoError)`），只钉类别/kind 不钉消息文本；局部自检：`cargo test --locked -p agent-host --all-features error::tests` 通过；完成条件：全部变体覆盖（当前 19 个）、测试通过、映射表本身未被改动。

## 3. Branch Validation

- [x] 3.1 [WP1、WP2] 负责人：主 Agent；依赖：2.1、2.2；在分支上执行 [PV1]：仓库根运行 `npm run verify`（= `npm run check` 合同门禁含规范增量校验 + `npm run check:rust` 的 fmt/clippy/workspace 全量测试，含新增两组测试与 S2–S5 既有场景回归），逐项记录完整命令、工具版本、退出码与日志；完成条件：全部子检查零失败，日志落 `reports/PV1.log`（候选），检查资源核实已释放。
- [x] 3.2 [WP1、WP2] 负责人：reviewer（kimi-coding/k3，非用例作者，只读独立子 Agent）；依赖：2.1、2.2，可与 3.1 并行；按 plan.md Code Review 的关注点检视同一版本：spawn 失败路径被真实驱动、断言精确到 `UnavailableKind::IoError`、16 变体覆盖且未钉消息文本、无产品代码夹带；完成条件：记录 Agent ID、版本、隔离方式与报告，无未解决阻断项；修复后由新子 Agent 复核。

## 4. Integration Readiness

- [ ] 4.1 [全变更]（仅一次，不随单元复制）负责人：主 Agent；依赖：3.1、3.2；单独创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、计划/契约、源提交及验证与 review 证据、目标分支与交付边界（远端 main 交付走 AGENTS.md §8 的 PR 路径，本地合入步骤不替代 PR）；完成条件：记录实际 Agent ID、上下文方式与交接清单；缺少独立执行能力时该任务 BLOCKED。
- [ ] 4.2 [DU1] 负责人：主 Agent；依赖：4.1；复核 DU1 的 integrated 模式与 WP1/WP2 组成，核对 [PV1] 与 3.2 review 的有效证据；完成条件：确认组合 verify/review 结果可用，变化时先同步计划与依赖。

## 5. Merge Unit

- [ ] 5.1 [DU1] 负责人：主 Agent（机械核实可交 environment/recon）；依赖：4.2；核实目标仓库与 `refs/heads/main` 当前提交（`git -C D:\Project\acp-remote rev-parse refs/heads/main`）并记录准确引用与核实证据；完成条件：记录提交 SHA、核实时间与命令输出；无法确认时保持 BLOCKED。
- [ ] 5.2 [DU1] 负责人：集成 Agent；依赖：5.1；基于已核实基线构造候选分支 `test/agent-host-spawn-failure-coverage`（或 `test/...` 等合规 type/scope 分支），固定基线与候选版本，记录组成与构建结果；完成条件：候选可构建且版本可追溯。
- [ ] 5.3 [DU1] 负责人：检查执行者；依赖：5.2；在候选版本上执行 [PV1]，将版本、范围、结果及有效复用依据关联到 verification.md；完成条件：候选 [PV1] 全绿并留证。
- [ ] 5.4 [DU1] 负责人：独立 reviewer；依赖：5.2，可与 5.3 并行；只读检视固定候选的 diff 与契约一致性（本变更只允许测试与规范增量文件），修复后独立复核；完成条件：无未解决阻断项，记录隔离设置、版本与报告。
- [ ] 5.5 [DU1；mode = not-applicable] 负责人：主 Agent；依赖：5.2；核对 Main E2E 不适用的 reason/basis 与 downgrade_approval（2026-09-25 本会话用户原话）仍在 plan.md 中有效，并确认替代验证（[PV1] 含 agent-host 全量测试）已执行且证据有效；完成条件：替代检查全部通过，verification.md 记 NOT_APPLICABLE 及依据。
- [ ] 5.6 [DU1] 负责人：主 Agent；依赖：5.3、5.4、5.5；候选合入前在固定候选提交上运行 `npx --quiet --no-install openspec-agentic workflow check --change agent-host-spawn-failure-coverage --stage premerge --planning-root D:\Project\acp-remote --json`，PASS 后按 AGENTS.md §8 走 PR：推送候选分支、`gh pr create --fill`（Conventional Commits 标题）、`gh pr checks --watch` 五个必需检查全绿（PR 需先合入最新 main）、`gh pr merge --squash --delete-branch`；完成条件：记录 PR 编号、各检查状态与合并后的 main 提交 SHA；premerge 非 PASS 不合入。
- [ ] 5.7 [DU1] 负责人：检查执行者；依赖：5.6；核对实际主分支结果与候选一致性，在合并后的 `main` 上完成 [PV1] 回归；有效复用逐项记录原证据与适用性；完成条件：主分支 [PV1] 全绿，日志落 `reports/PV1.log`（主分支份）。
- [ ] 5.8 [DU1] 负责人：独立 reviewer；依赖：5.6，可与 5.7 并行；独立检视合并新增差异；无新增差异时由主 Agent 记录依据及 3.2/5.4 的原 review ID，不强制同范围重复审查；完成条件：无未解决阻断项并留证。

## 6. Final E2E

- [ ] 6.1 [全变更] 负责人：主 Agent；依赖：5.7、5.8；执行 Main E2E 的替代验证：在固定的最终主分支版本上运行 `cargo test --locked -p agent-host --all-features` 与 `npm run verify`，逐项记录命令、版本、退出码与日志；完成条件：全部替代检查通过并留证（可与 5.7 证据逐项核对复用，复用须记录依据）。
- [ ] 6.2 [全变更] 负责人：主 Agent；依赖：6.1；汇总替代证据、核对 Coverage Index 的 6 行覆盖（R1、S1–S5）与 downgrade_approval 记录，确认测试临时资源与构建目录已清理；完成条件：verification.md 的替代验证结论完整且资源清理有记录。
- [ ] 6.3 [e2e-owned] [全变更] 负责人：扩展；依赖：6.2；运行 `npx --quiet --no-install openspec-agentic e2e check --change agent-host-spawn-failure-coverage`，仅确认不适用判据（downgrade_approval 齐备 + 替代验证先完成）并按结果自动勾选/回退本行；完成条件：门禁 PASS；主 Agent 不得手勾或手动回退本行。

## 7. Final Verification

- [ ] 7.1 [final-verification] 负责人：主 Agent；依赖：6.3；按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收（`/opsx:verify` 同样读取该入口），核对用户意图（proposal 的 `agentic-intent`：补上切片 2 启动失败路径缺口）、specs 的 S1 场景与 S2–S5 回归、design 决策 1–4、plan、tasks 与最终主分支证据，记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change agent-host-spawn-failure-coverage --stage final --json`；完成条件：全部通过并记录 PASS 结论；未通过或证据失效时保持待办并如实报告 FAIL/BLOCKED，不报告可归档。
