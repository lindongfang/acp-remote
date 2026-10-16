<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

**一、`session.create` 的落账时序让"失败"留下了不该有的痕迹，而且三份记录互相矛盾。**

现状：core 的**创建提交**（会话行 + 幂等行，同一事务）先落盘，然后才去启动 Agent。后端失败时以 `?` 直接冒泡——**不终结命令、不清理会话行**。实测（三次 `omp acp` 建会话失败）：

| 什么时候问 | 答案 |
|---|---|
| 客户端收到的 | `status: "rejected"`，`acceptedAt: null`——"未产生任何副作用" |
| 同一时刻库里写的 | `status='accepted'`，`session_id` 非空，无终态 |
| 守护进程重启之后 | `status='uncertain'`，`terminal_event_id` 非空，`error_code=command.uncertain` |

同一件事的三种答案里有两个是假的：客户端被告知"什么都没发生"，而库里有幂等记录 + 一行**没有 Agent、没有目录归属**的会话（它还进了快照，只是当前客户端在分组时把它丢掉）。

**二、失败判据是空白，而 Node Link 面用一个过粗的替身。** `sync-session-create` 的失败分类只写到"引用失败 / 解析失败"为止，"Agent 已登记但创建失败"**没有归类**；终态那条只列了 `completed`/`failed`/`uncertain` 三种可能，**不给判据**。Node Link 面则用"有没有持久记录"当"会话行有没有提交"的替身——在本变更的时序下这个替身恒为真。

**三、命令面一行日志都不打，事故只能靠外部工具定位。** `sync-command-handling` 没有任何日志要求，`server::sync::command` 全程无 tracing。上面那张表是我用浏览器调试协议抓帧 + 重启守护进程看恢复才拼出来的；普通用户机器上没有这些手段。

**为什么现在做**：配对模型（`pairing-model`）落地后，命令面是唯一还没收口的面；而"Agent 失败"是用户最常撞到的失败类别，它的记录是否可信、是否可诊断，直接决定这个产品能不能自己撑住。

## What Changes

**一、落账时序改为"先做完再落账"**

- `session.create` 的**受理** SHALL 只落幂等行（`session_id` 保持 `null`，MUST NOT 写会话行）；后端成功之后，会话行 + 回填幂等行的 `session_id` + 终态事件 SHALL 在同一次提交里落盘；**后端失败 MUST 零会话行**。
- 会话行落账之前到达的事件 SHALL 在 broker 内有界排队，落账后按序写库再广播（"先持久化成功再广播"因此第一次被端到端满足）；队列超限 SHALL 复用既有的"超限结束该 Agent"失败关闭口径（`local-agent-host` 的「超限结束的失败关闭顺序」）。
- 崩溃窗口因此只留一条 `accepted`、无会话行的幂等行；启动恢复终结为 `uncertain`（`CORE_PORTS` §6 第 16 条的"恢复走**有会话**分支"随之不再成立，要改写）。

**二、失败判据统一为「可证明 / 说不清」**

- **可证明 Agent 侧未产生会话**（spawn 失败、Agent 明确反回错误、请求未发出）⇒ 终态 `failed`；**说不清**（短请求超时、请求发出后连接断开、入库后的存储失败、崩溃窗口）⇒ 终态 `uncertain`。
- 该判据 SHALL 在 Sync 与 Node Link **两个面一致**；Node Link 面 MUST 换掉"有没有持久记录"这个替身，改用"会话行是否已提交"。
- `failed` 的错误码复用既有 `internal.unavailable`（成因由终态的 `message` 承担），MUST NOT 新增错误码。

**三、命令级结构化日志（新增要求）**

- 每个**受理**的命令与它的**终态** SHALL 各产生一条结构化日志：主体（哪个设备/客户端）、命令名、会话标识（若有）、终态类别、成因类别。日志 MUST NOT 含路径、凭据或完整 prompt（沿用既有脱敏口径）。
- 该要求同时承担 `pairing-model` 里"服务端要能看出当前由谁控制"的可见性诉求——因此主体字段是必需的。

**四、验收**

- 协议级：失败时**零会话行**、客户端拿到 `accepted` → 终态 `failed`（带成因）、同 `requestId` 重发返回首次终态；窗口期事件不丢不重、顺序正确；超限走失败关闭。
- 恢复：崩溃窗口（受理已落盘、会话行未落盘）重启后终结为 `uncertain`。
- 日志：失败路径上能在日志里看到主体、命令名、终态与成因。

**本次不包含**：

- wire 的 `details` 增字段（缓做；`details` 是兼容新增，现在不做不锁死）。
- 数据库列与迁移：`owned_command.session_id` **本来就允许为空**，本变更不引入 schema 变更。
- `admin-state-persistence`、`storage-schema-v2-migration`、`daemon-lifecycle` 三个能力（已核实：命令行形状不属它们管辖、无 schema 变更、启动恢复本来没有任何 Requirement 在管——本变更把新窗口的处置写进 `sync-session-create` 的终态要求里）。

## Capabilities

### New Capabilities

（无新增能力。「命令级结构化日志」作为**新增要求**归入既有的 `sync-command-handling`。）

### Modified Capabilities

- `sync-session-create`: 「引用失败与解析失败的分类」延伸到"请求已发出之后"；「终态与幂等」补 `failed`/`uncertain` 的判据、落账时序与崩溃窗口（含无会话行的那一支）。
- `sync-command-handling`: 「mutation 先接受后终态」明确受理时的持久化形状与"失败零行"；新增「命令级结构化日志」一条要求。
- `node-link-owner-server`: 「命令回复与终态契约」补 `failed`/`uncertain` 判据；「`command.status` 查询与落盘前失败的幂等语义」换掉"持久记录替身"。
- `local-agent-host`: 「失败路径的明确结果」的分类与判据挂钩；「会话创建时暴露 ACP 会话标识」按新时序重述（含"启动失败 / 超时 / Agent 返回错误"拆到两档）。

## Impact

- **代码**：`crates/core/src/broker.rs`（`create_session` 由"创建提交 + 后端 + 两列提交"改为三笔：受理 / 执行 / 落账，并加窗口期事件队列）、`crates/core/src/use_cases.rs`（同口径）、`crates/server/src/sync/command/mod.rs` 与 `crates/server/src/node_link/command.rs`（终态判据与日志）、`crates/agent-host/src/host.rs`（失败分类与判据的对接）。
- **权威文档**：`docs/CORE_PORTS_AND_STORAGE.md` §6 第 16/20/22 条（含"恢复走有会话分支"的改写）、`docs/SYNC_PROTOCOL.md` §11.2/§11.5、`docs/NODE_LINK_PROTOCOL.md` 相应小节、`docs/SECURITY_DESIGN.md` §14（日志允许字段）。
- **机器合同**：**无 wire 变更**——五态、事件形状与幂等语义不变，`schemas/`、`fixtures/` 不动；错误码不新增。
- **验收边界**：主判据是协议级端到端（`accepted` → 终态 `failed`、失败零行、窗口期事件不丢不重、日志可读）与启动恢复的正确收尾。**风险如实登记**：本变更动的是 core 的**事务时序**，是三个变更里最重的一处；窗口期的事件排队是新增机制，必须证明"落账后按序写库再广播"与既有"先提交后发布"不矛盾。
