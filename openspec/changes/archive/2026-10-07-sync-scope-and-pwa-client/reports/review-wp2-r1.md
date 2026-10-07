<!-- WP2 独立代码检视报告（交付提交轮，Round 3）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.2"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wp2-r1-new"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP2 的实现或修复对话，未继承任何实现上下文）"
target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
scope: "WP2（MU1a 单元成员，R5-R9 的字段/契约侧）的交付提交检视。Base 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f -> Target f64a1968ed56abbac680d661f0e57f05d57fda9d，7 files / +161 / -13。逐项核对：event-views.schema.json 的 file.changed 三个新可选字段与 agent.connected/agent.disconnected 的 state 封闭枚举；crates/sync-protocol/src/views.rs 的 VIEW_ENUMS 登记完整性、镜像 as_str 与 schema 取值的一一对应、FileChanged 与两个 Agent 视图的同形性；三个 view-* fixture 的 sessionId/sessionSequence null 化；fixtures/sync/v1/manifest.json 的不重不漏与排序保持；docs/SYNC_PROTOCOL.md 10.3 与 docs/ACP_COMPATIBILITY_MATRIX.md 的文档口径与命令目录一致性；写入范围合规。并复核前序遗留 F3、F5。生产者侧（core 派生、行级 diff、路径规范化、节点级落库、标题写入）属 WP3/WP4，不在本轮范围。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并、未运行任何写文件的构建。"
issues: "0 CRITICAL / 0 MAJOR；3 MINOR + 1 SUGGESTION"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp2-r1.md"
  - ".target-wt/wp2-verify.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp2-r2.md"

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-wp2-r1-new"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.2"
    work_package: WP2
    role: reviewer
    phase: branch
    round: 3
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: REVIEW
    evidence_id: review-wp2-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定提交 f64a1968ed56abbac680d661f0e57f05d57fda9d（其父即基线 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f）上做只读检视：读取 git diff 353ba6e..f64a196 的 7 个文件，对照 specs/core-derived-events、specs/local-agent-host、specs/workspace-resolution、design.md D4-D7、plan.md 的 WP2 行与 Shared File Ownership；只读消费 .target-wt/wp2-verify.log（exit 0）。未执行任何编译或测试。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp2-r1` / 3（本轮为交付提交轮；Round 1/2 是同一 Review ID 的工作区差异轮） |
| Review Type / Stage | branch（工作包交付前检视：实现已交付、PV1 PASS、合入 MU1a 候选之前） |
| Work Package | WP2（MU1a 成员，R5-R9 的字段侧） |
| Repository / Worktree | `D:\Project\acp-remote`，固定检视 worktree `.worktrees\wp2`（`git status --porcelain` 干净） |
| Base Revision | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（即 target 的父提交，已核对） |
| Target Revision | `f64a1968ed56abbac680d661f0e57f05d57fda9d` |
| 实际修改文件 | `schemas/sync/v1/event-views.schema.json`、`crates/sync-protocol/src/views.rs`、`fixtures/sync/v1/valid/view-file-changed.json`、`fixtures/sync/v1/valid/view-agent-connected.json`、`fixtures/sync/v1/valid/view-agent-disconnected.json`、`docs/SYNC_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`（7 文件，与作者声明一致） |
| 版本稳定性 | 稳定。worktree HEAD 为 `44bcda8`（并入主分支 `b964ae3` 的构建配置修复的合并提交），其内容对上述 7 个文件与 target 逐字节相同（已用 `git show f64a196:...` 与 worktree 文件比对）。`b964ae3` 只改 `vendor/windows-local-ipc/Cargo.toml`（加空 `[workspace]` 表），不属 WP2 范围，未误判为越界。 |
| 读取的规则与需求 | `roles/reviewer.md`；`specs/core-derived-events/spec.md`（5 需求全文）、`specs/local-agent-host/spec.md`（2 需求）、`specs/workspace-resolution/spec.md`（MODIFIED）、`proposal.md`、`design.md` D4/D5/D6/D7、`plan.md`（Contract Changes :17-18、Work Packages :860、Shared File Ownership :912-922、Project Verify :988+）；`docs/SYNC_PROTOCOL.md` 10.3、`docs/ACP_COMPATIBILITY_MATRIX.md` 7；`compatibility/acp/v1/matrix.json`、`compatibility/commands/v1/commands.json` |
| 验证证据 | 只读消费 `.target-wt/wp2-verify.log`（作者实例执行，exit 0）；Reviewer 本轮**未执行**任何编译/测试/E2E（只读边界）。另用只读临时 node 脚本对 schema 语义与 fixture/manifest 完整性做独立佐证（脚本已删除，未改动仓库任何文件）。 |
| 限制 | 不判「用例是否充分」（交 validator / 主 Agent Coverage Index）；不判生产者侧实现（WP3/WP4）；不执行 E2E。 |

## 逐项核对结论（对应派发单 Scope 的 7 点）

1. **`file.changed` 的三个新增可选字段与必填集合** — **通过**。`schemas/sync/v1/event-views.schema.json:322-325` 新增 `addedLines`/`deletedLines`（`$ref: common.schema.json#/$defs/decimalString`，实测 `^(0|[1-9][0-9]*)$`）与 `outsideWorkspace`（`type: boolean`）；`:321` 的 `required` 仍恰为 `["changeId","kind","displayPath","summary"]`，**必填集合未变**，`additionalProperties: true` 未动。与 design D4（decimalString 而非 JSON number）、D5 一致。ajv 实测：最小集通过、`addedLines:null` 被 `type` 拒、`addedLines:"012"` 被 `pattern` 拒、`outsideWorkspace:"false"` 被 `type` 拒——两侧口径一致。
2. **`agent.connected`/`agent.disconnected` 的 `state` 封闭枚举** — **通过**。`:334` = `{"enum":["connected"]}`、`:343` = `{"enum":["disconnected"]}`；ajv 实测跨取值互拒（`connected` 事件给 `idle` 被拒，`disconnected` 事件给 `connected` 被拒）。与 R8「各自唯一且互不相同」及 D6 一致。节点级语义（`sessionId` 为空、不表达会话活跃度、当前无投递通道）已在文档侧写明，见第 6 点。
3. **`VIEW_ENUMS` 登记完整性** — **通过**（本轮**重点复核项**）。`crates/sync-protocol/src/views.rs:61-62` 新增 `("agent.connected","/properties/state",&["connected"])` 与 `("agent.disconnected","/properties/state",&["disconnected"])`，两条 JSON Pointer 相对 `$defs[eventType]` 正确；新增的 `AgentConnectedState::ALL/as_str`（`:234`/`:238`）与 `AgentDisconnectedState::ALL/as_str`（`:283`/`:287`）与 schema 取值逐条同序。独立核对 `event-views.schema.json` 全 `$defs` 的每个 `enum`，与 `VIEW_ENUMS` 逐条比对后**无遗漏**；Rust 镜像的 `as_str` 文本与 `enum` 取值一一对应。PV1 日志中 `view_enums_match_schema` ok（`:1657`）与此一致，但该门禁的第三个方向（镜像 `ALL/as_str` 与登记序列）仍靠硬编码断言集，见 F3。
4. **`FileChanged` 与两个 Agent 视图与 schema 严格同形** — **通过**。`views.rs:610-615` 的 `changeId`/`kind`/`displayPath`/`summary` 与 schema 的 required 集合、`NonEmptyText<1024>`/`Text<2048>` 边界一致；三个新字段为 `Option<DecimalString>`/`Option<bool>` 且带 `default` + `deserialize_optional_non_null` + `skip_serializing_if`（`:619-642`），与同文件 `error`（`:473-477`）、`block`（`:507-511`）的既有约定同型，且修正了 Round 1 的 F2。`AgentConnected`（`:454-459`）为 `agent_id: NonEmptyText<128>` + `state: AgentConnectedState`；`AgentDisconnected`（`:464-482`）同型并多一个可选 `error: Option<PublicError>`——两者与 schema 的字段名、可选性、类型逐项一致。**前序 F1 的 Rust 类型化镜像未随 schema 收窄问题已修复**（`state` 已由 `NonEmptyText<32>` 改为各自的镜像枚举）。
5. **fixture 与 `manifest.json` 的不重不漏、排序与节点级 null 化** — **通过**。三个 `view-*` fixture 的 `sessionId`/`sessionSequence` 已置 `null`（**F5 已关闭**，见下）；`view-file-changed.json` 新增 `"addedLines":"12"`/`"deletedLines":"3"`/`"outsideWorkspace":false`，与 schema 相容。独立遍历 `.worktrees/wp2/fixtures/sync/v1/{valid,invalid}` 与 manifest：`view-*` 磁盘 33 个 = 登记 33 个，missing/extra/dupe 均为 none。`manifest.json` **未被本 diff 修改**（`name-status` 未列出），故「排序保持」成立——manifest 既有顺序本身就是作者声明的非字典序分组序，本 diff 未触碰。`view_projections.rs` 的 `eventType` 与 `viewDef` 绑定断言通过（PV1 全绿），可拦下「只改事件体不改声明」的方向。
6. **文档口径与命令目录** — **通过**。`docs/SYNC_PROTOCOL.md:1029` 登记了 `file.changed` 三个可选字段的类型；`:1039` 说明行级差异口径（**MUST NOT 呈现为零改动**，对应 R6 与 D4）；`:1040` 说明只覆盖 Agent 在工具调用里声明的改动、shell 驱动的改动不计入；`:1041` 说明 `displayPath` 相对工作区根、越界置 `outsideWorkspace: true` 且只给文件名、按**规范化后的前缀关系**判定（对应 R7/D5，与 `specs/workspace-resolution/spec.md` 的 MODIFIED 正文一致）。`:1030-1031` 与 `:1043` 写明节点级、`sessionId` 为空、封闭词表、不表达会话活跃度、当前无投递通道、客户端呈现状态未知（对应 R8/D6 与 `specs/local-agent-host/spec.md` 的进程生命周期需求）。`:1045` 写明标题单向来自 `session_info_update` 投影出的 `session.info.changed`（对应 R9/D7）。**R9 的「标题单向由 Agent 通知更新、无重命名命令」已落到文档**（`ACP_COMPATIBILITY_MATRIX.md:186` 亦有一句），且命令目录核对一致：`compatibility/commands/v1/commands.json` 的 13 条命令中**不存在**任何重命名/标题命令（`crates/sync-protocol/src/command.rs` 的命令集同样无 rename）。`state:"connected"`/`"disconnected"` 与 `delivery=mvp`/`sync=event` 等字面量与 `compatibility/acp/v1/matrix.json:73` 同 ID 同取值。
7. **写入范围合规** — **通过**。`git diff 353ba6e..f64a196 --name-only` 恰为上述 7 个文件：`schemas/sync/v1/event-views.schema.json`、`crates/sync-protocol/src/views.rs`、三个 `fixtures/sync/v1/valid/view-*.json`、`docs/SYNC_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`，全部落在 WP2 的 Write Scope 内。**确认 diff 未触及** `crates/sync-protocol/src/command.rs`、`crates/sync-protocol/src/sync.rs`（WP1 独占）、`crates/sync-protocol/tests/**`（TP1 区域）、`crates/node-link-protocol/**`、`crates/core/**`、`crates/storage-sqlite/**`、`crates/agent-host/**`、`crates/app/**`。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| review-wp2-r1-F3 | MINOR | `crates/sync-protocol/tests/schema_drift.rs:295-322`（TP1 区域，不在本 diff 内） | 该测试的第三个方向（「Rust 镜像的 `ALL`/`as_str` 必须等于登记序列」）由对 `PlanPriority`/`PlanStatus`/`ElicitationAction`/`TerminalStream` 四个枚举的**硬编码** `assert_eq!` 实现（`.worktrees/wp2` 内实测：这四条之后即函数结束，`AgentConnectedState`/`AgentDisconnectedState` 不在断言集）。作者在交付报告中称「TP1 工作区已在 :327/333 追加该断言」——核实结果：该追加**确实存在**，但位于 **TP1 worktree**（`D:\Project\acp-remote\.worktrees\tp1`）的**未提交工作区差异**（`git status` 显示 `M crates/sync-protocol/tests/schema_drift.rs`，TP1 分支 HEAD 仍为基线 `353ba6e`，无提交可绑定）。 | 对 WP2 交付面**无阻断**：WP2 的登记表已完整登记两条新枚举，前两个方向的门禁（登记表与 schema 双向）在 target 上真实生效并已 PASS（`view_enums_match_schema` ok）。影响面限于「登记表与镜像」这一方向对新枚举仍无机器判据——若日后单改 `AgentConnectedState::ALL` 或 `VIEW_ENUMS` 的取值，门禁不会失败（这正是 Round 1 的 F1 能在全绿下交付的原因）。 | 该断言属 TP1 的 `crates/*/tests/` 区域，由 TP1 在其交付提交中固化（当前仅为工作区差异，需落成提交后门禁才在固定版本上生效）。更彻底的做法是把该方向改为对 `VIEW_ENUMS` 逐条遍历比对，使「已登记必有镜像断言」对每条登记成立。**不要求 WP2 修改**（写入范围外）。 | **未关闭**（仍为 MINOR；断言存在于 TP1 工作区差异、未固化；不属 WP2 交付面，见 Assessment） |
| review-wp2-r1-F5 | — | `fixtures/sync/v1/valid/view-agent-connected.json:10-11`、`view-agent-disconnected.json:10-11` | 作者称已修（两个 fixture 的 `sessionId`/`sessionSequence` 置 null）。**diff 亲自核实成立**：两文件的 `sessionSequence` 由 `"225"`/`"226"` 改为 `null`、`sessionId` 由 UUID 改为 `null`（各 `-2/+2`）；worktree 实测 `connected.sessionId=null, sessionSequence=null`、`disconnected.sessionId=null, sessionSequence=null`。与 R8「会话标识 MUST 为空」及 D6 节点级语义一致，并与 `AGENTS.md` 的「缺失不得伪装为零」一致。 | 无。 | 无（已修）。 | **已关闭** |
| review-wp2-r1-F6 | MINOR | `docs/SYNC_PROTOCOL.md:1040`（新增段落第二项） | 该行断言「Agent 通过 shell 重定向、脚本或外部工具驱动的文件改动不在其中……**此类改动不产生 `file.changed` 事件**」。R5 正文的「系统 MUST NOT 为不含文件改动的工具调用派生该事件」限定在**工具调用**范围内；而 shell 驱动的改动的现实载体正是一次（通常是终端型）工具调用。该括注把「派生源限定为类型化 Diff 元素」（正确、可由上游内容判定）扩张为「对语义上属于文件改动的调用一律不产生事件」（一个**只可由 Agent 的意图推断**的更强主张），并嵌入「MUST NOT」。 | 文档口径比 spec 更严，且更严的部分不可由既有输入判定：实现侧只能按「调用内容里有无 Diff 元素」决定派生，无法区分「这次写文件是 shell 干的还是 Agent 声明的」。若按此句做验收，会把实现推向对调用意图的猜测（与 `local-agent-host/spec.md` 的「MUST NOT 从工具调用的自由形状原始输入中猜测文件改动」直接冲突）。 | 把该句收回到 R5/design 的可判定口径，例如改为「行数统计只覆盖 Agent 在其工具调用里以类型化 Diff 声明的改动；不含 Diff 元素的工具调用（含 shell 重定向、脚本或外部工具驱动的改动）不产生 `file.changed`，其行数也不计入。」即把结论挂在「有无 Diff 元素」上，而非挂在「改动由谁驱动」上。 | 不适用（本轮新发现） |
| review-wp2-r1-F7 | SUGGESTION | `docs/SYNC_PROTOCOL.md:1045` 与 `docs/ACP_COMPATIBILITY_MATRIX.md:186` | 两处把 `session.info.changed` 的来源判别子写作 `session_info_update`（下划线），而 sibling 行 `:1060` 的映射表与 `:1009` 的字段表把同一判别子写作 `session_info_update`——**本变更自身的文档措辞在两种写法之间不一致**。上游固定快照的实际取值是 `session_info_update`（`schemas/acp/v1/upstream/schema.json:3806`、`crates/acp-protocol/src/update.rs:72/94`）、`compatibility/acp/v1/matrix.json` 的 `wireValue` 亦为 `session_info_update`；仓库既有文档 `docs/SESSION_CONTINUITY_DESIGN.md:51` 同样用下划线。因此本 diff 的新措辞引入了与「上游快照 + 机器矩阵」不一致的写法。 | 无功能影响（纯措辞）。风险是读者/后续文档可能照抄该写法，而 `check:acp` 只强制 `wireValue` 取值、不校验散文措辞，漂移会留在文档里。 | 把新增段落的 `session_info_update` 统一为 `session_info_update`，与 `:1060` 的映射表、上游快照和 `matrix.json.wireValue` 同形。 | 不适用（本轮新发现） |

## Assessment

### 本轮结论：**PASS**

在 Target Revision `f64a1968ed56abbac680d661f0e57f05d57fda9d` 上，WP2 的交付内容与派发单 Scope 的 7 项逐项核对**全部通过**：`file.changed` 的三个可选字段类型正确且**必填集合未变**；`agent.connected`/`agent.disconnected` 的 `state` 已成封闭枚举且取值互不相同；`VIEW_ENUMS` 登记**完整**、镜像 `as_str` 与 schema 取值一一对应；`FileChanged` 与两个 Agent 视图与 schema 严格同形（**前序 F1 已修复**）；三个 fixture 的节点级 `sessionId`/`sessionSequence` 已置 null（**F5 已关闭**）；文档口径与命令目录一致（R9 的单向标题与无重命名命令已落到文档，命令目录无该命令）；写入范围**严格限于** WP2 的 Write Scope，未触及 WP1 独占的 `command.rs`/`sync.rs` 与 TP1 区域 `crates/*/tests/**`。

**未解决的 CRITICAL/MAJOR：无。** 剩余 3 项 MINOR + 1 项 SUGGESTION 均为非阻断：F3 是 TP1 区域的门禁方向缺口（不属 WP2 交付面，见下），F6 是新增文档句比 spec 更严且更严的部分不可判定，F7 是措辞与上游快照不一致。

### 对 F3、F5 的明确结论

- **F5 — 已关闭**。依据：diff 直接显示两个节点级 fixture 的 `sessionId`/`sessionSequence` 各由具体值改为 `null`（`view-agent-connected.json` 与 `view-agent-disconnected.json`，各 `-2/+2`），worktree 实测二次确认；与 spec R8 及 design D6 一致。
- **F3 — 未关闭（仍为 MINOR），但不属 WP2 的交付面，不阻断本包**。依据：(a) 断言在当前 **TP1 worktree 的未提交工作区差异**中存在（`git diff` 显示在 `view_enums_match_schema` 末尾追加了两条 `AgentConnectedState`/`AgentDisconnectedState` 的 `ALL`/`as_str` 断言，紧邻既有四条之后），**TP1 分支 HEAD 仍是基线 `353ba6e`，无提交可绑定**；(b) target 提交 `f64a196` 的 `crates/sync-protocol/tests/schema_drift.rs` 与基线逐字节相同（`git diff` 该路径为空），故该断言**不在 WP2 的交付面**；(c) WP2 侧相关契约（`VIEW_ENUMS` 登记完整性）本轮独立核对**已完整**，且门禁的另两个方向在 target 上真实生效并已 PASS。结论：门禁缺口真实存在，但归属 TP1，须在 TP1 将其固化为提交；**不作为 WP2 的 FAIL 依据**。

### 证据状态

| 检查 | 状态 | 结论 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1（`.target-wt/wp2-verify.log`，作者实例执行） | 已只读消费，末行 `exit=0` | `check:schemas`/`check:commands`/`check:docs` 等十道门禁 + `cargo fmt/clippy/test` 全绿；`schema_drift` 6/6、`view_projections` 通过 | **是**（作为 PV1 证据采信；Reviewer 未复跑，符合只读边界） |
| schema 语义独立佐证（ajv，复刻 check 脚本参数） | 已执行（只读临时脚本，已删除） | 5 条新增契约的接受/拒绝行为与 diff 声明一致 | 是（独立确认字段类型与枚举封闭性） |
| fixture/manifest 完整性独立佐证 | 已执行（只读临时脚本，已删除） | 33 = 33 不重不漏；manifest 未被 diff 修改 | 是 |
| 版本稳定性 | 已核实 | worktree HEAD `44bcda8` 对 7 个文件与 target 内容逐字节相同；`b964ae3` 仅改 `vendor/windows-local-ipc/Cargo.toml` | 否（不影响结论；已在 Review Context 记录） |
| PV2 / PV3 / IV1 / AC1–AC3 / E2E | 待补（MU2/MU3 及主分支阶段） | 本包无 E2E；生产者侧行为（行级 diff、路径规范化、节点级落库、标题写入）属 WP3/WP4，本轮不判 | 否 |
| 覆盖充分性（TP1 的 `state`/`file.changed` 向量与 `schema_drift` 断言是否足够） | 不在本轮范围 | 按角色边界交 validator / 主 Agent Coverage Index | 否 |

### 实际检查范围

- 固定版本的 full diff：`git diff 353ba6e..f64a196`（7 文件 / +161 / −13）逐行阅读，含 `views.rs` 新增的两个枚举与其全部 impl、`FileChanged` 三个新字段的 serde 属性。
- 需求与契约对照：`specs/core-derived-events/spec.md`（R5–R9 全文）、`specs/local-agent-host/spec.md`（2 需求）、`specs/workspace-resolution/spec.md`（MODIFIED）、`design.md` D4/D5/D6/D7、`plan.md`（Contract Changes、WP2 行、Shared File Ownership、Project Verify）、`AGENTS.md`。
- 权威文档与机器来源：`docs/SYNC_PROTOCOL.md` §10.2–§10.3、`docs/ACP_COMPATIBILITY_MATRIX.md` §7、`compatibility/acp/v1/matrix.json`、`compatibility/commands/v1/commands.json`、`schemas/acp/v1/upstream/schema.json`。
- 消费侧核对（cross-boundary）：`crates/sync-protocol/src/views.rs` 的 `view_registry!` 宏与 `project()` 分派表（两个 eventType 各自显式分支，无静默丢弃）；`views::VIEW_ENUMS` 与 `schema_drift.rs` 的 `view_enums_match_schema` 双向门禁；`crates/sync-protocol/tests/view_projections.rs` 的 fixture 与 eventType 绑定断言。
- 写入范围核对：`git diff --name-only` / `--name-status` 全量文件清单；确认未触及 `crates/sync-protocol/src/{command,sync}.rs`、`crates/sync-protocol/tests/**`、`crates/{core,storage-sqlite,agent-host,node-link-protocol,app}/**`。
- 前序遗留复核：F3（`schema_drift.rs` 的断言集，含 TP1 worktree 的未提交差异）、F5（两个节点级 fixture 的 null 化）。

### 未验证内容

- **未执行任何编译、测试、lint 或 E2E**（只读边界）。PV1 结论仅采信 `.target-wt/wp2-verify.log`，Reviewer 未复跑；对 schema 语义与 fixture 完整性的验证是只读的 node/ajv 独立佐证，非项目门禁执行。
- **未判「用例是否充分」**：TP1 是否已有覆盖 `state` 封闭枚举拒绝路径、`file.changed` 三个可选字段（含 `null` 拒绝与省略语义）的固定向量，以及 `schema_drift.rs` 第三个方向的断言是否固化，均交 validator 与主 Agent Coverage Index。
- **未判生产者侧实现**：R5 的 Diff 派生、R6 的行级 diff 计算、R7 的规范化前缀判定、R8 的节点级落库与 DDL CHECK 自洽、R9 的标题写入与部分更新语义，均属 WP3/WP4，不在本轮检查范围。
- **未核对 `b964ae3`**（主分支带入的 `vendor/windows-local-ipc/Cargo.toml` 空 `[workspace]` 表修复）的目的与正确性——按派发单声明跳过，仅确认其文件范围与 WP2 无交集。
- **未在真实 Daemon 或客户端环境上验证**节点的连接/断开事件投递（design D6 已声明本变更内无投递通道，属计划收窄）。

### 移交建议

- **F6**：交 WP2 的实现 Agent（coding-2）在合入前修订 `docs/SYNC_PROTOCOL.md:1040` 的措辞（把结论挂在「有无 Diff 元素」而非「改动由谁驱动」），因该句是本 diff 新增且与 `local-agent-host` 需求的可判定口径存在张力；若不修订，请由主 Agent 记录为已知的计划内收窄，并确保 TP 的验收把它读作「限于工具调用」。
- **F7**：交 WP2 或文档收口方统一 `session_info_update` 为 `session_info_update`；不阻断合入。
- **F3**：交 TP1（testing-1）把 `schema_drift.rs` 的第三个方向断言**固化为提交**；本轮已核实该断言仅存在于 TP1 worktree 的未提交差异中，请勿在 MU1b 之前把「已在 :327/333 追加」当作已落地的门禁证据。
- **主 Agent**：WP2 在本 Target Revision 上可进入 MU1a 候选；合入前只需决定 F6 是「修订」还是「记收窄」。

PASS
