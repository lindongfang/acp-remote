# DU1 集成报告（`node-link-owner`；任务 5.1、6.1–6.3 候选构造与 6.6 本地合入）

> 本报告只覆盖集成 Agent 自身执行的范围（核对 → 候选构造 → 候选 Project Verify → 本地合入 → 主分支检查 → 报告）。
> 本地合入已执行（`git merge --ff-only`，无新提交）；**未推送远端、未动远端分支、未回滚**；
> 未修改权威 `plan.md` / `tasks.md` / `verification.md`。
> **轮次说明**：本报告按「候选修订 / 阶段」分层记录。当前轮次为 **round 3 候选 `654c0c19`（§0）**
> 与其后的 **主分支轮次（§10，stage=main）**；§1–§9 保留 round 1（attempt 1 `8d87df18`）与
> round 2（attempt 2 `92fb9fdc`）的历史记录，各行的 `target_revision` 即为其适用版本；
> 历史轮次的 PASS 不构成本候选/主分支的证据。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | 5.1、6.1、6.2、6.3（候选构造与候选 Project Verify）、**6.6（本地合入）与 6.7 前半（主分支 PV1/PV2/PV5）** |
| `role` | integrator |
| `phase` | integrate（候选构造 + candidate Project Verify）+ merge（本地合入 + 主分支检查） |
| `agent_context` | 独立集成子 Agent（worker 角色，**不继承任何实现对话**；`PI_SUBAGENT_CHILD=1`，父 session `01a0db22-b46e-7543-a1f5-d4621ef3e22f`）；候选构造工作目录 `D:\Project\acp-remote-wt\nl-owner-integration`，合入执行在工作目录 `D:\Project\acp-remote`（主检出；主 Agent 授权，主 Agent 不兼任）。主 Agent 在 `verification.md` 的 Merge History 中记录本角色为「worker 子 Agent c51411bd」 |
| `target_revision` | **合入后：`refs/heads/main` = `654c0c1944c75bbc016eab775f3f3ff8aca9bf35`**（`stage=main`；即 round 3 候选，ff-only 合入无新提交）；候选阶段目标基线 `94a64e1f15d26f54a6601985fd13b1440fec8170`（合入前共 5 次核实未移动，`origin/main` 同 SHA）；`origin/main` 仍未移动 |
| `scope` | ①核对 DU1 组成、源/基线包含关系与已验收证据可读性；②基于已核实 main 构造固定候选；③候选 PV1 + PV2 + PV5；④报告；⑤**预处理两个前置障碍后 `git merge --ff-only` 本地合入 main**；⑥主分支 PV1/PV2/PV5；**不推送远端** |
| `changes` | **无产品代码改动**；**无冲突解决**（候选树与源提交逐字节相同）。实际写入：①本地分支 `refs/heads/integration/node-link-owner-du1` 与候选 merge commit；②主检出 `refs/heads/main` 快进到候选（含 47 个源提交带来的全部改动）；③`docs/DEVELOPMENT_PLAN.md` 行尾噪声恢复（内容零差异）；④未跟踪变更目录移出至备份；⑤报告与原始日志更新 |
| `checks` | **主分支（stage=main，当前权威）**：PV1 = PASS（exit 0）、PV2 = PASS（exit 0）、PV5 = PASS（exit 0，本分支上真实执行）；round 3 候选：PV1/PV2 PASS + PV5 REUSED（见 §0）；round 2：PV1/PV2/PV5 全 exit 0（历史，见 §3）；round 1：PV1/PV5 FAIL（已 INVALID，见 §5） |
| `issues` | I1：attempt 1 的 `check:docs` 红（源 `f02d2565` 自身同样红）→ 主 Agent 源分支勘误 `4c5a3f00` 后重建候选并复绿；I2：PV5 attempt 1 的 flake → 按主 Agent 裁决登记（§4，主分支轮次未复现）；I3：源分支新增规划工件提交 `5f62e77f` → 重建候选 `654c0c19`；I4：合入前置障碍两项（RV2-DU1-F3）已按序处理并留证（§10.1） |
| `result` | **PASS（合入完成，`stage=main`）**：ff-only 合入成功、`refs/heads/main` = `654c0c19`、主分支 PV1/PV2/PV5 全 exit 0、未推送。**仍未完成**：主 Agent 侧的 `verification.md` 回写、`agentic-verify` 最终验收与归档；远端交付仍需按 `AGENTS.md` §8 走 PR（本轮不推送） |
| `evidence_paths` | `reports/du1-candidate.log`（原始日志，13563 行；含 round 1/2/3 候选段、「round 3 附注」规划根对照运行两次、**`MAIN BRANCH ROUND` 主分支段**与报告编辑后的 check:docs 复核）、`reports/du1-integrate.md`（本报告） |
| `resource_cleanup` | 合入后保留：候选 worktree 与其自带 `target/`（约 9.6G）、主检出自带 `target/`（约 22G）与 `node_modules`（未重建）；**备份目录 `D:\Project\acp-remote-wt\planning-backup-node-link-owner\`（80 文件）保留待主 Agent 处理权威副本**；各轮跑完后无 `acpr*` 残留进程、`/tmp/acpr-*` 0 个；未删除任何规划工件 |

## 0. 当前权威轮次：round 3（候选 `654c0c19`，2026-09-26 追加）

触发：6.4 的 RV2-DU1 候选检视 PASS（`reports/rv2-du1.md`），其 F1 工件问题（本报告两处 doc 引用）已由主 Agent 修复并复绿，F4 的 `plan.md` basis 措辞也已修；源分支新增规划工件同步提交 `5f62e77f`。本轮由派单触发重建候选并重跑候选检查。

### 0.1 源提交与基线复核（6.1 职责）

| 项 | 命令 / 核对 | 结果 |
| --- | --- | --- |
| 目标基线未移动 | `git rev-parse refs/heads/main` + `git rev-parse origin/main` | 两者均 `94a64e1f15d26f54a6601985fd13b1440fec8170`（与 round 1/2 相同） |
| 新源提交 | `git rev-parse agentic/node-link-owner` | `5f62e77f320645c5dea55d0723ab743d24302297`（单亲 `4c5a3f00`；subject：同步候选阶段规划工件与证据，含 rv2-du1 与 doc-links 勘误） |
| 源提交内容 | `git diff --stat 4c5a3f00 5f62e77f` | 5 文件、+333/−7：`plan.md`（1 行 basis）、`tasks.md`（5.1/5.2/6.1–6.3 勾选）、`verification.md`（6.4 RV2-DU1 行 + EX7/EX8 + Merge History）、`reports/du1-integrate.md`（新增）、`reports/rv2-du1.md`（新增） |
| **零代码变化** | `git diff --stat 4c5a3f00 5f62e77f -- . ':(exclude)openspec/'` | 空（非 `openspec/` 路径零差异） |

### 0.2 候选构造（6.2 职责）

| 项 | 值 |
| --- | --- |
| 基线 | `94a64e1f15d26f54a6601985fd13b1440fec8170`（`refs/heads/main`） |
| 源 | `5f62e77f320645c5dea55d0723ab743d24302297` |
| 命令 | `git checkout -B integration/node-link-owner-du1 94a64e1f` + `git merge --no-ff agentic/node-link-owner` |
| 冲突 | **无**（无冲突解决差异） |
| 候选 SHA | `654c0c1944c75bbc016eab775f3f3ff8aca9bf35` |
| 候选亲本 | `94a64e1f` + `5f62e77f` |
| 树与源提交一致 | `git diff --stat HEAD agentic/node-link-owner` 空（逐字节相同） |
| 与 round 2 候选的差异 | `git diff --name-only 92fb9fdc 654c0c19` = 5 个文件，全在 `openspec/changes/node-link-owner/`（`plan.md`、`tasks.md`、`verification.md`、`reports/du1-integrate.md`、`reports/rv2-du1.md`）；`git diff --stat 92fb9fdc HEAD -- . ':(exclude)openspec/'` 空 |
| 依赖安装 | 沿用 round 2 的 `npm ci`：`git diff --stat 92fb9fdc HEAD -- package.json package-lock.json` 空（依赖集合零变化） |
| 分支 ref | `refs/heads/integration/node-link-owner-du1` → `654c0c1944c75bbc016eab775f3f3ff8aca9bf35` |

### 0.3 round 3 候选检查（6.3 职责）

**PV1 = `npm run verify`：exit 0**（在候选 `654c0c19` 上重跑；原始输出见 `reports/du1-candidate.log` §`CANDIDATE ROUND 3`）

| 子检查（命令） | 退出码 |
| --- | --- |
| `npm run check:schemas` / `check:commands` / `check:errors` / `check:features` / `check:assets` / `check:acp` | 0 / 0 / 0 / 0 / 0 / 0 |
| **`npm run check:docs`**（本轮重点：扫描规划工件内容，含新提交的 `reports/rv2-du1.md` 与本报告） | **0** |
| `npm run check:boundaries` / `check:drift` / `check:agentic` | 0 / 0 / 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 |
| `cargo test --locked --workspace --all-features` | 0（85 个目标，**973 passed / 0 failed / 2 ignored**，与 round 2 逐项相同） |

**PV2 = `node scripts/check-crate-boundaries.mjs`：exit 0** —— `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（本候选上重跑；另经 `npm run check:boundaries` 一并执行，同样 0）。

**PV5（本机 Windows，`cargo test --locked -p server -p app --all-features`）= PASS / REUSED**（复用 round 2 候选 `92fb9fdc` 轮次：exit 0，15 目标 394 passed / 0 failed / 0 ignored，declared 394 == executed 394）

适用依据（逐项，按派单授权与本角色核实）：

1. **零代码变化**：`git diff --stat 92fb9fdc 654c0c19 -- . ':(exclude)openspec/'` 为空，且两候选间的全部差异就是 §0.2 表中那 5 个 `openspec/changes/node-link-owner/` 规划/证据文件；PV5 的被测对象是 `crates/server` 与 `crates/app` 的测试目标及其依赖，该集合逐字节未变。
2. **`openspec/` 不在测试面内**：全仓库 `include_str!`/`include_bytes!` 只嵌入 `compatibility/`、`schemas/`、`fixtures/` 下的机器合同资产，无一处指向 `openspec/**`；`crates/**` 中出现的 `openspec/...` 全部是文档注释指针（仅注释文本），无运行时读取。因此规划工件内容变化不可能改变 PV5 任何用例的输入或断言。
3. **命令、环境、配置未变**：同一集成 worktree、同一 `target/`（worktree 自带）、同一固定工具链（cargo/rustc 1.98.1）、同一条命令与参数；round 2 的 PV5 已覆盖 cfg(windows) 与受控路径全链路用例并留证。
4. **本轮工件变化已被 PV1 覆盖**：`check:docs` 会扫描规划工件内容，因此 5 个文件的文字变化由 round 3 的 PV1（exit 0）实际验证，不属于 PV5 的判据面。

### 0.4 本报告自身的版本关系（供主 Agent 提交时注意）

- 本节的编辑发生在**权威规划根** `D:\Project\acp-remote\openspec\changes\node-link-owner\reports\du1-integrate.md`（该目录在主仓库为未跟踪文件）；候选 `654c0c19` 的树内携带的是 `5f62e77f` 提交的快照版本（即 RV2-DU1 F1 修复后的版本），不含本节的 round 3 记录。
- 因此 round 3 的 `check:docs` 扫描的是候选树内快照；本节新增文字对 `check:docs` 的兼容性另在规划根做了同名脚本的对照运行（结果见 §6 表末行），供主 Agent 提交前参考。

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

## 2. 候选构造（round 1 / round 2；round 3 见 §0）

| 项 | attempt 1（superseded） | attempt 2（round 2，已被 round 3 的 `654c0c19` 取代） |
| --- | --- | --- |
| 基线 | `94a64e1f`（main） | `94a64e1f`（main，合入前再次核实未移动） |
| 源 | `f02d256578efa25f0e2740ccf71e893f928198d1` | `4c5a3f0007cb72089f2879249f70eaeab1ab48ea` |
| 命令 | `git merge --no-ff agentic/node-link-owner` | `git checkout -B integration/node-link-owner-du1 94a64e1f` + `git merge --no-ff agentic/node-link-owner` |
| 冲突 | **无** | **无** |
| 候选 SHA | `8d87df18d1fca553a49acf781ed52b1cdf61a4e7` | `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5` |
| 候选亲本 | `94a64e1f` + `f02d2565` | `94a64e1f` + `4c5a3f00` |
| 树与源提交一致性 | `git diff --stat HEAD <source>` 空（逐字节相同） | 同上为空 |
| 结果 | PV1 **FAIL**（`check:docs`，源提交同样红）→ 被主 Agent 裁决重建替代 | PV1/PV2/PV5 全绿 |

- 集成分支引用：`refs/heads/integration/node-link-owner-du1` 在 round 2 指向 `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5`，**round 3 已重置并指向 `654c0c1944c75bbc016eab775f3f3ff8aca9bf35`**（attempt 1 与 round 2 的候选 SHA 已记录在原始日志与本表，不再持有 ref；round 2 的 PV5 证据仍按 §0.3 的适用依据被复用）。
- 依赖安装：合入前在该 worktree 执行 `npm ci`（exit 0，154 包，0 漏洞）；两轮候选之间 `package.json`/`package-lock.json` 零差异（`git diff --stat 8d87df18 HEAD -- package.json package-lock.json` 为空），故 attempt 1 的 `node_modules` 对 attempt 2 仍适用。
- 环境：node v24.19.0 / npm 12.0.2 / cargo 1.98.1 / rustc 1.98.1（`rust-toolchain.toml` 固定），Windows 11 x86_64-pc-windows-msvc，16 逻辑核。

## 3. 候选 Project Verify（round 2：attempt 2，已被 round 3 取代；当前权威结果见 §0.3）

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
| **round 3 前（收到 `5f62e77f` 后）** | `git rev-parse refs/heads/main` + `git rev-parse origin/main` + `git rev-parse agentic/node-link-owner` | main 与 origin/main 仍 `94a64e1f…`；源为 `5f62e77f…`（`4c5a3f00` 的单亲子提交） |
| **round 3 全部检查跑完后** | `git rev-parse main origin/main` | 仍 `94a64e1f…`（自 round 1 起共 4 次核实均未移动） |
| **round 3 报告编辑后（规划根对照运行，两次）** | `node scripts/check-doc-links.mjs`（在 `D:\Project\acp-remote` 规划根，非候选检查，仅供参考） | 两次均 exit 0；本报告 round 3 编辑完成后复跑：`doc links OK: 379 relative links, 5764 section refs across 333 markdown files`（首次为 5761 section refs）；原始输出见 `reports/du1-candidate.log` 的「round 3 附注」段 |

- round 3 变更不改变防竞态结论：本角色仍是当前唯一写入该集成 worktree/分支的执行者；工具链、命令、`target/` 均与 round 2 相同，仅候选引入的规划工件内容不同。
| 全部检查跑完后 | `git rev-parse main origin/main` | 仍 `94a64e1f…`（未移动，候选无需重建） |

- 目标基线在整个候选构造与验证期间未移动，因此候选与在其上采集的检查证据仍然对应当前 main；**若 6.6 合入前 main 又移动，本候选与三项检查证据须重建重跑**（`plan.md` Failure and Recovery 的「基线变化时重建候选并重验」）。
- 防竞态：本角色是当前唯一写入该集成 worktree/分支的执行者；主 Agent 工作区的未提交改动（`M docs/DEVELOPMENT_PLAN.md` 与未跟踪的 `openspec/changes/node-link-owner/`）未被读取为输入、也未被修改；`git status --porcelain` 在候选 worktree 内始终为空。

## 7. 资源与环境

- worktree `D:\Project\acp-remote-wt\nl-owner-integration`（分支 `integration/node-link-owner-du1`），`target/` 为 worktree 自带目录（`cargo metadata` 的 `target_directory` = `D:\Project\acp-remote-wt\nl-owner-integration\target`，round 3 后约 9.6G），**未与实现 worktree 或主仓库共用**；`CARGO_TARGET_DIR` 未设置（即默认本 worktree 的 `target/`）。round 3 未新增资源，仅复用该 target 的增量构建。
- 端口：PV5/PV4 集成用例一律 `127.0.0.1:0` 随机端口；临时数据目录由用例自建自删，跑完后 `/tmp/acpr-*` 计数 0、无 `acpr*` 残留进程、无锁文件/证书残留。
- 保留资源：候选 worktree + `target/` 保留供 6.6 合入与主分支复核（合入指令到达前不清理、不切换分支）。

## 8. handoff_index

> 本块按 `target_revision` 分层：每一行只对该行声明的固定提交生效。round 1（`8d87df18`）与 round 2（`92fb9fdc`）的行是各自修订的历史执行记录（含两条已 `INVALID` 的失败行），**不构成本候选/主分支的证据**；候选 `654c0c19` 的证据是 round 3 行（6.1/6.2 与 PV1/PV2 = NEW；PV5 = REUSED，复用 round 2 的 `92fb9fdc` 轮次，依据见 §0.3）；**主分支（`654c0c19` 即合入后的 `refs/heads/main`）的证据是本块末尾的 `stage: main` 行**（全部为 NEW，PV5 在 main 上重新真实执行；见 §10）。

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
  # ---- round 3（2026-09-26 追加；当前候选 654c0c19 的权威证据行）----
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
    applicability_basis: "round 3 重建前与全部检查后各核实一次：git rev-parse refs/heads/main = git rev-parse origin/main = 94a64e1f…（自 round 1 起共 4 次核实均未移动）；新源提交 5f62e77f…（单亲 4c5a3f00，非 openspec/ 路径零差异）；见本报告 §0.1/§6"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.2"
    role: integrator
    phase: build-candidate
    stage: candidate
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "git checkout -B integration/node-link-owner-du1 94a64e1f + git merge --no-ff agentic/node-link-owner（源 5f62e77f）；无冲突；候选亲本 94a64e1f + 5f62e77f；git diff --stat HEAD <source> 为空（树逐字节一致）；相对 round 2 候选的差异仅 5 个 openspec/changes/** 规划工件，package.json/lock 与 npm ci 结果沿用；见本报告 §0.2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "round 3 在该候选上重跑 npm run verify exit 0：10 道合同门禁逐项 0（含重点 check:docs 0、扫描候选树内新提交的 reports/rv2-du1.md 与 reports/du1-integrate.md 快照）、cargo fmt 0、cargo clippy 0、workspace 测试 973 passed/0 failed/2 ignored（85 目标）；原始日志 reports/du1-candidate.log §CANDIDATE ROUND 3"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "round 3 在该候选上重跑 node scripts/check-crate-boundaries.mjs exit 0：12 个 crate 依赖方向与 MODULE_ARCHITECTURE §5 矩阵一致（含 server→acpr-wire 格）；同轮 npm run check:boundaries 也退 0；日志 §ROUND3 PV2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.3"
    role: integrator
    phase: candidate-check
    stage: candidate
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "按派单授权复用 round 2 候选 92fb9fdc 轮次（本机 Windows：cargo test --locked -p server -p app --all-features exit 0，15 目标 394 passed/0 failed/0 ignored，declared 394 == executed 394）。适用性逐项：① 两候选间非 openspec/ 路径差异为空（crates/** 及全部构建输入逐字节未变）；② openspec/ 不在测试面内（全仓库 include_str!/include_bytes! 无一处指向 openspec/**，crates/** 中的 openspec 引用仅为文档注释）；③ 命令、worktree、worktree 自带 target/、rust-toolchain.toml 固定工具链均未变；④ 本轮工件文本变化由 round 3 的 PV1 check:docs 实际覆盖（见 §0.3）"
    source_evidence:
      id: PV5
      report_path: "reports/du1-integrate.md"
      target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5"
  # ---- 主分支轮次（stage=main，目标 = 合入后的 refs/heads/main = 654c0c19）----
  - task_id: "6.6"
    role: integrator
    phase: merge
    stage: main
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合入前复核 main=origin/main=94a64e1f 且 git merge-base --is-ancestor 94a64e1f 654c0c19 = exit 0；两项前置障碍（RV2-DU1-F3）按序处理并留证（docs/DEVELOPMENT_PLAN.md 零内容差异恢复；未跟踪变更目录移到 D:/Project/acp-remote-wt/planning-backup-node-link-owner）；git merge --ff-only 654c0c19 exit 0（fast-forward，无新提交）；合入后 refs/heads/main = 654c0c19，origin/main 仍 94a64e1f（未推送、未动远端分支、未回滚）；见本报告 §10.1/§10.2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.7（前半）"
    role: integrator
    phase: main-branch-check
    stage: main
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在主检出 D:/Project/acp-remote（refs/heads/main = 654c0c19）执行 npm run verify exit 0：10 道合同门禁逐项 0（check:schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）、cargo fmt 0、cargo clippy 0、workspace 测试 973 passed/0 failed/2 ignored（85 目标）；原始日志 reports/du1-candidate.log §MAIN BRANCH ROUND"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.7（前半）"
    role: integrator
    phase: main-branch-check
    stage: main
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支上执行 node scripts/check-crate-boundaries.mjs exit 0：12 个 crate 依赖方向与 MODULE_ARCHITECTURE §5 矩阵一致（含 server→acpr-wire 格）；同轮 npm run check:boundaries 也退 0；日志 §MAIN PV1 subcheck: check:boundaries / §MAIN PV2"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.7（前半）"
    role: integrator
    phase: main-branch-check
    stage: main
    target_revision: "654c0c1944c75bbc016eab775f3f3ff8aca9bf35"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/du1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支上重新真实执行（不再 REUSED）：cargo test --locked -p server -p app --all-features exit 0，15 目标 394 passed/0 failed/0 ignored，-- --list 交叉核对 declared 394 == executed 394（0 filtered out）；daemon_lifecycle 11/11（60.38 s；§4 登记的 flake 用例 PASS，未复现）、node_link_e2e 2/2（3.19 s）、node_link_listener 8/8，cfg(windows) 与平台差异用例真实执行；日志 §MAIN BRANCH ROUND"
    source_evidence: NOT_APPLICABLE
```

## 9. 未解决项与下一步

> 合入与主分支轮次已执行完毕（任务 6.6 + 6.7 前半），逐项记录见 **§10**。本节只保存仍需主 Agent 处理的开口项。

- **已不再待办**：6.6 本地合入（已 ff-only 完成，`refs/heads/main` = `654c0c19`，未推送）；6.4 RV2-DU1 候选 review 已 PASS（`reports/rv2-du1.md`，reviewer b35b3fef）。
- **留给主 Agent**：① `verification.md` 回写（6.6/6.7 行、Checks 表、Merge History、主分支轮次证据）；② 备份目录 `D:\Project\acp-remote-wt\planning-backup-node-link-owner\` 的权威副本处理（§10.1 列出 3 个 `.md` 差异与 15 个日志）；③ 将本报告（曾为未提交修改 `M`，期间被一份较早副本覆盖后又由本角色补回，见 §10.5）与 `reports/du1-candidate.log` 的新段落凡入版本控制前先跑 `npm run check`；④ 最终验收（`agentic-verify`）与归档；⑤ 远端交付按 `AGENTS.md` §8 的 PR 路径另行进行（本轮不推送）。
- **风险**：① §4 的 flake 未定位根因（主分支轮次未复现，但仍属观察项）；② `.log` 类原始证据被 `.gitignore:27` 忽略、不进版本控制，仅在磁盘（备份目录与主检出）可读；③ 本地 `npm run verify` 不含 `cargo-deny` 与 `gitleaks`，本变更新增依赖（axum / rustls 系等）的许可证与 advisory 判定只在 CI 完成，本轮不作为证据；④ 主检出现在领先 `origin/main` 47 个提交，未推送（收敛前不可假定远端可见）；⑤ 本报告在合入后曾被较早副本覆盖，§10 与 4 行 `stage: main` 由本角色据原文重组补回（§10.5）。

## 10. 合入记录与主分支检查（stage=main；任务 6.6 + 6.7 前半）

### 10.1 合入前置障碍处理（RV2-DU1-F3，按序留证）

| 障碍 | 处理前证据 | 动作 | 结果 |
| --- | --- | --- | --- |
| ① `M docs/DEVELOPMENT_PLAN.md` | `git status --porcelain` = ` M docs/DEVELOPMENT_PLAN.md`，但 `git diff --numstat` / `--raw` 无输出、`git diff \| wc -c` = **0 字节**，仅一条 LF→CRLF 归一化警告 → 纯行尾噪声（零内容差异） | `git checkout -- docs/DEVELOPMENT_PLAN.md`（exit 0） | 工作区恢复干净；无内容丢失（依据：内容差异为零） |
| ② 未跟踪目录 `openspec/changes/node-link-owner/`（80 文件）与传入的跟踪版本同名冲突 | `find ... -type f \| wc -l` = 80；与 `654c0c19` 跟踪版本（64 文件）逐名比对可判定 | `mv` 到 `D:\Project\acp-remote-wt\planning-backup-node-link-owner\`（exit 0，**现场不删除**）；ff-merge 后目录由跟踪版本重建（64 文件） | 备份 80 文件全部保留；合入无阻碍 |

**备份 vs 跟踪版本差异清单**（供主 Agent 处理权威副本；本角色只从备份回填自己拥有的两件）：

- 内容有差异的 4 个 `.md`：`reports/du1-integrate.md`（258 → 391 行，本报告 round 3；**已回填并继续更新**）、`reports/wp7-handoff.md`（行数同、内容不同）、`tasks.md`（行数同、内容不同）、`verification.md`（297 → 336 行）；后三者**原样留在备份目录**，未回填。
- 磁盘独有的 16 个原始日志（被 `.gitignore:27` 的 `openspec/changes/**/reports/**/*.log` 规则忽略，不进版本控制）：`du1-candidate.log`（**已回填并继续追加**）、`du1-pv1.log`、`env1-baseline-cargo-test.log`、`env1-baseline-npm-check.log`、`pv5-windows-nodelink.log`、`wp1-deps.log`、`wp2-transport-net.log`、`wp3-contract.log`、`wp3-pairing-http.log`、`wp4-handshake.log`、`wp4-wire-fix.log`、`wp5-catalog-resource.log`、`wp6-command.log`、`wp7-app-wiring.log`、`wp7-integration.log`、`wp7-workspace.log`（除已注明者，其余 15 个留在备份目录）。

### 10.2 合入执行与防竞态

| 项 | 值 |
| --- | --- |
| 合入前基线复核 | `git rev-parse refs/heads/main` = `94a64e1f15d26f54a6601985fd13b1440fec8170` = `origin/main`（自 round 1 起第 **5** 次核实，未移动） |
| ff 前提 | `git merge-base --is-ancestor 94a64e1f 654c0c19` = exit 0（候选亲本即 main） |
| 候选身份 | `654c0c1944c75bbc016eab775f3f3ff8aca9bf35`（亲本 `94a64e1f` + `5f62e77f`） |
| 合入前工作区 | 两项障碍已处理，`git status --porcelain` 空（未观察到并发写入） |
| 命令 | `git merge --ff-only 654c0c1944c75bbc016eab775f3f3ff8aca9bf35` |
| 结果 | **exit 0，fast-forward，无新提交**（main 与候选为同一个提交对象） |
| 合入后核实 | `refs/heads/main` = `654c0c1944c75bbc016eab775f3f3ff8aca9bf35`；`git log -1` = `654c0c19 94a64e1f 5f62e77f`；`origin/main` 仍 `94a64e1f…`（**未推送、未动远端分支、未回滚**） |
| 合入后工作区 | `git status --porcelain` 仅 `M openspec/changes/node-link-owner/reports/du1-integrate.md`（本报告，`git diff --stat` = +149/−16）；无其他非忽略改动 |
| 防竞态依据 | 使用 ff-only：若基线期间被移动，命令会失败而非生成错误合并；基线复核与合入之间无其他写入者 |

### 10.3 主分支检查（原始日志 `reports/du1-candidate.log` §`MAIN BRANCH ROUND`）

| 检查 | 命令（主检出 `D:\Project\acp-remote`） | 退出码 | 结果 |
| --- | --- | --- | --- |
| PV1 | `npm run verify` | 0 | 10 道合同门禁逐项 0（含 `check:docs` 0）+ `cargo fmt` 0 + `cargo clippy -D warnings` 0 + workspace 测试 **973 passed / 0 failed / 2 ignored**（85 目标） |
| PV2 | `node scripts/check-crate-boundaries.mjs` | 0 | 12 个 crate 依赖方向与 §5 矩阵一致（含 `server→acpr-wire` 格） |
| PV5 | `cargo test --locked -p server -p app --all-features` | 0 | 15 目标 **394 passed / 0 failed / 0 ignored**，`-- --list` 交叉核对 declared 394 == executed 394（0 filtered out） |

PV5 逐目标（主分支轮次）：server lib 281、app lib 52、`daemon_lifecycle` **11/11**（60.38 s，真实起停子进程；**§4 登记的 flake 用例 `the_status_result_keeps_the_documented_field_set` 本轮 PASS，未复现**）、`node_link_e2e` **2/2**（3.19 s = 受控路径全链路 + TLS direct）、`node_link_listener` 8/8、`local_admin_channel` 14、`local_admin_schema_drift` 6、`local_endpoint_naming` 4、`local_endpoint_windows` 4、`audit_export` 1、`cli_commands` 11、`local_endpoint_unix` 0（整文件 `#[cfg(unix)]`，Linux CI 覆盖）、app `main` 与 Doc-tests 各 0。平台差异用例在 main 上真实执行并通过（`direct_mode_permissions_are_unverifiable_on_this_platform`、`windows_inspection_is_unverifiable`、`transport::local::platform::windows::tests` 两条、`node_link_e2e` 两条）。

**分层意义**：主分支轮次的 PV1/PV2/PV5 均为该提交上的 **NEW** 证据（PV5 不再沿用候选阶段的 REUSED），因此任务 6.7 前半的判据不依赖任何候选阶段复用。

**编辑后复核（非 6.7 检查项）**：本报告 §10 写入后，在主检出重跑 `node scripts/check-doc-links.mjs` = exit 0（两次；终稿轮次输出 `doc links OK: 381 relative links, 5864 section refs across 333 markdown files`），确认规划工件文本编辑未破坏 `check:docs`；原始输出见日志 §`MAIN 附注`。主 Agent 把本报告提交前仍应完整跑一次 `npm run check`。

### 10.4 未做的动作与保留引用

- **未做**：推送远端 / 开 PR / 动远端分支 / 回滚 / 删除分支（本轮明确不推送；远端交付按 `AGENTS.md` §8 的 PR 路径另行进行）。
- **保留引用**：`main` = `654c0c1`；`agentic/node-link-owner` = `5f62e77f`；`integration/node-link-owner-du1` = `654c0c1`；备份目录 `D:\Project\acp-remote-wt\planning-backup-node-link-owner\`；候选 worktree 与其 `target/`（约 9.6G）。
- **待主 Agent**：`verification.md` 回写与最终验收（§9 已列）。
- **后来变化（由主 Agent 执行，本角色未参与）**：主分支在本节记录之后又前进了 1 个提交 `392efb791015b6a86a99ec9fd2ead45fe8c2881a`（`docs(repo): 回写 node-link-owner 合入阶段的规划记录与 premerge 证据块`，只改 `tasks.md` 与 `verification.md`）；`654c0c19` 仍是它的祖先（`git merge-base --is-ancestor 654c0c19 main` = exit 0），因此本节的合入事实与主分支检查记录仍然成立，差异仅为后续规划工件回写。

### 10.5 本节的补回记录（2026-09-26）

- 现象：合入后整理规划工件时，本报告被一份较早副本覆盖，磁盘版本退回 391 行的 round 3 内容（§0–§9，无 §10、无 `stage: main` 行、Shared Report 回到候选阶段口径）。
- 处理：本角色按上次写入的原文重组补回：顶部轮次说明、Shared Report 的 11 行（task_id / phase / agent_context / target_revision / scope / changes / checks / issues / result / evidence_paths / resource_cleanup）、§8 前言的 `stage: main` 指引、handoff_index 末尾 4 行（6.6 与 6.7 前半的 PV1/PV2/PV5，`stage: main`，target = `654c0c19`）以及本节 §10（10.1–10.4）。**只改动了本文件**，未改 `verification.md` / `tasks.md`。
- 补回后核实：`handoff_index` 共 **17** 行（13 行候选阶段 + 4 行 `stage: main`），YAML 可解析；`npm run check:docs`（主检出）= exit 0。
