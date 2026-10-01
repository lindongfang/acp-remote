# CR6 独立检视报告（WP6 / branch / work-package / Round 2 · 复核轮）

> 持久化说明：本报告由 reviewer 实例 `reviewer-F2`（**只读工具集，无写权限**）以全文返回、由主 Agent 原样落盘到 `openspec/changes/session-resume/reports/cr6-review-round2.md`。run `9d7a0b41-75ba-466a-bf5e-5ddd80a47bea`。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | `2.6`（WP6 DELIVERY）、`3.7`（WP6 REVIEW） |
| `role` | `reviewer` |
| `phase` | `branch`（复核轮沿用原类型） |
| `agent_context` | 子 Agent `reviewer-F2`，由主 Agent 以全新实例派发；**只收到本轮派发全文**（角色契约 + 复核简报），未继承 `coder` / `coder-F2` / `coder-F3` 的任何实现对话，也未参与本轮修复。依据：`cr6-review.md`（上一轮结论）与 `wp6-coder-f3.md`（实现者自述）是**被复核对象**，不作为正确性证据使用 |
| `work_package` | `WP6` |
| `stage` | `work-package` |
| `base_revision` | `76ab01126114294e7064e2618eedad0615b2561d` |
| `target_revision` | `a7bc596499170a6365e061c70668a203fa2534bb` |
| `result` | **PASS**（0 CRITICAL / 0 MAJOR） |
| `issues` | 无 CRITICAL/MAJOR。新增 1 条 SUGGESTION（CR6-F4，纠正一个被延后的遗留判断，非缺陷） |

## Review

**结论：PASS**（0 CRITICAL / 0 MAJOR）。CR6-F1 / F2 / F3 三条均**已解决**，且修复未引入新的 CRITICAL/MAJOR 问题。

**隔离方式（如实记录）**：全新实例，未参与本轮实现；工作树在 `a7bc596` 上干净（`watchdog_diff` 返回 "No working-tree changes against reviewer-launch HEAD a7bc59649917"）。只读了角色契约、AGENTS.md（项目上下文）、上一轮报告与实现者报告，以及目标版本的源码/测试/文档内容。

**工具限制（必须记录）**：`watchdog_diff` 只覆盖「工作树 vs 启动 HEAD」，不含已提交范围，因此**无法生成 `76ab011..a7bc596` 的权威 diff**。本轮结论基于目标版本的**内容**逐处核对；「只改了 3 个文件 / 只改了这几行」这类写入范围结论，是用**内容对账**（新增行逐条定位 + 实现者给出的行数统计）交叉验证的，不是 diff 级证据，需在集成阶段由 merger 用真实 diff 复核一次。另外未执行任何 `cargo` / `npm`（只读边界），测试执行证据引用实现者的日志而非自己的运行。

---

## 三条发现的闭环判定

### CR6-F1（MINOR，主要修复）—— 幂等比对漏了会话身份：**已解决**

**修复位置**：`crates/server/src/node_link/command.rs:1037-1040`（`same` 的第四项），理由注释在 `:1020-1026`。

```rust
let same = record.command() == CommandName::SessionResume.as_str()
    && record.kind() == CommandKind::Mutation
    && record.request_fingerprint() == &fingerprint
    && record.session() == Some(&target.session);
if !same {
    return self.reject_code(handle, CommandName::SessionResume, &submit.request_id,
        "nodelink.command.idempotency_conflict");
}
```

#### 五个子问题逐条回答

**(1) 是否真正堵住了原漏洞？是。**
同一 `(ownerNodeId, accessNodeId, requestId)` 改指另一个会话时，命中 `Ok(Some(record))` 臂 → `same` 的第四项为 `false` → 直接 `reject_code(..., "nodelink.command.idempotency_conflict")` 并 `return`，**在调用 `core.resume_session` 之前**返回（`command.rs:1041-1048` 早退，`:1076` 才是 `resume_session`）。因此不再回首次会话的终态结果，也不会产生第二次 spawn。
新用例（`tests.rs:2773` `session_resume_reusing_a_request_id_for_another_session_is_an_idempotency_conflict`）覆盖的正是这个形态：第二个会话 `SESSION_2` 被种下同样的授权、同样的 Export、同样 attach 成功、同样有持久化恢复数据（`tests.rs:2800-2806` 用 `session_summary_of` / `seed_recoverable_for` / `Fixture::attach_session`），所以「换一个会话」本身合法，冲突只可能来自 requestId 复用——这一点是这条用例设计得对的关键。

**(2) 是否引入了「合法重试被误判为幂等冲突」的风险？——没有误伤（本轮最需要盯的回归点，判定为无风险）。**
`record.session()` 为 `None` 时条件为 `false`（一律判冲突）这一推理成立，但逐条核实了「core 的 resume 幂等行**是否总会**带 session」：

- **唯一写入点**：`Broker::resume_session`（`crates/core/src/broker.rs:1516-1526`）自建 `IdempotencyRecord`，其中 `session: Some(session.clone())` 是**无条件**赋值（目标会话在提交前就已知）。core 自己的用例断言了这一点：`use_cases.rs:2150-2153`（`record.session() == Some(&fixture.session)`）。
- **没有第二条写入路径能产出 `command == "session.resume"` 且 `session IS NULL` 的行**：`session.resume` 没有 `CommandPayload` 变体（`core/src/model/session.rs:754-755` 明确说明），因此通用 mutation 管线产不出该命令名；全仓 `"session.resume"` 的其它出现（`broker.rs:4118`、`:6386`，`storage-sqlite/tests/migration.rs:1289`）都是测试辅助代码。core 侧 `settle_session_resume` 也把「持久记录缺少目标会话」当适配层误用显式报错（`broker.rs:1666`），与「resume 行必有 session」互为印证。
- **读取侧不会丢字段**：`command_status` → `SessionStore::find_request`（`storage-sqlite/src/session_store.rs:2031-2051`）用 `COMMAND_COLUMNS` 取整行交给 `command_record_from_row`，`owned_command.session_id` 是普通列（`migrate.rs:123`），终态推进只更新 status/result/error 列，不清 `session_id`。
- **重试的取值必然相同**：`target.session` 由 `core_session(&str) = SessionId::new(text)`（`command.rs:2425`）从 wire 解析，首次与重试用的是同一个 `sessionRef.sessionId` 文本 → 同一个 `SessionId`；落盘时写的就是这个值的 `as_str()`。
- **反向的完整性检查也成立**：`sessionRef` 的另外两个维度（`ownerNodeId`/`exportId`）不需另比——`attachment_is_current`（`command.rs:1258-1288`）比较的是**整个** `target.remote`（`RemoteSessionRef` 三字段）且发生在幂等回读之前，所以「同 sessionId 但换了 exportId」会被 `attach_generation_stale` 挡下。`expectedVersion` 维度也不适用：`CommandName::SessionResume.requires_expected_version()` 为 `false`（`node-link-protocol/src/command.rs:119-124, 931-943`）。
- **既有回归护栏在位**：`tests.rs:3005` `a_repeated_session_resume_replays_the_first_result`（[R67]）用同一 `REQUEST` + 同一 `sessionRef` 提交两次，断言第二次回**首次终态**且 `result` 逐字一致、`command.accepted` 为空。这条用例恰好是本轮条件成立的那一格（`Some == Some(同一 session)`），若条件写错（例如写成 `!=` 或漏比）它必然失败。

结论：`None ⇒ 冲突` 这条分支在真实持久化状态下不可达（只能来自库被外部篡改），不构成误伤。

**(3) 新用例是否有判别力？是（与实现者的红-绿证明一致）。**
独立推演了「删掉第四项」后的行为：第二次提交命中 `Ok(Some(record))`，命令名/种类/指纹三项全同 → `same = true` → 记录已是终态 → 走 `send_terminal`，即产出 **1 帧 `command.terminal`（首次会话的结果）+ 0 帧 `command.rejected`**。因此 `assert_eq!(rejected.len(), 1, ...)` 会以 `left: 0, right: 1` 失败。实现者的 `wp6-coder-f3-regression-proof.log` 记录的正是这一形态：`panicked at crates\server\src\node_link\command\tests.rs:2819:5 ... left: 0, right: 1`，且失败断言文本与当前源码 `tests.rs:2819` 的那句完全对应。**日志与独立推演一致，不是恒真断言**。该用例还额外断言「无 `command.accepted`、无 `command.terminal`」，把「不得回首次会话终态」写成了显式约束。

**(4) 是否新增了错误码？没有。**
修复复用既有码 `nodelink.command.idempotency_conflict`（`compatibility/errors/v1/errors.json:81`，`retryable: false`，`details: null`），且该字符串在同一函数的另一臂（core 抛 `Conflict(IdempotencyConflict)` 时，`:1097-1106`）已在用。`wire_error`（`command.rs:2298`）的词表未变。

**(5) 是否顺手扩到了通用 mutation 臂？没有。**
`command.rs:392-470` 的通用臂原样：它把 `session: target.as_ref().map(...)` 交给 `core.submit_command`（`:435`），自身**不做**指纹短路的 `same` 比对，本轮未加任何会话比较。边界守住了。

**附带纠正（对 coder 待澄清问题 #1 的回答）**：coder 报告里「通用 mutation 臂存在同形盲区」这一说法**不成立**，不必为此开新工作包。通用臂的幂等判定在 core：`Broker::idempotency`（`broker.rs:2783-2810`）的五项比对里**已经包含** `record.session().map(as_str) == command.session...`；resume 之所以成为唯一命中点，正是因为它在调 core **之前**就短路返回了首次结果（core 的那层保护够不到本路径），而通用臂不短路。详见 CR6-F4。

### CR6-F2（MINOR）—— R31 的路由层证据强度不足：**已解决**

- `CommandStore` 新增 `recovery_reads: Arc<AtomicUsize>`（`tests.rs:139-142`）+ `recovery_read_calls()`（`:162-164`），`load_recovery` 入口 `fetch_add`（`:363`）。
- 被拒路径断言（`tests.rs:2907-2919`）：`recovery_read_calls() == reads_before` 与 `commit_calls() == before` 两条**独立**断言，失败信息分别写明「授权必须先于本机读取」与「越权恢复不得触发任何提交」。
- **读计数是否覆盖了「读取」这一维度？覆盖。** 在 core 的 `resume_session` 里，读会话行的唯一入口就是 `deps.store.load_recovery`（`broker.rs:1500`，授权 `:1496` 之后的第一行），其后只有 `resume_request` / `revalidate_resume_workspace`（内存 + 文件系统）；路由层 `session_target`（`command.rs:1218-1256`）不查本机记录。因此「`load_recovery` 未被调用」是「未读会话行」的直接证据，不是间接推断。
- **断言是否有效？有效**，且用的是**基线差**而非字面 `0`——与实现者报告里写的「断言为 0」措辞略有出入，但等价或更强（基线非 0 时依然有效），不构成缺陷。
- **doc 注释是否与实际证明的东西一致？一致，未继续夸大。** `tests.rs:2883-2886` 现在写的是：「『不读取会话行』与『不产生副作用』是两条独立断言：`recovery_read_calls` 证明前者（`load_recovery` 是读会话行的唯一入口），`commit_calls` 证明后者」——与上一轮「把两者混成一句」的问题相比已改正。
- 一点如实补充（不构成缺陷）：`authorize_node` 会读节点信任/Export 目录（`node_link_catalog_view`），那不是会话行；注释的措辞「读会话行」是准确的，未越界声称「零本机读取」。

### CR6-F3（SUGGESTION）—— `local_terminal` 兜底收窄未登记：**已解决**

- 新增注记在 `docs/NODE_LINK_PROTOCOL.md:664`（`session.resume` 结果契约段，与 `:649` 的 `session.create` RV2-WP6-F1 同形），另在 `:663` 同段登记了 CR6-F1 的跨会话 requestId 复用口径。
- **登记是否与 `command.rs:1178-1199` 的实际行为一致？一致，逐句核对：**
  - 「恢复成功但终态落盘失败时仍发一帧本地 `command.terminal`」→ `settled` 为 `Err` 时只 `warn!`（`:1179-1187`），随后 `command_status` 回读命中 `_` 臂（`:1193`）调用 `local_terminal`（`:1197-1204`）。✔
  - 「`terminalEventId` 为 `null`」→ `local_terminal` 三臂都写 `terminal_event_id: Nullable::null()`（`:2258/2265/2272`）。✔
  - 「持久记录仍是 `accepted`，`command.status` 重查回 `accepted`」→ 落盘失败不改动该行。✔
  - 「由启动恢复按 `CORE_PORTS_AND_STORAGE.md` §6 第 16 条改写为 `uncertain`」→ 与 `session.create` 同一条既有规则。✔
  - 「重试会命中幂等行而不产生第二次副作用」→ 重试走 `Ok(Some(record))` 臂，记录非终态 → `send_accepted` + `watch`（`:1050-1064`），不再 `resume_session`。✔
  - 「第一次尝试可能已经拉起了 Agent 进程」→ 属实且是有价值的差异说明（`resume` 的 spawn 在终态落盘之前，`core/src/broker.rs:1535-1543`）。✔
- **§10 是否被误改？未发现误改（内容级证据）。** 该文件本轮的两条新增行就是 `:663` 与 `:664`，两行都落在 §12.7 的 `session.resume` 结果契约列表内；§12.5 的既有规定（`:594`：「`command`、`sessionRef`、`expectedVersion` 或解码后的 `payload` 语义不同则返回 `nodelink.command.idempotency_conflict`」）读起来与本次修复一致，无需改动。**限制**：无法生成已提交 diff，请 merger 在集成阶段用真实 diff 复核。
- **「分段登记」而非跨 `create`/`resume` 统一表述：可接受。** 两条注记指向同一条兜底代码形态，但各自的失败后果不同（create 落盘失败可能创建出另一个会话；resume 落盘失败可能已经拉起了进程），分段把差异写在了各自该在的地方；`:664` 已显式指向 `:649`（「与上文 `session.create` 的 RV2-WP6-F1 同形」），不构成重复定义。**不要求统一**，登记为可选的后续收拢。

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| CR6-F1 | MINOR | `crates/server/src/node_link/command.rs:1037-1040` | 已解决：`same` 追加 `record.session() == Some(&target.session)`；core `resume_session`（`broker.rs:1516-1526`）无条件写 `Some(session)`，`None ⇒ 冲突` 分支真实不可达；新用例有判别力（红-绿日志与独立推演一致）；复用既有错误码；未扩到通用臂 | 跨会话 requestId 复用不再回错会话的终态 | 无需动作 | **已解决**（`a7bc596`） |
| CR6-F2 | MINOR | `crates/server/src/node_link/command/tests.rs:139-142, 162-164, 363, 2907-2919, 2883-2886` | 已解决：`load_recovery` 读计数 + 两条独立断言；doc 注释与实际证明范围一致 | R31 在路由层有了「读取」维度的直接证据 | 无需动作 | **已解决**（`a7bc596`） |
| CR6-F3 | SUGGESTION | `docs/NODE_LINK_PROTOCOL.md:663-664` | 已解决：与 `command.rs:1179-1204` / `:2258` / `:1050-1064` 的实际行为逐句一致；§12.5 无需改 | `local_terminal` 兜底的口径已在权威文档登记 | 无需动作；统一表述为可选 | **已解决**（`a7bc596`） |
| CR6-F4 | SUGGESTION | `crates/core/src/broker.rs:2783-2810`（通用臂幂等判定） vs `crates/server/src/node_link/command.rs:392-470` | coder 报告的待澄清问题 #1「通用 mutation 臂存在同形盲区」**不成立**：通用臂不短路，幂等五项比对在 core 的 `Broker::idempotency` 里已含 `session`（`:2798-2799`），存储层 `commit` 再比一次。resume 之所以是唯一命中点，只因它在调 core 前就短路返回首次结果 | 无产品影响；**避免为一个不存在的问题开新工作包或改动通用臂行为** | 主 Agent 可据此关闭 coder 的待澄清问题 #1，不必新开发现 | 本轮新增（report-only；非本次修复引入） |

**未发现新的 CRITICAL/MAJOR/MINOR 缺陷。** 本轮修复未引入回归：新增的比较只在「已存在 `session.resume` 幂等行」这一分支生效，不触碰首次提交路径、core 契约、协议 wire 或错误码词表。

---

## Assessment

- **本轮结论：PASS**（0 CRITICAL / 0 MAJOR），对应 `a7bc596499170a6365e061c70668a203fa2534bb`。
- **本轮实际检查范围**：`command.rs` 的 `on_session_resume` 全函数（`:981-1208`）、core 侧 `Broker::resume_session` / `Broker::idempotency` / `settle_session_resume`、`storage-sqlite` 的 `find_request` 与 `owned_command` DDL、`node-link-protocol` 的 `CommandSubmit::validate` 与 `requires_expected_version`、`compatibility/errors/v1/errors.json`、`tests.rs` 的 `CommandStore` 与 4 条相关用例、`docs/NODE_LINK_PROTOCOL.md` §12.5/§12.7。
- **未验证 / 待补（不改变本轮判定）**：
  1. **已提交范围的权威 diff**（`76ab011..a7bc596`）——工具无法生成。请 merger 在集成阶段用真实 diff 复核一次（尤其「只改了 3 个文件」与「文档只有 +2/-0」）。
  2. **命令执行证据**——`cargo fmt/clippy/test -p server` 与 `npm run check` 按只读边界未执行，引用 `wp6-coder-f3.md` 的日志（退出码 0/0/0/0）作为待核对项。这属 PV1 阶段 2 / 候选阶段门禁，**不阻断本工作包的代码判定**；workspace 全量 `clippy`/`test` 仍待候选阶段执行，**未执行不等于通过**。
  3. **CI 专属判定**（`deps` / `advisories` / `secrets`）本地不可执行。
- **本轮不判的事项**：需求是否实现完毕、测试集合是否齐备（交主 Agent Coverage Index）、CR6 上一轮已 PASS 的 A–F 各条。

### handoff_index

- `task_id: "3.7"` · `work_package: WP6` · `role: reviewer` · `phase: branch` · `round: 2` · `stage: work-package` · `target_revision: a7bc596499170a6365e061c70668a203fa2534bb` · `evidence_type: REVIEW` · `evidence_id: CR6` · `result: PASS` · `evidence_status: NEW` · `report_path: openspec/changes/session-resume/reports/cr6-review-round2.md` · `applicability_basis: 复核轮，沿用 CR6 线程；静态核对 a7bc596 的源码/测试/文档内容，闭环 CR6-F1/F2/F3；无法生成已提交 diff，未执行任何构建或测试命令` · `source_evidence: NOT_APPLICABLE`
