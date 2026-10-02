> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：DR1 / Round 11；run 8003b50e-c807-4a3e-ab87-6b89ebc7d8f6

# DR1 Round 11 复核报告（Review Type: plan，plan 阶段）
> 本报告由**第十一个独立 reviewer 实例**产出（新实例、只读、无 write 工具；未参与前 10 轮、未参与任何实现或用例设计；宿主未提供本实例 ID 与 `fork_turns` 设置，故不声称已证明隔离方式）。main 请把本报告原样持久化为
> `openspec/changes/session-resume/reports/dr1-dependency-review-round11.md`，再回填 `verification.md` 的 `## Dependency Declaration Review`（见文末「给 main 的回填指引」）。
## Shared Report
| 字段 | 取值 |
| --- | --- |
| task_id | NOT_APPLICABLE（规划门禁，非 tasks 行） |
| role / phase / stage | reviewer / plan / plan |
| round | 11 |
| agent_context | 第十一个新实例；只读、无 write 工具、未切分支/未提交；与前 10 轮均不同实例；非任何 WP 的 Owner；未继承任何实现对话 |
| target_revision | `sha256:e75f8147c672c4dac372cf9094d489bcba449cdf8602225f90c3186d48600ea8`（本轮 contractDigest，由派发给出；**我无 shell/引擎能力，未自行复算**） |
| scope | ① F48/Path A 是否真正可行、无新洞、三处已一致（含既有 create 先例的基线代码独立复核）；② F47/F49/F50/F51 的处置是否落地；③ 本轮是否引入新的不一致（Coverage 37 行、Waves/Serialization、Shared File Ownership、Main E2E、门禁标记、DCR 最大轮次、tasks↔plan 义务）；④ 独立判断「WP3–WP6 的义务是否还有人面对『两种做法都读得通』或『无人可做』」 |
| changes | 无（只读；未修改任何文件、未暂存、未提交） |
| checks | 规划阶段无 PV/E2E 可跑；本轮只核对 Coverage Index 37 行、`## Verification Strategy`、`### Main E2E` 与三个门禁标记的登记与可产出性（不核对执行结果） |
| issues | **0×CRITICAL / 0×MAJOR**；2 项 MINOR/S 新发现（F52、F53）；F47/F48 复核为**部分闭环**（残留见 §2/§1.4） |
| result | **PASS**（无阻断项） |
| evidence_paths | 本报告（待 main 落盘）；`reports/dr1-dependency-review-round10.md`（本轮触发源）、`…round9.md`、`…round8.md`（`## Target` 摘要历史现值）、`reports/cr2-review.md`（CR2-F3 原文与 `:662` 现场） |
| resource_cleanup | NOT_APPLICABLE（未占用任何构建/端口/数据库资源） |
## Review Context
| Review ID + Round | Review Type / Stage | Work Package | Repository | Base / Target Revision | 读取的规则与需求 | 实际检查范围 | 验证证据与限制 | 报告路径 | 核对的 Check / E2E ID |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DR1 / Round 11 | plan / plan | NOT_APPLICABLE | `D:/Project/acp-remote` | Base = NOT_APPLICABLE；Target = `sha256:e75f8147…00ea8` | `roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md` §1/§3/§4/§5/§7/§10/§12 | 契约集合全文（proposal、5 份 specs 的 37 个 heading、design D1–D6 含 Round 10/Round 11 订正注记）、`plan.md`（Contract Changes 含 Round 11 段 / Coverage Index / Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs 含红窗口段 / Verification Strategy / Main E2E / Completion Criteria）、`tasks.md` 全文、`verification.md` 全文、前 10 轮报告；**基线代码独立复核**：`crates/core/src/broker.rs`（`required_grant` 111–131、`create_session` 1369–1425、`settle_session_create` 1440–1490 与 `:1454-1458` 守卫、`submit_mutation` 774–830、`recover_unsettled` 2329–2355、`recover_command` 2360–2440、`is_active` 2873、`submit_prompt` 831–900）、`crates/core/src/use_cases.rs:243-289`、`crates/core/src/model/session.rs:753-790`、`crates/server/src/node_link/command.rs:2189-2277`（`core_payload()`，含 `:2262-2264` 的 create 早退）、`crates/storage-sqlite/src/session_store.rs`（`verify_idempotent` 849–870、`commit_owned` 876–1030、幂等命中/回填 917–943/1009/1360–1372、`write_command` 1517–1560）、`scripts/check-command-catalog.mjs`（pack/scope/镜像比对）、`compatibility/commands/v1/commands.json:16`、`docs/CORE_PORTS_AND_STORAGE.md`（§3.3 `:112-113`、§4 `:196-202`、§5.1 `:266-269`、§6 第 20 条 `:836-841`、§7.3 `:943-949`、**§9 判据 1/28 `:1327/:1357`**、§11.8 `:1550`）、`docs/MODULE_ARCHITECTURE.md` §4.1 `:170-224` 与 §4.7 `:324-334`、`docs/NODE_LINK_PROTOCOL.md:600-660`；全仓 `impl SessionStore for` / `impl SessionBackendFactory｜SessionEndpoint for` / `SessionResume` / `CommandPayload` grep | 无任何执行证据（规划审查不得冒称已执行测试）。**限制**：① 无 shell → 不能复算 contractDigest 与 `verification.md` 的 6 行 sha256（F47 的判定不依赖复算，见 §2）；② 主工作区停在基线 `81e350f…`，故「现状代码」= 基线代码；WP1/WP2 的交付物只在 `verification.md`/CR1/CR2/merger 报告的记录层核对（本轮不重读 worktree）；③ 不审代码 diff、不判用例集合齐备性 | 本报告（待落盘） | 计划内：Coverage Index 37 行、PV1/PV2、C1/C2（仅核对登记与可产出性）；E2E = not-applicable |
---
## 1. F48 / Path A：**可行、无新洞、三处一致 —— 但第四处（plan 的 Shared File Ownership 区域注记）未同步**（逐条独立核实）
### 1.1 既有 `session.create` 先例（我自己读源码确认，不采信任何回写说明）
| 断言 | 我的独立核对 | 结论 |
| --- | --- | --- |
| core 的 `CommandPayload` 里没有 create 变体，注释明说不变量 | `crates/core/src/model/session.rs:754-755`：「命令 payload：按命令名一对一（§3.3）。`session.create` **不在其中**——它经 `super::CreateSessionRequest` 与 Node Link 的 `session.create` 路径进入（§12.7）。」`enum CommandPayload`（`:757-789`）只有 11 个变体 | ✅ 属实 |
| `core_payload()` 对 create 直接早退 | `crates/server/src/node_link/command.rs:2262-2264`：`WirePayload::SessionCreate(_) => { return Err("session.create is dispatched by its own handler"); }`；该 `match &submit.payload` **穷尽无通配臂**（末臂即它） | ✅ 属实（也是 E0004 的成因） |
| `resume_session` 镜像 create 的形态可实现 | `broker.rs:1369-1425` 的 `create_session` 是「1 次 `authorize` → 自建 `OwnedCommit{ state: Some(StateChange::Create), idempotency: Some(IdempotencyRecord{ command: "session.create", session: None, … }) }` → `commit_owned` → 重放则早退 → 否则 `factory.create`」。`use_cases.rs:243-273` 的 `create_session(actor, request_id, request_fingerprint, request, workspace_alias)` 先把授权与 alias 解析做完再转 broker | ✅ Path A「自建 accepted/幂等行 + 只返回 `SessionId`」与既有形态**同形** |
| `resume_session` 不会误入通用分发 | `broker.rs:774-830` 的 `submit_mutation` 先校验 `command.command == command_name(&command.payload) && kind == command_kind(...)`，再按**变体**派发；Path A 不新增变体 ⇒ resume 永不进入该管线，也就不存在「先副作用再回 accepted」的顺序问题 | ✅ 属实 |
| 崩溃窗口 | `recover_unsettled`（`2329-2355`）对 `unsettled_commands` 的每条 `accepted` 记录只按「有无会话」两分支走 `recover_command`，**无任何命令名特判**（core 内 `"session.create"` 只出现在 `required_grant`/`authorize`/幂等行构造/`settle` 守卫）；`recover_command` 两条分支都写 `command.uncertain` + `terminal_event_id` | ✅ resume 的 accepted 行会被同一趟恢复终结为 `uncertain`（R32） |
| `core_payload()` 新臂确实消除 E0004 | 该函数对 `&submit.payload` 穷尽匹配；WP6 补 `WirePayload::SessionResume(_) => return Err("session.resume is dispatched by its own handler")` 后匹配重新穷尽 | ✅ 消除 WP2 引入的 `E0004`（与 merger 在 `b0a387b…` 上实测的「唯一一个 E0004，无第二成因」一致） |
| 封闭词表门禁不要求 core 有对应变体 | `scripts/check-command-catalog.mjs:250-260` 只用 `parseRequiredGrant` 比对 `crates/core/src/broker.rs::required_grant` 的命令名集合与 grant 取值（WP2 已加 `"session.resume" => "grant.remote-work"` 一条臂）；`commands.json:16` 的 `session.create` 的 `scope` 就等于命令名，因此脚本 `:191` 的 scope 豁免对本变更无影响，`:194` 的 pack 判据已由 D6 改为「按 transport」 | ✅ Path A 下 core 不加变体**不会**触发任何门禁 |
### 1.2 Path A 与 R27/R34 的兼容性
- R34（`specs/node-link-owner-server/spec.md:47`）明写「Owner **先回** `command.accepted`（`result = null`），恢复完成后发 `command.terminal`」，R27（`:7`）要求 `completed` 的 `terminal.result` 必为 `SessionResumeResult`。Path A 的「core 自建 accepted 行 → 适配层投影 → `settle_session_resume` 落同一条持久记录」与 create 完全同构：`settle_session_create`（`broker.rs:1440-1490`）把适配层投影的结果原文写入 `CommandTerminalRecord`，`CORE_PORTS` §6 第 20 条 `:838` 已写明「`completed` 的 `result` 是适配层投影的结果原文」⇒ `command.status` 重查读的是同一条记录，**同形回复成立** ✅
- `resume_session` 只返回 `SessionId` 不投影、不写终态 ✅（design D3 步骤 5、plan WP3 行、tasks 2.3 一致）。
### 1.3 三处（design / plan WP3 行 / tasks 2.3）**完全一致**
| 位置 | 关键措辞 | 结论 |
| --- | --- | --- |
| `design.md:102`（Round 11 注记） | 「**core 不新增 `CommandPayload::SessionResume` 变体**、不改 `command_name`/`command_kind`/`family`、不加通用分发臂；`accepted` 行与幂等行由 `resume_session` 自建；适配层 `core_payload()` 对 `SessionResume` 按 `SessionCreate` 先例早退」 | ✅ |
| `plan.md:295`（WP3 行） | 「**入口形状 DR1-F48：完全镜像 `create_session`**——不新增 `CommandPayload::SessionResume` 变体、不改 `command_name`/`command_kind`/`family`、不加通用分发臂，`accepted` 与幂等行由 `resume_session` 自建」；旧的「payload 变体与分发」已消失 | ✅ |
| `tasks.md:23`（2.3） | 同上逐句，另加「§3.3 的 `CommandPayload` 行注明 `session.create` 与 `session.resume` 均走专用路径、不在枚举中（不新增变体）」「并同步 `crates/core/src/model/session.rs` 注释（补 `session.resume`）」 | ✅ |
| `plan.md:298`/`tasks.md:29`（WP6） | `core_payload()` 新增 `SessionResume` 臂并**按 `SessionCreate` 先例早退**（`return Err("session.resume is dispatched by its own handler")`） | ✅ |
`crates/core/src/model/session.rs:754` 的注释在 Path A 下**仍为真**（core 确实不加变体）；WP3 的义务只是把 `session.resume` 一并写进该注释（`tasks.md:23` 已点名该文件，落在 `crates/core/` 写范围内）✅
### 1.4 ⚠️ 唯一的 F48 残留：`plan.md:323` 的区域注记与上面四处相反
`plan.md` 的 `## Shared File Ownership` 表 `crates/core/src/broker.rs` 行 Region Note 现为（`plan.md:323`）：
> 「WP2 只改 `required_grant` 的一条臂…；**WP3 改 `CommandPayload` 变体、`command_name`、分发与 `resume_session` 用例**，互不重叠。两包分处 W1/W2，实际不同时写」
这与 `plan.md:295`（同一份 plan.md 的 WP3 行）**直接矛盾**：Path A 恰恰是「不新增变体、不改 `command_name`、不加分发臂」。⇒ **F48 只闭环了三处，第四处（登记表的区域描述）未同步**，见 Findings 的复核项。
（所有权仲裁字段本身正确：Writers=WP2,WP3 / Merge Owner=WP3 / Order=WP2→WP3 / Re-verify=WP3: PV1,PV2，且两包分处 W1/W2 不并发 —— 因此**不构成并发同处**，不按 MAJOR 判。）
### 1.5 独立实现点映射（我自己 grep，确认无「无主实现点」）
全仓实测 16 个相关 `impl` 站点，全部落在某个 WP 的 crate 写范围内：
| 实现点 | 文件:行 | 归属 |
| --- | --- | --- |
| `SessionStore for FakeStore` | `crates/core/src/broker.rs:4412` | WP3（`crates/core/`） |
| `SessionBackendFactory for FakeBackend` / `SessionEndpoint for FakeEndpoint` | `crates/core/src/broker.rs:5300/5335` | WP3 |
| `SessionStore for SqliteStore` | `crates/storage-sqlite/src/session_store.rs:1942` | WP4 |
| `SessionBackendFactory for AgentHost` / `SessionEndpoint for Endpoint` | `crates/agent-host/src/host.rs:449` / `session.rs:746` | WP5 |
| `SessionStore for NotTouched` / `FixedStore`、`SessionBackendFactory for NotTouched` | `crates/server/src/local_admin/test_support.rs:817/961/1021` | WP6（整 `crates/server/`） |
| `SessionStore for CommandStore`、`…Factory for FakeBackends`、`…Endpoint for FakeEndpoint` | `crates/server/src/node_link/command/tests.rs:145/437/484` | WP6 |
| `SessionStore for SliceStore` | `crates/server/src/node_link/resource/tests.rs:265` | WP6 |
| `SessionStore for FlakySessionStore`、`…Factory for ScriptedBackends`、`…Endpoint for ScriptedEndpoint` | `crates/app/tests/support/owner.rs:416/504/619` | WP6（`crates/app/`） |
`load_recovery` 的 5 个替身（server 四个 + `FlakySessionStore`）与 `NotTouched`/`ScriptedBackends` 的新方法在 plan WP6 行与 tasks 2.6② 都有主；`FakeBackends`/`FakeEndpoint` 未逐个点名但同属 WP6 整 crate 范围且在 `cargo test -p server` 的编译面内 ⇒ **无无主义务** ✅（细节见 §7）
**§1 结论：Path A 成立、与 create 完全同形、R27/R34/R32 均可满足、封闭词表门禁不冲突、E0004 被消除；唯一残留是 `plan.md:323` 的区域注记未随 Path A 同步（F48 部分闭环）。**
---
## 2. F47 复核：**部分闭环**（现值行已刷新，括注仍指向 CR7 的旧「新值」）
| 核对项 | 现状 | 结论 |
| --- | --- | --- |
| `verification.md:14-19` 的 6 行现值 | 现为 `proposal 7072345b…`、`acp 88ec88fe…`、**`local-agent-host 99c6eb8e7a7c5cbcd13abdfa45a5b5c1986ccb293a3430424541dcc4fbd5092f`**、`node-link-owner-server 2b1a726a…`、`storage-schema-v2-migration e94efd20…`、`workspace-resolution 45b5b693…`。第 16 行已不再是 Round 8 记录的 `1c0b1875…`（Round 10 的 F47 现场引述值） | ✅ 第 16 行**已刷新**（与「整块重写 6 行」的声明一致；我无 shell，未复算数值） |
| `verification.md:22` 的括注 | 仍写「`specs/local-agent-host/spec.md`（`6f7d725d…` → **`1c0b1875…`**，CR7-F1）」——与同节第 16 行的现值 `99c6eb8e…` **不一致**；且整条括注只列到 CR7，**完全没提 Round 10 的 R5 措辞改动** | ❌ **内部自相矛盾**（F33 同类缺陷复发） |
| 该文件确实在本轮之前被改 | `specs/local-agent-host/spec.md:7` 现文为「由 core 在**同一会话创建流程中的提交**里自己持久化（该提交不一定是被终态结算的那一次）」，即 Round 10 的 R5 措辞（`plan.md:82` 的 F46 行 / Round 11 表自述） | ✅ 文件已变 ⇒ 现值 ≠ CR7 值 |
| 节标题 | `verification.md:11` 仍写「已按 **CR7 的两次契约修正**刷新，DR1 Round 7」 | ❌ 未记录 Round 10 的 R5 改动 |
⇒ **F47 部分闭环**：机械核对所依赖的*现值行*已经正确，但*图例*（括注 + 节标题）仍把 CR7 的值当作当前值，读者按括注会得出错误结论。修法：把括注中 `local-agent-host` 的箭头目标改为 `99c6eb8e…`（并注明这是 DR1 Round 10 的 R5 措辞改动，`1c0b1875… → 99c6eb8e…`），节标题补「与 DR1 Round 10 的 R5 措辞订正」。该文件不进 `contractDigest`，修它不必再开轮次。
---
## 3. F49 复核：**已按「只写 §5.1 的 `[决定]`」闭环**
- `tasks.md:23`：`settle_session_resume`「…**只**终结 `session.resume` 记录；其幂等语义**只写在** `docs/CORE_PORTS_AND_STORAGE.md` §5.1 的 `[决定]`，**不改 §6 第 20 条**」✅
- `plan.md:295`（WP3 行的文档区域）：`§2、§3.1、§3.3、§3.6、§4、§5.1、§5.2` —— **不含 §6** ✅；`plan.md:328`（Shared File Ownership 行）同样只列这些区域 ✅
- 落点真实性：`docs/CORE_PORTS_AND_STORAGE.md:266-269` 的 §5.1 `[决定]` 段确实承载端口级决定（`EventSink`、`TurnAccepted.turn`、`read_history` 分流、`create_session` 的解析与幂等义务），把 `settle_session_resume` 同形语义加在 `:269` 一带的 `[决定]` 内是**同质落点** ✅
- 残留（不另立 finding，记录层）：`plan.md:90` 的「触及契约摘要」列对 F49 写「✅ tasks」，但 `design.md:104` 也带着 F49 的闭环段落 ⇒ 准确写法是「✅ design/tasks」。整体「代价：触及 design/plan/tasks」的结论仍然成立，不影响任何判定。
---
## 4. F50 复核：**已闭环**（且 §4.1 确实含需要同步的清单）
- 写范围：`plan.md:295` WP3 写范围新增 `docs/MODULE_ARCHITECTURE.md（§4.1，DR1-F50）`；`tasks.md:23` 同步（「会话后端端口行补 `resume`/`agent_session_id`、`SessionStore` 行补 `load_recovery`、值对象清单加 3 项」）✅
- 登记：`plan.md:340` 新增 Shared File Ownership 行（Writers=WP3 / Merge Owner=WP3 / Order=WP3 / Re-verify=PV2，单一写者，区域=§4.1）✅
- 事实核对（我自己读 `docs/MODULE_ARCHITECTURE.md:170-224`）：§4.1 的端口清单确有 `SessionBackendFactory 受约束地 create/open SessionEndpoint`、`SessionEndpoint prompt/…/close`、`SessionStore 原子提交…；提供 head 与一致性读视图`，值对象块之后写明「以上是**冻结全量**」（未含 `AgentSessionId`/`ResumeSessionRequest`/`SessionRecoveryRecord`）⇒ **同步确有必要**，F50 的事实依据成立 ✅
- §4.7（`docs/MODULE_ARCHITECTURE.md:324-334`）**只**描述 storage-sqlite 的职责与表族隔离，不列举端口方法，本变更无需改它（已核对，不另立 finding）；其中 `v1 → v2 升级` 的措辞属 v3/v4 时期的既存陈旧，非本变更引入。
---
## 5. F51 复核：**已闭环（写范围 + 登记 + 波次）**；处置栏有一处位置标签残留（→ F53）
| 核对项 | 现状 | 结论 |
| --- | --- | --- |
| WP6 写范围 | `plan.md:298`：`… docs/NODE_LINK_PROTOCOL.md（仅 §10 收窄句与 §12.7 示例注记，DR1-F51）`；`tasks.md:29` 额外义务④逐字写明两处（`:662` 收窄句 + §12.7 示例加「可见版本为 2」的非规范注记） | ✅ |
| Shared File Ownership | `plan.md:341`：`WP2, WP6 ｜ WP6 ｜ WP2 → WP6 ｜ WP6: PV2 ｜ …两包分处 W1/W4，不并发` —— 与派发要求（Writers=WP2,WP6 / Merge Owner=WP6 / Order=WP2→WP6 / Re-verify=WP6: PV2）**逐项相符** | ✅ |
| 两包是否分处不同波次 | WP2 在 W1（`plan.md:302`），WP6 在 W4（`plan.md:305`）⇒ 不并发 ✅ | ✅ |
| 收窄句是否真的存在、是否在 §12.7 | 基线 `docs/NODE_LINK_PROTOCOL.md:648` 即该句「本切片的 Owner 只为 `session.list` 与 `session.create` 投影 wire 结果…」，位于 §12.7（`### 12.8` 之前）；CR2 在其 target 版本记到 `:662`（WP2 在该段前新增了 §12.7 内容，行号前移）⇒ plan/tasks 的 `:662` 与 CR2-F3 的现场一致 ✅ | ✅ |
| `verification.md` 的 CR2-F3 处置栏 | 处置**实质**已与现状一致（「由 WP6 的文档收口一并处理」——本轮已把该文件加进 WP6 写范围）；但位置标签写「**F3（§10 措辞）**」，与 CR2-F3 自己的定位（`NODE_LINK_PROTOCOL.md:662`，`§12.7` 结果投影收窄句）不符，且未记录「该文件已入 WP6 写范围」 | ⚠️ → **F53（SUGGESTION）** |
---
## 6. 本轮是否引入新的不一致：逐项核对
| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| Coverage Index 37 行 ↔ 5 份 specs 标题逐字 | ✅ **37/37** | 我抓出全部 `^### Requirement:`/`^#### Scenario:`（acp 4 + local-agent-host 8 + node-link-owner-server 11 + storage 10 + workspace 4 = 37），与 `plan.md` 的 37 行 `source.heading` 逐条比对，含反引号、全角括号「（`session/resume`）」、`v4 到 v5…`、`能力未宣告时显式不支持且不发送恢复请求`，全部逐字命中 |
| Coverage 的 `tasks`/`checks` 引用实存 | ✅ | `tasks` = {2.1–2.6, 4.2} 全部在 `tasks.md` 存在；`checks` = {PV1, PV2} 均在 `## Verification Strategy` 定义 |
| Execution Waves 层级 | ✅ | W1 = WP1/WP2/TP1（无依赖）→ W2 = WP3（`code:WP2`）→ W3 = WP4（`code:WP3`）/WP5（`code:WP1+WP3`）→ W4 = WP6（五上游）→ W5 = TP2（六上游）；每层严格晚于其 code 依赖层；W2 行已定义「已验收集成基线」并有一行术语指向 |
| Serialization Reason | ✅ | 9 行全为 `NOT_APPLICABLE`；逐波写集合无交集（W2：core + `CORE_PORTS…` §2/§3.1/§3.3/§3.6/§4/§5.1/§5.2 + `MODULE_ARCHITECTURE` §4.1；W3：storage-sqlite + §7 标题/§7.2/§7.3、agent-host；W4：server/app/NODE_LINK）；同文件跨波者的 Merge Owner/Order/Re-verify 均有登记 |
| Shared File Ownership 全部行 | ✅（除 §1.4 的区域描述） | 22 行、无重复文件；每行 Merge Owner / Merge Order / Re-verify 齐备；新增的 `docs/MODULE_ARCHITECTURE.md`（WP3 单写）与 `docs/NODE_LINK_PROTOCOL.md`（WP2,WP6）两行与 F50/F51 的处置一致；`crates/core/src/broker.rs` 行的**所有权字段**正确（见 §1.4） |
| Main E2E 四要素 | ✅ | `mode: not-applicable` + `reason` + `basis` + 非空 `alternative_checks: [C1, C2]` + `downgrade_approval`（含用户原话/时间/来源）；C1/C2 在 `verification.md` 的 `## Checks` 各有行 |
| 三个门禁标记唯一 | ✅ | `[validation]`（tasks 7.1）、`[e2e-owned]`（8.3）、`[final-verification]`（9.1）各出现一次、互不重叠 |
| tasks 2.3/2.4/2.6 与 plan 的义务逐条对齐 | ✅（一处未点名，已闭合） | 2.3 = plan WP3 行逐条（含 F48 入口形状、F49 §5.1、F45 §3.3/`session.rs` 注释、F50 §4.1）；2.4 = load_recovery + §7 + 既有测试字面量；2.6 = ①②③④ + `core_payload` 早退 + 受控路径 + 三份交付级文档。**未点名项**：plan WP6 行点名 `ScriptedBackends` 的新方法，tasks 2.6 只写「收口跨 crate 编译涟漪」；因 `crates/app/` 整体属 WP6 且 PV1 含 `-p app`，不产生无主义务（记录层提示，不立 finding） |
| `## Dependency Declaration Review` 的最大轮次行 | ✅（待填，属 main） | 表中已有 `DR1 ｜ 11 ｜ reviewer-DR（待派发…）｜ 待填（F47–F51 处置后的新 contractDigest）｜ 待判`——回填前 `workflow check --stage plan` 会因最大轮次不是裸 PASS 而红，这是本轮的正常中间态；Round 10（PASS）行已就位 |
| `## Target` 摘要 | ❌ 括注陈旧 | → F47（§2） |
| 本轮是否把「实现的现实」正确写进契约 | ✅（1 处区域注记残留） | F48 的 Path A（除 `plan.md:323`）、F49 的 §5.1 落点、F50 的 §4.1 归属、F51 的两处写权均属实且自洽；F52/F53 为本轮**新发现**的既存缺口（非本轮引入） |
---
## 7. 独立判断：coder-C（及其后各包）是否还会遇到「两种做法都读得通」或「无人可做」？
我把 resume 相关义务重新摊平，逐条映射「谁写」：
| 义务 | 落点 | 有主？ |
| --- | --- | --- |
| `AgentSessionId` / `ResumeSessionRequest` / `SessionRecoveryRecord` | `crates/core/`（WP3）+ `CORE_PORTS` §3.1/§3.3/§3.6（WP3） | ✅ |
| `SessionBackendFactory::resume`、`SessionEndpoint::agent_session_id` | 定义 WP3；实现 WP5（agent-host）+ WP3（core fakes）+ WP6（`crates/server`、`crates/app` 的替身） | ✅（16 个 impl 站点已 grep 全归主，见 §1.5） |
| `SessionStore::load_recovery` | 定义 WP3；实现 WP4（SqliteStore）+ WP3（FakeStore）+ WP6（5 替身） | ✅ |
| `UnavailableKind` 新取值（`ALL`/`as_str`/`port_error_public`） | `crates/core/`（WP3） | ✅ |
| core→wire 错误映射（`port_error_code` 一条臂） | `crates/server/`（WP6 义务①） | ✅ |
| `settle_session_resume`（同形、只终结 `session.resume`） | WP3 | ✅ |
| `resume_session` 用例（授权先于读取、窄读取、cwd 复校验、`factory.resume`、返回 `SessionId`、自建 accepted/幂等行） | WP3 | ✅ |
| `SessionResumeResult` 投影 + 调用 settle + 投影失败结 `uncertain` | WP6 义务③ | ✅ |
| 两列落盘提交点（create 流程内一次 `StateChange::Update`） | WP3 | ✅ |
| v5 DDL/migration/读写、`docs/CORE_PORTS…` §7 | WP4 | ✅ |
| agent-host 恢复路径与能力门控、fake ACP agent 场景 | WP5 | ✅ |
| `core_payload` 早退 + 路由 + 受控路径用例 | WP6 | ✅ |
| 文档：`CORE_PORTS` §2/§3.1/§3.3/§3.6/§4/§5.1/§5.2（WP3）、§7（WP4）、`MODULE_ARCHITECTURE` §4.1（WP3）、`NODE_LINK` 两处 + `SESSION_CONTINUITY`/`README`/`DEVELOPMENT_PLAN`（WP6） | — | ✅ |
| **`CORE_PORTS` §9 判据 1/28 的「v1 → … → v4」版本链与 owned 列清单断言；文件头的版本记录** | **无任何 WP 拥有** | ❌ → **F52** |
**回答具体问题：**
1. **「无人可做」：只剩文档层一项**——`docs/CORE_PORTS_AND_STORAGE.md` §9（验收判据）与文件头版本记录不在任何写范围内，而 v5 落地后它们会与 §7.2 的新升级段自相矛盾（F52）。所有**代码**义务都有主，且我自己枚举了 16 个实现点，无遗漏。
2. **「两种做法都读得通」：有三处，但都不是阻断，且每一处的替代读法都能满足需求**：
   - **a. `resume_session` 的形参表**（会话 id 的位置、是否需要额外参数）：design D3 只定义语义顺序（授权 → 读取 → 复校验 → `factory.resume` → 返回 `SessionId`），未写死签名。这不构成二次 BLOCKED：WP6 依赖 WP3 且会读 WP3 的代码，属模块内实现细节（AGENTS.md §1「普通模块内部实现细节可以自主决定」）。
   - **b. 接受提交是否同时写一次 `StateChange::Update` 把会话状态抬回可交互**：我核对了 `submit_prompt`（`broker.rs:831-900`）——它只拒绝「会话行不存在」（`session.not_found`），**不拒绝 `Closed`**，且 `is_active(Closed) = false` 会正常把状态推到 `Queued`（`:2873`、`:885-890`）。因此「只绑定端点、不写状态」与「写一次 Update」都能满足 R35/R9 的「随后可接受 `session.prompt`」，差别只在持久状态字面值。
   - **c. 幂等行的 `session` 分量写 `Some(session)` 还是 `None`**：两条都安全——`session_store.rs:1360-1372` 会把 `idem.session = None` 用本次提交的 `session_id` **回填**，`verify_idempotent`（`:856-861`）对 `None` 跳过会话比对，二者语义等价。唯一差别：写 `None` 会让 `CORE_PORTS` §6 第 20 条 `:837` 的括注「（目前只有 `session.create`）」在字面上失真，而 **§6 没有任何写归属**（F49 已裁定不扩到 §6）。→ 建议（非阻断）：在 WP3 行/tasks 2.3 补一句「幂等行携带目标会话 `session: Some(<会话 id>)`」，即可同时保持 §6 该句为真。
   - 结论：Path A 之后，coder-C 面对的是**可自主裁定的实现细节**，不是 Round 9 那种「两种做法互斥、一种撞墙」的契约歧义；**不会再触发第二次 BLOCKED**。
---
## Findings
### 复核项（原问题 ID 沿用）
| ID | Severity（原） | 复核依据 | Recheck 结论 |
| --- | --- | --- | --- |
| DR1-F41 | MAJOR | 与 Round 10 相同的三条前提我重新独立复现（`settle_session_create` 守卫 `broker.rs:1454-1458`；`use_cases.rs` 唯一命令终态入口；适配层投影 export）；`settle_session_resume` 与 create 同形、WP3/WP6 各覆盖一半且不新增重叠 | **已解决**（回归检查：Path A 未削弱该结论） |
| DR1-F42 | MINOR | 我自己 grep 出 16 个相关 impl 站点，7 个 `SessionStore` 实现 / 6 个文件与 design `:112`、plan WP6 行、tasks 2.6 的计数与点名一致 | **已解决** |
| DR1-F43 | MINOR | tasks 2.3/2.4/2.6/3.7 与 plan 义务逐条对齐（含 F48/F49/F45/F50 的新义务） | **已解决** |
| DR1-F44 | MINOR | 红窗口段（`plan.md:356`）仍写「三个必需 trait 方法」+ storage-sqlite 新实现 + 四个 server 替身 + `FlakySessionStore` + `port_error_code` 一条臂；与 §1.5 的 grep 事实逐项相符 | **已解决** |
| DR1-F45 | SUGGESTION | §3.3/§3.6/§4 的登记 + `crates/core/src/model/session.rs` 注释**已进 tasks 2.3（补 `session.resume`）**，注释层不再无主 | **已闭环** |
| DR1-F46 | SUGGESTION | R5 正文与 Scenario 一致（`:7`/`:9-11`）；design D2 记录三项可观察后果；§12.7 示例读法已随 F51 归入 WP6 义务④ | **已闭环** |
| **DR1-F47** | MINOR | `verification.md:16` 现值行已刷新为 `99c6eb8e…`（不再是 Round 8 记录的 `1c0b1875…`），但 `:22` 的括注与 `:11` 的节标题仍把 CR7 值当成当前值、且未提 Round 10 的 R5 改动 ⇒ 同节内部矛盾复发（F33 同类） | **部分闭环**（残留：括注/节标题，见 §2） |
| **DR1-F48** | MINOR | design `:102`、plan WP3 行 `:295`、tasks 2.3 三处已与 Path A 完全一致；但 `plan.md:323`（Shared File Ownership 的 `crates/core/src/broker.rs` 区域注记）仍写「WP3 改 `CommandPayload` 变体、`command_name`、分发」 ⇒ **同一份 plan.md 内自相矛盾** | **部分闭环**（残留：`plan.md:323`，见 §1.4） |
| DR1-F49 | SUGGESTION | 幂等语义只落 `docs/CORE_PORTS_AND_STORAGE.md` §5.1 的 `[决定]`（WP3 范围内），plan/tasks 均明写「不改 §6 第 20 条」；记录层小瑕疵：`plan.md:90` 的「触及契约摘要」列应为 design/tasks | **已闭环** |
| DR1-F50 | MINOR | WP3 写范围 + `plan.md:340` 登记齐备；我独立确认 §4.1 确实含需同步的端口行与「冻结全量」值对象清单 | **已闭环** |
| DR1-F51 | MINOR | WP6 写范围（两处）+ `plan.md:341` 登记（Writers/Merge Owner/Order/Re-verify 全备，W1/W4 不并发）；`verification.md` 的 CR2-F3 处置栏实质一致、位置标签待订正 | **已闭环**（标签残留 → F53） |
### 新发现（本线程连续编号）
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation（最小修复面） |
| --- | --- | --- | --- | --- | --- |
| **DR1-F52** | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:1327`（§9 判据 1）与 `:1357`（§9 判据 28）；同文件头部版本记录（`:17-22` 一带）；plan WP4 的写范围（`plan.md:296`）与 Shared File Ownership 区域注记（`plan.md:328`） | v5 落地后：判据 1 写「`fixtures/storage/v2/from-v1.sqlite3` 的 **v1 → v2 → v3 → v4** 连续升级按判据 28 断言保留性」、判据 28 写「**v4 的追加列**不改变以上任何一条，并额外断言：① 升级库与新建库的 **owned** 家族列清单…逐项相等，`owned_node.export_ids_json` 在末尾」——版本链与列清单断言都停在 v4；而 WP4 的写范围只到「§7 标题、§7.2、§7.3」，**§9 与文件头不在任何 WP 的写范围内**（8 个包的写范围逐行核对）。先例明确：v3→v4 的变更（文件头 `版本：0.14`）**同批**做了「§9 新增判据 32 并同步判据 1/28 的版本链与 owned 列清单断言」。`check-contract-drift` 只比 §5/§7，**这类漂移不会被任何门禁发现** | 交付后 CORE_PORTS 内部自相矛盾：§7.2 会写 v4→v5 与新的版本常量，而 §9 的验收判据仍按「链到 v4、只有 v4 追加列」描述；迁移测试的「黄金列清单/版本链」判据在文档上没有对应更新，validator 的覆盖充分性核对（7.1）会读到过时判据 | 把 WP4 的 `CORE_PORTS` 区域扩为「§7 标题、§7.2、§7.3、**§9 判据 1/28 的版本链与 owned 列清单断言**」，并在 `plan.md:328` 的区域注记同步（§9 与 WP3 的 §2/§3.x/§4/§5.x 不相交，不新增重叠、不改变 Merge Owner/Order）。可选：由后写者 WP4 按既有惯例在文件头追加「版本：0.15」一条。若 main 判定 §9 不需要改，请在该处写明「判定不改」的理由（与 F50 的处置口径一致） |
| **DR1-F53** | SUGGESTION | `verification.md` 的 `## Review Findings` 中 CR2-F2/F3/F4/F5 行（`verification.md:193`）：Location 列与 Resolution 列都把 F3 记为「**NODE_LINK §10 措辞**」 | CR2-F3 自己的位置是 `docs/NODE_LINK_PROTOCOL.md:662`（§12.7 结果投影收窄句「本切片的 Owner 只为 `session.list` 与 `session.create` 投影 wire 结果」，基线行号 `:648`，WP2 改动后为 `:662`），与 §10 的 grant→命令表（DR1-F11 的对象）不是一回事；处置栏也未记录「该文件已加入 WP6 写范围」（F51 的实际处置）与 §12.7 示例注记一件 | 最终验收按 ID 核对 CR2-F3 闭环时会读到错误的位置描述（§10 已由 WP2 处理完毕，容易被判为「已闭环」而漏掉 `:662` 的真正对象）。门禁不覆盖此句 | 把该行的「§10 措辞」改为「§12.7 的结果投影收窄句（`:662`）」，并在 Resolution 补「已把 `docs/NODE_LINK_PROTOCOL.md` 加入 WP6 写范围（两处：`:662` 收窄句、§12.7 示例版本注记；DR1-F51）」。只改 `verification.md`，不进 `contractDigest` |
**New findings 汇总：0×CRITICAL、0×MAJOR、1×MINOR（F52）、1×SUGGESTION（F53）。**
---
## Check Plan 核对
- 本轮为 **plan 类型**，计划内检查 = `plan.md` 的 `## Coverage Index`（37 行）、`## Verification Strategy`、`### Main E2E` 决策与三个门禁标记。逐行核对结果见 §6：37/37 `source.heading` 逐字、`tasks`/`checks` 引用实存、Waves 层级合法、9 行 Serialization Reason 全 `NOT_APPLICABLE` 且同波无写入交集、Shared File Ownership 22 行齐备、Main E2E 四要素齐备、三门禁标记唯一。
- **不存在影响本轮判断的待返回执行证据**：PV1（阶段 1/阶段 2）、PV2、C1/C2 的结果行属后续阶段；`verification.md` 的 `## Checks` 目前只登记阶段/范围/证据路径，未冒称通过。**plan review 的结论不覆盖任何 PV/E2E 是否通过**。
- 本轮未改任何计划字段（本实例只读），故无新的 Check Plan Changes 需 reviewer 记录；`verification.md` 现有四条（Round 4/6/8/… ）与机器事实一致。
- 记录层待办（属 main，不是计划缺陷）：① DCR 的 Round 11 行仍为「待判/待填」，回填前 plan 门禁会红；`## Review Findings` 还需补 F47–F53 的复核/处置行。
## Assessment
- **本轮检视结论：PASS**，对应 `target_revision` = `sha256:e75f8147…00ea8`。**0×CRITICAL、0×MAJOR。**
- **F48/Path A 成立**（§1）：我在基线代码里逐条复现了 create 先例（`session.rs:754-755` 注释 + 11 个变体、`command.rs:2262-2264` 的早退、`broker.rs:1369-1425` 的「自建幂等行 → 提交 → 再工作」形态、`submit_mutation` 的按变体派发、`recover_unsettled` 的命令名无关恢复、`check-command-catalog.mjs` 只镜像 `required_grant`），确认 Path A 与 create 完全同形、满足 R27/R34/R32、消除 E0004，且 16 个实现点全部有主。**三处（design / plan WP3 行 / tasks 2.3）已完全一致**；残留的第四处 `plan.md:323` 区域注记与 Path A 相反（F48 部分闭环，MINOR）。
- **F49/F50/F51 已闭环**（§3–§5），F47 **部分闭环**（§2：现值行已刷新、括注与节标题仍为 CR7 值）。
- **非阻断项清单（按建议处理顺序）**：① **F48 残留**（`plan.md:323`：把「WP3 改 `CommandPayload` 变体、`command_name`、分发」改为「WP3 改 `resume_session`/`settle_session_resume` 用例、错误枚举与端口（**不新增变体、不改 `command_name`/`kind`/`family`**）」——**派发 WP3 前改**，因为 coder-C 已因契约歧义 BLOCKED 过一次）；② **F47 残留**（`verification.md:22` 括注 + `:11` 节标题）；③ **F52**（`CORE_PORTS` §9 判据 1/28 + 文件头无写归属）；④ **F53**（CR2-F3 处置栏位置标签）。①②③ 都触及 `plan.md`/`verification.md`；若在**同一批**里做完，只需**再一轮**（Round 12）独立复核即可放行 WP3（`plan.md` 与 `design.md` 的改动会让本轮 digest 失效）；若只改 `verification.md`（②④），本轮 PASS 对当前 digest 继续有效。
- **F52/F53 的引入时点**：两者都是**既存缺口**（不是本轮引入）——F52 的 §9/文件头从未进过任何写范围（历轮未报）；F53 的标签自 CR2 处置栏写入时即如此。按「新发现继续在同一 Review ID 下连续编号」的规则记为本轮 F52/F53。
- **待补证据与门禁**：PV1（各 WP 阶段 1；集成基线/候选/主分支阶段 2）、PV2 与 C1/C2 的 PASS 行须在对应交付/验证门禁前补齐；本轮为规划审查，不要求也不核对执行证据。
- **复用依据**：本轮不复用任何历史轮结论作为结论依据；`round8.md`（`## Target` 摘要历史现值 `1c0b1875…`）、`round9.md`/`round10.md`（F41/F48 的判据与现场引述）、`cr2-review.md`（CR2-F3 的 `:662` 现场）仅作核对对象。历史轮次保留不覆盖，当前结论以 Round 11（本报告）为准。
- **受阻/限制**：无 shell/引擎 → 未复算 contractDigest 与 `verification.md` 的 6 行 sha256（F47 的判定基于「同一节内现值行与括注互相矛盾」这一不依赖复算的事实）；主工作区为基线 `81e350f…`，故本轮以基线代码 + 记录层核对为准（WP1/WP2 的交付内容通过 merger/CR1/CR2 的现场记录交叉印证，未重读 worktree）。
## 给 main 的回填指引（记录层）
1. 把本报告原样落盘到 `openspec/changes/session-resume/reports/dr1-dependency-review-round11.md`（**先落盘**，否则路径不可读，重演 F19/F25/F37）。
2. `## Dependency Declaration Review` 的 Round 11 行把 `Reviewer` 填为本实例的实际 ID/隔离说明、`Plan Revision` 填 `sha256:e75f8147c672c4dac372cf9094d489bcba449cdf8602225f90c3186d48600ea8`、`Result` 单元格保持**裸 `PASS`**（门禁判据是「去空白后忽略大小写必须恰好等于 PASS」）、`Report Path` 填上面的路径。
3. `## Review Findings` 增补 F47–F53：F49/F50/F51 记「DR1 Round 11 复核：已闭环」；F47/F48 记「部分闭环」并写明残留位置（`verification.md:22`/`:11`；`plan.md:323`）；F52（MINOR）与 F53（SUGGESTION）按本报告表格逐列落。
4. `## Check Plan Changes` 记一条：本轮 main 的处置范围（F48 触 design/plan/tasks ⇒ digest 变化 ⇒ 需 Round 11；F49 触 tasks/design；F50/F51 触 plan；F47 只触 verification.md），以及是否把 F47 残留/F52/F53 一并做（决定是否再开 Round 12）。
5. 派发 WP3 前请把两条写进 coder-C 的派发提示：① **Path A 是可执行口径**（core 不加 `CommandPayload` 变体、不改 `command_name`/`kind`/`family`、不加分发臂；`accepted` 与幂等行自建；`resume_session` 只返回 `SessionId`）；② 幂等行携带目标会话 `session: Some(<会话 id>)`（有助于保持 §6 第 20 条 `:837` 的字面为真）。
```yaml
handoff_index:
  - task_id: NOT_APPLICABLE
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    stage: plan
    round: 11
    target_revision: "sha256:e75f8147c672c4dac372cf9094d489bcba449cdf8602225f90c3186d48600ea8"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round11.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "针对 F48 的 Path A 裁定与 F47/F49/F50/F51 处置后的当前 contractDigest 做静态规划审查（第十一个新实例、只读、未参与任何实现或用例设计）：独立在基线代码里复现 create 先例（crates/core/src/model/session.rs:754-755 的「session.create 不在 CommandPayload」+ 11 个变体；crates/server/src/node_link/command.rs:2262-2264 的早退臂；crates/core/src/broker.rs:1369-1425 的自建幂等行→提交→再工作形态；broker.rs:774-830 的按变体派发；broker.rs:2329-2355 的命令名无关恢复；scripts/check-command-catalog.mjs 只镜像 required_grant），确认 Path A 可行且与 create 同形；grep 全仓 16 个相关 impl 站点全部落在各 WP 的 crate 写范围内（无无主实现点）；核对 design:102 / plan:295 / tasks:23 三处与 Path A 完全一致，并发现第四处 plan.md:323 的区域注记仍与 Path A 相反（F48 部分闭环）；核对 Coverage Index 37/37 heading 逐字、Waves/Serialization/Shared File Ownership 22 行/Main E2E/三门禁标记/DCR 最大轮次；F49 落点核实为 CORE_PORTS §5.1 的 [决定]（WP3 范围内）、F50 的 §4.1 确含需同步的端口行与冻结值对象清单、F51 的 WP6 写范围与登记（WP2,WP6｜WP6｜WP2→WP6｜WP6: PV2，W1/W4 不并发）齐备；新报 F52（CORE_PORTS §9 判据 1/28 的版本链与 owned 列清单断言/文件头无写归属，v5 后与 §7.2 自相矛盾且无门禁覆盖）与 F53（verification 的 CR2-F3 处置栏把位置标签记为 §10，实际为 §12.7 :662）。未执行任何 PV/E2E，未复算 contractDigest 与 6 行 sha256（无 shell），已声明该限制"
    source_evidence: NOT_APPLICABLE
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "返回 DR1 Round 11 完整复核报告：Review Context + §1–§7 逐项结论 + Findings（复核项 + 新发现）+ Check Plan 核对 + Assessment + handoff_index。关键结论：F48/Path A 经基线代码独立复核判定可行（session.rs:754-755、command.rs:2262-2264、broker.rs:1369-1425/774-830/2329-2355、check-command-catalog.mjs:250-260），design:102/plan:295/tasks:23 三处一致，但 plan.md:323 的区域注记仍与 Path A 相反 → F48 部分闭环；F47 部分闭环（verification.md:16 现值行已刷新为 99c6eb8e…，而 :22 括注与 :11 节标题仍把 CR7 的 1c0b1875… 当当前值）；F49 已按「只写 §5.1 的 [决定]、不改 §6」闭环；F50 已闭环且 §4.1 确含需同步清单；F51 已闭环（WP6 两处写范围+SFO 行 Writers=WP2,WP6/Merge Owner=WP6/Order=WP2→WP6/Re-verify=WP6: PV2，W1/W4 不并发），CR2-F3 处置栏实质一致但标签记为 §10 → F53；新报 F52（MINOR：CORE_PORTS §9 判据 1/28 与文件头无写归属）与 F53（SUGGESTION）；0×CRITICAL/0×MAJOR → PASS"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "NOT_APPLICABLE（只读规划审查，无 shell/写权限，未执行任何构建或测试）",
      "result": "not-run",
      "summary": "规划审查不执行 PV1/PV2/C1/C2/E2E；本轮结论不覆盖任何执行证据。需 main 在对应交付/验证门禁前补齐 PV1（各 WP 阶段 1 + 集成基线/候选/主分支阶段 2）、PV2 与 C1/C2 的 PASS 行"
    }
  ],
  "validationOutput": [
    "F48/Path A 前提独立复现：crates/core/src/model/session.rs:754-755「命令 payload：按命令名一对一（§3.3）。session.create 不在其中」+ :757-789 共 11 个变体（无 SessionResume）；crates/server/src/node_link/command.rs:2262-2264 `WirePayload::SessionCreate(_) => return Err(\"session.create is dispatched by its own handler\")`，且该 match &submit.payload 穷尽无通配臂（E0004 成因）；crates/core/src/broker.rs:1369-1425 的 create_session = authorize → 自建 OwnedCommit{state: StateChange::Create, idempotency: IdempotencyRecord{command:\"session.create\", session:None}} → commit_owned → replayed 早退 → factory.create；:774-830 的 submit_mutation 依 command_name/command_kind 校验后按变体派发；:2329-2355 的 recover_unsettled 无命令名特判（两条分支都写 command.uncertain）",
    "Path A 与门禁不冲突：scripts/check-command-catalog.mjs:250-260 只用 parseRequiredGrant 比对 crates/core/src/broker.rs::required_grant 的命令名集合与 grant 值（WP2 已加 \"session.resume\" => \"grant.remote-work\"）；:191 的 scope 豁免对 session.create 是空转（commands.json:16 的 scope 就等于命令名）；:194 的 pack 判据已按 D6 改为「transport 不含 sync」",
    "R27/R34/R32 兼容：specs/node-link-owner-server/spec.md:47 要求「Owner 先回 command.accepted（result=null），恢复完成后发 command.terminal」、:7 要求 completed 的 terminal.result 为 SessionResumeResult；core 侧同形先例是 settle_session_create（broker.rs:1440-1490）+ CORE_PORTS §6 第 20 条 :838 的「completed 的 result 是适配层投影的结果原文」⇒ 同一条持久记录保证 command.status 同形回复",
    "实现点全归主（自 grep 16 处）：core/broker.rs:4412/5300/5335（WP3）；storage-sqlite/src/session_store.rs:1942（WP4）；agent-host/src/host.rs:449、session.rs:746（WP5）；server/src/local_admin/test_support.rs:817/961/1021、server/src/node_link/command/tests.rs:145/437/484、server/src/node_link/resource/tests.rs:265（WP6）；app/tests/support/owner.rs:416/504/619（WP6）",
    "F48 残留（同一份 plan.md 内自相矛盾）：plan.md:323 的 Shared File Ownership 区域注记写「WP3 改 CommandPayload 变体、command_name、分发与 resume_session 用例」，而 plan.md:295（WP3 行）、design.md:102、tasks.md:23 均写「不新增 CommandPayload::SessionResume 变体、不改 command_name/command_kind/family、不加通用分发臂」",
    "F47 残留（同节内部矛盾）：verification.md:16 现值行为 99c6eb8e7a7c5cbcd13abdfa45a5b5c1986ccb293a3430424541dcc4fbd5092f  specs/local-agent-host/spec.md，而 :22 括注写「specs/local-agent-host/spec.md（6f7d725d… → 1c0b1875…，CR7-F1）」（round8 报告 §4 记录当时现值为 1c0b1875…，round10 现场引述同值并判其陈旧）；:11 节标题只提 CR7",
    "F52 依据：docs/CORE_PORTS_AND_STORAGE.md:1327（§9 判据 1「v1 → v2 → v3 → v4 连续升级」）与 :1357（§9 判据 28「v4 的追加列不改变以上任何一条…owned 家族列清单…export_ids_json 在末尾」）；:20 的文件头先例「版本：0.14 … §9 新增判据 32 并同步判据 1/28 的版本链与 owned 列清单断言」证明 v4 变更同批改了 §9；plan.md:296 的 WP4 写范围只到 §7 标题/§7.2/§7.3，plan.md:328 的区域注记同样不含 §9；check-contract-drift 只比 §5/§7",
    "F53 依据：verification.md:193 的 CR2-F2/F3/F4/F5 行把 F3 的位置记为「NODE_LINK §10 措辞」，而 reports/cr2-review.md:30（CR2-F3）的位置是 docs/NODE_LINK_PROTOCOL.md:662（§12.7 结果投影收窄句，基线在 :648），且 §10 的 grant→命令表是 DR1-F11 的对象（WP2 已处理）；处置栏亦未记录「已把该文件加入 WP6 写范围」",
    "Coverage/Waves/登记全绿（除上述两处）：5 份 specs 共 37 个 heading（acp 4 + local-agent-host 8 + node-link-owner-server 11 + storage 10 + workspace 4）与 plan 的 37 行 source.heading 逐字一致；tasks 引用 {2.1–2.6,4.2}、checks {PV1,PV2} 实存；W1–W5 严格晚于 code 依赖；9 行 Serialization Reason 全 NOT_APPLICABLE；Shared File Ownership 22 行齐备无重复；Main E2E 五要素齐备 + C1/C2 双向登记；tasks 7.1/8.3/9.1 三门禁标记各唯一；DCR 已有 Round 11 待填行（最大轮次规则）",
    "F49 落点核实：docs/CORE_PORTS_AND_STORAGE.md:266-269 的 §5.1 [决定] 段（在 WP3 区域内）承载端口级决定，与 plan.md:295/tasks.md:23 的「幂等语义只写 §5.1、不改 §6 第 20 条」一致；grep plan/tasks 的「§6」只命中该否定句",
    "F50 落点核实：docs/MODULE_ARCHITECTURE.md:170-224 的 §4.1 含 SessionBackendFactory/SessionEndpoint/SessionStore 端口行与「以上是冻结全量」值对象块（缺 AgentSessionId/ResumeSessionRequest/SessionRecoveryRecord）；plan.md:295 写范围 + plan.md:340 登记行齐备；§4.7（:324-334）不列举端口方法，本变更无需改",
    "F51 落点核实：plan.md:298 WP6 写范围含 docs/NODE_LINK_PROTOCOL.md（仅 §10 收窄句与 §12.7 示例注记）、tasks.md:29 义务④逐字写明两处；plan.md:341 登记 Writers=WP2,WP6/Merge Owner=WP6/Order=WP2→WP6/Re-verify=WP6: PV2；WP2 在 W1、WP6 在 W4"
  ],
  "residualRisks": [
    "无 shell/引擎能力 → 未复算 contractDigest（结论绑定派发给出的 sha256:e75f8147…00ea8）与 verification.md 的 6 行 sha256（F47 的判定不依赖复算，基于同节现值行与括注互相矛盾）",
    "主工作区停在基线 81e350f…：WP1/WP2 的交付内容未重读 worktree，仅通过 verification.md/CR1/CR2/merger 的现场记录交叉印证",
    "F48 的残留（plan.md:323 区域注记仍写「WP3 改 CommandPayload 变体/command_name/分发」）若不改就派发 WP3，虽不影响权威指令（design:102 + plan WP3 行 + tasks 2.3 一致），但会让 coder-C/reviewer-C 读到同一份 plan.md 内的相反描述；建议派发前改掉",
    "F47 的括注与 F52/F53 若不修：verification.md 的「行为契约未变」机械核对的图例仍然错误；CORE_PORTS §9 与 §7.2 会在 v5 落地后自相矛盾且无门禁覆盖；最终验收按 CR2-F3 核对时会读到错误的 §10 标签",
    "若在同一批里修 F48 残留/F47 残留/F52（触及 plan.md，F52 若选「不改 §9」则只改记录），contractDigest 会再变 ⇒ 需第 12 轮独立复核后才放行 WP3；只改 verification.md 的项（F47 残留、F53）不影响本轮 PASS 的适用范围",
    "本轮不改任何文件、不执行任何检查；PV1/PV2/C1/C2 的执行证据仍待后续门禁补齐（本轮结论不覆盖它们）"
  ],
  "noStagedFiles": true,
  "diffSummary": "无改动：本会话为只读审查（无 write 工具），未修改、未暂存、未提交任何文件；主工作区仍在基线 81e350f…",
  "reviewFindings": [
    "no blockers: 0×CRITICAL / 0×MAJOR；DR1-F41/F42/F43/F44/F49/F50/F51 与本轮触发面一致，F45/F46 已闭环",
    "minor(partial): plan.md:323 的 Shared File Ownership 区域注记仍写「WP3 改 CommandPayload 变体、command_name、分发」，与 design.md:102、plan.md:295、tasks.md:23 的 Path A（不新增变体/不改 command_name/不加分发臂）相反 → DR1-F48 部分闭环",
    "minor(partial): verification.md:22 的括注与 :11 的节标题仍把 CR7 的 1c0b1875… 当作 specs/local-agent-host/spec.md 的当前值（现值行 :16 已是 99c6eb8e…）→ DR1-F47 部分闭环",
    "minor: docs/CORE_PORTS_AND_STORAGE.md §9 判据 1/28 的「v1 → … → v4」版本链与 owned 列清单断言 + 文件头版本记录无任何 WP 写归属，v5 后与 §7.2 自相矛盾且无门禁覆盖 → DR1-F52",
    "suggestion: verification.md 的 CR2-F2/F3/F4/F5 行把 F3 的位置记为「NODE_LINK §10 措辞」，实际是 §12.7 的结果投影收窄句（:662），且未记录该文件已入 WP6 写范围 → DR1-F53",
    "suggestion(研判，非缺陷): resume_session 的形参表、接受提交是否附带一次 StateChange::Update、幂等行 session 分量 Some/None 三处未写死，但三种替代读法都能满足 R9/R32/R34/R35（submit_prompt 不拒绝 Closed；存储层会回填 idem.session）⇒ 不构成第二次 BLOCKED 的歧义，建议在 WP3 行补一句「幂等行携带目标会话 session: Some(...)」"
  ],
  "manualNotes": "① 本轮为 DR1 第十一轮（recheck，Review Type=plan），我是第十一个新实例、只读、无 write 工具，未参与前十轮与任何实现；结论 **PASS**，对应 target_revision sha256:e75f8147…00ea8，0×CRITICAL/0×MAJOR。② 关于 F48：Path A 我判定**可行且是最优路径**，依据是 create 先例的四处硬事实（注释不变量、core_payload 早退臂、broker 的自建幂等行形态、submit_mutation 的按变体派发）与门禁脚本只镜像 required_grant；但 main 的回写漏了第四处 —— plan.md:323 的区域注记仍写着「WP3 改 CommandPayload 变体、command_name、分发」，这是同一份 plan.md 内部的自相矛盾，请务必在派发 WP3 前改掉（coder-C 已因契约歧义 BLOCKED 过一次）。③ F47 只修了现值行、没修图例：verification.md:22 的括注与 :11 的节标题仍以 CR7 值（1c0b1875…）为「新值」，与 :16 的 99c6eb8e… 矛盾 —— 这与 F33 是同一类缺陷（同节自相矛盾），修它不影响 digest。④ 我另发现一处**从来没人登记过的写归属缺口（F52）**：storage-sqlite 的 v5 改动要求同步 `CORE_PORTS` 的 §9 判据 1/28（「v1 → … → v4」版本链、owned 列清单断言）与文件头版本记录，但 WP4 的区域只到 §7 标题/§7.2/§7.3，§9 无人拥有；先例上 v3→v4 变更（文件头 `版本：0.14`）是**同批**同步 §9 的，而 `check-contract-drift` 只比 §5/§7，不会发现。最小修法是把 §9 判据 1/28 加进 WP4 区域（与 WP3 的 §2/§3.x/§4/§5.x 不相交）。⑤ F53 是记录层标签：CR2-F3 的对象是 §12.7 的收窄句（:662），不是 §10 的 grant 表（那是 DR1-F11，WP2 已处理），建议订正以免最终验收漏核对。⑥ 结论层面：本轮 PASS 表示「依赖声明、批次、资源互斥与写入归属」在当前 digest 下成立，且 Path A 之后 no 代码义务无主；它**不**表示 PV/E2E 已通过，也**不**表示 WP3 可以立刻落笔 —— 请先决定是否同批处理①②③（若动 plan.md 则需 Round 12），或仅动 verification.md（②④）以保住本轮 PASS。"
}
```
