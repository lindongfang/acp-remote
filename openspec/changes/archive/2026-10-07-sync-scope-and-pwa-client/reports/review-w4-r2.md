<!-- WP4 修复复核（recheck，Round 2）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "3.8"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-w4-r2"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP4 的任何实现或修复对话，未继承 r1 implementer/reviewer 的上下文；r1 结论仅供对拍问题 ID）"
base_revision: "c63e199dded5cd735054b899c5a2ecede418df11"
target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
scope: "仅 c63e199..4c4990a 的修复 diff（4 files / +168 / -48，全部在 crates/agent-host/），逐条复核 review-w4-r1-F1/F2/F3，并检查该 diff 是否引入回归。不重复第 1 轮已 PASS 的部分（Diff 透传字节保真、AgentLifecycle 生命周期语义、R21 路径复用）。另评估 main 落地组合根接线（crates/app/src/compose.rs）时按作者报告 §B 的形态存在的实现陷阱。"
changes: "只读检视，未修改任何文件；未切换分支、未提交、未合并、未运行任何编译/测试/E2E。仅新增本报告（review-w4-r2.md）。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR（F2 已由 main 裁决延后，非本轮范围）/ 0 SUGGESTION（F3 已登记取舍）。本轮 diff 未引入新的 patch 引入缺陷。"
result: PASS

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-w4-r2`（沿用 `review-w4-r1` 线程）/ 2（recheck） |
| Review Type / Stage | branch（recheck：PV1/PV2 已由 main 复跑 PASS，合入 MU2 候选之前） |
| Work Package | WP4（MU2 成员，R8/R21/R22） |
| Repository / Worktree | `D:\Project\acp-remote`，固定检视 worktree `.worktrees\wp4`（HEAD `4c4990a`，只读） |
| Base of this recheck | `c63e199dded5cd735054b899c5a2ecede418df11`（= target 的父提交，已 `git rev-parse` 核对） |
| Target Revision | `4c4990a3cefe445a36c9df9d5a7f57b572b10618` |
| 实际修改文件 | `crates/agent-host/src/{host.rs,lib.rs,node.rs}`、`crates/agent-host/tests/session.rs`（4 文件，`git diff --name-only` 全量核对）；`crates/core/**`、`crates/app/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` **零改动** |
| 读取的规则与需求 | `specs/local-agent-host/spec.md`（R8/R21/R22 行）、`specs/core-derived-events/spec.md`（「Agent 连接状态是节点级生命周期」R8 行 :66-68）、`design.md` D4/D6、`plan.md`（Write Scope / Merge Order / AC1）、`AGENTS.md` §3/§4；`docs/CORE_PORTS_AND_STORAGE.md` §5（`NodeEventSink`、节点级事件不走 `EventSink`） |
| 验证证据（只读消费） | `.target-wt/wp4-r2-verify.log`（**main 复跑的 PV1+PV2**，`WP4-r2 PV2 exit=0`；2371 行）、`reports/PV2-wp4-r2.log`、`reports/PV1-wp4-r2.log`、`reports/deliver-wp4-r2.md`、`reports/review-w4-r1.md`。**作者自评不作为检视证据**。Reviewer 未执行任何编译/测试/E2E（只读边界）。 |
| 限制 | 不判「用例是否充分」（交 validator / Coverage Index）；不重复跑 Project Verify；不执行 E2E；`crates/app/src/compose.rs` 的接线本轮不存在（由 main 落地），本节只做**接缝形态的风险评估**，不构成对未落地代码的发现。 |

---

## A. F1 复核（MAJOR，原阻断项，原 ID `review-w4-r1-F1`）——**已解决（本轮范围内）**

判定口径（依 main 裁决）：本轮的验收面是「**让接线缺失不再静默**」，**不是**接线本身。按此口径逐条复核派发单的 5 个子点。

### A.1 子点 1 —— 未绑定确实不再静默，且错误传播链完整

**结论：成立。** 链路：`NodeEvents::send`（`node.rs:98-113`）→ `report_connected`（`node.rs:322-331`）/ `report_disconnected`（`node.rs:337-351`）→ `AgentRuntime::report_connected`（`host.rs:106-117`）/ `report_disconnected`（`host.rs:120-140`）。

| 环节 | 取值（target） | 判据 |
| --- | --- | --- |
| `send` 未接线 | `tracing::error!(event_type = .., "节点级事件没有交付通道…")` + `Err(NodeEventError::Unbound)` | `node.rs:104-110`：日志与错误**同时**发出，二者互补（开发期靠返回值、运行期靠日志） |
| `report_connected` | `Some(event) => events.send(event)`（原样透传 `Result`）；`None => Ok(())` | `node.rs:327-330`。无 `let _ =`、无 `.ok()`、无 `unwrap_or` |
| `report_disconnected` | `!exited` 提前 `Ok(())`（无事件）；否则原样透传 | `node.rs:344-350`。语义边界正确：**没有事件** ≠ **投递失败** |
| `AgentRuntime` 两处 | `.is_err()` 分支各补一条 `log_undelivered(event_type, agent)`（含 `agent_id`） | `host.rs:112-116`、`host.rs:129-139` |
| 错误日志可见性 | `tracing::error!`；生产默认级别 `LevelFilter::INFO`（`crates/app/src/config.rs:99`），`error` 高于 `info` → 生产**必然**输出 | 非「打了但被级别过滤掉」 |

全仓库交叉核对（workspace 级 grep）：`NodeEvents` / `NodeEventError` / `report_connected` / `report_disconnected` 在 `crates/agent-host/` **之外零命中**，因此不存在「另一个我未读到的调用方把 `Err` 吞掉」的可能。本层没有丢错误的环节。

### A.2 子点 2 —— `Err(Unbound)` 不会被上层静默忽略

**结论：成立。** 两个生产调用方都用 `.is_err()` 消费返回值，但**不是**静默忽略：`host.rs:115` 与 `host.rs:138` 各再记一条**带 `agent_id` 的 `error` 级日志**（`node::log_undelivered`，`node.rs:305-315`），因此一次漏接在日志里留下两条可定位记录（一条含 `event_type`，一条含 `event_type + agent_id`）。这比「只返回 `Err` 无人接收」更强，正是 r1 要求的「接线缺失在运行期可见」。

需澄清的一个反直觉点（**不构成缺陷**）：`AgentRuntime::report_connected` 的签名仍是 `fn report_connected(&self)`（`host.rs:106`），即返回值在运行期层被 `is_err()` 消化，未继续上抛到 `AgentHost` 的会话入口。这是**正确**的：节点级事件的漏接不应让「创建会话/`agent_capabilities`」失败（那样会把一个可用的适配器变成不可用），而可观测性已由错误日志承担。r1 所禁的是「无声」，不是「不阻断调用方」。

### A.3 子点 3 —— 已绑定时 `Ok` + 正常投递，无重复投递

**结论：成立。** `node.rs:100-103`：已接线分支 `sink.send(event); Ok(())`——**恰好一次**调用；无循环、无重试、无二次派发。单测 `unbound_node_events_refuse_to_drop_silently`（`node.rs:478-511`）在一条 connect + 一条 disconnect 后断言 `collector.len() == 2`（`node.rs:511`），这**同时**排除了「已绑定也报错」（两处 `.is_ok()` 断言，`node.rs:509-510`）与「重复投递」（若重复，计数会 > 2）。`is_bound()` 语义未变（`node.rs:87-89`）。

### A.4 子点 4 —— 旧用例确实被替换，未被 `#[ignore]` 掩盖

**结论：成立。** ① 旧名 `unbound_node_events_drop_silently_but_stay_reportable` 在 target 上 **`git grep` 零命中**（仅存在于 baseline 的 `node.rs:414` 与陈旧编译产物里）；② 新名 `unbound_node_events_refuse_to_drop_silently`（`node.rs:478`）断言的是**相反**的契约（`Err(Unbound)` 而非「静默即预期」）；③ `git diff c63e199 4c4990a | grep '"ignore]"'` **零命中**，即本 diff 未新增任何 `#[ignore]`；④ 两条新用例在 main 复跑的日志里**实际执行且通过**：`.target-wt/wp4-r2-verify.log:304`（`node::tests::unbound_node_events_refuse_to_drop_silently ... ok`）、`:364`（`unbound_node_events_are_visible_not_silently_dropped ... ok`），`PV1-wp4-r2.log:301,361`、`PV2-wp4-r2.log:13,82` 同结论。旧契约基线已被拆除，不会把「静默丢弃」固化成回归基线。

### A.5 子点 5 —— Write Scope 合规

**结论：成立。** `git diff --name-only c63e199 4c4990a` = 4 个文件，全部在 `crates/agent-host/`（`src/host.rs`、`src/lib.rs`、`src/node.rs`、`tests/session.rs`）。`crates/core/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**`、`crates/app/**` 逐目录 `git diff --name-only` 均为 0 行。导出面只增一项（`lib.rs:43` 的 `NodeEventError`），未改既有导出。`crates/agent-host/Cargo.toml` **无新依赖**（`tracing` 早已是依赖，`process.rs`/`session.rs` 多处使用）。

### A.6 F1 的残留（不计为本轮缺陷，登记供 main）

修好后「漏接可见」，但 **R8 的持久化本身仍不可满足**——因为 `crates/app/src/compose.rs` 未接线这一**规划缺口**依旧存在（main 已登记为 MU2 收口项，owner = main）。本报告**不**把该缺口重复计为 WP4 的发现：它不在本 diff 内、不属 WP4 Write Scope、且已被裁决归口。但其落地风险见 §D（本节的存在价值正是把风险前置到 main 动手之前）。

---

## B. F2 复核（MINOR，main 已裁决延后，原 ID `review-w4-r1-F2`）——**patch 完整可用；本轮无半途改动**

### B.1 本轮 diff 没有半途改动该类型

`git diff c63e199 4c4990a | grep NodeEventSink` = **0 命中**。`NodeEvents { sink: Option<EventSink> }`（`node.rs:55`）与 `NodeEvents::new(sink: EventSink)`（`node.rs:81`）在本轮**逐字未动**，即不存在「改了一半又回退」的中间态残留。

### B.2 patch 完整性核实（独立枚举，与作者 §C2 对拍）

我在 target 上把必须改的位点**全部枚举**如下，并与作者 patch 逐项比对：

| # | 位点（target 实测） | 内容 | 作者 §C2 是否覆盖 |
| --- | --- | --- | --- |
| 1 | `node.rs:20` | `use acp_core::ports::{Clock, EventSink}` → `{Clock, NodeEventSink}` | ✔ |
| 2 | `node.rs:55` | 字段 `sink: Option<EventSink>` → `Option<NodeEventSink>` | ✔ |
| 3 | `node.rs:81` | `pub fn new(sink: EventSink)` → `NodeEventSink` | ✔ |
| 4 | `node.rs:79` | 文档注释里的 `[EventSink]` | ✔ |
| 5 | `node.rs:483` | 测试模块 `EventSink::new(..)` | ✔（「测试模块 2 处」的第一处） |
| 6 | `node.rs:516` | 测试模块 `EventSink::new(..)` | ✔（第二处） |
| 7 | `tests/session.rs:1401` | `NodeEvents::new(acp_core::ports::EventSink::new(..))` | ✔（「2 处 `NodeEvents::new(..)` 的实参」） |
| 8 | `tests/session.rs:1235` / `:1495` | `NodeEvents::new(node.sink())` → 需 `node.node_sink()` | ✔（经「`Collector` 需加 `node_sink`」间接覆盖；且类型不匹配会**编译失败**，不可能漏） |
| 9 | `tests/support/mod.rs:117-119` | 新增 `pub fn node_sink(&self) -> NodeEventSink` | ✔（作者明确要求） |

**未列入且不应改**（作者未列，正确）：`host.rs:656/703/748` 的 `sink: EventSink` 属 `SessionBackendFactory` 的**会话级**入口（`host.rs:651` 起的 impl），必须保持 `EventSink` 不动；`host.rs:25` 的 `EventSink` import 同理继续需要。`host.rs` 其余部分（`AgentHost` 只持有 `NodeEvents`）**零改动**。

结论：patch 的**覆盖面完整**（import / 字段 / 构造函数 / 测试 3 处 `::new` 面 + Collector 辅助函数），无遗漏位点，也无「多改」位点。`docs/CORE_PORTS_AND_STORAGE.md` 已由 WP3 登记 §5.4 的 `NodeEventSink`（`.worktrees/wp3/docs/CORE_PORTS_AND_STORAGE.md:28,808`），因此 patch 落地后 `check:drift` 不会因文档缺登记而失败。**可供 main 照做。**

### B.3 给 main 的落地顺序提示（F2 与 §D 的接线相互耦合）

`NodeEvents::new` 的类型一旦切换，§D 的接线处也必须同步由 `EventSink::new` 换成 `NodeEventSink::new`，否则出现「组合根传 `EventSink`、`NodeEvents::new` 要 `NodeEventSink`」的编译错。建议 F2 与接线**同一提交**落地（或在接线后立刻做 F2 并重跑 PV1/PV2）。

---

## C. F3 复核（SUGGESTION，原 ID `review-w4-r1-F3`）——**保留 `raw` 的理由成立（有保留）**

作者的理由：`raw` 是 R22/D4 字节保真的直接见证，被断言消费。**核实结果：成立**，但需把「唯一」二字限定到正确的层级。

**消费方（全仓库实测，仅以下 5 处，无生产消费者）**：

| 位置 | 性质 | 断言内容 |
| --- | --- | --- |
| `mapper.rs:704` | `#[cfg(test)]`（mod 起于 `mapper.rs:678`） | `raw.as_str().contains(slice)` 且 `slice.contains("src\\/main.rs")`——断言**转义原样保留** |
| `mapper.rs:805` | 同上（多字节用例） | `contains(slice)`，断言不切进多字节字符 |
| `tests/session.rs:1560` | 集成测 `typed_diff_elements_are_delivered_with_the_tool_call`（`:1507`） | `raw_json.contains(slice)` |
| `tests/session.rs:1640` | 集成测 `diff_slice_is_byte_identical_and_acp_raw_matches_its_digest`（`:1594`） | `raw_json[offset..offset+len] == slice` **逐字节相等** |
| `tests/session.rs:1648` | 同上 | `["path","oldText","newText","raw"]` 四字段齐备 |

生产侧：`crates/core/src/derive.rs:196-249` 的 `derive_from_elements` 只读 `path`/`oldText`/`newText`（`optional_string(&members, "path" | "oldText" | "newText")`），**从不读 `raw`**；`crates/sync-protocol/**` 零命中。故「无生产消费者、纯为可验证性付费」的表述属实。

**「移除会使回归无法捕获」的成立条件**：`path`/`oldText`/`newText` 三者都是**按 JSON 语义解码后的取值**（`mapper.rs:366-371` 的 `decoded_string`），因此若 `diff_elements` 改成「解析后重新序列化」，这三个字段**依然相等**，只有 `raw` 能区分。故在**交付视图层级**上，`raw` 确实是唯一直接见证（`tests/session.rs:1640/1648` 无法用其它三字段替代）。

**保留意见（不推翻结论）**：`mapper.rs:704/805` 两条**函数级单测**直接对 `diff_elements()` 的输出断言字节子串，不经过 view，因此「唯一」严格说限于视图层；若将来裁掉 `raw`，函数级断言仍在，只需重写两条集成断言。作者称「唯一直接见证」在**集成层**准确、在**函数层**略强。该差异不影响「保留 `raw`」的结论（当前保留是最省事的正确选择），故**不新开发现**。

---

## D. 组合根接线（`crates/app/src/compose.rs`）的风险评估 —— 供 main 落地时规避

作者报告 §B 给了形态；我按其形态在 target 上逐点核对现有装配代码，识别出 **5 个会让 main「照抄就踩」的陷阱**。以下均为**未落地代码**的风险提示，不作为对 WP4 的发现。

### D.1 【硬阻断】`with_node_events` 内联写法与现有装配顺序**互相死锁**，只能用 `set_node_events`

作者 §B2 的主示例是 `AgentHost::new(..).with_node_events(NodeEvents::new(node_sink))`，并把 `Broker` 构造放在其**之前**。但现有装配是：

- `compose.rs:218-225`：`let host = Arc::new(AgentHost::new(..));`
- `compose.rs:227`：`let backends: Arc<dyn SessionBackendFactory> = host.clone();`
- `compose.rs:230-245`：`let broker = Arc::new(Broker::new(BrokerDeps { backends, .. }))`

即 **`Broker` 依赖 `host`**（`backends`），而 `node_sink` 闭包又要捕获 `Arc<Broker>` → 形成 `host → (sink) → broker → (backends) → host` 的环。因此「先把 Broker 造好，再 `AgentHost::new(..).with_node_events(..)`」**在这里不成立**（作者自己的括注也承认「若组装顺序不变则用 `set_node_events`（等价）」，但主示例仍是被抄的那一段）。

**正确形态（唯一可行）**：`host` 先构造（`Arc<AgentHost>`），`broker` 照旧构造，然后在 `broker` 之后**原地补一行**

```rust
host.set_node_events(agent_host::NodeEvents::new(node_sink));  // host 已是 Arc<AgentHost>，&self 方法经 Deref 可用
```

`set_node_events` 在 target 上存在且 `&self`（`crates/agent-host/src/host.rs:298-300`），`Arc<AgentHost>` 解引用调用可行。**顺序**：`Broker` → 造 `node_sink`（捕获 `Arc<Broker>` + `Handle`）→ `host.set_node_events(..)`。

### D.2 【硬阻断】`runtime` 句柄在 `Composition` 上**不存在**，必须用 `tokio::runtime::Handle::current()`

作者 §B2 写 `let runtime = /* composition 持有的 tokio::runtime::Handle */;`——`Composition` **没有**该字段（`compose.rs` 全文无 `Handle`）。`assemble` 是 `async fn`（`compose.rs:181`），调用点均在 runtime 上下文内：`cli.rs:659-664`（`new_multi_thread` + `block_on(crate::daemon::run(..))`）、以及 `compose.rs` 自身的 `#[tokio::test]`（`:960/:1109/:1134/:1186`）。因此正确写法是**在 `assemble` 内**取 `tokio::runtime::Handle::current()`（`Handle` 是 `Clone + Send + Sync`，满足 `EventSink` 闭包的 `Send + Sync + 'static`，`crates/core/src/ports.rs:1083-1093`），并 `handle.spawn(..)`。**不要**用 `Handle::block_on`：闭包会在 async worker 上下文里被调用（`process.rs:479-481` 的同步钩子路径 / `spawn_router` 任务），`block_on` 在 runtime 内会 panic。

### D.3 【高】断开事件提交的是**游离任务**，与 daemon 关闭序列存在竞态

`agent.disconnected` 的最后一次投递发生在 `host.shutdown_all()` 内（`host.rs:231` → `report_disconnected`），此刻 sink 只会 `handle.spawn(commit_node_event)`（游离任务，**不被** daemon 的 `OwnedTasks` 追踪）。而 daemon 关闭序列是（`crates/app/src/daemon.rs:1433-1473`）：

1. `tasks.cancel_all(..)`（只取消 `OwnedTasks`，**管不到**上述游离任务）
2. `host.shutdown_all()`（在此 spawn 提交任务）
3. `drop(host)`
4. `composition.close()` → `wal_checkpoint(TRUNCATE)` + 关连接池

若游离提交任务在第 4 步之前没被 poll 到，写入会撞上已关闭的存储（或在 close 期间竞争）→ **最后一条 `agent.disconnected` 不落库**，R8 退化为一条日志。当前顺序里第 3→4 步之间只有少量 `await`，「通常」来得及，但**没有任何同步点保证**。

**建议**：把提交任务的 `JoinHandle` 收集到 `Composition` 持有的列表，在 `close()` 里先 join（带上限超时）再关存储；或退一步，在 `close()` 之前插入一次「等待节点级提交清空」的显式屏障。若接受该残余风险，请在 `compose.rs` 处写明注释与理由（本仓库对 shutdown 顺序有先例：`host` 的 `Arc` 必须在 `Composition::close` 前 `drop`，见 `daemon.rs:1470-1472`）。

### D.4 【高】接线必须早于接入层开始接受连接，否则留下真实的漏接窗口

`Broker::endpoint_for` 会**惰性**调用 `backends.open(..)`（`crates/core/src/broker.rs:2777-2791`），其调用方是 `session.mode.list`（`crates/core/src/use_cases.rs:545`）与 `live_config`（`:1533`）——即**任何一次本地管理调用都可能拉起 Agent 进程并触发 `agent.connected`**。daemon 的顺序是：`assemble`（`daemon.rs:1157`）→ `seed_if_needed`（`:1160`）→ `recover_unsettled`（`:1176`）→ `accept_loop`（`:1293`）。

因此：**在 `assemble` 末尾（或最迟 `accept_loop` 之前）接线是安全的**；接线晚于 `accept_loop` 则首个进程的 connected 必然漏投（`set_node_events` 不补报既有连接，`host.rs:286-288` 的文档与 r1 §E 已确认这是「恰好一次」的必然推论）。

补充核实：`recover_unsettled`（`broker.rs:2554-2581`）→ `recover_command`（`:2583+`）只把 `accepted` 命令终结为 `uncertain`，**不打开 endpoint/不 spawn Agent**，故恢复阶段本身不构成漏接窗口。**同时** `Broker::commit_node_event` 也必须接线（否则事件到了 core 会被 `InvalidRequest` 拒绝，见 `.worktrees/wp3/crates/core/src/broker.rs:640-660` 的校验），两者缺一都会让 R8 不成立。

### D.5 【中】提交失败的告警不要写成 `let _ =`

`commit_node_event` 返回 `Result<(), PortError>`。作者示例里的 `if let Err(error) = ... { tracing::error!(..) }` 是**正确**的；落地时务必保持（不要为了消 warning 写成 `let _ =`），否则 F1 刚修好的「不静默」会在 core 侧重新出现——本轮的整改精神（`node.rs:14-15` 的模块头）要求两层都不静默。

### D.6 【低】F2 与接线的原子性

见 §B.3：`NodeEvents::new` 的类型切换与 `compose.rs` 的 `EventSink::new` → `NodeEventSink::new` 必须同步，否则中间态编译失败。

---

## E. 新发现（编号自 `review-w4-r2-F4` 起）

**无。** 本轮修复 diff（`c63e199..4c4990a`）未引入新的 patch 引入缺陷（无越界改动、无行为回归、无死代码、无被 `#[ignore]` 掩盖的用例、无重复投递、无「已绑定也报错」）。`review-w4-r1-F1` 在本轮验收口径下**已解决**；`review-w4-r1-F2` 维持 main 的延后裁决（patch 已核实可用）；`review-w4-r1-F3` 维持保留（理由成立）。§D 的 6 项为**未落地接线**的风险提示，按派发要求供 main 规避，不计为发现。

---

## F. 实际检查范围

1. `git diff c63e199dded5cd735054b899c5a2ecede418df11 4c4990a`（含 `-U0` hunk 边界与 `--name-only`）逐 hunk 阅读；`.worktrees/wp4` 的 `node.rs`（全文 547 行）、`host.rs`（相关区间 100-410 / 940-975 / 279-302）、`tests/session.rs`（相关区间 1460-1500、1540-1660、1222-1236）、`lib.rs`、`tests/support/mod.rs:108-132`、`mapper.rs:325-380,678-810`。
2. 全仓库 grep：`NodeEvents|NodeEventError|report_connected|report_disconnected|with_node_events|set_node_events|node_events_bound`（确认 agent-host 外零命中）；`NodeEventSink|commit_node_event|node_submit`（确认 WP4 树上不存在，F2 延后前提成立）；`["raw"]|get("raw")`（确认 5 处消费方，全为测试）；`"ignore]"`（确认本 diff 未新增跳过）。
3. 交叉读 WP3 交付面（`.worktrees/wp3/crates/core/src/{ports.rs:1108-1140, broker.rs:610-665}`）作为 F2/接线的消费方证据；`.worktrees/wp3/crates/core/src/derive.rs:196-249` 作为 F3「无生产消费者」的证据。
4. 只读消费 `.target-wt/wp4-r2-verify.log`（main 复跑，含两条新用例的执行行）与 `reports/{PV1-wp4-r2.log,PV2-wp4-r2.log}`（作者的 CHECK 日志）——**仅用于核对用例是否真的执行**，不作为 F1 结论的依据（F1 结论基于 target 源码）。
5. 需求对照：`specs/local-agent-host/spec.md`（R22 行 :7）、`specs/core-derived-events/spec.md:66-68`、`docs/CORE_PORTS_AND_STORAGE.md` §5（`NodeEventSink` / 节点级事件不走 `EventSink`）、`crates/app/src/{compose.rs,daemon.rs,config.rs,cli.rs}`（仅用于 §D 的接线风险核对）。

**未验证内容（明确列出）**：

- **组合根接线的实际落地**（`crates/app/src/compose.rs` 的 `set_node_events` 与 `commit_node_event` 调用点）：本轮不存在该代码，未编译、未运行；§D 是静态核对结论，非运行时实证。
- **节点级事件的端到端落库/投递与 AC1**（`crates/app/tests/node_link_e2e.rs`）：未执行；且当前生产接线仍缺，即便跑也只能由测试自身手工接线，不能证明生产路径。
- **F2 的 patch**：未应用、未编译、未验证（受 Merge Order `WP3 → WP4` 约束——`NodeEventSink` 在 WP4 树 0 命中）。
- **`raw` 的尺寸核算**：未做（`server::sync` 未实现，无失败路径）。
- **真实 Agent 的 Diff 元素形态多样性**：与 r1 相同，未与真实 Agent 对拍。
- **PV1/PV2 的独立复跑**：Reviewer 只读消费 main 与作者的日志，未自行执行任何编译/测试；`deps`/`advisories`/`secrets` 三个 CI-only job 未执行亦未声称通过。
- **r1 已 PASS 的三项**（Diff 透传字节保真、`AgentLifecycle` 语义、R21 路径复用）：按派发单不重复检视；仅核实本 diff 未触碰对应代码（`mapper.rs` 不在改动文件列表；`node.rs` 的 hunk 全部落在 `NodeEvents`/`NodeEventError`/`report_*`/测试，`ExitMark`(:123)、`AgentLifecycle`(:164-190)、`node_event`(:226-258) 的 hunk 零命中）。

---

## Assessment

| 问题 ID | 严重度 | 结论 |
| --- | --- | --- |
| `review-w4-r1-F1` | MAJOR | **已解决（本轮验收口径）**。未接线从「静默丢弃」变为「`Err(Unbound)` + 两条 `error` 级日志（含 `event_type` 与 `agent_id`）」，传播链在 agent-host 内完整闭合、无吞错层；已绑定仍 `Ok` 且恰好一次投递；旧「静默即预期」的契约基线已替换而非 `#[ignore]`；越界零改动。R8 的残留取决于 `compose.rs` 接线，属 main 已登记的规划缺口。 |
| `review-w4-r1-F2` | MINOR | **维持延后**（main 裁决）。本轮无半途改动；patch 覆盖面经独立枚举核实**完整可用**（9 个位点，含 import/字段/构造函数/3 处构造面/`Collector::node_sink`），并已指出 `host.rs:656/703/748` 的会话级 `EventSink` 不得改动。 |
| `review-w4-r1-F3` | SUGGESTION | **维持保留**。`raw` 确被 2 条集成断言（`tests/session.rs:1560/1640/1648`）与 2 条单测（`mapper.rs:704/805`）消费，无生产消费者；移除会使交付视图层的字节保真断言无替代手段。「唯一」在集成层准确、在函数层略强（已登记，不新开发现）。 |
| `review-w4-r2-F4…` | — | **无新发现**。 |

**组合根接线风险**：见 §D。其中 D.1（`with_node_events` 与现有装配顺序死锁，只能 `set_node_events`）与 D.2（`Handle::current()` 而非不存在的 `Composition` 句柄）是**照抄作者 §B2 主示例就会踩的硬阻断**；D.3（游离提交任务 vs `composition.close()` 的竞态）是唯一可能让 R8 在**已接线之后**仍丢最后一条件断开事件的运行时风险，建议 main 在 `close()` 前加提交清空屏障；D.4（接线必须早于 `accept_loop`）决定漏接窗口是否存在。

**结论：PASS。**

判定依据：本轮验收面内不存在未解决的 CRITICAL/MAJOR。F1 的 5 个子点全部核实通过（§A），修复未引入回归（§E）；F2 按 main 裁决不在本轮，其 patch 已核实可用（§B）；F3 的保留理由成立（§C）。§D 的 6 项风险均属**尚未落地**的 `crates/app/src/compose.rs` 接线，按派发要求作风险登记交 main 规避；R8 的最终可满足性仍取决于该接线落地，属 main 已登记的 MU2 收口项，不影响对本轮修复 diff 的判定。

本结论绑定 Target Revision `4c4990a3cefe445a36c9df9d5a7f57b572b10618`。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md，不代替 main 判定整体变更可归档。

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-w4-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "3.8"
    work_package: WP4
    role: reviewer
    phase: branch
    round: 2
    stage: work-package
    target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
    evidence_type: REVIEW
    evidence_id: review-w4-r2
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-w4-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定提交 4c4990a（父 = c63e199，已 rev-parse 核对）上做只读检视：git diff c63e199..4c4990a 的 4 个文件（全在 crates/agent-host/）逐 hunk 阅读 + .worktrees/wp4 目标内容全文核对；按原问题 ID review-w4-r1-F1（5 子点）/F2/F3 逐条复核；全仓库 grep 确认 NodeEvents report_* 在 agent-host 外零命中、NodeEventSink 在 WP4 树零命中、raw 消费方仅 5 处测试；只读消费 .target-wt/wp4-r2-verify.log（main 复跑）与 reports/PV{1,2}-wp4-r2.log 以核对新用例实际执行。未执行任何编译/测试/E2E。"
    source_evidence: reports/review-w4-r1.md
  - task_id: "3.8"
    work_package: WP4
    role: reviewer
    phase: branch
    round: 2
    stage: work-package
    target_revision: "4c4990a3cefe445a36c9df9d5a7f57b572b10618"
    evidence_type: RISK_ASSESSMENT
    evidence_id: review-w4-r2-compose-wiring-risk
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-w4-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "对 main 待落地的 crates/app/src/compose.rs 接线（deliver-wp4-r2.md §B 的形态）做静态风险核对：compose.rs:218-245 的 host/broker 依赖方向（D.1）、Composition 无 runtime 句柄字段（D.2）、daemon.rs:1433-1473 的关闭序列与游离提交任务竞态（D.3）、daemon.rs:1157/1176/1293 的顺序与 broker.rs:2777-2791 的惰性 backend.open（D.4）。不构成对未落地代码的发现。"
    source_evidence: reports/deliver-wp4-r2.md
```
