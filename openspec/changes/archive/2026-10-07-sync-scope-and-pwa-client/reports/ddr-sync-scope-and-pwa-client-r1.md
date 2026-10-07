<!-- Dependency Declaration Review（plan 阶段）原始报告。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-1"
  isolation: "fork_turns=none（新建子 Agent，不继承 propose/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:78f86ac5b8fb0b682d00a6ad6c0e620b857c7386f3b314312c3d80ec92623d01"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）、批次与 Execution Waves 一致性、Write Scope 归属与 Shared File Ownership 登记、Reviewer 独立性与 tasks.md 独立检视任务、Independent Validation 与 [validation] 唯一性、Main E2E not-applicable 与 AC1/AC2/AC3 的连贯性，以及工作包所依赖的行为契约可满足性。"
changes: "只读检视；未修改任何规划文件、代码或任务状态。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json"
    scope: "机械门禁（仅取 planningDigest/contractDigest，不作为本轮结论）"
    result: "退出码 1；planningDigest=plan-v2:sha256:78f86ac5…23d01；contractDigest=sha256:605e82c5…d347"
    note: "本报告绑定当前 planningDigest。verification.md:35 登记的是 plan-v2:sha256:771a55f2…2cbc5，与当前值不同——属主 Agent 的登记格式修正项（scheduling.md「Plan Revision 直接填 CLI 返回的当前 planningDigest」），不构成规划语义缺陷，故不计入阻断问题。"
  - id: "依赖声明属实性（code）"
    scope: "WP3/WP4/WP5/WP6/WP7/TP2/TP4 的 code 依赖"
    evidence: "crates/core/Cargo.toml:18-28、crates/agent-host/Cargo.toml:11-25、scripts/check-crate-boundaries.mjs:92-113（core 禁用 sync-protocol）、docs/MODULE_ARCHITECTURE.md §5 依赖矩阵、crates/core/src/broker.rs:120-135、crates/core/src/model/session.rs:777-781"
  - id: "批次与波次一致性"
    scope: "plan.md:882-892 Execution Waves vs Work Packages 依赖列"
    evidence: "openspec/agentic.yaml dispatch.pool.coding=3 / testing=2"
  - id: "写入归属登记完整性"
    scope: "plan.md:852-862 Write Scope vs plan.md:896-907 Shared File Ownership"
  - id: "Reviewer 独立性与独立检视任务"
    scope: "plan.md:852-862 Reviewer 列 vs tasks.md:31-51 §3"
  - id: "Independent Validation 与 [validation]"
    scope: "plan.md:972-976、tasks.md:107"
  - id: "Main E2E / 替代检查连贯性"
    scope: "plan.md:963-968、985-1001；crates/server/src/lib.rs:14,27-29、crates/server/src/node_link/resource.rs:152-155,1023-1032、crates/app/src/compose.rs:821-840"
issues: "5 × MAJOR，2 × MINOR（见 Findings 表）。存在未解决的 MAJOR → 本轮结论 FAIL。"
result: FAIL
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/plan.md"
  - "openspec/changes/sync-scope-and-pwa-client/tasks.md"
  - "openspec/changes/sync-scope-and-pwa-client/verification.md"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 1
    stage: plan
    target_revision: "plan-v2:sha256:78f86ac5b8fb0b682d00a6ad6c0e620b857c7386f3b314312c3d80ec92623d01"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / proposal.md / design.md / tasks.md / verification.md 与 5 份增量 spec 做只读检视；planningDigest 由 `workflow check --stage plan --json` 在本轮重新推导并绑定。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1 |
| Round | 1 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-1`（不拥有任何 WP/TP；owners 为 `coding-1`..`coding-7`、`testing-1`..`testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖与写入归属声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:78f86ac5b8fb0b682d00a6ad6c0e620b857c7386f3b314312c3d80ec92623d01` |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `AGENTS.md`（由主 Agent 与 apply 阶段承接读取）、`openspec/config.yaml`、`openspec/agentic.yaml`、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`procedures/scheduling.md`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` §5/§7 |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；Project Verify（PV1–PV3）与 IV1/AC1–AC3 均记为 apply 阶段待补（`verification.md` `## Checks` 六行全为 PENDING/NOT_APPLICABLE） |
| Check Plan | `plan.md:958-968`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）；`verification.md` `## Check Plan Changes` 记「规划阶段无需求或接口澄清」，与本报告的 MINOR 项一致（登记摘要需刷新） |
| 实际检查范围 | (a) 8 条 `code:` 与 7 条 `contract:` 声明的逐条属实性；(b) 依赖图无环、无整批依赖整批；(c) 11 个 WP/TP 的 Reviewer≠Owner 与 tasks.md 独立检视任务；(d) 11 行 Execution Waves 与依赖列一致性 + Serialization Reason 取值域；(e) `## Independent Validation` 与 `[validation]` 唯一性；(f) Write Scope 两两重叠与 Shared File Ownership 登记；(g) Main E2E `not-applicable` 与 AC1/AC2/AC3 的连贯性；(h) 工作包所依赖的行为契约可满足性 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未核对 `main` E2E 是否真不可行（仅核对判据所依赖的仓库事实：`server::sync` 未落地、`e2e.command` 为空）；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r1.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告） |

### 已核对且通过的判据

- **(b) 无环、无整批依赖整批**：依赖图为 WP1/WP2（无入边）→ WP3/WP4/WP5/TP1 → WP6/WP7/TP2/TP3 → TP4，逐条为具体 WP 引用；`plan.md:944-946` 的 MU2/MU3 就绪条件按 WP 逐个列出（"WP6 需 WP5…；WP7 需 WP5+WP6…"），未出现 MU 对 MU 的整批声明。
- **(c) Reviewer 独立性与检视任务**：`plan.md:852-862` 中 11 个包的 Owner（`coding-1..7` / `testing-1..4`）与 Reviewer（`review-1..7`）无一相同；`tasks.md:31-51`（§3）逐 WP/TP 各有一条独立 review 任务（3.2/3.4/3.6/3.8/3.10/3.12/3.14/3.15-3.16/3.17-3.18/3.19-3.20/3.21-3.22），无自审。
- **(d) 波次一致性与 Serialization Reason**：11 行 Wave 的 Enter Condition 与依赖列逐条吻合（W1 无前置；W2 的 WP3/WP4/WP5/TP1 依赖 W1；W3 的 WP6←WP5、TP2←WP3；W4 的 WP7←WP5/WP6、TP3←WP5/WP6；W5 的 TP4←WP7）。全部 Serialization Reason 为 `NOT_APPLICABLE`，唯一带理由的串行列在 Runtime Resources 且用的是合法码 `resource-exclusive`（`plan.md:929`）；`plan.md:894` 明确写明写入范围重叠不作为串行理由。池容量核对：W2 的 3 个 coder 恰等于 `openspec/agentic.yaml` 的 `dispatch.pool.coding: 3`，未超容。
- **(e) Independent Validation 与 `[validation]`**：`plan.md:972-976` 存在 IV1 行（目标版本、覆盖点、通过条件、报告路径齐备）；`tasks.md` 中 `[validation]` 恰好出现一次（第 107 行 7.1）。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F1 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:867-868`（依赖列见同文件 854） | 声明依据为「WP3 在 `broker::required_grant` 与命令处理面引用它，缺上游无法编译与运行」与「WP3 构造该视图，缺上游字段定义无法编译」。两处均与仓库事实冲突：(1) WP1 的 Write Scope（`plan.md:852`）只有 `schemas/sync/v1/`、`fixtures/sync/v1/` 与两份 docs，**不产出任何 Rust 代码**，因此不存在 WP3「引用 WP1 代码」的对象；(2) `crates/core/Cargo.toml:18-28` 的依赖只有 `async-trait`/`thiserror`/`p256`/`sha2`，`scripts/check-crate-boundaries.mjs:92-113` 把 `sync-protocol` 列入 `CORE_FORBIDDEN`，`docs/MODULE_ARCHITECTURE.md` §5 矩阵 `core` 行整行为空——core **不可能**编译引用 WP2 的 `crates/sync-protocol/src/views.rs`；(3) 被点名的 `crates/core/src/broker.rs:120-135` `required_grant` 只对命令**名字符串**匹配，不触碰任何 DTO；core 侧 `session.read` 的载荷类型是 core 自有的 `crates/core/src/model/session.rs:777-781` `CommandPayload::SessionRead`。 | `code:WP1`/`code:WP2` 是虚假依赖声明。调度器会按「上游代码已合入」解锁 WP3，而真实门槛只是 schema 冻结（`contract:WP1`/`contract:WP2` 已足够）；同时计划把「core 引用 sync-protocol」写进依据，会误导 WP3 作者去写必然被 `check:boundaries` 拒绝的 import。 | 把 WP3 的两条 `code:` 删除，仅保留 `contract:WP1`/`contract:WP2`，并把 867-868 两条依据改写为「core 通过 `crates/core/src/model/session.rs` 的自有 `CommandPayload::SessionRead` 与 `ViewJson` 镜像冻结的 schema 形状，不需要跨 crate 引用」。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F2 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:869`（依赖列见同文件 855） | 依据为「`agent.connected` 的 `state` 枚举由 WP2 产出；WP4 上报时按该枚举取值」。`crates/agent-host/Cargo.toml:11-25` 的依赖为 `core`/`acp-protocol`/`tokio`/`async-trait`/`serde_json`/`thiserror`/`tracing`/`sha2`/`base64`，**无 `sync-protocol`**；`docs/MODULE_ARCHITECTURE.md` §5 矩阵 `agent-host` 行仅 `core`/`acp-protocol` 为 ✓，其余列（含 `sync-protocol`）为空。agent-host 经 `crates/agent-host/src/mapper.rs`/`session.rs` 的 `EndpointEvent`+`EventSink` 上报，`EventPayload.view` 是 `crates/core/src/model/json.rs:30-33` 的 `ViewJson`（JSON 文本），并非 `sync_protocol::views` 的类型化枚举。 | 同 F1：`code:WP2` 为虚假依赖；且该依据会让 WP4 作者试图 import `views::AgentConnected`，被 `npm run check:boundaries` 硬失败。 | 删除 WP4 的 `code:WP2`，仅保留 `contract:WP2`（`event-views.schema.json@WP2` 冻结的 `state` 取值集合），依据改为「agent-host 以 `EventType`+`ViewJson` 文本上报，`state` 取值按 WP2 冻结的封闭词表」。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F3 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:852`（Write Scope 列；契约变更见同文件 15-16） | WP1 收窄 `sync.schema.json#/$defs/snapshotResource` 并给 `command.schema.json#/$defs/sessionRead`/`sessionReadResult` 加 `before`/`limit`/「是否更早」布尔值（`plan.md:14-16`），TP1 还要交付「会话明细分页与复合游标向量」（`tasks.md:59`）。但**没有任何 WP 的 Write Scope 覆盖 `crates/sync-protocol/src/command.rs`**：WP1 只有 `schemas/sync/v1/`、`fixtures/sync/v1/` 与 docs（`plan.md:852`），WP2 只有 `crates/sync-protocol/src/views.rs`（`plan.md:853`），WP3 是 `crates/core/src/`+`crates/storage-sqlite/src/`。而该文件的类型化镜像 `SessionRead`（`crates/sync-protocol/src/command.rs:383-388`）带 `#[serde(deny_unknown_fields)]`，`SessionReadResult`（同文件 903-912）同样 `deny_unknown_fields`；`crates/sync-protocol/tests/envelope_fixtures.rs:141-155` 对每条 valid 用例做 `TypedBody::decode` 并在失败时 panic。 | 新的分页向量一旦按 WP1/TP1 交付，`SessionRead` 会因 `deny_unknown_fields` 拒绝含 `before` 的合法载荷，PV1（`npm run verify`，含 `cargo test --workspace`）必红；同时该 Rust 镜像是冻结契约的一部分却无归属人，属「写入范围无主」。 | 二选一：把 `crates/sync-protocol/src/command.rs` 加入 WP1 的 Write Scope 并在 tasks 2.1 补上「同步 `SessionRead`/`SessionReadResult` 的 payload 与结果形状」；或新建一个写入该文件的 WP 并登记进 WP 表、Wave 与 Shared File Ownership。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F4 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:861` | TP3 的 Dependencies 声明为 `contract:WP5, contract:WP6`，Contract Freeze 却是 `fixtures/sync/v1/transcripts/**@WP1`，Dependency Handoffs（`plan.md:919`）的 Upstream 也只写「WP5, WP6」。但 Coverage Index 把 TP3 与 WP7 绑在同一批场景上：`plan.md:605/613/621/629`（R17 上下文三态）tasks 为 `["2.7","4.10","4.11"]`、`plan.md:636`（R18 三类不支持可区分）tasks 含 `2.7`、`plan.md:648`（R19）tasks 含 `2.7`——`2.7` 即 WP7（`tasks.md:19`）。TP3 既未声明对 WP7 的依赖，又与 WP7 同处 W4（`plan.md:890-891`），且写入范围重叠未被登记：TP3 的 `clients/app/**/*.test.ts` 会命中 WP7 的 `clients/app/app/`、`clients/app/src/features/`、`clients/app/src/components/`（`plan.md:858`），而 Shared File Ownership（`plan.md:898-907`）只有 `clients/app/package.json` 与 `clients/app/src/protocol/` 两行。 | 依赖类型与真实需求相反：TP3 需要的是 WP5/WP6 的**代码**（`contract:` 不构成开工门槛），外加一个完全未声明的 WP7 与一个未登记的 `@WP1` 冻结路径。后果有三：TP3 可能在其被测对象落地前开工；W4 同层写入范围重叠无合并负责人；Contract Freeze 引用的 `@WP1` 不在依赖列，冻结版本不可核对。 | TP3 依赖改为 `code:WP5, code:WP6, code:WP7`（对齐 TP2 的 `code:WP3`、TP4 的 `code:WP7`），Contract Freeze 补上 WP7 侧路径与 `contract:WP1`（`fixtures/sync/v1/transcripts/**`），并在 Shared File Ownership 增加 `clients/app/**/*.test.ts` 行（Writers = WP7, TP3；Merge Owner = merger；顺序 WP7 → TP3；后合入方重跑 PV3）。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F5 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:966`（同义表述见 999、AC1 任务见 `tasks.md:55`） | AC1 用 `cargo test --locked -p app --test node_link_e2e` 断言「`file.changed` 与 `agent.connected` 经 owned 路径落库并**按 origin 顺序广播**」。但 `agent.connected`/`agent.disconnected` 按 D6 与 spec R097/R031/R032 是**节点级**事件（会话标识为空），而唯一的广播路径显式丢弃它们：`crates/server/src/node_link/resource.rs:1029-1032` 在 `event.session.is_none()` 时 `return`（注释即「非会话级事件没有会话级 origin cursor，也不属于任何 attachment」），`fan_out`（同文件 152-155）同样先 `let Some(session) = … else { return }`；`crates/app/src/compose.rs:821-840` 的 `ForkedPublisher` 只在 `LoggingPublisher`（trace 日志）与 `NodeLinkPublisher` 之间分叉，没有第三条订阅者；`crates/server/src/lib.rs:27-29` 确认 `server::sync` 不存在（亦为 `proposal.md` 的 non_goal）。存储侧落库可行（`crates/storage-sqlite/src/migrate.rs:85-113` 的 `session_id` 可空且 CHECK 自洽），但广播在 node-link 入口上不可能发生。 | AC1 作为 Main E2E `not-applicable` 的核心替代检查，在其声明的命令上**无法通过**（广播断言不可满足），因此 not-applicable 的「通过的替代验证」要件不成立，R8/R30 失去真实入口上的可观察性证据。同一缺口也使 `specs/core-derived-events/spec.md` R031/R032 场景中的「持久化并广播」在本变更范围内不可满足（WP3/WP4 只负责落库，无 WP 拥有 `crates/server/`）。 | 把 AC1 的 `agent.connected` 断言从「广播」降为「经 owned 路径落库且 `session_id` 为空、origin 三元组为 NULL」，并在 `plan.md` 的 Main E2E 说明中显式登记「节点级事件的客户端可见性由后续 `server::sync` 变更承担」；若坚持保留广播断言，则必须把 `crates/server/src/node_link/` 纳入某一 WP 的 Write Scope 并声明新的写入归属。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F6 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:903` | Shared File Ownership 声明 `schemas/sync/v1/event-views.schema.json` 为「WP2 单写者」，但 WP1 的 Write Scope 是整个 `schemas/sync/v1/`（`plan.md:852`），字面上包含该文件；`fixtures/sync/v1/` 同理（WP1 目录级 vs WP2 的 `fixtures/sync/v1/valid/view-*.json`）。登记行的 Region Note 只写「单写者」，未记录 WP1 被排除的区域。 | 计划自述的收窄（WP1 只写 `sync-*`/`command-*`）只出现在 `fixtures/sync/v1/valid/` 一行（`plan.md:901`），`schemas/` 行缺同样记录，两个 WP 同处 W1 时按字面 Write Scope 判重叠会得到与登记表相反的结论。 | 在 `plan.md:903` 的 Region Note 补「WP1 在 `schemas/sync/v1/` 只写 `sync.schema.json` 与 `command.schema.json`」，或把 WP1 的 Write Scope 从目录收窄到具体文件。 | 不适用 |
| DDR-sync-scope-and-pwa-client-r1-F7 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:860` | TP2 的 Write Scope 为 `crates/core/src/**/tests.rs` 与 `crates/storage-sqlite/tests/`，与 WP3 的目录级 `crates/core/src/`（`plan.md:854`）在 `crates/core/src/**/tests.rs` 上重叠；TP1 的 `crates/*/tests/`（`plan.md:859`）又与 WP2 的 `crates/sync-protocol/src/views.rs` 相邻（同一 crate 的 src/tests 对）。两处重叠均未出现在 Shared File Ownership（`plan.md:898-907`）。 | 二者分处 W2/W3 且 TP2 声明了 `code:WP3`，执行顺序由依赖保证，因此不构成同批冲突；但登记表按要求应覆盖全部重叠写范围，缺项会让后续 `workflow impact` 与合并登记缺少依据。 | 在 Shared File Ownership 增加 `crates/core/src/**/tests.rs`（Writers = WP3, TP2；Merge Owner = merger；顺序 WP3 → TP2；后合入方重跑 PV2）与 `crates/sync-protocol/tests/`（Writers = TP1, TP2）。 | 不适用 |

### 说明（不计为问题）

- `verification.md:35` 的 Plan Revision 登记为 `plan-v2:sha256:771a55f2…2cbc5`，与本轮重新推导出的 `plan-v2:sha256:78f86ac5…23d01` 不同。按 `procedures/scheduling.md`「机械格式检查失败先修记录……Plan Revision 直接填 CLI 返回的当前 planningDigest」，这是主 Agent 的登记刷新项，不是规划语义缺陷，故未计入 Findings；主 Agent 修正后应重跑 `workflow check --stage plan`。
- `verification.md` `## Handoff Index` 与 `## Dependency Declaration Review` 的 Executor / Result 仍为 `PENDING`：符合「规划审查尚未派发完成」的当前状态，待本报告登记后由 `workflow record` 写入。

## Assessment

### 本轮结论

FAIL。存在 5 项已确认且未解决的 MAJOR（DDR-sync-scope-and-pwa-client-r1-F1 ~ F5），按 `roles/reviewer.md` 的判定规则阻断交付。

问题集中在两处系统性根因：

1. **「code 依赖」被当成「接口冻结」的重复声明**（F1、F2）。计划把 `crates/core`、`crates/agent-host` 与 `crates/sync-protocol` 之间写成编译期依赖，但 `scripts/check-crate-boundaries.mjs:92-113` 的 `CORE_FORBIDDEN` 与 `docs/MODULE_ARCHITECTURE.md` §5 矩阵明确禁止这条边，`crates/core/Cargo.toml:18-28`、`crates/agent-host/Cargo.toml:11-25` 也确实没有它。真实依赖只有 `contract:`。这两条虚假声明不影响 Execution Waves 的**排序结果**（`contract:` 已把 WP3/WP4 放在 W2），但会让依据文本与调度语义同时失真。
2. **契约镜像与写入归属缺主**（F3、F4）。`crates/sync-protocol/src/command.rs` 是 `sessionRead`/`sessionReadResult` 的类型化镜像且带 `deny_unknown_fields`，却不在任何 WP 的 Write Scope 内；TP3 一侧则把需要「代码」的依赖写成 `contract:`，漏掉被测对象 WP7，并留下一个未登记的同波写入范围重叠。这两处都会在 apply 阶段变成硬失败（PV1 编译/解码 panic、W4 同层无合并负责人）。

F5 是独立的一类：Main E2E 判 `not-applicable` 的**理由**本身站得住（`crates/server/src/lib.rs:27-29` 无 `server::sync`，`openspec/agentic.yaml` 的 `e2e.command` 为空），但所选替代检查 AC1 在其声明的 `node_link_e2e` 入口上对 `agent.connected` 断言「按 origin 顺序广播」不可满足——`crates/server/src/node_link/resource.rs:1029-1032` 与 152-155 显式丢弃会话标识为空的事件，而 `crates/app/src/compose.rs:821-840` 证明没有第三条订阅者。同一缺口使 spec R031/R032 的「持久化并广播」在本变更内不可满足，属行为契约不可满足，按角色规则报 MAJOR。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 PENDING / NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用，不依赖执行证据 |
| IV1 | 待补（apply 阶段） | 同上，`## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC1 / AC2 / AC3 | 待补（apply 阶段） | 三行 Command 已声明；AC1 的可满足性问题见 F5 | AC1 的**可满足性**影响本轮判断（已计入 F5）；其执行结果本身待补 |
| Main E2E 判据 | 已核对 | `e2e.enabled: true`、`e2e.command: ""`；`server::sync` 未落地；`downgrade_approval` 已记录原话与来源 | 否（判据成立），但替代检查的覆盖强度见 F5 |
| Contract Freeze 路径可核对性 | 部分不通过 | WP3/WP4/WP5/WP6/WP7/TP2 的冻结路径可在仓库中定位；TP3 的 `@WP1` 不在其依赖列内 | 是（已计入 F4） |

### 对主 Agent 的处置建议

- F1/F2/F3/F4 属产品与计划缺陷，按 `roles/reviewer.md` 交实现 Agent 与主 Agent 协调责任人，在 plan/tasks 内集中一次修正，并同步 `## Check Plan Changes`。
- F5 的判定归主 Agent：要么收窄 AC1 断言（推荐），要么把 `crates/server/src/node_link/` 纳入写入范围并新增依赖声明；两者都会改变 planningDigest。
- F6/F7 为非阻断项，建议在同一轮一并修正以免再次刷新摘要。
- 修正后须重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新 `planningDigest`，再按同一 Review ID、递增 Round 派发独立复核；`verification.md:35` 的 Plan Revision 一并刷新为新摘要。本轮不宣称任何实现、E2E 或最终验收通过。

FAIL
