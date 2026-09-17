# ACP Remote Sync Protocol

> 状态：编码前协议基线（Draft）  
> 协议版本：1  
> 日期：2026-09-18
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

- 对本地 Agent/会话，当前 Daemon 是资源权威；对 imported Agent/会话，Owner Node 是资源权威，当前 Access Node 只对本地身份、无正文交付索引和 local sequence 负责。
- 本地资源事件必须先写入当前节点的权威事件日志再广播。远程资源事件已由 Owner 先持久化；Access 在向本地客户端广播前只提交 origin cursor、eventId/type/digest 和 local sequence 映射，默认不得复制正文。
- 事件采用至少一次投递，客户端按 `eventId` 去重。
- 同一事件日志实例内，`globalSequence` 严格递增；同一会话内，`sessionSequence` 严格递增。
- `requestId` 保证客户端重试不会创建第二次命令接受或第二次 Agent 派发。
- 不宣称网络或外部 Agent 副作用“恰好一次”；崩溃窗口无法确认时必须返回 `uncertain`，不得自动重复派发。
- 慢客户端不得阻塞 Agent、Broker 或其他客户端。

当前 Sync v1 baseline 只完整定义本节点资源。Access Node 向客户端暴露 imported resource 前，必须通过后续协商 feature/schema 增加 `ownerNodeId`、`exportId`、origin cursor、在线状态和 `no-content-cache`；在该 feature 落地前不得把远程资源伪装成本地权威会话。该模式下 snapshot 和 replay 正文必须在线回源 Owner，客户端不得持久化正文。

## 3. Transport Profile

### 3.1 正式连接

第一版正式 Profile：

```text
HTTPS/WSS + JSON text messages + ECDSA P-256 challenge-response
```

- PWA 从 canonical origin 加载静态资源。
- WebSocket endpoint 为同源 `/sync/v1`。
- 客户端必须请求 WebSocket subprotocol `acp-remote.sync.v1.json`。
- 服务端必须选择该 subprotocol，否则客户端终止连接。
- 正式连接只允许 `wss://`；`ws://` 只允许显式开发模式和 loopback。
- WebSocket binary message 在 v1 中不支持，收到后关闭连接。
- v1 不协商 `permessage-deflate`；压缩必须在后续 feature 中明确规定解压后上限和资源防护。
- 一个完整 JSON 文档对应一个 WebSocket message；WebSocket frame 分片由实现层重组，不暴露给协议层。

原生客户端可以连接已配置的多个 endpoint，但每个 endpoint 都必须证明同一个已配对 Host identity。PWA 身份只绑定一个 canonical origin。

### 3.2 Origin 与 Host 检查

- PWA 请求的 HTTP `Origin` 必须与设备记录中的 canonical origin 完全一致。
- canonical origin 是小写 scheme、浏览器规范化 hostname 和显式有效 port 的组合，不包含 path、query、fragment 或尾部 `/`。
- 默认端口规范化为 `https:443`、`http:80` 后再比较；线上的字符串形式省略默认端口。
- 服务端必须校验 `Origin`、允许的 `Host` 和 TLS 部署配置，不能信任客户端 JSON 中自报的 Origin。
- 非浏览器原生客户端可以没有 HTTP `Origin`，此时 transcript 中使用空字节串，授权仍绑定设备和 Host identity。

### 3.3 内容编码

- JSON 文本使用 UTF-8，不允许 BOM。
- 不允许重复对象键、无效 Unicode、`NaN`、`Infinity` 或尾随内容。
- 协议整数如果可能超过 JavaScript safe integer，在线上必须编码为无前导零的十进制字符串。
- `globalSequence`、`sessionSequence`、`connectionSequence` 和 cursor 中的 sequence 都是十进制字符串。
- 时间使用 UTC RFC 3339，精确到毫秒，例如 `2026-09-17T12:10:00.123Z`。
- UUID 使用带连字符的小写 canonical 文本；v1 接受 UUIDv4，服务端生成的有序 ID可以使用 UUIDv7，但排序不得依赖 UUID。
- 二进制字段使用无填充 base64url，不得使用标准 base64 的 `+`、`/` 或 `=`。
- 普通 JSON 不做 canonicalization，也不直接用于签名或 HMAC。

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

认证完成后：

- `connectionId` 和 `connectionSequence` 必须存在。
- 两个方向分别维护 sequence，不能相互共用计数器。
- sequence 重复、回退、跳号或 connection ID 不匹配均为 `protocol.sequence_invalid`。
- `messageId` 重复不自动重放业务结果；命令幂等只看 `requestId`。

为突出业务字段，本文后续部分消息示例会省略未变化的通用信封字段；实际 wire message 仍必须符合本节完整信封。

### 4.2 未知字段与类型

- v1 控制信封、认证 body、sync body 和 command payload 是 closed object；未知字段返回 `protocol.schema_invalid`。开放扩展只允许出现在本文明确标记的 `event.payload.view`、错误 `details` 和 ACP `rawJson` 中。
- 未知 `type` 必须返回 `protocol.type_unsupported`，不得静默丢弃。
- `body.requiredFeatures` 中任一 feature 未协商时，接收方必须返回 `protocol.feature_required`。
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

Feature ID 使用小写 ASCII、点分层级，例如：

```text
core.snapshot.v1
core.command-status.v1
session.model-switch.v1
acp.raw-payload.v1
```

- 客户端发送支持列表和必需列表。
- 服务端返回已选择的交集。
- 列表去重后按 UTF-8 字节升序排列。
- 认证 transcript 中的 feature bytes 为排序后 ID 逐项以 `0x00` 分隔的 UTF-8 字节；ID 本身不得包含 NUL。
- 客户端必需 feature 未被选择时，认证不得进入业务阶段。

v1 baseline 必须包含：

```text
core.snapshot.v1
core.command-status.v1
core.event-ack.v1
acp.raw-payload.v1
```

## 6. 密钥与二进制表示

### 6.1 算法

- Host 和 Device identity：ECDSA P-256 + SHA-256。
- 公钥：SEC1 uncompressed point，固定 65 bytes，`0x04 || X(32) || Y(32)`。
- 签名：IEEE P1363，固定 64 bytes，`r(32) || s(32)`。
- pairing proof 和 SAS：HMAC-SHA256。
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
| 配对短验证码 | `acp-remote/pairing-sas/v1` |
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

各用途字段集合：

- pairing proof：`1,2,3,4,5,6,8,9,14,15`。
- pairing Host proof：`1,2,3,4,6,7,8,9,10,13`。
- pairing SAS：`1,2,3,4,6,7,8,9,10,13`。
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
- 电脑确认时决定初始 scopes；PWA 不能在 claim 中申请或扩大 scopes。
- `approved` 后创建设备记录，但 pairing 在该设备首次 WSS 认证成功前保持可查询。
- 首次 WSS 认证成功后原子转为 `consumed`；此后二维码不能再次创建设备。
- 过期判断以 Daemon 时钟为权威。PWA 的本地时间检查只用于提前提示，不能延长有效期。
- `pairingSecret` 最迟在 `expiresAt` 清除；`approved` pairing 在首次 WSS 认证成功时提前清除。拒绝状态为支持可靠轮询可以保留 secret 到原过期时间，但不得超过该时间。

### 7.1 二维码内容

Daemon 创建一次性 pairing，二维码承载 HTTPS URL，秘密部分必须位于 fragment：

```text
https://work-pc.example.ts.net/pair#data=<base64url-json>
```

解码后的 JSON：

```json
{
  "pairingProtocol": "acp-remote-pairing-v1",
  "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  "hostPublicKey": "<base64url-65-bytes>",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  "pairingSecret": "<base64url-32-bytes>",
  "expiresAt": 1789650000
}
```

PWA 必须验证当前 origin、secure context、过期时间和字段长度，读取后立即用 `history.replaceState` 清除 fragment。fragment、secret 和完整 QR payload 不得进入日志或 analytics。

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

- `hostProof` 是 Host 对 pairing Host proof transcript 的签名，PWA 用二维码中固定的 Host public key 验证。
- 手机和电脑必须各自计算 SAS；服务端不得在 claim 响应中把电脑计算出的 SAS 当作手机结果下发。
- SAS 计算为 pairing SAS transcript 的 HMAC-SHA256 输出前 4 bytes 按 u32be 解释后 `% 1_000_000`，左侧补零为 6 位。
- 电脑和手机都显示 SAS；只有电脑本地用户明确确认后，Daemon 才保存设备及 scopes。
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

`approved` 响应只返回设备的非秘密元数据、scopes 和 Host identity；不签发 bearer token。首次正式 WSS 连接仍执行完整 challenge-response。

状态查询在 `pending_confirmation`、`approved` 和 `rejected` 下可以安全重复；每次新的轮询生成新 request nonce，只有网络重试才复用原 nonce，并获得原响应。客户端观察到 `approved` 后开始 WSS 认证；观察到 `rejected`、`expired` 或 `consumed` 后停止轮询。客户端成功认证或终态过期后必须清除二维码 payload 和内存中的 secret。

### 7.4 HTTP 状态和错误

| HTTP 状态 | 使用场景 |
|---:|---|
| 200 | 状态查询成功，包括业务状态 `pending_confirmation/approved/rejected` |
| 201 | claim 首次成功并创建 pairing request |
| 400 | JSON 或字段 schema 无效 |
| 401 | HMAC proof 无效；响应不得泄露具体校验差异 |
| 403 | Origin/Host 不允许 |
| 404 | pairing ID 不存在 |
| 409 | 已被 claim、状态转换冲突或 device ID 冲突 |
| 410 | pairing 已过期或 consumed |
| 413 | 请求体超过限制 |
| 429 | 配对尝试被限流 |

错误 body 使用与第 12 节一致的 `code/message/retryable/correlationId/details` 结构。网络响应丢失后，客户端可以使用相同 claim 内容重试；如果服务端已接受同一 device/client nonce，应返回原 pairing request，而不是创建第二条记录。不同 claim 内容则返回 `409 pairing.already_claimed`。

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
      "core.snapshot.v1"
    ],
    "requiredFeatures": ["core.event-ack.v1"]
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
    "scopes": ["session.list", "session.read", "session.prompt"],
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

v1 只定义 `scope: machine`，服务端仍按当前设备 scopes 过滤。全局 sequence 对单个设备可以有不可见的空洞，不能据此推断隐藏事件。

`cursor` 类型为 `Cursor | null`；首次同步发送 `null`。cursor sequence 大于当前 head、格式非法或不属于当前 Host 时返回 `sync.cursor_invalid`，不得把它钳制到 head。epoch 不匹配和已超过保留窗口分别触发 `epoch_mismatch`、`cursor_expired` reset。

### 9.3 增量重放

cursor epoch 正确且 sequence 仍在保留窗口内时：

1. 服务端从 SQLite 读取 cursor 之后的可见事件。
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

客户端发送 `sync.snapshot_request`。服务端在一致性读视图中选择 `snapshotSequence`，依次发送：

```text
sync.snapshot_begin
sync.snapshot_chunk (1..n)
sync.snapshot_end
event where globalSequence > snapshotSequence
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
    "chunkCount": 6
  }
}
```

```json
{
  "type": "sync.snapshot_chunk",
  "body": {
    "snapshotId": "8194de43-e213-423d-acf4-2e3549304566",
    "chunkIndex": 0,
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
    "chunkCount": 6,
    "snapshotDigest": "<base64url-32-byte-sha256>"
  }
}
```

`chunkIndex` 从 `0` 开始连续递增，服务端必须按 index 顺序发送。`chunkCount` 在 begin/end 中必须一致。`snapshotDigest` 的计算方式是：对每个完整 `sync.snapshot_chunk` WebSocket message 的原始 UTF-8 bytes 分别计算 SHA-256，按 chunk index 连接这些 32-byte digest，再计算一次 SHA-256。客户端不能通过重新序列化 JSON 计算 digest。

客户端必须把 snapshot 写入以 `snapshotId` 隔离的暂存区；只有 chunk 连续、数量、cursor 和 digest 全部验证后，才能在一个本地事务中替换旧缓存。收到另一个 `snapshot_begin` 时必须丢弃旧的未完成暂存区。v1 不支持 snapshot chunk 断点续传；连接断开、digest 错误、顺序错误或空间不足时，客户端丢弃整个暂存 snapshot，重连后重新请求。验证失败不得损坏最后一个已完成缓存。

v1 snapshot 资源种类：

```text
sessions
messages
turns
pending_interactions
models
capabilities
```

设备无权读取的字段和资源不得进入 snapshot。

Snapshot item 的最低 schema：

| Resource | 每个 item 的必填字段 |
|---|---|
| `sessions` | `sessionId`, `agent`, `state`, `version`, `createdAt`, `updatedAt`; `title`, `currentModel`, `currentMode` 可为 `null` |
| `messages` | `messageId`, `sessionId`, `role`, `content`, `status`, `createdAt`; `turnId` 可为 `null` |
| `turns` | `turnId`, `sessionId`, `state`, `createdAt`; `startedAt`, `completedAt`, `terminalError` 可为 `null` |
| `pending_interactions` | `interactionId`, `sessionId`, `kind`, `state`, `schema`, `createdAt` |
| `models` | `sessionId`, `models`, `currentModelId`, `version` |
| `capabilities` | `sessionId`, `agentCapabilities`, `brokerCapabilities`, `clientPresentation` |

`agent` 至少包含稳定 `agentId` 和展示用 `name`；不得包含 Provider credential。所有 session-scoped item 必须引用同一 snapshot 中存在或客户端已有的 session。`content`、model、interaction 和 capability 的具体值对象与第 10.3、11.5 节相同，不得为 snapshot 发明另一套语义。

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
      "deviceId": null
    },
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
- `origin.kind` 为 `agent`、`device`、`desktop` 或 `daemon`；设备来源必须包含 `deviceId`。
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
session.model.changed
session.mode.changed
turn.queued
turn.started
turn.completed
turn.failed
turn.cancelled
agent.message.delta
agent.message.completed
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

### 10.3 v1 Event View Contract

所有 `view` 都是 object。下面字段是最低必填合同；可以增加已协商 feature 所允许的可选字段，但不得改变既有字段语义。

| Event type | `view` 最低字段 |
|---|---|
| `session.created`, `session.updated` | `session: SessionSummary` |
| `session.model.changed` | `previousModelId: string|null`, `model: ModelRef`, `version: decimal string`, `effectiveFrom: "now"|"next_turn"` |
| `session.mode.changed` | `previousModeId: string|null`, `mode: ModeRef`, `version: decimal string`, `effectiveFrom` |
| `turn.queued`, `turn.started`, `turn.completed`, `turn.cancelled` | `turnId`, `state` |
| `turn.failed` | `turnId`, `state: "failed"`, `error: PublicError` |
| `agent.message.delta` | `messageId`, `turnId`, `deltaIndex: decimal string`, `text` |
| `agent.message.completed` | `messageId`, `turnId`, `content: AgentContentBlock[]` |
| `tool.call.started`, `tool.call.updated`, `tool.call.completed` | `toolCallId`, `turnId`, `title`, `state`; 摘要字段可选，完整 ACP 保留在 `acp` |
| `permission.requested` | `interactionId`, `turnId`, `title`, `description`, `options: InteractionOption[]` |
| `permission.resolved` | `interactionId`, `resolution`, `resolvedByDeviceId: UUID|null` |
| `elicitation.requested` | `interactionId`, `turnId`, `title`, `schema`, `initialValues` |
| `elicitation.resolved` | `interactionId`, `action: "submit"|"cancel"`, `resolvedByDeviceId: UUID|null` |
| `terminal.output` | `terminalId`, `chunkIndex: decimal string`, `stream: "stdout"|"stderr"`, `text`, `truncated: boolean` |
| `file.changed` | `changeId`, `kind`, `displayPath`, `summary`; diff 或结构化详情可选 |
| `agent.connected`, `agent.disconnected` | `agentId`, `state`; disconnected 可带 `error` |
| `command.completed` | `requestId`, `result` |
| `command.failed` | `requestId`, `error: PublicError` |
| `command.uncertain` | `requestId`, `reason`, `mayHaveReachedAgent: true` |
| `device.revoked` | `deviceId`, `revokedAt`；只发送给仍有权查看设备状态的其他客户端 |

公共值对象：

```text
SessionSummary {
  sessionId: UUID,
  title: string | null,
  agent: { agentId: string, name: string },
  state: "idle" | "queued" | "running" | "waiting_input" | "waiting_permission" | "failed" | "closed",
  currentModel: ModelRef | null,
  currentMode: ModeRef | null,
  version: decimal string,
  createdAt: timestamp,
  updatedAt: timestamp
}

ModelRef { modelId: string, displayName: string }
ModeRef  { modeId: string, displayName: string }
InteractionOption { optionId: string, label: string, kind: string }
PublicError { code: string, message: string, retryable: boolean, details: object }
```

`AgentContentBlock` v1 的公共 view 支持：

```text
{ type: "text", text: string }
{ type: "image_ref", mimeType: string, byteLength: decimal string | null, displayState: "available" | "not_fetched" | "unsupported" }
{ type: "resource_ref", uri: string, name: string | null, displayState: "available" | "not_fetched" | "unsupported" }
{ type: "unsupported", originalType: string, reason: "unsupported_by_client" }
```

公共 view 不是 ACP 原文替代品。无法形成专用 view 时必须使用 `unsupported` block 或未知事件降级，同时保留 `acp.rawJson` 或明确的 `rawUnavailable`。

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
- 服务端按设备 scopes、会话范围、当前状态和命令 schema 再次授权，不能依赖客户端 UI。

### 11.2 Command Result

服务端返回：

```json
{
  "type": "command.result",
  "body": {
    "requestId": "4c4dafda-dd98-442e-8d55-252b75bac72d",
    "status": "accepted",
    "acceptedAt": "2026-09-17T12:11:00.000Z",
    "result": {
      "turnId": "6601828b-3eca-4cec-9a58-18ae1e0a3a14"
    },
    "error": null
  }
}
```

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
- Daemon 恢复时发现外部 Agent 副作用无法确认，必须先持久化 `command.uncertain`，再向客户端广播。
- 重复提交已经终结的 request 时，`command.result` 返回当前终态及原 terminal event ID，不重新派发。
- `uncertain` 是终态。用户若明确决定再次执行，必须创建新的 `requestId`；UI 必须警告原命令可能已经影响 Agent。

产生 mutation 的幂等记录至少保留到设备被撤销且目标会话被删除；记录可以只保留 request fingerprint、接受状态和最终结果引用，不要求永久保存完整 prompt。查询类命令可以只在连接期去重。实现不能在仍允许客户端重试时静默删除 mutation 幂等记录并把旧 request 当作新命令执行。

命令处理建议顺序：

```text
schema -> authenticated device -> scope -> state/version
-> idempotency lookup/reserve -> durable accept
-> session actor -> Agent -> durable events -> broadcast
```

### 11.3 首批命令

Sync v1 baseline 可以授予：

```text
session.list
session.read
session.prompt
session.cancel
session.model.list
session.model.set
session.mode.list
session.mode.set
permission.resolve
elicitation.respond
command.status
```

Sync v1 尚未定义，或仅允许 Node 本地管理入口：

```text
session.create
session.delete
workspace.select
agent.configure
provider.configure
device.manage
```

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

`state` 为 `accepted|completed|failed|uncertain`。设备只能查询自己提交或其 scope 明确允许查看的命令；不存在或不可见统一返回 `command.not_found`，避免权限侧信道。

### 11.5 v1 Command Schema

字段约定：表中未列出的 payload 字段在 v1 中拒绝；`{}` 表示必须是空 object。`sessionId` 是 command body 顶层字段，不在 payload 内重复。

| Command | 类别 | `sessionId` | Payload | 成功结果/终态 |
|---|---|---|---|---|
| `session.list` | query | 禁止 | `{}` | `completed { sessions: SessionSummary[] }` |
| `session.read` | query | 必须 | `{ include: string[] }`; include 只允许 `messages,turns,pending_interactions,models,capabilities` | `completed`，结构与对应 snapshot resources 相同 |
| `session.prompt` | mutation | 必须 | `{ content: PromptContentBlock[] }` | `accepted { turnId }`，随后 turn/domain event 和 command terminal event |
| `session.cancel` | mutation | 必须 | `{ turnId }` | 取消请求生效后 `command.completed`；目标已经终态则幂等完成 |
| `session.model.list` | query | 必须 | `{}` | `completed { models: ModelRef[], currentModelId, version }` |
| `session.model.set` | mutation | 必须 | `{ modelId }`，body `expectedVersion` 必须存在 | `session.model.changed` 后 `command.completed` |
| `session.mode.list` | query | 必须 | `{}` | `completed { modes: ModeRef[], currentModeId, version }` |
| `session.mode.set` | mutation | 必须 | `{ modeId }`，body `expectedVersion` 必须存在 | `session.mode.changed` 后 `command.completed` |
| `permission.resolve` | mutation | 必须 | `{ interactionId, optionId }` | `permission.resolved` 后 `command.completed` |
| `elicitation.respond` | mutation | 必须 | `{ interactionId, action, values }` | `action` 为 `submit|cancel`; 校验后产生 `elicitation.resolved` |
| `command.status` | query | 禁止 | `{ targetRequestId }` | `completed CommandStatusRecord` |

`PromptContentBlock` v1 baseline 只有：

```json
{ "type": "text", "text": "..." }
```

- `content` 至少 1 项，最多 64 项；合计 UTF-8 text 受 `maxPromptBytes` 限制。
- v1 PWA 不发送图片、文件、resource link 或任意 ACP content block。尝试发送返回 `capability.unsupported_by_client`，不能静默删除非文本 block 后继续。
- Agent 输出中的图片/resource 仍通过 `AgentContentBlock` 和 ACP raw payload 显式呈现；缺少下载/渲染能力不等于丢弃事件。
- `elicitation.respond.values` 必须符合原 `elicitation.requested.schema`，最大 64 KiB、深度 16；`cancel` 时 values 必须为 `null`。
- `permission.resolve.optionId` 必须来自对应未解决 request，不能由客户端提交任意权限字符串。
- query 结果超过单消息上限时返回 `resource.result_too_large`，客户端应改用 snapshot/事件视图；不得截断后伪装成完整结果。

### 11.6 多端并发

- 不同会话的命令可以并行。
- 同一会话的 mutation 进入 Session Actor 串行处理。
- 同一会话同时最多一个 active turn。
- 并发 prompt 按服务端持久化接受顺序排队，或按配置明确返回 `session.busy`。
- 模型/模式变更必须带 `expectedVersion`；版本不匹配返回 `state.version_conflict` 和当前版本。
- 权限或 elicitation 响应使用请求自身的 expected state/version；第一个有效响应成为权威结果，之后返回 `interaction.already_resolved`。

## 12. Error

### 12.1 格式

```json
{
  "type": "error",
  "body": {
    "code": "authorization.scope_denied",
    "message": "Device is not allowed to change the model.",
    "retryable": false,
    "correlationId": "6dfa044f-05f8-4a02-b4a1-a4d9d034298b",
    "details": {}
  }
}
```

- `code` 是稳定机器标识；客户端不得解析 `message` 做逻辑判断。
- `message` 是安全、简短、可展示的描述，不得包含密钥、token、完整 prompt、敏感路径或内部堆栈。
- `correlationId` 通常引用导致错误的 `messageId` 或 `requestId`。
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
auth.proof_invalid
authorization.scope_denied
pairing.already_claimed
pairing.expired
pairing.consumed
sync.cursor_epoch_mismatch
sync.cursor_expired
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
internal.unavailable
```

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
| 单连接待发送队列 | 8 MiB 或 2,000 messages，先到者触发 |
| 认证超时 | 15 秒 |
| 配对默认有效期 | 5 分钟 |

认证前固定上限不能等待 `auth.authenticated` 下发：

| 项目 | v1 上限 |
|---|---:|
| WebSocket upgrade headers | 16 KiB |
| 第一条 `auth.client_hello` | 32 KiB |
| pairing HTTP request body | 16 KiB |
| supported + required feature 总数 | 64 |
| 单个 feature ID | 64 ASCII bytes |
| 单 IP pending authentication | 5 connections |
| 单 IP 新认证尝试 | 10/minute，允许配置更严格值 |
| 单 pairing proof 失败 | 5 次后使 pairing 失效 |

- WebSocket 建立后第一条消息必须是 `auth.client_hello`，且必须在 5 秒内完整到达。
- 整个认证必须在 15 秒内完成；认证前最多只接收 `client_hello` 和 `client_proof` 各一次。
- 超限应在读取/分配完整 body 前尽早拒绝；未认证错误不得回显请求内容或暴露设备是否存在。
- feature ID 只允许 `[a-z0-9.-]`，不得为空、包含连续分隔 NUL 或通过 Unicode 形成视觉混淆。

- 服务端可下调并通过 `auth.authenticated.limits` 公布运行时值。
- 超过单消息上限应在分配大对象前拒绝。
- attachment、大型 diff 和完整终端输出不内嵌绕过限制；未来使用单独的受授权内容接口或 binary feature。
- 待发送队列达到高水位后先停止读取新的 replay batch；持续过慢则发送 `resource.backpressure` 并断开。
- 断开慢客户端后由其 cursor 重放，不能丢弃 Agent 事件或阻塞其他连接。
- rate limit 至少按 IP、device ID 和 command 分类；网络身份不能替代设备授权。

## 15. 安全处理要求

- 每个业务命令在当前连接身份下重新做 scope 授权。
- 设备撤销后立即关闭所有对应连接，并拒绝后续认证。
- 配对 secret、私钥、HMAC、签名输入中的秘密和完整认证 payload 不得记录日志。
- nonce 必须单次连接使用；服务端在认证完成或超时后清除 pending challenge。
- 比较 HMAC、固定 hash 和其他秘密派生值时使用 constant-time compare。
- JSON schema 验证发生在进入 Broker 之前；授权发生在业务用例执行之前。
- Agent 生成的 Markdown、HTML、终端、diff 和工具输出始终是不可信数据。
- `Origin`、Host key、device key 和 scopes 都是独立检查，任一通过不能替代其他检查。
- TLS channel 不使用应用长期 signing key 做加密；身份密钥和连接临时密钥保持分离。

## 16. 兼容与演进

### 16.1 v1 内允许的兼容变化

- 在既有开放扩展点增加字段，或通过已协商 feature 增加仅发送给支持方的可选字段，并同步更新 schema。
- 增加新 event type、error code、command 或 feature ID。
- 增加客户端可忽略的 `details` 字段。
- 下调运行时 limits，但不得低于实现文档声明的可工作最小值。

### 16.2 需要新 feature 或 major version 的变化

- 改变现有字段含义、类型或必填性。
- 改变排序、ACK、cursor 或幂等语义。
- 改变 transcript codec、算法、domain 或 field tag。
- 引入 binary framing、压缩、附件传输或不同 Transport Profile。
- 增加 `session.create`、imported resource origin 等能力需要新 feature/schema；授权由 scope 和 Owner Export Policy 决定，不能再按手机/电脑形态硬编码。

### 16.3 数据迁移

- 服务端必须保留 `serverEpoch`，普通升级不得无故使所有 cursor 失效。
- 客户端缓存 schema 可以独立演进；无法迁移时清除缓存并请求 snapshot，不得清除长期设备私钥。
- Host 或 Device identity key 变化不是普通协议迁移，必须按重新配对处理。

## 17. 测试与 Fixture

语言无关 schema 位于 [`schemas/sync/v1/`](../schemas/sync/v1/)，fixture 位于 [`fixtures/sync/v1/`](../fixtures/sync/v1/)。两者是本文的机器可验证伴随物；协议行为仍以本文为权威，发现不一致时必须一起修正。

fixture 目录至少覆盖：

```text
fixtures/sync/v1/
├─ valid/          应通过 schema 的认证、同步、事件、命令、错误和配对样例
├─ invalid/        必须失败并声明 expectedKeyword 的反向样例
├─ transcripts/    transcript、HMAC 和签名固定向量
└─ manifest.json   schema、fixture 与预期结果映射
```

第一批资产可以运行：

```text
node scripts/check-contract-assets.mjs
```

该脚本只做资产完整性和密码学固定向量检查。Rust/TypeScript 实现必须各自使用支持 Draft 2020-12 的 validator 执行 `manifest.json` 中的正反 schema fixture。

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

## 18. 第一阶段暂缓项

以下能力不进入 v1 baseline，未来只能通过 feature 或新 Transport Profile 增加：

- Noise over WSS 或不可信中继。
- Protobuf、MessagePack 或通用 binary envelope。
- 应用层压缩和 attachment streaming。
- 云端离线队列、云推送和跨 Host 聚合。
- 多用户账号、组织权限或远程身份提供商。
- 每条业务消息重复签名。
- PWA 跨 Origin 身份迁移。

采用 JSON 不代表 Rust 内部使用无边界 `serde_json::Value`。控制信封和已知消息必须使用明确 DTO；只有 ACP 扩展容器及明确开放的 extension 字段可以保留任意 JSON 值。
