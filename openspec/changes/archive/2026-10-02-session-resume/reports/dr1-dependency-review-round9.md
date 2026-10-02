> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：DR1 / Round 9；run d07ac5c6-3211-4ece-b1ad-30a24592a77a

# DR1 Round 9 复核报告（Review Type: plan，plan 阶段）
> 本报告由**第九个独立 reviewer 实例**产出（新实例、只读、无 write 工具；未参与前八轮、未参与任何实现或用例设计；宿主未提供本实例 ID 与 fork_turns 设置，故不声称已证明隔离方式）。main 请把本报告原样持久化为
> `openspec/changes/session-resume/reports/dr1-dependency-review-round9.md`，再回填 `verification.md` 的 `## Dependency Declaration Review`（见文末「给 main 的回填指引」）。
## Shared Report
| 字段 | 取值 |
| --- | --- |
| task_id | NOT_APPLICABLE（规划门禁，非 tasks 行） |
| role / phase / stage | reviewer / plan / plan |
| round | 9 |
| agent_context | 第九个新实例；只读、无 write 工具、未切分支/未提交；与前八轮均不同实例；非任何 WP 的 Owner；未继承任何实现对话 |
| target_revision | `sha256:f4468e70fdc0de96c4eb4942fbd923493e90d82650d40daa47c5922d21dc682d`（本轮 contractDigest，由派发给出；**我无 shell/引擎能力，未自行复算**） |
| scope | ① 本轮触发面：`design.md` 的 D2/D3 契约订正、`plan.md` 的 `## Contract Changes` Round 9 段 + WP3/WP4/WP6 行 + Shared File Ownership + `## Dependency Handoffs` 红窗口段；② 契约集合（proposal + 5 份 specs + design）与 plan/tasks/verification 的互相一致性；③ **独立**核对三处技术主张所依赖的**基线代码与机器词表事实**（不采信回写说明） |
| changes | 无（只读；`watchdog_diff` 只显示 `openspec/changes/session-resume/**` 的 untracked 清单，无被跟踪文件的 staged/unstaged 改动） |
| checks | 规划阶段无 PV/E2E 可跑；本轮核对 Coverage Index 37 行、`## Verification Strategy`、`### Main E2E` 决策与三个门禁标记的登记与可产出性（不核对执行结果） |
| issues | **1×MAJOR（DR1-F41）** + 3×MINOR（F42、F43、F44）+ 2×SUGGESTION（F45、F46） |
| result | **FAIL**（唯一阻断项 DR1-F41） |
| evidence_paths | 本报告（待 main 落盘）；`reports/dr1-dependency-review-round8.md`（前轮基线）、`…round7.md`（已确认可读） |
| resource_cleanup | NOT_APPLICABLE（未占用任何构建/端口/数据库资源） |
## Review Context
| Review ID + Round | Review Type / Stage | Work Package | Repository | Base / Target Revision | 读取的规则与需求 | 实际检查范围 | 验证证据与限制 | 报告路径 | 核对的 Check / E2E ID |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DR1 / Round 9 | plan / plan | NOT_APPLICABLE | `D:/Project/acp-remote` | Base = NOT_APPLICABLE；Target = `sha256:f4468e70…c682d` | `roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md` §1/§3/§4/§5/§10、`procedures/workflow-check.md` 的 plan 判据（含 contractDigest 覆盖范围） | 契约集合全文（proposal、5 份 specs、design D1–D6）、`plan.md`（Coverage Index/Work Packages/Execution Waves/Shared File Ownership/Dependency Handoffs/Verification Strategy/Main E2E/Completion Criteria）、`tasks.md` 全文、`verification.md` 全文、前 8 轮报告与 CR1/CR2/CR7 报告、TP1 设计报告相关行；**基线代码独立核对**：`crates/server/src/node_link/command.rs`（create 路由 640–912、`port_error_code`/`wire_error`/`error_info` 1985–2070）、`crates/server/src/node_link/resource.rs:1217-1233`、`crates/storage-sqlite/src/session_store.rs:876-1101`、`crates/core/src/broker.rs:1359-1510` 与 `3049-3095`、`crates/core/src/use_cases.rs:243-288` 与全部 `pub async fn` 清单、`crates/core/src/model/error.rs:74-138`、`crates/core/src/model/session.rs:754-789`、`crates/server/src/local_admin/params.rs:914-940`/`1520-1568`、全仓 `SessionStore for` 枚举、`compatibility/errors/v1/errors.json`、`schemas/local-admin/v1/envelope.schema.json`、`docs/CORE_PORTS_AND_STORAGE.md` §2/§3.3/§3.6/§4/§5.1/§5.2/§7、`docs/NODE_LINK_PROTOCOL.md` §12.7 | 无任何执行证据（规划审查不得冒称已执行测试）。**限制**：① 无 shell → 不能复算 contractDigest；② 主工作区停在基线 `81e350f…`，因此「现状代码」= **基线代码**（对本轮的技术前提核对足够）；WP1/WP2 的交付物在其分支/worktree 中，未逐个复读；③ 不审代码 diff、不判用例集合齐备性（属 main 的 Coverage Index 与 validator） | 本报告（待落盘） | 计划内：Coverage Index 37 行、PV1/PV2、C1/C2（仅核对登记与可产出性）；E2E = not-applicable |
---
## 1. D2「两列落盘提交点」的订正：**成立且自洽**（逐条独立核实）
| 断言（design D2 的 Round 9 订正 / plan 的 D2 行） | 我的独立核对 | 结论 |
| --- | --- | --- |
| 「`session.create` 的终态由适配层提交」 | `crates/server/src/node_link/command.rs:724-728` 调 `core.create_session` → `:729` 调 `session_create_result` → `:765-800` 才调 `core.settle_session_create`（三种 outcome 各一次）。终态提交确实在适配层，且**晚于**结果投影 | ✅ 属实 |
| 「适配层在 settle 之前已投影 `sessionMeta`」 | `:884-916` 的 `session_create_result` 调 `core.node_link_session_view`，再用 `node_link::resource::session_meta(&view.session)`（`resource.rs:1221`）取 `state`+`version` | ✅ 属实 |
| 「`StateChange::Update` 会 bump version」 | `crates/storage-sqlite/src/session_store.rs:1091-1100`：`UPDATE owned_session SET … version = version + 1 … RETURNING version` | ✅ 属实 |
| 「在终态提交里写这两列会让 `sessionMeta.version` 与落盘值错开」 | 由上述三条合成：投影（读到 v）→ 终态提交（若带 `StateChange::Update` 则 v+1）→ 回给 Access 的是投影时的 v。**另**：`settle_session_create` 的签名只有 `(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>)`（`use_cases.rs:277-288`、`broker.rs:1440-1447`），**拿不到** core 解析出的 `ResolvedWorkspace::canonical_path()` | ✅ 属实的双重理由 |
| 「改为 `create_session` 内、`factory.create` 返回后紧接着一次 `StateChange::Update`」是否可做 | 基线 `broker.rs:1369-1428`：`Create` 提交 → `backends.create(...)` 返回 `Box<dyn SessionEndpoint>` → 之后仍在同一函数内；`StateChange::Update` 走 `state: Some(...)`、`expected_version: None`（不会被版本冲突挡住） | ✅ 可实现（且 `SessionUpdate` 需扩两列，属 WP3 的 `crates/core/`） |
| 「spec R6 字面满足」 | `specs/local-agent-host/spec.md:11`（Scenario）：「core 可在**同一会话创建流程的提交**中把它与已解析的规范化目录一起写入该会话行」——后续一次提交仍在「创建流程」内 ✅。但同文件 `:7`（Requirement 正文）写的是「由 core **在创建提交中**自己持久化」，措辞比 Scenario 紧（见 F46，非阻断） | ✅（Scenario 字面满足；Requirement 正文措辞有冗余空间） |
| 「不与 v5 语义冲突」 | `specs/storage-schema-v2-migration/spec.md:24-28`（R17「一个新建会话在**其创建流程**中写入…」）、`:50`（R22「恢复流程不覆写」）、`:19-23`（R16 升级行保持 `NULL`）、`docs/CORE_PORTS_AND_STORAGE.md:1140-1147` 的追加列约定（不触发 12-step 重建）——三处都与「创建流程内、单次 UPDATE」相容 | ✅ 无冲突 |
| 「`settle_session_create` 签名不变 ⇒ WP6 调用点零改动」是否与 plan 的 WP6 行一致 | plan 的 WP6 行没有列任何 create 路径改动，只列 `core_payload` 臂、`port_error_code` 臂与 `load_recovery` 实现；`command.rs` 的 create 路由也不引用新东西 | ✅ 一致 |
**副发现（非阻断）**：订正后的提交点会让**新建会话在创建后立刻从 v1 变成 v2**（Create 提交 v1 → Update v2；`session.create` 终态提交用 `state: None`，不再 bump）。可观察后果有二：① `docs/NODE_LINK_PROTOCOL.md` §12.7 的示例 `"sessionMeta": { "state": "idle", "version": "1" }` 从此与真实取值不符（示例非规范，且该文件的后续修改权目前在 WP2 之后无归属，CR2-F3 已登记同类问题）；② 在「Create 提交完成 → Update 提交完成」之间，本地客户端若 `session.list` 读到 v1 并随后带 `expectedVersion = 1` 提交写命令，会得到 `state.version_conflict`。两者都不与任何 spec 冲突，故按 SUGGESTION 报（F46）。
## 2. D3「错误映射归属」的订正：**成立**（(a)–(d) 逐项）
| 子问题 | 核对结果与依据 |
| --- | --- |
| (a) `port_error_code` 现状无法区分 `Unavailable(_)` | ✅ 属实。`crates/server/src/node_link/command.rs:2065-2070`：`InvalidRequest(reason)` → 原文、`NotFound(_)` → `command.not_found`、**其余全部（含 `Unavailable(_)`）→ `internal.unavailable`**（通配臂 `_`）。若不改，`BackendUnsupported` 会经 `wire_error` 落到 `ErrorCode::InternalUnavailable`（`error.rs:140`）而非 `nodelink.command.unsupported` |
| (b) 由 WP6 承担是否与其写范围一致 | ✅ 一致。plan 的 WP6 写范围为 `crates/server/`、`crates/app/`、`docs/SESSION_CONTINUITY_DESIGN.md`、`README.md`、`docs/DEVELOPMENT_PLAN.md`；`port_error_code` 就在 `crates/server/src/node_link/command.rs`。design 选它的理由（core 不吐 wire 码、`InvalidRequest` 漏斗越层）与既有分层一致 |
| (c) 是否引入新的封闭词表条目 | ✅ **没有**。`nodelink.command.unsupported` 已在 `compatibility/errors/v1/errors.json:79`（既有）；`port_error_public` 若取 `command.unsupported`，该码在 `errors.json:26` 既有，且 `command.rs:2017-2021` 的 `wire_error` 已把 `command.unsupported` 与 `nodelink.command.unsupported` 一并映射为 `ErrorCode::CommandUnsupported`（`crates/node-link-protocol/src/error.rs:58/135`）；`local.unavailable` 已是 `schemas/local-admin/v1/envelope.schema.json:63` 的既有码，且 `local_admin/params.rs:928-930` 对**任意** `Unavailable` 都映射为 `LocalErrorCode::Unavailable`，`params.rs:1562-1568` 的 `UnavailableKind::ALL` 循环会自动覆盖新取值 → **本地管理侧零改动**。新增的只是 core 内部枚举取值（`crates/core/src/model/error.rs:74-121` 的 `UnavailableKind` + `ALL` + `as_str`），不属任何协议词表 |
| (d) 与 `specs/node-link-owner-server/spec.md` 相容 | ✅ R34（`:47`）要求「未宣告能力时 MUST 以 `nodelink.command.unsupported` 终态失败」——订正后的映射正是确定性地产生该码，且不新增码。**注意**：R37（`:59-62`）的 `status = "failed" 或 uncertain` 析取本轮**未被修改**（本轮未动 specs），Round 8 的 F39 口径仍以「记录层登记」方式生效（见 §5 的核对） |
## 3. `SessionStore::load_recovery` 窄读取：**守住了 §3.6 边界；实现点枚举有计数误差（F42）**
- **边界**：`docs/CORE_PORTS_AND_STORAGE.md:175` 的既有约束是「`ResolvedWorkspace.canonical_path` … 不得进事件、错误 `details`、审计 `detail_digest` 的前像或 Node Link catalog」。而 `Session`/`SessionSummary`（§3.3:97-98）正是**可投影形状**：`server::node_link` 的 `sessionMeta` 就是 `session_meta(&SessionSummary)`（`resource.rs:1221` 只取 `state`/`version`），快照与 catalog 同源。若把 `workspace_cwd` 加进 `Session`/`SessionSummary`，本机规范化路径就会进入可投影面——**窄读取确实必要**。持久化本身不受该条禁止（`owned_workspace.canonical_path` 早已落盘，`§7.3:1140-1147`）。
- **实现点逐点枚举（我自己 grep 全仓 `SessionStore for`，不采信计数）**：共 **7 处实现、分布于 6 个文件**：
| # | 位置 | 归属（按 plan） |
| --- | --- | --- |
| 1 | `crates/core/src/broker.rs:4412`（`FakeStore`） | WP3 |
| 2 | `crates/storage-sqlite/src/session_store.rs:1942`（`SqliteStore`） | WP4 |
| 3 | `crates/server/src/local_admin/test_support.rs:817`（**`NotTouched`**） | WP6 |
| 4 | `crates/server/src/local_admin/test_support.rs:961`（**`FixedStore`**） | WP6（**未被点名**） |
| 5 | `crates/server/src/node_link/command/tests.rs:145`（`CommandStore`） | WP6 |
| 6 | `crates/server/src/node_link/resource/tests.rs:265`（`SliceStore`） | WP6 |
| 7 | `crates/app/tests/support/owner.rs:416`（`FlakySessionStore`） | WP6 |
  design D3 的「**6 个** `SessionStore` 实现点」与 plan WP6 的「本 crate 内的**三个**测试替身」都是**文件级**口径：按文件数确实是 6（WP3×1 / WP4×1 / WP6×4），但按 `impl` 数是 7，且 `local_admin/test_support.rs` 一个文件里有两个结构体（`NotTouched` 与 `FixedStore`），plan 的「三个测试替身」在 server 内实际是**4 个**。→ **F42（MINOR）**。影响有限：WP6 的写范围是整个 `crates/server/`，且缺实现会直接编译失败（自暴露），不存在「无主文件」。
## 4. WP3 写范围扩张到 §3.3/§3.6/§4：**三处确实含 core 侧形状；登记自洽；与 WP4 不冲突**
| 区域 | 文档实际内容（我读了原文） | 与 plan 声明的动作是否对得上 |
| --- | --- | --- |
| §3.3「会话、turn、命令与交互」（`CORE_PORTS_AND_STORAGE.md:95`） | `:111` 有 `CommandPayload` 行（枚举逐项列出） | ✅「加 `SessionResume{}`」落点正确 |
| §3.6「后端与能力」（`:168`） | `:174` 有 `CreateSessionRequest`、`:175` 有 `ResolvedWorkspace`（后端与能力族的请求/值对象表） | ✅「加 `ResumeSessionRequest` 行」落点正确 |
| §4「`core::use_cases` 用例面」（`:192`） | 表内 `SessionLifecycle` 行同时列 `create_session` 与 `settle_session_create` | ✅「加 `resume_session` 入口」落点正确 |
- **Shared File Ownership 登记一致**：`docs/CORE_PORTS_AND_STORAGE.md` 行已写「WP3 改 §2、§3.1、§3.3、§3.6、§4、§5.1、§5.2；WP4 改 §7 标题、§7.2、§7.3」，并注明「区域不相交；两包分处 W2/W3」。§7（`:843` 起，标题现为「v4 表结构」）与 §2/§3.x/§4/§5.x 确无重叠 → **与 WP4 的 §7 区域不冲突**；两包分处 W2/W3，`Execution Waves` 的层级算式仍严格晚于其 `code:` 依赖所在层（W2=WP3、W3=WP4/WP5、W4=WP6、W5=TP2）。
- **登记缺一项（非阻断）**：`load_recovery` 的返回类型 `SessionRecoveryRecord`（plan WP4 行与 design D3 都点名）**没有任何 write scope 归它**——WP3 的文档行只列 `CommandPayload` / `ResumeSessionRequest` / `SessionLifecycle`；而按 `AGENTS.md` §10，core 值对象形状的权威在 `CORE_PORTS_AND_STORAGE.md` §3。同时 `crates/core/src/model/session.rs:754-756` 的既有不变量注释（「`CommandPayload` 按命令名一对一；`session.create` **不在其中**——它经 `CreateSessionRequest` 与 Node Link 路径进入」）在加入 `CommandPayload::SessionResume{}` 后会与事实不符（resume 走的也是专用路径）。→ **F45（SUGGESTION）**。
## 5. 本轮是否引入新的不一致：逐项
| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| Coverage Index 37 行 heading ↔ 5 份 specs 标题逐字对应 | ✅ **通过**（37/37） | 我把 5 份 spec 的全部 `###`/`####` 行（4+8+10+4+11 = 37）与 plan 的 37 行 `source.heading` 逐条比对：R1–R37 全部逐字命中（含 R1「往返保真」、R10「…且不发送恢复请求」、R16「v4 到 v5 升级保留既有会话行且新列为空」等） |
| Coverage 的 `tasks`/`checks` 引用实存 | ✅ 通过 | `tasks` 引用集合 = {2.1–2.6, 4.2}，均在 `tasks.md` 存在；`checks` = {PV1, PV2}，均在 `## Verification Strategy` 定义 |
| Execution Waves 层级算式合法 | ✅ 通过 | W1 = WP1/WP2/TP1（依赖 none）→ W2 = WP3（code:WP2）→ W3 = WP4（code:WP3）、WP5（code:WP1+WP3）→ W4 = WP6（五上游）→ W5 = TP2（六上游），每层严格晚于其 code 依赖 |
| Serialization Reason 合法；WP3 扩大写范围后与 WP4 仍是不同波次 | ✅ 通过 | 9 行全为 `NOT_APPLICABLE`；逐波写集合：W1（acp-protocol/matrix/fixtures、node-link-protocol/identity-auth/commands.json/schemas/fixtures/4 份文档/脚本/broker.rs/AGENTS/README/ci.yml、reports/tp1-*）、W2（core + `CORE_PORTS…` §2/§3.x/§4/§5.x）、W3（storage-sqlite + `CORE_PORTS…` §7；agent-host）——**W2 与 W3 的 `CORE_PORTS…` 区域不相交**（§2/§3.1/§3.3/§3.6/§4/§5.1/§5.2 vs §7/§7.2/§7.3）；同文件跨波者均有 Merge Owner/Order/Re-verify 登记 |
| 三个门禁标记 | ✅ 通过 | `[e2e-owned]`（tasks 8.3）、`[final-verification]`（9.1）、`[validation]`（7.1）各唯一、分开 |
| Main E2E 决策完整性 | ✅ 通过 | `mode: not-applicable` + `reason` + `basis` + 非空 `alternative_checks: [C1, C2]` + `downgrade_approval`（含用户原话、时间、来源）齐备，且 C1/C2 在 `verification.md` 的 `## Checks` 各有行 |
| tasks.md 的 2.3/2.4/2.6 是否与 plan 的新义务一致 | ❌ **不一致（F43，MINOR）** | 逐条见下表 |
| Dependency Handoffs 的红窗口段是否仍准确 | ⚠️ **部分陈旧（F44，MINOR）** | 见下 |
| F36–F40 的处置是否落地 | ✅ 已核（见下） | — |
**F43 的具体差异（plan 有新义务、tasks 未同步）**
| tasks 行 | plan 的新义务 | tasks 是否含 |
| --- | --- | --- |
| 2.3（WP3） | ① `create_session` 内 `factory.create` 之后紧接着一次 `StateChange::Update` 的**提交点**；② `SessionStore::load_recovery` 窄读取（读取形状）；③ 文档区域 §3.3/§3.6/§4 | ❌ 仅有「`owned_session` 的提交与读取形状携带两个新字段」这一句笼统表述，且文档区域仍写「§2/§3.1/§5.1/§5.2」 |
| 2.4（WP4） | 额外义务：`SqliteStore` 必须实现 `SessionStore::load_recovery`（返回含 `agent`/`agent_session_id`/`workspace_cwd` 的 `SessionRecoveryRecord`） | ❌ 未提 |
| 2.6（WP6） | 额外义务：① `port_error_code` 新增一条臂（`UnavailableKind::BackendUnsupported` → `nodelink.command.unsupported`）；② 四处替身实现 `load_recovery`（`local_admin/test_support.rs`、`node_link/command/tests.rs`、`node_link/resource/tests.rs`、`app/tests/support/owner.rs`） | ❌ 未提（且原文「实现 WP3 新增的**两个**必需方法」在方法数变为三个后已失真） |
| 3.7（CR6） | Round 8 的 F39 处置声称把「`uncertain` 只属崩溃窗口、能力不支持路径终态必须是 `failed`」的口径也写入 CR6 的复核清单 | ❌ 3.7 只有笼统的「越权拒绝无副作用与 uncertain 语义」；对照 3.6（CR5）已按 F36 口径写成「失败无**残留进程**」✅ |
**F44 的具体差异（红窗口段陈旧）**：`## Dependency Handoffs` 的红窗口段仍写「WP3 新增**两个**必需 trait 方法后，四个下游实现点（`agent-host`、`server/node_link/command/tests.rs`、`server/src/local_admin/test_support.rs`、`app/tests/support/owner.rs`）与 `storage-sqlite` 的既有测试字面量会失配」。事实是：trait 方法现在是**三个**（`SessionBackendFactory::resume`、`SessionEndpoint::agent_session_id`、`SessionStore::load_recovery`），涟漪面新增 `storage-sqlite/src/session_store.rs`（新实现，不只是既有测试字面量）与 server 侧的第 4 个替身 `FixedStore`，WP6 还多背一条 `port_error_code` 臂。义务本身已落在 WP4/WP6 行（所以**不阻断**），但该段是全篇唯一的「红窗口责任人 + 范围」摘要，读者按它核对会漏点。
**F36–F40 处置核对（沿用原 ID）**
| 原 ID | 处置声称 | 我的核对 |
| --- | --- | --- |
| DR1-F36（MINOR） | 不改契约，改口径登记；写进 WP5/CR5 派发提示与 tasks 3.6 | ✅ 可核：`verification.md` 的 `## Check Plan Changes` Round 8 条与 `tasks.md:3.6`（「失败无**残留进程**」）一致 |
| DR1-F37（MINOR） | 第 7 轮报告已落盘，八份全可读 | ✅ 可核：`reports/dr1-dependency-review-round7.md` 实存且内容完整（首 15 行含 Shared Report 全部字段） |
| DR1-F38（MINOR） | CR1 行恢复准确表述 | ✅ 可核：`verification.md` 的 CR1 行现为「（历史）当时声明的 … 不存在 …（DR1-F38：本行原先被批量替换改坏，已恢复准确表述）」 |
| DR1-F39（MINOR） | 不改契约；口径写入 WP6/TP2 派发提示与 CR6 复核清单 | ⚠️ 部分可核：口径登记段存在 ✅；但 `tasks.md` 的 CR6（3.7）**未**承载该口径（见 F43 第 4 行）。specs 的 R37 析取（`:62`）本轮未改，仍为已登记的读法口径 |
| DR1-F40（SUGGESTION） | 交 TP1 修复轮更新 | ✅ 状态一致：`reports/tp1-test-design.md:51` 仍引「圆形保真」，属待修复轮内容 |
## 6. 独立判断：本轮回写是否把「实现的现实」正确落进契约？
**D2/D3 两处订正：是。** 四段技术主张我逐条在基线代码里复现（§1、§2），没有发现其中任何一句是「想当然」：适配层先投影后 settle、`Update` 会 bump version、`settle_session_create` 拿不到 cwd、`port_error_code` 的 `Unavailable` 落 `internal.unavailable`、所有映射目标码都是既有码。改成「core 在创建流程内自己提交一次 Update」确实同时满足 R6、不破坏 v5 语义、且让崩溃窗口更稳。
**但同一套推理暴露出一个**同族、未被本轮处理的**缺口：`session.resume` 的 `completed` 终态结果由谁投影、由谁落盘——**无可行承担者、也无归属**（DR1-F41，MAJOR）。证据链（全部来自基线代码与既有契约）：
1. 需求侧：`specs/node-link-owner-server/spec.md:7`（R27）「`status = "completed"` 时 `terminal.result` MUST 存在且为 object（… `session.resume` 必须是 `SessionResumeResult`）」；`:47`（R34）同义；且 wire schema 对 `session.resume` 的 `completed` 已加 `if/then` 收紧（WP2 落地，CR2-F2 已记录）。TP1 的 SR-R27-3/SR-R34-1（`reports/tp1-test-design.md:402/:450-452`）也按此断言。
2. `SessionResumeResult = { remoteSessionRef{ownerNodeId, exportId, sessionId}, sessionMeta }`（`design.md:47`，形状与 `SessionCreateResult` 一致）。`remoteSessionRef` 的 `exportId` 是**适配层概念**：`command.rs:884-916` 的 `session_create_result` 是由适配层传入 `&export_id` 才拼出来的；core 的 `ResumeSessionRequest`（`design.md:85`）只有 `{ agent, agent_session_id, workspace }`，**没有任何 export 信息**，而「Export 是否对该节点可见」按 `CORE_PORTS_AND_STORAGE.md:192-224` 是**适配器的单点可见性策略**（core 不重复实现）→ **core 物理上拼不出 `remoteSessionRef`**。
3. 流程侧却把这件事派给了 core：`design.md` D3 步骤 5「把会话状态抬回可交互态并写终态」（本轮与历轮均如此写），`tasks.md:2.3` 也写「…→ 终态/uncertain」。
4. 适配层无路可走：core 对外的终态写入入口只有 `UseCases::settle_session_create`（`use_cases.rs:277`），而它在 `broker.rs:1454-1458` **显式拒绝** `command != "session.create"` 的记录；我把 `use_cases.rs` 的全部 `pub async fn` 列了一遍，没有任何通用「按调用方给定 result 落终态」的入口；其余 `command_terminal: Some(…)` 都在 broker 内部异步 turn/取消路径上，适配层不可达，其结果也都是 core 视图（`view_command_*`）而非 wire DTO。
5. 绕过尝试都不成立：让 core 直接构造 Node Link 形状的 JSON = 越层（design 自己用同一理由否决了 core 吐 wire 错误码）；让适配层在发送时把 `{}` 改写成 `SessionResumeResult` = 违反 R27 的「同形回复」（`command.status` 重查读的是同一条持久记录）且过不了 wire schema。
6. plan/tasks 未补：WP3 行的产出清单是「值对象、端口、用例、payload 变体与分发、错误映射」，WP6 行是「路由与接线 + `core_payload` 臂 + `port_error_code` 臂 + `load_recovery` 实现 + 受控路径用例 + 文档注记」，**两处都没有「投影 `SessionResumeResult`」或「resume 终态结算入口」**；`tasks.md` 2.3/2.6 同样没有。
结论：按当前书面契约，这一条义务**没有能完成它的包**——WP6 若照做必须改 `crates/core/`（越出写范围，需重开 WP3），否则只能再次 BLOCKED；这正是本轮存在的意义所要消除的那类歧义。最小修法很小（下面 Findings 的 Recommendation），但它必然改 `design.md`/`plan.md`/`tasks.md` ⇒ contractDigest 变化 ⇒ 需第 10 轮。
## Findings
### 复核项（原问题 ID 沿用）
| ID | Severity | 复核依据 | Recheck 结论 |
| --- | --- | --- | --- |
| DR1-F36 | MINOR（原） | 口径登记 + tasks 3.6「失败无残留进程」 | **已闭环（记录层）** |
| DR1-F37 | MINOR（原） | `reports/dr1-dependency-review-round7.md` 实存且完整 | **已闭环** |
| DR1-F38 | MINOR（原） | CR1 行已恢复准确表述 | **已闭环** |
| DR1-F39 | MINOR（原） | 口径登记存在；但 CR6（tasks 3.7）未承载该口径 → 转 F43 | **部分闭环** |
| DR1-F40 | SUGGESTION（原） | `tp1-test-design.md:51` 仍引旧标题，属待修复轮 | **保持待修复轮** |
### 新发现（本线程连续编号）
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation（最小修复面） |
| --- | --- | --- | --- | --- | --- |
| **DR1-F41** | **MAJOR** | `design.md` D3 步骤 5（`design.md:78`）与 D1 的 `terminal.result` 行（`:47`）；plan 的 WP3 行/`## Contract Changes` Round 9 段；`specs/node-link-owner-server/spec.md:7`（R27）、`:47`（R34）；`tasks.md` 2.3/2.6 | `session.resume` 的 `completed` 终态结果必须是 `SessionResumeResult`，而 `remoteSessionRef.exportId` 只有适配层有（core 的 `ResumeSessionRequest` 无 export 信息；Export 可见性是适配器单点策略）。core 对外**唯一**的终态写入入口 `settle_session_create` 在 `broker.rs:1454-1458` 显式拒绝非 `session.create` 记录，`use_cases.rs` 也没有通用 settle（全量 `pub async fn` 已枚举）。而 design D3 步骤 5/tasks 2.3 把「写终态」派给 core 用例，plan 的 WP3/WP6 行均未提「投影 `SessionResumeResult`」或 resume 结算入口 | WP3 与 WP6 都无法在不越界/不违背 R27「同形回复」的前提下完成该义务：WP6 需要 `crates/core/` 的新入口（越出写范围），最可能的结局是又一次 BLOCKED 或静默扩权；TP2 的 SR-R27-3/SR-R34-1 也无从通过 | 照 create 的既有模式把三处写清楚（约 4 行）：① `design.md` D3 步骤 5 改为「`resume_session` 返回 `SessionId`；**适配层**用与 `session_create_result` 同源的映射投影 `SessionResumeResult`，并经 core 的 `settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool`（与 `settle_session_create` 同形、只终结 `session.resume` 记录）落终态」；② plan 的 WP3 行补该入口（`crates/core/`）、WP6 行补「投影 `SessionResumeResult` 并 settle」（`crates/server/`）；③ `tasks.md` 2.3/2.6 同步；④ `docs/CORE_PORTS_AND_STORAGE.md` §4 的 `SessionLifecycle` 行加第二个入口（该区域已是 WP3 的登记写范围） |
| DR1-F42 | MINOR | `design.md:110`（「**6 个** `SessionStore` 实现点」）、plan 的 WP6 行（「本 crate 内的**三个**测试替身」） | 我 grep 全仓 `SessionStore for`：共 **7 处 impl / 6 个文件**，其中 `crates/server/src/local_admin/test_support.rs` 有**两个**结构体（`NotTouched:817` 与 `FixedStore:961`），plan 的「三个测试替身」在 server 内实际是 4 个 | 计数与枚举不准；不影响可产出性（WP6 拥有整个 `crates/server/`，缺实现会编译失败而自暴露） | 两处改为「6 个文件 / 7 处实现，其中 `local_admin/test_support.rs` 含 `NotTouched` 与 `FixedStore` 两个替身」；与 F41 同批改（同为 digest 变更） |
| DR1-F43 | MINOR | `tasks.md` 2.3 / 2.4 / 2.6 / 3.7 | 见 §5 的四行差异表：plan 的新义务（update 提交点、`load_recovery`、§3.3/§3.6/§4 区域、`port_error_code` 臂、四处替身实现）未进 tasks；2.6 的「两个必需方法」已失真（现为三个）；3.7（CR6）未承载 F39 的 failed/uncertain 口径 | tasks 是实现者与台账的权威任务源（`role-report` 要求 task_id 与 tasks.md 对应）；缺项会让 WP3/WP4/WP6 只按 tasks 读时漏做（plan 行仍含义务，故非阻断） | 与 F41 同批（digest 已变）把 2.3/2.4/2.6 逐条补上并改写 2.6 的「两个必需方法」；3.7 补一句「能力不支持路径终态必须是 `failed`，`uncertain` 只属崩溃窗口」。若 main 决定本轮不改 tasks，则必须把这几条义务写进三包的派发提示，并在 `## Check Plan Changes` 记录「tasks 与 plan 的已登记偏差」 |
| DR1-F44 | MINOR | plan 的 `## Dependency Handoffs` 红窗口段（「WP3 新增**两个**必需 trait 方法…四个下游实现点…」） | 见 §5；trait 方法已为三个（+`SessionStore::load_recovery`），涟漪面新增 `storage-sqlite/src/session_store.rs` 与 `FixedStore`，WP6 另多一条 `port_error_code` 臂 | 该段是全篇唯一的红窗口「范围 + 责任人」摘要，按它核对会漏点（义务本身已在 WP4/WP6 行，故非阻断） | 一句改写：「WP3 新增三个必需 trait 方法（`resume`、`agent_session_id`、`SessionStore::load_recovery`）后，`agent-host`、`storage-sqlite`（新实现）、server 的四个替身（含 `FixedStore`）与 `app/tests/support/owner.rs` 会失配，另加 `port_error_code` 一条臂；收口责任人 WP4/WP5/WP6 不变」 |
| DR1-F45 | SUGGESTION | plan 的 WP3 行文档区域清单（§3.3/§3.6/§4）与 design D3 的 `SessionRecoveryRecord`；`crates/core/src/model/session.rs:754-756` 的既有不变量注释 | 新类型 `SessionRecoveryRecord`（`load_recovery` 的返回形状）无任何 write scope 归它登记；`CommandPayload::SessionResume{}` 加入后，`§3.3` 的「按命令名一对一」表述与 `session.rs` 的「`session.create` 不在其中」注释会同事实不符 | 属文档/注释漂移；`check:contract-drift` 只比对 §5/§7，**这类漂移不会被门禁发现** | 与 F41 同批：§3.3/§3.6 加 `SessionRecoveryRecord` 一行并注明「不进 `Session`/`SessionSummary`」；`§3.3` 与 `session.rs` 的注释补「`session.create`/`session.resume` 走专用路径，为兼容 wire 穷尽映射保留变体」之类说明 |
| DR1-F46 | SUGGESTION | `specs/local-agent-host/spec.md:7`（Requirement 正文「由 core **在创建提交中**自己持久化」）vs design D2 订正后的「创建流程内**紧接着的一次** Update 提交」；`docs/NODE_LINK_PROTOCOL.md` §12.7 的示例 `"version": "1"` | Scenario（`:11`）字面满足；Requirement 正文的「创建提交」比 Scenario 紧（其唯一自洽读法是「core 在创建流程中的提交」，否则与 R7「未取得标识时不编造取值」不可兼得）。另：新增 Update 提交让新建会话的可见版本变成 2，示例仍写 1；且 Create→Update 之间存在本地客户端带 `expectedVersion = 1` 被判 `state.version_conflict` 的窄窗口 | 纯措辞/示例层，不使任何要求不可满足；但 WP4/CR4/validator 仍可能按字面质疑 R5 的可满足性（与 F31 同类误读面） | **不改 digest 的做法**：在 `## Check Plan Changes` 固定读法（「创建提交」= core 在创建流程中的提交，不是 §7 建行的那一次）并写入 WP4/CR4/validator 的派发提示；若同批要改契约，则顺手把 R5 正文改为「在同一会话创建流程的提交中」并把 §12.7 示例版本改为注释说明（该文件当前无后续修改权归属，可参照 CR2-F3 一并处理） |
**New findings 汇总：1×MAJOR（F41）、3×MINOR（F42–F44）、2×SUGGESTION（F45–F46）。**
## Check Plan 核对
- 本轮为 **plan 类型**，计划内检查 = `plan.md` 的 `## Coverage Index`（37 行）、`## Verification Strategy`、`### Main E2E` 决策与三个门禁标记。我逐行核对了 `source.heading`（37/37 逐字实存）、`tasks`/`checks` 引用（全部实存/已定义）、Execution Waves 与 Serialization Reason（合法，同波无写入重叠）、Main E2E 四要素与 `alternative_checks`（C1/C2 双向登记）。
- **不存在影响本轮判断的待返回执行证据**：PV1/PV2/C1/C2 的结果行属后续阶段（`verification.md` 的 `## Checks` 目前只登记阶段、范围与证据路径，未冒称通过），E2E 为 not-applicable。**plan review 的结论不覆盖任何 PV/E2E 是否通过**。
- 本轮未改任何计划字段，故无新的 Check Plan Changes 需要 reviewer 记录；`verification.md` 现有三条（Round 4/6/8）与机器事实一致，仅 F39 的 CR6 承载面有偏差（已在 F43 报）。
- `## Dependency Declaration Review` 当前最大轮次行仍是 Round 8（`sha256:57b47d87…`）；按 `procedures/workflow-check.md` 的「plan 阶段取最大 Round 且绑定当前 contractDigest」，**本轮回填前 plan 门禁仍会红**——属 main 的记录动作。
## Assessment
- **本轮检视结论：FAIL**，对应 `target_revision` = `sha256:f4468e70…c682d`。**唯一阻断项 = DR1-F41（MAJOR）**：「`session.resume` 的 `completed` 终态结果（`SessionResumeResult`）由谁投影、由谁落盘」在 design/plan/tasks 三处都没有可行承担者与归属——core 拿不到 `exportId`（适配器单点可见性策略），core 对外也没有第二个终态写入入口（`settle_session_create` 显式拒绝非 create 记录，其余 `pub async fn` 已全量枚举），而 design D3 步骤 5 与 tasks 2.3 却把「写终态」派给 core 用例。按判据「已确认且未解决的 CRITICAL/MAJOR 即 FAIL」，本轮判 FAIL。**注意**：这不是 D2/D3 两处订正本身的问题——§1/§2 的八条技术主张我逐条在基线代码里复现，全部属实且自洽。
- **本轮回写中确认正确的部分**（不改判 FAIL 的结论）：D2 提交点订正成立且不破坏 v5 语义/`settle_session_create` 签名不变（§1）；D3 错误映射归属成立，映射目标码全是既有码、不新增封闭词表条目、WP6 写范围自洽（§2）；`load_recovery` 窄读取确实守住 `docs/CORE_PORTS_AND_STORAGE.md:175` 的 §3.6 边界（§3）；WP3 的 §3.3/§3.6/§4 扩张落在真实含 core 形状的区域、与 WP4 的 §7 不冲突且分处不同波次（§4）；Coverage 37/37 逐字、Waves/Serialization/门禁标记/Main E2E 全部合法（§5）。
- **非阻断项清单（按建议处理顺序）**：① F43（tasks 2.3/2.4/2.6/3.7 与 plan 的新义务不同步；与 F41 同批改最省）；② F42（实现点计数 6 vs 7、"三个测试替身" vs 4 个）；③ F44（红窗口段仍写「两个必需 trait 方法」）；④ F45（`SessionRecoveryRecord` 无文档登记 + §3.3/`session.rs` 注释将失效）；⑤ F46（R5 正文「创建提交」措辞 + §12.7 示例版本 + Create→Update 间的版本冲突窗口）。
- **待补证据与门禁**：PV1（各 WP 阶段 1、集成基线/候选/主分支阶段 2）、PV2 与 C1/C2 的 PASS 行须在对应交付/验证门禁前补齐；本轮为规划审查，不要求也不核对执行证据。
- **复用依据**：本轮不复用任何历史轮结论作为结论依据；`round7/round8.md` 仅作前轮基线与 F36–F40 处置的核对对象（F37 的落盘已核实可读）。历史轮次保留不覆盖，当前结论以 Round 9（本报告）为准。
- **受阻/限制**：无 shell/引擎 → 未复算 contractDigest；主工作区为基线 `81e350f…`，WP1/WP2 交付物在其分支内未逐个复读（其证据行来自 `verification.md` 与 CR1/CR2）。
## 给 main 的回填指引（记录层）
1. 把本报告原样落盘到 `openspec/changes/session-resume/reports/dr1-dependency-review-round9.md`（**先落盘**，否则路径不可读，重演 F37）。
2. 在 `## Dependency Declaration Review` 增一行：`DR1 | 9 | reviewer-DR（第九个实例，非任何 WP Owner，与前八轮均不同实例）| sha256:f4468e70fdc0de96c4eb4942fbd923493e90d82650d40daa47c5922d21dc682d | FAIL | openspec/changes/session-resume/reports/dr1-dependency-review-round9.md`（`Result` 单元格保持裸 `FAIL`，不要加 `**…**` 或括注——Round 5/6 已记录门禁判据是「去空白后忽略大小写必须恰好等于 PASS/FAIL」这一解析面）。
3. 在 `## Review Findings` 增补 DR1-F41…F46 六行（Severity/Location/Trigger-Evidence/Impact/Recommendation/Recheck 按现有格式），并把 F36–F40 的 Recheck 列补为「DR1 Round 9 复核：已闭环/部分闭环（F39 转 F43）/待 TP1 修复轮」。
4. 处置顺序建议：**先裁 F41**（约 4 行改动：design D3 + plan 的 WP3/WP6 行 + tasks 2.3/2.6 + §4 的 `SessionLifecycle` 行），与 F42/F43/F44/F45 同批；这批都触及 `design.md`/`plan.md`/`tasks.md` ⇒ digest 变化 ⇒ 需**第 10 轮**独立复核后才允许 WP3 继续落笔。若 main 认为 F46 值得一并改（R5 正文 + §12.7 示例），同批改`specs`也会改变 `## Target` 的摘要基线，需一并刷新。
5. `verification.md` 当前工作副本中 `## Review Findings` 的表头为「ID | Work Package | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck」，与 Round 8 报告所列列名不同；回填时以**文件现状**为准，不要照抄历史报告的列名。
```yaml
handoff_index:
  - task_id: NOT_APPLICABLE
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    stage: plan
    round: 9
    target_revision: "sha256:f4468e70fdc0de96c4eb4942fbd923493e90d82650d40daa47c5922d21dc682d"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round9.md
    result: FAIL
    evidence_status: NEW
    applicability_basis: "针对 WP3 实施核对触发的 D2/D3 契约订正后的当前 contractDigest 做静态规划审查（第九个新实例、只读、未参与任何实现或用例设计）：独立在基线代码里复现 D2 的四条前提（适配层先投影后 settle、StateChange::Update 会 bump version、settle_session_create 拿不到 cwd/无通用 settle 入口）、D3 的四条前提（port_error_code 的 Unavailable 落 internal.unavailable、WP6 写范围含 crates/server/、映射目标码全部既有、与 R27/R34/R37 相容）；自行 grep 全仓 `SessionStore for` 得 7 处 impl/6 文件（与 design 的「6 个实现点」口径不同）；核对 §3.3/§3.6/§4 是否真含 core 侧形状与 Shared File Ownership 登记；核对 Coverage Index 37 行 heading 逐字、Waves/Serialization/门禁标记/Main E2E；核对 tasks 2.3/2.4/2.6/3.7 与 plan 新义务的差异并核对 F36–F40 的处置落地。新报 1×MAJOR（F41：resume 的 completed 终态结果投影/结算无可行承担者与归属）+ 3×MINOR（F42/F43/F44）+ 2×SUGGESTION（F45/F46）。未执行任何 PV/E2E，未复算 contractDigest（无 shell），已声明该限制"
    source_evidence: NOT_APPLICABLE
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "返回 DR1 Round 9 完整复核报告：§1 逐条独立核实 D2 订正（command.rs:724-729 先投影后 :765+ settle；resource.rs:1221 的 session_meta 含 state+version；session_store.rs:1091 的 version = version + 1；settle_session_create 签名在 use_cases.rs:277 与 broker.rs:1440 均无 workspace → 四条前提全部属实，R6 Scenario 字面满足、与 v5 语义相容、WP6 调用点零改动与 plan 一致）；§2 核实 D3（command.rs:2065-2070 的 `_ => internal.unavailable` 无法区分 Unavailable；WP6 写范围含 crates/server/；errors.json:79/:26 与 envelope.schema.json:63 证明无新封闭词表条目，local_admin/params.rs:928 对任意 Unavailable 自动映射 local.unavailable；与 R27/R34/R37 相容）；§3 证明 load_recovery 窄读取确实守住 CORE_PORTS_AND_STORAGE.md:175 的 §3.6 边界，并自行 grep 得 7 处 impl/6 文件（F42）；§4 逐处确认 §3.3 含 CommandPayload 行、§3.6 含 CreateSessionRequest/ResolvedWorkspace 行、§4 含 SessionLifecycle 行且与 WP4 的 §7 不冲突；§5 核对 Coverage 37/37 heading 逐字、Waves/Serialization/三门禁标记/Main E2E 合法、tasks 2.3/2.4/2.6/3.7 与 plan 新义务的四处差异（F43）、红窗口段陈旧（F44）、F36–F40 处置落地情况；§6 得出唯一阻断项 DR1-F41（MAJOR：resume 的 completed 终态 SessionResumeResult 无可行承担者——core 无 exportId、use_cases 全量 pub async fn 中无通用 settle、settle_session_create 在 broker.rs:1454-1458 显式拒绝非 create 记录，而 design D3 步骤 5/tasks 2.3 把写终态派给 core），Assessment = FAIL 并给出最小修复面（design D3 + plan WP3/WP6 行 + tasks 2.3/2.6 + §4 SessionLifecycle 行）与非阻断清单 F42–F46"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "NOT_APPLICABLE（只读审查，无 shell/写权限，未执行任何构建或测试）",
      "result": "not-run",
      "summary": "规划审查不执行 PV1/PV2/C1/C2/E2E；本轮结论不覆盖任何执行证据。需 main 在对应交付/验证门禁前补齐 PV1（各 WP 阶段 1 + 阶段 2）、PV2 与 C1/C2 的 PASS 行"
    }
  ],
  "validationOutput": [
    "D2 订正四条前提均属实：command.rs:724-729（create_session → session_create_result 投影）先于 :765-800（settle_session_create）；resource.rs:1217-1233 的 session_meta 只承载 state/version（投影值会被终态提交的 version+1 甩开）；session_store.rs:1091-1100 的 `version = version + 1`；use_cases.rs:277-288 与 broker.rs:1440-1447 的 settle 签名无 workspace 通道",
    "D3 订正四条前提均属实：command.rs:2065-2070 的 port_error_code 通配臂把 Unavailable(_) 归入 internal.unavailable；errors.json:79 有 nodelink.command.unsupported、:26 有 command.unsupported，node-link-protocol/src/error.rs:135 与 command.rs:2017-2021 已完成二者到 ErrorCode::CommandUnsupported 的映射；envelope.schema.json:63 有 local.unavailable 且 local_admin/params.rs:928-930 + 1562-1568 对任意 UnavailableKind 自动映射 → 不新增任何封闭词表条目",
    "全仓 grep `SessionStore for`：7 处 impl、6 个文件（core/broker.rs:4412 FakeStore；storage-sqlite/session_store.rs:1942 SqliteStore；server/local_admin/test_support.rs:817 NotTouched 与 :961 FixedStore；server/node_link/command/tests.rs:145 CommandStore；server/node_link/resource/tests.rs:265 SliceStore；app/tests/support/owner.rs:416 FlakySessionStore）→ design「6 个实现点」为文件级、plan「三个测试替身」在 server 内实为 4 个（F42）",
    "Coverage Index 37/37 行 source.heading 与 5 份 specs 的 37 个标题逐字一致（4+8+10+4+11）；tasks 引用 {2.1–2.6,4.2} 与 checks {PV1,PV2} 全部实存；Execution Waves W1–W5 严格晚于其 code 依赖；9 行 Serialization Reason 全 NOT_APPLICABLE 且同波写集合无交集（W2 的 CORE_PORTS §2/§3.x/§4/§5.x 与 W3 的 §7 不相交）；三门禁标记各唯一；Main E2E 四要素 + C1/C2 双向登记齐备",
    "阻断项证据：specs/node-link-owner-server/spec.md:7/:47 要求 completed 的 terminal.result 为 SessionResumeResult（含 remoteSessionRef{ownerNodeId,exportId,sessionId}）；exportId 只存在于适配层（command.rs:884-916 由适配层传入）；core 的 ResumeSessionRequest（design.md:85）无 export 字段；use_cases.rs 全部 pub async fn 中唯一的终态写入入口 settle_session_create 在 broker.rs:1454-1458 显式拒绝非 session.create 记录 → design D3 步骤 5/tasks 2.3 把该义务派给 core 不可实现，plan 的 WP3/WP6 行也未投影 SessionResumeResult"
  ],
  "residualRisks": [
    "无 shell/引擎能力 → 未复算 contractDigest 与 verification.md 的 6 行 specs 摘要；本轮结论绑定派发给出的 sha256:f4468e70…c682d",
    "主工作区停在基线 81e350f…：WP1/WP2 的交付物在其分支/worktree 中，未逐个复读（其对契约的影响通过 errors.json/schema 等基线机器资产与 CR1/CR2 报告核对）",
    "F41 若按建议修（design/plan/tasks 或 specs 任一改动）会使 contractDigest 变化、本轮 FAIL 的历史记录保留但需第 10 轮独立复核后才允许 WP3/WP4/WP6 继续落笔",
    "WP3 的 coder-C 仍处「保持存活、暂不落笔」状态；若 main 选择不裁 F41 而直接放行 WP3，WP6 极可能再次 BLOCKED 或越出写范围修改 crates/core",
    "F39 的 failed/uncertain 口径目前只落在 verification.md 的记录段与 tasks 3.6（CR5）；CR6（tasks 3.7）未承载，specs 的 R37 析取仍在（本轮未改 specs）",
    "本轮不改任何文件；verification.md 的 DCR 最大轮次行在 main 回填前仍为 Round 8，plan 门禁仍会红"
  ],
  "noStagedFiles": true,
  "diffSummary": "无改动：本会话为只读审查（无 write 工具），未修改、未暂存、未提交任何文件；watchdog_diff 仅显示 openspec/changes/session-resume/** 的 untracked 清单，无被跟踪文件的 staged/unstaged 改动",
  "reviewFindings": [
    "blocker: design.md:78 (D3 步骤 5)、plan.md 的 WP3/WP6 行、tasks.md 2.3/2.6 - session.resume 的 completed 终态 SessionResumeResult 无可行承担者与归属（core 无 exportId、无通用 settle 入口、settle_session_create 显式拒绝非 create 记录）→ DR1-F41 MAJOR",
    "minor: design.md:110 与 plan.md 的 WP6 行 - SessionStore 实现点计数/枚举不准（真实 7 处 impl/6 文件；local_admin/test_support.rs 含 NotTouched 与 FixedStore 两个替身）→ DR1-F42",
    "minor: tasks.md 2.3/2.4/2.6/3.7 - 未同步 plan 的 Round 9 新义务（update 提交点、load_recovery、§3.3/§3.6/§4、port_error_code 臂）；2.6 的『两个必需方法』现应为三个 → DR1-F43",
    "minor: plan.md ## Dependency Handoffs 红窗口段 - 仍写『WP3 新增两个必需 trait 方法』与四个下游实现点，漏 load_recovery 的涟漪（storage-sqlite 新实现、FixedStore）与 port_error_code 臂 → DR1-F44",
    "suggestion: plan.md 的 WP3 文档区域清单 + crates/core/src/model/session.rs:754-756 - 新类型 SessionRecoveryRecord 无文档登记归属；§3.3 的『按命令名一对一/session.create 不在其中』注释将失效（门禁不覆盖）→ DR1-F45",
    "suggestion: specs/local-agent-host/spec.md:7 + docs/NODE_LINK_PROTOCOL.md §12.7 示例 - Requirement 正文『在创建提交中』措辞比 Scenario 紧；新增 Update 提交使新建会话版本变 2（示例写 1，且 Create→Update 间存在本地客户端 version_conflict 窄窗口）→ DR1-F46"
  ],
  "manualNotes": "① 本轮为 DR1 第九轮（recheck，Review Type=plan），我是第九个新实例、只读、无 write 工具，未参与前八轮与任何实现；结论 FAIL（唯一 MAJOR = DR1-F41），对应 target_revision sha256:f4468e70…c682d。② 结论分布：第 1 项（D2 提交点订正）与第 2 项（D3 错误映射归属）我逐条独立复现，**全部属实且自洽**——本轮回写确实把「实现的现实」正确落进了契约；第 3 项窄读取守住 §3.6 边界但计数要订正；第 4 项 WP3 写范围扩张的三处落点正确、与 WP4 的 §7 不冲突；第 5 项 Coverage/Waves/Serialization/门禁标记/Main E2E 全绿，但 tasks 未同步、红窗口段陈旧。③ 唯一阻断项 F41 的实质是**同一推理族的下一个缺口**：这轮把 create 的「谁提交什么」想清楚了（D2），却没有把 resume 的「completed 终态结果由适配层投影、经 core 的 resume 结算入口落盘」写进 design/plan/tasks；证据里最硬的一条是 core 根本无法构造 remoteSessionRef（exportId 是适配器的单点可见性策略），而 `settle_session_create` 在 broker.rs:1454-1458 显式拒绝非 create 记录、`use_cases.rs` 也没有第二个终态写入入口。④ 若 main 采纳 F41 的最小修法（约 4 行）+ F42/F43/F44/F45 同批，会改变 contractDigest，需第 10 轮；若 main 认为 F41 属「WP3 的『用例』本就包含结算入口」，请务必把该读法**显式写回** design D3/WP3 行/§4 表（否则 WP6 无处可调），并把结论登记为「口径裁定」而非默认。⑤ F36–F40 的处置我逐条核到实证：F37（round7 报告实存）、F38（CR1 行已恢复准确表述）、F36（tasks 3.6 已用『无残留进程』）已闭环；F39 只在 CR5 侧落地、CR6（tasks 3.7）未承载（转 F43）；F40 仍待 TP1 修复轮。⑥ 报告回填时请以 verification.md 的**文件现状列名**为准（其 `## Review Findings` 表头与 Round 8 报告的列名不同），并把 `Result` 写成裸 FAIL。⑦ commandsRun 为空是规划审查边界（无 shell/无执行），非跳过检查；本轮结论不覆盖任何 PV/E2E。"
}
```
