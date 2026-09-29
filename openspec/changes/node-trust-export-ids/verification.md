<!-- 主 Agent 持续维护执行证据与变更历史；判据见 schema.yaml 的 apply instruction 和 procedures/acceptance.md。 -->

## Target

- 变更：`node-trust-export-ids`（changeDir：`D:\Project\acp-remote\openspec\changes\node-trust-export-ids`，权威规划根同）。
- 代码仓库：`D:\Project\acp-remote`。
- 目标主分支：`refs/heads/main` = `3cadb12d79456750e40644a2ea53c96380b700e6`（2026-09-27 核实；本地与 `origin/main` 同 SHA，`git worktree list` 显示主检出位于 `main`）。
- 执行基线：起点提交 = 上述 `3cadb12d`；运行时基线见 `reports/env1-baseline.log`（task 1.1）。

## Handoff Index

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | environment / recon | `3cadb12d` | 运行时基线 | reports/env1-baseline.log | PASS / NEW | 起点核实 + 门禁与 workspace 测试基线 |
| 2.5 | coder / implement / work-package | `d6e01b3b` | CHECK / [PV1]（`check:docs` 子项） | reports/handoff-coder-docs.md | PASS / NEW | 主 Agent 已亲自复跑 `node scripts/check-doc-links.mjs`（exit 0，380 相对链接 / 5090 章节引用）并与 `git diff --name-only 3cadb12..HEAD`（仅 4 个文档文件、工作区干净）核对；独立 review 见 RV1-DOCS（待返回） |
| 2.5 | coder / implement / work-package | `d6e01b3b` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md | BLOCKED / PENDING | 预期受阻：本分支的 `crates/storage-sqlite/src/migrate.rs` 仍是旧版（WP1 在另一分支），漂移门禁只能在集成阶段复跑（任务 3.1、6.3） |
| 2.5 | reviewer / review / work-package | `d6e01b3b` | REVIEW / RV1-DOCS | reports/rv1-docs.md | FAIL（1 MAJOR + 3 MINOR + 2 SUGGESTION） / NEW | 首轮独立 review（fresh 上下文，未参与实现）；发现已逐条登记到 Review Findings |
| 2.5 | coder / fix / work-package | `426f8ae2` | CHECK / [PV1]（`check:docs` 子项） | reports/handoff-coder-docs.md §8 | PASS / NEW | fix 轮次（RV1-DOCS F1–F5 全部修完 + fixer 自查发现的 §10 第三处过期复述）；主 Agent 复核：复跑 `check-doc-links.mjs` exit 0（380 相对链接 / 5101 章节引用）、§9 判据 1…32 无重号无缺号、`d6e01b3b..426f8ae2` 只动 2 个文档文件、工作区干净 |
| 2.5 | coder / fix / work-package | `426f8ae2` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md §8 | BLOCKED / PENDING | 同前：待 WP1/WP3 分支并入后再跑（任务 3.1、6.3） |
| 2.5 | reviewer / review / work-package | `426f8ae2` | REVIEW / RV2-DOCS | reports/rv2-docs.md | BLOCKED（文档静态结论 PASS，仅因 `check:drift` 待集成阶段） / NEW | 复核 F1–F6：全部已解决；新增 2 条非阻断项（RV2-DOCS-F1/F3）已转 fix 轮次 2 |
| 2.5 | coder / fix / work-package | `7d5abeb8` | CHECK / [PV1]（`check:docs` 子项） | reports/handoff-coder-docs.md §9 | PASS / NEW | fix 轮次 2（RV2-DOCS-F1/F3）；主 Agent 复核：`426f8ae2..7d5abeb8` 仅 1 个文档文件（3 插入 1 删除）、既有版本行未动、判据 32 子项 ①–⑤ 连续、复跑 `check-doc-links.mjs` exit 0（380 相对链接 / 5114 章节引用） |
| 2.5 | coder / fix / work-package | `7d5abeb8` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md §9 | BLOCKED / PENDING | 同前：待 WP1/WP3 分支并入后跑（任务 3.1、6.3） |
| 2.5 | reviewer / review / work-package | `7d5abeb8` | REVIEW / RV3-DOCS | reports/rv3-docs.md | PASS（文档静态结论；报告 result 记 BLOCKED 仅因 `check:drift`/`check:agentic` 待集成阶段） / NEW | 定点复核 RV2-DOCS-F1/F3：均已解决；另提 2 条非阻断项（RV3-DOCS-F1/F2） |
| 2.5 | coder / fix / work-package | `39be2f77` | CHECK / [PV1]（`check:docs` 子项） | reports/handoff-coder-docs.md §10 | PASS / NEW | fix 轮次 3（RV3-DOCS-F1/F2）；主 Agent 复核：`7d5abeb8..39be2f77` 仅该文件 2 增 2 删（只命中文件头行与判据 32 首行）、复跑 `check-doc-links.mjs` exit 0（380 / 5117） |
| 2.5 | coder / fix / work-package | `39be2f77` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md §10 | BLOCKED / PENDING | 同前：待 WP1/WP3 分支并入后跑（3.1/6.3） |
| 2.5 | reviewer / review / work-package | `39be2f77` | REVIEW / RV4-DOCS | reports/rv4-docs.md | PASS（文档静态结论；报告 result 依收口约定判 PASS，非阻断项已列） / NEW | 复核 RV3-DOCS-F1/F2：均已解决（含逐字节比对）；另提 RV4-DOCS-F1（单字符排版）与越界的 `verification.md` 门禁发现 |
| 2.5 | coder / fix / work-package | `f47f9da` | CHECK / [PV1]（`check:docs` 子项） | reports/handoff-coder-docs.md §11 | PASS / NEW | fix 轮次 4（RV4-DOCS-F1，单字符；主 Agent 裁定选项 A）；主 Agent 核实：`39be2f77..f47f9da` 1 增 1 删（仅该文件）、第 22 行括号 3/3、worktree 干净、两侧 `check-doc-links` 均 exit 0（worktree 380/5117；主检出 381/7006） |
| 2.5 | coder / fix / work-package | `f47f9da` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md §11 | BLOCKED / PENDING | 同前：待 WP1/WP3 分支并入后跑（3.1/6.3） |
| 2.1–2.4 | coder / implement / work-package | `5190924d` | CHECK / [PV1]（Rust 侧三条） | reports/handoff-coder-impl.md | PASS / NEW | **主 Agent 亲跑**（非实现者自述）：`cargo fmt --check` 0、`clippy --workspace --all-targets --all-features -D warnings` 0、`cargo test --locked --workspace --all-features` 0（989 passed / 0 failed，85 个 test-result 行全 ok） → reports/du1-pv1.log |
| 2.1–2.4 | coder / implement / work-package | `5190924d` | CHECK / [PV3]、[PV4] | reports/{wp1-storage.log,wp2-core.log,wp3-server.log,wp4-app.log} | PASS / NEW | 实现者日志（同修订）；其测试目标集合是主 Agent `--workspace` 运行的子集，结论以 du1-pv1.log 为准（逐 crate 计数在该日志中） |
| 2.1–2.4 | reviewer / review / work-package | `5190924d` | REVIEW / RV1-IMPL | reports/rv1-impl.md | PASS（无 CRITICAL/MAJOR；3 MINOR + 3 SUGGESTION） / NEW | 独立代码检视（不继承实现对话）：自读完整 diff + 调用链，给了 R1–R18 覆盖表（无阻断缺口；R9、R14 两行为部分覆盖），独立核算了我给的 `du1-pv1.log`（989 passed / 0 failed）与我 5 项风险裁定的核实结论；待补 [PV2]、[PV5]、合并树门禁 |
| 2.1–2.4 | coder / fix / work-package | `e4c980c0` | CHECK / [PV1] | reports/handoff-coder-impl.md（fix 轮次） | PASS / NEW | RV1-IMPL 的 F4/F5/F6 + R9 组合断言；**主 Agent 亲跑**：fmt 0、clippy 0、workspace test 0（992 passed / 0 failed / 2 ignored）→ reports/du1-pv1-fix.log；diff `5190924d..e4c980c` = 5 文件全在 `crates/**`（+367/−2） |
| 2.1–2.4 | reviewer / review / work-package | `e4c980c0` | REVIEW / RV2-IMPL | reports/rv2-impl.md | PASS（F4/F5/F6/R9 全解决；新增 1 MINOR + 2 SUGGESTION） / NEW | 独立复核（不继承实现对话）：自读 fix diff 与受影响调用链，独立复算 `du1-pv1-fix.log`（992 / 0 / 2）；其 F1 推翻了我对 RV1-IMPL-F3 裁定的**理由**（“不可达”不成立） |
| 2.1–2.4 | coder / fix / work-package | `8ae3b613` | CHECK / [PV1] | reports/handoff-coder-impl.md（fix 轮次 2） | PASS（**仅实现者自报**；主 Agent 未在该修订上复跑） / NEW | RV2-IMPL-F1 的代码面；含负向对照（注释掉 `close_node` → 新用例失败）；**该修订已被 fix 轮次 3 取代**（见下一条主 Agent 复审发现），修订级证据以最终提交为准 |
| 2.1–2.4 | coder / fix / work-package | `043b359e` | CHECK / [PV1] | reports/handoff-coder-impl.md（fix 轮次 3）+ reports/du1-pv1-reauth.log | PASS（**重试后**） / NEW | 主 Agent 亲跑：首次因 `-p app --test daemon_lifecycle` 的**既有 flaky**（见 ISSUE-2）退出 101；同修订重试 exit 0（994 passed / 0 failed / 2 ignored）；`fmt`/`clippy` 两次均 0；diff 全在 `crates/**` |
| 2.1–2.4 | reviewer / review / work-package | `043b359e` | REVIEW / RV3-IMPL（专项：关闭码/reason 语义） | reports/rv3-impl.md | PASS（0 CRITICAL / 0 MAJOR；3 MINOR + 4 SUGGESTION） / NEW | 独立专项复核（前两轮漏掉的角度）：分流真实存在、reason 与文档 `ee1ca277` **逐字一致**、`1000` 不推消息 / `4410` 推消息；另指出我记录的 `§12.6` 应为 `§12.3`（已核实） |
| 2.5 | coder / fix / work-package | `0d0afc3b` | CHECK / [PV1]（`check:docs`） | reports/handoff-coder-docs.md §12 | PASS / NEW | fix 轮次 5（RV2-IMPL-F1 的文档面）：`NODE_LINK_PROTOCOL.md` §8.2 补「重新配对落定后关连接、收窄自下一次握手生效」并纠正旧口径；主 Agent 复核：`f47f9da..0d0afc3b` = 4/2 仅该文件、双侧 `check-doc-links` exit 0（380/5121 与 381/7052） |
| 2.5 | coder / fix / work-package | `fd597360` | CHECK / [PV1]（`check:docs`） | reports/handoff-coder-docs.md §13 | PASS / NEW | fix 轮次 6：把同一口径同步到 `LOCAL_ADMIN_PROTOCOL.md` §5.4/§7 + 1.4 版本行（消除两份权威文档不一致）；主 Agent 复核：`0d0afc3b..fd597360` = 3/3 仅该文件、双侧门禁 exit 0（380/5123 与 381/7093）、§6 词表表未动 |
| 2.5 | coder / fix / work-package | `659e5900` | CHECK / [PV1]（`check:docs`） | reports/handoff-coder-docs.md §15 | PASS / NEW | fix 轮次 8：`NODE_LINK_PROTOCOL.md` §15 的 `（§12.6）` → **（§12.3）**（既有错误引用勘误，逐处判断清单已入日志）；主 Agent 复核：`ee1ca277..659e5900` = 1/1 单一 hunk、全文件 `§12.6` 引用清零（`rg` exit 1）、双侧门禁 exit 0（380/5131 与主检出 381/7293）；**文档分支 tip = `659e5900`** |
| 2.5 | coder / fix / work-package | `ee1ca277` | CHECK / [PV1]（`check:docs`） | reports/handoff-coder-docs.md §14 | PASS / NEW | fix 轮次 7：把「关闭语义」写准确（§8.2/§14.2 表注/§15 的「撤销即停」限定 + 文件头修订行）；主 Agent 复核：`fd597360..ee1ca277` = 5/3 仅 `NODE_LINK_PROTOCOL.md`、双侧 `check-doc-links` exit 0（380/5131 与 381/7143）、§14.2 表内 12 行 code 取值/含义未动 |
| 2.5 | coder / fix / work-package | `ee1ca277` | CHECK / [PV1]（`check:drift` 子项） | reports/handoff-coder-docs.md §14 | BLOCKED / PENDING | 同前：待两分支并入集成树后跑 |
| 2.1–2.4 | coder / fix / work-package | `cc6faefd` | CHECK / [PV1] | reports/handoff-coder-impl.md（fix 轮次 4）+ reports/wp-fix-notes.log | PASS（实现者自报） / NEW | RV3-IMPL-F1 + F2 代码侧（**纯注释**：主 Agent 核实 `043b359e..cc6faefd` = 2 文件 8 增 3 删、非注释改动行为 **0 行**；含 crates 内 6 处 `12.6` 的逐处判断，5 处保留）；**代码分支 tip = `cc6faefd`**；本修订的 [PV1] 由集成树合并后运行覆盖（本行不单独计） |
| DU1 | integrator / candidate / stage=candidate | `64e179c6` | CHECK / [PV1]、[PV2]、[PV5]、`check:drift` | reports/du1-integration.md | PASS / NEW | 候选 = `64e179c6`（`1c632ad8` ← `cc6faefd` ← base；`64e179c6` ← `659e5900`）；**无冲突**；主 Agent 复核：祖先关系 ✓、与两源 tip 差异 0 行、**亲跑 `check:drift` = exit 0**（“§7 的 36 条 DDL … 逐条一致；§5 的 15 个 trait / 93 个方法签名…一致”）、`check-crate-boundaries` 12 crate ✓、`check-doc-links` ✓ |
| DU1 | integrator / merge / stage=main | `38c6b723` | CHECK / [PV1]、`check:docs`、`check:drift`、`check:agentic` | reports/du1-main-merge.md | PASS / NEW | 本地 `refs/heads/main` = `38c6b7233349fc7afee77de3ab0af73db7215fee`（`--no-ff` 合入 `64e179c6`，`MERGE_EXIT=0`、无冲突）；premerge 重跑 PASS（`8cc6ecf1…`）；权威树 `npm run check` 10/10、cargo fmt/clippy/test exit 0（994/0/2）；**未推送、未开 PR、未归档**，`origin/main` 未变 |
| DU1 | reviewer / review / stage=candidate | `64e179c6` | REVIEW / RV5-IMPL（候选/合并复核） | reports/rv5-impl.md | PASS（0 CRITICAL / 0 MAJOR）/ NEW | 合并完整性 ✓；两处未被独立审过的 delta（`cc6faefd` 注释级、`659e5900` 文档引用）均成立；历史 findings 全部闭环；4 条新发现均非阻断（F1/F2 接受登记、**F3 已修**、F4 接受）；报告 SHA-256 = `0d37a94b…` |
| DU1 | 主 Agent / 最终验证 / stage=main | `38c6b723` | CHECK / [PV5] 主分支版 + `npm run check` | reports/du1-main-final.log | PASS / NEW | 合入后由主 Agent 在**含完整变更目录的权威树**独立复跑：`CHECK_EXIT=0`（`Totals: 20 passed, 0 failed`；doc links 381/7527/357 md；`contract drift OK` 36 DDL + 15 trait/93 签名；`check:agentic` ✓）、`PV5_EXIT=0`（`a_narrowing_repair_closes_the_live_attachment ... ok`、`node_pair_export_ids` 2/2）；`git diff --stat 64e179c6 HEAD` = 0 行 |

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BASELINE（recon）/ 全变更 | `3cadb12d` | 合同门禁 + workspace 测试基线 | 主 Agent（environment/recon 口径） | `npm run check`；`cargo test --locked --workspace --all-features` | Windows x64 / rustc 1.98.1（`rust-toolchain.toml`）/ node v24.19.0 | PASS / 0；PASS / 0（`test result` 汇总仅保留日志末 30 行，退出码为权威判据） | reports/env1-baseline.log |
| [PV1]/WP5（文档面）work-package | base `3cadb12d` → `d6e01b3b` | `docs/{NODE_LINK_PROTOCOL,LOCAL_ADMIN_PROTOCOL,CORE_PORTS_AND_STORAGE}.md`、`openspec/specs/storage-schema-v2-migration/spec.md` 的 Purpose | coder 子 Agent（worktree `acp-remote-wt/export-ids-docs`）+ 主 Agent 复跑 | `node scripts/check-doc-links.mjs`（其余合同门禁作为「资产未动」的反证：`check-schema-fixtures`/`check-command-catalog`/`check-error-registry`/`check-features`/`check-contract-assets`/`check-acp-compatibility`） | Windows x64 / node v24.19.0 | PASS / 0（主 Agent 复现：380 相对链接、5090 章节引用） | reports/wp5-docs.log；主 Agent 复跑输出见本行 |
| [PV1]/WP5（文档 fix 轮次 1）work-package | base `3cadb12d` → `426f8ae2` | `docs/CORE_PORTS_AND_STORAGE.md`（§4/§6 第 5 条/§9 判据 32/§10/§11.3）、`docs/NODE_LINK_PROTOCOL.md`（文件头） | coder 子 Agent（fix 阶段）+ 主 Agent 复跑 | `node scripts/check-doc-links.mjs` | 同上 | PASS / 0（380 相对链接、5101 章节引用） | reports/wp5-docs-fix.log（第一轮）；主 Agent 复跑输出见本行 |
| [PV1]/WP5（文档 fix 轮次 3）work-package | base `3cadb12d` → `39be2f77` | `docs/CORE_PORTS_AND_STORAGE.md`（文件头 0.14 行整行替换、§9 判据 32 首行来源列表） | coder 子 Agent（fix 阶段）+ 主 Agent 复跑 | `node scripts/check-doc-links.mjs` | 同上 | PASS / 0（380 相对链接、5117 章节引用） | reports/wp5-docs-fix.log（第三轮）；主 Agent 复跑输出见本行 |
| [PV1]/DU1（代码交付）work-package | `5190924d`（base `3cadb12d`） | 全 workspace 的 Rust 侧三命令 | 主 Agent | worktree `acp-remote-wt/export-ids`，`CARGO_TARGET_DIR=acp-remote-wt/target-export-ids`：`cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked --workspace --all-features` | Windows x64 / `rust-toolchain.toml` 固定工具链 | PASS / 0；PASS / 0；PASS / 0（989 passed、0 failed） | reports/du1-pv1.log |
| [PV1]/DU1（fix 轮次）work-package | `e4c980c0`（起点 `5190924d`） | 全 workspace 的 Rust 侧三命令 | 主 Agent | worktree `acp-remote-wt/export-ids`：`cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked --workspace --all-features` | Windows x64 / 固定工具链 | PASS / 0；PASS / 0；PASS / 0（992 passed、0 failed、2 ignored） | reports/du1-pv1-fix.log |
| [PV1]/DU1（fix 轮次 3）work-package | `043b359e`（起点 `8ae3b613`） | 全 workspace 的 Rust 侧三命令 | 主 Agent | worktree `acp-remote-wt/export-ids`：`cargo fmt --all -- --check`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；`cargo test --locked --workspace --all-features`（首次 + 重试） | Windows x64 / 固定工具链 | fmt 0、clippy 0（两次）；test **首次 exit 101**（`-p app --test daemon_lifecycle` 的既有 flaky，见 ISSUE-2）→ **重试 PASS / 0（994 passed、0 failed、2 ignored）** | reports/du1-pv1-reauth.log（含两次运行） |
| [PV1]（合并候选）| `64e179c6` | 全 workspace Rust 側 + 合同门禁全量 | 集成 Agent 子 Agent（+主 Agent 复跑关键门禁） | 候选树 `acp-remote-wt/export-ids-integration`，`CARGO_TARGET_DIR=acp-remote-wt/target-integration`：`cargo fmt --check`；`clippy --workspace --all-targets --all-features -D warnings`；`cargo test --locked --workspace --all-features`；`npm run check`（10/10） | rust 1.98.1 / Node v24.19.0 | PASS / 0（994 passed、0 failed、2 ignored；ISSUE-2 的 flaky 两轮首次即过） | reports/du1-pv1-merged.log；reports/du1-integration.log |
| `check:drift`（首次联合比对）/ candidate | `64e179c6` | `docs/CORE_PORTS_AND_STORAGE.md` §7 SQL 与 §5 rust ↔ `migrate.rs` / `ports.rs` | 集成 Agent 子 Agent + **主 Agent 复跑** | `node scripts/check-contract-drift.mjs`（候选树根） | Node v24.19.0 | PASS / 0 | reports/du1-integration.log；主 Agent 复跑输出见本行 |
| [PV2] / candidate | `64e179c6` | 依赖方向 | 集成 Agent + 主 Agent 复跑 | `node scripts/check-crate-boundaries.mjs` | 同上 | PASS / 0（12 crate） | reports/du1-integration.log |
| [PV5]（替代验证主体）/ candidate | `64e179c6` | 受控路径全链路（`node_link_e2e` 等） | 集成 Agent 子 Agent | `cargo test --locked -p server -p app --all-features`，重点 `a_narrowing_repair_closes_the_live_attachment`、`node_pair_export_ids` 2/2 | loopback 随机端口 + 临时数据目录 | PASS / 0 | reports/pv5-windows-nodelink.log |
| [PV1]（合同门禁）/ 权威树（合入后补跑） | `38c6b723` | `npm run check` 全部门禁（含 `check:docs`/`check:agentic`） | 主 Agent | 仓库根（**含完整变更目录**，依 Check Plan Changes 的取位规则） | rust 1.98.1 / Node v24.19.0 | PASS / 0（`Totals: 20 passed, 0 failed`；`check:agentic` 识别本变更；`contract drift OK`） | reports/du1-main-final.log |
| [PV1]/WP5（文档 fix 轮次 4）work-package | base `3cadb12d` → `f47f9da` | `docs/CORE_PORTS_AND_STORAGE.md`（第 22 行去掉一个多余全角右括号） | coder 子 Agent（fix 阶段）+ 主 Agent 复跑 | `node scripts/check-doc-links.mjs`（worktree 与主检出各一次） | 同上 | PASS / 0（worktree 380/5117；主检出 381/7006） | reports/wp5-docs-fix.log（第四轮） |

## Check Plan Changes

- 2026-09-27（契约登记，task 1.2；主 Agent）：本变更改变三处权威合同面，均属**收窄授权**方向，无放宽。
  1. `docs/NODE_LINK_PROTOCOL.md` §8.2：原「首阶段不使用独立的 `exportIds` 维度（v1 的信任记录不落这一列），将来要落地必须同时定义填报、撤销语义与迁移」→ 新「信任记录带 `exportIds`；可见性 = 未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`；`export.revoke` 不级联清理清单；目前没有修改入口（改 = 撤销重配）」。理由：`exportIds` 是本次用户要求落地的能力，且 §8.2 的落地前置四项（填报/撤销/迁移/兼容）在 specs 与 design 中逐项闭合。影响：`server::node_link` 的可见性与 core 的 Owner 侧授权判定各加同一条件。
  2. `docs/LOCAL_ADMIN_PROTOCOL.md` §5.4：原 `node.pair.confirm` 的 `params = { pairingId, grants }`、`result = { nodeId, grants, confirmedAt }` → 新 params/result 均增 `exportIds`（`params` 侧为**必填**，可为空数组）。理由：清单只能在配对确认时填报（本变更范围）。影响：CLI 增 `--export-id`；按 §8 记为 **v1 内合同修订**（v1 未发布、CLI 与 Daemon 同版本发布；先例：`node.challenge.catalogRevision`）。
  3. `docs/CORE_PORTS_AND_STORAGE.md`：§3.5 `NodeRecord` 增 `exportIds`、`PairingSettlement::Approved` 增 `granted_export_ids`；§7.2 版本常量 `3/3/3` → `4/4/3` 并新增 v3 → v4 升级段（`owned_node` 末尾追加 `export_ids_json TEXT NOT NULL DEFAULT '[]'`，只追加不重建）；§7.3 `owned_node` DDL 同批更新；§9 判据 28 补「升级库与新建库 owned 列清单逐项相等」与「旧行置空、不得默认放权」。理由：方案 A 需要落盘清单列；旧行无可信来源不得默认放权（与 v1 → v2 对 Import `grants_json` 的既有口径一致）。影响：`storage-sqlite` 迁移与管理写集、`core` 模型与授权判定、`check:drift` 门禁。
- 不改的合同面及依据（已核实）：`schemas/local-admin/v1/envelope.schema.json` 只把 `params`/`result` 建模为通用对象（逐方法形状由 `LOCAL_ADMIN_PROTOCOL.md` 拥有），方法集与错误码词表不变；`fixtures/local-admin/v1/` 只覆盖信封与错误形状（无逐方法载荷样例）；`compatibility/commands/v1/commands.json` 的命令与 `localCapabilities` 不变。因此本次**不需要**改动这三处资产；「`exportIds` 必填」由守护进程运行期以 `local.invalid_params` 强制并由测试固定。
- 2026-09-27（task 调整，主 Agent）：dispatch 前的实现面复核发现 tasks.md 2.3 只点名了 `node_link::catalog`，而 `local-admin-methods` 规范要求「缺少 `exportIds` MUST 以 `local.invalid_params` 拒绝」，该分支落在 `crates/server/src/local_admin/{params.rs,router.rs}`（`NodePairConfirm` 现为 `reject_unknown_fields(params, &["pairingId", "grants"])`）。据此把该文件与「透传 `granted_export_ids`」写进 2.3 的完成条件：**原值**「server：仅 catalog 第三个条件」→ **新值**「catalog 第三个条件 + local_admin 参数必填与透传」。理由：规范里的 MUST 必须有实现与用例落点，否则 [PV4] 无法覆盖该分支。影响：WP3 的写入范围增加 `crates/server/src/local_admin/**`（仍属同一 crate 与同一执行者，无新增共享文件冲突）；WP4 的 CLI 参数名与 WP5 的 §5.4 文本不变。
- 2026-09-27（review 驱动的范围调整，主 Agent）：独立 review RV1-DOCS 提出 6 项发现（1 MAJOR + 3 MINOR + 2 SUGGESTION），主 Agent 裁决全部修（F6 由主 Agent 自改规划文本）。**原值**「WP5 只改 tasks.md 2.5 列出的三个 docs 文件 + 该 spec 的 Purpose」→ **新值**「同一批再修 `CORE_PORTS_AND_STORAGE.md` §4（RV1-DOCS 原文写作 §5.3，实为 §4 第 222 行的 `[决定]` 句）/§6 第 5 条/§11.3 的三处过期复述、新增 §9 判据 **32**（RV1-DOCS 原文写「31」，实际 §9 已有第 31 条，经主 Agent 裁定追加为 32、不重编号），并在 `NODE_LINK_PROTOCOL.md` 的历史修订行补半句取代说明」。理由：F1 使同一文档自相矛盾（§6 两条件 vs §11.6 三条件），且漂移门禁只比 §7 SQL 块与 §5 rust 块、**抓不到 §6 散文**，不在本次修就会留下「目录不可见但命令可用」的口径风险；F2/F3 是同一类过期复述，分开修反而更乱。影响：WP5 写入范围仍限 `docs/**`（不新增文件），只多一个 fix 提交；`plan.md` 的 WP5 Write Scope 同步补上该 spec 文件（F6），因此本文件 Final Assessment 的 `contract_digest` 会在下次 `workflow check --stage plan` 后重算。
- 2026-09-27（review 发现的方法论纠正，主 Agent）：**门禁证据的取得位置**。此前 WP5 角色报告的 `check:docs` PASS 均在其独立 worktree 里取得，而该 worktree 的 `openspec/changes/**` 只有空 `reports/`（变更目录未被任何分支跟踪），因此**根本没扫到权威 `verification.md`**；RV4-DOCS 在主检出复跑时发现 `verification.md:70` 的「本文件 §3.5」被文档引用门禁归到了 `LOCAL_ADMIN_PROTOCOL.md` 上（`exit 1`）。**原值**「文档门禁在 WP 自己的 worktree 里跑即可」→ **新值**「合同门禁一律在**含完整变更目录的权威树**（主检出）或并入后的树上跑；集成阶段的 `npm run check` 为权威判定，worktree 内的门禁结果只能作为该 WP 文件内的局部证据」。影响：WP5 已登记的 `check:docs` 行仍然有效（它证明了那四个文档文件本身无悬空引用），但**不能**代表权威树判定；集成阶段必须重跑并将结果记到 3.1/6.3 行。已修 `verification.md` 两处引用并实跑主检出门禁 = exit 0（381 链接 / 7003 章节引用 / 350 文件）。
- 2026-09-27（实现期勘误，主 Agent）：`design.md` 的 D9 原文写「`attach` 被拒的 Export 返回 `export.not_found`」有误。实测既有实现（`crates/server/src/node_link/resource.rs`）是两步判定：① 本机不存在/未导出的 id → `nodelink.export.not_found`；② 存在但可见性复核失败（已撤销 / `scopes ∩ grants` 不相交 / **不在 `exportIds` 清单内**）→ `nodelink.export.not_granted`。**原值**（D9 预期）「返回 `export.not_found`」→ **新值**「返回 `nodelink.export.not_granted`（未知 id 才是 `nodelink.export.not_found`）」，并在 D9 就地留下勘误段。理由：两个错误码都在冻结词表内，本次**不改 wire 语义**（不把「存在但未授权」静默降级为「不存在」），改的是我自己写错的预期。影响：`plan.md` 的 [PV5] 行未点名错误码，无需改；specs 未规定 `attach` 错误码，**无验收场景受影响**；受影响任务仅为 [PV5] 的断言文案。因此本文件 Final Assessment 的 `contract_digest` 需在下次 `workflow check --stage plan` 后重算。
- 2026-09-27（实现期勘误，主 Agent）：`design.md` D9 的两处预期有误，已就地勘误并向权威实现对齐。**原值**「`attach` 被拒的 Export 返回 `export.not_found`；`import.add` 该 export 得到 `local.not_found`」→ **新值**「`attach` 被拒的 Export 返回 `nodelink.export.not_granted`（`export.not_found` 只用于 `ownerNodeId` 不是本机或 id 不可解析）；`import.add` 在本切片**恒**返回 `local.unavailable`（无 catalog 快照，`router::import_add` 不校验参数）」。来源：RV1-IMPL-F1/F2（独立代码检视），主 Agent 已实测确认（`crates/server/src/node_link/resource.rs` 的 `on_attach` 两步判定、`crates/server/src/local_admin/router.rs` 的 `import_add` 注释与实现）。理由：不改 wire 词表、不改既有语义，改的是我写错的预期；`import.add` 的 `local.not_found` 分支属 §5.5 的「有快照但无该 id」语义，本切片不可达。影响：`plan.md` 的 Main E2E `alternative_checks` 第 1 条与 `design.md` D9 已同步；specs 未规定这两个错误码，**无验收场景受影响**；契约摘要重算。
- 2026-09-27（review 驱动的范围调整，主 Agent）：独立 review RV3-IMPL 提出 1 条关于 **delta spec 覆盖**的 MINOR（F3），主 Agent 裁定修，已改规划工件：**原值**「`specs/local-admin-methods/spec.md` 的 REQUIREMENT 与场景均未提「重新配对关连接」」→ **新值**「REQUIREMENT 内补一句（提交后 MUST 关连接、`1000` + close reason、不推 `node.trust.revoked`；与 `node.revoke` 的 `4410` 路径 MUST 可区分）+ 新增场景 `#### Scenario: 重新配对收窄清单后作废既有连接`；`plan.md` 的 Coverage Index 新增 **R19**（任务 2.3/2.4/3.1/7.1，检查 [PV4]/[PV5]）」。同时改 D8（写明两条关闭路径的码/语义必须可区分）与文档侧 §12.6→§12.3 的既有错误引用勘误（RV3-IMPL-F2）。理由：这条行为是本变更新引入的、可被验收断言的事实，不写进 delta spec 就在归档后的能力规范里没有踪迹。影响：`openspec validate --strict` = valid、plan 门禁重跑 PASS、契约摘要重算为 `sha256:94c6d39a` → 再次重算为 `sha256:8cc6ecf1`；受影响任务 = 2.3/2.4 的文档面与 [PV5] 断言。
- 无检查清单/脚本/排除项变更；无既有证据失效（本变更为新起点，此前变更的证据不在本变更范围内）。

## Dependency Handoffs

> 2026-09-27（premerge 门禁驱动，主 Agent）：`plan.md` 的 Main E2E `alternative_checks` 由**块状序列**改为**单行 flow 数组**（内容逐字不变，共 3 项）。原因：`workflow check` 的 `plannedAlternativeChecks` 对块状序列会把收集到的项用 `, ` 连接后按 `[,，、]` 切分，而三项文本自身含中文逗号/顿号 → 被碎成多段，导致 premerge 报「候选替代检查必须与计划 alternative_checks 逐项唯一对应」。已核归档前例（`archive/2026-09-27-node-link-owner/plan.md`）即为单行 flow 格式。副作用：`plan` 阶段契约摘要重算为 `sha256:94c6d39a`（本文件两处 `contract_digest` 已同步，候选检查证据不受影响：候选提交与检查日志未变）。

> 2026-09-27（RV5-IMPL-F3 驱动，主 Agent）：`tasks.md` 任务 6.4 的报告路径 `reports/rv2-du1.md` → `reports/rv5-impl.md`（与实际文件、`handoff_index` 对齐）。**任务描述参与契约摘要**，`plan` 阶段摘要因此**再次**重算为 `sha256:8cc6ecf1`；premerge 块的 `contract_digest` 已同步，候选提交与候选检查证据未变（逐项无关）。

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| DU1（集成候选） | 代码分支 WP1–WP4 | `cc6faefd`（RV1-IMPL / RV2-IMPL / RV3-IMPL 均 PASS；[PV1] 在 `043b359e` 由主 Agent 复跑 994/0；`cc6faefd` 为纯注释 delta） | `3cadb12d` | 集成 Agent 用 `git merge-base --is-ancestor` 逐条核实包含；集成树内 `cargo build` 只经公开类型消费 core | 代码侧再次变更 → 重建候选并重跑 [PV1]/[PV5]/`check:drift` |
| DU1（集成候选） | 文档分支 WP5 | `659e5900`（RV1–RV4-DOCS 与 RV3-IMPL 的文档面均已闭环；双侧 `check:docs` exit 0） | `3cadb12d` | 同上（`git merge-base --is-ancestor 659e5900 <candidate>`）；`check:drift` 在此合并树首次成立 | 文档侧再次变更 → 重建候选（尤重 `check:drift`） |
| 主分支交付 | DU1 候选 | 待候选证据 + RV5 合并后复核 | 候选合并提交 | 本地合入后需在**含完整变更目录**的权威树补跑 `npm run check` 才代表最终判定（见 Check Plan Changes 的门禁取位规则） | 基线（`refs/heads/main`）移动 → 重建候选并重验 |

## Runtime Resources

- task 1.1（recon）：只读命令 + `target/` 构建副产物；未改动任何受版本控制文件；无需释放的资源。
- 实现期隔离（计划内）：WP1–WP4 在 worktree `D:\Project\acp-remote-wt\export-ids`（分支 `agentic/node-trust-export-ids`，`CARGO_TARGET_DIR=acp-remote-wt/target-export-ids`）；WP5 在 worktree `D:\Project\acp-remote-wt\export-ids-docs`（分支 `agentic/node-trust-export-ids-docs`）。两者写入范围不相交（代码 / 文档）。
- WP5 角色报告记录的一处临时资源：为跑需要 `ajv` 的 `check-schema-fixtures.mjs`，在文档 worktree 里创建过指向主检出 `node_modules` 的目录联接，检查后已删除（已核实）。
- fix 轮次 4 在文档 worktree 内也写了一份 `openspec/changes/node-trust-export-ids/reports/wp5-docs-fix.log`（被 `.gitignore` 的 `openspec/changes/**/reports/**/*.log` 忽略，不影响任何提交或门禁）；**证据权威副本在主检出的同名路径**（RV1–RV4 读的都是它），worktree 内那份只作重复输出，后续不必清理。
- 实现与检查阶段：共享 `target/`（同一检出串行执行）；集成用例自建临时数据目录与自签证书、loopback 一律 `127.0.0.1:0` 随机端口，用完自删。不涉及数据库服务、容器、固定端口、外部账号或网络资源（依据：`plan.md` 的 Runtime Resources 行）。
- 集成阶段：独立分支/worktree = `D:\Project\acp-remote-wt\export-ids-integration`（分支 `integration/node-trust-export-ids-du1`，起点 `3cadb12d`），由独立集成 Agent 将 `agentic/node-trust-export-ids`（代码）与 `agentic/node-trust-export-ids-docs`（文档）汇入后跑合同门禁与受控路径；自己的 `CARGO_TARGET_DIR` 与主检出隔离。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-DOCS-F1 | `d6e01b3b` | 独立 reviewer 子 Agent（Review ID RV1-DOCS，新建 fresh 上下文，未参与实现；报告 reports/rv1-docs.md） | `docs/CORE_PORTS_AND_STORAGE.md` §6 第 5 条 | **MAJOR（阻断）**：同一文档内 Owner 侧授权仍写作两条件，与本变更在 §11.6/`NODE_LINK_PROTOCOL.md` §8.2 的三条件自相矛盾；漂移门禁只比 §7 SQL/§5 rust 块，抓不到 §6 散文，存在「目录不可见但命令可用」的口径风险 | 主 Agent 定为必修（fix 轮次 run e7e8b683，范围扩展已登记到 Check Plan Changes） | **已解决**（RV2-DOCS 复核 `426f8ae2`：§6 第 5 条已含清单条件且后半句自洽） |
| RV1-DOCS-F2 | `d6e01b3b` | 同上 | `docs/CORE_PORTS_AND_STORAGE.md` §4（第 222 行；RV1-DOCS 原文误写为 §5.3，实为 §4 的 `[决定]` 句） | MINOR：单点可见性策略（D14）仍以两条件复述（同一语义的第二处过期定义，`AGENTS.md` §10） | 必修，与 F1 同批 | **已解决**（RV2-DOCS 复核：§5.3 已是三条件 + 权威指向 + core 同口径声明） |
| RV1-DOCS-F3 | `d6e01b3b` | 同上 | `docs/CORE_PORTS_AND_STORAGE.md` §11.3 | MINOR：既有过期表述「`user_version = 3`」与常量 4 冲突 | 必修（改为 `= 5` = `FILE_FORMAT_VERSION + 1`） | **已解决**（RV2-DOCS 复核：§11.3 已是 `user_version = 5`，与 §7.2/判据 1 一致） |
| RV1-DOCS-F4 | `d6e01b3b` | 同上 | `docs/CORE_PORTS_AND_STORAGE.md` §9 | MINOR：§9 判据列表未覆盖 `settle_pairing` 清单校验（design D9 要求的测试无对应判据行） | 主 Agent 裁决：追加为**判据 32**（RV1-DOCS 原文写「接在 30 之后即 31」，但 §9 实际已有第 31 条「§10.3 的 view 身份与版本」，且被文件头版本记录与 §5.1 交叉引用；据仓库「不改既有判据编号」的约定追加，本次不重编号） | **已解决**（RV2-DOCS 复核：§9 现为 1…32 连续无重号缺号；判据 31 仍是 view 判据，§5.1 与文件头交叉引用未错位） |
| RV1-DOCS-F5 | `d6e01b3b` | 同上 | `docs/NODE_LINK_PROTOCOL.md` 文件头修订记录 | SUGGESTION：2026-09-26 历史行仍写「`exportIds` 维度推后」 | 主 Agent 裁决：保留历史原文，仅补半句注明已被取代 | **已解决**（RV2-DOCS 复核：原文逐字保留、仅追加取代句） |
| RV1-DOCS-F6 | `d6e01b3b` | 同上 | `plan.md` WP5 行 Write Scope | SUGGESTION：漏列 `openspec/specs/storage-schema-v2-migration/spec.md` | 主 Agent 已修（plan.md 变更同时登记到 Check Plan Changes，触发契约摘要重算） | 本行即闭环（规划文本，无需独立复核） |
| RV2-DOCS-F1 | `426f8ae2` | 独立 reviewer 子 Agent（RV2-DOCS，新 Review ID、新上下文） | `docs/CORE_PORTS_AND_STORAGE.md` 文件头 | MINOR：该文件的版本行约定（每次合同改动追加一行）在本分支被漏（`d6e01b3`/`c766010` 两次都没加），修订记录缺位 | 主 Agent 定必修（fix 轮次 2，run 5298841d：追加 `版本：0.14`，不改既有版本行） | **已解决**（RV3-DOCS 复核 `7d5abeb8`：新行在版本列表末尾、体例与 0.12/0.13 一致、既有行逐字未动）；但 RV3 发现该行自身有两处不实陈述，已转 fix 轮次 3（RV3-DOCS-F1） |
| RV2-DOCS-F2 | `426f8ae2` | 同上 | 本文件（`verification.md`）Check Plan Changes | MINOR：主 Agent 自己的记录写「新增 §9 判据 31」，与判据 32 及文档实际不符 | 主 Agent 已自行改正（改为判据 32 并注明 RV1 原文写 31 的差异） | 本行即闭环（主 Agent 自己的记录错误，已改） |
| RV3-DOCS-F1 | `7d5abeb8` | 独立 reviewer 子 Agent（RV4-DOCS 前的 RV3-DOCS，新上下文） | `docs/CORE_PORTS_AND_STORAGE.md` 文件头 0.14 行 | MINOR：新版本行自身有两处陈述不实——① 写「§5.3 的单点可见性策略」，而那句在本文件 **§4（第 222 行）**，§5.3 不含该文本；② 写「§5 的 rust 块已同批更新」，而本次**根本未改任何端口签名**（§5 与 `crates/core/src/ports.rs` 零改动）；另漏记 §9 判据 1/28、§10、§11.3 | 主 Agent 定必修（fix 轮次 3，run 57f86b0c：整行替换为逐字给定文本）；**注意误标源自 RV1-DOCS 报告并被我照抄进指令，已在本文 Check Plan Changes 与 F2 行注明溯源** | 待 RV4-DOCS 按原 ID 复核 |
| RV3-DOCS-F2 | `7d5abeb8` | 同上 | `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 32 首行 | SUGGESTION：来源列表里的裸 `§5.4` 在本文件指「发布与基础设施」（`EventSink`/`Clock`/`IdGenerator`），清单形状的权威在 `LOCAL_ADMIN_PROTOCOL.md` §5.4 与 `CORE_PORTS_AND_STORAGE.md` §3.5 | 主 Agent 裁决：修（fix 轮次 3：来源列表改为「`CORE_PORTS_AND_STORAGE.md` §3.5/§7.2/§7.3/§11.6 第 4 条/§6 第 5 条、`LOCAL_ADMIN_PROTOCOL.md` §5.4、`NODE_LINK_PROTOCOL.md` §8.2」） | **已解决**（RV4-DOCS 复核：列表逐字符合、判据正文 660 字符逐字节相同、子项 ①–⑤ 与 §9 编号 1…32 未受影响） |
| RV4-DOCS-F1 | `39be2f77` | 独立 reviewer 子 Agent（RV4-DOCS，新上下文） | `docs/CORE_PORTS_AND_STORAGE.md` 第 22 行 `版本：0.14` | MINOR（排版，非阻断）：该行全角括号不平衡（开 3 / 闭 4，上一版为 3/3）——主 Agent 给定文本时未含行尾括号，fixer 整行替换时带出了多佘的 `）` | 主 Agent 裁决：修（fix 轮次 4，run 待记：删行尾单个全角右括号）；其独立复核**并入集成阶段的合并后 review**（避免为单个字符再开一轮），本轮由主 Agent 机械核实（括号计数 3/3 + diff 1 增 1 删 + 主检出与 worktree 两侧 doc 门禁 exit 0） | 待集成阶段 review 覆盖（见本行 Resolution） |
| RV4-DOCS-X1 | `39be2f77` | RV4-DOCS 交接（越出文档 WP 范围的重要发现） | `openspec/changes/node-trust-export-ids/verification.md:70`（主 Agent 自己的记录） | **MAJOR 类（会使 `npm run check` 在集成阶段变红）**：该行把 `` `LOCAL_ADMIN_PROTOCOL.md` §5.4 `` 与「本文件 §3.5」写在同一子句，文档引用门禁（`scripts/check-doc-links.mjs`）没有「本文件」概念，把 §3.5 归到 `LOCAL_ADMIN_PROTOCOL.md` 上 → 主检出实测 exit 1。**根因超出该文件**：之前的 `check:docs` 全部在文档 worktree 里跑，而那里 `openspec/changes/**` 只有空 `reports/`，根本没扫到权威 `verification.md` | 主 Agent 已亲自修（两处：把「本文件 §3.5」改为指名 `CORE_PORTS_AND_STORAGE.md` §3.5），主检出 `node scripts/check-doc-links.mjs` = exit 0（381 链接 / 6997 章节引用 / 350 文件） | 本行即闭环（主 Agent 自验：主检出门禁 exit 0；集成阶段 `npm run check` 会再复核一次） |
| 风险裁定-1（attach 错误码） | `5190924d` | 主 Agent（非独立；仅为裁定登记） | `crates/server/src/node_link/resource.rs` | 实现者自报风险：design D9 写 `export.not_found`，实现返回 `nodelink.export.not_granted` | 裁定：**以既有实现为准**（两步判定：未知 id → `not_found`；存在但可见性复核失败（含不在 `exportIds` 内）→ `not_granted`），**不改 wire 词表**，改我自己的 **design D9 预期**（就地留勘误段 + 登记到 Check Plan Changes） | 已修正 design.md；待 RV1-IMPL 核实实现与该裁定一致 |
| 风险裁定-2（`PairingSettlement` 形状） | `5190924d` | 主 Agent（同上） | `crates/core/src/model/identity.rs`、`crates/identity-auth/src/pairing.rs:308`、`crates/server/src/local_admin/router.rs:838-846` | `Approved` 带 `granted_export_ids`；**保留 `approved()` 签名**并新增 `with_granted_export_ids()`（因为 `identity-auth` 的 `PairingDecision` 不带清单、且该 crate 不在本 WP 写入范围）；唯一的生产节点结算路径（`local_admin::router`）在 settle 后立即附加清单 | 裁定：**接受**（服务层单一入口、无旁路）；已知边界：将来若有新入口结算节点配对而忘附加，则清单为空、失败关闭（不默认放权）——**登记为已知边界**，不开新一轮文档改动（代码在附加点已有注释说明） | 待 RV1-IMPL 核实无旁路（含设备配对带非空清单会被拒） |
| 风险裁定-3（列序与迁移守卫） | `5190924d` | 主 Agent（同上） | `crates/storage-sqlite/src/migrate.rs:18/20/22/947/987/991/619`、`owned_node` DDL | 常量 4/4/3；v3 段 `if file_version < 3`；v4 段 `if file_version < 4 && owned_node_pre_existing`（该标志在 :947 于任何重建**之前**取）；新列在 `revoke_reason` 之后、表级约束之前（= 新库 DDL 的最后一列） | 裁定：**接受**（与文档 §7.3 同形，漂移门禁将在合并树复核） | 待 RV1-IMPL 核对四条升级路径与「升级库 vs 新建库列清单相等」断言 |
| 风险裁定-4（CLI 空清单警告通道） | `5190924d` | 主 Agent（同上） | `crates/app/src/cli/pairing.rs:371/419-421`；`docs/LOCAL_ADMIN_PROTOCOL.md` §7（第 562 行） | 警告走 stdout（`println!`），而契约规定「失败时 stdout 只输出人类可读说明、机器可读信息是 stderr 的**一行** JSON」→ stderr 不得放警告 | 裁定：**符合契约**，不引入新开关 | 待 RV1-IMPL 核实无路径破坏 stdout/stderr 契约 |
| 风险裁定-5（`node list` 展示） | `5190924d` | 主 Agent（同上） | `crates/server/src/local_admin/view.rs`、`crates/app/src/cli.rs` | 清单由 daemon 侧投影带出、CLI 原样打印 `result` | 裁定：接受（空清单如实显示） | 待 RV1-IMPL 核实 |
| 方法论纠正-1 | `39be2f77` | 主 Agent | 门禁运行位置 | 之前的「文档线 `check:docs` PASS」仅在文档 worktree 成立（那里没有变更目录），与权威树的判定不是同一件事 | 已登记到 Check Plan Changes：此后本变更的合同门禁一律在**主检出（含变更目录）**或并入后的主分支树上跑；集成阶段 `npm run check` 为权威判定 | 已记入本行与 Check Plan Changes |
| RV2-DOCS-F3 | `426f8ae2` | 同上 | `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 32 ③ | SUGGESTION：判据 32 只断言可见性，未含 design D9 要求的 core 侧「清单为空/不在清单内则该 Export 的会话命令不可用」断言（tasks 3.2 已覆盖，风险有限） | 主 Agent 裁决：修（fix 轮次 2：判据 32 补 core 侧同口径断言，子项顺延为 ①–⑤） | **已解决**（RV3-DOCS 复核：④ 已插入、与 §6 第 5 条/`NODE_LINK_PROTOCOL.md` §8.2/D1/D9 一致，①–⑤ 连续、其余子项逐字未动） |
| 跨 WP 待办-1 | `d6e01b3b` | RV1-DOCS 交接 | `crates/storage-sqlite/src/migrate.rs:963-964` | WP1 必须给 v3 段加 `file_version < 3` 守卫（主 Agent 已实测确认） | 已 steer 给运行中的代码轮次 dbf76a2b，要求补针对性迁移用例 | 待 [PV3] 与代码 reviewer 核实 |
| 跨 WP 待办-2 | `d6e01b3b` | RV1-DOCS 交接 | `crates/server/src/local_admin/params.rs:741` | WP3 需同批更新复述 §5.4 旧形状的文档注释 | 同上（steer 附加要求 2） | 待 [PV4] 与代码 reviewer 核实 |
| RV1-IMPL-F1 | `5190924d` | 独立 reviewer 子 Agent（RV1-IMPL，新上下文，未参与实现；报告 reports/rv1-impl.md） | `plan.md` Main E2E `alternative_checks` 第 1 条、`design.md` D9 | MINOR：两处断言 `import.add` 返回 `local.not_found`，而本切片 `import_add` **恒**返回 `local.unavailable`（无 catalog 快照） | 主 Agent 已实测确认（`router.rs` 的 `import_add` 注释与实现）并自行修正两处文本（plan.md + design.md 勘误，登记到 Check Plan Changes） | 本行即闭环（文案已改；待 RV2-IMPL 顺带确认文本与实现一致） |
| RV1-IMPL-F2 | `5190924d` | 同上 | `design.md` D9 勘误段第 ① 步 | MINOR：该勘误段仍写「本机不存在/未导出的 id → `not_found`」，实测 `not_found` 只用于 `ownerNodeId` 非本机或 id 不可解析；可解析但不在本机清单的 id 走可见性复核 → `not_granted` | 主 Agent 已改（同 F1，并在勘误段写明两步判定的真实边界） | 同上 |
| RV1-IMPL-F3 | `5190924d` | 同上 | `crates/core` 的 `ReadView::node_link_session_view` 硬校验 + `resource.snapshot`/`replay` | MINOR（reviewer 请主 Agent 判断）：收窄清单**不作用于已建立的 attachment**（snapshot/replay 走 core 读 seam，只校验「已配对 + Export 未撤销 + 会话 Agent 属于该 Export」，不查清单）；对 condition ② 属既有问题 | 主 Agent 裁定：**登记为已知限制 + 后续切片要求**，本次不加运行期校验。理由：本切片**没有运行期修改清单的入口**（改 = `node.revoke` + 重新配对，而 revoke 会关闭活动连接并停止重连），所以「已建立的 attachment 活在收窄之后」在当前 API 面下**不可达**；但将来若新增运行期编辑入口，**必须同时**把清单条件加进读 seam 的硬校验（或强制重握手），此要求随本行保留 | 本行**被 RV2-IMPL-F1 推翻并已改为修复**（保留原裁定作为历史）：原裁定以「不可达」为由只做书面登记，RV2-IMPL 给出反例（重新配对即可）并被主 Agent 实测确认；现由 fix 轮次 2（代码 run 3c6af73b）+ fix 轮次 5/6（文档 §8.2/§5.4/§7）落实「落定后关连接」，待合并树 review（RV5）复核 |
| RV1-IMPL-F4 | `5190924d` | 同上 | `crates/server/src/local_admin/test_support.rs`（`FakeTrust::revoke_node`） | SUGGESTION：假存储撤销时丢清单，与真存储（保留 `export_ids_json`）不同形，可能掩盖「撤销不清理清单」的回归 | 主 Agent 裁定：修（fix 轮次 bff77765，与真存储同形） | 待 RV2-IMPL |
| RV1-IMPL-F5 | `5190924d` | 同上 | `crates/core/src/model/identity.rs` 附近注释 | SUGGESTION：错别字「只能收窄不掉宽」 | 主 Agent 裁定：修（同批） | 待 RV2-IMPL |
| RV1-IMPL-F6 | `5190924d` | 同上 | `crates/storage-sqlite/src/admin/trust.rs` 的 `PairingTarget::Device` 分支 | SUGGESTION：设备配对**静默忽略**非空清单（不可达但属授权字段静默丢弃） | 主 Agent 裁定：修成与 grants 对称的显式拒绝（`InvalidRequest` + 针对性用例，`AGENTS.md` §3「不得静默忽略」） | 待 RV2-IMPL |
| RV1-IMPL-R9 | `5190924d` | 同上 | `local-admin-methods` R9 的组合覆盖 | MINOR（覆盖缺口）：缺 router + **真存储** 的组合断言——「不存在/已撤销 → `local.not_found`」「与本次 grants 不相交 → `local.invalid_params`」且零写入零审计 | 主 Agent 裁定：补用例（同批） | 待 RV2-IMPL |
| RV1-IMPL-R14 | `5190924d` | 同上 | `storage-schema-v2-migration` R14（升级库与新建库列清单相等） | 部分覆盖（非阻断）：v2 → v4 路径没有 `column_specs` 断言；reviewer 已手比对夹具 DDL 确认相等 | 主 Agent 裁定：接受（v3 → v4 与新建库两条主路径已有断言；v2 路径属既有测试资产范围）；**不在本次扩测** | 本行即闭环（书面裁定） |
| RV2-IMPL-F1 | `e4c980c0` | 独立 reviewer 子 Agent（RV2-IMPL，新上下文；报告 reports/rv2-impl.md） | 本方 RV1-IMPL-F3 行的裁定理由 + 实现现状（`storage-sqlite::approve_node`、`identity-auth::claim_pairing`、`router::node_pair_confirm`） | **MINOR（但推翻了我的「不可达」判断）**：`approve_node` 对已 `paired` 行无状态守卫、`claim_pairing` 无「已配对」守卫、`node.pair.confirm` 不调 `close_node` → **重新配对（不 revoke）就能收窄清单，而活动连接与已建立 attachment 继续收到该 Export 的 `resource.event`**（扇形扇出不判定 `exportIds`） | 主 Agent 已在 worktree 实测确认（全代码库仅 `router.rs:935` 一处 `close_node`，即 revoke 路径）→ 裁定**修，而非仅文档化**：复用 revoke 的**同一机制**在 confirm 提交后关连接（代码 run 3c6af73b），**明确否决**在 core 读 seam 复制条件 ③（那会造成第三处判定点）；文档侧同批补语义（run 3d79c3cb）。两处小改动的 delta 由合并树 review（RV5）覆盖 | 待 RV5（合并后） |
| RV2-IMPL-F2 | `e4c980c0` | 同上 | `crates/server/src/node_link/catalog.rs` 的测试断言 | SUGGESTION（既有）：`node_row.export_ids()` 的断言恒真（同义反复，申明了但不判别） | 主 Agent 裁定：**接受，不在本次改**（属既有测试资产、不影响行为正确性；已在本次交付之外） | 本行即闭环（书面裁定） |
| RV2-IMPL-F3 | `e4c980c0` | 同上 | `crates/server/src/local_admin/router.rs` 的测试替身 | SUGGESTION（既有）：清单归一化步骤只走 fake | 主 Agent 裁定：**接受**（R9 的组合断言已改走真存储，关键路径已被真存储覆盖） | 本行即闭环（书面裁定） |
| 主 Agent 复审-1（关闭码语义） | `8ae3b613` | **主 Agent 复审**（非独立 review；RV1-IMPL / RV2-IMPL 两轮独立检视均 PASS 且未发现） | `crates/app/src/compose.rs` 的 `close_node` 实现（→ `command.rs` 的 `node_revoked`）+ `crates/server/src/local_admin/router.rs` 的 confirm 调用 | **MAJOR（阻断，已修）**：重新配对复用了**撤销**关闭语义——会尝试推送 `node.trust.revoked`，并以 `close::REVOKED`（**4410「节点已撤销」**）关闭，reason 字符串写作 `"the node trust was revoked"`（对该节点是**事实错误**）。而 `NODE_LINK_PROTOCOL.md` §14.2 定义 4410 = 节点已撤销、§15 的「撤销即停」规定收到 `node.trust.revoked` 或本地 `node.revoke` → **停止重连**；合规 Access 客户端因此会把「收窄」读成「被撤销、永不重连」，与刚写进 §8.2 的「收窄自下一次握手后的新连接生效」直接矛盾 | 主 Agent 定方案并派修（代码 run 910ed262 + 文档 run 16529fcd）：新增**专用**关闭路径（不推消息、以 **1000 正常关闭** + 准确 reason），撤销路径保持 4410 不变；用例断言改为 1000 并断言**未收到** `node.trust.revoked` | 待合并后 review（RV5）按原问题复核 |
| 主 Agent 复审-1 的教训（记录用） | — | 主 Agent | review 覆盖 | 两轮独立代码检视都 PASS，却都没把「服务端主动关闭连接的 **code/reason** 与协议语义是否一致」当检视项 | 已把该项写进 RV5 的必查清单；最终验收说明里也会写明这是**主 Agent 复审**捕获、非独立 review 捕获。另：我在给两个 Agent 的指令里把「撤销即停」的节号误写为 **§13**（实为 **§15**，§13 是「节点配对 HTTP」），文档 Agent 自行核对后纠正并报告；引用权威文档条款前必须先核节号 | 不适用（记录） |
| RV3-IMPL-F1 | `043b359e` | 独立 reviewer 子 Agent（RV3-IMPL，新上下文；报告 reports/rv3-impl.md） | `crates/server/src/local_admin/router.rs:816` | MINOR：`node_pair_confirm` 文档注释首段仍写「与 `node.revoke` 同一机制」，与同函数新注释（883-884）及 `pairing.rs`/`compose.rs` 矛盾 | 主 Agent 裁定：修（fix 轮次 4，run cfa9052a，注释级） | 待合并树 review（RV5）按原 ID 复核 |
| RV3-IMPL-F2 | `043b359e` | 同上 | `docs/NODE_LINK_PROTOCOL.md:930`（§15）+ `crates/server/src/local_admin/pairing.rs:64` | MINOR（**既有**错误交叉引用）：`node.trust.revoked`/`export.revoked` 标为（§12.6），实为 **§12.3**（Catalog 消息；§12.6 是控制与错误消息）。另：`check-doc-links` **不校验 `§N` 引用是否真实存在** | 主 Agent 已自行核实（§12.3 第 521 行登记、§12.6 为控制消息）→ 裁定：修（代码侧 run cfa9052a + 文档侧 run d4de783a），逐处只改真实指错的那几处 | 待 RV5 复核 |
| RV3-IMPL-F3 | `043b359e` | 同上 | 变更 delta spec 与 `design.md` D8 | MINOR：delta spec 无「重新配对以 `1000` 关连接、不推 `node.trust.revoked`」的 Requirement/Scenario；D8 仍写「与 `node.revoke` 同一把机制」 | 主 Agent 裁定：两者都修——D8 已改（写明两条路径关闭码/语义必须可区分）；`specs/local-admin-methods/spec.md` 已在 REQUIREMENT 里补一句 + 新增 `#### Scenario: 重新配对收窄清单后作废既有连接`，并在 `plan.md` 的 Coverage Index 新增 **R19**（任务 2.3/2.4/3.1/7.1，检查 [PV4]/[PV5]） | 待 RV5；`openspec validate --strict` = valid，plan 门禁重跑 PASS |
| RV3-IMPL-F4 | `043b359e` | 同上 | `crates/server/src/node_link/command/tests.rs:2268-2271` | SUGGESTION：`pending_of(...).is_empty()` 在该用例恒真（既有同款写法） | 主 Agent 裁定：接受（但已在 ISSUE-3 登记为待清的技术债） | 本行即闭环 |
| RV3-IMPL-F5 | `043b359e` | 同上 | `crates/app/tests/node_link_e2e.rs:836-843` + `support/nodelink.rs:697` | SUGGESTION：e2e 只断言 close code，reason 被客户端丢弃，无线上字节断言 | 主 Agent 裁定：接受——reason 的**逐字一致性**改由「代码/测试 ↔ 文档」直接比对（RV3 已用 `grep -c -F` 验证三处一致）与单测覆盖；扩展客户端解析 close reason 属测试资产改进，登记为后续 | 本行即闭环 |
| RV3-IMPL-F6 | `043b359e` | 同上 | `crates/server/src/local_admin/router.rs:575-631` | SUGGESTION：设备面配对确认改 scopes 后不作废连接（今天无设备连接，不冲突） | 主 Agent 裁定：接受并**登记为切片 7 的后续要求**（设备连接面落地时必须同口径） | 本行即闭环（后续要求已写明） |
| RV3-IMPL-F7 | `043b359e` | 同上 | `crates/server/src/node_link/conn/registry.rs:358` | SUGGESTION：`ConnectionRegistry::close_node` 无调用者（base 亦然），而两条新路径各自手写同一循环 | 主 Agent 裁定：接受，**不在本次合并**（属重构；死代码为 base 既有）| 本行即闭环 |
| RV5-IMPL（结论 PASS） | `64e179c6` | 独立 reviewer（run `88d863eb`，不继承实现对话） | 候选全量（base `3cadb12d` → 候选） | **结论 PASS（0 CRITICAL / 0 MAJOR）**：合并完整性无缺陷（两源均祖先、各自文件集与源逐字节一致、无夹带/丢失、无冲突解决）；`cc6faefd` 确认为纯注释且与行为一致；`659e5900` 单一 hunk 且未误伤其它引用；历史 findings **全部闭环**（含两条 MAJOR 类：RV1-DOCS-F1、主 Agent 复审-1）；RV3-IMPL 列出的 7 项未覆盖面**无一项阻断候选**（②④属后续切片义务，其余为测试强度/证据登记项） | 无待修项；报告 `reports/rv5-impl.md` | 本行即闭环 |
| RV5-IMPL-F1 | `64e179c6` | 同上 | `verification.md` / `local-admin-methods` delta spec | MINOR：后续义务「设备面同口径」与「Access 侧真实重连」目前只落在 `verification.md`，无更持久登记 | 主 Agent 裁定：接受并登记——两者都是**尚未存在的能力面**（设备方向连接作废、Access 侧客户端）的验收义务，会在对应切片落地时生效；本变更的验收记录将在归档时随 `agentic-assessment` 与主规范同步，不额外改文档（避免为未实现能力写“已支持”式表述） | 本行即闭环 |
| RV5-IMPL-F2 | `64e179c6` | 同上 | `docs/NODE_LINK_PROTOCOL.md` §8.2 | MINOR：收窄后「已在途帧仍可能送达」这一残余窗口未显式写明、也无断言 | 主 Agent 裁定：接受（非阻断）——§8.2 已就「重新配对落定后关连接、收窄自下一次握手生效」限定生效范围（旧连接在途帧不在承诺内），补写价值低于再开一轮文档复核的成本；残余窗口登记于本表与最终验收 | 本行即闭环（复核选项已记录） |
| RV5-IMPL-F3 | `64e179c6` | 同上 | `tasks.md` 任务 6.4 / `reports/` 实际文件名 | SUGGESTION：任务 6.4 写的是 `reports/rv2-du1.md`，而实际报告为 `reports/rv5-impl.md`（主 Agent 早期命名漂移） | 主 Agent 已修：`tasks.md` 6.4 的报告路径改为 `reports/rv5-impl.md`（与 `handoff_index`、实际文件对齐） | 已在 `tasks.md` 写入修正；验收时复读即可 |
| RV5-IMPL-F4 | `64e179c6` | 同上 | `reports/rv5-impl.md` 文件头 | SUGGESTION：`git diff --check` 报 trailing whitespace，属该报告类文件头既有体例（base 同形） | 主 Agent 裁定：接受，不改（报告类文件非产品产物，与 base 一致） | 本行即闭环 |

- **仍未覆盖的检查面（RV3-IMPL 自己列出，随行保留）**：① 收窄后扇出窗口内可能仍到达一条已收窄 Export 的事件（未被任何用例/文档量化）；② Access 侧真实自动重连（`node-link-client` 未落地）只能算契约级；③ 三条负向对照均为实现者自报日志，未由独立角色复跑；④ `device.pair.confirm` 同类缺口。前两项为切片 7 的后续验证点。

- RV1-IMPL 的 Assessment：**PASS**（无 CRITICAL/MAJOR；6 项非阻断）；待补 [PV2]、[PV5]、合并树门禁。RV2-IMPL 的 Assessment：**PASS**（F4/F5/F6/R9 已解决，无回归）；待补同上。
- **我的裁定被推翻一条（保留记录）**：RV1-IMPL-F3 行里我以「本切片没有运行期改清单的入口 → 不可达」为由只做书面登记；RV2-IMPL-F1 给出反例（重新配对即可），我实测确认后改为修复。结论：**「不可达」类论断必须拿代码守卫作为依据，不能靠设计文档的意图推定**。
- RV1-DOCS 的 Assessment：**FAIL**（F1 为未解决的 MAJOR）；已确认成立的部分：三条件/只收窄/空清单/撤销不级联/无修改入口/升级副作用/wire 不变与 design D1–D8 及 15 个场景逐条对应；§7.3 新列与 D3 逐字一致；§7.2 域内已全量 4/4/3；封闭词表与合同资产未改；reviewer 独立复跑 `check-doc-links.mjs` = PASS。待补证据：`check:drift`（PENDING，集成阶段）、`check:agentic`（文档 worktree 无 `node_modules`，集成树补跑）。

## Merge History

<!-- 唯一 agentic-premerge 块（阶段 6.6 前录入；候选固定为 64e179c6，目标基线未移动） -->
```agentic-premerge
version: 1
delivery_unit: DU1
target_ref: refs/heads/main
target_commit: 3cadb12d79456750e40644a2ea53c96380b700e6
candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
contract_digest: sha256:8cc6ecf1af02fcdaa5f2ca289dbdf3698f369feb73158dd3fec88e445c23871d
verify:
  result: PASS
  candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
  evidence: {path: reports/du1-pv1-merged.log, sha256: "sha256:be76f4b6462f4ddfbd79fc5c35ac19d8621c64826e85a0cedc78983e9bc6de86"}
review:
  result: PASS
  candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
  reviewer: "subagent worker 88d863eb-e4c6-44e0-9850-2daab991bc16（RV5-IMPL，不继承实现对话）"
  author: "subagent worker dbf76a2b-1735-4558-9ebd-85cd771e40ac（WP1–WP4 实现者）"
  evidence: {path: reports/rv5-impl.md, sha256: "sha256:0d37a94b39f6597cfd180811f64699258c86adb4798b6da4a489b05bfd9eac8e"}
alternative_checks:
  - name: "cargo test --locked -p server -p app --all-features 的受控路径全链路用例（[PV5]）：配两个 Export、确认时只给一个，验证目录只见其一、另一 Export 的 attach 被拒为 nodelink.export.not_granted；import.add 在本切片**恒**返回 local.unavailable（Access 侧客户端未落地 → 无 catalog 快照），因此不断言它的 local.not_found"
    result: PASS
    candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
    evidence: {path: reports/pv5-windows-nodelink.log, sha256: "sha256:259e05fd23fe1b33fd005627cc1781c3dec891f2431db043cae4fbd7027cbc25"}
  - name: "cargo test --locked -p storage-sqlite -p core --all-features（[PV3]）：v3 → v4 迁移的旧行置空/幂等/列清单相等，以及 settle_pairing 的三类校验与失败零写入"
    result: PASS
    candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
    evidence: {path: reports/du1-integration.log, sha256: "sha256:8327ebed7fa80f7cb167ef908aec61aa4db7b1c2bc5b1dd8e1083849744d875e"}
  - name: "npm run verify（[PV1]）与 node scripts/check-crate-boundaries.mjs（[PV2]）：合同门禁（含 check:docs/check:drift）与全 workspace 测试"
    result: PASS
    candidate_commit: 64e179c618e690efcc39f64854d59861e9ecbacf
    evidence: {path: reports/du1-integration.log, sha256: "sha256:8327ebed7fa80f7cb167ef908aec61aa4db7b1c2bc5b1dd8e1083849744d875e"}
```

<!-- 合入结果与防竞态记录在 Phase B 填入（目标基线 3cadb12d 未移动；主 Agent 在合入前再核实一次） -->
| 阶段 | Candidate / Main | 证据 | 结论 |
| --- | --- | --- | --- |
| 候选（Phase A） | `64e179c6`（base `3cadb12d`） | reports/du1-integration.md；reports/du1-integration.log（`npm run check` 10/10 含 `check:drift` 首绿）、reports/du1-pv1-merged.log（994/0/2）、reports/pv5-windows-nodelink.log、reports/rv5-impl.md | PASS（集成 Agent 执行；主 Agent 独立复跑 `check:drift`/`check-crate-boundaries`/`check-doc-links` 均 0） |
| 本地合入（Phase B） | `main` = `38c6b723`（合入候选 `64e179c6`） | reports/du1-main-merge.md；reports/du1-main-verify.log（权威树 `npm run check` 10/10 含 `check:agentic` 识别本变更、cargo 994/0/2） | PASS（树与候选**逐字节相同**；未推送，`origin/main` 仍 `3cadb12d`；主 Agent 在 main 树独立复跑 [PV5]+`npm run check` = reports/du1-main-final.log，20/20） |

## Test Design and Authoring

## Candidate E2E

## Main E2E

- 项目开关：`npx --quiet --no-install openspec-agentic e2e --json` → `enabled: true`、`command: ""`。
- `plan.md` 的 mode：`not-applicable`（reason/basis/非空 `alternative_checks` 齐备，`downgrade_approval` 记录 2026-09-27 本会话用户批准原话「同意」）。E2E 标为 **NOT_APPLICABLE**；替代检查见 `Checks` 中的 [PV3]/[PV5] 行。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| ISSUE-2（既有 flaky） | `043b359e` / [PV1] `cargo test --locked --workspace --all-features` | 主 Agent | 无需修复（非本变更引入）：`crates/app/tests/daemon_lifecycle.rs:603` 在 `daemon.start()` 返回后立即断言 maintenance 首轮已发生（`!ticks.is_empty()`），而首轮 tick 是异步的 ⇒ 测试自身竞态；该文件与 maintenance/daemon 路径在本变 diff 中**零改动**（`git diff --name-only 3cadb12..HEAD` 无该文件） | 已核实非回归（同修订重试绿 + 该 target 连跑 3 次 2 绿 1 红） | 重试日志：`reports/du1-pv1-reauth.log` 的「重试」节（exit=0，994 passed / 0 failed / 2 ignored）；定向复现：`cargo test -p app --test daemon_lifecycle` ×3 = 1 红 2 绿 | **未解决（既有 flaky，建议单独变更修）**：建议让断言等待首轮 tick 或由 `daemon.start()` 返回就绪信号；已在交付说明里写明 CI `checks` job 有间歇红灯风险 |
| ISSUE-3（测试技术债） | `043b359e` | 主 Agent（登记） | `crates/server/src/node_link/command/tests.rs` 的恒真断言（RV3-IMPL-F4）及同款既有写法 | 无需本次修复 | 不适用 | **未解决（低风险，已登记）** |
| 无 | — | — | — | — | — | 其余无失败或受阻记录 |
| ISSUE-1（文档门禁在主检出变红） | `39be2f77` / [PV1] `check:docs` / 主检出 | 主 Agent（自己写入的记录文本） | 把 `verification.md:70` 的「本文件 §3.5」改为指名 `CORE_PORTS_AND_STORAGE.md` §3.5（另含一处同类表述），并登记门禁取位规则（见 Check Plan Changes） | RV4-DOCS 报告（reports/rv4-docs.md）的越界发现 | 主检出 `node scripts/check-doc-links.mjs` = exit 0（381 链接 / 7003 章节引用 / 350 文件，于 `3cadb12` + 本变更未跟踪产物上） | **已解决**（主 Agent 亲自复跑主检出门禁为 0；集成阶段 `npm run check` 会再复核） |

## Final Assessment

```agentic-assessment
assessment_id: "FA-2026-09-27-01"
target_commit: "3cadb12d79456750e40644a2ea53c96380b700e6"
contract_digest: "sha256:8cc6ecf1af02fcdaa5f2ca289dbdf3698f369feb73158dd3fec88e445c23871d"
result: BLOCKED
evidence:
  - path: reports/env1-baseline.log
    sha256: "sha256:3bf835a29dbe7e4e8af31eee4b418bc3e07c72b72cf16563ecefd4eab29ecd2d"
```

- Assessment ID / Time: FA-2026-09-27-01（apply 起点；待实现与最终验收后更新）
- Target / Task: `refs/heads/main` = `3cadb12d`；最终验收任务 = tasks.md 的 8.1
- CLI State: `openspec status --change node-trust-export-ids --json` 于 2026-09-27 apply 起点查询：5/5 产物 done、任务 0/23（原始输出见本文件 Target 与 tasks.md）
- Audit / Evidence: 尚未进入验收；当前已登记运行时基线、契约变化、四轮文档 review（RV1–RV4-DOCS）与首轮代码 review（RV1-IMPL = PASS）。契约摘要已因规划文本勘误（`plan.md` 的 `alternative_checks` 与 `design.md` D9 的错误码/`import.add` 预期）重算为 `sha256:6ee0bb31`（plan 门禁仍 PASS）
- Result / Open Issues: **BLOCKED**（实现与检查未开始；不是 FAIL）
- Required Follow-up: 完成 tasks.md 1.1–7.3 后按 `.agents/skills/agentic-verify/SKILL.md` 重做验收并更新本轮 assessment
