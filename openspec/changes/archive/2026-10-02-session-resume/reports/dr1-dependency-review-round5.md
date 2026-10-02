# DR1 — Dependency Declaration Review（session-resume / plan.md），Round 5（recheck）
## Shared Report
```
task_id: NOT_APPLICABLE（plan/tasks.md 未为「依赖声明审查」建任务行；该门禁由 plan.md 的 Dependency Declaration Review 段与 verification.md 的 ## Dependency Declaration Review 表承载）
role: reviewer
phase: plan
stage: plan
round: 5
agent_context: standalone task-level reviewer subagent（第五个新实例；未继承 Round 1–4 或任何实现/规划对话；本轮只读，无 write 工具，未修改/暂存/切换分支/提交任何文件）
target_revision: sha256:3b208486402023d4d52139b494590756b352b6c908c164f217b1bcd94ab1391d
scope: 只复核本轮修改面——tasks.md 1.5、plan.md 的 Contract Changes（F16 行）、PV1/PV2 阶段列与 Work Packages 的 Verification 列、Execution Waves 的 Enter Condition、
       Local Checks、Shared File Ownership（storage-sqlite/tests 行）、红窗口段、WP1 写范围（fixtures/acp/v1/）与 tasks 2.1/2.2、3.1，
       以及 verification.md 的 ## Target / ## Handoff Index / ## Dependency Declaration Review / ## Checks / ## Check Plan Changes / ## Review Findings / ## Failures and Retests；
       并交叉核对门禁脚本 / Rust 源码 / Cargo 依赖以判断「各 WP 阶段 1 检查是否可产出、是否仍有无主强制写目标」；不读代码 diff、不跑测试
changes: 无（只读；未修改、未暂存、未切换分支、未提交任何文件）
checks: NOT_APPLICABLE（规划审查不执行 PV1/PV2，不冒称已执行）
issues: 2×MINOR（DR1-F26、DR1-F25）+ 2×SUGGESTION（DR1-F27、DR1-F28）；0×CRITICAL、0×MAJOR
        复核结论：F18/F20/F21/F22/F23/F24 已解决；F19 的补救（Round 3 报告落盘）已解决，但其「四份报告现已全部可读」的断言不实（Round 4 报告仍缺失）→ 记 DR1-F25；
        F20 只在 plan.md 侧闭环，verification.md 的 PV2 记录行仍是旧口径 → 记 DR1-F26
result: PASS
evidence_paths: 本报告（openspec/changes/session-resume/reports/dr1-dependency-review-round5.md，由 main 持久化）
resource_cleanup: 未创建/启动/停止任何资源；未写仓库文件
```
## Review Context
| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / 5（同一线程；Type=plan、Review ID 沿用，显式 round=5；当前结论以本轮为准） |
| Review Type / Stage | plan / plan |
| Work Package | NOT_APPLICABLE（不代表任何 WP，非 WP Owner） |
| Repository | `D:/Project/acp-remote`（当前工作区；只读，未切分支、未建检视 worktree） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `sha256:3b208486402023d4d52139b494590756b352b6c908c164f217b1bcd94ab1391d`（派发给出的当前 `contractDigest`）。本会话无 OpenSpec 引擎调用能力，**无法重算该摘要**，按派发值绑定，不声称已复核其内容映射 |
| Previous Findings | `reports/dr1-dependency-review.md`（R1 FAIL，F1–F8）、`reports/dr1-dependency-review-round2.md`（R2 FAIL，F9–F12）、`reports/dr1-dependency-review-round3.md`（R3 FAIL，F13–F17，本轮确认实存且完整，见 F19 复核）、R4（PASS、F18–F24）的报告：**`reports/dr1-dependency-review-round4.md` 在本仓库中不存在**（对 `reports/…` 与 change 目录相对路径两处直读均 ENOENT，全库 `**/*dependency-review*` 检索只命中 3 份）→ 本轮的 F18–F24 **原文**只能取自 verification.md 的 `## Review Findings` 行与派发摘要，见 DR1-F25 与「限制」 |
| Requirements（已读全文） | `proposal.md`、5 份增量 specs（`acp-wire-protocol`、`local-agent-host`、`node-link-owner-server`、`storage-schema-v2-migration`、`workspace-resolution`）、`design.md`（D1–D6 + Migration/Risks）、`plan.md`（Scope and Contracts / Contract Changes R1–R3 / Coverage Index 37 行 / Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs / Runtime Resources / Target / Merge Strategy / Verification Strategy / Main E2E / Independent Validation / Completion Criteria）、`tasks.md`（1.1–9.1）、`verification.md`（全文） |
| Project Rules（已读） | `AGENTS.md` §4/§8/§9/§10/§11/§12；`roles/reviewer.md`、`roles/_shared/role-report.md`；`openspec/schemas/agentic/templates/plan.md`（Contract Changes 定位、Serialization Reason 取值与「晚于最早层级」判据）、`procedures/workflow-check.md`（plan/premerge/final 的输入与完成条件、Contract Freeze/报告路径口径） |
| 本轮交叉核对的机器资产（只读） | `scripts/check-command-catalog.mjs`（`parseRequiredGrant`/`compareSets`）、`scripts/check-schema-fixtures.mjs`（manifest 双向断言）、`scripts/pre-commit.mjs`（钩子步骤）、`package.json`（`check` 脚本链）、`fixtures/acp/v1/`（目录实存含 `manifest.json`）；`crates/core/src/broker.rs`（`command_name`/`required_grant`）、`crates/identity-auth/src/authorization.rs` + `tests/authorization.rs`（`GRANTS` 与 `scopes.len()==6` 硬编码计数）、`crates/storage-sqlite/tests/**`（`OwnedCommit`/`NewSession`/`SessionUpdate` 字面量分布）、`crates/*/Cargo.toml`（`node-link-protocol` 的依赖方 = app/server/identity-auth）；`docs/DEVELOPMENT_PLAN.md`、`docs/SESSION_CONTINUITY_DESIGN.md`（实存）；`design.md` 中 PV1/PV2/阶段 无命中（无冲突面） |
| Verification Evidence | 无（规划审查不得冒称已执行测试；本会话无 shell/测试执行能力，未运行 PV1/PV2、未跑 E2E） |
| Check Plan | plan.md `## Verification Strategy`（新阶段制 PV1 / PV2），仅作为「各 WP 声明的检查是否可产出」的判据，未执行；本轮核对 Check ID：PV1、PV2（另核对 C1/C2 与 `## Checks` 的对应） |
| Isolation / 限制 | 未继承任何实现或规划对话；无法重算 `contractDigest`；change 目录未跟踪（`watchdog_diff` 显示无已跟踪文件的 staged/unstaged 改动，仅 `openspec/changes/session-resume/**` 未跟踪），因此**没有可 diff 的基线**；**无法读取 Round 4 报告原文**；无法用 sha256 校验 `## Target` 的 6 个摘要是否与当前 proposal/specs 逐字节相符（只做了内容一致性核对）；`## Worktree Handoff` 的 TP2 行仍「待 provisioner 补发」（plan 已声明该前置） |
## DR1-F18–F24 逐项复核（沿用原问题 ID；独立读取现行文件，未采信处置说明）
| ID | 原级别（R4） | 本轮结论 | 复核依据 |
| --- | --- | --- | --- |
| DR1-F18 | MINOR | **已解决** | (a) `tasks.md` 1.5 现已带显式完成条件：「完成条件额外包括（DR1-F16/F18）：把 `proposal.md` 与 5 份 `specs/**/spec.md` 的 **sha256 内容摘要**写入 `verification.md` 的 `## Target`…使后续轮次可机械核对」；(b) `plan.md:44`（Contract Changes 的 F16 行）引用的位置**正确**：写的是「写入 `verification.md` 的 `## Target`」，而该节 `verification.md:11-20` 确有 `行为契约基线（tasks 1.5，DR1-F16）` 与 6 行 sha256（`proposal.md` + 5 份 spec，逐文件点名）；(c) `plan.md:6` 的 Specs Revision 也指向同一节；(d) `plan.md:392` 的 Completion Criteria 与之一致。计数自洽：5 份 specs + proposal = 6 行摘要 ✓。**唯一残余**是摘要数值本身无法在本会话内核验（见限制） |
| DR1-F19 | MINOR | **部分解决 → 原问题（Round 3 报告缺失）已解决；同一缺陷在 Round 4 行复发并新增不实断言 → DR1-F25** | `reports/dr1-dependency-review-round3.md` 现**实存且内容完整**：含 Shared Report 字段块、Review Context 表、Round 2 findings 复核表、独立判断 1–4、Findings 表（F13–F17）、Check Plan Reconciliation、Assessment、yaml `handoff_index` 与 acceptance-report。`verification.md:41` 第 3 行的 `report_path` = `openspec/changes/session-resume/reports/dr1-dependency-review-round3.md` **可读** ✓。但同一行/`verification.md:116` 的处置句「**四份报告现已全部可读**」与事实不符：Round 4 的报告缺失（`verification.md:42` 第 4 行为 `result=PASS`/`NEW` 行，其 `report_path` 不可读；`verification.md:157` 的 `## Failures and Retests` 也引用同一路径） |
| DR1-F20 | MINOR | **plan.md 侧已解决；verification.md 侧未同步 → DR1-F26** | 逐项核对：(a) `plan.md:348` PV1 阶段列 = 「分支（阶段 1，按 WP 的 crate 子集）/ 集成基线（阶段 2，**全部 WP 集成后、候选前**）/ 候选 / 主分支；全部 WP/TP」✓「集成」已限定；(b) `plan.md:264`（W2）Enter Condition 明写「**PV1 阶段 1** 与 PV2 PASS」✓，W3–W5（`:265-268`）用 W2 行定义过的术语「已验收集成基线」（见 F27 的可读性建议）；(c) PV2 的工作包集合 `plan.md:349` = WP1、WP2、WP3、WP4、WP6，与 Work Packages 逐行核对**完全一致**（WP5 只标 PV1、TP1 为 N/A、TP2 只标 PV1；`tasks.md` 2.5/2.8 的 `[PV1]` 标记同口径）✓；(d) `plan.md:341`（Local Checks）已补「局部自检不能代替 PV1 的**阶段 2**（workspace 全量）与 PV2」✓。**但** verification.md 的 PV2 记录行仍是旧口径（见 F26） |
| DR1-F21 | MINOR | **已解决（语义一致，非逐字）** | `verification.md:50-52` 已拆三行：`PV1 / 分支（阶段 1）/ WP1–WP6、TP2`（Scope=「本 WP 拥有的 crate 子集」，命令 = `cargo fmt --all -- --check` + `cargo clippy --locked -p <本 WP crate 列表> …` + `cargo test --locked -p <本 WP crate 列表> …`，证据 `reports/PV1-<WP>.log`）、`PV1 / 集成基线（阶段 2）/ 全部`（`npm run check:rust`，证据 `reports/PV1.log`）、`PV1 / 候选、主分支（阶段 2）/ 全部`（同）。与 `plan.md:348` 的口径**逐项一致**（crate 列表同样委派给 Work Packages 的 Verification 列）；差异只是缩写（Checks 行不重复「全部 WP 集成后、候选前」字样，而在 `plan.md:348` 与 `plan.md:388` 的 Completion Criteria 中均有），不构成矛盾 |
| DR1-F22 | MINOR | **已解决** | `plan.md:277` 现登记**整目录** `crates/storage-sqlite/tests/`：Writers=WP4,TP2；Merge Owner=TP2；Merge Order=`WP4 → TP2`；Re-verify=`TP2: PV1`，区域说明点名 `migration.rs`、`commit.rs`、`retention.rs`、`enum_coverage.rs`「等含 `OwnedCommit{…}`/`NewSession{…}` 全字段字面量的文件」。我独立核对了字面量分布：`commit.rs`、`retention.rs`、`enum_coverage.rs`、`migration.rs`（`:1140`）、`session_version_rule.rs`、`attachments.rs`、`compaction_recovery.rs`、`contract_v03.rs`、`imported.rs` **全部落在该目录内**，且 `NewSession { … }` 字面量在**全库**只出现在 `crates/core/src/**`（WP3 ✓）与该目录（WP4 ✓）——即「提交形状变更的涟漪面」已被该行完整覆盖。点名的 4 个文件确实含该类字面量（准确），其余以「等」收束（`OwnedCommit`/`NewSession` 亦见于 `attachments.rs`/`compaction_recovery.rs`/`contract_v03.rs` 等），与目录级登记并用后不影响归属完备性 |
| DR1-F23 | MINOR | **已解决** | `plan.md:309` 红窗口段新增：「`.husky/pre-commit` → `scripts/pre-commit.mjs` 在涉及 Rust 的提交上跑 **workspace 全量** `cargo clippy -D warnings`，因此红窗口内 WP3–WP6 的 Rust 提交会被钩子拒绝。按 `AGENTS.md` §8，钩子是「更早发现失败」而非门禁本体，此时可用 `git commit --no-verify`；这不改变 PV1 阶段 1/阶段 2 的判定责任，也不得成为跳过其它钩子检查的借口（非 Rust 部分仍须为绿）」；`tasks.md:37`（3.1）同步注明。机器事实核对：`scripts/pre-commit.mjs` 在 `rustTouched` 时确实追加 `cargo ["clippy","--locked","--workspace","--all-targets","--all-features","--","-D","warnings"]`（与 CI 参数逐字一致），且 `npm run check` 无条件运行 ✓ 描述准确；`AGENTS.md` §8 亦确实把钩子定位为「更早发现失败」而非门禁实现 ✓ |
| DR1-F24 | SUGGESTION | **已解决** | `plan.md:246`（WP1 写范围）已含 `fixtures/acp/v1/（含 manifest.json；DR1-F24 登记）`，其 Inputs/Outputs 明写「若要新增 ACP fixture，必须同时登记 `fixtures/acp/v1/manifest.json`（`check:schema-fixtures` 双向断言）；也可只用内联字节，此时不得新增 fixture 目录内容」；`tasks.md:19`（2.1）同口径。门禁事实核对：`package.json` 的 `check:schemas` = `scripts/check-schema-fixtures.mjs`，该脚本**确实双向断言**——manifest 中每条 case 的 fixture/schema 必须存在（`manifest: missing fixture/schema`），且目录内每个 JSON 必须被 manifest 列入（`fixture not listed in manifest`），另有 message type / event view 覆盖判据；`fixtures/acp/v1/manifest.json` 实存 ✓。因此「新增 fixture 会写到无归属路径」的缺口已闭环 |
## 独立判断（派发要求的 1–4 项）
**1. 本次修改是否引入了新的不一致 —— 一处（DR1-F26，MINOR），其余核对为一致。**
- `verification.md:53` 的 `PV2 / 分支 / WP1、WP2、WP3、WP4、WP6、**TP2**` 与本轮 F20 在 `plan.md:349` 建立的 PV2 集合（WP1、WP2、WP3、WP4、WP6）**不一致**：TP2 在 `plan.md:256`（Work Packages）、`tasks.md:45`（2.8 只标 `[PV1]`）都被排除在 PV2 之外；该行也未体现 `plan.md:349` 声明的 PV2 四个阶段（仅「分支」一行，缺「集成基线（阶段 2）/候选/主分支」）——即 F21 只把 PV1 行拆开，PV2 行留在旧口径。属记录与被记录计划不一致，不产生「不可产出」后果（PV2 命令与阶段口径本身以 plan.md 为准）。
- Coverage Index 的 `checks: [PV1, PV2]` 与 tasks 的 `[PV1]` 标记**无冲突**：我逐行检查了 37 行中所有含 PV2 的行（R1/R4 → task 2.1 = WP1；R13/R18 → 2.4 = WP4；R27 → 2.2 = WP2；R34 → 2.2 = WP2），每行的 PV2 都能落到一个真正声明 PV2 的 WP（WP1/WP2/WP4），**没有任何只含 WP5 或 TP2 的行声称 PV2**；行内同时含 `4.2`（TP2）不构成冲突，因为 `checks` 描述覆盖该需求行的检查集合，而 TP2 自身的派发行只标 `[PV1]` ✓。
- `design.md` 全文不含 PV1/PV2/阶段/红窗口字样（`broker:`/`layers` 只出现在 D5 资产表，且值为 `project_and_preserve`，与 R3 复核结论一致）→ 与阶段拆分无表述冲突 ✓。
- Completion Criteria（`plan.md:388-393`）与阶段制自洽：最终主分支要求 PV1 **阶段 2** + PV2 全绿（`reports/PV1.log`/`reports/PV2.log`），阶段 1 证据明确标注为「仅作开发期证据，不替代阶段 2」✓；Coverage Index 各行的 `evidence: [reports/PV1.log]` 仍可由阶段 2 产出，不构成矛盾。
**2. Execution Waves 层级算式与 Serialization Reason 仍合法；Coverage Index 的 37 行与 tasks 引用一一对应。**
- 逐行验算（上游取声明依赖中最早层级 +1）：WP1=none→W1、WP2=none→W1、TP1=none→W1；WP3=`code:WP2`(1)+1=W2；WP4=`code:WP3`(2)+1=W3、WP5=max(WP1=1, WP3=2)+1=W3；WP6=max(1,1,2,3,3)+1=W4；TP2=max(…,4)+1=W5——与 `plan.md:261-268` 逐行一致。8 行 Serialization Reason 全为 `NOT_APPLICABLE`，是模板允许取值（`NOT_APPLICABLE 或 <码>: <证据路径>`），且每个包都处在其**最早层级**（无「晚于最早层级」的情形），故无需理由码；Runtime Resources 三行 `Exclusive Scheduling=NOT_APPLICABLE` 有依据 ✓。表格本身未被本轮修改触及。
- Coverage Index：R1–R37 连续无重复；逐文件数 specs 标题 = `acp-wire-protocol` 4（1+3）、`local-agent-host` 8（2+6）、`node-link-owner-server` 11（2+9）、`storage-schema-v2-migration` 10（2+8）、`workspace-resolution` 4（1+3），合计 **37** ✓；我逐条比对了 `source.heading` 与实际标题字符串（含反引号与全角括号，抽查 R1/R2/R5/R8/R13/R23/R27/R34 全部**逐字相同**）；37 行引用的 task 集合 = {2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 4.2}，在 `tasks.md` 中全部真实存在且语义对齐 ✓。
**3. `## Contract Changes` 的 F18–F24 处置描述与实际改动 —— 无「声称改过而实际未改」的反向情形；一处断言过宽（并入 F25）。**
- 定位先说清：`plan.md` 的 `## Contract Changes` 只含 **Round 1 / Round 2 / Round 3** 三个块（`plan.md:13-45`），**没有 Round 4 块**；F18–F24 的处置描述实际写在 `verification.md` 的 `## Review Findings`（`:115-121`）、`## Check Plan Changes`（`:59`）与 `## Failures and Retests`（`:157-158`）。按 `templates/plan.md:20-21`（「原/新值、原因、影响分析及证据失效或复用历史写入 verification 的 Check Plan Changes，本节只引用」），这一分工**符合模板**，不作为发现。
- 逐条对照现行文件：F18 ✓（见上表）、F19 的补救动作 ✓ 但其「四份报告现已全部可读」过宽（F25）、F20 的 plan.md 三项 ✓、F21 ✓、F22 ✓、F23 ✓、F24 ✓。另：`plan.md:41-45` 的 Round 3 行声称的改动我逐条回读确认真实落地（阶段制 PV1、WP6 写范围 = 整个 `crates/server/`、`agent-host/tests/` 与 `app/tests/` 两行、tasks 1.5 补摘要、`design-rev-4` 统一 8 处）。**未见「声称已改而实际未改」的实质条目**（F19 属断言过宽而非改动虚报）。
**4. 是否还有被编译/测试/门禁强制却不在任何 WP 写范围内的文件 —— 本轮修改面**未引入新缺口**。**
- 本轮新增的两处写范围恰好闭合了 R1–R4 的两个候选缺口：`fixtures/acp/v1/`（含 manifest，WP1，`plan.md:246`）与 `crates/storage-sqlite/tests/` 整目录（WP4/TP2，`plan.md:277`）。
- 对「强制写目标」再核一遍关键面：① `NewSession { … }` 字面量全库只出现在 `crates/core/src/**`（WP3）与 `crates/storage-sqlite/tests/**`（WP4，已登记）→ 提交形状变更无无主文件 ✓；② 封闭词表门禁的核心镜像判据是 `required_grant`（`scripts/check-command-catalog.mjs:78-108` 解析 `pub fn required_grant(` 的 `"a" | "b" => "grant.x"` 臂，`:254` 做双向集合相等），**不解析 `command_name`/payload 枚举**→ WP2 只加一条 `required_grant` 臂即可让 PV2 通过，WP3 的 core payload 变体不构成 WP2 的门禁缺口 ✓；③ `node-link-protocol` 的依赖方为 `app`、`server`、`identity-auth`（三者分别属 WP6/WP6/WP2）→ 第 13 个 wire 变体的穷尽匹配消费者都在写范围内 ✓；④ `crates/identity-auth` 的硬编码计数（`tests/authorization.rs:267` 的 `request.scopes.len() == 6`）在本 crate 内，属 WP2 写范围 ✓（`:248` 的设备请求 6 项是 pack 展开，不受影响）；⑤ `docs/SESSION_CONTINUITY_DESIGN.md` 与 `docs/DEVELOPMENT_PLAN.md` 实存且都在 WP6 写范围 ✓。未见新的无主强制写目标。
## Findings
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F25 | MINOR | `verification.md:42`（DCR 第 4 行，`result=PASS`/`NEW`，`report_path=openspec/changes/session-resume/reports/dr1-dependency-review-round4.md`）、`verification.md:157`（`## Failures and Retests` 的 F13…F17 行同样引用该路径）、`verification.md:116`（F19 处置句） | 该路径**不可读**：`read` 对仓库相对路径与 change 目录相对路径均返回 ENOENT，全库 `**/*dependency-review*` 只命中 R1/R2/R3 三份；而 `roles/_shared/role-report.md` 明确「NEW/REUSED 的报告路径必须可读，且 ID、任务、阶段、目标版本和实际报告一致」，`workflow-check.md:45` 的 final 审计也「返回已存在的计划报告的路径和 SHA-256」。同时 F19 的处置句「四份报告现已全部可读」与现状不符（这正是上一轮对同一缺陷类别的判定：F19 因 `result=FAIL` 的 NEW 行路径不可读被记为 MINOR） | Round 4 的 PASS 结论（本轮回写动作的授权依据）没有可读的原始报告：后续轮次/final 审计无法独立核对 F18–F24 的原文与判据，只能依赖 verification.md 的转述；本轮派发也据「四份报告都在仓库内」下达，导致 F18–F24 的**原文**不可读（本报告只能引用转述）。不影响任何 WP 的开工就绪、写范围或检查可产出性，且当前结论以本轮（round 5）为准，故不阻断 | 二选一（都很小）：(a) 由 main 从 Round 4 的 run 日志/上下文将 `reports/dr1-dependency-review-round4.md` 原样落盘（与 F19 对 Round 3 的补救同法），并在持久化后保留 `verification.md:42`/`:157` 的引用；(b) 若确实无法恢复，把 `:42` 行的 `report_path` 改成可读的转述记录（如指向本文件 `## Review Findings` 的 F18–F24 行）并显式标注 `evidence_status=INVALID`/说明依据，同时把 `:116` 的「四份报告现已全部可读」改为准确表述。建议在 premerge/final 审计前完成 | 不适用（本轮新发现；与已闭环的 F19 同一类别） |
| DR1-F26 | MINOR | `verification.md:53`（`PV2 / 分支 / WP1、WP2、WP3、WP4、WP6、TP2`）对 `plan.md:349`（PV2 行：阶段 = 分支/集成基线/候选/主分支，工作包 = WP1、WP2、WP3、WP4、WP6）与 `tasks.md:45`（2.8 只标 `[PV1]`） | 本轮 F20 在 plan.md 侧把「PV2 的工作包集合去掉 TP2（与 TP2 只标 [PV1] 对齐）」，但 verification.md 的 `## Checks` PV2 行未同步：仍列 TP2，且仍只有「分支」一个阶段（同表 `:50-52` 的 PV1 行已按 F21 拆为阶段 1/阶段 2 三行）。两处对同一 Check 的工作包集合与阶段口径给出一致的两种读法 | 记录行与计划不一致：按该行执行会把 TP2 纳入 PV2，或让人以为 PV2 只覆盖分支阶段；不影响 plan 的依赖/写范围判定，也不影响 PV1 的阶段制 | 把 `verification.md:53` 改为与 `plan.md:349` 逐字对齐的记录行（工作包集合去掉 TP2，并补「集成基线（阶段 2）/候选/主分支」的对应行，或把阶段列写成与 plan.md 相同的四段式） | 不适用（本轮新发现；由 F20/F21 的修改面产生） |
| DR1-F27 | SUGGESTION | `plan.md:265-268`（W3 WP4 / W3 WP5 / W4 WP6 / W5 TP2 的 Enter Condition）对 `plan.md:264`（W2 行内含术语定义）、`plan.md:257` 模板注释 | 只有 W2 的 Enter Condition 显式写出了「**PV1 阶段 1** 与 PV2 PASS」；W3–W5 一律写「已进入已验收集成基线」，其内涵（上游交付提交 + 必要 Project Verify + 独立 review + main 已接收）只在 W2 行括号里定义一次 | 无矛盾（单处定义 + 多处引用是模板鼓励的写法，且 `templates/plan.md:57-60` 的就绪判据按同一口径），仅降低人工核对时「阶段 1 是否就是该上游的判据」的可读性 | 可在 W3–W5 的 Enter Condition 后追加半句「（同上，含 PV1 阶段 1 与适用的 PV2）」，或在该表下加一行「『已验收集成基线』含义见 W2 行」 | 不适用（本轮新发现，报告级提示） |
| DR1-F28 | SUGGESTION | `plan.md:16`（Round 1 前言引用块）、`plan.md:44`（F16 行末）、`plan.md` Coverage Index 各 `evidence:` 字段 | (a) `plan.md:16` 仍用将来时：「本变更在 tasks 1.5 之后**将把** specs 的基线固定为具体提交/摘要」，而 `verification.md:11-20` 的 6 个摘要已实写、`plan.md:44` 也已写「摘要本身已写入该节」；(b) `plan.md:44` 末仍写「（DR1 Round 4 复核为部分解决，见 DR1-F18 及其处置）」，第五轮已把该残留闭环；(c) Coverage Index 37 行的 `evidence:` 只列 `reports/PV1.log`/`reports/PV2.log`，未引用本轮新增的阶段 1 证据路径 `reports/PV1-<WP>.log`（阶段 2 仍可产出它们所列的路径，故非矛盾，仅证据登记的完整性） | 均为记录措辞/登记完整性层面，不影响门禁、依赖判定或检查可产出性 | 顺手把 (a) 改为完成时或删去该句、(b) 补「DR1 Round 5 复核：已解决」、(c) 视需要在各行的 `evidence` 中补阶段 1 日志（或保持现状并在 Completion Criteria 已写明阶段 1 为开发期证据即可） | 不适用（本轮新发现，报告级提示） |
`SAFE-NOTE`：除上表外，未发现其它与本轮 Scope 相关的具体问题；以下事项经核对**不计为发现**：（i）`## Worktree Handoff` 的 TP2 行仍「待 provisioner 补发」（plan 本身已把它写成开工前置）；（ii）`@design-rev-4` 版本标签（模板明确为「人读标识、不做机械校验」）；（iii）`plan.md` 无 Round 4 的 Contract Changes 块（处置史按模板归 `verification.md` 的 Check Plan Changes / Review Findings）；（iv）D6 尚未在 `scripts/check-command-catalog.mjs:194` 落地（属 WP2 的执行任务，非规划缺陷）；（v）Coverage Index 未引用阶段 1 日志（见 F28(c)）。
## Check Plan Reconciliation
- PV1/PV2 本轮**未执行**（规划审查 + 只读边界，无 shell/测试能力）。本轮核对的是「阶段拆分后，各包声明的检查是否可产出」：
  - **阶段 1 可产出（逐 crate 子集）**：WP1 `-p acp-protocol`、WP2 `-p node-link-protocol -p core -p identity-auth`（`required_grant` 加臂不影响 `cargo test -p core`；identity-auth 的计数断言在其自身 crate 内）、WP3 `-p core`（core 的 `command_name`/`command_kind`/`family` 均为本 crate）、WP4 `-p storage-sqlite`、WP5 `-p agent-host`、WP6 `-p server -p app`（`crates/server/` 与 `crates/app/` 整体在写范围内，含 `src/local_admin/test_support.rs` 的 `NotTouched` 与 `app/tests/support/owner.rs` 的 `ScriptedBackends`）。R3 的 F13/F14（WP2/WP3 曾宣告 workspace 全量 PV1 而不可产出、`NotTouched` 无主）据此已闭环 ✓。
  - **PV2 可产出**：门禁的核心镜像判据是 `required_grant`（不解析 payload 枚举），故 WP2 的「commands.json + schema/fixture + 文档四处 + broker 一条臂 + identity-auth 会员与计数」同包同提交即可绿；WP3/WP4 的漂移门禁两侧（§5↔`ports.rs`、§7↔`migrate.rs`）各自同包提交即可绿；`check:acp` 的 `layers.broker` 已回到 schema 枚举内 ✓；`check:schemas` 的 manifest 双向断言由 WP1 的写范围覆盖 ✓。
  - **阶段 2（集成基线/候选/主分支）在 WP2 合入后到 WP6 合入前预期为红**，已在 `plan.md:304-309` 明写为有主有界的红窗口，并由 `## Dependency Handoffs` 的 Invalidation 列与 `verification.md` 的 `## Check Plan Changes`（`:59`）登记；对应记录行已由 F21 拆分（阶段 1 与阶段 2 分行）✓，MVP 判据「候选阶段必须全绿」保留在 `plan.md:388` ✓。
- 与 Project Verify 无并行：本会话不具备执行能力，没有「待返回结果」需要判定其对结论的影响，因此可按 `roles/reviewer.md` 第 6 条完成静态审查。
- Main E2E 为 `not-applicable`（mode/reason/basis/alternative_checks/downgrade_approval 齐备，C1/C2 在 `tasks.md` 8.1/8.2 以 `[ID]` 声明并有 `## Checks` 行）→ 本轮不涉及 E2E 用例或其配置，不做 test-case 判据表核对。
- **待补证据**：无（本轮不需要执行证据即可完成判断）。**未核对项**：`contractDigest` 取值本身（无引擎调用能力，按派发值绑定）；`## Target` 的 6 个 sha256 是否与当前字节相符（无 shell）；Round 4 报告原文（文件缺失，见 F25）；proposal/specs 的字节级未改动（无 diff 基线，只能一致性核对）。这些都不影响本轮结论。
## Assessment
**结论：PASS**（对应 Target Revision = `sha256:3b208486402023d4d52139b494590756b352b6c908c164f217b1bcd94ab1391d`）。
本轮是复核轮：R4 的 7 项非阻断 finding 我逐项独立回读现行文件核实，**F18、F20（plan.md 侧）、F21、F22、F23、F24 已解决**，F19 所指的 Round 3 报告已落盘且其 DCR 行 `report_path` 可读（该原问题已解决），并且**未发现任何已确认且未解决的 CRITICAL/MAJOR**：
- 阶段制 PV1 已把「workspace 全量」限定为「全部 WP 集成后的集成基线、候选、主分支」，W2 的 Enter Condition 已明写「PV1 阶段 1 与 PV2 PASS」，PV2 的工作包集合与各 WP/TP 自声明逐行一致，Local Checks 已限定「不代替阶段 2」；各包阶段 1 的 crate 子集均可自足产出，候选/主分支的 workspace PV1 由 WP6 收口后可达全绿，Completion Criteria 因此可达（R3 的 F13/F14 已真正闭环，F22 的整目录登记进一步覆盖了提交形状变更的全部字面量落叶点）。
- 新增的写范围（`fixtures/acp/v1/`、`crates/storage-sqlite/tests/`）闭合了本轮修改面可能产生的新缺口；我按依赖与门禁脚本独立复核了「强制写目标是否都有主」，未发现无主文件。
- Execution Waves 的层级算式（W1–W5）与 Serialization Reason 取值合法；Coverage Index 的 37 行 heading 与 5 份 specs 的 37 个标题一一对应、tasks 引用全部实存；`## Contract Changes` 的处置描述与实际改动一致，无「声称改过而实际未改」的实质条目。
**非阻断项（须列出，不阻断）**：
- **DR1-F25（MINOR）**：`reports/dr1-dependency-review-round4.md` 缺失，导致 `verification.md:42` 的 `result=PASS`/`NEW` 行与 `:157` 的引用均指向不可读路径，且 `:116` 的「四份报告现已全部可读」不实；建议在 premerge/final 审计前落盘该报告或改用可读的转述记录（`roles/_shared/role-report.md` 对 NEW 行的报告路径有「必须可读」的硬要求，故**不应长期悬置**）。
- **DR1-F26（MINOR）**：`verification.md:53` 的 PV2 记录行仍是旧口径（含 TP2、仅「分支」阶段），与 `plan.md:349`/`tasks.md:45` 不一致；由 F20/F21 的修改面遗下，应同步。
- **DR1-F27、DR1-F28（SUGGESTION）**：W3–W5 的 Enter Condition 只引用 W2 行定义的术语；`plan.md:16`/`:44` 的记录措辞仍是 Round 4 口径（将来时、未标第五轮已闭环），Coverage Index 的 `evidence` 未列阶段 1 日志。
本报告不更新任何任务状态，也不代表整个变更可归档；R4 的 PASS 已因 plan.md/tasks.md 被修改而失效，本轮针对新 `contractDigest` 的结论即上文的 PASS。DR1-F25/F26 属记录层修复（不改任何行为契约与写范围），其修复后**不需要**再开新轮次即可由主 Agent 在本轮 PASS 上继续；`verification.md` 的 `## Dependency Declaration Review` 第 5 行与 `## Review Findings` 的 F18–F24 复核列由 main 按本轮结果回填。
```yaml
handoff_index: [{task_id: NOT_APPLICABLE, work_package: NOT_APPLICABLE, role: reviewer, phase: plan, stage: plan, round: 5, target_revision: "sha256:3b208486402023d4d52139b494590756b352b6c908c164f217b1bcd94ab1391d", evidence_type: REVIEW, evidence_id: DR1, report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round5.md, result: PASS, evidence_status: NEW, applicability_basis: "针对清掉 DR1-F18–F24 后的当前 contractDigest 做静态规划审查：逐项复核 F18–F24 的处置、核对阶段制 PV1/PV2 的可产出性与 PV2 工作包集合一致性、复核 Waves 层级与 Coverage 37 行、并交叉核对门禁脚本与 Cargo 依赖确认无无主强制写目标；未执行 PV1/PV2、未针对代码 diff，Base Revision 为 NOT_APPLICABLE；Round 4 报告缺失（记 DR1-F25）", source_evidence: NOT_APPLICABLE}]
```
handoff_index（单行，按要求原样给出）：`role: reviewer, phase: plan, stage: plan, round: 5, target_revision: sha256:3b208486…391d, evidence_type: REVIEW, evidence_id: DR1, result: PASS`
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "返回完整报告：Review Context（Inputs 逐项实际取值 + 隔离与限制）、DR1-F18–F24 逐项复核结论与独立依据、派发要求的 4 项独立判断、新发现 DR1-F25/F26（MINOR）与 F27/F28（SUGGESTION）、Check Plan Reconciliation、Assessment=PASS（0×CRITICAL/0×MAJOR，非阻断项已逐条列出）、单行 handoff_index + yaml handoff_index 与残余风险"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [],
  "validationOutput": [
    "DR1-F18=已解决：tasks.md 1.5 已补『把 proposal.md 与 5 份 specs/**/spec.md 的 sha256 摘要写入 verification.md 的 ## Target』完成条件；plan.md:44（F16 行）引用的位置正确，verification.md:11-20 确有 6 行 sha256 且逐文件点名；plan.md:6 与 plan.md:392 同口径",
    "DR1-F19=原问题已解决但同类别在 Round 4 行复发：reports/dr1-dependency-review-round3.md 实存且完整（Shared Report/Review Context/Round 2 复核表/独立判断/Findings/Check Plan Reconciliation/Assessment/handoff_index/acceptance-report），verification.md:41 的 report_path 可读；verification.md:42 的 round 4 PASS 行与 :157 引用的 reports/dr1-dependency-review-round4.md 不存在（两处直读 ENOENT，全库仅命中 3 份报告）→ 记 DR1-F25，并证伪 verification.md:116『四份报告现已全部可读』",
    "DR1-F20=plan.md 侧已解决：plan.md:348 PV1 阶段列含『集成基线（阶段 2，全部 WP 集成后、候选前）』；plan.md:264 W2 Enter Condition 明写『PV1 阶段 1 与 PV2 PASS』；plan.md:349 PV2 工作包 = WP1/WP2/WP3/WP4/WP6，与 Work Packages 逐行（WP5 只 PV1、TP1 N/A、TP2 只 PV1）及 tasks 2.5/2.8 的 [PV1] 一致；plan.md:341 Local Checks 已补『不代替 PV1 阶段 2 与 PV2』；但 verification.md:53 的 PV2 行仍是旧口径（含 TP2、仅分支阶段）→ 记 DR1-F26",
    "DR1-F21=已解决：verification.md:50-52 已拆三行（分支阶段 1 按 crate 子集 + 证据 reports/PV1-<WP>.log；集成基线阶段 2；候选/主分支阶段 2，证据 reports/PV1.log），与 plan.md:348 口径逐项一致（crate 列表同样委派 Work Packages 的 Verification 列），差异仅为缩写、无矛盾",
    "DR1-F22=已解决：plan.md:277 登记整目录 crates/storage-sqlite/tests/（Writers=WP4,TP2；Merge Owner=TP2；Order=WP4 → TP2；Re-verify=TP2: PV1）并点名 migration.rs/commit.rs/retention.rs/enum_coverage.rs；独立核对 OwnedCommit/NewSession/SessionUpdate 字面量分布：全库 NewSession 字面量只在 crates/core/src/**（WP3）与该目录（WP4），该目录内 9 个文件全部落在此行覆盖内",
    "DR1-F23=已解决：plan.md:309 红窗口段新增 husky pre-commit → scripts/pre-commit.mjs 在涉及 Rust 的提交上跑 workspace 全量 clippy、按 AGENTS.md §8 可用 --no-verify 的说明，tasks.md:37 同步；核对 scripts/pre-commit.mjs 确在 rustTouched 时追加 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 且 npm run check 无条件运行，描述准确",
    "DR1-F24=已解决：plan.md:246 WP1 写范围含 fixtures/acp/v1/（含 manifest.json），Inputs/Outputs 与 tasks.md:19 写明『新增 fixture 必须登记 manifest，或只用内联字节』；核对 package.json 的 check:schemas = scripts/check-schema-fixtures.mjs 确实双向断言（manifest 条目必须存在 + 目录内 JSON 必须被 manifest 列入），fixtures/acp/v1/manifest.json 实存",
    "独立判断 1：新引入的不一致只有 verification.md:53 的 PV2 行（DR1-F26）；Coverage Index 所有含 [PV1, PV2] 的行（R1/R4→2.1、R13/R18→2.4、R27/R34→2.2）都能落到真正声明 PV2 的 WP，与 tasks 的 [PV1] 标记无冲突；design.md 全文无 PV1/PV2/阶段字样；Completion Criteria（plan.md:388-393）与阶段制自洽",
    "独立判断 2：Waves 逐行验算 W1(WP1,WP2,TP1)/W2(WP3)/W3(WP4,WP5)/W4(WP6)/W5(TP2) 与 plan.md:261-268 一致，8 行 Serialization Reason=NOT_APPLICABLE 合法（均在最早层级）；Coverage Index R1–R37 连续、逐文件标题计数 4+8+11+10+4=37 与 specs 一一对应、抽查 8 行 heading 逐字相同、tasks 引用集合 {2.1…2.6,4.2} 全部实存",
    "独立判断 3：plan.md 的 ## Contract Changes 只含 Round 1–3 块，F18–F24 处置写在 verification.md 的 Review Findings/Check Plan Changes/Failures and Retests——按 templates/plan.md:20-21 符合模板分工；逐条对照现行文件：F18/F20(plan 侧)/F21/F22/F23/F24 描述与实际一致，F19 措辞过宽（F25）；未见『声称已改而实际未改』的实质条目",
    "独立判断 4：本轮修改面未引入无主强制写目标——新增 fixtures/acp/v1/（WP1）与 crates/storage-sqlite/tests/（WP4/TP2）闭合了 R1–R4 的两个候选缺口；封闭词表门禁的核心镜像判据是 required_grant（scripts/check-command-catalog.mjs:78-108,254，不解析 payload 枚举）故 WP2 一条臂即可过 PV2；node-link-protocol 的依赖方 app/server/identity-auth 均在 WP2/WP6 写范围内；identity-auth 的 scopes.len()==6 硬编码计数在本 crate（WP2）；docs/SESSION_CONTINUITY_DESIGN.md 与 docs/DEVELOPMENT_PLAN.md 实存且属 WP6"
  ],
  "residualRisks": [
    "无法重算 contractDigest（本会话无 OpenSpec 引擎调用能力），按派发值 sha256:3b208486…391d 绑定",
    "change 目录未跟踪，无可 diff 基线：proposal/specs 的字节级未改动只能做一致性核对；verification.md:11-20 的 6 个 sha256 摘要无法在本会话内验算（无 shell），摘要与当前文件的相符性只能在有 shell 的环境复核",
    "Round 4 的报告缺失（DR1-F25）：F18–F24 的原文不可读，本报告的复核依据是 verification.md:115-121 的转述行与派发摘要；若转述与原文有偏差，需以恢复后的报告为准",
    "本轮未执行 PV1/PV2、未运行任何命令（规划审查 + 只读边界）；阶段制 PV1 的可产出性是静态推断（依据源码依赖与门禁脚本），必须由各 WP 实跑阶段 1、候选/主分支实跑阶段 2 才能闭环",
    "红窗口内以 --no-verify 提交属计划明示的偏离：它依赖执行者自觉补跑 PV1 阶段 1；若某 WP 借此跳过本 crate 的阶段 1，plan 本身无法机械发现（属执行期风险，需 review/validator 核对日志）"
  ],
  "noStagedFiles": true,
  "diffSummary": "无 diff：本轮为只读规划审查（watchdog_diff 仅显示 openspec/changes/session-resume/** 未跟踪清单，无已跟踪文件的 staged/unstaged 改动），未修改、未暂存任何仓库文件，未切换分支、未提交",
  "reviewFindings": [
    "no blockers: 本轮未发现 CRITICAL/MAJOR；DR1-F18、F20（plan.md 侧）、F21、F22、F23、F24 复核为已解决，F19 所指的 Round 3 报告已落盘且其 DCR 行路径可读",
    "non-blocking: openspec/changes/session-resume/reports/dr1-dependency-review-round4.md 缺失 + verification.md:42/:157 - 该 PASS/NEW 行的 report_path 不可读、verification.md:116 的『四份报告现已全部可读』不实（违反 role-report『NEW/REUSED 报告路径必须可读』）；建议 premerge/final 前落盘该报告或改用可读的转述记录（DR1-F25, MINOR）",
    "non-blocking: verification.md:53 vs plan.md:349 + tasks.md:45 - PV2 记录行仍列 TP2 且只有『分支』阶段，与已修订的 PV2 集合（去掉 TP2）和四段阶段口径不一致（DR1-F26, MINOR）",
    "non-blocking: plan.md:265-268 - W3–W5 的 Enter Condition 未复述『PV1 阶段 1』，仅引用 W2 行定义的『已验收集成基线』（DR1-F27, SUGGESTION）",
    "non-blocking: plan.md:16 + plan.md:44 + Coverage Index 的 evidence 字段 - Round 4 口径的措辞未随第五轮更新（将来时表述、未标已闭环），阶段 1 证据路径未登记进覆盖行（DR1-F28, SUGGESTION）"
  ],
  "manualNotes": "本轮为 DR1 第五轮（recheck，plan 类型），我是第五个新实例、只读、无 write 工具，未参与前四轮或任何实现。核心结论：**PASS**——R4 的 7 项非阻断 finding 中 6 项已真正闭环、1 项（F19）的原对象已闭环但在 Round 4 行复发为一个记录层缺陷（F25）；此外 F20 的修复只落在 plan.md，verification.md 的 PV2 记录行未同步（F26）。两者都不改变任何行为契约、写范围或检查的可产出性，也不影响各 WP 的开工就绪判据，故按 MINOR 报（与 R4 对 F19 的定级一致：同一缺陷类别上一轮被记为 MINOR；且当前结论以 round 5 为准，round 4 行不再是当前结论）。特别提请 main 注意两点：(1) F25 必须修——`roles/_shared/role-report.md` 对 NEW 行的报告路径有『必须可读』的硬要求，final 审计会返回『已存在的计划报告的路径和 SHA-256』，一条指向不存在文件的 PASS 行无法通过独立核对；修法二选一（落盘 Round 4 报告 / 改写该行并标注依据），不必为此再开新检视轮。(2) 本报告需要由 main 持久化到 openspec/changes/session-resume/reports/dr1-dependency-review-round5.md，并在 verification.md 的 ## Dependency Declaration Review 第 5 行填入 Plan Revision = sha256:3b208486…391d 与 result=PASS、在 ## Review Findings 的 F18–F24 复核列写『Round 5：F18/F20(plan)/F21/F22/F23/F24 已解决；F19 原对象已解决、Round 4 行缺报告转 F25』并新增 F25–F28 行、在 ## Failures and Retests 的 F18…F24 行结果写『已修（F25/F26 待记录层收尾）』。commandsRun 为空是本轮无 shell/测试执行能力所致（规划审查边界），非跳过检查。另注：派发说明称『四份报告现在都在仓库内』，实测只有三份——这也是 F25 的依据之一，且使我无法读取 F18–F24 的原文（只能引用 verification.md 的转述行与派发摘要）。"
}
```
**Residual risks（摘要）**：`contractDigest` 无法重算；`## Target` 的 6 个 sha256 无法在本会话验算；change 目录无可 diff 基线，proposal/specs 字节级未变只能一致性核对；Round 4 报告缺失使 F18–F24 原文不可读；阶段制 PV1 的可产出性为静态推断，须由各 WP 实跑阶段 1、候选/主分支实跑阶段 2 才能闭环；红窗口内 `--no-verify` 依赖执行者自觉补跑阶段 1（执行期风险）。
