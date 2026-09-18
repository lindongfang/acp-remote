# ADR-0001：PWA 身份与安全传输

- 状态：已接受
- 日期：2026-09-17
- 修订记录（2026-09-18）：配对阶段的 proof 与 SAS 为 **HMAC-SHA256**，不是双方 ECDSA 签名（以 [SYNC_PROTOCOL.md](../SYNC_PROTOCOL.md) §7 与 `fixtures/sync/v1/transcripts/pairing-*.json` 的 `hmacSha256` 固定向量为准）；ECDSA P-256 只用于每次 WSS 连接的 challenge-response。下文“首次配对”第 5 步已按此订正。
- 决策范围：第一阶段 PWA，以及后续 Android/iOS/桌面网络客户端的默认安全 Profile

## 背景

ACP Remote 不使用应用级云中继，PWA 直接连接一个 ACP Remote 服务节点。页面、Service Worker、WebCrypto、摄像头和 WebSocket 都需要可信 secure context。网络可达和 Tailscale 身份不能替代 ACP Remote 自己的设备身份与权限。本 ADR 只约束 PWA/设备到节点的 Sync 连接；节点间 Node Link 使用独立身份与 transcript。

原设计建议使用 Noise 同时完成双向认证与应用层加密。但 PWA 即使使用 Noise，仍需要 HTTPS 才能满足浏览器安全要求；在 WSS 内再实现 Noise 会引入浏览器 WASM、密钥处理、跨语言互操作、framing 和实现审计成本。

## 决策

默认安全 Profile 定为：

```text
可信 HTTPS/WSS
+ ECDSA P-256 设备签名 challenge-response
+ Daemon 设备权限与撤销
```

该 Profile 适用于：

- 第一阶段 PWA。
- 后续 Android/iOS 原生客户端。
- 后续需要通过网络连接的桌面客户端。

Noise 不进入第一阶段，也不是原生客户端的默认路径。只有未来出现不可信中继、远程 TLS 终止点或必须在没有可信证书的网络上建立原生端到端加密等明确需求时，才评估独立的 Noise Transport Profile。

## 信任边界

正式模式要求 TLS 终止点位于服务节点主机的可信边界内：

```text
Client
  -> HTTPS/WSS
  -> Daemon direct TLS | same-host TLS provider
  -> loopback
  -> Daemon
```

可接受的 TLS Provider 包括：

- Daemon 将来直接加载用户提供的证书。
- 同机 Tailscale Serve。
- 同机 Caddy/Nginx 或用户已有的可信反向代理。

核心不得依赖某个 Provider 的 SDK、CLI、身份 Header 或网络成员身份。若 TLS 在另一台机器、云服务或其他不可信位置终止，默认 Profile 不再提供客户端到 Daemon 的端到端机密性，该部署不属于第一阶段正式支持范围。

## 密钥与职责

### TLS

- 提供传输机密性、完整性、endpoint 证书验证和临时会话密钥。
- PWA 只允许使用 `https://` 和 `wss://` 正式 endpoint。
- 明文 HTTP/WS 只允许显式开发模式和 loopback。

### Host identity

- Daemon 拥有独立的长期 ECDSA P-256 + SHA-256 host signing key。
- 二维码固定 `hostId` 与 host public key。
- 客户端在每次认证时验证 host signature；TLS endpoint 改变不自动改变 host identity，但 PWA 是否能沿用设备私钥还受 Origin 约束。

### Device identity

- PWA 使用 WebCrypto 生成不可导出的 ECDSA P-256 私钥，并将 `CryptoKey` 保存在 IndexedDB。
- Android 使用 Android Keystore，iOS 使用 Keychain/Secure Enclave；优先使用硬件保护。
- 客户端只向 Daemon提供公钥。
- PWA 与原生 App 即使位于同一台手机，也属于不同设备，必须分别配对，不迁移私钥。

Host 和设备签名统一使用 ECDSA P-256 + SHA-256。wire signature 固定为 IEEE P1363 的 64-byte `r || s`，使用无填充 base64url 编码；wire protocol 禁止使用 ASN.1 DER。Rust 与 WebCrypto 必须共享固定测试向量。

### Canonical Origin

IndexedDB 与 WebCrypto 持久化身份受浏览器同源策略约束。因此一个 PWA 设备身份只绑定一个 canonical origin：

```text
scheme + hostname + port
```

- `https://pc.example.ts.net` 与 `https://pc.home.arpa` 是两个设备身份，必须分别配对。
- PWA 第一阶段不支持同一私钥跨 Origin 自动切换 endpoint。
- 同一 Origin 内的路径变化不影响身份。
- Android/iOS/桌面原生客户端的私钥不受浏览器 Origin 隔离，可以在验证同一 Host identity 后尝试多个 endpoint。
- PWA 设备记录必须保存其 canonical origin，认证请求中的 Origin 必须完全匹配。
- Origin 变化、浏览器 profile 变化或站点数据清除都要求重新配对，不能导出私钥进行迁移。

PWA 应调用 `navigator.storage.persist()` 请求持久存储；请求被拒绝不阻止运行，但必须把潜在密钥丢失视为重新配对场景。

### 签名 Transcript 编码

所有签名/HMAC 输入使用域分离、固定字段顺序的长度前缀二进制编码，不直接签名普通 JSON 文本：

```text
domainTag
fieldCount
repeated(fieldTag, byteLength, rawBytes)
```

- 字符串使用 UTF-8，整数使用固定宽度 big-endian，无可选空白和隐式默认值。
- 每种用途使用不同 `domainTag`，至少包括 pairing proof、pairing SAS、host challenge 和 device proof。
- 具体 field tag、整数宽度、上限和测试向量由 `SYNC_PROTOCOL.md` 定义。
- 未进入规范 transcript 的字段不能参与安全决策。

## 首次配对

1. 服务节点本地管理入口创建一次性 `pairingId`、256-bit `pairingSecret` 和过期时间。
2. 二维码包含 HTTPS endpoint、`hostId`、host public key 与一次性配对数据。
3. PWA 确认 `window.isSecureContext`，生成不可导出的设备密钥。
4. PWA 使用 `pairingSecret` 对规范 pairing transcript 计算 HMAC-SHA256 proof，Daemon 验证后才接受设备公钥。
5. 双方各自用 `pairingSecret` 对包含 Host/设备公钥及随机 nonce 的规范 transcript 计算 HMAC-SHA256 证明，并使用独立 domain tag 从 HMAC-SHA256 结果派生相同的短验证码；用户在服务节点本地确认。配对阶段不使用 ECDSA 签名，`hostProof`/`deviceProof` 的 ECDSA P-256 签名只用于 WSS challenge-response。
6. Daemon 保存 device public key 与 scopes；为支持 `approved` 状态的可靠轮询，`pairingSecret` 只保留到该设备首次 WSS 认证成功或原过期时间，随后立即清除。它不得延长、复用或转成长期 bearer token。

二维码若使用 URL，秘密数据放在 fragment 而不是 query；PWA 读取后立即从地址栏和历史条目中清除。完整字段、规范编码和错误码在 Sync Protocol 中定义。

## 后续连接

每个 WSS 连接执行新的 challenge-response：

```text
client.hello
  -> server.challenge + host signature
  -> client.proof + device signature
  -> server.authenticated + scopes
```

签名 transcript 至少绑定协议域、版本、`hostId`、`deviceId`、client/server nonce、连接标识和 Web Origin，防止重放、跨主机与跨协议使用。认证后仍对每个业务命令执行 scope、撤销状态和 `requestId` 检查。

默认 Profile 不给每条业务消息重复签名；认证后的 WSS 连接负责传输机密性与完整性。消息 sequence/counter、ACK 和 `requestId` 继续承担排序、恢复和业务幂等职责。

第一阶段不签发 bearer session/refresh token。每个新 WSS 连接都使用新 nonce 完整执行 challenge-response；认证状态只绑定当前连接，连接关闭即失效。

## PWA 特有约束

- 不可导出 `CryptoKey` 不等同于硬件密钥；不得宣称达到 Keychain/Keystore 的保护等级。
- 浏览器清除站点数据、切换 profile 或密钥损坏后必须重新配对。
- 非法脚本虽然不能导出私钥，仍可能调用签名操作，因此 PWA 不加载第三方脚本并使用严格 CSP。
- Agent 内容、Markdown、终端和 diff 全部按不可信输入渲染。
- 限制 `Origin` 和 `Host`，防止跨站 WebSocket 劫持和 DNS rebinding。
- 最低 CSP 基线为 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`；新增资源类型必须显式放行，不允许用 `*`、`unsafe-eval` 或远程 CDN 简化配置。

## 网络 Provider 策略

- 第一阶段参考部署使用 Tailscale Serve，但这只是部署建议。
- 第一阶段还必须通过至少一种非 Tailscale Provider：同机 Caddy Local CA、用户证书或等价同机反向代理。
- 高级用户可以提供自有域名、ACME 证书或已有反向代理。
- Daemon 配置只接收 `publicEndpoint`/allowlist，不根据 Provider 身份自动授权。

## 被否决或暂缓的方案

### PWA 第一阶段运行 Noise over WSS

暂不采用。它没有消除 PWA 对 HTTPS 的需求，却增加第二套安全通道和浏览器实现风险。

### 只依赖 TLS 或 Tailscale 身份

不采用。它不能提供稳定的 ACP Remote 设备 ID、细粒度 scope、撤销和 endpoint-independent host identity。

### Bearer token 作为长期设备身份

不采用。长期 bearer secret 更容易被复制和重放，也无法利用不可导出的平台签名密钥。

### WebAuthn 作为自动重连身份

暂不采用。其 RP ID 与 origin 绑定、用户交互和跨 endpoint 行为不适合作为当前自动重连的基础身份机制。

## 后果

正面结果：

- PWA、Android、iOS 可以共享同一认证消息和设备记录模型。
- 浏览器使用原生 WebCrypto，不需要第一阶段维护 Noise/WASM。
- TLS Provider 可替换，核心不与 Tailscale 耦合。
- 后续原生客户端只替换 keystore/lifecycle adapter。

代价与限制：

- 正式 PWA 必须有浏览器信任的 HTTPS endpoint。
- 同机 TLS Provider 被纳入可信计算边界。
- PWA 密钥没有统一的硬件保护保证。
- 未来若引入不可信中继，需要新增端到端加密 Profile，而不能直接复用默认信任假设。

## 编码前验证

实现完整 Broker 前先完成最小原型并验证：

1. Android Chrome、iOS Safari 和桌面浏览器的 secure context、Service Worker、Camera 与 WSS。
2. P-256 不可导出 `CryptoKey` 在 IndexedDB 中跨刷新和浏览器重启持久化。
3. Rust 与 WebCrypto 的 P1363 签名、长度前缀 transcript 和固定测试向量互操作。
4. pairing secret 过期、单次使用、短验证码和本机确认。
5. challenge 重放、错误设备、设备撤销、Host key 变化和错误 Origin 全部失败。
6. Tailscale Serve 与至少一种非 Tailscale HTTPS Provider 均可工作。
7. 同一 PWA 在 Origin 变化后不能读取或迁移旧私钥，并进入重新配对流程。
8. `navigator.storage.persist()` 获批和被拒绝两条路径都能正确处理。
