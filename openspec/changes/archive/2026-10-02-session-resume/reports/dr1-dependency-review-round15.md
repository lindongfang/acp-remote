# DR1 Round 15 — 依赖声明审查报告（Review Type: plan）

> 持久化说明：本报告由第 15 个独立 reviewer 实例（只读工具集，无写权限）以全文返回、由主 Agent 原样落盘。run `0e3dc534-69e4-4ec2-8b1e-16e5fac189b2`。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 15）"
work_package: "NOT_APPLICABLE"
role: reviewer
phase: plan
agent_context: >
  第 15 个全新独立 reviewer 实例；未参与前 14 轮，也未参与任何 WP 的实现、修复或实现讨论。
  隔离方式由调度者设置（要求 fork_turns="none" / 等效隔离）；本实例只声明收到的输入与自身限制。
target_revision: "sha256:1154ae4b6cb0fb85778ded529db927ebf014d0a1de671af006e6dfbd3747d4f5"
scope: >
  plan.md 的依赖类型、波次、资源互斥、写入归属；以及
  plan.md ↔ tasks.md ↔ design.md ↔ proposal.md ↔ specs/** 的一致性。
changes: "无（只读；未修改任何文件、未运行 cargo/npm、未提交）"
checks: "见 Assessment 的逐条核对（F57–F61 闭环 + 本轮回写文本逐字复扫）"
issues: "0×CRITICAL / 0×MAJOR / 2×MINOR（DR1-F62、F63）/ 1×SUGGESTION（DR1-F64）"
result: PASS
evidence_paths:
  - openspec/changes/session-resume/plan.md
  - openspec/changes/session-resume/tasks.md
  - openspec/changes/session-resume/design.md
  - openspec/changes/session-resume/verification.md
  - openspec/changes/session-resume/reports/dr1-dependency-review-round14.md
  - D:/Project/acp-remote-wt/session-resume-du1/（集成基线工作树，只读取证）
  - node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs（门禁判据，只读）
resource_cleanup: "NOT_APPLICABLE（本轮未分配任何资源）"
```

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / **15**（沿用线程，续用编号 F62–F64） |
| Review Type / Stage | `plan` / 计划与依赖声明阶段 |
| Work Package | `NOT_APPLICABLE` |
| Base / Target Revision | `NOT_APPLICABLE` / `sha256:1154ae4b6cb0fb85778ded529db927ebf014d0a1de671af006e6dfbd3747d4f5` |
| Repository | `D:/Project/acp-remote`（规划根，权威） |
| 读取的规则与契约 | `AGENTS.md`（§4 依赖规则、§8 工作流、§10 文档维护、§12 工具链）、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`openspec/changes/session-resume/{proposal,design,plan,tasks,verification}.md`、`reports/dr1-dependency-review-round14.md`、门禁实现 `node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:93-106,280-305,380-390` |
| 实际检查范围 | ① Round 14 的 F57–F61 逐条闭环核对（是否真落地、是否引入新错）；② 主 Agent 本轮改过的三处文本逐字复扫：`plan.md:120/124/328/375`、`tasks.md:29`、`design.md:154-155`；③ plan↔tasks 同名义务的标签与正文逐字比对；④ 新增 SFO 行的列数/Merge Owner/Order/Re-verify 与既有行及门禁 `ownershipProblem` 判据的一致性；⑤ D5 补齐是否只登记资产级范围、与 plan 逐 WP 区域是否重复或冲突；⑥ Round 14 段新长句的每个提交号/结论对照实际记录核实；⑦ Round 1–13 已登记处置抽查 5 处 + 基线事实取证 |
| 使用的证据 | 集成基线工作树 `D:/Project/acp-remote-wt/session-resume-du1` 的**实际文件内容**：`docs/NODE_LINK_PROTOCOL.md:652-664`（`:662` 确在 §12.7、§12.8 起于 `:664`）、`docs/CORE_PORTS_AND_STORAGE.md:1479`（`user_version = 6`）与 `:24`（文件头 `版本：0.15`）、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs:17/81/202/892/931`（`--dump-request-params`）、`crates/server/tests/` 目录清单（恰 5 个 `.rs`，与新 SFO 行所列逐字一致）；`reports/cr5-review-round2.md:16/21/133`（`target_revision=1376e1b5c5bb…`、result PASS）；`verification.md:46/47/50/168/182`（交付、复核、合入与 worktree 重定向记录） |
| 限制 | 本实例**无 shell 权限**，未执行 `npx openspec-agentic workflow check --stage plan --json` 复核 `contractDigest`；摘要绑定依赖调度者给定取值（并与 `verification.md` 的 DCR 第 15 行所记 `sha256:1154ae4b…` 一致）。未判代码质量、未判测试充分性、未判需求是否实现；对 `verification.md` 的自述一律以 plan/tasks/design/specs 实际文本与基线文件为准 |

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F62 | MINOR | `plan.md:120`（`## Contract Changes` 的 Round 14 表「落点」列） | 该行仍写「plan WP6 行「额外义务 ④」+ tasks 2.6」。本轮回写已把 `plan.md:328` 与 `tasks.md:29` 的义务标签全部改为稳定 finding ID，全库检索 `plan/tasks/design/proposal` 中**已不存在任何「额外义务 ④」**（`grep 义务\s*[①-⑥]` 在契约文件中的唯一命中就是本行自身） | 契约内出现指向不存在对象的悬空引用：按「额外义务 ④」检索的复核者/派发提示会落空，而这恰是 F57 要消除的失效模式 | 把该格改为「plan WP6 行「额外义务（CR3-F1 · `uncertain` 终态）」+ tasks 2.6 同名义务」；同类悬空引用另见 `verification.md:100`/`:228`（「额外义务 ④（CR3-F1）」），不进摘要，可在下一轮回写 verification 时一并订正 | 待下次触碰 `plan.md` 时（**建议与 F63 同批**） |
| DR1-F63 | MINOR | `plan.md:328`（WP6 行 Inputs/Outputs 单元）对照 `tasks.md:29`；旁证 `verification.md:93`（Check Plan Changes 的 F57 条） | `tasks.md:29` 五个义务标签已全部改为 `（DR1 Round 9 · port_error_code 新臂）`、`（DR1-F42 · 五个 load_recovery 替身）`、`（CR3-F1 · uncertain 终态）`、`（DR1-F41 · 投影与 settle_session_resume）`、`（DR1-F51 · docs/NODE_LINK_PROTOCOL.md 两处）`；但 `plan.md:328` 仍留一个**裸序号**「；② 因 WP3 新增必需 trait 方法 `SessionStore::load_recovery`…」且**没有 `DR1-F42` 标签**。`verification.md:93` 却把处置写成「`tasks.md` 2.6 与 `plan.md` WP6 行的义务标签**全部**改用稳定 finding ID（弃用 ①②③④ 序号）」——与实际文本不符。另：两处义务顺序相反（plan：F41 → CR3-F1；tasks：CR3-F1 → F41） | F57 **只闭环了一半**：以 finding ID 检索义务的实现者只在 `tasks.md` 命中 F42，在 `plan.md` 只会看到一个无标签的「②」；而已落盘的证据（`reports/merge-u1-integrate-wp5.md:277/332/396`，按「plan.md:328 额外义务 ①/②」引用）继续依赖这套序号。不影响义务正文与归属事实，非阻断 | 把 `plan.md:328` 的「；② 因 WP3…」改为「**额外义务（DR1-F42 · 五个 `load_recovery` 替身）**：因 WP3…」，并顺手把 plan 的 CR3-F1/F41 顺序调成与 tasks 一致；或把 `verification.md:93` 的「全部」改为「tasks.md 2.6 已全部、plan WP6 行尚余 1 处序号」 | 待下次触碰 `plan.md` 时（**建议与 F62 同批**） |
| DR1-F64 | SUGGESTION | `plan.md:375`（新增 SFO 行 File 列） | 该行 File 写成 `` `crates/server/tests/` ``（带反引号），同表其余 22 行均不带反引号（如 `crates/app/tests/`、`crates/agent-host/tests/`）。门禁的匹配是**裸字符串**比较：`workflow-check.mjs:106` `scopeOverlap = (a,b) => a === b || a.startsWith(b+'/') || …`，`readTable(...).pick` 只做 `trim()`（`:99`），不去反引号 | 当前**无实际影响**（门禁本来就检测不到 WP6↔TP2 的重叠：TP2 写范围 `crates/**/tests/ 的新增用例文件` 与 `crates/server/` 不构成前缀重叠，故该行对门禁是惰性登记，与 `crates/app/tests/` 等同类）；但若将来该对被检出，带反引号的 File 值永远匹配不上，登记会「看起来在、实际不生效」 | 去掉反引号，与同表其它行保持一致 | 备选，不阻断 |

---

## Assessment

### A — Round 14 五项 MINOR 的闭环判定（逐条）

| ID | 判定 | 依据（实读） |
| --- | --- | --- |
| **DR1-F57**（义务编号缺③、④重号） | **部分解决** | ✅ `tasks.md:29` 已彻底弃用 ①②③④：五个义务标签依次为 `（DR1 Round 9 · port_error_code 新臂）`、`（DR1-F42 · 五个 load_recovery 替身）`、`（CR3-F1 · uncertain 终态）`、`（DR1-F41 · 投影与 settle_session_resume）`、`（DR1-F51 · docs/NODE_LINK_PROTOCOL.md 两处）`，无重号、无缺号。❌ 但 `plan.md:328` 仍留一个裸「②」且无 `DR1-F42` 标签，两处同名义务**未做到逐字一致** ⇒ 残留记 **DR1-F63**。另 `plan.md:120` 出现悬空的「额外义务 ④」⇒ **DR1-F62** |
| **DR1-F58**（WP6 写范围把 `:662` 误标 §10） | **已解决** | `plan.md:328` 写范围现为「`docs/NODE_LINK_PROTOCOL.md`（仅 §12.7 的 `:662` 收窄句与 §12.7 示例注记，DR1-F51）」，与 SFO 行 `plan.md:371`（「WP6 只补两处：:662 收窄句加 `session.resume`、§12.7 示例加「可见版本为 2」的非规范注记」）和 `tasks.md:29` 的 F51 义务语义一致。事实复核：集成基线 `docs/NODE_LINK_PROTOCOL.md:662` 确为「本切片的 Owner 只为 `session.list` 与 `session.create` 投影 wire 结果…」的收窄句，落在 §12.7 区间内（§12.8 标题在 `:664`）；§10 仍是 WP2 的 grant 表区域，未被 WP6 写范围触及 ⇒ 原撞车风险已消除 |
| **DR1-F59**（design D5 资产表落后） | **已解决** | `design.md:154`（`docs/CORE_PORTS_AND_STORAGE.md` 行）已补 §3.1/§3.3/§3.6/§4/§5.2、§7 标题与 §7.2、§9 判据 1/28 的版本链、§11.3 的「过新」取值、文件头版本记录；与 plan 的 WP3 写范围（§2/§3.1/§3.3/§3.6/§4/§5.1/§5.2）+ WP4 写范围（§7 标题/§7.2/§7.3/§9 判据 1/28/§11.3/文件头）**并集逐项相等**，无遗漏、无越权。`design.md:155`（`docs/NODE_LINK_PROTOCOL.md` 行）已补 WP6 的两处（`:662` 收窄句 + §12.7 示例「可见版本为 2」非规范注记），与 WP2 的 §10/§12.5/§12.7/§15 并集相等。该行末尾新增「逐 WP 的具体区域以 `plan.md` 的 `## Shared File Ownership` 为准，本表只登记资产级范围」——措辞用的是「本表」，作用域覆盖两行，**只做资产级登记、与 SFO 的逐 WP 区域不重复也不冲突**（SFO 行仍带区域注记且更细） |
| **DR1-F60**（Round 14 段仍是过期未来时） | **已解决，且新句无不实陈述** | `plan.md:124` 现写：「Round 14 已判PASS@`sha256:b7650f00…`，报告 `reports/dr1-dependency-review-round14.md`」＋「CR5-F1 的后续已完成：WP5 修复轮交付 `1376e1b`，CR5 Round 2 复核 PASS（`reports/cr5-review-round2.md`），并已入集成基线 `5ab7e9d`」。逐项核实：① Round 14 报告可读且 `target_revision` = `sha256:b7650f00…`（`reports/dr1-dependency-review-round14.md` front-matter 与结论段）；② `reports/cr5-review-round2.md:16/21/133` = `target_revision 1376e1b5c5bbd66…`、result **PASS（0×CRITICAL/0×MAJOR）**，与 `verification.md:47`（CR5 复核轮 PASS/NEW）一致；③ `verification.md:50` 记 U1 集成基线 `5ab7e9d…`（`--no-ff`，parents `b486c53` + `1376e1b`），`verification.md:182` 记 WP5 = **merged**；④ 基线代码侧抽查：`acpr-fake-acp-agent.rs:17/81/202` 确有 `--dump-request-params` 并列选项。**未发现任何不实陈述** |
| **DR1-F61**（SFO 缺 `crates/server/tests/` 行） | **已解决** | `plan.md:375` 已补 `crates/server/tests/ \| WP6, TP2 \| TP2 \| WP6 → TP2 \| TP2: PV1 \| DR1-F61 补登：…`。列数 6（与全表一致，含 Region Note）；Merge Owner/Order/Re-verify 口径与 `crates/app/tests/`（同为 WP6,TP2）、`crates/agent-host/tests/`、`crates/storage-sqlite/tests/`、`crates/node-link-protocol/tests/` 四行**完全同型**；按门禁判据 `workflow-check.mjs:295-302` 复核：`Merge Owner=TP2` 非 none、`Merge Order` 分词 `{WP6, TP2}` 覆盖全部 writers、`Re-verify` 的 `PV1` ∈ Coverage Index 的 checkIds ⇒ 三项判据均满足。所列 5 个既有测试文件与基线 `crates/server/tests/` 的实际清单**逐字一致**；区域说明「两包分处 W4/W5、不并发，故不构成同批重叠」与 Execution Waves（W4=WP6、W5=TP2）一致。唯一瑕疵是 File 列的反引号（**DR1-F64**，SUGGESTION，不影响门禁判定） |

### B — 本轮回写文本的逐字复扫（重点）

1. **义务正文有没有被改坏？——没有。** 五项义务在 `plan.md:328` 与 `tasks.md:29` 两侧的关键约束逐条对齐：
   - `port_error_code` 新臂：两侧同为「把 `UnavailableKind::BackendUnsupported` 映射为 `nodelink.command.unsupported`」（错误码未写错、未新增码）；
   - `load_recovery` 替身：plan「本 crate 内的**四个**替身（`NotTouched`/`FixedStore`/`CommandStore`/`SliceStore`）与 `crates/app/tests/support/owner.rs` 的 `FlakySessionStore`」＝ tasks「**五个**替身」且成员逐个点名一致（口径不同但等价，非矛盾）；
   - F41 投影与 settle：两侧同为「用与 `session_create_result` **同源**的映射投影 `SessionResumeResult`（`remoteSessionRef.exportId` 只有适配层有）→ core 恢复可交互后调用 `settle_session_resume` 落终态 → 投影失败按 `session.create` 既有模式结 `uncertain`」，条件无丢失；
   - CR3-F1：两侧同为「`create_session` 在**会话行已提交之后**失败（两列提交遇 `StorageFull`/`IoError`）时**不得**结 `failed`，结 **`uncertain`」，触发条件与终态取值均未被削弱；
   - F51 两处文档：plan 写范围与 tasks 义务、SFO 区域注记三处指向同两个位置。
   - `tasks.md:29` 尾部未丢：受控路径端到端用例、`docs/SESSION_CONTINUITY_DESIGN.md` 状态注记（含 CR1-F2）、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md`、分支 PV1 命令 `cargo test -p server -p app` + `npm run check` 全部在位。
2. **同名义务是否逐字一致？——标签层未一致（F63），正文层一致。** 顺序：plan 为 Round9→(②)→F41→CR3-F1，tasks 为 Round9→F42→CR3-F1→F41→F51；tasks 多一条 F51 义务（plan 侧由写范围承载）。这些差异不产生二义（plan 内 `②` 唯一、tasks 内标签唯一），但使「按 finding ID 检索义务」只在 tasks 命中。
3. **新增 SFO 行的口径一致性？——与既有行同型，门禁判据满足**（见上 F61 条）。与通用行 `crates/**/tests/ 的新增用例文件 | TP2` 不冲突：通用行只管**新增**文件，新行管该目录整体的 WP6↔TP2 交叠，与 `crates/app/tests/` 的处理方式相同。
4. **D5 补齐是否越界或重复？——没有。** 只登记资产级区域，并显式把逐 WP 区域让给 SFO；两行的并集与 plan 写范围并集逐项相等（见 F59 条）。
5. **Round 14 段新长句是否引入不实陈述？——没有**（四项提交号/结论逐条核实通过，见 F60 条）。
6. **其它回归面抽查（历史处置抽检，5 处全部仍在）**：① `docs/CORE_PORTS_AND_STORAGE.md:1479` = `user_version = 6`、`:24` 文件头 `版本：0.15` 条目覆盖 §3.3/§3.6/§4/§9/§11.3（CR4-F1、F52/F55 在位）；② `--dump-request-params` 在基线在位（CR5-F1）；③ `docs/NODE_LINK_PROTOCOL.md:662` 归属仍只属 WP6（F51/F56）；④ `plan.md:371` 的 `docs/NODE_LINK_PROTOCOL.md` 行 Writers=WP2,WP6 / Owner=WP6 / Order=WP2→WP6 / Re-verify=WP6: PV2 未被本轮回写触动；⑤ 波次表、Dependency Handoffs、红窗口段（收口责任人 WP4/WP5/WP6）、Runtime Resources、PV1/PV2 阶段制与 Completion Criteria 本轮**未改动**，仍与 Round 14 判定一致。

### C — WP6 派发条件复评（与 Round 14 相同结论，且前置更少一条）

- 依赖声明属实性未变：`code:WP1…WP5` 均真需要上游代码；W4 与上游 W1–W3 不同批，无同批互斥；`code:WP1` 仍属经 `agent-host` 的传递依赖（弱一档但非虚报，本轮未回写，仍按 Round 14 建议留在派发提示里）。
- 写范围覆盖仍完整：`core_payload()` 早退臂、`port_error_code` 新臂、5 个 `SessionStore` 替身、F41 投影与 settle、CR3-F1 终态、两处协议文档、三份交付级文档，全部落在 `crates/server/` + `crates/app/` 两个整 crate 与相应文档写范围内。
- **Round 14 遗留的 worktree 前置已完成**：`verification.md:168` 记 WP6 worktree 已由 provisioner 第 3 次重定向到 `5ab7e9d…`（含 WP1–WP5），main 已独立核实 HEAD/工作区干净。故 WP6 的实际前置只剩：① 本轮计划门禁 PASS；② `verification.md` 的 DCR 表第 15 行由「待返回」回填为 PASS（本报告落盘后由主 Agent 完成）。
- 两条挂起项（TP 存废、validator 的能力宣告假设）仍**不**阻塞 W4：TP 存废硬阻塞 W5/TP2，validator 假设只影响 7.1。

---

## 结论 **PASS**（0×CRITICAL / 0×MAJOR）。结论绑定 `sha256:1154ae4b6cb0fb85778ded529db927ebf014d0a1de671af006e6dfbd3747d4f5`。

- Round 14 的 5 项 MINOR：**F58、F59、F60、F61 已解决**（F60 的新句经四路交叉核实无不实陈述；F61 的门禁判据三项全部满足）；**F57 部分解决**，残留为 `plan.md:328` 的一个裸序号「②」与缺 `DR1-F42` 标签（F63），并牵出 `plan.md:120` 的悬空引用「额外义务 ④」（F62）。
- 本轮新发现 2×MINOR + 1×SUGGESTION，全部为**记录/措辞层**：不改依赖声明、不改写入归属事实、不改任何行为契约（`requirementsDigest` 保持 `sha256:53943270…`，与 Round 14 相同）。
- **是否阻塞 WP6 派发：不阻塞。** 两项残留若要修，只需改 `plan.md:120` 与 `plan.md:328` 两处（建议同批），属**下一轮**独立复核的范围，不应为此再压一轮才派 WP6；派发提示里请以 **finding ID**（`CR3-F1` / `DR1-F41` / `DR1-F42` / `DR1-F51`）而不是序号引用义务，以避开当前 plan/tasks 的标签差异。
- 派发前请主 Agent 完成的记录动作（与本结论无关但必要）：把 `verification.md` 的 `## Dependency Declaration Review` 第 15 行由「待返回」回填为 PASS 并指向本报告路径。

```yaml
handoff_index:
  - task_id: "NOT_APPLICABLE（规划门禁，Round 15）"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 15
    stage: plan
    target_revision: "sha256:1154ae4b6cb0fb85778ded529db927ebf014d0a1de671af006e6dfbd3747d4f5"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round15.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >
      Review Type=plan，stage=plan，目标为规划契约摘要（非提交 SHA）。核对对象为
      openspec/changes/session-resume/{plan,tasks,design,proposal,verification}.md 与
      reports/dr1-dependency-review-round14.md 的实际文本，外加集成基线工作树
      D:/Project/acp-remote-wt/session-resume-du1 的只读取证（NODE_LINK_PROTOCOL.md:652-664、
      CORE_PORTS_AND_STORAGE.md:24/1479、acpr-fake-acp-agent.rs 的 --dump-request-params、
      crates/server/tests/ 的 5 个文件清单）、reports/cr5-review-round2.md 的 target/result，
      以及门禁实现 node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs 的
      readTable/scopeOverlap/ownershipProblem 判据。
      Range of checks：① F57–F61 逐条闭环核对；② 本轮回写文本（plan.md:120/124/328/375、
      tasks.md:29、design.md:154-155）逐字复扫与义务正文完整性；③ plan↔tasks 同名义务的
      标签/正文一致性；④ 新增 SFO 行的列数、Merge Owner/Order/Re-verify 与门禁判据；
      ⑤ D5 资产级范围与 plan 写范围并集比对；⑥ Round 14 段新句的提交号与结论核实；
      ⑦ Round 1–13 处置抽查 5 处 + WP6 派发条件复评。
      限制：本实例无 shell，未执行 workflow check 复核 contractDigest（依赖调度者给定取值，
      并与 verification.md DCR 第 15 行一致）；未判代码质量/测试充分性/需求实现。
      历史轮次 Round 1–14 保留，本轮为最大 round 的当前结论。
    source_evidence: NOT_APPLICABLE
```
