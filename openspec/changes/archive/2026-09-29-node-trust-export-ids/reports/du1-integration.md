# DU1 集成报告（Phase A：候选构建）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | 6.2 / 6.3 / 6.5（候选构建与候选 Project Verify；Phase A） |
| role | integrator |
| phase | candidate |
| agent_context | 独立集成子 Agent（worker/integrator），**不继承**任何实现者或 reviewer 对话；仅接收主 Agent 传入的角色指令全文与本轮输入清单。上下文方式：全新会话 + 上述输入文件/路径只读读取。工作目录 `D:/Project/acp-remote-wt/export-ids-integration`。 |
| target_revision | 候选合并提交 `64e179c618e690efcc39f64854d59861e9ecbacf` |
| scope | 只做集成：把两条已验收源分支汇入集成分支、构造固定候选、在候选树上执行计划内 Project Verify 并留证。**未**合入 `refs/heads/main`、**未**推送、**未**开 PR（Phase B）。不改代码、不改文档、不改权威 plan.md/tasks.md/verification.md。 |
| changes | 集成分支 `integration/node-trust-export-ids-du1` 上新增 2 个合并提交（见下）。工作区无未提交改动。 |
| checks | [PV1]、[PV2]、[PV3]、[PV4]、[PV5]、`check:drift`：全部 PASS（exit 0）。 |
| issues | 无阻断。ISSUE-2 的已知 flaky 用例本轮**首跑即通过**（见 §6）。 |
| result | PASS（候选阶段） |
| evidence_paths | `reports/du1-integration.log`；`reports/du1-pv1-merged.log`；`reports/pv5-windows-nodelink.log`；本报告 `reports/du1-integration.md` |
| resource_cleanup | 临时 `node_modules` 目录联接已删除并核实主检出完好；`CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-integration`（8.1 GB，本分支专用构建缓存）**有意保留**供 Phase B 复用；未创建残留临时数据目录；loopback 一律 `127.0.0.1:0`。 |

## 1. 固定提交（源 / base / 候选 / main）

| 角色 | 引用 | 完整 SHA | 备注 |
| --- | --- | --- | --- |
| 本地主分支（base） | `refs/heads/main` | `3cadb12d79456750e40644a2ea53c96380b700e6` | 与任务输入一致；本次**未被移动**（构建前/构建后均为该值） |
| 源提交 1（代码，已验收） | `agentic/node-trust-export-ids` tip | `cc6faefd27d7d58a01945971c0cc48bcafd08501` | 含 `5190924d` + `e4c980c0`/`8ae3b613`/`043b359e`/`cc6faefd` 四个 fix |
| 源提交 2（文档，已验收） | `agentic/node-trust-export-ids-docs` tip | `659e5900ecf79daa110278a07a31bef418dc5d0d` | 含 `d6e01b3b` 起 8 个提交 |
| 中间合并提交 | `integration/node-trust-export-ids-du1` | `1c632ad8308eb3a46c83306ed4fe34e942016c1a` | 汇入源提交 1 |
| **候选合并提交（固定）** | `integration/node-trust-export-ids-du1` | **`64e179c618e690efcc39f64854d59861e9ecbacf`** | 汇入源提交 2；Phase A 的候选 |

两个源分支的 merge-base 均为主分支 base `3cadb12d`（即两者都从同一 base 直接长出），因此集成是"两条平行分支汇合"，不引入 base 之外的历史。

## 2. 组成与包含关系核实

- `git worktree list` 与本分支 `git rev-parse` 确认候选落在 `D:/Project/acp-remote-wt/export-ids-integration`，分支名正确。
- `git merge-base --is-ancestor` 逐条核实（**全部为 YES**）：源提交 `cc6faefd`、源提交 `659e5900`、base `3cadb12d` 都是候选 `64e179c` 的祖先 → 两个源提交被**完整**包含。
- 范围等价核实（防静默丢失）：
  - `git diff --stat cc6faefd HEAD -- crates/` → **空**（候选中的 `crates/**` 与源提交 1 逐字节一致）。
  - `git diff --stat 659e5900 HEAD -- docs/ openspec/specs/` → **空**（候选中的 `docs/**`、`openspec/specs/**` 与源提交 2 逐字节一致）。
- 文件集不相交（源 1 只动 `crates/**`；源 2 只动 `docs/**` + `openspec/specs/storage-schema-v2-migration/spec.md`），与预期一致。

## 3. 冲突解决记录

**无冲突**（预期兑现）。两次 `git merge --no-ff` 均由 `ort` 策略自动合并，退出码 0，无冲突标记、无手工解冲突、无内容取舍。因此本报告不含需交主 Agent 决策的需求/接口取舍项。

## 4. 候选 Project Verify

执行树：`D:/Project/acp-remote-wt/export-ids-integration`（候选 `64e179c`），`CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-integration`。
工具链：`cargo 1.98.1 (797e8a9bc 2026-08-05)` / `rustc 1.98.1 (48a229cea 2026-09-01)`（= `rust-toolchain.toml` channel 1.98.1）；Node `v24.19.0` / npm `12.0.2`；OS `MINGW64_NT-10.0-26200`。

| Check ID | 命令 | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| [PV1]（合同门禁半） | `npm run check` | 0 | PASS（10/10 子门禁） | `reports/du1-integration.log` |
| [PV1]（rust 半） | `cargo fmt --all -- --check` | 0 | PASS | `reports/du1-pv1-merged.log` |
| [PV1]（rust 半） | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | PASS | `reports/du1-pv1-merged.log` |
| [PV1]（rust 半） | `cargo test --locked --workspace --all-features` | 0 | PASS：86 个 test target，**994 passed / 0 failed / 2 ignored** | `reports/du1-pv1-merged.log` |
| [PV2] | `node scripts/check-crate-boundaries.mjs` | 0 | PASS：12 个 crate 依赖方向与 §5 矩阵一致 | `reports/du1-pv1-merged.log`（另在 `du1-integration.log` 内作为 `check:boundaries` 亦通过） |
| [PV3] | `cargo test --locked -p storage-sqlite -p core --all-features` | 0 | PASS：无 0 用例 target 异常，0 failed | `reports/du1-pv1-merged.log` |
| [PV4] | `cargo test --locked -p server -p app --all-features` | 0 | PASS（与 [PV5] 同命令，见其日志） | `reports/pv5-windows-nodelink.log` |
| [PV5] | `cargo test --locked -p server -p app --all-features` | 0 | PASS：受控路径全链路通过 | `reports/pv5-windows-nodelink.log` |
| `check:drift` | `node scripts/check-contract-drift.mjs`（`npm run check` 内） | 0 | PASS | `reports/du1-integration.log` |

> 说明：`npm run verify` 的等价命令是 `npm run check` + `check:rust`（`fmt`/`clippy`/`cargo test --workspace`）。本次按 `package.json` 中 `check` 与 `check:rust` 的原始参数**逐条**执行（未另抄参数），分为两个日志留证：`du1-integration.log`（合同门禁半）与 `du1-pv1-merged.log`（rust 半）。

### 4.1 `check:drift`（本轮关键项）原始输出

该门禁在本合并树上**首次**同时具备 R11–R15 的 `owned_node` DDL 文档侧（§7.3）与代码侧（`crates/storage-sqlite/src/migrate.rs` 常量）。原始成功行（逐字）：

```
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致
```

`check:drift` 退出码 0，无差异行，**未**需要任何代码或文档改动。

### 4.2 `npm run check` 全部子门禁（`du1-integration.log` 原文摘要）

```
schema fixtures OK: 118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 12 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 380 relative links, 5131 section refs across 324 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致
agentic gate: Installation: PASS + 19 specs passed, 0 failed + agentic 宿主入口检查完成：17 个文件
EXIT_CODE=0
```

### 4.3 `check:docs` 的适用范围（重要，非缺陷）

`openspec/changes/node-trust-export-ids/`（含本报告）属主检出的**未跟踪**文件，**不在**集成分支树内。`check:docs` 只扫描本树内的 markdown（本树 324 个文件），因此本变更目录内的文档一致性**不**在本轮判定范围内——这是设计预期，不是失败。权威树（含变更目录）的全量 `check:docs` 判定由主 Agent 在合入本地 `main` 后补跑。

### 4.4 [PV5] 受控路径全链路证据

`reports/pv5-windows-nodelink.log` 中受控路径用例（本机 Windows、loopback `127.0.0.1:0`、临时数据目录）：

`crates/app/tests/node_link_e2e.rs`（3 passed / 0 failed）：
- `a_narrowing_repair_closes_the_live_attachment` ... ok ← 任务点名的收窄修复用例
- `the_controlled_path_runs_end_to_end_and_revocation_propagates` ... ok ← 配两个 Export、确认时只给一个的受控路径
- `tls_direct_terminates_the_same_handshake` ... ok

`crates/app/tests/node_pair_export_ids.rs`（2 passed / 0 failed，两条组合断言）：
- `node_pair_confirm_rejects_an_unknown_export_id_without_writing` ... ok
- `node_pair_confirm_rejects_an_export_id_disjoint_from_the_grants` ... ok

已读源码核实断言与任务描述一致：
- **收窄修复**：先以 `[EXPORT_VISIBLE]` 握手并在该 Export 上建立**活的** attachment 且已收到正文事件 → 用更窄清单（空 `[]`）重新配对同一节点身份（`nodeId` 不变、无 revoke、无新 nodeId）→ 既有连接在 10s 内被关闭且 **close code == `1000`**（非 4410）、关闭前**未**收到 `node.trust.revoked`（`client.pending()` 中无该消息）→ 同身份重新握手后 `catalog.snapshot` 的 `exports == []`。
- **两条组合断言**（`node_pair_export_ids.rs`）：均覆盖「错误分类（`local.not_found` / `local.invalid_params`）+ 零写入（不建信任记录、不写审计）+ 配对状态未推进（换合法清单仍能成功确认并回显 `exportIds`）+ 成功路径确有 `node.paired` 审计」，即"组合"而非单点断言。

命令与主体全部落在 `cargo test --locked -p server -p app --all-features`（`PV5_EXIT=0`），该命令同时构成 [PV4] 的证据（两检查计划内共用同一命令）。

## 5. 未修改项（合规确认）

- 未改任何 `crates/**`、`docs/**`、`compat*/**`、`schemas/**`、`fixtures/**`（候选树 `git status --porcelain` 为空；无暂存改动）。
- 未改权威 `plan.md` / `tasks.md` / `verification.md`。
- 未 `git push`、未切主检出分支、未合入 `refs/heads/main`、未开 PR。
- 未动另两个 worktree（`export-ids`、`export-ids-docs`）与主检出的文件（只读读取）。

## 6. ISSUE-2（已知 flaky）本轮状态

`crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`：在 `npm run check` 的 workspace 测试与 [PV5] 的 `-p app` 测试中各跑一次，**两次均首跑通过**（`... ok`，见 `du1-pv1-merged.log:814` 与 `pv5-windows-nodelink.log:112`）。未出现 panic，**未**跳过、**未** `#[ignore]`、**未**改断言，因此不触发"重跑取第二次结果"的条件。

## 7. ignored 用例说明（非"全跳过"）

workspace 测试中 2 个 ignored 均为**既有**辅助子进程/夹具生成器，且与 base `3cadb12` 上完全一致（已用 `git show 3cadb12:...` 对照），并非本变更新增的跳过：

- `crates/storage-sqlite/tests/commit.rs`：`crash_child`（由 `crash_recovery_leaves_an_consistent_database` 主动拉起）。
- `crates/storage-sqlite/tests/migration.rs`：`regenerate_v1_fixture`（手动夹具重建器）。

## 8. 资源清理

| 资源 | 处理 | 核实 |
| --- | --- | --- |
| 临时 `node_modules` 目录联接（→ `D:\Project\acp-remote\node_modules`） | 以 `cmd /c rmdir`（非递归）删除联接本身 | 集成分支树已无 `node_modules`；主检出 `node_modules/@commitlint/cli/cli.js`、`node_modules/ajv/package.json` 均在，主检出 `git status` 仅含未跟踪的权威变更目录，**主检出完好** |
| `CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-integration` | **有意保留**（8.1 GB，本分支专用构建缓存）供 Phase B 复用 | 位于专用 worktree 外，不影响任何被跟踪文件 |
| 临时数据目录 / 证书 | 由测试用例自建自删 | 集成树无残留（`git status --porcelain --ignored` 除 `node_modules`/`target` 外为空） |
| loopback | 一律 `127.0.0.1:0` 临时端口 | 符合计划 Runtime Resources |

## 9. 结论与交接

- 候选 `64e179c618e690efcc39f64854d59861e9ecbacf` 已固定；两个源提交完整包含、无冲突、无静默丢失。
- 候选 Project Verify 计划内检查（[PV1]–[PV5]，含关键 `check:drift`）**全部 PASS**。
- **候选 PASS ≠ 已合入 ≠ 最终验收 PASS**。本地 `refs/heads/main` 仍为 `3cadb12d`，未被本次集成移动。
- 本阶段不含 E2E（`not-applicable`，替代验证 = [PV5] 已在本候选执行并留证）。

### 未解决项 / 需主 Agent 处理

1. 权威树全量 `check:docs`（含 `openspec/changes/node-trust-export-ids/`）须在合入本地 `main` 后于权威树补跑——本候选树按设计不含变更目录。
2. Phase A 之后由主 Agent 调度的独立 reviewer 检视候选新增交互/冲突差异（任务 6.4）；本轮无冲突可解，交回内容仅为"两条平行分支的并集"。
3. Phase B（任务 6.6）：主 Agent 固化 `agentic-premerge` 块 → 在候选 worktree 跑 `workflow check --stage premerge` → PASS 且主分支未移动才合入本地 `main`。本轮**未**执行。
4. `target-integration`（8.1 GB）保留决策：若主 Agent 判定不合规残留，可在 Phase B 后删除；删除代价为 Phase B 一次冷编译。

## handoff_index

```yaml
handoff_index:
  - task_id: "6.2"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integration.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选合并提交在集成分支 integration/node-trust-export-ids-du1 上固定；base=3cadb12d（构建前后未移动），源 cc6faefd 与 659e5900 均经 git merge-base --is-ancestor 核实为候选祖先，且 crates/** 与 docs/** 分别与各自源提交逐字节一致。无冲突。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.3"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-integration.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选树 npm run check（=package.json check 全 10 道子门禁，含 check:docs/check:drift/check:agentic）exit 0；rust 半（cargo fmt --check / clippy -D warnings / cargo test --locked --workspace --all-features）见 reports/du1-pv1-merged.log，exit 0，994 passed / 0 failed。工具链 rust 1.98.1（rust-toolchain.toml）、Node v24.19.0。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.3"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: "check:drift"
    report_path: "reports/du1-integration.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本合并树首次同时具备 §7.3 owned_node DDL 文档侧与 crates/storage-sqlite/src/migrate.rs 代码侧；原始成功行：'contract drift OK: §7 的 36 条 DDL 与 crates\\storage-sqlite\\src\\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\\core\\src\\ports.rs 一致'，exit 0，无差异，未改代码/文档。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.3"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/du1-pv1-merged.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选树 node scripts/check-crate-boundaries.mjs exit 0：'crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）'；同一脚本亦作为 npm run check 的 check:boundaries 通过（du1-integration.log）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.3"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/du1-pv1-merged.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选树 cargo test --locked -p storage-sqlite -p core --all-features exit 0（PV3_EXIT=0）：storage-sqlite 迁移/写集用例与 core 模型/授权用例全绿，无 0 用例 target 异常，0 failed。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.3"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "计划内 [PV4] 与 [PV5] 共用同一命令 cargo test --locked -p server -p app --all-features；该日志即其在本候选树（PV5_EXIT=0）的原始证据：server 286+14+6+4+4 passed、app 57+1+11+12+3+8+2 passed，0 failed。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.5"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选树、本机 Windows（MINGW64_NT-10.0-26200）、loopback 127.0.0.1:0、临时数据目录；cargo test --locked -p server -p app --all-features exit 0。受控路径全链路：node_link_e2e.rs 3/3（含 a_narrowing_repair_closes_the_live_attachment：收窄重新配对 → 既有连接 close code 1000、未收 node.trust.revoked、重连后 catalog 为空；the_controlled_path_runs_end_to_end_and_revocation_propagates）与 node_pair_export_ids.rs 2/2 组合断言。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.5"
    role: integrator
    phase: candidate
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integration.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "临时 node_modules 目录联接已删除、主检出 node_modules 完好且主检出 git status 干净；主分支 refs/heads/main 构建前后均为 3cadb12d（未移动）；loopback 均为 127.0.0.1:0；集成树无残留临时数据目录。CARGO_TARGET_DIR=target-integration 有意保留供 Phase B。"
    source_evidence: NOT_APPLICABLE
```

## 附：复现命令

```text
cd D:/Project/acp-remote-wt/export-ids-integration
git merge --no-ff --no-edit cc6faefd27d7d58a01945971c0cc48bcafd08501   # -> 1c632ad8
git merge --no-ff --no-edit 659e5900ecf79daa110278a07a31bef418dc5d0d   # -> 64e179c6

# 临时联接（跑完删除）
powershell -NoProfile -Command "New-Item -ItemType Junction -Path 'node_modules' -Target 'D:\Project\acp-remote\node_modules'"
npm run check                                                          # -> reports/du1-integration.log
export CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-integration
cargo fmt --all -- --check
node scripts/check-crate-boundaries.mjs
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features                         # -> reports/du1-pv1-merged.log
cargo test --locked -p storage-sqlite -p core --all-features           # -> reports/du1-pv1-merged.log ([PV3])
cargo test --locked -p server -p app --all-features                    # -> reports/pv5-windows-nodelink.log ([PV4]/[PV5])
powershell -NoProfile -Command "cmd /c rmdir node_modules"             # 删除联接
```
