## 1. 准备与契约登记

- [x] 1.1 全变更；environment/recon 执行者。在**新的任务级最小上下文**中核实：仓库绝对路径与主分支引用（`git rev-parse refs/heads/main`、`git status --porcelain`）、工具链版本（`rust-toolchain.toml`、`cargo --version`）、Node 版本、`npm ci` 状态、既有门禁与 `cargo test --locked --workspace --all-features` 的基线结果、`crates/{core,storage-sqlite,server,app}` 与本变更相关的实际现状。完成条件：返回结构化事实（命令、退出码、关键输出）并写入 `verification.md` 的运行时基线与 Target；基线为红时如实记录并告知主 Agent，不开始实现。

- [x] 1.2 全变更；主 Agent。把本变更的合同变化登记进 `verification.md` 的 Check Plan Changes：`docs/NODE_LINK_PROTOCOL.md` §8.2（信任记录增 `exportIds`、可见性三条件、撤销语义、无修改入口）、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.4（`node.pair.confirm` 增必填 `exportIds`，按 §8 记 v1 内合同修订）、`docs/CORE_PORTS_AND_STORAGE.md` §3.5/§7.2/§7.3/§9 判据 28（v4 迁移与常量 4/4/3）。完成条件：每条含原/新值、理由、影响与受影响的下游任务；`schemas/`、`fixtures/local-admin/v1/`、`commands.json` 不改的依据已写明。

## 2. 实现（WP1–WP5）

- [ ] 2.1 WP1；前置：1.2；实现 Agent（coder）。storage-sqlite：`FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION` → 4（`IMPORTED_SCHEMA_VERSION` 保持 3）；§7.3 的 `owned_node` 建表常量**末尾**追加 `export_ids_json TEXT NOT NULL DEFAULT '[]'`；新增 v3 → v4 升级段（`ALTER TABLE owned_node ADD COLUMN …`，不自作 12-step 重建）；节点行读写编解码该列（去重 + 字典序，空集合固定 `'[]'`）；`settle_pairing` 在**同一事务内**校验清单（不存在/已撤销 → `NotFound(EntityRef::Export)`；与本次 `granted_grants` 无交集 → `InvalidRequest`；空集合合法），失败零写入、零审计。完成条件：[PV3] 覆盖 R11–R15、R6–R9 的落盘面；把「升级库列清单 == 新建库列清单」断言从 `imported_*` 扩到 `owned_*`；迁移/写集用例全绿并写入 `reports/wp1-storage.log`。

- [ ] 2.2 WP2；前置：1.2；实现 Agent（coder）。core：`NodeRecord` 增 `export_ids: Vec<ExportId>`（构造校验去重/排序/空集合合法）；`PairingSettlement::Approved` 增 `granted_export_ids`；Owner 侧授权判定（`Broker::node_allowed` / `UseCases` 的 `Actor::Node` 分支）增加"目标 Export 必须在该节点 `export_ids` 内"的条件，与目录层同口径；`LocalCli` 既有路径行为不变。完成条件：[PV1] 覆盖 R1–R4、R6–R9 的判定面；核心断言「清单为空时即使 `scopes ∩ grants` 非空也不可用」有用例；`cargo test -p core --all-features` 全绿并写入 `reports/wp2-core.log`。

- [ ] 2.3 WP3；前置：2.2；实现 Agent（coder）。server：`node_link::catalog::visible_exports` 增加第三个条件（`export.exportId ∈ node.export_ids`），保持它是**唯一**判定点，`resource`/事件/ACK 的归属复核继续复用它；空清单返回空可见集；清单内 Export 被撤销后不可见且清单条目不被级联清理；`crates/server/src/local_admin/params.rs` 的 `NodePairConfirm` 增必填 `exportIds`（`reject_unknown_fields` 白名单同步，缺字段/非字符串数组 → `local.invalid_params`）并在 router 里透传为 `PairingSettlement::Approved` 的 `granted_export_ids`（§5.4 的「必填」由运行期强制，JSON Schema 不表达）。完成条件：[PV4] 覆盖 R1–R5 与 local-admin 的「缺少该字段」分支；catalog 与 `resource.attach` 结论一致有回归断言；用例写入 `reports/wp3-server.log`。

- [ ] 2.4 WP4；前置：2.2；实现 Agent（coder）。app：CLI `node pair confirm` 增加可重复 `--export-id`（零次 = 空清单）并在零次时打印醒目警告但照常执行（不引入新开关）；confirm 结果与 `node list` 展示 `exportIds`（空清单如实显示）；CLI 不做业务判定。完成条件：[PV4] 覆盖 R7/R8 的 CLI 面；参数映射与警告有用例；`cargo test -p app --all-features` 全绿并写入 `reports/wp4-app.log`。

- [ ] 2.5 WP5；前置：1.2；实现 Agent（coder）；主 Agent 核对。同步权威文档：`NODE_LINK_PROTOCOL.md` §8.2（信任记录形状、三条件、撤销不级联、升级副作用、无修改入口的已知限制）+ 文件头修订记录；`CORE_PORTS_AND_STORAGE.md` §3.5（`NodeRecord`/`PairingSettlement`）、§7.2（v4 段与常量 4/4/3）、§7.3（`owned_node` DDL）、§9 判据 28、§11.6 第 4 条、§11.7/§11.8 说明句；`LOCAL_ADMIN_PROTOCOL.md` §5.4（`NodeRecord.exportIds`、confirm 的 params/result）+ 修订记录；顺带修正 `openspec/specs/storage-schema-v2-migration/spec.md` 的 Purpose（原文只写"v1 到 v2"，已过期）。完成条件：[PV1] 的 `check:docs`/`check:drift` 通过；文档与代码/DLL 文本一致；写入 `reports/wp5-docs.log`。

## 3. 交付前验证与独立审查

- [ ] 3.1 全变更；前置：2.1–2.5；实现 Agent（coder）。交付前 project verify：`npm run verify`（[PV1]，含全部门禁与 workspace 测试）、`node scripts/check-crate-boundaries.mjs`（[PV2]）、`cargo test --locked -p storage-sqlite -p core --all-features`（[PV3]）、`cargo test --locked -p server -p app --all-features`（[PV4]），逐项记录完整命令、工具链版本、退出码与日志。完成条件：四项通过、无零用例/全跳过；覆盖索引 18 行（R1–R18）逐行有可读证据；日志写入 `reports/du1-pv1.log`。

- [ ] 3.2 WP1–WP4；前置：2.1–2.4；独立 reviewer。新建**不继承实现对话**的只读 reviewer 子 Agent，按 `roles/reviewer.md` 检视实现：可见性两处必须同口径（清单为空既看不到也不可用）、迁移不得默认放权且升级库与新建库列清单相等、`settle_pairing` 的校验与写入同事务且失败零写入/零审计、CLI 警告不阻断且不引入新开关、无 `unwrap`/detached task 类问题。完成条件：`reports/rv1-impl.md` 记录 Agent ID、固定版本、隔离方式、逐项结论；CRITICAL/MAJOR 修复后由新子 Agent 复核。

- [ ] 3.3 WP5；前置：2.5；独立 reviewer。隔离检视文档一致性：§8.2 的可见性三条件与撤销语义、§5.4 的参数必填与失败码、§7.2/§7.3 的迁移与 DDL 文本、§9 判据 28 的表述是否与实现和门禁一致；「已知限制」（无修改入口、升级副作用、Access 侧孤儿 Import）是否写明。完成条件：`reports/rv1-docs.md`；阻断项修复后复核。

## 5. 就绪复核

- [ ] 5.1 DU1；前置：3.1–3.3；主 Agent（机械核实可交 environment/recon）。复核 DU1 的预定模式（`integrated`）与组成（WP1–WP5），核对 [PV1]–[PV5] 与 RV1 的有效证据，确认无未解决阻断项与未登记漂移。完成条件：计划与依赖已同步（无待更新项）、就绪判据全部满足；变化先同步计划与依赖再进入第 6 组。

## 6. 候选与合入

- [ ] 6.1 DU1；主 Agent；前置：5.1。按计划核实目标仓库与主分支当前提交（`git rev-parse refs/heads/main` 等），记录准确引用及核实证据；无法确认目标时保持 BLOCKED。

- [ ] 6.2 DU1；集成负责人；前置：6.1。在独立集成 worktree 基于已核实基线构造 DU1 候选，固定基线与候选版本，记录组成与构建结果。

- [ ] 6.3 DU1；检查执行者；前置：6.2。完成候选 Project Verify：[PV1]、[PV2]，并把版本、范围、结果及有效复用依据关联到 `verification.md`。

- [ ] 6.4 DU1；独立 reviewer；前置：6.2，可与 6.3 并行。只读检视固定候选的新增交互与冲突解决，修复后独立复核，记录隔离设置、版本及报告（`reports/rv5-impl.md`）。

- [ ] 6.5 DU1；前置：固定候选及所需资源。E2E `not-applicable`：核对计划中的理由、依据与 `downgrade_approval` 记录有效，并完成适用替代检查的候选轮次——在候选上执行 [PV5] 受控路径全链路（`cargo test --locked -p server -p app --all-features`，本机 Windows）并把原始日志写入 `reports/pv5-windows-nodelink.log`；连续失败计入 `x-agentic.e2e.maxAttempts`（默认 3），达到后停止自动重跑并交用户决策。

- [ ] 6.6 DU1；合并负责人；前置：6.3、6.4、6.5。确认候选检查、独立 review 与替代验证通过，核实仓库规则与本地主分支基线，并运行 `npx --quiet --no-install openspec-agentic workflow check --change node-trust-export-ids --stage premerge --planning-root <权威规划根> --json`（非 PASS 不合入）；以条件更新或串行合并机制防止竞态，合入 `refs/heads/main`，记录实际提交；随后按 `AGENTS.md` §8 通过 PR 交付 `origin/main`，基线变化时重开受影响候选任务。

- [ ] 6.7 DU1；检查执行者；前置：6.6。核对实际主分支结果与候选一致性，完成 [PV1]/[PV2] 主分支回归；有效复用逐项记录原证据及适用性。

- [ ] 6.8 DU1；独立 reviewer；前置：6.6。独立检视合并新增差异；无新增差异由主 Agent 记录依据及原 review ID，不强制同范围重复审查，可与 6.7 并行。

## 7. 最终验证

- [ ] 7.1 全变更；主 Agent；前置：主分支检查（6.7、6.8）。执行最终替代验证（`not-applicable` 路径）：在最终主分支版本上重跑 [PV1]（`npm run verify`）、[PV3]（`cargo test -p storage-sqlite -p core --all-features`）、[PV4]/[PV5]（`cargo test -p server -p app --all-features` 含受控路径全链路），逐项记录命令、版本、退出码与日志路径（`reports/du1-pv1.log`、`reports/pv5-windows-nodelink.log`），并核实临时资源已清理。

- [ ] 7.2 全变更；主 Agent；前置：7.1。汇总替代验证证据：核对 `not-applicable` 的理由/依据/`downgrade_approval` 三要素仍有效、Coverage Index 的 R1–R18 全部有有效证据、无未解决 FAIL/BLOCKED；全部必要检查通过后完成。

- [ ] 7.3 [e2e-owned] 全变更；扩展；前置：7.2。运行 `npx --quiet --no-install openspec-agentic e2e check --change node-trust-export-ids`，仅 PASS 自动勾选；此行只检查门禁（不适用判据已按计划固化且替代验证已完成），不执行测试或汇总。

- [ ] 8.1 [final-verification] 使用 agentic-verify 执行最终验收（`/opsx:verify` 同样读取该入口），核对用户意图（2026-09-27 的四项裁决与三项小决策）、需求（R1–R15）、设计（D1–D10）、计划、任务与最终主分支证据；记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change node-trust-export-ids --stage final --json`，全部通过才完成；验收结论单独报告 PASS / FAIL / BLOCKED，不以 CLI 的 `all_done` 代替。
