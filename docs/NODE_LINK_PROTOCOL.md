# ACP Remote Node Link 设计

> 状态：编码前协议边界（Draft）  
> 版本：0.1  
> 日期：2026-09-18  
> 上位产品设计：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)  
> 模块边界：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)  
> 已接受决策：[ADR-0002](./adr/0002-node-link-first-owner-authority.md)

## 1. 文档职责

Node Link 是两个 ACP Remote 节点之间的安全访问协议。它解决“一个节点拥有 Agent，另一个节点向本地 Zed、CLI、PWA 或原生客户端提供该 Agent”的场景。

Node Link 不等于：

- ACP stdio 的透明 TCP 转发；
- 手机/PWA 使用的 Sync Protocol；
- Tailscale、VPN 或某个具体网络产品；
- 云端账号中心或持久中继；
- 自动形成传递信任的节点 federation。

本文先固定语义、不变量和消息族；完整 wire schema 在进入 Node Link 实现阶段前单独补齐。

## 2. 术语

### 2.1 ACP Remote Node

一个运行 ACP Remote Daemon、拥有稳定 `nodeId` 和 Node Identity Key 的实例。节点可以同时承担多种角色，不需要安装成不同产品。

### 2.2 Owner Node

直接管理某个 Agent 或会话的资源归属节点：

- 是该 Agent 的 ACP Client；
- 管理 Agent 生命周期和 workspace；
- 是会话状态、active turn、事件日志和命令终态的唯一权威；
- 决定向其他节点导出哪些 Agent、会话和操作权限。

### 2.3 Access Node

从 Owner Node 导入 Agent/会话并向本地客户端提供入口的访问节点：

- 可以通过本地 `acp-facade` 服务 Zed；
- 可以通过 Sync Server 服务 PWA、手机或桌面 UI；
- 默认只保存无正文交付索引和本地游标，不保存远程会话正文；
- 不能扩大 Owner Node 授予的能力。

### 2.4 Client

连接某个节点的调用方，包括 Zed、CLI、PWA、手机 App、桌面 App 或其他 ACP Client。设备形态不再决定产品权限，权限由 principal、scope、export policy 和会话状态决定。

### 2.5 Export / Import

- Export：Owner Node 发布一个受策略约束的 Agent/会话资源集合。
- Import：Access Node 保存该 Export 的远程引用，并将其作为 remote Agent backend 使用。

## 3. 目标拓扑

```text
远程 Zed
   │ local ACP stdio
   ▼
Access Node / acp-facade
   │
   │ Node Link over trusted HTTPS/WSS path
   ▼
Owner Node / Broker ── ACP stdio ── Company Agent
   ▲
   ├── 公司本地 Zed / CLI
   └── 直接连接的 PWA / 手机 / 电脑客户端
```

同一节点可以既导入远程 Agent，又导出自己的本地 Agent。第一阶段每个资源只允许一个 Node Link hop：导入的 Agent 可以提供给本节点本地客户端，但不能再次通过 Node Link 导出给第三个节点。

## 4. 为什么不直接转发 ACP

直接把 ACP JSON-RPC 放进 WSS tunnel 无法独立解决：

- 节点身份和长期配对；
- Agent/会话导出策略；
- 多个本地客户端的授权与审计；
- 断线 cursor、事件重放和慢消费者隔离；
- 跨节点命令幂等与 `uncertain`；
- Owner Node 权威和 Access Node 缓存边界；
- 防止 capability 在多跳链路上被虚报。

因此 Node Link 使用专用、版本化的 command/event/catalog 协议，并在需要时携带 ACP raw document。两端仍通过 ACP compatibility matrix 保证端到端语义，不把 Node Link 变成最低公共能力抽象。

## 5. 权威模型

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
originEventId
originSequence
originEpoch
localSequence
eventType
payloadDigest
```

`localSequence` 只服务本节点客户端，不得覆盖或伪装 `originSequence`。Access Node 必须先提交这份索引，再向本地客户端广播事件；正文重放按 origin cursor 向 Owner 请求。Prompt、Agent 回复、工具内容、diff、终端输出、附件和 ACP raw document 只允许在内存中完成有界转发。Owner Node 离线时，Access Node 不展示会话正文，也不能把 prompt 标记为已接受。

未来若增加加密离线正文缓存，必须通过新的协商 feature、Owner Export Policy 和独立 ADR 显式启用；不得通过本地配置静默绕过 Owner 的 `no-content-cache`。

该策略只能约束 ACP Remote 和项目自带客户端。Access 把事件交付给 Zed 或其他第三方 ACP Client 后，不能证明对方未写入自己的数据库、日志或崩溃转储；Export Policy 必须把向此类客户端交付正文视为显式数据披露。

## 6. 资源标识

所有跨节点资源使用复合身份，不能只传本地裸 ID：

```text
RemoteAgentRef  = ownerNodeId + exportId + agentId
RemoteSessionRef = ownerNodeId + exportId + sessionId
OriginEventRef  = ownerNodeId + originEpoch + eventId
LiveSessionRoute = RemoteSessionRef + attachmentId + attachmentGeneration
```

Access Node 可以生成本地 opaque handle，但持久化层必须保留完整来源。来自不同 Owner Node 的相同 `sessionId` 永远不是同一个会话。

`RemoteSessionRef` 是耐久资源身份，`attachmentId/generation` 是当前 Node Link 连接上的临时路由凭据。Access 每次重新连接或重新 attach 会话时必须取得新的 attachment；旧连接延迟到达的 command/event frame 必须被 Owner 拒绝，不能仅凭相同 `sessionId` 投递到新连接。这一分离参考 Pi 的 durable Session 与 live presentation attachment 模型，但 Node Link 仍使用自己的认证、授权和 wire schema。

## 7. 身份、配对与信任

### 7.1 Node Identity

Node Identity 与 PWA Device Identity 是不同用途的长期身份：

- 密钥算法第一阶段仍采用 ECDSA P-256，以复用已审查实现；
- key purpose、数据库 record type、签名 domain tag 和撤销记录必须分离；
- Node 私钥不能导出给浏览器或通过二维码传输；
- TLS 临时连接密钥仍与长期 Node Identity Key 分离。

### 7.2 长期配对

两个节点首次通过二维码或一次性配对码建立信任，用户在 Owner Node 本地确认 Node 名称、指纹、SAS、Export 与初始 scopes。后续连接执行双向 challenge-response，不重复扫码。

### 7.3 无传递信任

`Owner A` 信任 `Access B` 不代表 A 信任 B 的本地设备，也不代表 A 信任 B 已配对的 `Node C`。第一阶段：

- Owner 只把 Access Node 视为授权 principal；
- Access Node 对其本地 Zed/PWA/CLI 负责；
- 有效权限是 `Owner export grant ∩ Access local grant ∩ runtime capability`；
- imported Agent 禁止再次通过 Node Link 导出；
- 审计记录包含 `viaNodeId` 和由 Access Node 认证的 `localPrincipalRef`，但 Owner 不把后者当成自己直接认证的身份。

这是第一阶段明确接受的节点级信任模型。Owner 可以按 Access Node 单独授权、限流和撤销，但不能据此声称已经端到端认证实际操作人。需要 Owner 直接认证员工身份时，必须新增独立用户身份协议，不能把 `localPrincipalRef` 升格为安全凭据。

## 8. Export Policy

Owner Node 显式创建 Export。默认不导出任何 Agent。

Export 至少包含：

```text
exportId
displayName
agent selectors
allowed session selectors
allowed workspace aliases/templates
scopes
capability ceiling
retention and cache hints
createdAt / revokedAt
```

第一阶段 `cachePolicy` 固定为 `no-content-cache`；字段保留是为了以后协商更严格或经 ADR 接受的缓存模式，而不是允许 Access 自行选择正文缓存。

建议的授权 Profile：

| Profile | 能力 |
|---|---|
| `observe` | 发现已导出 Agent/会话、查看历史与实时事件 |
| `interact` | prompt、cancel、permission/elicitation 响应、允许的模型/模式切换 |
| `remote-work` | 在 Owner 预配置的 Agent 与 workspace template 中创建会话 |
| `admin` | 管理 Export；不默认包含 Provider credential 读取 |

设备类型不是授权依据。手机、电脑和 Zed 背后的 Access Node 都可以获得不同 Profile。Node Link 首个纵向切片必须实现 `remote-work` 所需的 `session.create`，以便 Access Node 的 ACP facade 正确处理 Zed `session/new`；PWA UI 首版可以不展示创建入口。

远程创建会话时，调用方只能引用 Owner 发布的 `workspaceAlias` 或 template 参数，不能提交任意 Owner 绝对路径。Provider/MCP 凭据仍保留在 Owner Node。

## 9. 能力协商

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

Node Link 握手至少交换：

- protocol versions 与 feature IDs；
- node identity 和 connection nonces；
- export catalog revision；
- max message、snapshot、in-flight command 和 replay limits；
- ACP wire versions；
- raw payload、compression 等可选能力。

## 10. 消息族

完整 wire schema 后续定义，但语义固定为：

```text
node.hello / node.challenge / node.proof / node.ready
catalog.subscribe / catalog.snapshot / catalog.changed
resource.attach / resource.attached / resource.detach
resource.subscribe / resource.snapshot / resource.event / resource.ack
command.submit / command.accepted / command.rejected
command.terminal / command.status
heartbeat / error
```

查询命令可以同步完成；mutation 只同步确认接受，最终结果必须使用持久化 terminal event。所有可重试 mutation 携带稳定 `requestId`，幂等键至少包含 `(ownerNodeId, accessNodeId, requestId)`。

## 11. 顺序、重放和冲突

- Owner 事件必须先持久化，再发送 Node Link。
- Node Link 至少一次投递，Access Node 按 `originEventId` 去重。
- Access Node ACK 的是 Owner cursor；给本地客户端使用独立 local cursor。
- session-scoped command 必须携带当前 `attachmentId/generation`；重连后先重新 attach，再恢复订阅。
- 同一会话最多一个 active turn，由 Owner Node 最终强制执行。
- 多个 Access Node 同时提交命令时，Owner 使用会话版本与串行队列裁决。
- Agent 调用结果无法确认时，Owner 写入 `uncertain`；Access Node 不能自行重试产生第二次副作用。
- 断线恢复只自动重建安全查询和订阅；mutation 必须通过原 `requestId` 查询状态，不因新 attachment 自动重放。
- Export 撤销后立即拒绝新命令并关闭订阅；Access 删除 import、无正文交付索引和内存内容。若未来启用离线正文缓存，不得声称 Owner 可以远程可靠擦除所有副本。

## 12. 网络与部署

Node Link 只要求可达的可信 HTTPS/WSS endpoint，不绑定 Tailscale：

- 同一 LAN；
- Tailscale/WireGuard/企业 VPN；
- 用户管理的反向代理或 SSH tunnel；
- 未来经 ADR 接受的其他 Transport Profile。

第一阶段不提供官方云中继、账号目录或 NAT rendezvous。网络路径中断时远程 Agent 不可操作；默认 `no-content-cache` 下 Access Node 不展示会话正文。TLS 终止点必须位于对应节点的可信边界，Node Link 仍执行应用级双向节点认证。

## 13. 第一阶段范围

Node Link 是项目的第一个纵向切片，先于 PWA：

1. Node Identity、节点配对、撤销；
2. 单个 Owner 与单个 Access Node；
3. 一个 Export、一个预配置 Agent 和一个预配置 workspace template；
4. 远程 Zed 经 Access Node `acp-facade` 完成 initialize/session-new/prompt/update/cancel；
5. capability 交集、raw ACP 保真、断线重放与命令幂等；
6. 不支持 imported Agent 再导出；
7. `session/new` 经稳定 `requestId` 映射为受 `remote-work`、Agent selector 和 workspace template 限制的远程 `session.create`；
8. Access Node 默认仅持久化无正文交付索引；
9. Owner 以 Access Node 为授权 principal，最终用户引用只用于审计。

## 14. 必测场景

1. Access Node 重连后从 Owner cursor 补发且不重复呈现。
2. 同一 origin event 经重发只产生一个本地领域事件。
3. 两个 Access Node 同时 prompt，同一会话仍只有一个 active turn。
4. command accepted 后链路中断，恢复时查询到同一 terminal 状态。
5. Owner Agent capability 变化后，Access facade 不继续虚报旧能力。
6. Export 撤销立即阻止命令和新订阅。
7. Access Node 本地高权限 principal 不能突破 Owner grant。
8. 未知 ACP 字段和超大整数跨两次持久化及 Node Link 后 raw bytes 保真。
9. 循环导入/再次导出被拒绝。
10. Owner 离线时会话正文不可用，prompt 不进入伪 accepted 状态。
11. Access Node 重启后可凭无正文交付索引和 origin cursor 从 Owner 重建投递，不产生第二份会话正文数据库。
12. Zed `session/new` 携带未导出的 Agent、未知 workspace alias 或任意绝对路径时被明确拒绝。
13. 旧 attachment 的延迟 frame 在重连后被拒绝，不能命中新 generation 的 SessionEndpoint。
