# RV5-WP5 · 最终候选收尾复核报告（候选 4595799）

> 由 reviewer 子 Agent 输出工件原样落盘（运行 16d217e1），主 Agent 未改动结论。
> result=PASS；其 §7 给出「块 + 勾选 + 报告」follow-up 提交的适用前提与门的自指约束（块声明的 candidate_commit 必须等于运行门时的 HEAD）。
> 其 F1/F2/F4 已在本轮收尾提交内修掉；F3 为报告级（不改历史工件）。

## result: **PASS**（`merge verdict: OK with notes`）

RV4 六项发现：**4 项已解决**（F1/F4/F5/F6-登记面）、**1 项未解决但工件已补齐、登记未同步**（F2）、**1 项未解决且已登记为延后落盘**（F3）。目标候选 `4595799` 的验证证据链**自洽**（HEAD 真回显 = `4595799`、树干净、五项 `EXIT=0` 齐备、PV1 计数我独立复算一致、[PV2]–[PV5] 已是完整输出）；`plan.md` 的 `alternative_checks` 形态修正**对症**（flow 序列解析为恰好 4 条、文本与原 4 条逐字相同），但**未登记进 `Check Plan Changes`**；delta 纯净（4 个文件，全在变更目录内）。新发现 4 条，全为 P2（登记面/报告级），其中 F1/F2 建议在写 `agentic-premerge` 块前用 2–4 行文本修掉。

**版本绑定事实（我的前提）**：`watchdog_diff` 报 "No working-tree changes against reviewer-launch HEAD 459579940c08"，即我读到的所有文件内容 = 该候选版本的树内容；以下行号均为该版本行号。

---

## 1. 逐项复核 RV4-WP5-F1…F6

| # | RV4 发现 | 状态 | 依据（target 版本 `4595799` 实测） |
|---|---|---|---|
| F1 | 工件无 `git rev-parse HEAD`/`git status --porcelain` 真回显，[PV2]–[PV5] 为节选，而 `verification.md:139` 称其「含」 | **已解决** | `reports/candidate-verify-final.log:3-8` 现为命令回显：`:3 ### $ git rev-parse HEAD` → `:4 459579940c08c6f64a4cc23814a56b7c46c94230`；`:5 ### $ git log --oneline -1` → `:6 4595799 docs(app): 修正 premerge 门对 alternative_checks 的解析形态`；`:7 ### $ git status --porcelain（空输出 = 干净）` 紧跟 `:8 EXIT(git-status --porcelain)=0`（标签与 EXIT 之间无输出行 ⇒ 空输出）。`[PV2]–[PV5]` 已是**完整输出**：`:1296-1457`（PV3，含 `Running unittests/tests/*.rs` 目标名与逐条用例名，如 `:1300 test local_admin::envelope::tests::… ok`）、`:1459-1570`（PV4 同型）、`:1572-1597`（PV5 含 11 条用例名）、`:1292-1294`（PV2 = 脚本自身一行输出）。**残留小瑕（不计为发现）**：porcelain 段没有显式「(no output)」行，`（空输出 = 干净）` 属编辑性括注，但空输出本身可见。 |
| F2 | PRO-4 的「隔离重跑 5/5（各 0.15–0.17 s）」无独立日志工件 | **未解决（工件已补齐，登记未同步）** | 工件**已在盘且合规**：`reports/agent-host-flaky-rerun.log:1-33`——`:2` 标注目标提交 `459579940c08c6f64a4cc23814a56b7c46c94230`，5 次运行各含 `test oversize_frame_ends_the_agent_and_fails_pending_requests ... ok`、`1 passed; 0 failed; 0 ignored; 14 filtered out` 与 `EXIT(run1..run5)=0`。**但**：① `verification.md:188` 声称「PRO-4 **改引该工件**」，而 PRO-4 行（`verification.md:220`）仍写「证据 `reports/candidate-verify.log` 候选轮 2b 段」、「该用例隔离重跑 5/5 通过」，全文未引用 `reports/agent-host-flaky-rerun.log`（变更目录内该名仅出现在 `:188` 这一行）；② `verification.md:138` 仍写「各 0.15–0.17 s」，而工件的五次耗时是 **0.35 / 0.17 / 0.16 / 0.18 / 0.17 s**（`:10/:16/:22/:28/:34` 区段），与登记区间不符。⇒ 见新发现 **RV5-WP5-F1**。 |
| F3 | `verification.md:75`「见 6.3 行」悬空；最终候选 PV 运行无 Handoff Index 归属行 | **未解决（已登记为延后落盘）** | `verification.md:74/75` 仍是 Handoff Index 的最后两行，其后直接是 `:76 MD2`、`:77 MD1` —— **无 6.3/6.4 行**；`:75` 仍含「最终候选的单一工件见 `reports/candidate-verify-final.log`（自洽单文件，见 **6.3 行**）」⇒ 指针在候选版本内仍悬空；`tasks.md:63` 6.3、`:64` 6.4 仍为 `[ ]`。处置行 `verification.md:189` 写「**已处理**：补 6.3/6.4 行（…）」，但其 Recheck 列写「已处理（**随合并后记录一并落盘**）」⇒ 属于**登记了的延后**，非已完成的修复。⇒ 见新发现 **RV5-WP5-F2**。 |
| F4 | 报告路径 `rv4-candidate.log` 命中 `.gitignore:22/27`，合并提交里将没有 RV4 工件 | **已解决** | `reports/rv4-candidate.md` 以**被跟踪的新文件**进入本 delta（`rv-input-rv5.diff.log:30-31`：`new file mode 100644`；磁盘 `ls` 只有 `.md`、无 `.log`）；delta 未改 `.gitignore`（策略未变）。**残留**：该报告自身 `handoff_index.report_path`（`reports/rv4-candidate.md:145`）仍指向不存在的 `…/rv4-candidate.log` ⇒ 见新发现 **RV5-WP5-F3**（报告级，建议以块内路径为准，不必回改历史工件）。 |
| F5 | `tasks.md:64` 完成条件与 `plan.md:678` 指向不存在的 `rv1-du1.md` | **已解决** | `tasks.md:64` 现列举实际报告链（`rv1-wp1.md`…`rv1-wp5.md`、`rv2-wp4.md`、`rv2-wp5.md`、`rv3-candidate.md`、`rv4-candidate.md`）并注明「原清单里的 `rv1-du1.md` 只存在于已归档变更」；`plan.md:678` 的 RV1 证据列同型改写（「（按轮次累积；原列的 `rv1-du1.md` 仅存在于已归档变更）」）。两处均在 delta 内（`rv-input-rv5.diff.log:8-16`、`:213-221`）且与磁盘一致。**未登记进 `Check Plan Changes`** ⇒ 见新发现 **RV5-WP5-F4**。 |
| F6 | 复核期间工作树被并发写，观测到含伪造 `candidate_commit` 与全零 sha256 的 premerge 草稿块 | **已解决（登记面）** | `verification.md:144`（Check Plan Changes 内）与 `:221`（Failures and Retests 新增 `PRO-5`：现象/处置/证据/后续/状态 CLOSED）如实登记，含「某广播式冻结期内不得为预演写入共享工作树」的教训。**占位值未进入提交**：全变更目录 grep `candidate_commit|sha256:0000|cc19ddc6e1b6d16a`，命中**只有** `verification.md:142/197/221` 的叙述与 `reports/rv4-candidate.md:93/96` 的 F6 描述；`verification.md` 内不存在任何 `agentic-premerge` 证据块（Merge History 仍为模板占位句 `:197`）。**未兑现的前瞻要求**（最终块只用真值）尚未发生，由本报告 §7 的前提承接。 |

---

## 2. `reports/candidate-verify-final.log` 证据链自洽性判定（要求 2）

**自洽（PASS），计数我全部独立复算，未采信转述。**

- **提交回显**：`:4` = `459579940c08c6f64a4cc23814a56b7c46c94230` ⇒ **等于目标提交** ✓；`:6` 的提交主题与 `4595799` 的定位（plan 形态修正）一致。
- **树干净**：`:7/:8` —— 命令标签后无输出行，`EXIT(git-status --porcelain)=0` ✓（与 `watchdog_diff` 的「无工作树改动」一致；**注意**这是「无输出 ⇒ 干净」而非带字面标记的转录）。
- **五项显式退出码齐备**：`:1290 EXIT(npm run verify)=0`、`:1294 EXIT(check-crate-boundaries)=0`、`:1457 EXIT(pv3-server)=0`、`:1570 EXIT(pv4-app)=0`、`:1597 EXIT(pv5-vendor)=0`（外加 `:8` 的 git 检查）。
- **[PV1] 计数（我逐行求和）**：`^test result:` 行在 `:79–:1288` 共 **82** 条；独立交叉核对目标数 = 70 条 `Running unittests/tests/…`（`:75–:1207`）+ 12 条 `Doc-tests`（`:1216–:1284`）= **82 targets** ✓；passed 逐条求和 = **718** ✓；failed 全 `0 failed` = **0** ✓；ignored 仅 `:1023`、`:1081` 各 1 ⇒ **2** ✓。与登记（`verification.md:75` 的 82/718/0/2）逐项一致。
- **[PV2]–[PV5] 计数**：PV2 `:1293 crate boundaries OK: 12 个 crate` ✓；PV3 7 targets / 117 passed（`:1391 89`、`:1411 14`、`:1423 6`、`:1433 4`、`:1439 0`、`:1449 4`、`:1455 0`）✓；PV4 6 targets / 72 passed（`:1515 50`、`:1521 0`、`:1528 1`、`:1545 11`、`:1562 10`、`:1568 0`）✓；PV5 vendor 11 passed（`:1589 11`）✓。
- **[PV2]–[PV5] 现为完整输出**（不再是节选）✓：PV3/PV4 含目标文件路径与逐条用例名（例 `:1300`、`:1463`），PV5 含 11 条用例名（`:1576-1586`）。
- **附带正向证据**：曾在候选轮 2a 变红的用例在**本候选**首轮 [PV1] 即为绿——`:302 test oversize_frame_ends_the_agent_and_fails_pending_requests ... ok`，同目标 `:308 test result: ok. 15 passed; 0 failed; … 7.06s`。
- **我无法核的部分**：文件 sha256（声明的 `443ef93c4aadde451e4c6e97d8866c2e86e147c125d6056e4bca52cd12de3075` 与四份 `alt-*.log:2` 头部的引用值一致，但我无 shell 不能重算）⇒ 列为需调度者执行。

## 3. PRO-4「5/5」独立工件判定（要求 3）

- **工件存在且内容合规**：`reports/agent-host-flaky-rerun.log`，5 次运行，每次 `oversize_frame_ends_the_agent_and_fails_pending_requests ... ok` + `1 passed; 0 failed; 0 ignored; 14 filtered out` + `EXIT(runN)=0`（N=1..5）✓。
- **提交标注**：头部 `:2` 写 `目标提交：459579940c08c6f64a4cc23814a56b7c46c94230`，时间 `2026-09-25T21:05:24+08:00` —— 即**跑在本轮最终候选上**，而不是 RV3 基红轮当时的候选（`c050c83`/`cc19ddc`）。
- **该关系必须说明**（我的结论：**需要**）：登记文本 `verification.md:138`/`:220` 描述的是 RV3 期的 5/5（各 0.15–0.17 s），与本工件（0.35/0.17/0.16/0.18/0.17 s）**不是同一批数字**，且两处都**没有引用**该工件、`:188` 却声称「PRO-4 改引该工件」。含混会造成两种错觉之一：「已改引」（事实未改）或「同一批」（数字不符）。最小说明：在 `:138` 与 PRO-4 行注明该工件是**在最终候选上重做的 5 次隔离重跑**（数字以工件为准），并把 Retest 列补上工件路径。

## 4. `plan.md` 形态修正是否对症（要求 4）

**对症，且正是 premerge 门的输入形态问题。**

- **形态**：`plan.md:693` 为单行 flow 序列 `alternative_checks: [ "…", "…", "…", "…" ]`，`trim` 后以 `[` 开头 ⇒ 命中 `workflow-check.mjs:62-64` 的 `YAML.parse` 分支 ⇒ 返回**恰好 4 条**字符串。
- **语义等价**：与 `rv-input-rv5.diff.log:17-28` 被删除的 4 条块序列条目**逐字相同、顺序相同、未增删检查项、未改弱描述**（我逐条比对：`[PV3]` server 行、`[PV4]` app 行、`[PV5]` Windows 行、`[PV1]/[PV2]` 门禁行）。`reason`/`basis`/`downgrade_approval` 在 delta 中为未改动的上下文行。
- **为什么必须改（源码依据，我自行核对）**：旧块序列形态下 `parseMainE2EFields` 把条目以 `', '` 拼成一行（`node_modules/@dongfanglin/openspec-agentic/src/e2e-check.mjs:89-102`），`plannedAlternativeChecks` 对**非 `[` 开头**的值按 `/[,，、]/` 切分（`workflow-check.mjs:60-67`）；而每条文本内部含大量 `、`/`，`，会被切成十余个碎片 ⇒ 永远无法与 receipt 的 4 个名字**逐项唯一对应**（`workflow-check.mjs:275-282` 的 `checks.length !== expected.length` / 名称集合比对）。改形后既满足 plan 阶段的非空判定，也让 premerge 阶段可逐项比对。
- **登记**：**未登记**。全变更目录 grep `alternative_checks` 只有 `plan.md:693`、`verification.md:210`（Main E2E 指向行）、`tasks.md:65/72`、`reports/rv4-candidate.md` 的历史引用；`Check Plan Changes`（`verification.md:107–144`）**没有**该改形的条目，也**没有** `plan.md:678` 证据列改写的条目（`plan.md:10` 明确要求「改变已冻结内容…写入 `verification.md` 的 Check Plan Changes」，且 2.27 已按此先例登记过 tasks.md 留证路径修正，见 `verification.md:132-134`）。⇒ 见 **RV5-WP5-F4**（应登记；这也直接支撑 `tasks.md:65` 的 6.5「未被实现期改动削弱」复核可审计）。

## 5. delta 纯度（要求 5）

**纯净（在供给工件与干净树的范围内）。**

- `rv-input-rv5.diff.log` 我**全文读完**（3 行头 + 4 个 `diff --git`；hunk 位置 `:8/:17` plan.md、`:30-35` 新增 `reports/rv4-candidate.md` 172 行、`:213` tasks.md、`:226/:237/:251` verification.md，文件止于 `## Final Assessment` 上下文）⇒ 文件集合 = **仅 4 个**：`plan.md`、`reports/rv4-candidate.md`（新增）、`tasks.md`、`verification.md`，**全部位于 `openspec/changes/daemon-cli-and-local-admin/` 内**。
- 无 `crates/**`、`docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`scripts/**`、`Cargo.toml/Cargo.lock`、`.gitignore`；`docs/**` 判据文本未动（verification.md 中提及 docs 的行均为既有内容或叙述）。与 `AGENTS.md` §8「不顺手改无关代码」不冲突（本轮确无代码改动）。
- 我把 delta 目标侧内容与磁盘逐 hunk 对齐（`plan.md:678/693`、`tasks.md:64`、`verification.md:141-144/187-192/221`、新增报告全文），无差异 ⇒ 与「worktree = 4595799」自洽。
- **局限**：我无 git，不能独立枚举该提交的文件清单；「4 个文件」的可信度建立在供给 diff + 干净树之上（需调度者以 `git show --stat` 复核，见 §8）。

## 6. 占位值残留检查（要求 6）

**证据/登记面零残留。** 全变更目录 grep `sha256:0000`、`cc19ddc6e1b6d16a`、`占位`、`TODO`、`TBD`：

- `sha256:0000…` / 伪造提交号 `cc19ddc6e1b6d16a…`：仅出现在 `reports/rv4-candidate.md:93/96`（F6 对事件的**描述**，非在用取值）；`verification.md`、`plan.md`、`tasks.md` 内**零命中**。
- `占位`：`design.md:9`（声明「不建目录之外的占位实现」）、`verification.md:144/221`（PRO-5 事件描述）、历史报告若干 —— 全为叙述，非字段值。
- `TODO` / `TBD`：变更目录内**零命中**。
- `verification.md` 内不存在 `agentic-premerge` 证据块（仅 `:142` 引述 RV4、`:197` 模板占位句、`:221` PRO-5 描述）⇒ 观测到的伪造块确实未进入目标修订。

---

## 7. 明确回答（要求 7）：随后「块 + 勾选 + 本报告」提交，本 PASS 是否仍适用？

**适用（有前提），但必须注意门的自指约束。** 内容意义上：本 PASS 的关键事实锚定在**被验证对象**——候选版本 `4595799` 的代码字节（我按 scope 未复检产品代码，RV2-WP4 已覆盖）与既有证据工件（PV1–PV5 全绿且计数可复算、flaky 重跑工件、4 份 `alt-*.log` 抽取件）。仅新增登记/报告/块**不改变被验证的代码与既有证据**，故结论不被推翻。

**前提条件（违反任一 ⇒ 需新 Review ID + 重跑）**：

1. 该提交只做这五类事：① 落盘本报告（`reports/rv5-candidate.md`）；② 勾选 `tasks.md` 6.3/6.4/6.5；③ 在 `verification.md` 补 Handoff Index `6.3`/`6.4` 行并改掉 `:75` 的悬空指针；④ 修正 §1 的 RV5-WP5-F1/F2 两处登记文本并补一条 `Check Plan Changes`（RV5-WP5-F4）；⑤ 写入 `agentic-premerge` 块。**不得**触碰 `crates/**`、`docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`scripts/**`、`Cargo.toml/Cargo.lock`，不得改 `plan.md` 的判据文本（形态/证据列已改完，不应再动）。
2. 不删除/不改写本轮 delta 已修好的行与结论（`plan.md:678`、`tasks.md:64`、`verification.md:74/75` 列值、`:121`、`:138`-`:139`、PRO-4 行），只做加法或等价澄清。
3. 块内**只用真值**：`candidate_commit`、`target_commit`、`contract_digest` 为实算值；`review.evidence`/`verify.evidence`/每个 `alternative_checks[].evidence` 的路径**必须在变更目录内且真实存在**（`workflow-check.mjs:212-226` 会逐个重算摘要并拒绝目录外路径），sha256 为**现算值**；`review.reviewer !== review.author`（`:229-234` 强制）。禁绝任何 `sha256:0000…` 与占位提交号（RV4-F6 的升级条件）。
4. **证据文件必须先冻结后哈希，且此后不得再追加**：`reports/candidate-verify-final.log`、`reports/agent-host-flaky-rerun.log`、`alt-pv*.log` 只要被追加，其 sha256 立即失效（`checkEvidence` 逐字节比对）。
5. **门的自指约束（我从扩展源码确认，务必先读）**：`inspectPremerge` 取 `candidate = git rev-parse HEAD`，并强制 `receipt.candidate_commit === candidate`（`workflow-check.mjs:205-209`），同时 `verify`/`review`/每个 `alternative_checks[]` 的 `candidate_commit` 也必须等于同一个 candidate，`target_commit` 必须等于 `refs/heads/main` 当前提交，且 `candidate !== target`、候选包含目标（`:210-212`）。⇒ 若把块写成 `candidate_commit: 4595799…` **然后把它提交**，新 HEAD ≠ 4595799，门会报「候选报告的提交与当前 HEAD 不一致」；而让块包含**自身**哈希在单次提交内不可能。可行次序：先在候选上提交「报告 + 勾选 + 登记（**不含块**）」得到 HEAD=X，再把块写成 `candidate_commit: X` 后**在 X 这个 HEAD 上**运行 `--stage premerge`，PASS 后再合入（或按项目惯例用临时 worktree/amend 完成自指）。关键是：**门的 PASS 必须发生在块声明的那一个 HEAD 上**，块不得指向一个"将要被新提交覆盖"的旧 HEAD。
6. 我**未**审阅块内新写文本、`6.3/6.4` 行文本与勾选后的 `tasks.md`（它们在本轮 target 版本里尚不存在）——本 PASS 不构成对这些**新增文本**的独立 review。若主 Agent 需要对新写文本本身有独立结论，请另派 RV6；若 follow-up 提交后发现任何与上述前提不符的内容（尤其块内值不实、sha256/路径错、或 `verification.md` 被再改），本 PASS 立即失效。

---

## 8. 新发现

#### RV5-WP5-F1 — P2（登记面；建议在写 `agentic-premerge` 块前修，成本 2–3 行）
- **位置**：`verification.md:188`（RV4-WP5-F2 处置行「**已处理**：重跑 5 次并落盘 `reports/agent-host-flaky-rerun.log`，**PRO-4 改引该工件**」）↔ `verification.md:220`（PRO-4 行仍写「证据 `reports/candidate-verify.log` 候选轮 2b 段」，未引用该工件）↔ `verification.md:138`（「各 0.15–0.17 s」）↔ `reports/agent-host-flaky-rerun.log`（0.35/0.17/0.16/0.18/0.17 s）。
- **事实**：变更目录内 `agent-host-flaky-rerun` 仅出现在 `:188` 这一行；「PRO-4 改引该工件」未发生；` :138` 的耗时区间与唯一存在的工件数字不符（5 次中 2 次落在区间外）。
- **预期**：处置列写的动作应与版本内事实一致；「可核的 flaky 刻画」应指向其工件（RV4 的最小修复②）。
- **影响**：`premerge` 阶段的读者若按 `:188` 检索会发现「已改引但无处可引」；PRO-4 判据②的解释在「同一批」与「新一批」之间含混（尽管实质证据更强：工件跑在最终候选 `4595799` 上，且 `candidate-verify-final.log:302` 显示该用例在候选首轮即绿）。
- **最小修复**：在 `:138` 与 PRO-4 行的 Retest 列补「最终候选上隔离重跑 5/5（`reports/agent-host-flaky-rerun.log`，耗时见工件）」，并把 `:188` 的「PRO-4 改引该工件」改为与之一致的表述。
- **严重度**：P2（我在 P1/P2 间权衡后定 P2：该声明只存在于 Review Findings 表、不影响门禁读到的证据，且修复是文本级；但请在写块前完成，否则 `verification.md:188` 会与块同期入库）。

#### RV5-WP5-F2 — P2（登记面）
- **位置**：`verification.md:189`（RV4-WP5-F3 处置行「**已处理**：补 6.3/6.4 行…」）↔ `verification.md:74-77`（Handoff Index 仅 6.1/6.2，后接 MD2/MD1）↔ `verification.md:75`（仍写「见 **6.3 行**」）↔ `tasks.md:63/64`（未勾选）。
- **事实**：目标版本内 6.3/6.4 行不存在，悬空指针仍在；该行 Recheck 列自述「随合并后记录一并落盘」。
- **预期**：处置列的「已处理」应只在动作完成后使用；延后应写在处置列而非只在 Recheck 列。
- **影响**：候选版本的「候选 PV 证据 → 修订」归属仍只能通过 6.2 行（`Target Revision` = `90c816a`）或悬空指针定位；RV3-F4 的第二半（行标与内容错位）在候选版本内未闭合。
- **最小修复**：在收尾提交内新增 `6.3`（`Target Revision` = `459579940c08c6f64a4cc23814a56b7c46c94230`、执行者 = 主 Agent、证据 = `reports/candidate-verify-final.log` + 现算 sha256、Result = PASS）与 `6.4`（reviewer 行，证据 = `reports/rv5-candidate.md`），并把 `:75` 的指针改为实际行号；若确实要延后，则把处置列改为「待收尾提交落盘（已登记）」。
- **严重度**：P2。

#### RV5-WP5-F3 — P2（报告级，**可不动**，仅要求块内路径正确）
- **位置**：`reports/rv4-candidate.md:145`（`report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv4-candidate.log`）↔ 磁盘仅有 `reports/rv4-candidate.md`（`ls`；且该 `.log` 路径命中 `.gitignore:22/27`）。
- **事实**：作为「原样落盘」的历史工件，其内嵌 `handoff_index` 仍指向被忽略且不存在的 `.log`；F4 的修复只改了落盘文件名。
- **预期**：门的 review 证据以 `agentic-premerge` 块内的 `review.evidence.path` 为准（`workflow-check.mjs:232-243` 只按块内路径解析），因此报告内部索引不应被当作门禁输入。
- **影响**：仅供人工按报告内索引检索时会落空；不影响门与证据链。
- **最小修复**：无需回改历史工件；在块的 `review.evidence.path` 明确写 `reports/rv5-candidate.md`（并（如需引用 RV4 时）写 `reports/rv4-candidate.md`）即可；若要消除歧义，可在 `Check Plan Changes` 或块内加一句「RV4 报告按 F4 以 `.md` 落盘，其内嵌 `handoff_index` 保留原文」。
- **严重度**：P2（报告级）。

#### RV5-WP5-F4 — P2（登记面，建议随收尾提交补一条）
- **位置**：`verification.md:107-144`（`Check Plan Changes` 无相关条目）↔ `plan.md:693`（`alternative_checks` 形态改自块序列，提交 `4595799`）↔ `plan.md:678`（RV1 证据列改写）↔ `plan.md:10`（「改变已冻结内容…写入 Check Plan Changes」）与 `verification.md:132-134`（2.27 对 tasks.md 留证路径修正的先例）。
- **事实**：两处 `plan.md` 改动均未进入 `Check Plan Changes`；`alternative_checks` 的形态修正在变更目录内**没有任何登记**（grep 仅命中 plan/verification/tasks 的引用与 RV4 报告）。
- **预期**：计划冻结字段在候选阶段被改写（哪怕是等价形态）应留下原/新值、原因与影响的登记；RV4-WP5-F5 的最小修复也明确要求按 2.27 先例登记 `plan.md:678`。
- **影响**：`tasks.md:65`（6.5）要核对四项「与 `plan.md` 一致且未被实现期改动削弱」，但 plan 字段在候选阶段被改写这件事无记录 ⇒ 6.5 的结论缺一条可审计依据；后续读者也无法得知该字段为何必须是 flow 序列（否则易被「整理」回块序列而重新弄红 premerge 门）。
- **最小修复**：在收尾提交内加一条 `Check Plan Changes` 条目（例：「2026-09-25（RV5 前，主 Agent）：RV1 行证据列改为按轮次累积的实际报告清单；`### Main E2E` 的 `alternative_checks` 改为单行 flow 序列（四条文本逐字不变）——原因：`@dongfanglin/openspec-agentic` 的 `plannedAlternativeChecks` 对非 `[` 开头的字段行按逗号切分，块序列经 `parseMainE2EFields` 拼接后会被条目内的 `、`/`，` 切碎，使 `premerge` 门无法逐项对应其 4 条 receipt」）。
- **严重度**：P2。

**附（非发现，仅告知）**：`candidate-verify-final.log:6` 显示提交 `4595799` 的标题为 `docs(app): 修正 premerge 门对 alternative_checks 的解析形态`——该提交只改规划文件，scope 用 `app` 属**不精确但不违规**（`app` 在 `commitlint.config.mjs:41` 的 `SCOPES` 内，CI `commits` 只校验词表）。历史不再改写，仅记录。

## Merge verdict: **OK with notes**
（RV4 六项：F1/F4/F5/F6 已解决，F2 工件已补齐但登记未同步，F3 已登记为延后落盘；候选证据链自洽、delta 纯净、无占位残留；4 条新发现全为 P2，其中 F1/F2/F4 建议在收尾提交内一并修掉。）

---

## 9. 需调度者执行的命令（我无 shell，全部**未执行**）

| # | 命令 | 目的 |
|---:|---|---|
| 1 | `git -C D:\Project\acp-remote rev-parse HEAD`（须 = `459579940c08c6f64a4cc23814a56b7c46c94230`）+ `git status --porcelain`（须空）+ `git log --oneline -2`（须见 `4595799` 与 `f9bc931`） | 冻结复核前提（我只有 `watchdog_diff` 的 HEAD 前缀 + 无工作树改动） |
| 2 | `git show --stat 4595799` 与 `git diff --stat cc19ddc..4595799`（须只有 `plan.md`、`reports/rv4-candidate.md`、`tasks.md`、`verification.md`，且无 `crates/`、`docs/`、`Cargo*`） | 独立确认 delta 纯度（我无法枚举提交文件清单） |
| 3 | `Get-FileHash` / `sha256sum`：`reports/candidate-verify-final.log` 须 = `443ef93c4aadde451e4c6e97d8866c2e86e147c125d6056e4bca52cd12de3075`（与该文件及 4 份 `alt-pv*.log:2` 的引用值一致）；并现算 `agent-host-flaky-rerun.log`、`alt-pv{1-2,3,4,5}*.log`、`rv4-candidate.md`、`rv5-candidate.md` 的 sha256 供块内使用 | 固定并核对 premerge 工件身份（我无法计算哈希） |
| 4 | `git diff main..HEAD -- crates/agent-host`（须空） | PRO-4 归因依据①（PRO-4 行自述，我未复核） |
| 5 | 采纳 §8 的 F1/F2/F4 文本修复后：`npm run check`（`check:docs` 对 §/链接敏感） | 合同门禁 |
| 6 | 写完 `agentic-premerge` 块后（且 HEAD == 块内 `candidate_commit`）：`npx --quiet --no-install openspec-agentic workflow check --change daemon-cli-and-local-admin --stage premerge --planning-root D:/Project/acp-remote --json` | 正式 premerge 门（依赖真实块；本报告只提供 REVIEW 证据） |
| 7 | CI-only（本地无等价物，**不得**记为本轮通过）：`deps`（cargo-deny）、`advisories`、`secrets`（gitleaks）、Linux `checks`（`#[cfg(unix)]` 路径） | 五个必需 job |

## 10. 未覆盖 / 无法确认声明

1. 我是结构只读子 Agent（只有 `read/grep/find/ls/watchdog_diff`），**未运行任何命令、未写任何文件**（包括本报告：`report_path` 按任务书给出，落盘由运行时/主 Agent 完成；我未创建 `reports/rv5-candidate.md`，`find **/rv5*` 亦确认其不存在）。
2. **不能计算/校验任何 sha256**：`443ef93c…`、`ca9ccb71…`、`5215a54c…` 及块内待写哈希均未复核（命令 3）。
3. **不能运行 git**：提交回显 / delta 文件清单 / `crates/agent-host` 空 diff（命令 1、2、4）均为「需调度者执行」。
4. 未重新检视产品代码（按 scope 由 RV2-WP4 覆盖；本轮 delta 不含 `crates/**`）；`[PV3]/[PV4]` 的语义正确性同样不在本轮范围。
5. 我不能解析 YAML：`plan.md:693` 的 flow 序列「解析为恰好 4 条」是**基于文本结构 + 扩展源码逻辑**的判定（无内部引号/反斜杠、4 个双引号标量、`,` 分隔），未用 YAML 解析器实跑。
6. `reports/**/*.log` 被 `.gitignore:22/27` 忽略、**不在任何提交里**：我只以磁盘工件身份读取并交叉核对，其「作用于某提交」依赖于工件内自述（本轮新工件已是真命令回显/带目标提交标注，强度高于 RV4 期）。
7. 长期未覆盖项不变：合并窗口端到端效果（本切片无 owned 会话）、真 TTY 凭据录入、真实设备 claim、平台 keystore 路径、Linux `#[cfg(unix)]` 路径；`agent-host` 该 flaky 用例在后续全量运行中仍可能再红（残余风险，非本变更引入，且本轮候选首轮 [PV1] 已绿）。
8. 时序风险：我读到的内容 = `4595799` 的树内容（`watchdog_diff` 为证）。**在我报告之后对 `verification.md`/`plan.md`/`tasks.md` 或证据工件的任何写入，都会使本报告的逐项判定与行号失效**，须按 §7 前提重新确认。

```yaml
handoff_index:
  task_id: "6.4 候选独立检视（最终轮）"
  role: reviewer
  phase: candidate recheck
  evidence_type: REVIEW
  evidence_id: RV5
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv5-candidate.md
  result: PASS
  evidence_status: NEW
  target_revision: 459579940c08c6f64a4cc23814a56b7c46c94230
  applicability_basis: >
    结构只读 reviewer（fresh，仅 read/grep/find/ls/watchdog_diff；watchdog_diff 报 HEAD=459579940c08 且工作树相对其无改动，
    故我读到的文件内容即 target 修订内容）。逐项复核 RV4-WP5-F1..F6：F1 已解决（candidate-verify-final.log:3-8 现为真命令回显
    ——rev-parse → 459579940c08c6f64a4cc23814a56b7c46c94230、log --oneline -1、porcelain 标签 + EXIT=0 且无输出行；
    :1296-1457 / :1459-1570 / :1572-1597 为 PV3/PV4/PV5 完整输出，含目标名与用例名）；F2 未解决（工件 reports/agent-host-flaky-rerun.log 已落盘：
    5 次 1 passed / EXIT(run1..5)=0、目标提交 4595799；但 verification.md:188 所称「PRO-4 改引该工件」未发生——PRO-4 行 :220 仍指向
    candidate-verify.log 2b 段，:138 的「各 0.15–0.17 s」与工件的 0.35/0.17/0.16/0.18/0.17 不符 ⇒ 新发现 RV5-WP5-F1，P2）；
    F3 未解决但已登记为延后（Handoff Index :74-77 只有 6.1/6.2 后接 MD2/MD1，:75「见 6.3 行」仍悬空，tasks.md:63/64 未勾选；
    :189 处置列已写「已处理」而 Recheck 列注「随合并后记录一并落盘」⇒ 新发现 RV5-WP5-F2，P2）；F4 已解决（reports/rv4-candidate.md 以
    new file mode 100644 被跟踪入库；残留 RV5-WP5-F3：报告内嵌 handoff_index report_path 仍指 rv4-candidate.log，P2 报告级）；
    F5 已解决（tasks.md:64 与 plan.md:678 改为按轮次累积的实际报告清单，磁盘与 delta 一致）；F6 已解决（登记面）：verification.md:144/221
    如实登记 PRO-5，全变更目录 grep 无 sha256:0000 / cc19ddc6e1b6d16a / TODO / TBD 的证据面残留，且 target 修订内不存在任何
    agentic-premerge 块（仅 :142/:197/:221 提及），伪造草稿块未入库。证据链判定：HEAD 回显=4595799、树干净、五项 EXIT(...)=0 齐备
    （:1290/:1294/:1457/:1570/:1597），PV1 我独立复算 82 targets（70 Running + 12 Doc-tests）/ 718 passed / 0 failed / 2 ignored
    （ignored 在 :1023/:1081），PV2 12 crate、PV3 7/117、PV4 6/72、PV5 11 与登记逐项一致；曾有红轮的用例在候选首轮即绿（:302）。
    plan.md 形态修正对症：:693 单行 flow 序列 trim 后以 [ 开头 ⇒ YAML 解析为恰好 4 条，文本与原 4 条逐字等价、不增删不减弱；
    源码依据 e2e-check.mjs:89-102 的 ', ' 拼接 + workflow-check.mjs:60-67 的 /[,，、]/ 切分会把旧块序列切碎，使门无法逐项对应；
    但该改形未登记进 Check Plan Changes（先例 2.27 见 verification.md:132-134）⇒ 新发现 RV5-WP5-F4，P2。delta 纯度：rv-input-rv5.diff.log
    全文读完，仅 plan.md / reports/rv4-candidate.md（新增）/ tasks.md / verification.md 四个文件，无 crates/docs/schemas/fixtures/scripts/Cargo*。
    无命令执行（无 shell）：HEAD/树、提交文件清单、crates/agent-host 空 diff、一切 sha256 与 premerge 门均列为需调度者执行。
    本 PASS 对随后「仅新增报告落盘 + 任务勾选 + 6.3/6.4 行 + F1/F2/F4 文本修复 + agentic-premerge 块」的提交继续适用，
    前提见报告 §7：不触代码与判据、块内只用真值、证据冻结后再哈希，且块所声明的 candidate_commit 必须等于运行门时的 HEAD
    （workflow-check.mjs:205-209），否则门会因「候选报告的提交与当前 HEAD 不一致」而失败；块内新写文本本身未被本轮 review 覆盖。
  source_evidence: NOT_APPLICABLE
```