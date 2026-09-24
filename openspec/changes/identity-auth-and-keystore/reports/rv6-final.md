<!-- 由独立 reviewer 子 Agent 产出（RV6 定向确认轮，target revision 44b9815），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名；报告正文未改动）。 -->
# RV6 最终定向确认报告（reviewer 子 Agent，只读）

```
task_id: RV6 / rv6-final
role: reviewer（独立、只读；未继承实现对话）
phase: 最终定向确认（recheck，范围刻意最小）
agent_context: 新 reviewer 子 Agent；工具集 read/grep/ls/watchdog_diff；无 shell、无 git、未执行任何命令、未修改任何文件
target_revision: 44b9815（编排者口径：代码提交 3a247a5 + 一个只改 openspec/changes/identity-auth-and-keystore/*.md 的记录提交）；证据日志头部统一绑定 3a247a5
scope: 仅确认 RV5 的 1 条 P1（RV5-F1）与 5 条 P2 是否收口（不做新的全量检视）
changes: 无（只读；未写 progress.md，未改 tasks/verification/plan/代码）
result: FAIL（1 条 P1，纯文档/合同一致性；代码与用例零 P0/P1）
```

## ① 结论

**FAIL** —— `RV5-F1`（mutation 证据与 5 处引用互相矛盾）本身**已闭环**：`reports/rv5-mutation.log` 现含 14 条自洽条目、5 处引用全部改写为与该工件一致的「14 个 mutation」且变更目录内**再无任何地方断言「10 个 mutation」**（只剩对上一轮错误口径的**引述**）；但本轮声明的「合同 §4.1 与 design.md D3 已同步」**不成立**——`docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.1 的 `settle` 注释（:122-123）仍写「批准时把内存材料标记为『已批准』」，与本轮改后的实现、design D3、同一合同 §5.1（:308-309）**相反**，属 **P1（纯文档；产品代码/用例零 P0/P1）**。另登记 6 条 P2 级报告项（均为记录/文档，不阻断）。

## ② 逐条收口表

| 原问题 ID | 原级别 | 本轮结论 | 关键证据（本轮落点） |
| --- | --- | --- | --- |
| **RV5-F1**（`reports/rv4-mutation.log` 内容与 5 处引用写的「10 个 mutation」矛盾） | P1 | **已闭环**（引用口径层面）。旧行改为「已被下一行取代」并只留「当轮 mutation 检查见 `reports/rv4-mutation.log`」，不再带数量主张；新增 RV5/RV6 行把全部 mutation 主张改指 `reports/rv5-mutation.log`（14 条） | `verification.md:37`（旧行，证据列括注已改为「头部含当轮 revision 与完整命令」，不再写 `de61aa5`）；`verification.md:38`（新行「**14** 个 mutation 全部被对应用例捕获…汇总行 `CAUGHT=14 / NOT-CAUGHT=0`」）；`:45`（Check Plan Changes 第 2 条）；`:128`（Failures RV2 段的 m1–m14 逐条映射）；`:81`/`:82`（RV2-WP2/RV2-WP3 行改指 `rv5-mutation.log` 的 m1–m6 / m9–m14）；工件 `reports/rv5-mutation.log`（14 条 + `### 汇总：CAUGHT=14 / NOT-CAUGHT=0 / SKIP=0`） |
| **P2①**（`verification.md:37` 证据列 revision 绑错 + RV5 行 revision 写错） | P2 | **已修** | `verification.md:37` 括注已泛化为「头部含**当轮** revision 与完整命令」（不再点名 `de61aa5`），与本行 revision 列并列的 `de61aa5/0813ad5` 不再冲突；`verification.md:89`（RV5 行）revision 现为 **`eee409b`**（RV5 本轮 target）；另新增 `plan.md:21` 的 evidence 口径句（R1–R90 指向 WP 轮，最终收口轮等价证据 revision `3a247a5` 见 `verification.md`），补上 RV5 指出的「等价性依据」缺口 |
| **P2②**（`plan.md:812`/`tasks.md:53` 裸 `reports/rv1-du1.md`，本变更外有同名文件） | P2 | **已修（指定的两处）**；同类残留 1 处（见 P2-c 笔记） | `plan.md:815`（现为 RV1 行）末项已写 `openspec/changes/identity-auth-and-keystore/reports/rv1-du1.md`，同列 `rv1-wp1..4.md` 也全部为完整路径；`tasks.md:53` 同为完整路径；`tasks.md:12/26/36/56`、`plan.md:810/811` 的 `du1-pv1.log`/`du1-main-verify.log` 均为完整路径 |
| **P2③**（自检行把 13 处命中归给 `store.rs` 的 `mode_tests`） | P2 | **已修，且经我独立复算逐项吻合** | `verification.md:39`：17 处、`unix_modes`（`#[cfg(all(test, unix))]`）**7** 处 = `499/515/523/525/529/530/539`、`atomic_tests`（`#[cfg(test)]`）**6** 处 = `567/570/571/573/581/587`、`mode_tests` **0** 处；`reports/rv5-selfcheck-rg.log` 第 1 行命令已改为**可执行**的 `grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der"`，末行 `### 命中总数：17` |
| **P2④**（`rv3-auth` 的 5 条发现 `RV3-WP2-F2..F6` 零登记） | P2 | **已登记**（登记落在 RV5 行）；但其中 **F2 的合同一侧只部分落地** → 见 `P1-1` | `verification.md:89` 的 P2 单元格点名「`rv3-auth` 的 5 条发现（RV3-WP2-F2..F6）」，同一行 Resolution 单元格列出 F2（改由 `mark_pairing_approved` 在提交后置位）/F3（新增 `cache_evicts_the_earliest_expiring_challenge`）/F4（两处断言改传落定态记录 + `rejected_record`）/F5（§4.1 装配要求）/F6（§5.1 公开面）的处置；残留：`verification.md:84`（RV3-WP2 行）的 P2 单元格仍列 `rv3-docs` 的三项，未按 `RV3-WP2-F2..F6` 重写 |
| **P2⑤**（命令/口径不一致的措辞） | P2 | **部分修** | 已修部分：证据日志与记录行**两者一致且可执行**（`reports/rv5-selfcheck-rg.log:1` 与 `verification.md:39` 的命令等价、`\|` 为合法 BRE 交替符，照抄执行能复现 17 处；RV5 指出的旧日志 `[|]` 写法已随 `rv4-selfcheck-rg.log` 被本轮 `rv5-*` 取代）；残留：`plan.md:801` 的「审查重点自检」仍写 `rg -n "from_der|unwrap\(|expect\("`（工具为 `rg`、模式缺 `panic!`/`unreachable!` 两个 token），与执行口径仍是两种写法（见 P2-d 笔记） |

### 核对清单 A–F 的逐条结论

- **A. 收口判定**：见上表。`RV5-F1` = 已闭环；5 条 P2 = 4 条已修 + `P2⑤` 部分修；另 `P2④` 的 F2 合同侧未完全回写（构成 `P1-1` 的实质）。
- **B. `reports/...` 引用存在性**：`verification.md`/`plan.md`/`tasks.md` 中被引用的日志**全部存在**（含 `rv5-final.md`、`rv5-pv1..pv5.log`、`rv5-linux-clippy.log`、`rv5-mutation.log`、`rv5-selfcheck-rg.log`；变更目录 `reports/` 实读 **46** 个 `*.log`，与其一一对应）。**不存在**的引用只有 3 个，均属任务 6.x 未开始、且已写成完整路径（指向本变更目录）：`…/reports/du1-main-verify.log`（`plan.md:810`、`tasks.md:56`）、`…/reports/rv1-du1.md`（`plan.md:815`、`tasks.md:53`）、`reports/du1-integration.md`（`plan.md:793`，**仍为裸路径**，见 P2-c）。跨变更同名判定：`du1-pv1.log`/`du1-main-verify.log`/`rv1-wp1..4.md`/`rv1-du1.md` 均已写完整路径 ✓；仅 `du1-integration.md` 例外。
- **C. `rv5-mutation.log` 自洽性**：14 条条目**每条**都有「退化位置（标题 + `@@ 退化命令：… -> 文件`）+ 目标用例（`@@ 测试命令：…`）+ `RESULT=`」；`RESULT=CAUGHT` 恰 14 行，**无 `SKIP`**；末行汇总 `CAUGHT=14 / NOT-CAUGHT=0 / SKIP=0` 与条目数一致。我用只读检索**独立复算**了 `rv5-selfcheck-rg.log`：命中总数 **17**，逐行 `file:line` = `types.rs:256`、`entropy.rs:37`、`entropy.rs:38`、`ephemeral.rs:54`、`store.rs:499/515/523/525/529/530/539/567/570/571/573/581/587`，与日志**逐条相同**，也与 `verification.md:39` 的 3 + 14 分段一致。14 条的工具计数（20/26/2/3/10 等过滤器外数量）与 `rv5-pv3.log`/`rv5-pv4.log` 的二进制用例数一致。方法学缺陷见 P2-a（m4）。
- **D. 模块归属**：以 `crates/identity-keystore/src/store.rs` 的实际行范围判定——`mod mode_tests` = :459-480（`#[cfg(test)]` :458）**0 处命中**；`mod unix_modes` = :483-548（`#[cfg(all(test, unix))]` :482）命中 7 处（499/515/523/525/529/530/539）；`mod atomic_tests` = :551-605（`#[cfg(test)]` :550）命中 6 处（567/570/571/573/581/587）。`verification.md:39` 的归属与行号**全部正确**。（同行把它写作「482–549 行」，`549` 为该模块后的空行，属可忽略的 1 行宽松，不计为发现。）
- **E. `rv5-final.md` 归档与忠实性**：已落盘于变更目录 `openspec/changes/identity-auth-and-keystore/reports/rv5-final.md`；其结论（FAIL / `RV5-F1` P1「4 条 vs 10 条」+ 5 条 P2，产品代码零 P0/P1）与 `verification.md:89`、`:137` 的**引述一致**（未发现夸大或漏项；`verification.md:89` 的 P2 清单与 `rv5-final.md:24-32` 的 5 条 P2 一一对应）。
- **F. §5.1 公开面登记 vs 实际公开方法集**：`docs/IDENTITY_AND_AUTH_CONTRACT.md:303-313` 登记的集合 `reset_memory`/`node_public_key`/`now`/`local_node`/四个 `sign_*`/`mark_pairing_approved(&PairingId) -> bool`/`challenge_cache_len`/`failure_count`/`has_secret`/`MAX_CHALLENGES`/`complete_auth`（实现体 `pub(crate) fn complete`，`:311`）与源码**逐项相符**（`crates/identity-auth/src/authority.rs:51/56/61/97/108/119/130/145/153`、`pairing.rs:403/408/413`、`state.rs:22`、`handshake.rs:305`、`pairing.rs:428` `pub(crate)`）。配对/握手入口在 §4.1（:101-146）与 §5.1 的代码块（:264-269，含 `hello`/`verify_proof`/`complete_auth`）登记，「全部公开入口」这一说法在 §4.1+§5.1 合读的意义上成立（唯一未逐字登记的是构造子 `Authority::new`，属琐碎口径，不计为发现）。

## ③ 新发现的阻断项

1 条 P1；另 6 条 P2 报告级注记（不阻断）。

### P1-1（文档，阻断）：合同 §4.1 的 `settle` 注释仍写「批准时置位已批准」，与本轮实现/design D3/§5.1 相反

- **位置**：`docs/IDENTITY_AND_AUTH_CONTRACT.md:122-123`（§4.1「`[决定]`（2026-09-24 定型）配对入口的**已实现**形状」代码块内）：
  「// 落定：…；批准时把内存材料 / // 标记为「已批准」（只有它才可能成为 complete_auth 的消费目标）。」
  同一措辞残留在 `crates/identity-auth/src/pairing.rs:270-271`（`settle` 的 rustdoc 段落）。
- **证据（三处相互矛盾）**：
  - 实现：`crates/identity-auth/src/pairing.rs:303-304`「**不**在这里置位『已批准』…置位由 [`Authority::mark_pairing_approved`] 在调用方**提交成功之后**完成」，`:403-405` 新增 `mark_pairing_approved(&PairingId) -> bool`；`settle` 内**已无** `mark_approved` 调用（旧脚本 `target/fixrv5a.mjs` 的替换对可从「`self.state().mark_approved(pairing.id());` → 注释」读出，且该脚本对合同只改了 §4.1 的 `due_pairings` 注释与 §5.1 条目，**未**改 :122-123，也未改 `pairing.rs:270-271`）。
  - 本轮声明的同步对象：`design.md:72` 已正确写成「`settle` **不**在内存里置位『已批准』：置位只由 `Authority::mark_pairing_approved(&PairingId) -> bool` 在 `§11.6 的批准写集提交成功之后`完成」；合同 §5.1 `:308-309` 亦写「**调用方在 §11.6 提交成功后**置位」。
  - 因此**同一权威文档内部**（§4.1 :122-123 ↔ §5.1 :308-309）与**文档 ↔ 实现**（:122-123 ↔ `pairing.rs:303-304`）都不自洽，而本轮 RV5 行的 Resolution（`verification.md:89`）与 Failures 段（`:138`）都声称「合同 §4.1 + design D3 同步」。
- **影响**：§4.1 是 `identity-auth` 配对入口形状的权威来源（`AGENTS.md` §10）；按 :122-123 的读法实现切片 4–7 的 adapter 会省掉提交成功后的确认调用，结果是「库里已批准、内存未批准」——`complete_auth` 永不给出 `consume_pairing`（R54 正向路径失效，方向为 fail-closed，非提权）。这正是 RV3 把 `design.md:72` 判 P1 的同类问题（文档与实现相反），故本轮按 P1 处理；**不涉及任何代码缺陷**，实现与新增反向断言（`crates/identity-auth/tests/pairing.rs:700-707`）本身正确且已被 mutation m5 击穿（`reports/rv5-mutation.log` m5：`first_authentication_consumes_the_approved_pairing_once` FAILED，断言文本「批准写集尚未提交（未确认）时，内存不得超前」）。
- **最小修复**（同一改动内、共 2 处文本）：把 `docs/IDENTITY_AND_AUTH_CONTRACT.md:122-123` 改为「批准时**不**在 `settle` 内置位；置位由调用方在 §11.6 批准写集**提交成功后**调用 `mark_pairing_approved` 完成」，并把 `crates/identity-auth/src/pairing.rs:270-271` 的同句改为指向 `mark_pairing_approved`（其余不变）；随后按 `AGENTS.md` §10 重跑 `npm run check`（`check:docs`）并由新的隔离子 Agent 复核。

### P2 报告级注记（不阻断，按要求只登记事实）

- **P2-a｜`m4` 的 `RESULT=CAUGHT` 依据是编译错误，与本日志自述判据不符**：`reports/rv5-mutation.log:43-49` 的 m4 输出为 `error[E0599]: no method named has_secret found …`，却写 `RESULT=CAUGHT（用例失败，退化被捕获）`；而该日志第 4 行自述「判定为『被捕获』= **相应用例失败**」。生成脚本 `target/rv6-mutations.sh` 的判据是「cargo 退出码非 0 即 CAUGHT」（其 `grep` 过滤器本身也匹配 `error\[`），因此编译失败被计入 CAUGHT，末行 `CAUGHT=14` 里含 1 条未真正演示「用例可失败」的条目。影响面：其余 13 条均为真实 `test … FAILED`（含编排者点名的 m1/m5/m14：`reports/rv5-mutation.log` m1 :13-19、m5 :52-61、m14 :157-167，断言文本分别见「满缓存再签发必须淘汰最旧条目」「批准写集尚未提交…不得给出消费目标」「magic 被破坏：必须被拒绝为 EntryCorrupt」），故**不判阻断**。最小修复：修正 m4 的退化写法使其可编译并真失败，或把该条标为 `INVALID`（编译失败）并把汇总改为 `CAUGHT=13 / INVALID=1`，同时在记录中同步数字。
- **P2-b｜合同 §5.1 有成对重复的 `[决定]` 条目**：`docs/IDENTITY_AND_AUTH_CONTRACT.md:297-299` ≡ `:314-316`（`verify_proof` 快照主体一致性）、`:300-302` ≡ `:317-319`（诊断/测试入口）。**归因不可自证**（无 diff）：本轮脚本 `target/fixrv5a.mjs` 在该节替换的锚点是「另两个公开入口也在此登记…」这一条，重复块在其上下文中即已存在，故可能是更早轮次引入；按「不改任务状态、范围最小」原则仅登记，建议下一轮清理（纯冗余、无语义影响）。
- **P2-c｜跨变更同名残留 1 处**：`plan.md:793`「集成报告写入 `reports/du1-integration.md`」仍为裸路径，而仓库根 `D:\Project\acp-remote\reports\du1-integration.md` **确实存在**（另一变更的文件；`reports/` 根目录实读），歧义与 RV5-P2② 同类。建议改为 `openspec/changes/identity-auth-and-keystore/reports/du1-integration.md`。
- **P2-d｜`plan.md:801` 的自检命令与执行口径仍是两种写法**：`plan.md:801`（Local Checks）为 `rg -n "from_der|unwrap\(|expect\("`（模式缺 `panic!`/`unreachable!`），而 `verification.md:39` 与 `reports/rv5-selfcheck-rg.log:1` 为 5 token 的 `grep -rn`。两者都能执行，但「计划口径」与「记录口径」的覆盖集合不同；本轮的日志与记录行已互相可复现，故只判部分修。
- **P2-e｜RV6 行的 revision 归属与编排者口径不一致**：`verification.md:90` 写「`3a247a5`（RV6 收口提交；代码提交 `0059603` + `3a247a5`）」，而编排者给我的口径是「代码提交 = `3a247a5`，其后另有只改变更 `*.md` 的记录提交（本轮 target `44b9815`）」；变更目录内 `44b9815`/`0059603` **零命中**，全部证据日志头部均绑定 `3a247a5`。我无 git、无法自证哪一份表述正确，仅如实登记（建议由编排者用 `git rev-parse`/`git show --stat` 校正，并把「代码在 `3a247a5` 与 `44b9815` 之间未变」的等价性依据落到记录里）。
- **P2-f｜RV3-WP2 行的 P2 单元格仍是 `rv3-docs` 的项**：`verification.md:84` 的 P2 单元格列「日志头标的是父提交 `fe52694`、自检计数与日志不符、`authority.rs` 的 rustdoc 死链」，未写 `RV3-WP2-F2..F6`（这 5 条现登记在 `verification.md:89`）。属记录可读性/一致性 nit。

## ④ 未覆盖与不可确认项（照实声明）

1. **未执行任何命令**：`npm run verify`、`cargo test/clippy/fmt`、`node scripts/check-*.mjs`、mutation 复跑**均未由本 Agent 执行**（无 shell）。本报告的一切 PASS/exit 0/用例数/计数（如 `reports/rv5-pv1.log:29` `doc links OK: 369 relative links, 3733 section refs`、`:58` `Totals: 9 passed, 0 failed (9 items)`、`:992` `EXIT=0`、`reports/rv5-pv3.log` 的 15/20/26/2/9/5/2 与末 `EXIT=0`、`reports/rv5-pv4.log` 的 3/5/6/10/12 与末 `EXIT=0`、`reports/rv5-linux-clippy.log:7` `EXIT=0`）**一律转抄自既有日志文本**，退出码未经我复算。
2. **无提交区间 diff**：`watchdog_diff` 只有「工作区相对 reviewer-launch HEAD」的 delta，且**没有提交区间 diff**；因此「`44b9815` 的代码部分等于 `3a247a5`」这一前提**由编排者声明，我无法自证**（`44b9815` 在变更目录零命中）。本轮结论只对**当前工作区内容**负责。同理，`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的重复条目（P2-b）、`plan.md:793`（P2-c）等**无法归因到某一轮**，我只报告现状。
3. **Linux 运行时未覆盖**：`reports/rv5-linux-clippy.log` 只有 `Checking/Finished` + `EXIT=0`，不链接、不执行；`unix_modes` 的 0700/0600 真实断言、`platform/unsupported.rs` 的失败关闭运行时行为只能由 CI 的 Linux runner 真跑。
4. **CI 专属判定**：`cargo-deny`（advisory/许可证）与 `gitleaks`（密钥扫描）在本地无等价物，未在本地验证（工具版本与残余风险见 `docs/adr/0008-ci-supply-chain-tooling.md`）。
5. **未做新的全量检视**：按本轮最小范围，我只复核 RV5 的 1 P1 + 5 P2 及其证据链；契约/实现的其它正确性、E2E（`not-applicable`）、任务 6.x（DU1 集成与主分支验证，`Merge History` 与 `verification.md` 的 `Final Assessment` 仍为未开始/`BLOCKED`）均不在本轮范围；我也未更新任何任务状态或 `verification.md` 的 RV6 行（仍为「待填」，按要求不改写状态）。
6. **本轮 P1 的修复属于「必须先做、再复核」**：修改合同/rustdoc 后需按 `AGENTS.md` §10 重跑 `npm run check` 并由新的隔离子 Agent 复核；本 Agent 未做、也不能做该修改（只读交付）。
7. 证据边界：`target/*.mjs`、`target/*.sh`、`target/*.bak`、`target/rv*-commit.txt` 是编排者的工作区脚本/备份（在 `.gitignore` 内），我只把它们当作**归因证据**引用，未视为交付物，也不建议入库。

## ⑤ 证据清单（实际读过的文件）

- 变更记录：`openspec/changes/identity-auth-and-keystore/verification.md`（全文相关行）、`plan.md`（:21、:793、:801、:807-815 与 Coverage Index :692）、`tasks.md`（:12/26/31/33/35/36/37/51/53/56）、`design.md`（D3 :66-79、Risks :144）、`reports/rv5-final.md`（全文）、`reports/du1-pv1.log`（头部）
- 本轮证据工件：`reports/rv5-mutation.log`（全文）、`reports/rv5-selfcheck-rg.log`（全文）、`reports/rv5-pv1.log`、`reports/rv5-pv3.log`、`reports/rv5-pv4.log`、`reports/rv5-linux-clippy.log`、`reports/rv5-pv2.log`/`pv5.log`（头部行）、`reports/rv4-mutation.log`（全文，用于对照）、`reports/rv3-auth.md`（:87-127）、`reports/rv4-design.md`（:76/95/107）、变更目录 `reports/` 与仓库根 `D:\Project\acp-remote\reports\` 全量 `ls`
- 规范/合同：`docs/IDENTITY_AND_AUTH_CONTRACT.md`（§4.1 :96-145、§4.3 :196-205、§5.1 :245-320）
- 实现与测试：`crates/identity-auth/src/pairing.rs`（:255-330、:385-445）、`crates/identity-auth/tests/pairing.rs`（:591-731、:734-760）、`crates/identity-auth/tests/handshake.rs`（:626-717）、`crates/identity-keystore/src/store.rs`（:456-605）、`crates/identity-keystore/tests/fail_closed.rs`（:309-364）
- 归因用工作区脚本（非交付物）：`target/rv6-mutate.mjs`、`target/rv6-mutations.sh`、`target/fixrv5a.mjs`、`target/fixrv3c.mjs`、`target/fixdocs1.mjs`、`target/rv6-commit.txt`、`target/rv7-commit.txt`