# Independent Validation Round 2 — sync-workspaces-and-create

- 任务：`tasks.md` 6.1（`[validation]`）第二轮；目标 `refs/heads/main` @ `a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7`
- 验证者：validator-2（独立子 Agent；未参与任何实现、WP review 或第一轮验证）
- 隔离：只读。唯一写入为本文件。`git status --porcelain` 运行前为 `?? artifact.txt`、`?? openspec/changes/sync-workspaces-and-create/`；`git rev-parse HEAD` = `refs/heads/main` = `a6f6210…`
- 实跑命令：`npm run check`（`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-validator`）、`openspec-agentic workflow check --stage plan/premerge/final --json`、逐文件 sha256、门禁解析器复现脚本
- 第一轮报告 `reports/validation-sync-workspaces-and-create.md` 作为清单使用，但每条均重新独立核实，未采信其结论

## Verdict

**FAIL**

拆开说，与第一轮同构：

- **代码与合同面：仍 PASS。** `npm run check` 在目标提交实跑退出码 0，十项子门禁逐项绿（`schemas 127 valid/30 invalid/39 views`、`commands 13`、`errors 58`、`features 13`、`assets 17 schemas/170 fixtures/12 transcripts/20 negative`、`acp 71 rows`、`docs 409 links/9053 refs/511 md`、`boundaries 12 crates`、`drift §7 36 DDL + §5 15 traits/96 methods`、`agentic Totals: 20 passed, 0 failed (20 items)`）。收口常量 67/2/9/5 与 manifest 83 的关系未被本轮破坏。`refs/heads/main` == `HEAD` == `a6f6210`，`a6f6210` 与 `6c093f1` 均为其祖先，**无 tag**，`origin/main` 仍停在 `6c093f1`（**确未 push**，与 `reports/merge-U1.md` 的声明相符）。合入本身没有发现使主线不安全的不合格实现。
- **变更账本：仍 FAIL，且本轮新增两条「声称已修实则未修」的硬缺陷。** 主 Agent 声明修好的 15 项里，**F-02 未真正修复（阻断 `final` 门禁）、F-14 只修了一半（标题仍损坏、正文仍 5–8 次重复）、F-07/F-11/F-12/F-13 四项「已记入 `## Failures and Retests`」的声明为不实（该表只有 4 行 DPR 记录，没有任何 F-07/F-11/F-12/F-13 行）**。F-04 只修了一半：旧的 47 行空泛收尾已被撤回，但**被它推翻的 CR-2「六行」段落仍以现在时原样留在 `:168`/`:196`**，与新增的 16 行声明正面冲突。

**含义**：合并可保留（代码干净、收口常量正确、证据文件现已全部可读）；但**本变更当前不能被判 PASS、不能进 8.1、不能归档**。阻塞项是 2 条门禁错误 + 3 项账本矛盾，不是产品代码。

## Round 1 发现逐条复核

| 编号 | 是否已解决 | 独立证据 | 结论 |
| --- | --- | --- | --- |
| **F-01** MAJOR｜53 行覆盖索引证据文件不存在 | **已解决** | `sha256sum reports/PV1.log reports/candidate-verify-round2.log` → 两者同为 `c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05`，`ls -l` 同为 120578 字节，**逐字节相同**。内容核对：`PV1.log:1-3` 为 `> acp-remote-contracts@0.0.0 verify` / `> npm run check && npm run check:rust`，`:102` 为 `Totals: 19 passed, 0 failed (19 items)`，其后为逐二进制 `test result:` 段——确为候选的 `npm run verify` 输出（agentic 19/19 与 `## Candidate Builds` 段记录一致，与主分支 ALT1 的 20/20 是不同 markdown 集合的两次运行，可解释）。`workflow check --stage plan` 现报 `evidence: [{path: reports/PV1.log, sha256: c328ab04…}]`，不再报「证据不可读取」 | **RESOLVED**。遗留 MINOR：该日志**不含 commit 标识**（`grep -c a6f6210 PV1.log` = 0），53 行覆盖证据无法自证对应哪个提交 |
| **F-02** MAJOR｜`## Checks` ALT1/ALT2 行过不了门禁 | **未解决（阻断）** | 我按门禁源码复现了解析：`workflow-check.mjs:74` 用 `trimmed.split('|').slice(1,-1)` 切单元格；`verification.md:66/67/68` 三行**行尾均无收尾竖线**（实测 `pipes=8, endsWithPipe=NO, cells=7`，而表头 `verification.md:64` 为 `pipes=9, endsWithPipe=YES, cells=8`）。末格被 `slice(1,-1)` 丢弃后，`table.pick(row,'Evidence')` 对 PV1/ALT1/ALT2 三行均返回 `undefined`。`npm run check` 之外的直接证据：`workflow check --stage final` 仍逐字输出 `替代检查 ALT1 必须有证据路径`、`替代检查 ALT2 必须有证据路径` | **NOT RESOLVED**。主 Agent 改了 `Result` 格的文本形状（`PASS — …` 已能被 `/^\s*PASS\b/i` 匹配，这一半确实改对了），但**真正的失败原因是行尾缺竖线导致 Evidence 整格被丢**，与 Result 文本无关。修法：在 66/67/68 三行末尾补 `|` |
| **F-03** MAJOR｜WP3 执行者与台账矛盾 | **已解决** | `dispatch-queue.jsonl` 末条 WP3 记录为 `{"at":"2026-10-02T19:17:20.074Z","wp":"WP3","executor":"merger-1-r2","attempt":2,"state":"merged"}`；`verification.md:104` Dispatch Reconciliation 记 `WP3 \| 2 \| merger-1-r2 \| merged`，一致。`verification.md:36` 保留的辩解经复核成立：台账中 WP3 的 `ready-to-merge` 标记确由 `merger-1` 于 `15:44:22.809Z` 打上，`merged` 才由 `merger-1-r2` 打上，两者不矛盾 | **RESOLVED** |
| **F-04** MAJOR｜「无代码落点」申报不完整 | **部分解决** | 正向：旧的空泛收尾「其余 47 行覆盖索引由实现证据支撑」**已删除**（`grep -c "其余 47 行"` = 2，但两处都在 `:192`/`:202` 更正句的**引述**里——「先前「…其余 47 行有实现证据」的表述**不成立**」——属正当撤回）；新增 `:192`/`:202` 的 33/16/4 声明，列出的 16 行编号与声称一致（R6,R7,R8,R9,R10,R11,R12,R14,R17,R19,R20,R28,R29,R30,R31,R32 = 16 项 ✓），并点名 R53 为部分落地。<br>反向：`verification.md:168` 与 `:196` 的 **CR-2（INFO）原段落仍以现在时**断言「覆盖索引中 R9、R10、R11、R12、R14、R17 **六行**在本仓库没有代码落点 …… 其余行由实现证据支撑」，**未被删除**，与新增的 16 行声明正面冲突。且「16 行**无代码落点**」这一标签对其中 8 行过强（见下节） | **PARTIAL**。账本现在**同时**含两个互相矛盾的「无代码落点」口径（6 行 vs 16 行 vs 我的 8 行），读者无法判断哪个是现行结论——这正是 F-14「陈旧副本未清理」的同类缺陷 |
| **F-05** MINOR｜ALT2 通过数 | **已解决** | 我对 `reports/main-workspace-test.log` 逐行重算：`grep -c "^test result:"` = **92**，`passed` 求和 = **1092**，`failed` 求和 = **0**，`ignored` 求和 = **2**，`grep -c FAILED` = **0**。`verification.md:68` 现记「92 个 test result 行、1092 passed / 0 failed / 2 ignored」，与日志逐项吻合 | **RESOLVED** |
| **F-06** MINOR｜PV1 指向失败日志 | **已解决** | `verification.md:66` PV1 行现写「PASS（分两条记录；首跑 main-verify.log 因既有偶发 F-03 为红，绿证据另存）」，证据指向 `reports/main-alt1-check.log`。核实 `reports/main-verify.log:493` `test the_periodic_task_runs_again_after_one_full_cycle ... FAILED`、`:525` `test result: FAILED. 11 passed; 1 failed`（2.26s）、`:527` `error: test failed, to rerun pass -p app --test daemon_lifecycle`——确为失败运行，且已被行内文字正名为「红」 | **RESOLVED** |
| **F-07** MINOR｜`prototypes/IMPLEMENTATION-GAPS.md` 过期 | **未解决，且「已记录」的声明不实** | `prototypes/IMPLEMENTATION-GAPS.md` 仍存在（13325 字节，`git log` 显示自 `6c093f1 Feat/pwa prototype (#41)` 起未改），`:83` C1 仍断言 `owned_session` 无目录列 / `SessionSummary` 无 workspace，`:84` C2 仍断言 `session.create` 的 `transport` 仅 `["node_link"]`，`:86` C4 仍把「目录绝对路径是否下发」列为**未决产品决策**（design D1 已明确否决下发路径）。`grep "F-07\|IMPLEMENTATION-GAPS" verification.md` = 0 命中——**根本没有记进 `## Failures and Retests`** | **NOT RESOLVED**。这是本变更**自己推翻**的非权威文件，对下一个变更的作者有实质误导性 |
| **F-08** MINOR｜两条 Review 证据路径不可读 | **已解决** | `reports/wp1-review.md`（22016 B）与 `reports/wp4-review.md`（28118 B）现存在于变更目录。内容核对：`wp1-review.md:3` `HEAD：ceedba8896c7d6a35df504a252e4c113d80c8308（ceedba8 feat(sync): session.create 进入 Sync 词表族…）`、基线 `6c093f1`，与 `verification.md:25` 记的 WP1 REVIEW R1 完全对应；`wp4-review.md:2` `被检视版本：8af5d21`、基线 `347399f…`，与 `verification.md:116-119` 的 WP4-F-01/F-04/F-02/F-03 全部指向 `8af5d21` 对应。**确为对应提交的首轮报告** | **RESOLVED** |
| **F-09** MINOR｜DPR 跳过 Round 3 | **已解决** | `verification.md:57` 现登记 `reviewer-plan \| 3 \| sha256:8172eeac5a43630f3703d614de5065db55a3434afbe0579091c08f88e6e9fa3d \| FAIL \| reports/dependency-declaration-review-round3.md`，FAIL 结论与报告路径齐全，`reports/dependency-declaration-review-round3.md` 实测可读 | **RESOLVED** |
| **F-10** MINOR｜`reports/merge-U1.md` 未产出 | **已解决（内容成立）** | 文件已产出（8 行）。逐条比对：`refs/heads/main` 实测 = `a6f6210…`；`git merge-base --is-ancestor a6f6210 refs/heads/main` = YES；`origin/main` = `6c093f1` 且 `git merge-base --is-ancestor a6f6210 origin/main` 失败 → **确未 push**；`git tag` 空 → **确未 tag**；候选日志 sha 与报告所述 `c328abf0…` 相符；`candidate-review.md` 实测 `43bb3eefb94e4064ac1c61e131746a37f2089f195e4f72a50d0be00e8bb19045`，与报告所述相符。与 `verification.md:125` Merge History、`:209-238` receipt 自洽 | **RESOLVED** |
| **F-11** MINOR｜R53 三列同门缺直接断言 | **未解决（代码层已复核成立）** | 实现侧正确：`crates/core/src/broker.rs:1458` `if let Some(agent_session_id) = endpoint.agent_session_id().cloned()` 单一同门内含 `agent_session_id` / `workspace_cwd` / `workspace_alias` 三列（`:1469`）。测试侧仍缺：唯一断言「无 `agent_session_id` 时不写恢复列」的用例 `crates/core/src/use_cases.rs:2267 create_session_writes_no_recovery_columns_without_an_agent_session_id` 传入 `None` workspace（`:2274`），因此 `workspace_alias` 全程为 `None`，对「别名列是否在同门内」零判别力；而唯一断言别名落盘的用例 `crates/core/src/use_cases.rs:2139 create_session_persists_the_resolved_alias_next_to_the_canonical_path` 在 `:2140-2142` **先调了 `set_agent_session_id`**。「解析出合法别名 + `agent_session_id()` 为 `None`」这一组合**无任何测试** | **NOT RESOLVED**。裁定见「假设裁定 (c)」与「后续建议」：**不阻塞本变更，但必须记录**——而记录的声明不实（见下） |
| **F-12** MINOR｜文档 §11.3 与 `daemon.rs` 顺序矛盾 | **未解决（确认为既有问题）** | 复核成立：`crates/app/src/daemon.rs:1157 let composition = Composition::assemble(config, publisher);` 早于 `:1192 let mut lock = match DaemonLock::acquire(&lock_path)`，与 `docs/CORE_PORTS_AND_STORAGE.md:1490`「migration 在取得单实例锁后」矛盾。`verification.md:118`（WP4-F-02）已把它判为基线既有问题并声明不在本变更修复——该判断我独立复核**属实**。`grep -c "F-12" verification.md` = 0，未进 `## Failures and Retests` | **NOT RESOLVED（既有缺陷，处置判断正确）** |
| **F-13** MINOR｜`daemon_lifecycle.rs:648` 偶发失败 | **未解决（确认为既有问题）** | `verification.md:200` 已如实记录「两次命中既有偶发，连跑 4 次 3 过 1 挂，失败那次 2.3 秒 / 通过需 60 秒」。本轮 ALT2 日志该用例通过（1092 passed / 0 failed）。`grep -c "F-13" verification.md` = 0，未进 `## Failures and Retests` | **NOT RESOLVED（既有缺陷，披露正确）** |
| **F-14** INFO｜`verification.md` 整体复制两份 | **部分解决** | 正向：`grep -c "^## "` = 18，且 `^## Checks`、`^## Merge History`、`^## Candidate Builds`、`^## Independent Validation`、`^## Failures and Retests`、`^## Final Assessment` **各恰好 1 次**——整文件双份已消除（265 行 vs 第一轮的 518 行）。<br>反向：① 标题仍损坏：`verification.md:136` 原样为 `## Candidate Builds（候选构建经过## Candidate Builds（…×8，非合入记录）…`，整行 469 字节，`grep -o "Candidate Builds" \| wc -l` = **8**。② 正文仍重复：`:127-134` 同一句 blockquote **重复 8 次**；`候选 R1（` ×8、`候选 R2（` ×5、`CR-1（MINOR）` ×5、`合入结果` ×2、`覆盖索引诚实口径更正` ×2、`有实现证据的 33 行` ×2 | **PARTIAL**。「每节恰好一份」达标，**「无损坏标题」不达标**，正文重复仍在 |
| **F-15** INFO｜`## Independent Validation` 表为空 | **已解决** | `verification.md:250` 现有一行：`6.1 [validation] \| refs/heads/main@a6f6210… \| FAIL（第 1 轮：代码与合同面 PASS；账本层 4 项 MAJOR + 10 项 MINOR/INFO） \| reports/validation-sync-workspaces-and-create.md`。本轮结论待主 Agent 回填为第 2 行 | **RESOLVED** |

### 附：主 Agent 的两项「已记录」声明与实测不符

主 Agent 称 F-07/F-11/F-12/F-13「已作为显式后续行记入 `## Failures and Retests`」。实测 `verification.md:256-264` 的该表**只有 4 行**，全部是 DPR 相关（`DPR-plan-1`、`DPR-R2-ATTEMPT-1/2/3`），**没有任何 F-07 / F-11 / F-12 / F-13 行**。全文件 `grep "F-07\|F-11\|F-12\|F-13"` = 0 命中。该声明**不实**（INFO 编号确实在正文中以别的措辞出现，但「已记入 Failures and Retests」这一具体动作没有发生）。

### 附：新增的证据路径不一致

`plan.md:389-390` 与 `tasks.md:53-54` 声明 ALT1/ALT2 的证据为 `reports/ALT1.log` / `reports/ALT2.log`——**两文件均不存在**。实际证据被存为 `reports/main-alt1-check.log` / `reports/main-workspace-test.log`，且 `## Checks` 的 ALT1/ALT2 行指向后者（正确）。这是 F-01 同类的「声明路径与实际产物不同名」，第一轮曾判为「任务未执行属预期缺失」；现在任务**已经执行完毕**（`## Checks` 有 PASS 记录），该不一致变为实际缺陷。`workflow check` 读的是 `## Checks` 表，故当前不报此错，但读者按 `tasks.md` 指引会找不到文件。

## 覆盖索引口径复核

我的口径（与第一轮一致）：
- **实现落地** = 我独立核实了 Rust 代码落点 + 该落点由本变更引入或本变更直接依赖。
- **部分落地** = 该行行为存在真实的代码结构，但缺少把结构钉住的断言/测试，或只在传输无关的 core 层有落点而 Sync 面映射缺失。
- **仅 wire 合同层** = 只有 schema / fixture / 文档 / 枚举门禁，**全仓无任何 Rust 落点**。

前提事实（我独立复核）：`ls crates/server/src` → 仅 `lib.rs`、`local_admin/`、`node_link/`、`transport/`，**无 `sync` 模块**；`grep -rn "SnapshotResource::" crates/` 只命中 `crates/sync-protocol/src/sync.rs` 的枚举定义与解析侧，**无任何构造点**。故「服务端快照组装 / 目录资源下发 / feature 发送侧门控 / 按设备授权过滤 / Sync 面错误构造」在本变更中确无运行时。

**我的逐行结论：53 行 = 33 实现落地 / 12 部分落地 / 8 仅 wire 合同层。**

| 分类 | 行 | 数 |
| --- | --- | --- |
| 实现落地（33） | R1–R5、R15、R16、R18、R21、R22、R23、R24、R27、R33–R52（R53 除外） | 33 |
| 部分落地（12） | R13、R17、R19、R20、R25、R26、R28、R29、R30、R31、R32、R53 | 12 |
| 仅 wire 合同层（8） | R6、R7、R8、R9、R10、R11、R12、R14 | 8 |

关键落点（我亲自复核过的 load-bearing 项）：
- R19/R20｜`crates/core/src/use_cases.rs:259-262` `self.broker.authorize(actor, "session.create", None, &request_id)` 位于任何本机读取与文件系统访问**之前**；`crates/core/src/broker.rs:631-636` `Actor::Device { scopes, .. } => scopes.contains(command)` 按命令名实时判定；`crates/server/src/node_link/command.rs:619-636` grant 面强制。→ **有落地**，非「无代码落点」。
- R28/R29｜`crates/core/src/use_cases.rs:262-270`：别名未登记 → `PortError::InvalidRequest("workspace alias is not registered on this node")`；`resolve_workspace` 失败 → 不可用类。用例 `use_cases.rs:2767`、`:2782`。→ **有落地**。
- R30/R31/R32｜`crates/core/src/broker.rs:1373-1414`（accepted + 幂等行）、`:1559-1600`（`settle_session_create` 终态规则，含 `Uncertain`）、`:2586-2688`（`recover_command` 写 `command.uncertain`）；`crates/server/src/node_link/command.rs:663-688`（replay）、`:748`（projection 失败转 Uncertain）；用例 `crates/server/src/node_link/command/tests.rs:1792`、`:1865`。→ **有落地**。
- R17｜`crates/server/src/node_link/command.rs:2393-2401` 的错误构造只带字段名/参数名，**不回显路径**（不泄漏成立）；但无 Sync 面错误构造。→ 部分。
- R13｜`crates/sync-protocol/src/common.rs:445-466` 的 `Option` + `skip_serializing_if` 是类型级机制，`crates/sync-protocol/tests/body_constraints.rs:188-217` 测的是 wire 三态；无按协商结果省略的运行时。→ 部分。
- R25/R26｜`crates/core/src/broker.rs:631-636` 按命令名实时判定（新登记目录无需重新授权，结构上成立）、`:711 if export.revoked_at().is_some() || !export.scopes().contains(grant)`（撤销即失效）；但**无针对这两行的直接断言**（与 R53 同类）。→ 部分。
- R53｜`crates/core/src/broker.rs:1458-1470` 三列同门，**无直接断言**。→ 部分。

**与 `verification.md:192`/`:202` 新声明的比对：**

- ✅ **33 行实现落地** —— 与我的计数一致，成立。
- ⚠️ **4 行部分落地** —— 数量与**唯一被点名的 R53** 相符；但另外三行（我判为 R13、R25、R26）**未被点名**，读者无法核对。且「4 行」这个数字**偏低**：R13/R17/R19/R20/R25/R26/R28–R32/R53 共 12 行按我的口径都属部分落地。
- ❌ **16 行「无代码落点」** —— **偏高**。其中 **8 行（R17、R19、R20、R28、R29、R30、R31、R32）实际存在传输无关的 core 层代码落点**，部分还是本变更自己的 WP3 交付（`crates/core/src/broker.rs:1435-1470` 的别名持久化同门）。第一轮报告已明确指出「R19/R20/R28–R32 的底层能力在 core 存在，但 Sync 面的映射与投递不存在」；新声明把这个限定词丢了，变成「在本变更中**只冻结了 wire 合同**」，比第一轮更不准确。
- ⚠️ **分类判据未声明** —— 新声明没说清「实现落地」的门槛是「Sync 面」还是「传输无关的 core」。按「Sync 面」判，R24/R25/R26（同样是 Sync 面设备授权场景）与 R19/R20 应同类，却被分到两侧；按「core」判，R19/R20/R28–R32 就不该在 wire-only 桶。**两种判据都会得出与现在不同的边界。**

净判断：F-04 的方向修对了（撤回 47 行空泛收尾是真改进），但**新的口径既未声明判据、又高估了「无落点」的范围，且与残留的 CR-2「六行」段落并存**。相较第一轮，账本从「虚假」变为「不精确」，尚未变为「准确」。

## 假设裁定

| | 假设 | 第一轮结论 | 本轮裁定 | 依据 |
| --- | --- | --- | --- | --- |
| **(a)** | 设备级 `session.create` 授权覆盖「当时及此后新增的全部已登记 workspace 与已配置 Agent」 | PASS（文档侧成立，运行期无实现） | **维持 PASS（证据更充分）** | 判定的可实现性由 `crates/core/src/broker.rs:631-636` 的 `Actor::Device { scopes } => scopes.contains(command)` 提供：授权判定只比对命令名，**不存在按 workspace 枚举的授权清单**，因此「此后新登记的目录」在结构上自动落入既有 `session.create` scope，不需要重新授权；`docs/SECURITY_DESIGN.md` / `specs/sync-session-create/spec.md:39` 的措辞与该实现一致。**注意**：本假设成立不等于 R25/R26 有测试钉住（见覆盖索引节） |
| **(b)** | `workspace` 在任何 schema / fixture / 文档 / Rust 类型里都不携带规范化文件系统路径 | PASS | **维持 PASS** | 类型层 `crates/core/src/model/ids.rs:421-438` `WorkspaceRef` 仅有 `{alias, displayName}`，**结构上没有路径字段**；`crates/sync-protocol/src/common.rs:400-419` 同形；`crates/storage-sqlite/src/session_store.rs:57-68` 的 JOIN 取展示名、`:476-492` 取不到时回退别名本身；`crates/sync-protocol/tests/body_constraints.rs:127-182` 有路径泄漏拒绝用例 |
| **(c)** | 打包写入门（`workspace_alias` + `agent_session_id` + `workspace_cwd` 三列同时 `NULL`）四处一致 | PASS（实现一致、断言缺失） | **维持 PASS，但**「四处一致」的**证据强度**下调一档 | 实现确一致：`crates/core/src/broker.rs:1458` 单一 `if let Some(agent_session_id)` 同门内写三列（`:1469`）。但把 `workspace_alias` 移出同门（仍在同一次 commit 内写）后，`crates/core/src/use_cases.rs` 与 `crates/storage-sqlite/tests/workspace_alias.rs` **全部测试仍会绿**——因为唯一覆盖 `None` 分支的 `use_cases.rs:2267` 传的是 `None` workspace，唯一断言别名落盘的 `:2139` 先设了 `agent_session_id`。**即：这是 shipped 代码里一个真实且无网可捕的覆盖缺口（R53）**，不是文档缺陷。**裁定：不阻塞本变更**（实现正确、缺口是测试强度而非行为、且 R53 系设计负责人裁定产物），**但必须在 `## Failures and Retests` 留痕并转入后续变更**——目前未留痕 |
| **(d)** | 除 `verification.md` 记录的内容外，没有新增或改名任何错误码 / feature ID / 命令名 | PASS | **维持 PASS** | `npm run check:errors` 实跑 `error registry OK: 58 codes across 2 protocols`，与 ALT1 记录一致；`check:features` → `13 feature ids across 2 protocols`；`check:commands` → `13 commands`。`compatibility/errors/v1/errors.json:21` `authorization.scope_denied`、`:41` `internal.unavailable` 均在册。R28 spec 点名的 `authorization.scope_denied` 与实现侧 `PortError::InvalidRequest` 的差异属**Sync 面映射尚未实现**（见覆盖索引节 R28），不构成「新增/改名」 |

## final 阶段门禁

命令：`npx --quiet --no-install openspec-agentic workflow check --change sync-workspaces-and-create --stage final --planning-root D:/Project/acp-remote --json`，退出码 **1**。

逐字输出：

```json
{
  "result": "FAIL",
  "stage": "final",
  "contractDigest": "sha256:19bc702f1c78f625f6a048c5c37e0cb8ca1f822c9d00b71ecd3830657a5c4cfe",
  "requirementsDigest": "sha256:3b6c490b45bce5f7df4419a70a4b653da27f9e18fd31e7f811dde336057a6411",
  "targetCommit": null,
  "evidence": [
    {
      "path": "reports/PV1.log",
      "sha256": "sha256:c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05"
    }
  ],
  "errors": [
    "替代检查 ALT1 必须有证据路径",
    "替代检查 ALT2 必须有证据路径",
    "任务 1.1 未完成",
    "任务 1.2 未完成",
    "任务 1.3 未完成",
    "任务 1.4 未完成",
    "任务 1.5 未完成",
    "任务 2.1 未完成",
    "任务 2.2 未完成",
    "任务 2.3 未完成",
    "任务 2.4 未完成",
    "任务 3.1 未完成",
    "任务 3.2 未完成",
    "任务 3.3 未完成",
    "任务 3.4 未完成",
    "任务 3.5 未完成",
    "任务 4.1 未完成",
    "任务 4.2 未完成",
    "任务 5.1 未完成",
    "任务 5.2 未完成",
    "任务 5.3 未完成",
    "任务 5.4 未完成",
    "任务 5.5 未完成",
    "任务 5.6 未完成",
    "任务 5.7 未完成",
    "任务 5.8 未完成",
    "任务 6.1 未完成",
    "任务 7.1 未完成",
    "任务 7.2 未完成",
    "任务 7.3 未完成",
    "需要唯一的 agentic-assessment 代码块"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。"
}
```

**31 条错误的分类（我判断）**：

- **2 条是本变更自身的账本缺陷，会在进入 8.1 后持续阻塞**：ALT1 / ALT2「必须有证据路径」。根因已定位到 `verification.md:66/67/68` 三行**行尾缺收尾竖线**，导致门禁解析器（`node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:74` 的 `split('|').slice(1,-1)`）把 Evidence 整格丢弃。**这两条不会因为「任务打勾」而消失，必须先补竖线。**
- **28 条「任务 N.M 未完成」**：`tasks.md` 全部 29 个复选框**一个都没勾**（`grep -cE "^\s*- \[x\]" tasks.md` = 0，`- [ ]` = 29）。这是正常的工作流推进状态（任务要由主 Agent 随执行逐项回写），**不是缺陷**。
- **1 条「需要唯一的 agentic-assessment 代码块」**：8.1 的产物，**现在缺失是正常的**。

**其余阶段复核（均通过）**：
- `--stage plan`：**PASS**，`contractDigest` = `sha256:19bc702f…`、`requirementsDigest` = `sha256:3b6c490b…`，与我独立重算值及 `agentic-premerge` receipt（`verification.md:217-218`）**逐字节一致**。
- `--stage premerge`：**FAIL**（4 条：「候选提交必须有待合入的变更」「候选报告的目标基线已移动」「候选不是合入提交」「合入提交不在目标 refs/heads/main 上」）。这是**合入已完成后的正常现象**（`targetCommit` 已推进到 `a6f6210`），premerge 是合入**前**的门禁，事后重跑必然失败。**不构成缺陷**，但意味着 `verification.md:209` 记录的 receipt PASS 无法在当前仓库状态下被复现，只能凭台账与哈希链验证。
- `npm run check`：**退出码 0**，十项子门禁全绿（数字见「Verdict」）。
- receipt 自洽性：`## Merge History`（`:125`，M1，merger-1-r2，candidate = merged = `a6f6210`，fast-forward）与 `## Premerge History`（`:209-238`，target `6c093f1` → candidate `a6f6210`，PASS）一致；receipt 内 verify / ALT1 / ALT2 三处证据均为 `reports/candidate-verify-round2.log` + `sha256:c328ab04…`，review 为 `reports/candidate-review.md` + `sha256:43bb3eef…`——**我逐一重算磁盘文件，全部吻合**（`c328ab04…` ×2 见 PV1.log 与 candidate-verify-round2.log；`43bb3eef…` 见 candidate-review.md）。
- 证据可读性：`verification.md` 引用的全部 18 个 `reports/*` 路径**现已全部可读**（此前不可读的 `PV1.log` / `wp1-review.md` / `wp4-review.md` / `merge-U1.md` 均已补齐）。**全目录无任何记录在案的 sha256 与磁盘文件不匹配。**

## 新发现

1. **MAJOR｜F-02 未真正修复：`## Checks` 的 PV1/ALT1/ALT2 三行行尾缺收尾竖线，导致门禁丢弃整个 Evidence 单元格。**
   `verification.md:66/67/68` 实测 `pipes=8, endsWithPipe=NO`（表头 `:64` 为 `pipes=9, endsWithPipe=YES`）。门禁 `workflow-check.mjs:74` 用 `trimmed.split('|').slice(1,-1)` 切行，末格被 `slice(-1)` 丢掉。我用与门禁相同的算法复现解析，三行的 `Evidence` 列取值为 `undefined`。后果：`--stage final` 持续输出两条 `替代检查 ALT1/ALT2 必须有证据路径`，且**即使证据文件存在也无法通过**。主 Agent 声称的 F-02 修法（改 `Result` 文本形状）与失败原因无关——那一半改动本身是对的，但没触及真因。

2. **MAJOR｜`verification.md` 内同时存在两个互相矛盾的「无代码落点」口径。**
   `:168` 与 `:196` 的 CR-2（INFO）段落**仍以现在时**断言「R9、R10、R11、R12、R14、R17 **六行**在本仓库没有代码落点……它们不得被表述为"已实现"」；`:192` 与 `:202` 则断言「实际为 33 行有实现证据、**16 行**仅 wire 合同层」。两段都在 `## Candidate Builds` 节内、都未标注作废。且旧的空泛收尾「其余 47 行覆盖索引由实现证据支撑」在 `:192`/`:202` 被引用撤回，但 CR-2 六行段落没有被一起清理。读者（人或换解析实现的下游）无从判断现行口径。

3. **MAJOR｜主 Agent「F-07/F-11/F-12/F-13 已记入 `## Failures and Retests`」的声明不实。**
   `verification.md:256-264` 的该表只有 4 行，全部是 DPR 记录（`DPR-plan-1`、`DPR-R2-ATTEMPT-1/2/3`）。全文件 `grep "F-07\|F-11\|F-12\|F-13"` = 0 命中。四项已知缺陷**没有任何一项进入台账**，其中 F-11 是 shipped 代码的无网可捕覆盖缺口。这不是措辞问题：未登记的已知缺陷会在归档后失联。

4. **MINOR｜F-14 只修了一半：`## Candidate Builds` 标题仍损坏，正文仍 5–8 次重复。**
   `:136` 整行 469 字节，含 8 段嵌套的 `## Candidate Builds（候选构建经过…非合入记录）`。`:127-134` 同一句 blockquote 重复 8 次；`候选 R1（` ×8、`候选 R2（` ×5、`CR-1（MINOR）` ×5、`合入结果` ×2、`覆盖索引诚实口径更正` ×2、`有实现证据的 33 行` ×2。（`## <section>` 标题本身已做到各一份，文件从 518 行降到 265 行，方向对但没做完。）

5. **MINOR｜新的 33/16/4 覆盖口径未声明判据，且「16 行无代码落点」高估。**
   按传输无关的 core 层计，R17、R19、R20、R28、R29、R30、R31、R32 **有真实代码落点**（`crates/core/src/use_cases.rs:259-270`、`crates/core/src/broker.rs:1373-1414`/`:1559-1600`/`:2586-2688`、`crates/server/src/node_link/command.rs:619-688`/`:748`），缺的是 Sync 面映射——正是第一轮报告已写明、新声明却丢掉的那个限定词。真正零 Rust 落点的只有 8 行（R6–R12、R14）。另外「4 行部分落地」里只有 R53 被点名，其余三行无从核对。

6. **MINOR｜`plan.md:389-390` / `tasks.md:53-54` 声明的 ALT1/ALT2 证据文件 `reports/ALT1.log`、`reports/ALT2.log` 不存在。**
   实际产物名为 `reports/main-alt1-check.log` / `reports/main-workspace-test.log`（`## Checks` 行指向正确）。任务 7.1/7.2 实际已执行完毕，该不一致由「预期缺失」变为实际缺陷。

7. **MINOR｜覆盖证据 `reports/PV1.log` 不自证对应提交。**
   全部 53 行的 `evidence` 都指向它，但 `grep -c a6f6210 reports/PV1.log` = **0**——日志里没有任何 commit 标识。一个无法自证版本的日志被用作 53 行实现证据，是 F-01 的残留弱点（路径修好了，版本锚点仍缺）。

8. **INFO｜`prototypes/IMPLEMENTATION-GAPS.md` 仍与本变更的裁定冲突。**
   `:83` C1（`owned_session` 无目录列 / `SessionSummary` 无 workspace）、`:84` C2（`session.create` 的 `transport` 仅 `["node_link"]`）、`:86` C4（目录绝对路径是否下发为**未决**）三条均已被本变更推翻或由 design D1 裁定。文件自述非权威，但下一个变更的作者若据以切分任务会被直接误导。

9. **INFO｜`verification.md` 的 `## Final Assessment` 是一节空标题（`:265`，其后无内容）。**

## 后续建议

**归档前必须修（阻塞 8.1，但都不是产品代码问题）**

1. 给 `verification.md:66/67/68` 三行**末尾补上收尾竖线 `|`**。这是唯一能解掉 `--stage final` 那两条 `必须有证据路径` 的动作；改 `Result` 文本无效。修完请自己再跑一次 `workflow check --stage final`，确认两条消失。
2. 删掉 `verification.md:168` 与 `:196` 的 CR-2「六行」段落（连同其重复副本），只保留 33/16/4 的现行口径；或在其开头加「**已作废，见下方更正**」的显式标注。当前同一节内两个矛盾口径并存，是本次最容易被下游误读的一处。
3. 清理 `verification.md` 的 `## Candidate Builds` 标题（`:136`，8 段嵌套）与正文重复：`:127-134` 的 8 份 blockquote、`候选 R1/R2`、`CR-1（MINOR）`、`合入结果`、`覆盖索引诚实口径更正`、`有实现证据的 33 行` 各留一份。
4. 把 F-07 / F-11 / F-12 / F-13 **真的写进 `## Failures and Retests`**（当前该表只有 4 行 DPR）。F-11 请写明它是 R53 的无网可捕覆盖缺口、属 shipped 代码、处置为后续变更补断言——**不要因为「实现正确」就不登记**。
5. 修 `plan.md:389-390` 与 `tasks.md:53-54` 的证据路径，与实际文件名对齐（或把实际日志复制为声明名）。53 行覆盖证据同批处理时，顺手考虑给 `reports/PV1.log` 加一行 commit 锚点。
6. 回填 `## Independent Validation` 的第 2 行（本报告）。

**转后续变更（不阻塞本变更）**

7. **补 R53 的直接断言**：构造「已登记别名 + `agent_session_id()` 为 `None`」的会话创建，断言 `owned_session.workspace_alias` 为 `NULL`、且与 `workspace_cwd` 同为 `NULL`。我的判断是**不阻塞本变更**——实现（`broker.rs:1458-1470`）经独立复核正确，缺口在测试强度而非行为，且 R53 是设计负责人裁定产物；但它确实是 shipped 代码里一处无网可捕的改动点，应当进 backlog 而不是消失。
8. `prototypes/IMPLEMENTATION-GAPS.md` 的 C1/C2/C4 按本变更裁定更新或加「已过期，见 `openspec/changes/sync-workspaces-and-create/`」抬头。
9. F-12 / F-13 的代码侧修法（`CORE_PORTS_AND_STORAGE.md:1343` 承诺的「锁 + migration 之后、监听之前同步完成一次初清理」在 `crates/app/src/daemon.rs:1157`/`:1192` 并不成立）：独立变更处理，两条同源，宜一并修。

**流程建议**

10. 覆盖索引的判据请显式写进 `verification.md`（例如：「实现落地 = 传输无关层有 Rust 落点；仅 wire 合同层 = 全仓无任何 Rust 落点」）。目前「Sync 面」与「传输无关层」两种判据会得出不同边界，而 R19 与 R24、R27 与 R28–R29 这些成对行正被分到两侧——不声明判据，下一轮验证还会得出第三个数字。
11. `## Premerge History` 的 receipt PASS **无法在当前仓库状态复现**（`--stage premerge` 现在必然 FAIL，因为目标已推进到 `a6f6210`）。建议在 receipt 旁补一句「premerge 为合入前门禁，合入后必然不再通过；本 receipt 的可验证性依赖其记录的哈希链」，免得下一轮把它当成缺陷。