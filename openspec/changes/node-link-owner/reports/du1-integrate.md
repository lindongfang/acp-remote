# DU1 集成报告（`node-link-owner`；任务 5.1 与 6.1–6.3 的候选构造部分）

> 本报告只覆盖集成 Agent 自身执行的范围（核对 → 候选构造 → 候选 Project Verify → 报告）。
> 未执行 6.6 的本地合入（主 Agent 在 `agentic-premerge` 门禁通过后另行触发）。
> 未修改权威 `plan.md` / `tasks.md` / `verification.md`。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | 5.1、6.1、6.2、6.3（候选构造部分；6.4/6.5/6.6 未执行） |
| `role` | integrator |
| `phase` | integrate（候选构造 + candidate Project Verify） |
| `agent_context` | 独立集成子 Agent（worker 角色；session `01a0de15-6c33-755a-9d1a-afce9d68f8c7`，`PI_SUBAGENT_CHILD=1`，父 session `01a0db22-b46e-7543-a1f5-d4621ef3e22f`）；首次派发，**不继承任何实现对话**；工作目录 `D:\Project\acp-remote-wt\nl-owner-integration`（由主 Agent 单独创建，主 Agent 不兼任本次执行） |
| `target_revision` | candidate `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5`（`stage=candidate`）；目标分支 `refs/heads/main` = `94a64e1f15d26f54a6601985fd13b1440fec8170`（本轮核实三次未移动） |
| `scope` | ①核对 DU1 组成、源/基线包含关系与已验收证据可读性；②基于已核实 main 构造固定候选；③在候选上执行 PV1 + PV2 + PV5；④报告（**不合入**） |
| `changes` | **无产品代码改动**；**无冲突解决**（两轮候选的树都与源提交逐字节相同：`git diff --stat HEAD agentic/node-link-owner` 为空）。唯一写入：本地分支 `refs/heads/integration/node-link-owner-du1`（指向候选）与候选 merge commit |
| `checks` | PV1 = PASS（exit 0）；PV2 = PASS（exit 0）；PV5 = PASS（exit 0）——逐子检查退出码见 §3 |
| `issues` | I1：attempt 1 的 `check:docs` 红（源 `f02d2565` 自身同样红）→ 主 Agent 源分支勘误 `4c5a3f00` 后重建候选并复绿；I2：PV5 attempt 1 的 flake → 按主 Agent 裁决登记（§4） |
| `result` | **PASS（候选阶段）**：候选三项检查全绿、基线未移动、证据链完整可读。**候选 PASS ≠ 已合入或最终验收 PASS**；6.4 独立 review／6.5 关键 E2E（本变更记 NOT_APPLICABLE，替代验证已在分支侧留证）／6.6 合入与 premerge 门禁仍待办 |
| `evidence_paths` | `reports/du1-candidate.log`（原始日志，6476 行；含 attempt 1 superseded 段与 attempt 2 权威段）、`reports/du1-integrate.md`（本报告） |
| `resource_cleanup` | 候选 worktree 与其自带 `target/`（7.8G）**保留**给 6.4 独立 review 与 6.6 合入；跑完后 `git status --porcelain` 空、无 `acpr*` 残留进程、`/tmp/acpr-*` 0 个；未触碰主 Agent 工作区未提交改动（`M docs/DEVELOPMENT_PLAN.md`、未跟踪的变更目录） |

## 1. 核对（DU1 组成、包含关系、证据链）

**交付单元组成**（`plan.md` Merge Strategy / `tasks.md` W9）：

- DU1 / `integrated` = WP1–WP8，无 TP；一次候选验证、一次合入；检查范围 [PV1]–[PV5]，Main E2E 记 `not-applicable`（替代验证 = plan §7.1 四项）。

**源提交与基线包含关系**（在 `D:\Project\acp-remote` 上核实）：

| 核对项 | 命令 | 结果 |
| --- | --- | --- |
| 本地 main 与远端一致 | `git rev-parse main origin/main` | 两者均 `94a64e1f15d26f54a6601985fd13b1440fec8170` |
| 基线是源提交祖先 | `git merge-base --is-ancestor 94a64e1f agentic/node-link-owner` | exit 0（是祖先） |
| merge-base | `git merge-base main agentic/node-link-owner` | `94a64e1f15d26f54a6601985fd13b1440fec8170` = main 本身（分支直接从 main 长出） |
| 分支提交数 | `git rev-list --count main..agentic/node-link-owner` | 46 |
| 源提交 | `git rev-parse agentic/node-link-owner` | `4c5a3f0007cb72089f2879249f70eaeab1ab48ea`（勘误提交，单亲 `f02d2565`；仅改 `reports/rv1-wp8.md` 一行 + 勘误注记） |
| 上游已验收提交都在源内 | `git merge-base --is-ancestor <sha> agentic/node-link-owner` | 31 个固定提交（`028a4d2a`、`af9064ed`、`4e8a1075`、`041aeb04`、`13f0a16b`、`d127a802`、`443c6c09`、`50a5af42`、`038349b`、`319c77b7`、`59bf1058`、`2937850f`、`6b1f0b9c`、`27ad3f80`、`82e7c52c`、`fc0cbc03`、`5e2d17c7`、`8bc2ff3e`、`f630859`、`709ef55`、`69419dd`、`07b956c7`、`a3109c8a`、`ee5f6319`、`690b9172`、`008dcef5`、`5c869160`、`f9dbece1`、`0161a778`、`a882c06f`、`f02d2565`）**全部为祖先（ancestor OK）** |
| 候选也包含上游 | `git merge-base --is-ancestor f9dbece1 HEAD`（候选 worktree） | exit 0 |

**已验收证据链可读性**：`verification.md` 的 Handoff Index 中被引用的 54 个 `reports/*.md|*.log` 路径逐条存在且可读（脚本核对 `missing_count=0`）。其中 52 份 `.md` 报告已随 `f02d2565` 进入版本控制（候选树内可读）；原始 `.log`（如 `du1-pv1.log`、`pv5-windows-nodelink.log`、`wp1-deps.log`）按既有惯例仅为磁盘留存，位于权威规划根内可读。

**结论**：DU1 组成与 `plan.md` 一致；源提交包含全部已验收上游；证据链无缺行、无不可读路径。本报告只引用其他角色证据的 ID/路径/版本，不复制其内容、不替其判定。

## 2. 候选构造

| 项 | attempt 1（superseded） | attempt 2（**权威**） |
| --- | --- | --- |
| 基线 | `94a64e1f`（main） | `94a64e1f`（main，合入前再次核实未移动） |
| 源 | `f02d256578efa25f0e2740ccf71e893f928198d1` | `4c5a3f0007cb72089f2879249f70eaeab1ab48ea` |
| 命令 | `git merge --no-ff agentic/node-link-owner` | `git checkout -B integration/node-link-owner-du1 94a64e1f` + `git merge --no-ff agentic/node-link-owner` |
| 冲突 | **无** | **无** |
| 候选 SHA | `8d87df18d1fca553a49acf781ed52b1cdf61a4e7` | `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5` |
| 候选亲本 | `94a64e1f` + `f02d2565` | `94a64e1f` + `4c5a3f00` |
| 树与源提交一致性 | `git diff --stat HEAD <source>` 空（逐字节相同） | 同上为空 |
| 结果 | PV1 **FAIL**（`check:docs`，源提交同样红）→ 被主 Agent 裁决重建替代 | PV1/PV2/PV5 全绿 |

- 集成分支引用：`refs/heads/integration/node-link-owner-du1` → `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5`（attempt 1 的候选 SHA 已记录在原始日志与本表，不再持有 ref）。
- 依赖安装：合入前在该 worktree 执行 `npm ci`（exit 0，154 包，0 漏洞）；两轮候选之间 `package.json`/`package-lock.json` 零差异（`git diff --stat 8d87df18 HEAD -- package.json package-lock.json` 为空），故 attempt 1 的 `node_modules` 对 attempt 2 仍适用。
- 环境：node v24.19.0 / npm 12.0.2 / cargo 1.98.1 / rustc 1.98.1（`rust-toolchain.toml` 固定），Windows 11 x86_64-pc-windows-msvc，16 逻辑核。

## 3. 候选 Project Verify（attempt 2，权威轮次）

权威组合命令与逐子检查全部在候选提交 `92fb9fdc` 上执行；原始输出见 `reports/du1-candidate.log`（`############ CANDIDATE ATTEMPT 2` 之后）。

**PV1 = `npm run verify`：exit 0**（22:31:46 → 22:33:53，约 127 s；`reports/du1-candidate.log` §PV1 attempt 2）

| 子检查（命令） | 退出码 | 关键输出 |
| --- | --- | --- |
| `npm run check`（组合入口，含以下 10 项） | 0 | — |
| `npm run check:schemas` | 0 | schema fixtures OK: 118 valid, 25 invalid, 39 event views bound |
| `npm run check:commands` | 0 | command catalog OK: 12 commands |
| `npm run check:errors` | 0 | error registry OK: 58 codes across 2 protocols |
| `npm run check:features` | 0 | feature registry OK: 11 feature ids across 2 protocols |
| `npm run check:assets` | 0 | contract assets OK: 17 schemas, 156 fixture files, 12 transcript vectors re-encoded, 20 negative vectors, 2 SAS values |
| `npm run check:acp` | 0 | ACP compatibility matrix OK: 25 methods / 11 updates / … 10 test families（71 行） |
| `npm run check:docs` | 0 | doc link check OK（attempt 1 在此红，见 §5-I1） |
| `npm run check:boundaries` | 0 | crate boundaries OK（12 crate，见 PV2） |
| `npm run check:drift` | 0 | 合同漂移门禁（§5/§7 ↔ core::ports / migrate.rs）通过 |
| `npm run check:agentic` | 0 | agentic 门禁 + `sync-agentic-host-entrypoints.mjs --check` 通过 |
| `cargo fmt --all -- --check` | 0 | 无差异 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | 无警告 |
| `cargo test --locked --workspace --all-features` | 0 | 85 个测试目标，**973 passed / 0 failed / 2 ignored**（2 ignored 为仓库声明的辅助/生成器，与基线同数） |

**PV2 = `node scripts/check-crate-boundaries.mjs`：exit 0** —— `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（含 WP1 新增的 `server → acpr-wire` 格）。

**PV5 = `cargo test --locked -p server -p app --all-features`（本机 Windows）：exit 0** —— 15 个目标、**394 passed / 0 failed / 0 ignored**、83 s（22:35:49 → 22:37:12）；`-- --list` 交叉核对 **declared 394 == executed 394**，每条 `test result` 行为 `0 filtered out`（无跳过、无 `--skip`）。

| 目标 | 结果 |
| --- | --- |
| server lib | 281 passed / 0 failed |
| app lib | 52 passed / 0 failed |
| `tests\daemon_lifecycle.rs` | 11 passed / 0 failed（60.35 s，真实起停 daemon 子进程） |
| `tests\node_link_e2e.rs` | 2 passed / 0 failed（3.20 s）= 受控路径全链路 `the_controlled_path_runs_end_to_end_and_revocation_propagates` + TLS direct `tls_direct_terminates_the_same_handshake` |
| `tests\node_link_listener.rs` | 8 passed / 0 failed（真实 `TcpListener` 绑定 / TLS 终止 / 失败关闭） |
| `tests\local_admin_channel.rs` / `local_admin_schema_drift.rs` / `local_endpoint_naming.rs` / `local_endpoint_windows.rs` / `audit_export.rs` / `cli_commands.rs` | 14 / 6 / 4 / 4 / 1 / 11 passed，均 0 failed |
| `tests\local_endpoint_unix.rs` | 0 条（整文件 `#[cfg(unix)]`，本机不参与编译，由 Linux CI 覆盖——如实记录，不用 Linux 结果替代） |
| app `main` / Doc-tests app / Doc-tests server | 0 条（仓库既有约定，非静默跳过） |

平台差异用例**真实执行并通过**：`transport::net::tests::direct_mode_permissions_are_unverifiable_on_this_platform`、`transport::net::permissions::tests::windows_inspection_is_unverifiable`、`transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable`、`transport::local::platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`，另有 `crates/server/tests/local_endpoint_windows.rs` 4/4。

## 4. 已登记 flake（PV5）

| 项 | 内容 |
| --- | --- |
| 用例 | `crates/app/tests/daemon_lifecycle.rs::the_status_result_keeps_the_documented_field_set`（失败点是该用例末行 `assert!(daemon.stop().success())`，即 daemon 子进程以非零码退出；**不是** `wait_exit` 的「未在 15 s 期限内退出」超时路径） |
| 首次观察 | 候选 attempt 1（`8d87df18`，源 `f02d2565`）执行 PV5：exit 1，`daemon_lifecycle` 10 passed / 1 failed（60.44 s），同轮其余 10 条通过 |
| 观察频率 | 约 **1/6**：同一二进制的 6 次执行中失败 1 次——PV5 attempt 1 FAIL；PV5 attempt 2 PASS、单用例 `--exact` 隔离跑 3/3 PASS、`daemon_lifecycle` 全量并行跑 2/2 PASS；另有 workspace 全量轮次 PASS |
| 是否候选引入 | **否**。`5c869160 → f02d256` 之间 `crates/app`、`crates/server` 代码零改动（`git diff --stat 5c869160 f02d256 -- crates/` 仅 `crates/core/src/broker.rs` +56 行，内容为注释、常量钉死断言与一条 core 用例）；`5c869160` 上的 PV5 记录为 PASS（394/0/0）。在候选 attempt 2（`92fb9fdc`）上同一用例 PASS |
| 处置 | 主 Agent 裁决（2026-09-26）：按「已登记 flake + 以 PASS 轮次为准」处理，本报告登记用例名与频率；同项由主 Agent 列入 `verification.md` 的 Failures and Retests 并在后续切片观察，若候选/主分支轮次再出现同类失败则升级为缺陷立案 |
| 证据 | `reports/du1-candidate.log` §PV5（attempt 1 失败原文，含 panic 行与 `test result: FAILED`）、同文件 §PV5 attempt 2 与 §flake probe 1/2 |

## 5. Issues（失败、恢复与复用）

**I1 — attempt 1 的 PV1 红：`check:docs`（已闭环，源分支勘误 → 重建候选）**

- 失败原文：`error: openspec/changes/node-link-owner/reports/rv1-wp8.md 第 36 行的「第 7.2 节」在 docs/CONFIG_REFERENCE.md 中不存在（该引用指向它，但该文档没有这个小节）` / `doc link check failed: 1 problem(s)`。
- 根因：该行原文为「CONFIG_REFERENCE 第一节第三分支与「第 7.2 节」引用：通过」，其中 `§7.2` 实指 `docs/SECURITY_DESIGN.md` §7.2（WP2 裁决的 Host 口径依据），未写文档名；doc-links 门禁按「同一子句内紧邻指名的文档」保守归属，把「第 7.2 节」归到同句唯一具名的 `CONFIG_REFERENCE.md`（该文件无 §7.2 小节）。
- **不是集成引入**：在实现 worktree（`D:\Project\acp-remote-wt\node-link-owner` @ `f02d2565`，工作区干净）只读执行同一脚本，得到同一行错误与 `exit 1`。即分支 tip `f02d2565` 自身不满足 PV1；`verification.md` 中 2.24/3.16 的绿证据绑定的是更早的 `0161a778`，而把 `reports/` 提交进仓库的 `f02d2565` 之后没有候选级检查。
- 处置（未由本角色取舍）：按派单边界（只解决集成冲突、不改产品代码）停下来上报；主 Agent 裁决选 (A)，在源分支提交勘误 `4c5a3f00`（补 `SECURITY_DESIGN` 文档名 + 勘误注记，仅 1 行改动），本角色据此重建候选（`92fb9fdc`）并全量重跑三项检查 → `check:docs` exit 0。
- 证据失效/复用：attempt 1 的 PV1 证据随候选重建**失效**（`INVALID`，见 handoff_index）；attempt 2 的 PV1/PV2/PV5 为在权威候选上的**新证据**。cargo/npm 构建缓存与 `node_modules` 属资源复用（依赖集合零变化），不改变结论适用性。

**I2 — attempt 1 的 PV5 flake**：见 §4；权威 PV5 结果为 attempt 2 的 PASS。

**其他复现记录（非仓库问题，如实保留）**：attempt 2 的 `-- --list` 交叉核对首次使用了复数式 `grep 'tests, …'`，漏掉恰好 1 条用例的目标（`audit_export` 1 条），导致 declared 合计误算为 393；已用正确式重跑并入日志（declared 394 == executed 394）。原始日志中两条命令的输出都在，勘误行紧随其后。

## 6. 基线复核与防竞态

| 时点 | 命令 | 结果 |
| --- | --- | --- |
| attempt 1 前 | `git rev-parse main origin/main` | `94a64e1f…`（两者一致） |
| attempt 2 前（收到 `4c5a3f00` 后） | `git rev-parse main origin/main` + `git rev-parse agentic/node-link-owner` | main 仍 `94a64e1f…`；源为 `4c5a3f00…`（`f02d2565` 的单亲子提交） |
| 全部检查跑完后 | `git rev-parse main origin/main` | 仍 `94a64e1f…`（未移动，候选无需重建） |

- 目标基线在整个候选构造与验证期间未移动，因此候选与在其上采集的检查证据仍然对应当前 main；**若 6.6 合入前 main 又移动，本候选与三项检查证据须重建重跑**（`plan.md` Failure and Recovery 的「基线变化时重建候选并重验」）。
- 防竞态：本角色是当前唯一写入该集成 worktree/分支的执行者；主 Agent 工作区的未提交改动（`M docs/DEVELOPMENT_PLAN.md` 与未跟踪的 `openspec/changes/node-link-owner/`）未被读取为输入、也未被修改；`git status --porcelain` 在候选 worktree 内始终为空。

## 7. 资源与环境

- worktree `D:\Project\acp-remote-wt\nl-owner-integration`（分支 `integration/node-link-owner-du1`），`target/` 为 worktree 自带目录（`cargo metadata` 的 `target_directory` = `D:\Project\acp-remote-wt\nl-owner-integration\target`，7.8G），**未与实现 worktree 或主仓库共用**；`CARGO_TARGET_DIR` 未设置（即默认本 worktree 的 `target/`）。
- 端口：PV5/PV4 集成用例一律 `127.0.0.1:0` 随机端口；临时数据目录由用例自建自删，跑完后 `/tmp/acpr-*` 计数 0、无 `acpr*` 残留进程、无锁文件/证书残留。
- 保留资源：候选 worktree + `target/` 保留供 6.4 独立 review 与 6.6 合入（合入指令到达前不清理、不切换分支）。

## 8. handoff_index

```yaml
handoff_index:
  - task_id: "5.1"
    role: integrator
    phase: integrate
    stage: candidate
    target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本轮实际记录：独立集成子 Agent session 01a0de15-6c33-755a-9d1a-afce9d68f8c7（PI_SUBAGENT_CHILD=1，父 01a0db22-…），不继承实现对话，主 Agent 未兼任；独立 worktree 与目标分支引用见本报告 §2/§6/§7"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.1"
    role: integrator
    phase: verify-baseline
    stage: candidate
    target_revision: "94a64e1f15d26f54a6601985fd13b1440fec8170"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "构造前、重建前、全部检查后各核实一次：git rev-parse main=origin/main=94a64e1f…；基线是源提交祖先（merge-base --is-ancestor exit 0），merge-base = main 本身；见本报告 §1/§6"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.2"
    role: integrator
    phase: build-candidate
    stage: candidate
    target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "git merge --no-ff agentic/node-link-owner（源 4c5a3f00）于基线 94a64e1f；无冲突；候选亲本 94a64e1f + 4c5a3f00；git diff --stat HEAD <source> 为空（树逐字节一致）；worktree 内 npm ci exit 0；见本报告 §2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify exit 0（含 10 道合同门禁逐项 0、cargo fmt 0、cargo clippy 0、workspace 测试 973 passed/0 failed/2 ignored/85 目标）；原始日志 reports/du1-candidate.log §CANDIDATE ATTEMPT 2 / §ATTEMPT2 PV1 subcheck"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "node scripts/check-crate-boundaries.mjs exit 0：12 个 crate 依赖方向与 MODULE_ARCHITECTURE §5 矩阵一致（含 server→acpr-wire 格）；独立执行与 npm run check:boundaries 两次都退 0；日志 §ATTEMPT2 PV2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows：cargo test --locked -p server -p app --all-features exit 0，15 目标 394 passed/0 failed/0 ignored，declared 394 == executed 394（--list 交叉核对，0 filtered out）；受控路径全链路 node_link_e2e 2/2、listener 8/8、daemon_lifecycle 11/11，平台差异用例真实执行；日志 §ATTEMPT2 PV5"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "8d87df18d1fca553a49acf781ed52b1cdf61a4e7"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-integrate.md"
    result: FAIL
    evidence_status: INVALID
    applicability_basis: "attempt 1 候选（源 f02d2565）上 npm run verify exit 1，唯一红点 check:docs（reports/rv1-wp8.md:36 的 §7.2 归属）；同一失败在源提交 f02d2565 上只读复现（exit 1）。该候选已被源分支勘误 4c5a3f00 后的候选 92fb9fdc 取代，本条失效；受影响任务/证据：6.3 的 PV1 与依赖其的 6.4/6.6 必须以 92fb9fdc 轮次为准（见本报告 §5-I1）"
    source_evidence:
      id: PV1
      report_path: "reports/du1-integrate.md"
      target_revision: "8d87df18d1fca553a49acf781ed52b1cdf61a4e7"
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "8d87df18d1fca553a49acf781ed52b1cdf61a4e7"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/du1-integrate.md"
    result: FAIL
    evidence_status: INVALID
    applicability_basis: "attempt 1 候选上的 flake：daemon_lifecycle::the_status_result_keeps_the_documented_field_set 的 daemon.stop().success() 断言失败（子进程非零退出，非超时），同轮 10/11 通过；该候选已被 92fb9fdc 取代，本条失效；权威 PV5 结论为 92fb9fdc 上的 PASS，flake 本身按主 Agent 裁决登记（本报告 §4）。受影响任务/证据：6.3 的 PV5、6.6 合入前若复现同类失败须升级为缺陷立案"
    source_evidence:
      id: PV5
      report_path: "reports/du1-integrate.md"
      target_revision: "8d87df18d1fca553a49acf781ed52b1cdf61a4e7"
```

## 9. 未解决项与下一步

- **未执行**：6.4 独立 review（`reports/rv2-du1.md`）、6.5 premerge 门禁、6.6 本地合入 `refs/heads/main`（均非本角色本轮范围；合入待主 Agent 指令）。
- **留给 6.4 reviewer 的输入**：候选 `92fb9fdc`（相对 main 的差异 = 46 个源提交带来的全部改动，无冲突解决差异，唯一"新增交互"是 `5c869160` 触及的 `crates/core/src/broker.rs` +56 行（注释/常量断言/core 用例）与 `f02d2565`/`4c5a3f00` 提交的变更目录工件）；本报告 §4 的 flake 与 §5 的勘误上下文。
- **风险**：① 若 6.6 前 main 移动，候选与三项检查证据失效、须重建重跑；② §4 的 flake 未定位根因（本角色范围内只做频率登记与排除回归）；③ `.log` 类原始证据未入版本控制，仅权威规划根内可读（既有惯例，非本轮引入）；④ 本地 `npm run verify` 不含 `cargo-deny` 与 `gitleaks`，本变更新增依赖（axum / rustls 系等）的许可证与 advisory 判定只在 CI 完成，本轮不作为证据。
- **下一步（主 Agent）**：把本报告与 `reports/du1-candidate.log` 关联进 `verification.md` 的 6.1–6.3 行与 Checks 表 → 派发 6.4 独立候选 review → 通过后按指令触发 6.6（premerge 门禁 + 本地合入）。
