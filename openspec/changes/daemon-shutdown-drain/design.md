<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes），行为契约见 specs/ 的 delta。 -->

# Design

## Context

- 现状（修复前）：关闭序列的排空把「发出 `daemon.stop` 的那条连接」也当作在途客户端连接等待，于是**排空时长取决于该客户端进程何时被调度**，而不是取决于服务端有多少在途工作。在 Windows 上 Named Pipe 句柄只在驱动它的 runtime 被驱动或销毁时才真正释放，因此实测可复现为「几乎总是等满 `daemon.shutdown_grace_ms`」（`drain_timeout{remaining:1}` 与 `elapsed_ms ≈ grace_ms`）。
- 修复后的机制是三处接线：①`crates/app/src/daemon.rs` 的 `ShuttingDownGate` 在**被接受**的 `daemon.stop` 上识别出「关闭请求者」；②它给本连接置位 `ConnectionShutdown`（`crates/server/src/transport/local/connection.rs`）；③`serve_connection` 对标记只做机械反应——**写回当前已完成的全部响应后**结束本连接（`CloseReason::ShutdownRequested`）。
- 判据口径（delta spec 的「关闭时的连接排空」）：排空窗口由 `graceMs` 界定；请求者 **MUST NOT 被计入排空等待**；窗口结束时仍未结束的连接被关闭并留下 `daemon.drain_timeout`；**判定取的是排空阶段自身的 `daemon.drain.drain_ms`**，不是整条关闭序列的耗时（后者还含停 Agent、`wal_checkpoint` 等与排空无关的阶段）。
- 约束：本地通道的帧形状与协议形状**不变**（传输层不解析方法名）；`graceMs = 0` 被 `MIN_DRAIN_MS = 200` 托底（「不等待任何在途工作」与「已接受的请求仍要落地」不能压在同一拍上）。

## Goals / Non-Goals

**Goals:**

- 让排空时长只由服务端在途工作决定：唯一在途连接就是请求者时，排空在远小于宽限的时间内结束。
- 让「谁在排空集合里、谁被等待」有一处可核对的口径，并把描述该口径的散文（`docs/` 两处、`crates/app/src/daemon.rs` 三处注释、`crates/app/src/cli.rs` 两处注释、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2）改成与之一致。
- 补齐规格第三个场景（宽限耗尽仍关闭剩余连接）的**正向**覆盖：日志里出现 `daemon.drain_timeout`。

**Non-Goals:**

- 不改本地通道的帧形状、协议形状或方法语义；不改排空窗口的阈值（`MIN_DRAIN_MS`、默认宽限）。
- 不删 `release_runtime()`（任务要求「说明而不删」：删除属行为变更，需另一次验证）。
- 不修相邻的第三条时序敏感用例（`the_periodic_task_runs_again_after_one_full_cycle`）：另案登记，见文末。
- 不改规格（规格已在 `2.1` 落地）。

## Decisions

**D1 —— 识别点放在管理语义层，传输层只对标记做机械反应。** 「这条请求是不是请求关闭」由 `ShuttingDownGate`（组合根的处理链）判定，结果写进每连接一份的标记；传输层不解析方法名。
理由：`daemon.stop` 的语义属于管理面；若让传输层按方法名判定，就把协议语义下沉到了通道层，且以后新增「请求关闭类」方法要改传输层。

**D2 —— 只有被接受（`accepted: true`）的关闭请求才置位。** 参数非法等失败不启动关闭序列，那条连接照旧是一条普通管理连接。
理由：置位意味着「本连接不必再被排空等待」，只有真的启动了关闭序列才成立。

**D3 —— 置位与响应写回是同一条连接任务的先后两步，因此不构成竞态。** `serve_connection` 在写回**当前已完成的全部响应**之后才结束连接。
理由：`daemon.stop` 的响应必须先送达（§5.2 的契约），否则「先返回 `accepted` 再关闭」就退化成「不返回任何东西就断连」。

**D4 —— 排空判据取排空阶段自身的耗时**（`daemon.drain` 的 `drain_ms`，同时记 `window_ms`）。
理由：整条关闭序列还含与排空无关的阶段；用总耗时做判据会把它们算进契约，宽限阈值也会因此失真（本变更把判据从总耗时改成 `drain_ms` 正是为此）。

**D5 —— `graceMs = 0` 仍被 `MIN_DRAIN_MS = 200` 托底。**
理由：把「不等待任何在途工作」与「已接受的请求仍要落地」压到同一拍上会得到自相矛盾的契约；这个下限是给「请求者连接与同一预算内的接入面收尾」留出的余地，实际窗口记在日志里、不静默。

## Risks / Trade-offs

- [请求者连接机械上仍在连接集合里] → 它由传输层自行结束，因此**不占用排空预算**。代价是「排空集合」不是字面上的白名单：文档与注释必须按「不被等待 / 不占用预算」表述，写成「不进入连接集合」就是不准确的。本批已把 `daemon.rs` 的三处措辞统一到这一口径。
- [Windows 句柄释放时机] → 不再进入排空判据；CLI 侧的 `release_runtime()` 判定为**就原目的而言已冗余**，但只说明不删（删除需要另一次行为验证，且它对关闭序列其余步骤的残留影响本变更证不了）。
- [用例依赖时间窗] → 判据是「远小于宽限」（2/3 宽限阈值）加 `MIN_DRAIN_MS` 托底；本批用**连跑 ≥10 次**证明确定性，且**没有**放宽任何既有断言。
- [文档面改动] → 两处 `docs/` 散文与一处协议条文只订正事实、不新增规范性条款；行为契约仍在 delta spec 里，合入后必须让 `npm run check`（含 `check:docs`/`check:drift`）保持全绿。

## Migration Plan

不涉及数据迁移、schema 变更或 wire 变更：改的是关闭序列的连接排空口径与描述它的文档。回滚 = 逆向该提交；回滚后回到「唯一在途连接是请求者时几乎必然等满宽限」的旧行为，无持久化副作用。

## 相邻项 3.3 的判定与登记（另案，本变更不修）

**对象**：`crates/app/tests/daemon_lifecycle.rs` 的 `the_periodic_task_runs_again_after_one_full_cycle` 开头的 startup-tick 断言（`wait_for_event("daemon.maintenance", 5s)` 之后断言 `startup.len() == 1`）。

**判定：用例侧**，不是实现问题。
- 依据（事实）：实现的首轮 tick 在任务起手即到点（`crates/app/src/daemon.rs` 的维护任务以 `next = Instant::now()` 起步，首轮标 `reason = "startup"`），第二轮要等 `MAINTENANCE_INTERVAL`（60 秒）；因此「计数变成 2」在 5 秒窗口内不可能，失败模式只能是**等待预算在高负载下超时**、`wait_for_event` 返回空，随后 `assert_eq!(len, 1)` 变红。
- 旁证：兄弟用例 `the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown` 对同一现象用的是 `!is_empty()` + 首轮 `reason == "startup"`（形态更稳，不依赖「等到恰好一条」）；`wait_for_event` 自己的文档也写明「零预算断言会把调度延迟误判成没跑」。
- 证据：本变更把该二进制连跑 10 次（记录见 `tasks.md` 的 1.3）；该用例是整条 `daemon_lifecycle` 里最长的用例（总时长约 70 秒，含 75 秒窗口的那一段），是这批里唯一仍带「等固定预算后按计数断言」形态的用例。

**修法建议（供另案）**：与兄弟用例对齐 —— 断言 `!is_empty()` 与首轮 `reason == "startup"`，把「必须恰好一条」换成「等到首轮出现」；或让等待条件本身等到首轮事件出现而不是等固定预算。**本变更不修**（它不在本变更的写范围内，且改它会让本批的连跑证据失去对照）。
