# DR1 — Dependency Declaration Review（session-resume / plan.md），Round 1

> 本文件由主 Agent 从 reviewer 子 Agent（Review ID DR1 / Round 1 / phase plan）的返回结果原样持久化；
> reviewer 角色的工具集为只读（无 write），因此报告由主 Agent 落盘，内容未改写。

# DR1 — Dependency Declaration Review（session-resume / plan.md）

## Shared Report

```
task_id: NOT_APPLICABLE
role: reviewer
phase: plan
stage: plan
agent_context: standalone task-level reviewer subagent（新实例，未继承任何实现/规划对话；用户上下文仅含本任务派发）
target_revision: sha256:77c651a4dbc32f14944b53f890ed055e8a58e804eb4a4ce50d00634fc78eaf96
scope: plan.md 的 Dependency Declaration Review 对象（Dependencies 类型声明、Contract Freeze 路径、Execution Waves、
       Runtime Resources 的 Exclusive Scheduling、Shared File Ownership、Coverage Index target_ref）
changes: 无（只读，未修改任何文件；不切换分支、不提交）
checks: NOT_APPLICABLE（规划审查不执行 PV1/PV2，不冒称已执行）
issues: 4×MAJOR（DR1-F1…F4）、4×MINOR（DR1-F5…F8）
result: FAIL
evidence_paths: 本报告（openspec/changes/session-resume/reports/dr1-dependency-review.md，待持久化）
resource_cleanup: 未创建/启动/停止任何资源；未写仓库文件
```

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / 1（新线程；Previous Findings = NOT_APPLICABLE，非复核轮） |
| Review Type / Stage | plan / plan |
| Work Package | NOT_APPLICABLE（不代表任何 WP；非 WP Owner） |
| Repository | `D:/Project/acp-remote`（既非独立检视 worktree，也未切换版本；只读） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `sha256:77c651a4dbc32f14944b53f890ed055e8a58e804eb4a4ce50d00634fc78eaf96`（派发给出的 plan.md 当前 contractDigest）。本会话只读且无 OpenSpec 引擎调用能力，**无法重算该摘要**，按派发值记录，不声称已复核其内容映射。 |
| Requirements（已读） | `proposal.md`；`specs/acp-wire-protocol/spec.md`、`specs/local-agent-host/spec.md`、`specs/node-link-owner-server/spec.md`、`specs/storage-schema-v2-migration/spec.md`、`specs/workspace-resolution/spec.md`；`design.md`（D1–D5）；`plan.md`（含 Coverage Index、Work Packages、Execution Waves、Shared File Ownership、Dependency Handoffs、Runtime Resources、Target Repository and Main Branch、Merge Strategy、Verification Strategy） |
| Project Rules（已读） | `AGENTS.md` §4 依赖规则、§8 实现工作流、§10 文档维护；另读 `tasks.md`、`verification.md` |
| 代码/资产依据（只读 grep/read） | `scripts/check-command-catalog.mjs`、`scripts/check-contract-drift.mjs`、`package.json`；`compatibility/commands/v1/commands.json`、`compatibility/acp/v1/matrix.json`、`schemas/acp/v1/upstream/schema.json`（ResumeSessionRequest/Response）；`crates/core/src/{ports.rs,broker.rs}`、`crates/core/Cargo.toml`；`crates/storage-sqlite/{Cargo.toml,src/session_store.rs}`；`crates/agent-host/{Cargo.toml,src/host.rs}`；`crates/node-link-protocol/{Cargo.toml,src/command.rs}`；`crates/server/Cargo.toml`、`crates/server/src/node_link/command.rs`；`crates/app/Cargo.toml`、`crates/app/tests/**`；`docs/CORE_PORTS_AND_STORAGE.md`、`docs/SECURITY_DESIGN.md` §10.2、`docs/SYNC_PROTOCOL.md` §11.5；`reports/scout-target.md` |
| Verification Evidence | 无（规划审查不得冒称已执行测试；本会话无 shell/测试执行能力） |
| Check Plan | plan.md `## Verification Strategy` 的 PV1/PV2 定义（**只核对其作为依赖/可产出性判据的适用性，未执行**） |
| Isolation | `fork_turns="none"` 等效（本实例无继承对话）。实际 Agent ID 与隔离设置由调度者关联登记；我只声明收到的输入与自身限制。 |

### 八项必核事项的核对结果

| # | 要求 | 结论 | 依据 |
| --- | --- | --- | --- |
| 1 | WP3/WP4 是否真需 WP2 的**代码** | **属实** | `crates/storage-sqlite/Cargo.toml:12` 依赖 `core`，`src/session_store.rs:23-41` 直接构造/消费 `acp_core::model::*` 与 `acp_core::ports::*`（`NewSession`/`OwnedCommit`/`SessionStore`），D2 要求的新列经 core 的创建提交写入 → WP2 改提交形状即改 storage 的编译面；`crates/agent-host/Cargo.toml:12` 依赖 `core`，`src/host.rs:445` `impl SessionBackendFactory for AgentHost` 必须实现 D3 新增的 `resume` 与 `SessionEndpoint::agent_session_id`（`crates/core/src/ports.rs:80-96` 现只有 `create`/`open`）。 |
| 2 | WP6 是否真需 WP2/WP3/WP4/WP5 的代码 | **属实** | `crates/server/Cargo.toml:19,27`（core、node-link-protocol）；`crates/server/src/node_link/command.rs:65,260,717-725` 已按 `CommandName::SessionCreate` + `core::use_cases` 路由，`session.resume` 同理需 WP5 的 `CommandName::SessionResume` 与 WP2 的用例/授权；`crates/app/Cargo.toml:22-35` 装配 core/storage-sqlite/agent-host/server，WP6 的「端到端受控路径测试」必须读到 WP3 的恢复列与 WP4 的恢复实现。 |
| 3 | WP1/WP2/WP5/TP1 的 `none` 是否属实 | **WP1 属实**（`crates/acp-protocol/Cargo.toml:12-14` 只有 serde 系，无 workspace crate）；**WP2 代码面属实**（`crates/core/Cargo.toml:18-29` 仅 async-trait/thiserror/p256/sha2），但与 WP5 存在被硬门禁绑定的合同点 → **DR1-F1**，且 Contract Freeze 缺 D1 → **DR1-F5**；**WP5 代码面属实**（`crates/node-link-protocol/Cargo.toml:12-16` 不依赖 core），同样受 **DR1-F1/F2/F3** 约束；**TP1 不属实** → **DR1-F4**。 |
| 4 | Contract Freeze 路径存在且可读 | **全部存在**：`specs/acp-wire-protocol/spec.md`、`specs/local-agent-host/spec.md`、`design.md`（D1 第 30-56 行、D2、D3、D4、D5 均实存，全文 148 行）、`docs/CORE_PORTS_AND_STORAGE.md`（§7.3 存在，第 872 行起）。精确性问题见 **DR1-F5/F6**（WP2 未列 D1；WP3 引 `@§7.3-v5` 而版本常量实际在 §7.2）。 |
| 5 | Execution Waves 层级与 Reason | **层级合规**：W1=无 code 依赖的 WP1/WP2/WP5/TP1（`plan.md:226-229`）；W2=WP3/WP4=上游最早层级 1+1（`:230-231`，与 `code:WP2` 一致）；W3=WP6=max(1,2,2,1)+1=3（`:232`）。7 行 Reason 全为 NOT_APPLICABLE，且每行正处其最早层级 → 合规。**但依赖本身不实者，层级也随之错误**（F1/F4）。 |
| 6 | Exclusive Scheduling=`NOT_APPLICABLE` 有依据 | **有依据**：网络测试一律 `127.0.0.1:0` 内核分配端口（`crates/app/tests/support/mod.rs:184,318`、`crates/server/src/transport/net/tests.rs:149`、`crates/server/src/node_link/conn/tests.rs:126`）；临时目录按 label+pid+计数器唯一（`crates/app/tests/support/mod.rs:76-78`、`crates/app/tests/node_link_e2e.rs:980`），daemon 锁/记录文件都在各自 `data_dir` 内（`crates/app/tests/support/mod.rs:432,461`）；`scripts/` 无任何写文件调用（grep `writeFileSync|mkdirSync|rmSync|unlinkSync` 无匹配），PV2 只读；`plan.md:255` 的每 worktree 独立 `CARGO_TARGET_DIR` 与 `verification.md` 的 `## Runtime Resources` 一致。唯一固定临时路径 `crates/app/tests/node_link_listener.rs:167` 只作「文件不存在」哨兵（从不创建），无跨 worktree 冲突。 |
| 7 | Shared File Ownership（`docs/CORE_PORTS_AND_STORAGE.md` 双写者） | Merge Owner=WP3、Merge Order=`WP2 → WP3`、Re-verify After Merge=`WP3: PV2` **齐备**（`plan.md:238`）；两写者区域（§2/§5.1 vs §7.3）**确实不相交**。区域**说明不精确**（§7.2 版本常量与 §7 标题不在登记的 §7.3 内；WP2 亦需 §3）→ **DR1-F6**。登记面上更严重的缺失见 **DR1-F1/F2/F3**（PV2 强制改动的两个文档与一个脚本不在任何写范围）。 |
| 8 | Coverage Index target_ref 与目标一致 | **一致**：`plan.md:19` `target_ref: refs/heads/main` == `plan.md:263` `- Target Ref: refs/heads/main` == `verification.md` `## Target`；scout 已固定实际提交 `81e350ff…`（`reports/scout-target.md`）。证据路径小瑕疵见 **DR1-F8**。 |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F1 | **MAJOR** | `plan.md:213`（WP2 Dependencies=none）、`plan.md:216`（WP5 Dependencies=none）、`plan.md:226,228`（同处 W1）、`plan.md:244-250`（Dependency Handoffs 无 WP2↔WP5 行）、`plan.md:234-239`（Shared File Ownership 无对应行） | `scripts/check-command-catalog.mjs:254` 以**双向集合相等**断言 `crates/core/src/broker.rs::required_grant` 的命令名集合 == `commands.json` 的全量命令集合（`compareSets` 对 missing/extra 都报错，`:35-43`）；且 `crates/core/src/broker.rs:671` 对未登记命令直接 `return Ok(false)`（授权失败关闭）。因此 WP5 加 `session.resume` 到 `commands.json` 必须同时有 core 镜像条目，WP2 加镜像条目必须同时有 `commands.json` 条目——**任一单独交付时各自声明的 PV2 必然 FAIL**。plan/tasks 要求「交付前」逐 WP 跑 PV2（`tasks.md:32`、plan.md:283 局部自检段），二者又同处 W1 且理由均写 NOT_APPLICABLE。若解释为「WP2 等 WP5 合入」，则 `Dependencies: none` 与 W1 入场条件同时不实；两种解释下计划都自我矛盾。 | 两个工作包被同一门禁绑在同一合同点上却按独立并发单元排程：开工窗口内的 PV2 证据不可产出，调度会得到「同批却互为前置」的死锁或被迫跳过声明的检查；`Dependency Handoffs` 的失效重跑也不覆盖该对（WP2 变→WP5 不重跑）。 | 二选一：(a) 让同一 WP 同时写两侧（把 broker.rs 的 `required_grant` 镜像条目并入 WP5 写范围，或在 WP2 里带 `commands.json` 条目），或 (b) 显式登记为串行点：`WP5 → WP2`（或反之）的 Dependency Handoffs 行 + Shared File Ownership 一致点行 + 后合入方合并后重跑 PV2 项，并把该 WP 移到上游层级的下一波。 | NOT_APPLICABLE（首轮） |
| DR1-F2 | **MAJOR** | `design.md` D1 表（`pack` = `null`，与 `session.create` 同形）、`plan.md:216`（WP5 写范围未含 `scripts/`）、`plan.md:234-239`、`design.md` D5 资产同步表 | `scripts/check-command-catalog.mjs:194-196`：`if (!command.pack && command.name !== "session.create") errors.push("… has no pack")`。`compatibility/commands/v1/commands.json:12` 的 `session.create` 正是靠这处**硬编码豁免**才能 `"pack": null`。冻结的 D1 给 `session.resume` 也取 `pack: null`，因此 WP5 声明的 PV2 无法通过，除非改 `scripts/check-command-catalog.mjs`（扩展豁免或调整断言）或改 D1 的 pack 取值。前者是门禁脚本改动、后者会牵动 pack/preset 与两份文档表——两条路径都**不在任何 WP 的 Write Scope**，D5 也未列 `scripts/`。 | 冻结契约在现行门禁下不可实现：WP5 要么改一份未登记的脚本（并使封闭词表门禁的豁免名单增长，属 AGENTS.md §10 需同步说明的门禁调整），要么被迫在实现期改契约——两者都会让「契约已冻结即可开工」的 WP5 无法按计划交付 PV2。 | 三选一并登记写范围与资产同步：(a) 扩展 `check-command-catalog.mjs` 的豁免并把 `scripts/check-command-catalog.mjs` 写入 WP5 写范围（同时按 §10 同步 gate 说明）；(b) 给 `session.resume` 指定/新增 pack 并同步 presets 与两份文档表；(c) 改 D1 并走 Contract Changes 流程重评证据。推荐 (a) 并同步文档，避免新增 pack 改变配对 UI 语义。 | NOT_APPLICABLE（首轮） |
| DR1-F3 | **MAJOR** | `plan.md:216` WP5 写范围、`plan.md:234-239`（两文件均不在表内）、`design.md` D5 资产同步表 | `scripts/check-command-catalog.mjs:241` 与 `:245` 用同一 `names`（= commands.json 全量命令集，`:181-197`）断言 `docs/SYNC_PROTOCOL.md` §11.5 表格与 `docs/SECURITY_DESIGN.md` §10.2 表格的命令集合**相等**。现状两表都列全 12 条（`docs/SYNC_PROTOCOL.md:1255-1266` 含 `session.create`；`docs/SECURITY_DESIGN.md:274-285` 同理），`docs/SYNC_PROTOCOL.md:1268` 还明写「完整命令集合（12 条）…node-link 全部 12 条」，且该段自述「任一处增删命令名都必须同步修改 … `SECURITY_DESIGN.md` 第 10.2 节与本节」。故新增第 13 条命令**必须**改这两份文档与计数文本，否则 PV2 FAIL。 | 变更所需的两处写目标无归属：无 WP 登记、无 Merge Owner、无 Re-verify 项；AGENTS.md §10 的「变更类型→权威文档」映射亦无覆盖（D5 与 plan 都漏）。WP5 的 PV2 与最终 Completion Criteria 不可达，或实现期出现无人负责的跨域写入（SYNC/SECURITY 文档由非 Owner 改写）。 | 把 `docs/SYNC_PROTOCOL.md`（§11.5 表格与「12 条」计数文本）和 `docs/SECURITY_DESIGN.md`（§10.2 表格）写入 WP5 的 Write Scope 并在 Shared File Ownership 各登记一行（单一写者 WP5、Merge Owner WP5、Re-verify PV2），同时在 `design.md` D5 资产同步表补上这两行。 | NOT_APPLICABLE（首轮） |
| DR1-F4 | **MAJOR** | `plan.md:218`（TP1：Dependencies=none，Verification=PV1）、`plan.md:229`（TP1 处 W1）、`tasks.md:27-28`（「契约已冻结即可开工，不等任何实现代码」）、`tasks.md:44`（4.2 `[PV1]` 编译/用例发现） | TP1 的产物是 `crates/**/tests/` 的用例文件（现有形态如 `crates/agent-host/tests/session.rs`、`crates/app/tests/node_link_e2e.rs`、`crates/storage-sqlite/tests/migration.rs`），在 Rust 里必须编译通过才算「基础检查」。Coverage Index 要求这些用例覆盖 R5–R12（恢复路径）、R13–R22（v5 列）、R27–R37（`session.resume` 路由/终态），因此必须引用尚未存在的 API：`core` 现在只有 `create`/`open`（`crates/core/src/ports.rs:80-96`），`agent-host` 尚未实现恢复（`crates/agent-host/src/host.rs:445` 起的 `impl`），`CommandName::ALL: [CommandName; 12]`（`crates/node-link-protocol/src/command.rs:86`）也没有 `SessionResume`。 | TP1 声明的检查（PV1 = `cargo test --locked --workspace --all-features`，见 plan.md PV1 行与 `package.json:25`）在其 W1 入场条件下不可产出；`none` 依赖声明不实，与 tasks 2.7「不等任何实现代码」直接冲突。实际后果要么是 TP1 被卡住（占住窗口却无法交付），要么写出不能编译的用例（违反用例 reviewer 的「基础检查」判据）。 | 二选一：(a) 声明 `TP1: code:WP1…WP6`（至少 WP2/WP4/WP5/WP6）并置于 `max(上游层级)+1`，使 PV1 在含实现的基线上运行；或 (b) 明确把 W1 的 TP1 交付限定为需求映射 + 稳定用例 ID（不含可编译用例），把 `[PV1] 编译/用例发现` 移到上游代码就绪后的阶段并同步 tasks 2.7/4.2 与 Verification。推荐 (a)，因为用例审查判据要求真实入口（Node Link 路由），mock 绕过会直接判不合格。 | NOT_APPLICABLE（首轮） |
| DR1-F5 | MINOR | `plan.md:213`（WP2 Contract Freeze=design.md@D2,D3；写范围=§2、§5.1） | (i) WP2 的目标含「命令路由」：core 必须新增 `"session.resume"` 的命令名/授权映射（`crates/core/src/broker.rs:94-137` 的 `command_name`/`required_grant` 表），而命令名与 `grant.remote-work` 的取值只在 `design.md` **D1** 冻结；Contract Freeze 未列 D1。(ii) WP2 新增 `AgentSessionId` 值对象与提交形状，但 `docs/CORE_PORTS_AND_STORAGE.md` §3（`## 3. core::model 值对象（全量冻结）`，§3.1 标识表）与 §5.2（`OwnedCommit` 形状块，第 271 行起）都不在登记的「§2、§5.1 区域」内（AGENTS.md §10 要求值对象/提交形状变化同步该文档）。 | 冻结引用不完整会放行「命令名/grant 取值未被冻结即开工」的解释空间；文档区域漏登记会让 core 的值对象/提交形状契约在交付时无归属（漂移门禁只核对 §7↔migrate.rs 与 §5 trait 方法集，不会自动发现 §3 缺条目）。 | 把 `design.md@D1` 补入 WP2 的 Contract Freeze，并把写范围修正为「§2、§3.1（新值对象）、§5.1、§5.2（提交形状）」。 | NOT_APPLICABLE（首轮） |
| DR1-F6 | MINOR | `plan.md:214`（WP3 写范围「§7.3 区域」，Contract Freeze `docs/CORE_PORTS_AND_STORAGE.md@§7.3-v5`）、`plan.md:238` 区域说明 | 版本常量与升级判据在 §7.2：`docs/CORE_PORTS_AND_STORAGE.md:856` 明写三个常量与「当前 v4 = 4」的升级判据，§11.8（第 1544 行）索引为「版本常量、migration 与 fixture（并入 §7.2）」；§7 的章节标题本身是 `## 7. storage-sqlite v4 表结构`（第 843 行），推进到 v5 必须改标题与 §7.2 正文。而 `plan.md:238` 的区域说明写成「WP3 改 §7.3 DDL 与版本常量」，与登记的「§7.3 区域」自相矛盾。 | 登记与说明不一致，会让后合入方（WP3 为 Merge Owner）按错误区域复核；虽然 §7.2/§7 标题与 WP2 的 §2/§5.1 实际不相交（不构成写冲突），但「双写者区域不重叠」的论证有一条不实。 | 把 WP3 写范围改为「§7 标题、§7.2（版本常量与升级步骤）、§7.3（DDL）」，并同步区域说明；两写者仍不相交，无需改 Merge Order。 | NOT_APPLICABLE（首轮） |
| DR1-F7 | MINOR | `plan.md:215`（WP4 Dependencies=code:WP2）、`plan.md:246-247`（Dependency Handoffs 只有 WP3/WP4 ← WP2） | WP4 的恢复实现必须解码 `session/resume` 的**响应**（pinned schema `schemas/acp/v1/upstream/schema.json:3337` `ResumeSessionResponse` 带 `modes`/`configOptions`），而仓库既有模式是对响应用 acp-protocol 的 typed DTO 解码：`crates/agent-host/src/host.rs:445-470` 用 `acp_protocol::message::NewSessionResponse`。该响应 DTO 由 WP1 交付（`specs/acp-wire-protocol/spec.md`：响应 SHALL 解码为类型化 DTO）。**不确定性**：WP4 也可以手写 `Value` 提取（`SessionModeState` 是既有公开类型），故不能断言必然编译依赖——因此记 MINOR 而非 MAJOR。 | 若 WP4 实际引用 WP1 的 DTO，则 `code:WP1` 未声明且无 WP1→WP4 的失效重跑行：WP1 后续变更不会触发 WP4 重跑。 | 在 WP4 的 Dependencies/Contract Freeze 明确 `code:WP1`（或写明「响应手写解码、不依赖 WP1 代码」），并在 Dependency Handoffs 补 `WP4 ← WP1` 行与失效条件。 | NOT_APPLICABLE（首轮） |
| DR1-F8 | SUGGESTION | `plan.md:264`（Confirmation Method / Evidence 写 `reports/scout-target.log`）、`verification.md` `## Target` 同写 `scout-target.log` | 实际存在的证据文件是 `openspec/changes/session-resume/reports/scout-target.md`（目录列表仅 `provisioner-worktrees.md`、`scout-target.md`），plan/verification 引用的 `.log` 不存在。 | 交接与门禁按路径取证据时会找不到文件（同类路径 PV1.log/PV2.log 属未来产物，尚不构成问题）。 | 把引用改为 `reports/scout-target.md`，或在同一目录补一个 `.log` 形态的证据文件并保持口径一致。 | NOT_APPLICABLE（首轮） |

`SAFE-NOTE`：除上表外，未发现其它与派发 Scope 相关的具体问题。

## Check Plan Reconciliation

- PV1/PV2 在本轮**未执行**（规划审查 + 只读边界）。本轮只核对其作为「依赖声明可产出性」判据的适用性：PV1 的 TP1 行在 W1 不可产出（DR1-F4）；PV2 的 WP2/WP5 行在各自分支不可产出（DR1-F1/F2/F3）。
- 与 Project Verify 无并行：本会话不具备执行能力，无「待返回结果」需要判定其对本轮结论的影响；因此结论不受未执行的调用检查影响，可完成静态审查（roles/reviewer.md 第 6 条）。
- Main E2E 为 `not-applicable`（plan.md），本轮不涉及 E2E 用例或其配置，不做 E2E 判据表核对。
- 未核对项：contractDigest 的取值本身（无引擎调用能力），按派发值记录；Coverage Index 37 行的计数连续性已核对（R1–R37 连续、与 Completion Criteria 的「37 行」一致），其**正确性/充分性**属主 Agent 的覆盖核对与 validator 范围，不在本轮。

## Assessment

**结论：FAIL**（对应 Target Revision = `sha256:77c651a4…c78eaf96`）。

依据 roles/reviewer.md：存在已确认且未解决的 MAJOR 问题即 FAIL。本轮确认 4 项 MAJOR：

- **DR1-F1**：WP2/WP5 的 `none` 声明在 `check:command-catalog` 的 `broker.rs ↔ commands.json` 双向集合断言下不成立，两者同处 W1 且无串行化/交接/重跑登记——属于「同一处却同批」。
- **DR1-F2**：D1 的 `pack: null` 与 `check-command-catalog.mjs:194-196` 冲突，修补路径所需的写目标（`scripts/check-command-catalog.mjs` 或 pack/preset 面）未登记。
- **DR1-F3**：变更强制要求改 `docs/SYNC_PROTOCOL.md` §11.5 与 `docs/SECURITY_DESIGN.md` §10.2（含「12 条」计数），两文件不在任何 WP 写范围、不在 Shared File Ownership、D5 亦未列——登记缺失。
- **DR1-F4**：TP1 的 `none` 与 W1 入场条件不实（其自身声明的 PV1 编译检查需要上游 WP1–WP6 的代码）。

已核实为**合规**的部分（不构成反对意见）：WP3/WP4/WP6 的 `code:` 声明属实且有代码层依据；WP1/WP2/WP5 的**代码**依赖面为空属实；Contract Freeze 引用的路径与 design D1–D4、5 份 specs 全部存在可读；Execution Waves 的层级算式与 `NOT_APPLICABLE` 理由在其当前（不实的）依赖图内自洽；Runtime Resources 的 Exclusive Scheduling=`NOT_APPLICABLE` 有依据（内核分配端口、唯一临时目录、脚本无写入）；`docs/CORE_PORTS_AND_STORAGE.md` 的 Merge Owner/Order/Re-verify 齐备且两写者区域实际不相交（说明不精确见 F6）；Coverage Index `target_ref` 与目标 ref 一致。

非阻断项（DR1-F5…F8）不单独构成 FAIL 理由，但建议在同一轮计划修订里一并处理。

本报告不更新任何任务状态，也不代表整个变更可归档；修复后需由**新的独立 reviewer** 在同一 Review ID 下按 F1–F4 逐项复核。

```yaml
handoff_index:
  - task_id: NOT_APPLICABLE   # plan/tasks.md 未为「依赖声明审查」建任务行；该门禁由 plan.md 的 Dependency Declaration Review 段与 verification.md 的 ## Dependency Declaration Review 表承载
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    stage: plan
    round: 1
    target_revision: "sha256:77c651a4dbc32f14944b53f890ed055e8a58e804eb4a4ce50d00634fc78eaf96"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review.md
    result: FAIL
    evidence_status: NEW
    applicability_basis: "本轮针对 plan.md 的当前 contractDigest 做静态规划审查；未执行 PV1/PV2、未针对代码 diff，Base Revision 为 NOT_APPLICABLE。"
    source_evidence: NOT_APPLICABLE
```

---