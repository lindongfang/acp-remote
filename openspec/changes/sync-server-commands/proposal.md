<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

`sync-server-connection` 落地后，PWA 能配对、能认证、能拿到目录与会话快照，但**只能看不能动**：`server::sync` 尚无命令处理器与事件视图投影，客户端派发的 11 条业务命令（`session.prompt`、`session.cancel`、`permission.resolve`、`session.create` 等）没有任何东西接收，Agent 产生的结构化事件也无法以 `payload.view` 的形状下发。前端已有的对话页、权限应答、模型切换、变更文件右栏全部因此启用不了。

本变更是同批第 2 段：把 Sync 的命令族与事件投递补齐，使「看得到」变成「用得了」。

## What Changes

**一、命令处理器（12 条 Sync 命令）**

- 查询类 5 条（`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list`）SHALL 同步完成并直接返回一次 `completed` 或 `rejected`，不进入异步队列。
- mutation 类 7 条（`session.prompt`、`session.cancel`、`permission.resolve`、`elicitation.respond`、`session.mode.set`、`session.config.set`、`session.create`）SHALL 先持久化接受（`accepted`，`terminalEventId` 为 `null`），终态只通过一个持久化的 `command.completed`/`command.failed`/`command.uncertain` 事件表达。
- 幂等：同一 `(deviceId, requestId)` 重复提交 MUST 返回首次结果，MUST NOT 二次派发；command、sessionId、`expectedVersion` 与解码后的 payload 语义必须与首次一致，否则 `command.idempotency_conflict`。`session.resume` 不在 Sync 命令枚举内，收到时按 `command.unsupported` 拒绝。
- 授权：每条业务命令在当前连接身份下重新按设备 scope 判定，不能依赖客户端 UI；被拒时回 `authorization.scope_denied` 且**零副作用**（不读会话行、不触碰文件系统、不启动进程）。
- `expectedVersion` 用于有并发覆盖风险的 mutation（`session.mode.set`/`session.config.set`），冲突时回 `state.version_conflict` 并携带 `currentVersion`。

**二、事件视图投影与投递**

- 按 `docs/SYNC_PROTOCOL.md` §10.3 把事件投影为 `payload.view`：34 类已登记事件类型逐类给出最低必填字段（`session.created`、`turn.*`、`tool.call.*`、`permission.*`、`elicitation.*`、`terminal.output`、`file.changed`、`command.*`、`device.revoked` 等）。
- `payload.acp` 装配：按 `acp.raw-payload.v1` 协商结果携带 ACP 原文，`rawJson` MUST 逐字节保真（不得先解析为通用 JSON 再重新序列化）；因单条上限、保留期或存储原因不可用时 MUST 按已登记原因标记不可用，MUST NOT 用占位内容冒充。
- **未登记的 ACP 判别子** MUST 按未知事件降级并保留原文，MUST NOT 静默丢弃，MUST NOT 把结构化事件降级成普通文本。
- 投递复用连接面既有的有界出站队列与 scope 过滤；慢客户端背压按 $14 的两级策略（先停读重放批次，持续过慢发 `resource.backpressure` 再断开），MUST NOT 阻塞 Broker 或其他连接。

**三、命令级限流与终态投递**

- 命令速率按 `deviceId` 与命令族分别统计，默认 120/分钟；超限回 `resource.rate_limited`（可重试，可带 `retryAfterMs`），连续超限按 `4429` 关闭连接。
- 被接受的 mutation 终态 SHALL 由**提交方连接上有所有者的观察者**推送（连接断开即取消），与持久化终态事件同源。

**四、合同与文档同步**

- 本变更**不预期修改**已冻结的 Sync wire 合同（`schemas/sync/v1/`、`fixtures/sync/v1/`、`compatibility/`）。实现中若发现 schema 缺口，必须在同一变更内补齐并让 `npm run check` 全绿。
- `docs/SYNC_PROTOCOL.md` §11（命令与幂等）与 §10.3（事件视图）的实绩写回；`docs/MODULE_ARCHITECTURE.md` §4.9 与 `README.md` 的现状刷新。

**本次不包含**：

- 连接面、订阅、快照、配对 HTTP（属 `sync-server-connection`，本变更的**前置**）。
- 静态资源托管（`sync-ui-hosting`）与前端接线（`frontend-sync-wiring`）。
- `node-link-client` 与 `server::acp_facade`（切片 6）；真实 Codex/OMP 兼容套件。

## Capabilities

### New Capabilities

- `sync-command-handling`: Sync 命令面 —— 查询命令同步完成、mutation 先接受后终态、`(deviceId, requestId)` 幂等与冲突判定、按设备 scope 的逐命令授权与零副作用拒绝、`expectedVersion` 并发控制、命令级限流与超限关闭。
- `sync-event-projection`: Sync 事件面 —— 34 类已登记事件类型的 `view` 最低字段投影、ACP 原文的逐字节装配与不可用标记、未登记判别子的显式降级、以及终态事件与 mutation 的对应关系。

### Modified Capabilities

（无。本变更只新增能力，不改写既有规范的要求。）

## Impact

- **Rust**：`crates/server/src/sync/command/`、`crates/server/src/sync/event/`（新增）；`crates/server/src/sync/mod.rs`（模块登记）；复用 `crates/sync-protocol/src/views.rs` 的类型化视图与 `crates/server/src/node_link/resource.rs` 的 `wire_payload` 口径（算法相同、类型各自定义，不跨协议复用 DTO）。
- **依赖**：不新增任何 crate。
- **存储**：**不新增表、不推进文件格式版本** —— 幂等记录复用既有的 `owned_command`（mutation 的接受行与终态），查询类命令只在连接期去重。
- **机器合同**：预期零改动；`npm run check` 必须全绿。
- **权威文档**：`docs/SYNC_PROTOCOL.md` §10.3/§11、`docs/MODULE_ARCHITECTURE.md` §4.9、`README.md`。
- **验收边界**：本变更的可观察结果是**协议级闭环**（脚本化客户端能发 prompt 并收到 `accepted` → 领域事件 → 终态，能在无 scope 时被拒且零副作用，能触发限流），不是浏览器级可用；浏览器级证据要等 `sync-ui-hosting` 与 `frontend-sync-wiring` 合入。
