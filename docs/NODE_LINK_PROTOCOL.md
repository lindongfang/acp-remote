# ACP Remote Node Link 设计

> 状态：Node Link v1 wire 标准已冻结；`node-link-protocol` crate 已实现 §9.3/§9.4 的 transcript domain/tag 表（含固定向量测试）、v1 的全部 29 个消息类型的类型化 body（握手、catalog、resource、command、error）与信封分派，以及配对 HTTPS 载荷。节点侧状态机（attachment 当前性、origin cursor 单调性、命令幂等与 `uncertain`、Export 可见性与授权、撤销传播）尚未实现。  
> 版本：1.0  
> 修订记录（2026-09-18，v1 内合同修订，未实现未发布）：`resource.event`/`resource.ack` 增加必需 `sessionRef`；§6 无正文索引增加 `sessionId`；`command.accepted`/`command.rejected`/`command.terminal` 增加必需 `command`；`session.create` 补齐结果契约（`SessionCreateResult`）；`payloadDigest`/`snapshotDigest` 前像改为 ACPR-CJ1 与 SYNC §9.4 规则；`payload` 允许只带 `acp`；握手阶段 `link.error` 允许省略 `connectionId`/`connectionSequence`；新增错误码 `nodelink.resource.rate_limited` 与 §2.5 固定限流；Export 增加 `defaultWorkspaceAlias`/`templates`；新增 §11.4 事件类型共享合同；§14.1 新增 `details` 登记表并为 `nodelink.protocol.feature_required`/`nodelink.export.not_granted`/`nodelink.resource.rate_limited`/`nodelink.command.unsupported_field` 登记机器可读字段（兼容新增）；§12.7 的 `elicitation.respond` 增加 `decline` 动作并把 `submit` 的 `values` 放宽为 `object|null`（对齐 ACP 的 `accept`/`decline`/`cancel`，兼容新增）。  
> 日期：2026-09-18  
> 上位产品设计：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)  
> 模块边界：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)  
> 已接受决策：[ADR-0002](./adr/0002-node-link-first-owner-authority.md)  
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

命令名、pack、preset、grant 与 Owner 本地管理能力的唯一来源是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json)；授权分级的权威表是 [`SECURITY_DESIGN.md`](./SECURITY_DESIGN.md) §10.2。本文只引用这些名字，不重新定义。

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

`[决定]` 本端点与 Sync 的 `/sync/v1`、两者的配对 HTTP 路径（§13.1）**共用同一个 listener**（`CONFIG_REFERENCE.md` §1 的 `daemon.listen`），按 path 路由，不需要单独配置端口。因为对端是另一台机器，`daemon.listen` 只监听 loopback 时本端点只在同机测试下可用；对外暴露必须显式监听非 loopback 地址或经同机可信反代，具体形态见 `CONFIG_REFERENCE.md` §1 的三形态表。

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
- 明确的开放扩展点是：`resource.event.payload.view`（事件视图，未知事件按降级规则处理）、`resource.event.payload.acp`（ACP raw 文档必须逐字节保真）、错误 `details`，以及两个由外部 schema 约束的容器 `session.create.payload.templateParams`（键由该 Export 的 workspace template 声明）和 `elicitation.respond.payload.values`（键由该次 elicitation 声明）。schema 用 `additionalProperties: true` 标注这五处，其他位置一律 `additionalProperties: false`。
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
| catalog 快照批次 `exports` 条数 | 500 | 是，经 `node.ready.limits.catalogSnapshotBatchSize` |
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
- 待发送队列达到高水位后先停止读取新的 snapshot 批次；持续过慢时发送 `link.backpressure`（`post_mvp`）并断开，由 origin cursor 重放补回（§15）。
- v1 不允许 attachment、diff、终端输出通过“再分片”绕过上限；正文一律走 `resource.event` 的既有上限或 `post_mvp` 的独立内容 feature。
- 超过单连接命令速率时返回 `link.error`（`code = "nodelink.resource.rate_limited"`，`retryable = true`），并在连续超限时以 `4429` 关闭连接；`details.retryAfterMs` 给出建议退避毫秒数，存在时 Access 必须遵守后再重试。
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
- 默认只保存无正文交付索引和本地游标，不保存远程会话正文；
- 不能扩大 Owner Node 授予的能力。

### 3.4 Client

连接某个节点的调用方，包括 Zed、CLI、PWA、手机 App、桌面 App 或其他 ACP Client。设备形态不再决定产品权限，权限由 principal、scope、export policy 和会话状态决定。

### 3.5 Export / Import

- Export：Owner Node 发布一个受策略约束的 Agent/会话资源集合。
- Import：Access Node 保存该 Export 的远程引用，并将其作为 remote Agent backend 使用。

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
| Export 定义与授权 | Owner Node |
| Session、turn、permission、模型状态 | Owner Node |
| Origin event sequence 与命令终态 | Owner Node |
| Import 配置、远程 endpoint | Access Node |
| Access Node 本地 principal 与本地授权 | Access Node |
| Access Node 无正文交付索引和 local sequence | Access Node |

第一阶段默认使用 `no-content-cache`。Access Node 不把远程会话正文写入本地 SQLite，只提交用于幂等、重连和本地投递的无正文索引：

```text
ownerNodeId
exportId
sessionId
originEventId
originEpoch
originSequence
localSequence
eventType
payloadDigest
```

`localSequence` 只服务本节点客户端，不得覆盖或伪装 `originSequence`。Access Node 必须先提交这份索引，再向本地客户端广播事件；正文重放按 origin cursor 向 Owner 请求。Prompt、Agent 回复、工具内容、diff、终端输出、附件和 ACP raw document 只允许在内存中完成有界转发。Owner Node 离线时，Access Node 不展示会话正文，也不能把 prompt 标记为已接受。

未来若增加加密离线正文缓存，必须通过新的协商 feature、Owner Export Policy 和独立 ADR 显式启用；不得通过本地配置静默绕过 Owner 的 `no-content-cache`。

该策略只能约束 ACP Remote 和项目自带客户端。Access 把事件交付给 Zed 或其他第三方 ACP Client 后，不能证明对方未写入自己的数据库、日志或崩溃转储；Export Policy 必须把向此类客户端交付正文视为显式数据披露。

## 7. 资源标识

所有跨节点资源使用复合身份，不能只传本地裸 ID：

```text
RemoteAgentRef  = ownerNodeId + exportId + agentId
RemoteSessionRef = ownerNodeId + exportId + sessionId
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

两个节点首次通过二维码或一次性配对码建立信任（§13），用户在 Owner Node 本地确认 Node 名称、指纹、SAS、Export 与初始 `grant.*`。后续连接执行双向 challenge-response（§12.2、§9.4），不重复扫码。

配对结果是一条 Owner 侧信任记录：

```text
accessNodeId
accessPublicKey
nodeName / nodeKind
scopes          # grant.* 子集
exportIds       # 该 Access 可见的 Export
createdAt / revokedAt
```

### 8.3 无传递信任

`Owner A` 信任 `Access B` 不代表 A 信任 B 的本地设备，也不代表 A 信任 B 已配对的 `Node C`。第一阶段：

- Owner 只把 Access Node 视为授权 principal；
- Access Node 对其本地 Zed/PWA/CLI 负责；
- 有效权限是 `Owner export grant ∩ Access local grant ∩ runtime capability`；
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
| 配对短验证码 | `acp-remote/node-link-pairing-sas/v1` | HMAC-SHA256（key = `pairingSecret`） |
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
| `node-link-pairing-sas/v1` | `1,2,3,4,7,8,9,10,13` |
| `node-link-pairing-status/v1` | `1,2,3,4,13,16` |
| `node-link-challenge/v1` | `1,2,3,6,9,10,11,12` |
| `node-link-proof/v1` | `1,2,3,6,9,10,11,12` |

- `node-link-challenge/v1` 与 `node-link-proof/v1` 字段集合相同但 domain 不同，且分别由 Owner 与 Access 的密钥签名，避免签名跨角色复用。
- 配对 SAS 计算为 `node-link-pairing-sas/v1` transcript 的 HMAC-SHA256 输出前 4 bytes 按 u32be 解释后 `% 1_000_000`，左侧补零为 6 位十进制数；双方各自计算并显示，Owner 不得把其中一方算出的 SAS 当作另一方的结果下发。
- HMAC 比较必须使用 constant-time compare；`accessPublicKey`/`ownerPublicKey` 必须先验证为曲线上的非无穷点。
- 不允许省略本表登记的字段，也不允许加入未登记字段；验证方先检查长度与类型，再执行密码学操作。

### 9.5 固定互操作向量

跨语言固定向量位于 [`fixtures/node-link/v1/transcripts/`](../fixtures/node-link/v1/transcripts/)，每个 domain 一份，包含 `input`、`expected.transcriptBase64url`、`expected.transcriptSha256Hex`，并按证明方式附带 `expected.hmacKeyBase64url`/`hmacSha256` 或 `expected.publicKey`/`p1363Signature`。测试私钥仅用于公开 fixture，不得用于真实节点。

`fixtures/node-link/v1/manifest.json` 的 `transcriptVectors` 是这六份向量的路径清单；没有共享 fixture 不得宣称 Rust/WebCrypto 互操作完成。

## 10. Export Policy

Owner Node 显式创建 Export。默认不导出任何 Agent。

Export 至少包含：

```text
exportId
displayName
agent selectors
allowed session selectors
allowed workspace aliases/templates
defaultWorkspaceAlias      # 必须出现在 allowed workspace aliases 内
templates                  # WorkspaceTemplate[]：{ templateId, displayName, workspaceAlias, params[] }
defaultTemplateId          # 必须出现在 templates 内
scopes            # grant.* 子集
capability ceiling
retention and cache hints
createdAt / revokedAt
```

`WorkspaceTemplate.params[]` 的每一项是 `{ name, type, required, pattern, enum }`：`name` 匹配 `^[A-Za-z_][A-Za-z0-9_]{0,63}$`，每项 template 至多 32 个 param；`type ∈ string|boolean|integer`，`pattern` 只对 `string` 生效且 ≤512 字符，`enum` 为 `null` 或成员全为 string 的非空数组（v1 的 wire 如此，非 string 类型的枚举不在 v1）——空数组不合法，表示「不限制取值」用 `null`。机器权威是 [`schemas/node-link/v1/common.schema.json`](../schemas/node-link/v1/common.schema.json) 的 `workspaceTemplateParam`；`docs/LOCAL_ADMIN_PROTOCOL.md` §5.5 的本地副本必须与它同形，不得放宽或收紧。

`[决定]`（2026-09-23）**首切片的 workspace template 必须零参数**（`params = []`）：

- 原因：参数的**来源**与**用途**两端都未定义。来源侧：`session/new` 不携带这些键（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6 同时禁止从 `_meta` 之类未导出字段偷渡），而 template 本身只有约束、没有默认值，因此任何 `required = true` 的参数在 Access 侧都无法填入；用途侧：参数到了 Owner 之后影响什么（环境变量？workspace 准备？初始 prompt？）没有任何文档定义，`owned_workspace` 也只有一个固定路径、没有模板准备步骤。
- 因此首切片：Owner 发布的 template `params` 必须为空数组；Access 只在**声明了参数**的 Export 上返回明确不支持（`nodelink.command.unsupported_field`），**不得**静默忽略参数。
- 有参 template 属 `post_mvp`：启用前必须先在本节定义「值由谁提供」与「影响什么」两侧语义（可能需要新 feature 与 ADR），并按 §2.3 的兼容流程登记。
- 参数存在时的 wire 校验规则（键集、`type`/`pattern`/`enum`）仍然有效，供 `post_mvp` 与跨版本兼容使用；`session.create.payload.templateParams` 的键必须来自被选 template 的 `params`，未知键返回 `nodelink.command.unsupported_field`，类型或约束不符返回 `nodelink.export.not_granted`。首切片恰好发布一个 workspace alias 与一个 template，两者分别由 `defaultWorkspaceAlias` 与 `defaultTemplateId` 指定。

第一阶段 `cachePolicy` 固定为 `no-content-cache`；字段保留是为了以后协商更严格或经 ADR 接受的缓存模式，而不是允许 Access 自行选择正文缓存。

`scopes` 只使用 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 中登记的跨节点导出授权：

| grant | 覆盖的命令 |
|---|---|
| `grant.observe` | `session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list` |
| `grant.interact` | `session.prompt`、`session.cancel`、`elicitation.respond` |
| `grant.configure-session` | `session.mode.set`、`session.config.set` |
| `grant.approve` | `permission.resolve` |
| `grant.remote-work` | `session.create` |

- `grant.interact` 不含审批：审批必须显式给 `grant.approve`。
- 设备配对分组使用 `pack.*`，预录用 `preset.*`（`preset.remote-control` = `pack.observe` + `pack.interact` + `pack.configure-session` + `pack.approve`；`preset.read-only` = `pack.observe`）。命令、pack、grant 的完整对应关系与 `local.*` 本地管理能力见 [`SECURITY_DESIGN.md`](./SECURITY_DESIGN.md) §10.2。
- Export 的创建、修改与撤销属于 Owner 本地管理能力 `local.export.manage`，永不远程授予：Access Node 不能通过 Node Link 修改自己的 scopes 或 Export 定义。
- 设备类型不是授权依据。手机、电脑和 Zed 背后的 Access Node 都可以获得不同 grant 组合。Node Link 首个纵向切片必须实现 `grant.remote-work` 所需的 `session.create`，以便 Access Node 的 ACP facade 正确处理 Zed `session/new`；PWA UI 首版可以不展示创建入口。

远程创建会话时，调用方只能引用 Owner 发布的 `workspaceAlias` 或 template 参数，不能提交任意 Owner 绝对路径（§12.7）。Provider/MCP 凭据仍保留在 Owner Node。

## 11. 能力协商

### 11.1 端到端能力交集

端到端可宣告能力必须取以下交集：

```text
downstream Agent capability
∩ Owner Broker implementation
∩ Export capability ceiling
∩ Node Link negotiated feature
∩ Access Broker implementation
∩ local client/facade capability
```

任一环节缺失都不能向 Zed 或其他上游客户端宣告支持。ACP raw payload 必须跨 Node Link 保真；无法形成公共 view 时使用 raw fallback 或显式不支持，不能静默丢弃。

### 11.2 握手交换

Node Link 握手交换：

- protocol versions 与 feature IDs（`node.hello` → `node.challenge`）；
- 节点身份和 connection nonces（§9.4 的两个连接 domain）；
- export catalog revision（`catalog.subscribe` / `catalog.snapshot`）；
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
| `node-link.export-revoke.v1` | `export.revoked` 与撤销后立即拒命令 | mvp | 否 |
| `node-link.node-rotation.v1` | `node.rotate-key.*`（只登记 ID 与消息名） | post_mvp | 否 |

- feature ID 语法、排序与 transcript 编码与 `SYNC_PROTOCOL.md` §5.2 相同：`[a-z0-9.-]`、去重后按 UTF-8 字节升序、`negotiatedFeatures` 以 NUL 分隔。
- `node-link.core.v1` 是必需 feature（`必需` 列为"是"，`required: true`）；Access 未声明时 Owner 返回 `nodelink.protocol.feature_required`，`details.features` 列出未被选择的 feature ID（排序去重），发送方必须给出。
- 首切片必须实现前五行；`node-link.node-rotation.v1` 只登记 ID 与消息名，实现时收到 `node.rotate-key.*` 返回 `nodelink.protocol.type_unsupported`，不得静默忽略。

### 11.4 事件类型与 `view` 契约（与 Sync 共享）

Node Link 不另立一份 event type 表：

- `resource.event.eventType` 的取值集合与每个类型的最低 `view` 字段，等于 `SYNC_PROTOCOL.md` §10.2 与 §10.3 的登记集合，减去只由 Access Node 本地产生、不跨节点传输的 `session.origin.online_changed`。
- 机器表达复用 [`schemas/sync/v1/event-views.schema.json`](../schemas/sync/v1/event-views.schema.json)：Node Link 的事件 fixture 用该文件的 `$defs` 校验 `payload.view`。
- 未登记的 `eventType` 按未知事件处理：保留 `payload.acp`、向本地客户端可见降级，不得静默丢弃，也不得把结构化事件降级成普通文本。
- `view` 内允许的 JSON 取值遵守 `SYNC_PROTOCOL.md` §3.3 的 ACPR-CJ1 取值域（数字必须是整数），否则 `payloadDigest` 无法跨实现复算。

## 12. 消息族与消息体

### 12.1 家族总览

命令名、`payload` 字段名与语义以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 和 `SYNC_PROTOCOL.md` §11.5 为唯一来源；本节只冻结 Node Link 的消息名、信封位置与必需性。

| 族 | 消息 | delivery |
|---|---|---|
| 握手 | `node.hello`、`node.challenge`、`node.proof`、`node.ready` | mvp |
| Catalog | `catalog.subscribe`、`catalog.snapshot`、`export.revoked`、`node.trust.revoked` | mvp |
| Catalog | `catalog.changed` | post_mvp |
| Catalog | `node.rotate-key.request`、`node.rotate-key.result` | post_mvp |
| Resource | `resource.attach`、`resource.attached`、`resource.subscribe`、`resource.snapshot_begin`、`resource.snapshot_chunk`、`resource.snapshot_end`、`resource.event`、`resource.ack` | mvp |
| Resource | `resource.detach` | post_mvp |
| Command | `command.submit`、`command.accepted`、`command.rejected`、`command.terminal`、`command.status` | mvp |
| 控制 | `link.ping`、`link.pong`、`link.error` | mvp |
| 控制 | `link.backpressure` | post_mvp |

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
| `node.challenge` | Owner → Access | 认证前 | `selectedProtocolVersion`(integer)、`connectionId`(UUID)、`ownerNodeId`(UUID)、`serverNonce`(base64url 32B)、`selectedFeatures`(feature 列表)、`nodeProof`(base64url 64B) | 全部必需 | Owner 选择版本与 feature 子集，签发连接 ID 与 server nonce；`nodeProof` 是 §9.4 连接节点挑战 domain 的 P1363 签名 |
| `node.proof` | Access → Owner | 认证前 | `connectionId`(UUID)、`accessNodeId`(UUID)、`nodeProof`(base64url 64B) | 全部必需 | Access 对 §9.4 连接节点证明 domain 签名，完成双向认证 |
| `node.ready` | Owner → Access | 认证后 | `ownerNodeId`(UUID)、`catalogRevision`(decimal string)、`limits`(object，§2.5)、`serverEpoch`(UUID) | 全部必需 | 认证完成；`serverEpoch` 是本次 Owner 事件保留窗口的 epoch，与 Sync 的 `serverEpoch` 同义 |

- `limits` 子字段：`maxMessageBytes`、`catalogSnapshotBatchSize`、`resourceSnapshotBatchSize`、`maxInFlightCommands`、`maxPendingQueueBytes`、`maxPendingQueueMessages`、`heartbeatIntervalMs`，全部为 integer。
- `node.challenge` 的信封不含 `connectionId`/`connectionSequence`（认证前规则，§2.2）；连接 ID 在 body 中下发，与 `SYNC_PROTOCOL.md` §8.2 的 `auth.server_challenge` 同一处理。`node.proof` 与 `node.ready` 之后的全部消息使用认证后信封。
- 认证失败一律返回 `link.error` 并以对应 close code 关闭；不得只关闭连接而不给出 `code`。

### 12.3 Catalog 消息

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `catalog.subscribe` | Access → Owner | `knownRevision`(decimal string \| null) | 必需 | `null` 表示首次获取；非空表示请求增量 |
| `catalog.snapshot` | Owner → Access | `revision`(decimal string)、`exports`(export 条目数组) | 必需 | 当前可见 Export 的完整视图；`exports` 超过 limits 时按批次分成多条消息，批次内条目顺序必须稳定 |
| `catalog.changed` | Owner → Access | `revision`、`added`(export 条目数组)、`updated`(export 条目数组)、`removed`(exportId 数组) | 全部必需 | `post_mvp`；增量更新，Access 按 `revision` 去重 |
| `export.revoked` | Owner → Access | `exportId`、`revokedAt`(timestamp) | 全部必需 | 立即生效：拒绝该 Export 的新命令与订阅，Access 删除 import 引用、无正文索引与内存内容 |
| `node.trust.revoked` | Owner → Access | `revokedNodeId`(UUID)、`revokedAt`(timestamp)、`reason`(string ≤512) | 全部必需 | Owner 撤销该 Access Node 的长期信任；Access 停止重连并对本地客户端明确报告 |
| `node.rotate-key.request` | Access → Owner | `requestId`(UUID)、`newNodePublicKey`(base64url 65B)、`requestNonce`(base64url 32B) | 全部必需 | `post_mvp`；轮换请求必须幂等（同 `requestId` 重放返回同一结果） |
| `node.rotate-key.result` | Owner → Access | `requestId`(UUID)、`status`(`"accepted"`\|`"rejected"`)、`newNodePublicKey`(base64url 65B)、`rotatedAt`(timestamp)、`reason`(string \| null) | `reason` 可为 `null` | `post_mvp`；`accepted` 只有在新公钥已被 Owner 信任记录接受后返回 |

export 条目字段（`catalog.snapshot.exports[]` 与 `catalog.changed.added/updated[]` 同一形状）：

| 字段 | 类型 | 必需性 | 语义 |
|---|---|---|---|
| `exportId` | string（`^[A-Za-z0-9._-]{1,128}$`） | 必需 | Owner 侧 Export 标识，跨节点唯一 |
| `displayName` | string ≤128 | 必需 | 展示名，不含凭据 |
| `agents` | array | 必需 | 每项 `{ agentId, name, capabilitiesRef }` |
| `workspaceAliases` | array | 必需 | 每项 `{ alias, displayName }`；`alias` 是可出现在 `session.create` 的符号名 |
| `defaultWorkspaceAlias` | string（`^[a-z0-9][a-z0-9._-]{0,63}$`） | 必需 | 必须是 `workspaceAliases[].alias` 之一；Access 的 `session/new` 映射用它填 `session.create.workspaceAlias` |
| `templates` | array | 必需 | 每项 `{ templateId, displayName, workspaceAlias, params[] }`；`params[]` 每项 `{ name, type, required, pattern, enum }`，`name` 匹配 `^[A-Za-z_][A-Za-z0-9_]{0,63}$`，`type ∈ string\|boolean\|integer`，`pattern` 可为 `null`（≤512 字符），`enum` 可为 `null` 或成员全为 string 的非空数组（`schemas/node-link/v1/common.schema.json` 的 `workspaceTemplateParam` 是机器权威）。**首切片的 template 必须 `params = []`**（§10 的 `[决定]`）：参数的来源与用途都未定义，有参 template 属 `post_mvp` |
| `scopes` | array | 必需 | `grant.*` 子集，取值限于 §10 表 |
| `capabilityCeilingRef` | string（`^[A-Za-z0-9._-]{1,128}$`） | 必需 | Owner 侧能力上限的不透明引用；节点不得跨节点解释其内容 |
| `cachePolicy` | const `"no-content-cache"` | 必需 | v1 固定值 |
| `revoked` | boolean | 必需 | 撤销标记；`true` 的 Export 不参与新 attach |

`[决定]` **catalog 投影的生命周期（Access 侧）**：

- catalog 是**绑定在当前 Node Link 连接上的内存数据**，不是持久化状态：连接建立时由 `catalog.subscribe`/`catalog.snapshot` 取一次，连接断开即失效；首切片不实现 `catalog.changed`（§17），因此没有增量。
- Access **不得**把它写入 SQLite 或任何持久存储：[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §13.4 的 Access 可持久化白名单是封闭的，catalog 不在其中；[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.7 因此**不新增** catalog 表。
- 依赖它的本地操作也因此只在连接期可用：`import.add` 在无常用快照时返回 `local.unavailable`（可重试），而不是 `local.not_found`（[LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.5）；`session/new` 的映射（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6）本就需要 Owner 在线，否则返回 `resource.remote_unavailable`。
- Owner 侧相反：它从自己的 SQLite Export 记录投影出 catalog（`RemoteCatalogQueries`），不需要额外缓存。

### 12.4 Resource 消息

```text
resource.attach → resource.attached → resource.subscribe
  → resource.snapshot_begin → resource.snapshot_chunk* → resource.snapshot_end
  → resource.event* → resource.ack*
```

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `resource.attach` | Access → Owner | `remoteSessionRef`(`{ ownerNodeId, exportId, sessionId }`) | 必需 | 为耐久会话身份申请新的临时路由凭据 |
| `resource.attached` | Owner → Access | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`sessionMeta`(`{ state, version }`) | 全部必需 | 新 generation 生效；旧 generation 的 frame 一律拒绝 |
| `resource.detach` | Access → Owner | `attachmentId`(UUID)、`reason`(`"client_request"`\|`"export_revoked"`\|`"owner_unavailable"`) | 全部必需 | `post_mvp`；主动释放 attachment |
| `resource.subscribe` | Access → Owner | `attachmentId`(UUID)、`attachmentGeneration`(decimal string)、`cursor`(`{ originEpoch, originSequence }` \| null) | 全部必需 | `null` 表示先走 snapshot；非空表示从 origin cursor 增量重放 |
| `resource.snapshot_begin` | Owner → Access | `snapshotId`(UUID)、`cursor`、`schemaVersion`(integer const 1)、`chunkCount`(integer) | 全部必需 | 快照 barrier；`cursor` 是 snapshot 结束点 |
| `resource.snapshot_chunk` | Owner → Access | `snapshotId`、`chunkIndex`(decimal string)、`resource`(`"session_meta"`\|`"pending_interactions"`)、`items`(array) | 全部必需 | 只承载元数据；`resource` 决定 item 形状 |
| `resource.snapshot_end` | Owner → Access | `snapshotId`、`cursor`、`chunkCount`(integer)、`snapshotDigest`(base64url 32B) | 全部必需 | 快照完成；其后事件从 `cursor` 继续 |
| `resource.event` | Owner → Access | `originEventId`(UUID)、`originEpoch`(UUID)、`originSequence`(decimal string)、`sessionRef`(`{ ownerNodeId, exportId, sessionId }`)、`eventType`(string，`^[a-z0-9_.-]{1,128}$`)、`payloadDigest`(base64url 32B)、`createdAt`(timestamp)、`payload`(`{ view?, acp? }`，二者至少一个) | 除 `payload.acp` 外全部必需 | Owner 的 origin 事件原样投递 |
| `resource.ack` | Access → Owner | `sessionRef`(`{ ownerNodeId, exportId, sessionId }`)、`cursor`(`{ originEpoch, originSequence }`) | 全部必需 | 该会话的累计 ACK：此 cursor 及之前的 origin 事件在 Access 侧已进入可恢复的无正文索引 |

- `sessionMeta` 字段：`state`(`"idle"`\|`"queued"`\|`"running"`\|`"waiting_input"`\|`"waiting_permission"`\|`"failed"`\|`"closed"`)、`version`(decimal string)。
- `pending_interactions` item 字段：`interactionId`(UUID)、`kind`(`"permission"`\|`"elicitation"`)、`createdAt`(timestamp)、`payloadDigest`(base64url 32B)；不含交互正文。
- **正文绝不入快照**：`SessionSummary`、message、turn 与会话正文历史不存在于 Node Link 快照中；Access 需要正文时按 origin cursor 通过 `resource.event` 重放或经 `grant.observe` 的查询命令向 Owner 在线请求（Sync 侧见 `SYNC_PROTOCOL.md` §9.6）。
- `resource.event` 必须携带完整 origin 三元组（`originEventId` + `originEpoch` + `originSequence`）；缺失任一字段即 `nodelink.protocol.schema_invalid`，Access 不得用本地生成的 ID 顶替，也不得把事件降级成本地事件。
- `payload.view` 是唯一允许携带未登记字段的视图位置（开放扩展点）；事件类型在本协议登记的 view 集合内时必须携带 `payload.view`，无法形成 view 时必须保留 `payload.acp`，不得丢弃 raw 文档（schema 只要求 `payload` 内至少出现 `view` 或 `acp` 之一，具体事件类型的最低字段见 §11.4 的共享合同）。
- `resource.event.sessionRef` 与 `resource.ack.sessionRef` 必须指向当前连接上 attachment 所属的会话。`sessionRef` 不属于本连接、或 `cursor.originEpoch` 与该会话的 origin epoch 不一致时，接收方返回 `nodelink.protocol.sequence_invalid`，不得投递或接受。
- `payloadDigest = base64url(SHA-256(ACPR-CJ1(payload)))`，其中 ACPR-CJ1 是 `SYNC_PROTOCOL.md` §3.3 定义的规范 JSON；`pending_interactions[].payloadDigest` 是对创建该交互的 origin 事件的 `payload` 对象应用同一规则的结果。
- `snapshotDigest` 的计算方式与 `SYNC_PROTOCOL.md` §9.4 相同：对每个完整 `resource.snapshot_chunk` 消息的原始 UTF-8 bytes 分别求 SHA-256，按 `chunkIndex` 顺序连接后对连接结果再求一次 SHA-256；接收方不得通过重新序列化 JSON 计算该值。
- `resource.ack` 的 `cursor` 必须单调不减；回退视为 `nodelink.protocol.sequence_invalid`。

### 12.5 Command 消息

| 消息 | 方向 | body 字段 | 必需性 | 语义 |
|---|---|---|---|---|
| `command.submit` | Access → Owner | `requestId`(UUID)、`command`(命令名)、`sessionRef`(`{ ownerNodeId, exportId, sessionId }` \| null)、`attachmentId`(UUID \| null)、`attachmentGeneration`(decimal string \| null)、`expectedVersion`(decimal string \| null)、`payload`(object) | 全部必需，后四者可为 `null` | 提交命令；`payload` 形状按 `command` 取值决定（§12.7） |
| `command.accepted` | Owner → Access | `requestId`、`command`(命令名)、`acceptedAt`(timestamp)、`result`(object \| null) | 全部必需 | 同步确认接受；`acceptedAt` 永远是真实接受时间（拒绝不复用本消息），`result` 为 `null` 时表示本次回复不携带同步结果 |
| `command.rejected` | Owner → Access | `requestId`、`command`(命令名)、`error`(`{ code, message, retryable, details }`) | 全部必需 | 未接受，无副作用；本消息没有 `acceptedAt` 字段 |
| `command.terminal` | Owner → Access | `requestId`、`command`(命令名)、`terminal`(`{ status, terminalAt, terminalEventId, result, error }`) | 全部必需 | 终态；`status` ∈ `"completed"`\|`"failed"`\|`"rejected"`\|`"uncertain"`；`terminalEventId` 是 `T \| null` 字段：`command.status` 重查返回时必须非 `null`，其余情况可为 `null`；`status=completed` 时 `result` 为非空 object 且 `error=null`，其余 status 必须给出 `error` |
| `command.status` | Access → Owner | `targetRequestId`(UUID) | 必需 | 重查同一 `requestId` 的终态；回复使用同一信封的 `command.terminal` 形状 |

- `command.submit` 的 `command` 取值是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 的 12 个命令名；`sessionRef`、`attachmentId`、`attachmentGeneration`、`expectedVersion` 在不适用时显式写 `null`，不用省略代替。
- `command.accepted`、`command.rejected` 与 `command.terminal` 都必须携带 `command`，取该 request 提交时的命令名（`command.status` 查询的回复填被查询 mutation 的命令名）；结果形状因此可以只凭帧自洽分派，不依赖接收方本地的 requestId 表。
- `command.status` 有两种等价形式：作为 `command.submit` 的 `command` 提交（`payload = { "targetRequestId": … }`），或使用独立的 `command.status` 消息。两者产生相同的 `command.terminal` 形状回复；独立消息不占用 mutation 的幂等键。
- `command.accepted` 与 `command.terminal` 可以连续发送，也可以只发送 `command.terminal`：同步查询结果放在 `command.accepted.result` 或 `command.terminal.terminal.result`；`command.terminal` 始终是权威终态，客户端以 `requestId` 去重。
- Node Link 把统一结果形状拆成三条消息，`acceptedAt` 规则与 `SYNC_PROTOCOL.md` §11.2 的 `status=rejected ⇒ acceptedAt=null` 等价：`command.accepted` 的 `acceptedAt` 永远非 `null`；`command.rejected` 表达拒绝，因此完全不携带 `acceptedAt`；`command.terminal` 也不携带 `acceptedAt`。
- `command.terminal` 的 `completed` 必须携带非空 `result`（与 Sync 的 `command.result` 允许 `result` 为 `null` 不同）：该差异是有意的，因为 Node Link 的 `session.create` 必须回传 `SessionCreateResult`（§12.7）。core 只保留 `Option<CommandResult>`，**非空由本协议的适配层（`server::node_link`）在映射期保证**并由契约测试覆盖。
- session-scoped command 必须携带当前 `attachmentId`/`attachmentGeneration`；generation 过期返回 `nodelink.resource.attach_generation_stale`，Access 必须重新 attach 后再重试。该错误码不登记 `details`：Access 已知自己发出与当前持有的 generation，动作固定（重新 attach 后重试），归因靠发生错误的帧自带的 `attachmentId`，回显代际数值不改变任何分支。
- 同一 `(ownerNodeId, accessNodeId, requestId)` 的重复提交必须返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后的 `payload` 语义不同则返回 `nodelink.command.idempotency_conflict`。
- 不能确认副作用是否发生时，Owner 必须写入 `uncertain` 终态，Access 不得自动重试产生第二次副作用。

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
| `session.read` | `{ "include": ["messages"\|"turns"\|"pending_interactions"\|"config_options"\|"capabilities", …] }` |
| `command.status` | `{ "targetRequestId": UUID }` |
| `session.mode.list` | `{}` |
| `session.config.list` | `{}` |
| `session.prompt` | `{ "content": [ { "type": "text", "text": "…" }, … ] }` |
| `session.cancel` | `{ "turnId": UUID }` |
| `elicitation.respond` | `{ "interactionId": UUID, "action": "submit"\|"decline"\|"cancel", "values": object\|null }`；`submit` 的 `values` 可为 `null`（= ACP `content: null`），`decline`/`cancel` 必须为 `null` |
| `session.mode.set` | `{ "modeId": string }`，body 的 `expectedVersion` 必须存在 |
| `session.config.set` | `{ "configId": string, "value": … }`，body 的 `expectedVersion` 必须存在 |
| `permission.resolve` | `{ "interactionId": UUID, "optionId": string }` |
| `session.create` | `{ "agentId": string, "exportId": string, "workspaceAlias": string, "templateParams": object（可选） }` |

`session.create` 的硬约束：

- 只允许上述四个键；`payload` 出现 `cwd`、`mcpServers`、任何绝对路径、任何凭据字段（例如 `apiKey`、`token`、`env`、`credential`）时，Owner **必须**以 `command.rejected` 回复，`error.code = "nodelink.command.unsupported_field"`，`details.field` 必须给出被拒的字段名，且不得创建会话或部分应用参数。
- `agentId`/`exportId`/`workspaceAlias` 必须同时存在于该 Access 可见的 Export（§12.3）与 `grant.remote-work` 的授权范围内；未导出的 Agent、未知 alias 或不属于该 Export 的组合返回 `nodelink.export.not_granted`，拒绝时可给出 `details.parameter` 指明被拒的参数名。
- `workspaceAlias` 是符号名（`^[a-z0-9][a-z0-9._-]{0,63}$`），不构成路径；绝对路径在语法上就无法通过该 pattern。
- `templateParams` 只能携带 Export template 声明的键；Owner 必须在应用前按 template 校验，未知键按 `nodelink.command.unsupported_field` 拒绝。**首切片 template 零参数**（§10）：`templateParams` 必须省略或为空对象；收到声明了参数的 Export 时 Access/Owner 都要明确拒绝，不得静默忽略。
- `session.create` 的 `sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` 必须为 `null`；创建成功后 Access 通过 `resource.attach` 取得 attachment，再提交其他会话范围命令。

`session.create` 的结果契约：

- Owner 先回 `command.accepted`，此时 `result` 必须为 `null`（创建是异步的）；会话创建完成后发 `command.terminal`。
- `status = "completed"` 时 `terminal.result` 必须是 `SessionCreateResult`：

```json
{
  "remoteSessionRef": { "ownerNodeId": "<UUID>", "exportId": "<exportId>", "sessionId": "<UUID>" },
  "sessionMeta": { "state": "idle", "version": "1" }
}
```

- `sessionId` 由 Owner 生成并写入自身事件日志；Access 不得改写、重编号或本地顶替。Access 用该 `remoteSessionRef` 发起 `resource.attach`（§12.4），成功后才提交该会话的其他命令。
- `status = "failed"` 时 `terminal.error` 给出 `PublicError`（例如 `nodelink.export.not_granted`、`nodelink.command.unsupported_field`）；`status = "uncertain"` 表示崩溃窗口内无法确认会话是否已创建，Access **不得**自动重试 `session.create`，必须向调用方返回显式错误，由用户决定是否以新 `requestId` 重试。

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
| `export.revoked` | `catalog.schema.json` | `exportRevoked` |
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

Owner 必须原子检查 pairing 存在、未过期、仍为 `created`、`endpoint` host 匹配和 HMAC 正确。成功后依次进入 `claimed` 和 `pending_confirmation`，不再接受第二个 claim；只有本机用户确认后才创建信任记录并分配初始 `grant.*`。

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
- 双方各自计算并显示 SAS（§9.4）；Owner 本地界面必须显示同一个 SAS、`nodeName`、`nodeKind` 与待授予的 `grant.*`。
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
- `approved` 响应只返回该节点的非秘密元数据、`grant.*` 与 Owner identity；不签发 bearer token。首次正式 WSS 连接仍执行完整 challenge-response。
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
nodelink.export.not_found
nodelink.export.revoked
nodelink.export.not_granted
nodelink.resource.attach_generation_stale
nodelink.resource.snapshot_unavailable
nodelink.resource.owner_unavailable
nodelink.resource.rate_limited
nodelink.command.unsupported
nodelink.command.not_found
nodelink.command.idempotency_conflict
nodelink.command.unsupported_field
nodelink.command.uncertain
nodelink.internal.unavailable
```

- 错误码只能向后兼容地增加；客户端遇到未知错误码时按 `retryable` 与 code 首段做保守处理。
- 可恢复的命令错误优先使用 `command.rejected`；连接级错误使用 `link.error`；握手与连接的致命错误在 `link.error` 之后必须关闭连接。
- `nodelink.resource.owner_unavailable` 表示 Owner 侧不可达；Access 必须停止展示正文并保持 `no-content-cache`（§6）。

#### details 登记

| code | 登记字段（`details`） |
|---|---|
| `nodelink.protocol.feature_required` | `features`（必需） |
| `nodelink.export.not_granted` | `parameter`（可选） |
| `nodelink.resource.rate_limited` | `retryAfterMs`（可选） |
| `nodelink.command.unsupported_field` | `field`（必需） |

未在上表列出的 code **不登记任何** `details` 字段：所有 `link.error`、`command.rejected` 与配对 HTTP 错误响应的 `details` 必须送 `{}`。字段的取值域由 registry 的 `details` 片段约束：`features` 是 1..64 个 feature ID 的排序去重列表（`requiredFeatures` 中未被选择的那些）；`field` 是被拒的 `command.submit.payload` 字段名（例如 `cwd`、`mcpServers`，见 §2.4/§12.7）；`parameter` 是被拒的 Export 参数名（例如未导出的 `agentId`、未知 `workspaceAlias`，见 §17 第 12 条）；`retryAfterMs` 是建议退避毫秒数。每个登记字段都要标注 `必需` 或 `可选`，与 registry 的 `required` 逐一对应。`details` 始终是开放扩展点（接收方必须忽略未知字段，§2.4），新增字段按 §8 的兼容变更登记。上表与 `compatibility/errors/v1/errors.json` 的 `details` 片段逐条相等，由 `npm run check` 的 `check:errors` 断言。

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

close reason 不得包含敏感信息，且不是结构化错误的替代品。

## 15. 顺序、重放和冲突

- Owner 事件必须先持久化，再发送 Node Link。
- Node Link 至少一次投递，Access Node 按 `originEventId` 去重。
- Access Node 按会话 ACK Owner cursor（`sessionRef` + `originEpoch` + `originSequence`）；给本地客户端使用独立 local cursor。
- session-scoped command 必须携带当前 `attachmentId`/`attachmentGeneration`；重连后先重新 attach，再恢复订阅。
- 同一会话最多一个 active turn，由 Owner Node 最终强制执行。
- 多个 Access Node 同时提交命令时，Owner 使用会话版本与串行队列裁决。
- Agent 调用结果无法确认时，Owner 写入 `uncertain`；Access Node 不能自行重试产生第二次副作用。
- 断线恢复只自动重建安全查询和订阅；mutation 必须通过原 `requestId` 查询状态，不因新 attachment 自动重放。
- Export 撤销后立即拒绝新命令并关闭订阅；Access 删除 import、无正文交付索引和内存内容。若未来启用离线正文缓存，不得声称 Owner 可以远程可靠擦除所有副本。
- 慢消费者触发 backpressure 或断开后，由 origin cursor 重放补回，不得丢弃 Owner 事件或阻塞其他连接。
- 交互请求（权限、elicitation）属于**会话**而非某条连接：Owner 的 `permission.requested` 到达 Access 后必须由持有该会话 attachment 的 facade 转成对上游客户端的请求（外部 turn 同样如此，Zed 会渲染并应答），答案按该 interaction 的 `requestId` 关联回传为 `permission.resolve`。Owner 已写入 `interaction.already_resolved` 之后到达的应答必须被拒绝，不得覆盖既有结果。

`[决定]`（2026-09-23）**连接建立与重连**（首切片固定行为；退避参数是固定 v1 常量，不是配置键）：

- **首次连接**：Owner 侧配对 `approved` 后，Access **立即**尝试建立 Node Link 连接，不等用户下一次操作。
- **启动连接**：组合根启动完成后，对每条 `kind = owner` 且 `state = paired` 的记录自动发起连接；`pending`/`revoked` 不连。
- **重连退避**：断线后指数退避（初值 1 s、上限 60 s、每次翻倍），并且**不得小于**服务端给出的退避要求（`nodelink.resource.rate_limited` 的 `details.retryAfterMs`，§14.1）。
- **心跳超时**：按 `node_link.heartbeat_interval_ms`（默认 30 s）发送心跳；超过 90 s（固定常量，§2.5）未收到对端任何消息 → 关闭连接并进入重连，**不**删除信任、**不**清理无正文索引。
- **撤销即停**：收到 `node.trust.revoked`（§12.6）或本地 `node.revoke` 提交后→停止重连并关闭连接。
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

1. Node Identity、节点配对（§13 的 claim/status）、撤销（`node.trust.revoked`、`export.revoked`）；
2. 单个 Owner 与单个 Access Node；
3. 一个 Export、一个预配置 Agent 和一个预配置 workspace template；
4. 远程 Zed 经 Access Node `acp_facade` 完成 initialize/session-new/prompt/update/cancel：握手用 `node.hello`/`node.challenge`/`node.proof`/`node.ready`，会话用 `resource.attach`/`resource.subscribe`/`resource.snapshot_*`/`resource.event`/`resource.ack`，命令用 `command.submit`/`command.accepted`/`command.rejected`/`command.terminal`（`command.status` 重查终态）；
5. capability 交集（§11.1）、raw ACP 保真（`node-link.raw-acp.v1` 与 `resource.event.payload.acp`）、断线重放与命令幂等（§15）；
6. 不支持 imported Agent 再导出；
7. `session/new` 经稳定 `requestId` 映射为受 `grant.remote-work`、Agent selector 和 workspace template 限制的 `command.submit{command:"session.create"}`（§12.7）；禁止在 **Node Link 的 `session.create.payload`** 中携带 `cwd`/`mcpServers`，ACP `session/new.cwd` 的处理见 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6；
8. Access Node 默认仅持久化无正文交付索引（§6）；
9. Owner 以 Access Node 为授权 principal，最终用户引用只用于审计（§8.3）。

首切片不实现的消息：`catalog.changed`、`resource.detach`、`node.rotate-key.request`/`node.rotate-key.result`、`link.backpressure`（§12.1 的 `post_mvp` 行）；收到时显式返回 `nodelink.protocol.type_unsupported`，不静默忽略。

## 18. 必测场景

1. Access Node 重连后从 Owner cursor 补发且不重复呈现（`resource.subscribe` + `resource.event` + `resource.ack`）。
2. 同一 origin event 经重发只产生一个本地领域事件。
3. 两个 Access Node 同时 prompt，同一会话仍只有一个 active turn。
4. command accepted 后链路中断，恢复时查询到同一 terminal 状态（`command.status` → `command.terminal`）。
5. Owner Agent capability 变化后，Access facade 不继续虚报旧能力。
6. Export 撤销立即阻止命令和新订阅（`export.revoked`）。
7. Access Node 本地高权限 principal 不能突破 Owner grant。
8. 未知 ACP 字段和超大整数跨两次持久化及 Node Link 后 raw bytes 保真。
9. 循环导入/再次导出被拒绝。
10. Owner 离线时会话正文不可用，prompt 不进入伪 accepted 状态。
11. Access Node 重启后可凭无正文交付索引和 origin cursor 从 Owner 重建投递，不产生第二份会话正文数据库。
12. Zed `session/new` 的必填绝对 `cwd` 不作为拒绝理由，也不改变 Owner 选定的 workspace；Access facade 对未导出的 Agent/Export 明确拒绝。直接向 Node Link `session.create.payload` 注入未知 `workspaceAlias` 或 `cwd` 等原始路径字段时，Owner 分别以 `nodelink.export.not_granted` / `nodelink.command.unsupported_field` 拒绝，且不创建会话（§12.7、[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6）。
13. 旧 attachment 的延迟 frame 在重连后被拒绝（`nodelink.resource.attach_generation_stale`），不能命中新 generation 的 SessionEndpoint。
14. 六个 transcript domain 的固定向量在 Rust 与 WebCrypto 两侧产生逐字节一致的 transcript、签名与 HMAC（§9.5）。
