<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 使用新的任务级最小上下文核实仓库、目标引用、工具版本、约定命令及资源状态，返回结构化事实（原始命令 + 输出）和证据
- [x] 1.2 WP1–WP4 确认实现所需契约（`design.md@D1–D9`）已冻结、文件归属与合并负责人已登记（plan.md 的 Shared File Ownership），记录各自编码起点；契约已冻结即可开工，不等待上游实现
- [x] 1.3 按 plan.md 的资源判定分配/隔离资源（每个 WP 一个 `CARGO_TARGET_DIR`、确认 `node_modules` 就绪），创建并分配 worktree，验证配置并记录释放方法，按 (WP, Attempt) 返回结构化交接记录由 main 校验后写入 verification 的 `## Worktree Handoff`
- [x] 1.4 WP4 在集成验证前接入已验收的 WP3 交付提交，核对 `crates/core` 端口形状的实际基线与包含关系，关联 verification 的交接证据；无 `code:WP3` 则不得开工
- [x] 1.5 全变更：确认上游契约（`design.md@D1–D9`）已冻结并固定路径与版本，登记 plan.md 的 Shared File Ownership；契约由本变更自身产出，冻结动作先于任何 WP 派发

## 2. Implementation

- [x] 2.1 [wp:WP1] [PV1] 派发 WP1 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change sync-workspaces-and-create --wp WP1 --executor <ID> --role coder`，交付后销毁并起 reviewer，review 通过后由主 Agent 勾选
      - WP1：把 `session.create` 加入 Sync 词表族（commands.json 的 transport/pack、两份协议 schema、feature 登记、`identity-auth` 的 `PACKS` 与测试、命令与 feature 协商 fixture **及其 `manifest.json` 条目**），同步 `crates/node-link-protocol/src/command.rs` 的命令计数注释与 SYNC §5.2/§11.3/§11.5/§16.2、SECURITY §9/§10.2/§11.2、FRONTEND §2.2/§4.1/§7/§9 第 1 条、DEVELOPMENT_PLAN、README；依赖：无（契约冻结于 design D3/D4/D5/D7/D8）；负责人 coder-vocab；局部验证：`node scripts/check-command-catalog.mjs`、`node scripts/check-features.mjs`、`node scripts/check-schema-fixtures.mjs`、`cargo test -p sync-protocol -p identity-auth`
- [x] 2.2 [wp:WP2] [PV1] 派发 WP2 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change sync-workspaces-and-create --wp WP2 --executor <ID> --role coder` 登记状态（coding）；依赖已冻结、资源就绪时开工，不等其他工作包
      - WP2：实现目录引用与两个快照资源的 wire 形状（`common.schema.json` 的 `workspaceRef` 与可选 `workspace`、`sync.schema.json` 的 `workspaces`/`agents` 资源与 item、`sync-protocol` 的 `SnapshotResource`/`SnapshotItems`/`SessionSummary` 变体、快照 fixture **及其 `manifest.json` 条目**），并修复 wire 类型变更在 Node Link 投影处的构造点（`crates/server/src/node_link/command.rs`、`resource.rs` 写 `workspace: None`），同步 SYNC §9.4/§9.6/§10.3；依赖：无（契约冻结于 design D1/D2）；负责人 coder-wire；局部验证：`cargo test -p sync-protocol`、`cargo clippy --locked -p sync-protocol -p server --all-targets --all-features -- -D warnings`、`npm run check`（含 fixture 覆盖门禁）
- [x] 2.3 [wp:WP3] [PV1] 派发 WP3 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change sync-workspaces-and-create --wp WP3 --executor <ID> --role coder` 登记状态（coding）
      - WP3：在 core 增加 `WorkspaceRef` 值对象、`SessionSummary.workspace`、`SessionUpdate.workspace_alias` 与 `create_session` 的写入，修复 core 接口变更波及的调用点（`crates/server/src/node_link/command/tests.rs`、`resource/tests.rs`，以及条件性的 `crates/app/**`），同步 `docs/CORE_PORTS_AND_STORAGE.md` §3.1/§3.6/§5 与 `docs/MODULE_ARCHITECTURE.md` §4.1 值对象清单；依赖：无（契约冻结于 design D1/D6）；负责人 coder-core；局部验证：`cargo test -p core`、`cargo clippy --locked -p core -p server --all-targets --all-features -- -D warnings`、`node scripts/check-contract-drift.mjs`（§5 部分）。**注意**：本 WP 落地到 WP4 交付之间 `crates/storage-sqlite` 处于已登记的预期红窗口，分支阶段不跑 workspace 级 clippy/test
- [x] 2.4 [wp:WP4] [PV1] 派发 WP4 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change sync-workspaces-and-create --wp WP4 --executor <ID> --role coder` 登记状态（coding）；必须等 WP3 进入已验收集成基线
      - WP4：文件格式与 owned 家族升到 v6（`ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT`）、会话读写的别名列与投影期 JOIN（`owned_workspace.display_name`，缺失时回退别名），同步 `docs/CORE_PORTS_AND_STORAGE.md` §7 标题/§7.2/§7.3、§9 判据 1/28 的版本链、§11.3 的「过新」取值与文件头版本记录，以及迁移测试；依赖：`code:WP3`；负责人 coder-storage；局部验证：`cargo test -p storage-sqlite`、`cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings`、`node scripts/check-contract-drift.mjs`（§7 部分）

## 3. Branch Validation

- [x] 3.1 WP1–WP4 按计划 Check ID PV1 完成交付前 project verify（各自 crate/脚本的定向子集，或在集成基线上的完整 PV1），逐项记录完整命令、版本/配置、实际资源、退出码及日志或有效复用依据，完成后核实释放检查资源
- [x] 3.2 [CR1] WP1 新建不继承实现对话的只读 reviewer 子 Agent 检视同一版本（与 coder 同一窗口，coder 已销毁），核对词表六处一致、feature 登记三方一致、文档副作用句子已落；修复由新的实现实例承担，修复后由新子 Agent 复核；记录 Agent ID、版本、隔离方式和报告，可与 3.1 并行
- [x] 3.3 [CR2] WP2 独立 reviewer 只读检视同一版本：schema 与 Rust 枚举双向一致、`workspace` 为可选字段、快照资源的 feature 门控与授权过滤、schema/文档/fixture 中无规范化路径；修复后独立复核并记录证据
- [x] 3.4 [CR3] WP3 独立 reviewer 只读检视同一版本：`WorkspaceRef` 与 `SessionSummary.workspace` 的构造约束、`workspace_alias` 为窄写入且不进恢复路径、§5 与 `ports.rs` 逐项一致；修复后独立复核并记录证据
- [x] 3.5 [CR4] WP4 独立 reviewer 只读检视同一版本：v6 只做追加、旧行为 `NULL`、JOIN 回退别名、§7 与 `migrate.rs` 逐字一致、旧库升级测试覆盖连续段；修复后独立复核并记录证据

## 4. Integration Readiness

- [x] 4.1 （仅一次，不随单元复制）主 Agent 单独创建独立合入 Agent，显式交接 roles/merger.md 全文、计划/契约、源提交及证据、U1 复用的执行 worktree、本地主分支及合入条件，记录实际 ID 及上下文方式；主 Agent 不兼任
- [x] 4.2 U1（主 Agent）复核该单元预定模式（integrated）及 WP1–WP4 组成，核对对应交付检查（PV1）与独立 review（CR1–CR4）的有效证据已进入集成基线；变化先同步计划和依赖

## 5. Merge Unit

- [x] 5.1 U1（合入负责人 merger-1；依赖该单元就绪）由 merger 在构建候选前机械核实目标仓库及主分支当前提交，记录准确引用及核实证据；无法确认目标时保持 BLOCKED
- [x] 5.2 U1（merger-1；依赖 5.1）基于已核实基线构造单元候选（含 WP1–WP4 全部已验收提交），固定基线和候选版本，记录组成与构建结果
- [x] 5.3 U1（检查执行者；依赖 5.2）按计划 Check ID PV1 完成候选 Project Verify（`npm run verify`），将版本、范围、结果及有效复用依据关联到 verification，证据 `reports/PV1.log`
- [x] 5.4 U1（独立 reviewer；依赖 5.2，可与 5.3 并行）只读检视固定候选新增交互和冲突解决（重点：`SYNC_PROTOCOL.md`、`fixtures/sync/v1/`、`CORE_PORTS_AND_STORAGE.md` 三处共享文件的合并结果），修复后独立复核，记录隔离设置、版本及报告
- [x] 5.5 U1（主 Agent；依赖 5.3、5.4）核对 plan.md 的 Coverage Index：每个需求/场景行都有实现证据与检查证据；未覆盖的行不得勾选，重开受影响任务
- [x] 5.6 U1（merger-1；依赖 5.5）先以 `agentic-premerge` 块运行 `workflow check --stage premerge` 并取得 PASS，再把该块持久化为版本化 receipt、在 verification 的 `## Premerge History` 记一行，确认候选检查、独立 review、覆盖核对均通过并核实仓库规则与本地主分支基线；以条件更新或串行合并机制防止竞态，直接合入计划中的本地主分支，记录实际提交
- [x] 5.7 U1（检查执行者；依赖 5.6）核对实际主分支结果与候选一致性，完成计划内必要 project verify 回归；有效复用逐项记录原证据及适用性
- [x] 5.8 U1（独立 reviewer；依赖 5.6）独立检视合并新增差异；无新增差异由主 Agent 记录依据及原 review ID，可与 5.7 并行

## 6. Independent Validation

- [x] 6.1 [validation] 全变更、validator：按 plan.md 的 `## Independent Validation` 表在最终主分支固定版本执行验证（覆盖充分性、feature 门控与 `NULL` 语义等假设判定、风险盲区），返回隔离方式与独立报告路径 `reports/validation-sync-workspaces-and-create.md`；必须早于 7.3 完成

## 7. Final E2E

- [x] 7.1 [ALT1] 全变更、主 Agent：在最终主分支固定提交上运行 `npm run check`（十项合同门禁）并逐项记录退出码与输出，证据 `reports/ALT1.log`；全部通过才算完成
- [x] 7.2 [ALT2] 全变更、主 Agent：在最终主分支固定提交上运行 `cargo test --locked --workspace --all-features`，逐项记录结果与范围，证据 `reports/ALT2.log`；全部通过才算完成
- [x] 7.3 [e2e-owned] 全变更、扩展；依赖 7.1、7.2：运行 `openspec-agentic e2e check --change sync-workspaces-and-create`，仅 PASS 自动勾选；此行只检查门禁，不执行测试或汇总

## 8. Final Verification

- [x] 8.1 [final-verification] 使用 agentic-verify 执行最终验收（/opsx:verify 同样读取该入口），核对用户意图、需求、设计、计划、任务与最终主分支证据；记录当前 agentic-assessment 后运行 `workflow check --stage final`，全部通过才完成
