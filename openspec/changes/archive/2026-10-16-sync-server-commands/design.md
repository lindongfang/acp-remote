<!-- 本变更的方案：接口形状、接缝与取舍。动机见 proposal.md，行为见 specs/。 -->

# Design

## Context

动机见 `proposal.md`。前置是已合入的 `sync-server-connection`（连接面、订阅、快照、配对 HTTP 与 core 读面）。这里只记决定方案形状的现状与约束。

**已就绪、必须复用而不是重写的东西**

| 资产 | 位置 | 复用方式 |
|---|---|---|
| 12 条 Sync 命令的 wire DTO 与结果形状 | `crates/sync-protocol/src/command.rs`（~1.7k 行，已冻结） | 只消费，不新增 wire 类型 |
| 34 类事件视图的类型化投影 | `crates/sync-protocol/src/views.rs` 的 `project(event_type, view)` | 直接调用；它已按 `$defs` 逐类映射 |
| ACP 原文的 wire 装配口径 | `server::node_link::resource::wire_payload`（`resource.rs:1049`） | 算法与 `media_type`/`256 KiB` 上限口径逐字相同；**类型各自定义**（§6 禁止跨协议复用 DTO） |
| 命令的授权与派发 | `core::use_cases::submit_command`、`Broker::authorize`（`broker.rs:781`） | `Actor::Device { device, scopes }` 已支持，`scopes.contains(command)` 即是逐命令判定 |
| 幂等记录与终态 | `SessionStore::find_request`（`session_store.rs:2102`）、`owned_command` 表、broker 的 `settle_*` | 复用既有持久化；**不新增表** |
| 终态观察者与撤销后的连接扫除 | `server::node_link::command` 的终态 watcher 与 `node_revoked` | 形态可照抄（Sync 侧的键是 `(deviceId, requestId)`） |
| 命令速率限流的形态 | `server::node_link::command` 的连续超限计数与 `4429` 关闭 | 数值与命令族不同，类型各自定义 |
| 事件投递的有界队列与 scope 过滤 | `sync-server-connection` 的 `sync/conn` 出站账本 | 直接复用其投递路径 |

**约束**

- `AGENTS.md` §5：命令与事件轨道都只调用 `core::use_cases`，不得 import `storage-sqlite`，不得与 `server::node_link` 互相调用。
- `AGENTS.md` §6：Sync 与 Node Link 的 wire DTO 不得互相复用；两侧可共享**算法与口径**，不可共享类型。
- `AGENTS.md` §4：Broker 只为业务决策创建公共领域视图，不能把该视图当作完整 ACP 消息。
- `AGENTS.md` §8：正常运行路径不得 `unwrap()`/`expect()`；异步任务必须有所有者与取消路径。

## Goals / Non-Goals

**Goals**

- 让脚本化客户端能对真实 Daemon 发 prompt 并观察 `accepted` → 领域事件 → 持久化终态，权限拒绝与限流均可复现。
- 事件视图与 ACP 原文的下发形状与已冻结合同逐字段一致，使前端不必自行解析 Agent 私有负载。
- 命令与事件的实现都不引入新表、新 crate 或新第三方依赖。

**Non-Goals**

- 连接面、订阅、快照、配对 HTTP（属前置变更）。
- 静态资源托管与前端接线（属同批第 3 段与独立变更）。
- 真实 Codex/OMP 兼容性报告（属切片 8）。

## Decisions

### D1：视图投影调 `sync-protocol`，原文装配在适配层

`sync-protocol::views::project` 已把 `eventType` 映射到 34 个类型化视图，并校验形状。事件轨道因此只做两件事：从持久化事件取 `payload_json` 的原样文本作为 `view`（core 的 `ReadView::event_payload` 已保证是库里原文），调 `project` 做一次形状判定（**判定失败也不得丢弃事件**，见下），以及装配 `payload.acp`。

**为什么判定失败不能丢弃事件**：§10.3 要求未登记判别子显式降级、不得静默丢弃。`project` 的失败意味着「这个 eventType 的 view 缺了协议要求的最低字段」——那是**生产者或我们的 bug**，但把它变成「事件消失」会让客户端看到序号空洞并误判丢包。因此判定失败时按诊断记录一条结构化日志，仍按原样下发 `view`（客户端自带降级卡片）。这与 Node Link 的 `view_shape` 处理同口径。

**原文装配为什么不能复用 Node Link 的函数**：那个函数住在 `server::node_link::resource`，跨模块调用会违反 §5 的「平级 adapter 不互相调用」。且 §6 明确禁止两侧复用 wire DTO。因此 Sync 侧写自己的 `wire_payload` 同形函数，`MAX_INLINE_ACP_BYTES`（256 KiB）与 `application/json` 固定值的口径来自 §14/§9.4，两侧各自持有常量并由各自的测试断言——**代价是常量有两处**，收益是依赖方向不被破坏；若将来该口径需要单一来源，正确做法是下沉到叶子 crate，不是横向复用。

### D2：命令走 core 既有的授权—幂等—提交链，适配层不另建判定

命令管线固定为：`schema → 已认证设备 → scope → 状态/版本 → 幂等查找/预留 → 持久接受 → 会话 actor → Agent → 持久事件 → 广播`（§11.2 的「命令处理建议顺序」）。

- **schema**：`sync-protocol` 的 DTO 解码已做形状与封闭词表校验。`session.resume` 不在 Sync 命令枚举内，解码阶段即按 `command.unsupported` 拒绝。
- **scope**：`Broker::authorize(actor, command, session, request)` 对 `Actor::Device` 就是 `scopes.contains(command)`（`broker.rs:788`）。授权在适配层**先于**任何 core 读取调用，保证「未授权不泄露会话是否存在」。
- **幂等**：`requestId` 由客户端生成且重试复用；幂等键是 `(deviceId, requestId)`。mutation 的接受行与终态由 core 的提交路径持久化（`owned_command` 已有该表），查询类命令只在连接期去重、不落盘。
- **冲突判定**：`command`/`sessionId`/`expectedVersion` 与解码后的 payload **语义**比较，不看键顺序——用 `acpr-wire::cj1` 的规范 JSON 摘要比较（它已是 `payloadDigest` 的既有前像），从而「键顺序不同但语义相同」不会被误判为冲突。

**为什么不自己维护一张内存幂等表**：mutation 的幂等必须跨重连、跨进程重启存活，否则「响应丢失后重试」会二次派发（§11.2 明确禁止把旧 request 当新命令）。内存表只在查询类命令上有意义。

### D3：终态观察者按连接持有，断开即取消

被接受的 mutation 终态必须推送回提交方连接（§11.2「只在重查/重复 request 时返回」——初次终态走事件）。实现形态照 Node Link 的终态 watcher：连接持有一个以 `(deviceId, requestId)` 为键的观察者集合，终态事件从事件扇出里匹配后入队该连接。

**关键约束**：观察者 MUST 有所有者并在连接断开时取消（`AGENTS.md` §8 禁止遗留 detached task）。由于终态本身已持久化，观察者丢失只影响「本次是否实时收到」，不影响「之后能否查到」——客户端重连后用 `command.status` 兜底。这与 Node Link 的取舍一致。

### D4：命令级限流按（设备，命令族）计数，连续超限才关连接

§14 要求「rate limit 至少按 IP、device ID 和 command 分类」。形态照 `conn/limits.rs` 的结论：连续超限计数达阈值才 `4429` 关闭，单次超限只回 `resource.rate_limited` + `retryAfterMs`。

**为什么不按连接计数**：同一设备可以重连来重置计数，按设备计数才拦得住；而命令族分开统计是协议明确要求（否则高频查询会挤掉 prompt 的额度）。

**数值来源**：120/分钟取 `SYNC_PROTOCOL.md` §14，与 `node_link` 的同类常量各自持有。

### D5：结果超限按 `resource.result_too_large` 显式失败

`session.read` 等查询命令的结果可能超过 `maxMessageBytes`。协议规定此时返回 `resource.result_too_large`，并让客户端改用快照/事件视图，**不得截断后伪装成完整结果**。因此投递路径在入队前先算编码后字节数：超限即回该错误码，不入队。

**替代方案**：分片下发查询结果——协议 v1 没有这个形状（快照分块只服务 `sync.snapshot_*`），否决。

### D6：不新增表、不推进文件格式版本

mutation 的接受与终态复用 `owned_command`；查询类命令无持久化；限流计数在连接期内存。**因此本变更对存储零改动**，`check:drift` 与 DDL 门禁不受影响。

### D7：事件轨道与连接面共用投递队列

`sync-server-connection` 已实现按设备 scope 过滤的有界出站账本与两级背压。本变更的事件轨道只是**生产者**：把投影后的事件交给该连接并按其水位处理。MUST NOT 另建一条队列（两条队列会让「谁先发」不可判定，破坏序号严格递增）。

## Risks / Trade-offs

- **[常量在两处]** `MAX_INLINE_ACP_BYTES` 与 `application/json` 的固定值在 Node Link 与 Sync 各自持有 → 两侧各有测试断言该值与协议文本一致；若口径变化，`npm run check` 的文档/合同门禁会先红，再由两个模块各自修正。下沉到叶子 crate 是未来的收敛路径，需要单独的 ADR。
- **[幂等记录的清理时机]** 清理过早会让重试变成新命令 → 复用既有保留策略（记录至少保留到设备撤销且会话删除）；本变更不引入新的清理路径。
- **[限流误伤]** 按设备计数会让多标签页共用同一设备额度 → 客户端规则是「同一浏览器多标签页应通过 Web Locks/BroadcastChannel 协调」，且 120/分钟对交互式使用有充足余量；误伤的代价是明确的 `retryAfterMs` 提示而非静默失败。
- **[视图形状判定失败]** 生产者写错字段时事件仍会下发（D1 的取舍）→ 代价是客户端走降级卡片，收益是不出现序号空洞；诊断日志记下 `event_type` 与缺失字段，便于定位。
- **[终态观察者丢失]** 连接断开时在途 mutation 的终态不再实时推送 → 终态已持久化，客户端重连后用 `command.status` 查询；这是协议设计的兜底路径，不是缺口。

## Migration Plan

- **schema 无变更**：不新增表、不新增列、不推进文件格式版本，因此**无 migration、无回滚数据风险**。
- **部署顺序**：纯增量——命令与事件在 `/sync/v1` 上新增处理能力，前置变更已提供端点。回滚即回到「能连上、能看目录，但不能发命令」。
- **配置**：不新增配置键（命令限流与结果上限取自协议固定常量与既有 `sync.*` 键）。

## Open Questions

- 无。方案中所有会影响 specs、approach 或任务拆分的判断都已在本文件内定。（幂等记录的保留策略由前置与既有实现决定，不在本变更重开。）
