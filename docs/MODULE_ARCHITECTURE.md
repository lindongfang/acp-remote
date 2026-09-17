# ACP Remote 模块架构

> 状态：编码前模块边界设计
> 版本：0.2
> 日期：2026-09-18
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)
> 已接受决策：[ADR-0003](./adr/0003-pi-inspired-module-boundaries.md)

## 1. 设计理念

本项目参考 Pi/Oh My Pi 的模块理念，但不机械复制其 TypeScript 目录结构：

- 保持核心最小，只保留不可替代的业务规则。
- 同一个核心支持 CLI、PWA、手机、Zed 和其他 ACP Remote Node 等多个入口。
- wire protocol、客户端、服务端、会话核心、持久化后端和应用组合分离。
- 每个模块只有一个主要变化原因。
- 高级行为优先通过适配器和配置扩展。

Pi 当前进一步把远程会话能力拆成独立 `protocol`、`client`、`server`、`agent-core` 和 SQLite session backend：协议只处理传输中立的信封与 framing，client/server 不解释应用业务 payload，核心不引入平台 SQLite。Oh My Pi 同样从 interactive、RPC、SDK、ACP 等入口复用同一 session engine。ACP Remote 采用相同原则：一个会话核心，多种 wire protocol，多种入站入口，以及本地/远程两种 Agent backend。

本项目的最小核心是：

```text
会话状态机
+ 每会话串行命令队列
+ ACP 命令路由
+ 事件排序与幂等
+ 多客户端同步规则
```

## 2. 总体依赖规则

```text
Clients / Peer Nodes
        │
        ▼
server adapters ───────> wire protocol crates
        │
        ▼
core::use_cases ───────> core::model
        │
        ├──────────────> core::ports
        │                       ▲
        ▼                       │
agent-host / node-link-client / storage-sqlite / identity-auth
```

依赖只允许指向更稳定的模块：

```text
app             -> server / backends / identity-auth / core
server          -> core + corresponding wire protocols
outbound backend-> core + corresponding wire protocols
core::use_cases -> core::ports + core::model
wire protocols  -> no core or infrastructure dependency
```

禁止：

```text
core -> axum/sqlx/tokio::process/Noise implementation
wire protocol -> core/domain model
storage-sqlite -> agent-host
server::sync -> server::node_link
agent-host -> server
adapter A -> adapter B
```

适配器之间只能通过 `core` 定义的 use case 或 port 协作。协议 crate 只拥有 wire schema、codec、framing、limits 和版本协商，不拥有业务领域类型；wire 与 core 的 mapper 属于使用该协议的 adapter。

### 2.1 客户端边界

UI 客户端不属于 Rust core，通过版本化 `sync-protocol` 与 `server::sync` 通信。ACP Remote 节点通过独立的 `node-link-protocol` 通信。wire contract 分别以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 和 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准；系统安全边界以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为准；前端阶段、分层、PWA 限制和后续原生 adapter 约束以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为准。

```text
PWA / Android / iOS
        ↓ Sync Protocol
server::sync -> core::use_cases

Access Node
        ↓ Node Link Protocol
server::node_link -> core::use_cases
core -> SessionBackendFactory -> node-link-client -> Owner Node
```

- 前端首个交付只实现 PWA，并位于 Node Link 首个纵向切片之后；后续原生客户端不能绕过 Sync Protocol 直接调用 Broker。
- 客户端可以共享协议类型、状态机和 feature 层，但不能复制服务端业务规则。
- Web/Native 存储、密钥、相机和生命周期差异必须通过客户端平台 adapter 隔离。
- PWA 的安全降级不能反向降低 Daemon 的认证与授权要求。

## 3. 建议的 Rust Workspace

```text
crates/
├─ core/                    领域、用例、端口与会话协调
├─ acp-protocol/
├─ sync-protocol/
├─ node-link-protocol/
├─ agent-host/              本地 ACP Agent backend
├─ node-link-client/        远程 Agent backend
├─ storage-sqlite/          core 持久化后端
├─ identity-auth/           身份、配对、签名与平台 keystore
├─ server/                  sync / node-link / acp-facade 入站模块
└─ app/                     daemon、CLI 与组合根
```

这组物理 crate 刻意少于逻辑模块数，接近 Pi 的“core + protocol + client + server + backend + app”划分。`model`、`use_cases`、`ports` 和 `broker` 先作为 `core` 内部模块；`sync`、`node_link` 和 `acp_facade` 先作为 `server` 内部平级模块。只有出现独立发布、编译隔离或明显构建成本后才继续拆 crate。

协议 crate 是例外：ACP、Sync 和 Node Link 分别拥有独立兼容周期与 fixture，必须从第一天物理隔离，且不得依赖 `core`。

## 4. 模块职责

### 4.1 `core`

唯一职责：承载与传输、数据库和具体 Agent 无关的会话业务。参考 Pi `agent-core`，首日把稳定且共同演进的领域、用例、端口和协调逻辑放在一个 crate 内，而不是为了目录整齐拆成三个相互依赖的 crate。

内部模块：

```text
core::model       值对象、状态机、事件与不变量
core::use_cases   Session/Model/Permission/Export/Subscription 用例
core::ports       backend、事务存储、身份仓库、时钟等能力接口
core::broker      Session Actor、命令协调与事件提交
core::testing     fake ports 和契约测试工具，仅测试 feature 导出
```

`model` 至少包含：

```text
NodeId / DeviceId / ExportId / SessionId / EventId / RequestId
OwnedSessionRef / RemoteSessionRef / OriginEventRef
Session / SessionState / TurnState / ResourceOrigin
Command / CommandResult / CommandTerminal
Event / EventKind / EventOrigin / PersistencePolicy
ModelRef / AgentRef / Capability
PermissionRequest / PermissionDecision
Sequence / Version / AttachmentGeneration
```

规则：

- `core` 不依赖任何 wire protocol、Tokio runtime、Axum、SQLite、WebSocket、子进程和具体 Agent。
- 领域类型不包含 HTTP 状态码、数据库列名、JSON-RPC method 或 Node Link wire discriminator。
- ID 使用 newtype；状态转换由领域方法验证，adapter 不能直接修改状态字段。
- Owned 与 imported session 必须在类型或 `ResourceOrigin` 上可区分，禁止依靠 nullable `ownerNodeId` 猜测持久化规则。
- `core::broker` 实现每会话串行、active turn、授权调用点、幂等、permission first-writer-wins、模型切换时机和先提交后发布。

入站 use case 按能力拆分：

```text
SessionCommands / SessionQueries
ModelCommands
PermissionCommands
SubscriptionQueries
DeviceManagement
ExportManagement
RemoteCatalogQueries
```

出站 port 不使用巨型 `AgentRuntime`，而采用会话级能力：

```text
AgentCatalog             枚举可用 Agent 与能力
SessionBackendFactory    受约束地 create/open SessionEndpoint
SessionEndpoint          prompt/cancel/set-mode/events，绑定单个 live session
SessionStore             原子提交 owned session 状态、事件与 requestId 幂等
RemoteDeliveryStore      只提交 imported event 的 cursor/digest/local-sequence 索引
TrustStore               设备、节点、Export grant 与撤销元数据
EventPublisher           发布已经提交的事件或远程交付
Clock / IdGenerator      可测试时间与 ID
```

`SessionStore` 必须提供单一事务提交 API，不能让 Broker 分别调用 `SessionRepository`、`EventJournal`、`CommandDeduper` 后假设三次调用天然原子。`SessionEndpoint` 表示带生命周期的会话句柄；本地与远程 backend 都实现相同接口，但不得把进程、socket 或 wire DTO 暴露给 core。

### 4.2 `acp-protocol`

唯一职责：描述并编解码 ACP wire protocol。

包含 JSON-RPC envelope、ACP wire DTO、codec、raw document、capability wire schema、limits 和协议 fixture。协议类型必须保留未知字段与 Agent 扩展 payload。ACP 覆盖集合与跨层验收合同以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 和 `compatibility/acp/v1/matrix.json` 为准。

它不依赖 `core`，不包含领域 mapper，不启动子进程、不管理会话、不访问数据库。ACP wire 到公共领域视图的 mapper 位于 `agent-host` 或 `server::acp_facade`，因为映射方向取决于 adapter 角色。

### 4.3 `sync-protocol`

唯一职责：定义 UI 客户端与其所连接 ACP Remote Node 之间的版本化 wire protocol。

具体消息、transcript、游标、重放和兼容语义由 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 定义；本模块不得另行发明第二套格式。

包含：

```text
Handshake envelope
Subscribe / Snapshot / Event / Ack
ClientCommand / CommandAccepted / CommandRejected
Pairing DTO
ProtocolVersion / Feature negotiation
```

它不允许客户端发送任意 ACP JSON-RPC，也不依赖 `core`。Sync wire 与 core 的 mapper 位于 `server::sync`。

### 4.4 `node-link-protocol`

唯一职责：定义两个 ACP Remote Node 之间的版本化 wire protocol。

包含 Node handshake、Export catalog、resource snapshot/event/ACK、跨节点 command、origin cursor、错误和 feature negotiation。完整语义以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准。

它不能直接复用 Sync DTO 冒充节点协议，也不能把 ACP stdio 透明封装成网络 tunnel。它不依赖 `core`；Node Link wire 与 core 的 mapper 位于 `node-link-client` 和 `server::node_link`。

### 4.5 `agent-host`

唯一职责：把一个本地 ACP 子进程实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。

包含：

- 启动和监督 ACP Agent 子进程。
- stdin/stdout JSON-RPC transport。
- request ID、ACP session ID 与 live endpoint generation 映射。
- capability negotiation。
- stderr 日志、超时、取消和进程树清理。
- 通用 Agent profile registry。
- 隔离的兼容性 quirk。

wire/core mapper 也位于本 crate，但必须把 `acp-protocol::RawDocument` 与公共领域 view 同时交给 core，不能只交规范化文本。优先使用一个通用 ACP 实现；Codex、OMP 差异优先表示为 capability/profile 数据。

### 4.6 `node-link-client`

唯一职责：把 Owner Node 导出的远程 Agent 实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。

它负责 Import 配置、catalog、live attachment、无正文交付索引、跨节点 requestId、origin cursor 和显式重连。参考 Pi client，客户端连接逻辑依赖最小 `NodeLinkTransport` 接口；WSS 是一个 transport 实现，协议和 session backend 不感知 Tailscale 或 socket 细节。

默认 `no-content-cache` 下不得把 prompt、回复、工具内容、diff、终端、附件或 ACP raw 写入 Access SQLite；正文重放回源 Owner。断线后只自动恢复订阅和安全查询，不自动重放副作用命令。`agent-host` 与 `node-link-client` 是平级 backend，不能互相调用。

### 4.7 `storage-sqlite`

唯一职责：实现持久化端口。

包含 schema、migration、`SessionStore`、`RemoteDeliveryStore`、TrustStore 持久部分、事务、容量清理、快照和 TTL。Owned content tables 与 imported delivery-index tables 必须物理或类型隔离，防止 Access 路径误写正文。

它不解析 ACP、不广播 WebSocket、不执行会话状态转换、不保存明文私钥。数据库 record 与领域对象通过 mapper 转换。

### 4.8 `identity-auth`

唯一职责：实现设备身份、配对、认证和授权。

内部可分：

```text
pairing/
handshake/
authorization/
keystore/
```

它负责 Node/设备 P-256 长期身份、PWA canonical origin 绑定、一次性配对、长度前缀 transcript、P1363 challenge-response、scope、撤销和平台安全存储。Export Policy 的业务交集由 core 执行；本 crate 只把验证后的 `Actor`、credential status 和 grant facts 交给 core。Node Identity 与 Device Identity 必须使用不同 key purpose、record type 和签名 domain。

### 4.9 `server`

唯一职责：承载所有入站协议 adapter，类似 Pi server 对连接、attachment 和应用服务路由的集中承载，但不把各协议合并成一个 wire format。

内部平级模块：

```text
server::sync         HTTP/WSS、设备认证、snapshot/event/ACK
server::node_link    节点认证、Export catalog、resource/command/ACK
server::acp_facade   ACP stdio facade，供 Zed/IDE 使用
server::transport    listener 与连接级 backpressure；不放业务命令
```

三个 adapter 只能调用 `core::use_cases`，不能互相调用、查询 SQLite、启动 Agent 或直接调用 `node-link-client`。每个 adapter 自己拥有 wire/core mapper；共享的只有通用连接生命周期原语，禁止抽出“万能消息 DTO”。

Node Link 和 Sync attachment 必须具有 connection generation 或 attachment ID。重新认证/重新订阅会生成新 generation，延迟到达的旧连接 frame 必须被拒绝，不能误投递到新会话绑定。

### 4.10 `app`

唯一职责：发布 `acp-remote` 可执行程序并作为组合根。它装配 daemon、CLI、server、backend、配置、后台任务、单实例锁、健康状态和 graceful shutdown，但不得承载业务规则。

CLI 子命令：

```text
daemon start|stop|status
session create|list
device pair|list|revoke
node pair|list|revoke
export create|list|revoke
import add|list|remove
acp-stdio
doctor
```

CLI 通过 core use case 或受认证的本地管理 transport 工作，不能复制 core 业务规则。`app` 是唯一允许依赖所有具体 crate 的位置。

## 5. 依赖矩阵

`✓` 表示允许直接依赖：

| From / To | core | acp-protocol | sync-protocol | node-link-protocol | identity-auth |
|---|---:|---:|---:|---:|---:|
| core | — |  |  |  |  |
| acp-protocol |  | — |  |  |  |
| sync-protocol |  |  | — |  |  |
| node-link-protocol |  |  |  | — |  |
| agent-host | ✓ | ✓ |  |  |  |
| node-link-client | ✓ | ✓ |  | ✓ |  |
| storage-sqlite | ✓ |  |  |  |  |
| identity-auth | ✓ |  | ✓ | ✓ | — |
| server | ✓ | ✓ | ✓ | ✓ | ✓ |
| app | ✓ | ✓ | ✓ | ✓ | ✓ |

额外规则：

- 三个 protocol crate 彼此也不直接依赖；包含 ACP raw 的 Node Link 字段只是受约束 bytes/string，不通过 Rust 类型依赖 ACP DTO。
- `server` 对 `identity-auth` 的依赖只用于完成连接认证；业务授权仍由 core 对 `Actor + grant facts` 执行。
- `app` 可以依赖全部具体 crate，但只做装配；任何其他 crate 不得依赖 `app`。

## 6. 组合根

依赖注入只发生在 `app`：

```rust
let store = SqliteStore::open(config.database).await?;
let auth = IdentityAuth::new(platform_keystore, store.identities());
let local = ProcessAgentBackend::new(config.agents);
let remote = NodeLinkBackend::new(config.imports, auth.node_identity());
let backends = RoutedSessionBackend::new(local, remote);

let core = Core::new(CoreDeps {
    sessions: store.session_store(),
    remote_deliveries: store.remote_delivery_store(),
    backends,
    publisher: EventHub::new(),
    clock: SystemClock,
    ids: SecureIds,
});

let server = Server::new(ServerDeps {
    use_cases: core.use_cases(),
    auth,
    sync: config.sync,
    node_link: config.node_link,
});
```

示例只表达装配关系，不锁定最终构造 API。

## 7. 命令与事件路径

命令路径：

```text
server::sync / server::node_link / server::acp_facade / app::cli
-> core::use_cases
-> SessionBackendFactory / SessionEndpoint
-> agent-host | node-link-client
-> ACP Agent
```

Owned session 事件路径：

```text
ACP Agent
-> agent-host mapper（公共 view + ACP raw）
-> core::broker
-> SessionStore::commit（state + event + dedupe 同一事务）
-> EventPublisher publish
-> subscribers
```

Imported session 事件路径：

```text
Owner 已提交事件
-> node-link-client mapper
-> core::broker
-> RemoteDeliveryStore::commit_receipt（无正文索引）
-> EventPublisher publish（正文仅内存）
-> local subscribers
```

Owned 事件只有 core 可以决定何时提交。Imported 事件的业务提交权属于 Owner，Access core 只能提交交付收据，不能把它写成第二份权威 `SessionStore` 内容。

## 8. 错误边界

```text
core::model       DomainError
core::use_cases   UseCaseError / PortError
acp-protocol      AcpCodecError
sync-protocol     SyncCodecError
node-link-protocol NodeLinkCodecError
agent-host        AgentHostError -> PortError
node-link-client  NodeLinkClientError -> PortError
storage-sqlite    StorageError -> PortError
server::sync      TransportError / HTTP mapping
server::node_link NodeLinkTransportError / wire mapping
server::acp_facade AcpFacadeError / JSON-RPC mapping
```

- 外部错误在适配器边界映射。
- `sqlx::Error` 不得进入 core。
- JSON-RPC error code 不得成为领域错误。
- 用户稳定错误码与内部诊断链分离。
- 日志可以保留 source chain，但不能泄漏密钥或完整消息。

## 9. 扩展策略

### 新增 Agent

如果 Agent 正确实现 ACP，只需增加启动 profile、环境变量白名单和 contract tests，不修改 core。仅在协议存在真实差异时增加专属 mapper/quirk。

### 新增客户端

在 `server` 增加入站模块并复用 `core::use_cases`。除非出现新的业务用例，否则不修改 core。

### 新增存储

实现 `SessionStore`、`RemoteDeliveryStore` 等既有事务端口。内存实现服务于测试，其他本地数据库不改变 core。

### 暂不设计动态插件 ABI

第一阶段采用编译期 trait 和静态 registry。只有出现第三方独立发布适配器的明确需求后，再评估进程插件、WASI 或稳定 RPC 插件协议。

## 10. 防止模块腐化

1. 禁止 `common` 或万能 `utils`；工具函数放在拥有其语义的模块。
2. 禁止跨适配器调用，例如 `server::sync -> storage-sqlite` 或 `server::node_link -> node-link-client`。
3. ACP DTO 只存在于 ACP 边界，Sync DTO 只存在于同步边界，DB record 只存在于 SQLite 适配器。
4. 一个状态只有一个权威写入者：Session 属于 Owner core，进程属于 AgentHost，连接属于对应 server/client adapter，节点/设备信任属于 IdentityAuth。
5. 仅当需要阻止反向依赖、多入口复用、独立协议、平台实现或独立测试时才拆新 crate。
6. 文件数量增加本身不是拆 crate 的理由。

### 10.1 ACP 兼容性边界

项目级不变量是：

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

模块实现必须满足：

1. `acp-protocol` 对未知字段和扩展 payload 往返保真。
2. `core::broker` 只使用参与业务决策的公共视图，不把公共视图当作完整 ACP 消息。
3. `agent-host` 不擅自改变 Agent 消息的结构化语义。
4. `server::acp_facade` 如实协商端到端能力，不虚报支持。
5. `sync-protocol` 可以提供移动端友好的表示，但必须能表达结构化能力、明确降级和不支持状态。
6. 任一边界无法处理某项能力时必须显式失败，不能丢弃、曲解或静默转成文本。
7. `node-link-client` 与 `server::node_link` 跨节点时必须保留 ACP raw payload、origin identity 和 capability gate；Access Node facade 只能宣告完整链路交集。

## 11. 测试边界

| 模块 | 测试重点 |
|---|---|
| core | 状态转换、值对象、每会话串行、幂等、事务提交、owned/imported 分流和 fake ports |
| acp-protocol | 官方 fixture、未知字段往返保真、扩展 payload、版本兼容 |
| sync-protocol | 编解码、协商、恶意输入 |
| node-link-protocol | 节点握手、catalog、origin cursor、命令幂等、版本协商 |
| agent-host | fake ACP child、超时、崩溃、乱序响应 |
| node-link-client | fake Owner、attachment generation、显式重连、origin 去重、capability 收缩、uncertain |
| storage-sqlite | migration、owned 原子提交、imported 无正文约束、TTL、容量限制 |
| identity-auth | 设备/节点配对过期、重放、无传递信任、grant 交集和撤销 |
| server::sync | auth、backpressure、续传、限流 |
| server::node_link | Export 过滤、节点 auth、attachment、ACK、backpressure、撤销 |
| server::acp_facade | ACP contract、能力协商真实性、扩展透传、外部 turn 重放 |
| app | 组合冒烟、关闭顺序、单实例 |

端到端测试使用可控的 fake ACP Agent。真实 Codex/OMP 测试作为可选兼容性套件，不作为普通 CI 的硬依赖。各模块测试使用机器矩阵中的 row/test ID 建立证据，矩阵结构先由 `node scripts/check-acp-compatibility.mjs` 检查。

## 12. 保持开放的决策

- `identity-auth` 是否拆成纯状态机与平台 keystore 两个 crate。
- CLI 使用 local socket、named pipe，还是受限 HTTP API 管理 Daemon。
- 是否为同步协议生成 TypeScript/Kotlin/Swift 类型。
- 是否公开部分 crate 到 crates.io；第一阶段可全部保持 workspace-private。

标准不是目录是否整齐，而是边界能否降低耦合、支持独立测试并控制变化传播。

## 13. 架构图

- 可交互 HTML：[acp-remote-modules.html](./diagrams/acp-remote-modules.html)
- 图源 JSON：[acp-remote-modules.architecture.json](./diagrams/acp-remote-modules.architecture.json)

## 14. 参考

- Pi 当前将 session engine、wire protocol、client、server 和 SQLite backend 分开，并保持 protocol transport-neutral：<https://github.com/earendil-works/pi/tree/main/packages>
- Pi protocol/client/server 对 routed envelope、logical server identity、session attachment 和 transport abstraction 的说明：<https://github.com/earendil-works/pi/tree/main/packages/protocol>
- Oh My Pi 将 AI、Agent Core、Coding Agent、TUI 和 native 能力拆分，并从 interactive、RPC、SDK、ACP 等入口复用同一引擎：<https://github.com/dankalish/oh-my-pi>
