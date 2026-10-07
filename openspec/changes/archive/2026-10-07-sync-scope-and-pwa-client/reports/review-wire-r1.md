<!-- 组合根接线提交（MU2 收口）独立检视，第 1 轮。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "4.1"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wire-r1"
  isolation: "fork_turns=none（新建独立子 Agent；未参与该接线的实现、未继承 WP3/WP4 实现者或任何 reviewer 的对话）"
base_revision: "559f21c"
target_revision: "0cb5e62b1941d97c685924fcbab87cae07c37d4f"
scope: "仅 559f21c..0cb5e62 的接线 diff（2 files / +414 / -4：crates/app/src/compose.rs、crates/app/tests/node_link_e2e.rs），外加检查是否引入回归。不重复 WP3/WP4 各自已通过的检视（review-w3-r2 / review-w4-r2）。"
changes: "只读检视，未修改任何文件；未切换分支、未提交、未合并、未运行任何编译/测试/E2E。仅新增本报告（review-wire-r1.md）。"
issues: "0 CRITICAL / 1 MAJOR（review-wire-r1-F1：AC1 只覆盖四项断言中的两项，plan.md 的 AC1 判据仍不成立）/ 2 MINOR（F2 注释事实错误、F3 关闭预算与错误分类）"
result: FAIL

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wire-r1` / 1（该提交的首个检视轮） |
| Review Type / Stage | branch（接线提交，合入 MU2 候选之前） |
| Work Package | MU2（接线提交本身不属于 WP3/WP4 任何包） |
| Repository / Worktree | `D:\Project\acp-remote`；检视 worktree `D:\Project\acp-remote\.worktrees\integ`（HEAD `0cb5e62`，`git status --porcelain` 为空，只读） |
| Base Revision | `559f21c`（集成 WP4 到 WP3 之上的合并点，接线之前，已 `git rev-parse` 核对） |
| Target Revision | `0cb5e62b1941d97c685924fcbab87cae07c37d4f` |
| 实际修改文件 | `crates/app/src/compose.rs`、`crates/app/tests/node_link_e2e.rs`（`git diff --stat 559f21c..0cb5e62` 全量核对，2 文件）；`crates/core/**`、`crates/agent-host/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**` 零改动；`Cargo.toml`/`Cargo.lock` 零改动（无新依赖） |
| 读取的规则与需求 | `specs/core-derived-events/spec.md`（「Agent 连接状态是节点级生命周期」及 5 个 Scenario、「会话标题只由 Agent 通知更新」及 5 个 Scenario、「文件改动事件由类型化 Diff 派生」组）、`specs/local-agent-host/spec.md`（R8 行 :26-36）、`plan.md`（Coverage Index R018–R041；Project Verify 的 AC1 行 :996；替代检查表 :1029；Completion Criteria :1048）、`tasks.md`（4.1、8.1）、`AGENTS.md` §3/§4/§7/§9、`design.md` D6、`docs/CORE_PORTS_AND_STORAGE.md` §5/§6 |
| 验证证据（只读消费） | `reports/wire-compose-nodeevents-r1.md`、`reports/AC1.log`（4 passed / AC1_EXIT=0）、`.target-wt/mu2int-pv1.log`（main 复跑 PV1 exit 0）。**作者自评不作为检视证据**；reviewer 未执行任何编译/测试/E2E |
| 限制 | 不判「用例设计是否最优」；不重跑 Project Verify；不执行 E2E；不评审 WP3/WP4 已通过部分 |

---

## A. 接线是否真的解除了「静默丢弃」——**通过（有保留）**

**结论**：是。三条判据逐条成立：

1. `host.node_events_bound()` 在实际组合中为 **true**。`assemble` 在 `Broker` 构造之后、`Ok(Self {..})` 返回之前调用 `host.set_node_events(...)`（`.worktrees/integ/crates/app/src/compose.rs:307-312`），而 `AgentHost::new` 的默认值是 `NodeEvents::unbound()`（`crates/agent-host/src/host.rs:282`），`set_node_events` 是 `&self` 写锁（`host.rs:296-299`），`node_events_bound` 读同一把锁（`host.rs:302-305`）——**同一实例**，因此生产装配路径上恒为 true。AC1 用例把它作为前置断言（`crates/app/tests/node_link_e2e.rs:1177-1180`）。`AgentHost::new` 在生产代码中只有这一个调用点（`compose.rs:240`），故不存在「另一条装配路径仍 unbound」。
2. `send` 的 `Err` **并非不可能**，但其唯一来源已从生产装配路径上消除。`NodeEvents::send` 只在 `sink.is_none()` 时返回 `Err`（`crates/agent-host/src/node.rs:98-113`），接线后该分支不可达。残余的两条路径都会**显式记录**，不静默：
   - `Weak::upgrade` 失败 → `tracing::warn!(daemon.node_event_commit_skipped)`（`compose.rs:945-954`）；
   - `commit_node_event` 返回 `Err` → `tracing::error!(daemon.node_event_commit_failed, reason = port_error_token(&error))`（`compose.rs:955-963`），`PortError` 的成因分类由 `port_error_token` 收敛（`compose.rs:129` 起），不泄漏内层 SQL/路径文本。
3. 新增代码**没有吞错**：`compose.rs` 全文中新增区域的唯一 `let _ =` 是 `futures_join` 里对 `JoinHandle::await` 结果的忽略（`compose.rs:529-533`）——`JoinHandle<()>::await` 的 `Err` 只表示任务 panic 或 abort，而任务体的 `Err` 已在任务内被 `tracing::error!` 记录，不构成吞错。

**保留**：`report_disconnected` 在「未判定退出」时返回 `Ok(())` 且不产生事件（`crates/agent-host/src/node.rs:340-352`），这是 WP4 已交付并被 `review-w4-r2` 复核过的既有语义，不是本 diff 引入，按边界不重复计入。

**复核依据**：`compose.rs:307-312`、`host.rs:282/296-305`、`node.rs:98-113`、`compose.rs:939-965`、`compose.rs:529-533`、`node_link_e2e.rs:1177-1180`。

---

## B. 同步→异步桥接——**通过（`Handle::current()` 无 panic 路径；一处预算问题见 F3）**

### B.1 `Handle::current()` 是否总能拿到——**是，构造性地检查了全部调用栈**

`Handle::current()` 在**没有** runtime 上下文的线程上会 panic。逐条核对：

- **生产路径**：`Composition::assemble` 在 `crates/app` 中只有一个调用点 `daemon.rs:1157`，在 `async fn run` 内；`run` 在 `crates/app` 中只有一个调用点 `cli.rs:664`，由 `tokio::runtime::Builder::new_multi_thread()` 的 `block_on` 驱动（`cli.rs:659-664`）。因此从 `daemon start` 到 `assemble` 的整条栈都在 runtime 上下文内，`Handle::current()` 必然成功。`Handle::current()` 在 `assemble` 内**只调用一次**并把 `Handle` 值捕获进闭包（`compose.rs:307`），闭包后续不重复取——这消除了「闭包在非 runtime 线程被调用」这一路径上的顾虑。
- **测试路径**：`compose.rs` 自身 4 处 `#[tokio::test]`（`:1101/:1250/:1275/:1327`）与 AC1 用例（`support::block_on`，`crates/app/tests/support/mod.rs:637-643` 用 `new_current_thread().enable_all().block_on`）都在 runtime 内。
- **闭包的实际调用线程**：`NodeEvents` 的出口只在 `AgentRuntime::report_connected`/`report_disconnected` 被调用（`crates/agent-host/src/host.rs:106-136`）。这两处的调用点包括 `spawn_router` 的任务体（`host.rs:576-584`，`supervisor.spawn_owned` → `tokio::spawn`，`process.rs:492-498`）与 `Supervisor` 的退出监视任务（`process.rs:231-242`，`tokio::spawn`），**全部在 async worker 上下文**。在 runtime 上下文里 `Handle::current()` 与 `Handle::spawn` 都安全；作者明确未用 `Handle::block_on`（那才会 panic）。**结论：该文件中不存在可构造的 panic 路径。**
- 一处**不属于本 diff 的范围外观察**（仅登记，不计为发现）：`Composition::assemble` 是 `pub` 且被导出，任何外部调用方若在非 runtime 上下文调用它，`Handle::current()` 会 panic。仓库内无此调用点，且原有 `SqliteStore::open` 已使 `assemble` 实际上必须异步驱动，故不构成本 diff 引入的可证明缺陷。

### B.2 `spawn` 的 future 是否被跟踪——**是**

每个提交任务的 `JoinHandle` 立即登记进 `node_event_tasks`（`compose.rs:955-968`），并由 `Composition::close` 在关池前排空（`compose.rs:494-517`）。`retain(!is_finished)` 同时回收已结束的句柄，避免长跑进程里列表无界增长（`compose.rs:964-967`）。登记与 `close` 的取用共用同一把 `std::sync::Mutex`，不存在「登记丢失」。

### B.3 `close` 的排空与 `NodeEventsPending` 是否显式失败——**是**

`close` 在 `Arc::try_unwrap(store)` **之前**循环：取走全部未结束句柄 → `tokio::time::timeout(remaining, futures_join(pending))`（`compose.rs:494-517`）。超时或剩余预算为零时 `return Err(ComposeError::NodeEventsPending)`（`compose.rs:510-516`），**不静默继续**，也不跳过检查点（`Arc::try_unwrap` 在屏障之后，`compose.rs:518`）。循环形态（`pending.is_empty()` 才 break）正确覆盖「sink 在等待期间又登记了新任务」的情形。`NodeEventsPending` 已接入 `ComposeError::message()`（`compose.rs:121-123`），无遗漏的 match 臂（`message` 是穷尽 match，新增变体会被编译器强制覆盖）。

**复核依据**：`cli.rs:659-664`、`daemon.rs:1157`、`compose.rs:202/307/307-312/494-533/939-968`、`host.rs:106-136/576-584`、`process.rs:231-242/492-498`、`support/mod.rs:637-643`。

---

## C. 强引用环的修复——**彻底；但 upgrade 失败路径只记 `warn`（见 §D 总评与复核项）**

**结论**：环确已断开，且断得彻底。

- 环的构成是 `host → NodeEvents → sink 闭包 → Broker → backends(host)`。本 diff 让闭包持 `Weak<Broker>`（`compose.rs:309` 的 `Arc::downgrade(&broker)`，`compose.rs:940` 的 `Weak<Broker>` 字段），**唯一**的强引用边被改弱。
- 全量核对 `broker` 的强持有者：`Composition::broker`（`compose.rs:183`）、`UseCases`（经 `UseCaseDeps.broker`，`compose.rs:268`）。两者都在 `close` 的 `drop(broker)` / `drop(use_cases)` 处释放（`compose.rs:485-486`），并都先于 `Arc::try_unwrap(store)`。`node_event_tasks` 只持有 `JoinHandle<()>`（任务体只捕获 `Weak` 升级出的**当时**强引用，任务结束即释放），不构成新的强环。因此 `close()` 不再报 `StoreStillShared`——AC1 用例的 `composition.close().await.expect("组合根可关闭")`（`node_link_e2e.rs:1299`）是该路径的实测证据。
- **残余的升级失败路径**：`broker.upgrade()` 失败时记 `tracing::warn!(daemon.node_event_commit_skipped)` 并 `return`（`compose.rs:945-954`）。按 A 的判据，「投递失败路径留下了可观测错误，没有吞错」**成立**。但等级是 `warn` 而非 `error`：正常关闭序列里该分支不可达（`Broker` 在 `host` 之后释放，`close` 的屏障在两者之前），一旦可达就意味着一整条 R8 事件永久丢失。作者的注释自陈该分支「此时没有需要落库的提交通道」。**判为可接受但不理想**——不作为独立发现（与 A 的「不静默」判据不冲突，且仓库对该语义的既有基线是 `warn`，例如 `NodeLinkPublisher` 的队列满）。作为**建议**（不计入发现计数）：将该等级提升为 `error`，与 `daemon.node_event_commit_failed` 对齐。

**复核依据**：`compose.rs:183/268/309/485-486/518/940/945-954`、`node_link_e2e.rs:1299`。

---

## D. AC1 的断言是否可证伪——**部分；但覆盖不达 plan 判据（F1）**

### D.1 断言绑定的是可观察结果，非内部实现——**是**

新增用例（`node_link_e2e.rs:1155-1313`）的落库断言**直接读真实 SQLite 文件**：`owned_event_rows` 用 `SqliteConnectOptions::new().filename(<data_dir>/acp-remote.sqlite3).read_only(true)` 直连并 `SELECT event_id, COALESCE(session_id,'<null>')`（`node_link_e2e.rs:1103-1123`），绕过任何 `Broker` 内存状态；主断言另用一条 5 列查询读 `session_id/session_sequence/origin_epoch/origin_sequence`（`:1224-1243`）。这满足 plan.md 的「落库后**可按事件库读回**」。装配面是真组合根 `app::compose::Composition` + 真实 fake ACP 子进程（`:1169-1175`）。

### D.2 断言不会恒真——**是，且已有反向对照**

关键断言全部是**存在性/计数/字段值**断言，不是「没报错」：`rows.len() == 1`（`:1246-1251`）、`session_id.is_none()`（`:1253-1256`）、`session_sequence.is_none()`（`:1257-1260`）、`origin_epoch/origin_sequence.is_none()`（`:1261-1264`）、`!seen.contains("agent.*")` 且 `seen.is_empty()`（`:1282-1290`）、`disconnected.len() == 1` 且 `session_id == "<null>"`（`:1303-1311`）。去接线后 `node_events_bound()` 前置断言（`:1177-1180`）先行失败，作者已在 `reports/wire-compose-nodeevents-r1.md` §3.3 记录该反向对照实测 FAIL。**该用例确实可证伪。**

### D.3 「不被会话级投递路径误收」断言是否绑定了消费者路由——**是（这恰是跨边界检查的正确做法）**

断言读的是组合根**实际安装**的 `forked_publisher()` 扇出接收端（`:1170-1175`），消费者丢弃点确在 diff 之外：`crates/server/src/node_link/resource.rs:1024-1032` 的 `NodeLinkPublisher::publish` 对 `CommittedDelivery::Owned` 且 `event.session.is_none()` 早退。生产者侧（`commit_node_event` 以 `session: None` 提交、随后 `self.publish(&outcome.appended)`，`crates/core/src/broker.rs:686-704`）与消费者侧的丢弃分支**一致**，未发现静默丢弃/错误归类。

### D.4 四项断言是否覆盖 R8 全部要求——**不覆盖，且缺的两项属 plan 的 AC1 判据（F1）**

`tasks.md` 4.1 与 `plan.md` 的 AC1 行（`:996`、`:1029`）明文要求**四项**断言：`file.changed`（会话级）经 owned 路径落库并按 origin 顺序回放、`agent.connected` 落库后会话标识为空且不被会话级投递路径误收、`session_info_update` 后会话标题更新。详细分析见 **F1**。

**复核依据**：`node_link_e2e.rs:1103-1123/1155-1313`、`resource.rs:1024-1032`、`broker.rs:686-704`、`plan.md:996/1029`、`tasks.md:62`。

---

## E. 边界与业务规则（`AGENTS.md` §4「`app` 是组合根…但不能承载业务规则」）——**通过**

逐项核对新增代码只做装配：

- 事件语义判断（`event_type` 词表、`state` 取值、turn/causation/acp 为空、`agentId` 非空）全部在 core 的 `Broker::commit_node_event`（`crates/core/src/broker.rs:640-700`），`app` 的 sink 闭包**不做**任何校验或分类；
- 错误分类只在两处稳定 token：`port_error_token`（`compose.rs:129` 起，既有函数）用于日志；
- 重试策略：**无**。`commit_node_event` 失败即记 `error`，不重试、不降级、不写入替代路径（`compose.rs:955-963`）；
- 会话/节点归属：闭包只 `Arc::downgrade`/`upgrade` 与 `spawn`，不构造会话标识、不伪造游标（`compose.rs:939-968`）；
- 关闭策略（5s 上限、显式失败）属**装配生命周期**，不是业务规则。

未发现夹带业务规则。

**复核依据**：`compose.rs:129/939-968`、`broker.rs:640-700`、`AGENTS.md` §4。

---

## F. 另需判断的一项：AC1 缺的两项是否必须在本 MU2 合入前补齐

**结论：必须。这两项不能在「AC1 只覆盖一半」的情况下判定 MU2 闭环。**

理由（三条，全部可核对）：

1. **plan 的判据是合取，且未按子项豁免。** `plan.md:996`（Project Verify 的 AC1 行）与 `:1029`（替代检查表）都写「…四项断言全绿」；唯一被明文排除的是**广播断言**（「不含广播断言：节点级事件当前无投递通道，见 Risks 的 F5 条」）。`file.changed` 与标题两项**不在**豁免范围内。`plan.md:1048` 的 Completion Criteria 要求「`## Checks` 表中…AC1/AC2/AC3 全部 PASS」。
2. **AC1 是本次变更在 Main E2E 上的替代检查，无兜底。** `plan.md:1005-1012` 的 Main E2E 为 `not-applicable`，替代检查即 AC1/AC2/AC3（`:1019`）。`tasks.md:168`（任务 8.1）把「4.1–4.3 三条替代检查均已 PASS 且证据可读」列为后续门禁。若 AC1 只覆盖一半，被替代的真实 Daemon 路径就没有任何检查覆盖 `file.changed`/标题的**端到端可读回**。
3. **其余覆盖都是分层内的，不构成 AC1 的等价物。** WP3 的 core 行为测试确实覆盖了 `file.changed` 派生/行数/越界（`crates/core/src/broker.rs:9001-9236`）与标题单向更新（`:9337-9400`，含「缺 title 不改」「显式置空」「客户端无重命名入口」），PV2 亦 PASS。但 `plan.md` 的 Coverage Index 把那两类需求（R018–R029、R036–R041）**同时**绑到 `[PV1, PV2, AC1]` 三个检查——`AC1` 被单独列出正是因为它是唯一经**真实组合根 + 真实 SQLite + 真实 fake ACP 子进程**的持久化/读回证据；core 单测用的是 in-process harness，不驱动 `Composition`，与 AC1 的证据面不同。

**驳「可留给 TP2」的两条事实**：TP2 的契约（`plan.md:868`）是「core 生产者行为…`crates/core/src/**/tests.rs`、`crates/storage-sqlite/tests/`」——TP2 的写入范围**不含** `crates/app/tests/`，其验证对象是 core 层而非组合根；且 TP2 的立项理由是补 TP1/WP3 的**行为测试**，不是替代检查 AC1。

**候选/合入执行风险（与本判定并列）**：`plan.md:971` 的 MU2 候选检查只有 `PV1, PV2`，**不含 AC1**。因此若以「AC1 只覆盖一半」判定 MU2 闭环，这半截证据**不会**被任何预合入门禁自动拦住，只能靠本检视拦下。这是把 F1 定级为 MAJOR 的决定性理由。

**可行路径（任一即可，均不改变本质结论）**：① 本提交内补两项断言——fake ACP Agent 已有现成素材：scenario `chunked-updates` 会连续发 `tool_call`（带 `type:"diff"` 内容、`path:"src/main.rs"`）与 `session_info_update`（带 `title:"会话标题"`）（`crates/agent-host/src/bin/acpr-fake-acp-agent.rs:575-680`），只需把 `ac1_config` 的 `--scenario normal`（`node_link_e2e.rs:1131`）换成该场景并加两组读库断言；② 或由 main 以**显式书面例外**修订 `plan.md:996/:1029` 的 AC1 判据（把缺的两项移出 AC1），再判定闭环——但该例外目前不存在。

**复核依据**：`plan.md:868/971/996/1005-1012/1019/1029/1048`、`tasks.md:62/168`、`core/src/broker.rs:9001-9236/9337-9400`、`acpr-fake-acp-agent.rs:575-680`、`node_link_e2e.rs:1131`。

---

## Findings

### review-wire-r1-F1（MAJOR）AC1 只覆盖四项断言中的两项，plan 的 AC1 判据仍不成立

**位置**：`crates/app/tests/node_link_e2e.rs:1155-1313`（新增用例整体）；判据来源 `openspec/changes/sync-scope-and-pwa-client/plan.md:996`、`:1029`；门禁后果 `plan.md:971`、`tasks.md:168`。

**触发条件**：以本提交作为 MU2 候选或合入 `main`，并依据 `plan.md:996/:1029` 声称 AC1 PASS。

**预期**：AC1 为四项断言全绿——① `file.changed`（会话级）经 owned 路径落库并按 origin 顺序回放；② `agent.connected`（节点级）落库后会话标识为空、可按事件库读回、不被会话级投递路径误收；③ `agent.disconnected` 同形；④ 会话标题在 `session_info_update` 后更新。证据 `reports/AC1.log`。

**实际**：新增用例只实现 ②③ 及其四个字段/路径断言（`:1246-1311`）。①④ 在 `crates/app/tests/` 中**零覆盖**——对 `crates/app/tests/` 全量检索 `file.changed|addedLines|outsideWorkspace|session_info_update|title` 均无命中（唯一命中是 `:1058` 的一句注释）。AC1 仍不满足 plan 的判据，因此**不能判定 MU2 闭环**。

**影响**：本次变更以 Main E2E `not-applicable` 降级（`plan.md:1005-1012`），AC1 是替代检查（`:1019`），且 MU2 候选检查只有 PV1/PV2、不含 AC1（`:971`）——这半截证据不会被任何预合入门禁自动拦下，只由本检视发现。被替代的真实 Daemon 路径上，`file.changed` 与标题更新的**端到端可持久化/可读回**在合入前无任何证据。WP3 的 core 单测虽覆盖同一语义，但其证据面是 in-process harness，且 `plan.md` 的 Coverage Index 把 R018–R029/R036–R041 同时绑定 `[PV1, PV2, AC1]`，AC1 被单列正因证据面不同（详见 §F）。

**建议**：在本提交内补两项断言后重跑 AC1（首选，无需新素材）——把 `ac1_config` 的 scenario 由 `--scenario normal` 改为 `--scenario chunked-updates`（该场景已发带 `type:"diff"` 的 `tool_call` 与带 `title` 的 `session_info_update`，见 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs:575-680`），再加两组断言：读 `owned_event` 中 `event_type='file.changed'` 的行按 `global_sequence`（origin 顺序）恰为 1 条且 `session_id` 非空；读 `owned_session.title`（或经 `session.read`/摘要投影）等于通知中的取值。若判定不应在本提交内补，则须由 main 以显式书面例外修订 `plan.md:996/:1029` 的 AC1 判据，并把缺项移入有明确所有者与检查的任务（注意 TP2 的写入范围不含 `crates/app/tests/`，见 `plan.md:868`）。

### review-wire-r1-F2（MINOR）新增注释声称本文件其余用例已覆盖 Diff/标题场景，与事实不符

**位置**：`crates/app/tests/node_link_e2e.rs:1058-1059`。

**触发条件**：任何读者（含后续 AC1 补测的执行者）依据该注释判断「Diff/标题已在本文件被脚本化后端覆盖」，从而不再补测。

**预期**：注释描述的文件内既有覆盖为真。

**实际**：该注释写「真实 Agent 的 Diff/标题场景…本文件其余用例已用脚本化后端覆盖；本节**不**重复」。但本文件其余三条用例（`the_controlled_path_runs_end_to_end_and_revocation_propagates`、`a_narrowing_repair_closes_the_live_attachment`、`tls_direct_terminates_the_same_handshake`）的驱动面是 `EnvBuilder`-style 的 `publish_prompt`，其 `OwnerNode`/`nodelink` 支持层全文检索 `diff|title|file.changed|session_info_update` **零命中**，即它们不产生 Diff 工具调用、也不产生会话信息更新通知。结论：本文件对 Diff/标题**无任何**覆盖（F1 的同一事实）。

**影响**：这是一句**反向不实**的注释——它把 F1 的缺口描述成已被覆盖，会误导补测决策；且 `AGENTS.md` §7 要求文档/注释与实现一致，`plan.md` 的 Completion Criteria 要求「无未解释的缺口」，该注释会使缺口看起来已被解释。

**建议**：删去 `:1058-1059` 的两行「已知边界」句，或改写为如实陈述：本文件其余用例用脚本化后端、不产生 Diff/标题事件，该两项在 `crates/app/` 层当前无覆盖。

### review-wire-r1-F3（MINOR）`NodeEventsPending` 的 5 s 屏障未纳入 daemon 关闭预算，且被归入与 `StoreStillShared` 同一错误码

**位置**：`crates/app/src/compose.rs:494-517`（5 s 屏障）；`crates/app/src/daemon.rs:1476`（`DaemonError::StoreClose` 包装）；`crates/app/src/daemon.rs:186`（错误码映射）。

**触发条件**：`agent.disconnected` 的提交任务在 5 s 内未完成（例如 SQLite 在关闭期持锁、写入被延迟）。此时 `close()` 返回 `Err(NodeEventsPending)`。

**预期**：关闭序列的既有节奏是「`deadline = started + max(grace_ms, MIN_DRAIN_MS)`」的**单一时限**预算（`daemon.rs:1409` 起），接入层排空、后台任务取消、`host.shutdown_all()` 都按 `deadline` 收敛并在超时后记日志**继续**关闭（例如 `shutdown_all` 超时走 `daemon.agent_stop_timeout` 分支后仍继续，`daemon.rs:1457-1470`）；`Drop` 守卫 `ShutdownCleanup` 再无条件完成收尾。`ComposeError::StoreStillShared` 的既有语义是「`Arc::try_unwrap` 失败 ⇒ 检查点未执行」。

**实际**：本 diff 在 `daemon.rs:1476` 的关闭路径上新增了一条**计划外、可长达 5 s 的阻塞**，且它不经 `deadline` 裁剪。若该屏障超时，(a) `Composition::close` 提前返回 `Err`，`wal_checkpoint(TRUNCATE)` **不再执行**（屏障位于 `Arc::try_unwrap` 之前，`compose.rs:494/518`）；(b) 该 `Err` 经 `DaemonError::StoreClose` 与 `DaemonError::code()` 被映射为 `Unavailable`（`daemon.rs:186` → `daemon.rs:183-187`），与「存储仍被共享」共用同一码与同一条日志路径——运维无法从码或消息区分「存储没关干净」与「一条节点级事件没排空」，而两者的处置完全不同（前者查句柄泄漏，后者查写入延迟）。

**影响**：关闭期的可观测性与预算一致性退化；在极端叠加下（前序 drain 已接近 `deadline`）总关闭时间可超出 `grace_ms` 预期。**不破坏 A/B 的正常路径结论**（正常情形毫秒级完成，且超时是显式失败而非静默丢弃），故定级 MINOR。

**建议**：把 `NODE_EVENT_DRAIN_TIMEOUT` 改为按 `daemon.shutdown_grace_ms`/`deadline` 的剩余预算裁剪，或在 `daemon.rs` 的 `StoreClose` 之前单独识别 `ComposeError::NodeEventsPending` 并记一条独立的结构化事件（含独立 token），使运维能从日志与事件名区分两种关闭失败；现有 `ComposeError::message()` 已提供不同文案（`compose.rs:121-123`），但未被 `daemon.rs` 的关闭日志转述。

---

## 实际检查范围与未验证内容

**实际检查（均已给文件:行）**
- `git diff --stat 559f21c..0cb5e62` 与两个文件的完整 diff（全文阅读，非仅 hunk）；确认改动文件集为 2 个、依赖清单零改动。
- `crates/app/src/compose.rs` 全文（聚焦 `assemble`、`close`、`node_event_sink`、`ComposeError`/`message`）；`crates/app/tests/node_link_e2e.rs` 全文（聚焦 1043-1313）。
- diff 之外的消费/路由面（按跨边界检查要求）：`crates/core/src/broker.rs`（`commit_node_event` 校验与 `publish`）、`crates/server/src/node_link/resource.rs:1024-1048`（`NodeLinkPublisher::publish` 的丢弃点）、`crates/agent-host/src/{host.rs,node.rs}`（`NodeEvents`/`send`/`report_*`/`set_node_events`/`node_events_bound`/`Weak` 退出钩子）、`crates/agent-host/src/process.rs`（任务 spawn 与 `shutdown` 顺序）、`crates/app/src/{daemon.rs,cli.rs}`（关闭序列与 runtime 建立）、`crates/app/tests/support/mod.rs`（`block_on`）。
- 规划/规则面：`plan.md`（Coverage Index、Project Verify、替代检查、Merge Strategy、Completion Criteria）、`tasks.md`（4.1、8.1）、`specs/core-derived-events/spec.md`、`specs/local-agent-host/spec.md`、`AGENTS.md` §4/§7/§9。
- 覆盖度事实核验（用于 F1/F2）：对 `crates/app/tests/` 检索 `diff|Diff|title|session_info|file.changed|addedLines|outsideWorkspace`，以及对 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 的场景表。

**未验证内容**
- **未运行任何编译/测试/E2E/PV**：本检视是只读边界。`reports/AC1.log`、`.target-wt/mu2int-pv1.log`、`reports/wire-compose-nodeevents-r1.md` 仅作只读消费，不作为独立复核证据；A 的「`node_events_bound()` 为 true」与 C 的「`close()` 正常完成」由代码路径与 AC1 断言推断（作者的反向对照未由 reviewer 复跑）。
- **未做**：SQLite `owned_session.title` 的读取路径与 `session_info_update` 的通知映射（WP3 交付面，已由 `review-w3-r2` 覆盖，按边界不重复）；WP3/WP4 已通过部分的复核。
- **未评估**：并发压力下 `node_event_tasks`（`std::sync::Mutex`）的正确性；`Weak` 升级失败路径在真实关闭竞态下的可达性（作者 §D 亦登记未做压力测试）。
- **未判定**：`file.changed` 场景换用 `chunked-updates` 后的具体断言写法是否与其 emit 顺序完全吻合（仅核对了该场景会发出带 Diff 的 `tool_call` 与带 `title` 的 `session_info_update`）。

---

## 结论

**FAIL（review-wire-r1-F1）**

- A（解除静默丢弃）：**通过**；B（同步→异步桥接、`Handle::current()` 无 panic、任务被跟踪、超时显式失败）：**通过**；C（强引用环断开彻底）：**通过**；D（断言可证伪、绑定可观察结果、消费者路由正确）：**通过**；E（只做装配、无业务规则）：**通过**。
- 阻断项是 **F1**：`plan.md:996/:1029` 的 AC1 判据要求四项断言全绿，本提交只覆盖其中两项，且 MU2 候选检查（`plan.md:971` 的 PV1/PV2）不含 AC1，缺口不会被预合入门禁拦下。
- F2（注释不实）与 F3（关闭预算/错误分类）为 MINOR，不单独阻断，但 F2 会误导 F1 的补测决策，建议与 F1 同批修复。
- **是否必须补 F1 的两项断言后才能判定 MU2 闭环：是**（完整论证见 §F）。
