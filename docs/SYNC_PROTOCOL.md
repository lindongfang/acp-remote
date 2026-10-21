# ACP Remote Sync Protocol

> 状态：wire 基线（Draft）已冻结；`sync-protocol` crate 已实现 v1 的全部 27 个消息类型——信封与消息类型分派，`auth`/`sync`（含 9 个 `sync.remote_*` 来源流消息）/`control`/`error`/`event`/`command` 六个家族的 body——33 个事件视图定义（`event.payload.view` 的类型化投影），以及配对 HTTPS 载荷（设备短二维码 `q2` 编解码、claim、status、lookup、qr-lookup、HTTP 错误体）。**服务端（Daemon 侧）已落地的面**：§3.1 的 WSS 端点与 subprotocol、§7 的既有配对 HTTP（§7.6 `qr-lookup` 待 WP2–WP4）、§8 的设备认证（握手、来源绑定、单设备替换、撤销传播、心跳与 close code 映射）、§9.1–§9.5 的 cursor 判定、增量重放与实时投递、快照组装与摘要、累计 ACK。**尚未落地**：`server::acp_facade`、§9.6 的 imported 事件回源投递、§9.4 的三类明细资源（`turns`/`pending_interactions`/`capabilities`）与 imported 会话明细（详见各节的实现状态注记）。会话状态机、保留窗口与授权判定的核心实现位于 `core`/`server` crate，本文仍是 wire 的权威来源。
> 协议版本：1  
> 修订记录（2026-10-18，`device-pairing-qr-reference`）：**设备二维码完全替换**为 `https://<canonical-origin>/pair#q2=<payload>`——payload 是 `base64url-no-pad(0x02 || 32 字节扫码取回凭据 || 32 字节主机公钥指纹)`，共 65 字节 / 87 个 ASCII 字符；旧设备 `#data=` 完整资料格式**退出且不降级**，节点二维码与 `/node-link/pair` 不变。新增 §7.6 `POST /sync/v1/pairing/qr-lookup`（请求 `{protocolVersion:1, qrToken}`，成功返回与配对码 lookup 同形的七字段，统一 `401 pairing.qr_invalid`，独立 10 次/分钟限流，16 KiB 上限）。PWA 只保留配对码入口：打开 `q2=` 或旧 `data=` 片段只在清除片段后显示固定指引/不支持文案，不发任何配对请求（行为见 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md)）。本轮先交付协议类型、机器合同、共享固定向量与本文；服务端取回路由、身份内存材料与 PWA/原生消费方为**计划/待实现**（WP2–WP4），原生 App 真机扫码仍是后续交付。机器资产同步：`pairing.schema.json` 增加 qr-lookup 请求/成功/失败形状、`pairing.qr_invalid` 登记到唯一错误词表、删除旧 `qrPayload`，新增 `fixtures/sync/v1/device-qr/vectors.json`（Rust/TypeScript 共享）。
> 修订记录（2026-10-18，`pairing-code-web-entry`）：配对新增第二个入口——`POST /sync/v1/pairing/lookup` 用 8 位 Crockford 配对码一次性换回 `pairingId`/`hostId`/`hostPublicKey`/`canonicalOrigin`/`pairingSecret`/`expiresAt`，之后与二维码入口共用同一套状态机、claim/status 与 transcript。§7.1 写清两个入口的关系；新增 §7.5 的 lookup wire 行为（成功即消耗、统一 `401 pairing.code_invalid`、与 claim 各自 10 次/分钟、同一组安全响应头与 `no-store`）；§7.4 的状态表补 lookup；§12.2 增加 `pairing.code_invalid`。schema、fixture 与错误词表同步；claim/status 的 wire 与 transcript 未变。
> 修订记录（2026-10-16，`pairing-model`）：**删除两套短验证码**——§6.1/§6.3 删去 `acp-remote/pairing-sas/v1` domain 与 `pairing SAS` 字段集合；§7.2 改为两条设备侧密码学校验（用配对载荷里固定的主机公钥验证认领响应的 `hostProof`、已批准响应的主机公钥与载荷逐字节相同），不再派生或展示 SAS；§7.3 的 `approved` 响应不再返回授权集合；§17 的固定向量判据去掉 SAS 重算。**删除设备权限层**——§7.0 的确认不再分配授权；§8.4 的 `auth.authenticated` 不再携带设备 scope 集合；§9.2/§9.3/§9.4 不再按设备授权过滤事件、会话与目录资源；§11.1/§11.3/§11.4/§11.5 改为「已认证即执行前提」，并把 `session.create` 的 Sync 面载荷放开为「`directoryId` 或客户端提交的绝对目录 `workspacePath`，二者取一」（Node Link 面仍只允许别名、且请求不得携带任意 Owner 路径）；§12.1 的示例错误码换掉设备 scope 拒绝；§14 的「命令族」改为服务端自持的 v1 常量（不再取自 `commands.json` 的 `packs`）；§15 的逐命令 scope 授权改为认证状态与幂等校验。**保留**：设备认证与每连接 challenge-response、撤销传播与速率上限、配对确认必须由本机用户完成、跨节点 `grant.*`/Export `scopes`/Import grants。  
> 修订记录（2026-10-16，`session-create-settle-order`，无 wire 变更）：`session.create` 的落账时序改为**三笔**（受理只落幂等记录 → 执行 → 会话行 + 标识回填 + 终态在**同一次提交**，失败零会话行）——§11.2 的「结果语义固定如下」补上资源行与终态同事务、以及崩溃窗口只有受理记录（**没有**会话行）的形状，命令处理顺序块的注记同步；§11.5 的 `session.create` 行与正文改写为「先受理再执行」并给出**两面一致**的终态判据（可证明 Agent 侧未产生会话 ⇒ `failed`、复用既有码 `internal.unavailable`；说不清 ⇒ `uncertain`），`引用失败与解析失败的分类` 不变（引用/解析失败仍是接受前的拒绝、零副作用）。错误码、feature、schema 与 fixture 均未变。  
> 修订记录（2026-09-18）：新增 §3.3 ACPR-CJ1 规范 JSON（`payloadDigest` 前像）；§4.1 明确认证前省略 `connectionId`/`connectionSequence`；§4.2 明确未知命令名返回 `command.unsupported`；§11.5 的 `command` 枚举只含 `transport` 含 `sync` 的命令；§14 把限流与若干上限固定为 v1 常量；§17 改为列出全部五个检查脚本与覆盖门禁；§12.2 新增 `details` 登记表并为 `protocol.feature_required`/`state.version_conflict`/`resource.rate_limited` 登记机器可读字段（兼容新增）；§10.2 新增 `turn.delta_compacted` 与 `agent.message.delta` 的可选 `block`，并写明 `deltaIndex` 的分配规则、收尾事件的生成与压缩后的重放语义（对齐 `CORE_PORTS_AND_STORAGE.md` §6 第 14/15 条）；§11.5 与 §10.2 的 `elicitation.respond` 增加 `decline` 动作并把 `submit` 的 `values` 放宽为 `object|null`（对齐 ACP 的 `accept`/`decline`/`cancel`，兼容新增）。  
> 修订记录（2026-10-02）：`session.create` 经 Sync 暴露（feature `core.session-create.v1`）——§11.3 把它移入可授予集合并写明双层授权与设备级判定，§11.5 给出 Sync 面的 `{ <目录标识>, agentId }` payload、`accepted`/`completed` 终态与失败分类，§16.2 说明它是可选 feature 引入而非 major version；§5.2 新增 `core.session-create.v1` 与 `core.local-catalog.v1` 两条 feature。本次不新增任何错误码（映射见 §11.5）。
> 修订记录（2026-10-14）：`event.body.origin` 增加第五个产生者类型 `kind = "node"`（由另一个节点经 Node Link 下发命令、在**本机**事件库落库的事件）与身份字段 `nodeId`，两者由 feature `core.event-origin.v1` 协商启用——§10.1 给出 `origin` 的两种形状（未协商时仍是冻结的 `{kind, deviceId}` 两个键）、`kind` 取值表的新行、唯一一条配对规则（`node` 必须带非空 `nodeId` 且 `deviceId` 为 `null`）与"经 feature 协商启用、不是加性变更"的理由；§9.6 同步 kind 枚举，§5.2 登记 `core.event-origin.v1`，§16.1/§16.2 把这次扩展归入 feature 协商并说明它为什么不需要 major version。机器资产同步：`schemas/sync/v1/event.schema.json` 的 enum、可选字段与 `if`/`then`（`origin.required` 保持 `kind`/`deviceId`），`compatibility/features/v1/features.json`，`fixtures/sync/v1/` 的固定向量（既有事件夹具补 `"nodeId": null`，新增 `kind = "node"` 的正例、两条 `node` 形状反例与一条未协商形状 `{kind, deviceId}` 的正例）。
> 修订记录（2026-10-14，`sync-server-connection` WP7 文档写回，无 wire 变更）：按 Daemon 侧实绩写入四节实现状态注记——§3.1（端点与 subprotocol 已可用、来源三形态的传递、binary 帧关闭）、§7.4（配对 HTTP 五项原子校验/幂等/status 一律 200/四个安全响应头/固定限流，并如实登记「证明失败 5 次失效只在内存」与本地确认入口在本机管理通道两项未落地）、§8.5（四消息握手、来源绑定、认证前固定上限、单设备替换与撤销传播、心跳与静默超时、出站账本双上限、close code 映射，并如实登记业务命令面未接线）、§9.6 末与 §9 末（cursor 判定、增量重放与 barrier、快照与摘要、ACK 已落地，§9.6 末条的 imported 回源投递**仍是计划**并注明缺口的接缝）。wire 形状、schema 与 fixture 均未变。
> 修订记录（2026-10-15，`sync-server-commands` 文档写回，无 wire 变更）：按 Daemon 侧实绩写入 §9.4/§9 末与 §10.3 的实现状态注记——业务命令面（§11.2 的五态 `command.result`、§11.3 的五条查询、§11.4 的 `command.status`、§11.5 的 12 条 Sync 命令）已接线并覆盖 mutation 的接受/幂等/版本冲突/终态观察者与 `session.create`；同时**如实登记本期降级**：`session.read` 的 `turns`/`pending_interactions`/`capabilities` 三类与 imported 会话明细一律回 `capability.unsupported_by_broker`（§9.4 说明「客户端 MUST NOT 当成会话无内容或已到最早一条」），§11.5 的 `session.read` 行同步标注。wire 形状、schema 与 fixture 均未变。
> 适用范围：Daemon 与 PWA，以及后续 Android、iOS 和网络桌面客户端

## 1. 文档职责

本文是 ACP Remote 客户端同步线协议的权威来源，定义：

- HTTPS 配对接口与 WSS 连接生命周期。
- 双向设备认证的消息和 transcript 编码。
- JSON 控制信封、命令、事件、快照、ACK 和错误。
- cursor、重放、幂等、排序、背压和版本兼容规则。
- ACP 原始 payload 在 Sync Protocol 中的承载边界。

本文不定义 ACP 本身的语义、Broker 内部领域模型、SQLite schema、UI 组件或 ACP Remote 节点间协议。Node Link 由 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 独立定义，不能直接复用本文 wire DTO。发生冲突时：

- 产品权限和保留策略以 [INITIAL_DESIGN.md](./INITIAL_DESIGN.md) 为准。
- 模块职责和依赖以 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) 为准。
- PWA 身份与传输安全以 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md) 为准。
- 本文只负责把上述决策落实成可互操作的 wire contract。

规范中的“必须”“不得”“应”“可以”分别表示强制要求、强制禁止、推荐要求和可选行为。

术语说明：Sync v1 的 wire 字段沿用 `hostId`、`hostPublicKey` 和 `hostProof`；在节点化架构中，它们表示当前向客户端提供 Sync 服务的 ACP Remote Node。Node Link 使用独立 Node Identity transcript，不复用这些设备配对消息。

## 2. 设计目标与不变量

### 2.1 第一版目标

- 使用人类可读、浏览器可直接调试的 JSON。
- 支持一个 Daemon 与多个已授权客户端同步已有会话。
- 支持断线重连、持久化游标、至少一次事件投递和命令重试。
- 不要求应用级云服务，不绑定 LAN、Tailscale 或具体反向代理。
- PWA、Rust 和未来原生客户端共享同一份协议 fixture。

### 2.2 ACP 兼容性

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

Sync Protocol 提供适合客户端消费的公共视图，但公共视图不是完整 ACP 消息：

- Broker 已理解的字段进入结构化 `view`。
- 与事件有关的合法 ACP 数据放入 `acp` 容器；原始 JSON document 以 `rawJson` 字符串承载，避免浏览器解析未知 64 位整数时丢失精度。
- 未知 ACP 字段和 Agent 扩展不得因 Rust 或 TypeScript DTO 解码而丢失。
- `rawJson` 解码后的 UTF-8 bytes 必须与 Agent wire message 的 JSON document 一致，不包含 stdio 换行等 transport delimiter；空白、键顺序和数字文本形式均保留。
- 客户端不支持某项能力时必须显示显式降级，不能把结构化事件静默转换成普通文本。
- 客户端不能通过 Sync Protocol 发送任意 ACP JSON-RPC；业务命令必须位于允许的命令集合内。

每一种 ACP 方法、`session/update`、content block 和 capability 在 Sync 层应使用 command、event、snapshot、raw fallback 还是显式不支持，由 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 的机器矩阵逐项约束；本节只定义通用承载规则。

### 2.3 权威性与投递语义

- 对本地 Agent/会话，当前 Daemon 是资源权威；对 imported Agent/会话，Owner Node 是资源权威，当前 Access Node 只对本地身份（会话摘要与授权）负责，不为远程内容维护交付索引、cursor/ACK 或 local sequence（§9.7）。
- 本地资源事件必须先写入当前节点的权威事件日志再广播。远程资源事件已由 Owner 先持久化；Access 只在已认证连接期经有界内存缓冲把来源事件逐字转发给本地客户端，不落盘、不分配 local sequence（§9.7），默认不得复制正文。
- 事件采用至少一次投递，客户端按 `eventId` 去重。
- 同一事件日志实例内，`globalSequence` 严格递增；同一会话内，`sessionSequence` 严格递增。
- `requestId` 保证客户端重试不会创建第二次命令接受或第二次 Agent 派发。
- 不宣称网络或外部 Agent 副作用“恰好一次”；崩溃窗口无法确认时必须返回 `uncertain`，不得自动重复派发。
- 慢客户端不得阻塞 Agent、Broker 或其他客户端。

当前 Sync v1 baseline 通过 feature `resource.remote-origin.v1` 定义 imported resource 的承载方式：`ownerNodeId`、origin cursor、在线状态和 `no-content-cache` 全部由第 9.6 节规定。未协商该 feature 时不得把远程资源暴露给客户端，也不得把远程资源伪装成本地权威会话。协商后 snapshot、replay 和 `session.read` 的正文必须在线回源 Owner，客户端不得持久化正文与 `acp.rawJson`。

## 3. Transport Profile

### 3.1 正式连接

第一版正式 Profile：

```text
HTTPS/WSS + JSON text messages + ECDSA P-256 challenge-response
```

- PWA 从 canonical origin 加载静态资源。
- WebSocket endpoint 为同源 `/sync/v1`；它与 Node Link 的 `/node-link/v1`、两者的配对 HTTP 路径**共用同一个 listener**（`CONFIG_REFERENCE.md` §1 的 `daemon.port`/`daemon.bind`），按 path 路由。
- 客户端必须请求 WebSocket subprotocol `acp-remote.sync.v1.json`。
- 服务端必须选择该 subprotocol，否则客户端终止连接。
- 正式连接只允许 `wss://`；`ws://` 只允许显式开发模式和 loopback。
- WebSocket binary message 在 v1 中不支持，收到后关闭连接。
- v1 不协商 `permessage-deflate`；压缩必须在后续 feature 中明确规定解压后上限和资源防护。
- 一个完整 JSON 文档对应一个 WebSocket message；WebSocket frame 分片由实现层重组，不暴露给协议层。

原生客户端可以连接已配置的多个 endpoint，但每个 endpoint 都必须证明同一个已配对 Host identity。PWA 身份只绑定一个 canonical origin。

实现状态（`openspec/changes/archive/2026-10-15-sync-server-connection`，截至本修订）：本节所定义的传输面**已在 Daemon 侧落地**——`/sync/v1` 与 subprotocol 常量的唯一来源是 `crates/server/src/sync/conn/mod.rs`（`WS_PATH`/`WS_SUBPROTOCOL`，后者与 `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol` 同源），路由注册与绑定 listener 在组合根 `crates/app/src/daemon.rs` 的 `register_sync_paths`（`register_ws`）。接入层按 §3.2 归一化请求来源并把三形态（`Known`/`Absent`/不可归一化）传给连接面；binary 帧关闭连接、不进入协议逻辑（`crates/server/src/sync/conn/session.rs`）。§3.2 的「非浏览器原生客户端可以没有 HTTP `Origin`，此时 transcript 中使用空字节串」一条**在 Sync 面未被采用**：`identity-auth` 对 `SyncDevice` 强制要求 `canonical_origin` 为 `Some`，因此无来源（`Absent`）与非 `https://` 来源一律走拒绝路径，不以空字节串放行（详见 §8.1 的实现状态注记）。

### 3.2 Origin 与 Host 检查

- PWA 请求的 HTTP `Origin` 必须与设备记录中的 canonical origin 完全一致。
- canonical origin 是小写 scheme、浏览器规范化 hostname 和显式有效 port 的组合，不包含 path、query、fragment 或尾部 `/`。
- 默认端口规范化为 `https:443`、`http:80` 后再比较；线上的字符串形式省略默认端口。
- 服务端必须校验 `Origin`、允许的 `Host` 和 TLS 部署配置，不能信任客户端 JSON 中自报的 Origin。
- 非浏览器原生客户端可以没有 HTTP `Origin`，此时 transcript 中使用空字节串，授权仍绑定设备和 Host identity。

### 3.3 内容编码

- JSON 文本使用 UTF-8，不允许 BOM。
- 不允许重复对象键、无效 Unicode、`NaN`、`Infinity` 或尾随内容。
- 所有序号和计数器在线上必须编码为无前导零的十进制字符串（`^(0|[1-9][0-9]*)$`）。v1 中属于该规则的字段包括 `globalSequence`、`sessionSequence`、`connectionSequence`、`cursor.globalSequence`、`originSequence`、`version`、`expectedVersion`、`byteLength`、`deltaIndex` 和 `chunkIndex`；新增同义字段时按同一规则处理。
- v1 的序号上界是 `2^63−1`（实现用 64 位有符号整数承载，见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.2）：超出该值的序号是协议错误（`protocol.schema_invalid`），接收方不得截断、回绕或钳制到边界。Node Link 沿用同一上界。
- 结构性常量保持 integer：`protocolVersion`、`minProtocolVersion`、`maxProtocolVersion`、`selectedProtocolVersion`、`schemaVersion`、`chunkCount`、`heartbeatIntervalMs`、`limit`（Sync 面 `session.read` 的可选页大小）和 `limits.*` 中的字节、条数、连接数上限。schema 里的 `maxItems`、`maxLength` 等校验常量不因该规则变成字符串。
- 时间使用 UTC RFC 3339，精确到毫秒，例如 `2026-09-17T12:10:00.123Z`；毫秒精度是强制的，秒精度或其他精度都不合法。该规则同样适用于配对二维码与配对状态响应中的 `expiresAt`。
- UUID 使用带连字符的小写 canonical 文本；v1 接受 UUIDv4，服务端生成的有序 ID可以使用 UUIDv7，但排序不得依赖 UUID。
- **ACP Remote canonical JSON v1（ACPR-CJ1）** 是为结构化负载定义的确定性序列化，供 `payloadDigest` 一类内容摘要使用：UTF-8 编码、无 BOM、无前后空白；对象成员名按 UTF-16 code unit 升序排列，不允许重复键；字符串使用 JSON 最小转义（`"`、`\` 与 `< U+0020` 的控制字符，控制字符写作小写十六进制 `\u00XX`）；数字必须是整数且 `|n| ≤ 2^53−1`，`view` 与 interaction payload 内禁止浮点；顶层必须是 object 或 array。内容摘要按 `base64url(SHA-256(ACPR-CJ1(value)))` 计算，接收方不得用自己重新序列化的结果替代。  
> 实现口径：ACPR-CJ1 在 Rust 侧只有一份实现（叶子 crate `acpr-wire::cj1`，`docs/adr/0007-shared-wire-value-crate.md`），`scripts/check-contract-assets.mjs` 里的 JS 版本是它的对照参考；两者必须逐字节一致（2026-09-18 对齐过一次：JS 原先用 `JSON.stringify` 的 `\t`/`\n` 短转义，违反本条「控制字符写作小写 `\u00XX`」，已改为手写转义器）。`payloadDigest`/`snapshotDigest` 的写入方（存储层）与校验方都调用这一份实现。
- 二进制字段使用无填充 base64url，不得使用标准 base64 的 `+`、`/` 或 `=`。
- 普通 JSON 不做 canonicalization，也不直接用于签名或 HMAC。

### 3.4 附件传输连接（独立已认证 WSS）

附件内容不占用业务消息连接：协商 `core.attachment-transfer.v1` 后，客户端另建一条已认证 WSS 连接专门承载附件帧。

- **端点与认证**：`wss://<host>/sync/v1/attachments`，subprotocol 仍为 `acp-remote.sync.v1.json`（§3.1）。连接必须完整执行 §8 的 challenge-response，沿用同一设备身份、Origin/Host 校验、撤销与心跳规则。认证完成前只接受认证消息（附件帧按 §4.2 的未知/阶段不符规则拒绝）；认证完成后，`selectedFeatures` 里没有 `core.attachment-transfer.v1` 的连接收到任何 `attachment.*` 帧时回 `attachment.negotiation_required` 并以 `4400` 关闭，且**不触碰存储**。认证状态不跨连接共享，也不引入长期 bearer 或公开下载 URL。
- **接线状态（实现事实，2026-10-20 WP5）**：服务端的连接面、协商门控与帧族已实现（`server::sync::attachments`），组合根在装配附件传输端口时把 `/sync/v1/attachments` 注册到 listener，并**同时**把 `core.attachment-transfer.v1` 纳入 `SUPPORTED_FEATURES`；未装配存储时不注册端点、并用 `feature_list_without` 从握手集合里去掉它（宣告一个必然 404 的端点就是能力虚报）。未协商的连接发 `attachment_ref` 会按 §5.3 被拒绝。
- **协商与隔离**：连接在 `selectedFeatures` 中声明 `core.attachment-transfer.v1` 后才进入附件帧族。未协商时新帧族、`attachment_ref` 内容块与 `attachment` 相关错误都不得出现（§5.3）。
- **绑定**：每一帧都重复 `transferId`，`begin`/`manifest_begin` 携带 `sessionId` 与 `subject`。传输只能被创建它的同一主体、同一会话使用；跨会话、跨主体或跨设备复用标识返回 `attachment.binding_mismatch`，不泄露内容也不触发任何 Agent 执行。清单页同样逐条校验：`manifest_page` 里的每个 `transferId` 必须已封存、且会话与主体都与该清单的 `manifest_begin` 一致，否则整页拒绝（不部分登记）。读取（`attachment.read`）同样先校验身份、会话与附件归属。
- **prompt 绑定（实现事实）**：带 `attachment_ref` 的 `session.prompt` 在派发前要求本连接已协商 `core.attachment-transfer.v1`、携带已封存的 `submissionId`，并逐块比对清单里的 `attachmentId`/`sha256`/`byteLength`；任一不符返回 `attachment.negotiation_required`/`protocol.schema_invalid`/`attachment.binding_mismatch`，不派发 Agent。只有 `submissionId`（全部是文本块）时同样先复核清单的封存状态与归属。
- **分块**：单块原始数据与偏移规则见下方预算表；`offset`、`length`、`totalLength`、`maxLength` 一律是无前导零十进制字符串。客户端按期望偏移提交：同偏移同内容重复提交返回原 `chunk_ack`（`duplicate: true`），同偏移不同内容返回 `attachment.chunk_conflict`（`details.offset`），非期望偏移返回 `attachment.offset_mismatch`（`details.expectedOffset`/`actualOffset`，可重试）。每传输同时最多一块未确认。
- **封存**：`attachment.seal` 携带完整长度与整段原始字节的流式 SHA-256；长度或摘要不符返回 `attachment.digest_mismatch`，且不产生可引用附件。重复封存相同内容返回原 `attachment.sealed`，不同内容返回 `attachment.sealed` 错误。
- **清单**：大清单不塞进单帧。`attachment.manifest_begin` 声明 `submissionId`、会话、主体与总数；`attachment.manifest_page` 每页最多 64 个 `transferId` 并以 `final` 标记最后一页；`attachment.manifest_sealed` 给出 `manifestDigest`。`manifestDigest` 按**清单顺序的 `transferId`** 计算：`SHA-256( Σ u32be(len(id)) || id )`，结果是无填充 base64url；服务端重新计算并逐字比较，客户端因此能用它能看到的标识自行复算。`session.prompt` 只携带该已封存 `submissionId` 的引用（§11.5），服务端在接受前重新核对清单完整性、身份、会话、路由与 Agent 能力。清单草稿只存在进程内（断线后重放 `manifest_begin` + 各页即可重建，重复页幂等）。
- **读取边界**：`attachment.read` 的 `offset` 必须指向仍有字节的位置。`offset == totalLength`（EOF 处再读一次）与 0 字节附件都读到空数据，而 `read_chunk.data` 至少 1 字节，因此服务端回 `protocol.schema_invalid`（不可重试）而不是认领后不发回帧；客户端按 `read_chunk.eof` 停止读取，不得把空块当成合法回复。
- **状态不等于受理**：`attachment.sealed`/`attachment.status` 只表达传输事实；消息仍要单独提交，`accepted` 才表示命令被受理，二者都完成前 Agent 不会收到内容。
- **活动传输预算的释放**：单设备 2 / 单节点 8 的活动传输计数在封存、取消、失效时释放，并在附件连接关闭时释放该连接预留的槽位——客户端放弃上传（断线/崩溃/主动关闭）不会把该主体的预算永久锁死。计数只存在于进程内；持久化的半成品由 24 小时清理负责。
- **撤销与过期**：设备或节点撤销时，`close_device`/`close_node` 同时扫描业务表与附件表并以 `4410` 关闭相关连接，正在进行的上传/读取随连接立即中止；已提交附件仍按消息保留策略处理，不因撤销被删除。最近活动不足 24 小时的传输与未提交清单不被清理；超时未提交后该 `transferId` 不再可读，服务端按 `attachment.transfer_not_found` 处理（`attachment.expired`/`attachment.revoked` 是登记的错误码，但本实现不主动发送它们——撤销走关闭连接，过期归入不存在）。已提交附件随所属消息保留与清理（见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.5）。
- **失败语义**：磁盘不足、摘要不符或超预算都显式失败，MUST NOT 截断后冒充完整内容，也不得因此阻塞业务连接上的事件交付。
- **机器合同**：帧名与预算的唯一跨协议登记是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 的 `transportFrames`，形状见 `schemas/sync/v1/attachment.schema.json`；`check:contract-assets` 断言帧表、预算表与它逐项相等。

#### 帧族（`attachment.*`）

| 帧 | 方向 | 说明 |
|---|---|---|
| `attachment.begin` | 客户端 → 服务端 | 声明 transferId、会话、主体、总长度、块大小与媒体类型 |
| `attachment.chunk` | 客户端 → 服务端 | 指定偏移的一块原始数据（base64url） |
| `attachment.chunk_ack` | 服务端 → 客户端 | 确认偏移与长度，`duplicate` 表示重复同块 |
| `attachment.seal` | 客户端 → 服务端 | 提交总长度与整段 SHA-256，请求封存 |
| `attachment.sealed` | 服务端 → 客户端 | 封存成功事实、内容寻址 `attachmentId` 与封存时间；`session.prompt` 的 `attachment_ref.attachmentId` 使用该值 |
| `attachment.status` | 双向 | 查询/报告当前传输状态与已接收长度 |
| `attachment.abort` | 双向 | 取消传输并给出封闭原因 |
| `attachment.read` | 客户端 → 服务端 | 请求读取已封存附件的一段 |
| `attachment.read_chunk` | 服务端 → 客户端 | 返回一段原始数据与 EOF 标记 |
| `attachment.manifest_begin` | 客户端 → 服务端 | 开始登记一次提交清单 |
| `attachment.manifest_page` | 客户端 → 服务端 | 分批登记清单中的 transferId |
| `attachment.manifest_sealed` | 双向 | 清单已封存及其摘要 |
| `attachment.error` | 双向 | 结构化附件错误（code 复用 §12.2 词表） |

#### 附件传输预算

| 项目 | 固定值 |
|---|---:|
| 单块原始数据 | 32768 bytes |
| 每传输未确认块 | 1 |
| 单设备活动传输 | 2 |
| 单节点活动传输 | 8 |
| 每连接附件帧预算 | 6000 帧 / 60 秒（固定窗口） |

超出活动传输或帧预算都回 `attachment.budget_exhausted`（可重试，`retryable: true`）；帧预算超限**不关闭**连接——附件是批量数据，关连接会让已传完的块全部作废，而错误本身已经给出退避信号。帧预算按附件连接独立计数（同一条连接上的业务与附件帧共用一个出站序号，但独立连接天然不占业务预算）。

上表是**运行保护**，不是附件数量或大小的产品额度：单连接发送预算与速率按附件传输族独立计数（§14），不占用业务命令的配额；附件总量只受会话/库容量与磁盘实际可用空间约束。

## 4. 通用消息信封

### 4.1 格式

所有 WSS 消息使用同一外层结构：

```json
{
  "protocolVersion": 1,
  "type": "event",
  "messageId": "018f6f89-8a23-7a10-a0d3-f92e6a31d952",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "17",
  "body": {}
}
```

字段规则：

| 字段 | 类型 | 规则 |
|---|---|---|
| `protocolVersion` | integer | 当前选择的 major wire version，v1 固定为 `1` |
| `type` | string | 本文登记的消息类型 |
| `messageId` | UUID | 每条发送消息唯一，用于诊断，不代替 `eventId` 或 `requestId` |
| `connectionId` | UUID | 认证连接 ID；认证前消息省略 |
| `connectionSequence` | decimal string | 每个方向独立，从 `1` 开始严格加一；认证前消息省略 |
| `body` | object | 与 `type` 对应的结构化内容 |

- 认证完成前（`auth.client_hello`、`auth.server_challenge`、`auth.client_proof`，以及认证阶段发生的 `error`）必须省略 `connectionId` 和 `connectionSequence`。
- 认证完成后：`connectionId` 和 `connectionSequence` 必须存在。
- 两个方向分别维护 sequence，不能相互共用计数器。
- sequence 重复、回退、跳号或 connection ID 不匹配均为 `protocol.sequence_invalid`。
- `messageId` 重复不自动重放业务结果；命令幂等只看 `requestId`。

为突出业务字段，本文后续部分消息示例会省略未变化的通用信封字段；实际 wire message 仍必须符合本节完整信封。

### 4.2 未知字段与类型

- v1 控制信封、认证 body、sync body 和 command payload 是 closed object；未知字段返回 `protocol.schema_invalid`。开放扩展只允许出现在本文明确标记的 `event.payload.view`、错误 `details` 和 ACP `rawJson` 中。
- 未知 `type` 必须返回 `protocol.type_unsupported`，不得静默丢弃。
- 命令名不在 v1 接受的命令集合内时返回 `command.unsupported`，不回 `protocol.schema_invalid`；服务端必须先按 `command` 字段分派，再校验 payload 形状。`command` 字段缺失或不是字符串才属于 schema 错误。
- `body.requiredFeatures` 中任一 feature 未协商时，接收方必须返回 `protocol.feature_required`；`details.features` 列出未被协商的 feature ID（排序去重），发送方必须给出，客户端据此关闭对应能力后重连，不得忽略该错误继续使用。
- 认证、授权、cursor、sequence 和尺寸字段不得靠“忽略未知”进行宽松解释。
- ACP `acp` 容器的未知字段必须保留；控制信封的未知字段不承诺往返保真。
- 服务端不得主动发送客户端未通过 feature 协商表示可处理的新控制 `type`；开放的 `eventType` 是例外，客户端按未知事件降级规则处理。

### 4.3 Schema 规则

- 本文标为“必须”的字段缺失时返回 `protocol.schema_invalid`。
- 只有明确写为 `T | null` 的字段可以为 `null`；其他可选字段应省略，不用 `null` 代替缺失。
- command payload 是 closed object，未知字段拒绝；event `view` 是可扩展 object，未知字段保留在客户端缓存但不参与安全决策。
- ID、sequence、timestamp、枚举和长度限制必须在 wire DTO 边界验证，不能延迟到 Agent adapter。
- 实现对应消息前，必须在 `schemas/sync/v1/` 增加 JSON Schema Draft 2020-12 文件，并由 Rust/TypeScript 契约测试共同消费。
- JSON Schema 是本文结构化合同的机器可验证表达；若二者不一致，应停止实现并先修正文档与 schema，不能任选其一。

## 5. 版本与 Feature Negotiation

### 5.1 版本规则

- WebSocket 路径和 subprotocol 标识 v1 framing。
- `auth.client_hello` 同时声明 `minProtocolVersion` 和 `maxProtocolVersion`。
- 服务端选择双方支持的最高版本。
- 无公共版本时发送 `protocol.version_unsupported` 并以 `4406` 关闭。
- minor 能力使用 feature ID 演进，不在 v1 内修改既有字段语义。

### 5.2 Feature ID

Feature ID 使用小写 ASCII、点分层级。取值集合是**封闭词表**，由 [`compatibility/features/v1/features.json`](../compatibility/features/v1/features.json) 机器登记；`npm run check` 的 `check:features` 断言本表、registry 与 fixture 中出现的 ID 三者逐一相等。新增或删除 ID 必须先改 registry 再改本表，不得只在代码、schema 或 fixture 里添加。

| feature | 说明 | delivery | 必需 |
|---|---|---|---|
| `core.snapshot.v1` | 快照订阅与 `sync.snapshot_*` 消息族 | mvp | 否 |
| `core.command-status.v1` | 命令终态查询与 `command.result` 的终态字段 | mvp | 否 |
| `core.event-ack.v1` | 客户端 ACK 与断线续传（`sync.ack`） | mvp | 否 |
| `acp.raw-payload.v1` | 事件携带 ACP 原文（`rawAcp.rawJson` 保真，见 §10.3） | mvp | 否 |
| `resource.remote-origin.v1` | imported 资源来源承载（§9.6：`ownerNodeId`/origin cursor/`online`） | mvp | 否 |
| `resource.stateless-origin.v1` | 无持久中转的远程来源流/运行期 ACK（§9.7）：`sync.remote_*` 帧族、Owner origin cursor 直接作为订阅游标、`session.read` 的 `readCursor`/`sessionSummary`/`messageCoverage` 与 `command.status` 的 `remoteTarget`；Access 不持久化远程正文、收据、ACK 或命令状态。全部已接线 | mvp | 否 |
| `core.local-catalog.v1` | 本机目录与 Agent profile 随快照下发（§9.4 的 `workspaces`/`agents` 资源、§10.3 的 `workspaceRef`/`agentCatalogEntry`），以及 `SessionSummary` 的可选 `workspace` 字段；`session.create` 的选择器由它供数。目录条目始终同时带 `directoryId` 与 `path` | mvp | 否 |
| `core.session-create.v1` | Sync 面 `session.create`（§11.5 的目录来源二选一 payload——`directoryId` 或客户端提供的 `workspacePath`——加必填 `agentId`，以及可选的「目录不存在则先创建」开关 `createIfMissing`；`completed { sessionId, session }` 终态结果） | mvp | 否 |
| `core.session-history.v1` | Sync 面按会话读取结构化历史条目：§11.5 的 `include` 取值 `history`、`beforeHistory` 序号游标与结果里的 `history`/`historyHasEarlier`，条目类型集合见 §10.2、明细资源口径见 §9.4 | mvp | 否 |
| `core.event-origin.v1` | 事件帧 `origin` 的身份字段 `nodeId` 与产生者类型 `node`（§10.1） | mvp | 否 |
| `core.workspace-browse.v1` | Sync 面逐层浏览节点目录的查询命令 `workspace.browse`（§11.5 的请求/结果形状与失败分类、§14 的单层条目上限） | mvp | 否 |
| `core.workspace-mkdir.v1` | Sync 独立创建当前父目录下的一层目录 `workspace.mkdir`（§11.5） | mvp | 否 |
| `core.file-diff.v1` | `file.changed` 视图携带结构化差异行 `diffLines` 与不可用标记 `diffUnavailable`（§10.3）：未协商时该键缺席 | mvp | 否 |
| `core.session-files.v1` | Sync 面当前会话工作目录内的只读文件查询：`session.files.list`/`session.files.read` 与 `session.git.status`/`session.git.diff`（§11.5 的请求/结果形状、§14 的分页与内容预算）；只登记 Sync，不进入 Node Link | mvp | 否 |
| `core.session-git-stats.v1` | `session.git.status` 的可选 `addedLines` / `deletedLines` 行数统计；依赖 `core.session-files.v1`，未协商时不发送这两个字段 | mvp | 否 |
| `core.session-git-compare.v1` | `session.git.diff` 可附两份完整文本 `comparison: { before, after }`，用于 HEAD／工作文件双栏对比；依赖 `core.session-files.v1`，未协商时省略该字段 | mvp | 否 |
| `core.attachment-transfer.v1` | 独立已认证附件 WSS 连接（§3.4）：`attachment.begin`/`chunk`/`chunk_ack`/`seal`/`sealed`/`status`/`abort`/`read`/`read_chunk`/`manifest_begin`/`manifest_page`/`manifest_sealed`/`error` 帧族；32 KiB 单块、十进制字符串偏移、每传输一块未确认、单设备 2/单节点 8 个活动传输；未协商时新帧族与 `attachment_ref` 必须缺席 | mvp | 否 |
| `core.typed-media.v1` | 类型化媒体与消息通道（§10.4）：`AgentContentBlock` 的 `audio_ref`/`file_ref` 与 `image_ref.attachmentId`、`messageChannel`、`partialResult` 与媒体引用可用性；完整 ACP 原文仍逐字节保留 | mvp | 否 |
| `core.pending-interactions.v1` | 可恢复的有效待处理交互（§10.4/§11.7）：`session.read` 的 `pending_interactions` 返回完整当前请求与 `requestVersion`，应答前重新校验有效性，首个有效答案胜出；已处理/失效请求不可复活 | mvp | 否 |
| `core.remote-history.v1` | 远程历史读取（§9.6/§11.5）：imported 会话的结构化历史按会话内序号独立分页、读取前重新鉴权、正文由 Owner 回源提供；不可达或超出保留期显式不可用，Access 不持久化正文 | mvp | 否 |
| `core.session-activity.v1` | 会话主状态三态合同（§10.2）：`sessionSummary.state` 只有 `idle`/`running`/`pending`——存在有效未解决交互时为 `pending`，否则存在未结束任务（含已接受的排队任务）时为 `running`，否则为 `idle`；create/read/list/快照/`session.updated` 视图与 Node Link 的 `sessionMeta`、catalog 会话共用同一取值集合。排队、等待原因与任务/命令结果不属于本 feature；旧七态只允许作为历史事件 `view` 的原文出现 | mvp | 是 |

- 本表的二十一个 feature 构成 v1 baseline，实现必须全部支持（`delivery = mvp`；`conditional_mvp` 表示端到端通路接线后才宣告，未接线前客户端必须按不支持降级）。
- **必需 feature 是双向拒绝点**：`core.session-activity.v1` 要求每次协商都选中。客户端 MUST 在 `supportedFeatures` 与 `requiredFeatures` 中声明它，服务端 MUST 在 `selectedFeatures` 中返回它；任一侧缺失时连接不得进入业务阶段——服务端对未声明该 ID 的连接返回 `protocol.feature_required`（`details.features` 列出缺失的 feature ID，排序去重），客户端对未被选中的必需项停止同步并提示版本不兼容。该 feature 未选中前 MUST NOT 发送或接收三态会话摘要、快照 chunk 与事件；MUST NOT 回退到七态，也 MUST NOT 把 `pending` 折算成某个 `waiting_*`。
- **本版服务端宣告 20 条**：`auth.server_challenge.selectedFeatures` 只返回已完整接线的 ID（二十一条减去尚未接线的 `core.remote-history.v1`）。
  - `core.remote-history.v1`：远程历史需要 Access 侧的 `node-link-client` 回源路径（尚未实现）。宣告一个当前必然失败的读取路径就是能力虚报。协商前 imported 会话的 `history` 仍回 `capability.unsupported_by_broker`，客户端必须明确显示不可用而非「没有历史」。接线后（WP5）再把它加入宣告表并补回源用例。
  - `resource.stateless-origin.v1`：运行期目录/来源端口、`sync.remote_*` 路由与客户端消费已接线；宣告后旧客户端据 `selectedFeatures` 缺失按「远程来源流不支持」明确降级，不回退到任何本地落盘中转。
  - `core.session-activity.v1`：三态映射、交互归约、`session.updated` 生产与持久迁移已接线；服务端在握手支持集合与必需集合中都登记它，任一侧缺失即拒绝进入业务阶段。
  - `core.attachment-transfer.v1`：连接面、帧族与引擎已实现（§3.4）；组合根装配附件传输端口时注册 `/sync/v1/attachments` 并把该 feature 纳入握手集合，未装配时不注册端点、不宣告。
- `必需` 列为"是"表示协议要求每次协商都必须选中该 feature；Sync v1 当前只有 `core.session-activity.v1` 是必需 feature。
- 客户端发送支持列表和必需列表。
- 服务端返回已选择的交集。
- 列表去重后按 UTF-8 字节升序排列。
- 认证 transcript 中的 feature bytes 为排序后 ID 逐项以 `0x00` 分隔的 UTF-8 字节；ID 本身不得包含 NUL。
- 客户端必需 feature 未被选择时，认证不得进入业务阶段。

### 5.3 旧 peer 与未协商行为

除唯一必需的 `core.session-activity.v1`（三态主状态，见下）外，新能力一律是**加性 feature**：未协商时行为与 v1 baseline 逐字相同，旧 peer 既不发送也不接收新封闭字段或新帧族。

- **服务端（新）面对旧客户端**：`auth.server_challenge.selectedFeatures` 只返回旧客户端声明过的 ID；旧连接不会收到 `attachment_ref`/`workspace_path_ref` 内容块、`audio_ref`/`file_ref` 内容块、`requestVersion`、`pending_interactions` 或 `media` 等新键——服务端省略这些键，而不是下发 `null`、空数组或占位对象（封闭 schema 不接受未协商字段）。
- **旧服务端面对新客户端**：新客户端只在 feature 被选中后才启用入口；未选中时附件、媒体、可恢复交互与远程历史入口显式不可用并说明原因，MUST NOT 退化为自动文本化附件、以空历史冒充成功或把失败答复当作已处理。
- **未协商却出现新帧/新字段**：业务连接收到 `type` 不在本连接协商集合内的消息（含任何 `attachment.*` 帧）返回 `protocol.type_unsupported`；收到未协商的封闭字段按 `protocol.schema_invalid` 拒绝；附件连接在协商前收到附件帧返回 `attachment.negotiation_required` 并以 `4400` 关闭。任何一侧都不得静默忽略该消息后继续；重复的错误计数按 §14 的认证前/连接速率规则处理。
- **降级不是静默改写**：未协商 `core.typed-media.v1` 时，无法表达的非文本内容仍按 §10.3 的 `unsupported` block 或未知事件降级，并保留 `acp.rawJson`；未协商 `core.remote-history.v1` 时，imported 会话的历史读取返回 `capability.unsupported_by_broker`，客户端明确说明不可用，而不是显示「没有历史」。
- **三态合同是必需 feature，不是加性降级**：`core.session-activity.v1` 未由双方选中时连接不进入业务阶段（§5.2）。旧服务端面对新客户端：`selectedFeatures` 里没有该 ID，客户端 MUST 停止同步并提示版本不兼容，MUST NOT 按七态摘要自行折算或把 `pending` 当作某个 `waiting_*`。旧客户端面对新服务端：`auth.client_hello` 未声明该 ID，服务端 MUST 以 `protocol.feature_required` 拒绝，且 MUST NOT 向该连接下发三态摘要。
- **远程来源流是加性 feature**：未协商 `resource.stateless-origin.v1` 时，新连接既不发送也不接收任何 `sync.remote_*` 类型、`session.read` 的 `readCursor`/`sessionSummary`/`messageCoverage` 或 `command.status.remoteTarget`；收到未协商的远程帧按 `protocol.type_unsupported` 拒绝，收到未协商的封闭字段按 `protocol.schema_invalid` 拒绝。服务端 MUST NOT 用旧的本地落盘中转来「兼容」，客户端也不得把不支持静默降级成本地缓存（§9.7）。
- **不新增 second version**：以上全部在 v1 内通过 feature 演进；提升 `protocolVersion` 才允许改变已协商字段的语义（§16）。

## 6. 密钥与二进制表示

### 6.1 算法

- Host 和 Device identity：ECDSA P-256 + SHA-256。
- 公钥：SEC1 uncompressed point，固定 65 bytes，`0x04 || X(32) || Y(32)`。
- 签名：IEEE P1363，固定 64 bytes，`r(32) || s(32)`。
- pairing proof 与配对状态 HMAC：HMAC-SHA256。
- nonce：CSPRNG 生成的 32 bytes。
- `pairingSecret`：CSPRNG 生成的 32 bytes。

上述字节在线上均使用无填充 base64url。DER 签名不得出现在 wire protocol 中；若底层库产生 DER，实现必须在边界显式转换并验证长度。

### 6.2 Transcript Codec v1

所有签名和 HMAC 输入使用以下二进制格式：

```text
magic          4 bytes   ASCII "ACPR"
codecVersion   u8        0x01
domainLength   u16be
domainTag      bytes     UTF-8
fieldCount     u16be
fields         repeated {
  fieldTag     u16be
  byteLength   u32be
  rawBytes     byteLength bytes
}
```

规则：

- 字段按 `fieldTag` 严格递增，只能出现一次。
- 字符串为未加 NUL 的 UTF-8 原始字节。
- UUID 解码为 16 个原始字节后进入 transcript。
- protocol version 使用 `u16be`，Unix 时间使用 `u64be`。
- public key、nonce 和 hash 使用原始字节，不使用 base64url 文本。
- canonical origin 使用规范化后的 ASCII URL origin 文本。
- 不允许省略规范标为 required 的字段，也不允许加入未登记字段。
- 验证方必须先检查长度和类型，再执行密码学操作。
- P-256 公钥必须验证为曲线上非无穷点；签名的 `r`、`s` 必须位于有效范围。为兼容 WebCrypto，v1 验证方接受合法 high-S 和 low-S 签名，不得把 signature bytes 当作唯一业务标识。

### 6.3 Domain 与字段 Tag

Domain：

| 用途 | domainTag |
|---|---|
| 配对 claim HMAC | `acp-remote/pairing-proof/v1` |
| 配对 Host 签名 | `acp-remote/pairing-host-proof/v1` |
| 配对状态 HMAC | `acp-remote/pairing-status/v1` |
| 连接 Host challenge | `acp-remote/host-challenge/v1` |
| 连接 Device proof | `acp-remote/device-proof/v1` |

全局字段 Tag：

| Tag | 名称 | 编码 |
|---:|---|---|
| 1 | `protocolVersion` | u16be |
| 2 | `hostId` | UUID 16 bytes |
| 3 | `deviceId` | UUID 16 bytes |
| 4 | `pairingId` | UUID 16 bytes |
| 5 | `pairingExpiresAt` | Unix seconds u64be |
| 6 | `canonicalOrigin` | UTF-8 |
| 7 | `hostPublicKey` | SEC1 65 bytes |
| 8 | `devicePublicKey` | SEC1 65 bytes |
| 9 | `clientNonce` | 32 bytes |
| 10 | `serverNonce` | 32 bytes |
| 11 | `connectionId` | UUID 16 bytes |
| 12 | `negotiatedFeatures` | 排序且以 NUL 分隔的 UTF-8 bytes |
| 13 | `pairingRequestId` | UUID 16 bytes |
| 14 | `deviceName` | UTF-8，最多 128 bytes |
| 15 | `clientKind` | UTF-8 枚举值 |
| 16 | `requestNonce` | 32 bytes |

字段 15 `clientKind` 的 v1 取值：

| `clientKind` | 含义 |
|---|---|
| `pwa` | 浏览器 PWA（含安装到主屏幕的实例） |
| `android` | Android 原生客户端 |
| `ios` | iOS 原生客户端 |
| `desktop` | 桌面原生客户端（Rust CLI、Tauri 桌面壳等） |

每个取值对应独立的设备记录与 `deviceId`；不允许用同一 `clientKind` 值掩盖多个安装实例。

各用途字段集合：

- pairing proof：`1,2,3,4,5,6,8,9,14,15`。
- pairing Host proof：`1,2,3,4,6,7,8,9,10,13`。
- Host challenge：`1,2,3,6,9,10,11,12`。
- Device proof：`1,2,3,6,9,10,11,12`。
- pairing status：`1,2,3,4,13,16`。

Host challenge 和 Device proof 字段相同但 domain 不同，避免签名跨角色复用。固定的十六进制 transcript、HMAC 和签名测试向量必须在实现协议 crate 时作为 fixture 提交；没有共享 fixture 不得宣称 Rust/WebCrypto 互操作完成。

### 6.4 固定互操作向量

以下是 Host challenge 的规范验证向量。测试私钥仅用于公开 fixture，不得用于真实设备：

```json
{
  "protocolVersion": 1,
  "hostId": "00010203-0405-4607-8809-0a0b0c0d0e0f",
  "deviceId": "10111213-1415-4617-9819-1a1b1c1d1e1f",
  "canonicalOrigin": "https://pc.example.test",
  "clientNonceHex": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  "serverNonceHex": "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
  "connectionId": "20212223-2425-4627-a829-2a2b2c2d2e2f",
  "negotiatedFeatures": [
    "acp.raw-payload.v1",
    "core.command-status.v1",
    "core.event-ack.v1",
    "core.snapshot.v1"
  ]
}
```

预期值：

```json
{
  "transcriptBase64url": "QUNQUgEAHGFjcC1yZW1vdGUvaG9zdC1jaGFsbGVuZ2UvdjEACAABAAAAAgABAAIAAAAQAAECAwQFRgeICQoLDA0ODwADAAAAEBAREhMUFUYXmBkaGxwdHh8ABgAAABdodHRwczovL3BjLmV4YW1wbGUudGVzdAAJAAAAIAABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fAAoAAAAgICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8ACwAAABAgISIjJCVGJ6gpKissLS4vAAwAAABMYWNwLnJhdy1wYXlsb2FkLnYxAGNvcmUuY29tbWFuZC1zdGF0dXMudjEAY29yZS5ldmVudC1hY2sudjEAY29yZS5zbmFwc2hvdC52MQ",
  "transcriptSha256Hex": "63fd636a148723e992fd2adefa5ef524ee8e0293cfb60b32ae65a95a7cd796eb",
  "publicKey": "BFJtc8UddTaNGZ6P9Dmvpr5IhKZbCihAwZ-K6LY-gJBqKIb_fCSFjJtFWkQbO-AGx0AXaZPhwTZbaa5qYnYv4eo",
  "privateJwk": {
    "kty": "EC",
    "crv": "P-256",
    "x": "Um1zxR11No0Zno_0Oa-mvkiEplsKKEDBn4rotj6AkGo",
    "y": "KIb_fCSFjJtFWkQbO-AGx0AXaZPhwTZbaa5qYnYv4eo",
    "d": "viq2e67z5g1bU-m9xHVTPc-pNneAeKyl8XC1rRjTmYc"
  },
  "p1363Signature": "Ibs4zNLWQ0f6ne4D--LdxQI2poA7yVY1QFCIeIhZRa4BHja45SrQeFd94_KfJvp_N_JdzhKdAXaWkI_9YNiQog"
}
```

实现必须能生成相同 transcript 和 SHA-256，并用 public key 验证固定 64-byte P1363 signature。ECDSA 签名本身允许随机 nonce，因此重新签名不要求产生相同 signature，但必须能被另一端验证。

## 7. 首次配对

### 7.0 配对状态机

```text
created
  -> claimed                  瞬时状态：claim 已通过校验并被原子占用
  -> pending_confirmation     等待电脑本地确认
  -> approved | rejected | expired
approved
  -> consumed                 设备首次 WSS 认证成功
```

规则：

- 一条 pairing 只允许从 `created` 成功 claim 一次；并发 claim 只有一个可以成功。
- `claimed` 只用于服务端事务和审计，外部正常看到的是 `pending_confirmation`。
- 电脑端确认必须通过本地 CLI、loopback UI 或其他已授权电脑入口完成，不提供给未认证的远程 PWA。
- 电脑确认时**不授予任何授权**：设备方向没有授权维度（设备权限层已删除），配对确认页也没有权限栏。PWA 不能在 claim 中申请、扩大或以任何方式影响授权；claim 载荷出现任何授权集合字段 MUST 被拒绝。
- `approved` 后创建设备记录，但 pairing 在该设备首次 WSS 认证成功前保持可查询。
- 首次 WSS 认证成功后原子转为 `consumed`；此后任何二维码或扫码取回都不能再次创建设备。
- 过期判断以 Daemon 时钟为权威。PWA 的本地时间检查只用于提前提示，不能延长有效期。
- `pairingSecret` 最迟在 `expiresAt` 清除；`approved` pairing 在首次 WSS 认证成功时提前清除。拒绝状态为支持可靠轮询可以保留 secret 到原过期时间，但不得超过该时间。
- 扫码取回凭据（`qrToken`）与 `pairingSecret` 分别独立随机生成，只存内存；创建、旧材料替换、内存重置、过期、证明失败失效、拒绝、成功认领与首次认证都必须关闭取回资格。为状态轮询保留 secret **不会**恢复取回资格；重启后不得从持久化摘要恢复凭据。

### 7.1 设备二维码与入口

设备配对有两个入口，共用 §7.0 的同一套状态机与 §7.2/§7.3 的 claim/status：

- **原生 App 扫码（后续交付）**：设备二维码携带协议版本、32 字节扫码取回凭据与 32 字节主机公钥指纹；App 先把 canonical origin 固定在二维码的外层地址上，经 §7.6 的 `qr-lookup` 取回完整配对资料、先核对主机公钥指纹，再走 claim/status。本轮只交付协议类型、机器合同与共享向量，**原生 App 本体、相机、深链与 keystore 不在本切片**。
- **PWA / 桌面浏览器**：`/pair` 无片段时只显示 8 位配对码输入，经 §7.5 的 lookup 取回同一组 `pairingId`/`hostId`/`hostPublicKey`/`canonicalOrigin`/`pairingSecret`/`expiresAt`。PWA **不使用**设备二维码取回接口：打开 `q2=` 片段时只清除片段并显示固定 App 指引，一个配对请求都不发。

配对码与扫码取回凭据都只用于取回 secret，不是 HMAC 密钥，也不构成新的授权维度。`/node-link/pair` 不提供配对码；节点二维码的格式与行为完全不变（见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §13.1）。

#### 7.1.1 新设备短二维码格式

```text
https://<canonical-origin>/pair#q2=<payload>
```

`<payload>` 是**无填充 base64url** 编码的 65 字节数据，编码后恰好 87 个 ASCII 字符：

```text
[ 0x02 ][ qrToken (32 bytes) ][ hostKeyFingerprint (32 bytes) ]
```

- `0x02` 是唯一接受的格式版本；其它版本一律拒绝。
- `qrToken` 是设备创建时由密码学随机源**独立生成**的 32 字节扫码取回凭据；它不是配对码、不是配对秘密，也不是它们的截断或派生值。
- `hostKeyFingerprint` 是主机公钥的**完整** SHA-256 原始 32 字节摘要，不是 hex：`SHA-256(规范 65 字节未压缩 SEC1 P-256 公钥)`。公钥必须先通过既有的长度与曲线合法性检查，再匹配这把摘要。指纹只绑定主机公钥，不单独认证返回的其它字段；匹配之后仍必须验证 `hostProof`、批准响应的同一 `hostId`/公钥以及每次 WSS challenge-response。
- 外层地址必须是 HTTPS、无 userinfo、无 query、路径恰为 `/pair`，且 fragment 只含一个 `q2=` 键。其它路径、重复片段键或额外文本一律按格式不支持处理。
- **旧设备 `#data=` 完整 JSON 载荷已退出**：不生成、不解析、不降级；收到即明确报告格式不支持且不发请求。节点二维码仍使用自己的 `/node-link/pair#data=…`，不得与设备格式互相回退。
- 尺寸预算：固定测试地址 `https://faraday.tail74842e.ts.net:8765/pair` 长 43 字节，加 `#q2=` 4 字节与 payload 87 字节，总长 **134** 字节（该测试地址承诺不超过 150 字节）。相同 L 级纠错、四模块白边下，实心半块字符矩阵不超过 **58 列 × 29 行**。具体编码器矩阵由生成端测试断言；本契约不承诺任意长主机名都落在 150 字节内。
- 设备二维码与扫码取回凭据、完整配对资料 MUST NOT 进入日志、文件、数据库、Cookie、URL query 或可导出状态；客户端读到 `qrToken` 后应尽快清除并在取回成功后丢弃。

实现状态（截至本修订）：`sync-protocol` 已提供 `device_qr` 的严格编解码与 Rust/TypeScript 共享固定向量（`fixtures/sync/v1/device-qr/vectors.json`），机器 schema 已删除旧 `qrPayload`。**服务端 `qr-lookup` 路由、`identity-auth` 的内存凭据登记，以及 PWA/原生消费方仍是计划/待实现**（WP2–WP4）；在它们落地前，旧设备 `#data=` 的前端路径仍存在于当前工作区，不得把本节当作已交付的运行行为。

所有 pairing HTTP 响应必须包含：

```http
Cache-Control: no-store
Pragma: no-cache
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
```

接口只接受同源 `application/json`；不使用 Cookie、HTTP Basic Auth 或 URL query 承载 pairing credential。

### 7.2 Claim

PWA 生成不可导出的设备密钥、`deviceId` 和 `clientNonce`，然后调用：

```http
POST /sync/v1/pairing/claim
Content-Type: application/json
Origin: https://work-pc.example.ts.net
```

```json
{
  "protocolVersion": 1,
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "deviceId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
  "deviceName": "Zhang's Phone",
  "clientKind": "pwa",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "devicePublicKey": "<base64url-65-bytes>",
  "clientNonce": "<base64url-32-bytes>",
  "proof": "<base64url-hmac-sha256>"
}
```

`proof = HMAC-SHA256(pairingSecret, pairing-proof-transcript)`。

服务端必须原子检查 pairing 存在、未过期、仍为 `created`、Host/Origin 匹配和 HMAC 正确。成功后依次进入 `claimed` 和 `pending_confirmation`，不再接受第二个 claim，但只有本机确认后才创建授权设备。

响应：

```json
{
  "protocolVersion": 1,
  "pairingRequestId": "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d",
  "serverNonce": "<base64url-32-bytes>",
  "hostProof": "<base64url-p1363-signature>",
  "status": "pending_confirmation",
  "expiresAt": "2026-09-17T12:15:00.000Z"
}
```

- `hostProof` 是 Host 对 pairing Host proof transcript 的签名；PWA MUST 用二维码里**固定的** Host public key 验证它，MUST NOT 使用响应自带的公钥或跳过校验。验签失败时 MUST 明确失败、停止后续轮询，且 MUST NOT 写入任何已配对主机记录。
- 认领响应与状态响应都不含短验证码：本协议不派生 SAS，也不要求任何一方展示它（服务端不得把某一方算出的结果当作另一方的结果下发）。理由与信任锚见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §5.1。
- 状态响应**没有任何主机签名字段**，因此设备 MUST 在状态为 `approved` 时校验响应里的主机公钥与二维码里的那把**逐字节相同**，并校验 `hostId` 一致；任一项不符 MUST 明确失败，MUST NOT 落盘该主机记录。
- 只有电脑本地用户明确确认后，Daemon 才创建设备记录（设备记录不保存任何授权集合）。
- HMAC 比较使用 constant-time compare。

### 7.3 状态确认

PWA 使用同一 pairing secret 查询状态：

```http
POST /sync/v1/pairing/status
```

请求必须包含：

```json
{
  "protocolVersion": 1,
  "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "deviceId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "pairingRequestId": "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d",
  "requestNonce": "<base64url-32-bytes>",
  "proof": "<base64url-hmac-sha256>"
}
```

proof 使用 pairing status domain 和本协议登记的字段集合。

返回状态之一：

```text
pending_confirmation
approved
rejected
expired
consumed
```

状态查询无论业务状态如何都返回 HTTP 200，业务状态只由 body 的 `status` 表达；`expired` 和 `consumed` 同样是 200，`410` 只用于 claim 端点（见第 7.4 节）。客户端据此决定是进入 WSS 认证还是停止轮询。

`approved` 响应只返回该设备的非秘密元数据（标识与展示名）与该节点的身份材料（`hostId` 与主机公钥），**不返回任何授权集合**；不签发 bearer token。首次正式 WSS 连接仍执行完整 challenge-response。

状态查询在 `pending_confirmation`、`approved` 和 `rejected` 下可以安全重复；每次新的轮询生成新 request nonce，只有网络重试才复用原 nonce，并获得原响应。客户端观察到 `approved` 后开始 WSS 认证；观察到 `rejected`、`expired` 或 `consumed` 后停止轮询。客户端成功认证或终态过期后必须清除二维码 payload 和内存中的 secret。

### 7.4 HTTP 状态和错误

| HTTP 状态 | 使用场景 |
|---:|---|
| 200 | 状态查询成功，包括业务状态 `pending_confirmation/approved/rejected/expired/consumed`；lookup 与 qr-lookup 成功 |
| 201 | claim 首次成功并创建 pairing request |
| 400 | JSON 或字段 schema 无效。**lookup / qr-lookup 例外**：可解析 JSON 的缺字段、未知字段、错误类型与凭据格式错误收敛为 401（lookup `pairing.code_invalid`、qr-lookup `pairing.qr_invalid`，§7.5/§7.6、design D4）；两者的坏 JSON 以及 `Content-Type` 不是 `application/json` 仍是 400 `protocol.schema_invalid` |
| 401 | HMAC proof 无效；lookup 的任何失败——配对码格式/body 形状错误、不存在、已过期、已消耗、已认领、来源或登记绑定不一致——统一为 `pairing.code_invalid`；qr-lookup 的任何失败——凭据格式/body 形状错误、不存在、已过期、已认领、已拒绝、已失效、来源或登记绑定不一致——统一为 `pairing.qr_invalid`；响应不得泄露具体校验差异 |
| 403 | Origin/Host 不允许（claim/status 等既有端点；qr-lookup 的来源不符统一走 401 `pairing.qr_invalid`） |
| 404 | pairing ID 不存在；lookup 与 qr-lookup 不使用该状态（统一 401） |
| 405 | 端点未声明的方法（接入层在处理器之前拒绝；响应仍带该路径注册的默认安全头） |
| 409 | 已被 claim、状态转换冲突或 device ID 冲突 |
| 410 | 仅 claim 端点使用：pairing 已过期或已 consumed |
| 413 | 请求体超过限制（配对 HTTP 的 claim/status/lookup/qr-lookup 同受 16 KiB 上限约束） |
| 429 | 配对尝试被限流（claim、lookup 与 qr-lookup 各自按来源独立计数，status 按配对） |

错误 body 使用与第 12 节一致的 `code/message/retryable/correlationId/details` 结构。网络响应丢失后，客户端可以使用相同 claim 内容重试；如果服务端已接受同一 device/client nonce，应返回原 pairing request，而不是创建第二条记录。不同 claim 内容则返回 `409 pairing.already_claimed`。

实现状态（`openspec/changes/archive/2026-10-15-sync-server-connection`，截至本修订）：§7.0–§7.4 的 HTTP 面**已落地**，实现与唯一入口在 `crates/server/src/sync/pairing.rs`（`CLAIM_PATH`/`STATUS_PATH` 与四个安全响应头常量同源，由组合根 `crates/app/src/daemon.rs` 的 `register_sync_paths` 注册到共享 listener）。已覆盖的实绩：claim 的五项原子校验与「先校验、后提交」的写入时序（失败时配对状态、对端行与信任记录一个字节都不改）、同内容重复 claim 幂等返回原结果而不同内容返回 `409`、`status` 的 proof 校验与一律 200、四个安全响应头同时出现在处理器自身与接入层预处理响应（413/405/Host 400）上、claim 按来源真实连接地址 10 次/分钟与 status 按 `pairingId` 60 次/分钟的固定限流（不可信转发头不参与计数）。

仍**未实现**或已知偏差（如实登记，不得当作已落地）：

- **证明失败 5 次使配对失效只在内存**（`crates/identity-auth/src/state.rs`）：失效没有持久记录，DB 行保持 `created` 直到过期扫描改写成 `expired`。已判定可接受：HTTP 面不伪造 `Actor::LocalCli`，而重启会一并清空 secret、只会更严；持久落账等「本机终态写面」提供不需伪造主体的结算入口后接上（`tasks.md` WP5 任务 5.8）。
- **本地确认入口仍在本机管理通道**：`device.pair.begin/status/confirm/reject` 经 `server::local_admin` 完成（§7.0 的「本地 CLI、loopback UI 或其他已授权电脑入口」），未提供未认证的远程确认路径。
- 配对有效期的时间戳口径按整秒截断（`transcript::unix_seconds`），与参数校验层的毫秒窗口存在已登记的亚秒取值差异（`tasks.md` WP5 任务 5.7）；`expiresInMs` 的语义本文不做放宽。

### 7.5 配对码 lookup

网页入口没有 fragment 时，用 8 位配对码换取配对绑定与 `pairingSecret`：

```http
POST /sync/v1/pairing/lookup
Content-Type: application/json
```

```json
{
  "protocolVersion": 1,
  "pairingCode": "ABCD-EFGH"
}
```

请求体只含这两个键；多余键、缺字段与错误类型同样进入统一失败面（`401 pairing.code_invalid`，不是 400——见 §7.4）。

`pairingCode` 是 8 位 Crockford base32（40 bit，字母表为 `0123456789ABCDEFGHJKMNPQRSTVWXYZ`），大小写不敏感，允许在 4+4 之间带一个 `-`。服务端先归一化再比较：去掉一个 `-`、转大写、`I`/`L` 映射为 `1`、`O` 映射为 `0`；`U` 不在字母表内。配对码由 `pairingSecret` 域分离派生，只是取回凭据，不是 HMAC 密钥。

成功 200：

```json
{
  "protocolVersion": 1,
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "hostPublicKey": "<base64url-65-bytes>",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "pairingSecret": "<base64url-32-bytes>",
  "expiresAt": "2026-09-17T12:15:00.000Z"
}
```

- 成功响应与同一配对的 §7.6 扫码取回响应给出逐字节相同的 `pairingId`、`hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingSecret` 与 `expiresAt`；不返回任何授权集合。
- lookup MUST NOT 推进配对状态、MUST NOT 创建信任。成功后配对仍可被 claim 一次，claim 的校验与 transcript 与扫码取回入口完全相同。
- **成功即消耗配对码**：一次成功 lookup 原子地移除该码，同一码的第二次 lookup 必须统一失败，即使第一次响应在网络上丢失。页面因此按固定失败文案结束，恢复路径是重新创建配对；本版没有 lookup 幂等键。
- 所有失败——格式不符、不存在、已过期、已被认领、已被使用、已被消耗、来源/Host 与登记绑定不一致——都返回同一个 `401` 与同一个错误码 `pairing.code_invalid`。响应形状、message 与耗时不得区分这些情况，也不得回显配对码。
- 网页入口没有带外主机公钥，因此把成功响应的 `hostPublicKey` 作为本次配对的绑定公钥：claim 的 `hostProof` 用它验证，`approved` 状态响应里的公钥必须与它逐字节相同（§7.2/§7.3 的设备侧校验不因入口而放宽）。用户点击「继续配对」明确发起重新配对后，允许在本机批准且上述校验通过时替换同源旧公钥；进页、等待确认或失败时不得覆盖旧记录。
- 限流与 claim **各自独立**：lookup 与 claim 都按来源真实连接地址 10 次/分钟，互不占用额度；status 仍按配对标 60 次/分钟。lookup 失败不指向某一配对，不增加该配对的证明失败计数。
- lookup 与 §7.1 描述的 pairing HTTP 响应使用同一组规则：只接受同源 `application/json`，请求体上限 16 KiB，响应带四个安全响应头与 `no-store`。`pairingCode` 与 `pairingSecret` 不得进入日志、URL query、Cookie 或任何明文持久化字段。

### 7.6 扫码资料取回（qr-lookup）

原生扫码消费方在核对二维码外层 canonical origin 后，用 fragment 里的扫码取回凭据换取完整配对资料：

```http
POST /sync/v1/pairing/qr-lookup
Content-Type: application/json
Origin: https://work-pc.example.ts.net
```

```json
{
  "protocolVersion": 1,
  "qrToken": "<base64url-32-bytes>"
}
```

请求体只含这两个键；`qrToken` 必须恰是 32 字节的无填充 base64url（43 个字符）。`Origin` 是必需输入，必须与该配对登记时的 canonical origin 逐字相同；`Host` 由接入层按既有 Host 策略先拒绝不合法值（沿用既有 400/403 边界，不进入端点处理）。请求体上限 16 KiB。

成功 200：

```json
{
  "protocolVersion": 1,
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "hostPublicKey": "<base64url-65-bytes>",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "pairingSecret": "<base64url-32-bytes>",
  "expiresAt": "2026-09-18T09:14:33.000Z"
}
```

- 成功响应与配对码 lookup **同形**、同源：同一配对的 `pairingId`、`hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingSecret`、`expiresAt` 必须逐字节相同；不返回任何授权集合。
- 取回 MUST NOT 推进配对状态、MUST NOT 消耗网页配对码、MUST NOT 创建配对请求或信任，也不延长有效期。只在该配对仍为 `created`、未过期、未被本机拒绝/失效、且来源与身份绑定一致时成功。
- 同一 `qrToken` 在有效且未认领期间可以**重复**取回同一组资料（响应丢失后可重试）。任一入口的首次成功认领关闭后续取回；为状态轮询保留 secret 不恢复取回资格。客户端在成功认领后复用已取得的资料做幂等重试，不重新取回。
- 所有端点内失败——可解析 JSON 的未知键/类型错误、凭据编码/长度、凭据不存在、已过期、已认领、已拒绝、已失效、来源或登记绑定不一致——统一返回同一个 `401` 与 `pairing.qr_invalid`。响应形状、message 与耗时不得区分这些情况，也不得回显凭据或材料。非 JSON、请求体超限、方法错误、接入层 Host 拒绝与限流仍使用既有 HTTP 边界（400/405/413/403/429）。
- 限流：qr-lookup 按真实连接来源地址 **10 次/分钟**，与配对码 lookup、claim 各自独立计数，互不占用额度。qr-lookup 的失败不指向某一配对，不增加该配对的证明失败计数。429 必须带 `Retry-After`。
- 与其它 pairing HTTP 响应相同：四个安全响应头（`Cache-Control: no-store`、`Pragma: no-cache`、`Referrer-Policy: no-referrer`、`X-Content-Type-Options: nosniff`）必须出现在处理器与接入层预处理响应上；不得记录请求/响应正文。

原生消费方合同：

- 固定二维码外层地址的 canonical origin；取回资料的 `canonicalOrigin` MUST 与目标一致。所有 HTTPS 请求与 WSS 连接使用同一目标 Origin，HTTP 重定向直接失败，不按响应改写目标或 public key。
- 先校验 `hostPublicKey` 是规范 65 字节未压缩 SEC1 P-256 公钥，并完整匹配二维码里的 32 字节 SHA-256 指纹，再接受其余绑定；指纹不符、换来源或换公钥一律停止，不发送 claim、不建立信任、不自动改用配对码或旧二维码。
- 指纹匹配后仍必须验 `hostProof`、校验批准响应同一 `hostId`/公钥、完成本机确认，并对每次 WSS 连接执行完整 challenge-response。
- 暂时网络故障、超时与 502/503/504 最多尝试 3 次、总墙钟预算 30 秒（退避 500 ms/1000 ms，单请求最长 8 秒）；429 按 `Retry-After`，超出预算停止；取得 `expiresAt` 后同时受其约束，任何重试不续期。格式、来源、指纹、认证失败立即停止。初始常量可在原生交付前调整，但本轮合同测试使用这些上界。
- 本轮状态：端点、身份内存材料、限流与 PWA/原生消费方为计划/待实现（WP2–WP4）；协议层解码、指纹向量与错误词表已在 WP1 冻结。

## 8. WSS 认证

认证阶段只允许本节消息。认证完成前收到业务消息，服务端以 `4401` 关闭。

### 8.1 Client Hello

```json
{
  "protocolVersion": 1,
  "type": "auth.client_hello",
  "messageId": "ed93263a-3628-4668-82aa-c0f551589fec",
  "body": {
    "minProtocolVersion": 1,
    "maxProtocolVersion": 1,
    "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
    "deviceId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
    "clientKind": "pwa",
    "clientNonce": "<base64url-32-bytes>",
    "supportedFeatures": [
      "acp.raw-payload.v1",
      "core.command-status.v1",
      "core.event-ack.v1",
      "core.session-activity.v1",
      "core.snapshot.v1"
    ],
    "requiredFeatures": [
      "core.event-ack.v1",
      "core.session-activity.v1"
    ]
  }
}
```

服务端从已验证 HTTP Origin 获取 canonical origin，不使用 body 自报值。`clientNonce` 每次连接重新生成，不得复用。

### 8.2 Server Challenge

```json
{
  "protocolVersion": 1,
  "type": "auth.server_challenge",
  "messageId": "cbb91891-8f84-4b47-bdf3-2c902c338367",
  "body": {
    "selectedProtocolVersion": 1,
    "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
    "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    "serverNonce": "<base64url-32-bytes>",
    "selectedFeatures": [
      "acp.raw-payload.v1",
      "core.command-status.v1",
      "core.event-ack.v1",
      "core.session-activity.v1",
      "core.snapshot.v1"
    ],
    "hostProof": "<base64url-p1363-signature>"
  }
}
```

`hostProof` 对 Host challenge transcript 签名。客户端必须使用已配对 Host key 验证，并检查 Host ID、device ID、Origin、两个 nonce、connection ID、版本和 features 全部匹配。

Host key 与已配对记录不一致时进入 `identity_changed`，不得询问后自动接受新 key。

### 8.3 Client Proof

```json
{
  "protocolVersion": 1,
  "type": "auth.client_proof",
  "messageId": "ec38c544-662b-4fcb-a011-b5135cb9bc63",
  "body": {
    "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    "deviceId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
    "deviceProof": "<base64url-p1363-signature>"
  }
}
```

`deviceProof` 对 Device proof transcript 签名。Daemon 必须检查设备存在、未撤销、公钥匹配、Origin 匹配和 proof 有效。

### 8.4 Authenticated

```json
{
  "protocolVersion": 1,
  "type": "auth.authenticated",
  "messageId": "8a536862-9a18-4766-b4ce-a2e478734980",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "1",
  "body": {
    "deviceId": "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
    "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
    "headGlobalSequence": "2318",
    "heartbeatIntervalMs": 30000,
    "limits": {
      "maxMessageBytes": 1048576,
      "maxPromptBytes": 262144,
      "maxReplayEventsPerBatch": 500
    }
  }
}
```

`auth.authenticated` 只承载已验证事实（设备标识、服务端 epoch 与游标、心跳与 limits）：设备方向**不携带任何授权集合**，也不含任何「允许此命令」的判定——通过认证即具备执行业务命令的前提（设备权限层已删除，见 `SECURITY_DESIGN.md` §10）。

认证状态只属于当前 WSS 连接，关闭后立即失效。协议不签发 session token 或 refresh token。

### 8.5 单设备连接规则

v1 对每个 `(hostId, deviceId)` 只保留一条 active WSS：

1. 新连接必须先独立完成完整认证。
2. 新连接进入 active 后，服务端以 `4411 connection_replaced` 关闭旧连接。
3. 切换窗口中可能重复投递事件，客户端仍按 `eventId` 去重。
4. 旧连接关闭不会撤销设备，也不会使新连接认证失效。
5. 同一浏览器多个标签页应通过 Web Locks、BroadcastChannel 或等价客户端协调减少争抢，但服务端规则始终是最终约束。

收到 `4411 connection_replaced` 的连接必须停止自动重连，直到该客户端重新取得本地连接所有权或用户显式操作，避免多个标签页互相替换形成重连风暴。

每个原生安装和每个 PWA origin profile 应拥有独立 `deviceId`。不得通过共享一个 device identity 来模拟多个独立客户端。

ACK 首先属于连接；服务端可以保存设备 ACK 的最大值用于保留策略和诊断，但新连接恢复位置以客户端在 `sync.subscribe` 明确提交的本地 durable cursor 为准，不能由服务端最大 ACK 擅自推进。

实现状态（`openspec/changes/archive/2026-10-15-sync-server-connection`，截至本修订）：§8.1–§8.5 的认证面**已落地**，实现集中在 `crates/server/src/sync/conn/`（`mod.rs` 的常量、`handshake.rs` 的纯函数、`session.rs` 的会话驱动、`registry.rs` 的连接注册表与出站账本）。已覆盖的实绩：

- 四消息握手经 `identity-auth` 的 `SyncDevice` 入口完成双向 challenge-response；**未知设备照常签发结构完整的挑战**，失败推迟到 proof 阶段并统一为 `auth.proof_invalid`（不用错误类别区分设备是否存在）。
- 来源绑定：来源与设备记录的 canonical origin **逐字**比较；不一致、无来源（`Absent`）、非 `https://` 来源一律拒绝（`auth.origin_mismatch`）；transcript 的 `canonicalOrigin` 取服务端判定值，不取 body 自报值。
- 认证完成前只接受 `hello` 与 `proof` 各一次，收到业务消息以 `4401` 关闭；整个认证在 15 秒内完成（超时 `4408`），首条 `hello` 5 秒窗口与 32 KiB 体上限、单节点并发 WSS 连接 256 均为 §14 的认证前固定值。
- 单设备替换（`4411 connection_replaced`：新连接认证成功后关旧连接、旧连接关闭不撤销设备）与撤销传播（提交后按 `deviceId` 关闭全部活动连接 `4410` 并拒绝后续认证）均已落地；设备撤销钩子由组合根接到真实连接注册表（`crates/app/src/daemon.rs`）。
- 心跳（`control.ping` 30 秒、`control.pong` nonce 回填）与连续 90 秒静默超时；出站账本双上限 8 MiB / 2,000 条，序号分配与入队同一临界区，被高水位拒绝的投递不消耗序号。
- §12.3 的 close code 表（`4400`/`4401`/`4403`/`4406`/`4408`/`4409`/`4410`/`4411`/`4429`/`4500`）与错误 body 的 `code/message/retryable/correlationId/details` 结构按 `compatibility/errors/v1/errors.json` 逐条映射；认证失败留痕（未知设备同样留痕、归因只取持久化身份）由 `core::use_cases` 的认证失败审计入口落库。

业务命令面（§11）与 `server::acp_facade` 仍**未落地**：认证后的消息目前只由订阅面认领 `sync.subscribe`/`sync.snapshot_request`/`sync.ack`，其余 type 以 `protocol.type_unsupported` 显式拒绝（`crates/server/src/sync/subscribe/mod.rs` 的 `MessageRoute` 实现；没有路由认领时 `conn` 给同一错误码）。

## 9. Cursor、订阅与快照

### 9.1 Cursor

持久化 cursor 是二元组：

```json
{
  "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
  "globalSequence": "2280"
}
```

- `serverEpoch` 在事件库首次创建时随机生成，正常重启保持不变。
- 数据库重建、不可兼容恢复或事件历史身份改变时必须生成新 epoch。
- sequence 只在同一 epoch 内有意义。
- 客户端不得把多个 Host 或 epoch 的 cursor 混用。

### 9.2 Subscribe

认证后客户端发送：

```json
{
  "protocolVersion": 1,
  "type": "sync.subscribe",
  "messageId": "a5255ab1-287f-4d55-a9ce-dbb9d567d875",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "1",
  "body": {
    "cursor": {
      "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
      "globalSequence": "2280"
    },
    "scope": "machine"
  }
}
```

`scope: machine` 是订阅范围（v1 只定义这一个取值），不是设备授权。设备方向不再按授权过滤事件：投递集合是「该 cursor 之后的全部已持久化事件」；仍可能出现的序号空洞只来自保留策略（已压缩或已清理的事件），不能据此推断隐藏事件。

`cursor` 类型为 `Cursor | null`；首次同步发送 `null`。cursor 格式非法、epoch 不匹配、sequence 大于当前 head 或已超过保留窗口时一律返回 `sync.cursor_invalid`，`details.reason` 取值 `malformed|epoch_mismatch|beyond_head|cursor_expired`，服务端不得把 cursor 钳制到 head。需要重建 snapshot 时由服务端主动发送 `sync.reset_required`（`reason` 见第 9.4 节），客户端不得仅凭错误码推断 reset，也不得依赖已被删除的独立错误码。

### 9.3 增量重放

cursor epoch 正确且 sequence 仍在保留窗口内时：

1. 服务端从 SQLite 读取 cursor 之后的全部已持久化事件（设备方向不再有按授权的可见性过滤）。
2. 按 `globalSequence` 升序发送 `event`。
3. 与实时 dispatcher 建立无缝 barrier，避免查询和订阅之间漏事件。
4. 发送 `sync.caught_up`，之后继续实时事件。

```json
{
  "type": "sync.caught_up",
  "body": {
    "cursor": {
      "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
      "globalSequence": "2318"
    }
  }
}
```

连接在 replay 期间收到的新事件必须排在 barrier 之后，不能越过较早的持久化事件。

### 9.4 Snapshot

以下情况必须重建 snapshot：

- 客户端没有 cursor。
- `serverEpoch` 不匹配。
- cursor 早于最早可重放事件。
- 客户端缓存版本不兼容。

服务端先发送：

```json
{
  "type": "sync.reset_required",
  "body": {
    "reason": "cursor_expired",
    "snapshotAvailable": true
  }
}
```

`reason` 枚举为 `initial_sync|epoch_mismatch|cursor_expired|cache_incompatible`。`initial_sync` 通常由 `cursor: null` 直接进入，不要求先发送错误。

客户端发送 `sync.snapshot_request`。服务端在一致性读视图中选择快照 cursor，并以它作为 barrier；该 cursor 同时出现在 `sync.snapshot_begin` 和 `sync.snapshot_end` 的 `cursor` 字段里。依次发送：

```text
sync.snapshot_begin
sync.snapshot_chunk (1..n)
sync.snapshot_end
event where globalSequence > 快照 cursor.globalSequence
sync.caught_up
```

消息 body：

```json
{
  "type": "sync.snapshot_request",
  "body": {
    "reason": "initial_sync"
  }
}
```

```json
{
  "type": "sync.snapshot_begin",
  "body": {
    "snapshotId": "8194de43-e213-423d-acf4-2e3549304566",
    "cursor": {
      "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
      "globalSequence": "2318"
    },
    "schemaVersion": 1,
    "chunkCount": 3
  }
}
```

```json
{
  "type": "sync.snapshot_chunk",
  "body": {
    "snapshotId": "8194de43-e213-423d-acf4-2e3549304566",
    "chunkIndex": "0",
    "resource": "sessions",
    "items": []
  }
}
```

```json
{
  "type": "sync.snapshot_end",
  "body": {
    "snapshotId": "8194de43-e213-423d-acf4-2e3549304566",
    "cursor": {
      "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
      "globalSequence": "2318"
    },
    "chunkCount": 3,
    "snapshotDigest": "<base64url-32-byte-sha256>"
  }
}
```

`chunkIndex` 是从 `0` 开始的十进制字符串，按 index 顺序连续递增，服务端必须按 index 顺序发送。`chunkCount` 在 begin/end 中必须一致，且保持 integer；它 MUST 等于本次快照实际下发的 chunk 数，取值在 `1..=3` 之间——上界是 3（可下发资源的种类数，见下），下界是 1：`sessions` chunk 恒发，即使会话为空也照发一个空数组，因此 `chunkCount: 0` 是形状错误。示例中的 `3` 是协商了 `core.local-catalog.v1` 时的一次完整快照（每种资源一个 chunk）；未协商该 feature 时目录资源 MUST NOT 发送，`chunkCount` 为 `1`。服务端 MUST NOT 声明多于实际发送的 chunk 数，客户端发现 chunk 数与 `chunkCount` 不符时按本节丢弃整个暂存 snapshot。`snapshotDigest` 的计算方式是：对每个完整 `sync.snapshot_chunk` WebSocket message 的原始 UTF-8 bytes 分别计算 SHA-256，按 chunk index 连接这些 32-byte digest，再计算一次 SHA-256。客户端不能通过重新序列化 JSON 计算 digest。Node Link 的 `resource.snapshot_end.snapshotDigest` 使用同一规则（见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.4）。

客户端必须把 snapshot 写入以 `snapshotId` 隔离的暂存区；只有 chunk 连续、数量、cursor 和 digest 全部验证后，才能在一个本地事务中替换旧缓存。收到另一个 `snapshot_begin` 时必须丢弃旧的未完成暂存区。v1 不支持 snapshot chunk 断点续传；连接断开、digest 错误、顺序错误或空间不足时，客户端丢弃整个暂存 snapshot，重连后重新请求。验证失败不得损坏最后一个已完成缓存。

v1 snapshot 资源种类——`sync.snapshot_chunk.resource` 只允许下列三类清单资源（`schemas/sync/v1/sync.schema.json` 的 `$defs/snapshotResource`）：

```text
sessions
workspaces
agents
```

六类 session-scoped 明细资源 `messages`、`turns`、`pending_interactions`、`config_options`、`capabilities`、`history` **MUST NOT 进入快照**，无论会话的 `origin.kind` 是 `local` 还是 `remote`——本节此前只对 imported 会话作此豁免，现已收窄为全部会话，口径与 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.4「正文绝不入快照」一致。会话明细一律由 `session.read` 在线按会话内序号或复合游标分页取得（见第 11.5 节），快照因此只随会话**条数**增长，与消息总量无关。`resource` 取这六个值之一的 chunk 按 `protocol.schema_invalid` 拒绝——它不在 `$defs/snapshotResource` 的封闭词表内，`sync.snapshot_chunk` 因此没有 `history` 这种资源可下发（结构化历史条目同样只在客户端按会话在线读取时下发）。


Snapshot item 的最低 schema：

| Resource | 每个 item 的必填字段 |
|---|---|
| `sessions` | `sessionId`, `agent`, `state`, `origin`, `version`, `createdAt`, `updatedAt`; `title`, `currentMode` 可为 `null`；`workspace` 是 `core.local-catalog.v1` 门控下的**可选**引用（缺席或 `null`，见第 10.3 节） |
| `messages` | `messageId`, `sessionId`, `role`, `content`, `status`, `createdAt`; `turnId` 可为 `null`；**只经 `session.read` 返回，不进快照** |
| `turns` | `turnId`, `sessionId`, `state`, `createdAt`; `startedAt`, `completedAt`, `terminalError` 可为 `null`；**只经 `session.read` 返回，不进快照** |
| `pending_interactions` | `interactionId`, `sessionId`, `kind`, `state`, `schema`, `createdAt`；**只经 `session.read` 返回，不进快照** |
| `config_options` | `sessionId`, `configOptions`, `version`；**只经 `session.read` 返回，不进快照** |
| `capabilities` | `sessionId`, `agentCapabilities`, `brokerAdditions`；**只经 `session.read` 返回，不进快照** |
| `history` | `eventId`, `eventType`, `sessionSequence`, `createdAt`, `view`；`acp` 可选（`rawAcp` 的原文或不可用说明）；**只经 `session.read` 返回，不进快照** |
| `workspaces` | `directoryId`, `path`；受 feature `core.local-catalog.v1` 门控 |
| `agents` | `agentId`, `displayName`, `default`；受 feature `core.local-catalog.v1` 门控 |

`agent` 至少包含稳定 `agentId` 和展示用 `name`；不得包含 Provider credential。上表中标为「不进快照」的六类元素形状由 `session.read` 的结果直接复用（`command.schema.json#/$defs/sessionReadResult.resources` 引用同一批 `snapshotItem.*` 定义），因此会话明细只有一套值对象，不得为快照与回源发明两套语义。

`workspaces` 与 `agents` 是 `core.local-catalog.v1` 门控下的目录资源：客户端未协商该 feature 时服务端 MUST NOT 发送这两个资源，连空数组占位也不得发送。`workspaces` 的每个条目是 `{ directoryId, path }`：`directoryId` 是 32 字符小写十六进制的不透明标识，`path` 是已记录目录的规范化路径，两者**始终成对**下发。目录由成功的会话创建自动记录；已记录但没有任何会话的目录仍出现在 `workspaces` 里，客户端据此渲染空目录而不是把它当作不存在。目录资源不按设备授权过滤：任何已认证设备都收到本节点完整的目录与已配置 Agent 集合；本次不新增任何授权词汇。


- 路径的可见面**只有两处**：快照 `workspaces` 条目与只读目录浏览 `workspace.browse` 的 `command.result`（§11.5）；会话摘要的 `workspace` 引用只有 `{ directoryId }`（§10.3），事件载荷、错误 `details`、其余 `command.result` 与审计记录的前像一律不含路径。客户端需要路径时按 `directoryId` 在同一份快照内查表。
- 客户端 MUST NOT 拼接、规范化、解析或反查这些值。

`origin` 区分本地与 imported 会话（见第 9.6 节）。imported 会话的正文始终由 Owner 权威，客户端拿到 `origin.kind = "remote"` 的摘要后必须用 `session.read` 在线回源 Owner；本地会话的明细同样只经 `session.read` 现取，两者在 wire 上没有区别。

**明细缺席不是「为空」**：客户端在快照里找不到某会话的明细资源时，该会话的明细状态是「尚未加载」而不是「没有消息、没有待处理交互」。客户端 MUST NOT 据此向用户呈现该会话为空。待处理交互数由 `SessionSummary.state` 为 `pending` 的会话数得出（目录页徽标按会话计数，不累加请求条数），交互细节按需用 `session.read { include: ["pending_interactions"] }` 取得。

**本期服务端对三类明细资源显式回不支持**：`turns`、`pending_interactions` 与 `capabilities` 在当前 `core` 读面上**给不出本表要求的字段**——`Turn` 没有必填的 `createdAt`（也没有可选的 `terminalError`），`CapabilitySet` 是一个能力集合、给不出 `agentCapabilities` 与 `brokerAdditions` 的拆分，`PendingInteraction` 没有 `schema` 对象。服务端因此对这三类 `include` 一律回 `capability.unsupported_by_broker`，**不静默省略、不伪造字段**（符合 [AGENTS.md](../AGENTS.md) 第 4 节「无法安全处理的能力必须明确返回不支持」）。

客户端 MUST NOT 把该拒绝当成「该会话没有 turn / 没有待处理交互 / 没有能力信息」，也 MUST NOT 因此认为明细已到最早一条——它表达的是「本期服务端不提供这一类」，不是空集合。因此客户端 MUST NOT 用 `resource.requested` 的拒绝推断该会话的真实内容。

**imported 会话的明细同样回该错误**：`resources` 里非本节点权威的会话，其明细本应**回源 Owner**（第 9.6 节），但回源通道需要 `node-link-client` 的出站客户端，而它尚未落地；`server::sync` 也不得调用平级的 `server::node_link`。本期实现因此对非自有引用一律回 `capability.unsupported_by_broker`，而不是返回一份「看起来完整」的本地空结果。接通时机与 `node-link-client` 的落地绑定。

上述两处都是**本期已知的功能缺口**，不是协议意图：补齐（为 `Turn` 定 `createdAt` 的产品语义、拆 `CapabilitySet`、给 `PendingInteraction` 补 `schema`、接通回源）属独立变更。

### 9.5 ACK

客户端在事件已经进入其可恢复本地状态后发送累计 ACK：

```json
{
  "type": "sync.ack",
  "body": {
    "cursor": {
      "serverEpoch": "00384a03-bc90-4095-b65d-82fb8cc47e13",
      "globalSequence": "2318"
    }
  }
}
```

- ACK 是最高已连续处理的可见事件 cursor。
- ACK 可以批量或定时发送，不要求逐事件发送。
- ACK 只能前进；回退值被忽略并记录诊断，不改变服务端已知 cursor。
- ACK 超过当前连接已发送的最高 cursor 时返回 `sync.cursor_invalid`；不能用客户端 ACK 推进服务端事件 head。
- ACK 不表示用户已阅读，也不改变业务状态。
- 服务端保留策略不能只依赖某个可能永久离线的设备 ACK；按 TTL、容量和设备活跃策略共同决定。
- 即使某段 global sequence 全部因权限过滤而不可见，客户端也可以 ACK `sync.caught_up.cursor`；它表示已完成同步 barrier，不表示看到了被过滤事件。

### 9.6 imported 资源（`resource.remote-origin.v1`）

Access Node 可以向已协商 feature `resource.remote-origin.v1` 的客户端展示由 Owner Node 导出的会话。该模式下 Owner 始终是正文权威，Access 只在线转发，不维护本地索引、收据或游标（§9.7）。

会话摘要增加必填 `origin`：

```text
origin = { kind: "local" }
       | { kind: "remote", ownerNodeId: UUID, originEpoch: UUID, online: boolean }
```

- `kind` 为 `local` 时表示该会话由本节点拥有并持久化正文。
- `kind` 为 `remote` 时表示该会话由 `ownerNodeId` 拥有；`originEpoch` 是该**会话**在 `ownerNodeId` 上的 origin epoch（会话首次持久化写入时生成，不是节点级 epoch）；`online` 反映最近一次已知的 Owner 可达性。
- `origin` 是 `SessionSummary` 的必填字段，形状见第 10.3 节。

事件 body 增加必填 `remoteOrigin`，非 imported 事件固定为 `null`：

```text
remoteOrigin = {
  ownerNodeId: UUID,

  originEpoch: UUID,
  originEventId: UUID,
  originSequence: decimal string
} | null
```

- imported 事件的 `eventId` 必须等于 `remoteOrigin.originEventId`；`eventId` 跨跳不变，客户端因此可以跨 Access 与 Owner 去重同一条事件。
- imported 事件不分配 Access 本机 `globalSequence`/`sessionSequence`；跨节点排序、ACK 与去重一律以 Owner 来源序号 `(originEpoch, originSequence)` 为准（去重键还落在 `ownerNodeId`/`sessionId` 命名空间内，见 §9.7）。
- `origin.kind` 仍描述产生事件的来源类型（`agent`/`device`/`daemon`/`local_cli`/`node`，其中 `node` 与身份字段 `nodeId` 由 feature `core.event-origin.v1` 门控，见第 10.1 节）；远程跳数只由 `remoteOrigin` 表达，不放进 `origin.kind`。
- 不得为本地事件伪造 `remoteOrigin`；`originEventId` 必须来自 `remoteOrigin.ownerNodeId` 的事件日志。

交付与缓存规则：

- imported 会话的 `messages`、`turns`、`pending_interactions` 和 `config_options` 不得进入 Access 的 snapshot，也不得进入 Access 的任何持久存储。Access **不持久化**远程会话身份索引、`eventId`/`eventType`/digest 或 local sequence 映射；这些值只在已认证连接期有界存在，断线、撤销或来源 epoch 变化即整代丢弃（design D1/D2）。
- `sessions` chunk 与 `session.list` 必须包含 imported 会话的摘要并带 `origin.kind = "remote"`，否则客户端无法发现该会话；摘要之外的正文类资源一律不进入 snapshot。
- 客户端一律通过 `session.read` 在线回源 Owner 获取正文、turn 历史和待处理交互。Access 不排队、不缓存正文、不改写 ACP 语义与 `acp.rawJson`、不扩张 capability、不修改会话状态。
- **（计划条款，待实施）** 远程历史：协商 `core.remote-history.v1` 后，imported 会话的结构化历史同样经 `session.read` 的 `history` include 与 `beforeHistory` 游标从 Owner 分页读取；Access **不保存** origin/local 序号映射、无正文收据或页缓存，页边界、条目标识与 `historyHasEarlier` 由 Owner 决定，Access MUST NOT 重新排序或截断后冒充完整页。每次读取前重新校验连接身份、会话来源与撤销状态；Owner 不可达返回 `resource.remote_unavailable`，超出保留期返回 `history` 不可用事实，两种情况都 MUST NOT 返回空页冒充「没有历史」。该能力当前尚未实现，实现前对 imported 会话仍按 §9.4 的不支持错误降级。
- 客户端对 `origin.kind = "remote"` 的会话不得持久化正文、`acp.rawJson`、摘要副本或事件 digest；只允许在**本次连接期内存**中持有来源游标与有界重建缓冲（design D4），断线后来源业务模型必须清除。
- Owner 不可达时 `session.read` 返回 `resource.remote_unavailable`，同时把该会话的 `origin.online` 置为 `false` 并广播一次 `session.origin.online_changed`；恢复可达后同样以该事件把 `online` 置回 `true`。
- Owner 离线期间客户端不得把远程会话的输入标记为已发送或已接受；本地 draft 可以保留，但必须显式标记为未提交。
- `session.list` 只返回该设备有权查看的本地会话与 imported 摘要，两者使用同一 `SessionSummary` 形状；不得为远程会话发明第二套字段。
- **来源流走 §9.7，不再经过本地交付索引**：imported 事件不进入本地事件库，也不分配 Access 本机 `globalSequence`/`sessionSequence`（design D3）；来源事件经 `sync.remote_event` 逐字转发，`eventId` 逐字等于 `remoteOrigin.originEventId`，正文只来自当次连接的有界内存（MUST NOT 落盘）。`origin.kind` 取 `"node"`、`nodeId` 取 Owner：Access 能如实陈述的产生者就是交付这条事件的节点（Node Link 的 `resource.event` 不携带 Owner 侧的产生者类型，Access 不得据此猜测 `agent`/`device`）；因此远程事件只投给同时协商了 `resource.remote-origin.v1`、`resource.stateless-origin.v1` 与 `core.event-origin.v1` 的连接，未协商时明确不支持并记诊断，绝不伪造来源。`causationRequestId` 为 `null`（诱发命令在 Owner 上，Node Link 不携带它）。**实现状态**：本段是 `access-owner-data-stateless` 冻结的新合同；旧实现中的 `imported_delivery_index.local_sequence`、本地交付索引与 Access 侧收据随该变更的 WP3/WP4 一并删除，任何 Access 序号都不再作为远程游标或恢复依据。

未协商该 feature 时，Access 不得返回任何 `origin.kind = "remote"` 的会话，也不得发送带非空 `remoteOrigin` 的事件。

imported 会话的 `workspace` 固定为 `null`，直到提供该会话的 Owner 侧投影携带目录引用为止。Access Node MUST NOT 用自己的 workspace 集合为 imported 会话猜测归属，也不得按本机规范化路径反查。

实现状态（`openspec/changes/archive/2026-10-15-sync-server-connection`，截至本修订）：§9.1–§9.5 的订阅、重放、快照与 ACK 面**已落地**，`crates/server/src/sync/subscribe/`（cursor 判定、`MessageRoute` 分派、`event` 帧装配与实时扇出）与 `crates/server/src/sync/snapshot/`（清单资源组装、分块下发与 `snapshotDigest`）。已覆盖的实绩：

- cursor 判定（§9.2）：`null` 走首次同步；格式非法、`epoch_mismatch`、`beyond_head`、`cursor_expired` 四种 `sync.cursor_invalid` 的 `details.reason` 可区分且互不混用；**不得钳制到 head**（越界 cursor 不按 head 续投）。超出 §3.3 序号上界（`2^63−1`）或 64 位承载范围的取值是**协议错误**（`protocol.schema_invalid`），不折叠为任一 `reason`。
- 增量重放（§9.3）：从 cursor 之后按 `globalSequence` 升序投递 `event` 帧，随后 `sync.caught_up`；重放期间到达的实时事件排在屏障之后，绝不越过序号更早的已持久化事件；权限过滤造成的序号空洞合法且不报错，屏障游标高于该段即可 ACK。事件正文经 `core::use_cases::sync_replay`（`session.list` 授权）读出并原样顺带 `origin_kind`/身份字段与 `causation`——`deviceId` 仅在 `device` 类事件非空，取值经 `causation` 回查诱发命令的 actor，不得由「有没有会话」推断。
- 无法归因的历史事件**显式失败关闭**：升级前写入、`causation` 缺失或无法归因的事件不伪造 `kind`/`deviceId`，由投递层记诊断并让该次订阅返回 `internal.unavailable`（retryable）；协商 `core.event-origin.v1` 的连接不会收到 `deviceId` 为 `null` 的 `kind = device` 事件。
- 快照（§9.4）：`sessions` chunk 恒发一个（空会话也发空数组），`workspaces`/`agents` 受 `core.local-catalog.v1` 门控（未协商不下发、不以空数组占位），`chunkCount` 与实际块数一致且取值 `1..=3`；六类会话明细资源不进快照；`snapshotDigest` 按每个 chunk 的**原始 UTF-8 文本字节**分别求 SHA-256、按 `chunkIndex` 顺序连接后再求一次 SHA-256，结果取无填充 base64url。
- ACK（§9.5）：只前进，回退被忽略并记诊断；超过本连接已发送最高游标时回 `sync.cursor_invalid`；ACK 不推进事件库 head，服务端保存的设备 ACK 最大值不决定新连接的恢复位置。

**已落地的业务命令面**（本小节相邻）：§11.2 的 `command.result`（`accepted`/`completed`/`failed`/`rejected`/`uncertain` 五态、`acceptedAt` 与 `terminalEventId` 的取值约束）、§11.3 的十条查询命令（同步 `completed` 或 `rejected`；含 `workspace.browse`）、§11.4 的 `command.status`、以及 §11.5 的 18 条 Sync 命令（含 8 条 mutation 的「先持久接受、后经持久记录给终态」路径、`(actor, requestId)` 幂等与 `command.idempotency_conflict`、`state.version_conflict` 带 `currentVersion`、`session.create` 的 feature 门控 `core.session-create.v1` 与 `uncertain` 收口）中的既有命令均已接线；新增的 `session.files.list`/`session.files.read`/`session.git.status`/`session.git.diff` 已由 WP3 接到真实用例与原生端口：门控先于任何文件/Git I/O，`session.files.read` 的连接绑定内容版本随连接关闭/撤销/daemon 关闭释放，`core.session-files.v1` 因此进入服务端支持集合。命令帧经 `/sync/v1` 的组合路由（订阅面 → 命令面）到达，mutation 的终态由**连接期观察者**按 `(deviceId, requestId)` 轮询持久记录后推回提交方，断开即取消。`session.resume` 不在 Sync v1 的封闭词表内，按 §4.2 回 `command.unsupported`。

**仍未落地**（与本小节相邻）：imported 事件的**回源重放**（实时腿已落地，见 §9.6 末条）；`session.read` 的三类明细资源与 imported 明细的降级见 §9.4 的说明。`sync.reset_required` 的四种 `reason` 中，`cache_incompatible` 是唯一只能由客户端自报的一种（v1 无「客户端缓存版本」字段），服务端对每连接确权一次以防回声循环，其余三种由服务端按 §9.2/§9.4 的判定主动下发。

### 9.7 无持久中转的远程来源流（`resource.stateless-origin.v1`）

本节是 feature `resource.stateless-origin.v1` 的冻结 wire 合同（design D3–D6）。它替换 §9.6 中「Access 维护本地交付索引/收据」的做法：**Owner 的来源日志是唯一顺序权威，Access 只在已认证连接期转发**，不为 imported 事件分配 Access 本地 `globalSequence`，也不持久化任何远程进度。

**分域前提**：`sync.subscribe`/`event`/`sync.caught_up`/`sync.ack` 与 §9.1–§9.5 的 `serverEpoch`/`globalSequence` **仍然只描述本节点 owned 日志**，语义与持久化合同不变。远程流使用独立的 9 个消息类型，游标直接是 Owner 的 `originCursor = { originEpoch, originSequence }`（`originEpoch` 是 UUID，`originSequence` 是十进制字符串）。两个序号域不得互相翻译、钳制或混用。

| 消息 | 方向 | body | 语义 |
|---|---|---|---|
| `sync.remote_subscribe` | 客户端 → Access | `subscriptionId`(UUID)、`sessionRef`(`{ ownerNodeId, sessionId }`)、`cursor`(`originCursor` \| null) | 绑定本设备可访问的当前来源会话；`cursor=null` 走首次重建路径，非空走来源增量重放 |
| `sync.remote_event` | Access → 客户端 | `subscriptionId`(UUID)、`event`(`{ originEventId, originEpoch, originSequence, sessionRef, eventType, payloadDigest, createdAt, payload }`) | Owner 来源事件逐字转发；**不带** Access 的 `globalSequence`/`sessionSequence` |
| `sync.remote_caught_up` | Access → 客户端 | `subscriptionId`、`cursor`(`originCursor`) | Owner 已完成来源重放屏障；此后才是实时事件 |
| `sync.remote_ack` | 客户端 → Access | `subscriptionId`、`cursor`(`originCursor`) | 仅本次连接该订阅的连续处理进度；不落盘，不表示可恢复索引 |
| `sync.remote_reset_required` | Access → 客户端 | `subscriptionId`、`reason`(`epoch_mismatch`\|`cursor_expired`\|`cache_incompatible`)、`cursor`(`originCursor` \| null) | 客户端必须回源 `session.read` 重建；只有 Owner 已验证的有效 head 才能给 `cursor` |
| `sync.remote_unsubscribe` | 客户端 → Access | `subscriptionId`(UUID) | 取消本连接订阅及其所属观察者 |
| `sync.remote_unavailable` | Access → 客户端 | `subscriptionId`、`error`(`publicError`) | 断线、撤销、不支持分别复用既有错误码，不新造词表 |
| `sync.remote_catalog_changed` | Access → 客户端 | `ownerNodeId`(UUID)、`revision`(十进制字符串) | 该来源清单失效；客户端重新查询，不携带旧资源副本 |
| `sync.remote_source_state` | Access → 客户端 | `ownerNodeId`(UUID)、`state`(`online`\|`offline`\|`revoked`) | 来源连接事实；不是 owned 业务事件，不写入本地事件流 |

全部 body 都是 closed object（`additionalProperties: false`），字段与 `schemas/sync/v1/sync.schema.json` 的 `$defs` 一一对应；不得为这些语义另造业务命令同义词。

- **事件形状**：`sync.remote_event.event` 保留 `resource.event` 的 origin 三元组、`sessionRef`、`eventType`、`payloadDigest`、`createdAt` 与 `payload`。`payload` 与 Node Link 同义，`view` 与 `acp` 至少出现一个；`payload.acp` 的原始 ACP 文本逐字转发，MUST NOT 经通用数字 DTO 重编码。`eventId` 使用 Owner 的 `originEventId`，不创建替代标识；客户端去重键绑定 `(ownerNodeId, sessionId, originEpoch, originEventId)`（`docs/NODE_LINK_PROTOCOL.md` §12.4）。
- **订阅来源**：`sessionRef` 必须是本设备当前有权访问的来源；`subscriptionId` 是本次连接内的随机标识，不得跨连接复用。同一 `(ownerNodeId, sessionId)` 的多个订阅互不影响。
- **订阅与退订的限流族**：两者归入既有 `observe` 速率族；单连接最多 32 个远程订阅，超限返回 `resource.rate_limited`（`details.retryAfterMs`）。单帧、发送队列与重放批次沿用 §14 的既有预算；客户端重建缓冲计入同一有界预算，超限必须明确 backpressure 或重建，MUST NOT 落盘扩容。本变更不新增配置开关。
- **重放与屏障**：非空 `cursor` 从该位置之后按 `originSequence` 升序重放，随后发 `sync.remote_caught_up`；屏障之后的实时事件不得越过它。**MUST NOT 用定时器推测追平**。
- **首次重建（`cursor=null`）**：Access 先取得 Owner 当前元数据 snapshot 的屏障 B，验证完成后向客户端发 `sync.remote_reset_required(reason=cache_incompatible, cursor=B)`；客户端回源 `session.read` 重建明细，在有界缓冲到 B 后再消费事件。此处客户端并不持有可兼容远程状态，因此不使用持久化 cursor。
- **重连**：只要断线策略已清除来源业务模型，客户端即使仍持有最后处理位置 C，也必须重新回源 `session.read` 并按每页覆盖信息重建，不能只重放 C 之后的事件。仍持有 C 时可以把它交给 Owner 请求来源重放并缓冲，同时读取最新权威页；有效 C 只是衔接与去重位置，不能证明页面仍拥有此前内容。没有 C 则走 null 订阅 + B 后读取的首次重建路径。epoch 不匹配/过期先明确 `remote_reset_required`，再走首次重建；Owner 不可达则明确 `remote_unavailable`，不展示已清除内容。只有重建读取成功且来源屏障已追平后才能呈现可用视图。
- **ACK 语义**：`sync.remote_ack` 只是当前连接该订阅的连续处理进度。Access MUST NOT 持久化它，也不得把它当成客户端已阅读或可恢复的承诺；Owner 不据此提前清理保留期内数据。回退的 ACK 忽略并记诊断，越过该订阅已发 head 的 ACK 返回 `protocol.sequence_invalid`。慢客户端超限时取消**该订阅**，不得阻塞 Owner 或其他客户端。
- **未协商降级**：未协商 `resource.stateless-origin.v1` 的连接 MUST NOT 收到任一 `sync.remote_*` 消息；服务端不得回退到 §9.6 的本地落盘中转来「兼容」。客户端若收到未协商的 `sync.remote_*` 类型，按 §5.3 以 `protocol.type_unsupported` 拒绝并停止该订阅；服务端若收到未协商的 `sync.remote_subscribe`/`remote_ack`/`remote_unsubscribe`，同样拒绝并在诊断中标明缺失 feature。远程资源只有在端到端能力（本 feature 与 §9.6 的来源 feature）实际满足时开放，未满足时明确不支持。
- **旧 imported 游标**：`sync.remote_subscribe.cursor` 只接受 `originCursor`（`originEpoch`/`originSequence`）或 `null`。收到 `sync.cursor`（`serverEpoch`/`globalSequence`）形状时按封闭对象在解码层以 `protocol.schema_invalid` 拒绝（§5.3），MUST NOT 翻译、钳制或改写成本地序号，也不得把它折叠成 `sync.cursor_invalid` 的任一 `reason`——`errors.json` 的 `cursor_invalid.reason` 中并没有 `cache_incompatible`。客户端需要重建时由 `sync.remote_reset_required(reason=cache_incompatible)` 驱动。

`session.read` 的加性读取覆盖与 `command.status` 的 `remoteTarget` 见 §11.5/§11.4；它们同样只在本 feature 协商后出现。

## 10. Event

### 10.1 结构

```json
{
  "protocolVersion": 1,
  "type": "event",
  "messageId": "04dbad03-8869-4c5a-94fa-2f3af2fb622b",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "42",
  "body": {
    "globalSequence": "2318",
    "sessionSequence": "109",
    "eventId": "da702ba1-f49f-4e16-a677-a7744dfedba6",
    "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    "eventType": "agent.message.delta",
    "causationRequestId": "4c4dafda-dd98-442e-8d55-252b75bac72d",
    "origin": {
      "kind": "agent",
      "deviceId": null,
      "nodeId": null
    },
    "remoteOrigin": null,
    "createdAt": "2026-09-17T12:10:00.123Z",
    "payload": {
      "view": {},
      "acp": {
        "mediaType": "application/json",
        "rawJson": "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{}}",
        "byteLength": "<decimal-string>",
        "sha256": "<base64url-32-byte-sha256>"
      }
    }
  }
}
```

规则：

- 非会话级事件的 `sessionId` 和 `sessionSequence` 为 `null`。
- 没有客户端命令直接导致的事件，其 `causationRequestId` 为 `null`；由命令产生的所有领域事件必须携带对应 `requestId`。
- `eventType` 匹配 `^[a-z0-9_.-]{1,128}$`，只允许小写 ASCII 字母、数字、`_`、`.` 和 `-`；未登记取值按未知事件处理，不因字符集合法而获得额外语义。
- `origin` 的形状由 feature `core.event-origin.v1` 门控，`kind` 是判别标签：

  ```text
  origin = { kind: OriginKind, deviceId: UUID | null }                        # 未协商该 feature
         | { kind: OriginKind, deviceId: UUID | null, nodeId: UUID | null }   # 已协商该 feature
  ```

  `kind` 取值固定为 `agent`、`device`、`daemon`、`local_cli`、`node`，含义如下：

  | `origin.kind` | 含义 |
  |---|---|
  | `agent` | 由本地或已导入 Agent 的 ACP 输出产生 |
  | `device` | 由已授权 Sync 设备经命令产生；语义上要求带非空 `deviceId`（见下面的配对规则） |
  | `daemon` | 由本节点 Broker、调度器或存储维护任务产生 |
  | `local_cli` | 由本机 CLI/桌面客户端经本地 IPC 发起，不经过远程设备身份 |
  | `node` | 由另一个节点经 Node Link 下发命令、在**本机**事件库落库的事件；必须包含非空 `nodeId` |

  配对规则只有一条，schema 与 Rust DTO 都按它强制：`kind` 为 `node` 时 `nodeId` 必须存在且为非空 UUID，且 `deviceId` 必须为 `null`。其余取值**不做**身份字段配对校验——冻结的 v1 形状里只有 `{kind, deviceId}` 两个键（`deviceId` 的类型是 `UUID | null`），`{kind: "device", deviceId: null}` 这类组合按 schema 本来就合法。`device` 的含义仍要求事件来自已授权设备（原 v1 的"必须包含 `deviceId`"是**发送方**的语义要求）：发出方 MUST NOT 产生 `{kind: "device", deviceId: null}`，但 schema 与接收方不得因此拒绝它——把必填性收紧上去是第 16.2 条列出的破坏性变更，不能借这次扩展顺手加。

  `node` 描述的是**产生者**是另一个节点，和 imported 事件是两回事：前者的事件在本机落库、`remoteOrigin` 为 `null`，后者的跳数与来源写在 `remoteOrigin` 里（见第 9.6 节）。`origin.kind` 里没有 `remote`——远程跳数不放进 `kind`。
- `origin.kind` 取值 `node` 与身份字段 `nodeId` 由 feature `core.event-origin.v1` 引入，不是加性变更（第 9.6 节 `resource.remote-origin.v1` 同口径）：`origin` 是 `additionalProperties: false` 的封闭对象，多出 `nodeId` 键会让未协商的旧客户端按 `protocol.schema_invalid` 拒绝整条事件。因此：

  - 协商了该 feature：`origin` 含 `nodeId` 键，并满足上面那条配对规则；
  - 未协商：`origin` 的形状与引入该 feature 之前**完全一致**（只有 `kind` 与 `deviceId` 两个键，`nodeId` 既不出现也不能被要求），且服务端 MUST NOT 投递 `kind = "node"` 的事件——服务端 MUST NOT 因客户端版本伪造或降级 `kind`（尤其不得把 `node` 写成 `device`）。

  协商与未协商两种形状都在 v1 内合法（schema 只要求 `kind`/`deviceId`），客户端按自身协商结果解释 `origin`。
- `remoteOrigin` 是必填字段；本地事件为 `null`，imported 事件必须为带 `ownerNodeId`、`originEpoch`、`originEventId`、`originSequence` 的对象，且 `eventId` 等于 `originEventId`。
- `createdAt` 是 Daemon 持久化时间，不使用客户端时间决定顺序。
- `payload.view` 是公共结构化视图。
- `payload.acp` 在事件来源或语义与 ACP 消息相关时存在，内容视为不可信。
- `rawJson` 是去除 transport delimiter 后的原始 UTF-8 JSON document；`byteLength` 是十进制字符串，和 `sha256` 都针对其 UTF-8 bytes。客户端不得先解析再重新序列化来校验 hash。
- 若完整原文会使 Sync message 超出协商上限，`acp` 改为 `{ "rawUnavailable": { "reason": "size_limit", "byteLength": ..., "sha256": ... } }`。Daemon 仍按本地保留策略保存原文，客户端必须明确显示原文未同步，不得伪装为完整消息。
- `rawJson` 和 `rawUnavailable` 二选一；任何一项都不能参与认证、授权或命令路由决策。
- 事件类型是开放集合；客户端必须为未知类型提供可见降级视图并保留原始 payload。

规范类型：

```text
RawAcpV1 = {
  mediaType: "application/json",
  rawJson: string,
  byteLength: decimal string,
  sha256: base64url(32 bytes)
} | {
  mediaType: "application/json",
  rawUnavailable: {
    reason: "size_limit" | "retention_expired" | "storage_failure",
    byteLength: decimal string,
    sha256: base64url(32 bytes) | null
  }
}
```

`storage_failure` 必须同时形成显式 daemon/storage 错误或降级事件，不能成为正常静默路径。

### 10.2 首批标准事件类型

```text
session.created
session.updated
session.info.changed
session.config.changed
session.mode.changed
session.plan.changed
session.commands.changed
session.usage.changed
session.origin.online_changed
turn.queued
turn.started
turn.completed
turn.failed
turn.cancelled
turn.delta_compacted
user.message.delta
agent.message.delta
agent.message.completed
agent.thought.delta
tool.call.started
tool.call.updated
tool.call.completed
permission.requested
permission.resolved
elicitation.requested
elicitation.resolved
terminal.output
file.changed
agent.connected
agent.disconnected
command.completed
command.failed
command.uncertain
device.revoked
```

本节列表是 v1 登记的标准事件类型集合，与 [`schemas/sync/v1/event-views.schema.json`](../schemas/sync/v1/event-views.schema.json) 的 `$defs` 一一对应；每个类型的最低 `view` 字段见第 10.3 节。未登记的 ACP 判别子仍按第 10.3 节的映射与降级规则处理，不得静默丢弃。

#### 10.2.1 哪些类型进入会话结构化历史条目

`session.read` 的 `include: ["history"]`（第 11.5 节）返回的条目**复用事件的 `view` 形状**，因此这里登记的是「上面这 34 类里，哪些会产生条目」。这个集合是**时间线语义**的判断——服务器按它决定条目，不因某个客户端没有对应卡片而改变；「客户端有专用视图」的另一套集合由客户端自己维护（[FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) §6）。

进入历史条目的 17 类：

| 进入历史条目的事件类型（17 类） | 条目在时间线上承载什么 |
|---|---|
| `turn.queued` / `turn.started` / `turn.completed` / `turn.cancelled` / `turn.failed` | 一轮的生命周期与终态（失败时带终态错误） |
| `tool.call.started` / `tool.call.updated` / `tool.call.completed` | 工具调用步骤；同一 `toolCallId` 的多条按会话内序号排列，归并留给消费方 |
| `permission.requested` / `permission.resolved` | 权限请求与它的结果 |
| `elicitation.requested` / `elicitation.resolved` | 提问与回答 |
| `terminal.output` | 终端输出分片 |
| `file.changed` | 文件变更统计与相对化路径，以及受 `core.file-diff.v1` 门控的结构化差异行（未协商时只有统计，第 10.3 节） |
| `session.plan.changed` | 计划变化 |
| `session.mode.changed` | 模式切换 |
| `session.config.changed` | 配置项变化 |

不进入的 17 类，按理由分四组：

| 不进入历史条目的事件类型（17 类） | 为什么 |
|---|---|
| `user.message.delta` / `agent.message.delta` / `agent.thought.delta` / `agent.message.completed` | 承载消息正文：正文的权威只在 `messages`，同一段正文有两份来源会打架；`agent.message.completed` 是收尾正文，同样不产生条目 |
| `session.created` / `session.updated` / `session.info.changed` / `session.usage.changed` / `session.commands.changed` / `session.origin.online_changed` | 摘要与仪表类：`SessionSummary`、`ModeState`、用量与可达性标注已有专门承载，条目里再来一份只是复制 |
| `command.completed` / `command.failed` / `command.uncertain` | 命令终态：由 `command.result` 与 `command.status` 承担，条目里重复会让同一事实有两个来源 |
| `agent.connected` / `agent.disconnected` / `device.revoked` / `turn.delta_compacted` | 连接与审计类：不是会话时间线上用户要读的内容 |

**已按保留策略压缩的事件不产生条目**：`compacted_into IS NOT NULL` 的行既不进历史也不可读（与第 11.5 节的保留窗口口径一致）。因此 `agent.thought.delta` 没有收尾事件，压缩后即从**事件流**消失。Agent 明确提供的思考按 `MessageChannel.Thought` 与回复分别落盘到 `owned_message`（`CORE_PORTS_AND_STORAGE.md` §7.6，计划由 WP2 实施），历史恢复经 `session.read` 的 `messages` 取得，不依赖 delta 事件仍在保留期内；压缩只影响事件重放，不删除已保存的正文。WP2 落地前，`owned_message` 的思考正文尚不存在，不得据此宣称思考历史恢复已实现。

### 10.3 v1 Event View Contract

所有 `view` 都是 object。下面字段是最低必填合同；可以增加已协商 feature 所允许的可选字段，但不得改变既有字段语义；向后兼容的可选字段（例如 `toolDetail`）不要求 feature 协商。 三条跨字段规则： (a) `deltaIndex` 在**每条消息内**从 `0` 开始严格加一，由生产事件的适配器分配；空洞不是协议错误，也不表示丢包（检测丢包请用 `payloadDigest`/序号），消费方按现存的 `deltaIndex` 升序拼接； (b) `agent.message.completed` **由 Broker 在 turn 结束时生成**，与该 turn 的终态事件在**同一次提交**内落盘：正文按该消息的 delta（含可选 `block`）折叠，规则见 `CORE_PORTS_AND_STORAGE.md` §6 第 14 条；`agent.thought.delta` 不产生收尾事件（登记在案的短保留期降级）； (c) delta 被 `turn.delta_compacted` 压缩后，**重放里由该 summary 事件替代**（不再出现那些 delta）；客户端察觉 delta 丢失靠会话内 `sessionSequence` 的稠密性（§9.4），终态正文一律以 `agent.message.completed` 为准（`CORE_PORTS_AND_STORAGE.md` §6 第 15 条）。`view` 内的 JSON 取值域遵守 §3.3 的 ACPR-CJ1：数字必须是整数（`|n| ≤ 2^53−1`），不得出现浮点，否则 `payloadDigest` 无法跨实现复算。

| Event type | `view` 最低字段 |
|---|---|
| `session.created`, `session.updated` | `session: SessionSummary` |
| `session.info.changed` | `title: string|null`, `updatedAt: timestamp` |
| `session.config.changed` | `configOptions: SessionConfigOptionView[]`, `version: decimal string` |
| `session.mode.changed` | `currentModeId: string|null`, `version: decimal string` |
| `session.plan.changed` | `entries: { content: string, priority: "high"|"medium"|"low", status: "pending"|"in_progress"|"completed" }[]` |
| `session.commands.changed` | `commands: { name: string, description: string }[]` |
| `session.usage.changed` | `used: decimal string`, `size: decimal string` |
| `session.origin.online_changed` | `ownerNodeId: UUID`, `originEpoch: UUID`, `online: boolean` |
| `turn.queued`, `turn.started`, `turn.completed`, `turn.cancelled` | `turnId`, `state` |
| `turn.failed` | `turnId`, `state: "failed"`, `error: PublicError` |
| `turn.delta_compacted` | `turnId`, `deltaCount: decimal string`——**只承载收据，不复制正文**；终态正文见 `agent.message.completed`。**不带** delta 摘要：客户端察觉 delta 丢失靠的是 §9.4 的「会话内 `sessionSequence` 无空洞」，那比一个自算摘要更强，而 ACPR-CJ1 摘要需要密码学依赖、Broker 层不允许持有（`CORE_PORTS_AND_STORAGE.md` §2） |
| `user.message.delta` | `messageId`, `turnId`, `deltaIndex: decimal string`, `text` |
| `agent.message.delta` | `messageId`, `turnId`, `deltaIndex: decimal string`, `text`, 可选 `block: AgentContentBlock`（**仅**在非文本内容时出现；`text` 始终是该 chunk 的文本投影，纯文本客户端只读它）。`block` 由**生产事件的适配器**投影（它持有 ACP DTO），Broker 不解析 ACP 原文 |
| `agent.message.completed` | `messageId`, `turnId`, `content: AgentContentBlock[]` |
| `agent.thought.delta` | `messageId`, `turnId`, `deltaIndex: decimal string`, `text` |
| `tool.call.started`, `tool.call.updated`, `tool.call.completed` | `toolCallId`, `turnId`, `title`, `state`; 可选 `toolDetail`（`input`/`output`/`text` 与 `truncatedParts`，语义见下）；摘要字段可选，完整 ACP 保留在 `acp` |
| `permission.requested` | `interactionId`, `turnId`, `title`, `description`, `options: InteractionOption[]` |
| `permission.resolved` | `interactionId`, `resolution`, `resolvedByDeviceId: UUID|null` |
| `elicitation.requested` | `interactionId`, `turnId`, `title`, `schema`, `initialValues` |
| `elicitation.resolved` | `interactionId`, `action: "submit"|"decline"|"cancel"`, `resolvedByDeviceId: UUID|null` |
| `terminal.output` | `terminalId`, `chunkIndex: decimal string`, `stream: "stdout"|"stderr"`, `text`, `truncated: boolean` |
| `file.changed` | `changeId`, `kind`, `displayPath`, `summary`; 可选 `addedLines`/`deletedLines`（decimal string，行级差异统计）、`outsideWorkspace`（boolean）；受 feature `core.file-diff.v1` 门控的可选 `diffLines`（结构化差异行，逐行标明 `added`/`deleted`/`context`）与不可用的 `diffUnavailable`（未协商该 feature 时两个键都缺席） |
| `agent.connected` | `agentId`, `state: "connected"`；节点级事件（`sessionId` 为空），描述 Agent profile 进程而非任何单个会话 |
| `agent.disconnected` | `agentId`, `state: "disconnected"`, 可选 `error`；同为节点级事件 |
| `command.completed` | `requestId`, `result` |
| `command.failed` | `requestId`, `error: PublicError` |
| `command.uncertain` | `requestId`, `reason`, `mayHaveReachedAgent: true` |
| `device.revoked` | `deviceId`, `revokedAt`；只发送给仍有权查看设备状态的其他客户端 |

`toolDetail` 是 `tool.call.started` / `tool.call.updated` / `tool.call.completed` 的**可选**工具详情，只在 Agent 确实提供了对应数据时出现；它不包含任何由视图层推断的取值：

- `input` / `output`：ACP `rawInput` / `rawOutput` 的 JSON 值，原样保留（不重新解释形状）。ACP 未提供对应字段时该键**缺席**；MUST NOT 以下发 `null`、空对象或空数组冒充。
- `text`：ACP 工具内容里文本块（`ToolCallContent::Content` 包装的 `Text`）的文本数组，顺序与 Agent 提供的一致；没有文本块时缺席。Diff、terminal 与未识别的 content 类型**不进入** `text`——差异行继续由类型化 `diff` 与 `file.changed` 承担，终端输出走 `terminal.output`，未识别类型仍可经 `acp.rawJson` 诊断查看。未识别的 content MUST NOT 被文本化或猜测成上述任一部分。
- `truncatedParts`：因超过 §14 的固定上限而**整体缺席**的部分名，取值只取 `"input"`、`"output"`、`"text"`（封闭词表，不得重复）；没有截断时缺席。它是「数据存在但未随本事件下发」的显式信号，客户端 MUST NOT 把缺席读成「参数/输出为空」。
- **ACPR-CJ1 值域约束**：`input`/`output` 是自由 JSON，可能含浮点或超出 2^53−1 的整数，而 `view` 落库前必须通过 ACPR-CJ1 校验（§3.3：数字有限、无小数部分、`|n| ≤ 2^53−1`，嵌套不超过规定深度）。无法以该值域表达的部分同样**整体缺席**并登记在 `truncatedParts`，MUST NOT 让整条事件因 `view` 校验失败而无法提交。`text` 是字符串，不做数值判定。
- 三个内容部分都缺席且没有截断时**整个 `toolDetail` 缺席**，视图仍完整；客户端 MUST NOT 呈现空占位。
- **绝不发送被截断的 JSON 片段**：某一部分超过 §14 的上限时该部分整体缺席并登记；整体预算不足时按 `output` → `text` → `input` 的优先序保留（结果与内容优先，输入通常已由 `tool.call.started` 下发），让出的部分同样整体缺席并登记。`truncatedParts` 是并集语义：一次事件里多个部分超限时必须逐个列出。
- `toolDetail` 不改写同一事件的 `acp.rawJson`：其字节、摘要与长度与不携带 `toolDetail` 时逐字节相同（§10.3），客户端仍可经原文诊断查看完整内容。

`file.changed` 的三个可选字段的语义固定如下，客户端必须照此呈现，不得自行推断：

- `addedLines`/`deletedLines` 是**行级差异**的统计，不是改动前后两段文本的行数相减。两段文本行数相等但内容不同时仍必须报告非零改动；新建文件的全部行计为新增、删除为零，删除文件反之。判定不出行数时**省略这两项**，客户端呈现为「未同步」或不可用，MUST NOT 呈现为「零改动」。
- 这两个字段只覆盖**Agent 在其工具调用里声明的改动**：派生源是 ACP 工具调用内容中的类型化 Diff 元素（见 `CORE_PORTS_AND_STORAGE.md`）。判定只依据该次调用内容**是否含 Diff 元素**，与改动由谁驱动无关：调用内容中不含 Diff 元素的（例如只做 shell 重定向、脚本或外部工具驱动的改动），其行数统计不计入，也不产生 `file.changed` 事件。
- `displayPath` 是**相对该会话工作区根**的路径，不下发绝对路径，也不含工作区根的任何片段或回退层级。路径经规范化后不在工作区根之下时置 `outsideWorkspace: true`，此时 `displayPath` 只给出该文件的名称。是否位于工作区之外按**规范化后的前缀关系**判定，不得仅按字符串前缀判断。

`diffLines` 与 `diffUnavailable` 是 feature `core.file-diff.v1` 门控的**结构化差异行**，语义同样固定：

- `diffLines` 的每一项是 `{ kind, text }`，`kind` 只取 `"added"`、`"deleted"`、`"context"`（封闭词表），`text` 是**不带** `+ `/`- `/`  ` 前缀的行文本。客户端按 `kind` 自行加前缀与配色，逐行区分新增、删除与上下文——它 MUST NOT 把该键当成拼好的文本块去解析，也不得为了补齐而去解析 `acp.rawJson`。
- 该键与同一条事件的行数统计**同源**：两者都来自同一次 ACP 类型化 Diff 的行级差异，因此不会出现「统计说改了 3 行、逐行却给了别的行数」。派生 MUST NOT 改写 ACP 原文的字节、摘要与长度（`acp.rawJson` 仍逐字节保真）。
- **未协商该 feature 时 `diffLines` MUST 缺席**（不是空数组，也不是占位行），其余字段与协商时逐字节一致。
- **超限时不发送被截断的行**：单处改动的差异行数与总字节数超过 §14 的固定上限时，`diffLines` 缺席而 `diffUnavailable: { reason }` 在场，`reason` 只取已登记取值（v1 只有 `"size_limit"`）。客户端据此呈现明确说明，MUST NOT 把省略读成「该处没有内容」，也 MUST NOT 用统计去反推正文。
- 两者互斥：同一条事件 MUST NOT 同时携带 `diffLines` 与 `diffUnavailable`。判定不出行级对应时按上一条既有口径省略统计与差异行，MUST NOT 用零值或整段替换冒充判定结果。

`agent.connected`/`agent.disconnected` 的 `state` 是**封闭词表**：连接事件只取 `"connected"`，断开事件只取 `"disconnected"`，两者互不相同，客户端据此区分事件类型，不得依赖自由文本或别的取值。它们是**节点级**事件（事件信封的 `sessionId` 为空），生命周期归属于 Agent profile 的进程：同一 profile 的进程被多个会话复用时不会重复产生连接事件，Agent 连接状态也**不表达**任何单个会话的活跃程度。当前这两类事件尚无投递通道，客户端在该通道就绪前 MUST 把缺失的覆盖层呈现为状态未知，不得以其它信号推断连接状态。

会话标题的更新是**单向**的：标题只来自 Agent 的 `session/update` 中 `session_info_update` 投影出的 `session.info.changed`（见上表），据此写入会话摘要。命令目录中**不存在**任何由客户端发起的重命名会话的命令，服务端也不提供绕过该通知直接写入标题的路径；通知只携带更新时间而不含标题时标题保持不变，通知显式把标题置空时客户端呈现为未命名会话。

ACP `session/update` 判别子到 Sync event type 的完整映射（判别子取值以上游 ACP v1 固定快照为准）：

| ACP `sessionUpdate` | Sync event type |
|---|---|
| `user_message_chunk` | `user.message.delta` |
| `agent_message_chunk` | `agent.message.delta`（`agent.message.completed` 由 turn 结束时由 Broker 生成） |
| `agent_thought_chunk` | `agent.thought.delta` |
| `tool_call` | `tool.call.started` |
| `tool_call_update` | `tool.call.updated`（终态额外生成 `tool.call.completed`） |
| `plan` | `session.plan.changed` |
| `available_commands_update` | `session.commands.changed` |
| `current_mode_update` | `session.mode.changed` |
| `config_option_update` | `session.config.changed` |
| `session_info_update` | `session.info.changed` |
| `usage_update` | `session.usage.changed` |

未出现在该表中的判别子必须按未知事件降级并保留 `acp.rawJson`，不得静默丢弃，也不得把结构化事件降级成普通文本。目标行为与节奏的逐条约束见 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md)。

公共值对象：

```text
SessionSummary {
  sessionId: UUID,
  title: string | null,
  agent: { agentId: string, name: string },
  state: "idle" | "running" | "pending",
  origin: { kind: "local" }
        | { kind: "remote", ownerNodeId: UUID, originEpoch: UUID, online: boolean },
  currentMode: ModeRef | null,
  version: decimal string,
  createdAt: timestamp,
  updatedAt: timestamp,
  workspace?: WorkspaceRef | null
}

ModeRef { modeId: string, displayName: string }
WorkspaceRef { directoryId: string }
WorkspaceCatalogEntry { directoryId: string, path: string }
AgentCatalogEntry { agentId: string, displayName: string, default: boolean }

ModeState { currentModeId: string | null, availableModes: ModeRef[], version: decimal string }
SessionConfigOptionView {
  id: string,
  name: string,
  description: string | null,
  category: string | null,
  type: "select" | "boolean",
  currentValue: string | boolean,
  options?: { value: string, name: string, description: string | null }[]
}
InteractionOption { optionId: string, label: string, kind: string }
PublicError { code: string, message: string, retryable: boolean, details: object }
```

`state` 是会话的**活跃程度摘要**，取值集合只有 `idle`/`running`/`pending`，由 feature `core.session-activity.v1` 协商启用（§5.2）：存在有效未解决交互时为 `pending`；否则存在未结束任务（含已持久接受的排队任务）时为 `running`；否则为 `idle`。排队、等待原因与本轮结果分别留在 turn、交互与命令的词表里，不进入 `state`；`idle` 不是「连接正常」或「Agent 一定可用」的断言。旧七态（`queued`/`waiting_input`/`waiting_permission`/`failed`/`closed`）只允许作为**历史事件 `view` 的原文**出现：`session.read` 的 `history` 条目按原样返回旧 payload（`view` 是开放对象），不得据此改写实时摘要，实时主状态只取最新权威 `SessionSummary`。

`SessionConfigOptionView` 是 ACP `SessionConfigOption` 的公开投影，按原样保留 `id`、`name`、`description`、`category` 和 `type`；`category` 为 `model` 或 `model_config` 的条目就是模型选择项，`currentValue` 是当前选中的 `value`（`type: "boolean"` 时为 boolean）。模型和模式都不再有独立的 Sync 专用类型。

`workspace` 是 feature `core.local-catalog.v1` 门控下的**可选**字段，刻意不在 `SessionSummary` 的必填集合中：该 schema 是 `additionalProperties: false`，未协商该 feature 的客户端收到 `null`（而不是键缺席）会把整条摘要判为非法。因此键在场时的三态含义是——缺席＝未协商、`null`＝未分组、`WorkspaceRef`＝归属该目录。`WorkspaceRef` 是第 9.4 节 `workspaces` 资源的**引用形**（只有目录标识一个键），资源条目的形状是 `WorkspaceCatalogEntry`（目录标识与路径始终成对）；客户端按 `directoryId` 在同一份快照内查表，`path` 逐字取自条目、不由客户端拼接或反推。



`AgentContentBlock` v1 的公共 view 支持：

```text
{ type: "text", text: string }
{ type: "image_ref", mimeType: string, byteLength: decimal string | null, displayState: "available" | "not_fetched" | "unsupported" }
{ type: "resource_ref", uri: string, name: string | null, displayState: "available" | "not_fetched" | "unsupported" }
{ type: "unsupported", originalType: string, reason: "unsupported_by_client" }
```

公共 view 不是 ACP 原文替代品。无法形成专用 view 时必须使用 `unsupported` block 或未知事件降级，同时保留 `acp.rawJson` 或明确的 `rawUnavailable`。

### 10.4 消息通道、类型化媒体与部分结果

`core.typed-media.v1` 为消息与媒体补上类型化投影。它们不替代 ACP 原文：同一事件的 `acp.rawJson` 字节、长度与摘要保持不变，客户端只消费专用视图。

| 值对象 | 形状与语义 |
|---|---|
| `messageChannel` | `reply｜thought`。回复与思考**分别**持久化、分别折叠；Agent 未提供思考时不得生成思考消息；`agent.thought.delta` 不产生 `agent.message.completed`（§10.3） |
| `mediaReference` | `{ attachmentId, sessionId, kind: image｜audio｜file, mediaType, byteLength, sha256, displayName, availability }`；只标识内容与可用性，**不含**下载 URL、临时路径或凭据 |
| `partialResult` | `{ state: completed｜partial｜failed｜cancelled, reason }`；`partial` 表示停止/失败/断线时已收到内容被保留，MUST NOT 被客户端当作完整成功回复 |

- `mediaReference.availability` 是封闭词表：`available`（可经 §3.4 读取）、`expired`（超出保留期或已随消息清理）、`not_fetched`（只有外部 URI，未自动下载）、`unsupported`（当前端到端能力不具备该类型）。不可用是显式事实，MUST NOT 用空字节或空 URL 冒充内容。
- `AgentContentBlock` 在既有 `text`/`image_ref`/`resource_ref`/`unsupported` 之外新增 `audio_ref` 与 `file_ref`，并给 `image_ref`/`audio_ref` 增加可选 `attachmentId`：嵌入字节由 Owner 抽取为附件后才有该标识；只有外部 URI 或未取得内容时为 `null` 或缺失。`file_ref` 必然指向已封存附件。读取一律经 §3.4 的已认证连接并重新校验会话与归属，不使用长期地址。
- **媒体不阻塞事件**：媒体字节不进入事件帧或业务命令（超过 §14 消息预算的原生块按 §11.5 的 PromptContentBlock 规则整条拒绝，见 `ACP_COMPATIBILITY_MATRIX.md`）。事件只携带引用与可用性，字节由附件连接按需分块读取。
- **只在 Agent 确实提供时产生**：Agent 输出的图片/音频/资源经 `agent-host` 类型化投影进入 core；未识别的 content 仍按 `unsupported` 或未知事件降级并保留原文，MUST NOT 文本化或猜测。外部链接不因展示而自动获取（§15）。
- 历史与实时同形：`session.read` 的 `messages` 与事件 view 使用同一组 `AgentContentBlock`；保留期外读到 `availability = expired`，不再返回正文或空占位。
- **消息页的加性字段**：`schemas/sync/v1/sync.schema.json#/$defs/snapshotItem.messages` 增加可选的 `channel` 与 `partial`（同一 session.read 消息页与事件视图复用它）。它们是**加性**的：未协商 `core.typed-media.v1` 的连接收到的消息里这两个键**缺席**（不是 `null`），旧 peer 看到的 shape 逐字不变；协商后 `channel` 区分回复/思考，`partial` 表达已保留但未完成。该修改不提升 `protocolVersion`（§16.1）。
- **有效待处理交互**：`effectiveInteraction`（`interactionId`、`sessionId`、`kind`、`requestVersion`、`expiresAt`、`options`、`schema`）是服务端重新确认过的当前请求，只在协商 `core.pending-interactions.v1` 后经 `session.read` 的 `pending_interactions` 返回，规则见 §11.7。历史卡片只有定位信息，不能直接应答。

## 11. Command 与幂等

### 11.1 Command

```json
{
  "protocolVersion": 1,
  "type": "command",
  "messageId": "6dfa044f-05f8-4a02-b4a1-a4d9d034298b",
  "connectionId": "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  "connectionSequence": "11",
  "body": {
    "requestId": "4c4dafda-dd98-442e-8d55-252b75bac72d",
    "command": "session.prompt",
    "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    "payload": {
      "content": [
        { "type": "text", "text": "继续修复测试" }
      ]
    }
  }
}
```

- `requestId` 由客户端生成，并在重试时保持不变。
- 同一设备内 `requestId` 唯一；幂等键为 `(deviceId, requestId)`。
- `command`、`requestId` 和 `payload` 必须存在；`sessionId` 仅按第 11.5 节要求存在；`expectedVersion` 仅用于要求乐观并发控制的 mutation，编码为十进制字符串。
- `payload` 必须是 command-specific object，不能为 `null`、array 或任意 ACP JSON-RPC。
- 重复 request 的 command、session、`expectedVersion` 和经明确 DTO 解码后的 payload 语义必须与首次一致，否则返回 `command.idempotency_conflict`；不得用 JSON 对象键顺序判断是否相同。
- `expectedVersion` 用于有并发覆盖风险的 mutation；不适用时省略。
跨节点面由连接认证与本机资源边界约束（见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md)）。

### 11.2 Command Result

服务端返回：

```json
{
  "type": "command.result",
  "body": {
    "requestId": "4c4dafda-dd98-442e-8d55-252b75bac72d",
    "command": "session.prompt",
    "status": "accepted",
    "acceptedAt": "2026-09-17T12:11:00.000Z",
    "terminalEventId": null,
    "result": {
      "turnId": "6601828b-3eca-4cec-9a58-18ae1e0a3a14"
    },
    "error": null
  }
}
```

body 字段规则：

- `command` 必填，取值是第 11.5 节表首列的命令名，必须与原始 `command` 请求一致；查询命令的 `result` 形状由它决定。
- `acceptedAt` 仅在 `status = "rejected"` 时为 `null`；其余状态都必须是该命令首次被持久化接受的时间。
- `terminalEventId` 是 `UUID | null`：查询命令固定为 `null`，mutation 的 `accepted` 也固定为 `null`；重复查询一个已终结的 mutation 时必填，且等于该 request 唯一的 command terminal event 的 `eventId`。
- `status = "rejected"` 时 `result` 必须是 `null` 且 `error` 必须是结构化 `PublicError`；其余状态 `error` 为 `null`。
- `completed` 时 `result` 必须符合 `command` 对应的结果形状（见第 11.5 节）；`failed` 携带 `PublicError`，`uncertain` 可只给 `reason`。mutation 完成后如果没有可返回的数据（如 `session.cancel`、`session.prompt`），`result` 允许为 `null`；Node Link 对此更严（`NODE_LINK_PROTOCOL.md` §12.5 要求 `completed` 必须带非空 `result`），该差异是有意的，由两侧各自的适配层负责保证。

`status`：

| 状态 | 含义 |
|---|---|
| `accepted` | 命令已持久化并接受；异步进展通过事件发送 |
| `completed` | 查询命令已完成，或重复查询一个已成功终结的 mutation；`result` 是最终结果 |
| `failed` | 已接受的 mutation 后续失败；只在重查/重复 request 时返回，初次终态通过事件发送 |
| `rejected` | 未产生业务副作用，包含结构化 error |
| `uncertain` | 命令可能已到达外部 Agent，但崩溃恢复后无法证明结果；不会自动重新派发 |

服务端必须持久化幂等记录，并在收到相同 request 时返回相同的已知接受/终态。客户端在响应丢失后使用同一 `requestId` 重试，不能生成新 ID。

结果语义固定如下：

- 查询命令不进入异步队列，直接返回一次 `completed` 或 `rejected`。
- mutation 命令首次提交只返回 `accepted` 或 `rejected`；`accepted` 不是业务完成。
- 已接受 mutation 的终态只通过一个持久化 `command.completed`、`command.failed` 或 `command.uncertain` 事件表达，并更新幂等记录。
- 同一个 request 最多产生一个 command terminal event；领域事件（例如 `turn.completed`）可以同时存在，并使用相同 `causationRequestId`。
- 领域状态变化和 command terminal event 应在同一数据库事务提交；sequence 上先写领域事件，再写 terminal event。客户端只在看到 terminal event 或 `command.status` 终态后把命令标为完成。
- 会创建资源的 mutation（当前只有 `session.create`）在受理时只持久化该命令的幂等记录：MUST NOT 因此创建资源行（受理期的资源标识为空）；资源行与其终态 MUST 在同一次提交内落盘；后端失败时 MUST NOT 留下资源行，也不得停留在「只有受理记录、既无终态、又无资源行」的状态（第 11.5 节）。
- Daemon 恢复时发现外部 Agent 副作用无法确认，必须先持久化 `command.uncertain`，再向客户端广播。`session.create` 在受理与落账之间崩溃留下的就是这一档：本机只有受理记录（会话标识为空）、**没有**会话行，恢复把它终结为 `uncertain`，同一 `requestId` 重发返回该终态、不派发第二次创建。
- 重复提交已经终结的 request 时，`command.result` 返回当前终态及原 terminal event ID，不重新派发。
- `uncertain` 是终态。用户若明确决定再次执行，必须创建新的 `requestId`；UI 必须警告原命令可能已经影响 Agent。

产生 mutation 的幂等记录至少保留到设备被撤销且目标会话被删除；记录可以只保留 request fingerprint、接受状态和最终结果引用，不要求永久保存完整 prompt。查询类命令可以只在连接期去重。实现不能在仍允许客户端重试时静默删除 mutation 幂等记录并把旧 request 当作新命令执行。

命令处理建议顺序：

```text
schema -> authenticated device -> state/version
-> idempotency lookup/reserve -> durable accept
-> session actor -> Agent -> durable events -> broadcast
```

（跨节点面与 Sync 面同一顺序：认证先于任何业务读取与执行）

### 11.3 首批命令

Sync v1 接受的命令集合（与 `compatibility/commands/v1/commands.json` 中 `transport` 含 `sync` 的命令一致）：

```text
session.list
session.read
command.status
session.mode.list
session.config.list
workspace.browse
workspace.mkdir
session.prompt
session.cancel
elicitation.respond
session.mode.set
session.config.set
permission.resolve
session.create
session.files.list
session.files.read
session.git.status
session.git.diff
```

Sync v1 尚未定义，或仅允许 Node 本地管理入口：

```text
session.delete
session.resume
local.agent.configure
local.device.manage

local.node.rotate-key
local.audit.export
```

`session.create` 在两个传输面同名，但前提与载荷不同。**Sync 面**以「该连接已通过设备认证」为唯一前提——设备权限层已删除，因此不再有 scope 或 grant 判定（提交方就是本机自己的设备）。payload 只允许 `agentId`、目录引用二者之一（本机已记录的目录标识 `directoryId`，或客户端提交的**绝对**目录 `workspacePath`）与一个可选的显式布尔开关 `createIfMissing`（「目录不存在则先创建」）；不接受 `templateParams`、MCP 配置或凭据字段，出现任何其它键一律 `protocol.schema_invalid`。`createIfMissing` 与 `directoryId` **互斥**（别名解析出的目录必然已存在），两者同时出现同样按 `protocol.schema_invalid` 拒绝——不解析别名、不触碰文件系统。本机按既有口径校验生效的目录来源（绝对路径、存在且为目录、以 `canonicalize` 结果为权威值）：形状非法（相对路径、含 `..`、别名未在本机登记）以参数类错误拒绝且不触碰文件系统；形式合法但无法解析（目录被删除、被换成文件、规范化失败、权限不足）以 `internal.unavailable` 拒绝；两类都不创建会话、不启动 Agent 进程、不回退到任何替代目录。开关为真且目录不存在时本机**先创建它**（连同缺失的父层）再启动 Agent，创建失败（父层不可达、无写权限、路径上已有非目录成分）同样以 `internal.unavailable` 拒绝；按它创建出的目录**不会**被登记成 workspace 别名，因此该会话的目录归属是「未分组」，已创建的目录在后续失败时也**不删除**。**Node Link 面**仍额外要求 `exportId`、Export 发布的 workspace template 与 Owner 的 `grant.remote-work`，且请求不得携带任意 Owner 路径（只允许 `directoryId`，也不接受 `createIfMissing`）。命令名与 grant 的权威映射以 `compatibility/commands/v1/commands.json` 和 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 第 10.2 节的表为准，本节只声明 Sync 面接受哪些命令名；第 11.5 节表首列是 Sync v1 接受的完整命令集合。

未知命令返回 `command.unsupported`。底层 Agent 不支持的已知能力返回 `capability.unsupported_by_agent`，Broker 无法表达时返回 `capability.unsupported_by_broker`，不能伪装成功。

### 11.4 `command.status`

`command.status` 是查询命令，body 的 `requestId` 是本次查询 ID，其 payload 指向目标 mutation：

```json
{
  "requestId": "a-new-query-request-id",
  "command": "command.status",
  "payload": {
    "targetRequestId": "4c4dafda-dd98-442e-8d55-252b75bac72d"
  }
}
```

完成结果：

```json
{
  "targetRequestId": "4c4dafda-dd98-442e-8d55-252b75bac72d",
  "state": "accepted",
  "acceptedAt": "2026-09-17T12:11:00.000Z",
  "terminalAt": null,
  "terminalEventId": null,
  "result": null,
  "error": null
}
```

`state` 为 `accepted|completed|failed|uncertain|rejected`；`rejected` 表示该 target request 已终结为未产生业务副作用的拒绝，`acceptedAt` 与 `terminalAt` 均为 `null`，`error` 携带结构化原因。设备只能查询自己提交的命令；不存在或不属于该设备的请求统一返回 `command.not_found`，避免侧信道。

`command.status` 的完成结果就是 `CommandStatusRecord`：`{ targetRequestId, state, acceptedAt, terminalAt, terminalEventId, result, error }`，字段语义与第 11.2 节一致。

协商 `resource.stateless-origin.v1` 后，`command.status` 的 payload 必须另带 `remoteTarget = { ownerNodeId, sessionId: UUID | null }`（§9.7，design D5）：Access 直接向原 Owner 查询命令的接受/终态，不凭本地记录判断，也不得因为本地没有该 `requestId` 就回答 `command.not_found` 或自动重发。`sessionId` 为 `null` 只用于尚未建成会话的 `session.create`，其他会话操作必须给出原来源会话。Owner 以既有 `(ownerNodeId, accessNodeId, requestId)` 隔离与参数冲突合同校验；不同设备仍必须生成全局随机且稳定的 `requestId`，碰撞按既有 `command.idempotency_conflict` 拒绝，Access MUST NOT 静默换号。未协商该 feature 的连接 MUST NOT 发送 `remoteTarget`；已协商却缺少它按 `protocol.schema_invalid` 拒绝。

### 11.5 v1 Command Schema

字段约定：表中未列出的 payload 字段在 v1 中拒绝；`{}` 表示必须是空 object。`sessionId` 是 command body 顶层字段，不在 payload 内重复。

| Command | 类别 | `sessionId` | Payload | 成功结果/终态 |
|---|---|---|---|---|
| `session.list` | query | 禁止 | `{}` | `completed { sessions: SessionSummary[] }` |
| `session.read` | query | 必须 | `{ include: string[] }`; include 只允许 `messages,turns,pending_interactions,config_options,capabilities,history`；Sync 面另接受可选 `before`（复合游标）、`beforeHistory`（会话内事件序号，十进制字符串）与 `limit` | `completed SessionReadResult`，`resources` 与第 9.4 节同名元素同形并附必填 `hasEarlier` 与只描述 `history` 这一页的可选 `historyHasEarlier`；**协商 `resource.stateless-origin.v1` 后**另带 `readCursor`（本次读取原子捕获的来源位置 R）、同次读取的完整 `sessionSummary` 与 `messageCoverage[]{messageId, through, lastDeltaIndex}`（design D4；`through` 等于 R，`lastDeltaIndex` 为十进制字符串或 `null`），未协商时三者缺席；imported 会话回源 Owner。**协商后**：`pending_interactions` 在 `core.pending-interactions.v1` 被选中时返回每个当前有效请求的完整 `effectiveInteraction`（含 `requestVersion`，见 §11.7），未协商时回 `capability.unsupported_by_broker`；imported 会话的 `history` 在 `core.remote-history.v1` 被选中时回源 Owner 分页，未协商时同样显式不支持。`turns`/`capabilities` 与未协商的 imported 明细仍回 `capability.unsupported_by_broker`（见第 9.4 节），客户端 MUST NOT 把不支持当成「会话无内容」或「已到最早一条」 |
| `command.status` | query | 禁止 | `{ targetRequestId }`；**协商 `resource.stateless-origin.v1` 后**必须另带 `remoteTarget`(`{ ownerNodeId, sessionId: UUID \| null }`)——直接指向原 Owner 与其来源会话，`sessionId` 为 `null` 只用于尚未建成会话的 `session.create`；未协商连接 MUST NOT 发送该键 | `completed CommandStatusRecord` |
| `session.mode.list` | query | 必须 | `{}` | `completed ModeState { currentModeId, availableModes: ModeRef[], version }` |
| `session.config.list` | query | 必须 | `{}` | `completed { configOptions: SessionConfigOptionView[], version }` |
| `workspace.mkdir` | mutation | 禁止 | `{ parentPath, name }`；父路径 1..4096 字符，单组件名字 1..255 字符 | `accepted` 后由持久终态报告 `completed { created: true }`；结果不含路径；受 `core.workspace-mkdir.v1` 门控 |
| `workspace.browse` | query | 禁止 | `{ path: 绝对路径或 null }`——只含一个父路径字段：绝对路径（1..=4096 字符）请求该层，`null` 请求该节点的根列表（该键必须存在，不可缺席） | `completed { path, entries: WorkspaceBrowseEntry[], truncated }`；`entries` 只含该层的**直接子目录**（`{ name, path, registered }`），不含文件条目、文件内容、大小、时间戳或权限；条目按名称以确定顺序（Unicode 码点升序）排列，超过 §14 的单层条目上限时只返回上限内条目并置 `truncated: true`（不静默丢弃、不整次失败）；受 feature `core.workspace-browse.v1` 门控，未协商时在触碰文件系统之前返回 `protocol.feature_required`；只读：不产生任何登记、会话或审计副作用 |
| `session.prompt` | mutation | 必须 | `{ content: PromptContentBlock[], submissionId?: UUID }`；`submissionId` 只指向本会话/本主体已 `manifest_sealed` 的附件清单，出现未封存或不存在的清单返回 `attachment.transfer_not_found`/`attachment.binding_mismatch`；未协商 `core.attachment-transfer.v1` 时该键缺席 | `accepted { turnId }`，随后 turn/domain event 和 command terminal event |
| `session.cancel` | mutation | 必须 | `{ turnId }` | 取消请求生效后 `command.completed`；目标已经终态则幂等完成 |
| `elicitation.respond` | mutation | 必须 | `{ interactionId, requestVersion?, action, values }`；`requestVersion` 只在协商 `core.pending-interactions.v1` 后必须携带，未协商的旧 peer 可省略 | `action` 为 `submit|decline|cancel`（对齐 ACP `accept`/`decline`/`cancel`）；`requestVersion` 必须等于当前有效请求的版本（§11.7），不符返回 `interaction.version_conflict` 且不消耗请求；`submit` 的 `values` 是 `object|null`（`null` 对应 ACP `content: null`），`decline`/`cancel` 的 `values` 必须为 `null`；校验后产生 `elicitation.resolved` |
| `session.mode.set` | mutation | 必须 | `{ modeId }`，body `expectedVersion` 必须存在 | `session.mode.changed` 后 `command.completed` |
| `session.config.set` | mutation | 必须 | `{ configId, value }`，body `expectedVersion` 必须存在 | 先发 `session.config.changed`，再发 `command.completed` |
| `permission.resolve` | mutation | 必须 | `{ interactionId, requestVersion?, optionId }`；`requestVersion` 只在协商 `core.pending-interactions.v1` 后必须携带，未协商的旧 peer 可省略 | `permission.resolved` 后 `command.completed`；版本不符或请求已失效返回 `interaction.version_conflict`/`interaction.unavailable` 且不消耗请求 |
| `session.create` | mutation | 禁止（会话尚不存在） | 只允许四个键：`agentId`（必填）、`directoryId`（本机已记录目录的标识）、`workspacePath`（客户端提交的绝对目录，1..=4096 字符）与可选的布尔键 `createIfMissing`（显式的「目录不存在则先创建」开关）；`directoryId` 与 `workspacePath` **恰好出现一个**，且 `createIfMissing` 与 `directoryId` **互斥**（同时出现即 `protocol.schema_invalid`）；出现任何其它键（`cwd`、`templateParams`、MCP 配置或凭据字段）一律 `protocol.schema_invalid` | `accepted { result: null, terminalEventId: null }` → `completed { sessionId, session: SessionSummary }` 并附唯一 `terminalEventId`；目录形状非法（相对路径、含 `..`、别名未登记）以参数类错误拒绝且不触碰文件系统，形式合法但本机解析失败以 `internal.unavailable` 拒绝，两者都是**接受前**的拒绝（零副作用）；`createIfMissing` 为真且目录不存在时先创建它（连同缺失的父层）再启动 Agent，创建失败以 `internal.unavailable` 拒绝且不启动 Agent、不留会话行，按它创建出的目录不登记成别名（该会话的目录归属是「未分组」）且在后续失败时**不删除**；受理只落该命令的幂等记录（会话标识为空、不写会话行），会话行与终态在**同一次提交**里落盘，因此已接受的创建按「能否证明 Agent 侧未产生会话」终结——进程未能启动、Agent 明确返回错误、请求未发出 ⇒ `failed`（复用既有错误码 `internal.unavailable`，成因写在终态 `message` 里，不新增错误码），短请求超时、请求发出后连接断开、会话已建立之后才失败或崩溃窗口 ⇒ `uncertain`；上述接受前的拒绝与**建立失败**的 `failed`/`uncertain` 终态都零会话行、不下发会话标识；只有「会话确实已建立、仅终态结果无法投影」那一档 `uncertain` 例外——那时会话行与 `uncertain` 终态在**同一次提交**里落盘（`CORE_PORTS_AND_STORAGE.md` §6 第 20 条），仍然不下发会话标识。同一 `requestId` 重发返回首次结果，不产生第二个会话 |
| `session.files.list` | query | 必须 | `{ path, hideIgnored, after? }`；`path` 是根内相对目录，空字符串表示会话根；`hideIgnored` 必填布尔值；`after` 是可选翻页位置 | `completed { path, gitState, entries: SessionFileEntry[], next? }`；条目含根内路径、名称、类型（file/directory/link/other）、可打开性与受限原因；`gitState` 为 `available｜not_repository｜unavailable`；每页最多 128 项，`next` 为续读位置；受 `core.session-files.v1` 门控 |
| `session.files.read` | query | 必须 | `{ path, purpose, offset, version? }`；`purpose` 为 `preview｜download`，`offset` 是规范十进制字符串，`version` 是连接期不透明标识 | `completed { path, kind, sizeBytes, version, offset, dataBase64, eof, previewTruncated }`；`kind` 为 `text｜image｜binary`，单块原始字节最多 32 KiB；版本缺失时建立快照，变化时明确要求重读 |
| `session.git.status` | query | 必须 | `{ after? }` | `completed { gitState, entries: GitStatusEntry[], next? }`；每项含路径、状态（added/modified/deleted/renamed/untracked）、可用的旧路径及冲突/二进制标识；只覆盖会话根内文件 |
| `session.git.diff` | query | 必须 | `{ path }` | `completed { path, base, kind, diffLines?, unavailable?, truncated }`；`base` 为 `head｜empty`，`kind` 为 `text｜binary｜conflict`；文本差异最多 400 行 / 64 KiB / 单行 4096 字符 |
| `session.resume` | mutation | 仅 Node Link（经 `sessionRef`） | `{}` | 仅 Node Link（见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md)）；`accepted` 的 `result` 为 `null`，成功终态为 `completed SessionResumeResult`（`remoteSessionRef` + `sessionMeta`）；目标 Agent 未宣告 `sessionCapabilities.resume` 时以 `nodelink.command.unsupported` 失败；Sync v1 收到返回 `command.unsupported` |

本表首列是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 的完整命令集合（19 条），也是 `SECURITY_DESIGN.md` 第 10.2 节的展开来源。`schemas/sync/v1/command.schema.json` 的 `command` 枚举包含其中 `transport` 含 `sync` 的 18 条；`session.resume` 只经 Node Link 接受，Sync v1 收到时按 §4.2 返回 `command.unsupported`，不出现在该枚举里。`schemas/node-link/v1/command.schema.json` 的枚举包含 `transport` 含 `node_link` 的全部 13 条。任一处增删命令名都必须同步修改 `commands.json`、两个协议 schema、`SECURITY_DESIGN.md` 第 10.2 节与本节。

`session.create` 在两个传输面同名但 payload 不同，本表是 Sync 面的形状；Node Link 面的 payload 是 `{ agentId, directoryId }`（见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 第 12.7 节），同样**不接受** `createIfMissing`。两个面共用同一套命令名、幂等与终态语义：先受理再执行（受理即持久化幂等记录，客户端拿到的首次响应是 `accepted`，其 `result` 与 `terminalEventId` 均为 `null`），最终以 `completed`/`failed`/`uncertain` 之一终结，同一 `requestId` 重发必须返回首次结果。`session.create` 的受理只写幂等记录（会话标识为空、不写会话行）：Agent 侧返回后的一次提交里落账——成功时写会话行（含 ACP 会话标识、core 自己解析的 cwd 与目录别名）、回填幂等行的会话标识并写 `completed`，失败时只写终态、**零会话行**。`session.create` 的 `accepted` 必须携带 `result: null`——这是服务端语义要求（与 `command.status` 的 `acceptedAt` 同类），schema 的 `accepted` 分支不对 `result` 取值做约束，因此客户端不得依赖它在 wire 上被强制。

`createIfMissing` 是**显式**开关而不是「路径不存在就默默创建」：后者会把一次笔误变成一棵被建出来的目录树（design D4）。它取布尔值、缺席即假，键在场时只接受 `true`/`false`（没有 `null` 分支）；它与 `directoryId` 互斥，两者同时出现按 `protocol.schema_invalid` 拒绝且不解析别名、不触碰文件系统。开关为真且目录不存在时先创建（含缺失的父层）再启动 Agent；路径上已存在非目录成分或创建失败（父层不可达、无写权限）按服务端不可用类错误（`internal.unavailable`）拒绝，不覆盖、不删除已有内容，也不启动 Agent。系统 **MUST NOT** 在后续失败时删除已创建的目录——删除要区分「哪些层是这次建的」，而误删用户目录的代价远高于留下一个空目录；按它创建出的目录也不成为已登记别名，该会话的目录归属因此是「未分组」。

`workspace.browse` 是 Sync 面的目录浏览查询，只经 Sync 接受（Node Link 的请求不得携带任意 Owner 路径）：

- **请求只含一个父路径字段**：`{ path }`，取值是绝对路径或 `null`（`null` 表示请求该节点的根列表，例如各盘符或根挂载点）。该键必须存在，不可缺席；`null` 只表示根列表，不表示「服务端替我挑一个」。
- **结果只含一层**：`{ path, entries, truncated }`，`path` 逐字回显请求的父路径，`entries` 是这一层的**直接子目录**，每项 `{ name, path, registered }`（名称、绝对路径、是否已是本机已登记的 workspace）。结果 MUST NOT 含文件条目，也 MUST NOT 含任何文件内容、字节大小、修改时间或权限位；MUST NOT 回退到父层或根列表来「凑」一个结果。
- **顺序确定**：条目按名称以 Unicode 码点升序排列，同一层在两次调用中给出相同顺序（不依赖平台的本地化排序，也不依赖文件系统枚举顺序）。
- **默认本机浏览可见性**：Windows 的隐藏或系统属性子目录不进入结果（在单层上限截断之前过滤）；这不是创建授权或禁止路径规则。实现与属性读取边界见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5.5。客户端可以对收到的条目施加界面排序，不改写结果数据；PWA 的显示排序以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) §4.1 为准。
- **形状校验与失败分类**：不是绝对路径、或含父目录引用（`..`）时以参数类错误拒绝，且 **MUST NOT 触碰文件系统**；形状合法但该层不存在、不可读或不是目录时以服务端不可用类错误（`internal.unavailable`）拒绝。两类都不返回任何条目。**在触碰文件系统之前**先检查 feature 协商：未协商 `core.workspace-browse.v1` 的连接提交该命令时返回 `protocol.feature_required`，不得读取任何目录，也不得返回空列表冒充成功。
- **只读、无副作用**：不写任何登记、缓存或会话状态，不改变本机 workspace 登记表，也不为被查看的目录补出任何后续权限或别名；一次浏览完成不产生审计记录，审计动作词表不因此扩张（design D6）。经它选定但未登记的目录创建的会话，其目录归属仍是「未分组」。
- **条目上限**：单层条目数上限是固定 v1 常量（§14）；超过上限时只返回上限内的条目并把 `truncated` 置真——不得静默丢弃，也不得因超限而整次失败。结果本身超过单条消息上限时按本节末的既有规则返回 `resource.result_too_large`，不得截断后伪装成完整结果。


`session.files.list` / `session.files.read` / `session.git.status` / `session.git.diff` 是 Sync 面带 `core.session-files.v1` 的四条**只读查询**（Node Link 不登记它们）：

- **认证、协商与来源先于 I/O**：四条命令都在 body 顶层要求 `sessionId`，并禁止 `expectedVersion`。已认证连接未协商 `core.session-files.v1` 时，服务端在任何文件系统读取或 Git 启动之前返回 `protocol.feature_required`（`details.features` 给出缺的 feature），不得返回空结果冒充成功。目标会话必须是本机 owned 会话；imported 会话一律返回 `capability.unsupported_by_broker`，不访问 Access 本机文件系统，也不向 Owner 转发这些查询。设备撤销按既有认证规则拒绝。
- **根是会话实际工作目录**：服务端从该会话持久化的实际工作目录（`SessionStore::load_recovery().workspace_cwd`）确定根；未分组会话同样使用其实际 cwd。目录登记/别名不参与根解析，也不存在回退目录。根不存在、不可读或恢复列为空时返回 `internal.unavailable`，MUST NOT 用空列表或其它目录冒充成功。
- **路径只有根内相对形状**：请求只接受根内相对路径（`session.files.list` 的空字符串表示根），结构是「非空分量 + 单个 `/` 或 `\` 分隔符」。绝对路径、盘符、UNC、父引用（`..`）、`.` 分量、控制字符（含换行与制表符）、NUL、任意冒号（Windows alternate data stream 与盘符）以及任一分量为 `.git`（大小写不敏感）的路径一律按 `protocol.schema_invalid` 拒绝，且判定在文件/Git 端口调用之前。隐藏 `.git` 目录与 worktree 的 `.git` 管理文件既不出现在枚举结果中，也不能被直接读取。实现仍须以实际打开的对象验证根边界；本层形状校验不能替代适配器的安全打开。
- **`session.files.list`**：`{ path, hideIgnored, after? }`，`hideIgnored` 必须显式给出。结果 `{ path, gitState, entries, next? }`；`path` 逐字回显请求（根为空串），条目含根内 `path`、`name`、`kind`（`file｜directory｜link｜other`）、`openable` 与不可打开时的 `restricted`（`outside_root｜unreadable｜unsupported`）。默认包含隐藏、系统属性与 Git 忽略项，只排除 `.git`；`hideIgnored` 为真时按 Git 有效忽略规则在分页前过滤。条目按目录优先、名称 Unicode 码点、路径打破并列的确定顺序排列；每页最多 128 项（§14），达到上限时给出 `next`，客户端据此继续读取，不得把一页当作已列完。`gitState` 区分可查询、非仓库与本机 Git 不可用；非仓库不把所有文件误判为被忽略。根外链接仍作为条目出现，但 `openable` 为假，且不下发目标绝对路径。
- **`session.files.read`**：`{ path, purpose, offset, version? }`；`purpose` 为 `preview｜download`，`offset` 是规范十进制字符串。结果 `{ path, kind, sizeBytes, version, offset, dataBase64, eof, previewTruncated }`；`kind` 为 `text｜image｜binary`，`sizeBytes`/`offset` 是规范十进制字符串，`dataBase64` 是无填充 base64url。`version` 缺席时服务端建立**连接绑定**的只读内容快照并从 `offset` 返回第一块；带 `version` 时从同一版本续读。文件在后续块之前变化时服务端 MUST 废弃快照并明确要求重新读取，MUST NOT 拼接两个版本。单块原始字节最多 32 KiB；文本预览最多 256 KiB、栅格图片预览最多 4 MiB、一次下载最多 64 MiB；超过预览预算只取得预算内内容并置 `previewTruncated`，超过下载预算返回 `resource.result_too_large`，不得悄悄只下载前一段。快照版本绑定连接、设备、会话与相对路径，不得跨连接当作 bearer 使用。
- **`session.git.status`**：`{ after? }`；结果 `{ gitState, entries, next? }`。每个文件只出现一次，合并已暂存、未暂存与未跟踪状态；每项含根内路径、`status`（`added｜modified｜deleted｜renamed｜untracked`）、可安全确认时的 `previousPath`，以及 `conflicted`/`binary` 标记。只覆盖会话根内的文件；根外的改名按根内删除/新增表达，不下发根外旧路径。非仓库与 Git 缺失是成功结果里的显式 `gitState`，不是空列表。
  协商 `core.session-git-stats.v1` 后，文本条目可附成对的 `addedLines` / `deletedLines`（0–4294967295 整数），表示相对 HEAD 与当前工作树的新增/删除行数，不把已暂存与未暂存统计相加；空仓库与未跟踪文件以空文件为基准。二进制、冲突、读取失败或超出统计预算时省略两个字段，不能以零冒充未知。未协商时服务端省略两字段，兼容旧客户端的严格解码；新客户端接旧主机时保留状态展示，不伪造行数。统计只读且不持久化正文：每页至多一次有界 numstat 查询，空基准文件的累计读取预算为 8 MiB，统计失败不使已有 Git 列表消失。
- **`session.git.diff`**：`{ path }`；结果 `{ path, base, kind, diffLines?, unavailable?, truncated }`。`base` 为 `head｜empty`（无 HEAD 时以空基准比较），`kind` 为 `text｜binary｜conflict`。差异默认比较 HEAD 与当前工作树，不把已暂存与未暂存补丁相加；文本差异最多 400 行、合计 64 KiB、单行 4096 字符，超出时只带可行子集并置 `truncated`。二进制与冲突不伪造成文本；非仓库、Git 不可用或差异过大时只带 `unavailable`（`not_repository｜git_unavailable｜too_large`），`diffLines` 与 `unavailable` 不得同时出现。实现 MUST NOT 运行仓库配置指定的 external diff、textconv、hook、fsmonitor 或 pager，也不修改索引/工作树。
  协商 `core.session-git-compare.v1` 时，成功的文本结果可附 `comparison: { before, after }`：每侧为至多 64 KiB UTF-8 的完整原文，不受补丁 400 行限制；读取不完整、超限、非 UTF-8、二进制或冲突时省略，不以截断内容冒充全文。`before` 是根内当前路径或安全确认的旧路径在 HEAD 中的原文，新增/未跟踪/无 HEAD 时为空；`after` 是通过根内安全句柄读取的当前文件，删除时为空。内容只存在于本次查询和客户端内存，不落入 Access 持久正文缓存。`truncated` 仍只描述 `diffLines`；完整 `comparison` 在场时客户端按两份原文对比。未协商时服务端省略字段，旧客户端继续按补丁解码；新客户端访问旧主机时明确展示“局部对比”，不声称已取得全文。
- **查询语义**：四条命令都同步返回一次 `completed` 或 `rejected`，不产生 mutation 受理，也不写命令终态事件；`completed` 的 `terminalEventId` 固定为 `null`（§11.2 的 `allOf` 已把这四条列入查询集合）。重复 query 沿用连接期去重；主动刷新必须使用新的 `requestId`，不得复用旧结果。查询不改变会话、目录登记、Git 索引或工作树。
- **错误映射**：非法路径/形状 → `protocol.schema_invalid`；会话不存在 → `session.not_found`；imported 会话 → `capability.unsupported_by_broker`；文件 I/O 或安全打开失败、根不可用 → `internal.unavailable`；一次下载超过 64 MiB（端口返回 `PortError::InvalidRequest(RESULT_TOO_LARGE)`）或整体结果超限 → `resource.result_too_large`；限流 → `resource.rate_limited`（遵守既有 `retryAfterMs`）。非仓库、二进制与冲突是成功结果里的显式状态。错误消息与日志 MUST NOT 含文件正文、内容版本令牌、Git stderr 原文或根外绝对路径。
- **兼容**：旧客户端不协商 `core.session-files.v1`，因此不会发送这四条命令，原有会话与 `workspace.browse` 行为逐字不变；新客户端连接未宣告该 feature 的旧主机时不派发这些查询，并在界面上明确降级，不得读取本机替代目录或伪造空列表。

服务端接线与连接生命周期（实现约束，不改变上面的 wire 形状）：

- **feature 宣告以真实实现为前提**：`core.session-files.v1` 只在四条查询已接到用例面、内容版本释放与组合根装配完成后进入服务端支持集合；`auth.server_challenge.selectedFeatures` 是客户端判断「这台主机是否真的能查文件」的唯一依据，不得提前宣告。
- **门控先于一切本地读取**：每条命令依次经过连接期去重与限流 → 设备认证（持久信任行）→ `core.session-files.v1` 协商判定，之后才进入 core 的授权、会话来源判定（owned / imported）与文件/Git 端口。未认证、设备已撤销、未协商或 imported 会话都在文件系统读取与 Git 进程启动**之前**失败；未协商时不产生任何端口调用。
- **每连接一个活动内容版本**：`session.files.read` 省略 `version` 时建立新快照，成功后才登记该版本，并用 `(设备, 会话, 相对路径, 版本)` 四个维度绑定到发起它的连接；同一连接换新版本前先释放旧版本。带 `version` 续读必须命中本连接登记过的同一组维度，跨连接（跨设备）、跨会话或跨路径的令牌一律显式拒绝（`internal.unavailable`），不得退化成按令牌直接读取。
- **释放责任在接线层**：连接正常关闭、传输断开、设备撤销与 daemon 关闭都经过同一连接收尾路径；服务端在该路径上调用端口 `release_content` 释放该连接的活动版本。daemon 关闭时还会对原生端口调用一次 `release_all` 兜底。快照只存在于内存，不写临时正文文件；空闲到期同样走 `release_content`。
- **消息预算与限流**：四条命令与既有查询同属 `observe` 限流族（§14），超限沿用 `resource.rate_limited` + `details.retryAfterMs`。`completed` 结果在入队前按**编码后的完整信封字节**与连接的单条消息上限比较；超限即返回 `resource.result_too_large`，绝不截断后冒充完整结果。单块内容已在端口层限制为 32 KiB，服务端仍按整帧预算复核，不依赖这个上限保证装得下。
- **安全响应头**：托管前端为安全预览图片只增加 `img-src 'self' blob:`（Blob URL 由页面从已认证查询得到的字节创建）；`script-src`、`connect-src`、`object-src`、`base-uri` 与 `frame-ancestors` 保持原策略，不放宽为任意外部来源、`data:` 或内联脚本。

`session.read` 是会话明细的唯一入口（第 9.4 节：六类明细资源不进快照），因此它在 Sync 面带分页参数：

- **`before` 是由消息创建时间与消息标识复合而成的游标**，`{ createdAt, messageId }`，两个键都必需。服务端 MUST 按 `createdAt` 升序返回一页，并在同一 `createdAt` 的多条消息之间给出稳定且可重复的次序（以 `messageId` 作 tie-breaker），使相邻两页不重不漏。`createdAt` 是 Daemon 持久化时间（第 10.2 节），不是客户端时间。
- **排序 MUST NOT 依赖 `messageId` 的数值或字典序**（第 3.3 节「排序不得依赖 UUID」）。`messageId` 只在同一创建时刻内部定序；消息标识的数值次序与创建时间次序不一致时，结果仍按创建时间给出。
- **单字段游标被拒**：只提供 `messageId` 或只提供 `createdAt` 的 `before` 按 `protocol.schema_invalid` 拒绝，服务端 MUST NOT 猜测或补全缺失的排序分量。`before` 只接受这两个键，出现别的键（例如 `offset`）同样拒绝——偏移量分页不是 v1 的形状。
- **省略 `before` 返回最新一页**；客户端把上一次结果中最早一条的 `(createdAt, messageId)` 作为下一次请求的 `before`，得到紧接在它之前的更早一页，不与上一次重叠也不跳条。同一会话在明细未变化时以同一 `before` 重复读取 MUST 返回相同结果。
- **默认页是最近 20 条用户输入**（`role: "user"` 的消息数）。用户输入与 turn 边界天然接近，是用户能数得清的量，服务端按此计数取页，使一页体积对用户可预期。
- **`limit` 是可选的整数码数**（`minimum: 1`，与 `chunkCount` 同属第 3.3 节的结构性常量一类，不走十进制字符串规则）。省略时用服务端配置默认页；**超过服务端为该会话配置的明细条数上限时，服务端收敛到自己的上限并照常返回，MUST NOT 因此让请求失败**——因此 schema 上没有 `maximum`。
- **到达最早一条时**结果给出 `hasEarlier: false`，且不返回空占位；客户端据此停止翻页。
- **一页包含该页边界内的全部消息**：服务端按页内用户输入计数，但返回的消息还包括页边界之后的 Agent 回复与思考——两段用户输入之间可以有任意多条 Agent 正文（回复 + 思考），服务端一直往前读到覆盖页首用户消息之前的那条为止，MUST NOT 因内部页大小（200 条）静默截断；那会让更早的 Agent 消息永远无法通过 `before` 取回。超过单次读取的硬上限时返回 `resource.result_too_large`，而不是一份缺条目的页。
- **未协商 `core.typed-media.v1` 时消息正文也降级**：`messages` 页的 `agentContentBlock` 与事件视图共用同一个判定——`audio_ref`/`file_ref` 整块不出现、`image_ref` 不带 `attachmentId`，`channel`/`partial` 亦缺席。旧 peer 的封闭词表因此不会遇到无法解码的块（否则整条 `session.read` 结果会失败，而不是少显示一张卡片）。
- **保留窗口之前的游标显式不可读**：请求的 `before` 早于该节点对与会话消息的保留窗口、且更早的消息已按保留策略清理时，服务端返回 `sync.cursor_invalid`（`details.reason = "cursor_expired"`），MUST NOT 静默返回更少的元素让客户端误以为已到最早一条。

结构化历史条目（`include` 含 `history`）走**同一命令、独立的一套游标与页标记**（design D2），因此上面关于消息的规则都不因它而改变：

- **`beforeHistory` 是会话内事件序号**（十进制字符串，提交时分配，天然单调唯一）。它与消息的 `{ createdAt, messageId }` 复合游标是两套字段：同一请求可以同时给出两者，各自翻各自的页，任一方都不改变另一方的页边界；`limit` 是本次请求里**每一页各自**的单页上限，不把两页合并计数。
- **条目按会话内序号升序返回**，省略 `beforeHistory` 时返回最新一页；客户端用这一页最早一条的 `sessionSequence` 作为下一次的 `beforeHistory`，得到紧接在它之前的一页，不重不漏。**到达最早一条时** `historyHasEarlier: false`，且不返回空占位。
- **游标不可由其它标识代替**：`beforeHistory` 只接受十进制序号，事件标识（`eventId`）不是它的替身，服务端 MUST NOT 猜测或补全。请求的游标早于该节点对结构化事件的保留窗口、且更早条目已按保留策略清理时，同样返回 `sync.cursor_invalid`（`details.reason = "cursor_expired"`），MUST NOT 静默返回更少的条目让客户端误以为已到最早一条。
- **条目与实时事件同形**：每条携带 `eventId`、`eventType`、`sessionSequence`、`createdAt` 与结构化 `view`，以及按 `acp.raw-payload.v1` 协商结果出现的 `acp`（原文逐字节保真；因单条上限、保留期或存储原因不可用时按已登记原因标记为不可用，不得用占位内容冒充）。`view` 是开放对象：协议未定义的扩展字段原样下发。哪些事件类型产生条目见第 10.2 节。
- **未协商 `core.session-history.v1` 时不下发**：客户端未通过 feature 协商该能力却请求了 `history` 时，服务端按第 4.2/12.2 节返回明确的不支持类错误，MUST NOT 静默忽略该 include 后返回看似完整的空结果。
- **历史条目只包含结构化事件**：承载消息正文的事件（`user.message.delta`/`agent.message.delta`/`agent.thought.delta`/`agent.message.completed`）不作为条目下发——消息正文的权威只在 `messages`；已按保留策略压缩或清理的事件同样不产生条目，也不以空条目占位（完整清单见第 10.2 节）。

`SessionReadResult` 的 `resources` 每个键都可缺席，但 `hasEarlier` 是必填布尔值，指示本次返回的这一页**消息**之前是否仍有更早内容；它缺席即形状错误，客户端不得默认成 `false`。只描述结构化历史条目那一页的同名结论由**可选**的 `historyHasEarlier` 承担：两者互相独立，`historyHasEarlier` 的缺席表示本次没有请求 `history`，它也不改变 `hasEarlier` 的消息语义。

**分页的传输面**（design D3、D11）：`before`、`beforeHistory`、`history` 这个 include 取值与 `limit` 在 `schemas/sync/v1/command.schema.json` 生效。`schemas/node-link/v1/command.schema.json` 与 `crates/node-link-protocol` 的同名命令 DTO 在协商 `node-link.remote-history.v1` 后也接受 `before`/`limit`（与 Sync 面同名同义，`limit` 为结构常量整数）与 `history`/`beforeHistory`；未协商该 feature 时这些键缺席，按既有形状发起、不带任何分页参数的 Node Link 调用方照常返回该会话明细，MUST NOT 因缺少分页参数被拒绝，也 MUST NOT 因出现 Sync 面的取值或字段而改变形状或失败。Access 的 imported `session.read` 因此可以把本地客户端的分页参数原样转给 Owner，并把 Owner 返回的同一 `sessionReadResult` 形状投影回本地；Access 仍不持久化正文。

`ModeState` 是 ACP `SessionModeState` 的公开投影：`currentModeId` 可为 `null`，`availableModes` 的每一项是 `ModeRef { modeId, displayName }`。`SessionConfigOptionView` 定义见第 10.3 节，`session.config.set` 的 `value` 必须是该 option 当前 `type` 允许的取值（`select` 用 `string`，`boolean` 用 boolean）。

`PromptContentBlock` 有三个封闭分支（`schemas/sync/v1/common.schema.json#/$defs/promptContentBlock`）：

```json
{ "type": "text", "text": "..." }
{ "type": "attachment_ref", "attachmentId": "...", "mediaType": "...", "byteLength": "...", "sha256": "..." }
{ "type": "workspace_path_ref", "path": "src/main.rs" }
```

- `text` 始终可用。`attachment_ref` 只在协商 `core.attachment-transfer.v1` 且附件已封存、已绑定本会话/本主体时接受；`workspace_path_ref` 只表示**当前会话工作目录内**的相对路径，服务端校验规范化后的前缀关系与链接逃逸，不固定版本、不追踪内容变化，也不把路径当作附件快照。
- `content` 至少 1 项，最多 64 项；合计 UTF-8 text 受 `maxPromptBytes` 限制。附件字节数不计入该文本预算，但计入 Agent 侧的真实输入预算（§14 与 `ACP_COMPATIBILITY_MATRIX.md`）。
- 未协商对应 feature 时这两个分支**必须缺席**：客户端发送它们会被 `protocol.schema_invalid` 拒绝；服务端不得把附件静默转成文本、删掉附件后只发文字，或用空 `attachment_ref` 占位。
- Agent 输出中的媒体/resource 仍通过 `AgentContentBlock`（§10.4）和 ACP raw payload 显式呈现；缺少下载/渲染能力不等于丢弃事件。外部 URI 不自动下载。
- `elicitation.respond.values` 必须符合原 `elicitation.requested.schema`，最大 64 KiB、深度 16；`cancel` 时 values 必须为 `null`。未知必需字段或无法表达的类型必须使提交不可用并说明，MUST NOT 以空对象冒充填写。
- `permission.resolve.optionId` 必须来自对应未解决 request，不能由客户端提交任意权限字符串；`requestVersion` 必须与当前有效请求一致（§11.7）。
- `session.prompt.submissionId` 与两个应答命令的 `requestVersion` 在 `schemas/sync/v1/command.schema.json` 里是**可选**字段，这是为旧 peer 保留的 wire 形状，不是语义上的可省略：未协商 `core.attachment-transfer.v1` / `core.pending-interactions.v1` 的客户端不发送它们，行为与本变更前逐字相同；一旦协商，客户端 MUST 发送，服务端不得用缺省值猜测版本或清单绑定，也不得因字段缺席而放宽校验。
- query 结果超过单消息上限时返回 `resource.result_too_large`，客户端应改用 snapshot/事件视图；不得截断后伪装成完整结果。

### 11.6 多端并发

- 不同会话的命令可以并行。
- 同一会话的 mutation 进入 Session Actor 串行处理。
- 同一会话同时最多一个 active turn。
- 并发 prompt 按服务端持久化接受顺序排队，或按配置明确返回 `session.busy`。
- config option 与模式变更必须带 `expectedVersion`；版本不匹配返回 `state.version_conflict` 和当前版本（`details.currentVersion` 必需，`details.expectedVersion` 回显请求携带的版本），客户端据此重读后再决定是否重试。
- 权限或 elicitation 响应使用请求自身的 expected state/version；第一个有效响应成为权威结果，之后返回 `interaction.already_resolved`。

### 11.7 有效待处理交互与应答版本

`core.pending-interactions.v1` 让刷新或换设备后的客户端重新取得**当前有效**的完整请求，而不是只能看历史卡片。

- **读取**：`session.read` 的 `include` 含 `pending_interactions` 时，`resources.pending_interactions` 返回 `effectiveInteraction[]`（机器定义 `schemas/sync/v1/media.schema.json#/$defs/effectiveInteraction`，语义见 §10.4），每项含服务端在本次读取时确认的 `requestVersion`、完整 `options`/`schema` 与可选的 `expiresAt`。未协商该 feature 时返回 `capability.unsupported_by_broker`，不得返回空数组冒充「没有待处理请求」。
- **历史只是定位信息**：从事件历史或快照恢复的待处理卡片仅携带 `interactionId`，MUST NOT 直接授权应答；客户端必须先调用 `session.read` 取得当前有效请求。
- **应答前校验**：`permission.resolve`/`elicitation.respond` 在接受前依次校验连接身份与撤销状态、会话归属、请求仍为 pending、`requestVersion` 等于当前值、Agent 上游等待方仍存活、表单值符合当前 schema。任一失败都**不消耗**请求：版本不符返回 `interaction.version_conflict`（`details.expectedVersion`/`currentVersion`，可重试），已处理返回 `interaction.already_resolved`，已失效/上游丢失/远程不可达返回 `interaction.unavailable`（`details.reason` 为 `expired｜upstream_lost｜owner_offline`）。
- **首个有效答案胜出**：多个设备并发提交时，服务端用一次原子条件更新选出唯一胜者；其余设备收到 `interaction.already_resolved` 并看到由其他设备完成的事实。重复相同 `requestId` 幂等返回首次结果，不重复派发。
- **不可复活**：Daemon 重启后上游 ACP 请求已不存在的交互保持终态，MUST NOT 被迁移或重新标记为 pending；历史应答不会「补发」给 Agent。
- **值保真**：表单整数不得先经 JavaScript number 再序列化；未知必需字段或不可表达的 ACP 类型使提交整体不可用并说明原因。`submit` 的空对象与 `null` 分别对应 ACP `content: {}` 与 `content: null`，不得互换。

## 12. Error

### 12.1 格式

```json
{
  "type": "error",
  "body": {
    "code": "capability.unsupported_by_broker",
    "message": "This detail resource is not provided by the broker yet.",
    "retryable": false,
    "correlationId": "6dfa044f-05f8-4a02-b4a1-a4d9d034298b",
    "details": {}
  }
}
```

- `code` 是稳定机器标识；客户端不得解析 `message` 做逻辑判断。
- `message` 是安全、简短、可展示的描述，不得包含密钥、token、完整 prompt、敏感路径或内部堆栈。
- `correlationId` 通常引用导致错误的 `messageId` 或 `requestId`；只有确实没有可关联的 message 或 request 时才允许为 `null`（例如 heartbeat 超时、服务端主动关闭或尚未收到任何业务消息）。有可关联对象却写 `null` 视为协议实现缺陷。
- `details` 只能包含该错误 code 登记的非敏感字段。
- 可恢复命令错误优先使用 `command.result(status=rejected)`；连接级错误使用 `error`。

### 12.2 v1 错误码族

```text
protocol.invalid_json
protocol.schema_invalid
protocol.message_too_large
protocol.version_unsupported
protocol.feature_required
protocol.type_unsupported
protocol.sequence_invalid
auth.required
auth.host_mismatch
auth.device_unknown
auth.device_revoked
auth.origin_mismatch
auth.insecure_context
auth.proof_invalid
authorization.scope_denied
pairing.already_claimed
pairing.expired
pairing.consumed
pairing.code_invalid
pairing.qr_invalid
sync.cursor_invalid
command.unsupported
command.not_found
command.idempotency_conflict
command.uncertain
state.version_conflict
session.not_found
session.busy
interaction.already_resolved
capability.unsupported_by_client
capability.unsupported_by_broker
capability.unsupported_by_agent
resource.rate_limited
resource.backpressure
resource.result_too_large
resource.remote_unavailable
attachment.negotiation_required
attachment.transfer_not_found
attachment.binding_mismatch
attachment.offset_mismatch
attachment.chunk_conflict
attachment.digest_mismatch
attachment.sealed
attachment.budget_exhausted
attachment.expired
attachment.revoked
media.unavailable
interaction.unavailable
interaction.version_conflict
internal.unavailable
```

#### details 登记

| code | 登记字段（`details`） |
|---|---|
| `protocol.feature_required` | `features`（必需） |
| `sync.cursor_invalid` | `reason`（必需） |
| `state.version_conflict` | `expectedVersion`（可选）、`currentVersion`（必需） |
| `resource.rate_limited` | `retryAfterMs`（可选） |
| `resource.remote_unavailable` | `ownerNodeId`（可选）、`reason`（必需） |
| `attachment.offset_mismatch` | `expectedOffset`（必需）、`actualOffset`（必需） |
| `attachment.chunk_conflict` | `offset`（必需） |
| `media.unavailable` | `reason`（必需） |
| `interaction.unavailable` | `reason`（必需） |
| `interaction.version_conflict` | `expectedVersion`（可选）、`currentVersion`（必需） |

未在上表列出的 code **不登记任何** `details` 字段：发送方必须送 `{}`。字段的取值域由 registry 的 `details` 片段约束：`sync.cursor_invalid.reason ∈ {malformed, epoch_mismatch, beyond_head, cursor_expired}`；`resource.remote_unavailable.reason ∈ {owner_offline}`；`protocol.feature_required.features` 是 1..64 个 feature ID 的排序去重列表（`requiredFeatures` 中未被协商的那些）；`resource.rate_limited.retryAfterMs` 是建议退避毫秒数；`state.version_conflict` 的两个版本号都是十进制字符串；`attachment.offset_mismatch` 的两个偏移与 `attachment.chunk_conflict.offset` 都是十进制字符串；`media.unavailable.reason ∈ {expired, not_found, external_uri_not_fetched}`；`interaction.unavailable.reason ∈ {expired, upstream_lost, owner_offline}`；`interaction.version_conflict` 的两个版本号同样是十进制字符串。每个登记字段都要标注 `必需` 或 `可选`，与 registry 的 `required` 逐一对应。`details` 始终是开放扩展点（接收方必须忽略未知字段，§4.2），新增字段按 §16.1 的兼容变更登记。上表与 `compatibility/errors/v1/errors.json` 的 `details` 片段逐条相等，由 `npm run check` 的 `check:errors` 断言。`resource.remote_unavailable` 只用于 `origin.kind = "remote"` 的会话正文回源失败，本节点资源不得返回该错误码。

错误码可以向后兼容地增加。客户端遇到未知错误码时按 `retryable` 和 code 首段做保守处理，并展示通用错误。

### 12.3 WebSocket Close Code

| Code | 含义 |
|---:|---|
| 1000 | 正常关闭 |
| 1009 | WebSocket message 过大 |
| 4400 | 协议或 schema 错误 |
| 4401 | 未认证/认证超时 |
| 4403 | 授权拒绝 |
| 4406 | 协议版本不兼容 |
| 4408 | heartbeat/handshake 超时 |
| 4409 | identity 或状态冲突 |
| 4410 | 设备已撤销 |
| 4411 | 同一设备的新连接已经替换当前连接 |
| 4429 | 限流 |
| 4500 | 服务端暂时不可用 |

close reason 不得包含敏感信息，且不是结构化错误的替代品。

## 13. Heartbeat、超时与重连

浏览器不能主动发送 WebSocket Ping frame，因此 v1 使用应用消息：

```json
{ "type": "control.ping", "body": { "nonce": "<base64url-16-bytes>" } }
```

```json
{ "type": "control.pong", "body": { "nonce": "<same-value>" } }
```

- 默认 heartbeat interval 为 30 秒，由 `auth.authenticated` 下发。
- 连续 90 秒无任何入站消息或 pong，服务端可以关闭连接。
- 任意有效业务消息都可视为连接活跃，但 ping nonce 仍需原样响应。
- PWA 进入后台后连接可能被挂起；恢复时走正常重连，不依赖旧连接。
- 客户端使用带随机抖动的指数退避，例如 1、2、4、8、16、30 秒上限。
- `revoked`、`identity_changed`、`incompatible` 不自动无限重试。
- 每次重连生成新 nonce，重新执行完整认证，再使用最后持久 ACK cursor 订阅。

第一阶段不支持电脑离线时排队 prompt。离线输入只能保留为明确的本地 draft，不能标记为已提交。

## 14. Limits 与 Backpressure

v1 默认上限：

| 项目 | 默认值 |
|---|---:|
| 单条 WebSocket JSON message | 1 MiB |
| 单次 prompt payload | 256 KiB |
| JSON nesting depth | 64 |
| 单对象字段数 | 1,024 |
| 单数组元素数 | 10,000 |
| 设备名称 UTF-8 长度 | 128 bytes |
| 单批 replay event 数 | 500 |
| 单连接的远程订阅数（`sync.remote_subscribe`，`resource.stateless-origin.v1`） | 32 |
| 单连接待发送队列 | 8 MiB 或 2,000 messages，先到者触发 |
| 认证超时 | 15 秒 |
| 配对默认有效期 | 5 分钟 |
| 目录浏览的单层条目数（`workspace.browse`） | 128 |
| 会话文件/ Git 状态枚举的单页条目数（`session.files.list` / `session.git.status`） | 128 |
| 根内相对路径的字符数（`session.files.*` / `session.git.diff`） | 4,096 |
| 单个文件内容块的原始字节（`session.files.read` 的 `dataBase64` 解码后） | 32 KiB |
| 文本预览的原始字节（`session.files.read`，`purpose = preview`） | 256 KiB |
| 栅格图片预览的原始字节（PNG/JPEG/GIF/WebP） | 4 MiB |
| 一次完整下载的原始字节（`purpose = download`） | 64 MiB |
| 单处改动的差异行数（`file.changed.diffLines`） | 400 行 |
| 单处改动的差异行合计文本 | 64 KiB（UTF-8） |
| 单行差异行文本 | 4,096 字符 |
| 工具详情的单一部分（`tool.call.*` 的 `toolDetail.input` / `toolDetail.output`） | 各 32 KiB（JSON 序列化后的 UTF-8 字节） |
| 工具详情的文本部分（`toolDetail.text`） | 32 KiB（各文本块 JSON 序列化后的字节合计） |
| 工具详情整体（`toolDetail`） | 64 KiB（JSON 序列化后的 UTF-8 字节） |

认证前固定上限不能等待 `auth.authenticated` 下发：

| 项目 | v1 上限 |
|---|---:|
| WebSocket upgrade headers | 16 KiB |
| 第一条 `auth.client_hello` | 32 KiB |
| pairing HTTP request body | 16 KiB |
| `supportedFeatures` 项数 | 64 |
| `requiredFeatures` 项数 | 64 |
| 单个 feature ID | 64 ASCII bytes |
| 单 IP pending authentication | 5 connections |
| 单 IP 新认证尝试 | 100/分钟 |
| 单节点并发 WSS 连接 | 256 |
| 单设备业务命令速率 | 120/分钟（按 `deviceId` 与命令族分别统计，命令族见下方说明） |
| 单个内嵌 diff / document | 256 KiB |
| 单 pairing proof 失败 | 5 次后使 pairing 失效 |

- WebSocket 建立后第一条消息必须是 `auth.client_hello`，且必须在 5 秒内完整到达。
- 整个认证必须在 15 秒内完成；认证前最多只接收 `client_hello` 和 `client_proof` 各一次。
- 超限应在读取/分配完整 body 前尽早拒绝；未认证错误不得回显请求内容或暴露设备是否存在。
- feature ID 只允许 `[a-z0-9.-]`，不得为空、包含连续分隔 NUL 或通过 Unicode 形成视觉混淆。

- 只有 `maxMessageBytes`、`maxPromptBytes`、`maxReplayEventsPerBatch` 三项通过 `auth.authenticated.limits` 下发，服务端可以按部署情况下调；其余上限（本节的认证前固定上限、JSON nesting depth、单对象字段数、单数组元素数、设备名称长度、单连接待发送队列、认证超时、配对有效期、目录浏览的单层条目数、会话文件/ Git 状态单页条目数、根内相对路径长度、内容块/预览/下载预算、差异行的行数/合计文本/单行文本、工具详情的各部分与整体上限）都是固定 v1 常量，不得通过 `limits` 覆盖或下调。
- **新增固定上限的取值理由**：单层条目数 128 与差异行 400 行 / 64 KiB 按单条消息 1 MiB 反推——128 条目录条目在「路径接近 4,096 字符上限、JSON 要求转义反斜杠」的最坏情形下约 0.8 MiB，400 行差异在单行不超过 4,096 字符、合计不超过 64 KiB 时转义后的体积同样留在同一条消息内；超限时两者都**显式降级**（目录浏览置 `truncated` 并只回上限内条目、差异行改为 `diffUnavailable`），而不是让请求失败或下发被截断的内容。
- **会话文件预算的取值理由**：单块 32 KiB 的 base64url 约 43.7 KiB，远低于 1 MiB 单条消息上限，给信封与同帧其它字段留足空间；文本预览 256 KiB、图片预览 4 MiB、下载 64 MiB 分别覆盖「可滚动阅读」「可解码栅格图」「用户主动保存的常规文件」，更大的文件明确不支持完整下载。快照只在内存中、绑定连接并受 60 秒空闲与并发上限约束，因此预算与内存占用可直接对应；超过预览预算时置 `previewTruncated`，超过下载预算时返回 `resource.result_too_large`，不截断后冒充完整内容。Git 文本差异沿用 400 行 / 64 KiB / 单行 4,096 字符的既有固定上限。
- 目录浏览的条目数上限与差异行上限都**不得**导致整次失败：前者按 §11.5 回上限内条目并置 `truncated: true`，后者按 §10.3 改为只带统计并标记 `diffUnavailable`。只有当结果整体超过单条消息上限时才回 `resource.result_too_large`（§11.5）。
- **工具详情的降级**：`toolDetail` 的三个部分独立判定 32 KiB（`text` 按各文本块 JSON 序列化字节合计），整体 64 KiB。超限的部分**整体缺席**并在 `truncatedParts` 中登记，MUST NOT 发送被截断的 JSON 片段；三个内容部分都缺席但存在截断时仍下发 `truncatedParts`，以便客户端区分「数据存在但超限」与「Agent 没有提供」。无法以 ACPR-CJ1 值域表达的部分（数字非有限、含小数部分或超出 2^53−1，或嵌套超过规定深度）同样**整体缺席**并登记，不得让整条事件因 `view` 校验失败而无法提交。整体预算不足时按 `output` → `text` → `input` 的优先序保留（§10.3）。这些上限是固定 v1 常量，不由 `auth.authenticated.limits` 下发。
- 超过单消息上限应在分配大对象前拒绝。
- 限流所用的「命令族」是**服务端自持的 v1 常量**：每个命令名恰好属于一个族，覆盖关系由服务端测试断言；命令名的封闭词表仍以 `compatibility/commands/v1/commands.json` 为唯一来源。数值不变：单设备 120 次/分钟、窗口 1 分钟，连续超限以 `4429` 关闭连接。
- 超过本节速率时返回 `resource.rate_limited`（`retryable = true`），并在连续超限时以 `4429` 关闭连接；客户端应遵守 `details.retryAfterMs`（存在时）退避后再重试，不得立即重发。
- 附件正文、大型 diff 和完整终端输出不内嵌到业务消息中绕过 1 MiB 上限：附件走 §3.4 的独立已认证连接，按 32 KiB 分块、每传输最多一块未确认；其单连接发送预算与速率按附件传输族**独立**计数，不占用业务命令配额，也不因大文件拖慢事件投递。diff 与终端输出仍按本节既有预算显式降级。
- 待发送队列达到高水位后先停止读取新的 replay batch；持续过慢则发送 `resource.backpressure` 并断开。
- 断开慢客户端后由其 cursor 重放，不能丢弃 Agent 事件或阻塞其他连接。
- rate limit 至少按 IP、device ID 和 command 分类；网络身份不能替代设备认证。

## 15. 安全处理要求

- 每个业务命令在当前连接身份下重新校验「连接已通过设备认证且设备未撤销」、会话状态、版本与幂等；设备方向没有命令级授权判定，跨节点面由连接认证与本机资源边界约束。
- 设备撤销后立即关闭所有对应连接，并拒绝后续认证。
- 配对 secret、私钥、HMAC、签名输入中的秘密和完整认证 payload 不得记录日志。
- nonce 必须单次连接使用；服务端在认证完成或超时后清除 pending challenge。
- 比较 HMAC、固定 hash 和其他秘密派生值时使用 constant-time compare。
- JSON schema 验证发生在进入 Broker 之前；设备认证（Sync 面）与 节点认证（Node Link 面）都发生在业务用例执行之前。
- Agent 生成的 Markdown、HTML、终端、diff 和工具输出始终是不可信数据。
- `Origin`、Host key、device key 与设备认证状态都是独立检查，任一通过不能替代其他检查。
- 附件连接的每一帧都重新校验连接身份、设备撤销状态、传输会话与主体绑定；附件读取不依赖公开或长期 bearer URL，跨会话/跨主体标识不得泄露内容。
- 外部 URI 与链接不因展示或解析而自动下载；媒体字节只在已认证读取路径中进入内存或页面 Blob，且按保留期与撤销立即失效。
- TLS channel 不使用应用长期 signing key 做加密；身份密钥和连接临时密钥保持分离。

## 16. 兼容与演进

### 16.1 v1 内允许的兼容变化

- 在既有开放扩展点增加字段，或通过已协商 feature 增加仅发送给支持方的可选字段，并同步更新 schema。
- 增加新 event type、error code、command 或 feature ID。
- 在既有封闭词表里增加取值（例如新的 event type）：这是加性变化，不需要 major version，前提是新取值只出现在客户端本来就按「未知/不支持」降级的开放位置（事件类型、`details`）；取值位于**封闭对象**内、或与新增键配对时（第 10.1 节 `origin.kind` 的 `node` 与身份字段 `nodeId`），改用 feature 协商引入（见第 16.2 节）。
- 增加客户端可忽略的 `details` 字段。
- 下调运行时 limits，但不得低于实现文档声明的可工作最小值。

### 16.2 需要新 feature 或 major version 的变化

- 改变现有字段含义、类型或必填性。
- 改变排序、ACK、cursor 或幂等语义。
- 改变 transcript codec、算法、domain 或 field tag。
- 引入 binary framing、压缩、或不同 Transport Profile。
- 附件传输、类型化媒体、可恢复交互与远程历史按 §5.2 的 feature 引入；它们不要求提升 `protocolVersion`，但未协商时不得出现新帧或新封闭字段（§5.3）。

### 16.3 数据迁移

- 服务端必须保留 `serverEpoch`，普通升级不得无故使所有 cursor 失效。
- 客户端缓存 schema 可以独立演进；无法迁移时清除缓存并请求 snapshot，不得清除长期设备私钥。
- Host 或 Device identity key 变化不是普通协议迁移，必须按重新配对处理。

## 17. 测试与 Fixture

语言无关 schema 位于 [`schemas/sync/v1/`](../schemas/sync/v1/)，fixture 位于 [`fixtures/sync/v1/`](../fixtures/sync/v1/)。两者是本文的机器可验证伴随物；协议行为仍以本文为权威，发现不一致时必须一起修正。`event-views.schema.json` 按第 10.2 节的每个事件类型给出 `view` 的最低必填定义，`manifest.json` 用 `viewSchema`/`viewDef` 把事件 fixture 绑定到对应 `$defs`；`transcripts/` 覆盖第 6.3 节的全部 domain，并由 `manifest.transcriptVectors` 逐条登记。

fixture 目录至少覆盖：

```text
fixtures/sync/v1/
├─ valid/          应通过 schema 的认证、同步、事件、命令、错误和配对样例
├─ invalid/        必须失败并声明 expectedKeyword 的反向样例
├─ attachment/     独立附件连接的正反样例（§3.4；重复块、错误偏移、未协商、跨会话/主体、撤销）
├─ media/          类型化媒体、消息通道、部分结果与有效交互的值对象样例（§10.4）
├─ transcripts/    transcript、HMAC 和签名固定向量（第 6.3 节全部 domain）
├─ device-qr/      新设备短二维码的 Rust/TypeScript 共享向量（§7.1.1，含旧 #data= 拒绝样例）
└─ manifest.json   schema、view $defs、fixture、deviceQrVectors 与预期结果映射
```

合同检查由 `npm run check` 统一执行（完整清单见 `README.md` 的「合同检查」，判据与 crate 依赖方向门禁见 `AGENTS.md` §9/§10）；与 Sync 资产直接相关的脚本是：

```text
node scripts/check-schema-fixtures.mjs
node scripts/check-command-catalog.mjs
node scripts/check-error-registry.mjs
node scripts/check-contract-assets.mjs
node scripts/check-acp-compatibility.mjs
```

- `check-schema-fixtures.mjs` 用 ajv（Draft 2020-12）逐条执行 `manifest.json`：标为 valid 的 fixture 必须通过，标为 invalid 的 fixture 必须以声明的 `expectedKeyword` 失败，带 `viewSchema`/`viewDef` 的 fixture 还必须通过对应事件视图定义。它同时强制覆盖门禁：`message.schema.json` 可达的每个 `type`、以及 `event-views.schema.json` 的每个 `$defs` 都至少被一个 fixture 覆盖（sync 与 node-link 共享同一份 view 定义），fixture 目录中也不允许存在未被 manifest 列出的文件。
- `check-contract-assets.mjs` 负责静态引用完整性与需要真正计算的绑定：`$ref` 目标存在、`$id` 在每个 asset root 内唯一、`rawJson` 的 `byteLength`/`sha256` 与文本本身一致、事件的 `payloadDigest` 与 `payload` 的 ACPR-CJ1 sha256 一致、transcript 固定向量的 SHA-256/HMAC/P1363 重算通过。transcript 向量还必须**由 `codec`/`domain`/`input` 经 `compatibility/transcripts/v1/transcripts.json` 的字段表重新编码**并与 `expected.transcriptBase64url` 逐字节一致，因此 tag 顺序、宽度或字段集合写错会让检查失败；`transcripts/invalid/` 下的负向量则断言解码器以声明的错误拒绝畸形输入。
- `check-command-catalog.mjs` 断言命令名在 `compatibility/commands/v1/commands.json`、`schemas/sync/v1/command.schema.json`（`transport` 含 `sync` 的子集）、`schemas/node-link/v1/command.schema.json`（`transport` 含 `node_link` 的子集）、本文 §11.5 与 `SECURITY_DESIGN.md` §10.2 之间一致。
- `check-error-registry.mjs` 断言 `compatibility/errors/v1/errors.json` 与两个协议的 `$defs.errorCode` enum、以及本文 §12.2 与 `NODE_LINK_PROTOCOL.md` §14.1 的列表一致，并拒绝任何第二处内联定义的公开错误码。
- `check-contract-assets.mjs` 同时断言附件帧族：`compatibility/commands/v1/commands.json` 的 `transportFrames`、两个 `attachment.schema.json` 的 `$defs.frameType.enum`/`$defs.transferBudget`、本文 §3.4 与 `NODE_LINK_PROTOCOL.md` §12.9 的帧表与预算表逐项相等，且所引用的 feature ID 已在 feature registry 登记。

- `manifest.deviceQrVectors` 登记的 `device-qr/vectors.json` 由 Rust `device_qr_vectors.rs` 与 TypeScript `device-qr.test.ts` **消费同一文件**：规范 65 字节布局、旧 `#data=`、未知版本、错误长度、非规范 base64url、主机公钥 SHA-256 指纹、134 字节地址与 58×29 矩阵预算都在两侧各断言一次。`valid/`/`invalid/` 中不得再出现旧设备完整资料样例；节点资产保持在 `fixtures/node-link/v1/`。

Rust 实现必须在自己的 wire DTO（`crates/sync-protocol`）上形成独立判定：用同一份 `manifest.json` 驱动——valid 夹具必须解析成功并往返一致，invalid 夹具必须在声明的层被拒绝——`type` 常量、错误码与视图登记表另有漂移门禁与 schema/registry 逐条比对。这里不要求再引入通用 Draft 2020-12 校验器：ajv 的职责是判 schema 自洽与夹具本身合法，DTO 的职责是判实现忠实于 schema；若将来需要 Rust 侧独立复算 schema 本身，可以另加一层，但它不能替代 DTO 层。

最低测试集合：

1. Rust 与 WebCrypto 对全部 domain 的 transcript bytes、P1363 签名和 HMAC 结果一致。
2. DER/P1363 混淆、错误长度、错误 base64url、重复 JSON key 全部失败。
3. Origin、Host、Device、nonce、connection ID、version 或 features 任一变化都会使 proof 失败。
4. pairing 状态机、并发/重复 claim、状态轮询重试、过期、拒绝、首次认证 consumed 和 secret 清理。
5. 认证超时、撤销、错误设备、Host key 变化、sequence 回退和同设备连接替换。
6. snapshot 与实时 event 的 barrier 不漏、不乱序。
7. ACK 丢失导致重复投递时，客户端按 `eventId` 去重。
8. command response 丢失后使用相同 `requestId` 重试，不产生第二次派发。
9. requestId 复用但 payload 不同返回 idempotency conflict。
10. Agent 调用崩溃窗口进入 `uncertain`，不会自动重复执行；query/mutation 终态规则和 `command.status` 一致。
11. 未知 ACP 字段、超出 JavaScript safe integer 的数字和扩展 payload 通过 `rawJson` 保持原始 UTF-8 JSON document。
12. 未知 Sync event 有明确降级，未知必需 feature 明确失败。
13. cursor 过期、epoch 变化、snapshot 中断/替换和 digest 错误的恢复路径。
14. 慢客户端被隔离后可从 ACK cursor 继续。
15. Android Chrome、iOS Safari、桌面 Chrome/Edge 与 Rust 服务端互操作。
16. 第 10.3 节列出的 11 个 ACP `session/update` 判别子逐条映射到对应 Sync event type；未映射判别子降级为未知事件并保留 `acp.rawJson`。
17. imported 会话的正文不进入 Access snapshot 与客户端持久化缓存，`session.read` 在线回源 Owner；Owner 离线时返回 `resource.remote_unavailable`、置 `origin.online = false` 并广播一次 `session.origin.online_changed`。
18. 设备短二维码的 65 字节布局、旧 `#data=`/版本/长度/非规范编码拒绝、主机公钥 SHA-256 指纹与 134 字节/58×29 预算由 Rust 与 TypeScript 消费同一固定向量断言；qr-lookup 的重复取回、统一 `pairing.qr_invalid` 与独立限流另由服务端/身份测试覆盖。

## 18. 第一阶段暂缓项

以下能力不进入 v1 baseline，未来只能通过 feature 或新 Transport Profile 增加：

- Noise over WSS 或不可信中继。
- Protobuf、MessagePack 或通用 binary envelope。
- 应用层压缩与通用 binary envelope；附件分块传输已在 v1 baseline 内由 §3.4 承载，不再属于暂缓项。
- 云端离线队列、云推送和跨 Host 聚合。
- 多用户账号、组织权限或远程身份提供商。
- 每条业务消息重复签名。
- PWA 跨 Origin 身份迁移。

采用 JSON 不代表 Rust 内部使用无边界 `serde_json::Value`。控制信封和已知消息必须使用明确 DTO；只有 ACP 扩展容器及明确开放的 extension 字段可以保留任意 JSON 值。

### 独立创建目录（workspace.mkdir）

`workspace.mkdir` 仅 Sync 接受，未协商 `core.workspace-mkdir.v1` 时在任何文件系统访问前返回 `protocol.feature_required`。body 禁止 sessionId 与 expectedVersion。payload 只允许 parentPath 与 name；拒绝名字 `.`、`..`、斜线、反斜线、冒号和 NUL。父路径必须为绝对路径且不含父引用，目标路径不超过 4096 字符；先规范化既有父目录，再创建一层，不补建父层、不覆盖同名目录或文件。

稳定 requestId 对相同内容返回原状态，不同内容报 `command.idempotency_conflict`。持久受理先于目录写入，成功/失败终态与全局 command 事件先持久后投递。启动恢复对未终结受理置 `uncertain`，不得自动再次创建。completed 结果严格为 `{ created: true }`，不得附路径或名字。目录创建不登记 workspace、不建立会话、不启动 Agent。成功后客户端重新浏览父层，从服务端真实条目取路径；列表刷新失败或截断须与创建结果分开显示。
