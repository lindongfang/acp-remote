<!-- WP4 独立代码检视报告（交付提交轮，Round 1）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "3.8"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-w4-r1"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP4 的实现或修复对话，未继承任何实现上下文；WP3 由另一名 reviewer 并行检视，与本案无关）"
target_revision: "c63e199dded5cd735054b899c5a2ecede418df11"
scope: "WP4（MU2 单元成员，R8/R21/R22）的交付提交检视。Base 33040d78324ff51be49219fcb3aac054d0100cc9 -> Target c63e199dded5cd735054b899c5a2ecede418df11，7 files / +1509 / -19，全部在 crates/agent-host/。核心必查：A. Diff 透传逐字节保真（R22/D4）；B. 进程生命周期两条硬语义与节点级事件形状（R8/D6）；C. R21 是核实而非新写。另判 D. 越界未验证项（组合根接线）归属；E. 作者自报开放问题与 spec 的关系。"
changes: "只读检视，未修改任何文件；未切换分支、未提交、未合并、未运行任何写文件的构建或测试。仅新增本报告。"
issues: "0 CRITICAL / 1 MAJOR / 1 MINOR / 1 SUGGESTION"
result: FAIL

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-w4-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "3.8"
    work_package: WP4
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "c63e199dded5cd735054b899c5a2ecede418df11"
    evidence_type: REVIEW
    evidence_id: review-w4-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-w4-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在固定提交 c63e199dded5cd735054b899c5a2ecede418df11（父即基线 33040d78）上做只读检视：读取 git diff 33040d78..c63e199 的 7 个文件与 .worktrees/wp4 的完整目标内容，对照 specs/local-agent-host、specs/core-derived-events、design.md D4/D6、plan.md（WP4 行、Shared File Ownership）、AGENTS.md §3/§4；只读消费 reports/PV1-wp4.log、PV1-wp4-rust-test.log、PV2-wp4.log 与 WP2/WP3 交付报告。未执行任何编译或测试。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-w4-r1` / 1（交付提交轮；本线程首轮） |
| Review Type / Stage | branch（工作包交付前检视：PV1/PV2 已 PASS，合入 MU2 候选之前） |
| Work Package | WP4（MU2 成员，R8/R21/R22） |
| Repository / Worktree | `D:\Project\acp-remote`，固定检视 worktree `.worktrees\wp4`（HEAD `c63e199`） |
| Base Revision | `33040d78324ff51be49219fcb3aac054d0100cc9`（target 的父提交，已核对） |
| Target Revision | `c63e199dded5cd735054b899c5a2ecede418df11` |
| 实际修改文件 | `crates/agent-host/src/{bin/acpr-fake-acp-agent.rs, host.rs, lib.rs, mapper.rs, node.rs, process.rs}`、`crates/agent-host/tests/session.rs`（7 文件，与作者声明一致）；`crates/core/`、`crates/sync-protocol/`、`schemas/`、`docs/`、`fixtures/` 零改动（已用 `git diff --name-only` 核对）；`crates/agent-host/Cargo.toml` 无新依赖（diff 未列出） |
| 读取的规则与需求 | `roles/reviewer.md`、`roles/_shared/role-report.md`；`specs/local-agent-host/spec.md`（R8 host 侧 + R21/R22）、`specs/core-derived-events/spec.md`（R5/R8 消费侧）、`proposal.md`、`design.md` D4/D6、`plan.md`（Contract Changes、WP4 行 :862、Shared File Ownership :908-931、Project Verify AC1 :996/1029、Failure and Recovery）、`AGENTS.md` §3/§4；`docs/SYNC_PROTOCOL.md` §10.3/§14、`schemas/sync/v1/event-views.schema.json`、`crates/core/src/ports.rs`（EventSink）、`crates/acp-protocol/src/{raw.rs,content.rs}`、`crates/core/src/model/backend.rs`（`view_only`） |
| 验证证据（只读消费） | `reports/PV1-wp4.log`（`npm run verify`，exit 0）、`reports/PV1-wp4-rust-test.log`（`cargo test --workspace` 重跑，exit 0）、`reports/PV2-wp4.log`（`cargo test -p core -p storage-sqlite -p agent-host`，exit 0）；`reports/deliver-wp2-r1.md`、`reports/deliver-wp3-r1.md`、`reports/deliver-wp4-r1.md`。Reviewer **未执行**任何编译/测试/E2E（只读边界）。 |
| 限制 | 不判「用例是否充分」（交 validator / Coverage Index）；不重复跑 Project Verify；WP3 的实现由 review-3 并行检视，本轮只在跨包接缝处读其代码作为消费方证据；不执行 E2E。 |

## A. Diff 透传的逐字节保真（R22 / D4）——**通过**

**结论**：`raw` 确实来自原始文档的字节切片，唯一来源是**类型化** Diff 元素，`payload.acp` 逐字不变，无 Diff 时 `diff` 键缺席。作者「恒为数组」与「无 Diff 时键缺席」两句**不矛盾**（分属生产者与消费方两个条件）。

| 核对点 | 证据（target） | 结论 |
| --- | --- | --- |
| 切片来自原文而非重新序列化 | `mapper.rs:347-370` 的 `diff_elements` 用 `member_literal`/`array_elements`/`scan_value_end` 按 `raw.as_str()` 的**字节偏移**切 `params.update.content[]` 元素；`Value::to_string()` 只用于 `decoded_string` 取三字段**取值**。集成用例 `diff_slice_is_byte_identical_and_acp_raw_matches_its_digest` 断言 `raw_json[offset..offset+len] == raw` | 成立（`raw` 必为原文子串） |
| 转义/空白/大整数保真 | 单测 `diff_elements_carry_raw_slices_and_decoded_paths` 用 `src\/main.rs` 断言 `raw` 保留 `\/` 原样（`RAW_WITH_DIFF` 刻意混入空白与转义） | 成立 |
| 多元素 | `array_elements` 按逗号切分，`diff_elements` 遍历全部；单测 `multiple_diff_elements_are_all_delivered` 断言 2 个元素全交付 | 成立（不静默只发第一个） |
| 嵌套/多字节 | `scan_value_end` 的 `{`/`[` 分支带 `in_string`+`escaped` 状态跟踪、深度计数、`checked_sub`；索引只落在 ASCII 结构位；单测 `diff_slices_survive_multibyte_content` 用中文路径/正文断言子串成立 | 成立（不切进多字节字符） |
| MUST NOT 从 `rawInput` 推断 | `attach_diffs`（`mapper.rs:325-341`）只读 `content: &[ToolCallContent]` 与 `raw`；`diff_elements` 只按 `element.type == "diff"` 选取；`tool_call_view`/`tool_call_update_view` 不投影 `raw_input`。全 crate grep 无 `rawInput` 读取路径参与文件改动 | 成立 |
| `payload.acp` 逐字不变 | `acp_raw`（`mapper.rs:657-660`）用 `raw.as_str()` 原样 + `digest_of` SHA-256 + `AcpRaw::available`；`AcpRaw::validate` 断言 `byte_length == raw_json.len()`。集成用例断言 `byteLength`/`sha256` 与原文一致 | 成立 |
| 无 Diff ⇒ 键缺席 | `attach_diffs` 先 `content.iter().any(has_diff)`，否则直接 return；单测 `views_without_typed_diff_carry_no_diff_key` 断言无类型化元素时 `view.get("diff").is_none()`（非 `null`、非空数组） | 成立 |
| 「恒为数组」vs「键缺席」 | 消费方 `crates/core/src/derive.rs` 的 `derived_file_changes` 对 `diff` 键做 `array_items` 并回退单 object（注释「适配器契约是数组（恒为数组，长度 ≥ 1）」）。生产者的「恒为数组」指**键存在时**必是数组；「键缺席」指**无类型化元素时**，两者条件互斥 | **不矛盾** |
| 类型化与原文不一致 | `attach_diffs` 在「类型化有 diff 但原文定位不到」时 `elements.is_empty()` → 不挂键（「不发明取值」）；「类型化无 diff 但原文有」时 `has_diff` 为假 → 不挂键。两向都以类型化 `content` 为准，保守且与「类型化 Diff 是唯一来源」一致 | 成立 |

## B. 进程生命周期的两条硬语义（R8 / D6）——**通过**

**结论**：`AgentLifecycle` 状态机使「每进程实例恰好一次 connected」与「断开不早于退出判定」成为状态性质而非调用方纪律；节点级事件形状正确且不经 session 的 `EventSink` 路由。**未构造出**「已发出 disconnected 但运行时仍被当作存活」的窗口。

1. **复用既有进程不重复连接（含同一 profile 多次 spawn）**：每个进程实例由 `ensure_runtime_tracked` 新建 `AgentRuntime`（`host.rs:354-362`），各持一份 `lifecycle`（`Mutex<AgentLifecycle>`，`host.rs:83`）。`on_spawn()` 仅在 `Stopped` 成立一次（`node.rs:104-110`），`create`/`resume`/`agent_capabilities` 三条入口都经 `ensure_runtime_tracked` 复用 runtime，只有首次 successful `ensure_runtime_tracked` 走到 `report_connected()`（`host.rs:395`）。同一 profile 再次 spawn 是**新进程、新 lifecycle**，因此产生新的 connected（正确）。单测 `lifecycle_reports_connect_at_most_once`、集成 `profile_process_reports_connect_once_and_reuse_adds_nothing`（3 会话复用仅 1 条 connected）、`capability_probe_spawn_reports_connect_once` 覆盖。
2. **断开不早于退出判定、且恰好一次**：`ExitMark`（`node.rs:79-96`）以 `compare_exchange` 承载「先标记、后上报」。上报入口 `report_disconnected`（`host.rs:116-134`）只在 `exit_mark.take_disconnect() || supervisor.has_exited()` 为真时发事件。两个调用点都在退出判定之后：退出钩子由 `converge_exit`（`process.rs`，`self.exit.mark(final_status); self.exit.report_exit();`）在 `mark` 之后同步调用；`on_exit()`（`host.rs:210-224`）先 `mark_exit_if_running` + `exit_mark.mark()` 再上报。钩子由 `Mutex<Option<ExitReporter>>::take` 保证只被取走一次（`process.rs` `report_exit`）。集成 `oversize_exit_reports_disconnect_after_the_exit_verdict` 在同步 sink 内现场回读 `runtime_running()` 并断言为假；单测 `exit_mark_makes_disconnect_impossible_before_the_exit_verdict`、`report_disconnected_is_a_noop_before_the_exit_verdict` 覆盖。
3. **「窗口」构造性检查**：`report_disconnected` 先取 `due = on_exit()`，若 `due==true` 但 `exited==false`，`node::report_disconnected` 会静默不发并已把 lifecycle 置 `Disconnected`，导致后续无法补发。逐一核验两个调用点后确认 `exited` 在该路径上恒为真（钩子路径 `exit.mark` 已先行；`on_exit` 路径先 `mark_exit_if_running` 再 `exit_mark.mark`），**不存在**该丢失窗口。`sweep_idle`/`shutdown_agent`/`shutdown_all`/`discard_spawned_runtime` 在 `shutdown()` 期间均持有 `Arc<AgentRuntime>`，`Weak` 钩子 `upgrade()` 成功，断开不丢。
4. **事件形状**：`node_event`（`node.rs:194-224`）产出 `EventKind::State`、`EndpointEvent::view_only(..., None, None, at)`（无 turn/causation/acp）、view = `{"agentId","state"}`（带 error 时多一 `error` 键且四键与 `common.schema.json#/$defs/publicError` 的 required 一致）；`connected_event`/`disconnected_event` 分别传封闭词表唯一取值 `connected`/`disconnected`，与 `event-views.schema.json` 的 `enum` 及 `views.rs:61-62` 的 `VIEW_ENUMS` 一致。单测 `connected_event_is_node_level_and_uses_the_closed_state`、`disconnected_event_carries_the_error_only_when_present` 覆盖。
5. **不经过 session 的 EventSink 路由**：节点级事件经独立的 `NodeEvents` → `send`（`node.rs:62-66`）投递，从不进入 `AcpSession::emit`/`session.sink`。会话级路径只发 `update_events`/`permission_event`/`elicitation_event`/`turn_event`（`session.rs`）。与 D6「不得把节点级事件错误归属到某个会话」一致。

## C. R21 是「核实」而非「新写」——**通过**

`git diff 33040d78..c63e199 -- crates/agent-host/src/host.rs` 的 hunks 只落在 `AgentRuntime` 字段/构造、`ensure_runtime_tracked`、`on_exit`、`AgentHost` 字段/构造/`with_node_events`/`set_node_events`/`node_events_bound` 以及 `read`/`write` 锁辅助函数；**`impl AgentCatalog for AgentHost` 与 `impl SessionBackendFactory` 的 `agents()`/`agent_capabilities()` 均未被触碰**（hunk 从 `ensure_runtime_tracked` 结束后直接跳到 `lock` 辅助函数处，行号 942）。逐字核对 target 版本：目录派生仍为 `self.config.profiles()` → `AgentDescriptor::new(agent_ref(&profile)?, available, ResourceOrigin::Local)`；可用性判定仍**只**由 `command_resolves(profile.command()) && self.credentials.resolve_env(&profile).await.is_ok()` 得出（`host.rs:616-634`），与基线逐字相同。全 crate 无第二条并行目录/可用性派生路径。符合 R21「既有，未改动」与 plan WP4 行「确认复用既有目录派生路径，不重复实现」。

## D. 越界未验证项的归属——**部分不成立，报 MAJOR（F1）**

作者称「节点级事件的端到端落库/投递」不属于 WP4、由组合根负责。**归属判断成立**（该接线确不在 WP4 的 Write Scope），但**结论不能到此为止**：在 target revision 上核实（全仓库）：

- `NodeEvents` 默认 `unbound`（`host.rs:273`），其 `send` 静默丢弃（`node.rs:62-66`）。
- **除 `crates/agent-host/tests/session.rs` 外，全仓库没有任何生产代码调用 `with_node_events`/`set_node_events`/`NodeEvents::new`**（`grep -rn` 全 crates/ 实测）；`crates/app/src/compose.rs:219` 的 `AgentHost::new(...)` 仍未接线。
- WP3 的 `Broker::node_submit`/`commit_node_event`（`.worktrees/wp3/crates/core/src/broker.rs:628/640`）**只在 WP3 自己的单测里被调用**，生产装配点（组合根）同样未接。

后果：MU2 合入后，任何真实 profile 进程的连接/断开都会被 `NodeEvents` 丢弃于 core 之前，`core-derived-events` R8 的「该节点持久化一次会话标识为空的连接事件」在本变更内**不可满足**（spec 仅豁免**投递/广播**，未豁免**持久化**）。且 `crates/app/` 在 `plan.md` 的 Write Scope 与 Shared File Ownership 中**没有任何所有者行**，`plan.md` 的 AC1（`:996`/`:1029`，主分支 MU2 收口）恰恰断言这条持久化——即该门禁**必须**依赖一次无人登记的 `crates/app/src/compose.rs` 改动才能通过。这与本仓库的先例一致（DDR r1 的 F5 对「spec 要求在本变更内不可满足」即按 MAJOR 报出）。因此按派发单「若是前者，按 MAJOR 报出」记录为 **F1（MAJOR）**；修复责任在组合根/main（接线或新开小包），**不是** WP4 的代码缺陷（见 Assessment）。

## E. 作者自报的开放问题——**不与 spec 冲突**

作者的问题：`NodeEvents` 共享可变，接线可能在首次 spawn 之后才发生，而已发生的 connect 不补报；若产品要回溯连接状态需先定义补报规则。

判断：**spec 未约束**，且「不补报」是 spec 的必然推论而非偏离。R8 要求「进程首次建立并可服务会话时上报一次连接」「复用不重复上报」，即事件锚定在**进程建立时刻**、每进程恰好一次；对「接线上线后为**既有**进程补一条 connected」会凭空产生第二条连接，直接违反「恰好一次」与「复用不重复上报」。同时 spec 已明言「这些事件当前没有投递通道……客户端 MUST 把缺失的覆盖层呈现为状态未知」——即「事件尚未到达客户端」是本次变更**已接受**的中间态，spec 未要求「接线即补齐历史」。结论：不构成发现；若产品确需回溯，应作为**后续变更**的显式补报规则，而非本次实现缺陷。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| review-w4-r1-F1 | MAJOR | `crates/agent-host/src/host.rs:273,282-290`（接线口）；缺口的落点 `crates/app/src/compose.rs:219`（不在本 diff/Write Scope 内） | 全仓库 grep 核实：`with_node_events`/`set_node_events`/`NodeEvents::new` 的**生产调用点为 0**（仅 `tests/session.rs`）；`NodeEvents` 默认 `unbound` 且 `send` 静默丢弃；WP3 的 `Broker::commit_node_event` 生产调用点同样为 0。触发：真实 Daemon 中任一 profile 进程建立/退出。预期：事件经 core 节点级入口持久化一次（R8、D6）。实际：事件在 core 之前被丢弃，持久化不发生。另核实 `crates/app/` 在 plan.md 的 Write Scope 与 Shared File Ownership 中无所有者行，而 main 的 AC1（plan.md:996/1029）断言该持久化。 | 该变更自述的交付物（产生与持久化）在合入 MU2 后为惰性；`core-derived-events` R8 的持久化场景在本变更内不可满足；AC1 门禁必须依赖一次无人登记的 `crates/app/src/compose.rs` 改动才能通过。非 WP4 代码缺陷（WP4 的注入接缝正确且完整），而是一次未被任何工作包认领的集成步骤。 | 在 MU2 收口前由 main 把 `AgentHost::new(...)` 改为 `.with_node_events(NodeEvents::new(sink))`（`sink` 包 `Broker::commit_node_event`），并在 plan.md 把 `crates/app/src/compose.rs` 登记为显式所有者/小包；或在 AC1 中对同一断言说明接线来源。 | 不适用（本轮新发现） |
| review-w4-r1-F2 | MINOR | `crates/agent-host/src/node.rs:50-56` | 作者在 `A.2` 明确「与 WP3 接口已互确认」并称节点级 sink 由组合根注入；WP3 为此在 `crates/core/src/ports.rs:1117-1138` 新增了专用类型 `NodeEventSink`（其文档写明唯一消费者是 `Broker::commit_node_event`，单独成型的目的是让「节点级事件不得进入会话槽位」在**类型上**可见），WP3 的接缝报告（`deliver-wp3-r1.md`）亦声明 `AgentHost` 接收的是 `NodeEventSink`。但 WP4 的 `NodeEvents { sink: Option<EventSink> }` 复用了通用的会话 `EventSink` 类型（`acp_core::ports::EventSink`），刻意新增的 `NodeEventSink` 在仓库中**无任何消费方**（全仓库 grep 为 0）。 | 无运行时失败（两者都是 `Arc<dyn Fn(EndpointEvent)>` 的包装）。影响限于跨包接缝：WP3 在类型层编码的「节点级 ≠ 会话级」区分被削弱为约定，组合根必须手工在 `NodeEventSink` 与 `EventSink` 之间做适配，且 `NodeEventSink` 目前是死类型。 | 令 `NodeEvents::new` 接受 `acp_core::ports::NodeEventSink`（其形状与本类一致），使接缝与 sibling 包的冻结契合同型。 | 不适用（本轮新发现） |
| review-w4-r1-F3 | SUGGESTION | `crates/agent-host/src/mapper.rs:360-369` | 每个 diff 元素携带 `raw`（`object.insert("raw", Value::String(element.to_owned()))`），其为元素在原文中的逐字节切片副本；而 `payload.acp` 已逐字节保存整份文档。WP3 的消费方 `derive.rs` 只读 `path`/`oldText`/`newText`（全仓库 grep 无任何消费方读取 `"raw"`）。触发：`params.update.content[]` 含较大 diff 的工具调用。 | 当前无失败路径（`server::sync` 未实现，`owned_event.payload_json` 无上限）。同一批字节被携带两次（`payload.view` 内转义一次 + `payload.acp` 一次），近 1 MiB 的工具调用会使 view 逼近同量级；待同步出站面落地后可能顶到 §14 的 1 MiB `maxMessageBytes`。 | 若无消费方/断言依赖 `raw`，可移除；若保留（作者以之直接断言「逐字节」），建议在发出前做尺寸核算或在文档登记该双份体量的取舍。 | 不适用（本轮新发现） |

## 未验证内容（明确列出）

- **节点级事件的端到端落库/投递**：本包不跑（属 AC1/WP3）；且经 F1 核实当前无生产者接线，故即便跑也只能由测试自身手工接线，不能证明生产路径。
- **真实 Daemon 路径**（`crates/app/tests/node_link_e2e.rs`）：属 AC1，本包不跑。
- **真实 Agent 的 Diff 元素形态多样性**：本包用例都来自 fake ACP child 的 `chunked-updates`；真实 Agent 用 `x-deserialize-default-on-error` 降级（如 `oldText: null`、多元素、超长正文）未与真实 Agent 对拍（DDT 口径下 `oldText:null` 按「缺席」处理，单测覆盖缺键分支）。
- **`clients/app`**：尚未落地，与本包无关。
- **`deps`/`advisories`/`secrets` 三个 CI-only job**：本地无等价物，未执行亦未声称通过。
- **WP3 实现本体**：由 review-3 并行检视；本轮只在跨包接缝（`diff` 键消费、`NodeEventSink`、`commit_node_event`）处读其代码作为消费方证据。
- **PV1/PV2 的独立复跑**：Reviewer 只读消费作者日志，未独立执行；作者已如实登记 `crates/app/tests/daemon_lifecycle.rs` 的已知 flake（5 次取证 + 完整重跑全绿），本报告不将其计入本包缺陷。

## Assessment

- **A（Diff 逐字节保真，R22/D4）**：通过。`raw` 来自原文字节切片、转义/嵌套/多元素/多字节均正确、`payload.acp` 逐字不变、无类型化 Diff 时键缺席、无 `rawInput` 读取路径；「恒为数组」与「键缺席」不矛盾。
- **B（进程生命周期，R8/D6）**：通过。每进程实例恰好一次 connected（含同 profile 多次 spawn）、断开恰好一次且不早于退出判定、无「已断开仍存活」窗口、节点级事件形状与封闭词表正确且不经 session 的 `EventSink`。
- **C（R21 核实）**：通过。`agents()`/`agent_capabilities()` 逐字未改，可用性仍为 `command_resolves && credentials.resolve_env`，无第二条并行路径。
- **D（越界未验证项归属）**：归属判断成立，但缺口真实存在且无登记所有者 → **F1（MAJOR）**。
- **E（作者开放问题）**：不与 spec 冲突，「不补报」是「恰好一次」的必然推论。

**结论：FAIL（review-w4-r1-F1）**。

判定依据：按派发单，存在「事件发不出去」的功能缺口时按 MAJOR 报出，并以未解决的 MAJOR 判 FAIL。需特别说明：**F1 不是 WP4 的代码缺陷**——WP4 的注入接缝（`with_node_events`/`set_node_events`）、生命周期状态机、Diff 透传与节点级事件形状在本轮全部通过；F1 的修复责任在组合根/main（在 `crates/app/src/compose.rs` 接线，或把该改动登记为有主的包/验收步骤），且与 `plan.md` 的 AC1 门禁直接相关。若 main 判定该接线已由 AC1 明确归口自身并承诺在 MU2 收口前完成，可据本报告降为集成待办并将 F1 关闭；在此之前，本工作包以其自身交付范围计**通过**，但 MU2 会因 F1 无法满足 R8 的持久化场景。

F2（MINOR）与 F3（SUGGESTION）为非阻断项，不影响本结论。

本结论绑定 Target Revision `c63e199dded5cd735054b899c5a2ecede418df11`。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md，不代替 main 判定整体变更可归档。
