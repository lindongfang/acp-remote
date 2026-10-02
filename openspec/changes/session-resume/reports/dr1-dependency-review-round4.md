# DR1 — Dependency Declaration Review（session-resume / plan.md），Round 4（recheck）
## Shared Report
```
task_id: NOT_APPLICABLE（plan/tasks.md 未为「依赖声明审查」建任务行；该门禁由 plan.md:254 的 Dependency Declaration Review 段与 verification.md 的 ## Dependency Declaration Review 表承载）
role: reviewer
phase: plan
stage: plan
round: 4
agent_context: standalone task-level reviewer subagent（新实例，未继承前三轮或任何实现/规划对话；只读，无 write 工具）
target_revision: sha256:0b6f6b5246545831da07583f9352151f7d5cc4c50db474eb8b056e627d691f11
scope: plan.md 的依赖类型声明、Contract Freeze 路径与写范围、Execution Waves、Shared File Ownership（含 Merge Owner/Order/Re-verify）、
       Dependency Handoffs 的红窗口声明与收口责任人、Verification Strategy 的 PV1/PV2 阶段制；并交叉核对 gates 脚本 / 源码 / schema / 各 crate Cargo.toml
       / hooks 以判断「各 WP 声明的分支级检查是否可产出」；不读代码 diff、不执行任何检查
changes: 无（只读；未修改、未暂存、未切换分支、未提交任何文件）
checks: NOT_APPLICABLE（规划审查不执行 PV1/PV2/，不冒称已执行）
issues: 0×CRITICAL、0×MAJOR；7 项非阻断（F18–F24：MINOR×6 + SUGGESTION×1）；F13/F14/F15/F17 复核为已解决，F16 部分解决
result: PASS
evidence_paths: 本报告（openspec/changes/session-resume/reports/dr1-dependency-review-round4.md，由 main 原样持久化）
resource_cleanup: 未创建/启动/停止任何资源；未写仓库文件
```
## Review Context
| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / 4（同一线程；Type/Stage 沿用，显式 round=4，当前结论以本轮为准） |
| Review Type / Stage | plan / plan |
| Work Package | NOT_APPLICABLE（不代表任何 WP，非 WP Owner） |
| Repository | `D:/Project/acp-remote`（只读；未切换版本、未建检视 worktree） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `sha256:0b6f6b5246545831da07583f9352151f7d5cc4c50db474eb8b056e627d691f11`（派发给出的当前 `contractDigest`）。本会话无 OpenSpec 引擎与 shell 能力，**无法重算该摘要**，按派发值绑定，不声称已复核其内容映射 |
| Previous Findings | R1 `reports/dr1-dependency-review.md`（FAIL，F1–F8）；R2 `reports/dr1-dependency-review-round2.md`（FAIL，F9/F10/F11/F12）；**R3 报告在仓库中不存在**（见下「输入偏差」）——R3 的 F13–F17 仅能通过 plan.md:37-45、verification.md:41/107-111/147 的转述读取，本轮已对其实质逐条独立复核 |
| Requirements（已读全文） | `proposal.md`；`specs/{acp-wire-protocol,local-agent-host,node-link-owner-server,storage-schema-v2-migration,workspace-resolution}/spec.md`；`design.md`（D1:30、D2:56、D3:75、D4:109、D5:120、D6:144）；`plan.md`；`tasks.md`；`verification.md` |
| Project Rules（已读） | `AGENTS.md` §4/§8/§9/§10/§11/§12；`roles/reviewer.md`；`roles/_shared/role-report.md` |
| 本轮交叉核对的机器资产（只读） | `package.json:11-31`；`scripts/check-command-catalog.mjs`（全文）、`check-acp-compatibility.mjs`（全文）、`check-features.mjs`（全文）、`check-schema-fixtures.mjs`、`check-contract-assets.mjs`、`check-contract-drift.mjs`、`pre-commit.mjs:60-108`、`.husky/pre-commit`；`schemas/acp/compatibility-matrix.schema.json`、`compatibility/acp/v1/matrix.json`（session_resume/cap.agent.session_resume 行）、`fixtures/acp/v1/manifest.json`；`crates/server/src/node_link/command.rs:2189-2265`、`crates/server/src/local_admin/test_support.rs:1021`、`crates/server/src/node_link/command/tests.rs:437,484`、`crates/app/tests/support/owner.rs:504,619`、`crates/agent-host/src/host.rs:449`、`crates/agent-host/src/session.rs:746`、`crates/core/src/broker.rs:94-137,3078-3091,5300,5335`、`crates/core/src/ports.rs`、`crates/core/src/model/tests.rs:2208`、`crates/storage-sqlite/tests/**`（OwnedCommit 字面量站点）、`crates/acp-protocol/src/methods.rs`（session/resume 行）、`crates/acp-protocol/tests/{fixtures.rs:32-40,raw_fidelity.rs,matrix_tables.rs,support/mod.rs:4,35}`、`crates/node-link-protocol/{Cargo.toml,src/command.rs:85-90,169-203}`、`crates/identity-auth/{Cargo.toml,src/authorization.rs:71,161}`、`crates/{core,node-link-protocol,identity-auth}/Cargo.toml` |
| Verification Evidence | 无（规划审查不得冒称已执行测试；本会话无 shell/测试执行能力，未运行 PV1/PV2、未跑 E2E） |
| Check Plan | plan.md `## Verification Strategy`（plan.md:337-372）的 PV1（plan.md:347）、PV2（plan.md:348）定义；只作为「各 WP 分支声明是否可产出该检查」的判据，未执行。本轮核对 Check ID：PV1、PV2 |
| Isolation / 限制 | 未继承任何实现/规划对话。**限制三项**：①无法重算 `contractDigest`；②**无 shell，因此无法重算 verification.md:14-19 的 6 个 sha256 基线**（F16 的第二问「与仓库当前文件内容一致」本轮只能给间接核对，见 F16 复核行）；③change 目录在 git 中未跟踪，无 diff 基线，`## Contract Changes` 的「proposal/specs 未变」只能做一致性核对 |
### 输入偏差（必须先记录）
派发说明称「三份报告都在仓库内，请实际读取」，但 `openspec/changes/session-resume/reports/` 实存文件仅 `dr1-dependency-review.md`、`dr1-dependency-review-round2.md`、`provisioner-worktrees.md`、`scout-target.md`（全仓库 `**/dr1*` 只命中前两份）。`reports/dr1-dependency-review-round3.md` **不存在**，而 plan.md:37、verification.md:41/146/147 都引用它。这不影响我对 F13–F17 的复核（我按派发要求自己读了脚本、源码与 schema，未采信修订说明），但影响证据链可读性 → 记为 **DR1-F19**。
## F13–F17 逐项独立复核
### DR1-F13（MAJOR）→ **已解决**
| 分问 | 结论 | 独立复核依据 |
| --- | --- | --- |
| (a) 解法是否为「PV1 阶段制 + WP6 写范围扩至整个 `crates/server/`」 | **是** | plan.md:347 PV1 行明写「分支（按 WP 的 crate 子集）/ 集成 / 候选 / 主分支」，阶段 1 = `cargo clippy -p <本 WP crate 列表>` + `cargo test -p <本 WP crate 列表>`，阶段 2 = `npm run check:rust`（workspace）；plan.md:251 WP6 Write Scope = `crates/server/, crates/app/, …`，Inputs/Outputs 明写由它补 `crates/server/src/node_link/command.rs` 的 `core_payload()` 臂；verification.md:56（Check Plan Changes）登记同一改动与代价 |
| (b) WP2 的分支检查是否真能产出（不编译 server） | **是** | WP2 的 crate 列表（plan.md:247）= `-p node-link-protocol -p core -p identity-auth`。依赖方向：`crates/server/Cargo.toml:27`、`crates/app/Cargo.toml:71`、`crates/identity-auth/Cargo.toml:18` 依赖 `node-link-protocol`，但**反向不存在**（`crates/node-link-protocol/Cargo.toml` 的依赖只有 `acpr-transcript`/`acpr-wire`/serde 系，dev-deps 只有 `sha2`；`crates/core/Cargo.toml` 无 dev-dependencies 段，依赖 `async-trait`/`thiserror`/`p256`/`sha2`；`crates/identity-auth/Cargo.toml` dev-deps 只有 `serde_json`）。因此该三包闭包不含 server/app/agent-host/storage-sqlite，`cargo {clippy,test} -p …` 不会编译 `core_payload`；`cargo fmt --all -- --check` 不做编译，不引入该失败面。且 WP2 的 PV2（`npm run check`）在自有改动集上确实可绿：`check-command-catalog.mjs` 的六个断言点（commands.json 自身、`packs ∪ grants` 成员集合 =:204、node-link schema 命令名集合 =:236-244、SYNC §11.5 =:241、SECURITY §10.2 =:245、`required_grant` 镜像 =:254）全部落在 WP2 写范围内，脚本改动本身也在（plan.md:247） |
| (c) 红窗口是否已在 `## Dependency Handoffs` 明写且有收口责任人 | **是** | plan.md:304-308 三段式：红窗口范围「从 WP2 合入集成基线起、到 WP6 合入为止」、性质「已知、有主、有界、不是失败」；责任人分工逐点列明（WP6 负责 `core_payload`/`NotTouched`/`ScriptedBackends` 与路由，WP5 负责 `crates/agent-host/`，WP4 负责 `crates/storage-sqlite/` 的失配测试字面量）；收口判据「候选阶段的 workspace PV1 必须全绿」（plan.md:308 + Completion Criteria plan.md:386）。失配根因经我实测确认：`core_payload()`（`crates/server/src/node_link/command.rs:2190-2265`）对 `&submit.payload` 的 match **无通配臂**（最后一条是显式 `WirePayload::SessionCreate(_)`），新增第 13 个变体即 E0004；函数内对 `command: CommandName` 的第二个 match 有 `_ =>`，故与 F14 的分工不冲突 |
| (d) 是否与 AGENTS.md §8 的本地入口冲突、是否已如实说明 | **无契约级冲突，「红窗口」已说明，但两处运行层细节未说明** | 阶段 2 = `npm run check:rust`（plan.md:347）与 `AGENTS.md` §8 的 `npm run verify` 的 Rust 半边逐字一致，且 plan.md:386 明写最终主分支必须阶段 2 全绿、阶段 1「不替代阶段 2」。**残余两处**：①`scripts/pre-commit.mjs:103` 在 Rust 提交上跑 workspace 全量 `cargo clippy -D warnings`（`.husky/pre-commit:4` 调用），红窗口内 WP3–WP6 的 Rust 提交会被钩子拒绝，只能用 `--no-verify`（AGENTS.md §8 允许，但 plan 未提）→ **F23**；②PV1 行把「集成」与候选/主分支并列进阶段 2，而红窗口恰覆盖集成期 → **F20** |
### DR1-F14（MAJOR）→ **已解决**
我自行枚举了全仓库 `impl <SessionBackendFactory|SessionEndpoint> for …`（含全限定写法），共 **6 处实现（7 个 impl 块）**：
| # | 位置 | 类型 | 是否在写范围内 | 归属 |
| --- | --- | --- | --- | --- |
| 1 | `crates/core/src/broker.rs:5300` / `:5335`（`#[cfg(test)]`） | FakeBackend / FakeEndpoint | 是 | **WP3**（`crates/core/`，plan.md:248） |
| 2 | `crates/agent-host/src/host.rs:449` | `AgentHost` | 是 | **WP5**（`crates/agent-host/`，plan.md:250；tasks 2.5 明写） |
| 3 | `crates/agent-host/src/session.rs:746` | `Endpoint` | 是 | **WP5** ✓ |
| 4 | `crates/server/src/node_link/command/tests.rs:437` / `:484` | FakeBackends / FakeEndpoint | 是 | **WP6**（`crates/server/`，plan.md:251） |
| 5 | **`crates/server/src/local_admin/test_support.rs:1021`** | `NotTouched` | **是（上轮指出的缺口已被 WP6 覆盖）** | **WP6** ✓ |
| 6 | `crates/app/tests/support/owner.rs:504` / `:619` | ScriptedBackends / ScriptedEndpoint | 是 | **WP6**（`crates/app/`，plan.md:251；also plan.md:292） |
结论：(a) 每个实现点都落在某个 WP 的写范围内（WP3/WP5/WP6）；(b) **不在的没有**，上轮的 `NotTouched` 已由 WP6 的整 crate 写范围覆盖，且 plan.md:304 与 tasks 2.6 都点名了该文件；(c) 候选/主分支 workspace PV1 因此可达全绿。另核：WP3 明确**不提供默认实现**（plan.md:248、design.md 未引入默认方法），与 `NotTouched` 需要补方法的推论一致；`crates/storage-sqlite` 不实现这两个 trait，故无第七处。
### DR1-F15（MINOR）→ **已解决**
`Shared File Ownership` 已登记两行且字段齐备：`crates/agent-host/tests/`（Writers=WP5,TP2；Merge Owner=TP2；Order=WP5→TP2；Re-verify=TP2: PV1）在 plan.md:291；`crates/app/tests/`（WP6,TP2；Merge Owner=TP2；Order=WP6→TP2；Re-verify=TP2: PV1）在 plan.md:292。两包分处 W3/W5、W4/W5，不同批。
### DR1-F16（MINOR）→ **部分解决（未完全落地）**
- 「摘要已在 `## Target`」= **是**：verification.md:11 的说明 + verification.md:14-19 的 6 行（proposal.md + 5 份 specs，文件集合正确）。
- 「tasks 1.5 已承载该动作」= **否**：tasks.md:13 的 1.5 正文只有「冻结本次契约并固定路径与版本：design.md 的 D1/D2/D3/D4/D6 与 5 份增量 specs；登记 plan.md 的 Shared File Ownership」，**没有** sha256/verification.md 的完成条件（tasks.md 全文匹配 `sha256` 为 0 命中）。而 plan.md:44（F16 行）与 verification.md:56、147 都声称「tasks 1.5 的完成条件补：把 5 份 specs 与 proposal 的 sha256 内容摘要写入 `verification.md` 的 `## Target`」→ **声称已改、实际未改**（→ **DR1-F18**）。
- 「摘要与当前文件内容一致」= **无法确认**：本会话无 shell，不能重算 sha256。间接证据：6 行覆盖的文件集合与 `## Target` 描述一致；5 份 specs 的 37 个标题与 Coverage Index 37 行仍逐条对应（见下方独立判断 3），未见漂移征兆，但这不能替代摘要复算。
### DR1-F17（SUGGESTION）→ **已解决**
全 change 目录检索 `design-rev`：现存标签只有 `design-rev-4`（plan.md:246、250、252、253、263，以及 plan.md:45 的 F17 行本身），无 `design-rev-2/3` 残留。plan.md:263 的 `openspec/changes/session-resume/specs/@design-rev-4` 是**同一** revision 字符串的略简写法（路径段 `specs/` 与 `@design-rev-4` 直接相连），属人读标识的措辞小瑕（plan.md:69 与 `procedures/workchain` 侧的模板确认此类标签不做机械校验），不另开 finding，建议顺手写成 `specs/**@design-rev-4`。
## 独立判断（派发要求的第 1–4 项）
**1. 是否还有「被编译/测试/门禁强制、却不在任何 WP 写范围内，或归属波次晚于该检查」的文件 —— 未发现新的强制缺口；仅一处条件性缺口。**
系统性枚举结果：
- **核心 trait 的实现点**：见 F14 表，6/6 有主（WP3/WP5/WP6）。
- **读取 `compatibility/**` 的 Rust 文件**：`crates/identity-auth/tests/authorization.rs:17`（`include_str!` commands.json，WP2 ✓）；`crates/node-link-protocol/tests/schema_drift.rs:166,202,216-218`（读 errors.json 与 commands.json，含 `declared.len() == 5` 的 grant 计数，在 WP2 的 crate 内 ✓）；`crates/acp-protocol/tests/support/mod.rs:35`（读 matrix.json，WP1 ✓）；`crates/server/src/node_link/conn/handshake.rs:251` 与 `crates/sync-protocol/tests/schema_drift.rs:332`（读 features.json 与 errors.json，**均不因本变更改动**：design 明写不新增 feature ID/错误码；server 侧文件由 WP6 整 crate 覆盖 ✓）。
- **被 `scripts/*` 校验的 `schemas/**` 资产**：`schemas/node-link/v1/**`（WP2 ✓）；`schemas/sync/v1/command.schema.json` 的 `commandName` 集合被 `check-command-catalog.mjs:235-240` 按 **sync transport 子集**断言 —— `session.resume` 的 transport 为 `["node_link"]`（design.md:41），sync 子集仍为 11，与 `crates/sync-protocol/src/command.rs:78` 的 `CommandName::ALL: [CommandName; 11]` 一致，**因此不需要也不应在 sync 侧改动** ✓（plan 未把 sync-protocol 写入任何 WP，正确）；`schemas/local-admin/v1/**`（未改动）与 `schemas/acp/compatibility-matrix.schema.json`（**不修改**，见 F10 复核：D5 改后的 `broker: project_and_preserve` 在 schema 枚举内，且 `acp:native`/`sync:explicit_unsupported`/`pwa:explicit_unsupported`/`facade:not_advertised` 对我逐字读过的 `$defs.layers` 枚举与 method 级 `allOf` 约束均合法）。
- **被门禁断言的 `docs/**` 表格**：SYNC §11.5（`check-command-catalog.mjs:241`，WP2 ✓）、SECURITY §10.2（`:245`，WP2 ✓）、LOCAL_ADMIN 方法小节/§5.1/§6/framing（`:104-170`，本变更不动 ✓）、NODE_LINK §11.3 feature 表（`check-features.mjs:14-80`；**关键核对**：`check-features.mjs` 只做 features.json ↔ 两份文档表 ↔ fixture 出现处三向比对，**不存在命令↔feature 的耦合**，故新增命令不强制新增 feature、也不强制改 SYNC §5.2 —— 与 WP2「只改 §11.5」一致）；CORE_PORTS §5/§7（`check-contract-drift.mjs:231,84`，WP3/WP4 ✓）。
- **唯一条件性缺口**：`fixtures/acp/v1/`（含 `manifest.json`）不在任何 WP 写范围，而 `check-schema-fixtures.mjs:295-300` 双向断言「fixtures 目录下每个 JSON 都必须在 manifest 登记」，`crates/acp-protocol/tests/support/mod.rs:4` 也把「新增用例必须登记进 `fixtures/acp/v1/manifest.json`」写成仓库约定 → **DR1-F24**（本轮不会自动失败，因 R1–R4 可只用内联字节，故记 SUGGESTION）。
- **既有测试字面量**：`crates/storage-sqlite/tests/` 中除已登记的 `migration.rs` 外，至少 8 个文件含全字段 `OwnedCommit{…}` / `NewSession{title,agent}` 字面量 → **DR1-F22**。
- 未发现「归属波次晚于该检查」的项：红窗口（F13/F14）已被 WP6 收口，WP3/WP4/WP5 的分支检查只查自有 crate，均早于红窗口收口。
**2. PV1 阶段制与 plan.md 其它声明的冲突 —— 存在 4 处口径不一致，均非阻断。**
①`## Local Checks`（plan.md:339-341）说的「按改动范围跑 `cargo clippy -p <crate>` / `cargo test -p <crate>`、局部自检不能代替 PV1/PV2」与 PV1 阶段 1 实际是同一动作，措辞上需读者自行理解为「不代替**阶段 2**」。②PV1 行的阶段列把「集成」并入阶段 2（plan.md:347），而 plan.md:304-308 又声明整个集成期 workspace PV1 预期为红；W2/W3/W4/W5 的 Enter Condition（plan.md:260-264）写「+ PV1/PV2 PASS」而未标阶段，两种读法都能成立。③PV2 行的工作包集合含 TP2（plan.md:348），但 TP2 自己的 Verification 列（plan.md:253）与 tasks 2.8 只标 `[PV1]`。④verification.md:49 的 `PV1 / 分支` 行仍是 workspace 全量命令 + `reports/PV1.log`，与阶段 1（按 crate 子集、证据 `reports/PV1-<WP>.log`）矛盾。→ 合并为 **DR1-F20**（①②③）与 **DR1-F21**（④）两项非阻断 finding。实质要求（候选/主分支阶段 2 全绿；Completion Criteria plan.md:385-386）明确且自洽，故不阻断。Coverage Index 的 `checks: [PV1, PV2]` 不绑定阶段，与阶段制不冲突。
**3. Execution Waves 层级算式 / Serialization Reason / Coverage Index 37 行 —— 仍合法。**
逐行验算（上游取声明依赖中最早层级 + 1）：WP1/WP2/TP1 = W1（none）；WP3 = WP2(W1)+1 = **W2** ✓；WP4 = WP3(W2)+1 = **W3** ✓；WP5 = max(WP1 W1, WP3 W2)+1 = **W3** ✓；WP6 = max(W1,W1,W2,W3,W3)+1 = **W4** ✓；TP2 = max(…,W4)+1 = **W5** ✓。8 行 Serialization Reason 全为 `NOT_APPLICABLE`，与 Runtime Resources 三行 Exclusive Scheduling 全 `NOT_APPLICABLE`（plan.md:310-314；端口用 `127.0.0.1:0`、tempdir 唯一、PV2 只读）自洽，取值合法。Coverage Index：我按 `^### Requirement:|^#### Scenario:` 逐份 specs 计数得 acp 4 + local-agent-host 8 + node-link-owner-server 11 + storage-schema-v2-migration 10 + workspace-resolution 4 = **37**，与 R1–R37 一一对应；37 行引用的任务集合 {2.1…2.6, 4.2} 在 tasks.md 中全部真实存在（2.1–2.6 为六个 WP 派发行，4.2 为 TP2 用例编写行），无悬空引用。
**4. `## Contract Changes` 的 F13–F17 表与实际改动一致性 —— F13/F14/F15/F17 一致，F16 不一致。**
F13 行声称的「阶段制 PV1」「WP6 写范围扩至整个 `crates/server/`」「红窗口与其拥有者写入 `## Dependency Handoffs` 与 `## Check Plan Changes`」我逐项在 plan.md:347/251/304-308 与 verification.md:56 找到实证；F14 行声称的四个下游实现点与「不采用默认实现」与我的枚举完全吻合（plan.md:248、304）；F15/F17 行与实际一致（plan.md:291-292、全库 `design-rev` 检索）。**F16 行声称「tasks 1.5 的完成条件补…」但 tasks.md:13 没有该内容**（plan.md:44 与实际不符）→ DR1-F18。
## Findings
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F18 | MINOR | `plan.md:44`（F16 行）、`plan.md:388`（Completion Criteria 的「（tasks 1.5）」）、`verification.md:56`/`:147`、`tasks.md:13` | plan.md 与 verification.md 三处声称「tasks 1.5 的完成条件补：把 5 份 specs 与 proposal 的 sha256 摘要写入 `verification.md` 的 `## Target`」，但 tasks.md:13 的 1.5 正文只有「冻结契约并固定路径与版本…登记 Shared File Ownership」；tasks.md 全文 `sha256` 0 命中。旁证：verification.md:14-19 的 6 个摘要确实已写入 | 摘要产物存在，但**计划/任务侧的追责点缺失**：plan.md:388 的完成判据指向一个不含该动作的任务行，1.5 无法按完成条件判定；「声称改过而实际未改」也会让后续轮次的机械核对失去依据 | 在 tasks.md 1.5 补完成条件（半句即可），或把 plan.md:388 与 `## Contract Changes` F16 行的出处改为 `verification.md` 的 `## Target` | 不适用（本轮新发现） |
| DR1-F19 | MINOR | `plan.md:37`、`verification.md:41`、`verification.md:146`、`:147` | 上述四处引用 `reports/dr1-dependency-review-round3.md`，但 `openspec/changes/session-resume/reports/` 实存仅 `dr1-dependency-review.md`、`dr1-dependency-review-round2.md`、`provisioner-worktrees.md`、`scout-target.md`；全仓库 `**/dr1*` 只命中前两份 | `## Dependency Declaration Review` 第 3 行（verification.md:41）是 `result=FAIL` 的 NEW 证据行，其 `report_path` **不可读**，违反 `roles/_shared/role-report.md` 的「NEW/REUSED 的报告路径必须可读」与 plan.md:254 自己声明的「报告可读」判据；本轮只能复核 R3 结论的转述，无法核对原始报告 | main 把第 3 轮报告按原样补齐到该路径（或把表内路径改为实际落盘文件名）后再进 premerge/final；若采用 `.log` 等其它载体，三处引用需同批改为可读文件 | 不适用（本轮新发现） |
| DR1-F20 | MINOR | `plan.md:347`（PV1 阶段列）、`plan.md:348`（PV2 工作包列）、`plan.md:260-264`（Enter Condition 的「+ PV1/PV2 PASS」）、`plan.md:339-341`（Local Checks） | ①阶段 2 写「集成/候选/主分支」，而 plan.md:304-308 声明「WP2 合入集成基线起…WP6 合入为止」workspace PV1 预期为红；②W2–W5 的进入条件写「PV1/PV2 PASS」未标阶段；③PV2 行含 TP2，而 plan.md:253 与 tasks 2.8 只标 TP2 的 `[PV1]`；④Local Checks 的「不代替 PV1/PV2」需读者自行限定为阶段 2 | 同一份计划对「集成阶段/进入条件里的 PV1 是否必须 workspace 全绿」给出两种读法：一种与红窗口声明冲突，一种与阶段 1 一致。实质要求（候选/主分支阶段 2 全绿，plan.md:308、386）已明确，故不阻断 | PV1 行的阶段列把「集成」限定为「全部 WP 集成后的集成基线（候选前）」；Enter Condition 与 Dependency Handoffs 统一写「PV1（阶段 1）+ PV2」；PV2 行的工作包集合与 TP2 行对齐；Local Checks 补「（阶段 2）」 | 不适用（本轮新发现） |
| DR1-F21 | MINOR | `verification.md:49` | 该行是 `PV1 / 分支 / WP1–WP6、TP2`，命令写 `cargo clippy --locked --workspace … && cargo test --locked --workspace --all-features`，证据写 `reports/PV1.log`；plan.md:347 的阶段 1 定义是按本 WP 的 crate 子集，证据写 `reports/PV1-<WP>.log` | 检查记录行仍描述旧口径：执行者可能按 workspace 全量跑「分支」行并把结果填进去，从而**掩盖阶段 1 与阶段 2 的区别**，让红窗口内的分支证据不可解释 | 把该行的命令与 Evidence 改为阶段 1 定义（或拆成「分支（阶段 1）」与「集成/候选/主分支（阶段 2）」两行），与 plan.md:347 逐字对齐 | 不适用（本轮新发现） |
| DR1-F22 | MINOR | `plan.md:290`（通用行）、`plan.md:276`（`crates/storage-sqlite/tests/migration.rs` 唯一登记行）；实证位置 `crates/storage-sqlite/tests/{commit.rs:77,retention.rs:58,session_version_rule.rs:66,enum_coverage.rs:363,imported.rs:622,compaction_recovery.rs:51,attachments.rs:27,contract_v03.rs:77}` | 这些既有测试文件都含**全字段** `OwnedCommit{…}` 与 `NewSession{title,agent}` 字面量（我逐处读过 retention.rs:54-70、enum_coverage.rs:359-380 等），而 design.md:56-74（D2）与 plan.md:249（WP4 Inputs/Outputs「修复本 crate 内因提交形状变化而失配的既有测试字面量」）都要求 core 的提交形状携带两个新字段；plan.md:290 却把 WP1–WP6 的写范围表述为「各自 `src/**`…与上表逐文件登记的既有测试目录/文件」，登记行只有 `migration.rs` | 文件级登记与 WP4 的实际写入面不一致（与 F15 同类）：按 plan.md:290 的字面读法，WP4 会「写到未登记文件」。因 WP4 的 WP 级写范围是**整个** `crates/storage-sqlite/`，且与 TP2 分处 W3/W5 不并发，不存在「同一处却同批」，故不阻断 | 按 F15 的同样做法补登 `crates/storage-sqlite/tests/`（WP4, TP2；Merge Owner=TP2；Order=WP4 → TP2；Re-verify=TP2: PV1），或把 plan.md:290 的措辞改为「整 crate（含 `tests/`）＋ TP2 独占新增文件」 | 不适用（本轮新发现） |
| DR1-F23 | MINOR | `plan.md:304-308`（红窗口段）；实证 `.husky/pre-commit:4` → `scripts/pre-commit.mjs:103` | 钩子在**涉及 Rust 的提交**上运行 `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（workspace 全量）；WP2 之后 `crates/server/src/node_link/command.rs:2190-2265` 的 `core_payload()` 穷尽匹配失配（我已确认该 match 无通配臂）⇒ 红窗口内 WP3–WP6 分支上的**每次 Rust 提交都会被钩子拒绝**，除非 `git commit --no-verify` | 执行者会遇到「计划说分支检查可产出、钩子却红」的落差，可能被误判为计划失败并触发无谓返工。AGENTS.md §8 明说钩子是「更早发现失败」而非门禁本体、可 `--no-verify`，CI 也只在 push/PR 执行，故不阻断 | 在 plan.md 的红窗口段补一句：红窗口内的 Rust 提交需 `--no-verify`（或接受钩子的 workspace clippy 失败），并说明这不改变 PV1 阶段 1/阶段 2 的判定责任 | 不适用（本轮新发现） |
| DR1-F24 | SUGGESTION | `plan.md:246`（WP1 写范围）、`plan.md:253`（TP2 写范围）；实证 `scripts/check-schema-fixtures.mjs:295-300`、`crates/acp-protocol/tests/support/mod.rs:4` | WP1 写范围 = `crates/acp-protocol/` + `compatibility/acp/v1/matrix.json` + `docs/ACP_COMPATIBILITY_MATRIX.md`；TP2 写范围 = 「`crates/**/tests/` 的**新增**用例文件」。而 `fixtures/**` 下每个 JSON 都必须在对应 `manifest.json` 登记，且仓库注释明确要求新用例登记进 `fixtures/acp/v1/manifest.json` | 本轮**不会**自动失败（R1–R4 的 round-trip/缺字段用例可只用内联字节实现），但若实现者按仓库既有做法新增 ACP fixture，就会写到无归属路径并使 WP1/TP2 的 PV2 变红（manifest 未登记即 FAIL） | 二选一：把 `fixtures/acp/v1/`（含 `manifest.json`）登记进 WP1 写范围；或在 tasks 2.1 明写「不新增 ACP fixture，round-trip 用例以内联字节实现」 | 不适用（本轮新发现） |
`SAFE-NOTE`：除上表与三处限制外，未发现其它与派发 Scope 相关的具体问题。
## Check Plan Reconciliation
- **PV1/PV2 本轮未执行**（规划审查 + 只读边界），只核对「在各 WP 声明的基线上能否产出」：WP1 `-p acp-protocol` / WP2 `-p node-link-protocol -p core -p identity-auth` / WP3 `-p core` / WP4 `-p storage-sqlite` / WP5 `-p agent-host` / WP6 `-p server -p app` 的 crate 列表与其写范围逐条一致，且除 WP6 外都不在红窗口内（WP2 的列表经 F13(b) 证明不编译 server）→ **各 WP 的分支级 PV1 均可产出**；PV2 的断言点全部落在 WP1/WP2/WP3/WP4/WP6 的写范围内，且 PV2 不受红窗口影响（每处合同改动与其镜像/文档同包同提交：WP2 的 commands.json+broker 臂+identity-auth+两文档，WP3 的 ports.rs+§5.1，WP4 的 migrate.rs+§7）→ **PV2 全程可产出**。
- **待补证据**：PV1/PV2 的执行记录应在 tasks 3.1（分支，`reports/PV1-<WP>.log`）、6.3（候选，`reports/PV1.log`/`PV2.log`）、6.7 与 8.1/8.2（主分支）由对应执行者补齐；7.1 的独立验证报告在 `reports/validation-session-resume.md`。这些**不影响本轮静态规划判断**，但 F13/F14 的收口结论只有在候选阶段实跑 workspace 全量 PV1 为绿时才算闭环 —— 应由 main 在 premerge 前核对。
- **Main E2E = not-applicable**（plan.md:356-366，含 `downgrade_approval` 记录的 2026-09-30 用户原话与替代检查 C1/C2）。该批准无法从仓库内自证（属会话原件），本轮不做 E2E 判据表核对；8.3 的 `e2e check` 门禁由 main 执行。
- **未核对项**：`contractDigest` 取值本身、verification.md:14-19 的 6 个 sha256 基线（无 shell）、proposal/specs 的字节级未改动（无 diff 基线，只能做一致性核对）、`reports/dr1-dependency-review-round3.md` 的原始内容（文件缺失）。
## Assessment
**结论：PASS**（对应 Target Revision = `sha256:0b6f6b5246545831da07583f9352151f7d5cc4c50db474eb8b056e627d691f11`）。
依据 `roles/reviewer.md`：只有存在**已确认且未解决**的 CRITICAL/MAJOR 才判 FAIL。本轮对 F13–F17 逐项独立复核（自读脚本、源码、schema、Cargo.toml 与钩子，未采信修订说明）后：
- **F13 已解决**：阶段制 PV1 在 plan.md:347 落地；WP2 的分支检查经依赖闭包核对确实不编译 server（F13(b)）；红窗口、边界与收口责任人（WP6）在 plan.md:304-308 明写；与 AGENTS.md §8 无契约级冲突（阶段 2 与 `npm run verify` 的 Rust 半边逐字一致，候选/主分支全绿为硬要求）。
- **F14 已解决**：全仓库 6 处 trait 实现点全部有主，上轮点名的 `crates/server/src/local_admin/test_support.rs:1021` 已由 WP6 的整 crate 写范围覆盖；候选/主分支 workspace PV1 可达全绿。
- **F15/F17 已解决**；**F16 部分解决**（摘要已写入 verification.md `## Target`，但 tasks 1.5 的完成条件未落地 → F18，MINOR；摘要值本轮无法复算）。
- **非阻断项 7 项**：F18（tasks 1.5 未落地，MINOR）、F19（R3 报告缺失、Dependency Declaration Review 第 3 行报告不可读，MINOR）、F20（PV1/PV2 阶段与进入条件的口径，MINOR）、F21（verification.md:49 的 PV1 分支行仍是旧口径，MINOR）、F22（`crates/storage-sqlite/tests/` 除 migration.rs 外未登记，MINOR）、F23（红窗口内 pre-commit 钩子的 workspace clippy，MINOR）、F24（`fixtures/acp/v1/` 未登记，SUGGESTION）。
- 已核实为**合规**、不构成反对意见：Execution Waves 的 W1–W5 层级算式与 Enter Condition 逐行一致、Serialization Reason 取值合法；四对已登记双写者（`crates/core/src/broker.rs`、`README.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`crates/storage-sqlite/tests/migration.rs` 与 `agent-host/tests`、`app/tests` 的 TP2 复用）均分处不同批且 Merge Owner/Order/Re-verify 齐备、区域不相交；`session.resume` 的 `transport=["node_link"]` 使 sync 侧（`schemas/sync/v1`、`sync-protocol`、SYNC §5.2 feature 表）**无需也不应**改动；D5 改后的 layers/`delivery` 取值对 `schemas/acp/compatibility-matrix.schema.json` 合法；Coverage Index 37 行与 5 份 specs 的 37 个标题、tasks 引用全部对应。
建议 main 在同一轮修订里顺手清掉 F18–F23（都是半句话级别的改动；F19 是把第 3 轮报告补齐），然后再进 tasks 3.1 分支执行；F24 属于「登记或明确声明不做」的二选一。本报告不更新任何任务状态，也不代表整个变更可归档，不代替 Project Verify / E2E / 独立验证。
```yaml
handoff_index: [{task_id: NOT_APPLICABLE, work_package: NOT_APPLICABLE, role: reviewer, phase: plan, stage: plan, round: 4, target_revision: "sha256:0b6f6b5246545831da07583f9352151f7d5cc4c50db474eb8b056e627d691f11", evidence_type: REVIEW, evidence_id: DR1, report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round4.md, result: PASS, evidence_status: NEW, applicability_basis: "针对 F13-F17 修复后的 plan.md 当前 contractDigest 做静态规划审查：独立复核 gating 脚本/源码/schema/Cargo.toml/hooks 判断各 WP 分支检查可产出性与写范围归属；未执行 PV1/PV2、未跑 E2E、未针对代码 diff，Base Revision 为 NOT_APPLICABLE；限制：无法重算 contractDigest 与 verification.md 的 sha256 基线，reports/dr1-dependency-review-round3.md 在仓库中缺失", source_evidence: NOT_APPLICABLE}]
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "完整报告已返回：Review Context（逐项 Inputs 实际取值 + 三项能力限制 + R3 报告缺失的输入偏差）、F13-F17 逐项复核表（含 F13 的四分问 (a)-(d) 与 F14 的 6 处实现点枚举表）、四项独立判断（系统枚举被编译/门禁强制的写目标、PV1 阶段制冲突面、Waves 算式与 37 行 Coverage、Contract Changes 一致性）、新发现 DR1-F18…F24（6×MINOR + 1×SUGGESTION）、Check Plan Reconciliation、Assessment=PASS、单行 handoff_index"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [],
  "validationOutput": [
    "F13(a) 确认：plan.md:347 PV1 阶段制（阶段1 按 WP crate 子集 / 阶段2 npm run check:rust）；plan.md:251 WP6 写范围=crates/server/,crates/app/；verification.md:56 Check Plan Changes 登记同一改动与代价",
    "F13(b) 确认：WP2 分支 crate 列表（plan.md:247）= -p node-link-protocol -p core -p identity-auth；反向前提被排除（node-link-protocol 只依赖 acpr-transcript/acpr-wire/serde，dev-deps=sha2；core 无 dev-deps；identity-auth dev-deps=serde_json），server/app/agent-host/storage-sqlite 不在闭包内，故 core_payload 不参与编译",
    "F13(c) 确认：plan.md:304-308 红窗口范围（WP2 合入集成基线 → WP6 合入）、性质、责任人分工（WP6/WP5/WP4）与收口判据（候选阶段 workspace PV1 全绿）齐备；根因实测：crates/server/src/node_link/command.rs:2190-2265 对 &submit.payload 的 match 无通配臂（末臂为显式 WirePayload::SessionCreate(_)）",
    "F13(d)：与 AGENTS.md §8 无契约级冲突（阶段2 = npm run check:rust 与 npm run verify 的 Rust 半边逐字一致；plan.md:386 要求最终主分支阶段2 全绿且阶段1 不替代阶段2）；残余两处运行层细节未说明 → F23（scripts/pre-commit.mjs:103 的 workspace clippy）、F20（集成阶段口径）",
    "F14：全仓库 impl SessionBackendFactory/SessionEndpoint 命中 6 处（core/broker.rs:5300/5335、agent-host/src/host.rs:449、agent-host/src/session.rs:746、server/src/node_link/command/tests.rs:437/484、server/src/local_admin/test_support.rs:1021、app/tests/support/owner.rs:504/619），全部落在 WP3/WP5/WP6 写范围内；上轮点名的 NotTouched 已被 WP6 覆盖；plan.md:248 明写不提供默认实现",
    "F15：plan.md:291 crates/agent-host/tests/（WP5,TP2）与 plan.md:292 crates/app/tests/（WP6,TP2）两行齐备（含 Merge Owner/Order/Re-verify）",
    "F16：摘要确在 verification.md:11,14-19（proposal + 5 份 specs，集合正确）；tasks.md:13 的 1.5 无 sha256 完成条件（tasks.md 全文 sha256 0 命中），与 plan.md:44/verification.md:56,147 的声称不符 → 部分解决 + F18；摘要值本轮无法重算（无 shell）",
    "F17：全 change 目录 design-rev 检索只剩 design-rev-4（plan.md:246,250,252,253,263 与 plan.md:45），无 design-rev-2/3 残留 → 已解决（plan.md:263 的 specs/@design-rev-4 为同一字符串的略简写法，措辞小瑕）",
    "系统枚举未发现新的强制无主写目标：identity-auth/tests/authorization.rs:17 与 node-link-protocol/tests/schema_drift.rs:166,202,216-218（WP2 内）、acp-protocol/tests/support/mod.rs:35（WP1 内）、server/src/node_link/conn/handshake.rs:251 与 sync-protocol/tests/schema_drift.rs:332（资产不改动）；schemas/node-link/v1（WP2）、schemas/sync/v1 的 commandName 集合因 session.resume 无 sync transport 保持不变（sync-protocol/src/command.rs:78 == 11）；check-features.mjs 无命令↔feature 耦合，故不强制新增 feature 或改 SYNC §5.2；唯一条件性缺口 fixtures/acp/v1/（check-schema-fixtures.mjs:295-300 的双向 manifest 断言）→ F24",
    "Waves 算式复核：W1{WP1,WP2,TP1} → WP3=W2 → WP4=W3、WP5=max(1,2)+1=W3 → WP6=max(1,1,2,3,3)+1=W4 → TP2=W5，与 Enter Condition 逐行一致；8 行 Serialization Reason 全 NOT_APPLICABLE 且与 Runtime Resources 三行自洽；Coverage 37 行标题计数 4+8+10+11+4=37，tasks 引用集合 {2.1..2.6,4.2} 全部实存",
    "Contract Changes 一致性：F13/F14/F15/F17 与实证一致；F16 声称的 tasks 1.5 改动实际未落地（plan.md:44 vs tasks.md:13）→ F18",
    "新发现项：提交形状涟漪波及 crates/storage-sqlite/tests/{commit.rs:77,retention.rs:58,session_version_rule.rs:66,enum_coverage.rs:363,imported.rs:622,compaction_recovery.rs:51,attachments.rs:27,contract_v03.rs:77} 的全字段 OwnedCommit/NewSession 字面量，仅 migration.rs 被登记（plan.md:276,290）→ F22"
  ],
  "residualRisks": [
    "无法重算 contractDigest（无 OpenSpec 引擎调用能力），按派发值 sha256:0b6f6b52…1f11 绑定",
    "无法重算 verification.md:14-19 的 6 个 sha256 契约基线（无 shell），F16 的『与当前文件内容一致』本轮只能给间接核对（37 标题与文件集合一致，无漂移征兆）",
    "reports/dr1-dependency-review-round3.md 在仓库中不存在，F13-F17 只能按 plan.md/verification.md 的转述复核（实质已独立复核，但原始报告无法核对）→ F19 需在 premerge/final 前闭环",
    "change 目录在 git 中未跟踪，无 diff 基线，『proposal/specs 未变』只能做一致性核对，无法逐字节证伪",
    "本轮未执行 PV1/PV2（规划审查 + 只读边界）；F13/F14 的红窗口收口只有在候选阶段实跑 workspace 全量 PV1（tasks 6.3/6.7）为绿时才算闭环，该结论不属本轮静态判断",
    "F24 是条件性风险：若实现者按仓库既有约定新增 ACP fixture 而 main 未登记 fixtures/acp/v1/，WP1/TP2 的 PV2 会在实现期变红"
  ],
  "noStagedFiles": true,
  "diffSummary": "无 diff：本轮为只读规划审查，未修改、未暂存任何仓库文件，未切换分支、未提交",
  "reviewFindings": [
    "no blockers: 未发现已确认且未解决的 CRITICAL/MAJOR；F13/F14/F15/F17 复核为已解决，F16 部分解决（非阻断）",
    "non-blocking: tasks.md:13 - 1.5 未承载 specs/proposal 的 sha256 基线，与 plan.md:44、verification.md:56/147、plan.md:388 的声称不符（DR1-F18, MINOR）",
    "non-blocking: openspec/changes/session-resume/reports/dr1-dependency-review-round3.md - 报告缺失，verification.md:41 的 NEW/FAIL 行报告路径不可读（DR1-F19, MINOR）",
    "non-blocking: plan.md:347/348, plan.md:260-264, plan.md:339-341 - PV1『集成』阶段与进入条件的口径、PV2 工作包集合与 TP2 不一致（DR1-F20, MINOR）",
    "non-blocking: verification.md:49 - PV1 分支行仍是 workspace 全量命令 + reports/PV1.log，与阶段制阶段 1 矛盾（DR1-F21, MINOR）",
    "non-blocking: plan.md:290, plan.md:276 - crates/storage-sqlite/tests/ 除 migration.rs 外未登记，而 WP3 的提交形状变化会打破其 8 个文件的字面量（DR1-F22, MINOR）",
    "non-blocking: plan.md:304-308 - 红窗口内 .husky/pre-commit → scripts/pre-commit.mjs:103 的 workspace clippy 会拦住 Rust 提交，未在计划说明（DR1-F23, MINOR）",
    "non-blocking: plan.md:246, plan.md:253 - fixtures/acp/v1/（含 manifest.json）无归属，若新增 ACP fixture 会使 PV2 变红（DR1-F24, SUGGESTION）"
  ],
  "manualNotes": "本轮为 DR1 的第 4 轮（recheck，Review Type=plan），第四个新实例，未参与前三轮与任何实现。判定 PASS：不存在已确认且未解决的 CRITICAL/MAJOR，7 项非阻断 находки全部为半句话级修订。需 main 注意三件事：①派发说明称三份历史报告都在仓库内，实际缺第 3 轮报告，Dependency Declaration Review 第 3 行的报告路径不可读（F19）；②plan.md 的 F16 行声称 tasks 1.5 已补 sha256 完成条件，实际未落地（F18）——这两项都是『声称与实际不一致』，建议同批修正以免第 5 轮再判；③本轮无法重算 contractDigest 与 6 个 sha256 基线（无 shell/引擎能力），属能力限制而非未检查，commandsRun 为空亦出于此。main 需把本报告原样持久化到 openspec/changes/session-resume/reports/dr1-dependency-review-round4.md，并在 verification.md 的 ## Dependency Declaration Review 第 4 行填入 Plan Revision = sha256:0b6f6b52…1f11 与 result=PASS，同时按 F18-F24 回填 ## Review Findings 与 ## Failures and Retests。"
}
```
