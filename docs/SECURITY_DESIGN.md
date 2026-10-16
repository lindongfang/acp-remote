# ACP Remote 安全设计

> 状态：编码前安全基线（Draft）  
> 版本：0.5
> 日期：2026-10-16  
> 修订记录（2026-10-16，`pairing-model`）：配对只保留两条密码学校验（用配对载荷里的主机公钥验证认领响应的 `hostProof`、已批准响应的主机公钥与载荷逐字节一致），**短验证码（SAS）两套全部删除**——§5 新增 §5.1 说明「为什么本协议不需要短验证码」，§9.3 的 pairing secret 用途、§9.4 的确认页展示项与 §18.1 的测试项随之收窄；**设备权限层删除**——§2.1/§2.2、§3 的 `security-metadata` 行、§4.2 的设备条目、§5 的威胁控制、§7.1/§7.2、§8、§9.4、§9.5、§10.1、§10.2 与 §12.3 改为「通过设备认证即可执行业务命令，设备不再有授权维度」，§10.2 的表格去掉「所属 pack」列、`pack.*`/`preset.*` 两类词汇删除；**保留**设备认证与每连接 challenge-response、配对确认只能由本机用户完成、撤销立即关闭连接、速率与各项上限、跨节点 `grant.*`/Export `scopes`/Import grants。威胁模型与信任边界没有放宽——删 SAS 的前提是 §5.1 写明的两条密码学要求已由 `pwa-web-client` 承担。  
> 修订记录（2026-10-16，`ui-csp-inline-styles`）：§11.1 的 `[实现现状]` 按实绩改写——前端内联样式形态已移除（入口文档把 reset 外置为同源 `/entry-reset.css`，并以**必须保持为空**的同源 `/rnw-stylesheet.css` 承载 `react-native-web` 的运行时样式表），样式改由**同源外部样式表**交付，CSP 取值**仍未放宽**（`insertRule` 不是 CSP 的检查点，既不得写成「阻止了样式注入」，也不得写成「放宽了基线」）。威胁模型与信任边界**未变**。  
> 修订记录（2026-10-15，`sync-ui-hosting` WP4）：§11.1 加 `[实现现状]`——静态面的 CSP **由响应头下发**（取值逐字等于 `FRONTEND_DESIGN.md` §4.5 的基线）并带四个与配对端点同取值的头，基线未被放宽（产物里的内联样式仍被拒）；§13.4 登记同包发布的静态资源属 `public-artifact` 类别（静态面只交付公开构建产物）；§16.3 写回「嵌入二进制 + 同包发布 + `daemon.status.uiBuildVersion` 可检查」与发布构建缺产物即拒绝启动。威胁模型与信任边界**未变**。  
> 修订记录（2026-10-14，`sync-server-connection` WP7）：§9.4 与 §9.5 各加一段 `[实现现状]`——§9.4 写回 Sync 连接面的 Origin 校验实绩（来源三形态不折叠、逐字比较、`Absent` 与非 `https` 一律拒绝、`HostPolicy` 的已知偏差）与配对/认证只覆盖到「认证完成」的边界（业务命令级授权未接线）；§9.5 写回设备撤销传播实绩（`4410` 持久提交后经组合根 `ConnectionCloser` 关闭真实连接注册表、拒绝后续认证）与仍未落地项（Access 出站重连、设备侧 scope 缩减关连接）。威胁模型与信任边界**未变**。  
> 修订记录（2026-10-02）：§9.4 新增「配对授予的 scope 是设备级而非资源快照」的显式规则，并写出 `session.create` 的设备级副作用；§10.2 把 `session.create` 归入新设备授权包 `pack.create-session`（不进任何预设），声明 Sync 面已暴露该入口与其 `{ workspaceAlias, agentId }` payload；§10.3 区分两个传输面的 workspace 引用边界；§11.2 把目录展示名列为按不可信内容渲染。  
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

- 只有已配对、未撤销的设备或节点可以访问本节点资源：设备通过认证即可执行本节点的全部业务命令（设备方向不再有授权维度，见 §10），节点仍受 Owner Export grant 约束。
- 跨节点远程主体创建会话时只能使用 Owner Node 显式导出的 Agent 与 workspace template，不能提交任意 Owner 路径（Node Link 面）；本机自己的设备在 Sync 面可以提交会话目录，但受 §12.3 的绝对路径、存在性与规范化校验约束。
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
| `security-metadata` | node/device public key、Export grant、节点信任记录与 Export 的授权集合（`grant.*` 展开值）、撤销时间、Node fingerprint、审计结果 | 完整性优先；只向有权主体暴露必要字段 |
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

- 已配对设备：只信任其持有设备私钥与「本机用户已确认这台设备」这一事实。设备方向不再有授权维度（没有可越出的 scope 集合）：认证通过即可执行本节点全部业务命令，边界由本机确认与撤销（而非 scope）决定，见 §10.1。
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
| 未配对设备连接 | WSS + Device challenge-response（认证即业务命令的执行前提） |
| 未配对/已撤销节点连接 | WSS + Node challenge-response + Export grant + revocation |
| Tailnet/LAN 内横向访问 | 应用身份独立于网络身份；Daemon 默认 loopback |
| Node 冒充或 endpoint 替换 | QR 固定 Node public key；每次连接验证 Node proof |
| 二维码转发/截获 | 短期单次 secret、配对载荷带外携带主机公钥、认领 HMAC、Host proof、本机确认（§5.1） |
| 握手和命令重放 | fresh nonce、connection ID/sequence、`requestId` 幂等 |
| 已撤销设备继续使用 | 撤销检查、立即关闭连接、后续认证失败 |
| CSWSH/CSRF/DNS rebinding | HTTPS、Origin/Host allowlist、同源接口、loopback 管理通道保护 |
| XSS 和 Agent 内容注入 | CSP、无第三方脚本、结构化渲染、HTML 禁用/清洗、`rawJson` 文本化 |
| 越权命令 | 服务端逐命令校验「已通过设备认证 + 会话状态 + workspace 边界」；跨节点命令另按 Owner Export grant ∩ 本地授权判定 |
| Access Node 权限转授过宽 | Owner grant ∩ Access local grant；Owner 最终授权；审计 `viaNodeId` |
| 节点循环和能力洗白 | 第一阶段单 hop；imported Agent 禁止再次导出；来源链检查 |
| 重复 prompt/审批 | durable idempotency、Session Actor、interaction first-writer-wins |
| 慢客户端和内存耗尽 | 有界队列、消息/速率限制、断开后 cursor 重放 |
| 磁盘耗尽和超大输出 | TTL、配额、截断标识、存储失败关闭 |
| 日志或错误泄露 | 数据分类、字段 allowlist、默认脱敏 |
| npm/二进制篡改 | 锁定构建、平台包映射、checksum、provenance、最小发布权限 |

### 5.1 `[决定]` 为什么本协议不需要短验证码（SAS）

**决策句**：ACP Remote 的配对与认证**不派生、不在 wire 上返回、不展示也不校验短验证码**。信任锚是「配对载荷**带外**携带主机公钥与一次性配对秘密 + 认领阶段以配对秘密计算的 HMAC」——教科书里 SAS 所防的公钥替换已经由这条锚承担；而对本流程里剩下的两种现实威胁（配对载荷在带外被整条替换、配对秘密泄露），SAS **完全无效**。设备侧的主机信任因此由两条密码学要求确立，不需要人眼比对。

理由：

- SAS 在教科书里的职责是「公钥经不可信通道交换时被替换」的带外校验。本协议的二维码/URL **带外**携带 `hostId` 与主机公钥，且认领与状态请求都必须用一次性配对秘密算出正确的 HMAC 才会被受理（`SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2），因此攻击者即使能改写通道，也无法在不掌握配对秘密的前提下让设备接受另一把主机公钥。这条锚的强度不依赖人眼比数字。
- SAS 能核对的只是「载荷里那把公钥」，**不是**「设备最终落盘的那把」。设备此后长期使用的主机公钥必须靠两条设备侧校验确立：用配对载荷里的主机公钥验证认领响应的 `hostProof`，以及已批准响应里的主机公钥与载荷**逐字节相同**（状态响应不含任何主机签名，只能靠比对）。两条都失败关闭、都不需要人参与，要求见能力规格 `pwa-web-client`。
- 对剩下的两种现实威胁，SAS 无效：① 配对载荷在带外被**整条替换**——真主机那次配对从未被认领，真主机终端上根本没有可比对的数字，用户比的是攻击者主机算出的数字；② 配对秘密泄露——持有者能算出同一个数字。
- 删除 SAS 不降低任何已承诺的安全属性：它剩下的作用只是「不依赖代码正确性的人眼检查点」，属纵深而非安全依据；保留它反而会让「已经比对过」的错觉掩盖第 2 条那个真正的缺口。

**明确保留**的仪式与边界（不因本条放宽）：配对确认仍必须由**本机用户**在电脑上完成（发起配对 + 按一次 y）；设备认证仍是每连接 challenge-response；设备撤销仍立即关闭活动连接；速率与各项上限不变。删除 SAS 的实施范围（两套 transcript domain 与固定向量、配对状态字段、本地管理方法字段、CLI 参数与展示项）见 `SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §9.2/§9.4 与 `LOCAL_ADMIN_PROTOCOL.md`。

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
- 不根据源 IP、`.ts.net` 后缀、Tailnet 用户或客户端证书自动增加授权（设备方向没有可增加的授权维度，节点方向只能是 Owner 显式授予的 grant）。
- 不能默认无认证监听 `0.0.0.0` 或 `[::]`。

### 7.2 反向代理

反向代理必须：

- 位于同一节点并转发到 loopback。
- 保留 WebSocket message 边界和 `Origin`。
- 不缓存 pairing、认证、Sync API 或敏感响应。
- 限制 Host，拒绝任意 Host 转发和 DNS rebinding。
- 不注入能够改变 ACP Remote Actor 或授权判定的身份 Header。

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
- 设备认证与 Export grant 都不能映射到 Provider credential 读取或未显式导出的 Agent/MCP 配置；Sync 面的设备可以提交会话目录（受 §12.3 的路径校验），Node Link 面仍不得让请求携带任意 Owner 路径。
- IPC endpoint 建立失败（ACL/权限不符、路径被占用、socket 被替换为符号链接）时正式模式拒绝启动，不得降级为无管理通道或临时无认证 endpoint。

## 9. 身份、密钥与配对

### 9.1 密钥职责分离

| 密钥 | 用途 | 生命周期 |
|---|---|---|
| TLS private key | endpoint TLS | 由 Daemon或同机 Provider 管理 |
| Node identity key | Sync `hostProof`（wire 字段名 `hostId`/`hostPublicKey`/`hostProof`）与 Node Link `nodeProof`、稳定节点身份和节点配对 | 长期；丢失后所有设备/节点重新配对 |
| Device identity key | Device proof | 每安装/origin 独立；撤销或数据清理后失效 |
| pairing secret | 首次 claim 与状态查询（配对认领 HMAC、状态 HMAC） | 单次、短期，最迟到原 expiresAt |
| TLS session key | 当前连接机密性/完整性 | 单连接临时，由 TLS 产生 |

Node/Device identity key 不用于业务内容加密，TLS key 不作为长期身份。

一个节点只有一把 Node Identity Key：它同时承担 Sync 的 host 角色（设备配对与每连接 challenge-response）与 Node Link 的节点角色（节点配对与每连接 node proof）。两种用途使用不同 domain tag、不同信任记录类型和不同 transcript 字段集合（见 `SYNC_PROTOCOL.md` §6.3 与 `NODE_LINK_PROTOCOL.md` 的 transcript 小节），密钥本身不复制、不派生第二把；“Node key 与 Device key 用途分离”只约束 Node 与 Device 之间，不要求在 Node 内部再拆分密钥。

### 9.2 Node key 存储

- Windows：优先 CNG/TPM；至少使用绑定当前用户的 DPAPI 保护。第一阶段实际交付的是下限（DPAPI 绑定当前用户），档位与理由见下方 `[决定]`。
- macOS：Keychain；可用时使用不可导出或硬件保护能力。
- Linux：Secret Service 或经过单独评审的系统 keystore。
- 第一阶段不允许把 Node private key 明文存入 SQLite、普通 TOML/JSON 或 npm 包目录。
- Linux 安全存储不可用时，正式模式必须失败关闭；显式开发模式可以使用进程期临时 key。持久化加密文件 fallback 需要单独 ADR 选择 KDF、解锁和备份策略，不能由单个补丁自行决定。

第一阶段不提供 Node key 导出、云备份或跨节点迁移。Node key 丢失按新 Node 处理。

`[决定]`（2026-09-23）**第一阶段的 Windows 档位已定案**：用 **DPAPI（当前用户 scope）包裹私钥字节**，签名在进程内由 `p256` 完成。依据：

- §9.2 已把「至少 DPAPI 绑定当前用户」列为 Windows 的基线下限，因此这不是 [ADR-0006](./adr/0006-identity-keystore-split.md) 决策 5 所说的「降级实现启用」——它不需要新 ADR；
- workspace 固定 `unsafe_code = "forbid"`（`Cargo.toml`），而 DPAPI/CNG 都要过 FFI：选择 DPAPI 包裹可用**安全 wrapper crate** 实现，CNG 不可导出密钥则做不到（见 `MODULE_ARCHITECTURE.md` §4.12）；
- 代价必须写清：签名瞬间私钥存在于进程内存，不具备「密钥不出 OS」的性质。CNG/TPM 不可导出档位作为开放项列在 §20。

`[决定]` 同一档位适用于 Daemon 侧需要长期保护的一切凭据：第一阶段是 **Node identity key** 与 **Provider 凭据**（§13.1）。PWA 与将来原生客户端的 Device key 由客户端平台自己持有（§9.3 的 WebCrypto IndexedDB / Android Keystore / iOS Keychain），**不**经本档位。两者都只经 `identity-keystore` 的端口进出，不得绕过端口落到 SQLite 或配置文件。

### 9.3 Device key

- PWA 使用 WebCrypto P-256 不可导出 key，并存入当前 canonical origin 的 IndexedDB。
- PWA key 不宣称硬件保护；浏览器脚本被控制时，攻击者可能调用签名操作。
- Android/iOS 后续使用 Keystore/Keychain adapter，不迁移 PWA key。
- 每个 PWA origin、浏览器 profile 和原生安装是独立设备。

### 9.4 配对和认证

- 精确状态、HMAC、transcript 和 challenge-response 以 Sync Protocol 为准（节点侧以 Node Link 为准）；`identity-auth` 一侧的状态机边界、握手入口、nonce/重放与授权展开见 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md)。
- Device/Node pairing 只能由目标节点本地管理入口创建和确认。
- 配对 UI 必须显示设备名称、key fingerprint 与过期时间；节点方向另显示待授予的 `grant.*`。设备方向没有可展示的授权集合，也不展示短验证码（理由见 §5.1）。
- 用户确认前设备记录不能获得 active 权限。
- 设备配对不授予任何授权：设备记录不保存 scope 集合，配对确认页也没有权限栏，通过认证的设备即可执行本节点全部业务命令（§10.1）。`session.create` 因此不再有「设备级授权 vs 资源快照」的语义——它的判定只剩本机的资源校验（已登记别名，或客户端提交的绝对目录，见 `SYNC_PROTOCOL.md` §11.5）；跨节点方向仍按 Owner 的 `grant.remote-work` 判定。
- 每个新 WSS 完整认证，不签发长期 bearer session/refresh token。
- Node identity 变化进入 `identity_changed`，不能自动接受。

`[实现现状]`（2026-10-14，`sync-server-connection` 的同步面已落地）配对与设备认证在 Sync 面的落地边界：两条配对 HTTP 端点（`POST /sync/v1/pairing/claim`、`/status`）与四消息 WSS 握手均已实现，但**只覆盖到认证完成**——即「谁在连接、来自哪个 canonical origin、持有哪些 scope」这一段事实已经由 `identity-auth` 的状态机与 `core` 的持久判定给出（未知设备同样拿到结构完整的挑战、失败统一收敛为 `auth.proof_invalid`；认证失败无论设备是否已知都留审计且归因只取持久化身份）。业务命令的逐条授权（§10.1/§10.2）在 Sync 面**尚未接线**：命令路由与执行路径未落地，因此「撤销该 scope 或撤销设备立即使后续命令被拒」这条在 Sync 侧当前只由**设备撤销传播**（关闭活动连接 + 拒绝后续认证）与连接级的 `session.list` 观察授权体现，命令级拒绝要等 §11 业务命令面接线后才是端到端事实。设备**撤销**的实绩见 §9.5 的 `[实现现状]`。

`[实现现状]`（2026-10-14）**Origin 校验**在 Sync 连接面已落地，口径是「失败关闭」而非「可选的加固项」：

- 来源取自升级请求的 HTTP `Origin` 头，经接入层归一化（小写 scheme、规范化 hostname、显式有效 port、省略默认端口、不含 path/query/fragment 与尾部 `/`），并以三形态传入连接面（`Known` / 头缺席的 `Absent` / 头存在但不可归一化），**不折叠成同一个 `None`**——因此「没有来源」与「来源不可归一化」在对端可见行为上不可混用（`crates/server/src/transport/net/` 的 `WsHandler::handle` 参数与 `crates/server/src/sync/conn/handshake.rs` 的来源判定）。
- 来源与设备记录绑定的 canonical origin **逐字**比较，不一致以 `auth.origin_mismatch` 拒绝；transcript 的 `canonicalOrigin` 取服务端判定值，**不取**客户端 body 自报值（对应 `SYNC_PROTOCOL.md` §8.1）。
- **无来源（`Absent`）与非 `https://` 来源一律拒绝**，不以空字节串放行：`identity-auth` 对 `SyncDevice` 强制要求 `canonical_origin` 为 `Some`，且 `CanonicalOrigin::parse` 只接受 `https://` 前缀。因此 `SYNC_PROTOCOL.md` §3.2 的「非浏览器原生客户端可以没有 HTTP `Origin`」在 v1 未被采纳（节点间另有 Node Link 面，不依赖 Origin）。
- 配对 HTTP 的 `Origin` 同样必须先存在、可归一化，并与该配对登记的 canonical origin 逐字相等；`Host` 边界由接入层在调用处理器前按 `daemon.allowed_hosts`/`daemon.public_origin` 判完（`crates/server/src/sync/pairing.rs`）。
- **已知偏差（未收紧，已登记）**：接入层 `HostPolicy` 只要求 `daemon.public_origin` 能解析成 `http`/`https` 的 origin，因此 `http://` 字面量能过 Host 边界、却会在配对 URL 派生处失败关闭；收紧时两处必须同时改（见 `CONFIG_REFERENCE.md` §1 末的「实现现状」）。

### 9.5 撤销与轮换

- 撤销设备必须提交持久状态后立即关闭其 active connection。
- 授权缩减（节点方向的 `grant.*` 与信任记录的 `exportIds`）立即作用于后续命令；高风险缩减应主动关闭连接并要求重新认证，以刷新客户端显示。设备方向没有可缩减的授权集合——设备只有「已批准」与「已撤销」两态。
- 怀疑 Device key 泄露时撤销该设备并重新配对。
- 怀疑 Node key 泄露时生成新 Node identity、撤销所有设备/节点信任并重新配对；不能用普通 endpoint 更新掩盖 key 变化。
- TLS 证书轮换不改变 Node identity；Node key 轮换不应由 TLS 证书更新自动触发。

`[实现现状]`（2026-10-14，`sync-server-connection`）**撤销传播**在 Sync 面已落地，且接线点唯一：持久提交（`core` 的撤销写集，含审计）之后，由组合根的 `ConnectionCloser` 调用 Sync 连接注册表按 `deviceId` 关闭其全部活动连接——close code `4410 device_revoked`，语义是「先排空已入队消息，再发 close 帧」，随后拒绝该设备的后续认证（未知/已撤销设备在 proof 阶段统一失败）。设备撤销钩子此前恒报「没有可关闭的连接」，现已接到真实注册表（`crates/app/src/daemon.rs` 的撤销缝与 `crates/server/src/sync/conn/registry.rs`）。撤销**不**撤销配对本身，也不影响同设备的新配对流程；撤销后重新认证被拒是设备级事实，不依赖连接表是否已排空。**仍未落地**：Node Link 侧的 Access 出站重连管理器（属切片 6 的 `node-link-client`），因此 `daemon.status.links` 恒为空；scope 缩减的主动关连接当前只对 Node 重配对路径成立（`close_node_after_reauth`），设备侧 scope 缩减关连接要等 §11 业务命令面接线后才有意义。

### 9.6 Node Link 信任约束

- Node Identity Key 与 Device Identity Key 使用不同 key purpose、数据库 record type 和签名 domain tag，即使算法相同也不得互换。
- Owner Node 只信任已配对 Access Node 代表其自身提交命令，不自动信任 Access Node 的本地用户、设备或其他下游节点。
- Access Node 提供的 `localPrincipalRef` 只用于审计和本地归因，不能绕过 Owner 的 Export grant。
- 第一阶段明确采用节点级信任；Owner 可以按 Access Node 授权、限流和撤销，但不得把 `localPrincipalRef` 描述为 Owner 已直接认证的员工身份或不可抵赖证据。
- 有效授权固定为 `Owner grant ∩ Access local grant ∩ runtime capability`，任一侧撤销或缩减后立即收紧。
- 第一阶段只允许一个 Node Link hop，并对资源来源链进行循环检查。

## 10. 授权模型

### 10.1 授权是服务端判定

- UI 隐藏按钮不是授权，隐藏入口也不是拒绝。
- 每条命令根据当前连接 Actor、最新设备记录（是否已撤销）、session 状态和 workspace 边界重新检查。设备方向的唯一前提是「该连接已通过设备认证」——设备配对不授予授权，因此不存在命令级授权判定。
- 命令的 `scope` 字段在 `compatibility/commands/v1/commands.json` 中与命令名同值，是命令词表的登记副本，不是设备侧的判定依据；设备方向不接受任何授权词汇（`pack.*`、`preset.*` 与 scope 在配对请求或命令参数中出现即被拒绝）。
- 授权词汇只剩跨节点导出授权 `grant.*`：其名称与命令名的对应关系以 `compatibility/commands/v1/commands.json` 为唯一来源，未知或拼错的名称在展开时显式拒绝，不自动产生权限（见 §10.2）。
- 已认证设备看到本节点全部快照与事件（不再按授权过滤）；仍可能出现的 sequence 空洞只来自保留策略（已压缩或已清理的事件），客户端不得据此推断隐藏内容。
- 认证类拒绝 MUST 先于本机读取：不能把目标会话或资源是否存在泄露给未认证或已撤销的连接。

### 10.2 命令与授权词汇

授权词汇只剩两层，同一概念只用一个写法：**命令 scope**（`compatibility/commands/v1/commands.json` 中与命令名同值的登记字段，设备方向不再按它判定）与**跨节点导出授权 `grant.*`**（Owner 授予 Access Node 的能力上限）。设备授权包 `pack.*` 与配对预设 `preset.*` 已随设备权限层删除——设备配对不授予任何授权，wire、设备记录、本地管理方法与配对 UI 都不再出现这两类名称。完整清单与机器可读定义在 `compatibility/commands/v1/commands.json`；下表是该文件的权威展开。

| 命令 | 类别 | 所需 scope | 对应 grant | 首阶段 |
|---|---|---|---|---|
| `session.list` | query | `session.list` | `grant.observe` | mvp |
| `session.read` | query | `session.read` | `grant.observe` | mvp |
| `command.status` | query | `command.status` | `grant.observe` | mvp |
| `session.mode.list` | query | `session.mode.list` | `grant.observe` | conditional_mvp |
| `session.config.list` | query | `session.config.list` | `grant.observe` | conditional_mvp |
| `session.prompt` | mutation | `session.prompt` | `grant.interact` | mvp |
| `session.cancel` | mutation | `session.cancel` | `grant.interact` | mvp |
| `elicitation.respond` | mutation | `elicitation.respond` | `grant.interact` | conditional_mvp |
| `session.mode.set` | mutation | `session.mode.set` | `grant.configure-session` | conditional_mvp |
| `session.config.set` | mutation | `session.config.set` | `grant.configure-session` | conditional_mvp |
| `permission.resolve` | mutation | `permission.resolve` | `grant.approve` | mvp |
| `session.create` | mutation | `session.create` | `grant.remote-work` | mvp（Node Link 与 Sync 共用命令名，payload 不同） |
| `session.resume` | mutation | `session.resume` | `grant.remote-work` | conditional_mvp（仅 Node Link；Sync 收到返回 `command.unsupported`） |

- 「所需 scope」列是**登记副本**，不是设备侧的授权判定：设备方向对任何命令都没有 scope 前提，认证通过即可执行。
- 五条查询命令（`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list`）在设备方向无需任何授权；断线恢复所需的 `command.status` 因此始终可用。节点方向它们仍归 `grant.observe`。
- `session.create` 经两个传输面暴露，前提与载荷不同：
  - **Sync 面**以「该连接已通过设备认证」为唯一前提（不再有设备侧授权判定）。payload 只允许 `agentId`（必填）与目录引用二者之一：已登记别名 `workspaceAlias`，或客户端提交的**绝对**目录 `workspacePath`（1..=4096 字节）；两者恰好出现一个，同时出现或都不出现，以及出现任何其它键（含 `exportId`、`templateParams`、MCP 配置或凭据字段）一律以 `protocol.schema_invalid` 拒绝且不创建会话。
  - **Sync 面的目录校验**：形状不合法（相对路径、含 `..`、别名未在本机登记）以参数类错误拒绝，且不触碰文件系统；形式合法但无法解析（目录被删除、被换成文件、规范化失败、权限不足）以 `internal.unavailable` 拒绝。两类都不得创建会话、不得启动 Agent 进程、不得回退到任何替代目录；解析得到的规范化路径不得回显给对端（§12.3）。
  - **Node Link 面**仍额外要求 `exportId`、Export 发布的 workspace template 与 Owner 的 `grant.remote-work`，且**请求不得携带任意 Owner 路径**（只允许 `workspaceAlias`），形状见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.7。两个面共用同一套命令名、幂等与终态语义。
- `session.resume` 只归 `grant.remote-work`（不新增授权维度），它的 payload 是空对象：恢复所需的 Agent、ACP 会话标识与创建时目录一律取自 Owner 自身的持久化记录，不接受客户端提供。目标 Agent 未宣告 `sessionCapabilities.resume` 时显式失败，不降级为新建会话。
- 节点方向的配对确认页与 Export 管理 UI 必须完整展示最终 `grant.*` 集合，不能用含糊的「完全访问」替代；设备方向的配对确认页没有权限栏——设备方向没有可授予的授权。
- 节点信任记录与 Export 只保存 `grant.*` 展开后的命令名集合；`grant.*` 是唯一的授权管理输入形式，落到 wire 与记录前必须展开。设备记录不保存任何授权集合。
- `grant.approve` 单独分组是为了让审批风险在 Export 与节点配对 UI 中清晰可见，不是为了削弱远程控制：跨节点的 `grant.interact` 不包含审批，审批必须显式授予 `grant.approve`。远程客户端可以选择 Agent 当前 permission request 明确提供、且 Owner 本地策略允许的任一 option；如果 Agent 明确说明某个 option 会形成持久授权，客户端必须展示该持续范围。客户端不能伪造新 option，也不能在请求之外修改 Owner 的全局沙箱或权限策略。

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

远程 `session.create` 是唯一与 workspace 相关的受限远程能力。Node Link 面只能引用 Export 发布的 alias/template；Sync 面只能引用本机当前已登记的 workspace 别名与已配置的 Agent 标识（wire 上传 `{ workspaceAlias, agentId }`，规范化路径、目录追加与凭据永远不出网）。两者都不得让客户端提交任意 Owner 路径或 Provider/MCP 凭据。未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`，不能只增加一个 command schema。

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

`[实现现状]`（2026-10-15，`sync-ui-hosting`）PWA 静态资源由 Daemon 同源托管在根路径，其 CSP **由响应头下发**（不依赖文档内的 `<meta>` 元标记——元标记约束不了 `frame-ancestors` 之类的指令），取值逐字等于 `FRONTEND_DESIGN.md` §4.5 的基线 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`。与它一起下发的还有与两个配对端点同取值的 `cache-control: no-store`、`pragma: no-cache`、`referrer-policy: no-referrer`、`x-content-type-options: nosniff`；实现见 `crates/server/src/transport/net/static_ui.rs`，经接入层的「每路径默认响应头」缝下发，因此 Host 边界在静态 path 上的 400 也带这些头。**基线未被放宽**：产物里的两条内联 `<style>`（`expo-reset` 与 `react-native-web` 的运行时样式表）已由前端侧移除（`ui-csp-inline-styles`）——入口文档把 reset 外置成同源 `/entry-reset.css`，并在 bundle 之前声明一个**必须保持为空**的同源 `/rnw-stylesheet.css`，`react-native-web` 于模块求值期复用该 `<link>` 的 `sheet`、把它运行时生成的原子类 `insertRule` 进去；因此样式现在由**同源外部样式表**交付，CSP 取值一字未动。**口径要说准**：`insertRule` 本身**不是** CSP 的检查点——向一个受允的同源样式表写入规则既不能被描述成「CSP 阻止了样式注入」，也不能被描述成「放宽了 CSP」；样式文本的来源仍是同源 bundle（`script-src 'self'` 已信任），这条控制在原内联形态下本就已失效，本次只是消除冲突。裁定与判据见归档的 `openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md`（D1/D3/R4）。

### 11.2 不可信内容渲染

- Markdown 默认禁用原始 HTML；链接只允许明确 scheme allowlist。
- 不自动加载 Agent 返回的远程图片、iframe、音视频或 tracking URL。
- 终端使用纯文本/受控 ANSI renderer，不把输出写入 `innerHTML`。
- diff、路径、工具参数和错误栈使用文本节点或经过测试的结构化组件。
- ACP `rawJson` 只能作为文本查看、复制或受控下载，不能执行、注入 DOM 或作为授权输入。
- 目录展示名（`displayName`）是用户经 `local.workspace.select` 输入的**不可信内容**（§4.3）：按 §4.3 的转义规则渲染，不得当 HTML/Markdown 解析、不得据此拼接路径或 URL、不得进入 `innerHTML`。目录别名是稳定主键；规范化路径既不是授权输入，也不出网（§12.3）。
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
- `[决定]`（2026-09-23）**注入集合可判定**：实际注入的环境变量 = `env_allowlist` ∩ profile 的 `env` 绑定声明的 `name` 集合，加必要的进程环境（如 `PATH`）；凭据值只能来自 `CredentialResolver`（`CORE_PORTS_AND_STORAGE.md` §11.6），未绑定或未列入白名单的变量一律不注入，keystore 不可用则**失败关闭**。
- `[决定]`（2026-09-24）**保留前缀在启动前静态拒绝**：`agent-host` 在 spawn 之前拒绝注入名为 `ACPR_*`（本 crate 的保留前缀约定）与 `ACP_REMOTE_*`（与 `core::model` 的 `is_reserved_env_name` 对齐）的变量——不把「不得注入节点/设备密钥」全部押在运行时的 `env_clear()` 上，从而也拦住旁路 core 校验的自定义配置/凭据来源。
- stdout 只作为 ACP wire；stderr 只做**有界采集 + 结构化计数**（只记收到的总字节数与丢弃字节数，不记内容）：内容进入固定上限的环形缓冲，**不进日志**，需要时按上限回取（§14.1）。
- `[决定]`（2026-09-23）**Agent 进程的上下限是固定 v1 常量，不是配置键**（与 `SYNC_PROTOCOL.md` §14 同惯例）；未列出的值不得由实现自行发明：

| 项 | v1 值 | 理由 / 约束 |
|---|---:|---|
| Agent 启动 → `initialize` 完成 | 10 s | 与 `daemon.shutdown_grace_ms` 同量级；超时即失败并回收进程树，不得无限等待 |
| `initialize`/`cancel` 等短请求 | 30 s | 与 Node Link 心跳同量级；**不适用于 `session/prompt`** |
| `session/prompt`（turn） | **不设超时** | 长任务是合法的；取消只由用户或 `session.cancel` 触发，超时杀进程会破坏 ACP 语义 |
| 关闭 grace（友好终止 → 强杀） | 5 s | 必须小于 `daemon.shutdown_grace_ms`（10 s），否则 daemon 无法在自己的 grace 内收尾 |
| 单条 ACP 消息（stdout 解析上限） | 1 MiB | 与 `SYNC_PROTOCOL.md` §14 的 `maxMessageBytes` 同值，避免同一条消息在两跳上有两个上限 |
| stderr 环形缓冲 | 256 KiB | 有界采集：内容原样进入固定上限的环形缓冲（不做内容脱敏），**内容不进日志**，只记丢弃字节数与采集总字节数（§14.1）；需要时按上限回取；超出丢弃最旧并记一条计数，不无界缓存 |
| 空闲回收 | 复用 `sessions.idle_timeout_ms` | `0` = 不因空闲关闭；非零时只有「无 active 会话且空闲超过该值」才关闭进程 |
| 内存 | **规则而非数值** | 无法可靠测量 RSS；改为可判定规则：必须流式处理 ACP 消息与 stderr，不得缓存完整会话正文，超限即报错并结束该 Agent |

  这些常量落在 `agent-host`，并由 `agent-host` 的 fake ACP child 测试固定（测试要求见 `docs/MODULE_ARCHITECTURE.md` §11）；需要随部署变化的只有 `sessions.idle_timeout_ms`。
- Windows 使用 Job Object 或等价机制清理完整子进程树。
- Agent 崩溃、乱序或非法 JSON 形成明确事件，不能使 Daemon 接受伪造客户端身份。

### 12.3 Workspace

- 原始 workspace 路径的登记只能由 Owner Node 本地管理入口完成（`local.workspace.select`）。唯一例外是 Sync 面的 `session.create` 可以携带本机自己的设备提交的绝对目录；它必须先通过本节的路径校验与规范化，且不得被当作「别名」登记（见 §10.2 与 `SYNC_PROTOCOL.md` §11.5）。
- 跨节点远程主体只能选择 Export 发布的 workspace alias/template，不能构造任意绝对路径、追加目录或修改 Agent 沙箱；Node Link 面的请求不得携带任意 Owner 路径。
- 文件路径展示和下载必须重新检查其属于会话授权边界；不能只依赖客户端传来的 path。
- 规范化路径后再比较，处理 symlink、junction、大小写、UNC 和 `..`。可判定规则（权威实现归 core，见 `CORE_PORTS_AND_STORAGE.md` §11.9）：只接受**绝对路径**；写入与使用前都必须存在且是**目录**；`canonicalize` 后的结果才是持久权威值；拒绝相对路径与含 `..` 的输入；UNC/网络路径允许但记一次结构化警告；路径与规范化结果**不得**出现在 catalog、事件、错误 `details` 或对端可见的任何输出里——口径是「不得泄漏**客户端尚未知道的**宿主路径」：客户端自己提交的目录因此不构成泄漏，但它同样 MUST NOT 被回显为对端可见字段。
- Agent 的实际文件/命令权限仍由 Agent sandbox 和用户配置控制；ACP Remote 不能虚报更严格隔离。

## 13. 持久化与本地数据保护

### 13.1 第一阶段静态数据决策

第一阶段不自行实现 SQLite 全库加密，也不引入自定义透明加密层：

- 依赖 OS 用户账户隔离、目录 ACL 和用户启用的 BitLocker/FileVault/LUKS 等磁盘保护。
- 文档和 CLI 应建议在存放敏感工程的电脑上启用全盘加密。
- Provider credential、Node private key 和 pairing secret 不进入普通 SQLite 字段。
- `[决定]`（2026-09-23）**Provider 凭据只在启动子进程时经 `CredentialResolver` 端口注入**（`CORE_PORTS_AND_STORAGE.md` §11.6）：`owned_provider_ref` 只存字段名、keystore 引用与版本，profile 的 `env` 绑定只存变量名；凭据值不落盘、不进事件、不进日志与错误消息。keystore 不可用或引用失效时**失败关闭**（不得跳过变量后继续启动）。
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
- 与 Daemon 同包发布的 PWA 静态资源属 §3 的 `public-artifact` 类别：根路径静态面只交付这些公开构建产物（入口文档与前端资源），不交付任何会话内容、凭据或用户数据，因此未认证的请求也能取到它。同包发布、构建版本可检查与发布构建缺产物即拒绝启动见 §16.3。

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
device.authenticated / auth_failed / revoked / scopes_changed   # scopes_changed 为历史取值，本版本不再产生（见下方 [决定]）
node.paired / node.authenticated / node.auth_failed / node.trust_revoked / node.identity_changed
export.created / revoked
import.added / removed
provider.configured
authorization.denied
rate_limit.triggered
storage.integrity_failed
```

`[决定]`（2026-10-16，`pairing-model` 变更）`device.scopes_changed` 标注为**只读的历史取值**：设备不再有授权集合（删除了设备权限层，设备通过认证即可执行命令），本版本**不再产生**该动作。保留它是因为升级前的库可能已有这类审计行——两张审计表的 `action` CHECK 仍接纳它，收缩取值会让历史行写不回去、读不出来（`CORE_PORTS_AND_STORAGE.md` §7.2 的 v8 段与 §11.6 第 1 条）。

`[决定]`（2026-09-26）新增 `node.authenticated`/`node.auth_failed` 两类：节点握手是安全动作，成功与失败都要留痕，而此前节点侧只有 `node.paired`/`node.trust_revoked`/`node.identity_changed`，握手的成败只能靠误用其它取值表达。首次认证把已批准配对推进到 `consumed` 时**不另设动作**——那次转移随该写集记录 `node.authenticated`（设备为 `device.authenticated`）；配对的认领方也会出现在审计里（`actor_kind = 'pairing_claimant'`），落库时两张审计表的 `actor_kind`/`action` CHECK 必须同步扩宽（12-step 表重建，见 `CORE_PORTS_AND_STORAGE.md` §7.2/§11.8）。

`[决定]`（2026-09-23）新增 `export.created`/`export.revoked`/`import.added`/`import.removed`/`provider.configured` 五类：Export/Import 的授权面与 Provider 凭据写入都是安全动作，而管理写集要求“涉及已登记安全动作的 mutation 其成功审计与状态同事务提交”（`CORE_PORTS_AND_STORAGE.md` §11.2 第 6 条）；没有取值只能靠误用 `node.trust_revoked`/`authorization.denied`，事后无法区分。本地个人配置（`agent.configure`/`workspace.select`）**不**登记，保持“只记安全动作”的取舍。落库时 `owned_audit`/`imported_audit` 的 CHECK 要同步改，而 SQLite 不能修改现有 CHECK，必须走 12-step 表重建（见该文档 §11.8）。

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

- CI 使用固定 action/toolchain 与锁定依赖，并保存审核记录：
  - 官方 `actions/*` 固定到 major，且该 major 必须运行在受支持的 Node runtime 上；
  - 第三方 action 只允许按完整 commit SHA 固定，并必须在 ADR 里记录其许可证、向外发送的数据与失败模式；
  - Rust 工具链版本唯一来源是仓库的 `rust-toolchain.toml`，CI 从该文件读取，不在 workflow 里另写一份；
  - 依赖锁文件（`Cargo.lock`、`package-lock.json`）入库，并由 `--locked` / `npm ci` 强制。
  当前使用的工具、版本与已知残余风险见 [ADR-0008](./adr/0008-ci-supply-chain-tooling.md)。
- 依赖准入与许可证由机器判定：`deny.toml` 的 `[licenses] allow`、`[sources]` 与 advisory 判定在 CI 强制执行；
  例外条目（`ignore` / `exceptions` / `[bans] deny`）必须逐条写明理由、受影响版本范围与移除条件；
  密钥扫描的允许清单遵循同一规则，且不得用于处置真实凭据（正确处置见 §17）。
- 发布凭据使用短期 OIDC/可信发布，避免长期 npm token 存入开发机或仓库 secret。
- 发布 job 与普通 PR CI 分离，需要受保护 tag/branch 和人工或策略批准。
- 不从 fork PR、未受信脚本或未验证 artifact 直接发布。
- 生成 SBOM、依赖许可证清单和平台 checksum；安全修复可以定位受影响版本
  （普通 CI 已有的许可证与 advisory 判定不替代本节：SBOM、checksum 签名与 provenance 属于发布阶段，见 §20）。

### 16.3 运行时更新

- 第一阶段不实现 Daemon 自更新器，不在运行时下载并执行远程 binary。
- PWA 静态资源与 Daemon 同包发布，构建版本可检查。`[实现现状]`（2026-10-15，`sync-ui-hosting`）静态资源**嵌入 Daemon 二进制**、随二进制一起发布，不经运行期磁盘目录、也不从远程下载；构建版本由前端构建钩子写进产物的 `build-version.json`（`clients/app/scripts/write-build-version.mjs`），Daemon 从**同一份**嵌入资源读出并经 `daemon.status` 的 `uiBuildVersion` 暴露（`string | null`，因此版本与所托管资源必然出自同一次构建；`null` = 版本缺失的显式语义，不编造版本）。发布构建在构建时没有可用前端产物（`clients/app/dist` 不含入口文档）即**拒绝启动**，不静默提供空页面（`FRONTEND_DESIGN.md` §4.3）。
- 协议不兼容时明确失败，不能通过关闭认证或加载远程旧脚本解决。

## 17. 安全事件处理

| 场景 | 响应 |
|---|---|
| 客户端设备丢失 | Owner/Access Node 执行 `device revoke`，关闭连接；必要时检查审计记录 |
| Device key 怀疑泄露 | 撤销该 device ID，重新配对为新设备 |
| Node key 丢失 | 生成新 Node identity，所有设备和节点重新配对 |
| Node key 怀疑泄露 | 停止远程入口、轮换 Node key、撤销全部设备/节点信任、检查本地系统 |
| TLS key/证书泄露 | 轮换证书和 TLS key；Node identity 保持不变但检查是否存在中间人窗口 |
| PWA XSS/静态资源篡改 | 停止服务、修复并发布新资源、轮换受影响设备身份，审查 Node/设备操作 |
| SQLite/附件泄露 | 视为聊天和工程敏感内容泄露；撤销设备不能补救已复制数据 |
| npm 包被篡改 | 撤回版本、发布安全公告和已验证新版本、轮换发布权限 |
| 依赖被披露漏洞（advisory） | Dependabot 告警与安全更新会给出受影响的锁定版本（仓库设置，已启用）；CI 的 `advisories` job 会阻塞合并。处理顺序：先升级到已修复版本；无法立即升级时按 §16.2 在 `deny.toml` 加带理由的 `ignore`（写明受影响版本范围、为什么本项目不受影响、何时移除），不得靠调整 job 状态绕过；影响已发布版本时按 §16.1/§16.2 评估撤回与公告 |

事件响应命令和 UI 必须优先提供 `device/node list/revoke`、Node fingerprint 查看和本地审计导出；审计导出默认不包含聊天内容。

## 18. 安全验证计划

### 18.1 自动化

- Sync schema 正反 fixture、重复键、尺寸、深度和恶意 JSON。
- transcript/HMAC/P1363 Rust-WebCrypto 互操作与错误向量（Rust 侧已在 `p256` 上对 12 个固定向量与 20 个畸形输入验证通过，见 `INITIAL_DESIGN.md` §16 第 6 条；实现阶段必须把同一批向量固化成 Rust 测试，而不是依赖一次性验证）。
- pairing 并发 claim、过期、重放、拒绝和 secret 清理。
- Node mismatch、Origin mismatch、错误 device/node、设备撤销和节点授权（`grant.*`/`exportIds`）缩减。
- Export grant 交集、Access Node 越权、循环导出和 Node Link 重放。
- `requestId` 重试、payload 冲突、崩溃窗口和 `uncertain`。
- snapshot barrier、ACK 越界、sequence 回退和慢客户端。
- XSS fixture：Markdown HTML、恶意链接、SVG、ANSI、diff、路径和 `rawJson`。
- 路径穿越、symlink/junction、workspace 越界和 shell argument 注入。
- 日志/错误/snapshot/fixture 中的 secret 扫描：仓库侧目前是两层——GitHub 原生 secret scanning 与
  push protection（推送前拦截已知 provider 模式，已启用）＋ CI 的 `secrets` job（push/PR 扫本次范围、
  每日定时任务扫全历史，覆盖 provider 模式之外的凭据，脱敏输出；见
  [ADR-0008](./adr/0008-ci-supply-chain-tooling.md)）。两者都不能替代运行时日志、错误与 snapshot 的扫描，
  后者仍待实现。**非 provider 模式（本项目自研格式的私钥与配对密钥）的仓库级检测已核实为不可用**
  （个人账号 public 仓库无 GHAS：界面无 Secret scanning 区域、API 不接受 `secret_scanning_non_provider_patterns`；
  见 `README.md` 的「分支保护」小节），因此这一类凭据在推送前没有服务端防线：机制是「一份规则、两个执行器」——
  规则写在 `.gitleaks.toml`（CI 的 `secrets` job 与本地 pre-commit 共用），本地执行器扫**暂存内容**；
  自研格式的规则等密钥格式定稿时与格式定义同改（残余风险与实现细节见 ADR-0008）。
- npm 平台选择、checksum、版本错配和禁止运行时下载。

`[实现现状]`（2026-10-14，`sync-server-connection` 的同步面已落地）上述清单中与 Sync 连接/投递面相关、且**已有 Rust 测试覆盖**的条目：Origin 不一致/缺席/非 `https` 三种拒绝、未知设备的行为一致性、错误设备、撤销（活动连接 `4410` 与后续认证被拒）、同设备连接替换、sequence 回退与未知 type 的拒绝、snapshot barrier、ACK 越界、慢客户端隔离与恢复、配对并发/重复 claim/过期/拒绝/secret 清理、pairing/认证载体里不出现 secret 字面量。这些用例的位置在 `crates/server/src/sync/`（`conn/tests.rs`、`subscribe/tests.rs`、`snapshot/tests.rs`、`subscribe/fanout_tests.rs`、`pairing.rs`）与 `crates/app/src/daemon/sync_ingress_tests.rs`（组合根接线级：subprotocol 升级、两条配对 path 不再 404、安全头）。**尚未有覆盖**：真实 WSS 端到端用例（脚本化 fake 客户端经真实 loopback listener 的完整链路，属 OpenSpec 变更 §WP8）；浏览器侧（WebCrypto/IndexedDB/Origin）与平台验收仍在 §18.2 的 PWA 阶段；§11 业务命令面的授权与幂等要等命令面接线。

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
  - 2026-09-23 定案：**本阶段维持失败关闭**，不提供持久化 fallback；将来若要提供，必须新开 ADR 定 KDF、解锁来源与备份策略（§9.2），并同步 `CONFIG_REFERENCE.md` §8 与本节。
- 是否以及何时把密钥存储提升到**硬件保护的不可导出档位**（Windows CNG/TPM、Linux 除 Secret Service 以外的系统 keystore）：需要单独 ADR，因为它会改变已接受的「非硬件保护实现允许存在、默认不启用」边界（ADR-0006 决策 5）。当前档位见 §9.2（Windows 用 DPAPI 包裹 + 进程内签名）。注意 `unsafe_code = "forbid"` 下交付该档位需要自写或引入 wrappable 的 FFI 封装，成本与风险必须在 ADR 里评估。
- `identity-keystore` 与 `agent-host` 的平台包装 crate 选型（DPAPI wrapper、Windows Job Object wrapper 等）：候选必须按 `AGENTS.md` §8 核验维护状态、许可证、平台支持与安全风险，并确认不超出 `deny.toml` 的许可证 allow 列表与 `[graph] targets`；若候选的 **MSRV 高于 `Cargo.toml` 的 `rust-version`（1.85）**，必须先单独做一个「是否抬 MSRV」的决定（工具链版本 `rust-toolchain.toml` 是另一件事，不能代替 MSRV 决策）。选型结论必须写回 `MODULE_ARCHITECTURE.md` §4.5/§4.12。
- npm provenance、checksum 签名和 SBOM 的发布工作流与格式仍待发布阶段确定。普通 CI 已接入 GitHub Actions（`.github/workflows/ci.yml`），执行合同门禁、Rust 检查、提交规范校验、依赖许可证与来源判定、依赖安全公告、密钥扫描及文档引用检查；已有普通 CI 不代表发布 provenance、签名或 SBOM 已实现。发布 job 继续按 §16.2 与普通 CI 分离。
- release crash dump 的平台默认策略。
- 是否以及何时通过 ADR 引入 SQLCipher、字段加密或 Noise Transport Profile。

本地管理通道已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定（stdio + 平台本地 IPC，不实现 loopback HTTP 管理面）。

这些选择不阻塞领域模型和 Sync DTO 开发，但相关平台 adapter 或正式发布在选择落定前不能声称安全完成。
