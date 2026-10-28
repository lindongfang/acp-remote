# ACP Remote Node Link 设计

> 对话顺序修订（2026-10-24，`preserve-agent-output-order`）：`session.read.resources.messages[].sequence` 与 `before.sequence` 使用 Owner 的 origin 序号，不使用 Owner 的独立 session 序号，也不映射为 Access 本机序号。Owner 持久保存片段起点；Access 仅连接期转发。消息页按权威序号分页，缺少可靠位置的旧历史保留为显式未知分区；具体字段与分段行为同 [Sync Protocol](SYNC_PROTOCOL.md) §11.5。不支持旧版本排序兼容。

> 状态：Node Link v1 wire 标准已冻结；`node-link-protocol` crate 已实现 §9.3/§9.4 的 transcript domain/tag 表（含固定向量测试）、v1 的全部 30 个消息类型的类型化 body（握手、catalog、resource、command、error）与信封分派，以及配对 HTTPS 载荷。Owner 侧的节点状态机（attachment 当前性、origin cursor 单调性、命令幂等与 `uncertain`、Export 可见性与授权、撤销传播）已由 `server::node_link` 落地（切片 5，`node-link-owner`）；Access 侧的出站客户端 crate `node-link-client` 已落地（WSS 握手、单任务重连与撤销、catalog 无正文投影、`commit_receipt` → 发布 → `resource.ack`、远程 `SessionEndpoint`/`SessionBackendFactory`、独立附件连接与有界内存转发）；**已接线**的是 Access 侧配对（`node pair --mode access` 的 HTTPS claim/status 与 Owner 信任落盘）、Daemon 启动时对 `NodeLinkClient`/Owner 连接的实例化，以及 `AccessManager` 向本地调用方暴露的远程 `SessionEndpoint` 与附件转发入口（2026-10-20，WP5d）；仍未落地的是 `server::acp_facade`（本地 ACP 客户端的业务命令转发）。  
> 版本：1.0  
> 修订记录（2026-10-26，`session-create-and-empty-cleanup` WP1，**docs-only：无 wire 变更**）：新增 §12.11 登记会话删除的边界——`session.delete` 不在 Node Link 登记或暴露、`session.deleted` 不跨节点转发（§11.4 的共享事件集合随之排除该类型）、Access 按 Owner 当前 catalog/快照丢弃运行期目录条目并返回常规 not-found、不持久化任何远程删除状态（索引/墓碑/事件镜像/正文/游标/receipt/命令状态），重连一律回源，imported 会话不可再导出。§11.5 补一条未协商口径。消息形状、feature 表、错误码与 fixture 均未变。
> 修订记录（2026-10-21，custom-session-title（合同与实现））：§11.3 登记 `core.session-title.v1` 并宣告（Owner 持久化、Access 转发已接线）；§11.5 补未协商时的拒绝口径；§12.7 新增 `session.title.set`（closed payload `{ title }`，输入按 wire 原文 1..=512 个 Unicode 标量值、保存 trim 后的文本），`command.submit` 的命令名计数更新为 14。**取代**旧条款「标题仅由 Agent 更新、不存在客户端重命名入口」（原文见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.3 与 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §5；本文件此前没有单独的标题写入条款）；Owner 侧持久化与命令接线已落地。  
> 修订记录（2026-10-19，`conversation-area-complete` 的合同包，无实现变更）：§11.3 登记 `node-link.attachment-transfer.v1`/`node-link.typed-media.v1`/`node-link.pending-interactions.v1`/`node-link.remote-history.v1`；新增 §11.5（旧 peer 与未协商行为）、§12.9（独立已认证附件连接、13 个附件帧与固定预算）与 §12.10（有效交互与远程历史）；§12.7 的 `session.read`/`session.prompt`/`elicitation.respond`/`permission.resolve` 增加协商后字段；§14.1 增补附件/媒体/交互错误码与 details 登记；§2.5、§17、§18 同步预算、范围与必测场景。实现与 fixture 之外的 wire 行为仍标为计划/待实施，`node-link-client` 仍未落地。  
> 修订记录（2026-10-16，`pairing-model`）：**删除节点配对的短验证码**——§9.2 的 domain 表与 §9.4 的字段集合删除 `acp-remote/node-link-pairing-sas/v1` 一行与 SAS 计算规则，§8.2 与 §13.2 的确认仪式去掉 SAS（本机界面仍显示节点名、类型、指纹与待授予的 `grant.*`），机器资产（`compatibility/transcripts/v1/transcripts.json` 与节点侧固定向量）同步删除。**设备方向不再有授权维度**——§3.4 与 §10 的措辞收窄为「设备通过认证即可执行本节点命令」，`pack.*`/`preset.*` 两类词汇（设备授权包与配对预设）删除，跨节点导出授权 `grant.*`、Export `scopes` 与 Import `grants` 边界不变。Node Link 面的这次修订**不改 wire**：本节列出的都是节点配对仪式与词汇表口径，消息形状、握手、catalog、resource、command 与 `fixtures/node-link/v1/` 的其余向量不变（仅删除 `transcripts/pairing-sas.json` 与 `manifest.json` 的对应条目；§12.7 的 `session.create` 仍只接受 `workspaceDirectory`、请求不得携带任意 Owner 路径）。  
> 修订记录（2026-10-16，`session-create-settle-order`，无 wire 变更）：§12.5 明确判定「该命令有没有产生业务副作用」以**资源行是否已提交**为准（对 `session.create` 即会话行），MUST NOT 用「该 `requestId` 是否存在持久记录」代替（受理记录在受理阶段就已存在），并补上「可证明未产生副作用 ⇒ `failed`」与崩溃窗口（受理与落账之间崩溃的受理记录由启动恢复结 `uncertain`）；§12.7 的结果契约补「受理只落幂等记录、会话行 + 会话标识回填 + 终态必须同一次提交、后端失败不留会话行」，`failed`/`uncertain` 的分类与 `command.status` 的重放口径随之改写。消息形状、错误码登记与 fixture 未变。  
> 修订记录（2026-10-12，文档登记，无 wire 变更，`pwa-conversation-ui` 变更）：§12.7 与 §17 登记「Node Link 面无结构化历史」这一**尚未实现**的已知缺口——本次变更将在 Sync 面新增历史条目读取（`session.read` 的 `history` include 与 `beforeHistory` 游标，与它同批落地），该读取只在 Sync 面生效，本面不登记对应的 feature，经本面展示的 imported 会话在客户端侧对结构化历史显式降级。wire schema、消息族、错误码登记、§11.3 feature 表与 `fixtures/node-link/v1/` 均不变。  
> 修订记录（2026-09-27，v1 内合同修订，未实现未发布，`node-trust-export-ids` 变更）：§8.2 的信任记录新增 `exportIds`（按节点收窄可见 Export 的白名单）——可见性三条件、清单只能收窄、`export.revoke` 不级联清理清单条目、只在配对确认时填报（无配对后修改入口，属已知限制）、v3 及更早的库升级到 v4 后既有节点行清单为空（不得默认放权）。Node Link wire 与 fixture 不变：`exportIds` 不进握手/catalog/resource/command，消息形状与错误码登记均未动；并写明重新配对（同一 `nodeId` 再次经 `node.pair.begin`/claim/`node.pair.confirm` 落定，含密钥变化后的重新配对）在落定后立即关闭该节点的活动连接，收窄因此自下一次握手后的新连接生效；并明确该关闭与 `node.revoke` 的**语义与关闭码不同**：重新配对**不**推送 `node.trust.revoked`、以 `1000`（正常关闭）关闭并给出说明「信任已重新确认、请重连以重取 catalog」的 close reason，因此 §15 的「撤销即停」不适用、该节点仍按 §15 自动重连；只有 `node.revoke` 才推送 `node.trust.revoked` 并以 `4410` 关闭、停止重连。  
> 修订记录（2026-09-26，v1 内合同修订，未实现未发布，`node-link-owner` 的 WP6 修复轮次）：§12.7 补注本切片的**结果投影范围**——`session.read`/`session.mode.list`/`session.config.list` 回 `nodelink.command.unsupported`（结果投影属 Access facade 的正文切片），Owner 不得返回被裁剪的结果。wire schema、消息与错误码登记未变。  
> 修订记录（2026-09-26，v1 内合同修订，未实现未发布）：`node.challenge` 增加必需字段 `catalogRevision`(decimal string)——§9.3/§9.4 的两个连接 transcript domain 都含 tag 6 `catalogRevision`，而此前的握手消息不带该字段，Access 无法在首次连接上验证 `nodeProof` 或构造自己的 proof（design.md D13 的用户裁决 A）；§8.2 明确首阶段 Export 可见性只取「未撤销且 `export.scopes ∩` 信任记录 `grants ≠ ∅`」，`exportIds` 维度推后（用户裁决 (b)，与 `node-link-owner` 的 R51/R52 一致）（其中 `exportIds` 推后一条已被 2026-09-27 的修订取代，见上一条）。  
> 修订记录（2026-09-18，v1 内合同修订，未实现未发布）：`resource.event`/`resource.ack` 增加必需 `sessionRef`；§6 无正文索引增加 `sessionId`；`command.accepted`/`command.rejected`/`command.terminal` 增加必需 `command`；`session.create` 补齐结果契约（`SessionCreateResult`）；`payloadDigest`/`snapshotDigest` 前像改为 ACPR-CJ1 与 SYNC §9.4 规则；`payload` 允许只带 `acp`；握手阶段 `link.error` 允许省略 `connectionId`/`connectionSequence`；新增错误码 `nodelink.resource.rate_limited` 与 §2.5 固定限流；Export 增加 `defaultWorkspaceDirectory`/`templates`；新增 §11.4 事件类型共享合同；§14.1 新增 `details` 登记表并为 `nodelink.protocol.feature_required`/`nodelink.export.not_granted`/`nodelink.resource.rate_limited`/`nodelink.command.unsupported_field` 登记机器可读字段（兼容新增）；§12.7 的 `elicitation.respond` 增加 `decline` 动作并把 `submit` 的 `values` 放宽为 `object|null`（对齐 ACP 的 `accept`/`decline`/`cancel`，兼容新增）。  
> 日期：2026-09-18  
> 上位产品设计：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)  
> 模块边界：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)  
> 已接受决策：[ADR-0002](../openspec/adr/0002-node-link-first-owner-authority.md)
> 机器可验证资产：[`schemas/node-link/v1/`](../schemas/node-link/v1/)、[`fixtures/node-link/v1/`](../fixtures/node-link/v1/)

## 1. 文档职责

Node Link 是两个 ACP Remote 节点之间的安全访问协议。它解决“一个节点拥有 Agent，另一个节点向本地 Zed、CLI、PWA 或原生客户端提供该 Agent”的场景。

Node Link 不等于：

- ACP stdio 的透明 TCP 转发；
- 手机/PWA 使用的 Sync Protocol；
- Tailscale、VPN 或某个具体网络产品；
- 云端账号中心或持久中继；
- 自动形成传递信任的节点 federation。

本文是 Node Link 语义、不变量与 **v1 wire 标准**的权威来源：§2 冻结传输、信封、版本协商与 limits，§9 冻结节点专属 transcript，§12 冻结全部消息体，§13 冻结节点配对 HTTP，§14 冻结错误码与 close code。机器表达位于 [`schemas/node-link/v1/`](../schemas/node-link/v1/)，固定向量位于 [`fixtures/node-link/v1/`](../fixtures/node-link/v1/)；三者不一致时必须先修正文档与资产，不能任选其一。

命令名与 Owner 本地管理能力的唯一来源是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json)。本文只引用这些名字，不重新定义。

本文不定义 ACP 本身、Broker 领域模型、SQLite schema、UI 组件或 Sync wire。Sync 由 [`SYNC_PROTOCOL.md`](./SYNC_PROTOCOL.md) 独立定义，两套协议不共享 DTO；两者只在命令名、授权词汇和 ACP 语义上对齐。

## 2. Wire Contract

### 2.1 传输与 framing

| 项目 | v1 规则 |
|---|---|
| 端点 | `wss://<host>/node-link/v1` |
| WebSocket subprotocol | `acp-remote.nodelink.v1.json` |
| 帧类型 | 只接受 text frame；收到 binary frame 返回 `link.error`（`nodelink.protocol.invalid_json`）并以 `4400` 关闭 |
| 负载 | UTF-8 JSON 文本，不允许 BOM、重复对象键、`NaN`、`Infinity` 或尾随内容 |
| 压缩 | v1 不接受 `permessage-deflate`；协商到压缩的 upgrade 必须拒绝 |
| 消息模型 | 一帧一条完整 JSON message；v1 不支持分片组装出的逻辑消息 |
| 认证顺序 | 连接建立后的第一条消息必须是 `node.hello`；认证完成前的业务消息以 `4401` 关闭 |

Node Link 不依赖 TLS 提供的节点身份：TLS 只保护传输，节点身份由 §9 的 transcript 证明。TLS 终止点必须位于对应节点的可信边界内。

`[决定]` 本端点与 Sync 的 `/sync/v1`、两者的配对 HTTP 路径（§13.1）**共用同一个 listener**（`CONFIG_REFERENCE.md` §1 的 `daemon.port`/`daemon.bind`），按 path 路由，不需要单独配置端口。因为对端是另一台机器，只绑定 loopback 时本端点只在同机测试下可用；对外暴露必须显式绑定非 loopback 地址（`direct` TLS）或经同机可信反代（`proxy` 模式只允许 loopback），具体形态见 `CONFIG_REFERENCE.md` §1 的三形态表。

### 2.2 消息信封

所有 WSS 消息使用同一外层结构：

```json
{
  "protocolVersion": 1,
  "type": "resource.event",
  "messageId": "018f6f89-8a23-7a10-a0d3-f92e6a31d952",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "17",
  "body": {}
}
```

| 字段 | 类型 | 必需性 | 规则 |
|---|---|---|---|
| `protocolVersion` | integer | 必需 | 当前选择的 major wire version，v1 固定为 `1` |
| `type` | string | 必需 | §12 登记的消息类型 |
| `messageId` | UUID | 必需 | 每条发送消息唯一，只用于诊断，不代替 `requestId` 或 `originEventId` |
| `connectionId` | UUID | 认证后必需 | 认证前（`node.hello`/`node.challenge`/`node.proof`）必须省略 |
| `connectionSequence` | decimal string | 认证后必需 | 每个方向独立，从 `"1"` 开始严格加一；认证前必须省略 |
| `body` | object | 必需 | 与 `type` 对应的结构化内容 |

- framing、序号编码、未知字段和 schema 规则与 `SYNC_PROTOCOL.md` §3.3、§4 一致：序号是**无前导零十进制字符串**，结构性常量（`chunkCount`、`schemaVersion`、`heartbeatIntervalMs`、`limits.*`）保持 integer；时间戳是毫秒精度 UTC RFC 3339。
- 两个方向分别维护 `connectionSequence`，不能共用计数器；重复、回退、跳号或 `connectionId` 不匹配都返回 `nodelink.protocol.sequence_invalid`。
- `messageId` 重复不重放业务结果；命令幂等只看 `requestId`（§15）。
- 认证前（`node.hello`/`node.challenge`/`node.proof` 以及握手阶段发生的 `link.error`）必须省略 `connectionId` 和 `connectionSequence`；认证完成后两者必须存在。

### 2.3 版本协商

- `node.hello` 同时声明 `minProtocolVersion` 和 `maxProtocolVersion`。
- Owner 在 `node.challenge` 中用 `selectedProtocolVersion` 返回选择结果；v1 只有版本 `1`，因此两个字段都固定为 `1`。
- 无法形成交集时 Owner 返回 `link.error`（`nodelink.protocol.version_unsupported`）并以 `4406` 关闭。
- feature 协商规则见 §11；Access 声明的必需 feature 未被选择时，握手不得进入业务阶段。

### 2.4 未知字段、未知 type 与 schema 规则

- v1 控制信封、全部消息 body 与 `command.submit.payload` 是 closed object；未知字段返回 `nodelink.protocol.schema_invalid`。
- 明确的开放扩展点是：`resource.event.payload.view`（事件视图，未知事件按降级规则处理）、`resource.event.payload.acp`（ACP raw 文档必须逐字节保真）、错误 `details`，以及一个由外部 schema 约束的容器 `elicitation.respond.payload.values`（键由该次 elicitation 声明）。schema 用 `additionalProperties: true` 标注这四处，其他位置一律 `additionalProperties: false`。
- 唯一例外见 §12.7：`command.submit` 中 `payload` 直接出现 `cwd`、`mcpServers`、绝对路径或凭据字段时，返回更具体的 `nodelink.command.unsupported_field`，而不是通用的 `nodelink.protocol.schema_invalid`。
- 未知 `type` 返回 `nodelink.protocol.type_unsupported`，不得静默丢弃；`post_mvp` 消息族（§12.1）在 v1 首切片一律按未知 type 处理。
- 标为必需的字段缺失返回 `nodelink.protocol.schema_invalid`；只有写成 `T | null` 的字段可以为 `null`，其他可选字段应省略，不用 `null` 代替缺失。
- ID、序号、时间戳、枚举和长度限制必须在 wire DTO 边界验证，不能延迟到 Agent adapter。
- JSON Schema 是本文结构化合同的机器表达，位于 [`schemas/node-link/v1/`](../schemas/node-link/v1/)；契约测试必须消费 [`fixtures/node-link/v1/manifest.json`](../fixtures/node-link/v1/manifest.json) 中的正反用例。

### 2.5 Limits 与 Backpressure

固定 v1 上限：

| 项目 | 默认值 | 是否可下调 |
|---|---:|---|
| 单条 WebSocket JSON message | 1 MiB | 是，经 `node.ready.limits.maxMessageBytes` |
| catalog 快照各列表条数（agents/directories/sessions） | 500 | 是，经 `node.ready.limits.catalogSnapshotBatchSize` |
| 单个 resource 快照批次 `items` 条数 | 500 | 是，经 `node.ready.limits.resourceSnapshotBatchSize` |
| 单连接 in-flight command 数 | 32 | 是，经 `node.ready.limits.maxInFlightCommands` |
| 单连接待发送队列 | 8 MiB 或 2,000 条，先到者触发 | 是，经 `node.ready.limits.maxPendingQueueBytes` / `maxPendingQueueMessages` |
| heartbeat interval | 30 秒 | 是，经 `node.ready.limits.heartbeatIntervalMs` |
| 握手超时 | 15 秒 | 否 |
| heartbeat 超时 | 90 秒 | 否 |
| JSON nesting depth | 64 | 否 |
| 单对象字段数 | 1,024 | 否 |
| 单数组元素数 | 10,000 | 否 |
| 节点名称 UTF-8 长度 | 128 bytes | 否 |
| 单 IP 新认证尝试 | 10/分钟 | 否 |
| 单连接命令速率 | 120/分钟 | 否 |
| 单连接并发 snapshot | 1 | 否 |
| pairing claim 速率 | 10/分钟/IP | 否 |
| pairing status 速率 | 60/分钟/pairingId | 否 |
| 配对有效期 | 5 分钟 | 否 |

- `node.ready.limits` 只能把上表标为可下调的值调低，不能上调；未收到的 limits 使用本表默认值。
- 整个握手（`node.hello` → `node.ready`）必须在 15 秒内完成，否则任一方以 `4408` 关闭。
- 连续 90 秒无任何入站消息或 `link.pong`，Owner 可以关闭连接。
- 超过单消息上限应在分配大对象前以 `1009` 拒绝。
- `resource.caught_up`/`resource.reset_required`/`resource.unavailable` 与收窄后的 `resource.ack` **不新增上限**：单帧、单连接待发送队列与来源重放批次沿用本表既有预算；Access 为重建持有的来源缓冲计入同一有界预算，超限必须明确 backpressure 或重建，不得落盘扩容（design D3/D4）。
- 待发送队列达到高水位后先停止读取新的 snapshot 批次；持续过慢时发送 `link.backpressure`（`post_mvp`）并断开，由 origin cursor 重放补回（§15）。
- v1 不允许 diff 或终端输出通过“再分片”绕过上限；附件正文走 §12.9 的独立已认证连接（32 KiB 分块、每传输一块未确认、单连接 2 个/单节点 8 个活动传输），其发送预算与速率按附件族独立计数，不占用本节业务命令/消息配额。除此之外的正文仍走 `resource.event` 的既有上限或 `post_mvp` 的独立内容 feature。
- 超过单连接命令速率时返回 `link.error`（`code = "nodelink.resource.rate_limited"`，`retryable = true`），并在连续超限时以 `4429` 关闭连接；`details.retryAfterMs` 给出建议退避毫秒数，存在时 Access 必须遵守后再重试。该拒绝（以及单连接 in-flight 上限的拒绝）有明确的关联命令：`correlationId` 取被拒命令帧的 `messageId`，Access 立即结清该请求——admission 拒绝代表命令**未执行**，不得登记 `uncertain`。
- 单个内嵌 diff/document 超过 256 KiB 时，事件内 `payload.acp` 使用 §12.4 的 `rawAcp.rawUnavailable`（`reason = "size_limit"`），查询结果返回 `nodelink.resource.snapshot_unavailable`，不得截断后伪装成完整内容。

## 3. 术语

### 3.1 ACP Remote Node

一个运行 ACP Remote Daemon、拥有稳定 `nodeId` 和 Node Identity Key 的实例。节点可以同时承担多种角色，不需要安装成不同产品。

### 3.2 Owner Node

直接管理某个 Agent 或会话的资源归属节点：

- 是该 Agent 的 ACP Client；
- 管理 Agent 生命周期和 workspace；
- 是会话状态、active turn、事件日志和命令终态的唯一权威；
- 决定向其他节点导出哪些 Agent、会话和操作权限。

### 3.3 Access Node

从 Owner Node 导入 Agent/会话并向本地客户端提供入口的访问节点：

- 可以通过本地 `acp_facade` 服务 Zed；
- 可以通过 Sync Server 服务 PWA、手机或桌面 UI；
- 远程正文与事件只在已认证连接期经有界内存目录/缓冲转发，不写本地交付索引、cursor/ACK 或 local sequence，也不保存远程会话正文；断线、撤销或来源 epoch 变化时整代清除；
- 不能扩大 Owner Node 授予的能力。

### 3.4 Client

跨节点的能力由「该节点已配对且未撤销」这一条信任事实决定。

### 3.5 Owner 与 Access

- **Owner Node** 是资源的唯一权威：本机会话、事件、命令终态与审计都只写它的 SQLite。
- **Access Node** 通过 Node Link 访问 Owner 的资源；默认 `no-content-cache`：远程正文、事件与命令结果只经已认证连接期的有界内存目录/缓冲转发，不写本地交付索引、cursor/ACK 或 local sequence，断线/撤销即整代清除。Access 只持久化自己的 Import 配置、远程 endpoint 与本地 principal/授权。
- 节点之间没有 Export/Import 记录：一次成功的配对就是该节点可以访问本节点全部资源的信任事实（`local.*` 除外）。

## 4. 目标拓扑

```text
远程 Zed
   │ local ACP stdio
   ▼
Access Node / acp_facade
   │
   │ Node Link over trusted HTTPS/WSS path
   ▼
Owner Node / Broker ── ACP stdio ── Company Agent
   ▲
   ├── 公司本地 Zed / CLI
   └── 直接连接的 PWA / 手机 / 电脑客户端
```

同一节点可以既导入远程 Agent，又导出自己的本地 Agent。第一阶段每个资源只允许一个 Node Link hop：导入的 Agent 可以提供给本节点本地客户端，但不能再次通过 Node Link 导出给第三个节点。

## 5. 为什么不直接转发 ACP

直接把 ACP JSON-RPC 放进 WSS tunnel 无法独立解决：

- 节点身份和长期配对；
- Agent/会话导出策略；
- 多个本地客户端的授权与审计；
- 断线 cursor、事件重放和慢消费者隔离；
- 跨节点命令幂等与 `uncertain`；
- Owner Node 权威和 Access Node 缓存边界；
- 防止 capability 在多跳链路上被虚报。

因此 Node Link 使用专用、版本化的 command/event/catalog 协议，并在需要时携带 ACP raw document。两端仍通过 ACP compatibility matrix 保证端到端语义，不把 Node Link 变成最低公共能力抽象。

## 6. 权威模型

一个状态只能有一个权威写入者：

| 状态 | 权威写入者 |
|---|---|
| Agent 配置、凭据、workspace | Owner Node |

| Session、turn、permission、模型状态 | Owner Node |
| Origin event sequence 与命令终态 | Owner Node |
| Import 配置、远程 endpoint | Access Node |
| Access Node 本地 principal 与本地授权 | Access Node |

第一阶段默认使用 `no-content-cache`。Access Node 不把远程会话正文、事件或任何交付索引写入本地 SQLite：来源事件在内存中只保留 Owner 的来源坐标、事件类型与摘要，经已认证连接期逐字转发给本地客户端，断线、撤销或来源 epoch 变化即整代清除：

```text
ownerNodeId
sessionId
originEventId
originEpoch
originSequence
eventType
payloadDigest
```

本节点不生成 `localSequence`，来源坐标也不得被本地序号覆盖或伪装。Access 不写本地交付索引、cursor/ACK，也不「先落盘再广播」：排序、ACK 与去重一律以 `(originEpoch, originSequence)`（去重键还含 `ownerNodeId`/`sessionId`）为准，正文重放按 origin cursor 向 Owner 请求（§12.4）。Prompt、Agent 回复、工具内容、diff、终端输出、附件和 ACP raw document 只允许在内存中完成有界转发。Owner Node 离线时，Access Node 不展示会话正文，也不能把 prompt 标记为已接受。

Owner 的 `no-content-cache` 策略和独立 ADR

Owner 必须把向此类客户端交付正文视为显式数据披露

## 7. 资源标识

所有跨节点资源使用复合身份，不能只传本地裸 ID：

```text
RemoteAgentRef  = ownerNodeId + agentId
RemoteSessionRef = ownerNodeId + sessionId
OriginEventRef  = ownerNodeId + originEpoch + originEventId
LiveSessionRoute = RemoteSessionRef + attachmentId + attachmentGeneration
```

Access Node 可以生成本地 opaque handle，但持久化层必须保留完整来源。来自不同 Owner Node 的相同 `sessionId` 永远不是同一个会话。

`RemoteSessionRef` 是耐久资源身份，`attachmentId/attachmentGeneration` 是当前 Node Link 连接上的临时路由凭据。Access 每次重新连接或重新 attach 会话时必须取得新的 attachment；旧连接延迟到达的 command/event frame 必须被 Owner 拒绝（`nodelink.resource.attach_generation_stale`），不能仅凭相同 `sessionId` 投递到新连接。这一分离参考 Pi 的 durable Session 与 live presentation attachment 模型，但 Node Link 仍使用自己的认证、授权和 wire schema。

`originEpoch` 是**会话级** origin epoch：由 Owner 在会话首次持久化写入时生成，`(originEpoch, originSequence)` 因此构成该会话的 origin cursor，不能跨会话比较；`originEventId` 在该会话内全局唯一且跨跳不变。Access 侧不得重新生成、重编号或本地改写这两个值，也不得把 `originEpoch` 当作节点级 epoch。

## 8. 身份、配对与信任

### 8.1 Node Identity

Node Identity 与 PWA Device Identity 是不同用途的长期身份：

- 密钥算法第一阶段仍采用 ECDSA P-256，以复用已审查实现；
- 一个节点只有一把 Node Identity Key：它同时用于 Sync 的 `hostProof`（wire 字段名 `hostId`/`hostPublicKey`）与 Node Link 的 `nodeProof`；两种用途使用不同 domain tag（§9.2）、不同 transcript 字段集合（§9.4）和不同信任记录类型，密钥本身不复制、不派生第二把；
- “Node key 与 Device key 分离”只约束 Node 与 Device 之间：Node 的信任记录不得复用 Device 的 record type、撤销记录或签名 domain；
- Node 私钥不能导出给浏览器或通过二维码传输；
- TLS 临时连接密钥仍与长期 Node Identity Key 分离。

### 8.2 长期配对

节点首次通过二维码或一次性配对码建立信任（§13）。Owner 本地确认时只核对节点名称、类型与指纹，不填报任何 Export 清单或 `grant.*`：

- 展示名来自本机配置 `[node] name`（缺失时回落到 `nodeId` 前缀），MUST NOT 取自配对请求；
- 确认成功后该节点即为 `paired`：已认证即可执行本节点全部业务命令，`local.*` 永不远程可用；
- 撤销（`node.revoke`）立即关闭该节点的活动连接；重新配对同样在提交后关闭既有连接，要求对端重连；
- 配对不产生传递信任：Access 的本地客户端与下游节点不因这次配对获得任何额外身份。

### 8.3 无传递信任

`Owner A` 信任 `Access B` 不代表 A 信任 B 的本地设备，也不代表 A 信任 B 已配对的 `Node C`。第一阶段：

- Owner 只把 Access Node 视为授权 principal；
- Access Node 对其本地 Zed/PWA/CLI 负责；
- 有效权限是「已配对且未撤销的节点」∩ 端到端 capability；
- imported Agent 禁止再次通过 Node Link 导出；
- 审计记录分两侧，**不靠 wire 传递**：**Owner 侧**的命令审计行只记 `viaNodeId`（即 `AuditRecord.via_node`），`localPrincipalRef`/`local_principal_ref` 为 `null`——v1 的 `command.submit` 与握手上没有承载它的字段，Owner 也**不得**假设能知道对端本地是谁；**Access 侧**的审计行记录它自己认证的本地 principal（`local_principal_ref`），只供本节点归因。因此 Owner 不把 `localPrincipalRef` 当成自己直接认证的身份；将来若要让 Owner 得到它，必须在握手或 `command.submit` 上定义字段并走 §2.3 的兼容流程，不能把现有字段当已有能力使用。

这是第一阶段明确接受的节点级信任模型。Owner 可以按 Access Node 单独授权、限流和撤销，但不能据此声称已经端到端认证实际操作人。需要 Owner 直接认证员工身份时，必须新增独立用户身份协议，不能把 `localPrincipalRef` 升格为安全凭据。

## 9. Node Link Transcript v1

### 9.1 算法与 codec

Node Link 与 `SYNC_PROTOCOL.md` §6.1、§6.2 复用同一套算法与二进制 codec：

- 签名：ECDSA P-256 + SHA-256，签名格式为 IEEE P1363 固定 64 bytes（`r(32) || s(32)`），DER 不得出现在 wire 或 transcript 中。
- 公钥：SEC1 uncompressed point，固定 65 bytes（`0x04 || X(32) || Y(32)`）。
- HMAC：HMAC-SHA256；nonce 与 `pairingSecret` 为 CSPRNG 生成的 32 bytes。
- codec：magic `ACPR`、`codecVersion 0x01`，字段按 `fieldTag` 严格递增、只能出现一次，字符串为无 NUL 的 UTF-8 原始字节，UUID 解码为 16 字节。

线上字段本身（签名、HMAC 结果、nonce、公钥）使用无填充 base64url。Node Link 只新增 domain 与 `fieldTag` 含义；codec 结构与 Sync 完全一致，因此 Rust 与 WebCrypto 可以共享同一 codec 实现与 fixture 校验器。

Node Link 的 `fieldTag` 表**独立于** `SYNC_PROTOCOL.md` §6.3 的全局表：同一个 tag 编号在两套协议里含义不同，实现不得跨协议复用 tag 映射，也不得把 Sync 的字段集合套用到 Node Link。

### 9.2 Domain 表

| 用途 | domainTag | 证明方式 |
|---|---|---|
| 配对 claim 证明 | `acp-remote/node-link-pairing-proof/v1` | HMAC-SHA256（key = `pairingSecret`） |
| 配对 Owner 证明 | `acp-remote/node-link-pairing-owner-proof/v1` | ECDSA，Owner Node Identity Key |
| 配对状态证明 | `acp-remote/node-link-pairing-status/v1` | HMAC-SHA256（key = `pairingSecret`） |
| 连接节点挑战 | `acp-remote/node-link-challenge/v1` | ECDSA，Owner Node Identity Key |
| 连接节点证明 | `acp-remote/node-link-proof/v1` | ECDSA，Access Node Identity Key |

domain 字符串必须逐字节一致；不得通过大小写、尾随 `/` 或版本后缀变体绕过 domain 分离。

### 9.3 字段 Tag 表

| Tag | 名称 | 编码 |
|---:|---|---|
| 1 | `protocolVersion` | u16be |
| 2 | `ownerNodeId` | UUID 16 bytes |
| 3 | `accessNodeId` | UUID 16 bytes |
| 4 | `pairingId` | UUID 16 bytes |
| 5 | `pairingExpiresAt` | Unix 秒 u64be |
| 6 | `catalogRevision` | u64be |
| 7 | `ownerPublicKey` | SEC1 65 bytes |
| 8 | `accessPublicKey` | SEC1 65 bytes |
| 9 | `clientNonce` | 32 bytes |
| 10 | `serverNonce` | 32 bytes |
| 11 | `connectionId` | UUID 16 bytes |
| 12 | `negotiatedFeatures` | 排序且以 NUL 分隔的 UTF-8 bytes |
| 13 | `pairingRequestId` | UUID 16 bytes |
| 14 | `nodeName` | UTF-8，最多 128 bytes |
| 15 | `nodeKind` | UTF-8 枚举值（`owner`/`access`） |
| 16 | `requestNonce` | 32 bytes |

### 9.4 各 domain 字段集合

| domain | 字段集合（按 tag 升序） |
|---|---|
| `node-link-pairing-proof/v1` | `1,2,3,4,5,8,9,14,15` |
| `node-link-pairing-owner-proof/v1` | `1,2,3,4,7,8,9,10,13` |
| `node-link-pairing-status/v1` | `1,2,3,4,13,16` |
| `node-link-challenge/v1` | `1,2,3,6,9,10,11,12` |
| `node-link-proof/v1` | `1,2,3,6,9,10,11,12` |

- `node-link-challenge/v1` 与 `node-link-proof/v1` 字段集合相同但 domain 不同，且分别由 Owner 与 Access 的密钥签名，避免签名跨角色复用。
- 本表**不含短验证码 domain**：节点配对不派生、不下发也不显示 SAS（信任锚与理由见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §5.1）。
- HMAC 比较必须使用 constant-time compare；`accessPublicKey`/`ownerPublicKey` 必须先验证为曲线上的非无穷点。
- 不允许省略本表登记的字段，也不允许加入未登记字段；验证方先检查长度与类型，再执行密码学操作。

### 9.5 固定互操作向量

跨语言固定向量位于 [`fixtures/node-link/v1/transcripts/`](../fixtures/node-link/v1/transcripts/)，每个 domain 一份，包含 `input`、`expected.transcriptBase64url`、`expected.transcriptSha256Hex`，并按证明方式附带 `expected.hmacKeyBase64url`/`hmacSha256` 或 `expected.publicKey`/`p1363Signature`。测试私钥仅用于公开 fixture，不得用于真实节点。

`fixtures/node-link/v1/manifest.json` 的 `transcriptVectors` 是这五份向量的路径清单；没有共享 fixture 不得宣称 Rust/WebCrypto 互操作完成。

## 10. 授权边界（无 Export 维度）

配对即完全信任：已配对且未撤销的节点可以读取全部会话、提交命令、在本机创建会话并批准权限请求。仍然保留的边界：

- `local.*` 本地管理能力永不远程授予；
- 连接/消息/频率上限与协议 schema 校验照常执行；
- 配对确认必须由本机用户完成，撤销即时生效；
- Access 默认不持久化远程会话正文；
- 审计保留 actor/目标/结果与摘要，不记正文或凭据。

## 11. 能力协商

### 11.1 端到端能力交集

端到端可宣告能力必须取以下交集：

```text
downstream Agent capability
∩ Owner Broker implementation
∩ 本机 capability
∩ Node Link negotiated feature
∩ Access Broker implementation
∩ local client/facade capability
```

任一环节缺失都不能向 Zed 或其他上游客户端宣告支持。ACP raw payload 必须跨 Node Link 保真；无法形成公共 view 时使用 raw fallback 或显式不支持，不能静默丢弃。

### 11.2 握手交换

Node Link 握手交换：

- protocol versions 与 feature IDs（`node.hello` → `node.challenge`）；
- 节点身份和 connection nonces（§9.4 的两个连接 domain）；
- catalog revision（`catalog.subscribe` / `catalog.snapshot`）；
- max message、snapshot 批次、in-flight command、待发送队列与 heartbeat limits（`node.ready.limits`，§2.5）；
- ACP wire versions 与 raw payload 支持（`node-link.raw-acp.v1`）；
- 压缩与 binary 等可选能力：v1 一律不协商、不使用（§2.1）。

### 11.3 Feature ID

取值集合是**封闭词表**，由 [`compatibility/features/v1/features.json`](../compatibility/features/v1/features.json) 机器登记；`npm run check` 的 `check:features` 断言本表、registry 与 fixture 中出现的 ID 三者逐一相等。新增或删除 ID 必须先改 registry 再改本表。

| feature | 说明 | delivery | 必需 |
|---|---|---|---|
| `node-link.core.v1` | 握手、catalog、resource snapshot/event/ACK、错误与 limits | mvp | 是 |
| `node-link.raw-acp.v1` | `resource.event.payload.acp` 携带 ACP raw document 并保真 | mvp | 否 |
| `node-link.command-status.v1` | `command.status` 查询与 `command.terminal` 重查 | mvp | 否 |
| `node-link.session-create.v1` | `command.submit{command:"session.create"}`（Zed `session/new` 的映射） | mvp | 否 |
| `node-link.node-rotation.v1` | `node.rotate-key.*`（只登记 ID 与消息名） | post_mvp | 否 |
| `node-link.attachment-transfer.v1` | 节点间独立已认证附件连接（§12.9）：与 Sync 同名的 13 个附件帧、32 KiB 单块、十进制字符串偏移与流式摘要；主体绑定为已认证节点，Access 只做有界内存转发、不落盘 | mvp | 否 |
| `node-link.typed-media.v1` | 跨节点类型化媒体、消息通道与部分结果（§11.4）：`resource.event.payload.view` 的媒体引用/思考/部分结果与 Sync §10.4 同义；Access 不解析 ACP 原文，完整原文仍逐字节保真 | mvp | 否 |
| `node-link.pending-interactions.v1` | 跨节点有效待处理交互与真实表单（§12.7）：读取当前有效完整请求、`requestVersion` 校验与首个有效应答胜出；历史待处理项不可直接应答 | mvp | 否 |
| `node-link.remote-history.v1` | 远程历史回源（§12.7/§17）：Access 按 origin cursor 向 Owner 读取历史分页、读取前鉴权；Owner 不可达或超出保留期显式不可用，Access 不持久化正文 | mvp | 否 |
| `resource.stateless-origin.v1` | 无持久中转的远程来源流/运行期 ACK（§11.4/§12.4）：`resource.caught_up` 来源重放屏障、`resource.reset_required`/`resource.unavailable` 显式错误、`resource.ack` 收窄为运行期累计处理进度（不表示 Access 落盘），以及 `session.read` 的读取覆盖与 `command.status` 的 `remoteTarget`。全部已接线 | mvp | 否 |
| `core.session-activity.v1` | 跨节点会话主状态三态合同（§11.4/§12.3/§12.4）：`sessionMeta.state` 与 catalog `sessions[].state` 只有 `idle`/`running`/`pending`，与 Sync `sessionSummary.state` 同一取值集合；有效未解决交互优先为 `pending`，其次未结束任务（含排队）为 `running`，否则 `idle`。排队、等待原因与任务结果不经本 feature；旧七态只允许作为 `resource.event.payload.view` 的历史原文出现 | mvp | 是 |
| `core.session-title.v1` | 跨节点 Owner 元数据命令 `command.submit{command:"session.title.set"}`（§12.7 的 closed payload `{ title: string }`，输入按 wire 原文 1..=512 个 Unicode 标量值、保存值为 trim 后的文本）与有效标题投影：标题由 **Owner 持久化**（自定义标题优先，否则 Agent 标题），`sessionMeta`/catalog 与 `session.read` 的 `sessionSummary` 始终下发有效值、不新增 `customTitle` 键；Access 只转发命令、不回源保存标题或命令状态，`command.status` 的 `remoteTarget` 直接回源 Owner（Owner 负责去重与归属校验）；未协商时 Owner 返回 `nodelink.protocol.feature_required`。不是 ACP 能力，不新增 ACP 方法 | mvp | 否 |

- feature ID 语法、排序与 transcript 编码与 `SYNC_PROTOCOL.md` §5.2 相同：`[a-z0-9.-]`、去重后按 UTF-8 字节升序、`negotiatedFeatures` 以 NUL 分隔。
- 必需 feature 是 `node-link.core.v1` 与 `core.session-activity.v1`（`必需` 列为"是"，`required: true`）。Access MUST 在 `supportedFeatures` 与 `requiredFeatures` 中声明两者，Owner MUST 在 `node.challenge.selectedFeatures` 中返回两者；任一侧缺失时连接不进入业务阶段——Access 未声明时 Owner 返回 `nodelink.protocol.feature_required`，`details.features` 列出未被选择的 feature ID（排序去重），发送方必须给出；Owner 未选中时 Access 停止业务同步并提示版本不兼容。`core.session-activity.v1` 未选中前 MUST NOT 收发 `sessionMeta`、catalog 会话、快照条目或事件中的三态主状态，MUST NOT 回退七态或把 `pending` 折算成某个 `waiting_*`。
- 首切片必须实现本表 `delivery = mvp` 的全部行；`conditional_mvp` 行在真实通路接线后才宣告（未接线前收到对应新消息按 `nodelink.protocol.type_unsupported` 拒绝）；`node-link.node-rotation.v1` 只登记 ID 与消息名，实现时收到 `node.rotate-key.*` 返回 `nodelink.protocol.type_unsupported`，不得静默忽略。
- **实现事实（2026-10-21，`custom-session-title`）**：Owner 在 `node.challenge.selectedFeatures` 里宣告 registry 中已完整接线的 11 条 mvp feature（`node-link.core.v1`、`resource.stateless-origin.v1`、`core.session-activity.v1`、`raw-acp.v1`、`command-status.v1`、`session-create.v1`、`attachment-transfer.v1`、`typed-media.v1`、`pending-interactions.v1`、`remote-history.v1`、`core.session-title.v1`）。前五条与既有实现一致；`typed-media.v1`/`pending-interactions.v1`/`remote-history.v1` 的门控点都在 `session.read`，该命令现已由 Owner 的真实存储投影（共享 `server::session_read`，消息正文/结构化历史/待处理交互/配置项）；`core.session-title.v1` 由 `command.submit{command:"session.title.set"}` 的独立路由接线。附件传输仍要求组合根注册端点并装配存储（未装配时用 `feature_list_without` 去掉它）；其余 feature 无额外装配前提。`core.session-activity.v1` 的三态映射、交互归约、`session.updated` 生产与持久迁移已接线，Owner 在支持集合与必需集合中都登记它。`supported_features_match_the_registry_mvp_subset` 逐条断言本表与 registry 的 mvp 子集相等。

- **`core.session-title.v1` 已宣告**：`node.challenge.selectedFeatures` 只返回已完整接线的 ID；本 feature 的 `session.title.set`、有效标题投影、Owner `custom_title` 持久化与命令事务均已接线，因此纳入宣告集合。

### 11.4 事件类型与 `view` 契约（与 Sync 共享）

Node Link 不另立一份 event type 表：

- `resource.event.eventType` 的取值集合与每个类型的最低 `view` 字段，等于 `SYNC_PROTOCOL.md` §10.2 与 §10.3 的登记集合，减去只由 Access Node 本地产生、不跨节点传输的 `session.origin.online_changed`，以及只属于本机生命周期的 `session.deleted`（删除是 Owner 的 Sync-only 事实，Node Link 不登记、不转发，见 §12.11）。
- 机器表达复用 [`schemas/sync/v1/event-views.schema.json`](../schemas/sync/v1/event-views.schema.json)：Node Link 的事件 fixture 用该文件的 `$defs` 校验 `payload.view`。
- 未登记的 `eventType` 按未知事件处理：保留 `payload.acp`、向本地客户端可见降级，不得静默丢弃，也不得把结构化事件降级成普通文本。
- `view` 内允许的 JSON 取值遵守 `SYNC_PROTOCOL.md` §3.3 的 ACPR-CJ1 取值域（数字必须是整数），否则 `payloadDigest` 无法跨实现复算。

### 11.5 旧 peer 与未协商行为

- 未选中 `node-link.attachment-transfer.v1`/`typed-media.v1`/`pending-interactions.v1`/`remote-history.v1` 的连接行为与既有 v1 逐字相同：Owner 不下发 `view` 中的媒体引用、`requestVersion` 或历史分页字段，也不得让 Access 使用新的附件连接。
- 未协商的附件连接不会建立：Access 在 feature 未选中时不发起 `/node-link/v1/attachments`；若仍收到 `attachment.*` 帧，Owner 返回 `nodelink.attachment.negotiation_required` 并以 `4400` 关闭。
- 旧 Owner 面对新 Access：新能力入口显式不可用（附件不支持、历史不可读、历史交互不可答），不得退化为本地文本或空历史；Access MUST NOT 用本机路径或缓存冒充 Owner 内容。
- 未协商的封闭字段按 `nodelink.protocol.schema_invalid` 拒绝，未登记 type 按 `nodelink.protocol.type_unsupported` 拒绝，二者都不得静默忽略。
- **来源流是加性 feature**：未协商 `resource.stateless-origin.v1` 时不发送也不接收 `resource.caught_up`/`resource.reset_required`/`resource.unavailable`、`command.status.remoteTarget` 或 `session.read` 的 `readCursor`/`sessionSummary`/`messageCoverage`；收到未协商的新资源帧按 `nodelink.protocol.type_unsupported` 拒绝，收到未协商的封闭字段按 `nodelink.protocol.schema_invalid` 拒绝。`resource.ack` 的 wire 形状不变，但只有在该 feature 协商后才按「运行期处理进度」解释；未协商时沿用既有 v1 语义。Access MUST NOT 用本地落盘索引冒充该 feature，Owner 也不得据此提前清理保留期数据。
- **三态合同是必需 feature**：`core.session-activity.v1` 未由双方选中时连接不进入业务阶段（§11.3）。旧 Owner 面对新 Access：`node.challenge.selectedFeatures` 里没有该 ID，Access MUST 停止同步并提示版本不兼容，MUST NOT 按七态摘要自行折算或把 `pending` 当作某个 `waiting_*`。旧 Access 面对新 Owner：`node.hello` 未声明该 ID，Owner MUST 以 `nodelink.protocol.feature_required` 拒绝，且 MUST NOT 下发三态 `sessionMeta`、catalog 会话或快照条目。
- **标题写入是加性 feature**：未协商 `core.session-title.v1` 时 Access MUST NOT 发送 `session.title.set`；Owner 收到时在触碰会话状态之前以 `nodelink.protocol.feature_required` 拒绝（`details.features` 给出该 ID），标题与版本都不变。协商后 Access 只把命令转发给 Owner、不落盘标题或命令状态，结果未知时以 `command.status` + `remoteTarget` 回源原请求，不自动换号重发；`sessionMeta`/catalog 与 `session.read` 的 `sessionSummary` 始终是 Owner 给出的有效标题（不新增 `customTitle` 键），旧 peer 的读取形状因此逐字不变。
- **删除不跨节点**：`session.delete`/`session.deleted` 不属于 Node Link（§12.11）——本面既不登记该命令与事件，也没有「远程已删除」的持久事实；Access 只按 Owner 当前 catalog/快照收敛目录，重连回源重建。未协商 `core.local-session-lifecycle.v1` 的连接与本节其它加性 feature 一样看不到任何删除相关字段或事件。

## 12. 消息族与消息体

### 12.1 家族总览

命令名、`payload` 字段名与语义以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 和 `SYNC_PROTOCOL.md` §11.5 为唯一来源；本节只冻结 Node Link 的消息名、信封位置与必需性。

| 族 | 消息 | delivery |
|---|---|---|
| 握手 | `node.hello`、`node.challenge`、`node.proof`、`node.ready` | mvp |
| Catalog | `catalog.subscribe`、`catalog.snapshot`、`node.trust.revoked` | mvp |
| Catalog | `catalog.changed` | post_mvp |
| Catalog | `node.rotate-key.request`、`node.rotate-key.result` | post_mvp |
| Resource | `resource.attach`、`resource.attached`、`resource.subscribe`、`resource.snapshot_begin`、`resource.snapshot_chunk`、`resource.snapshot_end`、`resource.event`、`resource.ack` | mvp |
| Resource | `resource.caught_up`、`resource.reset_required`、`resource.unavailable` | mvp（受 `resource.stateless-origin.v1` 门控） |
| Resource | `resource.detach` | post_mvp |
| Command | `command.submit`、`command.accepted`、`command.rejected`、`command.terminal`、`command.status` | mvp |
| 控制 | `link.ping`、`link.pong`、`link.error` | mvp |
| 控制 | `link.backpressure` | post_mvp |
| 附件传输（独立已认证连接，§12.9） | `attachment.begin`、`attachment.chunk`、`attachment.chunk_ack`、`attachment.seal`、`attachment.sealed`、`attachment.status`、`attachment.abort`、`attachment.read`、`attachment.read_chunk`、`attachment.manifest_begin`、`attachment.manifest_page`、`attachment.manifest_sealed`、`attachment.error` | mvp（受 `node-link.attachment-transfer.v1` 门控） |

- 查询命令可以同步完成；mutation 只同步确认接受，最终结果必须使用持久化 terminal event。所有可重试 mutation 携带稳定 `requestId`，幂等键至少包含 `(ownerNodeId, accessNodeId, requestId)`。
- `post_mvp` 消息体在本节冻结形状，但 v1 首切片不实现：收到时显式返回 `nodelink.protocol.type_unsupported`（对 family 内的 post_mvp 消息）或对应的 `nodelink.*` 不支持错误，绝不静默忽略。
- 认证前只允许 `node.hello`、`node.challenge`、`node.proof`；其余消息在认证完成前以 `4401` 关闭。

- `node.rotate-key.request`/`node.rotate-key.result`（本节的 `post_mvp` 行）是 Node Link 上的轮换握手，与本地管理能力 `local.node.rotate-key`（见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md)）不是同一件事：后者是 Owner 本地用户发起的轮换意图，前者只负责把轮换后的结果同步给已配对的 Access Node。Node Identity Key 变化后所有已配对设备与节点必须重新配对（`SECURITY_DESIGN.md` §9.1）；首切片不实现这两个消息，收到时返回 `nodelink.protocol.type_unsupported`，不得静默忽略。

### 12.2 握手消息

```text
node.hello → node.challenge → node.proof → node.ready
```

| 消息 | 方向 | 信封 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|---|
| `node.hello` | Access → Owner | 认证前 | `minProtocolVersion`(integer)、`maxProtocolVersion`(integer)、`accessNodeId`(UUID)、`role`(const `"access"`)、`clientNonce`(base64url 32B)、`supportedFeatures`(feature 列表)、`requiredFeatures`(feature 列表) | 全部必需 | Access 声明版本区间、身份与 feature；`role` 固定 `"access"` 以阻止角色混用 |
| `node.challenge` | Owner → Access | 认证前 | `selectedProtocolVersion`(integer)、`connectionId`(UUID)、`ownerNodeId`(UUID)、`serverNonce`(base64url 32B)、`selectedFeatures`(feature 列表)、`catalogRevision`(decimal string)、`nodeProof`(base64url 64B) | 全部必需 | Owner 选择版本与 feature 子集，签发连接 ID 与 server nonce；`nodeProof` 是 §9.4 连接节点挑战 domain 的 P1363 签名；`catalogRevision` 是 Owner 当前目录修订号（与 `node.ready.catalogRevision` 同源），Access 用它验证 `nodeProof` 并构造自己的 `node.proof`——两个连接 domain 的 tag 6 都取自本字段（2026-09-26 修订补入） |
| `node.proof` | Access → Owner | 认证前 | `connectionId`(UUID)、`accessNodeId`(UUID)、`nodeProof`(base64url 64B) | 全部必需 | Access 对 §9.4 连接节点证明 domain 签名，完成双向认证 |
| `node.ready` | Owner → Access | 认证后 | `ownerNodeId`(UUID)、`catalogRevision`(decimal string)、`limits`(object，§2.5)、`serverEpoch`(UUID) | 全部必需 | 认证完成；`serverEpoch` 是本次 Owner 事件保留窗口的 epoch，与 Sync 的 `serverEpoch` 同义 |

- `limits` 子字段：`maxMessageBytes`、`catalogSnapshotBatchSize`、`resourceSnapshotBatchSize`、`maxInFlightCommands`、`maxPendingQueueBytes`、`maxPendingQueueMessages`、`heartbeatIntervalMs`，全部为 integer。
- `node.challenge` 的信封不含 `connectionId`/`connectionSequence`（认证前规则，§2.2）；连接 ID 在 body 中下发，与 `SYNC_PROTOCOL.md` §8.2 的 `auth.server_challenge` 同一处理。`node.proof` 与 `node.ready` 之后的全部消息使用认证后信封。
- 认证失败一律返回 `link.error` 并以对应 close code 关闭；不得只关闭连接而不给出 `code`。

### 12.3 Catalog 消息

| 消息 | 方向 | body | 必需 | 语义 |
|---|---|---|---|---|
| `catalog.subscribe` | Access → Owner | `knownRevision`(decimal string \| null) | 必需 | 订阅 catalog；`null` = 首次获取 |
| `catalog.snapshot` | Owner → Access | `revision`(decimal string)、`agents`、`directories`、`sessions` | 全部必需 | 本机全部资源清单，不按 Export 或授权集合过滤 |

`agents[]` 是 `{ agentId, name, capabilitiesRef }`；`directories[]` 是 `{ directoryId }`（不含路径）；`sessions[]` 是 `{ sessionId, originEpoch, agentId, state, version, createdAt, workspaceDirectoryId }`（不含正文）。

- `sessions[].state` 与 `sessionMeta.state`、Sync 的 `sessionSummary.state` 是**同一个**三态取值集合（`common.schema.json#/$defs/sessionState`，feature `core.session-activity.v1`）：有效未解决交互优先为 `pending`，否则未结束任务（含已接受的排队任务）为 `running`，否则 `idle`。catalog 与 `sessionMeta` 都只承载该摘要，不回传旧七态（旧值只允许作为 `resource.event.payload.view` 的历史原文），Access MUST NOT 用它推断任务或交互结果。
- `sessions[].originEpoch` 是该会话在 Owner 上的 origin epoch（与 `resource.event.originEpoch` 同一取值，来自 `owned_session.origin_epoch`）。Access 用它把 imported 会话投影成带 `origin.kind = "remote"` 的摘要，因此即使该会话还没有任何事件，也能在本地发现它；Owner MUST 每次投影时现读，不得缓存或改写。
- 三个数组各自最多 500 项；catalog 是**连接期内存数据**，断开即失效，重连后重新订阅。
- 已配对且未撤销的节点看到同一份清单。

### 12.4 Resource 消息

```text
resource.attach → resource.attached → resource.subscribe
  → resource.snapshot_begin → resource.snapshot_chunk* → resource.snapshot_end
  → resource.event* → resource.caught_up → resource.event*  (resource.stateless-origin.v1)
  → resource.ack*；错误路径：resource.reset_required / resource.unavailable
```

`resource.caught_up`、`resource.reset_required`、`resource.unavailable` 与收窄后的 `resource.ack` 语义属于 feature `resource.stateless-origin.v1`；未协商该 feature 的连接收到它们按 `nodelink.protocol.type_unsupported` 拒绝（§11.5），空游标订阅仍按既有 snapshot 路径工作。

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `resource.attach` | Access → Owner | `remoteSessionRef`(`{ ownerNodeId, sessionId }`) | 必需 | 为耐久会话身份申请新的临时路由凭据 |
| `resource.attached` | Owner → Access | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`sessionMeta`(`{ state, version }`) | 全部必需 | 新 generation 生效；旧 generation 的 frame 一律拒绝 |
| `resource.detach` | Access → Owner | `attachmentId`(UUID)、`reason`(`"client_request"`\|`"owner_unavailable"`) | 全部必需 | `post_mvp`；主动释放 attachment |
| `resource.subscribe` | Access → Owner | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`cursor`(`{ originEpoch, originSequence }` \| null) | 全部必需 | `null` 表示先走 snapshot；非空表示从 origin cursor 增量重放 |
| `resource.snapshot_begin` | Owner → Access | `snapshotId`(UUID)、`cursor`、`schemaVersion`(integer const 1)、`chunkCount`(integer) | 全部必需 | 快照 barrier；`cursor` 是 snapshot 结束点 |
| `resource.snapshot_chunk` | Owner → Access | `snapshotId`、`chunkIndex`(decimal string)、`resource`(`"session_meta"`\|`"pending_interactions"`)、`items`(array) | 全部必需 | 只承载元数据；`resource` 决定 item 形状 |
| `resource.snapshot_end` | Owner → Access | `snapshotId`、`cursor`、`chunkCount`(integer)、`snapshotDigest`(base64url 32B) | 全部必需 | 快照完成；其后事件从 `cursor` 继续 |
| `resource.event` | Owner → Access | `originEventId`(UUID)、`originEpoch`(UUID)、`originSequence`(decimal string)、`sessionRef`(`{ ownerNodeId, sessionId }`)、`eventType`(string，`^[a-z0-9_.-]{1,128}$`)、`payloadDigest`(base64url 32B)、`createdAt`(timestamp)、`payload`(`{ view?, acp? }`，二者至少一个) | 除 `payload.acp` 外全部必需 | Owner 的 origin 事件原样投递 |
| `resource.ack` | Access → Owner | `sessionRef`(`{ ownerNodeId, sessionId }`)、`cursor`(`{ originEpoch, originSequence }`) | 全部必需 | **运行期累计处理进度**（`resource.stateless-origin.v1`）：此 cursor 及之前的 origin 事件已被有界下游接收；不表示 Access 落盘，不是可恢复索引，Owner 不得据此提前清理保留期内数据 |
| `resource.caught_up` | Owner → Access | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`cursor`(`{ originEpoch, originSequence }`) | 全部必需 | 来源重放屏障：此 cursor 之前的来源事件已全部发出，之后才是实时事件；Access MUST NOT 用定时器推测追平 |
| `resource.reset_required` | Owner → Access | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`reason`(`"epoch_mismatch"`\|`"cursor_expired"`\|`"cache_incompatible"`)、`cursor`(`{ originEpoch, originSequence }` \| null) | 全部必需，`cursor` 可为 `null` | Access 必须以新的订阅或读取重建；只有 Owner 已验证有效 head 才能给 `cursor`；不新增错误码 |
| `resource.unavailable` | Owner → Access | `attachmentId`(UUID \| null)、`error`(`{ code, message, retryable, details }`) | 全部必需 | 来源不可继续投递：断线/撤销/快照不可用分别复用既有错误码（`nodelink.resource.owner_unavailable`、`nodelink.auth.node_revoked`、`nodelink.resource.snapshot_unavailable`） |

- `sessionMeta` 字段：`state`(`"idle"`\|`"running"`\|`"pending"`)、`version`(decimal string)。`state` 是会话的活跃程度摘要，取值集合由必需 feature `core.session-activity.v1` 协商（§11.3）：有有效未解决交互 → `pending`，否则有未结束任务（含排队）→ `running`，否则 `idle`；排队、等待原因与本轮结果不进入该字段。旧七态 MUST NOT 出现在 `sessionMeta` 里，它们只允许作为 `resource.event.payload.view` 的历史原文。
- `pending_interactions` item 字段：`interactionId`(UUID)、`kind`(`"permission"`\|`"elicitation"`)、`createdAt`(timestamp)、`payloadDigest`(base64url 32B)；不含交互正文。
- **正文绝不入快照**：`SessionSummary`、message、turn 与会话正文历史不存在于 Node Link 快照中；Access 需要正文时按 origin cursor 通过 `resource.event` 重放或经查询命令向 Owner 在线请求（Sync 侧见 `SYNC_PROTOCOL.md` §9.6）。
- `resource.event` 必须携带完整 origin 三元组（`originEventId` + `originEpoch` + `originSequence`）；缺失任一字段即 `nodelink.protocol.schema_invalid`，Access 不得用本地生成的 ID 顶替，也不得把事件降级成本地事件。
- `payload.view` 是唯一允许携带未登记字段的视图位置（开放扩展点）；事件类型在本协议登记的 view 集合内时必须携带 `payload.view`，无法形成 view 时必须保留 `payload.acp`，不得丢弃 raw 文档（schema 只要求 `payload` 内至少出现 `view` 或 `acp` 之一，具体事件类型的最低字段见 §11.4 的共享合同）。
- `resource.event.sessionRef` 与 `resource.ack.sessionRef` 必须指向当前连接上 attachment 所属的会话。`sessionRef` 不属于本连接、或 `cursor.originEpoch` 与该会话的 origin epoch 不一致时，接收方返回 `nodelink.protocol.sequence_invalid`，不得投递或接受。
- `payloadDigest = base64url(SHA-256(ACPR-CJ1(payload)))`，其中 ACPR-CJ1 是 `SYNC_PROTOCOL.md` §3.3 定义的规范 JSON；`pending_interactions[].payloadDigest` 是对创建该交互的 origin 事件的 `payload` 对象应用同一规则的结果。
- `snapshotDigest` 的计算方式与 `SYNC_PROTOCOL.md` §9.4 相同：对每个完整 `resource.snapshot_chunk` 消息的原始 UTF-8 bytes 分别求 SHA-256，按 `chunkIndex` 顺序连接后对连接结果再求一次 SHA-256；接收方不得通过重新序列化 JSON 计算该值。
- `resource.ack` 的 `cursor` 必须单调不减、不得越过 Owner 已发送的最高来源位置；回退或越界视为 `nodelink.protocol.sequence_invalid`。ACK 只是当前连接/来源会话的运行期处理进度：Access MUST NOT 持久化它，Owner MUST NOT 把它当作「客户端已阅读」或可恢复索引，也不得据此提前清理保留期内数据（design D5）。
- `resource.caught_up` 是来源重放的明确屏障（design D4）：Owner 先按 origin cursor 增量重放来源事件，再发这一帧，之后的 `resource.event` 才是实时事件。Access 只有在收到屏障后才能把重建缓冲置为可用状态；MUST NOT 用计时器或事件空闲推测追平。
- 重连与重建：`resource.reset_required(reason=cache_incompatible)` 或 epoch 不匹配/游标过期时，Access 必须重新订阅并按 `session.read` 的 `readCursor`/`messageCoverage` 重建，不得沿用旧 generation 或旧游标继续重放。来源不可达或已撤销时 Owner 发 `resource.unavailable`，Access 清除该来源的运行期模型并明确降级，不展示已清除内容。

### 12.5 Command 消息

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `command.submit` | Access → Owner | `requestId`(UUID)、`command`(命令名)、`sessionRef`(`{ ownerNodeId, sessionId }` \| null)、`attachmentId`(UUID \| null)、`attachmentGeneration`(decimal string \| null)、`expectedVersion`(decimal string \| null)、`payload`(object) | 全部必需，后四者可为 `null` | 提交命令；`payload` 形状按 `command` 取值决定（§12.7） |
| `command.accepted` | Owner → Access | `requestId`、`command`(命令名)、`acceptedAt`(timestamp)、`result`(object \| null) | 全部必需 | 同步确认接受；`acceptedAt` 永远是真实接受时间（拒绝不复用本消息），`result` 为 `null` 时表示本次回复不携带同步结果 |
| `command.rejected` | Owner → Access | `requestId`、`command`(命令名)、`error`(`{ code, message, retryable, details }`) | 全部必需 | 未接受，无副作用；本消息没有 `acceptedAt` 字段 |
| `command.terminal` | Owner → Access | `requestId`、`command`(命令名)、`terminal`(`{ status, terminalAt, terminalEventId, result, error }`) | 全部必需 | 终态；`status` ∈ `"completed"`\|`"failed"`\|`"rejected"`\|`"uncertain"`；`terminalEventId` 是 `T \| null` 字段：`command.status` 重查返回时必须非 `null`，其余情况可为 `null`；`status=completed` 时 `result` 为非空 object 且 `error=null`，其余 status 必须给出 `error` |
| `command.status` | Access → Owner | `targetRequestId`(UUID)、`remoteTarget`(`{ ownerNodeId, sessionId: UUID \| null }`，仅 `resource.stateless-origin.v1` 协商后) | `targetRequestId` 必需；`remoteTarget` 在协商后必需 | 重查同一 `requestId` 的终态；回复使用同一信封的 `command.terminal` 形状。`remoteTarget` 直接指向原 Owner 与其来源会话，`sessionId` 为 `null` 只用于尚未建成会话的 `session.create` |

- `command.submit` 的 `command` 取值是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 的 14 个命令名；`sessionRef`、`attachmentId`、`attachmentGeneration`、`expectedVersion` 在不适用时显式写 `null`，不用省略代替。
- `command.accepted`、`command.rejected` 与 `command.terminal` 都必须携带 `command`，取该 request 提交时的命令名（`command.status` 查询的回复填被查询 mutation 的命令名）；结果形状因此可以只凭帧自洽分派，不依赖接收方本地的 requestId 表。
- `command.status` 有两种等价形式：作为 `command.submit` 的 `command` 提交（`payload = { "targetRequestId": … }`），或使用独立的 `command.status` 消息。两者产生相同的 `command.terminal` 形状回复；独立消息不占用 mutation 的幂等键。
- 协商 `resource.stateless-origin.v1` 后，两种形式的 `command.status` payload 都必须另带 `remoteTarget = { ownerNodeId, sessionId: UUID | null }`（§12.4/§12.7，design D5）：Access 直接向原 Owner 查询，不凭本地记录判断，也不得因本地没有该 `requestId` 就回答 `nodelink.command.not_found` 或自动重发。`sessionId` 为 `null` 只用于尚未建成会话的 `session.create`。Owner 以既有 `(ownerNodeId, accessNodeId, requestId)` 隔离与参数冲突合同校验；未协商的连接 MUST NOT 发送该键。
- `command.accepted` 与 `command.terminal` 可以连续发送，也可以只发送 `command.terminal`：同步查询结果放在 `command.accepted.result` 或 `command.terminal.terminal.result`；`command.terminal` 始终是权威终态，客户端以 `requestId` 去重。
- Node Link 把统一结果形状拆成三条消息，`acceptedAt` 规则与 `SYNC_PROTOCOL.md` §11.2 的 `status=rejected ⇒ acceptedAt=null` 等价：`command.accepted` 的 `acceptedAt` 永远非 `null`；`command.rejected` 表达拒绝，因此完全不携带 `acceptedAt`；`command.terminal` 也不携带 `acceptedAt`。
- `command.terminal` 的 `completed` 必须携带非空 `result`（与 Sync 的 `command.result` 允许 `result` 为 `null` 不同）：该差异是有意的，因为 Node Link 的 `session.create` 必须回传 `SessionCreateResult`（§12.7）。core 只保留 `Option<CommandResult>`，**非空由本协议的适配层（`server::node_link`）在映射期保证**并由契约测试覆盖。
- session-scoped command 必须携带当前 `attachmentId`/`attachmentGeneration`；generation 过期返回 `nodelink.resource.attach_generation_stale`，Access 必须重新 attach 后再重试。该错误码不登记 `details`：Access 已知自己发出与当前持有的 generation，动作固定（重新 attach 后重试），归因靠发生错误的帧自带的 `attachmentId`，回显代际数值不改变任何分支。
- 同一 `(ownerNodeId, accessNodeId, requestId)` 的重复提交必须返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后的 `payload` 语义不同则返回 `nodelink.command.idempotency_conflict`。
- 判定「该命令有没有产生业务副作用」MUST 以**资源行是否已提交**为准（对 `session.create` 即会话行），MUST NOT 以「该 `requestId` 是否存在持久记录」代替——受理记录在受理阶段就已经存在，用它当替身会把可证明的失败误报成 `uncertain`。
- 不能确认副作用是否发生时，Owner 必须写入 `uncertain` 终态，Access 不得自动重试产生第二次副作用；能证明未产生副作用（进程未能启动、请求未发出、Agent 明确返回错误，或本机解析失败）时，Owner MUST 写入 `failed` 终态并给出结构化错误，MUST NOT 用 `uncertain` 代替。受理与落账之间崩溃留下的受理记录由启动恢复终结为 `uncertain`（`CORE_PORTS_AND_STORAGE.md` §6 第 16 条）；此后 Access 以 `command.status` 重查得到的就是该 `uncertain` 终态，Owner 不猜测成功或失败，也不重新派发。

### 12.6 控制与错误消息

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `link.ping` / `link.pong` | 双向 | `nonce`(base64url 16B) | 必需 | pong 必须原样回填 nonce；任一方向都可发起 |
| `link.error` | 双向 | `code`(string)、`message`(string ≤1024)、`retryable`(boolean)、`correlationId`(UUID \| null)、`details`(object) | 除 `correlationId` 外全部必需；`correlationId` 无关联请求时为 `null` | 连接级错误；可恢复的命令错误优先使用 `command.rejected` |
| `link.backpressure` | 双向 | `scope`(`"resource"`\|`"connection"`)、`retryAfterMs`(integer) | 全部必需 | `post_mvp`；提示对端暂停发送，达到高水位后仍可断开 |

### 12.7 `command.submit` 的 payload

`payload` 字段名与语义与 `SYNC_PROTOCOL.md` §11.5 同名同义；Node Link 侧 `sessionId` 由 `sessionRef` 承载，不重复出现在 `payload` 内。

| `command` | `payload` |
|---|---|
| `session.list` | `{}`（空 object） |
| `session.read` | `{ "include": ["messages"\|"turns"\|"pending_interactions"\|"config_options"\|"capabilities"\|"history", …], "beforeHistory"?: decimal string, "before"?: { "createdAt": timestamp, "messageId": UUID, "sequence"?: decimal string }, "limit"?: integer ≥ 1 }`；`history` 与 `beforeHistory` 只在协商 `node-link.remote-history.v1` 后接受，按 origin 会话内序号独立分页并回源 Owner；消息页的 `before`/`limit` 在同一 feature 下接受（与 Sync 面同名同义，`limit` 是结构常量而非十进制字符串）；`pending_interactions` 只在协商 `node-link.pending-interactions.v1` 后返回完整有效请求；未协商却携带新字段按 `nodelink.command.unsupported_field` 拒绝 |
| `command.status` | `{ "targetRequestId": UUID, "remoteTarget"?: { "ownerNodeId": UUID, "sessionId": UUID \| null } }`；`remoteTarget` 在协商 `resource.stateless-origin.v1` 后必须携带（`sessionId` 为 `null` 只用于尚未建成会话的 `session.create`），未协商时缺席 |
| `session.mode.list` | `{}` |
| `session.config.list` | `{}` |
| `session.prompt` | `{ "content": [ { "type": "text", "text": "…" }, … ], "submissionId"?: UUID }`；`attachment_ref`/`workspace_path_ref` 与 `submissionId` 只在协商 `node-link.attachment-transfer.v1` 后接受，且清单必须已在本节点连接上封存并绑定同一会话/主体 |
| `session.cancel` | `{ "turnId": UUID }` |
| `elicitation.respond` | `{ "interactionId": UUID, "requestVersion"?: decimal string, "action": "submit"\|"decline"\|"cancel", "values": object\|null }`；`requestVersion` 只在协商 `node-link.pending-interactions.v1` 后必须携带（旧 peer 可省略），且必须等于当前有效请求（§12.10），不符返回 `nodelink.interaction.version_conflict` 且不消耗请求；`submit` 的 `values` 可为 `null`（= ACP `content: null`），`decline`/`cancel` 必须为 `null` |
| `session.mode.set` | `{ "modeId": string }`，body 的 `expectedVersion` 必须存在 |
| `session.config.set` | `{ "configId": string, "value": … }`，body 的 `expectedVersion` 必须存在 |
| `permission.resolve` | `{ "interactionId": UUID, "requestVersion"?: decimal string, "optionId": string }`；`requestVersion` 的协商门控与版本校验同 `elicitation.respond` |
| `session.create` | `{ "agentId": string, "directoryId": string }` |
| `session.resume` | `{}`（空 object；出现任何键即以 `nodelink.command.unsupported_field` 拒绝） |
| `session.title.set` | `{ "title": string }`：closed object，只允许这一个键；输入（trim 前）是 1..=512 个 Unicode 标量值，保存值是 trim 后的文本，trim 后为空则拒绝；超长按输入计、不截断（wire codec 保真原文并校验标量与空白，Owner 的 core 去首尾空白后保存）；body 的 `expectedVersion` 必须为 `null`；受 feature `core.session-title.v1` 门控，未协商时 Owner 返回 `nodelink.protocol.feature_required` |

`session.read` 在两个传输面同名，载荷在协商 `node-link.remote-history.v1` 后才有分页参数：Sync 面另有可选的 `before`（`sequence?` 加创建时间与消息身份）与 `limit`；Node Link 面在同一 feature 下接受同名同义的 `before`/`limit`（`limit` 为结构常量整数），并保留 `beforeHistory`（origin 会话内序号）供结构化历史独立分页。未协商该 feature 的旧 peer 省略这些键，请求与结果形状与本变更前逐字相同；Owner 收到未协商的新键时按 `nodelink.command.unsupported_field` 拒绝，不静默忽略。

协商 `resource.stateless-origin.v1` 后，`session.read` 的 `completed` 结果另带三个加性字段（`schemas/node-link/v1/command.schema.json#/$defs/sessionReadResult`，design D4）：`readCursor`（同一次 Owner 只读事务原子捕获的来源位置 R）、该次读取的完整 `sessionSummary`、以及 `messageCoverage[]{messageId, through: originCursor, lastDeltaIndex: decimalString | null}`——每个已返回消息恰有一项，`through` 等于 R，`lastDeltaIndex` 是该消息在所返回完整内容中已覆盖的最后连续增量索引（无增量时为 `null`）。数据和覆盖信息必须同次原子读取，不能在读取结束后再采样 head。`messageCoverage`/`readCursor` 只在该 feature 协商后出现；`messages`/`history` 原有页边界与 `hasEarlier`/`historyHasEarlier` 不改变，每个读取结果独立合并，不声明整页或整个分页浏览是同一历史快照。Access 逐字节转发 Owner 的结果对象，不重新解释或补造条目。

客户端以 `(来源, messageId)` 记录当前已应用覆盖位置，只有较新或相同覆盖页可替换消息；页面内容覆盖 `through` 及之前针对该消息的创建/增量事件，对应缓冲项不再拼接，`through` 之后按来源次序与 `deltaIndex` 应用。未返回的消息不受 R 覆盖，不能丢弃其缓冲事件；`history` 按来源稳定 `eventId` 去重。缓冲溢出必须明确取消并重建，不得静默丢事件。

`schemas/node-link/v1/command.schema.json` 对上述协商后字段保持**可选**：`session.read` 的 `history`/`beforeHistory`/`before`/`limit`、`session.prompt` 的 `attachment_ref`/`workspace_path_ref`/`submissionId`、以及 `elicitation.respond`/`permission.resolve` 的 `requestVersion`。`command.status` 的 `remoteTarget` 与 `session.read` 结果的 `readCursor`/`sessionSummary`/`messageCoverage` 同样按 `resource.stateless-origin.v1` 门控为可选。这是旧 peer 的 wire 形状——未协商对应 feature 时它们缺席，连接行为与本变更前逐字相同；协商后客户端 MUST 发送，服务端不得用缺省值猜测版本或清单绑定，也不得因字段缺席放宽校验。`session.prompt` 的 `promptContentBlock` 分支与 Sync 面同名同义，两个面不各自维护第二套词表。

> **实现事实：Node Link 远程读取（2026-10-20，`conversation-area-complete` WP5E5-read）**。Owner 的 `session.read` 已接线到 core 用例面与共享投影 `server::session_read`：消息页（用户输入 + Agent 回复/思考、`before`/`limit`、`hasEarlier`）、结构化历史（`beforeHistory`、`historyHasEarlier`、`history` include）、有效待处理交互（协商 `node-link.pending-interactions.v1`）与配置项都按 §11.4/§11.5 的共享形状返回；`turns`/`capabilities` 仍显式回 `nodelink.command.unsupported`（core 读面无法投影）。未协商 `typed-media.v1` 时消息的 `channel`/`partial` 与媒体块按旧 peer 形状降级，未协商 `remote-history.v1` 时历史与消息分页字段被显式拒绝。Access 侧 `node-link-client::RemoteSession::read_page` 把上述结果原样（不落盘）交给 `UseCases::read_imported_session`，由 Sync 的 imported `session.read` 投影给本地客户端；Owner 离线返回 `resource.remote_unavailable`（`details.reason = "owner_offline"`），不返回 `session.not_found` 或空历史。

`session.create` 的硬约束：

- 只允许上述四个键；`payload` 出现 `cwd`、`mcpServers`、任何绝对路径、任何凭据字段（例如 `apiKey`、`token`、`env`、`credential`）时，Owner **必须**以 `command.rejected` 回复，`error.code = "nodelink.command.unsupported_field"`，`details.field` 必须给出被拒的字段名，且不得创建会话或部分应用参数。
- `agentId` 必须是本机配置声明的 Agent，`directoryId` 必须是本机已登记的目录；未知值返回 `nodelink.command.unsupported_field` 并给出 `details.field`。
- `directoryId` 是 32 字符小写十六进制的不透明标识（`^[0-9a-f]{32}$`），不构成路径；绝对路径在语法上就无法通过该 pattern。
- `session.create` 不接受 `templateParams`、`cwd`、MCP 配置或凭据字段。
- `session.create` 的 `sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` 必须为 `null`；创建成功后 Access 通过 `resource.attach` 取得 attachment，再提交其他会话范围命令。

`session.create` 的结果契约：

- Owner 先回 `command.accepted`，此时 `result` 必须为 `null`（创建是异步的）；会话创建完成后发 `command.terminal`。受理只落该命令的幂等记录（会话标识为空、MUST NOT 因此创建会话行，MUST NOT 因受理派发第二次创建）；会话行、它的会话标识回填与终态 MUST 在同一次提交内落盘，后端失败时 MUST NOT 留下会话行。
- `status = "completed"` 时 `terminal.result` 必须是 `SessionCreateResult`：

```json
{
  "remoteSessionRef": { "ownerNodeId": "<UUID>", "sessionId": "<UUID>" },
  "sessionMeta": { "state": "idle", "version": "1" }
}
```

（非规范示例：新建会话的落账提交完成后，可见版本为 `1`（`CORE_PORTS_AND_STORAGE.md` §5.2：新建行的 `version` 只有一个取值）；上面这帧展示的是结果形状，不是可复现的终态取值。）

- `sessionId` 由 Owner 生成并写入自身事件日志；Access 不得改写、重编号或本地顶替。Access 用该 `remoteSessionRef` 发起 `resource.attach`（§12.4），成功后才提交该会话的其他命令。
- 终态类别按「Agent 侧是否可证明未产生会话」判定（证据口径见 §12.5）：进程未能启动、请求未发出、Agent 明确返回错误，或本机解析失败 ⇒ `status = "failed"` 并给出结构化 `terminal.error`（例如 `nodelink.internal.unavailable`），MUST NOT 用 `uncertain` 代替；短请求超时、请求发出后连接断开、会话已建立之后才失败，或崩溃窗口 ⇒ `status = "uncertain"` 表示无法确认会话是否已创建，Access **不得**自动重试 `session.create`，必须向调用方返回显式错误，由用户决定是否以新 `requestId` 重试。
- 注记（`node-link-owner` 的修复轮次，`session-create-settle-order` 变更补充分类）：幂等行落盘前失败（如 Owner 存储写失败）的 `session.create` **不是持久首次结果**——Owner 在那一轮仍会发一帧本地 `command.terminal`，但同 `requestId` 的 `command.status` 重查回 `nodelink.command.not_found`，重试也可以创建出另一个会话、得到与首次尝试不同的结果。因此 `failed` 分两类，Access MUST NOT 混用：① **落盘前失败**（授权拒绝、本机 workspace 解析失败）只有本地帧、没有持久记录，重查回 `nodelink.command.not_found`；② **幂等行已落盘、可证明 Agent 侧未产生会话**（进程未能启动、请求未发出、Agent 明确返回错误）的 `failed` 是**已持久化**的终态（零会话行），同一 `requestId` 重查回同一 `failed`。落盘失败类（如存储写失败）不属于这两类，Access 不得把它的 `failed` 当成可稳定重放的终止事实（`CORE_PORTS_AND_STORAGE.md` §6 第 20 条）。

`session.resume` 的硬约束：

- `payload` 必须是空对象：出现任何键（例如 `cwd`、`agentId`）时，Owner **必须**以 `command.rejected` 回复，`error.code = "nodelink.command.unsupported_field"`、`details.field` 给出被拒的字段名，且不得启动 Agent 进程、不得部分应用参数。
- `sessionRef`/`attachmentId`/`attachmentGeneration` 必须为指向本机已有会话的非 `null` 值（resume 是会话范围命令）；`expectedVersion` 必须为 `null`。
- 恢复所需的 Agent 标识、ACP 会话标识与创建时目录一律由 Owner 从自身持久化记录读取，不接受客户端提供，也不回退到别名重新解析。
- 该会话没有持久化恢复数据（恢复列为 `NULL`）与「目标 Agent 未宣告 `sessionCapabilities.resume`」走同一条路径：终态失败、错误码 `nodelink.command.unsupported`，**不得**降级为新建会话；能力只有经 `initialize` 才能得知，因此能力不支持的判定发生在进程拉起之后、发送 `session/resume` 之前，并由终态错误如实回报。

`session.resume` 的结果契约：

- Owner 先回 `command.accepted`（`result` 为 `null`），恢复完成后发 `command.terminal`；`status = "completed"` 时 `terminal.result` 必须是 `SessionResumeResult`，字段形状与 `SessionCreateResult` 一致（`remoteSessionRef` + `sessionMeta`），`status = "failed"`/`"uncertain"` 时必须给出 `PublicError`。
- 恢复是带副作用的 mutation（Owner 可能因此拉起一个 Agent 进程）：`status = "uncertain"` 表示崩溃窗口内无法确认副作用是否发生，Access **不得**自动重试 `session.resume`，必须向调用方返回显式错误，由用户决定是否以新 `requestId` 重试。
- 创建时目录的复校验失败（目录被删、被改成文件、规范化结果与创建时不同）返回 `nodelink.internal.unavailable`；同一 `(ownerNodeId, accessNodeId, requestId)` 的重复提交仍返回首次结果，不做第二次恢复。
- 幂等比对包含**会话身份**：`session.resume` 的 `payload` 恒为空对象，语义全部落在 `sessionRef` 上，因此除命令名、种类与 `payload` 指纹外还必须比对 `sessionRef` 指向的会话（§12.5）。同一 `(ownerNodeId, accessNodeId, requestId)` 改指另一个已授权会话时返回 `nodelink.command.idempotency_conflict`，不得回上一个会话的终态结果，也不得据此拉起第二个 Agent 进程。
- 注记（`node-link-owner` 的修复轮次，与上文 `session.create` 的情形同形）：恢复成功但终态落盘失败（如 Owner 存储不可用）时，Owner 在那一轮仍会发一帧本地 `command.terminal`（`terminalEventId` 为 `null`），但持久记录仍是 `accepted`，同 `requestId` 的 `command.status` 重查回 `accepted`，并由启动恢复按 `CORE_PORTS_AND_STORAGE.md` §6 第 16 条改写为 `uncertain`。因此该帧不构成可稳定重放的首次结果：Access 不得把这一轮的成功帧当成终态事实，也不得以同一 `requestId` 重试（重试会命中幂等行而不产生第二次副作用）。相比 `session.create`，这条收窄的副作用更重——第一次尝试**可能已经拉起了 Agent 进程**。

`node-link-owner` 切片的结果投影范围（2026-10-20 起扩展）：Owner 为 `session.list`、`session.read`、`session.create` 与 `session.resume` 投影 wire 结果；`session.read` 的消息/历史/交互/配置由共享投影 `server::session_read` 生成，`turns`/`capabilities` 因 core 读面无法表达而显式回 `nodelink.command.unsupported`（`command.rejected`）。Owner **不得**为避免该错误而返回被裁剪的结果或把结构化事件退化成文本（§15 的保真要求优先）；Access 遇到该错误码应显式报「该资源在本版本不可用」，不要当成重试可恢复的失败。

### 12.8 消息名 ↔ schema 对照

| 消息名 | schema 文件 | `$defs` |
|---|---|---|
| `node.hello` | `handshake.schema.json` | `nodeHello` |
| `node.challenge` | `handshake.schema.json` | `nodeChallenge` |
| `node.proof` | `handshake.schema.json` | `nodeProof` |
| `node.ready` | `handshake.schema.json` | `nodeReady` |
| `catalog.subscribe` | `catalog.schema.json` | `catalogSubscribe` |
| `catalog.snapshot` | `catalog.schema.json` | `catalogSnapshot` |
| `catalog.changed` | `catalog.schema.json` | `catalogChanged` |
| `node.trust.revoked` | `catalog.schema.json` | `nodeTrustRevoked` |
| `node.rotate-key.request` | `catalog.schema.json` | `nodeRotateKeyRequest` |
| `node.rotate-key.result` | `catalog.schema.json` | `nodeRotateKeyResult` |
| `resource.attach` | `resource.schema.json` | `resourceAttach` |
| `resource.attached` | `resource.schema.json` | `resourceAttached` |
| `resource.detach` | `resource.schema.json` | `resourceDetach` |
| `resource.subscribe` | `resource.schema.json` | `resourceSubscribe` |
| `resource.snapshot_begin` | `resource.schema.json` | `resourceSnapshotBegin` |
| `resource.snapshot_chunk` | `resource.schema.json` | `resourceSnapshotChunk` |
| `resource.snapshot_end` | `resource.schema.json` | `resourceSnapshotEnd` |
| `resource.event` | `resource.schema.json` | `resourceEvent` |
| `resource.ack` | `resource.schema.json` | `resourceAck` |
| `resource.caught_up` | `resource.schema.json` | `resourceCaughtUp` |
| `resource.reset_required` | `resource.schema.json` | `resourceResetRequired` |
| `resource.unavailable` | `resource.schema.json` | `resourceUnavailable` |
| `command.submit` | `command.schema.json` | `commandSubmit` |
| `command.accepted` | `command.schema.json` | `commandAccepted` |
| `command.rejected` | `command.schema.json` | `commandRejected` |
| `command.terminal` | `command.schema.json` | `commandTerminal` |
| `command.status` | `command.schema.json` | `commandStatus` |
| `link.error` | `error.schema.json` | `linkError` |
| `link.ping` | `error.schema.json` | `linkPing` |
| `link.pong` | `error.schema.json` | `linkPong` |
| `link.backpressure` | `error.schema.json` | `linkBackpressure` |

Node Link 的配对 HTTP 载荷（§13）不在 WSS 联合入口内：

| 载荷 | schema 文件 | `$defs` |
|---|---|---|
| 二维码 payload | `pairing.schema.json` | `qrPayload` |
| claim 请求 | `pairing.schema.json` | `claimRequest` |
| claim 响应 | `pairing.schema.json` | `claimResponse` |
| 状态查询请求 | `pairing.schema.json` | `statusRequest` |
| 状态查询响应 | `pairing.schema.json` | `statusResponse` |
| HTTP 错误 body | `pairing.schema.json` | `httpError` |

`message.schema.json` 是所有 WSS 消息的唯一联合入口（`oneOf` 上述五个消息族文件）。

附件连接不在该联合入口内：`attachment.schema.json` 是**独立已认证连接**的帧联合入口（§12.9），与业务 `message.schema.json` 分离，避免巨量正文进入业务消息路径，也让旧 peer 的联合入口保持逐字不变。`fixtures/node-link/v1/attachment/` 的正反样例直接绑定该 schema。

### 12.9 附件传输连接（独立已认证 WSS）

节点间的附件正文不进入 `resource.event` 或业务命令：协商 `node-link.attachment-transfer.v1` 后，Access 与 Owner 另建一条已认证 WSS 连接。

- **端点与认证**：`wss://<host>/node-link/v1/attachments`。连接先完整执行 §12.2 的 `node.hello`/`node.challenge`/`node.proof`（同一节点身份、§9 的 transcript、撤销与心跳规则）。认证完成前只接受认证消息；认证完成后，未选中 `node-link.attachment-transfer.v1` 的连接收到任何附件帧返回 `nodelink.attachment.negotiation_required` 并以 `4400` 关闭，且不触碰存储。附件连接不复用业务连接的认证状态，也不产生长期 bearer。
- **实现边界（2026-10-20，`conversation-area-complete` WP5）**：Owner 入站侧（接收上传、服务读取、Owner 自己的传输存储与撤销）已实现；组合根在装配了附件传输端口时把 `/node-link/v1/attachments` 注册到 listener，并同时把 `node-link.attachment-transfer.v1` 加入握手能力（未装配存储时不注册端点、不宣告）。Access 侧的 `node-link-client::attachment::AttachmentForwarder` 已实现有界内存转发（`begin`/`chunk`/`seal`/`status`/`abort`/`read`/清单，停等、单块 32 KiB、不落盘、断线/撤销显式失败）。**已接线**（2026-10-20，WP5d）：Access 侧配对（`node pair --mode access`）与 `app` 组合根的 Owner 连接实例化；远程附件转发经 `AccessManager::open_attachment` 暴露给本地调用方，但本地 Sync 命令面到远程会话的附件路由仍属 `server::acp_facade`（尚未落地），因此不要把本节的协议定义读成已可从 UI 使用。
- **协商与隔离**：只有双方握手选中 `node-link.attachment-transfer.v1` 才进入帧族；未协商时不建立连接、不下发新帧，业务 `message.schema.json` 保持逐字不变（§11.5）。
- **绑定**：每帧重复 `transferId`；`begin`/`manifest_begin` 携带 `sessionId` 与 `subject{kind:"node", id}`。`subject.id` 必须是本条连接上已认证的对端节点；跨节点、跨会话或跨 Access 连接复用标识返回 `nodelink.attachment.binding_mismatch`，不泄露内容也不触发命令。
- **分块与封存**：偏移、长度一律十进制字符串；同偏移同内容重复提交返回原 `chunk_ack`（`duplicate: true`），不同内容返回 `nodelink.attachment.chunk_conflict`；非期望偏移返回 `nodelink.attachment.offset_mismatch`。`attachment.seal` 校验总长度与整段流式 SHA-256，不符返回 `nodelink.attachment.digest_mismatch`。
- **清单**：`manifest_begin` → `manifest_page`（每页 ≤64 个 `transferId`）→ `manifest_sealed`；`session.prompt` 只携带已封存 `submissionId`（§12.7）。清单页逐条校验：每个 `transferId` 必须已封存，且会话/主体都与该清单的 `manifest_begin` 一致，否则整页 `nodelink.attachment.binding_mismatch`（不部分登记）。
- **prompt 绑定（实现事实）**：Owner 在派发 `session.prompt` 之前要求：本连接协商了 `node-link.attachment-transfer.v1`；带 `attachment_ref` 时必须有 `submissionId`；清单已封存、属于本会话与本已认证节点；每个引用的 `attachmentId`/`sha256`/`byteLength` 与清单逐块一致。任一不符返回 `nodelink.attachment.negotiation_required`/`nodelink.protocol.schema_invalid`/`nodelink.attachment.binding_mismatch`，且**不派发 Agent**。通过后走与 Sync 相同的整批绑定入口，`submissionId` 绝不会被静默丢弃。
- **读取边界**：`attachment.read` 的 `offset == totalLength`（EOF 再读）与 0 字节附件读到空数据，而 `read_chunk.data` 至少 1 字节，因此回 `nodelink.protocol.schema_invalid`（不可重试），不会认领后不发回帧。
- **活动传输预算的释放**：单节点 8 个活动传输在封存、取消、失效时释放，并在附件连接关闭时释放该连接预留的槽位——Access 放弃上传不会永久占住 Owner 的预算。
- **Access 转发边界**：Access 只做**有界内存**转发，MUST NOT 把远程附件正文写入临时文件、SQLite、日志或持久缓存；已转发上传不等于消息已提交。Owner 是附件的唯一权威，读取同样经该连接回源并重新校验主体、会话与归属。
- **撤销与过期**：节点信任撤销时，`close_node` 同时扫描业务表与附件表并以 `4410` 关闭相关连接，正在进行的传输/读取随连接中止；已提交附件仍由 Owner 按保留策略处理。临时传输闲置 24 小时后过期，过期/不存在的 `transferId` 一律按 `nodelink.attachment.transfer_not_found` 处理（`nodelink.attachment.expired`/`revoked` 是登记的错误码，本实现不主动发送：撤销走关闭连接，过期归入不存在）。
- **失败语义**：磁盘不足、摘要不符、预算耗尽都显式失败（`nodelink.attachment.budget_exhausted` 可重试），不得截断后冒充完整内容，也不得阻塞 `resource.event` 投递。
- **机器合同**：帧名与预算的唯一跨协议登记是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 的 `transportFrames`，形状见 `schemas/node-link/v1/attachment.schema.json`；`check:contract-assets` 断言两侧一致。

#### 帧族（`attachment.*`）

| 帧 | 方向 | 说明 |
|---|---|---|
| `attachment.begin` | Access → Owner（上传）/ Owner → Access（读取准备） | 声明 transferId、会话、主体、总长度、块大小与媒体类型 |
| `attachment.chunk` | 双向 | 指定偏移的一块原始数据（base64url） |
| `attachment.chunk_ack` | 双向 | 确认偏移与长度，`duplicate` 表示重复同块 |
| `attachment.seal` | 上传方 → 接收方 | 提交总长度与整段 SHA-256，请求封存 |
| `attachment.sealed` | 接收方 → 上传方 | 封存成功事实、内容寻址 `attachmentId` 与封存时间；`session.prompt` 的 `attachment_ref.attachmentId` 使用该值 |
| `attachment.status` | 双向 | 查询/报告当前传输状态与已接收长度 |
| `attachment.abort` | 双向 | 取消传输并给出封闭原因 |
| `attachment.read` | 双向 | 请求读取已封存附件的一段 |
| `attachment.read_chunk` | 双向 | 返回一段原始数据与 EOF 标记 |
| `attachment.manifest_begin` | 双向 | 开始登记一次提交清单 |
| `attachment.manifest_page` | 双向 | 分批登记清单中的 transferId |
| `attachment.manifest_sealed` | 双向 | 清单已封存及其摘要 |
| `attachment.error` | 双向 | 结构化附件错误（code 复用 §14.1 词表） |

#### 附件传输预算

| 项目 | 固定值 |
|---|---:|
| 单块原始数据 | 32768 bytes |
| 每传输未确认块 | 1 |
| 单设备活动传输 | 2 |
| 单节点活动传输 | 8 |
| 每连接附件帧预算 | 6000 帧 / 60 秒（固定窗口） |

上表是节点间转发的**运行保护**，不是附件数量或大小产品额度；它同样约束 Access 的内存工作集（每传输至多一块未确认、单节点连接 8 个活动传输）。帧预算按附件连接独立计数，不占用业务命令的 §2.5 配额；超出活动传输或帧预算都回 `nodelink.attachment.budget_exhausted`（可重试），帧预算超限不关闭连接。

`manifestDigest` 与 Sync §3.4 同一算法：按清单顺序的 `transferId` 计算 `SHA-256( Σ u32be(len(id)) || id )`（无填充 base64url），两端都能复算；接收方重新计算并逐字比较，不符返回 `nodelink.attachment.digest_mismatch`。

### 12.10 有效交互与远程历史

- **有效待处理交互**：协商 `node-link.pending-interactions.v1` 后，`session.read` 的 `pending_interactions` 从 Owner 取得当前有效的完整请求（`effectiveInteraction`，形状与 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.4 相同），包括 `requestVersion` 与完整 options/schema。历史卡片只提供定位信息，不能直接应答。该 feature 已随 `session.read` 接线并宣告（§11.3）。
- **应答校验**：`permission.resolve`/`elicitation.respond` 在 Owner 侧重新校验节点身份、会话、请求仍为 pending、`requestVersion`、上游等待方存活与表单值；版本不符返回 `nodelink.interaction.version_conflict`，已处理返回既有 `nodelink.command.idempotency_conflict` 或已处理事实，失效/上游丢失/Owner 不可达返回 `nodelink.interaction.unavailable`，均不消耗请求。首个有效答案胜出，其余显式返回已处理。
- **远程历史**：协商 `node-link.remote-history.v1` 后，`session.read` 的 `history` include、`beforeHistory` 与消息页的 `before`/`limit` 由 Owner 按 origin 会话内序号/复合游标分页，Access 只转发页边界与来源游标，不保存无正文索引；每次读取前重新鉴权，Owner 不可达返回 `nodelink.resource.owner_unavailable`，超出保留期显式不可用，MUST NOT 返回空页。
- **不复制正文**：两类读取都只在内存转发；Access MUST NOT 把历史条目、交互 schema 或表单值写入数据库、临时文件、日志或持久缓存。

### 12.11 会话删除的边界（`session.delete` / `session.deleted`）

删除只发生在 Owner，且**只经 Sync 面**（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.9，存储与事务见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §13）。Node Link 侧因此是纯边界，没有新增帧、字段或 feature：

- **不在 Node Link 登记或暴露**：`session.delete` 不是 Node Link 命令——它不在 `command.submit.command` 的封闭集合里（[`schemas/node-link/v1/command.schema.json`](../schemas/node-link/v1/command.schema.json) 的 `commandName`，§12.5/§12.7）、不在 §11.3 的 feature 表中、也没有对应 payload 形状。由于该 `enum` 是封闭的，Access 把它当作 `command` 提交时在**结构层**就被拒绝（`nodelink.protocol.schema_invalid`），不会到达 Owner 的命令分派，因此也不会被映射成任何本地删除动作；`nodelink.command.unsupported` 只用于「命令名本身合法、但本 Owner 不投影该能力」的既有分支。它同样不是 ACP `session/delete`。
- **Owner catalog 是唯一事实**：会话被删除后，Owner 的 catalog、`sessionMeta` 与 `session.read` 一律不再包含它（tombstone 不是会话）。Owner 用既有 `catalog.snapshot` 消息向在线 Access 发布调用删除事务后的当前清单；该快照是目录刷新，不承载删除事件或原因。Access 在收到新 catalog/快照时**丢弃该会话的运行期目录条目与来源模型**；此后的 `resource.attach`、`session.read`、`session.prompt` 等按常规 not-found 失败（`nodelink.command.not_found` 或既有归属校验错误），MUST NOT 触发自动重建、自动重放或「可能只是暂时读不到」的降级。
- **不持久化删除事实**：Access MUST NOT 为远程删除写任何持久状态——没有删除索引表、没有墓碑行、没有镜像的 `session.deleted` 事件、没有正文/摘要/游标/receipt/ACK/命令状态。删除后的目录收敛完全来自 Owner 的当前 catalog 与快照；**本协议不存在**「Access 本地记录了一次 Owner 删除」这种事实，也不存在需要跨重启保留的「已删除会话」清单。
- **重连一律回源**：断线、重连或 catalog revision 变化后，Access 重新 `catalog.subscribe` 并 `resource.attach`（§15），目录与可见性由 Owner 当时的状态决定；不沿用旧目录、不据上一次快照推断删除，也不需要保留任何「离线期间的删除」。
- **imported 会话不能被重新导出**：同一资源只允许一个 hop（§4），因此 Access 本地持有的 imported 会话既不能通过 Node Link 再导出给第三节点，也不能成为本节点 `session.delete` 的对象——删除只作用于 Owner 自己 owned 的本机会话。

## 13. 节点配对 HTTP

### 13.1 二维码内容

Owner Node 创建一次性 pairing，二维码承载 HTTPS URL，秘密部分必须位于 fragment：

```text
https://work-pc.example.ts.net/node-link/pair#data=<base64url-json>
```

解码后的 JSON：

```json
{
  "pairingProtocol": "acp-remote-nodelink-v1",
  "ownerNodeId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "ownerPublicKey": "<base64url-65-bytes>",
  "endpoint": "wss://work-pc.example.ts.net/node-link/v1",
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "pairingSecret": "<base64url-32-bytes>",
  "expiresAt": "2026-09-18T12:15:00.000Z"
}
```

- `expiresAt` 是毫秒精度 UTC RFC 3339 字符串，与全部 Node Link 时间戳一致。
- `endpoint` 是 §2.1 的 WSS 端点，也是 claim/status 的唯一目标来源：HTTP origin 由 `endpoint` 的 host 加 `https` 得到，路径固定为 `/node-link/v1/pairing/claim` 与 `/node-link/v1/pairing/status`。Access 必须验证 endpoint 的 host 与 owner 配置一致，不一致时拒绝 claim。
- 读取后必须立即清除 fragment 与内存中的 secret；fragment、`pairingSecret` 和完整 QR payload 不得进入日志或 analytics。
- 所有 pairing HTTP 响应必须包含：

```http
Cache-Control: no-store
Pragma: no-cache
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
```

接口只接受 `application/json`；不使用 Cookie、HTTP Basic Auth 或 URL query 承载 pairing credential。

### 13.2 Claim

Access Node 生成不可导出的节点密钥（若尚未持有）、`accessNodeId` 和 `clientNonce`，然后调用：

```http
POST /node-link/v1/pairing/claim
Content-Type: application/json
```

```json
{
  "protocolVersion": 1,
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "ownerNodeId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "accessNodeId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
  "nodeName": "Office Access",
  "nodeKind": "access",
  "endpoint": "wss://work-pc.example.ts.net/node-link/v1",
  "accessPublicKey": "<base64url-65-bytes>",
  "clientNonce": "<base64url-32-bytes>",
  "proof": "<base64url-hmac-sha256>"
}
```

`proof = HMAC-SHA256(pairingSecret, node-link-pairing-proof/v1 transcript)`，字段集合见 §9.4。

只有本机用户确认后才创建信任记录。

响应：

```json
{
  "protocolVersion": 1,
  "pairingRequestId": "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d",
  "serverNonce": "<base64url-32-bytes>",
  "ownerProof": "<base64url-p1363-signature>",
  "status": "pending_confirmation",
  "expiresAt": "2026-09-18T12:15:00.000Z"
}
```

- `ownerProof` 是 Owner 对 `node-link-pairing-owner-proof/v1` transcript 的签名，Access 用二维码中固定的 `ownerPublicKey` 验证。
Owner 本地界面必须显示 `nodeName`、`nodeKind` 与指纹
- HMAC 与签名比较失败返回 `401`，且响应不得泄露具体校验差异。

### 13.3 状态确认

Access Node 使用同一 `pairingSecret` 查询状态：

```http
POST /node-link/v1/pairing/status
```

请求必须包含：

```json
{
  "protocolVersion": 1,
  "ownerNodeId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "accessNodeId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "pairingRequestId": "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d",
  "requestNonce": "<base64url-32-bytes>",
  "proof": "<base64url-hmac-sha256>"
}
```

`proof` 使用 `node-link-pairing-status/v1` domain 与 §9.4 登记的字段集合。

返回状态之一：

```text
pending_confirmation
approved
rejected
expired
consumed
```

- **状态查询一律返回 `200`**，包括 `expired` 与 `consumed`；业务状态只由 body 的 `status` 表达。
- 本机已不持有配对 secret 时（到期，或 `approved` 配对首次 WSS 认证成功后按上方末条清除），终态配对（`rejected`/`expired`/`consumed`）的状态查询仍按 `200` 报告其业务状态（终态判定不依赖对端输入），非终态配对返回 `401`。
- `approved` 响应只返回该节点的非秘密元数据与 Owner identity
- 状态查询在 `pending_confirmation`、`approved` 和 `rejected` 下可以安全重复；每次轮询生成新 `requestNonce`，只有网络重试才复用原 nonce 并获得原响应。
- 观察到 `approved` 后开始 WSS 认证；观察到 `rejected`、`expired` 或 `consumed` 后停止轮询。
- 认证成功或终态过期后必须清除二维码 payload 与内存中的 secret。

### 13.4 HTTP 状态

| HTTP 状态 | 使用场景 |
|---:|---|
| 200 | 状态查询成功，包括业务状态 `pending_confirmation`/`approved`/`rejected`/`expired`/`consumed` |
| 201 | claim 首次成功并创建 pairing request |
| 400 | JSON 或字段 schema 无效 |
| 401 | HMAC 或签名 proof 无效；响应不得泄露具体校验差异 |
| 403 | `endpoint` host 不允许 |
| 404 | pairing ID 不存在 |
| 409 | 已被 claim、状态转换冲突或 `accessNodeId` 冲突 |
| 410 | **仅** claim 端点：pairing 已过期或已 consumed |
| 413 | 请求体超过限制 |
| 429 | 配对尝试被限流 |

- `410` 不出现在状态查询端点：状态查询用 `200` + `status=expired|consumed` 表达同一情况。
- 错误 body 使用 `code/message/retryable/correlationId/details` 结构（`pairing.schema.json#/$defs/httpError`）。
- 网络响应丢失后，Access 可以用相同 claim 内容重试；若 Owner 已接受同一 `accessNodeId`/`clientNonce`，必须返回原 pairing request，而不是创建第二条记录。不同 claim 内容返回 `409`，`code = "nodelink.auth.proof_invalid"`。
- `pairingSecret` 最迟在 `expiresAt` 清除；`approved` pairing 在首次 WSS 认证成功时提前清除。拒绝状态为支持可靠轮询可以保留 secret 到原过期时间，但不得超过该时间。

**Access 侧客户端口径（2026-10-20，WP5d）**：`node pair --mode access` 由 `app::access::AccessManager` 执行本节的 claim/status。

- TLS 信任锚由组合根加载（系统根 + `node_link.access.trust_roots`，见 `CONFIG_REFERENCE.md` §3.1）；证书校验失败即失败关闭，接口地址与 fragment 不写日志，响应正文只取 `code` 归类。
- 帧前检查：URL 的 `authority` 必须与 `endpoint` 的 host 逐字相同（大小写不敏感），路径必须是 `/node-link/pair`，fragment 必须是 `data=<base64url-json>`；不一致即拒绝 claim。
- `claim` 成功前不做任何持久写入；`ownerProof` 用二维码固定的 `ownerPublicKey` 验签。`status` 观察到 `approved` 时再核对：被批准的 `accessNodeId` 是本机、`ownerNodeId`/`ownerPublicKey` 与邀请逐字一致；随后才写 Owner 信任记录（`kind = owner`、`state = paired`、`endpoint`）并建立 WSS 连接。
- Access 侧没有本地 `confirm`：确认只能由 Owner 本机完成；`node.pair.confirm` 在 Access 方向只做幂等回读。claim 状态只在内存，Daemon 重启后同一 `pairingId` 不再可查询。
- 人类核对使用 `peerPublicKeyFingerprint`（与 `--fingerprint` 逐字比较）；`pairingUrl` 只回显给本次 `node.pair.begin` 的调用方，CLI 不把它写进日志/文件。

## 14. 错误码与 Close Code

### 14.1 错误码族

```text
nodelink.protocol.invalid_json
nodelink.protocol.schema_invalid
nodelink.protocol.message_too_large
nodelink.protocol.version_unsupported
nodelink.protocol.feature_required
nodelink.protocol.type_unsupported
nodelink.protocol.sequence_invalid
nodelink.auth.proof_invalid
nodelink.auth.node_unknown
nodelink.auth.node_revoked
nodelink.auth.catalog_revision_invalid
nodelink.resource.attach_generation_stale
nodelink.resource.snapshot_unavailable
nodelink.resource.owner_unavailable
nodelink.resource.rate_limited
nodelink.command.unsupported
nodelink.command.not_found
nodelink.command.idempotency_conflict
nodelink.command.unsupported_field
nodelink.command.uncertain
nodelink.attachment.negotiation_required
nodelink.attachment.transfer_not_found
nodelink.attachment.binding_mismatch
nodelink.attachment.offset_mismatch
nodelink.attachment.chunk_conflict
nodelink.attachment.digest_mismatch
nodelink.attachment.sealed
nodelink.attachment.budget_exhausted
nodelink.attachment.expired
nodelink.attachment.revoked
nodelink.media.unavailable
nodelink.interaction.unavailable
nodelink.interaction.version_conflict
nodelink.internal.unavailable
```

- 错误码只能向后兼容地增加；客户端遇到未知错误码时按 `retryable` 与 code 首段做保守处理。
- 可恢复的命令错误优先使用 `command.rejected`；连接级错误使用 `link.error`；握手与连接的致命错误在 `link.error` 之后必须关闭连接。
- `nodelink.resource.owner_unavailable` 表示 Owner 侧不可达；Access 必须停止展示正文并保持 `no-content-cache`（§6）。

#### details 登记

| code | 登记字段（`details`） |
|---|---|
| `nodelink.protocol.feature_required` | `features`（必需） |
| `nodelink.resource.rate_limited` | `retryAfterMs`（可选） |
| `nodelink.command.unsupported_field` | `field`（必需） |
| `nodelink.attachment.offset_mismatch` | `expectedOffset`（必需）、`actualOffset`（必需） |
| `nodelink.attachment.chunk_conflict` | `offset`（必需） |
| `nodelink.media.unavailable` | `reason`（必需） |
| `nodelink.interaction.unavailable` | `reason`（必需） |
| `nodelink.interaction.version_conflict` | `expectedVersion`（可选）、`currentVersion`（必需） |

新登记字段的取值域与 Sync 面同义：`offset`/`expectedOffset`/`actualOffset`/`currentVersion`/`expectedVersion` 是十进制字符串；`media.unavailable.reason ∈ {expired, not_found, external_uri_not_fetched}`；`interaction.unavailable.reason ∈ {expired, upstream_lost, owner_offline}`。附件错误既可出现在 `attachment.error` 帧，也可作为 `command.rejected` 的 error（例如提交未封存清单）。

`parameter` 是被拒的参数名（如未知 `agentId`/`directoryId`，见 §12.7）

### 14.2 WebSocket Close Code

v1 复用 Sync 的 close code 集合与同名含义：

| Code | 含义 |
|---:|---|
| 1000 | 正常关闭 |
| 1009 | WebSocket message 过大 |
| 4400 | 协议或 schema 错误（含收到 binary frame 或压缩帧） |
| 4401 | 未认证/认证超时 |
| 4403 | 授权拒绝 |
| 4406 | 协议版本不兼容 |
| 4408 | heartbeat/handshake 超时 |
| 4409 | identity 或状态冲突 |
| 4410 | 节点已撤销 |
| 4411 | 同一 Access Node 的新连接已经替换当前连接 |
| 4429 | 限流 |
| 4500 | 服务端暂时不可用 |

表注：服务端因**授权变化**主动结束连接时用 `1000`（正常关闭）加 close reason，不用 `4410`——例如 `node.pair.confirm` 提交后的重新配对（§8.2）使既有连接作废、要求对端重连以重取 catalog，客户端因此**不**按 §15 的「撤销即停」停止重连，而是按 §15 自动重连；`4410` 只用于节点撤销（收到 `node.trust.revoked` 或本地 `node.revoke`）。

close reason 不得包含敏感信息，且不是结构化错误的替代品。

## 15. 顺序、重放和冲突

- Owner 事件必须先持久化，再发送 Node Link。
- Node Link 至少一次投递，Access Node 按 `originEventId` 去重。
- Access Node 按会话 ACK Owner cursor（`sessionRef` + `originEpoch` + `originSequence`）；ACK 只表示当前连接/来源会话的运行期处理进度，不落盘、不是可恢复索引。对本地客户端直接使用 Owner origin cursor 定位与去重，不创建 Access 本地序号映射。
- `resource.caught_up` 是来源重放的屏障：Owner 必须先重放完 `cursor` 之前的来源事件再发这一帧，Access 收到后才把重建缓冲置为可读；不得用计时器推测追平。
- session-scoped command 必须携带当前 `attachmentId`/`attachmentGeneration`；重连后先重新 attach，再恢复订阅。
- 同一会话最多一个 active turn，由 Owner Node 最终强制执行。
- 多个 Access Node 同时提交命令时，Owner 使用会话版本与串行队列裁决。
- Agent 调用结果无法确认时，Owner 写入 `uncertain`；Access Node 不能自行重试产生第二次副作用。`session.create` 与 `session.resume` 都会改变 Owner 本机状态（后者会拉起 Agent 进程），因此两者在不确定窗口结束后必须靠原 `requestId` 查询终态，不得重发。
- 断线恢复只自动重建安全查询和订阅；mutation 必须通过原 `requestId` 查询状态，不因新 attachment 自动重放。
- 节点撤销后立即拒绝新命令并关闭连接；Access 清除该来源的运行期模型与内存内容（本就不存在持久交付索引）。
- 慢消费者触发 backpressure 或断开后，由 origin cursor 重放补回，不得丢弃 Owner 事件或阻塞其他连接。
- 交互请求（权限、elicitation）属于**会话**而非某条连接：Owner 的 `permission.requested` 到达 Access 后必须由持有该会话 attachment 的 facade 转成对上游客户端的请求（外部 turn 同样如此，Zed 会渲染并应答），答案按该 interaction 的 `requestId` 关联回传为 `permission.resolve`。Owner 已写入 `interaction.already_resolved` 之后到达的应答必须被拒绝，不得覆盖既有结果。

`[决定]`（2026-09-23）**连接建立与重连**（首切片固定行为；退避参数是固定 v1 常量，不是配置键）：

- **首次连接**：Owner 侧配对 `approved` 后，Access **立即**尝试建立 Node Link 连接，不等用户下一次操作。
- **启动连接**：组合根启动完成后，对每条 `kind = owner` 且 `state = paired` 的记录自动发起连接；`pending`/`revoked` 不连。
- **重连退避**：断线后指数退避（初值 1 s、上限 60 s、每次翻倍），并且**不得小于**服务端给出的退避要求（`nodelink.resource.rate_limited` 的 `details.retryAfterMs`，§14.1）。
- **心跳超时**：按 `node_link.heartbeat_interval_ms`（默认 30 s）发送心跳；超过 90 s（固定常量，§2.5）未收到对端任何消息 → 关闭连接并进入重连，**不**删除信任；来源业务模型与重建缓冲按断线策略整代清除，重连后重新订阅并用 `session.read` 重建。
- **撤销即停**：收到 `node.trust.revoked`（§12.3）或本地 `node.revoke` 提交后→停止重连并关闭连接。该条**只**由撤销触发（这两个触发条件之一），**不**包括服务端因其它原因主动关闭连接——例如 `node.pair.confirm` 提交后的重新配对以 `1000` 关闭该节点的活动连接（§8.2）：那只是连接作废、授权已重算，对端仍按上面的首次/启动连接与重连退避规则自动重连。
- **每次连接都重取 catalog**：catalog 是连接期内存数据（§12.3），重连后重新 `catalog.subscribe`；先 `resource.attach` 再恢复订阅（本节首段）。
- **不自动重放副作用**：重连只自动恢复安全查询与订阅；mutation 只能按原 `requestId` 查询终态（本节首段）。
- **状态可见性**：Access 侧链路状态由组合根暴露在本地管理的 `daemon.status.links[]`（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.2），**不**写进 `owned_node`/`imported_import` 等持久记录（运行时状态不是授权状态）。

## 16. 网络与部署

Node Link 只要求可达的可信 HTTPS/WSS endpoint，不绑定 Tailscale：

- 同一 LAN；
- Tailscale/WireGuard/企业 VPN；
- 用户管理的反向代理或 SSH tunnel；
- 未来经 ADR 接受的其他 Transport Profile。

第一阶段不提供官方云中继、账号目录或 NAT rendezvous。网络路径中断时远程 Agent 不可操作；默认 `no-content-cache` 下 Access Node 不展示会话正文。TLS 终止点必须位于对应节点的可信边界，Node Link 仍执行应用级双向节点认证（§12.2、§9.4）。

## 17. 第一阶段范围

Node Link 是项目的第一个纵向切片，先于 PWA：

1. Node Identity、节点配对（§13 的 claim/status）与撤销（`node.trust.revoked`）；
2. 单个 Owner 与单个 Access Node；
3. 一个预配置 Agent 与一个预配置目录；
4. 远程 Zed 经 Access Node `acp_facade` 完成 initialize/session-new/prompt/update/cancel：握手用 `node.hello`/`node.challenge`/`node.proof`/`node.ready`，会话用 `resource.attach`/`resource.subscribe`/`resource.snapshot_*`/`resource.event`/`resource.ack`，命令用 `command.submit`/`command.accepted`/`command.rejected`/`command.terminal`（`command.status` 重查终态）；
5. capability 交集（§11.1）、raw ACP 保真（`node-link.raw-acp.v1` 与 `resource.event.payload.acp`）、断线重放与命令幂等（§15）；
6. 不支持 imported Agent 再导出；
7. `session/new` 经稳定 `requestId` 映射为 `command.submit{command:"session.create"}`（§12.7）；禁止在 **Node Link 的 `session.create.payload`** 中携带 `cwd`/`mcpServers`，ACP `session/new.cwd` 的处理见 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6；
8. Access Node 默认不持久化任何远程正文、交付索引、cursor/ACK 或 local sequence，远程内容只在已认证连接期经有界内存转发（§6）；
9. Owner 以 Access Node 为授权 principal，最终用户引用只用于审计（§8.3）。

首切片不实现的消息：`catalog.changed`、`resource.detach`、`node.rotate-key.request`/`node.rotate-key.result`、`link.backpressure`（§12.1 的 `post_mvp` 行）；收到时显式返回 `nodelink.protocol.type_unsupported`，不静默忽略。

本面的能力现状（实现事实，2026-10-20 WP5）：`node-link.attachment-transfer.v1` 的 Owner 入站侧（认证后的帧门控、分块/封存/清单/读取、撤销与预算）已实现，`/node-link/v1/attachments` 在组合根装配附件传输端口时注册并**同时**宣告该 feature；`node-link-client` 的 `attachment` 模块已实现 Access 侧有界内存转发（不落盘、停等、断线/撤销显式失败）。Access 侧的配对（`node pair --mode access`）与 Daemon 对 Owner 连接的实例化**已接线**（2026-10-20 WP5d）：`app::access::AccessManager` 按持久化 Owner 信任建链，远程附件转发经 `AccessManager::open_attachment` 暴露给本地调用方；从 Daemon 入口发起远程附件转发的本地路由仍属 `server::acp_facade`；`node-link.remote-history.v1` 与 `node-link.pending-interactions.v1` 未接线、不宣告，Node Link 的 `session.read` 本身仍显式回 `nodelink.command.unsupported`。imported 会话的历史显式降级、附件入口在未接线时显式不可用；不得以 Access 本地缓存或文本化冒充。边界见 §12.9/§12.10。

## 18. 必测场景

1. Access Node 重连后从 Owner cursor 补发且不重复呈现（`resource.subscribe` + `resource.event` + `resource.ack`）。
2. 同一 origin event 经重发只产生一个本地领域事件。
3. 两个 Access Node 同时 prompt，同一会话仍只有一个 active turn。
4. command accepted 后链路中断，恢复时查询到同一 terminal 状态（`command.status` → `command.terminal`）。
5. Owner Agent capability 变化后，Access facade 不继续虚报旧能力。
6. 节点撤销立即阻止命令和新订阅（`node.trust.revoked`）。
7. `local.*` 能力永不远程可用。
8. 未知 ACP 字段和超大整数跨两次持久化及 Node Link 后 raw bytes 保真。
9. 循环导入/再次导出被拒绝。
10. Owner 离线时会话正文不可用，prompt 不进入伪 accepted 状态。
11. Access Node 重启后不沿用任何本地投递状态：重新建链、向 Owner 重新订阅，并从 origin cursor 或 `session.read` 重建投递，不产生第二份会话正文数据库（§6、§12.4）。
12. Zed `session/new` 的必填绝对 `cwd` 不作为拒绝理由；直接向 Node Link `session.create.payload` 注入未知 `workspaceDirectory` 或 `cwd` 等原始路径字段时，Owner 以 `nodelink.command.unsupported_field` 拒绝且不创建会话（§12.7、[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6）。
13. 旧 attachment 的延迟 frame 在重连后被拒绝（`nodelink.resource.attach_generation_stale`），不能命中新 generation 的 SessionEndpoint。
14. 六个 transcript domain 的固定向量在 Rust 与 WebCrypto 两侧产生逐字节一致的 transcript、签名与 HMAC（§9.5）。
15. 附件连接：未协商不建连；重复同块返回原确认、不同内容重复位置拒绝、错误偏移可恢复；跨会话/跨节点标识返回 `binding_mismatch`；撤销立即中止；Access 全程不落远程正文（§12.9）。
16. 有效交互：两个 Access 节点并发应答同一请求恰好一个胜出；版本不符不消耗请求；历史卡片不能直接应答（§12.10）。
17. 远程历史（实现后）：Owner 不可达时显式不可用而非空页；分页不重不漏且 Access 磁盘无正文（§12.10）。

`core.workspace-mkdir.v1` 是 Sync 专属 feature，不属于本节 §11.3 的 Node Link feature 表；Node Link 不协商它，也不接受 `workspace.mkdir`，不得以此传入任意 Owner 路径。
