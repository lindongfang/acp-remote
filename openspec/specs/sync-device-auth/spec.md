# sync-device-auth Specification

## Purpose
定义 Sync 设备的逐连接认证面：WSS 端点与其 subprotocol、四消息握手与认证窗口、基于已验证 HTTP Origin 的来源校验、同一设备只保留一条活动连接、撤销的即时传播、心跳与静默超时、以及协商限额的下发。它让浏览器 PWA 能在无长期令牌的前提下与 Daemon 建立可信的双向认证连接。

## Requirements

### Requirement: Sync WSS 端点与 subprotocol

listener SHALL 在 `/sync/v1` 接受 WebSocket 升级，且该端点要求的 subprotocol 固定为 `acp-remote.sync.v1.json`：未声明该 subprotocol 的升级与协商到 `permessage-deflate` 的升级 MUST 被拒绝，且 MUST NOT 建立连接或产生任何认证状态。Sync 端点与 Node Link 端点、两者的配对 HTTP 共用同一个 listener，按 path 路由，互不进入对方的处理逻辑。WebSocket binary message 在 v1 中 MUST 被拒绝并关闭连接。

#### Scenario: 携带约定 subprotocol 的升级成功

- **WHEN** 客户端向 `/sync/v1` 发起携带 `acp-remote.sync.v1.json` 且不请求压缩的 WebSocket 升级
- **THEN** 升级成功，连接进入认证前状态，等待 `auth.client_hello`

#### Scenario: 缺少或有误的 subprotocol 被拒绝

- **WHEN** 升级请求未声明 `acp-remote.sync.v1.json`，或声明的是 Node Link 的 subprotocol
- **THEN** 升级被拒绝，连接不建立，不产生会话、认证状态或审计记录

#### Scenario: 非文本帧被拒绝

- **WHEN** 已建立的 Sync 连接收到一个 binary frame
- **THEN** 连接被关闭，该帧的内容 MUST NOT 进入任何协议处理逻辑

### Requirement: 认证消息序列与窗口

连接建立后的第一条消息 SHALL 是 `auth.client_hello`，且完整认证（`auth.client_hello` → `auth.server_challenge` → `auth.client_proof` → `auth.authenticated`）MUST 在 15 秒内完成；超时 MUST 以 `4408` 关闭。认证完成前收到任何业务消息 MUST 以 `4401` 关闭，且认证阶段 MUST NOT 接受 hello 与 proof 之外的消息类型。认证成功后才向客户端下发 `connectionId` 与出站 `connectionSequence`，认证前的消息 MUST 省略这两个字段；认证完成后每条出站消息 MUST 携带它们并严格递增。

#### Scenario: 正常四消息握手

- **WHEN** 已配对设备发起连接，依次发送 `auth.client_hello` 与 `auth.client_proof`，其签名均有效
- **THEN** 服务端返回 `auth.server_challenge` 与 `auth.authenticated`，认证完成后客户端可发送业务消息

#### Scenario: 认证前发送业务消息被关闭

- **WHEN** 连接在认证完成前收到一条业务消息（如 `sync.subscribe`）
- **THEN** 连接以 `4401` 关闭，MUST NOT 处理该业务消息，也 MUST NOT 产生任何业务副作用

#### Scenario: 认证超时

- **WHEN** 连接建立后 15 秒内未完成认证
- **THEN** 连接以 `4408` 关闭，且未完成认证的连接不占用设备的连接槽位

### Requirement: 服务端来源绑定来自已验证的 HTTP Origin

服务端 SHALL 从**已验证的 HTTP 请求头 `Origin`** 取得 canonical origin，并把它与设备记录中持久化的绑定逐字比较；MUST NOT 信任客户端 JSON 中自报的来源值。两者不一致时 MUST 以 `auth.origin_mismatch` 拒绝认证。握手签名 transcript 中的 `canonicalOrigin` 字段 MUST 取服务端判定出的值，使客户端与服务端对同一字段的取值一致才可能验签通过。

#### Scenario: 来源与设备绑定一致时认证通过

- **WHEN** 浏览器从设备记录绑定的 canonical origin 发起连接，且其请求头 `Origin` 与该绑定逐字相同
- **THEN** 认证正常完成，握手签名使用该来源作为 transcript 字段

#### Scenario: 来源不一致时拒绝

- **WHEN** 请求头 `Origin` 与设备记录绑定的 canonical origin 不同（换了 scheme、hostname 或 port）
- **THEN** 认证被拒绝并返回 `auth.origin_mismatch`，不建立已认证连接，设备身份不被迁移到新来源

#### Scenario: 无来源的连接被拒绝

- **WHEN** 一个不携带 HTTP `Origin` 头的客户端对 Sync 端点发起握手
- **THEN** 认证被拒绝、不建立已认证连接；系统 MUST NOT 以「空字节串来源」放行（v1 的设备信任绑定在 canonical origin 上，缺少来源即无可比较的绑定）

> 已知偏差（不在本变更内解决）：`docs/SYNC_PROTOCOL.md` §3.2 允许「非浏览器原生客户端没有 HTTP `Origin`，transcript 用空字节串」。已落地的 `identity-auth` 对 `SyncDevice` **强制要求** `binding.canonical_origin` 为 `Some`（否则 `BindingKind` 失败），且设备信任行里的绑定是 `https://` 形式的 canonical origin。两者对「无来源的原生客户端如何绑定」尚未统一；本变更按已落地合同实现（拒绝），并**不得**为迁就该条文放宽 `identity-auth`。原生客户端落地时再单独定这条。

### Requirement: 未知与已撤销设备的可见行为

对不在持久化信任中的 deviceId，服务端 MUST 照常签发结构完整的挑战，MUST NOT 用错误类别区分该设备是否存在；失败推迟到证明校验阶段并统一为 `auth.proof_invalid`。设备存在但已撤销时 MUST 返回 `auth.device_revoked` 并以 `4410` 关闭，且 MUST NOT 建立已认证连接。

#### Scenario: 未知设备获得挑战但验签失败

- **WHEN** 未配对的设备提交 `auth.client_hello` 并随后提交任意 proof
- **THEN** 服务端照常返回结构完整的 `auth.server_challenge`，proof 阶段返回 `auth.proof_invalid`，响应 MUST NOT 透露该设备是否存在

#### Scenario: 已撤销设备被拒绝

- **WHEN** 一个已撤销的设备提交有效签名的完整握手
- **THEN** 服务端返回 `auth.device_revoked` 并以 `4410` 关闭，不建立已认证连接，且该次尝试 MUST NOT 把设备状态改回可用

### Requirement: 单设备只保留一条活动连接

同一 `(hostId, deviceId)` SHALL 最多保留一条已认证的 WSS 连接：新连接 MUST 先独立完成完整认证，认证成功后服务端以 `4411` 关闭旧连接。旧连接的关闭 MUST NOT 撤销设备，也 MUST NOT 使新连接的认证失效。

#### Scenario: 新连接顶替旧连接

- **WHEN** 某设备已有一条已认证连接，同一设备再建立第二条并完成认证
- **THEN** 旧连接收到 `4411` 并被关闭，新连接保持已认证状态，设备的信任与授权不变

#### Scenario: 被顶替的连接不撤销设备

- **WHEN** 一条连接因 `4411` 被关闭
- **THEN** 该设备仍可用于后续连接与命令，MUST NOT 因此在审计中记为撤销或认证失败

### Requirement: 设备撤销立即传播到活动连接

设备撤销提交成功后，服务端 SHALL 立即以 `4410` 关闭该设备的所有活动 Sync 连接，并拒绝其后续认证尝试。撤销判定 MUST 以持久化记录为准，进程内连接状态 MUST NOT 让已撤销设备继续可用。

#### Scenario: 撤销后活动连接被关闭

- **WHEN** 某设备有一条已认证的活动连接，本机撤销该设备
- **THEN** 该连接收到 `4410` 并被关闭，此后同一设备的重新认证被拒绝

#### Scenario: 撤销后不再接受新连接

- **WHEN** 已撤销的设备发起新的握手
- **THEN** 认证被拒绝并以 `4410` 关闭，且 MUST NOT 重新建立信任记录

### Requirement: 心跳与静默超时

服务端 SHALL 按 `auth.authenticated` 下发的 `heartbeatIntervalMs`（默认 30 秒）向连接发送应用层 `control.ping`，并要求客户端以同值 nonce 回 `control.pong`。连续 90 秒内未收到任何入站消息或 pong 时，服务端 MAY 关闭该连接。ping nonce MUST 每次新生成，MUST NOT 复用。

#### Scenario: 客户端正常回应心跳

- **WHEN** 服务端发送 `control.ping`，客户端回填同值 nonce 的 `control.pong`
- **THEN** 连接保持，服务端不因心跳关闭它

#### Scenario: 静默连接被回收

- **WHEN** 一条已认证连接连续 90 秒没有任何入站消息，也没有回应 pong
- **THEN** 服务端关闭该连接，其后客户端按正常重连流程重新执行完整认证

### Requirement: 限额下发只下调不下调之外

`auth.authenticated` SHALL 下发 `limits.maxMessageBytes`、`limits.maxPromptBytes` 与 `limits.maxReplayEventsPerBatch` 三项，取值分别来自配置键 `sync.max_message_bytes`、`sync.max_prompt_bytes` 与 `sync.max_replay_events_per_batch`。这些键 SHALL 只允许按部署情况**下调**，取值高于协议默认值时 MUST 取默认值；协议中标记为固定常量的其余上限（JSON 嵌套深度、单对象字段数、单数组元素数、设备名称长度、单连接待发送队列、认证超时、配对有效期）MUST NOT 受这三项影响，也 MUST NOT 被放宽或收窄。

#### Scenario: 配置下调生效

- **WHEN** 部署把 `sync.max_message_bytes` 配置为低于协议默认值的取值，客户端完成认证
- **THEN** `auth.authenticated.limits.maxMessageBytes` 返回该下调后的取值，且超过该值的单条消息在分配大缓冲前被拒绝

#### Scenario: 配置上调被忽略

- **WHEN** 部署把 `sync.max_message_bytes` 配置为高于协议默认值的取值
- **THEN** 下发的取值仍为协议默认值，MUST NOT 按配置放宽该上限

#### Scenario: 三个键接线后不再报未接线

- **WHEN** 配置文件中显式设置上述三个键并启动 Daemon
- **THEN** 它们不再出现在「已知但未接线」的键清单中，且其取值确实被 Sync 连接消费

### Requirement: 认证前的固定上限

认证前 SHALL 施加协议规定的固定上限，且它们 MUST NOT 由配置放宽或收窄：连接建立后第一条消息 MUST 是 `auth.client_hello` 且 MUST 在 **5 秒**内完整到达，超时以 `4408` 关闭；首条 `auth.client_hello` 的消息体上限为 **32 KiB**，超限 MUST 被拒绝；单个节点的**并发 WSS 连接数**上限为 **256**，超限的新连接 MUST 被拒绝而不是排队或静默丢弃；同一来源地址的**在途认证**上限为 5 条、**新认证尝试**上限为每分钟 100 次。

#### Scenario: 首条消息超时

- **WHEN** 连接建立后 5 秒内未收到完整的 `auth.client_hello`
- **THEN** 连接以 `4408` 关闭，MUST NOT 一直等待到 15 秒整窗结束

#### Scenario: 超大 client_hello 被拒绝

- **WHEN** 首条 `auth.client_hello` 的消息体超过 32 KiB
- **THEN** 该消息被拒绝，不进入握手状态机，也不因它而建立任何认证状态

#### Scenario: 超出并发连接上限的新连接被拒绝

- **WHEN** 某节点已有 256 条并发 WSS 连接，此时又有一条新连接建立
- **THEN** 新连接被拒绝，MUST NOT 被排队等待、MUST NOT 静默丢弃，也 MUST NOT 挤掉既有连接

#### Scenario: 同一来源的在途认证与尝试速率受限

- **WHEN** 同一来源地址已有 5 条在途认证，或一分钟内发起超过 100 次新认证尝试
- **THEN** 超出的连接以限流语义被拒绝，连续超限时按 `4429` 关闭

### Requirement: 协议错误码与入站序号推进

协议错误的下发 SHALL 与**入站序号推进**解耦：`connectionSequence` 描述的是对端**已经发出**的消息，本机是否接受其类型或载荷不影响该序号已被消耗这一事实。收到词表之外的 `type` 时 MUST 返回 `protocol.type_unsupported`，MUST NOT 返回 `protocol.schema_invalid`。一次协议错误 MUST NOT 让后续入站消息永久被判序号错误——那会把连接变成只出不进的哑管道。

#### Scenario: 未知类型回可区分的错误码

- **WHEN** 已认证连接收到一条 `type` 不在协议词表内的消息
- **THEN** 回 `protocol.type_unsupported`（MUST NOT 是 `protocol.schema_invalid`），且连接保持可用

#### Scenario: 一次协议错误不让连接变成哑管道

- **WHEN** 已认证连接先收到一条未知 `type` 的消息，随后收到一条类型合法、序号按对端计数器正常递增的消息
- **THEN** 后一条 MUST NOT 被判为序号错误，它 MUST 按其自身内容被正常处理或被拒绝；连接 MUST NOT 因此退化为只出不进

### Requirement: 认证失败的审计留痕

认证失败 SHALL 留一条审计记录，动作取协议登记的认证失败动作、目标为该设备、结果取失败；该记录 MUST 在**认证未成功**时写出（成功路径的 `device.authenticated` 由收尾写集承担，两者不得互相替代）。审计的 actor 归因 MUST 取持久化的该设备身份，MUST NOT 取握手消息自报的取值；MUST NOT 因设备未知而跳过留痕。

#### Scenario: 证明校验失败留痕

- **WHEN** 某设备提交的 proof 验签失败、挑战过期或被重放
- **THEN** 写出一条认证失败的审计记录，其动作为协议登记的认证失败动作、目标为该设备

#### Scenario: 未知设备的失败同样留痕

- **WHEN** 一个不在持久化信任中的 deviceId 提交握手且 proof 阶段失败
- **THEN** 同样写出认证失败审计，MUST NOT 因该设备未知而不留痕，也 MUST NOT 借此在响应中泄露该设备是否存在

### Requirement: 写路径有期限且写卡住时判据仍被检查

Sync 连接会话 SHALL 与 Node Link 同口径：出站写有期限，「对端不读、socket 缓冲被填满」不会让会话任务无限期停在写路径上；写未被推进的期间，心跳、静默超时与慢消费者看门狗 SHALL 仍按各自窗口被检查并生效。超时的处置 SHALL 与既有分级一致（先按判据处置，最终断开），MUST NOT 静默丢弃事件或让连接无限期存活。

#### Scenario: 对端不读时连接被有界收尾

- **WHEN** Sync 对端停止读取、socket 缓冲被写满
- **THEN** 连接 SHALL 在有界时间内被收尾，MUST NOT 一直保持在「已连接」状态

#### Scenario: 写卡住期间判据仍被检查

- **WHEN** Sync 会话的写路径长时间未被推进
- **THEN** 心跳、静默判定与慢消费者看门狗 SHALL 仍按窗口被检查并生效
