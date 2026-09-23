<!-- 分组标题使用英文，任务正文使用中文。保留 - [ ] X.Y 格式，每项写明完成条件。
     Main E2E 为 not-applicable，故不生成 Test Design and Authoring 分组，并对后续分组重编号。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 [全变更] 负责人：environment/recon（新任务级最小上下文）；依赖：无；核实目标仓库绝对路径与 `refs/heads/main` 当前提交、工作区是否干净、`rust-toolchain.toml` 与 Node 版本、以及 `cargo test -p storage-sqlite --test admin_store --test imported` 是否可运行；完成条件：返回结构化事实与证据（命令、输出、时间）并记录到 `verification.md`，无法运行的原因如实写明。
- [x] 1.2 [WP1、WP2、WP3] 负责人：主 Agent；依赖：1.1；确认契约与文件所有权：行为契约 = 本变更 `specs/admin-state-persistence/spec.md` 与 `specs/peer-identity-material/spec.md` 的 `## ADDED Requirements`，技术决策 = `design.md` 的 D1–D6；WP1 独占 `crates/storage-sqlite/src/admin/trust.rs`、`src/admin/export.rs`、`tests/admin_store.rs`，WP2 独占 `src/session_store.rs`、`tests/imported.rs`，WP3 独占 `docs/CORE_PORTS_AND_STORAGE.md`；完成条件：记录编码起点（具体提交）与「不新增 ConflictKind/错误码、不改 DDL/端口签名/wire」的边界。
- [x] 1.3 [全变更] 负责人：主 Agent；依赖：1.1；确认共享运行资源与隔离方式：`target/` 按执行者设 `CARGO_TARGET_DIR`，SQLite 临时目录由 `support::temp_dir`（含进程 id）隔离，本变更不读写 `fixtures/`；完成条件：在 `verification.md` 记录依据与清理方式。
- [x] 1.4 [全变更] 负责人：主 Agent；依赖：无；记录本变更无跨变更上游代码依赖（单仓库单 crate 守卫补丁），WP3 的文档文本仅依赖 design 决策；完成条件：在 `verification.md` 记录不适用依据与「三条实现轨道可并行」的结论。

## 2. Implementation

- [x] 2.1 [WP1] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：1.2；按 design D2 修改 `crates/storage-sqlite/src/admin/export.rs` 的 `put_export`：传入记录 `revoked_at` 非空 → `InvalidRequest`（零写入）；库内已撤销且传入未撤销 → `Conflict(ConflictKind::AlreadyExists)`；`DO UPDATE` 的该列改为保留已存非空值的 `CASE`；完成条件：`cargo test -p storage-sqlite --test admin_store` 通过，且 diff 内无 DDL/端口签名与其它无关改动，由 [PV3] 覆盖；证据写入 `reports/lc1-admin-store.log`。
- [x] 2.2 [WP1] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：1.2；按 design D3 在 `crates/storage-sqlite/src/admin/trust.rs` 加入同行指纹核对：`load_peer_key`（`owned_peer_key`）、`pairing_peer_from_row`（`owned_pairing_peer`）、`device_from_row`（把 `public_key` 加入 `DEVICE_COLUMNS`，`device()`/`devices()` 共用）、`load_existing_device_key`，不一致一律 `StorageError::Corrupt`；同时修正 `load_peer_key` 文档注释里「指纹列不参与判定」的口径（模块头无需改动，RV1-F2）；完成条件：`cargo test -p storage-sqlite --test admin_store` 通过且单读/列表读两条路径都被覆盖，由 [PV3] 覆盖；证据写入 `reports/lc1-admin-store.log`。
- [x] 2.3 [WP1] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：1.2；按 design D4 把 `upsert_device.last_seen_at` 与 `upsert_node.last_connected_at` 的 `DO UPDATE` 改为显式 `CASE`（新值为空保留旧值、旧值为空写入新值、两者非空取较大者），不得使用标量 `MAX`/`COALESCE` 组合；完成条件：`cargo test -p storage-sqlite --test admin_store` 通过（含 2.4 的三方用例），由 [PV3] 覆盖；证据写入 `reports/lc1-admin-store.log`。
- [x] 2.4 [WP1] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：2.1、2.2、2.3；按 design D5 在 `crates/storage-sqlite/tests/admin_store.rs` 增加三组回归：Export 撤销终态（清除撤销 → `Conflict(AlreadyExists)` 且首次 `revoked_at` 不变；携带已撤销记录 → `InvalidRequest` 且零写入）、身份材料读取失败关闭（改写 `owned_peer_key`/`owned_device`/`owned_pairing_peer` 的 `fingerprint` 列 → 对应读取返回 `Corrupt`，并保留「一致时读取照常」的正向断言）、活动时间三方（空→Some 落库、Some(晚)→Some(早) 保留晚值、Some→None 保留 Some，设备与节点各一遍，并断言同写集其它字段仍更新）；完成条件：`cargo test -p storage-sqlite --test admin_store` 通过且新增用例确实执行，由 [PV3] 覆盖；证据写入 `reports/lc1-admin-store.log`。
- [x] 2.5 [WP2] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：1.2；按 design D1 在 `crates/storage-sqlite/src/session_store.rs` 的 `upsert_session` 与 `commit_receipt` 的 `BEGIN IMMEDIATE` 事务内、任何写入之前先校验 `(owner_node_id, export_id)` 在 `imported_import_export` 中仍有归属，缺失即 `NotFound(EntityRef::Export(export_id))` 并零写入（不推进 `local_sequence`、不建会话行与索引）；同步修正该写路径的注释措辞；完成条件：`cargo test -p storage-sqlite --test imported` 通过，由 [PV3] 覆盖；证据写入 `reports/lc1-imported.log`。
- [x] 2.6 [WP2] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：2.5；按 design D5 在 `crates/storage-sqlite/tests/imported.rs` 增加回归：建立 import + 会话行 + 交付索引基线 → `remove_import` → 同一 `RemoteSessionRef` 的 `upsert_session` 与 `commit_receipt` 都断言 `NotFound(Export)`，并用 raw pool 断言三张 imported 表为空、`imported_audit` 计数不变；完成条件：`cargo test -p storage-sqlite --test imported` 通过，由 [PV3] 覆盖；证据写入 `reports/lc1-imported.log`。
- [x] 2.7 [WP3] 负责人：实现 Agent（完整读取 `roles/coder.md`）；依赖：1.2；按 design D6 同步 `docs/CORE_PORTS_AND_STORAGE.md`：§5.2 约束（imported 写路径归属前置与 `NotFound(Export)`）、§5.3 约束（`put_export` 撤销终态、身份材料读取核对、活动时间单调）、§7.4 `[决定]`（失去 Import 的写入必须被拒绝 + 重导入后无法区分新旧连接的显式注记）、§9 新增判据 30、§11.2 第 5 条的端口级细化；**不得**改动 §5/§7 的任何 fenced 代码块、既有判据编号与措辞；完成条件：`npm run check` 全绿（尤其 `check:contract-drift` 与文档引用门禁），由 [PV4] 覆盖；证据写入 `reports/pv4-check.log`。

## 3. Branch Validation

- [x] 3.1 [WP1、WP2、WP3] 负责人：主 Agent；依赖：2.4、2.6、2.7；在固定提交上执行 LC1 与 [PV1][PV2][PV3][PV4][PV5]，逐项记录完整命令、工具链/Node 版本、退出码与日志，并核实临时资源已释放；完成条件：LC1 与 [PV1][PV2][PV3][PV4][PV5] 全绿并留证，`cargo-deny`/`gitleaks` 如实记为「未在本地执行」，Coverage Index 的 12 行都有可读断言证据。
- [x] 3.2 [WP1、WP2、WP3] 负责人：新建只读 reviewer 子 Agent（不继承实现对话，完整读取 `roles/reviewer.md`）；依赖：2.4、2.6、2.7，可与 3.1 并行；按 plan.md 的 RV1 关注点检视固定版本的实际 diff（判定顺序与失败关闭先于写入、错误变体、三条身份材料读路径、`CASE` 三分支、文档是否避开 fenced 块、有无越界改动与同源手抄期望值）；修复后由新子 Agent 复核；完成条件：报告完整且无未解决阻断项，记录 Agent ID、版本与隔离方式。
- [x] 3.3 [WP1、WP2、WP3] 负责人：主 Agent；依赖：3.1、3.2；按 `workflow check --stage plan`（若尚未执行）与 `npx --quiet --no-install openspec-agentic workflow check --change fix-admin-store-integrity-gaps --stage plan --json` 复核意图字段、覆盖引用与唯一门禁任务；完成条件：plan 检查 PASS 或缺口已修正并留证。

## 4. Integration Readiness

- [x] 4.1 [全变更]（仅一次，不随单元复制）负责人：主 Agent；依赖：3.1、3.2、3.3；单独创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、计划/契约、源提交及验证与 review 证据、目标分支与授权边界（用户已授权本地合入 `refs/heads/main`，不含推送/PR/发布/归档）；完成条件：记录实际 Agent ID、上下文方式与交接清单；缺少独立执行能力时该任务 BLOCKED。
- [x] 4.2 [DU1] 负责人：主 Agent；依赖：4.1；复核 DU1 的 integrated 模式与 WP1/WP2/WP3 组成，核对 LC1、PV1–PV5 与 RV1 的有效证据；完成条件：确认组合 verify/review 结果可用，变化时先同步计划与依赖。

## 5. Merge Unit

- [x] 5.1 [DU1] 负责人：主 Agent（机械核实可交 environment/recon）；依赖：4.2；核实目标仓库与 `refs/heads/main` 当前提交并记录准确引用与核实证据；完成条件：记录提交 SHA、核实时间与命令输出；无法确认时保持 BLOCKED。
- [x] 5.2 [DU1] 负责人：集成 Agent；依赖：5.1；基于已核实基线构造本单元候选，固定基线与候选版本，记录组成与构建结果；完成条件：候选 build 通过且版本可追溯。
- [x] 5.3 [DU1] 负责人：检查执行者；依赖：5.2；在候选版本上执行 [PV1][PV2][PV3][PV4][PV5] 与 LC1，将版本、范围、结果与复用依据关联到 `verification.md`；完成条件：候选 [PV1][PV2][PV3][PV4][PV5] 与 LC1 全绿并留证。
- [x] 5.4 [DU1] 负责人：独立 reviewer（`roles/reviewer.md`）；依赖：5.2，可与 5.3 并行；只读检视固定候选的 diff 与契约一致性，修复后独立复核；完成条件：无未解决阻断项并记录隔离设置、版本与报告。
- [x] 5.5 [DU1；mode = not-applicable] 负责人：主 Agent；依赖：5.2；核对 Main E2E 不适用的理由与依据，并确认替代验证（LC1 + PV1–PV5）已执行且证据有效；完成条件：替代检查全部通过，`verification.md` 记 NOT_APPLICABLE 及依据。
- [x] 5.6 [DU1] 负责人：独立 integrator；依赖：5.3、5.4、5.5；按 plan.md 的 Authorization（用户 2026-09-23 本会话选 (b)：授权本地合入 `refs/heads/main`，不含推送/PR/发布/归档）在本地 `refs/heads/main` 提交已验证候选；提交前核对基线与候选一致性，提交后报告实际 SHA；完成条件：记录实际提交、授权来源与包含范围。
- [x] 5.7 [DU1] 负责人：检查执行者；依赖：5.6；核对实际主分支结果与候选的一致性，在 `main` 上完成 [PV1][PV2][PV3][PV4][PV5] 与 LC1 回归；有效复用逐项记录原证据与适用性；完成条件：主分支 [PV1][PV2][PV3][PV4][PV5] 与 LC1 全绿并留证。
- [x] 5.8 [DU1] 负责人：独立 reviewer；依赖：5.6，可与 5.7 并行；独立检视合并新增差异；无新增差异时由主 Agent 记录依据及原 RV1，不强制同范围重复审查；完成条件：无未解决阻断项并留证。

## 6. Final E2E

- [x] 6.1 [全变更] 负责人：主 Agent；依赖：5.7、5.8；执行 Main E2E 的替代验证：在固定的最终主分支版本上运行 LC1 与 [PV1][PV2][PV3][PV4][PV5]，逐项记录命令、版本、退出码与日志；完成条件：全部替代检查通过并留证。
- [x] 6.2 [全变更] 负责人：主 Agent；依赖：6.1；汇总替代证据、核对 Coverage Index 的 12 行覆盖与重导入残留缺口的登记，并确认测试临时资源与构建目录已清理；完成条件：`verification.md` 的替代验证结论完整且资源清理有记录。
- [x] 6.3 [e2e-owned] [全变更] 负责人：扩展；依赖：6.2；运行 `npx --quiet --no-install openspec-agentic e2e check --change fix-admin-store-integrity-gaps --run-if-missing`，仅确认不适用判据并按结果自动勾选/回退本行；完成条件：门禁 PASS；主 Agent 不得手勾或手动回退本行。

## 7. Final Verification

- [x] 7.1 [final-verification] 负责人：主 Agent；依赖：6.3；按 `.agents/skills/agentic-verify/SKILL.md` 执行最终验收（`/opsx:verify` 同样读取该入口），核对用户意图（proposal 的 `agentic-intent` 四条缺陷与四点复核意见）、specs 的 12 个场景、design D1–D6、plan、tasks 与最终主分支证据，记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change fix-admin-store-integrity-gaps --stage final --json`；完成条件：全部通过并记录 PASS 结论；未通过或证据失效时保持待办并如实报告 FAIL/BLOCKED，不报告可归档。
