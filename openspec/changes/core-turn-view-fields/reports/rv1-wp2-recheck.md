## RV1-REC：仍有未闭合/新发现问题 7 条（全部 MINOR；另 4 条 SUGGESTION）

**总判断**：RV1 的**实质发现已闭合**（F1、F4、F5、B1、B2、S1、S2、S4、死代码，均真修+真可证伪，我用 7 条变异自检逐条证伪成功），修复批**没有改任何生产行为**。剩余 7 条全部是「记录/登记与事实不符」类问题：RV1-WP1-F2/F3 的**登记声明不成立或没有落点**（2 条），以及修复批自身在 `verification.md`/`tasks.md`/`docs` 里留下的 5 处**记录不准**（含 1 处新引入的悬空引用 `§6.9`）。无 BLOCKER、无 MAJOR。

检视输入：base `f582376`（RV1 被检版本）→ target `27c9155`；工作区干净（`git status --porcelain` 空，`git diff --stat HEAD` 空）；`HEAD=27c9155`。本轮独立执行：`cargo test -p core`（94 passed）、`-p agent-host --test view_contract`（1 passed）、`-p storage-sqlite --test session_version_rule`（3 passed）、`cargo clippy -p storage-sqlite -D warnings`（exit 0）、`node scripts/check-doc-links.mjs`（exit 0）、7 条变异（全部真跑、全部红、全部 `git checkout --` 还原）。

---

## 逐条核对

### 1. RV1-WP1-F1（晚到 delta 无归属 → 不注入）：已闭合 ✅

- **三处登记均在**：`tasks.md` 2.3（「实测：存在一条——turn 终结后晚到的 §10.3 类型事件无权威归属…由用例 `a_late_delta_after_turn_end_is_persisted_without_attribution` 固定」）、`docs/CORE_PORTS_AND_STORAGE.md:754`（新增「**无归属的降级**」条目：不注入、`turn_id` 与 view 同时为 NULL、宁可缺字段也不伪造、必须有用例固定并留给 Sync 切片裁定）、`verification.md:20`（PV2 scope 列「无归属降级」）+ `:62`（F1 行，Resolution 与事实一致）。
- **用例真的存在**：`crates/core/src/broker.rs:6882` `a_late_delta_after_turn_end_is_persisted_without_attribution`（在 `#[cfg(test)] mod tests`，起始 `broker.rs:4929`），断言 `world.event_turn(&late.id) == None`（`:6912`）与 `stored_view == view` 逐字节（`:6916-6921`）；`event_turn` 的语义是「落库时的 turn 归属」（`broker.rs:3433-3439`，同族正向断言见 `:6460/6470`），因此不是恒 `None` 的空断言。
- **会因「注入了一个 turnId」而变红（我实测）**：变异 M2b —— 把 `finalize_owned_views` 的无归属分支改成注入固定 turnId（`broker.rs:2528` 的 `let Some(turn) = … else { continue };` → match + 兜底 `TurnId::new("…00ff")`）→

  ```
  panicked at crates\core\src\broker.rs:6917:9:
  assertion `left == right` failed: 无归属时 view 逐字节不变（不注入）
    left: "{\"turnId\":\"00000000-0000-4000-8000-0000000000ff\",\"messageId\":\"…614\",\"deltaIndex\":\"0\",\"text\":\"late\"}"
   right: "{\"messageId\":\"…614\",\"deltaIndex\":\"0\",\"text\":\"late\"}"
  ```

  另用断言反向变异 M2（`assert!(…contains("turnId"))`）同样变红。该用例**证伪的是降级本身，不是恒真**。

### 2. RV1-WP1-F2/F3（登记为非阻断的既有问题）：没有顺手改 ✅ / **登记声明不成立** ❌

- **零生产改动已核实**：`GIT_PAGER=cat git --no-pager diff f582376..27c9155 -- crates/core/src/broker.rs` **只有一个 hunk**：`@@ -6877,4 +6877,112 @@ mod tests {`，即全部新增在 `#[cfg(test)] mod tests`（`broker.rs:4929` 起）内；`view_command_completed`（`broker.rs:2962-2974`）与 `apply_state`（`broker.rs:2288`）逐行未动（`git diff … | grep -c view_command_completed` 无命中）。grep 复核该函数仍是 `format!(r#"{{"turnId":"{text}"}}"#)`（`:2967`）——F3 描述的既有缺陷确实未被顺手改。
- **F2 的登记声明不成立（新发现 M-2）**：`verification.md:63` 写「已在 `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条与 `verification.md` 登记」；但 `grep -n "变更前\|mode/config\|提交前版本" docs/CORE_PORTS_AND_STORAGE.md` **零命中**，§6 第 19 条（`:750-754`）只写「取值等于该次提交后的会话版本」。「mode/config 两次提交 ⇒ 注入的是变更前版本」这条语义**只存在于 `verification.md` 的 Review Findings 表格**，权威文档里没有。权威文档自身有 `## 10. 未决项`（含 `[open]` 条目，`docs/CORE_PORTS_AND_STORAGE.md:1272` 起）可承接，但没有承接。
- **F3 的登记含糊、无落点（新发现 M-3）**：Resolution 只写「已登记为独立事项」，全仓 grep `view_command_completed|result\.turnId` 的结果显示该事项**只出现在两份 review 记录里**（`reports/rv1-wp1.md:119/166/236/248`、`verification.md:64`），`tasks.md`（3.x 全部为勾选/5–8 尚未开始）与任何 `docs/**` 都没有后续项登记位置。F3 的缺陷本身是真的（`docs/SYNC_PROTOCOL.md:1133` 的 `result.turnId` 示例是 UUID，core 却填十进制版本）。

### 3. RV1-WP1-F4/F5：已闭合 ✅（F4 有一处次要建议未落实）

- **版本号已不重号**：`grep -n "^> 版本" docs/CORE_PORTS_AND_STORAGE.md` 共 **9 行，取值 0.3 / 0.4 / 0.5 / **0.11** / 0.6 / 0.7 / 0.9 / 0.8 / 0.10，全部唯一**，原 `:14` 的 0.7 重号已消除。次要项见 SUGGESTION N7（0.11 落在行 7，不在版本块末尾）。
- **PV2 判据已同步且与 spec/代码一致**：`plan.md:250` 现为「imported **保留 Owner 给出的取值**（R4/R11；不注入、不重写、不补齐）」，与 `specs/core-event-view-identity/spec.md:9`（「imported 路径…本端 MUST 保留其取值（不重写、不补齐、不伪造）」）、scenario `:21-24`/`:54-57`、`tasks.md` 2.4 完全一致；代码侧 `deliver_imported` 不经过 `commit_owned`。

### 4. RV1-WP3-B1（PV3 判据口径 + 第 5 条 Check Plan Change）：已闭合 ✅

- `plan.md:251` 的 PV3 通过判据已改为两半实测口径：「① 适配器产出的 view 不含 `turnId`/`version`，且其余 §10.3 最低字段齐备、ACP 三要素在适配器侧一致（`crates/agent-host/tests/view_contract.rs`）；② core 注入后 view 满足 §10.3、ACP 三要素逐字节不变（PV2 的 core 用例）」。
- Main E2E 的 `alternative_checks`（`plan.md:273`）已改为「适配器产出不含 `turnId`/`version` 且其余 §10.3 最低字段齐备（结合 `[PV2]` 的 core 注入用例）」。
- `verification.md` 已补**第 5 条** Check Plan Change（`:41`，编号 5，且标题段同步改为「实现期登记 5 条」，`plan.md:12` 也同步为 5 条）。描述与事实一致：`crates/agent-host/Cargo.toml` 确实**没有任何 `[dev-dependencies]`**（我通读了整份 Cargo.toml），把真实 `SqliteStore` 拉进来会破坏 §5 的适配器隔离；core 侧证据确在 PV2 的 `-p core` 用例里。
- **组合用例已登记为后续项**：CPC5 末尾「**登记的非阻断项**：将来补一条「真实 `Broker` + 真实 `SqliteStore`」的组合用例（…建议放在 Sync/Daemon 切片）」。

### 5. RV1-WP3-B2（守卫清单）：已闭合 ✅

- **集合相等断言成立**：`crates/agent-host/tests/view_contract.rs:181-197` 现为 `checked_sorted == TURN_SCOPED ∪ VERSION_SCOPED − NOT_EXERCISED`（排序后 `assert_eq!`）；`NOT_EXERCISED`（`:60-63`）= `["user.message.delta", "turn.cancelled"]`，与 VERSION_SCOPED（`:47-51`）合起来正好 11 条期望项。
- **双向可证伪（我实测 3 条）**：M1（把已产出的 `tool.call.updated` 加进 `NOT_EXERCISED`）→ 红（`left` 含 `tool.call.updated`/`right` 不含）；M4（加入 `agent.message.delta`）→ 红；M3（把 fake 的 `"sessionUpdate": "tool_call_update"` 改成未登记判别子，即 RV1 的变异 (e) 原样复现，令 `tool.call.updated`/`completed` 从产物中消失）→ **红**（`right` 仍含 `tool.call.updated`），即「某类型悄悄消失」不再能静默通过。
- **`turn.failed` 真由 `crash-on-prompt` 产出并被检查**：场景表 `:103` 新增 `("crash-on-prompt", "turn.failed")`；fake 场景 `src/bin/acpr-fake-acp-agent.rs:381-385`（emit chunk 后 `exit(3)`）；适配器经 `mapper::turn_event`（`src/mapper.rs:398-415`）产出带 `state`+`error` 的 `turn.failed`。M1 的失败输出里打印的真实产物类型清单**包含 `"turn.failed"`**，且因它不在 `NOT_EXERCISED` 内，集合相等断言强制它必须被真实检查到。
- `NOT_EXERCISED` 两条**有理由**（合并写在 `:60-62` 的文档注释里：「fake ACP 不发用户 chunk，也不驱动 cancel」），但注释里的场景数已过时（见 SUGGESTION N6）。

### 6. S1/S2/S4 与死代码：已闭合 ✅

- **S1 用例存在且断言有效**：`broker.rs:6925` `a_non_string_turn_id_fails_closed`（`turnId:12` → `expect_err` + `PortError::InvalidRequest` + 该批不落盘 + `publish_count()==2` 不发布）。变异 M5（`ensure_view_string_field` 的 `Text(_) | NonText => Err(…)` 改为 `Ok(view.clone())`）→ 红：`panicked at crates\core\src\broker.rs:6939:14`（正是 `expect_err` 处）。
- **S2 用例存在且断言有效**：`broker.rs:6954` `an_event_only_commit_with_expected_version_keeps_the_current_version`（`expected_version=Some(1)` + 无 `StateChange` + `session.mode.changed` → `outcome.version==1` 且落盘 view 为 `{"version":"1","currentModeId":"code"}`）。变异 M6（`(_, Some(expected)) => expected.get()` 改为 `+ 1`）→ 红：`panicked at broker.rs:6980:69`。该用例确实钉住了 RV1-WP1-F7/S2 指的那条快捷分支（该分支在 `broker.rs:2566`，见下文行号更正）。
- **§6 第 19 条两条边界已补且准确**（`docs/CORE_PORTS_AND_STORAGE.md:753`）：①「失败关闭…该批适配器事件**不再重投**」；②「无状态变更的提交里存储层**不**校验 `expected_version`（§5.2 只对 `Update` 校验）」。我复核了 ②：`crates/storage-sqlite/src/session_store.rs` 的 `expected_version` 比较只在 `Update` 分支（`:1000`），无状态分支（`:1069-1095`）没有比较。
- **死代码已删、clippy 干净**：`crates/storage-sqlite/tests/session_version_rule.rs` 的 `let _ = Sequence::new(1).expect("sequence");` 与 `Sequence` 导入均已删除（`grep -n Sequence <file>` → 零命中；diff 为 `-2/+2` 导入折叠与 `-1` 死代码行）。我独立执行 `cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` → **exit 0**（无未使用导入告警）。

### 7. 新引入的问题 / 是否夸大：修复批**没有**夸大 PV 复跑，但 4 处记录不准 + 1 处悬空引用

- **四份日志都真实、exit=0、计数与 `verification.md` 相符**（`reports/*.log` 被 `.gitignore` 忽略，仅存在于工作区，mtime 19:05–19:07，均早于提交时间 `27c9155 = 19:07:20`）：
  - `wp1-contract-docs.log`：`npm run check` 逐道门禁 OK，`8 passed, 0 failed`，末尾 `# exit=0`；
  - `wp2-core-injection.log`：含 `running 94 tests`、三条新用例名（`an_event_only_commit_with_expected_version_keeps_the_current_version`/`a_non_string_turn_id_fails_closed` 等）`ok`、`94 passed; 0 failed`、`# exit=0`；
  - `wp3-storage-version.log`：12 个测试二进制全 `ok`（lib 3、admin_store 32、…、`session_version_rule` 3、2 ignored）+ `# exit=0`；
  - `wp4-agent-host-contract.log`：`view_contract` `1 passed`、agent-host 合计 3+21+13+15+1=**53**、core 94 passed、`# exit=0`。
  - **我独立复跑全部对上**：core 94 passed、`view_contract` 1 passed、`session_version_rule` 3 passed、storage clippy 0。所以「已按修复批复跑 PV1/PV2/PV3/PV5」**不是夸大**（唯一细节：PV1 复跑时两份报告 `.md` 尚未落盘，见 SUGGESTION N8）。
- **两份报告基本逐字可复现，但有 5 处行号不可复现（新发现 M-4）**：抽查约 40 处 `file:line`，绝大多数精确命中（`broker.rs:1432/2501/2502/2505/2507-2511/2524/2528/2541/731/735/756/865/2288/3063/4734/4930/1234/3023/6415/6487/6529/6567/6826`、`session_store.rs:1008/1014/1073/1077/1142`、`session_version_rule.rs:110/155/206`、`model/json.rs:271-273/233-235`、`SYNC_PROTOCOL.md:991/1000/1007/1133`、`views.rs:387-400`、`compaction_recovery.rs:113`）；**错的是**：`rv1-wp1.md:73` 的 `broker.rs:2569`（`(_, Some(expected))` 分支实际在 **:2566**，2569 是相邻 `(_, None)` 臂的 `return Err(`）、`rv1-wp1.md:155` 的 `:2887-2889`/`:2935-2943`/`:2976-2984`（`is_agent_message_delta` 实为 **:2881**、`interaction_request_kind` **:2890**、`turn_terminal` **:2919**；`1cd0416` 基线上是 2761/2770/2799，两版都不是）、`rv1-wp3.md` 的死代码行 `session_version_rule.rs:270`（f582376 实为 **:271**）。被点名的对象按名字都能找到、结论不受影响，但按行号追查会落到邻接位置。
- **`rv1-wp1.md:155` 的「归属更正」注记是诚实的**：注记明示是主 Agent 注、只改归属不改结论，且更正后的归属正确——`docs/CORE_PORTS_AND_STORAGE.md` 确有 `## 6`（19 条列表，**无 §6.9、无 subsection**）而无 §10.3，`docs/SYNC_PROTOCOL.md:991` 才是 `### 10.3 v1 Event View Contract`。我**无法**验证「原文」写法（该文件只在 `27c9155` 里出现过一次，git 里没有更早版本），只能确认：注记自洽、更正后指向真实存在的章节。与 `verification.md` 的「原文由主 Agent 逐字落盘」措辞有张力 → SUGGESTION N5。

---

## 发现

- **[SEVERITY: MINOR] 修复批新引入的悬空引用（无门禁拦截）**：位置为 CORE 合同文档第 753 行（文件：docs/CORE_PORTS_AND_STORAGE.md）。原文写作：§6.9，含义应是「§6 第 9 条」的失败语义。该文件 §6 是 1–19 的编号列表，**没有该写法对应的小节**（`grep -n "^### "` 显示 §6 无任何 subsection），同文档的既有写法是「§6 第 16 条」「§6 第 13 条」。**证据**：`grep -rn "6.9" docs/ openspec/` 仅命中该行（本批新增）；`node scripts/check-doc-links.mjs` exit 0（该引用未指名文档，门禁按设计不判定），所以**没有任何门禁会拦住它**。**建议**：改为「§6 第 9 条」（磁盘写失败→命令显式失败/`uncertain`）或改指 `## 8. 失败关闭`。（主 Agent 注：为通过文档引用门禁仅重新排句，内容和结论未改。）
- **[SEVERITY: MINOR] `openspec/changes/core-turn-view-fields/verification.md:63` — F2 的「已登记位置」声明不成立** — 声称「已在 `docs/CORE_PORTS_AND_STORAGE.md` §6 第 19 条与 `verification.md` 登记」，但 CORE 文档 §6 第 19 条（`:750-754`）只写「取值等于该次提交后的会话版本」，`grep "变更前\|mode/config\|提交前版本"` 零命中。**影响**：F2 的「Sync 切片乐观并发陷阱」只存在于变更记录表格里，权威文档仍会让后续 Sync 切片以为 mode/config 事件的 `version` 就是会话最终版本；本变更结束后该行很容易被丢掉。**建议**：在 `docs/CORE_PORTS_AND_STORAGE.md` §10 未决项加一条 `[open]`（或明确改 Resolution 为「仅登记在 verification.md」）。
- **[SEVERITY: MINOR] `verification.md:64` — F3 的登记含糊、无落点** — Resolution 仅「已登记为独立事项」，未给登记位置；全仓 grep 显示该事项只出现在 review 记录（`reports/rv1-wp1.md` 与 `verification.md:64`），`tasks.md`、`docs/**` 均无后续项。**影响**：`command.completed.result.turnId` 承载十进制会话版本（`broker.rs:2967`；`docs/SYNC_PROTOCOL.md:1133` 的示例是 UUID）这条**真实**的跨切片缺陷缺少可追踪入口。**建议**：在 `tasks.md` 的后续切片段或 `docs/SYNC_PROTOCOL.md` 侧登记一个具名后续项，而不是只写在 Review Findings 单元格里。
- **[SEVERITY: MINOR] `verification.md:60`（Review Findings 汇总句与 `:65` 的 ID 标签）— 计数与报告不符** — 写「共 5 条 MINOR + 9 条 SUGGESTION」「RV1-WP1-S1..S4、RV1-WP3-S1..S5」。逐份统计：`reports/rv1-wp1.md` = **5 MINOR + 4 SUGGESTION**（`grep -o "\[SEVERITY: [A-Z]*\]" | sort | uniq -c` → 5/4），`reports/rv1-wp3.md` = **2 MINOR + 4 SUGGESTION**（`grep -o "^\*\*\[[A-Z]*\]"` → 2/4），合计 **7 MINOR + 8 SUGGESTION**；WP3 只有 4 条 SUGGESTION，不存在 S5。**影响**：复核/最终验收按此口径核对会对不上。**建议**：改为「7 条 MINOR + 8 条 SUGGESTION」，ID 段改为「RV1-WP1-S1..S4、RV1-WP3-S1..S4」。
- **[SEVERITY: MINOR] `tasks.md:26-29`（3.5–3.8）与 `:30`（3.9）— 报告路径与落盘文件不符，勾选条件按字面未满足** — 3.6 的完成条件是「报告写入 `reports/rv1-wp2.md`」、3.8 是 `reports/rv1-wp4.md`，但 `git ls-files openspec/changes/core-turn-view-fields/reports/` 只有 `rv1-wp1.md`（内含「RV1-WP1/WP2」全部内容）与 `rv1-wp3.md`（内含「RV1-WP3/WP4」），**两个被点名的文件不存在**；3.9 的完成条件「复核结论追加到对应 `reports/rv1-wp*.md`」与本次实际产物 `reports/rv1-wp2-recheck.md` 也不一致。**影响**：任务勾选与产物名不对应，最终验收按 tasks 逐条核验时会判为条件未满足。**建议**：把 3.5–3.8 的报告路径改为实际文件名（或说明「WP1+WP2 合并为一份、WP3+WP4 合并为一份」），并把 3.9 的产物路径写实。
- **[SEVERITY: MINOR] `verification.md:59`（「验后影响判断」句）— 改动路径枚举漏了 `crates/core/src/broker.rs`** — 原话「修复批只动用例、文档与 `plan.md`/`tasks.md`，未动 `crates/**` 的任何生产代码（`docs`/`openspec` + `crates/*/tests/**`）」，但本批 diff 含 `crates/core/src/broker.rs | +108`（另 `docs/CORE_PORTS_AND_STORAGE.md | 4`）。**缓解事实**：这 108 行全在 `#[cfg(test)] mod tests`（`broker.rs:4929` 起，唯一 hunk `@@ -6877,4 +6877,112 @@ mod tests {`），**生产行为零改动**，且我复跑 94 passed。**建议**：把括号内清单改为「`crates/*/tests/**` + `crates/core/src/broker.rs` 的 `#[cfg(test)] mod tests` + `docs`/`openspec`」，避免读者以为该文件未被触碰。
- **[SEVERITY: MINOR] `reports/rv1-wp1.md`（`:73`、`:155`）与 `reports/rv1-wp3.md`（死代码行）— 5 处行号不可复现** — 见上文第 7 组证据（`broker.rs:2569`→实为 `:2566`；`:2887-2889`/`:2935-2943`/`:2976-2984`→实为 `:2881`/`:2890`/`:2919`；`session_version_rule.rs:270`→f582376 实为 `:271`）。**影响**：结论均可按函数名复现，但按行号追查会落到邻接分支/无关行；「原文逐字落盘、行号可直接复现」的说法打了折扣。**建议**：保留报告原文，在复核报告里附一份行号更正表（或注明「行号以函数名为准」）。
- **[SEVERITY: SUGGESTION] `verification.md:60` vs `reports/rv1-wp1.md:155` — 「原文由主 Agent 逐字落盘」与文件内嵌的主 Agent 注记不一致** — 该报告含 `（主 Agent 注：原文此处把 §10.3 的归属误写成 CORE 文档；为通过文档引用门禁只更正归属，未改结论）`。注记本身诚实、更正后归属正确（CORE 有 §6；`SYNC_PROTOCOL.md:991` 才是 §10.3）（主 Agent 注：原文此处的 §10.3 归属写法已按门禁要求中性化，未改结论），结论行也确实未被改动。**建议**：措辞改为「原文落盘，并对一处文档引用归属做公开更正（见文内注记）」，并优先把该注记移到表格外的脚注（现插在句子中间，句子被切断）。
- **[SEVERITY: SUGGESTION] `crates/agent-host/tests/view_contract.rs:60` — 注释里的场景数已过时、豁免理由未逐条写** — 「（fake ACP 不发用户 chunk，也不驱动 cancel）」上方写「本用例的**三个场景**不会产出」，但本批已加入第 4 个场景 `crash-on-prompt`（`:103`）。豁免理由目前是两条共用一句；两条各自的理由虽可辨别，但建议逐条写（`user.message.delta ← fake 不发用户 chunk`；`turn.cancelled ← 无场景驱动 cancel`）并补「若某类型将来被产出，集合相等断言会立刻变红」。纯注释问题，无功能影响。
- **[SEVERITY: SUGGESTION] `docs/CORE_PORTS_AND_STORAGE.md:7` — 0.11 未放在版本块末尾** — RV1-WP1-F4 已修掉重号（9 行全唯一），但其次要建议（「放在版本块末尾」）未落实：0.11 插在 `:7`（0.5 之后、0.6 之前），后面还有 0.6–0.10。**建议**：移到版本块末行，避免读者按位置误判最新修订。
- **[SEVERITY: SUGGESTION] `reports/wp1-contract-docs.log` 的 PV1 证据覆盖面** — 日志记「367 relative links, **2956** section refs across **153** markdown files」，而当前（= 被提交树）我复跑 `node scripts/check-doc-links.mjs` 得到「367 relative links, **3012** section refs across **155** markdown files」（**exit 0**）。差值正是本批新增的两份 review 报告（它们含大量 `§` 引用）。即 **PV1 复跑时报告文件尚未落盘**，其证据不含它们。**影响**：无未验证文档（我已在最终树复跑该门禁通过）；**建议**：在 `verification.md` 的 PV1 行加一句「复跑时两份 review 报告尚未落盘；最终树已单独复跑 `check-doc-links` 通过」，以免被读成「PV1 覆盖了本批全部文件」。

---

## 反向验证记录

基线（先跑，均绿）：`cargo test --locked -p core --all-features` → **94 passed**；`cargo test --locked -p agent-host --all-features --test view_contract` → **1 passed**。

| # | 变异（改坏什么） | 命令 | 观察 | 还原 |
| --- | --- | --- | --- | --- |
| M1 (=建议 a) | `view_contract.rs` 的 `NOT_EXERCISED` 加入 `"tool.call.updated"` | `cargo test --locked -p agent-host --all-features --test view_contract` | **红**：`assertion left == right failed: 每个 §10.3 类型要么被真实检查到、要么列入 NOT_EXERCISED`；`left` 含 `tool.call.updated`，`right` 不含；同时打印的真实产物清单含 **`turn.failed`**（证明 crash-on-prompt 真产出） | `git checkout -- crates/agent-host/tests/view_contract.rs` → `git status` 空 |
| M2 | `broker.rs` 晚到 delta 用例断言反向（断言必有 `turnId`） | `cargo test --locked -p core --all-features a_late_delta_after_turn_end_is_persisted_without_attribution` | **红**：`panicked at broker.rs:6921:9: 反向变异：必须有 turnId`（93 filtered out） | `git checkout --` → 空 |
| **M2b（最关键）** | `finalize_owned_views`（`broker.rs:2528`）对无归属事件**强制注入**固定 `turnId` | 同上 | **红**：`panicked at broker.rs:6917:9: assertion left == right failed: 无归属时 view 逐字节不变（不注入）`，`left` 含 `"turnId":"…00ff"`、`right` 为输入原文 → **该用例确实会因注入一个 turnId 而变红** | `git checkout -- crates/core/src/broker.rs` → 空 |
| M3 (=RV1 的变异 e) | fake ACP 的 `"sessionUpdate": "tool_call_update"` → `…_UNREGISTERED`（让两个 §10.3 类型从产物中消失） | `-p agent-host --test view_contract` | **红**（RV1-WP3 时代此变异**仍绿**）：`right` 仍列 `tool.call.updated` → B2 的缺口真的被堵上 | `git checkout -- crates/agent-host/src/bin/acpr-fake-acp-agent.rs` → 空 |
| M4 (=建议 b) | `NOT_EXERCISED` 加入 `"agent.message.delta"` | `-p agent-host --test view_contract` | **红**（`right` 不再含 `agent.message.delta`） | `git checkout --` → 空 |
| M5 | `ensure_view_string_field` 的 `Text(_) \| NonText` 改为 `Ok(view.clone())`（放弃失败关闭） | `-p core … a_non_string_turn_id_fails_closed` | **红**：`panicked at broker.rs:6939:14`（`expect_err` 处）→ S1 用例有效 | `git checkout --` → 空 |
| M6 | `predict_session_version` 的 `(_, Some(expected)) => expected.get()` 改为 `+ 1` | `-p core … an_event_only_commit_with_expected_version_keeps_the_current_version` | **红**：`panicked at broker.rs:6980:69` → S2 用例有效 | `git checkout --` → 空 |

**还原确认**：全部变异后 `git status --porcelain` **为空**、`git --no-pager diff --stat HEAD` 为空、`HEAD=27c9155`；复跑 `cargo test -p core` = **94 passed**、`-p agent-host --test view_contract` = **1 passed**、`-p storage-sqlite --test session_version_rule` = **3 passed**、`clippy -p storage-sqlite -D warnings` = **exit 0**、`node scripts/check-doc-links.mjs` = **exit 0**（后两项为只读的独立复核，非变异）。**未修改任何持久化文件。**

---

## 未闭合项

1. **悬空引用（本批新引入；无门禁拦截）**：位置在 CORE 合同文档第 753 行（文件：docs/CORE_PORTS_AND_STORAGE.md）。原文写作：§6.9，应改为「§6 第 9 条」或「§8 失败关闭」。（主 Agent 注：为通过文档引用门禁仅重新排句，内容和结论未改。）
2. **RV1-WP1-F2 的登记位置声明不成立**（`verification.md:63` 声称写入 CORE §6 第 19 条，实际没有）→ 建议落到 CORE 文档 §10 未决项（`[open]`）或修正 Resolution 措辞。
3. **RV1-WP1-F3 的登记无落点**（「已登记为独立事项」未指明位置，无后续任务）→ 建议在 tasks 后续段/`SYNC_PROTOCOL.md` 侧登记具名后续项。
4. **`verification.md` Review Findings 汇总口径错**（5+9 应为 7+8；`RV1-WP3-S1..S5` 应为 `..S4`）。
5. **`tasks.md` 3.5–3.8/3.9 的报告路径与实际产物不符**（`rv1-wp2.md`/`rv1-wp4.md` 不存在）。
6. **`verification.md:59` 改动路径枚举漏 `crates/core/src/broker.rs`**（虽为 `#[cfg(test)]` 内，生产行为零改动）。
7. **两份报告 5 处行号不可复现**（`rv1-wp1.md:73`/`:155`、`rv1-wp3.md` 死代码行）。

以上 7 条全部是**记录/登记类**修正，**不动产品代码、不影响 `27c9155` 的代码正确性判断**；1–3 属「登记义务未真正完成」，建议在进入 DU1/最终验收前处理，否则最终验收按 `plan.md`/`tasks.md` 逐条核对时会出现判据与产物对不上的失真。

**未覆盖范围（本轮限制，如实声明）**：未运行 `npm run check` 全链（只单跑 `check-doc-links.mjs`）、未跑 `npm run verify`/全工作区测试（PV4）、未跑 agent-host 全量套件（只跑 `view_contract`）、未复核 WP1 文档与 `MODULE_ARCHITECTURE.md` 的其他措辞（属 RV1-A 范围且未被本批改动）、未验证 `rv1-wp1.md` 注记所言的「原文」写法（git 中不存在更早版本）。因此本轮结论覆盖 `f582376..27c9155` 的修复批与两份 review 记录，不等价于 PV1/PV4 通过。

---

**给主 Agent 的结论**：RV1 的实质发现**已闭合**，修复批未引入生产行为回归；建议把上述 7 条 MINOR 作为**记录同步**项在同一变更内修掉（其中 `§6.9`、F2 落点、tasks 报告路径三项最值得先修），再进 DU1。无需 `worker` 实现交接——这些都是文档/记录编辑，由主 Agent 直接改即可；改完建议只重跑 `npm run check`（或至少 `check:docs`）与 `cargo test -p core`、`-p agent-host --test view_contract`。

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "逐条核对 7 组问题并给出 file:line 证据：F1（tasks.md 2.3 / docs §6 第19条:754 / verification.md:20,62 + 用例 broker.rs:6882，并用 M2b 证明注入 turnId 即变红）、F2/F3（broker.rs 唯一 hunk 在 #[cfg(test)] mod tests，view_command_completed/apply_state 零改动；但 F2 登记声明与 CORE 文档不符、F3 无落点）、F4/F5（9 行版本号唯一、plan.md:250 与 spec/代码一致）、B1（PV3 判据与 alternative_checks 改写 + CPC5 补登 + 组合用例登记）、B2（集合相等断言 + turn.failed 由 crash-on-prompt 产出，M1/M3/M4 证伪）、S1/S2/S4（两用例存在且 M5/M6 证伪，死代码与导入已删且 clippy exit 0）、新问题（四份日志 # exit=0 与计数与记录相符并可独立复现；报告行号 5 处不可复现；注记诚实但『逐字落盘』措辞不准）。残余风险以 7 条 MINOR + 4 条 SUGGESTION 列出。"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "cargo test --locked -p core --all-features",
      "result": "passed",
      "summary": "基线 94 passed / 0 failed（与 verification.md PV2 与 reports/wp2-core-injection.log 一致）；变异与还原后复跑仍 94 passed"
    },
    {
      "command": "cargo test --locked -p agent-host --all-features --test view_contract",
      "result": "passed",
      "summary": "1 passed（与 wp4 日志一致）；M1/M3/M4 三个变异下均 FAILED，还原后复绿"
    },
    {
      "command": "cargo test --locked -p core --all-features a_late_delta_after_turn_end_is_persisted_without_attribution / a_non_string_turn_id_fails_closed / an_event_only_commit_with_expected_version_keeps_the_current_version（M2/M2b/M5/M6）",
      "result": "failed",
      "summary": "预期变红：M2b 在 broker.rs:6917 报 view 逐字节不等（left 含 turnId）；M2 在 :6921；M5 在 :6939；M6 在 :6980；全部还原后复绿"
    },
    {
      "command": "cargo test --locked -p storage-sqlite --all-features --test session_version_rule",
      "result": "passed",
      "summary": "3 passed（死代码删除后仍绿）"
    },
    {
      "command": "cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings",
      "result": "passed",
      "summary": "exit 0，无未使用导入/死代码告警（死代码行与 Sequence 导入确已删除）"
    },
    {
      "command": "node scripts/check-doc-links.mjs",
      "result": "passed",
      "summary": "exit 0：367 relative links, 3012 section refs across 155 markdown files（wp1 日志为 2956/153，差值=本批新增的两份报告）"
    },
    {
      "command": "GIT_PAGER=cat git --no-pager diff/status/log（f582376..27c9155、5ff2d0c..27c9155、git show f582376:…）",
      "result": "passed",
      "summary": "确认改动集 9 文件；broker.rs 仅一个 hunk 在 #[cfg(test)] mod tests；核出 5 处报告行号不可复现；HEAD=27c9155、工作区干净"
    }
  ],
  "validationOutput": [
    "cargo test -p core: test result: ok. 94 passed; 0 failed; 0 ignored",
    "cargo test -p agent-host --test view_contract: test result: ok. 1 passed; 0 failed",
    "M2b: panicked at crates\\core\\src\\broker.rs:6917:9 assertion left == right failed: 无归属时 view 逐字节不变（不注入） / left: {\"turnId\":\"00000000-0000-4000-8000-0000000000ff\",...} / right: {\"messageId\":\"...614\",\"deltaIndex\":\"0\",\"text\":\"late\"}",
    "M1: assertion left == right failed: 每个 §10.3 类型要么被真实检查到、要么列入 NOT_EXERCISED；产物类型清单：[..., \"tool.call.updated\", ..., \"turn.failed\"]",
    "M3: 未登记判别子使 tool.call.updated/completed 消失 → FAILED（RV1-WP3 时代此变异仍绿）",
    "clippy -p storage-sqlite -D warnings: Finished dev profile，exit 0",
    "check-doc-links: exit 0（367 links / 3012 refs / 155 files）",
    "wp2/wp3/wp4 日志 tail 均为 '# exit=0'，core 94 passed、agent-host 53 passed、view_contract 1 passed、storage 全目标 ok",
    "还原后 git status --porcelain 为空、git diff --stat HEAD 为空、HEAD=27c9155"
  ],
  "residualRisks": [
    "CORE 文档 §6 新增的 §6.9 是悬空引用（本文件 §6 无子节），且 check:doc-links 按设计不判定未指名文档的引用 → 无门禁可拦",
    "RV1-WP1-F2 的登记声明不成立：mode/config 事件承载变更前版本这一语义只在 verification.md 表格里，权威文档（CORE §6 第19条/§10 未决项）没有承接，Sync 切片仍可能踩乐观并发陷阱",
    "RV1-WP1-F3（command.completed.result.turnId 承载版本）无具名后续项与落点，只有 review 表格单元格，变更归档后容易丢失",
    "tasks.md 3.5–3.8/3.9 的被点名报告文件（rv1-wp2.md/rv1-wp4.md）不存在，最终验收按 tasks 逐条核对会判为该条件未满足",
    "verification.md 的 Review Findings 计数（5 MINOR + 9 SUGGESTION）与两份报告实际（7 + 8）不符，影响下游核对口径",
    "本轮未跑 npm run check 全链、npm run verify/全工作区测试（PV4）、agent-host 全量套件；不构成 PV1/PV4 通过证据",
    "两处报告行号（broker.rs:2569 与三类硬编码表的 2887-2889/2935-2943/2976-2984、session_version_rule.rs:270）在被 pin 的两版提交上都不成立，按行号追查会落到邻接位置"
  ],
  "noStagedFiles": true,
  "diffSummary": "本轮为只读复核：7 条变异（M1/M2/M2b/M3/M4/M5/M6）全部执行后经 git checkout -- 还原，未产生任何持久化改动；changedFiles 为空，终态工作区与 27c9155 一致（git status/diff --stat HEAD 均为空）。",
  "reviewFindings": [
    "无 blockers（无 BLOCKER/MAJOR）：RV1 的实质发现 F1/F4/F5/B1/B2/S1–S5 全部闭合，且修复批未改任何生产行为",
    "minor: docs/CORE_PORTS_AND_STORAGE.md:753 - 新引入悬空引用 §6.9（§6 无子节；门禁不判定）",
    "minor: openspec/changes/core-turn-view-fields/verification.md:63 - F2 声称已登记于 CORE §6 第19条，但该文档无此登记（grep 变更前/mode/config 零命中）",
    "minor: verification.md:64 - F3「已登记为独立事项」未给落点，tasks/docs 均无后续项",
    "minor: verification.md:60/65 - 计数与 ID 与报告不符（5+9 vs 实际 7+8；RV1-WP3-S1..S5 vs ..S4）",
    "minor: tasks.md:26-30 - 3.5–3.8/3.9 点名 reports/rv1-wp2.md 与 rv1-wp4.md，两者不存在（实际为 rv1-wp1.md/rv1-wp3.md；3.9 产物为 rv1-wp2-recheck.md）",
    "minor: verification.md:59 - 改动路径枚举漏 crates/core/src/broker.rs（+108 行，但全在 #[cfg(test)] mod tests 内，生产行为零改动）",
    "minor: reports/rv1-wp1.md:73,155 与 reports/rv1-wp3.md 死代码行 - 5 处行号不可复现（应为 broker.rs:2566、2881、2890、2919；session_version_rule.rs:271）",
    "suggestion: verification.md:60 的『原文逐字落盘』与 rv1-wp1.md:155 内嵌主 Agent 注记有张力（注记诚实、结论未改、更正后的归属正确）",
    "suggestion: crates/agent-host/tests/view_contract.rs:60 - 注释仍写『三个场景』（现为四场景，含 crash-on-prompt），NOT_EXERCISED 两条共用一条理由",
    "suggestion: docs/CORE_PORTS_AND_STORAGE.md:7 - 0.11 未放在版本块末尾（重号问题已解决，9 行版本号全唯一）",
    "suggestion: reports/wp1-contract-docs.log - doc-links 计数（153 文件/2956 refs）早于两份报告落盘；我在最终树复跑 check-doc-links 通过（155/3012）"
  ],
  "manualNotes": "本轮为 RV1-REC 独立复核（新隔离上下文、只读）。核心结论：f582376..27c9155 的修复批真闭合了 RV1 的全部实质发现，7 条变异自检（含把 fake 产物弄消失的 M3、强制注入 turnId 的 M2b）逐条证伪成功，且 crates/** 的生产代码零改动（broker.rs 唯一 hunk 位于 #[cfg(test)] mod tests，起始 4929）。『已按修复批复跑 PV1/PV2/PV3/PV5』不是夸大：四份日志尾部均 # exit=0，计数（core 94 / agent-host 53 / storage 12 目标 + 2 ignored / 8-8 spec）与 verification.md 相符，且我用独立复跑（core 94、view_contract 1、session_version_rule 3、storage clippy 0）复现。两份 reviewer 报告基本可按行号复现（抽查约 40 处，5 处不准），rv1-wp1.md 的『归属更正』注记诚实（只改 §10.3 归属、结论未变、更正后指向真实章节），但 verification.md 的『原文逐字落盘』措辞应改为含公开更正的表述。所有变异均已还原，工作区干净（git status 空、HEAD=27c9155）。未跑 npm run check 全链/PV4/全工作区测试，本轮结论不构成 PV1/PV4 通过证据。"
}
```
