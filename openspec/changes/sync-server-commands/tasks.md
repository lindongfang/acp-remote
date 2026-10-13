# Tasks

## 1. 事件视图与原文装配（WP1）

- [ ] 1.1 新建 `crates/server/src/sync/event/`，在 `crates/server/src/sync/mod.rs` 追加模块声明；模块文档写明它只调用 `core::use_cases` 与 `sync-protocol` 的公开面，不 import `storage-sqlite`、不调用 `server::node_link`（算法口径相同但类型各自定义）。完成判据：`npm run check:boundaries` 通过。
- [ ] 1.2 实现视图投影：从持久化事件取 `payload_json` 的**原样文本**作为 `view`，调 `sync-protocol::views::project` 做形状判定；判定失败时记一条含 `event_type` 与缺失字段的结构化日志，但仍按原样下发该 `view`（不得丢弃事件、不得造序号空洞）。完成判据：单元测试覆盖「形状正确时按类型投影」「形状缺失时事件仍下发且日志可见」两条路径。
- [ ] 1.3 实现 `payload.acp` 装配：按 `acp.raw-payload.v1` 协商结果决定是否携带；`rawJson` 解码后的字节 MUST 与原始文档逐字节一致（**不得先解析再重新序列化**）；不可用时按已登记原因标记并给出字节长度。完成判据：单元测试用含超 64 位整数字面量与未识别字段的原文驱动，断言下发文本与输入逐字节相同。
- [ ] 1.4 实现不可用标记的三种原因与内嵌上限：超过 256 KiB 或 media type 非 `application/json` 时降级为不可用（原因区分 `size_limit` 与 `storage_failure`），保留期原因对应 `retention_expired`；未协商能力时不下发原文容器。完成判据：单元测试逐条覆盖三种原因与「未协商时不携带容器」，且断言未发生截断。
- [ ] 1.5 实现未登记内容的降级路径：权威协议未登记的事件类型与 ACP 判别子仍下发并保留原文或不可用标记，MUST NOT 静默丢弃、MUST NOT 降级为普通文本、MUST NOT 阻断同会话后续事件。完成判据：单元测试覆盖「未登记判别子仍有事件」「同会话后续事件投递不受影响」。
- [ ] 1.6 事件轨道与投递队列衔接：把投影后的事件交给前置变更实现的有界出站队列（按其水位处理），**不另建第二条队列**。完成判据：单元测试断言事件经由既有队列路径入队，且水位触发时投递层被告知停止读取重放批次。

## 2. 命令管线与查询命令（WP2）

- [ ] 2.1 新建 `crates/server/src/sync/command/` 与命令解码分派：按 `sync-protocol::command` 的 DTO 解码，封闭词表外与 `session.resume` 一律按 `command.unsupported` 拒绝。完成判据：单元测试断言 `session.resume` 在 Sync 面被拒且未在本机执行任何操作。
- [ ] 2.2 实现授权前置：每条业务命令在读取会话行、触碰文件系统、启动进程**之前**按设备 scope 判定；被拒时回 `authorization.scope_denied` 且零副作用，且不因会话是否存在改变错误类型。完成判据：单元测试断言「未授权设备对不存在的会话派发命令也只得授权拒绝」，并用一个记账替身证明未发生任何 core 读取调用。
- [ ] 2.3 实现 5 条查询命令：`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list` 同步返回一次 `completed` 或 `rejected`，结果形状与协议登记一致。完成判据：单元测试逐条断言结果形状，并断言任一查询命令都不会先回 `accepted`。
- [ ] 2.4 实现结果超限的显式失败：投递前计算编码后字节数，超过单条消息上限时回 `resource.result_too_large`，MUST NOT 截断后当完整结果返回。完成判据：单元测试用一个超大结果驱动，断言返回该错误码而非被截断的结果。
- [ ] 2.5 实现命令级限流：按设备标识与命令族分别计数，默认 120/分钟；超限回 `resource.rate_limited`（可重试，带 `retryAfterMs`）；连续超限达阈值时以 `4429` 关闭连接。完成判据：单元测试覆盖「单次超限只回可重试错误」「连续超限关闭连接」「不同命令族的额度互不挤占」。
- [ ] 2.6 实现查询类命令的连接期去重（不落盘）与命令 `messageId` 重复不重放业务结果。完成判据：单元测试断言同一查询在不同连接上各自独立、同一连接内重复 `messageId` 不产生第二次业务读取。

## 3. mutation 与终态（WP3）

- [ ] 3.1 实现 7 条 mutation 的接受路径（`session.prompt`、`session.cancel`、`permission.resolve`、`elicitation.respond`、`session.mode.set`、`session.config.set`、`session.create`）：先持久化接受并回 `accepted`（`result` 与 `terminalEventId` 均为 `null`），MUST NOT 在同一条响应里给出业务结果。完成判据：单元测试逐条断言首响应为 `accepted`，且接受行已持久化。
- [ ] 3.2 实现 `(deviceId, requestId)` 幂等：重复提交返回首次的已知接受或终态，MUST NOT 二次派发；查询类之外的 mutation 幂等由持久记录承担，MUST NOT 只放内存。完成判据：单元测试断言「同 requestId 重发只产生一个业务副作用」，并断言该性质跨「重新打开存储」仍成立。
- [ ] 3.3 实现冲突判定：比较 `command`、`sessionId`、`expectedVersion` 与**解码后 payload 的语义**（用 ACPR-CJ1 摘要比较，不得依赖 JSON 键顺序），不一致回 `command.idempotency_conflict`。完成判据：单元测试覆盖「键顺序不同但语义相同 → 不判冲突」与「值不同 → 判冲突」两条。
- [ ] 3.4 实现 `expectedVersion` 冲突：模式与配置设置必须携带版本，冲突时回 `state.version_conflict` 并给 `currentVersion`，且零副作用。完成判据：单元测试断言冲突时返回当前真实版本，且会话状态与版本未变。
- [ ] 3.5 实现终态观察者：按 `(deviceId, requestId)` 持有、连接断开即取消（不留 detached task）、终态从事件扇出匹配后入队该连接；MUST NOT 由适配层重新推导业务结果。完成判据：单元测试覆盖「提交方收到自己的终态」「断开后观察者被取消且无遗留任务」，并断言重连后可用 `command.status` 查到终态。
- [ ] 3.6 实现 `uncertain` 的处理：持久化 `command.uncertain` 后广播、MUST NOT 自动重派；`command.status` 对已终结 mutation 返回终态与原 `terminalEventId`。完成判据：单元测试断言「uncertain 不触发第二次派发」与「status 返回原终态标识」。

## 4. 文档同步（本变更自带）

- [ ] 4.1 更新 `docs/SYNC_PROTOCOL.md` §10.3/§11 的实绩写回（命令面与事件面已落地），保持「计划」类措辞只用于仍未实现的部分。完成判据：`npm run check:docs`。
- [ ] 4.2 更新 `docs/MODULE_ARCHITECTURE.md` §3/§4.9 的 `[现状]` 与 `README.md` 的「仓库当前状态」与 `server` 行。完成判据：`npm run check:docs`。

## 5. 协议级端到端验收（WP4）

- [ ] 5.1 在 `crates/app/tests/support/sync.rs` 追加命令相关方法（派发命令、读取 `command.result`、等待终态事件、读取事件视图）。完成判据：客户端能对真实 listener 派发一条查询命令并收到 `completed`。
- [ ] 5.2 在 `crates/app/tests/sync_commands_e2e.rs` 实现主链路：派发 `session.prompt` 收到 `accepted` → 观察同一轮次的领域事件（含视图最低字段断言）→ 收到 `command.completed` 且 `terminalEventId` 与终态事件一致。完成判据：用例断言各阶段可观察结果，并断言事件视图含协议要求的最低字段。
- [ ] 5.3 覆盖边界：未持 scope 的设备派发 mutation 得 `authorization.scope_denied` 且零副作用（用存储查询证明未写入事件、未启动进程）；同 `requestId` 重发只产生一个副作用；`session.resume` 在 Sync 面得 `command.unsupported`。完成判据：三条各有独立断言，且每条都能在实现错误时变红。
- [ ] 5.4 覆盖限流与原文保真：超限得到 `resource.rate_limited`（含 `retryAfterMs`）且被拒命令无副作用；一条含未识别字段的 ACP 原文经事件下发后逐字节可比。完成判据：两条各有独立断言。
- [ ] 5.5 在报告里如实登记隔离区限制：交付的是用例代码，真实运行由主 Agent 在合入后按 PV5 执行；不得声称已在隔离区通过。完成判据：`agent://<id>` 的报告含该说明。
