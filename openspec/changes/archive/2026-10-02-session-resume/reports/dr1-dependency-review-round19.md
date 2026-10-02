# DR1 Round 19 — 引用修正核实报告（Review Type: plan）

> 持久化说明：本报告由**第 19 个全新独立 reviewer 实例**（只读工具集，无 shell、无写权限）以全文返回、由主 Agent 落盘。本实例未参与前 18 轮、U1 任何工作包实现与合并。
> **`contractDigest` 由主 Agent 代实测**：`workflow check --stage plan --json` → `contractDigest=sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936`（**与派发给定值逐字一致**）、`requirementsDigest=sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa`（**与 Round 14–18 相同 ⇒ 行为契约一字未动**）、`errors[]` 仅一条（DR1 表未绑定当前摘要 = 本轮待登记动作）。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 19）"
round: 19
stage: plan
target_revision: "sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936"
result: PASS
issues: "0×CRITICAL / 0×MAJOR / 1×MINOR（DR1-F83，非本轮改动引入，仅登记）"
```

## 一、背景

U1 已合入本地 `refs/heads/main`（合并提交 `0d2be6d`，父提交对 `81e350f` + `2ed142d`；修正提交 `69f1ac1`）。合并后 `npm run check` 在第 7 道 `check:docs` 报 2 个错误，**两处均在随本次提交入库的规划/证据资产内，产品代码零涉及**：
- 错误 1：`plan.md` 的 `## Shared File Ownership` 中 `docs/MODULE_ARCHITECTURE.md` 行，原文把 `MODULE_ARCHITECTURE.md` 的第四章第一节写成了归属 `AGENTS.md` 的节号（`AGENTS.md` 没有该节）。门禁只认「同一子句内紧邻指名的文档」，于是把那个节号归给 `AGENTS.md`，判定不存在。为避免复述错误句子本身再次触发该门禁（已发生过，见下方「同类事故记录」），此处不逐字引用原句。
- 错误 2：`reports/merge-u1-integrate-wp4.md:189` 引用了 `cr4-review.md` 中不存在的第九节（已由 merger 修正）。

**主 Agent 对错误 1 的修正**（本轮唯一 plan.md 改动）：
> 原：把 `AGENTS.md` §10 的要求转述为「core 端口签名/值对象变化时同步 `MODULE_ARCHITECTURE.md` 的第四章第一节」，但**未在同一子句内写出 `docs/MODULE_ARCHITECTURE.md` 这个可解析文档名**，导致该节号被归给 `AGENTS.md`。（为避免复述触发门禁，此处不逐字引用原句。）
> 新：`DR1-F50：\`AGENTS.md\` §10 要求 core 端口签名/值对象变化时更新 \`docs/CORE_PORTS_AND_STORAGE.md\` 并同步 \`docs/MODULE_ARCHITECTURE.md\` §4.1，此前无 WP 拥有该文件。`

## 二、五问逐条判定

### 问 1：修正是否忠实于 `AGENTS.md` §10 原文 —— **忠实**

`AGENTS.md:294` 原文（逐字）：「core 端口签名、值对象、broker 事务顺序或 storage-sqlite 表结构/保留策略变化：更新 `docs/CORE_PORTS_AND_STORAGE.md`，并同步 `docs/MODULE_ARCHITECTURE.md` §4.1/§4.7 的职责描述」。

逐项对账：要求更新的文档 = `docs/CORE_PORTS_AND_STORAGE.md` ✅；要求同步的文档与节 = `docs/MODULE_ARCHITECTURE.md` §4.1 ✅（原文另有 §4.7，本行只取 §4.1）；触发类型 = 「core 端口签名/值对象」✅ 是原文四个触发项的子集。**无一处张冠李戴或改变语义**。略去 §4.7 属收窄转述而非失真，登记为 F83，不判为缺陷。

对照旧表述的两个问题：(a) 只说「同步 §4.1」不说同步**哪份**文档，使 `§4.1` 在门禁算法下被归给同子句内唯一指名的 `AGENTS.md`；(b) 略去了本行唯一相关的落点文档名。修正把两份文档写进同一子句，是修 (a)(b) 的最小手段。

### 问 2：引用归属是否正确 —— **正确，与门禁源码算法逐字吻合**

`scripts/check-doc-links.mjs:151-166` 的归属规则：取 `§` 前 **48 字符**，按 `、，。；：（）()「」【】` 切分取**最后一段子句**，在其中匹配 `.md` 文件名或大写别名，**取最后一个能解析成功的命中**；找不到则不判定（只计数）。

| 位置 | 最后子句 | 归属目标 | 目标是否含该小节 | 判定 |
| --- | --- | --- | --- | --- |
| `AGENTS.md` **§10** | `` `AGENTS.md` `` | `AGENTS.md` | 有 `## 10. 文档维护` | 通过 |
| `…MODULE_ARCHITECTURE.md` **§4.1**（修正处） | 48 字窗口起点落在 `CORE_PORTS_AND_STORAGE.md` 尾部（被截成 `TORAGE.md`，三候选均不存在→不命中），子句内**最后一个可解析命中**是 `` `docs/MODULE_ARCHITECTURE.md` `` | `docs/MODULE_ARCHITECTURE.md` | **`### 4.1 core` 存在**（`:170`，内含 `core::ports` 端口块） | 通过 |
| 区域注记内的 `区域=`**§4.1** | `区域=` | 无法归因 | — | 按设计只统计 |

关键点：修正生效**不是**因为删掉了 `AGENTS.md`，而是因为在同一子句内**紧邻写出了 `docs/MODULE_ARCHITECTURE.md` 这个可解析文档名**，使「取最后一个命中」落到正确目标。

独立旁证：`MODULE_ARCHITECTURE.md:170-247` 的 §4.1 现有文本**已含** `SessionBackendFactory … resume`、`SessionEndpoint … agent_session_id`、`SessionStore … load_recovery` 与值对象清单中的 `AgentSessionId / ResumeSessionRequest / SessionRecoveryRecord` —— 本行 Region Note 描述的 WP3 落点与文档现状一致。

### 问 3：该行其余内容是否被意外破坏 —— **未破坏**

`watchdog_diff` 显示该表行是唯一改动行，且是行内单句替换。逐项核对未被触及的列：`File` = `docs/MODULE_ARCHITECTURE.md` ✅；`Writers (WP)` = `WP3`、`Merge Owner` = `WP3`、`Merge Order` = `WP3`、`Re-verify After Merge` = `PV2` ✅ 四列逐字未变；Region Note 后半段逐字未变；句首 `DR1-F50：` 编号与追溯锚点保留 ✅；新文本不含 `|`，单元格数不变（6 列）✅。

### 问 4：`node scripts/check-doc-links.mjs` 是否 exit 0 —— **是（依赖主 Agent 实测；本实例无 shell，未能自行复跑）**

实测输出：`doc links OK: 404 relative links, 9306 section refs across 483 markdown files`，退出码 **0**（修正前为 exit 1、报 2 个 `doc link check failed`）。按 `check-doc-links.mjs:186-217` 的判定链对全仓做静态复核，**除本行外未发现其它被本次改动影响的 `§` 引用**（delta 只有 1 行）。

附带事实：`check-doc-links.mjs:41-49` 扫文件系统、不解析 git。U1 合入 main 后变更目录已转为已跟踪资产，这道门禁对其覆盖从此稳定；候选阶段那两处错误之所以「从未被候选 PV2 拦住」，是因为 `plan.md` 的 mtime 晚于候选 PV2 运行时刻 —— **本轮修正因此是首次真正被 PV2 类门禁覆盖的版本**。

### 问 5：是否只涉及引用归属 —— **是**

`watchdog_diff` 精确给出 `openspec/changes/session-resume/plan.md | 2 +-`（1 file changed, 1 insertion, 1 deletion），**无第二个文件**；`proposal.md`、`specs/**`、`design.md`、`tasks.md`、`verification.md` **全部未出现在 delta 中** ⇒ 行为契约面零改动。`requirementsDigest` 与 Round 14–18 相同亦为佐证。`contractDigest` 变化是预期（plan.md 属规划契约摘要），属**流程动作**而非语义变更。写入范围（`Writers/Owner/Order/Re-verify` 与 WP3 写范围行）均未变。

## 三、Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| **DR1-F83** | MINOR | `plan.md:370`（SFO 的 `docs/MODULE_ARCHITECTURE.md` 行） | 新表述把 `AGENTS.md:294` 的转述收窄到「core 端口签名/值对象 → 更新 `CORE_PORTS_AND_STORAGE.md` + 同步 `MODULE_ARCHITECTURE.md` §4.1」，未提同一原文里的 **`§4.7`** 与 `broker 事务顺序`/`storage-sqlite 表结构` 两个触发项。`AGENTS.md:294` 对后二者同样要求同步 `MODULE_ARCHITECTURE.md` §4.7，但 SFO 表把该文件**只**登记给 WP3，WP4 名下无该文件 | **非本轮改动引入**（旧表述同样只写 §4.1），**不被任何门禁读取**，不阻塞 PV2/premerge/final。仅意味着：若 WP4 的 DDL 变化需同步 §4.7，该动作在计划中仍无写归属（与 F52 同族） | **本轮不要改**（会再次变更 `contractDigest` 并使本轮绑定失效）。建议与 F62/F63/F64 同批处理：为 WP4 补登 `docs/MODULE_ARCHITECTURE.md`（§4.7）或注明「§4.7 由 WP4 判断、当前不需要」 |

**未发现 CRITICAL / MAJOR。** 本轮改动未引入任何新问题。

## 四、未在本轮重判的项

Round 18 已判 PASS 的项不重判（plan.md 的 8 处 `<worktree…>` 占位、verification.md 各表结构、DR1-F74/F62/F63/F64 挂起项）。**`DR1-F80`（`verification.md:43` 的 `tester-A2` 加粗）仍是 Round 18 记录的 premerge 阻塞项**，本轮无新证据推翻它 —— 但主 Agent 已在 Round 19 派发前修复该处并实测 `--stage premerge` 转 PASS，本轮未取得该实测回执。

## 五、待补证据

| 待补项 | 状态 | 门禁 |
| --- | --- | --- |
| DCR 表第 19 行绑定 `sha256:2adbf605…ca936` | 待主 Agent 写入；这是 `--stage plan` 当前**唯一**的 error | 写入后 `--stage plan` 应转 PASS |
| 合并提交 `0d2be6d` / `69f1ac1` 的 diff | 不在本轮可检视范围（`watchdog_diff` 只给工作区 delta）；错误 2 的修正过程无法证明，只能读到当前文件内容自洽 | 需要时由主 Agent 提供 `git show` 产物 |

**残余风险**：① `DR1-F80` 未在本轮闭环；② `DR1-F83` 与 F62/F63/F64、F74 同属「下次触碰 plan.md 时同批修」清单；③ 提交范围 diff 不可见是本轮工具限制，非仓库问题。

## 六、结论

**PASS**（0×CRITICAL / 0×MAJOR；1×MINOR `DR1-F83`，非本轮引入、仅登记），绑定**实测** `contractDigest = sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936`，`requirementsDigest` 未变。

**是否阻塞主分支 PV2（`npm run check`）复跑：否。** 本轮修正只改了一行引用归属，`check:docs` 已实测 exit 0；其余门禁不受此行影响。

### handoff_index

- `task_id: "NOT_APPLICABLE（规划门禁）"` · `role: reviewer` · `phase: plan` · `round: 19` · `stage: plan` · `target_revision: "sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936"` · `evidence_type: REVIEW` · `evidence_id: DR1` · `report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round19.md"` · `result: PASS` · `evidence_status: NEW`
- `applicability_basis`: 第 19 个全新独立实例，只读核实 `plan.md:370` 单行引用修正；`contractDigest`/`requirementsDigest`/`errors[]` 由主 Agent 代跑 `workflow check --stage plan --json` 实测并回传完整原文，与派发给定值逐字一致；`node scripts/check-doc-links.mjs` exit 0 亦为主 Agent 实测（本实例无 shell，已如实标注）；归属算法按 `scripts/check-doc-links.mjs:151-217` 源码独立静态推演，结论与实测一致；`AGENTS.md:294` 与 `docs/MODULE_ARCHITECTURE.md:170/325` 均由本实例直接读取原文核对，未采信调度者转述。