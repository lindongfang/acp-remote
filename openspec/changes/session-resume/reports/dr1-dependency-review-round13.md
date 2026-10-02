> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：DR1 / Round 13；run 2e063929-120f-4bc8-a752-c16bfa991bc9

# DR1 Round 13 复核报告（Review Type: plan，plan 阶段）
> 本报告由**第十三个独立 reviewer 实例**产出（新实例、只读、无 write 工具；未参与前十二轮、未参与任何实现或用例设计；宿主未提供本实例 ID 与 `fork_turns` 设置，故**不声称**已自行证明隔离方式）。main 请把本报告原样持久化为
> `openspec/changes/session-resume/reports/dr1-dependency-review-round13.md`，再回填 `verification.md` 的 `## Dependency Declaration Review`（见文末「给 main 的回填指引」）。
## Shared Report
| 字段 | 取值 |
| --- | --- |
| task_id | NOT_APPLICABLE（规划门禁，非 tasks 行） |
| role / phase / stage | reviewer / plan / plan |
| round | 13 |
| agent_context | 第十三个新实例；只读、无 write 工具、未切分支/未提交；与前 12 轮均不同实例；非任何 WP 的 Owner；未继承任何实现对话 |
| target_revision | `sha256:1c04e5a3f177c8c8397a4d26db07993bb1bf456b2bbb726d1629fb7154348d13`（本轮 contractDigest，由派发给出；**我无 shell/引擎能力，未自行复算**） |
| scope | ① 新形状在 `design.md`（D3 形状块 / 步骤 ③ / Round 13 注记）、`plan.md`（Round 13 表 + WP3 行）、`tasks.md` 2.3 是否一致、并全库检索 resume 相关的 `ResolvedWorkspace`/`alias` 每一处；② 独立在基线代码里复核裁定三条前提，并判断两个备选「不采用」的理由是否成立；③ 逐条对照新形状能否满足 `specs/workspace-resolution` R23–R26（含 SR-R26-1）与 `specs/local-agent-host` R8/R9/R10；④ 写范围覆盖（`CORE_PORTS` §3.6 行、WP5 的字段消费是否需补登记）；⑤ 本轮是否引入新的不一致（Coverage 37 行、Waves/Serialization、SFO 全部行、Main E2E、三门禁标记、`## Checks` 表、DCR 最大轮次行、tasks↔plan）；⑥ 独立判断「是否还有两种读法都读得通或无人可做的义务」（含 WP4/WP5/WP6/TP2） |
| changes | 无（只读；未修改任何文件、未暂存、未提交；`watchdog_diff --stat` 显示工作区仅有未跟踪的 `openspec/changes/session-resume/**`，无 tracked 改动） |
| checks | 规划阶段无 PV/E2E 可跑；本轮只核对 Coverage Index 37 行、`## Verification Strategy`、`### Main E2E`、三个门禁标记与 DCR 表最大轮次行的**登记与可产出性**（不核对执行结果）。另**读**了门禁实现 `node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs`（`checkDependencyReview`，只读、不执行）以确认「最大 Round 行 + 摘要逐字符相等」的判据 |
| issues | **0×CRITICAL / 0×MAJOR**；轮内复核：F47/F48/F52/F54/F57 **已闭环**，F55/F56 **未闭环**；新发现 1×MINOR（F60）+ 2×SUGGESTION（F59、F61） |
| result | **PASS**（无阻断项） |
| evidence_paths | 本报告（待 main 落盘）；`reports/dr1-dependency-review-round12.md`（本轮触发源）、`…round11.md`、`…round10.md`（F47/F48/F51/F52/F53 现场）、`reports/tp1-test-design.md`（F58/F59 现场）、`reports/cr2-review.md:30`（CR2-F3 原文） |
| resource_cleanup | NOT_APPLICABLE（未占用任何构建/端口/数据库资源） |
## Review Context
| Review ID + Round | Review Type / Stage | Work Package | Repository | Base / Target Revision | 读取的规则与需求 | 实际检查范围 | 验证证据与限制 | 报告路径 | 核对的 Check / E2E ID |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DR1 / Round 13 | plan / plan | NOT_APPLICABLE | `D:/Project/acp-remote` | Base = NOT_APPLICABLE；Target = `sha256:1c04e5a3…8d13` | `roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md` §1/§3/§4/§5/§7/§10/§12 | `design.md` 全文（D1–D6 + 三处订正注记，重点 D2/D3）、`plan.md` 全文（Contract Changes 含 Round 13 段、Coverage Index 37 行、Work Packages 8 行、Execution Waves 8 行、Shared File Ownership 22 行、Dependency Handoffs、Runtime Resources、Merge Strategy、Verification Strategy、Main E2E、Independent Validation、Completion Criteria）、`tasks.md` 全文、`proposal.md`、5 份 `specs/**/spec.md` 的 37 个 heading 与 R23–R26/R8–R10 正文、`verification.md` 全文、前 12 轮 DR1 报告与 CR2/CR7/TP1 报告；**基线代码独立复核**：`crates/core/src/model/config.rs:425-456`（`ResolvedWorkspace`）、`crates/storage-sqlite/src/migrate.rs:17-22`（版本常量）、`:53-66`（`owned_session` DDL）、`:313-319`（`owned_workspace` DDL）、`:611-621`（v4 段）、`crates/storage-sqlite/src/admin/local_config.rs:252-288`（读写与 upsert）、`crates/server/src/local_admin/{method.rs,router.rs:131-160,params.rs:215-233}`（`workspace.select` 的重指向语义）、`crates/agent-host/src/host.rs:539-556`（后端只消费 `canonical_path`）、`docs/SESSION_CONTINUITY_DESIGN.md:109-122/:211/:233-237`、`docs/CORE_PORTS_AND_STORAGE.md` §3.6（`:168-185`）§4（`:192-205`）、`docs/MODULE_ARCHITECTURE.md:170-224`；**全库检索**：`workspace_cwd`、`ResolvedWorkspace`、`alias`、`不含 alias`、`Round 13`、`§9 判据`、`文件头`、`## Checks`、`F51`、`§12.7`、`:662`；`workflow-check.mjs:509-573` 的 DCR 判据 | 无任何执行证据（规划审查不得冒称已执行测试）。**限制**：① 无 shell → 不能复算 contractDigest 与 `verification.md` 的 6 行 sha256；② 主工作区停在基线 `81e350f…`，WP1/WP2 交付内容（集成基线 `b0a387b…`）只在 `verification.md`/CR1/CR2/merger 记录层核对，未重读 worktree；③ 单行长于 500 字符的表格行被工具截断，个别单元格只能以「整行是否命中某 token」的方式取证（已在下文逐条注明取法）；④ 不审代码 diff、不判用例集合齐备性 | 本报告（待落盘） | 计划内：Coverage Index 37 行、PV1/PV2、C1/C2（仅核对登记与可产出性）；E2E = not-applicable |
---
## 1. 改动的自洽性：**四处（实为六处）一致，无「把 alias 用于恢复」的残余要求**
| 位置 | 现行文本（我实读/实检索） | 判定 |
| --- | --- | --- |
| `design.md:84`（D3 形状块） | `core::model::ResumeSessionRequest { agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }` | ✅ |
| `design.md:85-86`（同块注释） | `// workspace_cwd = 持久化的「创建时 canonical path」原文；构造约束同 ResolvedWorkspace::canonical_path（非空、≤4096 字节、无 NUL、绝对路径形状）` + `// **不含 alias**（订正见下方 Round 13 注记）` | ✅ |
| `design.md:99`（步骤 ③） | 「cwd 复校验：对 **`request.workspace_cwd`**（持久化原文）执行同创建口径的校验（绝对、存在、是目录、`canonicalize` 结果与持久化值一致）→ 失败返回服务端不可用类错误」 | ✅ 已点名新字段 |
| `design.md:104`（Round 13 注记） | 「原冻结形状 `workspace: ResolvedWorkspace` 要求带 `WorkspaceAlias`，但**恢复路径上没有任何权威 alias 来源**…故把 `workspace` 换成 `workspace_cwd: String`」 | ✅ |
| `plan.md:107` + `:109-112`（Contract Changes Round 13） | 表：`ResumeSessionRequest` 的 workspace 字段 `workspace: ResolvedWorkspace`（含 alias）→ `workspace_cwd: String`（持久化 canonical path 原文；构造约束同 `ResolvedWorkspace::canonical_path`）；受影响面含 `design` D3 形状块与步骤 ③、`tasks` 2.3、`CORE_PORTS` §3.6 该行、**WP5（agent-host）读该字段** | ✅ |
| `plan.md:315`（WP3 行） | 该行同时命中 `值对象形状（DR1`、`workspace_cwd`、`不含 alias` 三个 token（我分别 grep，均落在 `:315` 这一行）⇒ 行内确为「**值对象形状（DR1 Round 13）**：`ResumeSessionRequest { agent, agent_session_id, workspace_cwd: String }`——**不含 alias**…」 | ✅（取法：整行命中，非目视 500 字符前缀） |
| `tasks.md:23`（2.3） | 「新增 `AgentSessionId`、`ResumeSessionRequest { agent, agent_session_id, workspace_cwd: String }`（**不含 alias**；见 design D3 的 Round 13 订正）、`SessionRecoveryRecord`…」 | ✅ |
**残余检索（全库、逐处摊开）**：
| 命中处 | 表述 | 判定 |
| --- | --- | --- |
| `proposal.md:30` | 「恢复时 MUST NOT 按 alias 重新解析，因为 alias 之后可能指向别处」 | ✅ 与新形状同向（是禁令，不是要求带 alias） |
| `design.md:66` | 「`NULL` 的语义…恢复必须显式失败，MUST NOT 用别名重解析补齐」 | ✅ 同向 |
| `design.md:67-68` | `workspace_cwd` 由 core 在**创建时**用 `ResolvedWorkspace::canonical_path()` 写入 | ✅ 讲的是创建路径（alias 的合法来源就在那里），与恢复路径无关 |
| `plan.md:111`（Round 13 表「原值」列）、`design.md:104` | 引述被替换的旧形状 | ✅ 历史段，含裁定，不会读成现行要求 |
| `specs/workspace-resolution/spec.md:5`（R23） | 「两类失败都 MUST NOT 回退到按别名重新解析」；R26 的 THEN 是「恢复使用该持久化取值作为 `session/resume` 的 `cwd`，**不按 workspace 别名重新解析**」 | ✅ 与新形状完全同向 |
| `specs/local-agent-host/spec.md:19`（R8） | 输入「至少为该会话的 Agent 标识、持久化的 ACP 会话标识与持久化的创建时工作目录」 | ✅ 没有 alias 要求 |
| `reports/tp1-test-design.md:207/:255/:348/:387` | 仍按旧字段写：`ResumeSessionRequest::new(agent_ref(), AgentSessionId("acp-session-1"), workspace)`、`ResumeSessionRequest.workspace.canonical_path()` | ⚠️ 下游产物陈旧 → **F59**（不是「把 alias 用于恢复」，而是旧字段名） |
| `docs/SESSION_CONTINUITY_DESIGN.md:122/:211` | 「`cwd` 必须记录**创建时解析出的真实路径**，不能在恢复时重新解析 alias」「三者都需要同样的两样数据：Agent 侧 sessionId + 创建时 cwd」 | ✅ **权威设计文档本来就只要求 cwd**——新形状消掉了一处既有矛盾（旧的 `workspace: ResolvedWorkspace` 才与该文档相反） |
**§1 结论：现行契约集合（design/plan/tasks）中不存在任何「把 alias 用于恢复」的指令，四处（含形状块注释与 Round 13 表共六处）逐字一致。**
## 2. 裁定理由是否成立：三条前提**全部独立复现属实**，两个备选不采用的理由**成立**
| 前提（main 陈述） | 我的独立核对 | 判定 |
| --- | --- | --- |
| `ResolvedWorkspace::try_new` 要求 alias | `crates/core/src/model/config.rs:429-456`：`pub struct ResolvedWorkspace { alias: WorkspaceAlias, canonical_path: String }`；`try_new(alias: WorkspaceAlias, canonical_path: String)` 是唯一构造入口，`alias()`/`canonical_path()` 为只读访问器（无 `Option`、无替代构造） | ✅ 属实 |
| `owned_session` 无 workspace/alias 列（v5 只加两列） | `crates/storage-sqlite/src/migrate.rs:53-66` 的 v1 DDL 只有 `session_id/title/agent_id/agent_name/state/origin_epoch/current_mode_id/current_mode_name/version/created_at/updated_at/closed_at`；`:17-22` 现为 `FILE_FORMAT_VERSION=4 / OWNED_SCHEMA_VERSION=4`；`:611-621` 的 v4 段仅 `ALTER TABLE owned_node ADD COLUMN export_ids_json`；design D2 的 v5 只追加 `agent_session_id`/`workspace_cwd` | ✅ 属实 |
| `owned_workspace` 的 alias 映射可被重指向 | `migrate.rs:313-319`：`alias TEXT PRIMARY KEY, display_name, canonical_path TEXT NOT NULL, created_at, updated_at`；`crates/storage-sqlite/src/admin/local_config.rs:285-288`：`INSERT INTO owned_workspace … ON CONFLICT(alias) DO UPDATE SET display_name=…, canonical_path=excluded.canonical_path`；`crates/server/src/local_admin/router.rs:141` 明确「**同一 alias 再次调用是更新**」，`:131-160` 的 `workspace_select` 就是该入口（`local.workspace.select` 是唯一 workspace 方法，见 `method.rs:18/103` 与 `compatibility/commands/v1/commands.json:36`） | ✅ 重指向属实（**「或删除」这半句未获证实**，见 F61） |
| 反查取 alias 会「写入不成立的断言」 | `owned_workspace` 对 `canonical_path` **没有 UNIQUE 约束**（PK 只在 `alias`）⇒ 按路径反查可能得到 0 条、1 条或多条 alias，本身不构成映射的权威；且后端**从不消费 alias**：`crates/agent-host/src/host.rs:547-555` 的 `session_new_params` 只取 `workspace.canonical_path()` 作为 `cwd`（alias 在创建路径上也是空的装饰）。加上 `docs/SESSION_CONTINUITY_DESIGN.md:122` 明令恢复不得重解析 alias | ✅ 理由成立（且比陈述更强：反查甚至可能无解） |
| 「再持久化第三列（alias）」超范围 | `design.md:65-71`（D2 只两列）+ `specs/storage-schema-v2-migration/spec.md:7`（「v5 相对 v4 的**唯一差异**是…两列」）+ plan WP4 写范围（§7/§9/文件头）三者一致；加第三列要改行为契约（specs）⇒ 变更需求摘要，代价远超收益 | ✅ 理由成立 |
**额外支持（本轮新发现的正向证据）**：新形状让 R23 的「不按别名重解析」变成**结构性成立**，而旧形状反而留下一条危险路径（拿到 `ResolvedWorkspace{alias, path}` 的实现者可以用 alias 再解析）；同时步骤 ③ 只需对持久化字符串做一次路径校验，不再需要先构造一个带 alias 的值对象。**结论：改契约（选 B）是正确的裁定，不是权宜之计。**
## 3. 新形状能否满足规格：逐条对照（**全部满足**）
| 规格条目 | 要求 | 新形状下的落点 | 判定 |
| --- | --- | --- | --- |
| workspace R23（目录复校验） | 恢复前对持久化目录做同口径校验；`NULL` 走 `nodelink.command.unsupported`；其余失败走服务端不可用类；**MUST NOT** 回退按别名重解析 | `ResumeSessionRequest.workspace_cwd` = 持久化原文；design D3 步骤 ② 读 `load_recovery`、③ 对 `request.workspace_cwd` 复校验、④ 才 `factory.resume` | ✅ 结构上不可能重解析（请求里没有 alias） |
| workspace R24（目录被删除 → 不可用且不启动 Agent） | 在调用后端之前失败，不启动进程、不写目录取值 | ③ 在 ④ 之前（design D3 的顺序号即约束）；失败分支返回 `Unavailable(..)` → `nodelink.internal.unavailable` | ✅ |
| workspace R25（规范化结果变化 → 拒绝） | 返回服务端不可用类，**不使用新解析出的路径**发送 | ③ 比对 `canonicalize(workspace_cwd) == workspace_cwd`，不等即失败；④ 发送的只能是同一字段原文 | ✅ |
| workspace R26（目录有效 → 用持久化取值，不按别名重解析） | `session/resume` 的 `cwd` = 持久化取值 | D4 步骤 4 发送「`cwd: <持久化目录>`」；新形状下该值逐字等于 `request.workspace_cwd` | ✅（SR-R26-1 的「alias 已重指向别处」场景由「请求里根本没有 alias」保证） |
| local-agent-host R8（恢复入口输入「至少为」Agent 标识 + 持久化 ACP 标识 + 持久化创建时目录） | 三个输入齐备 | `{ agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }` | ✅ |
| local-agent-host R9（宣告能力 → 恢复到可交互、可接受 prompt） | 与输入形状无关 | D4 的 spawn/`initialize`/`session/resume {sessionId, cwd}` 与端点登记 | ✅ 不受本次改动影响 |
| local-agent-host R10（未宣告 → 不发送 + 回收子进程） | 门控在发送之前、失败无残留 | D4 步骤 3；CR7-F1 修正后的措辞 | ✅ 不受影响 |
| storage R17/R22（新列可读回且不推导 / 恢复不覆写） | 两列只在创建与恢复流程中写、恢复只读 | `SessionRecoveryRecord` 字段名本就是 `workspace_cwd`（plan WP4 行/tasks 2.4），**本次改名不触及存储侧** | ✅ |
## 4. 写范围覆盖：**两项都齐备**，WP5 无需新增登记（建议派发提示补一句）
- **`docs/CORE_PORTS_AND_STORAGE.md` §3.6 的 `ResumeSessionRequest` 行在 WP3 写范围内**：WP3 行（`plan.md:315`）写范围含「§2、§3.1、§3.3、§3.6、§4、§5.1、§5.2」；SFO 行（`plan.md:348`）区域注记逐字写「§3.6（**`ResumeSessionRequest` 与 `SessionRecoveryRecord`** 行）」；tasks 2.3 同。落点正确：`docs/CORE_PORTS_AND_STORAGE.md:168-185` 的 §3.6 正是「后端与能力」族请求/值对象的表（`:174` `CreateSessionRequest`、`:175` `ResolvedWorkspace`）。✅
- **WP5 消费新字段名不构成未登记的交接**：① 影响面已在 `plan.md:112`（Round 13 表）登记「WP5（agent-host）读该字段」；② WP5 声明 `code:WP1, code:WP3`（`plan.md:317`），字段来自 WP3 已合入的代码，WP5 从集成基线开工后必然读到；③ WP5 行的 Invalidation 列已写「**WP3 变更端口 → WP5/WP6/TP2 重跑**」，WP4 行更明写「WP3 变更 core 端口、**值对象**或提交形状 → WP4/WP5/WP6/TP2 重跑」⇒ 值对象形状变化已被失效条款覆盖。**结论：不需要改 plan/tasks（改了反而会再改摘要、白开一轮）；建议把「`resume` 的 cwd 取 `request.workspace_cwd`」写进 WP5 派发提示即可。**
- 顺带核对：`docs/MODULE_ARCHITECTURE.md:170-224` 的 §4.1 只列**类型名**（不写字段），故「值对象清单加 3 项」（WP3）不受改名影响；`docs/SESSION_CONTINUITY_DESIGN.md` 已与新形状同向，**不新增文档义务**。✅
## 5. 本轮是否引入新的不一致：逐项核对
| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| Coverage Index 37 行 ↔ 5 份 specs 标题逐字 | ✅ **37/37** | 我抓出全部 `^### Requirement:`/`^#### Scenario:`（acp 4 + local-agent-host 8 + node-link-owner-server 11 + storage 10 + workspace 4 = 37），与 `plan.md` 的 37 行 `source.heading` 逐条比对：含反引号「`` `session/resume` ``」、全角括号「（`session/resume`）」、`v4 到 v5 升级保留既有会话行且新列为空`、`能力未宣告时显式不支持且不发送恢复请求`、`目录仍然有效时使用持久化取值`、`Agent 不支持恢复时终态失败` 等全部逐字命中；改名不触及任何 heading（R 行是需求映射，非字段映射） |
| Coverage 的 `tasks`/`checks` 引用实存 | ✅ | `tasks` = {2.1–2.6, 4.2} 全部实存；`checks` = {PV1, PV2} 均在 `## Verification Strategy` 定义 |
| Execution Waves 与 Serialization Reason | ✅ | W1 = WP1/WP2/TP1 → W2 = WP3（`code:WP2`）→ W3 = WP4（`code:WP3`）/WP5（`code:WP1+WP3`）→ W4 = WP6 → W5 = TP2；8 个包各一次；**8 行** Serialization Reason 全 `NOT_APPLICABLE`（注：R12 报告写「9 行」，实为 8 行——历史报告的计数笔误，不影响任何判定）；同波写集合无交集 |
| Shared File Ownership **全部行** | ✅ | **22 行、无重复文件**；每行 Merge Owner / Merge Order / Re-verify 齐备（含单写者行如 `crates/core/src/lib.rs` 三列均为 WP3、`crates/**/tests/ 的新增用例文件` 均为 TP2）；`Re-verify` 只引用 PV1/PV2（∈ Coverage 的 checks）；双写者四行（`broker.rs` WP2→WP3、`CORE_PORTS` WP3→WP4、`README.md` WP2→WP6、`NODE_LINK_PROTOCOL.md` WP2→WP6）都分处不同波次且区域互斥；**`crates/core/src/broker.rs` 行（`:343`）已是 Path A 表述，「WP3 改 `CommandPayload` 变体…」的旧文不复存在** |
| Main E2E 四/五要素 | ✅ | `mode: not-applicable` + 非空 `reason` + `basis` + 非空 `alternative_checks: [C1, C2]` + `downgrade_approval`（含用户 2026-09-30 原话/时间/来源「仅供本变更使用」）；C1/C2 在 `## Checks` 各有一行 |
| 三个门禁标记唯一 | ✅ | `[validation]`（tasks 7.1）、`[e2e-owned]`（8.3）、`[final-verification]`（9.1）各一次、互不重叠 |
| `verification.md` 的 `## Checks` 表结构完整 | ✅ | 标题（`:65`）+ 表头 + 分隔行 + **7 数据行**（PV2 预变更基线、PV1 阶段 1、PV1 阶段 2（集成基线）、PV1 阶段 2（候选/主分支）、PV2 四段、C1、C2），列数 8 与分隔行一致；**F57 的行尾 `\|## Checks` 残迹已消失**（全文 `## Checks` 仅出现一次） |
| DCR 最大轮次行 | ⚠️ **待 main 补行（正常中间态，非计划缺陷）** | 现表最大 Round = **12**（PASS，`Plan Revision = sha256:2c04a445…90de`）；**尚无 Round 13 行**。按门禁实现 `workflow-check.mjs:536-568`：同名 Review ID 取**最大 Round** 行，且要求其 `Plan Revision` 与当前 `contractDigest` **逐字符相等** ⇒ Round 13 改动后该行摘要不匹配，plan 门禁**现在是红的**，WP3 不会被过期 PASS 放行（不存在「旧 PASS 被误用」的口子）。main 必须在回填时新增 Round 13 行并填**改动后重算**的摘要 |
| tasks 2.3 / 2.4 / 2.5 / 2.6 与 plan 义务对齐 | ⚠️ **2.3 ✅ / 2.5 ✅（一致地都不点名字段名，靠 D4 + WP3 代码）/ 2.6 ✅ / 2.4 ❌ → F55** | 2.3 逐条含 Round 13 形状、F48 入口形状、F49 §5.1、F45 §3.3/`session.rs` 注释、F50 §4.1、`-p core`；2.6 含①–④+`core_payload()` 早退+`port_error_code` 一条臂+5 个 `load_recovery` 替身；**2.4 仍只写「§7 标题/§7.2/§7.3」，缺 §9 判据 1/28 与文件头（我 grep `§9 判据\|文件头` 于 `tasks.md` → 0 命中）** |
| 记录层与文件实际是否一致（新一轮核对） | ⚠️ 2 处不实 | `verification.md:80`（Check Plan Changes 的 Round 12 条）把 F56 记成「F55」并声称「已订正并补记 F51 的处置」，而 `## Review Findings` 的 CR2 行（`:197`）仍是「§10 措辞」、全文**没有** `F51` 字样 → **F60**；`verification.md:197` 本身未订正 → **F56** |
## 6. 独立判断：**不存在「无人可做」的义务；除已知三条外只多一条无害的两义项**
**问题 A：WP3 拿到现在的 design/plan/tasks 后，除已知三条（`resume_session` 形参表、是否伴随一次 `StateChange::Update`、幂等行 `session` 分量取值）外，还有没有「两种做法都读得通」或「无人可做」的义务？**
- **「无人可做」：没有。** 我把改名波及的每条义务摊平并逐条落主：core 值对象与 §3.6 行（WP3）／`resume_session` 用例（WP3）／`SessionRecoveryRecord` + `load_recovery` 7 处实现（WP3 ×1、WP4 ×1、WP6 ×5 含 `FlakySessionStore`）／`SessionBackendFactory::resume` 与 `SessionEndpoint::agent_session_id`（定义 WP3、实现 WP5 + WP3/WP6 替身）／agent-host 发送 `cwd`（WP5，D4 步骤 4）／适配层投影 + `settle_session_resume` + `port_error_code` 一条臂（WP6）／两列提交点（WP3）／v5 DDL、migration、§7/§9/文件头（WP4）／文档九处（WP3/WP4/WP6 已逐一登记）。**没有一条落在「无写范围」里。**
- **「两种做法都读得通」：只多一条**（连同已知三条共四条，全部是模块内实现细节，`AGENTS.md` §1 允许自行裁定）：
  1. **（已知）** `resume_session` 的形参表（是否携带 `request_fingerprint`、会话 id 的位置）——Path A 下与 `create_session` 同形，WP6 依赖 WP3 代码可读；
  2. **（已知）** 返回时是否伴随一次把会话状态抬回可交互的 `StateChange::Update`（R9/R35 两种做法都满足）；
  3. **（已知）** 幂等行的 `session` 分量取值（`Some(<会话 id>)` 与 `None` 安全等价，建议取前者以保持 `CORE_PORTS` §6 第 20 条字面为真）；
  4. **（本轮新增，非阻断）** **持久化值本身无法构造值对象时的错误分类**：若 `owned_session.workspace_cwd` 是相对路径／空串／>4096 字节／含 NUL（只能经库被篡改或写入侧 bug 到达），`ResumeSessionRequest` 的构造校验会先失败；R23 只枚举了「目录被删除/被替换为文件/规范化结果不同/无权限读取」归服务端不可用类，**没有点名这一类**。两种读法（按 R23 的「其余校验失败」记为服务端不可用类；或与 `NULL` 同路记为 `nodelink.command.unsupported`）都能自洽，且**没有任何 spec 或 TP1 用例区分它们**（SR-R25-1 用的是「仍然存在但未规范化」的绝对路径，能通过值对象构造）。建议在 WP3 派发提示里钉一句「构造失败按服务端不可用类处理」，不必改契约。
- 另**顺带**：`SessionRecoveryRecord` 的「两列缺失时为 `None`」是指外层 `None` 还是字段为 `Option`（plan WP4 行/tasks 2.4 的措辞）在两种读法下**可观察行为相同**（都落到「显式不支持」），不构成本轮歧义。
**问题 B：WP4/WP5/WP6/TP2 的义务里是否还有同类空档（尤其 WP5 消费新字段名那面）？**
| 包 | 是否有同类空档 | 依据 |
| --- | --- | --- |
| WP4 | **无未登记义务**；但**tasks 侧未同步**（F55） | `load_recovery` 的返回形状含 `workspace_cwd`（字段名未变，存储侧零改动）；§7/§9/文件头三项在 plan WP4 行 + SFO 行都在，只有 tasks 2.4 缺（会漏改 §9/文件头） |
| WP5 | **无未登记交接** | 见 §4：影响面已在 `plan.md:112` 登记、`code:WP3` 依赖 + Invalidation「WP3 变更端口 → WP5/WP6/TP2 重跑」覆盖值对象改名；`design.md:99`（步骤 ③）与 D4 步骤 4 一起把「`cwd` 取持久化原文」钉死成唯一读法（新形状下请求里只有一个 cwd 来源，比旧形状的 `workspace.canonical_path()` 少一层间接）。建议只在派发提示补一句字段名 |
| WP6 | **无空档** | 路由/投影/settle/`port_error_code` 均不触碰 `ResumeSessionRequest` 的字段；`core_payload()` 早退臂与字段无关 |
| TP2 | **无空档，但有下游输入陈旧** | TP2 的断言只观察 wire `cwd` 与后端收到的请求值（fake ACP `--dump-requests`），改名不改变 wire；其**输入** TP1 的设计报告仍写旧字段（`tp1-test-design.md:207/:255/:348/:387`）→ **F59**（漏读会**编译失败**，不会静默通过） |
---
## Findings
### 复核项（原问题 ID 沿用）
| ID | Severity（原） | 复核依据（本轮实读/实检索） | Recheck 结论 |
| --- | --- | --- | --- |
| **DR1-F41** | MAJOR | 新形状**只减少**信息（core 侧更不可能有 export 信息）：`design.md:101` 仍写「core 只返回 `SessionId`，适配层投影 + `settle_session_resume`」，`plan.md:315`（WP3）/`:318`（WP6 义务③）/`tasks.md:23`、`tasks.md:29` 一致 | **已解决**（改名未削弱该结论，回归检查通过） |
| **DR1-F42** | MINOR | 改名不改变实现点计数（7 处 `load_recovery` / 6 文件；16 个 trait 实现点），`design.md:112`、WP4/WP5/WP6 行与 tasks 2.4/2.6 的点名仍一致 | **已解决** |
| **DR1-F43** | MINOR | tasks 2.3/2.5/2.6 与 plan 对齐；2.4 缺 §9/文件头 → 转 F55 | **部分**（残留 = F55） |
| **DR1-F44** | MINOR | 红窗口段现写「**WP3 新增三个必需 trait 方法**（`resume`、`agent_session_id`、`load_recovery`）」+ storage 新实现 + server **四个**替身 + `FlakySessionStore` + `port_error_code` 一条臂，与 WP3/WP4/WP5/WP6 行的义务逐项相符 | **已解决** |
| **DR1-F45/F46/F49/F50** | SUGGESTION | §3.3 专用路径注记（`design.md:106-108`）+ `session.rs` 注释入 WP3；R5 措辞与 Scenario 一致；幂等语义只写 §5.1 且「不改 §6 第 20 条」；`MODULE_ARCHITECTURE` §4.1 入 WP3 写范围并有 SFO 行 | **均已闭环**（本轮无回归） |
| **DR1-F47** | MINOR | `verification.md:11` 标题已含「CR7 两次 + DR1 Round 10 一次措辞订正」；`:16` 现值 = `99c6eb8e…`；`:22` 括注已是三段链（`6f7d725d…` → `1c0b1875…`（CR7-F1）→ `99c6eb8e…`（DR1 Round 10 的 R5 正文措辞订正））⇒ 同节内部自洽 | **已闭环** |
| **DR1-F48** | MINOR | 四处现行义务仍全为 Path A：`design.md:106`、`plan.md:315`、`plan.md:343`（SFO `broker.rs` 行，旧文已不存在）、`tasks.md:23`；历史段（Round 11/12 表、F13 行、红窗口段）只含「原值 + 裁定」 | **已闭环**（本轮无回归） |
| **DR1-F51** | MINOR | WP6 写范围含 `docs/NODE_LINK_PROTOCOL.md`（仅两处）+ SFO 行 `WP2, WP6 ｜ WP6 ｜ WP2 → WP6 ｜ WP6: PV2`（W1/W4 不并发）；`tasks.md:29` 义务④逐字写明两处 | **已闭环**（记录层残留转 F56） |
| **DR1-F52** | MINOR | 计划侧：`plan.md:316` + `:348` 已把「**§9 判据 1/28 的版本链与 owned 列清单断言**」与「按既有惯例在文件头追加版本记录」纳入 WP4；与 WP3 的 §2/§3.x/§4/§5.x 区域不相交、W2/W3 不并发。残留 = tasks 2.4 | **计划侧已闭环**（残留 → F55） |
| **DR1-F53** | SUGGESTION | `verification.md:197` 仍为「…NODE_LINK **§10 措辞**…」与「F3（§10 措辞）由 WP6 的文档收口一并处理」；`reports/cr2-review.md:30` 的 CR2-F3 实为 `docs/NODE_LINK_PROTOCOL.md:662`（§12.7 收窄句）；全文无 `F51` 字样 | **未处置** → F56 |
| **DR1-F54** | MINOR | `## Target` 现值行/括注/标题三者已自洽（见 F47 行） | **已闭环** |
| **DR1-F55** | MINOR | 我 grep `§9 判据\|文件头` 于 `tasks.md` → **0 命中**；`tasks.md:25`（2.4）仍只写「§7 标题/§7.2/§7.3」 | **仍未闭环** |
| **DR1-F56** | MINOR | `verification.md:197` 仍写「§10 措辞」，未补「`:662`（§12.7 收窄句）」与 F51 的实际处置；且**没有**任何一行提到 F51 | **仍未闭环** → 与 F60 同一处 |
| **DR1-F57** | SUGGESTION | `verification.md` 全文 `## Checks` 仅一次（`:65`），`:63` 的 DCR 行以 `| reports/dr1-dependency-review-round12.md |` 正常收尾 | **已闭环** |
| **DR1-F58** | SUGGESTION | `tp1-test-design.md:496/:560` 仍把 resume 的 `CommandPayload`/分发臂落在 core（Path A 下 core 无该变体）——该报告仍在 CR7 修复轮、TP 存废由用户挂起，处置方式不变 | **未处置（预期）**，与 F59 同批处理 |
### 新发现（本线程连续编号）
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation（最小修复面） |
| --- | --- | --- | --- | --- | --- |
| **DR1-F59** | SUGGESTION | `reports/tp1-test-design.md:207`、`:255`、`:348`、`:387` | 四处在 Round 13 改名后仍按旧字段写：`:207` `factory.resume(&session, ResumeSessionRequest::new(agent_ref(), AgentSessionId("acp-session-1"), workspace), sink)`；`:255`/`:348`/`:387` 断言「`ResumeSessionRequest.workspace.canonical_path()` == 持久化值」（SR-R26-2/SR-R23-1/SR-R26-1）。现行契约是 `workspace_cwd: String`（design.md:84、plan.md:315、tasks.md:23） | TP2 按 tasks 2.8「TP1 的修正后设计」编写用例时会写出不存在的字段访问/传错类型 ⇒ **编译失败**（不会静默通过，但会浪费一轮返工）；与 F58 属同一份下游产物、同一处置批次 | **不动契约与计划**：把「`ResumeSessionRequest` 的字段是 `workspace_cwd: String`（无 `workspace`/`canonical_path()`）」与 F58 那句一起写进 **TP1 修复轮与 TP2 的派发提示**；同时在 `verification.md` 的 `## Review Findings` 记一行 F59（只改 verification.md，不进摘要）。TP1 的报告本身由 tester 侧修复 |
| **DR1-F60** | MINOR | `verification.md:80`（`## Check Plan Changes` 的 Round 12 条） | 该条写「另报 2×非阻断：**F54** … 与 **F55**（`## Review Findings` 的 CR2-F2/F3/F4/F5 行位置标签原记「§10 措辞」…——**已订正并补记 F51 的处置**）」。但 `reports/dr1-dependency-review-round12.md` 的第 55 号问题是 **`tasks.md:25` 未同步 §9/文件头**，CR2 位置标签问题是**第 56 号（F56）**；且 `verification.md:197` 实际**未改**、全文无 `F51`。即：编号张冠李戴 + 声称改过实际未改（与 F18/F25/F29/F38 同类），并让 F55（tasks 2.4）在记录里被读成「已闭环」 | 最终验收按 ID 核对时会同时读错两件事：F55 被误记为已修（真实缺口仍在 tasks 2.4），F53/F56 被误记为已订正（`:662` 对象仍可能被漏掉，而该句无任何门禁覆盖） | 该条改为按 R12 报告编号如实写：**F54 已闭环；F55（`tasks.md` 2.4 缺 §9 判据 1/28 与文件头）尚未落地；F56（CR2 行位置标签 = `docs/NODE_LINK_PROTOCOL.md:662` 的 §12.7 收窄句 + 补记 F51 的实际处置）尚未落地**；并把 `:197` 的 Location/Resolution 一并订正（同 F56 的建议文本）。只改 `verification.md`，不进摘要 |
| **DR1-F61** | SUGGESTION | `design.md:104`（Round 13 注记）与 `plan.md:107`（Round 13 引言） | 两处都写 alias 映射「可以被**重指向或删除**」。重指向属实（`crates/server/src/local_admin/router.rs:131-160` + `:141`「同一 alias 再次调用是更新」；`crates/storage-sqlite/src/admin/local_config.rs:285-288` 的 `ON CONFLICT(alias) DO UPDATE SET … canonical_path = excluded.canonical_path`）；**「删除」无入口**：workspace 相关方法只有 `workspace.select`（`crates/server/src/local_admin/method.rs:18/103`、`compatibility/commands/v1/commands.json:36`），端口侧只有 `put_workspace`（`crates/core/src/ports.rs:928`），`owned_workspace` 无删除路径 | 仅表述精确性；裁定的成立只依赖「重指向」（SR-R26-1 正是测这一点），故**不影响任何义务或判定**，不阻断、不建议为此开轮次 | 下一次触碰 `design.md` 时把「或删除」删去（或改成「可被重指向，且本机没有任何东西保证它与创建时的路径仍然对应」）。**同批不要为它单独改 design**（会再改摘要） |
**New findings 汇总：0×CRITICAL、0×MAJOR、1×MINOR（F60）、2×SUGGESTION（F59/F61）。**未闭环的历史非阻断项：**F55（MINOR）、F56（MINOR）**、F58/F59（SUGGESTION）、F61（SUGGESTION）。
---
## Check Plan 核对
- 本轮为 **plan 类型**：计划内检查 = `plan.md` 的 `## Coverage Index`（37 行）、`## Verification Strategy`、`### Main E2E` 决策与三个门禁标记。逐行核对结果见 §5：37/37 `source.heading` 逐字、`tasks`/`checks` 引用实存、Waves 层级合法且 8 包各一次（Serialization Reason 8 行全 `NOT_APPLICABLE`）、Shared File Ownership 22 行齐备（Merge Owner/Order/Re-verify 三列全有、Re-verify 引用 Coverage 内的 Check ID）、Main E2E 五要素齐备 + C1/C2 双向登记、三门禁标记各唯一、`## Checks` 表 7 行结构完整。
- **不存在影响本轮判断的待返回执行证据**：PV1（阶段 1/阶段 2）、PV2、C1/C2 的结果行属后续阶段；`verification.md` 的 `## Checks` 目前只登记阶段/范围/证据路径（唯一 PASS 行是本变更前的 PV2 预变更基线），**未冒称通过**。**plan review 的结论不覆盖任何 PV/E2E 是否通过。**
- 本轮未改任何计划字段（本实例只读），故无新的 Check Plan Changes 需 reviewer 记录。
- **DCR 门禁可产出性核对（本轮新增）**：`workflow-check.mjs` 的 `checkDependencyReview`（`:512-572`）按 Review ID 分组、取**最大 Round** 行，并要求 `Plan Revision === contractDigest`（`:566-568`）与 `Result == PASS`（`:569`）。当前最大行是 Round 12（PASS @ `2c04a445…`），而 Round 13 已改 `design/plan/tasks` ⇒ 摘要已变 ⇒ **plan 门禁现在为红**，WP3 不会被过期 PASS 放行。main 回填 Round 13 行（新摘要 + 裸 `PASS` + 本报告路径）后即恢复为绿。
- 记录层待办（属 main，不是计划缺陷）：① DCR 补 Round 13 行；② `## Review Findings` 补 F54–F61 的本轮处置行、并订正 `:197`（F56）；③ `## Check Plan Changes` 补 Round 13 条并订正 Round 12 条的编号（F60）。
## Assessment
- **本轮检视结论：PASS**，对应 `target_revision` = `sha256:1c04e5a3f177c8c8397a4d26db07993bb1bf456b2bbb726d1629fb7154348d13`。**0×CRITICAL、0×MAJOR。**
- **改名自洽**（§1）：`design.md` D3 形状块（`:84-86`）、步骤 ③（`:99`）、Round 13 注记（`:104`）、`plan.md` Round 13 表（`:107-112`）、WP3 行（`:315`，经 token 命中确认含「值对象形状（DR1 Round 13）/`workspace_cwd`/不含 alias」）、`tasks.md:23` 六处逐字一致；全库检索确认**没有任何**「把 alias 用于恢复」的现行要求，两处 `ResolvedWorkspace` 残留（`design.md:67-68`）讲的是创建路径。
- **裁定理由成立**（§2）：三条前提我在基线代码里逐条复现（`config.rs:429-456` 的 alias 必需构造；`migrate.rs:53-66`/`:611-621` 的 `owned_session` 无 workspace 列且 v4 段只加 `owned_node.export_ids_json`；`migrate.rs:313-319` + `local_config.rs:285-288` + `router.rs:141` 的同 alias 重指向）。两个备选不采用的理由也成立，且比陈述更强：`owned_workspace.canonical_path` **无 UNIQUE 约束**，反查本身可能无解；后端（`agent-host/src/host.rs:547-555`）**从不消费 alias**。`docs/SESSION_CONTINUITY_DESIGN.md:122/:211` 本来就只要求 cwd ⇒ 本次改契约**消掉**了一处既有矛盾。
- **新形状满足规格**（§3）：R23（含「不得按别名重解析」变成结构性成立）、R24、R25、R26（含 SR-R26-1 的重指向场景）、local-agent-host R8/R9/R10、storage R17/R22 逐条对照通过。
- **写范围覆盖**（§4）：`CORE_PORTS` §3.6 的 `ResumeSessionRequest` 行在 WP3 写范围与 SFO 区域注记内；WP5 的字段消费不需要新增登记（影响面已登记 + `code:WP3` 依赖 + Invalidation 覆盖值对象），建议只在派发提示补一句字段名。
- **非阻断项清单（按建议处理顺序）**：① **F55**（`tasks.md` 2.4 补「§9 判据 1/28 的版本链与 owned 列清单断言 + 按既有惯例在文件头追加版本记录」；**只改 tasks.md ⇒ 会改变 digest，请与 DCR 回填一并做**）；② **F56 + F60**（`verification.md:197` 的 CR2 位置标签 → `docs/NODE_LINK_PROTOCOL.md:662`（§12.7 收窄句）+ 补记 F51 的实际处置；`verification.md:80` 的 Round 12 条按 R12 报告编号订正；只改 verification.md，不进摘要）；③ **F59**（TP1 修复轮/TP2 派发提示同步新字段名，与 F58 同批）；④ **F61**（`design.md:104`/`plan.md:107` 的「或删除」措辞，留待下次触碰 design/plan 时顺手改，不建议单独开轮次）。
- **§6 独立判断**：**没有「无人可做」的义务**（改名波及的每条义务都有主：WP3/WP4/WP5/WP6 与 main 各自覆盖）；**没有阻断级的两义项**，除已知三条外仅多一条无害的（持久化值无法通过值对象构造时的错误分类——两种读法都可，无 spec/用例区分，建议在 WP3 派发提示里钉一句）。WP4 无未登记义务（只有 tasks 侧 F55）；WP5 无未登记交接；WP6 不受影响；TP2 无空档（其输入 TP1 报告陈旧 = F59，且会**编译失败**而非静默通过）。
- **待补证据与门禁**：PV1（各 WP 阶段 1；集成基线/候选/主分支阶段 2）、PV2 与 C1/C2 的 PASS 行须在对应交付/验证门禁前补齐；本轮为规划审查，不要求也不核对执行证据。
- **复用依据**：本轮不复用任何历史轮结论作为结论依据；`round12.md`（本轮触发源与 F54–F58 现场）、`round11.md`（F47/F48/F51–F53）、`round10.md`（F41/F50/F51）、`cr2-review.md:30`（CR2-F3 的 `:662` 现场）仅作核对对象。历史轮次保留不覆盖，当前结论以 Round 13（本报告）为准。
- **受阻/限制**：无 shell/引擎 → 未复算 `contractDigest` 与 `verification.md` 的 6 行 sha256（结论绑定派发给出的 `1c04e5a3…`）；主工作区为基线 `81e350f…`，WP1/WP2 的交付内容仅通过 `verification.md`/CR1/CR2/merger 的记录层交叉印证；工具对超长表格行截断到 500 字符，`plan.md:315` 等行采用「整行是否命中指定 token」的方式取证（已在 §1 注明取法）。
## 给 main 的回填指引（记录层）
1. 把本报告原样落盘到 `openspec/changes/session-resume/reports/dr1-dependency-review-round13.md`（**先落盘**，否则路径不可读，重演 F19/F25/F37）。
2. `## Dependency Declaration Review` **新增 Round 13 行**：`Reviewer` 填本实例的实际 ID/隔离说明、`Plan Revision` 填**改动后重算**的 contractDigest（若同时做 F55 改 `tasks.md`，请以改动**后**重算的值为准；门禁要求与 `workflow check --stage plan --json` 的输出**逐字符相等**）、`Result` 单元格保持**裸 `PASS`**、`Report Path` 填上面的路径。当前表最大行是 Round 12（旧摘要），在补行前 plan 门禁为红，属正常中间态。
3. `## Review Findings` 增补：F47/F48/F52/F54/F57 **已闭环**；F55 记「**DR1 Round 13 复核：仍未闭环**（`tasks.md` 2.4 缺 §9 判据 1/28 与文件头）」；F56 记「仍未闭环」并把 `:197` 的 Location/Resolution 一并订正（`docs/NODE_LINK_PROTOCOL.md:662`（§12.7 结果投影收窄句）+「已把该文件加入 WP6 写范围（两处，DR1-F51）」）；F58/F59/F61 按本报告表格逐列落（F59 与 F58 同批交给 TP1 修复轮/TP2）；F60 记对 `verification.md:80` 的订正。
4. `## Check Plan Changes` 补 Round 13 条：本轮 main 的处置范围（改名触 `design/plan/tasks` ⇒ 摘要变化 ⇒ Round 12 的 PASS 失效 ⇒ Round 13）、reviewer 复核 PASS、以及 F55 若落地会再次改变摘要（**应在回填 DCR 之前一次性做完**）；同时按 F60 订正 Round 12 条的编号与措辞。
5. 派发 WP3（coder-C，新实例）时把四条写进提示：① 新形状 `ResumeSessionRequest { agent, agent_session_id, workspace_cwd: String }`（**不含 alias**），步骤 ③ 对 `request.workspace_cwd` 做同创建口径复校验；② **Path A 是可执行口径**（core 不加 `CommandPayload` 变体、不改 `command_name`/`kind`/`family`、不加分发臂；`accepted` 与幂等行自建；`resume_session` 只返回 `SessionId`）；③ 幂等行携带 `session: Some(<会话 id>)`；④ 不要重复添加 `required_grant` 的 `session.resume` 臂（已由 WP2 落地）。派发 WP5 时补一句「`resume` 的 `cwd` 取 `request.workspace_cwd`」；给 TP1 修复轮/TP2 同时补 F58 与 **F59** 两句。
```yaml
handoff_index:
  - task_id: NOT_APPLICABLE
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    stage: plan
    round: 13
    target_revision: "sha256:1c04e5a3f177c8c8397a4d26db07993bb1bf456b2bbb726d1629fb7154348d13"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round13.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "针对 Round 13 改契约（`ResumeSessionRequest.workspace: ResolvedWorkspace` → `workspace_cwd: String`）后的当前 contractDigest 做静态规划审查（第十三个新实例、只读、未参与任何实现或用例设计）：① 四处（实为六处）一致性逐字核对（design.md:84-86 形状块与注释、:99 步骤 ③、:104 Round 13 注记、plan.md:107-112 Round 13 表、:315 WP3 行（以整行命中『值对象形状（DR1』『workspace_cwd』『不含 alias』三个 token 取证）、tasks.md:23）并全库检索 resume 相关的 ResolvedWorkspace/alias（残余仅在 design.md:67-68 的创建路径与 tp1-test-design.md 的下游旧字段）；② 三条前提在基线代码独立复现：config.rs:429-456（ResolvedWorkspace::try_new 必须带 alias）、migrate.rs:53-66 + :611-621（owned_session 无 workspace 列，v4 段只加 owned_node.export_ids_json）、migrate.rs:313-319 + admin/local_config.rs:285-288 + local_admin/router.rs:131-160/:141（同 alias 重指向为受支持操作）；两个备选不采用的理由成立且更强（canonical_path 无 UNIQUE ⇒ 反查可能无解；agent-host/src/host.rs:547-555 证明后端从不消费 alias）；docs/SESSION_CONTINUITY_DESIGN.md:122/:211 本就只要求 cwd；③ R23/R24/R25/R26 与 local-agent-host R8/R9/R10、storage R17/R22 逐条对照通过；④ §3.6 的 ResumeSessionRequest 行在 WP3 写范围（plan.md:315 + :348 + tasks.md:23），WP5 的字段消费由 plan.md:112 影响面登记 + code:WP3 依赖 + Invalidation『WP3 变更端口/值对象 → WP4/WP5/WP6/TP2 重跑』覆盖，无需新增登记；⑤ Coverage 37/37 heading 逐字、Waves 8 包层级与 8 行 Serialization Reason、SFO 22 行三列齐备、Main E2E 五要素、三门禁标记唯一、## Checks 7 行结构、workflow-check.mjs:536-572 的 DCR 最大 Round 判据（当前最大行 Round 12 旧摘要 ⇒ plan 门禁红，无过期 PASS 放行口）；⑥ 独立判断『无无人可做义务；除已知三条外仅多一条无害两义项（持久化值无法构造值对象的错误分类）』，WP4/WP5/WP6/TP2 无同类空档（WP4 仅 tasks 侧 F55；TP2 的输入陈旧 = F59 且会编译失败）。未闭环历史项：F55/F56（MINOR）；新报 F60（MINOR）+ F59/F61（SUGGESTION）；0×CRITICAL/0×MAJOR → PASS。未执行任何 PV/E2E，未复算 contractDigest 与 6 行 sha256（无 shell），已声明该限制"
    source_evidence: NOT_APPLICABLE
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "返回 DR1 Round 13 完整复核报告：Review Context + §1–§6 逐项结论 + Findings（复核项 + 新发现 F59–F61）+ Check Plan 核对 + Assessment + handoff_index。结论 PASS（0×CRITICAL/0×MAJOR）：① 改名六处逐字一致（design.md:84-86/:99/:104、plan.md:107-112/:315、tasks.md:23），全库无『把 alias 用于恢复』的残余；② 三条前提在基线独立复现（config.rs:429-456；migrate.rs:53-66 + :611-621；migrate.rs:313-319 + local_config.rs:285-288 + router.rs:141），两个备选不采用的理由成立（canonical_path 无 UNIQUE ⇒ 反查可能无解；host.rs:547-555 证明后端从不消费 alias；SESSION_CONTINUITY_DESIGN.md:122/:211 本就只要求 cwd）；③ R23/R24/R25/R26 + R8/R9/R10 逐条满足（R23 的『不得按别名重解析』成为结构性成立）；④ §3.6 行在 WP3 写范围，WP5 字段消费已被影响面与失效条款覆盖；⑤ 结构核对全绿（37/37 heading、8 包 Waves、SFO 22 行、Main E2E 五要素、三门禁标记、## Checks 7 行、DCR 最大 Round 判据）；⑥ 无无人可做义务，除已知三条外仅多一条无害两义项。未闭环非阻断项：F55（tasks 2.4 缺 §9/文件头）、F56（verification.md:197 CR2 位置标签）、F60（verification.md:80 编号错记并声称已改）、F59（TP1 报告旧字段）、F61（design/plan『或删除』措辞）"
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
    "新形状四处一致（实读）：design.md:84 `core::model::ResumeSessionRequest { agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }`；:85-86 注释『workspace_cwd = 持久化的「创建时 canonical path」原文；构造约束同 ResolvedWorkspace::canonical_path（非空、≤4096 字节、无 NUL、绝对路径形状）』+『不含 alias（订正见下方 Round 13 注记）』；:99 步骤 ③『对 `request.workspace_cwd`（持久化原文）执行同创建口径的校验』；:104 Round 13 注记（原形状 workspace: ResolvedWorkspace 要求 WorkspaceAlias，恢复路径无权威来源；替换为 workspace_cwd: String）；plan.md:107-112 Round 13 表（原值/新值/受影响面含 design D3 形状块与步骤 ③、tasks 2.3、CORE_PORTS §3.6 该行、WP5）；plan.md:315（整行分别命中『值对象形状（DR1』『workspace_cwd』『不含 alias』）；tasks.md:23『ResumeSessionRequest { agent, agent_session_id, workspace_cwd: String }（不含 alias；见 design D3 的 Round 13 订正）』",
    "残余检索：proposal.md:30『恢复时 MUST NOT 按 alias 重新解析』；design.md:66『MUST NOT 用别名重解析补齐』；design.md:67-68（创建时用 ResolvedWorkspace::canonical_path() 写入 workspace_cwd，属创建路径）；plan.md:111=design.md:104 为历史/裁定段；specs/workspace-resolution/spec.md:5（R23）『两类失败都 MUST NOT 回退到按别名重新解析』；specs/local-agent-host/spec.md:19（R8）输入为 Agent 标识 + 持久化 ACP 标识 + 持久化创建时目录；reports/tp1-test-design.md:207/:255/:348/:387 仍写旧字段 workspace（→ F59）；docs/SESSION_CONTINUITY_DESIGN.md:122『cwd 必须记录创建时解析出的真实路径，不能在恢复时重新解析 alias』、:211『三者都需要同样的两样数据：Agent 侧 sessionId + 创建时 cwd』",
    "前提 1（config.rs）：:429-432 `pub struct ResolvedWorkspace { alias: WorkspaceAlias, canonical_path: String }`；:436 `pub fn try_new(alias: WorkspaceAlias, canonical_path: String) -> Result<Self, InvalidValue>`（:437-440 只校验 bounded/无 NUL/绝对路径形状）；:448/:453 只读访问器 alias()/canonical_path()",
    "前提 2（migrate.rs）：:18 FILE_FORMAT_VERSION=4、:20 OWNED_SCHEMA_VERSION=4、:22 IMPORTED_SCHEMA_VERSION=3；:53-66 owned_session DDL（session_id/title/agent_id/agent_name/state/origin_epoch/current_mode_id/current_mode_name/version/created_at/updated_at/closed_at，无 workspace/alias）; :611-621 v4 段仅 `ALTER TABLE owned_node ADD COLUMN export_ids_json TEXT NOT NULL DEFAULT '[]'`",
    "前提 3（重指向）：migrate.rs:313-319 `CREATE TABLE IF NOT EXISTS owned_workspace (alias TEXT PRIMARY KEY, display_name TEXT NOT NULL, canonical_path TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)`（canonical_path 无 UNIQUE）；admin/local_config.rs:285-288 `INSERT INTO owned_workspace … ON CONFLICT(alias) DO UPDATE SET display_name = excluded.display_name, canonical_path = excluded.canonical_path, updated_at = excluded.updated_at`；local_admin/router.rs:131-160 workspace_select（:141 注释『同一 alias 再次调用是更新』）；method.rs:18/103 与 compatibility/commands/v1/commands.json:36 只有 local.workspace.select ⇒ 可重指向、无删除入口（→ F61）",
    "备选不采用的理由：canonical_path 无唯一约束 ⇒ 按路径反查 alias 可能 0/1/N 解；agent-host/src/host.rs:547-555 的 session_new_params 只取 workspace.canonical_path() 作为 cwd（alias 从不被后端消费）；storage 规格 specs/storage-schema-v2-migration/spec.md:7『v5 相对 v4 的唯一差异是两列』+ design.md:65-71（D2 两列）⇒ 加第三列必须改行为契约（超范围）",
    "规格逐条对照：workspace R23（复校验 + NULL→unsupported + 不得按别名重解析）由 design D3 步骤 ②③④ 直接满足；R24（目录被删除→不可用且不启动 Agent）由 ③ 早于 ④ 满足；R25（canonicalize 变化→拒绝、不用新路径）由 ③ 的等比对满足；R26/SR-R26-1（目录有效→用持久化取值）由『请求里没有 alias』结构性满足；local-agent-host R8（输入三项齐备）、R9/R10（能力门控与回收，与字段形状无关）；storage R17/R22（SessionRecoveryRecord.workspace_cwd 字段名未变，存储侧零改动）",
    "写范围覆盖：plan.md:315 WP3 写范围含『docs/CORE_PORTS_AND_STORAGE.md（§2、§3.1、§3.3、§3.6、§4、§5.1、§5.2）』；plan.md:348 SFO 行区域注记『§3.6（ResumeSessionRequest 与 SessionRecoveryRecord 行）』；CORE_PORTS §3.6（:168-185）承载 CreateSessionRequest（:174）/ResolvedWorkspace（:175）行；plan.md:112 影响面登记 WP5；plan.md:317 WP5 行 Dependencies=code:WP1, code:WP3 且 Invalidation=『WP3 变更端口 → WP5/WP6/TP2 重跑』；MODULE_ARCHITECTURE.md:170-224 的 §4.1 只列类型名（改名不新增 §4.1 义务）",
    "结构核对：5 份 specs 共 37 个 heading（acp 4 + local-agent-host 8 + node-link-owner-server 11 + storage 10 + workspace 4）与 plan 的 37 行 source.heading 逐字一致；tasks 引用 {2.1–2.6,4.2}、checks {PV1,PV2} 实存；Waves W1(WP1/WP2/TP1)→W2(WP3)→W3(WP4/WP5)→W4(WP6)→W5(TP2)，8 包各一次、8 行 Serialization Reason 全 NOT_APPLICABLE（R12 报告的『9 行』为计数笔误）；SFO 22 行三列齐备、无重复文件、Re-verify 只引 PV1/PV2；Main E2E 五要素 + C1/C2 双登记；tasks.md:71/:77/:81 三标记各唯一；verification.md:65 起的 ## Checks 表 7 行 8 列（且全文 ## Checks 仅一次 ⇒ F57 已闭环）",
    "复核项结论：F47（verification.md:11/:16/:22 三处自洽）已闭环；F48（design.md:106 + plan.md:315 + plan.md:343 SFO broker.rs 行 + tasks.md:23 四处 Path A）已闭环；F52 计划侧已闭环（plan.md:316/:348 含 §9 判据 1/28 与文件头），残留 tasks.md:25（grep『§9 判据|文件头』于 tasks.md = 0 命中）→ F55 未闭环；F54/F57 已闭环；F53/F56 未闭环（verification.md:197 仍『§10 措辞』、全文无 F51）；F55 未闭环；F58 未处置（预期）",
    "门禁判据核对（只读 workflow-check.mjs）：checkDependencyReview（:512-572）按 Review ID 分组取最大 Round（:536-555），要求 Plan Revision === contractDigest（:566-568）与 Result==PASS（:569）、报告可读（:571）⇒ 当前最大行 Round 12 的摘要 2c04a445… 已过期，plan 门禁为红，过期 PASS 无法放行 WP3；回填 Round 13 行后恢复",
    "新发现：F59（reports/tp1-test-design.md:207/:255/:348/:387 仍按旧字段 workspace 写，与 design.md:84/plan.md:315/tasks.md:23 不符，会编译失败，SUGGESTION，随 F58 同批入 TP1 修复轮/TP2 派发提示）；F60（verification.md:80 把 R12 的 F56 记成 F55 并声称『已订正并补记 F51 的处置』，而 :197 未改、全文无 F51，MINOR）；F61（design.md:104/plan.md:107 的『可被重指向或删除』中『删除』无入口，SUGGESTION，不影响裁定）"
  ],
  "residualRisks": [
    "无 shell/引擎能力 ⇒ 未复算 contractDigest（结论绑定派发给出的 sha256:1c04e5a3…8d13）与 verification.md 的 6 行 sha256；main 回填 DCR 时必须以改动后重算值填入，否则 plan 门禁与 reviewer 结论绑定到不同摘要",
    "主工作区停在基线 81e350f…：WP1/WP2 的交付内容（含集成基线 b0a387b…）未重读 worktree，仅通过 verification.md/CR1/CR2/merger 的记录层交叉印证",
    "F55 未闭环：若 coder-D 只读 tasks 会漏改 docs/CORE_PORTS_AND_STORAGE.md §9 判据 1/28 与文件头，导致 v5 落地后 §7.2 与 §9 自相矛盾，而该处无任何门禁覆盖",
    "F56/F60 未闭环：最终验收按 ID 核对 CR2-F3 时会读到错误的『§10 措辞』标签，真正的 :662 收窄句可能被漏掉；且 F55 在记录层被误读为已闭环",
    "F59：TP2 若按 TP1 现版设计编写用例会编译失败（loud，非静默），浪费一轮返工；TP 存废仍由用户挂起，若选『删除 TP1/TP2』该风险自然消失",
    "F61：design.md:104/plan.md:107 的『或删除』措辞略宽于事实；只影响表述精确性，裁定的成立只依赖『重指向』，不建议为此开轮次",
    "本轮不改任何文件、不执行任何检查；PV1/PV2/C1/C2 的执行证据仍待后续门禁补齐（本轮结论不覆盖它们）"
  ],
  "noStagedFiles": true,
  "diffSummary": "无改动：本会话为只读审查（无 write 工具），未修改、未暂存、未提交任何文件；watchdog_diff --stat 显示仅有未跟踪的 openspec/changes/session-resume/**，主工作区仍在基线 81e350f…",
  "reviewFindings": [
    "no blockers: 0×CRITICAL / 0×MAJOR；Round 13 的契约改名（workspace: ResolvedWorkspace → workspace_cwd: String）六处逐字一致、三条裁定前提在基线代码全部复现、R23/R24/R25/R26 + R8/R9/R10 逐条满足、§3.6 行在 WP3 写范围、WP5 的字段消费无未登记交接、结构核对全绿 → PASS",
    "minor(open): tasks.md:25（2.4）未同步 plan.md:316/:348 的『§9 判据 1/28 的版本链与 owned 列清单断言 + 文件头版本记录』（grep 0 命中）→ DR1-F55 仍未闭环",
    "minor(open): verification.md:197 仍把 CR2-F3 记为『NODE_LINK §10 措辞』、未补 docs/NODE_LINK_PROTOCOL.md:662（§12.7 收窄句）与 F51 的实际处置 → DR1-F56 仍未闭环",
    "minor(new): verification.md:80（Check Plan Changes 的 Round 12 条）把 R12 的 F56 记成 F55，并声称『已订正并补记 F51 的处置』，与 :197 的实际状态和全文无 F51 的事实相反（F18/F25/F29/F38 同类）→ DR1-F60",
    "suggestion(new): reports/tp1-test-design.md:207/:255/:348/:387 仍按旧字段 workspace 写（应为 workspace_cwd: String），随 F58 同批写进 TP1 修复轮/TP2 派发提示 → DR1-F59",
    "suggestion(new): design.md:104 与 plan.md:107 的『可被重指向或删除』中『删除』无入口（workspace 只有 local.workspace.select / 端口只有 put_workspace），重指向已证实；不影响裁定，不建议为此开轮次 → DR1-F61",
    "record-layer (main): DCR 表尚无 Round 13 行，最大行 Round 12 绑定过期摘要 ⇒ plan 门禁当前为红（正确中间态）；§6 独立判断 = 无『无人可做』义务，除已知三条实现细节外仅多一条无害两义项（持久化值无法构造值对象时的错误分类）"
  ],
  "manualNotes": "① 本轮为 DR1 第十三轮（recheck，Review Type=plan），我是第十三个新实例、只读、无 write 工具，未参与前十二轮与任何实现或用例设计，也未参与产生任一报告；结论 PASS，对应 target_revision sha256:1c04e5a3…8d13。② 我看的是**当前工作区**（主仓库 D:/Project/acp-remote，停在基线 81e350f… 的只读视图）：change 目录未跟踪，因此我核对的是「此刻磁盘上的 design/plan/tasks/verification/specs」，而不是某个提交；本轮判定不依赖任何提交范围。③ 工具限制：单行长于 500 字符的表格行会被截断，plan.md:315 这类单元格我改用『整行是否命中指定 token』取证（『值对象形状（DR1』『workspace_cwd』『不含 alias』三者均命中该行），已在 §1 与 handoff_index 的 applicability_basis 里注明取法，不把截断前缀当成全文。④ 我没有 shell，无法复算 contractDigest 与 verification.md 的 6 行 sha256，也未执行任何 PV/E2E；main 回填 DCR 时请以改动后重算的摘要为准，并先做完 F55（会改 tasks.md ⇒ 改摘要）再回填，避免摘要与结论错配。⑤ 若 main 只想做「不变更摘要」的清理，可只改 verification.md（F56/F60）并把 F59 写进派发提示；F55 与 F61 触及 tasks.md/design.md/plan.md，建议合并到下一次必须进行的契约改动里，或明确记录为「已知残留」。⑥ 我未修改任何仓库文件、未切分支、未提交，也未占用任何构建/端口/数据库资源。"
}
```
