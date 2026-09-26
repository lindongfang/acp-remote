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

## Intent and Constraints

```agentic-intent
sources:
  - "用户在 2026-09-26 的本会话中给出上下文引用 `docs/DEVELOPMENT_PLAN.md#L48:50`，指向 `## 3. 实施切片` 的 `### 5. Owner 侧 Node Link`（原文：「实现 `server::node_link` 的节点认证、Export 过滤、catalog、resource、command、ACK 和重放。先用单 Owner、单 Access、单 Export 跑通受控路径，再覆盖撤销、连接 generation 和慢连接。[Node Link 协议](NODE_LINK_PROTOCOL.md)定义 wire 和顺序语义。」/「验收：只有已授权 Access 能访问导出的资源；Owner 先持久化事件和命令终态再发布；相同 `requestId` 的重试不会重复派发；撤销后旧连接不能继续取资源或发命令。」），未附加自由文字。"
  - "同会话用户原话：「Propose a new change - create the change and generate all artifacts in one step.」——授权建立本变更并生成全部规划工件，同时明确「planning artifacts only」，不在同一响应内开始实现。"
  - "同会话用户对三个范围问题的裁决（2026-09-26，原话：「1B 2A 3批准」）：1B = TLS 两种模式（proxy/direct）都接线；2A = 配对 HTTP 端点（claim/status）纳入本切片；3 = 批准本变更 Main E2E 记 not-applicable（原话、时间与来源将写入 plan.md 的 downgrade_approval）。"
constraints:
  - "依赖方向（`MODULE_ARCHITECTURE.md` §5 矩阵为唯一判据）：`server` 只允许依赖 `core`、`acp-protocol`、`sync-protocol`、`node-link-protocol`、`identity-auth` 与本切片新增登记的 `acpr-wire`（只为 ACPR-CJ1 digest）；`server::node_link`/`server::local_admin`/`server::transport` 平级，只能调用 `core::use_cases` 与 `identity-auth` 的认证入口，不能互相调用、不能查询 SQLite、不能启动 Agent。"
  - "权威与持久化顺序（`NODE_LINK_PROTOCOL.md` §6/§15、`AGENTS.md` §3）：Owner 事件必须先持久化再发送 Node Link；命令终态持久化后才发布；`persist_deltas: false` 不允许未持久化就广播；跨节点幂等键至少含 `(ownerNodeId, accessNodeId, requestId)`，崩溃窗口必须显式进 `uncertain`。"
  - "ACP 兼容性不变量（`AGENTS.md` §3、`NODE_LINK_PROTOCOL.md` §11.4）：`resource.event.payload.acp` 的 ACP raw document 必须逐字节保真；结构化事件不得文本化；未登记 eventType 保留 `payload.acp` 降级；能力协商如实反映端到端交集，不虚报。"
  - "安全不变量（`AGENTS.md` §3、`SECURITY_DESIGN.md`）：每个新 WSS 连接完整执行 challenge-response，不引入 bearer token；验签公钥只能来自持久化信任记录；私钥/pairingSecret/凭据不进入日志、协议错误或普通 SQLite 字段；TLS 终止点必须位于本节点可信边界内；`dev_mode.allow_plaintext` 只允许 loopback。"
  - "Export 策略（`NODE_LINK_PROTOCOL.md` §10/§12.7）：默认不导出任何 Agent；有效权限是 grant 交集；`session.create` 不携带任意 Owner 路径或凭据；首切片 template 零参数，声明了参数的 Export 明确拒绝而非静默忽略。"
  - "封闭词表只有一处机器定义（`AGENTS.md` §5/§10）：错误码、feature ID、命令名/grant 只消费 `compatibility/**` 与 `node-link-protocol` 的既有定义，Rust 侧不另造词表；`schemas/node-link/v1` 与 `fixtures/node-link/v1` 的合同测试必须消费同一 manifest。"
  - "新增第三方依赖按 `SECURITY_DESIGN.md` §20 与 `AGENTS.md` §7 核验必要性、维护状态、许可证与平台支持，不得抬高 `rust-version = 1.85`；workspace 固定 `unsafe_code = \"forbid\"`，不放开。"
  - "本地入口 `npm run verify`；`cargo-deny`（deps/advisories）与 `gitleaks`（secrets）只在 CI 运行，本地没有等价物，不得声称已在本地通过（`AGENTS.md` §8）。"
non_goals:
  - "不实现切片 6/7：`node-link-client`、`server::acp_facade`、`server::sync` 与 Web/PWA 前端；`/sync/v1*` 路径在本切片明确 404。"
  - "不实现四个 post_mvp 消息（`catalog.changed`/`resource.detach`/`node.rotate-key.*`/`link.backpressure`），收到时显式返回 `nodelink.protocol.type_unsupported`。"
  - "不实现 `catalog.changed` 增量：catalog 是连接期内存数据，`knownRevision` 非空也回完整快照（§12.3 的投影生命周期决定）。"
  - "不改变配对确认路径：批准/拒绝仍只经切片 4 的本地管理入口（CLI `node pair`），本变更只新增 Access 侧可触达的 claim/status HTTP 端点。"
  - "不实现证书热轮换：`direct` 模式的证书变更经重启生效（`SECURITY_DESIGN.md` §9.5：证书轮换不改变 Node identity）。"
  - "不修改 `schemas/`、`fixtures/`、`compatibility/` 的既有内容；不新增/删除错误码、feature ID、命令名、grant 取值。"
  - "不在本变更内抬高 MSRV/工具链基线，也不为放开 `unsafe_code` 新增 lint 覆盖；不做 Linux 产品交付（Unix 路径只要求编译与 CI 覆盖）。"
success_criteria:
  - "授权边界：只有已配对、未撤销且持有相应 Export/grant 的 Access 节点能完成握手、取到 catalog、attach 会话与提交命令；未知节点（`node_unknown`）、已撤销节点（4410）、超 grant 命令（`not_granted`）逐条有测试（对应切片 5 验收第 1 条）。"
  - "持久化顺序：集成测试证明 `resource.event` 与 `command.terminal` 只在 SQLite 提交成功后发出；相同 `requestId` 重试返回首次结果不重复派发；模拟崩溃窗口的命令进 `uncertain`（切片 5 验收第 2/3 条）。"
  - "撤销即时生效：本地 `export.revoke`/`node.revoke` 提交后，受影响连接收到 `export.revoked`/`node.trust.revoked`，旧连接不能继续取资源或发命令，旧 attachment generation 的 frame 被拒绝（切片 5 验收第 4 条）。"
  - "受控路径闭环：脚本化 fake Access 客户端经真实 listener（loopback）完成 配对 claim → 本地确认 → 握手 → catalog.snapshot → attach → snapshot/event/ack → command（含 `session.create` 正常与各拒绝路径）→ 撤销传播 的全链路集成测试，由 `cargo test` 可重复执行。"
  - "配对 HTTP：claim/status 覆盖 §13.4 的状态码表（含 401 不泄露差异、幂等重试、429 限流），四个安全头逐项断言，secret 不进日志。"
  - "TLS：proxy 模式起明文 listener 且 Host 边界校验生效；direct 模式证书/私钥缺失或权限不符即拒绝启动；loopback 之外无明文接入。"
  - "`npm run check` 与 `npm run verify` 全绿；`check:boundaries` 对 `server` 新增依赖格逐条通过；新增测试确实执行（无零用例、无全跳过）。"
decision_bounds:
  - "Agent 可自主决定：`server::transport::net` 与 `server::node_link` 内部模块划分、HTTP/WS/TLS 具体 crate 选型（前提：§20 四判据核验通过并留证）、catalog revision 的持久化来源、attachment 注册表与每连接发送队列的内部结构、撤销通知的进程内机制（在已冻结语义内扩展切片 4 的 `ConnectionCloser` 装配缝）、限流的内部数据结构、测试基座与 fake 客户端写法。"
  - "需要用户决策：抬高 workspace `rust-version`/工具链基线；放开 `unsafe_code`；改动 wire 合同（schema/fixture/错误码/feature/命令词表）、`core` 端口签名或依赖矩阵中除 `server`→`acpr-wire` 外的方向；把切片 6/7 的任何内容并入本变更；改变配对确认只经本地入口的既定边界。"
  - "用户已批准：TLS 两种模式都接线（1B）；配对 HTTP 端点纳入本切片（2A）；Main E2E 记 not-applicable（3批准）；「先出规划工件、不在同一响应内实现」。"
assumptions:
  - "本切片不产生可端到端运行的产品路径（Access 侧 `node-link-client` 与 `acp_facade` 属切片 6），Main E2E 记 `not-applicable`；替代验证 = 本变更相关 cargo 测试（含 fake Access 全链路集成测试）+ `npm run check` / `npm run verify`。批准原话已记录（2026-09-26「3批准」）。"
  - "`identity-auth` 的 `Authority` 三个握手入口与配对状态机（`verify_claim`/`settle`/`pairing_sas`）已覆盖 Node Link 配对与连接认证的全部状态机语义，本变更只做 IO 适配与写集装配；若实现期发现缺口，按既有模式在 `identity-auth` 增补并同步 `IDENTITY_AND_AUTH_CONTRACT.md`，不改变其状态机语义方向。"
  - "catalog revision 可以从既有管理存储导出单调值（例如 Export/信任记录的变更水位）；若实现期确认需要新增持久化计数器，按 `CORE_PORTS_AND_STORAGE.md` 的合同流程在同一变更内同步 §5/§7 并让漂移门禁通过。"
  - "TLS direct 模式的 rustls 栈能满足 §20 核验（纯 Rust、许可证允许、MSRV 达标）；若核验失败，回到用户决策（换栈或本切片只接线 proxy），不擅自放开约束。"
  - "`server`→`acpr-wire` 的依赖格增补是唯一预计的矩阵变化（digest 计算）；它只扩大 `server` 的允许依赖，不改变其他 crate 的方向。"
```

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
