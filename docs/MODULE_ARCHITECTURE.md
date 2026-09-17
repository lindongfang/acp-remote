# ACP Remote 模块架构

> 状态：编码前模块边界设计  
> 版本：0.1  
> 日期：2026-09-17  
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)

## 1. 设计理念

本项目参考 Pi/Oh My Pi 的分层理念，但不机械复制其目录结构：

- 保持核心最小，只保留不可替代的业务规则。
- 同一个核心支持 CLI、手机和 Zed 等多个入口。
- 协议、业务核心、基础设施和 UI 分离。
- 每个模块只有一个主要变化原因。
- 高级行为优先通过适配器和配置扩展。

Pi 将模型访问、Agent 循环、Coding Agent 应用和 TUI 分成不同包，同一引擎可以从交互终端、RPC、SDK 和 ACP 等入口使用。ACP Remote 采用相同思想：一个 Broker 核心，多种入站方式，多种可替换的基础设施实现。

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
Clients
   │
   ▼
Inbound Adapters ──> Wire Protocols
   │
   ▼
Application Ports <── Broker Core
   ▲                     │
   │                     ▼
Outbound Adapters      Domain
   │
   ▼
ACP Agents / SQLite / Platform Security
```

依赖只允许指向更稳定的模块：

```text
composition -> adapters -> ports -> domain
composition -> broker   -> ports -> domain
adapters    -> wire protocols
```

禁止：

```text
domain -> broker
broker -> axum/sqlx/tokio::process/Noise implementation
storage -> agent-host
sync-server -> storage-sqlite
agent-host -> sync-server
adapter A -> adapter B
```

适配器之间只能通过核心定义的端口协作。

### 2.1 客户端边界

客户端不属于 Rust Broker 的内部依赖图，通过版本化 `sync-protocol` 与 `sync-server` 通信。wire contract 以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 为准；系统安全边界以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为准；前端阶段、分层、PWA 限制和后续原生 adapter 约束以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为准。

```text
PWA / Android / iOS
        ↓ Sync Protocol
sync-server -> application ports -> broker-core
```

- 第一阶段只实现 PWA，后续原生客户端不能绕过 Sync Protocol 直接调用 Broker。
- 客户端可以共享协议类型、状态机和 feature 层，但不能复制服务端业务规则。
- Web/Native 存储、密钥、相机和生命周期差异必须通过客户端平台 adapter 隔离。
- PWA 的安全降级不能反向降低 Daemon 的认证与授权要求。

## 3. Rust Workspace

```text
crates/
├─ domain/
├─ acp-protocol/
├─ sync-protocol/
├─ application-ports/
├─ broker-core/
├─ agent-host/
├─ storage-sqlite/
├─ device-auth/
├─ sync-server/
├─ acp-facade/
├─ daemon/
└─ cli/
```

这些是明确的依赖边界，不代表每个内部概念都必须拆成 crate。只有需要编译期禁止反向依赖、被多个入口复用、拥有独立协议或独立平台实现的部分才提升为 crate。

## 4. 模块职责

### 4.1 `domain`

唯一职责：定义与传输和持久化无关的领域语言。

包含：

```text
HostId / DeviceId / SessionId / EventId / RequestId
Session / SessionState / TurnState
Command / CommandResult
Event / EventKind / EventOrigin
ModelRef / AgentRef / Capability
PermissionRequest / PermissionDecision
Sequence / Version
```

规则：

- 不依赖 Tokio、Axum、SQLite、WebSocket、子进程和具体 Agent。
- 领域类型不包含 HTTP 状态码、数据库列名或 JSON-RPC method 名。
- ID 使用 newtype，不在核心到处传裸 `String`。
- 状态转换由领域方法验证，适配器不能直接修改状态字段。

### 4.2 `acp-protocol`

唯一职责：描述并编解码 ACP wire protocol。

包含 JSON-RPC envelope、ACP DTO、capability negotiation、领域映射和协议 fixture。协议类型必须保留未知字段与 Agent 扩展 payload，使合法但尚未被 Broker 理解的能力仍可转发或重放；显式 mapper 可以生成公共领域视图，但不得以映射为由丢失原始语义。ACP 覆盖集合与跨层验收合同以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 和 `compatibility/acp/v1/matrix.json` 为准。它不启动子进程、不管理会话、不访问数据库，也不包含手机协议和具体 Agent 业务。

### 4.3 `sync-protocol`

唯一职责：定义电脑与手机之间的版本化 wire protocol。

具体消息、transcript、游标、重放和兼容语义由 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 定义；本模块不得另行发明第二套格式。

包含：

```text
Handshake envelope
Subscribe / Snapshot / Event / Ack
ClientCommand / CommandAccepted / CommandRejected
Pairing DTO
ProtocolVersion / Feature negotiation
```

它不允许客户端发送任意 ACP JSON-RPC；wire DTO 与领域类型通过显式 mapper 转换。

### 4.4 `application-ports`

唯一职责：定义跨层稳定接口，分为：

```text
inbound    外部调用系统的用例
outbound   Broker 完成用例所需的能力
```

建议入站端口：

```text
SessionUseCases
ModelUseCases
PermissionUseCases
SubscriptionUseCases
DeviceManagementUseCases
```

建议出站端口：

```text
AgentRuntime       创建、恢复、prompt、cancel、set model
SessionRepository  会话元数据和快照
EventJournal       追加与读取有序事件
CommandDeduper     requestId 幂等记录
DeviceRepository   配对设备和权限
EventPublisher     通知实时订阅者
Clock              可测试时间
IdGenerator        可测试 ID
```

端口签名只能使用领域类型或专门的应用输入/输出类型，不能泄漏 `sqlx::Error`、`axum::Response` 或 ACP DTO。端口按能力拆分，避免巨型 `AppService`。

### 4.5 `broker-core`

唯一职责：实现核心业务规则。

包含 Session Actor、每会话串行队列、active turn 约束、prompt 排队、模型切换时机、授权调用点、命令幂等、Agent 事件公共视图、先持久化后发布、permission first-writer-wins 和快照生成。公共视图用于业务决策，不能替代或截断需要透传、持久化或重放的 ACP 原始语义。

它只能依赖：

```text
domain
application-ports
```

它不能依赖网络、SQLite、子进程、Noise、Codex、OMP 或 Tailscale。测试使用内存 fake ports，不启动真实基础设施。

### 4.6 `agent-host`

唯一职责：实现 `AgentRuntime` 出站端口。

包含：

- 启动和监督 ACP Agent 子进程。
- stdin/stdout JSON-RPC transport。
- request ID 与原生 session ID 映射。
- capability negotiation。
- stderr 日志、超时、取消和进程树清理。
- 通用 Agent profile registry。
- 隔离的兼容性 quirk。

优先使用一个通用 ACP 实现。Codex、OMP 的差异优先表示为 capability/profile 数据，只有无法数据化的差异才建立专属 module。

### 4.7 `storage-sqlite`

唯一职责：实现持久化端口。

包含 schema、migration、SessionRepository、EventJournal、CommandDeduper、设备非秘密元数据、事务、容量清理、快照和 TTL。

它不解析 ACP、不广播 WebSocket、不执行会话状态转换、不保存明文私钥。数据库 record 与领域对象通过 mapper 转换。

### 4.8 `device-auth`

唯一职责：实现设备身份、配对、认证和授权。

内部可分：

```text
pairing/
handshake/
authorization/
keystore/
```

它负责 Host/设备 P-256 长期身份、PWA canonical origin 绑定、一次性配对、长度前缀 transcript、P1363 challenge-response、设备权限、撤销和平台安全存储。TLS/WSS 负责默认 Profile 的临时连接密钥与传输保护；上层只看到认证后的 `Actor` 和授权结果，看不到原始私钥或具体密码学类型。Noise 只作为未来可选 Transport Profile，不进入第一阶段。

### 4.9 `sync-server`

唯一职责：把 HTTP/WebSocket 请求适配为应用用例。

包含连接生命周期、`sync-protocol` 编解码、认证接入、subscribe/snapshot/event/ACK、输入限制、rate limit 和 backpressure。

它不能直接查询 SQLite、启动 Agent 或自行决定设备权限。

### 4.10 `acp-facade`

唯一职责：把应用用例暴露为可选的 ACP stdio Agent，供 Zed 和其他 ACP Client 使用。

它与 `sync-server` 是平级入站适配器；两者不能互相调用，只调用相同的入站端口。

它必须如实执行 capability negotiation：只声明端到端实际可用的能力；对可保留语义的扩展进行透传；无法安全提供的能力返回明确的不支持结果，不得静默降级或伪装为普通文本。

### 4.11 `daemon`

唯一职责：组合并运行完整进程。

它负责配置、连接池、具体适配器装配、后台任务、单实例锁、健康状态和 graceful shutdown。Daemon 是唯一允许依赖所有具体实现的 crate，但不得承载业务规则。

### 4.12 `cli`

唯一职责：参数解析、输出格式化和调用应用接口。

```text
daemon start|stop|status
session create|list
device pair|list|revoke
acp-stdio
doctor
```

CLI 不能复制 Broker 业务规则。

## 5. 依赖矩阵

`✓` 表示允许直接依赖：

| From / To | domain | acp-protocol | sync-protocol | ports | broker | agent-host | sqlite | auth |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| domain | — |  |  |  |  |  |  |  |
| acp-protocol | ✓ | — |  |  |  |  |  |  |
| sync-protocol | ✓ |  | — |  |  |  |  |  |
| application-ports | ✓ |  |  | — |  |  |  |  |
| broker-core | ✓ |  |  | ✓ | — |  |  |  |
| agent-host | ✓ | ✓ |  | ✓ |  | — |  |  |
| storage-sqlite | ✓ |  |  | ✓ |  |  | — |  |
| device-auth | ✓ |  | ✓ | ✓ |  |  |  | — |
| sync-server | ✓ |  | ✓ | ✓ |  |  |  | ✓ |
| acp-facade | ✓ | ✓ |  | ✓ |  |  |  |  |
| daemon | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

`sync-server -> device-auth` 可在接口稳定后通过认证端口消除；早期允许直接依赖，但不得向外泄漏具体加密类型。

## 6. 组合根

依赖注入只发生在 `daemon`：

```rust
let store = SqliteStore::open(config.database).await?;
let auth = DeviceAuth::new(platform_keystore, store.devices());
let agents = ProcessAgentHost::new(config.agents);

let broker = Broker::new(BrokerDeps {
    sessions: store.sessions(),
    events: store.events(),
    deduper: store.commands(),
    agents,
    publisher: EventHub::new(),
    clock: SystemClock,
    ids: SecureIds,
});

let sync = SyncServer::new(broker.clone(), auth, config.network);
```

示例只表达装配关系，不锁定最终构造 API。

## 7. 命令与事件路径

命令路径：

```text
sync-server / acp-facade / cli
-> inbound port
-> broker-core
-> AgentRuntime
-> agent-host
-> ACP Agent
```

事件路径：

```text
ACP Agent
-> agent-host
-> normalized domain event
-> broker-core
-> EventJournal append
-> EventPublisher publish
-> subscribers
```

只有 Broker 可以决定事件何时成为已提交事件；AgentHost 不得绕过 Broker 直接广播。

## 8. 错误边界

```text
domain            DomainError
application       UseCaseError / PortError
acp-protocol      AcpCodecError
sync-protocol     SyncCodecError
agent-host        AgentHostError -> PortError
storage-sqlite    StorageError -> PortError
sync-server       TransportError / HTTP mapping
```

- 外部错误在适配器边界映射。
- `sqlx::Error` 不得进入 Broker。
- JSON-RPC error code 不得成为领域错误。
- 用户稳定错误码与内部诊断链分离。
- 日志可以保留 source chain，但不能泄漏密钥或完整消息。

## 9. 扩展策略

### 新增 Agent

如果 Agent 正确实现 ACP，只需增加启动 profile、环境变量白名单和 contract tests，不修改 Broker。仅在协议存在真实差异时增加专属 mapper/quirk。

### 新增客户端

新增入站适配器并复用 `application-ports`。除非出现新的业务用例，否则不修改 Broker。

### 新增存储

实现既有 repository/journal 端口。内存实现服务于测试，其他本地数据库不改变 Broker。

### 暂不设计动态插件 ABI

第一阶段采用编译期 trait 和静态 registry。只有出现第三方独立发布适配器的明确需求后，再评估进程插件、WASI 或稳定 RPC 插件协议。

## 10. 防止模块腐化

1. 禁止 `common` 或万能 `utils`；工具函数放在拥有其语义的模块。
2. 禁止跨适配器调用，例如 `sync-server -> storage-sqlite`。
3. ACP DTO 只存在于 ACP 边界，Sync DTO 只存在于同步边界，DB record 只存在于 SQLite 适配器。
4. 一个状态只有一个权威写入者：Session 属于 Broker，进程属于 AgentHost，连接属于 SyncServer，设备信任属于 DeviceAuth。
5. 仅当需要阻止反向依赖、多入口复用、独立协议、平台实现或独立测试时才拆新 crate。
6. 文件数量增加本身不是拆 crate 的理由。

### 10.1 ACP 兼容性边界

项目级不变量是：

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

模块实现必须满足：

1. `acp-protocol` 对未知字段和扩展 payload 往返保真。
2. `broker-core` 只规范化参与业务决策的公共视图，不把公共视图当作完整 ACP 消息。
3. `agent-host` 不擅自改变 Agent 消息的结构化语义。
4. `acp-facade` 如实协商端到端能力，不虚报支持。
5. `sync-protocol` 可以提供移动端友好的表示，但必须能表达结构化能力、明确降级和不支持状态。
6. 任一边界无法处理某项能力时必须显式失败，不能丢弃、曲解或静默转成文本。

## 11. 测试边界

| 模块 | 测试重点 |
|---|---|
| domain | 状态转换、值对象、不变量 |
| acp-protocol | 官方 fixture、未知字段往返保真、扩展 payload、版本兼容 |
| sync-protocol | 编解码、协商、恶意输入 |
| broker-core | fake ports、并发、幂等、先存后发、公共视图不损失原始语义 |
| agent-host | fake ACP child、超时、崩溃、乱序响应 |
| storage-sqlite | migration、事务、TTL、容量限制 |
| device-auth | 配对过期、重放、撤销、错误密钥 |
| sync-server | auth、backpressure、续传、限流 |
| acp-facade | ACP contract、能力协商真实性、扩展透传、外部 turn 重放 |
| daemon | 组合冒烟、关闭顺序、单实例 |

端到端测试使用可控的 fake ACP Agent。真实 Codex/OMP 测试作为可选兼容性套件，不作为普通 CI 的硬依赖。各模块测试使用机器矩阵中的 row/test ID 建立证据，矩阵结构先由 `node scripts/check-acp-compatibility.mjs` 检查。

## 12. 保持开放的决策

- `application-ports` 独立成 crate，还是先作为 `broker-core::ports` module。
- `device-auth` 是否拆成纯状态机与平台 keystore 两个 crate。
- CLI 使用 local socket、named pipe，还是受限 HTTP API 管理 Daemon。
- 是否为同步协议生成 TypeScript/Kotlin/Swift 类型。
- 是否公开部分 crate 到 crates.io；第一阶段可全部保持 workspace-private。

标准不是目录是否整齐，而是边界能否降低耦合、支持独立测试并控制变化传播。

## 13. 架构图

- 可交互 HTML：[acp-remote-modules.html](./diagrams/acp-remote-modules.html)
- 图源 JSON：[acp-remote-modules.architecture.json](./diagrams/acp-remote-modules.architecture.json)

## 14. 参考

- Pi 保持核心最小，通过 extensions、skills、prompts 和 packages 扩展：<https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent>
- Oh My Pi 将 AI、Agent Core、Coding Agent、TUI 和 native 能力拆分，并从 interactive、RPC、SDK、ACP 等入口复用同一引擎：<https://github.com/dankalish/oh-my-pi>
