<!-- 分组标题使用英文，任务正文使用中文。保留 - [ ] X.Y 格式，每项写明完成条件。
 Main E2E 为 not-applicable，故不生成 Test Design and Authoring 分组，并对后续分组重编号。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 核实目标仓库绝对路径与 `refs/heads/main` 当前提交、工作区是否干净、`rust-toolchain.toml` 与 Node 版本、以及 `cargo test -p storage-sqlite --test enum_coverage --test admin_store` 是否可运行；完成条件：返回结构化事实与证据（命令、输出、时间）并记录，无法运行的原因如实写明。
- [x] 1.2 [WP1、WP2] 确认契约（`docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3/§7.4、§9 判据 14 与 §10 裁定）与文件所有权：WP1 独占 `crates/storage-sqlite/tests/enum_coverage.rs`，WP2 独占 `crates/storage-sqlite/tests/admin_store.rs`；完成条件：记录编码起点（具体提交）与「零生产代码改动」的边界。
- [x] 1.3 确认无共享运行资源（SQLite 临时目录由 `support::temp_dir` 按进程 id 隔离；并行分片需各自 `CARGO_TARGET_DIR`）；完成条件：记录依据与清理方式。
- [x] 1.4 [WP1、WP2] 记录本变更无上游代码依赖（单仓库测试补丁），不需要跨变更交接；完成条件：记录不适用依据。

## 2. Implementation

- [x] 2.1 按 design D1/D2 扩展 `crates/storage-sqlite/tests/enum_coverage.rs`：(a) 为 `owned_device`/`owned_node` 的 `revoke_reason` 增加 DDL 允许值断言，期望值锚在 §7.3 合同文本而非 `revoke_token()`；(b) 以最小方式让取值解析支持 `<col> = '<literal>'` 等值 CHECK，并为 `owned_export`/`imported_import` 的 `cache_policy` 增加与 `core::model::CachePolicy` 一致的断言；不得为测试给 `core::ports::RevokeReason` 增加或暴露任何公开 API；完成条件：`cargo test -p storage-sqlite --test enum_coverage` 通过且 diff 内无生产代码改动，由 全量测试覆盖。
- [x] 2.2 按 design D3/D4 在 `crates/storage-sqlite/tests/admin_store.rs` 增加：(a) `RevokeReason::KeyChanged` 的设备与节点撤销行为回归（经 `TrustStore::revoke_device`/`revoke_node` 端口执行，事务成功并从库内读回 `key_changed`）；(b) 非法 `revoke_reason` 被真实 CHECK 拒绝的测试，含合法 token 控制组；完成条件：`cargo test -p storage-sqlite --test admin_store` 通过，由 全量测试覆盖。

## 3. Branch Validation

- [x] 3.1 [WP1、WP2] 在固定提交上执行 LC1 与，逐项记录完整命令、工具链/Node 版本、退出码与日志，并核实临时资源已释放；完成条件：LC1 与 全绿并留证，`cargo-deny`/`gitleaks` 如实记为「未在本地执行」。
- [x] 3.2 [WP1、WP2] 按 RV1 检视固定版本的实际 diff，重点确认零生产代码改动、`KeyChanged` 确实经过 `revoke_token()`、非法值测试针对真实 CHECK、等值解析扩展未影响既有断言；修复后由新子 Agent 复核；完成条件：报告完整且无未解决阻断项，记录 Agent ID、版本与隔离方式。

## 4. Integration Readiness

- [x] 4.1 （仅一次，不随单元复制）单独创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、计划/契约、源提交及验证与 review 证据、目标分支与授权边界（用户已授权本地 main 直接提交；无独立集成 worktree，按 计划 的 Merge Strategy）；完成条件：记录实际 Agent ID、上下文方式与交接清单；缺少独立执行能力时该任务 BLOCKED。
- [x] 4.2 复核 DU1 的 integrated 模式与 WP1/WP2 组成，核对 LC1、PV1–PV5 与 RV1 的有效证据；完成条件：确认组合 verify/review 结果可用，变化时先同步计划与依赖。

## 5. Merge Unit

- [x] 5.1 核实目标仓库与 `refs/heads/main` 当前提交并记录准确引用与核实证据；完成条件：记录提交 SHA、核实时间与命令输出；无法确认时保持 BLOCKED。
- [x] 5.2 基于已核实基线构造本单元候选，固定基线与候选版本，记录组成与构建结果；完成条件：候选 build 通过且版本可追溯。
- [x] 5.3 在候选版本上执行，将版本、范围、结果与复用依据关联到 `验收记录`；完成条件：候选 全绿并留证。
- [x] 5.4 只读检视固定候选的 diff 与契约一致性，修复后独立复核；完成条件：无未解决阻断项并记录隔离设置、版本与报告。
- [x] 5.5 [DU1；mode = not-applicable] 核对 Main E2E 不适用的理由与依据，并确认替代验证（LC1 + PV1–PV5）已执行且证据有效；完成条件：替代检查全部通过，`验收记录` 记 NOT_APPLICABLE 及依据。
- [x] 5.6 按 计划 的 Authorization（用户 2026-09-23 本会话授权「直接本地合并」）在本地 `refs/heads/main` 直接提交已验证候选（不含推送/PR/发布）；提交前核对基线与候选一致性，提交后报告实际 SHA；完成条件：记录实际提交、授权来源与包含范围。
- [x] 5.7 核对实际主分支结果与候选的一致性，完成计划内必要的 回归；有效复用逐项记录原证据与适用性；完成条件：主分支 全绿并留证。
- [x] 5.8 独立检视合并新增差异；无新增差异时由主 Agent 记录依据及原 RV1，不强制同范围重复审查；完成条件：无未解决阻断项并留证。

## 6. Final E2E

- [x] 6.1 执行 Main E2E 的替代验证：LC1 与 （在固定的最终主分支版本上），逐项记录命令、版本、退出码与日志；完成条件：全部替代检查通过并留证。
- [x] 6.2 汇总替代证据、核对覆盖范围并确认测试临时资源已清理；完成条件：`验收记录` 的替代验证结论完整且资源清理有记录。
- [x] 6.3 运行 `变更校验`，仅确认不适用判据并按结果自动勾选/回退本行；完成条件：门禁 PASS；主 Agent 不得手勾或手动回退本行。

## 7. Final Verification

- [x] 7.1 执行最终验收，核对用户意图、既有契约、design、tasks 与最终主分支证据；完成条件：全部通过并记录 PASS 结论（未获合入授权时如实记 BLOCKED，不报告可归档）。
