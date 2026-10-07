<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 6。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-6"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:c7a697c04b5fa591e43185f26c97dc2403d6760efcf2a9b2b10bdecbe3a895b4"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 独立检视任务、Coverage Index 与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order。本轮重点：MU1 拆为 MU1a/MU1b 后的四处协同改动（Merge Strategy 单元表与 Order、TP1 的 code: 依赖与 Contract Freeze、TP1 的波次与 Enter Condition、Shared File Ownership 的 envelope_fixtures.rs 行）是否彼此一致、是否与 MU2/MU3 的「MU1b 已合入」及全篇其他旧 MU1 表述自洽，以及 tasks.md 的交付单元任务是否随之同步。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。仅新增本报告文件。过程中临时创建的两个 node 复现脚本（.review-r6.mjs / .review-r6b.mjs）已删除，未留在工作区。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote），本轮执行 3 次"
    result: "退出码 1；planningDigest=plan-v2:sha256:c7a697c04b5fa591e43185f26c97dc2403d6760efcf2a9b2b10bdecbe3a895b4（三次一致）；contractDigest=sha256:5245480118ce479100785a2c64696f1a400a43d0ddc5c4f393b93917c18fcd79；requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（与 Round 4/5 相同）"
    note: "另以 planningModel() 直接复算得同一 planningDigest。plan.md mtime 2026-10-03T12:30:33Z（早于本轮开始），verification.md 12:49:16Z，tasks.md 10:49:21Z，design.md/proposal.md 07:15:15Z。本轮全部实质结论基于该组字节，报告落笔期间未再改写。"
  - id: "交付单元解析复现（MU1a/MU1b/MU2/MU3 与 Order 1/2/3/4）"
    scope: "plan.md:957-969 的 Merge Strategy 全节"
    evidence: "直接以 node 调用 @dongfanglin/openspec-agentic 的 deliveryUnits(plan)：返回 4 个单元，id 依次 MU1a/MU1b/MU2/MU3，mode 均 independent，members 依次 {WP1,WP2} / {TP1} / {WP3,WP4,TP2} / {WP5,WP6,WP7,TP3,TP4}，order 分别 '1'/'2'/'3'/'4'，orderGroup 1/2/3/4（numeric 口径，无抛错）；并集恰为 Work Packages 声明的 11 个 ID，无重复归属、无遗漏"
  - id: "code/contract 依赖逐条属实性"
    scope: "TP1 由 contract:WP1/contract:WP2 改为 code:WP1/code:WP2；WP6/TP2/TP3/TP4 的 8 条 code 边；WP3/WP4/WP5 的纯 contract 声明"
    evidence: "TP1 的 Write Scope 为 `crates/*/tests/`、`fixtures/sync/v1/`，交付物是 crates/sync-protocol/tests/ 下的 Rust 集成测试（reports/test-tp1-r1.md:87 记「基线 353ba6e 上 AgentConnectedState/SessionReadBefore/hasEarlier/收窄后的 SnapshotResource 全部不存在」；tests/schema_drift.rs 断言 SnapshotResource::ALL 与 $defs.snapshotResource.enum 逐条同序），必然 import 并链接 WP1/WP2 交付的 Rust 类型 → code: 属实。crates/core/Cargo.toml:18-28 仍只有 async-trait/thiserror/p256/sha2、crates/agent-host/Cargo.toml 无 sync-protocol，故 WP3/WP4 的纯 contract 降级仍成立"
  - id: "Contract Freeze 引用 ⊆ Dependencies（去反引号后逐条）"
    scope: "11 个包的 Contract Freeze 单元格"
    evidence: "按 workflow-check.mjs:91 的 `^(\\S+?)@(\\S+)$` 语义、剥离反引号后逐条比对：19 条 `@WPn` 引用全部落在对应包的 Dependencies 列内；唯一例外是 plan.md:867 TP1 的 `@MU1a`——MU1a 是交付单元 ID 而非工作包 ID，不在 TP1 的 Dependencies（code:WP1, code:WP2）内（见 F17）。11 个包的 frozenContractReadable 仍全为 false（F13 结转）"
  - id: "Execution Waves 与依赖列一致性（双口径重算 earliest + 无环）"
    scope: "plan.md:888-903 的 11 行 vs Work Packages 依赖列"
    evidence: "按 workflow-check.mjs:203-221 复现：TP1 改为 code:WP1/code:WP2 后 earliest=2，声明波次 W2，逐行相等；「全部 contract 视为硬依赖」口径下 11 行亦逐行相等（W1/W2/W3/W4/W5），「contract 视为软依赖」口径下各行均不早于最早批次；依赖图 DFS 结果 cycles=[]；上游同批/更早批检查 11 行均通过；11 行 Serialization Reason 全为 NOT_APPLICABLE，无 waveIndex>minimum 的行故无需理由码；池容量：W1=2 coder、W2=3 coder+1 tester、W5=2 tester，对 openspec/agentic.yaml:11-13（coding=3 / testing=2）均未超容"
  - id: "Shared File Ownership 覆盖与登记充分性（含本轮新增行）"
    scope: "plan.md:906-926 共 17 行 vs 按字面展开的写入重叠"
    evidence: "工具口径（workflow-check.mjs:388-395）下唯一被命中的重叠对仍是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`（plan.md:912 覆盖，ownershipProblem() 返回空串）。本轮新增的 plan.md:923 行（Writers=WP1, TP1；Merge Owner=merger；Merge Order=WP1 → TP1；Re-verify=TP1: PV1）四列齐备，ownershipProblem() 逐项求值返回空串（mergeTokens 覆盖全部 writers；reverify token `PV1` 在 agentic-coverage 的 checks 集合 {PV1,PV2,PV3,AC1,AC2,AC3} 内）。该行不会被机械重叠循环命中（WP1 的 Write Scope 不含 `crates/*/tests/`，scopeOverlap 不做 glob 展开），属人工判断登记，与 Round 3/4 对 F7b/F9/F12 的同一口径"
  - id: "Dependency Handoffs 完整性"
    scope: "plan.md:928-939 共 8 行"
    evidence: "TP3 行（plan.md:938）的 Accepted Revision 已同步为「MU1a 合入后的冻结契约」，与 TP3 的 `contract:WP1` 边一致；8 条 handoff 中 7 条的 Upstream 集合等于该包依赖目标集合，WP7 行仍为 `WP5, WP6, WP2` 而 Dependencies 含 `contract:WP1`（F14 结转未解决）；TP1 无 handoff 行——本轮 TP1 由同单元变为跨单元（MU1b ← MU1a）后，该判断的前提已变（见 F18）"
  - id: "MU2/MU3 的「MU1b 已合入」与新单元名一致性"
    scope: "plan.md:965-966 的 Start / Readiness 列"
    evidence: "MU2 行读「MU1b 已合入主分支且契约冻结可读」、MU3 行读「MU1b 已合入（契约冻结）」，与 Merge Strategy 表中新命名的 MU1b 一致；且与 scheduling-status.mjs:150-152 的 orderGroup 前序阻塞（MU2 orderGroup=3 阻塞于 MU1a/MU1b，MU3 orderGroup=4 阻塞于 MU1a/MU1b/MU2）方向一致，无反向依赖"
  - id: "全篇旧 MU1 表述扫描"
    scope: "plan.md / tasks.md / verification.md 中所有 MU1 字样"
    evidence: "正则 `MU1(?!a|b)` 扫描：plan.md 命中 13 行（29-32 Coverage Index 的 Closure Unit 列、959 拆分说明句、996 IV1、1000 Code Review 分层、1030 下游失效范围、1032 回滚条件、1033/1041 交付状态、1037/1038 Completion Criteria）；tasks.md §5/§6 共 16 行仍写 MU1（tasks.md:77/83-90）；verification.md 的叙述性段落亦用 MU1"
  - id: "Reviewer 独立性与独立检视任务"
    scope: "11 个包的 Owner/Reviewer/Role 列 + tasks.md §3/§4 的检视任务"
    evidence: "11 个包 Owner 与 Reviewer 无一相同（TP1=testing-1/review-1、TP2=testing-2/review-3、TP3=testing-3/review-5、TP4=testing-4/review-7）；按 workflow-check.mjs:161-167 的口径复算，每个包至少一条引用它且不含 [wp:…] 标签、含检视标记的任务，11/11 通过；每个包恰好一条 [wp:WPn] 派发任务"
  - id: "Coverage Index 与 5 份 delta spec 的对应"
    scope: "`agentic-coverage` 块的 102 行"
    evidence: "block() 解析出 102 行、ID 无重复、target_ref=refs/heads/main；102 行的 tasks 全部存在于 tasks.md；checks 仅取 {PV1,PV2,PV3,AC1,AC2,AC3}"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源；临时复现脚本已删除）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-6"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 6
    stage: plan
    target_revision: "plan-v2:sha256:c7a697c04b5fa591e43185f26c97dc2403d6760efcf2a9b2b10bdecbe3a895b4"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r6.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份增量 spec 做只读复核；逐项核实 MU1→MU1a/MU1b 拆分后的 Merge Strategy 单元表与 Order 1/2/3/4、TP1 的 code: 依赖与 Contract Freeze、TP1 的波次与 Enter Condition、Shared File Ownership 的 envelope_fixtures.rs 行四处协同改动，并扫描全篇旧 MU1 表述；对照 crates/core/Cargo.toml 复核 WP3/WP4 的纯 contract 降级；Merge Strategy 以 node 直接调用 deliveryUnits() 复现；写入重叠、波次 earliest、Contract Freeze 引用、所有权四列完整性按 workflow-check.mjs 的口径自写脚本重算。planningDigest 由 workflow check --stage plan --json 在本轮 3 次重新推导，三次一致（c7a697c0…b4），并经 planningModel() 独立复算确认。判定 FAIL 的唯一依据是 F17（MAJOR）：TP1 的 Contract Freeze 引用 @MU1a，该 token 既不在其 Dependencies 列内、也不是工作包 ID，违反本变更自 Round 1 起写入 verification.md 的既定规则「freeze 引用必须在依赖列内」；另有 4 项 MINOR（F18–F21）与 2 项结转 MINOR（F13/F14）。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–5 的 Review ID） |
| Round | 6 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-6`（不拥有任何 WP/TP；owners 为 `coding-1..coding-7`、`testing-1..testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖、写入归属与交付单元成员声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:c7a697c04b5fa591e43185f26c97dc2403d6760efcf2a9b2b10bdecbe3a895b4` |
| 版本稳定性 | 本轮未发生目标版本变动。`workflow check --stage plan --json` 本轮执行 3 次，`planningDigest` 恒为 `c7a697c0…b4`，并经 `planningModel()` 独立复算一致。`plan.md` mtime 2026-10-03T12:30:33Z（早于本轮开始），`verification.md` 12:49:16Z、`tasks.md` 10:49:21Z，本轮期间均未再改写 |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`schemas/agentic/schema.yaml` 的依赖类型与 Execution Waves 判据、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:978-990`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1002-1021`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 Round 1 的 F1–F11 修正 |
| Previous Findings | F1–F11（Round 1 MAJOR×5 + MINOR×2；Round 2 MAJOR×1 + MINOR×3）、F12/F13（Round 3 MINOR×2）、F14（Round 4 MINOR×1）、F15/F16（Round 5 MINOR×2），逐项复核见下 |
| 实际检查范围 | (a) 四处协同改动（Merge Strategy 单元表与 Order、TP1 的 `code:` 依赖与 Contract Freeze、TP1 的波次与 Enter Condition、Shared File Ownership 的 `envelope_fixtures.rs` 行）彼此及与全篇的相容性；(b) TP1 `code:` 依赖的属实性（对照 Rust 类型化镜像与 schema_drift 门禁）；(c) 全篇旧 `MU1` 表述扫描（plan.md/tasks.md/verification.md）；(d) MU2/MU3 的「MU1b 已合入」与新单元名、与 scheduling-status orderGroup 阻塞方向的一致性；(e) 依赖图无环、波次双口径 earliest、池容量、Serialization Reason 取值域；(f) Contract Freeze 引用 ⊆ Dependencies 与机械可读性；(g) Shared File Ownership 覆盖与本轮新增行的四列完整性；(h) Dependency Handoffs 8 条行与各自 Dependencies 列的逐行比对；(i) Reviewer 独立性与 tasks.md 的独立检视任务覆盖；(j) Coverage Index 102 行与 tasks/checks 引用的有效性；(k) 交付单元解析复现与 Order 组序 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 MU1b 的候选构建与 receipt 能否实际产出（属 apply 阶段 merger 职责）；变更目录 `git status` 为 `?? openspec/changes/sync-scope-and-pwa-client/`（未跟踪），**因此无法用版本控制逐行 diff 出 Round 5 之后的改动**，本轮改以「按当前字节重新验证 Round 5 的每一条结论 + 逐项核实本轮声明的四处改动」的方式确认（见下节） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r6.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 6） |

### 本轮声明的四处改动逐项核实

| 改动 | 位置 | 结论 | 依据 |
| --- | --- | --- | --- |
| Merge Strategy 拆为 MU1a/MU1b/MU2/MU3、Order 1/2/3/4 | `plan.md:959-966` | **正确** | `deliveryUnits()` 实跑返回 4 单元、mode 均 `independent`、无抛错；成员并集恰为 11 个已声明 ID，无重复、无遗漏；Order 全为数字故 `orderGroup` 取 1/2/3/4，与 `scheduling-status.mjs:150-152` 的前序阻塞方向一致（MU2 阻塞于 MU1a/MU1b，MU3 阻塞于 MU1a/MU1b/MU2），不存在「后序单元先合入」的反向依赖 |
| TP1 依赖改为 `code:WP1, code:WP2`，Contract Freeze 指向 `{sync,views,command}.rs@MU1a` | `plan.md:867` | **依赖类型正确，Contract Freeze 不合格** | `code:` 属实：TP1 交付 `crates/sync-protocol/tests/` 下的 Rust 集成测试，必然 import 并链接 WP1 的 `sync.rs`/`command.rs` 与 WP2 的 `views.rs`；`reports/test-tp1-r1.md:87` 独立佐证「基线上 `AgentConnectedState`/`SessionReadBefore`/`hasEarlier`/收窄后的 `SnapshotResource` 全部不存在」。但 `@MU1a` 是交付单元 ID，既不在 TP1 的 Dependencies 列内、也不是工作包 ID，违反本变更自 Round 1 起即写入 `verification.md:63/65` 的既定规则「freeze 引用必须在依赖列内」→ **F17（MAJOR）** |
| TP1 保持 W2、Enter Condition 增补等待、Serialization Reason 维持 `NOT_APPLICABLE` | `plan.md:897` | **正确且自洽** | 双口径重算下 TP1 的 earliest=2（`code:WP1`/`code:WP2` 均在 W1），声明波次 W2 逐行相等，故 `waveIndex(2) > minimum(2)` 为假、无需理由码，`NOT_APPLICABLE` 通过 `workflow-check.mjs:338-347` 的 `isNone` 判定；「等待是 Merge Strategy 的 Order 约束而非 code/contract/resource 依赖」的理由与 `EXECUTION_REASONS`（`workflow-check.mjs:42`，仅四个码）一致；上游 WP1/WP2 在 W1，早于 TP1 的 W2 |
| Shared File Ownership 新增 `envelope_fixtures.rs` 行（WP1, TP1；merger；WP1 → TP1；TP1: PV1） | `plan.md:923` | **正确，Round 5 的 F15 关闭** | 四列齐备，`ownershipProblem()` 逐项求值返回空串（mergeTokens 覆盖全部 writers；reverify token `PV1` 在 coverage 的 checks 集合内）；Region Note 的「MU1a 由 WP1 定中间值、MU1b 由 TP1 在 MU1a 合入提交上定最终值」与 `plan.md:964` 的 MU1b Start/Readiness 一致；且与仓库事实相符——`git -C .worktrees/wp1 status --porcelain` 显示该文件确被 WP1 修改（2 行计数常量），`git diff --stat` 为 4 行变更 |

### Round 5 结论在当前字节的保持情况

变更目录未被 Git 跟踪，无法逐行 diff 改动，故按当前字节逐条重新验证 Round 5 的结论：

| Round 5 结论 | 本轮结论 | 复核依据（当前文本 + 仓库/工具事实） |
| --- | --- | --- |
| F1（WP3 的 `code:WP1/WP2` 虚假）已解决 | 维持 | `plan.md:862` 仍为 `contract:WP1, contract:WP2`；`crates/core/Cargo.toml:18-28` 依赖闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`，无 `sync-protocol` |
| F2（WP4 的 `code:WP2` 虚假）已解决 | 维持 | `plan.md:863` 仍为 `contract:WP2`；`crates/agent-host/Cargo.toml` 仍无 `sync-protocol` |
| F3（`command.rs` 无归属）已解决 | 维持 | `plan.md:860` 的 WP1 Write Scope 含 `crates/sync-protocol/src/command.rs`，`plan.md:918` 的登记行仍在 |
| F4/F8（TP3 依赖类型与波次）已解决 | 维持 | `plan.md:869` 仍为 `code:WP5, code:WP6, code:WP7, contract:WP1`，波次 W5；冻结引用 ⊆ 依赖列（去反引号后 7 条 `@WPn` 全部命中） |
| F5（AC1 不可满足）已解决 | 维持 | `plan.md:984/1017`、`tasks.md:4.1`、`verification.md` AC1 行仍为「落库 + 按事件库读回 + 会话标识为空 + 会话级投递路径不误收」并显式不验证广播 |
| F6/F7/F9/F10/F11 已解决 | 维持 | `plan.md:913`（event-views 区域收窄）、`:921`（`crates/core/src/**/tests.rs`）、`:922`（`clients/app/**/*.test.ts`）、`:924`（`crates/storage-sqlite/tests/`）四行仍在；`plan.md:868` 的 TP2 仍含 `contract:WP2`；`verification.md:38` 起的 DDR 表 Report Path 仍不带反引号 |
| F12（Shared File Ownership 两行缺失）已解决 | 维持 | `plan.md:925`/`926` 两行四列齐备；`ownershipProblem()` 返回空串 |
| Merge Strategy 分隔符为 `/`、无抛错 | 维持并扩展 | 本轮四单元全部解析成功（见上节） |
| Reviewer 独立性与独立检视任务 | 维持 | 11 个包 Owner≠Reviewer；每个包至少一条独立 review 任务 |
| F13（Contract Freeze 机械不可核对） | **维持未解决（非阻断）** | 11 个包的 `frozenContractReadable` 仍全为 false（多路径单元格不匹配 `^(\S+?)@(\S+)$`） |
| F14（WP7 的 handoff 漏 `WP1`） | **维持未解决（非阻断）** | `plan.md:936` 的 WP7 行 Upstream 仍为 `WP5, WP6, WP2`，而 `plan.md:866` 的 Dependencies 仍含 `contract:WP1` |
| F15（`envelope_fixtures.rs` 未登记 WP1） | **本轮已解决** | `plan.md:923` 新增该行，四列齐备且求值为空串 |
| F16（tasks.md 未提 `sync.rs`） | **本轮已解决** | `tasks.md:11` 的冻结清单已含 `crates/sync-protocol/src/{views,command,sync}.rs`；`tasks.md:17` 已写明 `command.rs` 与 `sync.rs` 均为 WP1 写入范围并说明其类型化镜像关系 |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F17 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:867`（TP1 的 Contract Freeze 单元格；关联同文件 `:961-966` 的 Merge Strategy、`:963` 的 MU1a 行） | TP1 的 Contract Freeze 写为 `` `schemas/sync/v1/**@WP1`, `crates/sync-protocol/src/{sync,views,command}.rs@MU1a` ``。**`@MU1a` 是交付单元 ID，不是工作包 ID，且不在 TP1 的 Dependencies（`code:WP1, code:WP2`）列内。** 本变更自 Round 1 起就把「freeze 引用必须在依赖列内」写成自身规则并据以判 MAJOR：`verification.md:63` 的 F4 行（「freeze 引用必须在依赖列内」作为 TP3 修正理由）与 `verification.md:65` 的 F10 行（同规则，修正方式为给 TP2 补 `contract:WP2`）。逐条复算 11 个包共 19 条 `@` 引用，其余 18 条（WP1–WP3、WP4、WP5×2、WP6、WP7、TP1 的 `@WP1`、TP2×2、TP3×7）全部落在各自 Dependencies 列内，仅本条例外 | 该单元声明的是 TP1 编译/运行所依据的**被冻结版本**，而版本 token 指向一个交付单元而非任何被声明的上游工作包：执行者据此无法定位「冻结的是 WP1 的哪次交付还是 WP2 的哪次交付」，`MU1a` 一旦重命名或再次拆分，冻结引用即失效且无告警。更实质的是它与 `plan.md:964` 的 MU1b Start/Readiness 重复表达了同一件事（TP1 从 MU1a 的合入提交分支），但该列不承载契约冻结语义——按 reviewer 角色「contract 是否真的已冻结且路径与版本可核对」的判据，此处版本不可核对。按规则「声明不实、登记缺失……按 MAJOR 阻断」，计 MAJOR | 把该 token 改为工作包 ID 并与 TP1 的依赖对齐，例如 `crates/sync-protocol/src/sync.rs@WP1`、`crates/sync-protocol/src/views.rs@WP2`、`crates/sync-protocol/src/command.rs@WP1`（与 `plan.md:860`/`:861` 的 Write Scope 归属一致）；若确实要表达「以 MU1a 的合入提交为冻结点」，应写入 Dependency Handoffs 的 Accepted Revision 列（该表已有 TP1 缺席，见 F18），而不是 Contract Freeze。**注意**：任何对 `plan.md` 的修改都会改变 planningDigest，修正后须重跑 `workflow check --stage plan --json` 固定新摘要并按同一 Review ID 递增 Round 复核 |
| DDR-sync-scope-and-pwa-client-r1-F18 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:930-939`（`## Dependency Handoffs` 表；TP1 行缺席，对照 `plan.md:867` 的跨单元 `code:` 依赖与 `:964` 的 MU1b Start/Readiness） | Round 4 曾判「TP1 没有 Dependency Handoffs 行属正确」，理由是「TP1 的上游 WP1/WP2 与它同属 MU1，同一交付单元内部的合入证据由 MU1 的候选/合入记录承载」。本轮 TP1 移入 MU1b、上游 WP1/WP2 留在 MU1a，该前提不再成立：TP1 现有两条**跨单元** `code:` 边（`code:WP1`、`code:WP2`），而表中 8 条 handoff 的 Downstream 为 WP3/WP4/WP5/WP6/WP7/TP2/TP3/TP4，**无 TP1 行** | TP1 的固定起点与失效路径无书面落点：MU1b 若在 TP1 开工后因 WP1/WP2 重开而重建，TP1 的「从 MU1a 的合入提交分支」这一约束与两个计数常量（`EXPECTED_VALID_MESSAGE_CASES`、`EXPECTED_BODY_REJECTED`）的重跑责任只出现在 Merge Strategy 的 Start/Readiness 单元格与 Shared File Ownership 的 Region Note 里，handoff 表无行可填。**不构成 MAJOR**：这是登记缺口而非「声明不实」——依赖类型、波次与写入归属均已正确，且 MU1a 先于 MU1b 的 Order 约束已由 `scheduling-status.mjs:150-152` 机械保证 | 在 `## Dependency Handoffs` 增 TP1 行：Upstream = `WP1, WP2`；Accepted Revision = MU1a 的合入提交；Start Revision = 同左（MU1b 的起點）；Transfer / Inclusion Check = 「TP1 的源提交包含 MU1a 的合入提交；`npm run check` 与 `cargo test -p sync-protocol` 在该提交上跑」；Invalidation = 「MU1a 因 WP1/WP2 重开而重建 → TP1 从新提交重新分支并重跑 PV1，两个计数常量按新的向量总数重定」 |
| DDR-sync-scope-and-pwa-client-r1-F19 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:29-32`（`## Coverage Index` 的 Closure Unit / Stage 列，R1–R4 四行）；同类表述另见 `plan.md:996`、`:1000`、`:1030`、`:1032`、`:1033`、`:1037`、`:1038`、`:1041` | R1–R4 的 Closure Unit 仍写 `MU1/premerge`，而 R1–R4 的 Responsible Units 含 TP1（现属 MU1b）与 WP1（现属 MU1a），拆分后不再有单一的「MU1」单元可闭合。`plan.md:996` 的 IV1 Target Revision 写「MU1–MU3 全部合入后」、`:1000` 的 Code Review 分层写「MU1 只审合同资产」、`:1030` 的下游失效范围写「MU1 契约变更」、`:1037` 的 Completion Criteria 写「全部**三个**交付单元已合入」、`:1038` 写「`## Premerge History` 中 MU1/MU2/MU3 各有对应 PASS」、`:1041` 写「MU1–MU3 已合入主分支」——单元数与名称均停留在拆分前 | 完成判据与失效范围的可核对性下降：`:1038` 是 main 收尾时逐条核对的清单，照字面执行会漏掉 MU1a 与 MU1b 两个单元各需一条 premerge PASS 记录（`workflow-check.mjs:978-981` 对 Merge Strategy 的每个单元强制要求），`:1037` 的「三个交付单元」与实际四个单元直接矛盾。**不构成 MAJOR**：这些是叙述性单元格，不参与任何机械判据（`Closure Unit` 列不被 `workflow-check.mjs` 读取，已 grep 确认），权威成员归属仍在 Merge Strategy 表且该表正确 | R1–R4 的 Closure Unit 改为 `MU1a/MU1b/premerge`；`:1037` 改为「全部四个交付单元（MU1a、MU1b、MU2、MU3）已合入」；`:1038` 的单元枚举同步为 MU1a/MU1b/MU2/MU3；`:996`/`:1000`/`:1030`/`:1032`/`:1033`/`:1041` 的「MU1」按语境改为 MU1a（契约本体）或 MU1a+MUb |
| DDR-sync-scope-and-pwa-client-r1-F20 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:77` 与 `openspec/changes/sync-scope-and-pwa-client/tasks.md:83-90`（§5 Integration Readiness 与 §6 Merge Unit 的 MU1 段） | tasks.md 的交付单元任务未随拆分同步：`tasks.md:77` 写「MU1、main 复核 independent 模式的 **WP1/WP2/TP1** 组成」、`tasks.md:84` 写「构造 **MU1** 候选（WP1 + WP2）」、`tasks.md:88` 写「登记 `## Premerge History`（**MU1、WP1/WP2/TP1**、目标与候选提交…）」，另 `:83`/`:85`/`:86`/`:87`/`:89`/`:90` 共 8 行同样以 MU1 为前缀；全表 grep `MU1a`/`MU1b` 在 tasks.md 中**零命中** | 执行任务的单元名与 plan.md 的四个单元不一致：MU1b（TP1）没有对应的候选构建、PV1、独立 review、premerge receipt 与合入任务，MU1a 也没有独立的候选任务——main 按 tasks.md 推进会按 6.6 登记一个 `MU1` 行，而 `workflow-check.mjs:978-981` 要求 MU1a/MU1b/MU2/MU3 各有一行，`rowContext`（`:914-919`）也会因 `MU1` 不在 `unitWps` 中而无法核对成员，MU1b 的 receipt 与 MU1a 的 receipt 将无法在同一单元名下收口。**不构成 MAJOR**：这是任务层文本与计划层声明的同步缺口，权威的单元成员与 Order 已在 plan.md 正确登记，main 可据 plan.md 执行；与 Round 3/4/5 把同类 tasks.md 文本陈旧定为 MINOR 的口径一致 | 把 §5.2 拆为「MU1a、main 复核 independent 模式的 WP1/WP2 组成」与「MU1b、main 复核 TP1 组成并确认其源提交包含 MU1a 的合入提交」；把 §6.1–6.8 拆为 MU1a 段（候选 = WP1+WP2，登记 MU1a / WP1、WP2）与 MU1b 段（候选 = TP1，登记 MU1b / TP1，证据文件名按单元区分），并把 MU2/MU3 段的前序条件由「WP1/WP2 已合入」明确为「MU1b 已合入」 |
| DDR-sync-scope-and-pwa-client-r1-F14 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:936` | Round 4 提出、维持未解决。WP7 的 Dependencies 为 `code:WP6, code:WP5, contract:WP1, contract:WP2`（`plan.md:866`），但 handoff 行的 Upstream 只写 `WP5, WP6, WP2`，漏 `WP1`；8 条 handoff 中其余 7 条的 Upstream 集合均等于该包依赖目标集合 | `contract:WP1` 的上游在 WP7 行没有 Accepted Revision 与 Invalidation 落点。**不构成 MAJOR**：WP7 的两个代码上游 WP5/WP6 已在 Upstream 中，且 WP7 在 W4 而 WP1 在 W1，波次已排序，缺的是一条已满足的 contract 边的失效登记 | 在 `plan.md:936` 的 WP7 行 Upstream 补 `WP1`，Invalidation 列补「WP1 的 `schemas/sync/v1/**` 变更 → WP7 重跑 PV3」 | 未解决（Round 4 提出，Round 5、本轮复核仍未修正；严重度与阻断判定不变） |
| DDR-sync-scope-and-pwa-client-r1-F13 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:859-870`（11 个包的 Contract Freeze 单元格） | Round 3 提出、维持未解决。复现 `workflow-check.mjs:91-101`：`frozenContractReadable` 要求单元格整体匹配 `^(\S+?)@(\S+)$`；11 个包的多路径单元格不匹配，单路径单元格因反引号使 `path.resolve` 落空，**11 个包全部 readable=false** | 冻结可核对性依赖人工而非机械判据。**不构成 MAJOR**：所引路径逐个 `fs.accessSync` 均存在、无一条写错；且当前波次合法性正是在「冻结不可读 ⇒ contract 视为硬依赖」这一口径下逐行自洽（见 Assessment） | 去掉单元格内的反引号并把多路径引用拆到单条，或改用工具接受的分隔写法；**注意**：若规范化后冻结变为可读，必须同步为受影响包补合法 Serialization Reason 码，否则 `workflow check --stage plan` 会转 FAIL（耦合关系见 Assessment） | 未解决（Round 3 提出，Round 4、Round 5、本轮复核仍未修正；严重度与阻断判定不变） |

## 说明（不计为问题）

- **`workflow check --stage plan` 报出的五条「缺少执行者接收确认」（WP2 Attempt 1 coding / Attempt 1 reviewing / Attempt 2 fixing、WP1 第 2 轮 reviewing、WP2 第 3 轮 reviewing）属已知、用户已接受的派发台账过程缺口，不是规划缺陷。** 根因是派发说明未要求执行者运行 `dispatch --ack`，attempt 推进后无事后补录入口；五个执行者均已确认收到并实际交付（`verification.md:107-121`），用户 2026-10-03 裁定选 (a) 接受为已知偏离，完整记录在 `verification.md` 的 `## Dispatch Reconciliation`。本报告不将其计为 Finding，亦不影响本轮结论；按该裁决，final/archive 阶段须在 `## Final Assessment` 中一并复述。
- **门禁的第六条 error「`Dependency Declaration Review 的 DDR-sync-scope-and-pwa-client-r1 Plan Revision 未绑定当前规划契约摘要`是登记滞后，非规划缺陷。** `verification.md` 的 DDR 表最新行（Round 5）仍绑 `plan-v2:sha256:6751091d…`，而当前摘要为 `c7a697c0…b4`。按 `procedures/scheduling.md`「机械格式检查失败先修记录」，由主 Agent 在导入本报告时把该表补一行 Round 6（Reviewer `reviewer-plan-ddr-6`、Plan Revision `plan-v2:sha256:c7a697c04b5fa591e43185f26c97dc2403d6760efcf2a9b2b10bdecbe3a895b4`、Result FAIL、Report Path 不带反引号的纯路径）即可消除该条。
- **TP1 的波次声明未因拆分而失真。** `Execution Waves` 的波次是**静态最早可开工层级**，Merge Strategy 的 Order 是**合入顺序**，两者本就是不同维度；TP1 声明 W2（`code:` 上游在 W1，earliest=2）并以 Enter Condition 文字记录「实际开工落在 MU1a 合入之后」，是把合入顺序如实写进开工条件，而非把它伪装成依赖类型。`workflow-check.mjs:338-347` 只在 `waveIndex > minimum` 时才要求理由码，TP1 恰为相等，故 `NOT_APPLICABLE` 合法。拆分没有制造新的层级倒置。
- **`plan.md:959` 拆分说明句中的「MU1」是正确的**（描述被拆的对象），不计入 F19。
- **`plan.md:963-966` 的 `Owners: Implementation / Test / Review / Merge` 列在四个单元里各只列一名 reviewer**（MU1a 列 `review-1, review-2` 已补全，MU1b 列 `review-1`，MU2 列 `review-3` 而成员含 WP4 的 `review-4`，MU3 列 `review-5` 而成员含 WP6/WP7 的 `review-6`/`review-7`）。该列不被任何机械判据读取（`deliveryUnits()` 只取 `Delivery Unit / Mode` 与 `WP / TP`；`planning-model.mjs:108` 只把它纳入摘要计算），权威的 reviewer 分配在 Work Packages 的 Reviewer 列（`plan.md:860-870`，11 行互不相同且均不等于 Owner）与 `tasks.md:§3` 的检视任务中逐条列明且无自审。属摘要列的简写，不计为 Finding（与 Round 4/5 同一口径）。
- **MU1a 的 Merge Owner 与 Shared File Ownership 的登记不冲突。** `plan.md:968` 的 Merge Worktree 段只规定候选在独立的 `.worktrees/mu1a-merge`/`.worktrees/mu1b-merge` 构建、不复用执行 worktree；`plan.md:910`–`:926` 各行的 Merge Owner 列是**文件级**合并负责人，二者是不同粒度。`reports/merge-mu1a-assemble-r1.md:10` 记录 MU1a 的 merger 复用了 `.worktrees/mu1-merge`，这与 `plan.md:968` 新写的「每单元的候选在独立的合入 worktree 构建」存在措辞落差，但该报告已如实记录实际路径，属执行记录与计划措辞的对账事项，不改变本轮任何依赖结论。
- **`plan.md:919`（`crates/sync-protocol/src/sync.rs`）与 `plan.md:918`（`command.rs`）的单写者行永不被 `ownershipProblem()` 求值**，故其 Merge Order 取「—」不影响门禁；本轮新增的 `plan.md:923` 是多写者行且四列齐备，两类形态均安全。

## Assessment

### 本轮结论

FAIL。Round 5 的实质结论对当前字节**全部继续成立**，F15/F16 两项已关闭，MU1 拆分所声明的四处改动中有三处**正确且彼此相容**；但第四处（TP1 的 Contract Freeze 指向 `@MU1a`）**违反本变更自身已写入 `verification.md` 的既定规则**，构成一项 MAJOR。

逐项依据：

1. **TP1 的 `code:WP1, code:WP2` 属实 —— 核实通过，且比拆分前更准确。** TP1 交付的是 `crates/sync-protocol/tests/` 下的 Rust 集成测试：`tests/schema_drift.rs:332` 的 `snapshot_resources_match_schema_enum` 把 `SnapshotResource::ALL` 与 `$defs.snapshotResource.enum` 逐条同序钉死，WP2 的 `VIEW_ENUMS` 登记同理；`reports/test-tp1-r1.md:87` 独立记录「基线 `353ba6e` 上 `AgentConnectedState`/`SessionReadBefore`/`hasEarlier`/收窄后的 `SnapshotResource` 全部不存在」。这类测试必须 import 并链接上游的 Rust 类型，属 `code:` 而非 `contract:`。反向侧亦成立：`crates/core/Cargo.toml:18-28` 的闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`，故 WP3/WP4 的纯 `contract:` 降级不是虚假声明。
2. **Execution Waves、Contract Freeze 引用、无环、池容量、独立性 —— 全部通过。** 双口径重算 earliest：TP1 改为 `code:` 后 earliest=2，11 行声明波次与 earliest 逐行相等（W1/W2/W3/W4/W5），软口径下各行均不早于最早批次；`cycles=[]`；上游同批/更早批检查 11 行通过；11 行 Serialization Reason 全为 `NOT_APPLICABLE` 且无 `waveIndex > minimum` 的行；W1=2 coder、W2=3 coder+1 tester、W5=2 tester，对 `openspec/agentic.yaml:11-13`（coding=3 / testing=2）均未超容；11 个包 Owner 与 Reviewer 无一相同，每个包至少一条独立 review 任务、恰好一条 `[wp:WPn]` 派发任务。**唯一例外**是 TP1 的 `@MU1a`（F17）。
3. **Merge Strategy 与 Order —— 核实通过。** `deliveryUnits()` 返回 MU1a/MU1b/MU2/MU3 四单元、mode 均 `independent`、无抛错，成员并集恰为 11 个已声明 ID；Order 全为数字故 `orderGroup` = 1/2/3/4，与 `scheduling-status.mjs:150-152` 的前序阻塞方向一致。**MU2/MU3 的「MU1b 已合入」（`plan.md:965-966`）与新单元名一致**，与各自 Order（3/4）不矛盾，也不存在要求后序单元先合入的反向依赖。
4. **MU1b 拆分引入的跨单元约束已由机械判据承接。** `scheduling-evidence.mjs:165-170` 对 `code:` 边要求跨单元上游处于 `merged`（或同 `integrated` 单元）；TP1 的两条 `code:` 边指向 MU1a，MU1b 的 Order=2 在 MU1a 的 Order=1 之后，`plan.md:964` 的 Start/Readiness 与之一致。TP3 的 handoff 行（`plan.md:938`）已同步为「MU1a 合入后的冻结契约」。**唯一直落点缺失的是 TP1 自身的 handoff 行**（F18）。
5. **Shared File Ownership —— 本轮新增行正确，F15 关闭。** `plan.md:923` 四列齐备、`ownershipProblem()` 返回空串，Region Note 与 `plan.md:964` 及仓库事实（`git -C .worktrees/wp1 diff --stat` 确认 WP1 改动该文件 4 行）三方一致。工具口径下唯一被命中的重叠仍是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`（`plan.md:912` 覆盖）。
6. **F15/F16 已关闭。** `plan.md:923` 关闭 F15；`tasks.md:11`/`:17` 已含 `sync.rs` 的冻结清单与镜像关系说明，关闭 F16。
7. **F17（MAJOR）—— TP1 的 Contract Freeze 引用 `@MU1a` 不合格。** 该 token 既不在 TP1 的 Dependencies（`code:WP1, code:WP2`）列内，也不是工作包 ID。`verification.md:63`（F4）与 `:65`（F10）两行把「freeze 引用必须在依赖列内」写成了本变更的既定规则，Round 1 据此判 MAJOR、Round 2 据此修正 TP2；本轮同一规则在 TP1 上未被应用。11 个包 19 条 `@` 引用中其余 18 条全部合规，仅此一条例外——这不是风格问题而是登记口径的破例，且版本 token 指向交付单元会使「冻结的是哪次交付」不可核对。按 reviewer 角色的判定规则（声明不实/登记缺失按 MAJOR 阻断），本轮为 **FAIL**。
8. **F18–F20 为 MINOR，不构成额外阻断。** F18 是跨单元化后新出现的 handoff 登记缺口（Round 4「同单元故不需 handoff 行」的判断前提已随拆分失效），F19 是叙述性单元名与单元计数陈旧，F20 是 tasks.md 的单元名与 MU1 成员未随拆分同步。三者均属登记/文本同步缺口，权威的依赖类型、波次、写入归属与单元成员在 plan.md 中均已正确声明，且均不参与机械判据（`Closure Unit` 列不被工具读取已 grep 确认）。

按角色判定规则（存在已确认且未解决的 CRITICAL/MAJOR 问题时为 FAIL，列出对应 ID），本轮结论为 **FAIL**，唯一阻断项为 **F17**。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify` 与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；可满足性已核对 | 四项断言逐条对照 `crates/server/src/node_link/resource.rs:151-155`/`:1026-1032` 后确认不依赖广播（Round 1 F5 的不可满足性已消除），本轮该处字节未变 | 否 |
| Main E2E 判据 | 已核对（沿用 Round 4/5） | `not-applicable` 的理由与用户降级批准原话在 `plan.md:1004-1013` 与 `verification.md` 同名节；本轮未发现与该判定冲突的新事实 | 否 |
| MU1a/MU1b 拆分四处改动 | **一项不通过** | Merge Strategy 单元表与 Order、TP1 的 `code:` 依赖与波次/Enter Condition、Shared File Ownership 新增行三项通过；TP1 的 Contract Freeze `@MU1a` 不通过（F17） | 是（已计入 F17，阻断） |
| MU2/MU3 的「MU1b 已合入」 | **通过** | `plan.md:965-966` 的单元名与 Order 3/4 一致；`scheduling-status.mjs:150-152` 的 orderGroup 阻塞方向无反向 | 否 |
| Contract Freeze 引用 ⊆ Dependencies | **不通过** | 19 条引用中 18 条通过；TP1 的 `@MU1a` 不在其 Dependencies 且非工作包 ID（F17）；去反引号后逐条比对 | 是（已计入 F17，阻断） |
| Contract Freeze 路径可读性 | **不通过（机械口径）** | 11 个包 readable 全部 false（F13）；路径本身存在，冻结引用 ⊆ Dependencies 除 F17 外逐项通过 | 是（已计入 F13，非阻断） |
| Shared File Ownership 覆盖 | **通过** | 工具口径下唯一被命中的重叠 WP1×WP2 `docs/SYNC_PROTOCOL.md` 已登记且 `ownershipProblem()` 为空串；本轮新增行四列齐备并求值为空串；按字面展开的重叠另有 1 处已由 F15 的新增行覆盖 | 否 |
| Dependency Handoffs 完整性 | **部分不通过** | 8 条 handoff 中 WP7 的 Upstream 漏 `WP1`（F14）、TP1 无行（F18） | 是（已计入 F14/F18，非阻断） |
| Coverage Index 与单元名 | **部分不通过** | 102 行与 tasks/checks 引用全部有效；R1–R4 的 Closure Unit 及其他叙述性单元名仍为旧 `MU1`（F19） | 是（已计入 F19，非阻断） |
| 任务层交付单元任务 | **部分不通过** | tasks.md §5/§6 仍写 MU1 且含 TP1，无 MU1a/MU1b 任务（F20） | 是（已计入 F20，非阻断） |
| Reviewer 独立性与检视任务 | **通过** | 11 个包 Owner≠Reviewer；每包至少一条独立 review 任务、恰好一条派发任务 | 否 |
| 交付单元解析 | **通过** | `deliveryUnits()` 返回四单元、mode `independent`、无抛错、成员并集=11、无重复 | 否 |
| WP2/WP1 派发台账 ack 缺口 | **上下文，非规划缺陷** | `verification.md` 的 `## Dispatch Reconciliation` 记录根因、五个执行者的实际接收证据与用户 2026-10-03 的 (a) 裁决 | 否 |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md` DDR 表最新行绑 Round 5 摘要；导入本报告后补 Round 6 行即可消除该条 error | 否（属主 Agent 登记刷新） |

### 后续建议与不确定性

- **F17 的修正会改变 planningDigest。** 任何对 `plan.md:867` 的修改都会使当前摘要 `c7a697c0…b4` 失效。修正后须重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新摘要，并按同一 Review ID 递增 Round 7 复核。建议在同一轮内一并修正 F17–F20，避免多次改摘要。
- **F13 与波次的耦合（沿用 Round 3/4/5）。** 当前波次合法性依赖「冻结不可读 ⇒ contract 视为硬依赖」这一事实。若按 F13 建议规范化 Contract Freeze 单元格，必须同步为受影响包补合法理由码，否则 `workflow check --stage plan` 会转 FAIL。规范化不是本轮要求，仅记录该耦合关系。
- **MU1b 的候选与 receipt 能否实际产出，本轮无法验证。** `reports/merge-mu1a-assemble-r1.md:196-200` 记录 MU1a 候选已组装为固定提交 `683dbbb…` 并在等本轮 DDR 绑定；TP1 的交付（`reports/test-tp1-r1.md`）在 MU1a 合入提交上的完整门禁尚未执行，MU1b 的 receipt 尚不存在。这是 apply 阶段 merger 的职责，不影响本轮规划判据的结论。
- **`MU1a` 是否会被再次重命名或拆分，属计划演进风险而非当前缺陷。** F17 的影响评估基于「token 不可反查具体工作包交付」这一可核对性缺口；即使 MU1a 保持稳定，该 token 仍不满足「引用在依赖列内」的既定规则。
- **本报告不宣称任何实现、测试、E2E 或最终验收通过。** MU1a 的候选 PV1、独立 review 与 premerge 门禁均未执行；`## Checks` 六行仍全为 NOT_APPLICABLE。

FAIL
