# ACP Remote 安全设计

> 状态：编码前安全基线（Draft）  
> 版本：0.3
> 日期：2026-09-18  
> 修订记录（2026-09-18）：§13.4 的可持久化元数据白名单显式加入「不含内容的审计元数据」，消除与 §11.5「保留审计记录」的矛盾。
> 适用范围：Owner/Access Node、Node Link、Daemon、CLI、PWA、后续原生客户端、ACP Agent 子进程及 npm 发布链路

## 1. 文档职责

本文是 ACP Remote 系统级威胁模型、信任边界和安全控制的权威来源。它回答“保护什么、信任谁、防御谁、失败时如何处理”，不重复定义具体 wire 字段。

相关权威来源：

- 产品权限和功能边界：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)
- 模块职责和依赖：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)
- 客户端平台边界：[FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md)
- 认证、游标、命令和事件 wire contract：[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md)
- 节点间 Export/Import、权威、认证和重放：[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md)
- PWA 身份与安全传输决策：[ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)

发生冲突时，已接受 ADR 优先于本文中的候选描述；本文优先于分散在实现注释中的安全假设。改变信任边界、密码算法、密钥生命周期、默认授权或正式部署要求时，必须同步更新本文，必要时新增 ADR。

## 2. 安全目标

### 2.1 必须保证

- 只有已配对、未撤销且具有对应 scope 的设备或节点可以访问被授权资源。
- 远程主体创建会话时只能使用 Owner Node 显式导出的 Agent 与 workspace template，不能提交任意 Owner 路径。
- Access Node 不能扩大 Owner Export grant，也不能取得 Provider/MCP 凭据。
- 节点配对不产生传递信任；第一阶段 imported Agent 不允许再次通过 Node Link 导出。
- 网络路径、Tailnet 成员身份或同一 LAN 不能代替应用身份。
- Node 和 Device identity 不能被 endpoint、TLS 证书或临时连接密钥替代。
- ACP 未知字段和结构化能力得到保真或显式降级，不能被攻击者利用宽松解析绕过安全决策。
- 事件先持久化再广播；命令重试、断线恢复和多端并发不能造成未声明的重复 Agent 副作用。
- Agent、Markdown、终端、diff、文件名、工具输出和 `rawJson` 始终作为不可信内容处理。
- 私钥、Provider credential、pairing secret 和认证 proof 不进入日志、错误页面、普通数据库字段或客户端同步。
- 正式模式不依赖应用级云中继，且不把 Tailscale、反向代理或 Zed 写成安全根。

### 2.2 可用性目标

- 单个慢连接、异常浏览器标签页或超大 Agent 输出不能阻塞 Broker、Agent 或其他设备。
- 存储损坏、磁盘满和进程崩溃必须显式失败，不能广播未持久化事实。
- Owner Node 离线时远程控制不可用是已接受限制，不通过不受控云队列补偿。

### 2.3 不承诺

- 不防御已经取得用户 OS 管理员/root 权限的攻击者。
- 不承诺抵抗能够任意执行当前用户进程代码的本机恶意软件。
- PWA 不承诺达到 Secure Enclave、Keychain 或 Android Keystore 的硬件保护等级。
- 不承诺安全删除 SSD、浏览器存储、备份或文件系统快照中的历史字节。
- 不把 Agent 自身的模型安全性、命令判断或沙箱正确性归因于 Sync Protocol。
- 第一阶段不提供不可信远程 TLS 终止点之后的应用层端到端加密。

这些非目标不能成为忽略最小权限、输入验证、脱敏和安全失败的理由。

## 3. 资产与数据分类

| 分类 | 资产示例 | 最低保护要求 |
|---|---|---|
| `secret` | Node/Device 私钥、Provider token、pairing secret、TLS private key | 不同步、不记录；平台安全存储或明确失败关闭 |
| `sensitive-content` | prompt、回复、终端、diff、文件内容、ACP `rawJson`、附件 | 仅授权设备可读；受保留期、配额和渲染隔离约束 |
| `security-metadata` | node/device public key、Export grant、scopes、撤销时间、Node fingerprint、审计结果 | 完整性优先；只向有权主体暴露必要字段 |
| `operational-metadata` | session title、Agent/model、时间、错误码、sequence | 仍可能泄露项目活动；按授权过滤和日志最小化 |
| `public-artifact` | PWA 静态资源、公开 schema、fixture、公钥算法信息 | 必须有发布完整性，不能包含生产 secret |

处理规则：

- 数据分类跟随内容，不因进入日志、缓存、fixture 或错误对象而降低。
- fixture 只能使用明确标记的公开测试密钥和虚构内容。
- 客户端可见不等于可写；每个 mutation 单独授权。
- 从高等级数据派生的摘要、文件名和路径仍可能敏感，不能默认公开。

## 4. 主体与信任假设

### 4.1 可信主体

- 当前节点 OS 用户及其受保护的用户配置目录。
- ACP Remote 发布并加载的 Daemon/PWA 代码。
- 位于同一节点可信边界内、由用户配置的 TLS Provider。
- 操作系统提供的 DPAPI、Keychain、Secret Service、Keystore 等安全存储接口。

### 4.2 有限信任主体

- 已配对设备：只信任其持有设备私钥，不信任其超出 scopes 的行为。
- 已配对 Access Node：只信任其持有 Node 私钥并遵守 Export grant；不自动信任其本地客户端或下游节点。
- ACP Agent 子进程：信任它按 ACP 交互，但其输出、错误、扩展字段和资源消耗不可信。
- 反向代理：只信任 TLS 终止与转发；其身份 Header 不产生 ACP Remote 权限。
- Tailscale/WireGuard/LAN：只提供网络可达性与外层传输属性，不提供应用授权。
- 浏览器和 Service Worker：在未被 XSS、扩展或本机恶意软件控制的前提下执行客户端逻辑。

### 4.3 不可信输入源

- 未配对网络客户端和任意公网/LAN/Tailnet 主机。
- 普通网站、iframe、跨站脚本和恶意 WebSocket 发起方。
- 所有 Agent、MCP、工具、终端、文件系统和模型产生的内容。
- 用户可编辑的 endpoint、设备名称、会话标题、路径和配置值。
- npm 依赖、构建输入和下载内容，直到被锁定和验证。

## 5. 威胁模型

第一阶段至少防御：

| 威胁 | 主要控制 |
|---|---|
| 未配对设备连接 | WSS + Device challenge-response + scopes |
| 未配对/已撤销节点连接 | WSS + Node challenge-response + Export grant + revocation |
| Tailnet/LAN 内横向访问 | 应用身份独立于网络身份；Daemon 默认 loopback |
| Node 冒充或 endpoint 替换 | QR 固定 Node public key；每次连接验证 Node proof |
| 二维码转发/截获 | 短期单次 secret、Node proof、双方独立 SAS、本机确认 |
| 握手和命令重放 | fresh nonce、connection ID/sequence、`requestId` 幂等 |
| 已撤销设备继续使用 | 撤销检查、立即关闭连接、后续认证失败 |
| CSWSH/CSRF/DNS rebinding | HTTPS、Origin/Host allowlist、同源接口、loopback 管理通道保护 |
| XSS 和 Agent 内容注入 | CSP、无第三方脚本、结构化渲染、HTML 禁用/清洗、`rawJson` 文本化 |
| 越权命令 | 服务端逐命令 scope + 状态 + workspace 检查 |
| Access Node 权限转授过宽 | Owner grant ∩ Access local grant；Owner 最终授权；审计 `viaNodeId` |
| 节点循环和能力洗白 | 第一阶段单 hop；imported Agent 禁止再次导出；来源链检查 |
| 重复 prompt/审批 | durable idempotency、Session Actor、interaction first-writer-wins |
| 慢客户端和内存耗尽 | 有界队列、消息/速率限制、断开后 cursor 重放 |
| 磁盘耗尽和超大输出 | TTL、配额、截断标识、存储失败关闭 |
| 日志或错误泄露 | 数据分类、字段 allowlist、默认脱敏 |
| npm/二进制篡改 | 锁定构建、平台包映射、checksum、provenance、最小发布权限 |

## 6. 信任边界与数据流

```text
Local clients -> Access Node -> Node Link/WSS -> Owner Node -> AgentHost -> ACP Agent
                       │              │                │
                  local grants   Node identity    Export Policy
                                                   + authority log
```

边界规则：

- 每个节点的 TLS Provider 与本节点 Daemon 之间只能走同机 loopback 或等价受保护本地通道。
- `server::sync` 只把已验证 DTO 和认证 Actor 传入 `core::use_cases`，不能直接访问 SQLite 或 Agent stdin。
- `identity-auth` 返回设备/节点身份与授权结果，不向上层泄漏私钥或平台 keystore handle。
- `agent-host` 负责进程与 ACP wire，不根据远程内容自行扩大 workspace 或权限。
- Owner Event Store 是远程资源的事实来源；Access cache 和内存队列都不是权威。

## 7. 网络与部署安全

### 7.1 正式 Profile

- Daemon 默认只监听 `127.0.0.1`/`::1` 或 OS 本地 IPC。
- 正式远程入口只能是 `https://`/`wss://`。
- TLS 必须在 Daemon 或同机可信 Provider 终止。
- 配置必须声明 canonical public origin、允许的 Host 和可信代理地址。
- 除明确配置的同机代理外，不信任 `Forwarded`、`X-Forwarded-*` 或供应商身份 Header。
- 不根据源 IP、`.ts.net` 后缀、Tailnet 用户或客户端证书自动增加 scopes。
- 不能默认无认证监听 `0.0.0.0` 或 `[::]`。

### 7.2 反向代理

反向代理必须：

- 位于同一节点并转发到 loopback。
- 保留 WebSocket message 边界和 `Origin`。
- 不缓存 pairing、认证、Sync API 或敏感响应。
- 限制 Host，拒绝任意 Host 转发和 DNS rebinding。
- 不注入能够改变 ACP Remote Actor/scopes 的身份 Header。

TLS 在其他机器或云服务终止时，默认 Profile 不再提供客户端到 Daemon 的端到端机密性，第一阶段不视为正式支持部署。

### 7.3 开发模式

- 明文 HTTP/WS 只允许显式开发模式和 loopback。
- 使用独立短期开发身份，不复用正式设备数据库和 pairing secret。
- UI、日志和启动输出必须持续显示非安全模式。
- 发布构建不能默认启用，不得为了测试自动扩大监听地址。

## 8. 本地管理入口

Provider/MCP 配置、原始 workspace 路径、Node/Device 管理和 Export 管理属于 Owner Node 本地管理能力。远程 `session.create` 是受限业务能力，只能引用 Owner 预发布的 workspace template。loopback 本身不是充分授权，因为普通网页也可能访问 localhost。

优先顺序（已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定）：

1. CLI 前台进程内调用或 stdio：`daemon start`、离线 `doctor`，以及确认 Daemon 未运行后的 status/stop 回答在此完成；运行中的 status/stop 经下一项本地 IPC 交给组合根（具体生命周期见 `LOCAL_ADMIN_PROTOCOL.md` §7）。
2. 带当前 OS 用户 ACL 的 Windows Named Pipe / Unix domain socket：运行中 Daemon 的设备配对与撤销、节点配对与撤销、Export 管理、Import 管理、审计导出都走这里，两端共用 `core::use_cases` 的同一组 `DeviceManagement`、`ExportManagement`、`RemoteCatalogQueries`。
3. 只有在单独设计本地认证、Origin/CSRF 和权限模型后，才允许 loopback HTTP 管理 API；当前不实现。

本地通道还承载 `acp-remote acp-stdio` 与 Daemon 之间的 ACP 会话流：facade 常驻 Daemon，CLI 侧只是 stdin/stdout 的字节泵，因此这条流是长期、双向、需要背压的，与上面的请求/响应管理载荷分开定义。Daemon 未运行时 `acp-stdio` 以明确错误退出，不得自行打开数据库或启动第二套核心。

约束：

- `acp-facade` 通过 stdio 服务 Zed，不开放远程 TCP 管理面。
- 本地 IPC 只允许启动 Daemon 的 OS 用户访问；授权依据是"调用方是同一 OS 用户"，不新增本地 token，也不复用设备/节点身份。
- 本地管理请求/响应编码由该 adapter 自己拥有，不复用 Sync 或 Node Link DTO。
- Sync/Node Link scope 不能映射到任意原始路径选择、Provider credential 读取或未显式导出的 Agent/MCP 配置。
- IPC endpoint 建立失败（ACL/权限不符、路径被占用、socket 被替换为符号链接）时正式模式拒绝启动，不得降级为无管理通道或临时无认证 endpoint。

## 9. 身份、密钥与配对

### 9.1 密钥职责分离

| 密钥 | 用途 | 生命周期 |
|---|---|---|
| TLS private key | endpoint TLS | 由 Daemon或同机 Provider 管理 |
| Node identity key | Sync `hostProof`（wire 字段名 `hostId`/`hostPublicKey`/`hostProof`）与 Node Link `nodeProof`、稳定节点身份和节点配对 | 长期；丢失后所有设备/节点重新配对 |
| Device identity key | Device proof | 每安装/origin 独立；撤销或数据清理后失效 |
| pairing secret | 首次 claim、状态查询和 SAS | 单次、短期，最迟到原 expiresAt |
| TLS session key | 当前连接机密性/完整性 | 单连接临时，由 TLS 产生 |

Node/Device identity key 不用于业务内容加密，TLS key 不作为长期身份。

一个节点只有一把 Node Identity Key：它同时承担 Sync 的 host 角色（设备配对与每连接 challenge-response）与 Node Link 的节点角色（节点配对与每连接 node proof）。两种用途使用不同 domain tag、不同信任记录类型和不同 transcript 字段集合（见 `SYNC_PROTOCOL.md` §6.3 与 `NODE_LINK_PROTOCOL.md` 的 transcript 小节），密钥本身不复制、不派生第二把；“Node key 与 Device key 用途分离”只约束 Node 与 Device 之间，不要求在 Node 内部再拆分密钥。

### 9.2 Node key 存储

- Windows：优先 CNG/TPM；至少使用绑定当前用户的 DPAPI 保护。
- macOS：Keychain；可用时使用不可导出或硬件保护能力。
- Linux：Secret Service 或经过单独评审的系统 keystore。
- 第一阶段不允许把 Node private key 明文存入 SQLite、普通 TOML/JSON 或 npm 包目录。
- Linux 安全存储不可用时，正式模式必须失败关闭；显式开发模式可以使用进程期临时 key。持久化加密文件 fallback 需要单独 ADR 选择 KDF、解锁和备份策略，不能由单个补丁自行决定。

第一阶段不提供 Node key 导出、云备份或跨节点迁移。Node key 丢失按新 Node 处理。

### 9.3 Device key

- PWA 使用 WebCrypto P-256 不可导出 key，并存入当前 canonical origin 的 IndexedDB。
- PWA key 不宣称硬件保护；浏览器脚本被控制时，攻击者可能调用签名操作。
- Android/iOS 后续使用 Keystore/Keychain adapter，不迁移 PWA key。
- 每个 PWA origin、浏览器 profile 和原生安装是独立设备。

### 9.4 配对和认证

- 精确状态、HMAC、SAS、transcript 和 challenge-response 以 Sync Protocol 为准。
- Device/Node pairing 只能由目标节点本地管理入口创建和确认。
- 配对 UI 必须显示设备名称、key fingerprint、SAS、请求 scopes 和过期时间。
- 用户确认前设备记录不能获得 active 权限。
- 每个新 WSS 完整认证，不签发长期 bearer session/refresh token。
- Node identity 变化进入 `identity_changed`，不能自动接受。

### 9.5 撤销与轮换

- 撤销设备必须提交持久状态后立即关闭其 active connection。
- scope 缩减立即作用于后续命令；高风险缩减应主动关闭连接并要求重新认证，以刷新客户端显示。
- 怀疑 Device key 泄露时撤销该设备并重新配对。
- 怀疑 Node key 泄露时生成新 Node identity、撤销所有设备/节点信任并重新配对；不能用普通 endpoint 更新掩盖 key 变化。
- TLS 证书轮换不改变 Node identity；Node key 轮换不应由 TLS 证书更新自动触发。

### 9.6 Node Link 信任约束

- Node Identity Key 与 Device Identity Key 使用不同 key purpose、数据库 record type 和签名 domain tag，即使算法相同也不得互换。
- Owner Node 只信任已配对 Access Node 代表其自身提交命令，不自动信任 Access Node 的本地用户、设备或其他下游节点。
- Access Node 提供的 `localPrincipalRef` 只用于审计和本地归因，不能绕过 Owner 的 Export grant。
- 第一阶段明确采用节点级信任；Owner 可以按 Access Node 授权、限流和撤销，但不得把 `localPrincipalRef` 描述为 Owner 已直接认证的员工身份或不可抵赖证据。
- 有效授权固定为 `Owner grant ∩ Access local grant ∩ runtime capability`，任一侧撤销或缩减后立即收紧。
- 第一阶段只允许一个 Node Link hop，并对资源来源链进行循环检查。

## 10. 授权模型

### 10.1 Scope 是服务端能力

- UI 隐藏按钮不是授权。
- 每条命令根据当前连接 Actor、最新设备记录、session 状态和 workspace 边界重新检查。
- scope 名称与命令名一致，词表与归类以 `compatibility/commands/v1/commands.json` 为唯一来源；未知 scope 不自动产生权限。
- 设备只能看见其 scopes 允许的 snapshot/event；过滤后的 sequence 空洞不能泄露内容。
- 授权失败不能把目标是否存在泄露给无权设备。

### 10.2 命令、scope、pack 与 grant

授权词汇分四层，同一概念只用一个写法：**命令 scope**（wire 与数据库保存的最小授权单位，等于命令名）、**设备授权包 `pack.*`**（Owner 配对界面上的分组）、**配对预设 `preset.*`**（一组 pack 的默认值）、**跨节点导出授权 `grant.*`**（Owner 授予 Access Node 的能力上限）。完整清单与机器可读定义在 `compatibility/commands/v1/commands.json`；下表是该文件的权威展开。

| 命令 | 类别 | 所需 scope | 所属 pack | 对应 grant | 首阶段 |
|---|---|---|---|---|---|
| `session.list` | query | `session.list` | `pack.observe` | `grant.observe` | mvp |
| `session.read` | query | `session.read` | `pack.observe` | `grant.observe` | mvp |
| `command.status` | query | `command.status` | `pack.observe` | `grant.observe` | mvp |
| `session.mode.list` | query | `session.mode.list` | `pack.observe` | `grant.observe` | conditional_mvp |
| `session.config.list` | query | `session.config.list` | `pack.observe` | `grant.observe` | conditional_mvp |
| `session.prompt` | mutation | `session.prompt` | `pack.interact` | `grant.interact` | mvp |
| `session.cancel` | mutation | `session.cancel` | `pack.interact` | `grant.interact` | mvp |
| `elicitation.respond` | mutation | `elicitation.respond` | `pack.interact` | `grant.interact` | conditional_mvp |
| `session.mode.set` | mutation | `session.mode.set` | `pack.configure-session` | `grant.configure-session` | conditional_mvp |
| `session.config.set` | mutation | `session.config.set` | `pack.configure-session` | `grant.configure-session` | conditional_mvp |
| `permission.resolve` | mutation | `permission.resolve` | `pack.approve` | `grant.approve` | mvp |
| `session.create` | mutation | `session.create` | 无（仅 Node Link） | `grant.remote-work` | mvp（Node Link）/ Sync 首版不暴露 |

- 查询类命令（`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list`）全部归 `pack.observe`，因此断线恢复所需的 `command.status` 不需要额外授权。
- `session.create` 是设备/Access principal 的 scope，同时要求 Owner 侧 `grant.remote-work`；请求只能引用 Export 中发布的 Agent 与 workspace template，不能提交任意 Owner 路径或 Provider/MCP 凭据。Node Link 首个纵向切片必须实现它以支持 Zed `session/new`；Sync 首版不暴露该入口。
- 配对预设：`preset.remote-control` = `pack.observe` + `pack.interact` + `pack.configure-session` + `pack.approve`；`preset.read-only` = `pack.observe`。配对确认页必须完整展示最终 scopes，不能用含糊的“完全访问”替代。
- 设备记录与 wire 只保存独立 scopes，不保存 pack 或 preset 名称；`pack.*`、`preset.*`、`grant.*` 都是授权管理的输入形式，落到 wire 前必须展开。
- `pack.approve` 单独分组是为了让风险在配对和设备管理 UI 中清晰可见，不是为了削弱远程控制；跨节点的 `grant.interact` 不包含审批，审批必须显式授予 `grant.approve`。远程客户端可以选择 Agent 当前 permission request 明确提供、且 Owner 本地策略允许的任一 option；如果 Agent 明确说明某个 option 会形成持久授权，客户端必须展示该持续范围。客户端不能伪造新 option，也不能在请求之外修改 Owner 的全局沙箱或权限策略。

### 10.3 本地管理能力（永不远程授予）

以下能力没有 scope，只能由 Owner Node 本地管理入口（CLI、用户 ACL 保护的本地 IPC，或 §8 定义的受保护通道）执行：

```text
local.workspace.select     选择并绑定原始 workspace 路径
local.agent.configure      Agent 启动 profile 与环境变量白名单
local.provider.configure   Provider/MCP 凭据
local.device.manage        设备与节点信任管理
local.export.manage        Export 定义与撤销
local.node.rotate-key      Node Identity 轮换
local.audit.export         本地审计导出（不含会话正文）
```

远程 `session.create` 是唯一与 workspace 相关的受限远程能力，且只能引用 Export 发布的 alias/template。未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`，不能只增加一个 command schema。

### 10.4 并发决策

- 同一 session mutation 由 Session Actor 串行化。
- permission/elicitation 使用 interaction ID 和版本，first valid writer wins。
- 后续设备得到 `interaction.already_resolved`，不能覆盖第一个决策。
- config option 与 mode 切换使用 `expectedVersion`。
- `requestId` 重试不能产生第二次接受或 Agent 派发；无法确认外部副作用时进入 `uncertain`。

## 11. PWA 与浏览器安全

### 11.1 脚本和资源

- PWA 不加载第三方脚本、字体、analytics 或 CDN 资源。
- CSP 最低基线沿用 ADR-0001；新增资源类型必须显式放行。
- 禁止 `unsafe-eval`，不使用动态远程 module。
- 可用时启用 Trusted Types；不支持的平台仍必须通过安全组件和 sanitizer 保证相同内容边界。
- production source map 不得公开包含本地路径、secret 或未发布源信息；是否发布独立调试符号由发布流程决定。

### 11.2 不可信内容渲染

- Markdown 默认禁用原始 HTML；链接只允许明确 scheme allowlist。
- 不自动加载 Agent 返回的远程图片、iframe、音视频或 tracking URL。
- 终端使用纯文本/受控 ANSI renderer，不把输出写入 `innerHTML`。
- diff、路径、工具参数和错误栈使用文本节点或经过测试的结构化组件。
- ACP `rawJson` 只能作为文本查看、复制或受控下载，不能执行、注入 DOM 或作为授权输入。
- sanitizer 配置、Markdown renderer 和链接策略必须有恶意 fixture。

### 11.3 Service Worker 与缓存

- Service Worker 只缓存带版本的静态应用资源；imported resource 的会话正文不得进入 Cache Storage。
- 不缓存 pairing URL/fragment、pairing/status 响应、认证消息、WSS 数据或 Provider credential。
- 新版本必须原子激活；旧页面和新 Daemon 协议不兼容时进入 `incompatible`，不能降级认证。
- Cache key 不得包含 secret；清理站点数据按 Device key 丢失处理。

### 11.4 浏览器存储

- Device private `CryptoKey` 只在 IndexedDB 中以不可导出形式保存。
- imported resource 的会话正文、ACP raw、终端和 diff 不得持久化；对 Owner 本地资源未来允许的缓存仍按敏感内容设置容量、版本和清理策略。
- `localStorage` 不保存私钥、pairing secret、完整 prompt 或可恢复认证材料。
- `navigator.storage.persist()` 被拒绝不降低认证，数据被清理后要求重新配对。

## 12. Daemon、Agent 与 Workspace

### 12.1 Daemon

- 配置、数据和日志目录归当前 OS 用户所有；拒绝不安全权限或符号链接替换。
- 单实例锁和 IPC endpoint 必须防止其他本机用户抢占。
- 配置解析使用明确 schema、路径规范化和安全默认值。
- 关闭时按顺序停止接入、取消任务、关闭 Agent、刷新存储并清理进程树。

### 12.2 Agent 子进程

- 使用参数数组启动，不经 shell 拼接用户输入。
- 只传递 Agent 启动所需环境变量；不把 ACP Remote Node/Device key 注入子进程。
- stdout 只作为 ACP wire；stderr 作为受限、脱敏的 Agent 日志。
- 设置启动、请求、空闲和关闭超时，以及 stdout/stderr/内存可承受上限。
- Windows 使用 Job Object 或等价机制清理完整子进程树。
- Agent 崩溃、乱序或非法 JSON 形成明确事件，不能使 Daemon 接受伪造客户端身份。

### 12.3 Workspace

- 原始 workspace 路径只能由 Owner Node 本地管理入口选择。
- 远程主体只能选择 Export 发布的 workspace alias/template，不能构造任意绝对路径、追加目录或修改 Agent 沙箱。
- 文件路径展示和下载必须重新检查其属于会话授权边界；不能只依赖客户端传来的 path。
- 规范化路径后再比较，处理 symlink、junction、大小写、UNC 和 `..`。
- Agent 的实际文件/命令权限仍由 Agent sandbox 和用户配置控制；ACP Remote 不能虚报更严格隔离。

## 13. 持久化与本地数据保护

### 13.1 第一阶段静态数据决策

第一阶段不自行实现 SQLite 全库加密，也不引入自定义透明加密层：

- 依赖 OS 用户账户隔离、目录 ACL 和用户启用的 BitLocker/FileVault/LUKS 等磁盘保护。
- 文档和 CLI 应建议在存放敏感工程的电脑上启用全盘加密。
- Provider credential、Node private key 和 pairing secret 不进入普通 SQLite 字段。
- 若用户威胁模型要求防御离线磁盘读取，必须启用 OS 磁盘加密；未启用时这是明确剩余风险。
- 将来采用 SQLCipher 或字段加密需要独立 ADR，说明 key 来源、迁移、备份、崩溃恢复和平台发布成本。

### 13.2 文件权限

- 数据库、WAL、附件、配置和日志目录只允许当前 OS 用户访问。
- Unix 创建权限目标为目录 `0700`、敏感文件 `0600`；Windows 使用当前用户专属 ACL，不依赖只读属性。
- 启动时检查明显宽松权限并在正式模式失败或给出不可忽略的安全错误。
- 临时文件创建在受保护目录，使用原子创建和重命名，不能使用可预测的共享临时路径保存敏感内容。
- 备份或拷贝数据库时必须连同主库、`-wal`、`-shm` 三个文件（或先做一次 checkpoint），否则副本可能陈旧或损坏；操作说明见 `CONFIG_REFERENCE.md` §5.1「备份与拷贝」。

### 13.3 保留、压缩和删除

- sequence event 必须先持久化再广播。
- `persist_deltas: false` 只允许 turn 完成后压缩/清理短期 delta。
- 清理必须尊重 ACK、TTL、审计和 ACP 语义保真；过期 cursor 走 snapshot reset。
- 大型终端、diff、附件和 raw payload 使用明确配额及 `truncated/rawUnavailable` 标识。
- 删除是逻辑/文件系统删除，不宣称擦除 SSD、备份或快照。

### 13.4 客户端与 Access Node 数据最小化

- 第一阶段 Export 固定为 `no-content-cache`：Access Node 和受其服务的远程客户端默认不持久化会话正文，只允许有界内存转发。
- Access Node 可以把 imported 资源交付给它自己的 Sync 客户端（浏览器 PWA），但只允许按 `SYNC_PROTOCOL.md` 的 `resource.remote-origin.v1` 转发：不改写 ACP 语义与 `acp.rawJson`、不缓存正文、不扩张 capability、不改会话状态；快照只含元数据，正文历史一律在线回源 Owner，离线时返回 `resource.remote_unavailable` 并置 `origin.online=false`。
- Access Node 可持久化 import、owner/origin、cursor/ACK、requestId、命令终态引用、event type/digest、local sequence 映射，以及**不含内容的审计元数据**（`SECURITY_DESIGN.md` §14.2 的动作类别、时间、actor、目标 ID 与摘要）；上述元数据一律不得包含可还原 prompt、回复、diff、终端、附件或 ACP raw 的内容。
- 上述约束可由 ACP Remote 和项目自带客户端执行，但无法约束 Zed 或其他第三方 ACP Client 的历史、日志和崩溃转储；向第三方客户端交付正文必须被视为 Export 授权的数据披露，并在管理界面明确提示。
- 撤销设备或 Export 后 Owner 不再提供数据，Access 删除上述索引并清空内存内容。
- 客户端收到自己被撤销或用户执行“清除此设备”时，应删除缓存和 device key；浏览器能力不足时明确说明残留风险。
- 未来启用加密离线正文缓存时，必须新增协商 feature、Owner 明示授权、容量/TTL/撤销提示和独立 ADR；Owner 仍不得声称能远程可靠擦除已有副本。

## 14. 日志、审计与崩溃信息

### 14.1 默认日志允许字段

- 稳定但非秘密的 correlation ID、event ID、request ID。
- 事件/命令类型、状态、耗时、大小和错误码。
- 截断并脱敏的 Agent executable 名称，不记录完整参数中的 secret。

默认禁止：

- 完整 prompt、回复、ACP `rawJson`、终端、diff 和文件内容。
- Node/Device private key、pairing secret、HMAC、签名输入、Provider token。
- 完整敏感路径、URL fragment、Authorization/Cookie 和 QR payload。

### 14.2 审计事件

至少记录不含内容的安全审计元数据：

```text
pairing.created / claimed / approved / rejected / expired
device.authenticated / auth_failed / revoked / scopes_changed
node.paired / node.trust_revoked / node.identity_changed
authorization.denied
rate_limit.triggered
storage.integrity_failed
```

审计日志不能成为第二份聊天记录。失败原因对本地日志可以比远程错误更详细，但仍不得包含 secret。

### 14.3 崩溃

- panic/error report 不包含敏感 DTO 的 `Debug` 全量输出。
- release 构建不得自动上传 crash dump 或 telemetry。
- OS crash dump 可能包含内存敏感数据，这是已知风险；文档应说明如何按平台禁用或保护 dump。

## 15. 资源滥用与失败安全

- 在分配大对象前应用 Sync Protocol 的认证前、单消息和数组/深度限制。
- 按 IP、device、command 和 pairing ID 限流；限流状态有界且会过期。
- 每连接发送队列、每 session command queue、Agent 输出、数据库和附件都有独立上限。
- 慢客户端断开重放，不能向 Broker 传播无界背压。
- 磁盘写入失败时，不广播对应事件；命令返回明确失败或 `uncertain`。
- 数据库完整性异常进入只读/停止服务等失败安全状态，不能从内存继续假装成功。
- 密码学 RNG、keystore 或 TLS 初始化失败时正式模式拒绝启动。
- 解析失败、未知必需 feature 和不支持 ACP 能力显式报错，不回退成宽松文本命令。

## 16. 发布与供应链

### 16.1 npm 分发

- npm 主包只选择并调用与当前 OS/architecture 精确匹配的平台包。
- 平台包直接携带预编译 Rust binary 和 PWA 静态资源；安装脚本不得从任意 URL 再下载可执行文件。
- 主包版本、平台包版本、binary protocol/build version 必须一致，不允许模糊 fallback 到其他平台 binary。
- 发布 manifest 记录每个 binary 和静态资源的 SHA-256。
- npm registry integrity 是基础校验；正式发布流程还应启用 registry provenance/可信发布能力。

### 16.2 构建与发布权限

- CI 使用固定 action/toolchain major 或 digest、锁定 Rust/npm 依赖并保存审核记录。
- 发布凭据使用短期 OIDC/可信发布，避免长期 npm token 存入开发机或仓库 secret。
- 发布 job 与普通 PR CI 分离，需要受保护 tag/branch 和人工或策略批准。
- 不从 fork PR、未受信脚本或未验证 artifact 直接发布。
- 生成 SBOM、依赖许可证清单和平台 checksum；安全修复可以定位受影响版本。

### 16.3 运行时更新

- 第一阶段不实现 Daemon 自更新器，不在运行时下载并执行远程 binary。
- PWA 静态资源与 Daemon 同包发布，构建版本可检查。
- 协议不兼容时明确失败，不能通过关闭认证或加载远程旧脚本解决。

## 17. 安全事件处理

| 场景 | 响应 |
|---|---|
| 客户端设备丢失 | Owner/Access Node 执行 `device revoke`，关闭连接；必要时检查审计记录 |
| Device key 怀疑泄露 | 撤销该 device ID，清除 scopes，重新配对为新设备 |
| Node key 丢失 | 生成新 Node identity，所有设备和节点重新配对 |
| Node key 怀疑泄露 | 停止远程入口、轮换 Node key、撤销全部设备/节点信任、检查本地系统 |
| TLS key/证书泄露 | 轮换证书和 TLS key；Node identity 保持不变但检查是否存在中间人窗口 |
| PWA XSS/静态资源篡改 | 停止服务、修复并发布新资源、轮换受影响设备身份，审查 Node/设备操作 |
| SQLite/附件泄露 | 视为聊天和工程敏感内容泄露；撤销设备不能补救已复制数据 |
| npm 包被篡改 | 撤回版本、发布安全公告和已验证新版本、轮换发布权限 |

事件响应命令和 UI 必须优先提供 `device/node list/revoke`、Node fingerprint 查看和本地审计导出；审计导出默认不包含聊天内容。

## 18. 安全验证计划

### 18.1 自动化

- Sync schema 正反 fixture、重复键、尺寸、深度和恶意 JSON。
- transcript/HMAC/P1363 Rust-WebCrypto 互操作与错误向量（Rust 侧已在 `p256` 上对 12 个固定向量与 20 个畸形输入验证通过，见 `INITIAL_DESIGN.md` §16 第 6 条；实现阶段必须把同一批向量固化成 Rust 测试，而不是依赖一次性验证）。
- pairing 并发 claim、过期、重放、SAS、拒绝和 secret 清理。
- Node mismatch、Origin mismatch、错误 device/node、撤销和 scope 缩减。
- Export grant 交集、Access Node 越权、循环导出和 Node Link 重放。
- `requestId` 重试、payload 冲突、崩溃窗口和 `uncertain`。
- snapshot barrier、ACK 越界、sequence 回退和慢客户端。
- XSS fixture：Markdown HTML、恶意链接、SVG、ANSI、diff、路径和 `rawJson`。
- 路径穿越、symlink/junction、workspace 越界和 shell argument 注入。
- 日志/错误/snapshot/fixture 中的 secret 扫描。
- npm 平台选择、checksum、版本错配和禁止运行时下载。

### 18.2 平台验收

按 `INITIAL_DESIGN.md` §14 的交付范围分批执行：当前 Windows 节点必须完成密钥保护、Named Pipe 对端身份/ACL、数据目录权限、Job Object 与休眠恢复验收；Linux/macOS 节点验收在其后续交付前完成。已有 Linux 单元测试不能替代 Windows 平台证据。下列浏览器检查属于 PWA 阶段，不要求节点运行在同一种 OS。

- Windows/macOS/Linux Node key 存储与数据目录权限。
- Windows Job Object 和 Unix 进程组清理。
- Android Chrome、iOS Safari、桌面 Chrome/Edge 的 WebCrypto、IndexedDB、Origin 和 Service Worker。
- Tailscale Serve 与至少一种非 Tailscale 同机 HTTPS Provider。
- 休眠、后台挂起、网络切换、浏览器数据清理和磁盘满。

### 18.3 发布门禁

正式发布前至少要求：

```text
format + lint + unit/integration tests
schema/fixture contract tests
dependency and license review
secret scan
platform binary checksum
PWA CSP/XSS tests
manual pairing/revoke smoke test
```

## 19. 已接受的第一阶段剩余风险

| 风险 | 接受原因 | 缓解 |
|---|---|---|
| SQLite 不做应用层加密 | 避免自研密钥与迁移系统 | OS ACL + 建议全盘加密 + secret 分离 |
| PWA key 不保证硬件保护 | 浏览器跨平台限制 | 不可导出 WebCrypto、CSP、撤销和重新配对 |
| 同机 TLS Provider 在可信边界 | PWA 必须有可信 HTTPS | Provider 可替换、仅 loopback、Node proof 独立 |
| 本机管理员/恶意软件可读进程数据 | 不属于应用可可靠防御范围 | 最小权限、keystore、少日志、短期 secret |
| 无云时 Owner Node 离线不可用 | 产品明确选择 local-first | 默认不展示 imported 会话正文，明确离线状态 |
| 内存或未来显式缓存无法保证远程擦除 | Web/PWA/分布式系统限制 | 默认 `no-content-cache`、断线/撤销清内存；未来缓存需单独授权和提示 |
| 超大 ACP 原文可能不下发 | 有界资源要求 | Daemon 保留策略 + `rawUnavailable` 显式降级 |

## 20. 编码前仍需收口的实现选择

以下选择不能由普通实现补丁静默决定：

- Linux（没有可用的 D-Bus Secret Service，例如无桌面会话或容器）上是否提供经过审计的持久化 fallback，推迟到 Linux 平台开发阶段处理；当前优先交付 Windows，见 `INITIAL_DESIGN.md` §14。在决定前 Linux 正式模式仍失败关闭，不引入明文 fallback。该决定只影响 `identity-keystore`：`identity-auth` 的 keystore 端口必须允许非硬件保护的实现存在，但默认不启用（[ADR-0006](./adr/0006-identity-keystore-split.md) 决策 5）。
- npm provenance、checksum 签名和 SBOM 的发布工作流与格式仍待发布阶段确定。普通 CI 已接入 GitHub Actions（`.github/workflows/ci.yml`），执行合同门禁、Rust 检查及提交规范校验；已有普通 CI 不代表发布 provenance、签名或 SBOM 已实现。发布 job 继续按 §16.2 与普通 CI 分离。
- release crash dump 的平台默认策略。
- 是否以及何时通过 ADR 引入 SQLCipher、字段加密或 Noise Transport Profile。

本地管理通道已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定（stdio + 平台本地 IPC，不实现 loopback HTTP 管理面）。

这些选择不阻塞领域模型和 Sync DTO 开发，但相关平台 adapter 或正式发布在选择落定前不能声称安全完成。
