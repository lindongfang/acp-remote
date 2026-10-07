<!-- WP4 交付报告（MU2，第 2 轮 / Attempt 2：F1 缺陷修复）。coder 只交修复实现、局部检查与工作包 Project Verify；不判独立 review、合入与最终验收。 -->

# WP4 缺陷修复（F1）交付（deliver-wp4-r2）

## Shared Report

- **task_id**: 2.4（`phase: fix`，DELIVERY + CHECK）/ 3.8 的复核回环（本报告只交 NEW 证据）
- **work_package**: WP4（交付单元 MU2）
- **role**: coder（`coder-w4-r2`，台账已 claim / ack）
- **agent_context**: `agent_id: coder-w4-r2`，`isolation: fork_turns=none`（新实例接管既有 worktree；未继承 r1 实现者或 reviewer 的对话）
- **target_revision**: `4c4990a3cefe445a36c9df9d5a7f57b572b10618`（分支 `feat/wp4`；父提交 = r1 交付 `c63e199dded5cd735054b899c5a2ecede418df11`）
- **scope**: 只改 `crates/agent-host/src/`（plan 的 WP4 Write Scope）。`crates/core/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**`、`crates/app/**` **零改动**
- **result**: **PASS**。F1 修毕（未接线出口默认拒绝 + 显式告警 + 两条断言）；F3 按裁量保留并说明消费方；F2 按 main 裁决**不在本轮做**（给出精确 patch 供 MU2 收尾）。PV1 exit 0、PV2 exit 0（均在 target revision 上重跑，日志见下）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp4` |
| 分支 | `feat/wp4` |
| r1 交付（本轮父提交） | `c63e199dded5cd735054b899c5a2ecede418df11` |
| **交付提交（target）** | **`4c4990a3cefe445a36c9df9d5a7f57b572b10618`** |
| 提交信息 | `fix(agent-host): WP4 F1 修复——未接线 NodeEvents 不再静默丢弃节点级事件` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\wp4`（独占） |

`git diff --stat c63e199..4c4990a`（4 files changed, **168 insertions(+), 48 deletions(-)**）：

```
 crates/agent-host/src/host.rs      |  21 +++--
 crates/agent-host/src/lib.rs       |   2 +-
 crates/agent-host/src/node.rs      | 157 +++++++++++++++++++++++++++----------
 crates/agent-host/tests/session.rs |  36 ++++++++-
```

对 `crates/core/`、`crates/sync-protocol/`、`schemas/`、`docs/`、`fixtures/`、`crates/app/` **零改动**（逐目录 `git diff --name-only c63e199..4c4990a -- <dir>` 均为 0 行）。

---

## A. F1（MAJOR，必修）—— 未绑定的 `NodeEvents` 不再静默丢弃

### A.1 问题

`reports/review-w4-r1.md` 的 F1：`NodeEvents` 默认 `unbound`，其 `send` 在未接线时**空转**。组合根（`crates/app/src/compose.rs:219`，不在 WP4 Write Scope）未接线 → 节点级事件在 core 之前被丢弃，且**没有任何信号**。这把一个「规划缺口（接线无人认领）」升级成「运行时无声失败」，使 `core-derived-events` R8 的节点级事件持久化不可满足且无人可见。

### A.2 修法（`crates/agent-host/src/node.rs`、`host.rs`、`lib.rs`）

判据：**接线缺失必须在开发期或运行期被看见，不能无声无息**。为此（而非仅加日志）：

1. **新增 `NodeEventError`（`node.rs`）**：单变体 `Unbound`，`Display` + `std::error::Error`。语义写死在类型上——`Unbound` 表示「事件**没有**被交付（也就不会落库）」，绝不是「已交付」。经 `lib.rs` 导出（`pub use node::{NodeEventError, NodeEvents, PublicErrorView}`）。
2. **`NodeEvents::send` 改为返回 `Result<(), NodeEventError>`（原为私有 `fn send(&self, EndpointEvent)`）**：
   - 已接线：`sink.send(event)` 后返回 `Ok(())`；
   - 未接线：`tracing::error!(event_type = .., "节点级事件没有交付通道：组合根未接线 NodeEvents，事件被丢弃（AgentHost::new(...).with_node_events(...) 或 set_node_events(...) 未调用）")` 后返回 `Err(NodeEventError::Unbound)`。
3. **`report_connected` / `report_disconnected` 由 `bool` 改为 `Result<(), NodeEventError>`**（`node.rs`）。语义边界严格保留：
   - 事件构造失败（常量形状不会发生）→ `Ok(())`：没有事件需要交付，就无所谓投递失败；
   - `exited == false`（断开的前置不满足）→ `Ok(())`：同样没有事件；
   - 有事件但出口未接线 → `Err(Unbound)`。
4. **`AgentRuntime` 两条上报路径补运行期上下文**（`host.rs`）：失败时调用新增的 `node::log_undelivered(event_type, agent)`，记一条带 `agent_id` 的 `error` 级日志——`NodeEvents::send` 只有 `event_type`（`NodeEvents` 不持有 Agent 标识），这里补上「哪个 profile 进程」，让漏接在运维侧可定位。
5. **文档同步**：`node.rs` 模块头、`NodeEvents`/`AgentRuntime`/`AgentHost` 字段注释由「静默丢弃」改为「默认拒绝 + 显式告警」。

**保留的能力语义**：`agent-host` 在没有组合根注入通道时**仍然可用**——构造、目录查询、会话端点、进程生命周期状态机都不依赖 `NodeEvents`；受影响的只有「已构造事件的交付」这一件事，而它现在返回错误而不是假装成功。

### A.3 新增用例（断言「未绑定不会静默丢弃」）

| 用例 | 位置 | 断言 |
| --- | --- | --- |
| `node::tests::unbound_node_events_refuse_to_drop_silently` | `crates/agent-host/src/node.rs`（单测） | 未接线的 `report_connected`/`report_disconnected` 返回 `Err(NodeEventError::Unbound)`（不是 `Ok`、不是 panic）；已接线时两者返回 `Ok` 且 sink 恰好收到 2 条事件 |
| `unbound_node_events_are_visible_not_silently_dropped` | `crates/agent-host/tests/session.rs`（集成） | 默认构造的 `AgentHost` 的 `node_events_bound()` 为假；未接线出口的 `send(connected_event)` 返回 `Err(Unbound)`；`set_node_events(..)` 后 `node_events_bound()` 为真 |

原用例 `unbound_node_events_drop_silently_but_stay_reportable`（断言「静默丢弃」是**期望行为**）已被**替换**为 `unbound_node_events_refuse_to_drop_silently`——保留它会把旧契约固化成回归基线。`report_disconnected_is_a_noop_before_the_exit_verdict` 增补了一条断言：未接线 + 未判定退出仍是 `Ok(())`（没有事件产生 ≠ 投递失败）。

---

## B. 组合根接线所需的精确调用形态（供 main 在 `crates/app/src/compose.rs` 落地）

> 这是 main 修 F1 剩余部分（接线）时要照抄的形态。**本轮未执行**（该文件不在 WP4 Write Scope）。

### B.1 缝的类型与方法签名（target 版本实测）

```rust
// crates/agent-host/src/node.rs
pub struct NodeEvents { sink: Option<acp_core::ports::EventSink> }   // 不透明，字段不公开
impl NodeEvents {
    #[must_use] pub fn unbound() -> Self;                            // 未接线：send 返回 Err(Unbound)
    #[must_use] pub fn new(sink: acp_core::ports::EventSink) -> Self; // 接线
    #[must_use] pub fn is_bound(&self) -> bool;
    pub fn send(&self, event: acp_core::model::EndpointEvent) -> Result<(), NodeEventError>;
}
pub enum NodeEventError { Unbound }                                  // 经 agent_host::NodeEventError 导出

// crates/agent-host/src/host.rs
impl AgentHost {
    #[must_use] pub fn new(config, credentials, host_config, ids, clock) -> Self; // 默认 unbound
    #[must_use] pub fn with_node_events(self, node_events: NodeEvents) -> Self;
    pub fn set_node_events(&self, node_events: NodeEvents);          // 任意时刻可调用
    #[must_use] pub fn node_events_bound(&self) -> bool;
}
```

core 侧入口（WP3 `0f70c4c`，plan.md:916 的 Merge Order `WP3 → WP4` 之后才在 WP4 基线上可见）：

```rust
// crates/core/src/broker.rs（WP3 交付面）
impl Broker {
    pub async fn node_submit(&self, event: EndpointEvent) -> Result<(), PortError>;      // = commit_node_event
    pub async fn commit_node_event(&self, event: EndpointEvent) -> Result<(), PortError>;
}
```

### B.2 落点与顺序（`compose.rs::assemble`）

关键约束：`NodeEvents::new` 接受的是**同步** `EventSink`（`Fn(EndpointEvent)`），而 `commit_node_event` 是 **async**——core 不依赖 runtime，因此组合根必须在能驱动 future 的位置（组合根本就持有 runtime）桥接。另外 `Broker` 必须在 `AgentHost` 接线**之前**构造（现有代码顺序即如此：`broker` 在 `host` 之后构造，需要调整或延后接线）。

```rust
// 1) 先构造 broker（现有代码已有），再构造 host。
let broker = Arc::new(Broker::new(/* 现有参数 */));

// 2) 用 broker 的节点级入口包一个同步 sink；事件交给持有 runtime 的任务提交。
let node_submit_handle = Arc::clone(&broker);
let node_sink = acp_core::ports::EventSink::new(move |event| {
    let broker = Arc::clone(&node_submit_handle);
    // 组合根的唯一 runtime 句柄（daemon 启动时创建的 handle）。
    let runtime = /* composition 持有的 tokio::runtime::Handle */;
    runtime.spawn(async move {
        if let Err(error) = broker.commit_node_event(event).await {
            // 节点级事件稀少（每进程各一次），失败即显式告警——不静默吞掉 R8 的落库失败。
            tracing::error!(error = %error, "节点级事件提交失败（agent.connected/disconnected 未落库）");
        }
    });
});

let host = Arc::new(
    AgentHost::new(
        Arc::clone(&local_config),
        credentials,
        HostConfig::default(),
        Arc::clone(&ids),
        Arc::clone(&clock),
    )
    .with_node_events(agent_host::NodeEvents::new(node_sink)),   // ← 唯一必须新增的一行
);
```

- **顺序**：`Broker` → 包 `EventSink` → `AgentHost::new(..).with_node_events(..)`。若组装顺序不变（`host` 在前），则用 `set_node_events`（等价），但它**必须在本进程第一次 spawn 之前**调用；否则那个进程的 connected 已发生过，不会被补报（`NodeEvents` 共享可变，已运行进程的下一次断开仍会走到新通道）。
- **不需要适配器**：`NodeEvents::new` 的入参就是 `acp_core::ports::EventSink`（`EventSink::new` 即构造面）。F2 落地（改收 `NodeEventSink`）后，此处只需把 `EventSink::new` 换成 `NodeEventSink::new`，调用形态不变。
- **必须同时接**：`Broker::commit_node_event`（否则事件到了 core 也是 `InvalidRequest`/丢弃）；两者缺一都会让 R8 的持久化场景不成立。

### B.3 可核对的落地判据（供 AC1 / 复核）

1. `crates/app/src/compose.rs` 中出现 `agent_host::NodeEvents::new(`（或等价 `set_node_events`）；
2. 该处的 `EventSink` 闭包最终调用 `Broker::commit_node_event` / `node_submit`；
3. `crates/app/tests/node_link_e2e.rs` 的 AC1 断言：节点级事件落库后 `session_id` 为空、且不被会话级投递路径误收；
4. 未接线时（例如只构造 `AgentHost::new` 不接线的测试路径）`node_events_bound()` 为假且 `send` 返回 `Err`。

---

## C. F2（MINOR）—— **按 main 裁决不在本轮做**；给出精确 patch

### C.1 事实与裁决

- `crates/core/src/ports.rs` 的 `NodeEventSink`（`ports.rs:1122`）由 WP3 的 `0f70c4c` 新增；**不在 WP4 基线 `33040d7`**（WP4 worktree 全仓库 grep `NodeEventSink` = 0 命中）。
- 因此「`NodeEvents::new` 接受 `NodeEventSink`」的**字面修法在 WP4 分支上无法编译**；把它加进 WP4 还会连带触发 PV1 `check:drift` 对 `docs/CORE_PORTS_AND_STORAGE.md §5` 的比对失败，而那份 doc 不在 WP4 范围。
- main 裁决（IRC）：选 (a)——F2 由 main 在 MU2 合入后于 `main` 上作为独立收尾任务执行；本轮只做 F1 与 F3 裁量。**已遵从**。

### C.2 精确 patch（**只有在 MU2 合入、`ports.rs` 含 `NodeEventSink` 之后**才可应用与验证）

`crates/agent-host/src/node.rs`（3 处）：

```diff
-use acp_core::ports::{Clock, EventSink};
+use acp_core::ports::{Clock, NodeEventSink};
@@ impl NodeEvents {
-    /// 接线的出口（组合根把 core 的节点级事件入口包成 [`EventSink`] 后注入）。
+    /// 接线的出口（组合根把 core 的节点级事件入口包成 [`NodeEventSink`] 后注入）。
     #[must_use]
-    pub fn new(sink: EventSink) -> Self {
+    pub fn new(sink: NodeEventSink) -> Self {
         Self { sink: Some(sink) }
     }
@@ struct NodeEvents {
-    sink: Option<EventSink>,
+    sink: Option<NodeEventSink>,
```

`crates/agent-host/src/node.rs` 的测试模块（`use super::*` 可见 `NodeEventSink`，2 处 + 模块内 `EventSink::new` → `NodeEventSink::new`）：

```diff
-            EventSink::new(move |event| {
+            NodeEventSink::new(move |event| {
```

`crates/agent-host/tests/session.rs`（2 处 `agent_host::NodeEvents::new(..)` 的实参）：

```diff
-            acp_core::ports::EventSink::new(move |event| {
+            acp_core::ports::NodeEventSink::new(move |event| {
```

以及 `crates/agent-host/tests/session.rs` 的 `Collector`（`tests/support/mod.rs:117` 的 `pub fn sink(&self) -> EventSink`）需要加一个 `pub fn node_sink(&self) -> NodeEventSink`（同形，仅换类型），供 `host_with_node_events` 使用。

**验证要点**：应用后 `cargo clippy --locked -p agent-host --all-targets -- -D warnings` 与 PV1/PV2 必须重跑（main 在 `main` 上执行）。功能语义**不变**——`NodeEventSink` 与 `EventSink` 同为 `Arc<dyn Fn(EndpointEvent)>` 的包装，仅类型层区分节点/会话。

---

## D. F3（SUGGESTION）—— **保留 `raw`**，消费方如下

`raw`（每个 diff 元素在 ACP 原文里的逐字节切片）**有明确消费方**，不是无人读取的死字节：

1. `crates/agent-host/tests/session.rs:1560`（`typed_diff_elements_are_delivered_with_the_tool_call`）：
   ```rust
   let slice = element["raw"].as_str().expect("raw 切片");
   assert!(raw_json.contains(slice), "diff 元素的 raw 必须是 ACP **原文**的子串（逐字节）");
   ```
2. `crates/agent-host/tests/session.rs:1640/1648`（`diff_slice_is_byte_identical_and_acp_raw_matches_its_digest`）：
   ```rust
   let slice = element["raw"].as_str().expect("raw");
   let offset = raw_json.find(slice).expect("raw 必须在原文里");
   assert_eq!(&raw_json[offset..offset + slice.len()], slice, "raw 是原文的逐字节子串");
   for field in ["path", "oldText", "newText", "raw"] { assert!(element.get(field).is_some(), ..); }
   ```

即 `raw` 是 R22/D4「Diff 元素**逐字节**透传」这条**唯一可直接采信的现场判据**：没有它，测试只能证明「三个解码后的字段相等」，无法证明切片来自原文（转义 `\/`、空白、多字节的保真就失去断言面）。生产消费方：core 侧只读 `path`/`oldText`/`newText`（全仓库 grep 无生产代码读 `"raw"`，WP3 的 `derive.rs` 亦然）——因此它是「为可验证性付出的双份体积」。

按 reviewer 建议**同时登记取舍**：当日无失败路径（`server::sync` 未实现、`owned_event.payload_json` 无上限）；待同步出站面落地、单事件载荷逼近 `SYNC_PROTOCOL §14` 的 1 MiB `maxMessageBytes` 时，应做尺寸核算或按需裁掉 `raw`（裁掉会削弱上述两条断言，需同步替换断言策略）。

---

## E. 不可改动边界的确认

| 边界 | 状态 |
| --- | --- |
| Diff 透传的字节扫描/切片逻辑（`mapper.rs`） | **逐字未改**（本轮 4 个改动文件中不含 `mapper.rs`） |
| `AgentLifecycle` 状态机、`ExitMark` 顺序语义 | **逐字未改**（`node.rs` 的改动只在 `NodeEvents`/`NodeEventError`/`report_*` 的返回类型与文档） |
| R21 的目录派生路径（`impl AgentCatalog`） | **逐字未改** |
| `crates/core/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` | **零改动** |
| `crates/app/**` | **零改动**（接线交 main，见 §B） |

事件形状（`EventKind::State`、无 turn/causation/acp、view `{agentId,state[,error]}`、封闭词表取值）**未改**——`node_event`/`connected_event`/`disconnected_event` 逐字保留。

---

## F. Checks

### F.1 局部检查（开发中）

| ID | 命令 | cwd | 环境 | 退出码 | 结果 |
| --- | --- | --- | --- | --- | --- |
| LC-FMT | `cargo fmt --all` | `.worktrees/wp4` | `CARGO_TARGET_DIR=.target-wt/wp4` | 0 | PASS |
| LC-CLIPPY-PKG | `cargo clippy --locked -p agent-host --all-targets -- -D warnings` | `.worktrees/wp4` | 同上 | 0 | PASS（0 warning；`Finished dev profile`） |
| LC-TEST-NODE | `cargo test --locked -p agent-host --all-features --lib node::` | `.worktrees/wp4` | 同上 | 0 | PASS — `8 passed; 0 failed`（含新的 `unbound_node_events_refuse_to_drop_silently`） |
| LC-TEST-F1 | `cargo test --locked -p agent-host --all-features --test session unbound_node_events_are_visible_not_silently_dropped` | `.worktrees/wp4` | 同上 | 0 | PASS — `1 passed; 0 failed` |

### F.2 PV1 —— `npm run verify`（在 target revision `4c4990a` 上）

```
$ cd D:\Project\acp-remote\.worktrees\wp4
$ CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp4 npm run verify
PV1_EXIT=0
```

- `check` 十道全绿（含 `check:drift`：`contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`）；
- `check:rust` 三条全绿：`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`、`cargo test --locked --workspace --all-features`（日志中 `test result: ok` 94 处、`FAILED`/`test result: FAILED` **0** 处）。
- 日志：`reports/PV1-wp4-r2.log`（按 `.gitignore` 第 27 行的 `openspec/changes/**/reports/**/*.log` **有意不入库**，与 TP1/r1 同一约定）。本轮**未命中**已知 flake `crates/app/tests/daemon_lifecycle.rs`。

### F.3 PV2 —— `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features`（在 target revision `4c4990a` 上）

```
$ cd D:\Project\acp-remote\.worktrees\wp4
$ CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp4 cargo test --locked -p core -p storage-sqlite -p agent-host --all-features
PV2_EXIT=0
```

| suite | 结果 |
| --- | --- |
| `agent_host` 单测（`lib`） | ok — 18 passed（`node::tests` 8 条，含新的拒投递断言） |
| `agent_host` 集成 `tests/session.rs` | ok — **28 passed**（r1 的 27 + 新的 `unbound_node_events_are_visible_not_silently_dropped`） |
| `agent_host` 其余集成（`catalog`/`resume`/`supervision`/`view_contract`） | 全部 ok，0 failed |
| `core` 全部 suite | ok，0 failed |
| `storage_sqlite` 全部 suite | ok，0 failed |
| Doc-tests（agent_host/core/storage_sqlite） | 0 tests |

- 日志：`reports/PV2-wp4-r2.log`（同上不入库）。
- **未发现或全部跳过的测试不算通过**：本包 PV2 无「全部跳过」的 suite；唯一 `ignored` 计数来自既有用例，非本包新增。

---

## G. 未验证内容（明确列出）

- **组合根接线的落地**：`crates/app/src/compose.rs` 的 `.with_node_events(..)` 与 `Broker::commit_node_event` 的生产调用点**未执行**（不在 WP4 Write Scope，main 处置）。§B 给出精确形态，但**本轮没有在真实 Daemon 上跑过**。
- **节点级事件的端到端落库/投递**（`crates/app/tests/node_link_e2e.rs`，AC1）：本包不跑；且经 F1 核实当前生产接线仍缺，即便跑也只能由测试自身手工接线。
- **F2 的 patch**：未应用、未编译、未验证（受 Merge Order 约束，见 §C）。上一段写明「只有在 MU2 合入、`ports.rs` 含 `NodeEventSink` 之后才可应用与验证」。
- **F3 的尺寸核算**：未做（当日无失败路径；`server::sync` 未实现）。
- **真实 Agent 的 Diff 元素形态多样性**：与 r1 相同——用例来自 fake ACP child，未经真实 Agent 对拍。
- **`deps`/`advisories`/`secrets` 三个 CI-only job**：本地无等价物，未执行亦未声称通过。
- **PV1 内的 `check:rust` 全量 `cargo test --workspace`**：本轮 exit 0 且 0 FAILED，但 `daemon_lifecycle` 的时序 flake 属已知非确定性，本轮未命中不等于消除。

---

## H. Handoff（块内结构化字段见文末围栏）

1. **交付提交**：`4c4990a3cefe445a36c9df9d5a7f57b572b10618`（分支 `feat/wp4`，父 `c63e199`）。F1 的复核应以它为 target、以原问题 ID `review-w4-r1-F1` 对拍。
2. **F1 的可复核判据**：未接线 → `NodeEvents::send` 返回 `Err(NodeEventError::Unbound)` + `error` 级日志；两条用例 `node::tests::unbound_node_events_refuse_to_drop_silently`、`unbound_node_events_are_visible_not_silently_dropped`。
3. **组合根接线**：见 §B（类型/签名/顺序/判据），交 main 落地。
4. **F2**：见 §C 的精确 patch，登记为 MU2 收尾项，在 `main` 上应用并重跑 PV1/PV2。
5. **F3**：保留 `raw`，消费方与取舍见 §D。
6. **review 归属**：tasks 3.8 需要非作者 reviewer 按原问题 ID 复核 F1。

```agentic-handoff
version: 1
agent_context:
  agent_id: coder-w4-r2
  isolation: fork_turns=none
handoff_index:
  - task_id: "2.4"
    work_package: WP4
    role: coder
    phase: fix
    round: 2
    stage: work-package
    target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
    evidence_type: DELIVERY
    evidence_id: wp4-delivery-r2
    report_path: reports/deliver-wp4-r2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 4c4990a = r1 交付 c63e199 + crates/agent-host 的 4 文件增量（F1 修复：NodeEventError::Unbound + send 返回 Result + 未接线 error 日志 + 两条断言）；F2 按 main 裁决不在本轮（NodeEventSink 属 WP3 交付面，受 Merge Order WP3 -> WP4 约束）；F3 保留 raw。对 crates/core、crates/sync-protocol、schemas、docs、fixtures、crates/app 零改动。"
    source_evidence: reports/review-w4-r1.md
  - task_id: "2.4"
    work_package: WP4
    role: coder
    phase: fix
    round: 2
    stage: work-package
    target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: reports/deliver-wp4-r2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 target revision 4c4990a 上重跑 npm run verify（cwd=.worktrees/wp4, CARGO_TARGET_DIR=.target-wt/wp4）：check 十道全绿（含 check:drift §5 15 traits/96 methods 一致）；check:rust 的 fmt/clippy/全量 cargo test --workspace --all-features exit 0，日志中 test result: ok 94 处、FAILED 0 处。日志 reports/PV1-wp4-r2.log。未命中已知 flake daemon_lifecycle。"
    source_evidence: reports/PV1-wp4-r2.log
  - task_id: "2.4"
    work_package: WP4
    role: coder
    phase: fix
    round: 2
    stage: work-package
    target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: reports/deliver-wp4-r2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 target revision 4c4990a 上重跑 cargo test --locked -p core -p storage-sqlite -p agent-host --all-features exit 0：agent-host 单测 18/18（node::tests 8 条）、集成 session 28/28（含新的 unbound_node_events_are_visible_not_silently_dropped）、catalog/resume/supervision/view_contract 与 core/storage-sqlite 全量 suite 0 failed。日志 reports/PV2-wp4-r2.log。"
    source_evidence: reports/PV2-wp4-r2.log
checks:
  - id: PV1
    work_package: WP4
    command: "npm run verify (cwd=.worktrees/wp4, CARGO_TARGET_DIR=.target-wt/wp4)"
    exit_code: 0
    log_path: reports/PV1-wp4-r2.log
    result: PASS
  - id: PV2
    work_package: WP4
    command: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/wp4, CARGO_TARGET_DIR=.target-wt/wp4)"
    exit_code: 0
    log_path: reports/PV2-wp4-r2.log
    result: PASS
test_delivery:
  WP4:
    kind: automated
    artifacts:
      - reports/PV1-wp4-r2.log
      - reports/PV2-wp4-r2.log
    basic_checks:
      - PV1
      - PV2
```
