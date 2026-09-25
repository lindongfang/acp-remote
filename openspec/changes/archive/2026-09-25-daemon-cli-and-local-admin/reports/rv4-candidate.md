# RV4-WP5 · 收尾复核报告（候选 cc19ddc；本报告按 F4 的建议以被跟踪的 .md 落盘）

> 由 reviewer 子 Agent 输出工件原样落盘（运行 b1482e8e），主 Agent 未改动结论。
> F6 如实记录了复核期间工作树被并发写（主 Agent 的 premerge 草稿块预演，已立即回退）：该风险由 reviewer 的 watchdog_diff 兜底，并已登记为 PRO-5。
> 本报告 result=PASS，适用性前提见其 §8；主 Agent 在修复 F1/F2/F3/F5 后以 RV5 复核新的候选提交。

result: **PASS**（`merge verdict: OK with notes`）—— RV3 的 P0 阻断项已消解，三条 P2 已按最小修复到位，delta 纯净；本轮新发现 6 条均为 P2（报告级/登记面），其中 F4/F6 需在**收尾提交与 premerge 块落盘前**处理。

## Review

### 1. 逐项复核（读 target 版本文件本身）

| # | RV3 发现 | 结论 | 依据（target 版本实测） |
|---|---|---|---|
| 1 | **RV3-WP5-F1（P0 阻断）**：候选最新轮 [PV1] 红且未登记 | **已解决** | ①红轮已登记：`verification.md:209` 新增 `PRO-4` 行（现象=`supervision.rs:422 oversize_frame_…` panic；复现/重试=同一提交重跑 [PV1] `EXIT=0` + 82 targets/718 passed/0 failed/2 ignored + 隔离重跑 5/5 + `reports/candidate-verify.log` 候选轮 2b 段；归因=`git diff main..HEAD -- crates/agent-host` 为空；残留风险与后续建议=留待后续变更加「有界等待」、本变更按 §8 不改）；`verification.md:138` 另有叙述性登记。②红轮原始证据在盘：`candidate-verify.log:3331`（`FAILED`）、`:3341`（panic）、`:3349`（`test result: FAILED. 14 passed; 1 failed; … 7.08s`）、`:3352`（`EXIT(npm run verify)=101`）。③全文件我逐一 grep：`candidate-verify.log` 中 `FAILED/panicked/error: test failed` **仅出现在 2a 段（3331–3351）**，2a 之后无任何失败痕迹，2b 段以 `:4732 EXIT(npm run verify)=0` 收束。④新工件 `candidate-verify-final.log` 存在且**五项检查各自有显式退出码**：`:1287 EXIT(npm run verify)=0`、`:1291 EXIT(check-crate-boundaries)=0`、`:1308 EXIT(pv3-server)=0`、`:1323 EXIT(pv4-app)=0`、`:1330 EXIT(pv5-vendor)=0`。⑤计数我独立复算（不采信转述）：PV1 段 82 条 `test result:` 行、逐条求和 **718 passed / 0 failed / 2 ignored**（ignored 在 `:1020`、`:1078`）；PV2 `12 个 crate`（`:1290`）；PV3 7 targets/117 passed（89+14+6+4+0+4+0，`:1295-1307`）；PV4 6 targets/72 passed（50+0+1+11+10+0，`:1312-1322`）；PV5 vendor 11 passed。与主 Agent 声称**逐项一致**。⑥该次运行包含曾失败用例转绿的直接证据：`candidate-verify-final.log:286/:299`（`Running tests\supervision.rs` → `test oversize_frame_ends_the_agent_and_fails_pending_requests ... ok`）。⑦归因旁证：`wp4wp5-final-verify.log:279/298`、`wp5-verify.log:313/332`（均 `15 passed; 0 failed`，7.08s/7.05s）与 `crates/agent-host/tests/supervision.rs:414-424` 的断言形状（`!supervisor.is_running()` 紧跟错误收敛后读取）一致，支持「测试侧时序竞态、非产品回归」。**残留**：见 F1/F2（工件不绑定版本、5/5 无独立日志）。 |
| 2 | **RV3-WP5-F2（P2）**：121 行「全仓零命中」范围不成立 | **已解决** | `verification.md:121` 现为「该条措辞的旧版本在 `docs/**`、`README.md`、`AGENTS.md` 内已零命中（……；任务书与已落盘的检视报告按『历史记录不回改』保留原文）」。我全仓 `grep 停周期任务` 复核：命中仅 `verification.md:68/121/172`、`tasks.md:34/46`、`reports/{rv1-wp4,rv1-wp5,rv3-candidate,wp425-handoff,wp427-handoff}` —— **`docs/**`、`README.md`、`AGENTS.md` 零命中**，且 `docs/CORE_PORTS_AND_STORAGE.md:1220` 现措辞为「以 §12.1 为准」的显式序列、`docs/MODULE_ARCHITECTURE.md:384` 与之同向。范围表述成立。**遗留措辞小瑕（不单独计为发现）**：括注里的命令写成不限范围的 `grep -rn 停周期任务`，照此执行会得到 12 处命中；与 `wp427-handoff.md:70` 的限定命令（`… docs README.md AGENTS.md`）不一致。若顺手，改括注为限定命令即可（改动后需重跑 `npm run check`）。 |
| 3 | **RV3-WP5-F3（P2）**：6.2 行工件行数/sha256 因追加日志失效 | **已解决（最小修复到位）** | `verification.md:75` 现标注 `reports/candidate-verify.log` 为「**追加型、含第 1 轮**：第 1 轮段 3033 行，`sha256=5215a54c…` 系追加前工件；文件含第 2 轮追加段后该 sha256 不再适用」，并指向新单文件 `reports/candidate-verify-final.log`；`verification.md:139` 登记该工件的范围与用途。阈值事实核对：`candidate-verify.log:3035` 起为候选轮 2，与「第 1 轮段 3033 行」自洽。**残留**：新工件本身的 sha256 尚未登记（拟在 premerge 块），且该工件若再被追加即重演 F3（见 F1 最小修复建议：一次成型后冻结）。 |
| 4 | **RV3-WP5-F4（P2）**：6.1/6.2 行 `Target Revision` 填运行 ID | **已解决** | `verification.md:74` = `ab62773（主分支基线；集成运行 6d132632）`；`:75` = `90c816a（第 1 轮候选；集成运行 6d132632）`。提交回到该列、运行 ID 退入文本，与同表 `3.8/3.10/5.1/5.2` 的行例一致。**未删改既有结论**：`rv-input-rv4.diff.log` 对 `verification.md` 只有 4 个 hunk（2 行替换 + 4 行新增 + 1 行新增），无删除行、无改写既有结论；新增文件只有 `reports/rv3-candidate.md`（上一轮 reviewer 报告原样落盘）。 |

### 2. 关键判定：候选验证证据链是否自洽、足以支撑 `premerge` 门？

**自洽且足以支撑（OK with notes）**，逐项核对结果：

- **候选提交声明**：`candidate-verify-final.log:2` 头部声明 `cc19ddc359aaf414f0b515139efceec5dfd0a5f4`；该值与我读到的 `rv-input-rv4.diff.log` 头部目标（`cc19ddc359aaf…`）以及 `watchdog_diff` 报告的 reviewer-launch HEAD（`cc19ddc359aa`）**一致** ⇒ 工件声明的提交就是被检视的修订。**但**该文件**没有** `git rev-parse HEAD` 回显（全文 `rev-parse` 0 命中），版本只存在于手写头注释里（见 F1）。
- **`git status --porcelain` 是否为空**：文件 `:5/:6` 是手写注释 `# git status --porcelain 输出（应为空）：` + `# EXIT(git-status-clean-check)=0`，**没有命令回显、也没有输出行**（空即代表干净，但与第 1 轮 `candidate-verify.log:3029-3030` 的真命令+真输出不同级）。旁证：`watchdog_diff` 报「No working-tree changes against reviewer-launch HEAD cc19ddc359aa」（覆盖被跟踪文件；忽略/未跟踪面我无法穷举，见 F6）。
- **五项显式退出码**：全在（§1 第 1 条 ④）。**但** [PV2]–[PV5] 段是**节选**（只有 `running N tests` / `test result:`，无 target 名与用例名；PV1 段含用例名），故它是「计数可核的汇总」而非完整原始转录——这不影响退出码与计数，但报告/门禁不应称其为「完整输出」。
- **PV1 计数与声称一致**：是（82/718/0/2，我独立复算；§1 第 1 条 ⑤）。
- **「红轮 2a + 重试绿 2b + 隔离重跑 5/5」的处置是否如实、是否把未验证写成已验证**：**总体如实**——红轮保留原始证据并被登记（未隐藏、未美化），重试绿有可核的 `EXIT=0` 与计数，失败归因给出了可检验的判据（① 分支未动 `crates/agent-host`；② 隔离 5/5；③ 三次全量运行均通过，我核到 `wp4wp5-final-verify.log`/`wp5-verify.log`/候选第 1 轮）。**一处越界**：判据 ② 的 **5/5（各 0.15–0.17 s）没有任何独立日志工件**（`reports/` 内无该批次输出，全仓也无 `--test supervision` 运行文本），属自述证据 ⇒ 见 F2。另 F1 属同一类「工件内容被描述得比实际强」的轻度形态，但方向是**描述超出工件**，不足以否定绿结论。

### 3. PRO-4 登记完整性判定（要求 3）

| 必备项 | 是否具备 | 内容/位置 |
|---|---|---|
| 现象 | ✅ | `verification.md:209`：`c050c83` 首跑 [PV1] `EXIT=101`，`supervision.rs:422` panic + 断言原文 |
| 复现/重试证据 | ✅（一处为自述） | 同一提交重跑 `EXIT=0`（82/718/0/2）+ 5/5 + 指定到 `candidate-verify.log` 2b 段；红轮原始证据在 `candidate-verify.log:3331-3352`。⚠️ 5/5 无工件（F2） |
| 归因依据 | ✅（声明，需调度者核实） | `git diff main..HEAD -- crates/agent-host` 为空 ⇒ 非本变更引入（我无 shell，未复核；旁证：`supervision.rs:414-424` 的竞态形状 + 冻结轮/2b 均绿） |
| 残留风险 + 后续建议 | ✅ | 「留待后续变更以『有界等待』修测试侧竞态；本变更内按 §8 不顺手改无关代码」 |
| 与 `AGENTS.md` §8 口径一致 | ✅ | 本 delta 不含任何 `crates/**` 改动（见 §4）；不在本变更内改无关 flaky 测试、只登记 + 建议，符合「不顺手重构无关代码」 |

补充口径正确性：PRO-4 把红轮归到 `c050c83`（当时的候选），而最终候选 `cc19ddc` 另有自己的全绿运行——两者并存不矛盾，且 `cc19ddc` 相对 `c050c83` 的 delta 只含登记/文档（见 §4），归因链自洽。

### 4. delta（`c050c83..cc19ddc`）纯度判定（要求 5）

- **只含登记/报告类改动**：`rv-input-rv4.diff.log`（173 行、全文读完）仅两项——`openspec/changes/daemon-cli-and-local-admin/reports/rv3-candidate.md`（新增 119 行，上一轮 reviewer 报告原样落盘）+ `verification.md`（4 hunk：6.1/6.2 行替换、121 行范围限定、新增「候选轮 2/RV3 处置」3 行、Failures and Retests 新增 `PRO-4` 行）。
- **无意外产品代码改动**：`crates/**`、`docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`scripts/**`、`Cargo*`、`plan.md`、`tasks.md` 均**未出现在 delta 中** ⇒ 与 `AGENTS.md` §8 一致。（`reports/**/*.log` 被 `.gitignore:22/27` 忽略，故候选验证日志本身不在 delta 内；我只能把磁盘文件当证据读，见 F1/F2 的限定。）
- **未把未落地能力写成已存在**：新增文本只涉及验证证据、flaky 处置与列值修正；`server::sync`/`node_link`/`acp_facade`/`node-link-client` 仍标注为后续切片，无新增能力声明。
- **登记面缺口（新发现 F3）**：delta **没有**为最终候选 `cc19ddc` 的 [PV1]–[PV5] 增加 `Handoff Index` 行；6.2 行改完列值后指向「6.3 行」，而该表**没有 6.3 行**（`tasks.md:63` 的 6.3 仍 `[ ]`）。

### 5. 新发现

#### RV4-WP5-F1 — P2（报告级，但请在写 premerge 块前处理）
- **位置**：`verification.md:139`（声称工件「含 `git rev-parse HEAD`、`git status --porcelain` 与逐项显式退出码」）↔ `reports/candidate-verify-final.log:1-8`（实际头部全是手写注释：`# 候选提交：cc19ddc359aa…`、`# git status --porcelain 输出（应为空）：`、`# EXIT(git-status-clean-check)=0`）。
- **事实**：全文 `rev-parse` **0 命中**；无 porcelain 命令回显（只有注释标签 + 空输出）；[PV2]–[PV5] 段为节选（`:1289-1331` 仅 `running N tests`/`test result:`，无 target/用例名，对比 PV1 段含用例名）。
- **预期**：登记文本与工件内容逐字相符；工件至少能自证「跑在哪个提交、树是否干净」。
- **影响**：premerge 证据块将按 sha256 pin 该文件；文件本身无法证明其运行版本与树状态，而 `verification.md` 的表述会让后续读者以为有命令级回显。属「声明强于工件」，尚不足以否定绿结论。
- **最小修复**：工件**尚未**被任何 sha256 固定（`verification.md` 内无该 hash 记录）⇒ 现在重生成一次（把 `git rev-parse HEAD` 与 `git status --porcelain` 真回显进去、其余部分不变），随后计算 sha256 再写 premerge 块；若选择不改工件，则把 `verification.md:139` 改为「头部以手工登记记录候选提交与干净性（非命令回显）」。
- **严重度**：P2（报告级；不阻断）。

#### RV4-WP5-F2 — P2
- **位置**：`verification.md:138`、`verification.md:209`（PRO-4 的「隔离重跑 5/5 通过（各 0.15–0.17 s）」）与 `candidate-verify.log:3451`（2b 段头重复该断言）。
- **事实**：`openspec/changes/daemon-cli-and-local-admin/reports/` 目录枚举中**无**该批次日志；全仓 grep 无 `--test supervision` 运行文本、无 `0.15s/0.16s/0.17s` 记录；唯一书面痕迹就是这两处自述。
- **预期**：RV3 §4 #2 要求的是可核的 flaky 刻画；「未运行/未留证」不应写成「5/5 通过」。
- **影响**：PRO-4 的「判据 ②」不可复核，归因只剩 ①（分支未动，需 git）+ ③（我已核到）。
- **最小修复**：把 5 次隔离运行输出追加到一个日志（如 `reports/agent-host-flaky-rerun.log`）并在 PRO-4 的 Retest 列引用；或把该判据改写为「由检查执行者自述，未单独留证」。
- **严重度**：P2。

#### RV4-WP5-F3 — P2（登记面，premerge 前建议补齐）
- **位置**：`verification.md:75`（6.2 行「……最终候选的单一工件见 `reports/candidate-verify-final.log`（自洽单文件，见 **6.3 行**）」）。
- **事实**：`Handoff Index` 只有 `6.1`（`:74`）与 `6.2`（`:75`）两行，**无 6.3 行**（`tasks.md:63` 6.3 仍 `[ ]`）⇒ 「见 6.3 行」是悬空指针；同时最终候选 `cc19ddc` 的 [PV1]–[PV5] 运行在整个表里**没有归属行**（6.2 行的 `Target Revision` 是 `90c816a`）。
- **预期**：门禁需要的「候选 Project Verify」证据应可被一行指向候选修订；表内引用必须可解析。
- **影响**：premerge 阶段按行检索证据会落到 90c816a 或悬空；RV3-F4 的第二半（行标与内容错位）未闭合。
- **最小修复**：新增 `6.3` 行（`Target Revision` = `cc19ddc359aaf414f0b515139efceec5dfd0a5f4`、执行者 = 主 Agent、证据 = `reports/candidate-verify-final.log` + sha256、Result = PASS），并在随后勾选 `tasks.md:63`；或把该指针改成实际可解析的目标。
- **严重度**：P2。

#### RV4-WP5-F4 — P2（影响收尾提交；成本≈0 的先手修复）
- **位置**：本轮任务/`handoff_index` 指定的报告路径 `openspec/changes/daemon-cli-and-local-admin/reports/rv4-candidate.log` ↔ `.gitignore:22`（`*.log`）与 `.gitignore:27`（`openspec/changes/**/reports/**/*.log`），且 `.gitignore:23-26` 明确写下策略「**不要把这类日志 `git add -f` 强行入库**」。
- **事实**：该路径匹配上述两条忽略规则；本变更既有 reviewer 报告都是**被跟踪的 `.md`**（`reports/rv1-wp1..wp5.md`、`rv2-wp4.md`、`rv2-wp5.md`、`rv3-candidate.md`，后者就在本轮 delta 里）；而依据本轮指令，premerge 块会把该路径 + sha256 写成 `review.evidence`。
- **预期**：门的 review 证据应能在合并提交/检出处取到，且不与仓库自定的忽略策略冲突。
- **影响**：若报告落成 `.log`，合并提交里将**没有**任何 RV4 检视工件；`verification.md` 的 premerge 块会引用一个本机才存在的文件。
- **最小修复**：把本报告落盘为 `reports/rv4-candidate.md`（与 `rv3-candidate.md` 同例，被跟踪），premerge 块的 `review.evidence.path` 指向它；若确实要用 `.log`，则在 premerge 块/`verification.md` 明写「该证据按 `.gitignore` 策略不入库、仅本机可核」。
- **严重度**：P2（不阻断本轮 PASS，但建议在写 premerge 块前定下来）。

#### RV4-WP5-F5 — P2（既有，勾选 6.4 时即触发）
- **位置**：`tasks.md:64`（任务 6.4 完成条件：「`openspec/changes/daemon-cli-and-local-admin/reports/rv1-du1.md` 记录隔离设置、版本与结论」）；`plan.md:678` 的 RV1 行证据列同样写 `…/reports/rv1-du1.md`。
- **事实**：全仓 `find **/rv1-du1.md` 只有 3 个**已归档**变更下有该文件；本变更目录与仓库根 `reports/`（`/reports/` 按 `.gitignore:32` 整体不入库、是历史遗留）都没有。实际产出的是 `reports/rv1-wp1..wp5.md`、`rv2-wp4.md`、`rv2-wp5.md`、`rv3-candidate.md`（以及本轮的 rv4 报告）；RV2-WP5 报告也曾就 `rv1-du1.md` 的路径归属登记过一条。
- **预期**：勾选 6.4 时，其完成条件引用的路径应存在或应被同步修正。
- **影响**：6.4 的「完成条件」无法按字面满足；另注意仓库根遗留 `reports/` 内有同名概念文件（`du1-main-verify.log`、`rv1-du1*.md`），易被误当作本变更工件（`tasks.md:67` 又要求 `…/reports/du1-main-verify.log`）。
- **最小修复**：勾选 6.4/6.5 时在同一提交内把 6.4 的完成条件路径改成实际报告（或把报告落到该名），并按 2.27 的先例把 `plan.md:678` 的路径修正登记进 `Check Plan Changes`（改动后重跑 `npm run check`）。
- **严重度**：P2。

#### RV4-WP5-F6 — P2（**非** target 版本内容；风险提示 + 需调度者确认）
- **位置**：`verification.md`（观测时出现的 `## Premerge 候选证据（agentic-premerge）` 块）。
- **事实（同一 Review ID 内的两次互不相容视图）**：会话中段我 `read` 到该文件含 premerge 块，其中 `candidate_commit: cc19ddc6e1b6d16a0d6f9b1b6b0a3a5f8d0e0e0`（与真实候选 `cc19ddc359aaf414f0b515139efceec5dfd0a5f4` **不同**）、`review.evidence ... sha256:0000…0`（全零）、`alternative_checks` 证据指向 `reports/alt-pv3-server.log` 等；`grep` 同时命中 `reports/alt-pv1-pv2-verify.log`（而此前 `ls` 未列出）。随后（多次、含 `watchdog_diff`）复核：`verification.md` 现为 215 行、止于 `## Final Assessment`，`Merge History` 仍为 `:186`（合入前记录唯一 agentic-premerge 块。），grep `candidate_commit|agentic-premerge|alt-pv` 在 `verification.md` 内**只剩 :186 一行**；`watchdog_diff` 报工作树相对 HEAD `cc19ddc359aa` 无改动；`ls reports/` 现在**列出** `alt-pv1-pv2-verify.log`、`alt-pv3-server.log`、`alt-pv4-app.log`、`alt-pv5-windows.log`。
- **预期**：复核期间候选应冻结；premerge 块只能填真实值（提交号、附件 sha256、真实存在的证据路径）。
- **影响**：①「工作树在复核期间被并发写」⇒ 我的版本绑定虽有 `watchdog_diff` 兜底（HEAD=cc19ddc359aa、树=HEAD），但前提不再由环境保证；②**若把上述观测到的块原样提交，premerge 门会带着一个伪造的 `candidate_commit` 与全零 sha256**，这正是「把未验证写成已验证」的最严重形态（该形态若落在 target 修订内我会判 P0；因它不在 `cc19ddc` 内，本轮按 P2 登记 + 升级提示）。
- **最小修复**：收尾提交内写 premerge 块时只用真值——`candidate_commit: cc19ddc359aaf414f0b515139efceec5dfd0a5f4`；`review.evidence` 与各 `alternative_checks.evidence` 的 sha256 由调度者现算；证据路径只用实际存在且确实由本轮产生的文件（`alt-pv*.log`、`alt-7.1-run.log` 是否为**本轮候选/最终**替代验证的产物需说明，7.1 的替代验证按 `tasks.md:72` 属合并后）；**不要**保留任何 `sha256:0000…` 占位，也不要用观测到的占位提交号。
- **严重度**：P2（不在本轮 target 修订内）；**若原样入库则升级为 P0**。

### 6. 需调度者执行的命令（我无 shell，全部未执行）

| # | 命令 | 目的 |
|---:|---|---|
| 1 | `git -C D:\Project\acp-remote rev-parse HEAD`（须 `cc19ddc359aaf414f0b515139efceec5dfd0a5f4`）+ `git status --porcelain`（须空）+ `git log --oneline -1` | 冻结复核前提（我的 `watchdog_diff` 仅给到 `cc19ddc359aa` + 无工作树改动；F6 提醒期间出现过并发写） |
| 2 | `git diff --stat c050c83..HEAD`（应仅 `reports/rv3-candidate.md` + `verification.md`）+ `git diff ab62773..HEAD -- crates/agent-host`（须空） | 复核 delta 纯度与 PRO-4 归因依据 ①（我只有供给的 diff 工件） |
| 3 | `Get-FileHash` / `sha256sum` `openspec/changes/daemon-cli-and-local-admin/reports/candidate-verify-final.log` → 须等于 `ca9ccb71a7c20815f4b3dfd2e191f7ebdfd10cb40fed56b425d29cb6e2dff4af`；写入 premerge 块**前**做，之后**不得再追加**该文件 | 固定 premerge 工件身份（我无法计算哈希） |
| 4 | 若要 F1 的命令级回显：重生成该工件（含 `git rev-parse HEAD`、`git status --porcelain` 真回显）+ 重算第 3 步的哈希 | 让工件自证版本与干净性 |
| 5 | `cargo test --locked -p agent-host --all-features --test supervision oversize_frame -- --nocapture` 连跑 5 次并追加到日志 | 给 F2 的 5/5 断言补工件 |
| 6 | 若采纳 F2/F3/F5 的文本改动：`npm run check`（`check:docs` 对 §/链接敏感） | 合同门禁 |
| 7 | CI-only（本地无等价物，**不得**记为本轮通过）：`deps`（`cargo-deny`：`rpassword`/`rtoolbox` 许可与来源）、`advisories`、`secrets`（gitleaks）、Linux `checks`（`#[cfg(unix)]` 路径） | 五个必需 job |
| 8 | `npx --quiet --no-install openspec-agentic workflow check --change daemon-cli-and-local-admin --stage premerge --planning-root D:/Project/acp-remote --json` | 正式门（依赖真实 premerge 块；本报告只提供 REVIEW 证据） |

### 7. 未覆盖 / 无法确认声明

1. 我是结构只读子 Agent（无 shell/git/cargo/npm），**未运行任何命令**；全部计数是我 `read`/`grep` 逐行求和（PV1 82/718/0/2、PV3 117/7、PV4 72/6、PV5 11、PV2 12 crate）。
2. 无法计算/验证任何 sha256（`ca9ccb71…`、`plan.md` 的合同摘要）；无法核算 6.2 行的「105 文件（A=86/M=19）」；无法运行 `git`（F1/§6 的命令 1、2）。
3. `candidate-verify.log` 2b 段的 82/718 计数我**未**逐行求和（grep 窗口限制）；决定性证据是 `:4732 EXIT(npm run verify)=0` 且 2a 之后全文件无失败痕迹。
4. `reports/**/*.log` 被 `.gitignore:22/27` 忽略、**不在任何提交里**：我以磁盘工件身份读取并交叉核对，其「作用于某提交」只有文件内自述（第 1 轮有真命令回显，2b/最终工件为手写头，见 F1）。
5. 未重新检视产品代码（按 scope 由 RV2-WP4 覆盖）；本轮 delta 亦不含产品代码。
6. 复核期间工作树出现过并发写（F6），故「我读到的即 target 版本内容」这一点由 `watchdog_diff`（HEAD=cc19ddc359aa、无工作树改动）与我读到的内容与 diff 目标侧逐 hunk 吻合共同支撑，而非环境保证。
7. 更早的长期未覆盖项不变：合并窗口端到端效果（本切片无 owned 会话）、真 TTY 凭据录入、真实设备 claim、平台 keystore 路径、Linux `#[cfg(unix)]` 路径；`agent-host` 该 flaky 用例在后续全量运行中仍可能再红（残余风险，非本变更引入）。

### 8. 明确回答（要求 6）：若随后仅新增「本报告自身的落盘」提交，本 PASS 是否仍适用？

**是（有条件）**——适用，但必须同时满足以下前提；违反任一即需新的 Review ID + 重跑：

1. 该提交相对 `cc19ddc` **只**做三类事：① 落盘本报告；② 勾选 `tasks.md` 的 6.3/6.4/6.5（如需 6.6）；③ 在 `verification.md` 新增 6.3/6.4 行与 `agentic-premerge` 块。**不得**触碰 `crates/**`、`docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`scripts/**`、`Cargo.toml/Cargo.lock`、`plan.md`（判据）。
2. 不删除、不改写本轮 delta 已修好的既有行与其结论（6.1/6.2 列值、`:121` 的范围限定、PRO-4 行）；如需修正措辞，只做加法或等价澄清。
3. premerge 块用**真值**：`candidate_commit` = `cc19ddc359aaf414f0b515139efceec5dfd0a5f4`；`review.evidence` 与非空 `alternative_checks` 的 sha256 均为现算值、路径均为实际存在且确为本轮产生的文件；**不得**出现 `sha256:0000…` 或观测到的占位提交号（F6）。
4. `candidate-verify-final.log` 在哈希登记前**不再追加**；若为 F1 重生成过，则用重生成后文件的哈希（顺序：生成 → 冻结 → 哈希 → 写块）。
5. 报告落盘后仍需让 premerge 门的证据可解析（F4：建议 `.md`；若用 `.log` 须登记「不入库、仅本机可核」）。
6. 兜底复核（调度者执行，成本低）：命令 1/2/3 的一条或全部——即「HEAD 仍是 `cc19ddc359aa` + `git diff --stat c050c83..HEAD` 只多出上述登记文件 + 第 3 步哈希与块内一致」。这三条绿且第 1 条前提成立时，本 PASS 结论对该后续提交继续有效。

理由：本轮通过的关键事实全部锚定在**候选源码内容与已冻结的代码版本**（`f39dda9`）上——[PV1]–[PV5] 全绿且计数可复算、红轮已登记且归因有据、delta 只含登记/文档类改动。仅新增登记/报告不会改变任何被验证的代码字节；反之，任何触及代码、文档判据或证据内容的提交都会使「该绿轮覆盖的对象」改变，须回到 Project Verify + 独立 review。

## Merge verdict: **OK with notes**
（RV3 的 P0 阻断已消解；6 条发现全为 P2。建议在写 `agentic-premerge` 块前处理 F1/F4/F6，并顺手补 F2/F3/F5 的登记。）

```yaml
handoff_index:
  task_id: "6.4 候选独立检视（收尾轮）"
  role: reviewer
  phase: candidate recheck
  evidence_type: REVIEW
  evidence_id: RV4
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv4-candidate.log
  result: PASS
  evidence_status: NEW
  base_revision: c050c83dbbe21a1b94bb09460e84810c5a157538
  target_revision: cc19ddc359aaf414f0b515139efceec5dfd0a5f4
  applicability_basis: >
    结构只读 reviewer（fresh，仅 read/grep/find/ls/watchdog_diff；watchdog_diff 报 HEAD=cc19ddc359aa 且工作树相对其无改动）
    逐项复核 RV3-WP5-F1..F4：F1（P0 阻断）已解决——红轮 2a 原始证据在 candidate-verify.log:3331-3352
    （EXIT(npm run verify)=101，supervision.rs:422 panic，14 passed/1 failed），已登入 verification.md:209 的 PRO-4
    （现象/重试证据/归因依据/残留风险与后续建议齐备，且与 AGENTS.md §8「不顺手改无关代码」一致，delta 不含 crates/**），
    2b 段以 candidate-verify.log:4732 EXIT=0 收束且 2a 之后全文件无失败痕迹；最终候选工件
    reports/candidate-verify-final.log 五检查各有显式 EXIT(...)=0（:1287/:1291/:1308/:1323/:1330），
    PV1 段我独立求和 82 test result 行 / 718 passed / 0 failed / 2 ignored（ignored 在 :1020/:1078），
    PV3 117/7、PV4 72/6、PV5 11、PV2 12 crate 与登记一致，且 :299 可见曾失败用例已 ok。F2 已解决
    （docs/**、README.md、AGENTS.md 内 `停周期任务` 零命中，我全仓复算）。F3 已解决（6.2 行标注追加型日志 +
    指向自洽单文件）。F4 已解决（6.1=ab62773、6.2=90c816a，运行 ID 退入文本），diff 未删改既有证据行。
    delta 仅 reports/rv3-candidate.md（新增）与 verification.md（4 hunk），无产品代码/判据改动。新发现 6 条 P2：
    F1 工件无 git rev-parse 回显而 verification.md:139 称其含之（PV2–PV5 亦为节选）；F2「隔离重跑 5/5
    （0.15–0.17s）」无独立日志工件；F3 6.2 行「见 6.3 行」悬空且最终候选 PV 运行无 Handoff Index 行；
    F4 指定报告路径 rv4-candidate.log 命中 .gitignore:22/27 且策略禁止 git add -f（建议落 .md）；
    F5 tasks.md:64 的完成条件路径 rv1-du1.md 不存在（仅 3 个归档变更有）；F6 复核期间出现互不相容视图：
    一次读到含 agentic-premerge 块的 253 行版本（candidate_commit 为伪造的 cc19ddc6e1b…、review/alt 证据全零
    sha256），随后多次复核为 215 行、无该块、Merge History 仍为占位（:186）——若该块原样入库则升级 P0。
    未执行任何命令（无 shell）：需调度者核实 HEAD/树、delta 纯度、agent-host diff 为空、工件 sha256
    （ca9ccb71…）并重算 premerge 块内一切哈希。PASS 对随后「仅新增本报告与登记/premerge 块」的提交仍适用，
    前提见报告 §8（不触代码与判据、块内只用真值、工件冻结后再哈希）。
  source_evidence: NOT_APPLICABLE
```