# ADR-0003：采用 Pi 风格的模块与 crate 边界

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：Rust workspace、依赖方向、Agent backend、持久化事务和入站 adapter

## 背景

早期设计把 `domain`、`application-ports`、`broker-core`、三个 server adapter、daemon 和 CLI 分别列为计划 crate。职责虽然清晰，但在尚无代码时就形成十五个物理 crate，容易产生样板 mapper、跨 crate 类型泄漏和为了目录整齐而拆分的问题。

Pi 的当前结构把 session/agent 核心、transport-neutral protocol、client、server 和 SQLite session backend 分开；核心不携带平台数据库依赖，协议不解释应用业务 payload。Oh My Pi 也从 interactive、RPC、SDK、ACP 等入口复用同一 session engine。

## 决策

1. 首版 Rust workspace 使用十个粗粒度 crate：`core`、三个 protocol、`agent-host`、`node-link-client`、`storage-sqlite`、`identity-auth`、`server` 和 `app`。
2. `core` 内部包含 `model/use_cases/ports/broker`，不在首日拆成多个 crate。
3. `server` 内部包含平级的 `sync/node_link/acp_facade` adapter；它们不能互相调用。
4. 三个 protocol crate 是 transport/wire contract，只包含 schema、codec、framing、limits、版本协商和 fixture，不依赖 `core`。wire/core mapper 放在 adapter。
5. 不定义巨型 `AgentRuntime`。本地与远程 backend 实现 `AgentCatalog`、`SessionBackendFactory` 和绑定单个 live session 的 `SessionEndpoint`。
6. `SessionStore` 用一次事务提交 owned session 状态、事件和幂等；imported session 使用只含 cursor/digest/local sequence 的 `RemoteDeliveryStore`。
7. Node Link client 的 transport 是可替换接口，WSS 只是实现；连接 attachment 带 generation，旧连接 frame 不能进入新绑定。

## 结果

- crate 数量减少，但协议、核心、client/server、backend 和组合根仍由编译期依赖隔离。
- wire contract 不会因 core 领域重构而被迫变化，core 也不会依赖 JSON-RPC/WSS DTO。
- 本地 Agent 与远程 Agent 共享会话级能力接口，而不共享进程或传输细节。
- SQLite 原子性在 port 层可表达，Access `no-content-cache` 也有独立存储端口保证。
- 如果某个内部模块未来需要独立发布、平台实现或编译隔离，再通过 ADR 提升为 crate。

后续：[ADR-0005](./0005-shared-transcript-codec.md) 在本决策之上新增第 11 个叶子 crate `acpr-transcript`（Sync 与 Node Link 共用的 transcript codec 结构），协议 crate 仍互不依赖。
