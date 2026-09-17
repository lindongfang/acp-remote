# ACP Remote 初始设计文档

> 状态：编码前原始设计（Draft）  
> 版本：0.1  
> 日期：2026-09-17  
> 用途：记录项目最初的产品边界、架构选择、安全模型和发布方式，作为后续设计与编码的基线。

## 1. 项目目标

ACP Remote 是一个运行在用户电脑上的、有状态的 ACP（Agent Client Protocol）中转站。

它负责连接 Codex、Oh My Pi 等支持 ACP 的 Agent，并允许电脑客户端和已配对的手机客户端共同访问已有 ACP 会话。项目不实现 Agent 推理，也不替代 Codex、Oh My Pi 或其他 Agent。

核心目标：

- 会话只能在电脑端创建。
- 手机不能创建会话，但可以查看和控制电脑端已经创建的会话。
- 手机可以继续与 Agent 对话。
- 手机可以查看流式输出、工具调用、状态和历史消息。
- 手机可以查看可用模型，并为已有会话选择或切换模型。
- 手机可以接收会话状态变化和需要关注的通知。
- 项目不与 Zed 强绑定；Zed 只是可选的 ACP 客户端。
- 项目不使用应用级云服务器或持久化云中继。
- 网络连接方式可替换，不与 Tailscale 耦合。
- 电脑与手机首次扫码配对，之后建立长期信任，不需要每次扫码。
- 核心 Daemon 和 CLI 使用 Rust 实现，最终通过 npm 分发预编译二进制。

## 2. 非目标

第一阶段明确不包含：

- 不在云端运行 Agent。
- 不在云端保存聊天记录、命令或凭据。
- 不提供电脑离线时的消息排队。
- 不提供电脑离线时的远程历史读取；手机只能查看本地已缓存内容。
- 不实现模型供应商账号系统。
- 不允许手机配置 Provider API Key、OAuth 凭据或订阅。
- 不允许手机选择新的工作目录或扩大文件系统访问范围。
- 不重新实现 Codex、Oh My Pi 的 Agent Runtime。
- 不将 Tailscale、Zed 或某个具体 Agent 写入核心协议。
- 第一阶段不追求完全复刻 Happy 的云端同步、推送和多设备服务。

## 3. 已确认的产品边界

### 3.1 电脑端

电脑端负责：

- 启动 ACP Remote Daemon。
- 安装并登录具体 Agent。
- 创建会话。
- 选择初始 Agent、模型和工作目录。
- 配置 Agent 凭据、MCP、技能、沙箱和权限策略。
- 配对、查看和撤销手机设备。
- 查看和操作已有会话。

### 3.2 手机端

手机端允许：

- 查看电脑在线状态。
- 查看电脑端已经创建的会话。
- 查看消息、流式输出、计划、工具调用和执行状态。
- 向已有会话发送 prompt，继续与 Agent 对话。
- 查看当前 Agent 暴露的模型列表。
- 为已有会话选择或切换模型。
- 接收会话完成、失败、等待输入、等待权限等状态通知。
- 取消当前 turn。
- 响应 Agent 的结构化提问。
- 审批或拒绝 Agent 当前发起的权限请求。
- 切换 Agent 实际暴露的运行模式。

建议支持但仍需在实现前进一步确认产品细节的操作：

- 调用 Agent 暴露的 slash command。

手机端不允许：

- 创建新会话。
- 删除电脑端会话，除非未来显式授予权限。
- 更换会话的 Agent 类型。
- 更换工作目录或添加额外目录。
- 新增或修改 MCP Server。
- 查看、写入或导出 Agent/Provider 凭据。
- 修改全局沙箱和安全策略。

### 3.3 Zed

Zed 不是系统核心，也不是会话所有者。它是一个可选的电脑端 ACP Client：

- 可以通过本地 `acp-remote acp-stdio` 接入 Daemon。
- 可以列出和加载 Daemon 管理的会话。
- 可以继续已有会话。
- 是否允许 Zed 创建会话由电脑端策略决定；默认可以把所有电脑侧调用方视为有创建权限。

### 3.4 前端交付策略

客户端的阶段、行为和平台边界以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为权威来源。

- 前端第一阶段只实现 Web/PWA，不实现 Android/iOS 原生包。
- PWA 是真实的端到端验证客户端，与后续原生客户端共享协议、状态机和功能分层，不另建一次性 UI。
- 后续客户端采用 TypeScript 与 Expo/React Native 通用工程，通过平台 adapter 使用 Keychain、Keystore、SQLite、相机和生命周期能力。
- PWA 不能被视为原生安全存储、后台连接和系统通知的等价实现。

## 4. 总体架构

```text
电脑 CLI/Desktop ──┐
                   │
PWA/手机 App ──────┼── ACP Remote Daemon ── ACP stdio ── Codex ACP
                   │          │
Zed（可选）────────┘          └───────────── omp acp
                                              其他 ACP Agent
```

Daemon 是系统唯一权威：

- 它是底层 ACP Agent 的唯一 ACP Client。
- 它管理 Agent 子进程、ACP request ID 和会话映射。
- 它接收来自电脑和手机的命令。
- 它将 Agent 事件持久化后分发给所有订阅客户端。
- 它强制执行设备权限，而不是依赖客户端隐藏按钮。

客户端之间不直接通信，也不能直接共享 Agent 的 stdin/stdout。

## 5. 核心模块

本节描述 Daemon 的**运行时职责**，不代表 Rust crate 的拆分方式。一个职责可能由多个 crate 协作完成，也可能只是某个 crate 的内部模块。crate 名称、边界和依赖关系以 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) 为唯一权威来源。

```text
ACP Remote Daemon
├─ Agent Process Manager   Agent 子进程生命周期
├─ ACP Transport           JSON-RPC/stdin/stdout
├─ Session Registry        会话与 Agent 会话 ID 映射
├─ Session Actor           每会话串行命令队列
├─ Command Router          命令验证、授权和幂等
├─ Event Store             SQLite 持久化
├─ Event Dispatcher        实时广播与断线补发
├─ Identity Service        配对、认证、撤销
├─ Sync Server             HTTP/WebSocket
└─ Local ACP Facade        可选 Zed/IDE 接入
```

### 5.1 Agent Process Manager

- 启动 `codex-acp`、`omp acp` 等程序。
- stdout 只按 ACP JSON-RPC 解析。
- stderr 作为 Agent 日志单独采集。
- Agent 异常退出时发布 `agent.disconnected`。
- Daemon 退出时清理完整子进程树。
- Windows 上应使用 Job Object 或等价机制避免孤儿进程。

### 5.2 Session Actor

每个会话拥有独立的串行执行器：

```text
session-1 -> command queue -> agent
session-2 -> command queue -> agent
```

约束：

- 同一会话同时最多存在一个 active turn。
- 不同会话可以并行执行。
- Agent 忙碌时，新 prompt 进入队列或由策略拒绝。
- 模型切换在空闲时立即生效；运行中则默认从下一 turn 生效。

### 5.3 ACP 兼容性不变量

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

ACP Remote 是 `ACP-aware broker`，不是把不同 Agent 压缩成最低公共能力集合的抽象层。实现必须遵守：

- ACP 标准能力保持原有语义；中转站只增加会话 ID 映射、事件序列和来源等必要元数据。
- Broker 必须理解的能力转换为统一领域模型；Broker 无需理解的合法能力和 Agent 扩展应保留原始 ACP payload 并尽可能透传。
- Sync Protocol 使用 `view + acp.rawJson` 双轨表示，防止浏览器解析未知字段时损坏大整数、键顺序或数字文本；公共视图不能替代原始 ACP document。
- 解码未知字段或新版扩展时不得静默丢弃；无法安全处理时必须返回明确的 `unsupported` 或协议错误。
- 权限请求、工具调用、终端、文件修改和 diff 等结构化事件不得降格为普通文本。
- `acp-facade` 必须如实进行 capability negotiation，不得声明实际无法提供的能力。
- 手机端暂不支持某项能力时，应显示明确的降级或不支持状态，不能改变该能力的含义。
- 存储可以按保留策略压缩或清理无价值的流式噪声，但在事件有效期内必须保留完成重放所需的结构化语义和扩展数据。

具体 ACP v1 方法、通知、content block、capability gate、各层处理策略和验收测试以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 及其机器矩阵为准。任何能力不得只靠文档中的概括性描述宣称支持。

## 6. 网络与部署

### 6.1 Direct Only

第一阶段只支持手机直接连接电脑，不提供应用级云中继：

```text
手机 -> HTTPS/WSS -> 可信的同机 TLS 终止点 -> 电脑 ACP Remote Daemon
```

可用网络路径：

- 同一局域网。
- Tailscale。
- 用户自行配置的 WireGuard、IPv6、反向代理或其他可达网络。

### 6.2 不耦合 Tailscale

ACP Remote 核心只认识：

- IP/域名。
- HTTPS/WSS 正式 endpoint。
- 仅限 loopback 的 HTTP/WS 开发 endpoint。
- 用户提供的 endpoint URL。

核心不得：

- 调用 Tailscale SDK。
- 依赖 `tailscale` CLI 才能启动。
- 根据 `100.x.x.x` 或 `.ts.net` 自动授予信任。
- 把 tailnet 成员身份当作应用身份。

Tailscale 仅作为推荐部署方式。可选集成模块可以帮助检查安装和生成 endpoint，但不能成为核心依赖。默认安全 Profile 及信任边界见 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)。

建议 Daemon 默认监听本机地址，由外部网络层暴露：

```text
acp-remote -> 127.0.0.1:8765
Tailscale Serve/反向代理 -> 127.0.0.1:8765
```

正式模式要求 TLS 直接终止在 Daemon 或用户电脑上的可信 Provider。局域网模式可以使用同机反向代理和受信本地 CA；Daemon 不应默认无认证监听 `0.0.0.0`。明文 HTTP/WS 只允许显式开发模式和 loopback。

### 6.3 Endpoint 与身份分离

原生客户端保存一个电脑身份和多个候选地址，并在每个 endpoint 上验证同一个 Host identity：

```json
{
  "hostId": "host-uuid",
  "hostPublicKey": "base64...",
  "endpoints": [
    "wss://work-pc.example.ts.net/sync",
    "wss://work-pc.home.arpa/sync"
  ]
}
```

原生客户端的 IP/endpoint 变化只需要重新发现地址，不需要重新配对。任何 endpoint 都必须证明持有同一个电脑私钥。

PWA 是例外：浏览器密钥和 IndexedDB 按 Origin 隔离，一个 PWA 设备身份只绑定一个 `scheme + hostname + port` canonical origin。更换 Origin 必须作为新设备重新配对，第一阶段不支持 PWA 跨 Origin 自动切换；同一 Origin 内的路径变化不受影响。详见 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)。

### 6.4 无云模式限制

- 电脑关机、休眠或 Daemon 停止时，手机无法控制 Agent。
- 手机只能查看已经缓存到本地的历史。
- 手机不能在电脑离线时可靠排队 prompt。
- 手机后台 WebSocket 可能被操作系统挂起。
- 在不使用 APNs/FCM 等云推送服务时，后台通知不能保证实时到达。
- 手机重新打开应用后，通过 cursor 自动补发遗漏事件。

## 7. 消息分发与同步

本节只说明产品级同步语义。JSON 信封、认证消息、cursor、快照、ACK、错误码、限制及 transcript 的完整 wire contract 以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 为唯一权威来源。

### 7.1 分发原则

```text
Agent event
-> 规范化
-> SQLite commit
-> Event Dispatcher
-> 所有有权限且已订阅的客户端
```

必须先提交数据库，再广播，避免进程在广播与持久化之间崩溃造成不可恢复的消息。

### 7.2 事件结构

```json
{
  "globalSequence": "2318",
  "sessionSequence": "109",
  "eventId": "evt-uuid",
  "sessionId": "sess-123",
  "type": "agent.message.delta",
  "origin": "agent",
  "createdAt": "2026-09-17T20:10:00Z",
  "payload": {}
}
```

- `globalSequence`：一台电脑范围内的增量同步游标。
- `sessionSequence`：单会话严格排序。
- `eventId`：客户端去重。
- `origin`：记录事件来自 Agent、电脑还是手机。

### 7.3 订阅与补发

客户端连接后发送：

```json
{
  "type": "subscribe",
  "scope": "machine",
  "afterSequence": 2280
}
```

Daemon：

1. 从 SQLite 补发 `afterSequence` 之后的可见事件。
2. 切换到实时事件流。
3. 接收客户端 ACK。
4. 断线后从最后 ACK 继续。

投递语义：

- 事件至少投递一次。
- 客户端使用 `eventId` 去重。
- 命令使用 `requestId` 实现重试幂等；重复重试不得造成第二次接受或 Agent 派发，崩溃窗口无法确认时显式返回 `uncertain`。
- 不尝试宣称网络层“恰好一次”。

### 7.4 慢客户端

每条 WebSocket 连接使用有上限的发送队列。客户端过慢时：

1. 停止继续堆积内存。
2. 断开慢连接。
3. 客户端重连。
4. 从 SQLite 按最后 ACK 补发。

慢手机不能阻塞 Agent 或其他客户端。

## 8. 客户端命令

手机和电脑使用受约束的业务命令，而不是直接发送任意 ACP JSON-RPC。

示例：

```json
{
  "type": "command",
  "command": "session.prompt",
  "sessionId": "sess-123",
  "requestId": "client-generated-uuid",
  "payload": {
    "content": [
      { "type": "text", "text": "继续修复测试" }
    ]
  }
}
```

Daemon 立即返回接受、排队或拒绝状态；实际执行结果通过事件流发送。

初期命令集合：

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
```

只有电脑端具有：

```text
session.create
session.delete（可选）
workspace.select
agent.configure
provider.configure
device.manage
```

## 9. 模型选择与切换

Daemon 将不同 Agent 的模型能力统一抽象：

```rust
trait AgentAdapter {
    async fn list_models(&self, session_id: &str) -> Result<Vec<Model>>;
    async fn current_model(&self, session_id: &str) -> Result<ModelRef>;
    async fn set_model(&self, session_id: &str, model: &ModelRef)
        -> Result<SetModelResult>;
    fn supports_runtime_model_switch(&self) -> bool;
}
```

规则：

- 手机只能选择电脑端已经配置且当前 Agent 实际暴露的模型。
- Provider 凭据不发送到手机。
- 模型切换带 `expectedVersion`，避免并发覆盖。
- 当前 turn 运行中时，切换默认从下一 turn 生效。
- 如果 Agent 不支持会话内切换，必须明确返回“不支持”，不能静默丢失上下文。
- 模型变化通过事件同步给所有客户端。

## 10. 数据存储

项目需要保存足以支持手机历史查看和断线恢复的数据，但不永久保存完整 ACP 原始流。

### 10.1 长期保存

- 用户最终消息。
- Agent 合并后的最终回复。
- 工具调用摘要和结果状态。
- 权限请求与最终决策。
- 模型、模式变化。
- Turn 生命周期。
- 必要的文件修改摘要。
- 会话与底层 Agent session ID 映射。

### 10.2 短期保存

- 流式文本 delta。
- 临时工具进度。
- 未完成的请求状态。

所有带 sequence 并进入 Sync Protocol 的流式 delta 都必须先写入短期事件日志，再广播。`persist_deltas: false` 只表示 turn 完成并形成最终消息后可以按 ACK、TTL 和容量策略压缩或清理，不允许绕过“先持久化、后广播”；清理造成 cursor 超出保留窗口时必须要求客户端重建 snapshot。

### 10.3 不保存或严格限量

- 心跳、typing、presence：只放内存。
- reasoning/thinking：默认不长期保存。
- 终端完整输出：限制 head/tail 和总大小。
- 大型 diff：压缩并限制容量。
- 图片和附件：独立文件或内容寻址存储，配置配额。

### 10.4 建议默认值

```yaml
storage:
  transcript_retention: 90d
  sync_event_retention: 7d
  max_total_size: 2GB
  max_session_size: 100MB

terminal:
  max_output_per_command: 1MB
  keep_head_bytes: 128KB
  keep_tail_bytes: 896KB

attachments:
  max_file_size: 20MB
  max_total_size: 1GB

streaming:
  persist_deltas: false
  flush_interval_ms: 250
```

这些值是初始建议，编码时应成为可配置项并通过实际使用数据调整。

## 11. 双方认证与长期配对

本节描述产品级身份行为；系统威胁模型、数据保护、授权分级、供应链和安全验收以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为权威来源，具体 wire contract 以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 为准。

### 11.1 安全原则

- 网络可达不等于应用授权。
- Tailscale 身份不能替代 ACP Remote 设备身份。
- 每台电脑和手机拥有独立长期密钥。
- 首次扫码只用于建立信任。
- 后续连接自动执行双向设备签名认证。
- TLS 为每次连接产生临时会话密钥，应用长期密钥只用于设备和 Host 身份签名。
- 每个业务命令仍需执行权限检查。

### 11.2 首次配对

电脑生成长期身份：

```text
hostId
hostPrivateKey       ECDSA P-256，不可写入普通存储
hostPublicKey        P-256 public key
```

`acp-remote device pair` 创建一次性二维码：

```json
{
  "protocol": "acp-remote-pairing-v1",
  "hostId": "host-uuid",
  "hostPublicKey": "base64...",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "pairingId": "pair-uuid",
  "pairingSecret": "256-bit-random-secret",
  "expiresAt": 1789650000
}
```

规则：

- `pairingSecret` 使用 CSPRNG 生成。
- 配对请求只允许成功一次。
- 二维码 2–5 分钟过期。
- 新配对使旧的未完成配对失效。
- 手机扫码后生成自己的长期设备密钥。
- 电脑端显示设备名称、指纹和短验证码，并要求用户确认。
- 手机显示相同验证码，降低二维码转发攻击风险。
- `pairingSecret` 只作为短期配对状态凭据：设备首次 WSS 认证成功时立即清除，未完成、拒绝或未连接的 pairing 最迟在原过期时间清除；不得延长或转成长期 bearer token。
- PWA 使用 `pairingSecret` 对规范 transcript 计算 HMAC-SHA256 proof；短验证码使用独立 domain tag 派生。
- 所有签名/HMAC transcript 使用固定字段顺序的长度前缀二进制编码，不直接签名普通 JSON。

### 11.3 后续连接

正常情况下不再扫码：

```text
客户端启动
-> PWA 使用已配对 canonical origin；原生客户端选择可达 endpoint
-> 建立可信 WSS
-> 验证 Host 签名并完成设备 challenge-response
-> 检查设备是否撤销
-> 发送最后同步 cursor
-> 补发事件并进入实时通信
```

默认安全 Profile 的 Host 与设备签名统一使用 ECDSA P-256 + SHA-256；wire signature 固定为 64-byte P1363 `r || s` 后进行无填充 base64url 编码。TLS/WSS 负责临时密钥、机密性和完整性。正式 TLS 必须直接终止在 Daemon 或用户电脑上的可信 Provider。Noise 仅保留为未来不可信中继等场景的可选 Transport Profile，不进入第一阶段。

每个新 WSS 连接都使用新 nonce 完整执行 challenge-response，不签发长期 bearer session/refresh token；认证状态随连接关闭而失效。

### 11.4 设备记录与权限

电脑保存：

```json
{
  "deviceId": "phone-uuid",
  "name": "User Phone",
  "clientKind": "pwa",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "keyAlgorithm": "ECDSA_P256_SHA256",
  "signatureEncoding": "P1363_BASE64URL",
  "publicKey": "base64...",
  "permissions": [
    "session.list",
    "session.read",
    "session.prompt",
    "session.cancel",
    "session.model.list",
    "session.model.set",
    "session.mode.list",
    "session.mode.set",
    "permission.resolve",
    "elicitation.respond"
  ],
  "createdAt": "...",
  "revokedAt": null
}
```

每条消息必须检查：

- 连接是否完成双向认证。
- `deviceId` 是否匹配当前连接身份。
- 设备是否已撤销。
- 设备是否拥有该命令权限。
- `requestId` 是否已经处理。
- 连接消息计数器是否有效，防止重放。

### 11.5 密钥存储

电脑：

- Windows：DPAPI 或 CNG/TPM。
- macOS：Keychain。
- Linux：Secret Service 或经过单独评审的系统 keystore；不可用时正式模式失败关闭。持久化加密文件 fallback 必须先通过独立 ADR，不能临时自研。

手机：

- iOS：Keychain/Secure Enclave 中的 P-256 签名密钥。
- Android：Keystore 中的 P-256 签名密钥。
- 可用时优先硬件保护。

私钥不得写入日志、普通配置文件或 SQLite 明文字段。

### 11.6 撤销和重新配对

```text
acp-remote device list
acp-remote device revoke <device-id>
acp-remote device revoke-all
```

撤销后：

- 立即关闭该设备现有连接。
- 拒绝新的认证握手。
- 清除短期恢复凭据。
- 保留不含敏感数据的审计记录。

需要重新扫码的情况：

- 用户主动解除配对或撤销设备。
- 手机卸载应用或清除数据。
- 任一端长期身份密钥丢失、损坏或被怀疑泄露。
- 电脑重装并丢失 Daemon 数据。
- 发生不受信任的电脑身份密钥变化。

网络切换、IP 变化、重启、升级和 WebSocket 重连不需要重新扫码。

## 12. WebSocket 与输入安全

- 对每条消息独立执行身份和权限检查。
- 使用明确 schema 验证所有输入。
- 设置消息尺寸、连接数和命令频率限制。
- 浏览器客户端使用明确的 Origin allowlist。
- 设置握手、读取、写入和空闲超时。
- 日志不记录完整 prompt、响应、token、私钥或敏感路径。
- 附件必须检查声明大小、实际大小和类型。
- 不允许手机构造任意底层 ACP 方法。

## 13. Rust 实现建议

详细的 crate 职责、依赖矩阵、端口设计和防腐化规则见：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)。

### 13.1 Workspace

仓库顶层暂按交付物划分：

```text
acp-remote/
├─ Cargo.toml
├─ crates/                 Rust workspace
├─ npm/
│  ├─ acp-remote/
│  └─ platforms/
├─ clients/
│  └─ app/                  Expo/React Native 通用客户端
└─ docs/
```

`crates/` 下的具体成员不在本文重复维护，当前规划见 [MODULE_ARCHITECTURE.md § 3](./MODULE_ARCHITECTURE.md#3-建议的-rust-workspace)。实际创建 workspace 时，以该文档为准；架构调整也应先修改该文档。

### 13.2 候选技术栈

```text
异步运行时       tokio
HTTP/WebSocket   axum
序列化           serde / serde_json
SQLite           sqlx 或 rusqlite（优先 bundled SQLite）
CLI              clap
日志             tracing
错误处理         thiserror / anyhow
子进程           tokio::process
设备签名         WebCrypto 兼容的 ECDSA P-256 验证实现
TLS（如直终止）  rustls 或等价成熟实现
```

具体 crate 在实现前通过维护状态、安全性、跨平台和许可证进行选择。

### 13.3 CLI

```text
acp-remote daemon start
acp-remote daemon stop
acp-remote daemon status

acp-remote session create
acp-remote session list

acp-remote device pair
acp-remote device list
acp-remote device revoke

acp-remote acp-stdio
acp-remote doctor
```

## 14. npm 分发

npm 是安装入口，不是运行时实现。用户安装预编译 Rust 二进制，不需要 Rust 工具链。

推荐包结构：

```text
acp-remote
@acp-remote/win32-x64-msvc
@acp-remote/win32-arm64-msvc
@acp-remote/darwin-arm64
@acp-remote/darwin-x64
@acp-remote/linux-x64-gnu
@acp-remote/linux-arm64-gnu
@acp-remote/linux-x64-musl
@acp-remote/linux-arm64-musl
```

主包：

- 使用 `bin` 暴露 `acp-remote` 命令。
- 使用 `optionalDependencies` 引用平台包。
- JavaScript launcher 只选择并启动正确二进制。
- 原样转发 stdin/stdout/stderr、参数、信号和退出码。
- 平台包缺失时给出明确错误，不静默在线下载未知二进制。

首批平台：

```text
Windows x64
macOS ARM64
Linux x64 glibc
```

后续再增加其他架构和 musl。

发布流程：

1. CI 在对应平台构建和测试二进制。
2. 生成校验和和发布产物。
3. 先发布相同版本的所有平台包。
4. 最后发布 npm 入口包。
5. 所有平台包与主包严格保持同一版本。

## 15. MVP 实施顺序

### 阶段一：端到端 PWA MVP

- Rust Workspace、ACP/Sync 协议类型和最小 Broker。
- 启动一个 ACP Agent，优先选择 `omp acp` 验证。
- 实现 initialize、session/new、session/prompt、session/update、cancel。
- Session Actor、请求 ID 映射、SQLite 精简存储和 CLI 创建会话。
- HTTP/WebSocket Sync Server、事件序列、ACK、补发和命令幂等。
- 设备身份、一次性配对、双向认证、基本权限和撤销。
- 只交付 Web/PWA 前端：查看已有会话、继续对话、结构化事件、权限处理、模型列表与切换。
- PWA 静态资源由 Daemon 本地托管；不实现 Android/iOS 原生包。

### 阶段二：可靠性与安全完善

- 浏览器密钥方案与跨端加密互操作验证。
- 重连、慢客户端、缓存损坏、进程崩溃和数据库迁移测试。
- 存储 TTL、大小限制、流式事件合并和敏感信息审计。
- 多 endpoint、局域网与可替换 HTTPS 暴露方案。

### 阶段三：网络与可选客户端

- Tailscale 部署文档和可选辅助工具。
- 多 endpoint 自动连接。
- `acp-remote acp-stdio`。
- Zed 等 IDE 的可选接入。
- Codex ACP 适配和能力差异测试。

### 阶段四：原生客户端

- Android Expo Development Build，使用 Keystore、原生 SQLite、扫码和生命周期 adapter。
- iOS 客户端，使用 Keychain 并验证本地网络权限和后台恢复。
- 复用 PWA 已验证的 Sync Client、状态机、协议类型和 feature 层。
- 是否引入系统推送服务作为独立产品与隐私决策处理。

### 阶段五：发布

- Windows/macOS/Linux CI。
- npm 平台包和 launcher。
- 数据库迁移策略。
- 升级、回滚和 Daemon 单实例处理。
- 安全与异常恢复测试。

## 16. 编码前必须验证的风险

1. 各 ACP Agent 是否都支持会话恢复、模型列表和运行中模型切换。
2. Codex ACP 与 OMP ACP 的事件、权限和终端能力差异。
3. Zed 是否能正确展示由手机发起的、非 Zed 本端发起的 turn。
4. Windows 上 Daemon、子进程树和休眠恢复行为。
5. 手机后台 WebSocket 被系统挂起后的恢复体验。
6. WebCrypto P-256 密钥持久化及其与 Rust 的签名格式互操作性。
7. 手机和电脑 endpoint 变化时的安全发现方案。
8. SQLite 写入频率、流式事件合并和磁盘上限策略。
9. npm optional dependency 在 npm、pnpm、yarn 不同配置下的安装行为。
10. ACP 协议升级时的版本协商和向后兼容方式。

## 17. 核心设计原则

1. Daemon 是会话与安全状态的唯一权威。
2. 底层 Agent 只保持一个受控 ACP 连接。
3. 先持久化、后广播。
4. 每会话串行执行，不允许并发 prompt 破坏状态。
5. 事件至少投递一次，客户端负责去重。
6. 命令可重试，但效果必须幂等。
7. 网络可达与应用认证分离。
8. 设备身份与 endpoint 分离。
9. 长期设备身份与临时连接密钥分离。
10. 手机权限由 Daemon 强制执行。
11. 不永久保存无价值的流式和终端噪声。
12. 网络层、IDE 和具体 Agent 都必须可替换。
13. npm 只负责分发，Rust 二进制负责实际运行。
14. 第一阶段保持无应用云服务器的本地优先设计。
15. 遵守 ACP 兼容性不变量：只增强传输与协调能力，不削弱、曲解或静默丢弃 ACP 原生能力。

## 18. 外部参考

- ACP Protocol：<https://agentclientprotocol.com/protocol/v1/overview>
- ACP Session Setup：<https://agentclientprotocol.com/protocol/v1/session-setup>
- ACP Session List：<https://agentclientprotocol.com/protocol/v1/session-list>
- Zed External Agents：<https://zed.dev/docs/ai/external-agents>
- Happy：<https://github.com/slopus/happy>
- Oh My Pi：<https://github.com/dankalish/oh-my-pi>
- Codex ACP：<https://www.npmjs.com/package/@agentclientprotocol/codex-acp>
- Noise Protocol Framework：<https://noiseprotocol.org/>
- libsodium Key Exchange：<https://doc.libsodium.org/key_exchange>
- Web Cryptography API：<https://www.w3.org/TR/WebCryptoAPI/>
- OWASP WebSocket Security：<https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html>
- npm package.json：<https://docs.npmjs.com/files/package.json/>
- Cargo Targets：<https://doc.rust-lang.org/cargo/reference/cargo-targets.html>
