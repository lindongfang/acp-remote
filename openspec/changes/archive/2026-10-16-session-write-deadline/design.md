<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes），行为契约见 specs/ 的两份 delta。 -->

# Design

## Context

现状（修复前，两个会话同构）：

- `crates/server/src/transport/net/ws.rs` 的 `send_text` 就是 `SinkExt::send` 的裸 await；`recv` 同样是裸 await。传输层没有任何期限。
- `crates/server/src/node_link/conn/session.rs` 的业务循环用一条 `select!` 驱动四个事件源（关闭请求、daemon 关闭信号、tick、出站队列、入站帧），而 `Event::Outbound` 分支**内联** `self.write_now(&outbound.text).await`。心跳（`link.ping` 入队）、90 秒静默判定与慢消费者看门狗（`SessionWindows`：`SESSION_TICK` 500 ms、`SLOW_CONSUMER_GRACE` 30 s、静默 90 s）都在这同一条 `select!` 的 tick 分支里。
- 因此：**对端停止读取、socket 缓冲被填满时，写会一直 Pending，整个 `select!` 停摆**——保活与判据在最需要它们的时候不再运行。该结论与实测见已归档变更 `openspec/changes/archive/2026-10-16-node-link-saturation-recovery/design.md` 的「相邻项 3.1」（本变更的 tasks 1.2 因此不要求重测，只要求说明差异）；`node_link/conn/session.rs` 的模块文档也已把这一缺口如实登记为「不在本模块」。
- 传输层封装的两条约束会影响修法：① `WsConnection::recv` 在收到超限消息时**自己发 1009 close 帧**（读路径需要写能力）；② `WsConnection::close` 消费所有权。
- `crates/server` 目前没有 `futures`/`futures-util` 直接依赖；`WsConnection` 内部是一个 `axum::extract::ws::WebSocket`（`Stream + Sink`）。

## Goals / Non-Goals

**Goals:**

- 让「对端不读」这一情形**不能**让会话任务无限期停在写路径上：写路径有期限，超期限的处置是**断开**（不在半截帧上重试）。
- 写未被推进的期间，心跳、静默判定与慢消费者看门狗**仍按各自窗口被检查**——这是本变更真正的难点，也是不能只给写包一个 `timeout` 的原因（见 D1）。
- 两侧（`node_link` / `sync`）同口径，且不改变既有分级处置的顺序：先按看门狗/静默判据处置，断开是最终一步。

**Non-Goals:**

- 不改协议语义：不新增 wire 消息、不改 close code 语义、不改 §2.5 的限流与队列上限数值（proposal 的非目标）。
- 不对齐 `sync` 侧「饱和期心跳」的口径差异（`sync` 目前仍是 `else if now >= next_ping`）：`node_link/conn/session.rs` 的模块文档已声明那是另一个变更的决策。
- 不重做饱和记账（水位、迟滞、低水位）本身：那是已归档变更 `node-link-saturation-recovery` 的范围。

## Decisions

**D1 —— 把写从会话循环里**移出去**：每连接一个写任务，而不是给内联写包一个 `timeout`。**
理由（两条都独立否决「只加 timeout」）：

1. 内联写即使包了 `timeout`，在**窗口之内**仍然阻塞同一条 `select!`：tick 得不到轮询 ⇒ 心跳/静默/看门狗在「写卡住期间」不成立，直接违反 delta 的第二个场景。
2. 把 `timeout` 缩到「不阻塞 tick」的量级也不行：WS 帧写到一半被取消就**无法续写**，只能视为连接已损坏 ⇒ 那就等于「第一次写卡住就断开」，跳过了规格要求的「先按判据处置（分级），最终断开」。

写任务持有写半边、会话保留读半边与 tick，于是：写卡住时**读与判据都照常推进**（入站仍被处理，`last_inbound` 因此仍然准确），写期限只作为最后一道界。

**D2 —— 每帧写期限取 `SLOW_CONSUMER_GRACE`（30 s），与看门狗同一窗口；到期即该连接的最终处置：断开并记日志。**
理由：一个窗口一个含义，避免出现「两套阈值互相猜」；窗口内看门狗/静默判据仍会先给出分级处置（例如心跳被拒按推迟、饱和段计时），写期限是它们的兜底而非替代。到期不重试：半截帧不可恢复（D1 第 2 条），且此刻连接已经不可信。

**D3 —— 读半边的 1009 关闭改为经写任务发出。**
理由：拆分之后 `recv` 不再持有写能力，而「超限消息按 1009 关闭」是传输层的既有契约（`WsConnection::recv` 的文档与 `message_too_large` 的用例都锁定它）。做法：`recv` 仍负责**判定**（保持 `WsError::MessageTooLarge` 与既有文本形态不变），关闭动作由会话转交写任务。

**D4 —— 同一时刻至多一帧在写，帧序由单写者保证。**
理由：写任务串行消费一个队列，天然保持帧序；会话侧不得绕过写任务直接碰 socket（传输层不暴露写半边给会话）。

**D5 —— 关闭顺序：会话是写任务的拥有者，关闭 = 有界的三步。**
① 会话向写任务发出关闭请求（带 close code/reason）；② 写任务在「正在推进的帧」与「关闭请求」之间 `select`，关闭请求胜出即**放弃该帧**，按 `CLOSE_DRAIN_BUDGET`（2 s）尽力发 Close 帧后结束；③ 会话有界地 join 写任务，超时则放弃等待并让 socket 随任务结束而关闭。
理由：`AGENTS.md` §8 要求异步任务有所有者、取消路径与关闭顺序；这里的取消路径就是关闭请求通道 + 任务 drop（socket 随之关闭）。`Session::drain` 现有的写也要走同一条路：它只界定了 `recv` 的预算，写本身此前没有期限。

**D6 —— 两侧同构，但只改「写路径」这一件事。**
`node_link/conn/session.rs` 与 `sync/conn/session.rs` 的循环形状逐行同构（`Event` 枚举、`select!` 分支、`write_now`、`drain`），因此两处用同一套形状改：新增写任务、`write_now` 变成向写任务投递、`Event::Outbound` 不再 await 写。`sync` 侧饱和期心跳的口径差异**不动**（见 Non-Goals）。

## Risks / Trade-offs

- [新增直接依赖 `futures-util`（仅用 `StreamExt::split`）] → 这是把「读半边 + 写半边」分开的最小代价；不引入该依赖就得靠 `Arc<Mutex<WebSocket>>` 共享 socket，那会在每帧上持锁并让 `recv` 与写互相阻塞，比缺口本身更糟。依赖是 axum 自身已在用的 crate，许可证与维护状态无新增风险；若审查认为不可接受，退路是手写一个只含 `split` 语义的薄封装（不推荐：等于自造 split）。
- [写任务带来的生命周期风险] → 由 D5 的所有者/取消/有界 join 覆盖；写任务不得 detach。
- [既有用例的假设] → `node_link/conn/tests.rs` 的用例文档以「填满 socket ≈ 10～30 次 32 KiB 写」为前提（proposal 已列），写路径改造后**该前提仍然成立**（那描述的是 socket 容量，不是本层行为），但涉及「写会卡住」的断言可能需同步；`sync` 侧同构用例同理。
- [写期限取 30 s 是否够快] → 与看门狗同窗口，且**远小于**静默 90 s；换句话说：一条「对端彻底不读」的连接会在 30 s 量级被断开，而不会拖到静默判定。若产品上要更快，改的是 `SLOW_CONSUMER_GRACE` 这一个常量（本变更不调）。
- [可观测性] → 断开必须带 `event` 日志（写期限触发与看门狗触发要能区分），否则「为什么断的」在事后无法判定。

## Migration Plan

不涉及数据迁移、schema 或部署步骤：改的是进程内的连接写路径与任务划分。回滚 = 逆向该提交；回滚后回到「写卡住 ⇒ 会话 `select!` 停摆 ⇒ 连接不会被断开」。

## 已知文档缺口（如实登记，随本变更一并更新）

- `crates/server/src/node_link/conn/session.rs` 的模块文档现在写着「写路径没有期限……那是传输层写期限的问题，不在本模块」——本变更修掉它之后，这段必须改写成新口径（写经写任务、有期限），否则模块文档会与代码相反。
- `docs/NODE_LINK_PROTOCOL.md` §2.5 与 `docs/SYNC_PROTOCOL.md` 的相邻条目：目前只写「高水位先停读、持续过慢断开」，没有「写路径有期限」这层。按 §9 的映射，协议语义若因此改了口径需更新对应文档；本变更**不改协议语义**（不新增消息、不改数值），因此先按「登记」处理：在实现任务的报告里给出结论，必要时再单列文档更新。
