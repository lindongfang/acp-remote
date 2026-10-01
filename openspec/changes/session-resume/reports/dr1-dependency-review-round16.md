# DR1 Round 16 — 依赖声明审查报告（Review Type: plan）

> 持久化说明：本报告由第 16 个独立 reviewer 实例（只读工具集：read / grep / find / ls / watchdog_diff / contact_supervisor，**无 shell、无写权限**）以全文返回、由主 Agent 原样落盘。run `1b8da3a7`（首次运行基础设施中断后由 `57423f65` 恢复完成）。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 16）"
work_package: "NOT_APPLICABLE"
role: reviewer
phase: plan
agent_context: >
  第 16 个全新独立 reviewer 实例；未参与前 15 轮，也未参与任何工作包（W1–W6/TP1/TP2/U1）的实现、
  修复或实现讨论。隔离方式由调度者设置（要求 fork_turns="none" / 等效隔离）；
  本实例只声明收到的输入与自身限制，不声称能自行证明宿主未注入其他上下文。
target_revision: "sha256:ce6bb8c7c5d6b92172e8a2b5f2428b416b305fd47488af557c9d923e1aad1ef3"
scope: >
  plan.md 的依赖类型、波次、资源互斥与写入归属；plan.md ↔ tasks.md ↔ design.md ↔ proposal.md ↔ specs/**
  的一致性；Round 15 之后主 Agent 对 tasks.md / dispatch-queue.jsonl / verification.md 的改动是否诚实、
  是否夹带文字变更；以及 tasks 6.6 之前的 premerge 门禁可达性。
changes: "无（只读；未修改任何文件、未提交、未执行 cargo/npm）"
checks: >
  ① 逐字复读 tasks.md / plan.md / design.md / verification.md / dispatch-queue.jsonl / openspec/agentic.yaml；
  ② 复读门禁实现 node_modules/@dongfanglin/openspec-agentic/src/{workflow-check.mjs, workflow-contract.mjs,
  dispatch.mjs} 的相关判据；③ 复读 reports/{dr1-dependency-review-round15.md, merge-u1-candidate.md,
  cr-c1-candidate-review.md}；④ reports/ 目录清单核对证据文件是否真实存在。
issues: "0×CRITICAL / 0×MAJOR / 6×MINOR（F65–F69、F71）/ 1×SUGGESTION（F70）"
result: PASS
resource_cleanup: "NOT_APPLICABLE（本轮未分配任何资源）"
```

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / **16**（沿用线程，续用编号 F65–F71） |
| Review Type / Stage | `plan` / 计划与依赖声明阶段 |
| Base / Target Revision | `NOT_APPLICABLE` / `sha256:ce6bb8c7c5d6b92172e8a2b5f2428b416b305fd47488af557c9d923e1aad1ef3`（由调度者给定） |
| 对照基线 | `requirementsDigest` = `sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（与 Round 14/15 相同） |
| Repository | `D:/Project/acp-remote`（规划根） |
| **限制（重要）** | ① 本实例**无 shell**，**未能**执行 `workflow check --stage plan --json` 实测 `contractDigest`，也**未能**重算 `agentic-premerge` 块内证据文件的 sha256。本轮结论绑定调度者给定的 `ce6bb8c7…`；**若主 Agent 实测值不同，应以实测值为准并重开一轮**。② 变更目录未进入 git，**无 diff 基线**，因此「Round 15 之后改了哪些字」只能靠现读文本 + 门禁机制推断。③ 未判代码质量、测试充分性、需求实现。 |

---

## 一、第 2 节四个问题的逐条判定

### 问题 ①：`tasks.md` 是否已回到「只有任务定义与复选框」形态？

**主体已清理，但有 2 处残留；另有 1 处子行不属于残留、不应删除。**

| 位置 | 内容 | 判定 |
| --- | --- | --- |
| `tasks.md:8`（1.1 下） | `- 完成：scout PASS，refs/heads/main = 81e350f…；报告 reports/scout-target.md` | ❌ **残留**：这是执行结果/证据，不是任务定义 |
| `tasks.md:11`（1.3 下） | `- 完成：provisioner PASS，8 个 worktree 与独立 CARGO_TARGET_DIR 已创建并核实…` | ❌ **残留** |
| `tasks.md:14`（1.5 下） | `- 完成条件额外包括（DR1-F16/F18）：…sha256 内容摘要写入 verification.md 的 ## Target…` | ✅ **不是残留**：这是**完成条件**，经 Round 5 复核确认为 DR1-F16/F18 要求的合法内容，**建议原样保留** |

派发书列出的 11 处说明行**确已删除**。1.1 与 1.3 不在派发书批次内，属**同类既有残留**——主 Agent 既已判定「完成证据属 `verification.md`」，这两行按同一口径应一并清掉。两行**内容本身是如实的**（对应报告均真实存在），所以不是造假问题，是**口径未统一**。

### 问题 ②：大量任务被勾选完成，是否夹带文字变更？

**勾选按门禁机制不进入摘要（已从门禁源码逐字核实），但勾选状态与 `verification.md` 的两处明文记录直接矛盾。**

1. **归一化机制属实**：`workflow-contract.mjs` 的 `contractDigest()` 对 `tasks.md` 执行 `text.replace(/^(\s*[-*]\s*)\[[ xX]\]/gm, '$1[ ]')`，只归一化复选框字符；**任何附在同行或另起子行的文字都会进入摘要**。
2. **没有夹带文字变更**：被勾选任务的正文逐字未改。摘要从 `1154ae4b…` 变为 `ce6bb8c7…`，与「删除 11 条说明行 + 2.7 内联注记」这组**纯删除**改动完全自洽；`verification.md` 与 `dispatch-queue.jsonl` **均不在摘要文件清单内**（只读 `.openspec.yaml / proposal.md / design.md / plan.md / tasks.md` + `specs/**` + Coverage Index 源文件）⇒ 摘要变化**只可能**来自 tasks.md 那批删除。
3. **但勾选状态与 `verification.md` 明文冲突（须处置）**：决策行明写「`tasks.md` 的 2.7/4.1 **不单独勾选**」，而两行**现已勾选 `[x]`**；且要求「勾选时注明『由 TP2 交付合并结案』」的注记已删除 ⇒ **两文件互相矛盾，且矛盾方之一在摘要覆盖范围内**。另 `## Dispatch Reconciliation` 仍记 TP2 = `unstarted`，与台账 `ready-to-merge` 不一致。

### 问题 ③：`U1` 台账记录标记 `superseded` 是否诚实？merger 执行证据是否仍可追溯？`plan.md` 是否需要相应登记？

**处理本身诚实、依据正确、可追溯性完整；但 `plan.md` 侧缺少一处登记（F71 之外的一条建议）。**

1. **依据正确（门禁源码逐字核实）**：`checkDispatchQueue()` 用 `planWorkPackages(plan)` 构造 ID 集合，而该函数**只读 `## Work Packages` 表**；`U1` 只出现在 `## Merge Strategy` 的 Delivery Unit 列 ⇒ `U1` 确实**不是 Work Package**，门禁报错**准确**，主 Agent 处置**对症**。
2. **诚实**：被标记的记录附原因逐字说明「U1 是 Merge Units 表的交付单元而非 Work Package」，并**显式指向证据的实际落点**。没有删除任何行、没有改时间戳（时间戳单调递增）、没有伪造执行者。
3. **merger 执行证据仍可追溯**，三条独立落点均在且可读：`## Worktree Handoff` 的 U1 行（门禁按「已退役工作包的历史行」放行）、`## Handoff Index` 的 6 条 merger DELIVERY/PASS 行（报告文件均真实存在）、`## Premerge` 块的 `delivery_unit: U1`。
4. **建议**：`verification.md` 的 `## Dispatch Reconciliation` 段尾补一句说明即可，**无需改 `plan.md`**——「交付单元不是工作包」是 schema 既定划分。

### 问题 ④：`## Premerge` 与 `agentic-premerge` 块字段是否逐字一致？两条「证据瑕疵」是否如实？

**目标/候选/行为契约摘要三项逐字一致且有外部佐证；`contract_digest` 一项已过期（会使 premerge 必 FAIL）；两条瑕疵中第 ① 条属实、第 ② 条不属实。**

| 字段 | 核对结果 |
| --- | --- |
| `version: 1` | ✅ 门禁要求 `===1` |
| `delivery_unit: U1` | ✅ 与 plan Merge Strategy 一致 |
| `target_ref` / `target_commit` / `candidate_commit` / `requirements_digest` | ✅ 逐字一致（三处 candidate 同值） |
| **`contract_digest: sha256:1154ae4b…`** | ❌ **已过期**：当前为 `ce6bb8c7…`。门禁 `inspectPremerge()` 有 `if (receipt.contract_digest !== planned.contractDigest) fail('候选报告的规划契约摘要已失效')` ⇒ **premerge 当前必 FAIL**（F66） |
| `verify` / `review` 结构、`reviewer ≠ author` | ✅ 合规；**sha256 数值未独立复算** |
| `alternative_checks` C1/C2 | ✅ 与 plan、tasks 的 `[C1]`/`[C2]`、`## Checks` 对应 |

两条自登记瑕疵：
- **① 属实**：日志文件名标注 stage1 但内容是 workspace 全量 clippy（`merge-u1-candidate.md:314` 命令确为 `--workspace`），块内说明准确不掩盖。
- **② 不实（串号 + 指向空表）**：块内写「WP6 的 **6.6** 复用依据见 `## Merge History`」，但 merger-A5 的待澄清项是 **`tasks.md` 6.3**（`merge-u1-candidate.md:444`）；且 `## Merge History` 当前是**空表**（`<!-- M1 -->` 占位，三列全空）⇒ F67。

---

## 二、第 3 节挂起项的判断

### （1）TP1 处置是否在计划层面正确登记？—— **没有，这是本轮最实质的发现（F71，阻塞 premerge）**

`plan.md` 的 **TP1 行**仍是完整独立工作包（`Dependencies: none`、Owner/Reviewer 齐备）；`## Execution Waves` 的 W1 行仍有 TP1；`## Merge Strategy` 的 U1 `WP / TP` 列仍含 TP1。**「TP1 并入 TP2」只写在 `verification.md` 的决策行里，而 `verification.md` 不在摘要内** ⇒ **计划（权威件）没有登记**。

**可证明的门禁后果**（`checkPremergeHistory` 的 `readiness()`，对 U1 名单逐包核对）：台账状态必须 `ready-to-merge`/`merged`，而 **TP1 台账终态是 `fixing`** ⇒ 报「工作包 TP1 台账状态必须是 ready-to-merge 或 merged（当前：fixing）」。

**必须改什么（最小集，同批一次改完）**：
- `plan.md` `## Work Packages` 的 TP1 行加状态说明（或删除该行）；
- 若删除 TP1，**必须同批**去掉 `tasks.md:2.7` 的 `[wp:TP1]` 标签（门禁 `checkExecutionPlan:399` 会报「引用了不存在的工作包」）；
- `## Execution Waves` W1/TP1 行同步；`## Merge Strategy` 的 U1 `WP / TP` 列同步；
- `tasks.md:2.7 / 4.1` 的勾选与注记，与 `verification.md` **择一并一致**。

**推迟是否可接受？——不可接受推迟到 6.6 之后。** 理由：① 任一残留都会让 premerge 失败（6.6 第一道动作）；② `checkDependencyReview` 在**每个 stage** 都要求 DCR 表最大 Round 那行的 `Plan Revision` 等于当前摘要，**任何 plan.md/tasks.md 后续改动都会强制再开一轮**——现在改可以把 F62/F63/F64/F65/F70/F71 六项一次性合并，**只付一轮代价**。

### （2）DR1-F62/F63/F64 是否阻塞？—— **均未修复，均不阻塞本轮 PASS，也不阻塞 premerge**

| ID | 现状 | 本轮 PASS | premerge |
| --- | --- | --- | --- |
| F62 | `plan.md:120` 仍写「plan WP6 行「额外义务 ④」」，而全库已无该标签 ⇒ 悬空引用 | 不阻塞 | 不阻塞（门禁不解析该字符串） |
| F63 | `plan.md:328` 仍有裸序号「；② 因 WP3 新增必需 trait 方法…」且无 `DR1-F42` 标签 | 不阻塞 | 不阻塞 |
| F64 | `plan.md:375` SFO 行 File 列仍带反引号，与同表其余 22 行不一致 | 不阻塞 | 不阻塞 |

三项与 F71 属同一批 `plan.md` 回写，**建议同批一次改完**。

### （3）并发窗口是否满足？—— **满足**

| 门禁要求 | 实际窗口（UTC） | 结论 |
| --- | --- | --- |
| 同层同角色 ≥2 且池容量 ≥2，需至少一对真实重叠 | W1 coder {WP1, WP2}：`02:38:55 → 12:07:01` / `02:38:56 → 12:07:02` ⇒ 重叠约 9.5 小时 | ✅ |
| 同上 | W3 coder {WP4, WP5}：`16:08:41 → 06:35:44` / `16:08:43 → 06:53:15` ⇒ 重叠 | ✅ |
| 同层同时有 coder 与 tester，需至少一对跨角色重叠 | W1：WP1/WP2（coder 自 `02:38:5x`）与 TP1（tester `02:38:58 → 05:39:22`）重叠 | ✅ |

池容量 `coding: 3` / `testing: 2` 均 ≥2 ⇒ 探针生效。U1 标记 `superseded` 后其窗口按设计从探针释放，不影响上述三组成立。

---

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **DR1-F65** | MINOR | `tasks.md:8`（1.1 下）、`:11`（1.3 下） | `tasks.md` 出现「同类内容两种口径」；后续复核者无法判断哪些续行合法 | 删除这两行（**保留 1.5 的完成条件行**）。会再次变更摘要 ⇒ 与 F62/F63/F64/F70/F71 同批 |
| **DR1-F66** | MINOR | `verification.md` `## Premerge` 块的 `contract_digest` | **阻塞 premerge**：块内为 `1154ae4b…`，当前 `ce6bb8c7…` | 刷新为最终 `contractDigest`（**只改 `verification.md`，不进摘要**，成本为零）；刷新时机放在所有 plan/tasks 改动落定、DCR 表已指向最终摘要之后，一次改到位 |
| **DR1-F67** | MINOR | `verification.md` `## Premerge` 块的自登记「瑕疵 ②」 | 合入前证据链有一处指向**空表** + 一个错任务号 | 改为指向 `reports/merge-u1-candidate.md` §6/§8 与 `## Handoff Index` 的候选级 PV 行，并明确回答 **6.3 记 REUSED**（同一候选提交、零冲突、tree 与 `3484541^{tree}` 逐字节相同） |
| **DR1-F68** | MINOR | `verification.md` `## Dispatch Reconciliation` 全表 | **阻塞 premerge**：四类失配——① State 列带尾注不在 `WORK_PACKAGE_STATES` 内；② Executor 列写成链式串；③ TP1 行与台账不符（`fixing`/attempt 2/`tester-A2`）、TP2 行 State=`unstarted`（非法值）；④ Evidence 列写时间线而非可核对路径。共 8 个工作包行全部受影响 | 把 State/Executor/Attempt 三列改成与台账逐字一致的**裸值**（尾注移入 Evidence 列或同段散文）；Evidence 填 `dispatch-queue.jsonl` 或报告路径。**只改 `verification.md`** |
| **DR1-F69** | MINOR | `verification.md` 的 Handoff Index / Review Findings / Worktree Handoff / Merge History / Premerge History | **阻塞 premerge**（①②③④ 各自独立触发）：① Handoff Index 的 DELIVERY 行只有 TP1 与 U1，**WP1–WP6 与 TP2 全无 DELIVERY 行**；② Review Findings **无 TP2 的独立 review 行**，且多行 Work Package 列为 `NOT_APPLICABLE（规划）`；③ Worktree Handoff 未覆盖 WP5 第 2 轮、WP6 第 2/3 轮、TP2 第 2/3 轮；④ `<!-- M1 -->` **不是注释而是数据行**（`readTable` 只跳过 `---` 分隔行，`rows` 过滤 `nonempty(pick(cells,'Merge ID'))`）⇒ premerge 会报「必须有 receipt 报告路径」 | ① 为 WP1–WP6、TP2 补 `DELIVERY / PASS` 行并绑定该轮认领执行者；② 为 TP2 补 CR8 的 Review Findings 行；③ 按 (WP, Attempt) 补 Worktree Handoff 行；④ 在拿到真数据前把两处占位行的 `Merge ID` 单元格**留空**。**全部只改 `verification.md`** |
| **DR1-F70** | SUGGESTION | `plan.md` Coverage Index 的 `evidence: [reports/PV1.log, reports/PV2.log]`；`verification.md` `## Checks` 同名行 | **不阻塞 premerge，阻塞 final/archive**；`reports/` 中**没有** `PV1.log`/`PV2.log`（真实产物叫 `merge-u1-candidate-PV1-stage2-workspace-test.log` 等），门禁在 `plan` 阶段容忍 ENOENT，但 `final`/`archive` 会 fail | 二选一：① 在跑 8.1/8.2 时真实产出这两个文件；② 把 Coverage Index 与 `## Checks` 的证据改指已存在的候选/最终日志（需改 `plan.md` ⇒ 与 F62/F63/F64 同批） |
| **DR1-F71** | MINOR | `plan.md` 的 TP1 行 / W1 行 / U1 `WP / TP` 列；`tasks.md:2.7 / 4.1` | **阻塞 premerge**：TP1 台账终态 `fixing` 而 U1 名单含 TP1；且计划与已生效的用户决定不一致，`tasks.md`（在摘要内）的勾选状态无法解释 | 同批改四处（见第 3 节）。**会变更摘要 ⇒ 需 Round 17** |

---

## Assessment

### A — Round 15 findings 的复核

| ID | 判定 |
| --- | --- |
| DR1-F62 | **未解决**（`plan.md:120` 逐字未变） |
| DR1-F63 | **未解决**（`plan.md:328` 裸序号仍在，与 `tasks.md:29` 口径仍不一致，顺序仍相反） |
| DR1-F64 | **未解决**（`plan.md:375` 反引号仍在） |

三项均为记录/措辞层，不涉及依赖声明、批次、资源互斥与写入归属的实质，**不阻塞本轮 PASS，不阻塞 premerge**。

### B — 依赖声明属实性复评

`code:` 声明全部属实（WP3←WP2、WP4←WP3、WP5←WP1+WP3、WP6←WP1–WP5、TP2←WP1–WP6 均真的需要上游代码）；波次划分成立（W1 的 WP1/WP2 无 code 依赖、契约已冻结、无共享写入、无独占资源；W3 的 WP4/WP5 写范围不相交）；Shared File Ownership 的 23 行逐行复核，三项判据均满足；同批写入重叠的行均分处不同波次且已写明区域不相交的理由；**未发现新的越界或无主文件**。

### C — 本轮新发现汇总

`F65`（tasks.md 残留 2 行）、`F66`（Premerge 块摘要过期）、`F67`（瑕疵②不实）、`F68`（Dispatch Reconciliation 失配）、`F69`（Handoff/Review/Worktree/占位行四类登记缺口）、`F70`（证据路径未产出）、`F71`（TP1 退役未在计划登记 + 与已勾选任务矛盾）。

**F66/F67/F68/F69/F71 会阻塞 premerge，F70 只阻塞 final/archive，F65 只影响记录口径；全部修复都不需要改行为契约**（`requirementsDigest` 保持 `sha256:53943270…`）。

### D — 待补证据

| 待补项 | 门禁 | 影响本轮判断 |
| --- | --- | --- |
| `contractDigest` 的**实测**复核（本实例无 shell） | plan | 否——但若实测 ≠ `ce6bb8c7…`，本轮结论须整体重开 |
| `agentic-premerge` 块内 3 个证据路径的 sha256 实算 | premerge | 否——结构合规已核，数值未复算 |
| `reports/PV1.log` / `PV2.log` 的产出或改指 | final/archive | 否 |

---

## 结论 **PASS**（0×CRITICAL / 0×MAJOR）。结论绑定 `sha256:ce6bb8c7c5d6b92172e8a2b5f2428b416b305fd47488af557c9d923e1aad1ef3`。

- **四个问题**：① 已清理但 1.1/1.3 残留两行同类内容（1.5 合法、勿删）；② 无文字夹带、勾选不入摘要，但 2.7/4.1 的勾选与 verification 明文矛盾；③ U1 标记 superseded 诚实有据、merger 证据完整可追溯、plan.md 无需补登 U1；④ 目标/候选/requirements 逐字一致，contract_digest 已过期，两条瑕疵中①属实②不实。
- **挂起项**：F62/F63/F64 不阻塞本轮 PASS 也不阻塞 premerge；**TP1 计划登记缺失（F71）必须现在同批补，不得拖到 6.6 之后**；并发窗口满足。
- **是否阻塞 premerge 门禁**：**本轮 findings 不阻塞；但 premerge 门禁当前仍会被记录层的 5 项独立问题挡住——F66、F67、F68、F69、F71。** 其中 F66/F67/F68/F69 **只改 `verification.md`**（零摘要成本）；只有 F71（宜连同 F62/F63/F64/F65/F70）要改 `plan.md`/`tasks.md`，会变更摘要并强制再开一轮——**建议立刻把五项 plan/tasks 回写一次性做完，再开 Round 17，然后才跑 6.6**；若先跑 6.6，则必然因 F71 失败并浪费一次候选门禁。

### handoff_index

- `task_id: "NOT_APPLICABLE（规划门禁，Round 16）"` · `work_package: NOT_APPLICABLE` · `role: reviewer` · `phase: plan` · `round: 16` · `stage: plan` · `target_revision: "sha256:ce6bb8c7c5d6b92172e8a2b5f2428b416b305fd47488af557c9d923e1aad1ef3"` · `evidence_type: REVIEW` · `evidence_id: DR1` · `report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round16.md"` · `result: PASS` · `evidence_status: NEW`
- `applicability_basis`: Review Type=plan，stage=plan，目标为规划契约摘要（非提交 SHA）。核对 plan/tasks/design/verification、dispatch-queue.jsonl、三份历史报告、`openspec/agentic.yaml`，以及门禁实现 `workflow-check.mjs` / `workflow-contract.mjs` / `dispatch.mjs` 的 contractDigest 归一化、checkDependencyReview、checkDispatchQueue、checkConcurrencyProbe、checkReconciliation、checkReviewFindings、checkWorktreeHandoff、checkPremergeHistory、inspectPremerge 与 readReceipt 判据。限制：无 shell，未实测 contractDigest、未复算证据 sha256、变更目录未入 git 无 diff 基线、未判代码质量/测试充分性/需求实现。
