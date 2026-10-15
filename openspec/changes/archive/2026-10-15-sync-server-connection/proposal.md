<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

用户要求「开始使用 PWA 对接后端」。现状是**前端已就绪、后端入口不存在**：`clients/app` 的 Sync 客户端（约 7500 行）已实现逐连接握手、订阅、快照校验、去重、ACK 与命令派发，浏览器级 E2E 对**假主机** 48/48 通过，但 `crates/server/src/` 只有 `local_admin`/`node_link`/`transport` 三个模块，`server::sync` 一行都没有 —— 真实 Daemon 上 `/sync/v1` 与 `/sync/v1/pairing/*` 落到 404（`node-link-listener` 规范当前**要求**它们 404）。配对、认证、快照、事件投递四条链路全部缺失，PWA 无法与真机对接。

本次先落地**能让 PWA 真正连上并读到本机目录与会话**的最小闭环：设备配对、逐连接认证、订阅与快照、增量重放、ACK，以及它们依赖的 core 读面。命令族与事件视图投影（能交互）属同批第 2 个变更；`/ui` 静态托管属同批第 3 个变更。

## What Changes

**一、新增 `server::sync` 入站适配器的连接面**

- **设备配对 HTTPS**：`POST /sync/v1/pairing/{claim,status}`，按 `SYNC_PROTOCOL.md` §7 实现原子校验（存在、未过期、仍为 `created`、Origin/Host 匹配、HMAC proof）、claim 只成功一次、状态查询恒定 200、四个安全响应头与固定限流。**证书与 `pairingSecret` 不进日志**。
- **WSS 逐连接认证**：`/sync/v1` 上线（当前该 path 被规范要求 404）。四消息 `auth.client_hello` → `auth.server_challenge` → `auth.client_proof` → `auth.authenticated`，用 `identity-auth` 既有的 `SyncDevice` 入口完成双向 challenge-response；认证前只接受 hello/proof，认证在 15 秒内完成，超时以 `4408` 关闭。
- **浏览器来源校验**：服务端从**已验证的 HTTP `Origin`** 取 canonical origin（不使用 body 自报值），与设备记录中的绑定逐字比较；不一致 → `auth.origin_mismatch`。为此 transport 层必须把 `Origin` 头透传给 WS 会话（今天只传 `(WsConnection, PeerInfo)`）。
- **单设备连接规则**：每个 `(hostId, deviceId)` 只保留一条 active 连接，新连接认证完成后以 `4411` 关闭旧连接；设备撤销后立即以 `4410` 关闭其连接并拒绝后续认证。
- **心跳与 `limits`**：按协商的 `heartbeatIntervalMs`（默认 30 秒）发 `control.ping`，`control.pong` 原样回填 nonce；连续 90 秒无入站消息或 pong 时关闭。`auth.authenticated.limits` 下发 `sync.max_message_bytes`/`sync.max_prompt_bytes`/`sync.max_replay_events_per_batch` 三项（**今天这三个键被登记为「已知但未接线」，本变更让它们生效**）。

**二、事件投递面（订阅、快照、重放、ACK）**

- **cursor 判定**：`sync.subscribe` 的 cursor 为 `null` 时走首次同步；epoch 不匹配、超出 head 或早于保留窗口时回 `sync.cursor_invalid`（`details.reason` 取 `malformed`/`epoch_mismatch`/`beyond_head`/`cursor_expired`），**不得把 cursor 钳制到 head**；需要重建时由服务端主动发 `sync.reset_required`。
- **增量重放**：从 cursor 起按 `globalSequence` 升序投递可见事件，与实时投递建立无缝 barrier（重放期间的新事件必须排在 barrier 之后），随后发 `sync.caught_up`。
- **快照组装**：`sessions`/`workspaces`/`agents` 三类资源，按 `snapshotId` 分块，`chunkCount` 与实发数一致（`sessions` 恒发、空会话也发空数组；未协商 `core.local-catalog.v1` 时目录资源最多 1 块），`snapshotDigest` 按「逐 chunk 原始帧文本 SHA-256 → 按 index 连接再 SHA-256」计算 —— **与 Node Link 同一算法**。会话明细资源一律不进快照。
- **设备可见的会话清单**：快照 `sessions` 资源必须**同时**覆盖本节点自有会话与已导入会话（后者受 `resource.remote-origin.v1` 门控），并按设备 scope 过滤。今天 `SessionStore::list` 只返回 owned、`RemoteDeliveryStore::list_sessions` 只返回 imported，**没有合并读面**。
- **ACK**：`sync.ack` 只前进、回退被忽略并记诊断、超过本连接已发送最高 cursor 时回 `sync.cursor_invalid`。服务端可在连接期保存设备 ACK 最大值用于保留策略与诊断，但**新连接的恢复位置以客户端 `sync.subscribe` 提交的 cursor 为准**（§9.5），因此**不需要持久化表，也不需要 migration**。

**三、core 读面与组合根接线**

- **Sync 设备握手读面**：对照 Node Link 的 `node_link_handshake_view`，新增「按自报 deviceId 只读它自己的信任行、公钥、最近配对与本机 head」的未认证窄入口。今天 `device()`/`peer_key()` 被 `require_local()` 锁死为 `LocalCli` 专用。
- **设备认证收尾**：对照 `record_node_connected`，让设备首次认证推进 `last_seen_at` 并随写集提交 `device.authenticated` 审计；复用既有的 `consume_pairing`（它已支持 `Actor::Device`）。
- **事件扇出**：`EventPublisher` 增加 Sync 分支（现有 `ForkedPublisher` 只扇给 Node Link）。按**设备 scope 过滤**，每连接待发送队列有界（8 MiB / 2000 条），达到高水位先停读新的重放批次、持续过慢则按协议断开，绝不阻塞 Broker 或其他连接。
- **接线**：`app::daemon` 注册 `/sync/v1`（WS）+ 两条 pairing POST；`Composition` 增加 Sync 连接表与撤销钩子（`close_device` 今天恒报「没有可关闭的连接」）。

**四、合同与文档同步**

- `node-link-listener` 规范中「`/sync/v1` 必须 404」的要求**必须改写**为「Sync 端点上线」——这是本次唯一为行为正确性而改的既有规范。
- 修正文档内部冲突：`FRONTEND_DESIGN.md` §4.3 与 `INITIAL_DESIGN.md` §6.3 把 Sync 端点写作 `/sync`，而 wire 权威 `SYNC_PROTOCOL.md` §3.1 写 `/sync/v1`。按 `AGENTS.md` §9「wire 以协议文档为准」，改文档、改前端连接路径（前端改动属同批第 4 个变更），不反向修改协议。
- `CONFIG_REFERENCE.md` §2 撤回「三个 `sync.*` 键未接线」的注记；`MODULE_ARCHITECTURE.md` §4.9/§3 与 `README.md` 的「仓库当前状态」写回本次实绩。

**本次不包含**（属同批后续变更或明确不做）：

- **命令族与事件视图投影**（12 条 Sync 命令、幂等、终态投递、34 类视图、`acp.rawJson` 装配、命令级限流 4429）→ 同批第 2 个变更 `sync-server-commands`。
- **静态资源托管**与「前端构建版本 ↔ Daemon 可检查关联」→ 同批第 3 个变更 `sync-ui-hosting`。页面挂在**根目录**（用户 2026-10-12 裁定：根目录当前只有 404，且配对二维码与编译产物的资源引用都是根相对，挂根目录无须改路由与二维码）；该变更同时定下资源嵌入方式（用户同日裁定：嵌入 Rust 二进制）。
- **前端接线修正**（连接路径 `/sync`→`/sync/v1`、WS subprotocol、`approved` 响应落 `hostPublicKey`、配对页扫码入口）→ 另开变更（用户裁定）。
- **`node-link-client` 与 `server::acp_facade`**（切片 6）；**真实 Codex/OMP 兼容套件**。

## Capabilities

### New Capabilities

- `sync-device-auth`: Sync 设备的逐连接认证面 —— WSS 端点与 subprotocol、四消息握手与 15 秒窗口、基于已验证 HTTP Origin 的来源校验、单设备连接替换（`4411`）、撤销传播（`4410`）、心跳与静默超时、`limits` 下发与 close code 映射。
- `sync-event-delivery`: Sync 的事件投递面 —— 订阅与 cursor 判定、增量重放与 barrier、`reset_required`、三类清单资源的快照组装与分块摘要校验、设备可见会话清单的范围（自有 + 已导入、按 scope 过滤）、ACK 单调性与恢复位置口径。
- `sync-pairing-http`: 设备配对的 HTTPS 端点 —— claim 的原子校验与一次性、status 的业务状态承载（恒定 200）、四个安全响应头、固定限流、以及「确认只能由本机入口完成」的边界。

### Modified Capabilities

- `node-link-listener`: 该能力当前要求「`/sync/v1` 与 `/sync/v1/pairing/*` 在 `server::sync` 缺席期间 MUST 返回 404」；本次把 Sync 端点接上线，因此该要求与它的「Sync 路径明确不可用」场景必须改写为「Sync 端点按各自协议处理」，并保持 Node Link 与未知 path 的既有行为不变。

## Impact

- **Rust**：新增 `crates/server/src/sync/`（连接、订阅与重放、快照、配对 HTTP）；`crates/server/src/transport/net/`（把 HTTP `Origin` 透传给 WS 会话）；`crates/core/src/use_cases.rs`（Sync 设备握手读面、设备认证收尾、设备可见会话清单）；`crates/app/src/{compose,daemon,config}.rs`（Sync 事件扇出、路由注册、`sync.*` 配置接线、撤销钩子）。
- **依赖**：不新增任何 crate；快照摘要与限流复用 `sha2` 与既有的连接注册表/限额样板。
- **存储**：**不新增表、不推进文件格式版本** —— 设备 ACK 按 §9.5 只在连接期保存。
- **机器合同**：`schemas/sync/v1/`、`fixtures/sync/v1/`、`compatibility/` 的 Sync 面**已冻结**，本变更不预期修改；若实现中发现 schema 缺口，必须在同一变更内补齐并让 `npm run check` 全绿。
- **权威文档**：`docs/SYNC_PROTOCOL.md`（§3.1/§7/§8/§9 的实绩写回）、`docs/CONFIG_REFERENCE.md`（§2 撤下「未接线」注记）、`docs/MODULE_ARCHITECTURE.md`（§3/§4.9 现状写回）、`docs/FRONTEND_DESIGN.md` 与 `docs/INITIAL_DESIGN.md`（`/sync` → `/sync/v1` 的冲突修正）、`docs/SECURITY_DESIGN.md`（Origin 校验与撤销传播的实绩）、`README.md`（仓库当前状态、`server` 行、尚未开始清单）。
- **验收边界**：本变更的可观察结果是**协议级闭环**（脚本化 fake Sync 客户端经真实 loopback listener + 真实 SQLite + 真实 `Authority` 走完配对→握手→订阅→快照→ACK），不是「浏览器点得开」。端到端 PWA 可用还依赖同批第 3、4 个变更，届时才具备浏览器级证据。
