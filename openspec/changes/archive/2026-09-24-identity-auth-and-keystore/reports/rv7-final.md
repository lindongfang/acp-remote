<!-- 由独立 reviewer 子 Agent 产出（RV7 定向确认轮，target 代码 revision 69dd81f / launch HEAD 05bf0e2），主 Agent 原样落盘（仅歧义 `§` 引用补全被引文档名；报告正文未改动）。 -->
# RV7 最终定向确认报告（reviewer 子 Agent，只读）

```
task_id: RV7 / rv7-final
role: reviewer（独立、只读；未继承实现对话）
phase: 最终定向确认（recheck；范围刻意最小：只确认 RV6 的 1 条 P1 + 6 条 P2）
agent_context: 新 reviewer 子 Agent；工具集 read/grep/ls/watchdog_diff；无 shell、无 git、未执行任何命令、未修改任何文件
target: 代码 revision 69dd81f（全部证据日志头部自报）；reviewer-launch HEAD = 05bf0e2c7d2c（watchdog_diff 自报），工作区相对该 HEAD 零改动
scope: 仅 RV6-P1-1 + P2-a..P2-f 的收口确认；不做全量检视
changes: 无（只读；未写 progress.md，未改 tasks/verification/plan/代码）
result: PASS（RV6-P1-1 已闭环；无新增 P0/P1；残留 3 条 P2，均为文档/记录措辞）
```

## ① 结论

**PASS（本轮最小范围）**——**RV6-P1-1 已闭环**：`docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.1 的 `settle` 注释、`crates/identity-auth/src/pairing.rs` 的 `settle` rustdoc、同一合同 §5.1、`design.md` D3 与实现（`settle` / `mark_pairing_approved` / `complete` / `take_approved_secret`）**四处已经不矛盾**，且合同内再无「`settle` 置位/标记已批准」的残留（关键词 `已批准`/`mark_approved`/`mark_pairing_approved`/`置位` 全量核对）。RV6 的 6 条 P2 中 **4 条已闭环、2 条部分闭环**（P2-d 的修复行留下重复残片；P2-e 的记录提交哈希与同一文件另两处定义互斥），另有 **3 条同主题 P2 残留**（`state.rs:80`、`pairing.rs:74` 的旧口径；`plan.md` evidence 列的裸 `reports/...` 写法）。**没有任何阻断项、没有任何代码缺陷**。

## ② 逐条收口表（清单 A）

| 原问题 ID | 原级别 | 本轮结论 | 关键证据（文件:行号） |
| --- | --- | --- | --- |
| **RV6-P1-1**（合同 §4.1 `settle` 注释写「批准时置位已批准」，与实现/D3/§5.1 相反；`pairing.rs` rustdoc 同句未改） | P1 | **已闭环** | `docs/IDENTITY_AND_AUTH_CONTRACT.md:122-125`（「`settle` **不**在内存里置位「已批准」…由调用方在 §11.6 的批准写集**提交成功后**调用 `mark_pairing_approved`…design D3：内存态绝不超前于已提交状态」）；`crates/identity-auth/src/pairing.rs:270-272`（rustdoc 同句改为指向 `Authority::mark_pairing_approved`）；`pairing.rs:304-305`（`settle` 批准分支内注释同口径，**无** `mark_approved` 调用）；`state.rs:80-88`/`pairing.rs:404-406`（`mark_approved` 的唯一调用者是 `mark_pairing_approved`）；`handshake.rs:304-311`（`complete_auth` 无相关口径歧义）。合同内其余 `settle` 表述（`:129-135`、`:199-203`）不涉置位时机 |
| **P2-a**（`rv5-mutation.log` 的 m4 以编译错误记 `CAUGHT`，与本日志自述判据不符） | P2 | **已闭环** | 新证据工件 `reports/rv7-mutation.log`：判据在 `:4-5` 写明「输出里出现 `test result: FAILED` 或 `panicked at`；编译不过记 `INVALID` 不并入 `CAUGHT`」；m4（`:37-46`）输出为 `test unapproved_pairings_are_never_reported_as_consumption_targets ... FAILED` + `panicked at crates\identity-auth\tests\pairing.rs:790:5: assertion ... 未批准的配对不得被当作消费目标`；断言文本与 `crates/identity-auth/tests/pairing.rs:790-793` 逐字一致。**14 条 `RESULT=CAUGHT`（`:21/34/46/59/72/84/97/110/123/136/149/162/174/187`）每一条都能在其上方找到真实 `test result: FAILED`**，无一条为编译错误（无 `error[E…]`-only 条目）；汇总 `:189` = `CAUGHT=14 / NOT-CAUGHT=0 / INVALID=0 / SKIP=0`，与条目数一致；末尾 `:190` 有「工作区状态：0 个改动文件」 |
| **P2-b**（合同 §5.1 成对重复的 `[决定]` 条目） | P2 | **已闭环** | `docs/IDENTITY_AND_AUTH_CONTRACT.md` 中「快照主体 == 提交主体」只在 `:299-301` 出现一次；「诊断/测试入口属于公开 API 的一部分」只在 `:302-304` 出现一次；`grep '快照主体|诊断/测试入口'` 各 1 命中，原 `:314-319` 的重复块已删除 |
| **P2-c**（`plan.md` 的 `du1-integration.md` 为裸路径，仓库根有另一变更同名文件） | P2 | **已闭环** | `openspec/changes/identity-auth-and-keystore/plan.md:793` 现为完整路径 `openspec/changes/identity-auth-and-keystore/reports/du1-integration.md`（该文件尚不存在，属任务 6.x 待交付物，已如实写在完整路径下） |
| **P2-d**（自检命令与记录口径不一致：`rg` 且缺 `panic!`/`unreachable!`） | P2 | **部分闭环（P2 残留）** | 命令 token 已对齐：`plan.md:804` 内含 `grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der" crates/identity-auth/src crates/identity-keystore/src`，与 `verification.md:40` 的命令列、`reports/rv7-selfcheck-rg.log:1` 三处**命令部分一致**；**但同一行残留重复片段**：`plan.md:804` 实文为 `…crates/identity-keystore/src`" crates/identity-auth/src crates/identity-keystore/src`（**可失败路径**零命中…`（多出 `" crates/identity-auth/src crates/identity-keystore/src` 与一个多余反引号），因此「与日志第 1 行**逐字**一致」不成立。最小修复：删掉 `plan.md:804` 中的重复片段与多余引号/反引号（1 行文本、零语义影响） |
| **P2-e**（RV6 行 revision 归属与编排者口径不一致） | P2 | **部分闭环（P2 残留）** | 已落地部分：`verification.md:39` 写了「代码 `69dd81f`（工作区干净）…其后本行所在的记录提交只改 `openspec/changes/identity-auth-and-keystore/*.md`，代码未变——**由编排者声明，只读 reviewer 无法用 git 自证**」；`verification.md:92`（RV7 行）revision 写「`69dd81f` + 记录提交」。残留：`verification.md:39` 把该记录提交**具名成 `44b9815`**，而同一文件 `:91`、`:144` 与 `reports/rv6-final.md:9` 都把 `44b9815` 定义为「**代码 `3a247a5`** + 记录提交」（即上一轮的记录提交），三处互相矛盾；编排者口径中的 `05bf0e2` 在变更目录**零命中**（`grep '05bf0e2'` 无结果）。最小修复：把 `:39` 的 `44b9815` 换成实际记录提交哈希（编排者口径为 `05bf0e2`），或删去哈希只留「只改 `*.md` 的记录提交（由编排者声明）」 |
| **P2-f**（`verification.md` RV3-WP2 行的 P2 单元格未指向 `RV3-WP2-F2..F6`） | P2 | **已闭环** | `verification.md:85`（RV3-WP2 行）P2 单元格末括注现为「该行的 5 条代码/契约发现 `RV3-WP2-F2..F6` 已改记在 RV5 行，见下」；`verification.md:89` 的 P2 单元格与 Resolution 单元格逐条列出 F2–F6 处置 |

### 核对清单 A–F 的逐条结论

- **A. 收口判定**：见上表（1 条 P1 已闭环；6 条 P2 = 4 已闭环 + 2 部分闭环）。
- **B. 合同 ↔ 实现 §4.1/§4.3/§5.1 ↔ design D3 ↔ 实现**：**已一致**。逐句对照结果：合同 §4.1 `:122-125`（`settle` 不置位 → 调用方提交成功后调 `mark_pairing_approved`）、合同 §4.3 `:199-203`（secret 生命周期，只谈清除不谈置位，无冲突）、合同 §5.1 `:305-312`（`mark_pairing_approved(&PairingId) -> bool`「**调用方在 §11.6 提交成功后**置位」，并明确「见 §4.1 与 design D3」）、`design.md:72`（同口径）、`pairing.rs:270-272`/`:304-305`/`:398-406`/`:420-428`/`:435-439`、`state.rs:48-52`/`:91-96`、`handshake.rs:304-311`（`complete_auth` 只是 `complete` 的公开名，无相关口径）。**仍存在的互相矛盾句子有 2 处（均为 crate 内部注释，非权威合同）**：
  - `crates/identity-auth/src/state.rs:80`：「标记该配对已批准（**`settle(Approve)` 时置位**，不改变 secret 的保留期）。」——实际置位由 `mark_pairing_approved` → `mark_approved`（`pairing.rs:404-406`）在提交成功后完成，`settle` 内已无该调用。
  - `crates/identity-auth/src/pairing.rs:74`：「// 新建的配对本就未批准；批准由 **`settle(Approve)` 置位**。」——与同文件 `:270-272`/`:304-305` 相反。
  最小修复：把这两处的「`settle(Approve)` 时置位/置位」改为「由 `Authority::mark_pairing_approved` 在 §11.6 批准写集提交成功后置位」（各 1 行，纯注释，不影响行为；级别 P2）。
- **C. `rv7-mutation.log` 自洽性与判据遵守**：满足（见 P2-a 证据）。**独立复算** `reports/rv7-selfcheck-rg.log`：我以只读检索在源码上重跑同一模式，**命中总数 17**，逐行 `file:line` 与日志 `:5-21` **逐条相同** = `identity-auth/src/types.rs:256`、`identity-keystore/src/entropy.rs:37`、`:38`、`ephemeral.rs:54`、`store.rs:499/515/523/525/529/530/539`（7 处）+ `store.rs:567/570/571/573/581/587`（6 处）。模块归属亦独立复核：`store.rs:458-459` `#[cfg(test)] mod mode_tests`（**0 处**命中）、`:482-483` `#[cfg(all(test, unix))] mod unix_modes`（7 处命中，行号全部落在 484–549 体内）、`:550-551` `#[cfg(test)] mod atomic_tests`（6 处命中），与 `verification.md:40` 的分段一致。（同行把 `unix_modes` 写作「482–549 行」，把属性行与尾部空行算入，属 RV6 已判可忽略的 1 行宽松，本轮未变、不计为发现。）
- **D. `reports/...` 引用存在性**：`verification.md`/`plan.md`/`tasks.md` 中引用的 `rv7-*`、`rv6-final.md`、`rv5-*`、`rv4-*`、`rv3-*`、`rv2-*`、`bv-pv2..5`、`wp1..wp4-*`、`w0-*`、`du1-pv1.log` **全部实读存在**（变更目录 `reports/` 全量 `ls` 逐一对照）。**仍不存在的 3 个引用**（均为任务 6.x 未开始的待交付物，已写成指向本变更目录的完整路径）：`…/reports/du1-integration.md`（`plan.md:793`）、`…/reports/du1-main-verify.log`（`plan.md:810`、`tasks.md:56`）、`…/reports/rv1-du1.md`（`plan.md:815`、`tasks.md:53`）。**仍为裸路径的引用**：`plan.md:34-723`（Coverage Index 各 R 行的 `evidence:` 字段，如 `reports/wp2-identity-auth-pairing.log`）与 `plan.md:811-814`（Project Verify 的 PV2/PV3/PV4/PV5 evidence 列）仍沿用旧写法；这些名字在仓库根 `reports/` **无同名文件**（根目录只有另一变更的 `du1-*`/`verify-*`/`rv1-*` 等），故无 RV5-P2②/ RV6-P2-c 那类歧义，仅写法不统一（P2 级观察，无必须动作）。RV6 点名的那一处（`du1-integration.md`）已修。
- **E. `reports/rv6-final.md` 归档与忠实性**：已落盘于变更目录（`openspec/changes/identity-auth-and-keystore/reports/rv6-final.md`）；其结论（**FAIL**；`RV5-F1` 已闭环；新 P1-1 = 合同 §4.1 `:122-123` 的 `settle` 注释与实现 `pairing.rs:303-304`、design D3、合同 §5.1 `:308-309` 四处不一致；另 6 条 P2 = m4 判据、§5.1 重复条目、`plan.md:793` 裸路径、`plan.md:801` 自检口径、RV6 行 revision、RV3-WP2 行 P2 单元格）与 `verification.md:91`/`:144` 的引述**逐项一致，未发现夸大或漏项**（未重做 RV6 的检视）。
- **F. RV7 行与「声称已修但未修」的断言**：`verification.md:92` 仍为占位（Resolution/Evidence 列 = 「待 RV7 结果填入 / 待填 / 待填」），符合预期。整份记录中未发现「声称已修但实际未修」的**断言**：`verification.md:144`/`:145`/`:146` 的 RV7 收口陈述逐条与现状相符（唯一两处措辞过宽者为 P2-d 的「与记录/日志一致」与 P2-e 的哈希，已在上表登记为部分闭环）；对历史错误口径（「10 个 mutation」）的**引述**只出现在历史报告 `rv3-*`/`rv4-*`/`rv5-final.md` 与 `verification.md` 的历史行中，不构成本轮断言。

## ③ 新发现的阻断项

**无 P0/P1 阻断项。** 本轮仅登记 3 条 **P2**（均非阻断、纯文本）：

1. **P2-新-1（同 B 清单）**：`crates/identity-auth/src/state.rs:80` 与 `crates/identity-auth/src/pairing.rs:74` 仍写「`settle(Approve)` 时置位/置位已批准」，与同文件已修正的 `:270-272`/`:304-305` 相反。最小修复：两处改为「由 `Authority::mark_pairing_approved` 在提交成功后置位」。**注**：这两处不在 RV7 声明的修复面内（`verification.md:145` 只声明合同 §4.1 与 `pairing.rs:270-272`），因此**不构成**「声称已修但未修」；属本轮 B 清单逐句对照时新发现的历史残留。
2. **P2-新-2**：`plan.md:804` 行内重复片段（`…/src`" crates/identity-auth/src crates/identity-keystore/src``）——即 P2-d 的修复残片，最小修复见上表。
3. **P2-新-3**：`verification.md:39` 的记录提交哈希 `44b9815` 与同一文件 `:91`/`:144` 及 `rv6-final.md:9` 的定义互斥（`05bf0e2` 零命中）——即 P2-e 的残留，最小修复见上表。

## ④ 未覆盖与不可确认项（照实声明）

1. **未执行任何命令**：`npm run verify`、`cargo test/clippy/fmt`、`node scripts/check-*.mjs`、mutation 复跑、`openspec*` 全部**未由本 Agent 执行**（无 shell）。本报告的一切 exit 0、用例数、计数（如 `rv7-pv1.log:992 EXIT=0`、`:58 Totals: 9 passed`、`:29 doc links OK: 369 relative links, 3746 section refs`；`rv7-pv3.log:133 EXIT=0` 与 15/20/26/2/9/5/2；`rv7-pv4.log:79 EXIT=0` 与 3/5/6/10/12；`rv7-pv5.log:41 EXIT=0`；`rv7-linux-clippy.log:8 EXIT=0`；`rv7-pv2.log:5-6`）**一律转抄自既有日志文本**，退出码与用例结论未经我复算（我独立复算的只有 `rv7-selfcheck-rg.log` 的 17 处命中与 mutation 日志的内部一致性）。
2. **无提交区间 diff**：`watchdog_diff` 只给「工作区相对 reviewer-launch HEAD」的 delta，自报 `HEAD 05bf0e2c7d2c` 且**工作区零改动**；它**不提供提交区间 diff**。因此「`05bf0e2` 的代码部分等于 `69dd81f`」**由编排者声明，我无法用 git 自证**（`05bf0e2` 在变更目录零命中，`69dd81f` 只出现在日志头部与记录行）。本轮结论只对**当前工作区内容**负责；对历史轮次的归因（如「§5.1 重复条目由哪一轮引入」「`state.rs:80` 何时成为旧口径」）**不可自证**，我只报告现状。
3. **Linux 运行时未覆盖**：`reports/rv7-linux-clippy.log` 只有 `Checking/Finished` + `EXIT=0`，**不链接、不执行**；`unix_modes` 的 0700/0600 真实断言与不可用平台的失败关闭运行时行为只能在 **CI 的 Linux runner** 上真跑。
4. **CI 专属判定**：`cargo-deny`（advisory/许可证）与 `gitleaks`（密钥扫描）在本地无等价物，未在本地验证（工具版本与残余风险见 `docs/adr/0008-ci-supply-chain-tooling.md`）。
5. **本轮不做全量检视**：只复核 RV6 的 1 P1 + 6 P2 及其证据链；契约/实现的其它正确性、P2 之外的代码质量、任务 **6.x**（DU1 集成、候选/主分支验证）、**7.x**（替代验证）、**E2E**（`not-applicable`）均**不在范围**（`tasks.md:50-63` 全部仍未勾选，`verification.md` 的 `Merge History` 与 `Final Assessment` 仍为「尚未开始」）。我未改动 `verification.md` 的 RV7 行，也未更新任何任务状态。
6. **未写入任何文件**：本报告是唯一交付物；`progress.md` 与其它记录文件未创建/未修改（只读约束优先）。
7. 证据边界：`target/*.mjs`、`target/*.sh`、`target/*.bak`、`target/rv*-commit.txt` 是编排者的工作区脚本/备份（`.gitignore` 内），我只把它们当**归因参考**（本轮未依赖它们得出任何结论），不视为交付物。

## ⑤ 实际读过的文件清单

- 记录与规范：`openspec/changes/identity-auth-and-keystore/verification.md`（:1-60、:61-92 相关行、:93-152）、`plan.md`（:21、:34-723 抽样、:785-829）、`tasks.md`（:12-56、:50-63）、`design.md`（D3 :66-79）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（§4.1 :90-205、§4.3、§5.1 :245-340）
- RV6/RV7 工件：`reports/rv6-final.md`（全文）、`reports/rv5-final.md`（片段）、`reports/rv7-mutation.log`（全文）、`reports/rv7-selfcheck-rg.log`（全文）、`reports/rv7-pv1..pv5.log`、`reports/rv7-linux-clippy.log`（头部/结果行/mutation 行检索）、`reports/rv4-design.md`/`rv3-docs.md`（历史引述比对）、变更目录 `reports/` 与仓库根 `D:\Project\acp-remote\reports\` 全量 `ls`
- 实现与测试：`crates/identity-auth/src/pairing.rs`（:60-90、:255-330、:385-455）、`src/state.rs`（:40-100）、`src/handshake.rs`（:300-315）、`src/types.rs`（:250-260、:775-785）、`src/authority.rs`（关键词检索）、`crates/identity-auth/tests/pairing.rs`（:683-707、:786-798）、`crates/identity-keystore/src/store.rs`（命中行与测试模块边界）、`src/ephemeral.rs`、`src/entropy.rs`（命中行）

---

**Merge verdict: OK with notes** —— RV6-P1-1 已闭环、无新 P0/P1；3 条 P2 残留（`state.rs:80`+`pairing.rs:74` 旧注释、`plan.md:804` 行内残片、`verification.md:39` 的记录提交哈希）建议就地清理（各 1 行文本），不阻断。