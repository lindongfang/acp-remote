# Proposal: node-link-owner

## Why

`docs/DEVELOPMENT_PLAN.md` 的实施切片 5（行 48–50）还没有任何实现：`server` crate 只有本地通道（`transport::local`）与 `local_admin`，`server::node_link` 不存在；`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 在切片 4 是「已解析但**未接线**」的配置键（`crates/app/src/config.rs` 的 unwired 清单），Daemon 没有任何网络监听。其后果是直接的：Node Link v1 wire 标准自 2026-09-18 冻结、`node-link-protocol` crate 的全部 29 个消息类型与配对载荷早已实现，但没有进程能**接受**一条 Node Link 连接——Access 节点无处握手、无 catalog 可取、无资源可 attach、无命令可发，切片 6（Access 侧远程 backend 与 Zed 闭环）与整个「首个产品闭环」因此没有 Owner 一侧。

前置切片已经就位：切片 1/3 落地了信任记录、Export 管理与 `identity-auth` 的配对/握手状态机（含 Node Link 的 6 个 transcript domain 与 `Authority::hello`/`verify_proof`/`complete_auth` 三个入口），切片 2 落地了本地 Agent backend 与事件 broker（先持久化后发布），切片 4 落地了组合根、Daemon 生命周期与 CLI 配对管理入口。本变更把「Owner 侧 Node Link」作为第五个纵向实现切片落地：共享网络 listener（含两种 TLS 模式）、节点配对 HTTP 端点与 Node Link WSS 服务端，为切片 6 提供可连、可信、受 Export 策略约束的 Owner 接入面。

## What Changes

- `server::transport` 新增网络部分（`server::transport::net`）：`daemon.listen` 的共享 HTTP/WSS listener、按 path 路由（`/node-link/v1` 升级 WebSocket；`/node-link/v1/pairing/claim` 与 `/node-link/v1/pairing/status` 接受配对 HTTP；`/sync/v1*` 与其余路径在本切片明确 404）、Host 边界校验（`daemon.public_origin`/`daemon.allowed_hosts`/`daemon.trusted_proxies`）、非 loopback 监听的启动告警、连接级读取与请求体上限。TLS 按用户裁决**两种模式都接线**（`CONFIG_REFERENCE.md` §1）：`proxy` = 明文 HTTP 由同机可信反代终止 TLS；`direct` = Daemon 用 rustls 栈自行终止 TLS（证书/私钥加载或权限检查失败即拒绝启动，私钥不进日志）。
- `server::node_link` 的配对 HTTP 端点（`NODE_LINK_PROTOCOL.md` §13）：`claim`（原子检查 + HMAC 校验 + `ownerProof` 签名 + `pending_confirmation`、幂等重试返回原 pairing request、401 不泄露校验差异、409/410/404/403 语义）与 `status`（一律 200 + body 状态、五种业务状态、nonce 重试规则）；四个安全响应头、配对限流（claim 10/min/IP、status 60/min/pairingId → 429）；配对确认仍只经切片 4 的本地管理入口，本变更不新增确认路径。
- `server::node_link` 的 WSS 服务端（`NODE_LINK_PROTOCOL.md` §2/§11/§12/§14/§15）：
 - 握手：认证前只允许 `node.hello`/`node.challenge`/`node.proof`；版本交集与 feature 协商（封闭词表只消费 `compatibility/features/v1/features.json`，必需 feature 未满足 → `nodelink.protocol.feature_required`）；经 `identity-auth::Authority` 的 NodeLink 入口完成双向 challenge-response；凭据状态映射（revoked → `nodelink.auth.node_revoked` + 4410，unknown → `nodelink.auth.node_unknown`）；握手 15 秒超时；认证收尾写集（pairing 转 consumed、last_seen、审计）随提交落库。
 - 信封与序号：认证前省略/认证后必带 `connectionId`/`connectionSequence`，两个方向各自从 `"1"` 严格加一，回退/跳号/不匹配 → `nodelink.protocol.sequence_invalid`；closed object 校验、未知字段 → `schema_invalid`、未知 type 与 `post_mvp` 消息 → `type_unsupported`（不静默忽略）；binary frame → `invalid_json` + 4400；超限消息在分配大对象前以 1009 拒绝。
 - Catalog：`catalog.subscribe`/`catalog.snapshot` 从 Owner 自己的 SQLite Export 记录投影（按该 Access 信任记录的 `exportIds` 过滤），批次大小遵守协商 limits 且批次内顺序稳定；`catalog.changed` 不实现，`knownRevision` 非空时仍回完整快照。
 - Resource：`resource.attach`/`attached` 签发新 `attachmentId`/`attachmentGeneration`，旧 generation 的 frame 一律 `attach_generation_stale`；`resource.subscribe` 的 `cursor = null` 走快照（`session_meta`/`pending_interactions` 两种 item、正文绝不入快照、`snapshotDigest` 按原始 bytes 规则），非空走 origin cursor 增量重放（epoch 不一致 → `sequence_invalid`）；`resource.event` 只在 broker 持久化提交后发布，携带完整 origin 三元组与 `payload.view`/`payload.acp`（至少其一），`payloadDigest` 走 ACPR-CJ1，单个内嵌 diff/document 超 256 KiB 用 `rawAcp.rawUnavailable(size_limit)`；`resource.ack` 累计游标单调不减。
 - Command：`command.submit` → `accepted`/`rejected`/`terminal` 与 `command.status` 重查；有效权限 = Export grant ∩ 信任记录 grant ∩ 实际 capability；幂等键 `(ownerNodeId, accessNodeId, requestId)` 重试返回首次结果、语义不同 → `idempotency_conflict`；`terminal` 必带 `command`、`completed` 必带非空 `result`；崩溃窗口进 `uncertain`；`session.create` 的硬约束（只允许四个键、`cwd`/`mcpServers`/绝对路径/凭据字段 → `unsupported_field` 且 `details.field`、未导出组合 → `not_granted`、首切片零参数 template、`terminal.result` = `SessionCreateResult`）；in-flight 上限与 120/分钟命令限流（`rate_limited` + `details.retryAfterMs`，连续超限 4429）。
 - 控制与撤销：`link.ping`/`link.pong` 心跳（间隔取协商 limits，90 秒静默 → 4408）；待发送队列高水位先停读快照批次、持续过慢断开由 cursor 重放补回（`link.backpressure` 属 post_mvp，不发送）；本地 `export.revoke`/`node.revoke` 提交后向受影响连接推送 `export.revoked`/`node.trust.revoked` 并立即拒绝相应命令与订阅（节点撤销同时以 4410 关闭连接）。
- `app` 组合根接线：`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 从 unwired 清单移出并接线；启动序列在恢复管理记录后、开放接入前绑定 listener（TLS `direct` 证书失败即拒绝启动）；关闭序列的「停接入层」扩展为含网络 listener 排空（宽限上限内等待在途连接）；`daemon.status` 的 `listen` 字段返回实际监听地址（切片 4 恒为空数组）；Node Link 入站连接纳入单实例 Daemon 的统一关闭所有权。
- 依赖登记与核验：HTTP/WS/TLS 栈候选（axum、tokio-tungstenite、rustls 系）按 `SECURITY_DESIGN.md` §20 四判据核验（语义、许可证、MSRV ≤ 1.85、维护状态），结论与证据写入 design 并登记 `Cargo.toml`/`deny.toml`；`docs/MODULE_ARCHITECTURE.md` §5 矩阵视设计结论为 `server` 增补 `acpr-wire` 依赖格（payloadDigest/snapshotDigest 需要 ACPR-CJ1，`acpr-wire` 是唯一实现）。
- 合同与状态同步：`docs/MODULE_ARCHITECTURE.md`（§3.1 依赖登记、§4.9 现状注记、§5 矩阵与注记）、`docs/DEVELOPMENT_PLAN.md` §2、`README.md`「仓库当前状态」、`AGENTS.md` §4 模块状态表、`docs/CONFIG_REFERENCE.md` 的 unwired 注记收敛。
- **不修改**任何 wire 合同资产：`schemas/node-link/v1/`、`fixtures/node-link/v1/`、`compatibility/**` 保持原样，本变更只新增**消费**既有 schema 与 fixture 的 Rust 侧编解码/校验与契约测试；实现期若发现实现与已冻结合同冲突，先停下报告，不擅自改合同。

本次**不包含**：`node-link-client`、`server::acp_facade`（切片 6）、`server::sync` 与 Web/PWA（切片 7）；`catalog.changed`/`resource.detach`/`node.rotate-key.*`/`link.backpressure` 四个 post_mvp 消息（收到显式 `type_unsupported`）；catalog 的持久化（合同定为连接期内存数据，Owner 侧本就只从 SQLite 投影）；imported Agent 再导出；Noise 或其他 Transport Profile；多 hop 转发。

## Capabilities

### New Capabilities

- `node-link-listener`: Daemon 共享网络接入面的可观察行为——`daemon.listen` 绑定与失败关闭、非 loopback 启动告警、按 path 的路由（Node Link WS 与配对 HTTP 可用、Sync 与其余路径明确 404）、Host/代理头边界校验、TLS `proxy`/`direct` 两种终止模式、请求体与单消息上限。
- `node-link-pairing-http`: Owner 侧节点配对 HTTP 端点的可观察行为——`claim` 的原子检查/HMAC/`ownerProof` 与幂等重试、`status` 的一律 200 与五种业务状态、§13.4 状态码语义、四个安全响应头、配对限流与 secret 生命周期。
- `node-link-owner-server`: Node Link WSS 服务端的可观察行为——四步握手与 feature 协商、信封与双向 connectionSequence、Export 过滤的 catalog 投影、attachment generation 与快照/事件/ACK、命令接受/幂等/终态与 `session.create` 硬约束、心跳与慢连接隔离、撤销传播与审计。

### Modified Capabilities

- `storage-schema-v2-migration`: 「版本常量与 migration 幂等」需求的版本目标从 2 推进到 3（v3 = v2 + 两张审计表 CHECK 扩展的 12-step 重建；v1 连续升级、失败回滚与幂等语义不变）。起因：RV1-WP3C-F1——D12 的合同扩展改了版本常量，必须有对应 MODIFIED 增量。

其余既有能力（`identity-pairing`/`identity-handshake`/`scope-expansion` 等）的**需求**不变，本变更只消费它们已冻结的状态机与入口。

## Impact

- **代码**：`crates/server`（新增 `transport::net` 与 `node_link` 两大模块，扩展 `local_admin` 的 `ConnectionCloser` 装配缝）；`crates/app`（listener/TLS 接线、启动与关闭序列、`daemon.status` 的 `listen` 字段、配置 unwired 清理）；`Cargo.toml`（workspace 依赖登记）、`deny.toml`（如需）。
- **文档**：`docs/MODULE_ARCHITECTURE.md`（§3.1/§4.9/§5）、`docs/DEVELOPMENT_PLAN.md`（§2）、`README.md`（仓库当前状态）、`AGENTS.md`（§4 模块状态表）、`docs/CONFIG_REFERENCE.md`（unwired 注记收敛）。
- **合同资产**：不改；新增消费 `schemas/node-link/v1/` 与 `fixtures/node-link/v1/` 的 Rust 侧契约/漂移测试。
- **依赖**：新增 HTTP/WS/TLS 栈（候选 axum + tokio-tungstenite + rustls 系，以 §20 核验结论为准），许可证/来源/advisory 判定只在 CI 的 `deps`/`advisories` job 完成。
- **外部行为**：Daemon 开始监听 `daemon.listen`（默认仍为 loopback）；`daemon.status` 的 `listen` 不再恒空；Node Link 配对与 WSS 端点对外可达性取决于用户显式配置（非 loopback 监听或同机反代）。
