# DR1 Round 17 — 依赖声明审查报告（Review Type: plan）

> 持久化说明：本报告由**第 17 个全新独立 reviewer 实例**（只读工具集，无 shell、无写权限）以全文返回、由主 Agent 原样落盘。run `43307c2f-afa2-4fd3-9a5a-1d8f46c3775c`。
> **`contractDigest` 由主 Agent 代为实测**（本实例无 shell）：`workflow check --stage plan --json` → `result=FAIL`、`contractDigest=sha256:700caa254e736c1d5f8c5678ff823df5b695d4ec29b24ca70f0bd7592e5a019c`（与派发给定值逐字一致）、`requirementsDigest=sha256:53943270…408fa`、`errors[]` 仅一条（DR1 表未绑定当前摘要，即本轮待办）。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 17）"
work_package: "NOT_APPLICABLE"
role: reviewer
phase: plan
target_revision: "sha256:700caa254e736c1d5f8c5678ff823df5b695d4ec29b24ca70f0bd7592e5a019c"
result: PASS
issues: "0×CRITICAL / 0×MAJOR / 5×MINOR（F72–F76）/ 1×SUGGESTION（F77）"
```

## 一、六项 `plan.md` / `tasks.md` 回写的逐条判定

| 项 | 判定 | 依据 |
| --- | --- | --- |
| **DR1-F71**（TP1 并入 TP2 的计划层登记） | ✅ **已解决** | 四处同批回写（`plan.md:329` TP1 行状态注记、`:340` W1/TP1 行注记、`:414` Merge Strategy U1 的 `WP / TP` 列**移除 TP1**、`tasks.md` 2.7 内联注记删除）一致自洽。**readiness 阻塞确实解除**：`checkPremergeHistory`（`workflow-check.mjs:708-716`）按表头名取 `WP / TP` 列，现值不含 TP1 ⇒ `checkCurrentUnitReadiness()`（`:746-754`）只遍历 7 个包，不再检查 TP1 的 `fixing` 状态。`tasks.md` 的 `[wp:TP1]` **仍然合法**：TP1 仍是 `## Work Packages` 内字段齐全的工作包、在 W1 有且仅有一行、并有 `tasks.md` 3.8 `[CR7]` 这条**非派发** review 任务，满足 `checkExecutionPlan` 全部要求。「保留 TP1 行 + 从 U1 就绪名单移除」是正确选择 |
| **DR1-F62** | ⚠️ **部分解决** | `plan.md:120` 已改为可定位标签；但 `verification.md:223` 仍引用已废弃的「额外义务 ④（CR3-F1）」，成为新悬空引用 → F76 |
| **DR1-F63** | ✅ **已解决** | `plan.md` 全文检索 `；①`…`；⑤` **零命中**；`plan.md:328` 现为 `额外义务（DR1-F42 · 五个 load_recovery 替身）`，与 `tasks.md` 2.6 的标签名与关键约束逐字一致（仅次序与 F51 载体差异，不影响按标签检索） |
| **DR1-F64** | ✅ **已解决** | `plan.md:375` 现为裸字符串 `crates/server/tests/`，与同表其余 22 行一致，并与 `scopeOverlap`（`:106`）的裸字符串比较口径对齐 |
| **DR1-F65** | ✅ **已解决**（区分正确） | 1.1、1.3 下的执行证据行已删；1.5 的「完成条件额外包括（DR1-F16/F18）」**保留**——那是**完成条件**（对未来动作的约束）而非执行结果，Round 5 对 F16/F18 的复核亦确认其为合法内容。`tasks.md` 现在只剩「复选框 + 任务定义 + 派发/复核参数 + 1.5 唯一完成条件子行」 |
| **DR1-F70** | ⚠️ **部分解决**（阻塞 final/archive，不阻塞 premerge） | 两个真实日志确实存在；`verification.md` 的 `## Checks` 已 100% 同步；但 `plan.md` Coverage Index **只改了 6/37 行**（R1/R4/R13/R18/R27/R34），其余 31 行仍是 `evidence: [reports/PV1.log]`（该文件不存在），且 `### Project Verify` 表 Evidence 列与 `## Completion Criteria` 首条仍写该路径。门禁影响已逐字核实：`workflow-check.mjs:1255-1257` 对 `stage !== 'plan'` 不容忍 ENOENT，而 `inspectPremerge`（`:1302-1303`）内部跑的是 `stage:'plan'` ⇒ **premerge 容忍、final/archive 阻塞** |

## 二、`verification.md` 记录层修复的结构达标判定

| 项 | 判定 | 证据 |
| --- | --- | --- |
| **F66** `contract_digest` 刷新 | ✅ 已解决 | 现为 `sha256:700caa25…a019c`，与实测值逐字一致 |
| **F67** 瑕疵②订正 | ✅ 已解决 | 已改为「DR1 Round 16 已订正为不实」并指向真实存在的报告与台账行，不再指向空表 |
| **F68** `## Dispatch Reconciliation` | ⚠️ 部分解决 → F73 | State/Attempt/Evidence/散文四项到位（逐行比对台账：WP1/1、WP2/1、TP1/2、WP3/2、WP4/1、WP5/2、WP6/3、TP2/3 全部一致）；但 Executor 改裸值后与 Handoff Index 白名单冲突 |
| **F69①** 7 条 `DELIVERY / PASS` 行 | ❌ 未真正交付 → F72 | 落在 `## Target` 段内、`## Handoff Index` 标题之前且无表头 ⇒ `readiness()` 读不到 |
| **F69②③④** | ✅ 已解决 | `## Review Findings`（`:219-253`）内已无任何 `NOT_APPLICABLE（规划）` 行（40 处命中全部落在 `:260-299` 的 `## Planning Findings` 内）；`tableRows`（`:65-77`）遇下一标题即 break，故 `checkReviewFindings`（`:440`）读不到规划表；Worktree Handoff 已含 WP5 第 2 轮、WP6 第 2/3 轮、TP2 第 2/3 轮共 5 行；两张历史表的占位行首格已留空，`rows.filter(nonempty(pick(cells,'Merge ID')))`（`:704`、`:760`）会过滤掉 |

**特别核实**：`## Review Findings` 的 30 余行 Work Package 列全为 `WP1`…`WP6`/`TP1`/`TP2`（全部计划内 ID）；`## Planning Findings（DR1 线程）` 承接了 DR1-F1…F64（含 F62/F63/F64）。

## 三、`verification.md` 完整性核对（本轮附带任务）

**结构完好**：384 行、19 个二级标题全部齐全无缺失无重复、无尾部截断；各关键行（CR1-F2/CR2-F1/CR3-F1/CR4-F1/CR5-F1/CR6-F4/CR7-F1/CR8-F7/CR-C1-F2/DR1-F64）各自唯一；`agentic-premerge` 块 YAML 自洽，三处 `candidate_commit` 同为 `2ed142de…`，`reviewer ≠ author`。

**内容有两处脚本拼接残留**（→ F75 / F72，均不在门禁读取路径上，但会让后续复核者读到自相矛盾的执行历史）：
1. `:135` Round 6 的 bullet 被拦腰截断（反引号未闭合），紧接着 `:137` 又出现一条内容几乎相同的完整版 `DR1-F29` bullet；
2. `:161` `## Dependency Handoffs` 段内出现一行**无列表符、无主语**的孤儿半句，且其「尚未合入集成基线，因此 W2（WP3）仍为 unstarted」与同段前面的「集成基线已建成（b0a387b）」及 `:171` 的「目前无下游接入」互相矛盾（三句属不同时点，被压成同段且丢了时间限定词）。

## 四、挂起项

- **U1 台账 `superseded`**：仍恰当、可追溯。依据属实（`planWorkPackages` 只读 `## Work Packages` 表，U1 只在 Merge Strategy 的 Delivery Unit 列）；三条证据落点均在且可读；该标记只把 U1 从并发探针释放，不影响其余三组。
- **并发窗口探针**：仍满足。池 `coding:3`/`testing:2` 均 ≥2；W1 同角色（WP1∥WP2，重叠约 9.5h）、W3 同角色（WP4∥WP5）、W1 跨角色（WP1/WP2∥TP1）三组均有真实重叠。
- **DR1-F63/F64** 已解决；**F62/F70** 部分解决（见上）。

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **DR1-F72** | MINOR | `verification.md` 的 7 条 `DELIVERY / PASS` 行位于 `## Target` 段内、`## Handoff Index` 标题之前且无表头 | **阻塞 premerge**（7 个工作包全数触发「缺少…DELIVERY PASS 行」） | 把这 7 行原样搬进 `## Handoff Index` 表体（表头与 `---` 分隔行之后）。已逐行核对搬入后即可通过：WP1→`coder-A`、WP2→`coder-B`、WP3→`coder-C`、WP4→`coder-D`、WP5→`coder-E2`、WP6→`coder-F3`、TP2→`tester-A3`，与 `dispatchAttempts` 各 attempt 首条事件的认领执行者逐字一致 |
| **DR1-F73** | MINOR | `## Dispatch Reconciliation` 的 Executor 列 vs `## Handoff Index` 的 `Executor / Agent` 列 | **阻塞 premerge**：3 条「执行者未在 Handoff Index 中登记」。`checkReconciliation`（`:945-947,966-967`）以 `caseIds` 作白名单，而 `caseIds`（`:32`）按 `[,，、\s]+` 切分 | 补 CR4（`reviewer-D`）与 CR8（`reviewer-T3`）的 REVIEW 行，并在 TP1 行的 `Executor / Agent` 加上 `tester-A2`。**必须写成 `reviewer-D, run xxx` 这种含空格的形态**才能切出裸 token |
| **DR1-F74** | MINOR | `plan.md` Coverage Index 31 行、`### Project Verify` Evidence 列、`## Completion Criteria` 首条 | **阻塞 final/archive，不阻塞 premerge** | ① 在 8.1/8.2 时真实产出 `reports/PV1.log` 与 `reports/PV2.log`（推荐，`npm run check` 与 `cargo test` 需重定向落盘）；② 或把剩余 31 行与 plan 两处 Evidence 一并改指已存在的日志。**注意 ② 会再改 `plan.md` ⇒ 再变更摘要** |
| **DR1-F75** | MINOR | `verification.md:135`（截断 + 反引号未闭合）与 `:137`（重复完整版）；`:161`（孤儿半句且与同段自相矛盾） | 不阻塞门禁（`## Check Plan Changes` 与 `## Dependency Handoffs` 都不被 `readTable` 读取），但会让后续复核者读到**自相矛盾**的执行历史 | 删掉 `:135` 的截断半句（保留 `:137` 完整版）；给 `:161` 补回行首与主语并显式标注时点 |
| **DR1-F76** | MINOR | `verification.md:223`（CR3-F1 行 Resolution 单元格） | 不阻塞门禁，但与 F62 是同一失效模式：引用已删除的标签，使该单元格成为对不存在对象的事实性断言 | 改用 `额外义务（CR3-F1 · uncertain 终态）` 的现标签 |
| **DR1-F77** | SUGGESTION | `plan.md:329`（12 格/11 列表头）、`:340`（5/4）、`:414`（7/6） | F71 三条注记都写在**表头列数之外的溢出格**里，`readTable.pick`（`:96`）按表头名取下标 ⇒ **不被任何检查读取**。已确认不会致 plan 阶段 FAIL（`:340` 第 4 格仍是 `NOT_APPLICABLE` ⇒ `hasReason=false`，跳过 `:338` 的枚举检查），但对机器惰性；若将来有人把 `:340` 注记挪进第 4 格会立刻报「Serialization Reason 非法」 | 或并入该行已有列，或在表下方加表外散文。**不必现在做** |

### 残余风险（既有、非本轮引入）

- `## Worktree Handoff` 的 U1 行 Executor 写 `merger-A`，而台账 U1 第 1 轮认领执行者为 `merger-A4`；`checkWorktreeHandoff:681` 会核对二者。
- `checkWorktreeHandoff:675` 用 `provisionerReports.has(provisioner.trim())` 核对，而 `provisionerReports` 的键是 `caseIds(Executor / Agent)` 的单个 token（不含空格）；`## Worktree Handoff` 各行的 `Provisioner` 单元格是含空格的整句 ⇒ 该核对在当前文本下**可能恒不成立**。列为待实测项，建议在 6.6 前用一次 `--stage premerge` 实测确认。

## 结论 **PASS**（0×CRITICAL / 0×MAJOR；5×MINOR F72–F76 + 1×SUGGESTION F77），绑定**实测** `contractDigest = sha256:700caa254e736c1d5f8c5678ff823df5b695d4ec29b24ca70f0bd7592e5a019c`。

**是否阻塞 premerge 门禁：是** —— F72 与 F73，二者均为本轮回写引入的记录层缺口，不改变依赖声明事实，且**都只改 `verification.md`、零摘要成本**，可一次性做完。F74 只阻塞 final/archive，不阻塞 premerge，建议在 8.1/8.2 真实落盘 `reports/PV1.log`/`PV2.log` 以零摘要成本闭合。