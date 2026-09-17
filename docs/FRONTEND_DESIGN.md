# ACP Remote 前端设计

> 状态：编码前客户端约束  
> 版本：0.2
> 日期：2026-09-18
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)  
> 后端模块边界：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)

## 1. 文档职责

本文是前端交付阶段、客户端行为、状态模型和平台边界的权威来源。它不定义 Rust Broker 内部结构，也不重复定义 [Sync Protocol](./SYNC_PROTOCOL.md) 的 wire schema。

发生冲突时：

- 产品权限、安全与同步语义以 `INITIAL_DESIGN.md` 为准。
- 认证消息、JSON DTO、cursor、ACK、重放和错误码以 `SYNC_PROTOCOL.md` 为准。
- Web/PWA 威胁模型、内容渲染、缓存和平台安全验收以 `SECURITY_DESIGN.md` 为准。
- Rust crate 职责和依赖以 `MODULE_ARCHITECTURE.md` 为准。
- ACP 能力在 PWA 中是完整操作、只读展示还是显式降级，以 `ACP_COMPATIBILITY_MATRIX.md` 为准。
- imported Agent/会话的 Owner、在线状态和缓存语义以 `NODE_LINK_PROTOCOL.md` 为准。
- 前端阶段、客户端分层与平台能力以本文为准。

## 2. 已确认决策

### 2.1 实现路线

- 客户端采用 TypeScript 和 Expo/React Native 通用工程。
- **前端的首个交付只实现 Web/PWA，不实现 Android/iOS 原生包。**
- 项目先完成 Owner—Access—Zed 的 Node Link 纵向切片，再实现 PWA；PWA 复用已验证的 Broker 和协议边界，不另建一套临时核心。
- 后续 Android/iOS 尽量复用页面、领域状态、Sync Client 和协议类型，通过平台适配器补齐原生能力。
- PWA 是首个可用客户端，但不被视为原生安全存储、后台连接和系统通知的等价实现。

### 2.2 产品边界

设备形态不定义权限。客户端行为由当前节点、principal scope、Owner Export Policy 和端到端 capability 的交集决定：

- 可以访问当前节点本地拥有或从其他节点导入、且已授权的 Agent/会话。
- 可以查看历史和实时事件、继续对话、取消 turn、处理权限请求、选择模型和调用 Agent 暴露且被允许的能力。
- 当前 PWA v1 不展示会话创建；Node Link 已支持的 `session.create` 只能选择 Owner 导出的 Agent 与 workspace template，未来 PWA 只增加该受限入口。
- 客户端不能提交任意 Owner 路径、配置 Provider 凭据、扩大沙箱权限或直接发送任意 ACP JSON-RPC。
- UI 隐藏按钮不是安全措施；Access 与 Owner Node 必须分别执行授权。

## 3. 前端总体架构

```text
Screens / Components
        ↓
Feature Controllers + Client State Machines
        ↓
Client Application Services
        ↓
Sync Client / Protocol Types
        ↓
Platform Ports
        ↓
WebSocket / Secure Storage / Local Cache / Camera / Lifecycle
```

建议目录：

```text
clients/
└─ app/
   ├─ app/                         Expo Router 页面
   ├─ src/
   │  ├─ domain/                  客户端领域类型与视图模型
   │  ├─ protocol/                生成或显式定义的 Sync DTO
   │  ├─ sync-client/             连接、认证、订阅、ACK、重放
   │  ├─ state/                   连接与会话状态机
   │  ├─ features/
   │  │  ├─ pairing/
   │  │  ├─ hosts/
   │  │  ├─ sessions/
   │  │  ├─ conversation/
   │  │  ├─ permissions/
   │  │  └─ models/
   │  ├─ components/
   │  └─ platform/
   │     ├─ secure-storage.web.ts
   │     ├─ secure-storage.native.ts
   │     ├─ local-cache.web.ts
   │     ├─ local-cache.native.ts
   │     ├─ lifecycle.web.ts
   │     └─ lifecycle.native.ts
   └─ public/
      ├─ manifest.json
      └─ icons/
```

规则：

- 页面不能直接读写 WebSocket、IndexedDB、SecureStore 或 SQLite。
- 平台差异集中在 `platform/`，不能散布大量 `Platform.OS` 分支。
- 协议 DTO 与 UI view model 分离，组件不直接依赖 wire payload。
- 客户端只保存策略允许的设备侧同步状态。对 imported resource 默认 `no-content-cache`，会话正文只在内存中；Owner 仍是会话事实来源。
- 不在客户端复制 Broker 的权限、会话串行或 Agent 生命周期规则。

## 4. 第一阶段：Web/PWA 客户端

### 4.1 必须实现

- 主机连接与首次配对。
- 主机身份显示、连接状态和重新配对入口。
- 已有会话列表与会话详情。
- 历史快照、cursor 补发和实时事件切换。
- 用户 prompt、Agent 流式回复和最终消息。
- 第一阶段只发送文本 prompt；图片、文件和 resource 输入必须明确显示不支持，不能静默删除后提交剩余内容。
- 结构化工具调用、权限请求、终端、文件修改和 diff 的基础展示。
- 取消当前 turn。
- 模型列表、当前模型、切换中、下一 turn 生效和不支持状态。
- WebSocket 断线重连、ACK、事件去重和命令幂等。
- 未识别 ACP 扩展事件的明确降级展示和诊断信息入口。
- 响应式手机布局，同时保证桌面浏览器可用于调试。

### 4.2 明确不包含

- Android APK/AAB 和 iOS App/IPA。
- APNs、FCM 或 Web Push 服务。
- 后台常驻 WebSocket。
- 电脑离线时的 prompt 排队。
- 原生 Keychain/Keystore 安全等级承诺。
- 生物识别解锁、系统小组件、分享扩展和系统级后台任务。
- 单独的云端 Web 托管或服务端渲染服务。

### 4.3 分发方式

PWA 构建为静态资源，由 Daemon 托管或随发行包分发：

```text
Expo Web export
      ↓
static assets
      ↓
ACP Remote Daemon /ui
      ↓
Browser + same-origin WebSocket /sync
```

优先使用相对 URL 和同源 WebSocket，避免额外的 CORS、endpoint 注入和版本漂移。前端构建版本必须与 Daemon 可检查地关联，协议握手仍需独立执行版本与 feature negotiation。

PWA 不需要独立云服务器。静态资源可以嵌入 Rust 二进制，也可以作为 npm 平台包中的只读资源随二进制安装；最终方式在验证二进制大小和升级策略后确定。

### 4.4 HTTPS 与安装限制

- 除浏览器认可的本地开发环境外，Service Worker、可安装 PWA、摄像头和部分密码学能力通常需要 secure context。
- 明文 HTTP/WS 只允许显式开发模式和 loopback，不能作为正式安全部署方式。
- 正式 TLS 必须直接终止在 Daemon 或用户电脑上的可信 Provider；HTTPS 可由 Daemon 的未来 TLS 能力或同机外部网络层提供，核心不得绑定 Tailscale、Caddy 或特定证书服务。
- 无 HTTPS 时，UI 必须明确显示“开发/不安全连接”，并禁止把该环境当作安全验收结果。

### 4.5 PWA 设备身份

- PWA 仍然必须经过应用级配对和双向认证，不能因为位于 LAN/Tailnet 就跳过认证。
- 私钥不得存入 `localStorage`、普通日志、URL、二维码历史或可导出的普通配置。
- 浏览器使用 WebCrypto 生成不可导出的 ECDSA P-256 私钥，并将 `CryptoKey` 保存在 IndexedDB；Rust/WebCrypto 签名格式必须使用固定测试向量验证。
- 每次 WSS 连接通过 Host 签名和设备签名 challenge-response 完成双向应用认证；TLS 负责临时连接密钥和业务消息保护。
- Host 与设备 wire signature 统一使用 64-byte P1363 `r || s` + 无填充 base64url；签名/HMAC 输入使用协议定义的长度前缀 transcript，不签名普通 JSON。
- 一个 PWA 身份只绑定一个 canonical origin；更换 scheme、hostname 或 port 后必须作为新设备重新配对，不能导出或跨 Origin 迁移私钥。
- PWA 请求 `navigator.storage.persist()`；无论获批与否，浏览器清除数据都按密钥丢失处理。
- 每个新 WSS 连接重新执行 challenge-response，不保存 bearer session/refresh token。
- 浏览器清理站点数据、切换浏览器 profile 或不支持所需密码学能力时，视为设备密钥丢失，需要重新配对。
- PWA 的设备身份强度不得描述为等同于 iOS Keychain 或 Android Keystore。

完整决策和信任边界见 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)。Noise 不进入第一阶段，仅保留为未来不可信中继场景的可选 Transport Profile。

第一阶段浏览器验收覆盖 Android Chrome、iOS Safari 和桌面 Chrome/Edge。HTTPS Provider 必须同时通过 Tailscale Serve 和至少一种非 Tailscale 同机方案。

PWA 不加载第三方脚本、字体或 CDN 资源。CSP 最低基线为 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`，Agent 产生的 Markdown、终端、diff 和工具内容全部按不可信输入渲染。

若可靠的浏览器长期密钥方案尚未完成，PWA 只能运行在显式开发模式。开发模式必须：

- 默认只允许 loopback，扩大监听范围需要显式配置和醒目警告。
- 使用独立的短期开发凭据。
- 不能复用或降级正式配对数据库中的设备身份。
- 不能出现在发布构建的默认配置中。

## 5. 客户端状态模型

连接状态必须使用互斥状态机，而不是相互矛盾的布尔值组合：

```text
unpaired
  -> pairing
  -> disconnected
  -> connecting
  -> authenticating
  -> replaying
  -> online
  -> reconnecting
  -> revoked | incompatible | identity_changed | replaced
```

最低要求：

- 只有进入 `online` 后，客户端才能把命令显示为已被服务器接受。
- `replaying` 阶段只有本节点资源或未来显式允许正文缓存的资源可以展示持久缓存，并必须标记尚未追平；imported resource 默认等待 Owner 回源。
- 未完成 snapshot 写入独立暂存区；`no-content-cache` 资源使用内存暂存，其他允许缓存的资源只有数量和 digest 验证成功后才能原子替换缓存。断线时丢弃未完成 snapshot 并重新请求。
- 连接断开时不能把本地输入伪装成已经发送；第一阶段默认不离线排队 prompt。
- `identity_changed` 必须阻止自动信任新主机密钥。
- `revoked` 必须清理会话密钥和受保护缓存，并要求重新配对。

命令至少具有：

```text
draft -> submitting -> accepted -> completed | failed | rejected | uncertain
```

连接在确认前中断时，客户端先使用同一 `requestId` 重试或执行 `command.status`；只有服务端确认进入 `uncertain` 才显示该终态，不能仅根据网络断开自行断定，也不能生成新 ID 导致重复执行。

第一阶段同一 PWA 设备只允许一条 active WSS。多标签页应协调连接；服务端以新认证连接替换旧连接时，旧页面进入 `replaced` 并停止自动重连，避免标签页互相抢占。切换窗口中的事件仍按 `eventId` 去重。

## 6. ACP 能力的前端呈现

前端同样遵守 ACP 兼容性不变量：

- 结构化 ACP 能力使用结构化 UI，不静默降格为文本。
- 客户端理解公共字段，但保留扩展数据供诊断或未来组件使用。
- 暂时没有专用组件时显示明确的“当前客户端未提供专用视图”，不能直接隐藏事件。
- ACP 原文按 `acp.rawJson` 作为不可信文本保存或诊断展示；不得为了渲染先解析再覆盖原值，尤其不能损坏超过 JavaScript safe integer 的未知数字。
- `unsupported_by_client`、`unsupported_by_broker` 和 `unsupported_by_agent` 必须可区分。
- capability negotiation 的结果进入客户端 feature gate，不能只靠版本号猜测能力。
- 未知事件不得导致整个会话渲染崩溃。

## 7. 本地数据边界

PWA 首个版本只持久化连接和恢复所需的最小数据：

- 已配对主机的非秘密元数据。
- 当前 PWA 身份绑定的 canonical origin。
- 最后确认的 global/session cursor。
- 客户端 UI 偏好。
- 待确认命令的 `requestId` 和最小恢复信息。

规则：

- imported resource 遵守 `no-content-cache`，不得把会话摘要、prompt、回复、工具内容、diff、终端、附件或 ACP raw 写入 IndexedDB、Cache Storage 或其他持久浏览器存储。
- 对当前节点本地拥有的资源，只有节点本地策略显式允许时才缓存精简历史；缓存不是权威数据，重连后以 Daemon 快照和事件日志校正。
- 敏感字段不得因方便调试而进入普通浏览器存储。
- 必须设置大小上限、版本和迁移策略。
- 清除缓存与撤销设备是不同操作；清除设备密钥后必须重新配对。
- `expo-sqlite` 的 Web 支持在采用前必须单独验证；第一阶段可以使用受封装的 IndexedDB adapter，不能让存储实现泄漏到 feature 层。

## 8. 后续原生客户端约束

### 8.1 共享部分

Android/iOS 应复用：

- Sync Protocol 类型与契约测试。
- Sync Client、ACK、重放、幂等和连接状态机。
- 客户端领域模型、feature controller 和大部分页面。
- ACP 能力组件和降级策略。

### 8.2 原生替换部分

原生平台必须提供独立 adapter：

- 长期私钥：生成与 PWA 认证协议兼容的 ECDSA P-256 签名密钥，存入 iOS Keychain/Secure Enclave 或 Android Keystore，优先硬件保护。
- 缓存：原生 SQLite。
- 扫码：原生相机权限与二维码扫描。
- 生命周期：前后台切换、系统终止和恢复。
- 通知：仅在项目明确接受对应平台服务与隐私边界后实现。

不得把 PWA 的 IndexedDB 密钥直接迁移成原生长期身份。原生 App 首次安装视为新设备，需要单独配对和授权。

### 8.3 实施顺序

1. PWA 稳定 Sync Protocol 和核心交互。
2. Android Development Build 验证原生密钥、扫码、本地网络和生命周期。
3. iOS 验证 Keychain、本地网络权限和后台恢复。
4. 再评估平台通知、桌面 GUI 和其他客户端。

原生实现不能反向迫使 Broker 引入平台特例；平台差异应停留在客户端 adapter 或协议 capability 中。

## 9. 第一阶段验收标准

PWA MVP 至少满足：

1. 只能查看和操作当前节点可见的已有会话；PWA v1 无法通过 UI 或构造普通命令创建会话。
2. 能完成配对、认证、订阅、历史追平和实时切换。
3. 网络断开后自动重连，并从最后 ACK cursor 补发，不重复显示事件。
4. prompt 重试不会导致 Agent 重复执行。
5. 能正确显示流式消息、权限请求、工具调用、终端和模型变化的最低结构化视图。
6. 未知 ACP 扩展不会丢失、崩溃或被伪装成普通文本。
7. 慢速渲染或后台标签页不会阻塞 Agent 和其他客户端。
8. 浏览器数据清理后不会继续冒充原设备，而是要求重新配对。
9. 远程 Owner 离线时 imported resource 不展示正文，只展示连接与资源元数据；任何离线输入都不能表示为已发送。
10. PWA 构建可以由 Daemon 本地托管，不需要应用级云服务器。

## 10. 测试边界

- 协议 fixture：版本协商、未知字段、错误和 feature negotiation。
- 状态机：连接、认证、重放、在线、断线、撤销、身份变化。
- 幂等：接受响应丢失、重试、重复事件和 cursor 回退。
- 组件：结构化 ACP 事件、未知事件和能力降级。
- 存储：同步元数据迁移、容量限制、清除和损坏恢复，并验证 imported resource 正文不会落盘。
- 浏览器端到端：Daemon fake/fixture、真实 WebSocket、重连和慢客户端。
- 安全：敏感信息不进入日志、URL、普通存储或错误页面。

PWA 测试通过不代表原生客户端的安全存储、后台行为和系统权限已经验证。

## 11. 暂缓决策

- 前端状态库和表单库的最终选择。
- PWA WebCrypto P-256/IndexedDB 的实际浏览器支持矩阵和验收结果。
- 静态资源嵌入二进制还是作为旁路资源安装。
- 原生端是否需要一个最小 Rust `mobile-security` 库。
- Android/iOS 的最低系统版本。
- 是否以及何时引入系统推送服务。

暂缓决策不能被单个实现补丁静默确定；需要记录验证结果并更新本文。
