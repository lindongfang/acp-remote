# ACP Remote 模块架构

> 状态：模块边界已冻结并开始落地（`acpr-transcript`/`acpr-wire`/`sync-protocol`/`node-link-protocol`/`core`/`storage-sqlite` 已实现，见 `README.md` 的 crate 表）
> 版本：0.3
> 修订记录（2026-09-18）：补全本地管理通道的权威文档指向；`fixtures/acp/v1` 的校验口径改为与实现一致（快照 vendored 前只做存在性与解析检查）；§5 依赖矩阵放开 `storage-sqlite → acpr-wire`（`payload_digest` 的 ACPR-CJ1 只能有一份实现），§4.13 补 ACPR-CJ1。  
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
- **模块之间尽可能独立，协作靠依赖传递**：一个模块需要另一模块的能力时，通过端口或函数签名把它传进来（组合根负责装配），不通过横向调用或"顺手 import 隔壁"实现；跨模块共享的底层实现（例如 Sync 与 Node Link 共用的 transcript codec）下沉为无依赖的叶子 crate，而不是让平级模块互相依赖。

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
        │
        └──────────────> identity-keystore（平台安全存储，仅 app 装配）
```

依赖只允许指向更稳定的模块：

```text
app             -> server / backends / identity-auth / identity-keystore / core
server          -> core + corresponding wire protocols
outbound backend-> core + corresponding wire protocols
core::use_cases -> core::ports + core::model
wire protocols  -> no core or infrastructure dependency
identity-keystore -> identity-auth（实现其 keystore 端口）
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

平级模块共享的底层实现放在叶子 crate：`acpr-transcript`（[ADR-0005](./adr/0005-shared-transcript-codec.md)）只拥有长度前缀 transcript 的编码与解码，不拥有任何 domain/tag 取值或协议语义；`acpr-wire`（[ADR-0007](./adr/0007-shared-wire-value-crate.md)）只拥有跨协议共用的 wire 值对象与字段校验机制（uuid/decimalString/timestamp/base64url/featureList/rawAcp、`Nullable`、`RawObject`、`ExtraFields`、泛型 `PublicError`），不拥有任何协议词表。协议 crate 不互相依赖，Node Link 复用 Sync 的 codec 结构与值对象靠的是"共同依赖同一个叶子 crate"，而不是"一个协议 crate 依赖另一个"。

### 2.1 客户端边界

UI 客户端不属于 Rust core，通过版本化 `sync-protocol` 与 `server::sync` 通信。ACP Remote 节点通过独立的 `node-link-protocol` 通信。CLI 与运行中的 Daemon 通过平台本地 IPC 通信，其信封、framing 与方法集以 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) 为准。wire contract 分别以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 和 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准；系统安全边界以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为准；前端阶段、分层、PWA 限制和后续原生 adapter 约束以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为准。

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
├─ acpr-transcript/         叶 crate：Sync 与 Node Link 共用的 transcript codec 与表驱动校验
├─ acpr-wire/               叶 crate：跨协议共用的 wire 值对象与字段校验机制
├─ agent-host/              本地 ACP Agent backend
├─ node-link-client/        远程 Agent backend
├─ storage-sqlite/          core 持久化后端
├─ identity-auth/           身份、配对、签名与授权的纯状态机（不含平台 API）
├─ identity-keystore/       平台安全存储实现（DPAPI/CNG、Keychain、Secret Service）
├─ server/                  sync / node-link / acp-facade / local-admin 入站模块
└─ app/                     daemon、CLI 与组合根
```

这组物理 crate 刻意少于逻辑模块数，接近 Pi 的“core + protocol + client + server + backend + app”划分。`model`、`use_cases`、`ports` 和 `broker` 先作为 `core` 内部模块；`sync`、`node_link` 和 `acp_facade` 先作为 `server` 内部平级模块。只有出现独立发布、编译隔离或明显构建成本后才继续拆 crate。

协议 crate 是例外：ACP、Sync 和 Node Link 分别拥有独立兼容周期与 fixture，必须从第一天物理隔离，且不得依赖 `core`。

`core` 的包名与库名不同：包名是 `core`（与上表、§5 矩阵一致），**库名是 `acp_core`**——一个名为 `core` 的依赖会在依赖方的宏展开里遮蔽 sysroot 的 `core`（`thiserror::Error` 展开出的 `core::fmt` 会解析到本 crate 而编译失败）。依赖方照旧在 `Cargo.toml` 写 `core = { path = "../core" }`，代码里用 `acp_core::…`；本文档其余部分的 `core::model`/`core::use_cases`/`core::ports`/`core::broker` 均指 `acp_core::…`。

平台安全存储是第二处例外：`identity-keystore` 独立成 crate 是为了隔离平台依赖（原生 keystore API 与 `cfg` 分支），让 `identity-auth` 的状态机在所有平台都能编译与单测（[ADR-0006](./adr/0006-identity-keystore-split.md)）。

### 3.1 Workspace 基线

创建 workspace 时固定以下基线，避免各 crate 各自漂移：

- `edition = "2024"`（与 `rust-version = "1.85"` 一致，edition 2024 的最低工具链即 1.85），`resolver = "3"`。
- `[workspace.package]` 统一 `version`、`edition`、`rust-version`、`license`、`repository`；第一阶段全部 crate `publish = false`（`§12` 的"是否公开部分 crate"仍未定）。
- `[workspace.dependencies]` 统一第三方版本（tokio、axum、serde、serde_json、sqlx 或 rusqlite、tracing、thiserror 等）；crate 内只写 `workspace = true`；新增依赖按 `AGENTS.md` §7 先审必要性、维护状态、许可证与平台支持。密码学原语固定为 `p256`（ECDSA P-256）+ `sha2` + `hmac` + `base64`（无填充 base64url）：四者都是纯 Rust、无原生依赖，且已对 `fixtures/*/v1/transcripts/` 的固定向量验证通过（结论与实现约束见 `INITIAL_DESIGN.md` §16 第 6 条）。
- `[workspace.lints]` 默认 `clippy::all = "deny"`，并保持 `AGENTS.md` §8 要求的 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 可直接通过。
- 保持默认 `panic = "unwind"`：`AGENTS.md` §7 要求正常路径无 `unwrap()`/`expect()`，而测试与 `cargo test` 需要 unwind；不通过 `panic = "abort"` 掩盖失败。
- workspace 成员随实现增量增长：每个 crate 真正落地时才加入 `members`，最终为 §3 列出的十二个；不得为凑齐列表创建只有占位实现的空 crate。

## 4. 模块职责

### 4.1 `core`

唯一职责：承载与传输、数据库和具体 Agent 无关的会话业务。参考 Pi `agent-core`，首日把稳定且共同演进的领域、用例、端口和协调逻辑放在一个 crate 内，而不是为了目录整齐拆成三个相互依赖的 crate。

内部模块：

```text
core::model       值对象、状态机、事件与不变量
core::use_cases   Session/Config/Permission/Export/Subscription 用例
core::ports       backend、事务存储、身份仓库、时钟等能力接口
core::broker      Session Actor、命令协调与事件提交
core::testing     fake ports 和契约测试工具，仅测试 feature 导出
```

```text
NodeId / DeviceId / ExportId / ImportId / SessionId / EventId / RequestId / TurnId / InteractionId / PairingId / AttachmentId
OwnedSessionRef / RemoteSessionRef / OriginEventRef / SessionReference
Session / SessionSummary / SessionSnapshot / SessionState / Turn / TurnState / ResourceOrigin
ClientCommand / CommandPayload / CommandReceipt / CommandRecord / CommandTerminalRecord / CommandStatus / CommandKind
InteractionId / InteractionKind / InteractionOption / PermissionDecision / InteractionResolution / PendingInteraction / Resolution
Event / EventType / EventKind / EventOrigin / EventPayload / AcpRaw / PersistencePolicy / StoredPolicy
AgentRef / AgentDescriptor / CapabilitySet / ConfigOptionId / ConfigOption / ConfigValue / ModeRef / ModeId
Sequence / Version / AttachmentGeneration / ServerEpoch / OriginEpoch / GlobalCursor / OriginCursor / LocalCursor / Timestamp / Digest
Actor / DeviceRecord / NodeRecord / PairingRecord / ExportRecord / ImportRecord / AuditRecord / AuditAction
```

以上是**冻结全量**；每个类型的形状与不变量见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3。

规则：

- `core` 不依赖任何 wire protocol、Tokio runtime、Axum、SQLite、WebSocket、子进程和具体 Agent。
- 领域类型不包含 HTTP 状态码、数据库列名、JSON-RPC method 或 Node Link wire discriminator。
- ID 使用 newtype；状态转换由领域方法验证，adapter 不能直接修改状态字段。
- Owned 与 imported session 必须在类型或 `ResourceOrigin` 上可区分，禁止依靠 nullable `ownerNodeId` 猜测持久化规则。
- `core::broker` 实现每会话串行、active turn、授权调用点、幂等、permission first-writer-wins、模型切换时机和先提交后发布。

入站 use case 按能力拆分：

```text
SessionCommands / SessionQueries
ConfigCommands
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
SessionEndpoint          prompt/cancel/set-mode/list-config/set-config/resolve-interaction/read-history/close，绑定单个 live session
SessionStore             原子提交 owned session 状态、事件与 requestId 幂等；提供 head 与一致性读视图
RemoteDeliveryStore      只提交 imported event 的 cursor/digest/local-sequence 索引
TrustStore               设备、节点配对、信任与撤销元数据
ExportStore              Owner Export 与 Access Import 记录
AuditStore               不含内容的审计记录（读写）
AttachmentStore          内容寻址附件字节与 LRU 清理
EventPublisher           发布已经提交的事件或远程交付
EventSink                后端事件通道；调用顺序即提交顺序
ReadView                 一致性读视图，`sync.snapshot_*` 的 barrier
Clock / IdGenerator      可测试时间与 ID（eventId 由存储层在提交事务内分配）
```

签名以 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5 为准。

`SessionStore` 必须提供单一事务提交 API，不能让 Broker 分别调用 `SessionRepository`、`EventJournal`、`CommandDeduper` 后假设三次调用天然原子。`SessionEndpoint` 表示带生命周期的会话句柄；本地与远程 backend 都实现相同接口，但不得把进程、socket 或 wire DTO 暴露给 core。

`read-history` 服务于 Sync 的 `session.read`：owned session 由 `storage-sqlite` 从本地事件日志回答，imported session 必须由 `node-link-client` 在线向 Owner 取，Access 不得把它写进本地正文缓存；Owner 不可达时返回可区分的错误，由 `server::sync` 映射成 `resource.remote_unavailable`。远程可达性变化通过 `EventPublisher` 以 `session.origin.online_changed` 暴露给客户端，不在 core 里维护独立的在线状态缓存。

端口签名、值对象形状、用例入口与 broker 事务顺序已冻结在 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3–§6；本节只保留职责边界与依赖规则。

### 4.2 `acp-protocol`

唯一职责：描述并编解码 ACP wire protocol。

包含 JSON-RPC envelope、ACP wire DTO、codec、raw document、capability wire schema、limits 和协议 fixture。协议类型必须保留未知字段与 Agent 扩展 payload。ACP 覆盖集合与跨层验收合同以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 和 `compatibility/acp/v1/matrix.json` 为准。

它不依赖 `core`，不包含领域 mapper，不启动子进程、不管理会话、不访问数据库。ACP wire 到公共领域视图的 mapper 位于 `agent-host` 或 `server::acp_facade`，因为映射方向取决于 adapter 角色。

### 4.3 `sync-protocol`

唯一职责：定义 UI 客户端与其所连接 ACP Remote Node 之间的版本化 wire protocol。

具体消息、transcript、游标、重放和兼容语义由 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 定义；本模块不得另行发明第二套格式。

包含：

```text
Handshake envelope（信封 + 连接生命周期：认证前后连接字段规则、Phase 校验）
Subscribe / Snapshot / Event / Ack
ClientCommand / CommandAccepted / CommandRejected
Pairing DTO
ProtocolVersion / Feature negotiation
Heartbeat（control.ping / control.pong）
Connection error lifecycle（error body 与 34 个错误码，认证前后两种信封形状）
```

信封的 `body` 在传输层保持原文承载（`RawValue`），家族 body 由调用方显式解析——这条同时适用于已实现的家族与后续增量，`AGENTS.md` §3 的保真要求不允许中间经通用 DTO 往返。

它不允许客户端发送任意 ACP JSON-RPC，也不依赖 `core`。Sync wire 与 core 的 mapper 位于 `server::sync`。

### 4.4 `node-link-protocol`

唯一职责：定义两个 ACP Remote Node 之间的版本化 wire protocol。

包含 Node handshake、Export catalog、resource snapshot/event/ACK、跨节点 command、origin cursor、错误和 feature negotiation。完整语义以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准。

它不能直接复用 Sync DTO 冒充节点协议，也不能把 ACP stdio 透明封装成网络 tunnel。它不依赖 `core`；Node Link wire 与 core 的 mapper 位于 `node-link-client` 和 `server::node_link`。

`node-link-protocol` 同时拥有机器可验证资产 [`schemas/node-link/v1/`](../schemas/node-link/v1/) 与 [`fixtures/node-link/v1/`](../fixtures/node-link/v1/)；wire 变更必须同时更新两者，Rust 实现消费同一 manifest，不能各自复制一套测试样例。

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

第一阶段采用**同一数据库文件、两族表 + 每族专属 Store 类型**的隔离方式：

- 表名以 `owned_*` 与 `imported_*` 前缀区分，两族不共享外键、不共享事务边界之外的写入路径；`imported_*` 族不包含任何正文列（prompt、回复、工具内容、diff、终端、附件、ACP raw）。
- 端口层就是隔离面：`RemoteDeliveryStore` 只暴露 `imported_*` 的读写，Access 侧代码拿不到 `SessionStore`，因此"忘了过滤"在类型上不可表达。
- migration 按族分开维护，允许单独重放或清理 `imported_*` 而不影响 owned 权威事件日志。
- 只有当出现"必须靠操作系统级隔离（不同文件/不同权限）才能满足的威胁模型"时，才拆成两个数据库文件，并按 `AGENTS.md` §10 新增 ADR。

它不解析 ACP、不广播 WebSocket、不执行会话状态转换、不保存明文私钥。数据库 record 与领域对象通过 mapper 转换。

v1 的表结构（`owned_*` / `imported_*`）、PRAGMA、migration、保留与容量策略、崩溃恢复与失败关闭已冻结在 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7–§8。

### 4.8 `identity-auth`

唯一职责：实现设备身份、配对、认证和授权——**纯状态机，不依赖任何平台 API**。

内部可分：

```text
pairing/
handshake/
authorization/
port/          # keystore 端口定义（trait），实现见 identity-keystore
```

它负责 Node/设备 P-256 长期身份、PWA canonical origin 绑定、一次性配对、长度前缀 transcript、P1363 challenge-response、scope、撤销，以及**通过端口**访问平台安全存储（[ADR-0006](./adr/0006-identity-keystore-split.md)）。所有密钥读写都经过组合根注入的端口：本 crate 不直接调用 DPAPI/Keychain/Secret Service，也不为平台差异写 `cfg` 分支。密码学原语固定为 `p256` + `sha2` + `hmac`（纯 Rust、无原生依赖）；65 字节公钥前置校验、禁用 DER、接受 high-S 等实测约束见 `INITIAL_DESIGN.md` §16 第 6 条。Export Policy 的业务交集由 core 执行；本 crate 只把验证后的 `Actor`、credential status 和 grant facts 交给 core。Node Identity 与 Device Identity 必须使用不同 key purpose、record type 和签名 domain。

`pack.*`、`preset.*`、`grant.*` 只是授权管理的输入形式：由 `authorization/` 按 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 展开成命令级 scope 后才写入设备记录或随 wire 下发（`SECURITY_DESIGN.md` §10.2）。core 只看到展开后的 scope 与 grant facts，不认识 pack/preset 名称。

### 4.9 `server`

唯一职责：承载所有入站协议 adapter，类似 Pi server 对连接、attachment 和应用服务路由的集中承载，但不把各协议合并成一个 wire format。

内部平级模块：

```text
server::sync         HTTP/WSS、设备认证、snapshot/event/ACK
server::node_link    节点认证、Export catalog、resource/command/ACK
server::acp_facade   ACP stdio facade 的 daemon 侧：为本地 Zed/IDE 提供 ACP 会话
server::local_admin  平台本地 IPC（Named Pipe / Unix socket）上的管理请求/响应
server::transport    listener 与连接级 backpressure；不放业务命令
```

四个 adapter 只能调用 `core::use_cases`，不能互相调用、查询 SQLite、启动 Agent 或直接调用 `node-link-client`。每个 adapter 自己拥有 wire/core mapper；共享的只有通用连接生命周期原语，禁止抽出“万能消息 DTO”。

`server::acp_facade` 常驻 daemon：ACP 会话状态、幂等记录与事件提交都必须落在拥有该会话的进程里，因此 `acp-remote acp-stdio` 只是“stdin/stdout ↔ 本地通道”的字节泵，不内嵌 core、storage 或 agent-host（否则会与 daemon 争用同一 SQLite，违反单实例锁与单一权威写入者）。本地通道因此承载两类载荷：`server::local_admin` 的管理请求/响应，以及 `server::acp_facade` 的长期双向 ACP 流；两者各自的编码由本地通道适配器拥有，不复用 Sync 与 Node Link 的 DTO。该通道的 endpoint、访问控制、framing、管理信封、方法集与本地错误码以 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) 为唯一权威来源。daemon 未运行时 `acp-stdio` 必须以明确错误退出，不得自行打开数据库或启动第二套核心。

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

### 4.11 `acpr-transcript`

唯一职责：实现 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §6.2 定义的签名/HMAC 输入编码——magic `ACPR`、`codecVersion`、`domainTag`、严格递增且唯一的 `fieldTag`、长度前缀字段——以及对应的解码与校验错误。`table` 模块在此之上提供表驱动层：泛型表类型（`DomainSpec`/`FieldSpec`/`FieldType`/`Proof`）与"按表校验并编解码"（字段数量、tag 成员、定长宽度）。它是叶 crate（[ADR-0005](./adr/0005-shared-transcript-codec.md)）：不依赖任何其他项目 crate。

它**不**包含任何 `domainTag` 取值、`fieldTag` 取值或协议语义：

- Sync 的 domain 与 tag 表在 `sync-protocol`（对应 `SYNC_PROTOCOL.md` §6.3），Node Link 的在 `node-link-protocol`（对应 §9.2–§9.4，tag 编号与 Sync 独立）；协议 crate 只导出 `DOMAINS` 常量，不重复实现校验或编解码；
- `identity-auth` 依赖本 crate 与两个协议 crate，取表后调用表驱动编解码，自己只负责验证 P-256/HMAC 结果。

`sync-protocol` 与 `node-link-protocol` 正常依赖它，宽度表与"按表校验"因此只有一份实现；协议 crate 之间仍然互不依赖。

### 4.12 `identity-keystore`

唯一职责：实现 `identity-auth` 定义的 keystore 端口，把长期密钥与凭据落到平台安全存储。

- Windows：优先 CNG/TPM，至少 DPAPI 绑定当前用户。
- macOS：Keychain；可用时使用不可导出或硬件保护能力。
- Linux：Secret Service（D-Bus）。没有可用的 Secret Service 时，按 `SECURITY_DESIGN.md` §20 的当前决定失败关闭，不得静默降级为明文文件。

约束：不实现业务逻辑、不解析协议、不做授权判定；端口与错误类型由 `identity-auth` 拥有；任何密钥字节不得进入日志、协议错误或 `Debug` 输出。除 `app` 外没有其他 crate 依赖它（[ADR-0006](./adr/0006-identity-keystore-split.md)）。

### 4.13 `acpr-wire`

唯一职责：跨协议共用的 wire 值对象与字段级校验机制（[ADR-0007](./adr/0007-shared-wire-value-crate.md)）——`Uuid`、`DecimalString`、`Timestamp`、`Base64Url<N>`、`FeatureId`、`FeatureList`、`RawAcp`、`Text<N>`、`NonEmptyText<N>`、`BoundedU64<MIN,MAX>`/`UIntAtLeast<MIN>`、`Nullable<T>`（`required` 且可 null 的键存在性语义）、`RawObject`/`ExtraFields`（开放扩展点的保真载体）、`ValueError`、`deserialize_optional_non_null`、**ACPR-CJ1 规范 JSON**（`SYNC_PROTOCOL.md` §3.3：对象成员按 UTF-16 code unit 排序、禁止重复键与浮点、控制字符写作小写 `\u00xx`；`payloadDigest`/`snapshotDigest` 的前像，Rust 侧唯一实现，与 `scripts/check-contract-assets.mjs` 的参考实现互校），以及泛型 `PublicError<Code>`。

它**不**包含任何协议词表或语义：归属检查（哪些语义属于谁）如下——

- 错误码词表（`ErrorCode`）、命令名、grant、domain/tag 取值、会话状态机规则都不在此 crate；`PublicError<Code>` 只提供形状，各协议以自己的错误码枚举具体化；
- 协议专属的值对象留在各自 crate（Sync 的 `cursor`/`sessionSummary`/`originBlock`，Node Link 的 `originCursor`/`sessionMeta`/`exportEntry` 等），不因"看着像"而下沉；
- `Base64Url` 复用 `acpr-transcript` 的规范化 base64url 解码，因此依赖方向是 `acpr-wire → acpr-transcript`，仍是叶 crate 链上的单向依赖。

`sync-protocol` 与 `node-link-protocol` 正常依赖它，字段级校验与开放对象保真因此只有一份实现；协议 crate 之间仍然互不依赖。

## 5. 依赖矩阵
`✓` 表示允许直接依赖：

| From / To | core | acp-protocol | sync-protocol | node-link-protocol | acpr-transcript | acpr-wire | identity-auth | identity-keystore |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| core | — |  |  |  |  |  |  |  |
| acp-protocol |  | — |  |  |  |  |  |  |
| sync-protocol |  |  | — |  | ✓ | ✓ |  |  |
| node-link-protocol |  |  |  | — | ✓ | ✓ |  |  |
| acpr-transcript |  |  |  |  | — |  |  |  |
| acpr-wire |  |  |  |  | ✓ | — |  |  |
| agent-host | ✓ | ✓ |  |  |  |  |  |  |
| node-link-client | ✓ | ✓ |  | ✓ |  |  |  |  |
| storage-sqlite | ✓ |  |  |  |  | ✓ |  |  |
| identity-auth | ✓ |  | ✓ | ✓ | ✓ |  | — |  |
| identity-keystore |  |  |  |  |  |  | ✓ | — |
| server | ✓ | ✓ | ✓ | ✓ |  |  | ✓ |  |
| app | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

额外规则：

- 三个 protocol crate 彼此也不直接依赖；它们共享的 transcript codec 结构与表驱动校验来自叶子 crate `acpr-transcript`（协议 crate 只导出自己的 `DOMAINS` 表并调用它），跨协议共用的 wire 值对象与字段校验机制来自叶子 crate `acpr-wire`（`docs/adr/0007-shared-wire-value-crate.md`），包含 ACP raw 的 Node Link 字段只是受约束 bytes/string，不通过 Rust 类型依赖 ACP DTO。
- `storage-sqlite` 依赖 `acpr-wire` **只为** §7.3/§9.9 要求的 `payload_digest = base64url(SHA-256(ACPR-CJ1(payload_json)))`：ACPR-CJ1 是跨 Sync/Node Link 的共享机制，只能有一份实现（v1 早期因为存储层手写摘要而与协议侧口径不一致）；除该函数外不得使用 `acpr-wire` 的业务类型，也不得经它访问协议语义。
- `identity-auth` 依赖两个协议 crate 与 `acpr-transcript` 仅用于 transcript 编解码与 domain/字段 tag 定义：它不复制这些常量，也不使用协议 crate 的业务类型或业务规则。
- `server` 对 `identity-auth` 的依赖只用于完成连接认证；业务授权仍由 core 对 `Actor + grant facts` 执行。
- `app` 可以依赖全部具体 crate，但只做装配；任何其他 crate 不得依赖 `app`。
- `identity-auth` **不得**依赖 `identity-keystore`：keystore 端口由 `identity-auth` 定义、由组合根注入实现；反向依赖会让纯状态机重新绑上平台 API，这正是本次拆分要消除的东西。
- 除 `app` 外没有 crate 依赖 `identity-keystore`。需要密钥存储的模块（例如本地管理入口写入 Provider 凭据）通过 `identity-auth` 的端口表达，不直接 import 平台实现。

## 6. 组合根

依赖注入只发生在 `app`：

```rust
let store = SqliteStore::open(config.database).await?;
let keystore = PlatformKeystore::open(&config.identity)?;
let auth = IdentityAuth::new(keystore, store.identities());
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
server::sync / server::node_link / server::acp_facade / server::local_admin / app::cli
-> core::use_cases
-> SessionBackendFactory / SessionEndpoint
-> agent-host | node-link-client
-> ACP Agent
```

`server::local_admin` 与 `app::cli` 只出现在 `local.*` 能力与配对/Export 管理这些用例上；`app::cli` 的 `acp-stdio` 子命令本身不承载业务规则，它把 stdin/stdout 转给 `server::acp_facade`。

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

从 cursor 补发 imported 事件时不得伪造历史 view：Access 没有持久化正文，必须按 origin cursor 向 Owner 重新获取对应事件（`resource.subscribe` 的增量重放）再交付；无法回源的区间只能以 `sync.reset_required` 让客户端重建会话视图，不允许发送"只有 digest、没有内容"的伪事件。

Owned 事件只有 core 可以决定何时提交。Imported 事件的业务提交权属于 Owner，Access core 只能提交交付收据，不能把它写成第二份权威 `SessionStore` 内容。

## 8. 错误边界

```text
core::model       DomainError
core::use_cases   UseCaseError / PortError
acp-protocol      AcpCodecError
sync-protocol     EnvelopeError（信封与阶段）+ ValueError（body 字段级）
node-link-protocol 表驱动编解码错误复用 acpr-transcript::table::TableError
acpr-transcript   TranscriptError（codec）+ table::TableError（按表校验）
acpr-wire         ValueError（字段级校验；各协议私有变体不出此 crate）
agent-host        AgentHostError -> PortError
node-link-client  NodeLinkClientError -> PortError
storage-sqlite    StorageError -> PortError
server::sync      TransportError / HTTP mapping
server::node_link NodeLinkTransportError / wire mapping
server::acp_facade AcpFacadeError / JSON-RPC mapping
server::local_admin LocalAdminError / 本地通道编码与权限错误
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
| sync-protocol | 表与 registry 逐项相等、六个 domain 的固定向量逐字节复算、恶意输入；v1 信封（body 字节保真、认证阶段连接字段规则、未知 `type`/字段拒绝）；全部 18 个消息类型与 33 个事件视图的字段级边界、fixture 覆盖与类型化 body 往返；33 个视图夹具按 `viewDef` 投影到同一 event 类型；配对 HTTPS 载荷（二维码/claim/status 的常量与宽度、`canonicalOrigin` 的 origin 形状、approved 与 device/host 的对应关系）；三个漂移门禁（`type` 常量与 schema union 同源抽取、错误码与 registry 的 `(code, retryable)` 逐条相等、`views::VIEW_TYPES` 与 `event-views.schema.json` 的 `$defs` 逐条相等） |
| node-link-protocol | 表与 registry 逐项相等、六个 domain 的固定向量逐字节复算；v1 信封与全部 29 个消息类型（连接字段规则、body 字节保真、类型化 body 往返、非法 body 分层拒绝）；配对 HTTPS 载荷；grant 词表与 `commands.json`、错误码与 registry、`MessageType` 与 schema union 三个漂移门禁 |
| acpr-transcript | 长度前缀编解码往返、字段乱序/重复/缺字段拒绝、magic 与 `codecVersion` 校验；表驱动层的字段数量、tag 成员与定长宽度校验 |
| acpr-wire | 值对象取值域（uuid/decimalString/timestamp/featureList/rawAcp 的正反用例）、`Nullable` 的「缺键报错 / null / 值」三态、`ExtraFields` 的未知字段值保真（含超出 u64 的整数字面量）、base64url 规范无填充口径 |
| agent-host | fake ACP child、超时、崩溃、乱序响应 |
| node-link-client | fake Owner、attachment generation、显式重连、origin 去重、capability 收缩、uncertain |
| storage-sqlite | migration、owned 原子提交、imported 无正文约束、TTL、容量限制 |
| identity-auth | 设备/节点配对过期、重放、无传递信任、grant 交集和撤销 |
| identity-keystore | 平台 keystore 可用与不可用两条路径、无可信 keystore 时失败关闭、密钥不进入日志与错误信息 |
| server::sync | auth、backpressure、续传、限流 |
| server::node_link | Export 过滤、节点 auth、attachment、ACK、backpressure、撤销 |
| server::acp_facade | ACP contract、能力协商真实性、扩展透传、外部 turn 重放 |
| app | 组合冒烟、关闭顺序、单实例 |

端到端测试使用可控的 fake ACP Agent。真实 Codex/OMP 测试作为可选兼容性套件，不作为普通 CI 的硬依赖。各模块测试使用机器矩阵中的 row/test ID 建立证据，矩阵结构由 `npm run check` 检查：矩阵本身、`fixtures/acp/v1` 与 vendored 上游固定快照（`schemas/acp/v1/upstream/schema.json`）都由 ajv 校验，快照 digest 与 commit 另行重算。`sync-protocol` 与 `node-link-protocol` 的编解码、协商和 transcript 测试直接消费 `schemas/sync/v1`、`fixtures/sync/v1`、`schemas/node-link/v1` 与 `fixtures/node-link/v1` 中的 manifest，不另建样例。

命令名、scope、pack、grant 与 transport 的词表以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 为准，Rust 侧不得再硬编码第二份：`sync-protocol`、`node-link-protocol` 的命令判别子必须与该文件由契约测试断言一致，`identity-auth` 的授权展开直接读同一份定义（编译期常量或启动时加载后校验，二者取一，但必须由测试证明与 JSON 一致）。

## 12. 保持开放的决策

- `identity-auth` 是否拆成纯状态机与平台 keystore 两个 crate；平台 keystore 的具体 crate 在选择时按 `AGENTS.md` §7 审必要性、维护状态、许可证与平台支持。
  - 2026-09-18 决定：**拆**。新增第 12 个 crate `identity-keystore`，`identity-auth` 收敛为纯状态机，keystore 以端口注入（[ADR-0006](./adr/0006-identity-keystore-split.md)）。判据是 `AGENTS.md` §4 的「独立平台实现」：平台 keystore 各自拖原生依赖与 `cfg` 分支，且在没有桌面会话的 Linux / CI 容器里不可用，混在一起会让状态机无法在所有平台编译与单测。
  - 仍然开放的部分：Linux 无可用 Secret Service 时是否提供降级存储（`SECURITY_DESIGN.md` §20）。它只影响 `identity-keystore`，不影响状态机；端口必须允许"非硬件保护"的实现存在，但默认不启用。
- 是否为同步协议生成 TypeScript/Kotlin/Swift 类型。
- 是否公开部分 crate 到 crates.io；第一阶段可全部保持 workspace-private。

CLI 与 Daemon 的管理通道已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定（stdio + 平台本地 IPC），不再开放。

标准不是目录是否整齐，而是边界能否降低耦合、支持独立测试并控制变化传播。

## 13. 架构图

- 可交互 HTML：[acp-remote-modules.html](./diagrams/acp-remote-modules.html)
- 图源 JSON：[acp-remote-modules.architecture.json](./diagrams/acp-remote-modules.architecture.json)

图源 JSON 是权威输入，HTML 是生成物：模块集合、边界或连接发生变化时必须改图源并重新生成 HTML，不允许手改 HTML。生成使用 archify 工具（仓库不内置该 CLI，也不作为运行时依赖）：

## 14. 参考

- Pi 当前将 session engine、wire protocol、client、server 和 SQLite backend 分开，并保持 protocol transport-neutral：<https://github.com/earendil-works/pi/tree/main/packages>
- Pi protocol/client/server 对 routed envelope、logical server identity、session attachment 和 transport abstraction 的说明：<https://github.com/earendil-works/pi/tree/main/packages/protocol>
- Oh My Pi 将 AI、Agent Core、Coding Agent、TUI 和 native 能力拆分，并从 interactive、RPC、SDK、ACP 等入口复用同一引擎：<https://github.com/dankalish/oh-my-pi>
