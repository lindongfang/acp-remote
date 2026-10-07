<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 3 复核。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-3"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:b484b5618ffc2f51b86cf5cd3234454608889f003685531a3405480c73aa52ee"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Reviewer 独立性与 tasks.md 独立检视任务、AC1 断言可满足性、Contract Freeze 的可核对性，以及 core-derived-events / pwa-web-client 需求在 spec/design/plan/tasks 间的收窄一致性。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "退出码 1；planningDigest=plan-v2:sha256:b484b561…52ee；contractDigest=sha256:79608fd5…1e36c；requirementsDigest=sha256:c74ff6ab…1e68"
    note: "本轮开始时该命令返回的是 plan-v2:sha256:aa971d8a…d08b5a（contractDigest=efb88a1b…d8703）。经 ls --time-style=full-iso 核实，plan.md / design.md / proposal.md 在 15:15:15 被同批重写（三个 mtime 相差 9ms），发生在本轮第一次取摘要与读取正文之间；其后连续 3 次调用返回同一 b484b561，仓库内（排除 node_modules/target/.git）无 15:50 之后的写入。本报告的全部实质核对均基于 15:15:15 之后的当前字节。详见 Review Context 的「版本稳定性」行。"
  - id: "code 依赖逐条属实性（对照仓库事实）"
    scope: "WP6 code:WP5、WP7 code:WP6/WP5、TP2 code:WP3、TP3 code:WP5/WP6/WP7、TP4 code:WP7，以及 WP3/WP4/WP5/TP1 的纯 contract 声明"
    evidence: "crates/core/Cargo.toml:18-28（仅 async-trait/thiserror/p256/sha2）、crates/agent-host/Cargo.toml:11-25（core/acp-protocol/tokio/async-trait/serde_json/thiserror/tracing/sha2/base64，无 sync-protocol）、scripts/check-crate-boundaries.mjs:106（CORE_FORBIDDEN 含 sync-protocol）、docs/MODULE_ARCHITECTURE.md:481（core 行整行空白）与 :483（agent-host 行仅 core/acp-protocol）"
  - id: "Contract Freeze 引用与 Dependencies 列交叉核对"
    scope: "11 个工作包的 Contract Freeze 引用（path@WPn）是否全部落在该包 Dependencies 列内"
    evidence: "plan.md:860-870；逐包求值 workflow-check.mjs:91 的 frozenContractReadable 口径：readable 全部为 false（多路径单元格不匹配 ^(\\S+?)@(\\S+)$；单路径单元格因反引号导致 path.resolve 落空）"
  - id: "批次与波次一致性（双口径重算 earliest）"
    scope: "plan.md:888-902 的 11 行 vs Work Packages 依赖列；分别按「全部 contract 视为硬依赖」与「全部 freeze 可读、contract 视为软依赖」两种口径重算 earliest 并与声明波次比对"
    evidence: "node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:209-221（earliest/hardDeps）、:316-328（批次早于/晚于最早批次两条判定）、openspec/agentic.yaml:12-13（pool.coding=3 / pool.testing=2）"
  - id: "写入归属登记完整性（字面 glob 展开口径）"
    scope: "11 个 Write Scope 两两重叠（剥离反引号后按前缀比较，含目录级 scope 对子路径的覆盖）vs Shared File Ownership 13 行"
    evidence: "node 复现 workflow-check.mjs:114-117 的 scopePaths/scopeOverlap 语义：去引号后 9 组字面重叠中 8 组有登记行，WP5×TP3 与 WP5×TP4 无任何行同时列出两个写入者"
  - id: "Reviewer 独立性与独立检视任务"
    scope: "plan.md:860-870 的 Owner/Reviewer 列 vs tasks.md:31-51 §3 的 11 条 review 任务"
  - id: "AC1 可满足性与需求收窄一致性"
    scope: "plan.md:981 / 1014、tasks.md:56、verification.md AC1 行的四项断言；specs/core-derived-events/spec.md:66-90、specs/pwa-web-client/spec.md:192、design.md D6 与 Risks 的四处口径"
    evidence: "crates/server/src/node_link/resource.rs:152-155 与 :1027-1032（会话标识为空即 return）、crates/storage-sqlite/src/migrate.rs:85-113（session_id 可空 + 三条成对 CHECK）、crates/core/src/broker.rs:7462-7499（非会话级事件仍进入 replay 流）、crates/core/src/broker.rs:3143 与 :606/1733-1841（commit_owned 非 pub、公开面均为会话级入口）、crates/app/tests/support/owner.rs:169/173（ScriptedBackends/ScriptedCatalog 替换 agent-host）"
  - id: "依赖图无环与无整批依赖"
    scope: "依赖图拓扑、plan.md:928-935 Dependency Handoffs 的逐 WP Upstream、plan.md:959-961 Merge Strategy 的 MU 就绪条件"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-3"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 3
    stage: plan
    target_revision: "plan-v2:sha256:b484b5618ffc2f51b86cf5cd3234454608889f003685531a3405480c73aa52ee"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份增量 spec 做只读复核，并对照 crates/core、crates/agent-host、crates/storage-sqlite、crates/server、crates/app、crates/sync-protocol 的实际代码与 docs/MODULE_ARCHITECTURE.md §5、scripts/check-crate-boundaries.mjs、scripts/check-contract-drift.mjs 核实依赖声明属实性；planningDigest 由 workflow check --stage plan --json 在本轮连续 3 次重新推导，三次一致（b484b561…52ee）。本轮全部实质核对在 plan.md/design.md/proposal.md 于 15:15:15 重写之后进行。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1/2 的 Review ID） |
| Round | 3 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-3`（不拥有任何 WP/TP；owners 为 `coding-1`..`coding-7`、`testing-1`..`testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖与写入归属声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:b484b5618ffc2f51b86cf5cd3234454608889f003685531a3405480c73aa52ee` |
| 版本稳定性 | **本轮发生过目标版本变动**：首次 `workflow check` 返回 `aa971d8a…`，随后返回 `b484b561…`。经 `ls --time-style=full-iso` 核实 `plan.md`/`design.md`/`proposal.md` 的 mtime 均为 `2026-10-03 15:15:15`（相差 9ms，同批重写），而 `tasks.md`(14:42:15)、`verification.md`(14:42:32)、5 份 spec(12:05–13:33) 与两份既有报告(13:14/14:39) 未变；`find -newermt 15:50` 在仓库内（排除 node_modules/target/.git）无任何命中。当前字节在连续 3 次调用中摘要恒为 `b484b561…`。本报告每一条实质结论均以 15:15:15 之后的字节为准；若此后 `plan.md`/`design.md`/`proposal.md` 再被改写，本轮结论对新版失效 |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`schemas/agentic/schema.yaml:186-215`（依赖类型与 Execution Waves 判据）、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` §5/§7 |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:978-984`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1010-1016`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 F1–F11 的修正 |
| Previous Findings | F1–F11（Round 1 MAJOR×5 + MINOR×2；Round 2 MAJOR×1 + MINOR×3），逐项复核见下表 |
| 实际检查范围 | (a) 8 条 `code:` 与 11 条 `contract:` 声明的逐条属实性；(b) 每个包的 Contract Freeze 引用是否落在其 Dependencies 列内；(c) 依赖图无环、无整批依赖整批；(d) 11 个 WP/TP 的 Reviewer≠Owner 与 tasks.md 独立检视任务；(e) 11 行 Execution Waves 与依赖列一致性（双口径重算 earliest）+ Serialization Reason 取值域；(f) Contract Freeze 的机械可核对性；(g) Write Scope 两两重叠与 Shared File Ownership 登记；(h) AC1 四项断言可满足性与 Main E2E `not-applicable` 判据；(i) core-derived-events / pwa-web-client 收窄需求在 spec/design/plan/tasks 的口径一致性 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未核对 main E2E 之外的真实 Daemon 行为；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 WP3/WP4 落地后节点级事件的具体生产入口形态（属实现设计，见 Assessment 的不确定性说明）；未核对 TP3 与 TP4 在 `clients/app/e2e` 下的文件命名约定是否会使 TP4 产物落入 TP3 的 `**/*.test.ts` glob（计划未规定该约定，属未证实项，不计为问题） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r3.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 3） |

### Round 1 / Round 2 逐项复核结论

| 原问题 ID | 原严重度 | 本轮结论 | 复核依据（当前文本 + 仓库事实） |
| --- | --- | --- | --- |
| DDR-…-r1-F1 | MAJOR | **已解决** | `plan.md:862` WP3 依赖为 `contract:WP1, contract:WP2`。仓库侧复核成立：`crates/core/Cargo.toml:18-28` 依赖闭包只有 `async-trait`/`thiserror`/`p256`/`sha2`；`scripts/check-crate-boundaries.mjs:106` 的 `CORE_FORBIDDEN` 含 `sync-protocol`；`docs/MODULE_ARCHITECTURE.md:481` 的 `core` 行整行为空。core 不可能编译引用 `sync-protocol`，`contract:` 属实 |
| DDR-…-r1-F2 | MAJOR | **已解决** | `plan.md:863` WP4 依赖为 `contract:WP2`。`crates/agent-host/Cargo.toml:11-25` 无 `sync-protocol`；`docs/MODULE_ARCHITECTURE.md:483` 的 `agent-host` 行仅 `core`/`acp-protocol`。声明属实 |
| DDR-…-r1-F3 | MAJOR | **已解决** | `plan.md:860` WP1 的 Write Scope 已含 `crates/sync-protocol/src/command.rs`；`plan.md:918` 有对应登记行（单写者 WP1，区域说明与 WP2 的 `views.rs` 不重叠）；`tasks.md:15` 同步写明必须与 schema 同批更新。`crates/sync-protocol/src/command.rs:386-388`（`SessionRead`）与 `:906-912`（`SessionReadResult`）确为 `deny_unknown_fields` 镜像，归属已有着落，且无第二个 WP 声明该文件 |
| DDR-…-r1-F4 / r2-F8 | MAJOR | **已解决** | `plan.md:869` TP3 依赖改为 `code:WP5, code:WP6, code:WP7, contract:WP1`，波次 `plan.md:901` 改为 W5；Contract Freeze 的 8 条 `@WPn` 引用（WP1×2 / WP5×2 / WP6×2 / WP7×2）全部落在依赖列内；`tasks.md:65` 给出与 `code:` 同型的理由（测试与被测模块同处一个 TS 工程，必须 import 上游模块）；写入重叠已在 `plan.md:921` 登记（merger / WP7 → TP3 / TP3: PV3）。Coverage 中仍含 `2.7` 的行（`plan.md:613/621/629/637/644/652/660/668/675/683/691/699`）现由 `code:WP7` 保证就绪 |
| DDR-…-r1-F5 | MAJOR | **已解决** | `plan.md:981`、`plan.md:1014`、`tasks.md:56`、`verification.md` AC1 行的断言已改为「落库 + 按事件库读回 + 会话标识为空 + 会话级投递路径不误收」并在四处显式声明不验证广播。仓库侧逐条可满足：`crates/storage-sqlite/src/migrate.rs:104-106` 三条 CHECK 使 `session_id IS NULL` 与 `session_sequence/origin_epoch/origin_sequence IS NULL` 自洽；`crates/core/src/broker.rs:7462-7499` 的既有用例 `non_session_event_has_no_session_scoped_identifiers` 证明非会话级事件进入 replay 流；`crates/server/src/node_link/resource.rs:152-155` 与 `:1027-1032` 确实丢弃会话标识为空的事件，故「不被误收」可观察。需求侧收窄一致（见下） |
| DDR-…-r1-F6 | MINOR | **已解决** | `plan.md:913` 的 `schemas/sync/v1/event-views.schema.json` 行 Region Note 已记录 WP1 的目录级 Write Scope 排除该文件；`plan.md:911` 的 `fixtures/sync/v1/valid/` 行亦有前缀划分说明 |
| DDR-…-r1-F7 | MINOR | **已解决** | `plan.md:920`（`crates/core/src/**/tests.rs`）与 `plan.md:921`（`clients/app/**/*.test.ts`）两行已补齐，各带区域收窄说明与后合入方重跑项 |
| DDR-…-r2-F9 | MINOR | **已解决** | `plan.md:922` 已新增 `crates/storage-sqlite/tests/` 行（Writers = TP1, TP2，Merge Order TP1 → TP2，Re-verify TP2: PV2），并记录 TP1 的 glob 语义与区域收窄 |
| DDR-…-r2-F10 | MINOR | **已解决** | `plan.md:868` TP2 依赖已补 `contract:WP2`，与 `code:WP3` 分列；`plan.md:933` 的 Dependency Handoffs 行 Upstream 为 `WP2, WP3`，Invalidation 已补「WP2 的 `views.rs` 字段或枚举取值变更 → 形状基准失效」。核对成立：TP2 的 Write Scope（`crates/core/src/**/tests.rs`、`crates/storage-sqlite/tests/`）按 `check-crate-boundaries.mjs:106` 与 `docs/MODULE_ARCHITECTURE.md:481/483` 均无法 link `sync-protocol`，故 `views.rs@WP2` 只能是形状基准、不能是 `code:` |
| DDR-…-r2-F11 | MINOR | **已解决** | `verification.md` `## Dependency Declaration Review` 的 Round 1/2 行 Report Path 均为不带反引号的纯路径（`reports/ddr-sync-scope-and-pwa-client-r1.md`、`…-r2.md`），`looksLikePath`/`existsAt` 可解析。注：Round 3 行当前为 `PENDING`，属派发前状态，由主 Agent 在登记本报告时填写 |

### 已核对且通过的判据

- **(a) `code:` 依赖全部属实**：WP6 `code:WP5`（Sync Client 消费 WP5 的 `src/protocol/`/`src/platform/`）、WP7 `code:WP6` 与 `code:WP5`（页面/feature 消费状态机与协议层）、TP2 `code:WP3`（针对 WP3 改后的 `crates/core/src/ports.rs` 与提交面按公开端口编写）、TP3 `code:WP5`/`code:WP6`/`code:WP7`（可运行 `clients/app` 测试必然 import 上游模块）、TP4 `code:WP7`（浏览器用例针对 WP7 的路由与选择器）。反向侧亦成立：`crates/core` 与 `crates/agent-host` 的依赖闭包均不含 `sync-protocol`，且被 `CORE_FORBIDDEN` 与 `MODULE_ARCHITECTURE.md` §5 双重禁止，故 WP3/WP4/WP5/TP1 的纯 `contract:` 声明不是虚假降级。
- **(b) Contract Freeze 引用 ⊆ Dependencies 列**：11 个包逐项核对通过（WP3 的 `@WP2`/`@WP1`、WP4 的 `@WP2`、WP5 的 `@WP1`×2、WP6 的 `@WP1`、WP7 的 `@WP2`、TP1 的 `@WP1`、TP2 的 `@WP3`/`@WP2`、TP3 的 `@WP1`×2/`@WP5`×2/`@WP6`×2/`@WP7`×2 均有对应依赖项；WP1/WP2/TP4 的 Contract Freeze 为 `NOT_APPLICABLE` 且无依赖）。
- **(c) 同批重叠均已登记且非「同一处」**：W1 的 WP1×WP2 三处重叠（schemas 目录、fixtures 目录、`docs/SYNC_PROTOCOL.md`）各有登记行与区域说明；W2 的 WP3×WP4（`crates/core/src/ports.rs`）、WP3×TP2（`crates/core/src/**/tests.rs`）各有登记行且 Region Note 明确两侧改的不是同一处。13 行登记的 Merge Owner / Merge Order / Re-verify 三列齐备，Merge Order 覆盖全部写入者，Re-verify 引用的 `PV1`/`PV2`/`PV3` 均在 Coverage Index 的 `checks` 中。
- **(d) 波次与依赖一致，理由码合法**：按 `workflow-check.mjs:209-221` 重算，11 个包的声明波次与 `earliest` **逐行相等**（W1/W2/W3/W4/W5 对应 earliest 1/2/3/4/5），既无「早于最早可开工批次」，也无「晚于最早批次却无 Serialization Reason」。全部 11 行 Serialization Reason 为 `NOT_APPLICABLE`（合法空值，`NONE_LABEL` 覆盖）；唯一带理由码的 `resource-exclusive`（浏览器实例）在 `plan.md:944` 的 Runtime Resources 行，属合法码且其前置条件由 `verification.md` `## Runtime Resources` 的对应行支撑。池容量：W2 的 3 个 coder 恰等于 `dispatch.pool.coding: 3`，W5 的 2 个 tester 恰等于 `pool.testing: 2`，无超容。
- **(e) 无环、无整批依赖、Reviewer 独立**：依赖图为 DAG（WP1/WP2 无入边 → WP3/WP4/WP5/TP1 → WP6/TP2 → WP7 → TP3/TP4）；Dependency Handoffs 的 9 行 Upstream 逐条指向具体 WP；Merge Strategy 的 MU 就绪条件亦按包逐条列出（MU3 明确「WP6 需 WP5；WP7 需 WP5+WP6」），无 MU 对 MU 的整批依赖。11 个包的 Owner（`coding-1..7`/`testing-1..4`）与 Reviewer（`review-1..7`）无一相同，`tasks.md` §3 为每个包各有一条独立 review 任务且行内无 `[wp:…]` 派发标签，无自审。
- **(h) AC1 可满足性与收窄一致性**：`crates/server/src/node_link/resource.rs:152-155`（`fan_out` 先 `let Some(session) = … else { return }`）与 `:1027-1032`（`publish` 对 `event.session.is_none()` 直接 return，注释写明「非会话级事件没有会话级 origin cursor」）共同保证「会话级投递路径不误收」可观察；`crates/storage-sqlite/src/migrate.rs:85-113` 的 `session_id TEXT REFERENCES …` 可空且 `CHECK (session_id IS NOT NULL OR session_sequence IS NULL)` 等三条成对 CHECK 自洽；`crates/core/src/broker.rs:7462-7499` 证明非会话级事件可提交并进入 `replay()` 流，故「可按事件库读回」可断言。需求侧四处口径一致：`specs/core-derived-events/spec.md` 的覆盖范围声明段与「节点级事件不被会话级投递路径误收」场景、`specs/pwa-web-client/spec.md:192`（覆盖层缺席含「服务端尚不具备投递通道」）、`design.md` D6 与 Risks、`plan.md:981/1014`、`tasks.md:56/65/67`。Coverage Index 的 R082（状态未知不等于已断开）与 R030–R035（节点级生命周期及新增场景）分别映射到 `2.5/2.6/2.7/4.10/4.11` 与 `2.2/2.3/2.4/4.4/4.8/4.1`，覆盖索引与收窄后的需求集合对齐。
- **Main E2E `not-applicable` 判据**：`crates/server/src/lib.rs:28-31` 的模块声明只有 `local_admin`/`node_link`/`transport`，确认 `server::sync` 未落地；`openspec/agentic.yaml:14-15` 的 `e2e.enabled: true` 与 `e2e.command: ""` 成立；`verification.md` `## Main E2E` 记录了降级批准原话与来源。AC2/AC3 的入口（`npm run verify`、`clients/app` 契约测试）不依赖尚不存在的 Sync 入口，可满足。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F12 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:906-907`（`## Shared File Ownership` 表；缺 `clients/app/e2e/` 行，且 `clients/app/**/*.test.ts` 行的 Writers 缺 WP5） | 按字面 Write Scope 展开，WP5 的 `clients/app/`（目录级）覆盖 TP3 的 `clients/app/**/*.test.ts` 与 TP4 的 `clients/app/e2e/`，构成两组重叠。现有 13 行登记中，`clients/app/**/*.test.ts` 行（`plan.md:921`）的 Writers 为 `TP3, WP7`、Region Note 只说明 WP7 的目录级 scope 如何收窄，**未列 WP5**；`clients/app/e2e/`（TP4）**没有任何登记行**（全表未出现 TP4）。机械检查器不报错——`workflow-check.mjs:114` 的 `scopePaths` 不剥离反引号，WP5 的 `` `clients/app/` `` 与 TP3 的 `` `clients/app/**/*.test.ts` `` 字符串前缀比较不成立（node 复现：去引号后 9 组字面重叠，去引号前仅 1 组） | 两组重叠的「谁先写、谁合并、后合入方重跑什么」无据可依。实际冲突风险低（WP5 在 W2、TP3/TP4 在 W5，且 TP3 声明 `code:WP5`、TP4 经 `code:WP7` 传递到 WP5，开工顺序由依赖保证），故按 MINOR 记；但登记表按本计划自身标准应覆盖全部重叠写范围——同一目录已在 `plan.md:913/920/921/922` 四处为 WP1/WP3/WP7/TP1 做了区域收窄登记，唯独 WP5 的目录级 scope 没有对应记录，标准适用不一致 | 在 Shared File Ownership 增补两行或合并为一行：`clients/app/**/*.test.ts` 与 `clients/app/e2e/` 的 Writers 补入 WP5，Merge Owner `merger`，Merge Order `WP5 → WP7 → TP3`（TP4 同理），Re-verify 沿用 `TP3: PV3`/`TP4: PV3`，Region Note 写明「WP5 的 Write Scope 是 `clients/app/` 目录级，按区域收窄：WP5 只创建工程骨架与配置，不写测试文件与 `e2e/`；测试与浏览器用例分别由 TP3/TP4 独占」 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F13 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:866-869`（TP2/TP3 的 Contract Freeze 单元格；同样形态见 WP3/WP5/WP6/WP7/TP1 共 8 个包） | 按 `workflow-check.mjs:91` 的 `frozenContractReadable` 口径求值：多路径单元格（含空格与逗号）不匹配 `^(\S+?)@(\S+)$`；单路径单元格虽匹配但 `path.resolve` 保留反引号导致 `fs.access` 落空。node 复现结果：**11 个包的 Contract Freeze 全部 readable=false**，而这些路径在仓库中**确实存在**（`schemas/sync/v1/event-views.schema.json`、`crates/sync-protocol/src/views.rs`、`crates/core/src/ports.rs`、`fixtures/sync/v1/manifest.json` 等均可逐一定位）。后果是 `hardDeps`（`workflow-check.mjs:203-206`）把所有 `contract:` 依赖当作未冻结的硬排序约束，11 个包的 `earliest` 恰好等于其声明波次，门禁因此通过 | 声明的波次本身正确（见「已核对且通过的判据 (d)」），故非阻断。但两点后果需登记：(1) Contract Freeze 列宣称的「路径可核对」在机械门禁上**完全失效**——没有任何一个包的冻结版本被真正读取或记录，后续 `workflow impact`、premerge receipt 的 `planning_digest` 绑定拿不到真实冻结内容（`planning-model.mjs:70-76` 会把 `frozenContract.content` 记为 `UNREADABLE`）；(2) 波次合法性**依赖这一失效**：若按 R2-F11 已确立的反引号约定把这些单元格规范化，contract 依赖转为软依赖，重算得 WP3/WP4/WP5/TP1 earliest=1（声明 W2）、WP6/TP2 earliest=2（声明 W3），`workflow-check.mjs:325` 会报「晚于最早可开工批次但没有 Serialization Reason」共 6 条，门禁由 PASS 转 FAIL | 二选一并保持自洽：(a) 保留反引号与多路径写法，则在 `## Execution Waves` 的说明段显式记录「Contract Freeze 以人工核对为准，机械门禁按未冻结处理，故各包的波次下界等于其全部 code+contract 上游波次 +1」；或 (b) 规范化单元格（去反引号；多路径拆成 `` `path`@`WPn` `` 形态），并为 W2 的四个包与 W3 的两个包各补一条合法理由码——依据 `schema.yaml:196-199`，`capacity` 需池上限 <2 或同最早层级同角色包数超池上限，而 W2 的 3 个 coder 恰等于 `pool.coding: 3`、不满足该前置条件，故应改用 `contract-unfrozen` 或直接采用 (a)。无论选哪条，都应在 `verification.md` `## Check Plan Changes` 登记，避免下一轮再次因摘要变化而失效 | 不适用 |

### 说明（不计为问题）

- `verification.md` 的 `## Check Plan Changes` 开头仍写「Round 1 独立依赖声明审查（）判 FAIL，报 5 项 MAJOR + 2 项 MINOR」（括号为空），而表内已混入 Round 2 的 F7b/F10/F11 与 F4 的改写行，未记录 Round 2 本身也判 FAIL 且含 1 项 MAJOR（F8）。实质修正内容齐备且与 `plan.md` 当前字节逐条一致，属登记表述陈旧而非规划语义缺陷，按前两轮口径不计入 Findings；建议主 Agent 在登记本报告时把该段改为覆盖 Round 1–3 的累计修正表。
- `verification.md` `## Dependency Declaration Review` 的 Round 1 行 Plan Revision 为 `plan-v2:…771a55f2`、Round 2 行为 `plan-v2:…2fb86ed0`，均非当前 `b484b561…`；Round 3 行为 `PENDING`。`workflow check --stage plan` 的三条 error 全部指向这一行（Plan Revision 未绑定、Result 必须 PASS、必须有报告路径），是「Round 3 尚未登记」的预期状态，按 `procedures/scheduling.md`「机械格式检查失败先修记录」由主 Agent 在导入本报告时刷新，不计为规划缺陷。
- `reports/.f4.tmp`（3653 字节，mtime 14:21:44）是上一轮检视过程留下的临时文件，内容为 R1-F4 的片段。它既非 `## Handoff Receipts` 登记的报告路径，也未被任何 `evidence_paths` 引用；不影响门禁（该目录不参与 planningDigest），建议主 Agent 在归档前删除，以免与 r1/r2 正式报告混淆。

## Assessment

### 本轮结论

PASS。Round 1 的 5 项 MAJOR（F1、F2、F3、F4、F5）与 2 项 MINOR（F6、F7）、Round 2 的 1 项 MAJOR（F8）与 3 项 MINOR（F9、F10、F11）在本轮版本中**全部确认解决**，逐条复核依据见上表。Round 2 报告末尾提示的连带影响（「TP3 改为 `code:WP5/WP6/WP7` 后波次 W4→W5，需重跑 `workflow check` 确认 `ownershipProblem` 的 Re-verify 引用仍命中 Check ID」）也已核实：`plan.md:921` 的 Re-verify 为 `TP3: PV3`，`PV3` 在 Coverage Index 的 `checks` 中存在，重跑无相关 error。

本轮新发现 2 项 MINOR（F12、F13），均不构成 MAJOR：

- **F12**（写入归属登记缺两组重叠）是登记完整性问题，不产生同批冲突——WP5 在 W2、TP3/TP4 在 W5，且 `code:` 依赖已保证先后。这与 Round 1 F7 / Round 2 F9 的同类项（跨波次、依赖已排序）同级，上限为 MINOR。
- **F13**（Contract Freeze 机械不可核对）是**格式脆弱性**而非声明不实：11 条冻结路径在仓库中全部真实存在且逐条可人工核对，无一条路径写错或指向不存在的文件；当前波次与依赖在门禁口径下逐行自洽。判为 MINOR 而非 MAJOR 的依据是：角色约定的 MAJOR 三要件（「声明不实、登记缺失、同批却同一处」）均不成立——路径真实、登记在册（Dependencies 列交叉核对通过）、无同批重叠。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用，不依赖执行证据 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify`（含 `check:drift` 对 `docs/CORE_PORTS_AND_STORAGE.md §5` 与 `crates/core/src/ports.rs` 的逐字比对、`check:schemas`/`check:fixtures` 双向门禁）与 `clients/app` 契约测试（读仓库根 `fixtures/sync/v1/`、`fixtures/sync/v1/transcripts/device-proof.json` 字节级对拍）均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；**可满足性已核对** | 四项断言逐条对照 `crates/storage-sqlite/src/migrate.rs:85-113`、`crates/core/src/broker.rs:7462-7499`、`crates/server/src/node_link/resource.rs:152-155/1027-1032` 后确认可满足且不依赖广播 | 否（Round 1 F5 的不可满足性已消除） |
| Main E2E 判据 | 已核对 | `crates/server/src/lib.rs:28-31` 无 `sync` 模块；`openspec/agentic.yaml:14-15` 的 `e2e.command` 为空；降级批准原话与来源已记录 | 否 |
| Contract Freeze 路径可核对性 | **不通过（机械口径）** | 11 个包 readable 全部 false（F13）；路径本身真实存在，Dependencies 列交叉核对通过 | 是（已计入 F13，非阻断） |
| 写入重叠登记 | **部分不通过** | 13 行覆盖了全部同波重叠与 6/8 组跨波字面重叠；`clients/app/**/*.test.ts`（缺 WP5）与 `clients/app/e2e/`（无行）缺失（F12） | 是（已计入 F12，非阻断） |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md` DDR 表 Round 3 行为 `PENDING`、Round 1/2 的 Plan Revision 非当前摘要 | 否（属主 Agent 登记刷新），但会阻断机械门禁转 PASS |

### 不确定性与后续建议

- **AC1 中节点级事件的生产入口**：`crates/core/src/broker.rs:3143` 的 `commit_owned` 非 `pub`，公开面（`:606` 的 `sink(&SessionId)`、`:1733-1841` 的 `read_view/head/replay/read_session/flush/pump`）全部以 `SessionId` 或只读为入口；同时 `crates/app/tests/support/owner.rs:169/173` 用 `ScriptedBackends`/`ScriptedCatalog` 整体替换了 agent-host，因此该测试**不经由 profile 进程生命周期**产生 `agent.connected`。AC1 的可执行性依赖 WP3 在 `crates/core/src/`（其 Write Scope 内）暴露一个可直接调用的节点级提交入口。这落在 WP3 的实现设计内、不是计划缺陷，故不计为 Findings；但 AC1 派发前需确认该入口存在，否则该项会在执行期才暴露。建议 WP3 的完成条件显式写明「提供可从集成测试直接调用的节点级事件提交入口」。
- **波次与 Contract Freeze 的耦合**：F13 已说明当前波次合法性依赖冻结不可读这一事实。若后续按建议 (b) 规范化单元格，必须同步为 W2/W3 的六个包补合法理由码，否则门禁转 FAIL。
- **对主 Agent 的处置建议**：F12、F13 为非阻断项，建议在同一轮一并修正以免再次刷新 planningDigest；修正后须重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新 `planningDigest`。若该摘要因 F12/F13 的修正而变化，本轮 PASS 结论对新版失效，须按同一 Review ID 递增 Round 复核。本轮不宣称任何实现、E2E 或最终验收通过。

PASS
