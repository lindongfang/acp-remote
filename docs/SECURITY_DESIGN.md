# ACP Remote 安全设计

> 状态：编码前安全基线（Draft）  
> 版本：0.6
> 修订记录（2026-10-22，`web-session-preparation` WP3，**identity-auth 已实现；服务端/存储接线待 WP2–WP5**）：§12.8 写回准备凭据的签名/验证实现边界——专用 domain、`Authority::sign_preparation_receipt` 与 `verify_preparation_receipt` 的固定顺序（认证/撤销 → 签名 → 绑定）、`storeEpoch` 单独的不可重试结果、逐字段绑定与跨设备/跨节点拒绝、错误脱敏与重放边界；确认 receipt 不是 bearer 凭证、不替代逐连接认证且不授予 scope。临时资源、附件暂存与服务端帧仍为计划。
> 修订记录（2026-10-22，`session-activation-lifecycle` WP1，**合同先行；实现待 WP2–WP5**）：新增 §12.7「本机资源回收与查看租约」——查看声明只绑定**已认证且页面可见**的连接、租期 30 秒/续租 10 秒、普通心跳不续租，断开/撤销/连接被替换立即清除且**不持久化**；资源回收只有 Agent 适配器确认真实释放后才交还容量名额，关闭失败/不确定保留占位。§9.5 补「撤销即刻失效查看声明」；§10.2 的 `session.resume` 更新为双传输面（Sync 侧受 `core.local-session-lifecycle.v1` 门控，payload `{}`、结果不带 `remoteSessionRef`），并说明 `session.view` 不是业务命令；§12.2 的空闲回收行更新为缺省 30 分钟与新增容量上限的语义；§5 威胁表补「伪造查看/容量」一行。威胁模型与信任边界**未变**：没有新增设备权限、没有新的长活凭据，也不放宽路径/内容可见面。
> 修订记录（2026-10-21，custom-session-title（合同与实现））：§10.2 的命令表登记 `session.title.set`（mutation、mvp、feature `core.session-title.v1`，Owner 元数据而非 ACP capability；只写本地会话标题列、不启动 Agent）；客户端改名禁令与「标题仅由 Agent 更新」的旧口径由本变更取代（原文见 SYNC_PROTOCOL §10.3 与 ACP_COMPATIBILITY_MATRIX §5）。威胁模型与信任边界未变；`custom_title` 持久化与命令接线已落地。
> 日期：2026-10-17  
> 修订记录（2026-10-18，`device-pairing-qr-reference`）：设备二维码改为短引用格式——fragment 只带版本、32 字节扫码取回凭据与 32 字节主机公钥指纹，完整资料经同源 `POST /sync/v1/pairing/qr-lookup` 取回；取回统一失败 `pairing.qr_invalid`、独立 10 次/分钟限流、16 KiB 上限，凭据只存内存且认领/拒绝/失效/过期/重启后不可用。§5.1 增补移动端「先匹配完整 SHA-256 指纹，再验证 `hostProof`」的信任链说明；威胁模型与其它边界未变。字段与端点以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.1/§7.6 为唯一权威。
> 修订记录（2026-10-20，`conversation-area-complete` WP3）：新增 §12.6「ACP 终端服务」——Agent 经 `terminal/*` 请求在宿主执行命令的信任边界、会话归属、cwd 限制、环境注入、进程树与输出预算；§5 威胁表补一行对应控制；§12.2 固定上限表登记终端输出保留/分块/并发与排空时长。终端服务只在组合根宣告 `clientCapabilities.terminal` 后接受调用（未宣告时不启动任何进程）；命令、参数、环境值、输出与路径都不进日志。威胁模型与信任边界**未变**（终端执行属于 ACP 原生能力，不新增授权维度）。
> 修订记录（2026-10-17，`sync-path-browse-and-diff-delivery`）：把「本机规范化路径永不出网」改写为**带前提的新边界**——**已登记**目录的规范化路径只下发给**协商了 `core.workspace-path.v1`** 的**已认证设备**，且只出现在快照的 `workspaces` 资源条目上（协商 `core.workspace-browse.v1` 时只读浏览 `workspace.browse` 的 `command.result` 是第二处路径可见面，只含该层条目）；客户端自己提交的目录不得被回显；日志、审计记录与错误 `details` 仍不含任何路径（§14.1 **未变**）。§3 把已记录目录的规范化路径纳入 `operational-metadata` 并写明例外共有两处（目录资源条目与只读浏览结果）；§8 补一句远程提交目录与目录浏览能力的边界；§10.2 新增只读查询命令 `workspace.browse`（feature `core.workspace-browse.v1` 门控、不登记、不产生审计动作）并改写 Sync 面目录校验的路径口径；§10.3 删去与 §10.2 自相矛盾的「规范化路径永不出网」表述、改为新口径；§12.3 把路径口径改为「只下发给协商该能力的设备、且只含已登记目录，另加只读浏览返回的那一层条目」；§19 登记残余风险。决策理由与被否决备选见 [ADR-0009](./adr/0009-authenticated-workspace-path-and-browse.md)。威胁模型与信任边界**变**：在新前提下路径的可见面扩大（残余风险见 §19 与 ADR-0009）。  
> 修订记录（2026-10-17，`directory-auto-registration`）：目录改由**成功的会话创建自动记录**（`directoryId = sha256(canonical_path)[..16]`），删除人工登记 `workspace.select` 与目录别名/展示名；目录的规范化路径**始终**随快照 `workspaces` 条目下发给已认证设备（撤销 `core.workspace-path.v1` 门控），会话摘要只带 `{ directoryId }`。路径的两处可见面（目录资源与只读浏览结果）与其余输出的禁路径口径不变。
> 修订记录（2026-10-16，`pairing-model`）：配对只保留两条密码学校验（用配对载荷里的主机公钥验证认领响应的 `hostProof`、已批准响应的主机公钥与载荷逐字节一致），**短验证码（SAS）两套全部删除**——§5 新增 §5.1 说明「为什么本协议不需要短验证码」，§9.3 的 pairing secret 用途、§9.4 的确认页展示项与 §18.1 的测试项随之收窄；**设备权限层删除**——§2.1/§2.2、§3 的 `security-metadata` 行、§4.2 的设备条目、§5 的威胁控制、§7.1/§7.2、§8、§9.4、§9.5、§10.1、§10.2 与 §12.3 改为「通过设备认证即可执行业务命令，设备不再有授权维度」，§10.2 的表格去掉「所属 pack」列、`pack.*`/`preset.*` 两类词汇删除；**保留**设备认证与每连接 challenge-response、配对确认只能由本机用户完成、撤销立即关闭连接、速率与各项上限、跨节点 `grant.*`/Export `scopes`/Import grants。威胁模型与信任边界没有放宽——删 SAS 的前提是 §5.1 写明的两条密码学要求已由 `pwa-web-client` 承担。  
> 修订记录（2026-10-16，`session-create-settle-order`）：§14.1 的「默认日志允许字段」补上**命令级日志**的固定字段（主体标识——哪个设备/节点在控制、命令名、资源标识、终态类别与**封闭词表**的失败成因类别），并重申「默认禁止」不变（不写路径、凭据与 prompt 正文）。威胁模型与信任边界**未变**。  
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
- 节点间权威、认证、重放与 owner/access 边界：[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md)
- PWA 身份与安全传输决策：[ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)

发生冲突时，已接受 ADR 优先于本文中的候选描述；本文优先于分散在实现注释中的安全假设。改变信任边界、密码算法、密钥生命周期、默认授权或正式部署要求时，必须同步更新本文，必要时新增 ADR。

## 2. 安全目标

### 2.1 必须保证

- 只有已配对、未撤销的设备或节点可以访问本节点资源：认证通过即可执行本节点的全部业务命令（设备与节点方向同一规则，见 §10）。
- 跨节点远程主体创建会话时只能使用 Owner Node 显式导出的 Agent 与 workspace template，不能提交任意 Owner 路径（Node Link 面）；本机自己的设备在 Sync 面可以提交会话目录，但受 §12.3 的绝对路径、存在性与规范化校验约束。
- Access Node 不能取得 Provider/MCP 凭据；跨节点可见面只受连接级认证与本机资源边界约束。
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
| `secret` | Node/Device 私钥、Provider token、pairing secret、网页入口 pairing code（可一次性取回 secret）、TLS private key | 不同步、不记录；平台安全存储或明确失败关闭 |
| `sensitive-content` | prompt、回复、终端、diff、文件内容、ACP `rawJson`、附件 | 仅授权设备可读；受保留期、配额和渲染隔离约束 |
| `security-metadata` | node/device public key、节点信任记录、撤销时间、Node fingerprint、审计结果 | 完整性优先；只向有权主体暴露必要字段 |
| `operational-metadata` | session title、Agent/model、时间、错误码、sequence、**已记录目录的规范化路径**（随目录资源下发，见 §12.3） | 仍可能泄露项目活动；按授权过滤和日志最小化 |
| `public-artifact` | PWA 静态资源、公开 schema、fixture、公钥算法信息 | 必须有发布完整性，不能包含生产 secret |

处理规则：

- 数据分类跟随内容，不因进入日志、缓存、fixture 或错误对象而降低。
- fixture 只能使用明确标记的公开测试密钥和虚构内容。
- 客户端可见不等于可写；每个 mutation 单独授权。
- 从高等级数据派生的摘要、文件名和路径仍可能敏感，不能默认公开；许可的路径面**只有两处**：(1) **已登记** workspace 的规范化路径——它随快照的 `workspaces` 资源条目下发（§12.3）；(2) 只读目录浏览 `workspace.browse` 的结果——它在客户端协商 `core.workspace-browse.v1` 时于 `command.result` 里返回被请求那一层的直接子目录绝对路径与逐字回显的父路径（§10.2、§12.3）。除这两处之外的一切输出（事件、错误 `details`、其余 `command.result`、日志与审计记录）仍不含任何路径，客户端自己提交的目录也不被回显——`workspace.browse` 逐字回显的请求父路径属上述第 (2) 处例外。

## 4. 主体与信任假设

### 4.1 可信主体

- 当前节点 OS 用户及其受保护的用户配置目录。
- ACP Remote 发布并加载的 Daemon/PWA 代码。
- 位于同一节点可信边界内、由用户配置的 TLS Provider。
- 操作系统提供的 DPAPI、Keychain、Secret Service、Keystore 等安全存储接口。

### 4.2 有限信任主体

- 已配对设备：只信任其持有设备私钥与「本机用户已确认这台设备」这一事实。设备方向不再有授权维度（没有可越出的 scope 集合）：认证通过即可执行本节点全部业务命令，边界由本机确认与撤销（而非 scope）决定，见 §10.1。
- 已配对 Access Node：只信任其持有 Node 私钥；配对即完全信任，不自动信任其本地客户端或下游节点。
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


**配对即完全信任（2026-10-17 裁决）**：删除 Export/Import 后，一次成功的节点配对等价于授权该节点读全部会话、发命令、**在本机创建会话（启动配置里的 Agent 进程）**并批准工具权限请求。保留的边界只有 `local.*` 永不远程、连接/消息/速率上限、配对须本机确认、撤销即时生效、Access 默认不落远程正文与审计留痕。
第一阶段至少防御：

| 威胁 | 主要控制 |
|---|---|
| 未配对设备连接 | WSS + Device challenge-response（认证即业务命令的执行前提） |
| 未配对/已撤销节点连接 | WSS + Node challenge-response + revocation |
| Tailnet/LAN 内横向访问 | 应用身份独立于网络身份；`proxy` 模式默认 loopback，`direct` 模式全接口监听告警 |
| Node 冒充或 endpoint 替换 | QR 固定 Node public key；每次连接验证 Node proof |
| 二维码转发/截获 | 短期单次 secret、配对载荷带外携带主机公钥、认领 HMAC、Host proof、本机确认（§5.1） |
| 握手和命令重放 | fresh nonce、connection ID/sequence、`requestId` 幂等 |
| 已撤销设备继续使用 | 撤销检查、立即关闭连接、后续认证失败 |
| CSWSH/CSRF/DNS rebinding | HTTPS、Origin/Host allowlist、同源接口、loopback 管理通道保护 |
| XSS 和 Agent 内容注入 | CSP、无第三方脚本、结构化渲染、HTML 禁用/清洗、`rawJson` 文本化 |
| 越权命令 | 服务端逐命令校验「已认证 + 会话状态 + workspace 边界」；`local.*` 无远程入口 |
| Agent 借 `terminal/*` 在宿主执行任意命令 | 只在能力宣告后接受；参数数组不经 shell、cwd 限于会话根、环境先清空后按必要项+Agent 值注入（`ACPR_*`/`ACP_REMOTE_*` 静态拒绝）、每终端独立 Job/进程组与输出预算；命令/输出/路径不进日志 |
| 已配对节点可完全访问本机资源 | 配对确认必须由本机用户完成；撤销即时关闭连接；审计保留 `viaNodeId` |
| 节点循环和能力洗白 | 第一阶段单 hop；imported Agent 禁止再次导出；来源链检查 |
| 重复 prompt/审批 | durable idempotency、Session Actor、interaction first-writer-wins |
| 伪造查看保活或挤占会话容量 | 查看声明只接受已认证连接、绑定当前会话与单调 `viewVersion`，租期 30 秒且普通心跳不续租；断开/撤销/被替换立即清除且不持久化；容量准入由 core 判定，只有确认释放的会话才归还名额，满额且无安全候选时显式拒绝（§12.7） |
| 慢客户端和内存耗尽 | 有界队列、消息/速率限制、断开后 cursor 重放 |
| 磁盘耗尽和超大输出 | TTL、配额、截断标识、存储失败关闭 |
| 附件跨会话/跨主体冒用或公开下载地址泄露 | 独立已认证连接 + 每帧会话/主体绑定 + 内容寻址读取；不使用 bearer URL；撤销立即中止 |
| 恶意附件内容（HTML/SVG/可执行）或外链追踪 | 只按不可信数据处理；HTML/脚本/可执行不活动渲染；外部图片与链接不自动加载 |
| 日志或错误泄露 | 数据分类、字段 allowlist、默认脱敏 |
| npm/二进制篡改 | 锁定构建、平台包映射、checksum、provenance、最小发布权限 |

### 5.1 `[决定]` 为什么本协议不需要短验证码（SAS）

**决策句**：ACP Remote 的配对与认证**不派生、不在 wire 上返回、不展示也不校验短验证码**。信任锚是「配对载荷**带外**携带主机公钥指纹（设备二维码；节点二维码携带节点公钥）与一次性配对秘密 + 认领阶段以配对秘密计算的 HMAC」——教科书里 SAS 所防的公钥替换已经由这条锚承担；而对本流程里剩下的两种现实威胁（配对载荷在带外被整条替换、配对秘密泄露），SAS **完全无效**。设备侧的主机信任因此由两条密码学要求确立，不需要人眼比对。

理由：

- SAS 在教科书里的职责是「公钥经不可信通道交换时被替换」的带外校验。本协议的设备二维码 **带外**携带主机公钥的完整 SHA-256 指纹（节点二维码仍带外携带节点公钥），且认领与状态请求都必须用一次性配对秘密算出正确的 HMAC 才会被受理（`SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2），因此攻击者即使能改写通道，也无法在不掌握配对秘密的前提下让设备接受另一把主机公钥。这条锚的强度不依赖人眼比数字。
- SAS 能核对的只是「载荷里那把公钥」，**不是**「设备最终落盘的那把」。设备此后长期使用的主机公钥必须靠两条设备侧校验确立：先用§5.1 的指纹规则把扫码取回的公钥固定下来，再用它验证认领响应的 `hostProof`，以及已批准响应里的主机公钥与该绑定公钥**逐字节相同**（状态响应不含任何主机签名，只能靠比对）。两条都失败关闭、都不需要人参与，要求见能力规格 `pwa-web-client`。
- 对剩下的两种现实威胁，SAS 无效：① 配对载荷在带外被**整条替换**——真主机那次配对从未被认领，真主机终端上根本没有可比对的数字，用户比的是攻击者主机算出的数字；② 配对秘密泄露——持有者能算出同一个数字。
- 删除 SAS 不降低任何已承诺的安全属性：它剩下的作用只是「不依赖代码正确性的人眼检查点」，属纵深而非安全依据；保留它反而会让「已经比对过」的错觉掩盖第 2 条那个真正的缺口。

**明确保留**的仪式与边界（不因本条放宽）：配对确认仍必须由**本机用户**在电脑上完成（发起配对 + 按一次 y）；设备认证仍是每连接 challenge-response；设备撤销仍立即关闭活动连接；速率与各项上限不变。删除 SAS 的实施范围（两套 transcript domain 与固定向量、配对状态字段、本地管理方法字段、CLI 参数与展示项）见 `SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §9.2/§9.4 与 `LOCAL_ADMIN_PROTOCOL.md`。

`[决定]`（2026-10-18，`pairing-code-web-entry`）**网页入口的信任边界是 canonical origin + TLS 的首次信任（TOFU）；移动端仍是带外锚。**两者的区别必须按下列口径理解，不得把网页入口描述为与二维码入口同强度：

- `[决定]`（2026-10-18，`device-pairing-qr-reference`）二维码/原生移动端入口的带外锚变成**主机公钥指纹**：fragment 只携带 `0x02`、32 字节扫码取回凭据与主机公钥的完整 SHA-256 摘要；App 先固定外层 canonical origin，经同源 `qr-lookup` 取回完整资料，要求返回的 `canonicalOrigin` 与目标一致、`hostPublicKey` 是规范 65 字节未压缩 P-256 公钥且 SHA-256 与二维码指纹逐字节相同，**之后**才接受其余绑定。claim 响应仍用这把公钥验证 `hostProof`，已批准响应携带的公钥必须与它逐字节相同（`SYNC_PROTOCOL.md` §7.2/§7.3 原样）。指纹只绑定公钥，不单独认证其它字段；请求与 WSS 仍受可信 HTTPS、Host/Origin 与有效期约束，不能把本方案描述为不依赖 TLS 或页面代码。
- qr-lookup 的所有失败（凭据不存在/已过期/已认领/已拒绝/已失效、格式错误、来源或绑定不符）统一为 `401 pairing.qr_invalid`，不泄露差异；它不消耗配对码、不推进状态、不增加配对证明失败计数。限流与配对码 lookup、claim 各自独立（按真实连接来源 10 次/分钟）。移动端只使用二维码，不提供输码入口，本节不改变它的一分强度。
- 网页 `/pair` 无 fragment 时输入 8 位配对码，经同源 `POST /sync/v1/pairing/lookup` 取回 `pairingId`/`hostId`/`hostPublicKey`/`canonicalOrigin`/`pairingSecret`/`expiresAt`。主机公钥此时来自 TLS 保护、由 canonical origin 限定的 lookup 响应，客户端把它作为**本次配对**的绑定：用它验证 `hostProof`，并要求已批准响应逐字节相同；已有配对记录时，用户点击「继续配对」即明确发起重新配对；仅在本机批准且主机证明与本次绑定公钥校验通过后替换旧记录，进页、等待确认或失败均保留旧记录。允许任意 canonical origin（含 LAN/Tailscale）——网页 JS 本就由这个 origin 提供；origin 被攻破时，任何带外指纹也无法阻止恶意 JS 跳过校验。网页入口因此只丢失「同源代理把请求错路由到另一台 daemon」这一部署异常的检测，剩余可观察面是终端仍停在等待认领；配对码输入页显示当前 canonical origin 与 HTTPS 标识，让用户能发现打开的是另一台机器。
- 配对码由 pairing secret 做域分离派生（HMAC-SHA256，domain tag `acp-remote/pairing-code/v1`，取前 40 bit 编码为 8 位 Crockford base32），只作为**一次性取回凭据**：不是 claim/status 的 HMAC 密钥，不替代或缩短 256 bit 的 pairing secret，不新增持久字段。40 bit 可接受的前提是**独立限流**——lookup 与 claim 各按来源 10 次/分钟、互不占用额度，lookup 失败不增加任何配对的证明失败计数，也不区分「配对不存在」与「已过期/已消耗」。5 分钟有效期把单个来源在整个配对生命周期内的尝试压到约 50 次（命中概率约 `50/2^40`），且码只负责取回已存在配对的 secret，不承担认证；离线攻击面仍是完整 secret 的 256 bit。
- lookup 成功**原子地消耗**配对码；迟到的第二次请求，或第一次响应在网络上丢失，都只能得到统一的 `401 pairing.code_invalid`。这是单次语义的显式代价：恢复路径是重新执行 `device pair`，页面不做自动重试，只呈现固定文案「配对码不对或已过期，请重新生成配对码」。本版不引入 lookup 幂等键。
- `pairingCode` 与 `pairingSecret` MUST NOT 进入日志、URL query、Cookie、localStorage/sessionStorage、地址历史或任何明文持久化字段，MUST NOT 落盘。配对码只存在于三处：页面内存、`identity-auth` 的进程内查找表（`State.codes`）、以及 `device.pair.begin` 的一次性管理响应；secret 只存在于三处：`identity-auth` 内存、lookup 响应体、页面私有会话状态（`ClientStore` 的私有字段）。三处都不写盘、不进日志、不随可导出状态外泄（与 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md) §4.3、[ADR-0010](./adr/0010-web-pairing-code-entry.md) 一致）。处理路径不得记录请求/响应体，错误消息不得回显配对码；`PairingCode` 的 `Debug` 必须脱敏且不提供 `Display`。

## 6. 信任边界与数据流

```text
Local clients -> Access Node -> Node Link/WSS -> Owner Node -> AgentHost -> ACP Agent
                       │              │                │
                  Node identity    capability
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

- Daemon 在 `proxy` 模式默认只监听 `127.0.0.1`/`::1`；`direct` 模式默认监听 `0.0.0.0` 并尽力补绑 `[::]`，必须在启动输出中告警。`proxy` 模式禁止非 loopback 绑定（明文面不得超出 loopback）。
- 正式远程入口只能是 `https://`/`wss://`。
- TLS 必须在 Daemon 或同机可信 Provider 终止。
- 配置必须声明 canonical public origin、允许的 Host 和可信代理地址。
- 除明确配置的同机代理外，不信任 `Forwarded`、`X-Forwarded-*` 或供应商身份 Header。
- 不根据源 IP、`.ts.net` 后缀、Tailnet 用户或客户端证书自动增加授权；已认证节点即为执行前提。
- 不能默认以**明文**形态监听 `0.0.0.0` 或 `[::]`；全接口监听必须由 `direct` TLS 承担，并在启动输出中告警。

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

Provider/MCP 配置、原始 workspace 路径的**登记**、Node/Device 管理属于 Owner Node 本地管理能力。远程 `session.create` 只能引用已登记的目录与配置声明的 Agent；Node Link 面不得让请求携带任意 Owner 路径。

优先顺序（已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定）：

1. CLI 前台进程内调用或 stdio：`daemon start`、离线 `doctor`，以及确认 Daemon 未运行后的 status/stop 回答在此完成；运行中的 status/stop 经下一项本地 IPC 交给组合根（具体生命周期见 `LOCAL_ADMIN_PROTOCOL.md` §7）。
运行中 Daemon 的设备配对与撤销、节点配对与撤销、审计导出都走这里，两端共用 `core::use_cases` 的同一组用例。
3. 只有在单独设计本地认证、Origin/CSRF 和权限模型后，才允许 loopback HTTP 管理 API；当前不实现。

本地通道还承载 `acp-remote acp-stdio` 与 Daemon 之间的 ACP 会话流：facade 常驻 Daemon，CLI 侧只是 stdin/stdout 的字节泵，因此这条流是长期、双向、需要背压的，与上面的请求/响应管理载荷分开定义。Daemon 未运行时 `acp-stdio` 以明确错误退出，不得自行打开数据库或启动第二套核心。

约束：

- `acp-facade` 通过 stdio 服务 Zed，不开放远程 TCP 管理面。
- 本地 IPC 只允许启动 Daemon 的 OS 用户访问；授权依据是"调用方是同一 OS 用户"，不新增本地 token，也不复用设备/节点身份。
- 本地管理请求/响应编码由该 adapter 自己拥有，不复用 Sync 或 Node Link DTO。
- 设备认证与节点信任都不能映射到 Provider credential 读取或未显式配置的 Agent/MCP 配置；Node Link 面不得让请求携带任意 Owner 路径。
- IPC endpoint 建立失败（ACL/权限不符、路径被占用、socket 被替换为符号链接）时正式模式拒绝启动，不得降级为无管理通道或临时无认证 endpoint。

## 9. 身份、密钥与配对

### 9.1 密钥职责分离

| 密钥 | 用途 | 生命周期 |
|---|---|---|
| TLS private key | endpoint TLS | 由 Daemon或同机 Provider 管理 |
| Node identity key | Sync `hostProof`（wire 字段名 `hostId`/`hostPublicKey`/`hostProof`）与 Node Link `nodeProof`、稳定节点身份和节点配对 | 长期；丢失后所有设备/节点重新配对 |
| Device identity key | Device proof | 每安装/origin 独立；撤销或数据清理后失效 |
| pairing secret | 首次 claim 与状态查询（配对认领 HMAC、状态 HMAC） | 单次、短期，最迟到原 expiresAt |
| 扫码取回凭据（`qrToken`） | 设备二维码取回尚未认领的配对资料（`qr-lookup`）；不参与 proof | 单次配对、内存、最迟到原 expiresAt；认领/拒绝/失效/重启后不可用 |
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
- 配对 UI 必须显示设备/节点名称、key fingerprint 与过期时间；节点方向不展示授权集合（配对即完全信任）。
- 用户确认前设备记录不能获得 active 权限。
跨节点方向与设备相同：已认证即可，参数只剩本机资源校验。
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
- 撤销、断开与同设备连接被替换必须同时清除该连接持有的**查看声明与租约**（`session.view`，`core.local-session-lifecycle.v1`）：租约只存在于内存，不因连接仍在线而继续有效，也不写入任何表；撤销后该连接既不能续租也不能声称仍在查看，受影响的会话按「无查看者」重新计时（§12.7）。
- 配对的只有「已批准」与「已撤销」两态；撤销立即作用于后续命令并关闭连接。
- 怀疑 Device key 泄露时撤销该设备并重新配对。
- 怀疑 Node key 泄露时生成新 Node identity、撤销所有设备/节点信任并重新配对；不能用普通 endpoint 更新掩盖 key 变化。
- TLS 证书轮换不改变 Node identity；Node key 轮换不应由 TLS 证书更新自动触发。

`[实现现状]`（2026-10-14，`sync-server-connection`）**撤销传播**在 Sync 面已落地，且接线点唯一：持久提交（`core` 的撤销写集，含审计）之后，由组合根的 `ConnectionCloser` 调用 Sync 连接注册表按 `deviceId` 关闭其全部活动连接——close code `4410 device_revoked`，语义是「先排空已入队消息，再发 close 帧」，随后拒绝该设备的后续认证（未知/已撤销设备在 proof 阶段统一失败）。设备撤销钩子此前恒报「没有可关闭的连接」，现已接到真实注册表（`crates/app/src/daemon.rs` 的撤销缝与 `crates/server/src/sync/conn/registry.rs`）。撤销**不**撤销配对本身，也不影响同设备的新配对流程；撤销后重新认证被拒是设备级事实，不依赖连接表是否已排空。**仍未落地**：Node Link 侧的 Access 出站重连管理器（属切片 6 的 `node-link-client`），因此 `daemon.status.links` 恒为空；scope 缩减的主动关连接当前只对 Node 重配对路径成立（`close_node_after_reauth`），设备侧 scope 缩减关连接要等 §11 业务命令面接线后才有意义。

### 9.6 Node Link 信任约束

- Node Identity Key 与 Device Identity Key 使用不同 key purpose、数据库 record type 和签名 domain tag，即使算法相同也不得互换。
- Owner Node 只信任已配对 Access Node 代表其自身提交命令，不自动信任 Access Node 的本地用户、设备或其他下游节点。
- Access Node 提供的 `localPrincipalRef` 只用于审计和本地归因；它不构成额外身份，也不能绕过 `local.*` 的本地边界。
- 第一阶段明确采用节点级信任；Owner 可以按 Access Node 授权、限流和撤销，但不得把 `localPrincipalRef` 描述为 Owner 已直接认证的员工身份或不可抵赖证据。
- 有效能力固定为「端到端 capability ∩ 本机资源边界」，任一侧撤销后立即收紧。
- 第一阶段只允许一个 Node Link hop，并对资源来源链进行循环检查。
- **Access 侧出站 TLS 信任边界**（用户裁决：系统根 + 可选固定 CA）：信任锚 = 操作系统根存储 ∪ `node_link.access.trust_roots` 列出的 PEM 证书；自定义 CA 只追加、不替换系统根，也没有「空锚」或「跳过校验」的运行形态（两处都为空、任一文件不可读或不含证书即拒绝启动）。配对 claim/status 的 HTTPS 与业务/附件 WSS 共用同一份信任构造（`app::trust` → `node-link-client` 的 `TlsTrust::Roots`）。出站传输按每条 Owner 端点构造（`node-link-client::TransportFactory`），Owner 端点只允许 `wss://` 并使用这份根集合：持久化的 Owner 信任记录与配对 URL 的 endpoint 都不接受 `ws://`，`AccessManager` 在建立链路前显式拒绝明文 scheme（`dev_mode.allow_plaintext` 不放宽出站 Owner 链路）。不存在把 `ws://` 端点套上 `Roots`、或把业务链路降级成明文的路径。
- 证书校验失败即失败关闭：连接直接失败，不回退明文、不重试降级。入站 listener 与配对请求的明文只允许在 `dev_mode.enabled && dev_mode.allow_plaintext` 且目标为 loopback 时由组合根显式选择；出站 Owner 链路不提供明文选项。
- 出站错误与日志只含状态/分类（`daemon.trust_*`、`local.remote_error`），不记录完整 URL、fragment、pairing secret、响应正文或凭据；`pairingUrl` 只作为同一次 `node.pair.begin` 的返回值交回调用方。
- claim 的 Owner 身份以二维码中固定的 `ownerPublicKey` 验签 `ownerProof`，`approved` 状态再核对 `ownerNodeId`/公钥与本机 node id 逐字一致；人类核对使用 `peerPublicKeyFingerprint`（`--fingerprint` 逐字比较），指纹只在内存与终端出现。

## 10. 授权模型

### 10.1 授权是服务端判定

- UI 隐藏按钮不是授权，隐藏入口也不是拒绝。
- 每条命令根据当前连接 Actor、最新设备记录（是否已撤销）、session 状态和 workspace 边界重新检查。设备方向的唯一前提是「该连接已通过设备认证」——设备配对不授予授权，因此不存在命令级授权判定。
- 命令的 `scope` 字段在 `compatibility/commands/v1/commands.json` 中与命令名同值，是命令词表的登记副本，不是设备侧的判定依据；设备方向不接受任何授权词汇（`pack.*`、`preset.*` 与 scope 在配对请求或命令参数中出现即被拒绝）。
- 不存在跨节点授权词汇：已认证节点即为执行前提，`local.*` 除外。
- 已认证设备看到本节点全部快照与事件（不再按授权过滤）；仍可能出现的 sequence 空洞只来自保留策略（已压缩或已清理的事件），客户端不得据此推断隐藏内容。
- 认证类拒绝 MUST 先于本机读取：不能把目标会话或资源是否存在泄露给未认证或已撤销的连接。

### 10.2 命令与授权词汇

授权判定只剩连接级边界：已认证且未撤销的设备或节点即可执行本节点业务命令，不存在逐命令的 `grant.*` 展开或交集判定。命令名的唯一机器定义仍是 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json)：

| 命令 | 类别 | 首阶段 |
|---|---|---|
| `session.list` | query | mvp |
| `session.read` | query | mvp |
| `command.status` | query | mvp |
| `session.mode.list` | query | conditional_mvp |
| `session.config.list` | query | conditional_mvp |
| `workspace.mkdir` | mutation | mvp |
| `workspace.browse` | query | mvp |
| `session.prompt` | mutation | mvp |
| `session.cancel` | mutation | mvp |
| `elicitation.respond` | mutation | conditional_mvp |
| `session.mode.set` | mutation | conditional_mvp |
| `session.config.set` | mutation | conditional_mvp |
| `permission.resolve` | mutation | mvp |
| `session.create` | mutation | mvp（Node Link 与 Sync 共用命令名，payload 不同） |
| `session.files.list` | query | mvp |
| `session.files.read` | query | mvp |
| `session.git.status` | query | mvp |
| `session.git.diff` | query | mvp |
| `session.resume` | mutation | conditional_mvp（Node Link 与 Sync 共用命令名；Sync 侧仅在协商 `core.local-session-lifecycle.v1` 后开放，payload `{}`、结果 `{ sessionId, session }` 不带 `remoteSessionRef`） |
| `session.title.set` | mutation | mvp（feature `core.session-title.v1`；Owner 元数据，非 ACP capability） |
| `session.start` | mutation | conditional_mvp（feature `core.session-preparation.v1`；Sync-only 首次提交与显式继续。`envelope.sessionId` 必须是准备凭据证明的 ID 或已存在的正式会话，`payload` 是 `kind` 闭合判别联合；不进入 Node Link，也不改变 ACP `session/new` 与旧 `session.create`） |

仍由各层独立执行的下界：schema/长度/频率限制、会话状态机、幂等键、连接上限，以及本地管理入口的 OS 用户边界。

`session.view` / `session.view.result` **不是**业务命令：它们不产生受理/终态、不进上表、也不获得命令级授权；它们只允许已认证连接更新自己的运行期查看声明，每连接一个查看者、独立 120 帧/分钟限流，未协商 `core.local-session-lifecycle.v1` 时按 `protocol.type_unsupported` 拒绝（`SYNC_PROTOCOL.md` §11.8）。`session.capacity_exceeded` 是新增的业务错误（`retryable = true`），只用于「无安全候选可回收」的激活/恢复拒绝，不得用它区分会话是否存在。

### 10.3 本地管理能力（永不远程授予）

Owner Node 本地管理能力只由操作系统用户边界校验后的本地入口使用，永不远程授予，也不参与任何展开：

| 能力 | 覆盖方法 |
|---|---|
| `local.agent.configure` | `agent.list` |
| `local.device.manage` | 设备/节点的配对、确认、拒绝、列表与撤销 |
| `local.node.rotate-key` | `node.rotate-key.begin`（本切片回 `local.unsupported`） |
| `local.audit.export` | `audit.export` |

`local.*` 的实现位于 `server::local_admin`，远程连接（Sync/Node Link）没有任何调用路径。

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

`[实现现状]`（2026-10-15，`sync-ui-hosting`）PWA 静态资源由 Daemon 同源托管在根路径，其 CSP **由响应头下发**（不依赖文档内的 `<meta>` 元标记——元标记约束不了 `frame-ancestors` 之类的指令），取值逐字等于 `FRONTEND_DESIGN.md` §4.5 的基线 `default-src 'self'; script-src 'self'; connect-src 'self'; img-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`。其中 `img-src 'self' blob:` 是只读图片预览的唯一增量（Blob URL 由已认证查询得到的字节创建），`script-src` 等其余指令未放宽。**计划增量**：Agent 音频播放（`conversation-area-complete` WP6）需要追加 `media-src 'self' blob:`；在 Daemon 响应头真正下发之前该能力不得启用或宣称，当前取值仍是上面的基线。与它一起下发的还有与两个配对端点同取值的 `cache-control: no-store`、`pragma: no-cache`、`referrer-policy: no-referrer`、`x-content-type-options: nosniff`；实现见 `crates/server/src/transport/net/static_ui.rs`，经接入层的「每路径默认响应头」缝下发，因此 Host 边界在静态 path 上的 400 也带这些头。**基线未被放宽**：产物里的两条内联 `<style>`（`expo-reset` 与 `react-native-web` 的运行时样式表）已由前端侧移除（`ui-csp-inline-styles`）——入口文档把 reset 外置成同源 `/entry-reset.css`，并在 bundle 之前声明一个**必须保持为空**的同源 `/rnw-stylesheet.css`，`react-native-web` 于模块求值期复用该 `<link>` 的 `sheet`、把它运行时生成的原子类 `insertRule` 进去；因此样式现在由**同源外部样式表**交付，CSP 取值一字未动。**口径要说准**：`insertRule` 本身**不是** CSP 的检查点——向一个受允的同源样式表写入规则既不能被描述成「CSP 阻止了样式注入」，也不能被描述成「放宽了 CSP」；样式文本的来源仍是同源 bundle（`script-src 'self'` 已信任），这条控制在原内联形态下本就已失效，本次只是消除冲突。裁定与判据见归档的 `openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md`（D1/D3/R4）。

### 11.2 不可信内容渲染

- Markdown 默认禁用原始 HTML；链接只允许明确 scheme allowlist。
- 不自动加载 Agent 返回的远程图片、iframe、音视频或 tracking URL。
- 终端使用纯文本/受控 ANSI renderer，不把输出写入 `innerHTML`。
- diff、路径、工具参数和错误栈使用文本节点或经过测试的结构化组件。
- ACP `rawJson` 只能作为文本查看、复制或受控下载，不能执行、注入 DOM 或作为授权输入。
- 目录展示名（`displayName`）是用户经 成功的会话创建（目录自动记录） 输入的**不可信内容**（§4.3）：按 §4.3 的转义规则渲染，不得当 HTML/Markdown 解析、不得据此拼接路径或 URL、不得进入 `innerHTML`。目录别名是稳定主键；规范化路径既不是授权输入，其对外下发也只有两处、都受 feature 门控且只含对端必需的那一层：随目录资源下发**已记录**目录，协商 `core.workspace-browse.v1` 时随 `workspace.browse` 的结果下发该层直接子目录（§12.3）。
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
- **准备草稿（`web-session-preparation` WP6，已实现）**：唯一允许的客户端正文留存是「本机准备中、尚未首次提交」的标签页草稿（[`FRONTEND_DESIGN.md`](./FRONTEND_DESIGN.md) §7）。它由专用平台端口 `PreparationDraftPort` 实现，使用独立 IndexedDB 数据库 `acp-remote-preparation-v1`（`drafts`/`chunks`/`ownership`），按 `canonical origin + ownerNodeId + deviceId + tabSlotId + sessionId` 分区；sessionStorage 只保存非敏感标签页定位。草稿 MUST NOT 含 Provider/MCP 凭据、设备密钥、配对码/配对秘密、认证材料、续传令牌或 imported 内容；运行期精确键守门拒绝任何未知字段，配置值只接受字符串/布尔值。附件以 128 KiB 分块保存为字节，不保存 `File` 句柄或 Blob URL。每次保存先写完新版本分块，再在单个 IndexedDB 读写事务里比较版本并提交，旧版本在提交前保持可读；迟到写看到墓碑即失败。配额失败逐项返回 `itemLocalIds`，不得截断、静默删附件或改走服务端上传；实现不设产品级附件数量/大小上限，只受浏览器配额约束。可见性隐藏/冻结只更新归属心跳；主动离开、更换 Agent/目录先写逻辑删除墓碑再清字节；无人持有的活跃残留以 24 小时孤儿窗口回收，活标签页持有的草稿不删；墓碑只含版本/原因并保留以阻止迟到写，不承载正文。只有在权威 `accepted` 且读取验证完整消息后才清除正文/Blob，并保留未终结请求关联直至命令终结。站点数据被清理时草稿随之丢失，按草稿丢失处理；设备密钥同时丢失时仍按本节下一条重新配对。本例外不放宽 imported/no-content-cache 或摘要缓存规则。
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
- `[决定]`（2026-10-17，`agent-declaration-from-config`）**注入集合固定**：`env_clear()` 后只注入 `PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`（宿主缺失时跳过）；不再有 `env_allowlist` 或 Provider 凭据绑定，凭据由 Agent 自己持有。
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
| 空闲回收 | 复用 `sessions.idle_timeout_ms` | 缺省 `1800000`（30 分钟，**合同已冻结，实现待 WP4**；落地前解析器仍为 `0`）；显式 `0` 只关闭按时间回收，**不**关闭容量保护。非零时只有「无查看者、无任务/交互且已确认释放」的会话超时才回收；容量另由 `sessions.max_active_sessions` 约束（候选默认 50，待实测），回收仍受同一「必须确认释放」约束 |
| 单个终端保留输出 | 1 MiB（Agent 的 `outputByteLimit` 更小时取更小值） | 超出从**开头**丢弃并落在字符边界；`terminal/output` 置 `truncated`，实时 `terminal.output` 事件仍携带真实分块（每块 ≤ 8 KiB） |
| 单条 `terminal/output` 响应 | 1 MiB（含 JSON-RPC 信封与 JSON 转义后的实际字节） | 保留正文放不进帧预算时再次从**开头**裁剪并置 `truncated`；发送失败必须回显式错误，不能悬挂 |
| 单条 `terminal.output` 事件 / 单次读取 | 8 KiB | 远低于 1 MiB 单条消息上限；读取边界可以落在 UTF-8 多字节字符中间，由增量解码器拼接 |
| 单会话并发终端 | 8 | 运行保护，不是产品额度；超出显式拒绝 `terminal/create` |
| 终端关闭排空 | 2 s | 先结束进程树再等驱动任务交付已读输出；超时不阻塞关闭，剩余读取由任务结束收敛 |
| 内存 | **规则而非数值** | 无法可靠测量 RSS；改为可判定规则：必须流式处理 ACP 消息与 stderr，不得缓存完整会话正文，超限即报错并结束该 Agent |

  这些常量落在 `agent-host`，并由 `agent-host` 的 fake ACP child 测试固定（测试要求见 `docs/MODULE_ARCHITECTURE.md` §11）；需要随部署变化的只有 `sessions.idle_timeout_ms`。
- Windows 使用 Job Object 或等价机制清理完整子进程树。
- Agent 崩溃、乱序或非法 JSON 形成明确事件，不能使 Daemon 接受伪造客户端身份。

### 12.3 Workspace

- 原始 workspace 路径的**登记**只能由 Owner Node 本地管理入口完成（成功的会话创建（目录自动记录））。设备侧另有一条受限入口：Sync 面的 `session.create` 可以携带客户端提交的绝对目录；它必须先通过本节的路径校验与规范化，且不得被当作「别名」登记（见 §10.2 与 `SYNC_PROTOCOL.md` §11.5）。
- 跨节点远程主体只能使用本机已登记的目录与配置声明的 Agent，不能构造任意路径或修改 Agent 沙箱。
- 文件路径展示和下载必须重新检查其属于会话授权边界；不能只依赖客户端传来的 path。
- 规范化路径后再比较，处理 symlink、junction、大小写、UNC 和 `..`。可判定规则（权威实现归 core，见 `CORE_PORTS_AND_STORAGE.md` §11.9）：只接受**绝对路径**；写入与使用前都必须存在且是**目录**；`canonicalize` 后的结果才是持久权威值；拒绝相对路径与含 `..` 的输入；UNC/网络路径允许但记一次结构化警告；路径与规范化结果 MUST NOT 出现在事件、错误 `details`、日志或审计记录里，也不得出现在下面两处例外之外的任何 `command.result` 里。例外**只有两处**：一是快照的 `workspaces` 资源条目——已认证设备在那里收到**已记录**目录的规范化权威值（下发形式是剥掉平台 verbatim 前缀后的规范化结果）；二是只读目录浏览 `workspace.browse` 的 `command.result`——协商了 `core.workspace-browse.v1` 的设备在那里收到被请求那一层的直接子目录绝对路径与逐字回显的父路径（§10.2）。口径是「不得泄漏**客户端尚未知道的**宿主路径」：客户端自己提交的目录因此不构成泄漏，但它同样 MUST NOT 被回显为对端可见字段，也不会被登记为别名。`workspace.browse`（§10.2）只返回它必然要给客户端的那一层条目（名称、绝对路径、是否已登记），不得借该命令泄漏其它层的路径，且不产生登记、缓存或审计副作用。
- Agent 的实际文件/命令权限仍由 Agent sandbox 和用户配置控制；ACP Remote 不能虚报更严格隔离。

### 12.4 会话文件读取与 Git 查询

`[决定]` 认证设备可以只读浏览当前本机会话工作目录（`workspace_cwd`）内的文件、预览文本/图片并主动下载；这不是新的授权维度，命令仍走 Sync 既有认证与 feature 协商。实现边界：

- **根边界按实际对象判定**：core 只接受根内相对路径（拒绝绝对路径、盘符、UNC、`..`、NUL、ADS 冒号与 `.git` 管理路径）；原生适配器（`crates/app/src/session_files/`）必须重新规范化根与目标，并以**实际打开句柄的身份**确认对象仍在校验时的根内位置——Unix 比较 `dev+ino`，Windows 比较创建/修改时间、大小与属性（std 的 `file_index` 尚不稳定且仓库禁止 `unsafe`）。校验后被替换的链接、根外 symlink/junction、指向自身或祖先的循环链接一律拒绝；根外链接只显示名称，不下发目标绝对路径。目录枚举还逐项复核**非链接子项的规范父目录等于已校验目录**，枚举期间目录被替换成根外 junction 再换回时，无法证明归属的名称被跳过并计数，不当作本目录条目返回。`.` / `.GIT` 任一分量永久隐藏且不可读；Windows 设备保留名（`CON`/`NUL`/`COM1` 等）在打开前拒绝。
- **只读普通文件**：只读取普通文件；命名管道、设备、套接字在列表里标记不可打开，读取直接拒绝且不得挂起（Unix 先按文件类型判定）。
- **正文保护**：预览与下载复用已认证 Sync 查询，不生成 bearer URL。预览只取得固定预算（文本 256 KiB / 栅格图片 4 MiB）并如实给出原始大小与截断标记；下载超过 64 MiB 返回 `resource.result_too_large`，不静默截断。内容快照只在内存中、绑定连接期版本，单块不超过 32 KiB；每块发出前重新检查源文件身份，变化即废弃快照。快照总量与条数有界，连接断开/取消/撤销/空闲 60 秒都经同一释放入口，**不写临时正文文件、不写日志**。
- **Git 是可选运行依赖**：Git status/diff 通过受管理的本机 `git` 子进程执行（Windows Job Object / Unix 进程组，10 秒超时，超时/取消/断连结束整棵进程树），固定参数、不经 shell、用户路径只作为 `:(literal)` pathspec。查询使用 `--no-optional-locks`/`GIT_OPTIONAL_LOCKS=0`，不写索引或工作树，不调用 checkout/clean/add/reset；禁用 external diff、textconv、fsmonitor、pager、hooks 与仓库全部有效配置层（含 `extensions.worktreeConfig` 的 `.git/config.worktree` 与 `include.path` 展开）声明的 `filter.*` 命令（含 `.gitattributes` 里 `filter=` 指向的空驱动名；驱动名无法按 UTF-8 无损解码、含 `=` 或含控制字符时——这些名字无法生成精确的覆盖键——整个枚举失败关闭）；枚举失败即报告 Git 不可用，不执行未识别的仓库命令。Git 缺失或不是仓库时分别返回 `gitState = unavailable` / `not_repository`，文件浏览照常；忽略过滤不可用时显式降级，不声称已过滤。
- **版本与错误**：文件内容、版本令牌与 Git stderr 原文不进入日志或协议错误；预览/快照过期返回明确不可用，不复用会话 version conflict 语义。

`[实现现状]` 原生只读适配器已实现并有 Windows 真实 junction/替换/Job Object 与临时仓库测试；Sync 入站门控与组合根接线完成前，设备端尚不能调用这四条查询（feature 未宣告）。

### 12.5 附件传输与媒体读取（服务端引擎、连接面与组合根端点已实现）

附件与媒体遵守与业务连接相同的身份、撤销与主机边界，但不把正文塞进业务消息：

- **独立认证**：附件连接（Sync `/sync/v1/attachments`、Node Link `/node-link/v1/attachments`）必须重新完成对应协议的 challenge-response、Origin/Host 与撤销检查；不得复用业务连接的认证状态，也不得引入长期 bearer 或公开下载 URL。认证完成后按 `selectedFeatures` 门控：未协商对应 feature 的附件帧回 `attachment.negotiation_required`/`nodelink.attachment.negotiation_required` 并以 `4400` 关闭，**门控先于存储调用**（未协商的连接不产生任何写入）。
- **逐帧绑定**：每一帧校验连接身份、`transferId`、会话与主体（设备/节点）；跨会话、跨主体或跨设备标识一律拒绝且不泄露内容。主体取自已认证连接的身份，**不采信**帧里自报的 `subject`；读取同样先鉴权再返回字节，存储层再核一次会话绑定。清单页（`manifest_page`）同样逐条比对会话与主体，整页拒绝而不是部分登记。
- **prompt 绑定**：`session.prompt` 的 `attachment_ref` 不是「随便贴一个附件标识」：服务端在派发 Agent 之前要求本连接协商了附件 feature、携带已封存的 `submissionId`，并逐块比对清单里的 `attachmentId`/`sha256`/`byteLength`（`sha256`/`byteLength` 以已封存传输行为准，客户端自报值只是声明）。任一不符返回 `attachment.binding_mismatch`（或形状错误），既不泄露附件是否存在，也**不触发任何 Agent 执行**；`submissionId` 在 Node Link 面同样必须进入整批绑定入口，不能被静默丢弃。
- **内容校验**：校验声明/实际大小、单块上限、期望偏移、总长度与整段流式 SHA-256；摘要不符或重复位置内容不同即拒绝。清单摘要按清单顺序的 `transferId` 复算（`SHA-256( Σ u32be(len) || id )`），不信任对端给出的摘要。类型按声明检查，渲染仍按不可信内容处理（HTML/SVG/可执行不活动渲染）。
- **资源与失败关闭**：固定 32 KiB 分块、每传输一块未确认、单设备 2/单节点 8 个活动传输、每连接 6000 帧/60 秒（固定窗口）；发送预算与速率按附件族独立计数，不占业务命令配额。活动传输的预算检查与槽位预留在**同一临界区**完成（否则并发 `begin` 会突破上限），并在附件连接关闭时按连接释放该连接预留的槽位——拒绝服务不可由「开两个上传再断线」实现。磁盘不足映射为可重试的 `attachment.budget_exhausted`，I/O/后端故障映射为 `internal.unavailable`（可重试）；都显式失败，不截断冒充完整内容，也不阻塞事件投递。帧预算超限只拒绝该帧，不关闭连接。EOF 处的重复读取与 0 字节附件回显式形状错误，不会出现「已认领但无回帧」的挂起请求。
- **临时内容**：上传先落临时文件，封存校验通过后才允许引用；孤儿文件由启动回收清理。未提交上传 24 小时闲置过期；最近活动不足窗口的不清理。
- **撤销与保留**：设备/节点撤销立即中止上传与读取：`close_device`/`close_node` 同时扫描业务表与**附件连接表**并请求关闭（`4410`），附件连接不占业务连接槽位，但撤销绝不漏掉它。已提交附件不因撤销被删除，按消息保留策略处理。
- **Access 无正文**：Access 只做有界内存转发，不把远程附件写入临时目录、SQLite、日志或持久缓存；Owner 是唯一权威。实现现状（2026-10-20，`conversation-area-complete` WP5）：Owner 入站侧与 `node-link-client::attachment::AttachmentForwarder` 均已落地（停等、单块 32 KiB、断线/撤销显式失败、无临时文件）；Access 侧配对（`app::access::AccessManager` 的 HTTPS claim/status）与 Daemon 启动时的 Owner 连接实例化已接线，转发由 `AccessManager::open_attachment` 提供，使用同一出站信任锚与独立认证连接。`revoke`/`shutdown_all` 通过 `SharedLink` 的关闭通道立即中止该 Owner 的在途上传与读取（不需要等 5 秒兜底，也不依赖对端先断开）。当前明确未接线的只有 `server::acp_facade`（把本地 Sync/ACP 的 mutation 转发给 Owner 的那一层），因此「本地客户端经 Sync 触发远程附件」仍不可用；库级与 `AccessManager` 级路径已可用。
- **外部 URI**：只有外部 URI 的资源显示链接与「未获取」，客户端与服务端都不得因展示而自动下载。
- **接线现状（安全相关的可用性事实，2026-10-20 WP5）**：两个附件端点由组合根在装配附件传输端口时注册到共享 listener；未装配存储时既不注册端点、也不宣告 `core.attachment-transfer.v1`/`node-link.attachment-transfer.v1`（宣告必然 404 的端点就是能力虚报）。端点本身沿用既有握手、Origin/Host、撤销与心跳边界；附件连接不进入业务连接表，撤销按 `4410` 同时关闭两类连接。
- 机器合同：`schemas/{sync,node-link}/v1/attachment.schema.json`、`fixtures/` 的正反样例与 `compatibility/commands/v1/commands.json` 的 `transportFrames`。

### 12.6 ACP 终端服务（已实现，`conversation-area-complete` WP3）

Agent 可以通过 ACP 的 `terminal/create`/`output`/`wait_for_exit`/`kill`/`release` 请求 Daemon 在**宿主用户权限下**执行命令。这不是新增授权维度（ACP 原生能力），但必须按不可信输入处理：

- **能力与执行同一判据**：只有组合根把 `clientCapabilities.terminal = true` 接进 `initialize` 时才接受 `terminal/*`；未宣告时按「方法未找到」拒绝，且**不启动任何进程**。反过来，宣告了就说明服务已完整接线（启动、输出、等待、停止、释放与清理都可工作）。
- **会话绑定**：终端标识只存在于所属会话的服务里；另一个会话拿同一个标识只会得到「未知终端」，不泄露输出也不影响持有它的任务。`terminal/*` 的 `sessionId` 必须与会话一致。
- **命令形状**：命令与参数按**数组**直接 spawn，不经 shell 拼接；stdin 关闭（不提供交互终端或 PTY，页面也不能手动输入命令）。
- **工作目录**：缺省为会话工作目录；显式 `cwd` 必须是**已存在目录**，且 `canonicalize` 后仍位于会话工作目录内（挡 `..`、链接/junction 逃逸）。路径不进入日志或错误响应。
- **环境**：先 `env_clear()`，再注入 `PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`，最后叠加 Agent 显式给出的变量；变量名按 §12.2 的保留前缀（`ACPR_`/`ACP_REMOTE_`）静态拒绝。节点/设备密钥不在 Daemon 进程环境里，也不注入。
- **输出**：stdout/stderr 分开读取、按实际接收次序合并保留；UTF-8 跨读取边界由增量解码器拼接（非法字节用替换字符消费，不静默丢弃）。保留正文受 §12.2 的固定上限与 Agent `outputByteLimit` 的较小值约束，从开头丢弃并落在字符边界；实时事件不截断。`terminal/output` 的**响应**还必须留在 ACP 单条消息上限内（按 JSON-RPC 信封与 JSON 转义后的**实际**字节计算，换行与 ANSI 序列都会膨胀），放不下时再次从开头裁剪并同样置 `truncated = true`；响应发送失败必须显式回 `-32603`（或已知的会话/进程错误），不能让 Agent 的请求悬挂。输出内容从不进日志。
- **生命周期与进程树**：每个终端有独立的 Job Object（Windows，`KILL_ON_JOB_CLOSE`）或进程组（Unix），与 Agent 自己的 Job 分离。会话关闭、Agent 崩溃与 Daemon 关闭的顺序是：停止接受新请求 → 唤醒 `wait_for_exit` 等待者（返回明确错误，不悬挂）→ 结束整棵终端进程树 → 排空已读输出 → 结束驱动任务。同步关闭排空出的终端必须保留给随后的异步关闭，否则标准关闭路径上的「排空已读输出」会变成空操作；`create` 的关闭判定、并发上限与登记必须在同一临界区完成（先登记占位再 spawn；关闭在 spawn 期间发生时要结束刚启动的树），保证关闭不会漏掉已经 spawn 的终端。kill 只结束进程树并保留标识（之后仍可读输出与退出事实）；release 结束未退出任务并移除标识，之后所有操作都返回「未知终端」；kill 与自然退出竞争只记录一个退出事实。Unix 被信号终止时退出事实填信号名（如 `SIGKILL`），Windows 填退出码；两边都取不到时给确定性的非成功退出码。
- **失败语义**：未知/已释放终端、会话已关闭、环境变量名不合法与启动失败分别返回可区分的 JSON-RPC 错误；响应只包含固定文案，不回显命令行、参数、环境值或路径。

### 12.7 本机资源回收与查看租约（计划，`session-activation-lifecycle` WP1 合同；实现待 WP2–WP5）

本机生命周期让「无人查看」成为可回收的条件，因此必须防止**伪造保活**与**挤占容量**：

- **查看声明只属于当前连接**：`session.view`/`session.view.result` 只在已认证、未撤销且协商了 `core.local-session-lifecycle.v1` 的连接上合法；每连接最多一个当前查看会话，`viewVersion` 单调且同版本重试幂等，过期版本不复活旧页面。租期 30 秒、可见页面每 10 秒续租，普通 `control.ping` 与心跳不续租——连接在线不是保活理由。
- **不持久化、不泄露**：查看者、租约与资源代只存在于内存（`CORE_PORTS_AND_STORAGE.md` §5.10），不写 SQLite、不进快照、事件、错误 `details`、日志或审计；断开、撤销与连接被替换立即清除（§9.5）。
- **容量与释放必须真实**：容量准入按 owned 会话的真实运行资源计数（含待处理、激活中与回收中），只有 Agent 适配器确认释放（真实 ACP `session/close` 成功、端点与映射清理完成）后才交还名额。关闭超时、被拒或结果不确定保留占位并报「回收未完成」，MUST NOT 只改本地标志或展示状态就宣称释放。同 Agent 的其他会话不受影响；只有全部会话都可回收时才允许停止共享进程。
- **输入与错误边界**：控制帧受 schema、字段数、单帧大小、`viewVersion` 范围（0..=`2^53−1`）、会话归属与频率（120 帧/分钟，独立于业务命令）校验，且门控先于任何存储或 Agent 调用；拒绝使用既有 `protocol.schema_invalid`/`protocol.type_unsupported`/`resource.rate_limited`/`session.not_found`，容量拒绝用 `session.capacity_exceeded`（`retryable = true`）。错误与日志不含密钥、正文、路径或任意会话内容。
- **配置边界**：显式 `sessions.idle_timeout_ms = 0` 只关闭按时间回收，容量保护仍然生效；`sessions.max_active_sessions` 必须是正整数（无「无限」取值），运行期调低上限不强杀受保护会话，只拒绝新增直到计数回落（`CONFIG_REFERENCE.md` §4）。

### 12.8 准备凭据与临时资源（`web-session-preparation`；`identity-auth` 签名/验证边界已实现，服务端帧/存储/临时资源待 WP2–WP5）

准备凭据让 Owner 证明「某个 `sessionId` 是我分配的、并绑定到这个设备/标签页/Agent/目录」，它**不是**一种新的认证方式：

- **不是 bearer token**：receipt 不替代每条 WSS 的 challenge-response，不授予 scope，不带来跨设备/跨会话访问，不读取其他标签页草稿或任意目录，也不能恢复已提交会话或旧存储代。任何把 receipt 当作登录凭证、缓存成长期凭据或写进 URL 的做法都违反本节。`identity-auth` 的验证入口强制要求当前连接的 `Authenticated`（来自 §5 握手）与最新设备凭据状态；返回值 `PreparationAdmission` 只有私有字段与 `session_id()`，不携带授权、scope、端点或密钥。
- **签名与验证边界（已实现）**：claims 是闭合的 10 个 wire 字段（`version`/`ownerNodeId`/`storeEpoch`/`origin`/`deviceId`/`tabNonce`/`sessionId`/`agentId`/`normalizedWorkspacePath`/`workspaceSource`，[`SYNC_PROTOCOL.md`](./SYNC_PROTOCOL.md) §6.3），签名域是专用 transcript `acp-remote/session-preparation/v1`（P-256 P1363 64 字节 + 无填充 base64url）。签发只经 `Authority::sign_preparation_receipt` → `IdentityKeystore::sign(KeyPurpose::NodeIdentity)`；验签只经纯函数 `verify_preparation_receipt` / `Authority::verify_preparation_receipt`。不新增「任意字节签名」入口，不接受调用方自选 domain，也不复用设备认证的签名域；`identity-auth` 不持私钥、不做平台 IO。
- **验证顺序（已实现）**：`verify_preparation_receipt` 固定为 **当前连接认证与撤销 → 专用 domain 签名 → claims 绑定**。`Revoked`/`Unknown`/`ScopeReduced` 在验签之前失败（分别映射既有认证/关闭规则，不返回业务错误）；签名结构/编码失败与密码学失败分类不同；之后才比较 `ownerNodeId`、当前已认证 `deviceId`、连接 canonical origin、持久 `storeEpoch`、`sessionId`、`tabNonce`、`agentId`、规范化目录与来源。跨设备/跨节点/跨标签页/跨会话一律拒绝；本地 `PreparationBinding` 只服务决策与测试。`PreparationAdmission` 在全部检查通过后才可构造，资源查询/重建/启动入口应以它为前提（服务端接线待 WP5）。
- **绑定与拒绝（已实现 + 服务端映射待 WP5）**：`storeEpoch` 取持久 `meta.server_epoch`（正常重启不变，整库重建时改变），`origin` 是 canonical origin，`tabNonce` 是 32 字节随机标签页归属（不进入 URL）。伪造签名、跨设备/跨会话/跨标签页、`subject` 与连接不符、同 ID 不同声明、附件连接未重绑、混用正式与准备资源，统一返回 `attachment.binding_mismatch` / `{}`（控制面用已登记业务错误），不泄露目标存在性，也不指出哪个签名字段失败。
- **存储重建**：receipt 的 `storeEpoch` 与当前持久值不一致时，`identity-auth` 返回独立的 `PreparationError::StoreRebuilt`；服务端 MUST 映射为不可重试的 `preparation.store_rebuilt`（`details: {}`），客户端丢弃 receipt 并认证后回目录重新分配；不得重放旧 receipt、不得沿用旧临时端点，也不得把这种情况报成 `preparation.stale`。
- **重放边界**：receipt 没有 `consumed` 状态、不写墓碑，也不是可兑换的一次性令牌。重放最多证明「同一未撤销设备、同一 origin、同一标签页/会话绑定仍成立」，不会产生新的正式会话；对正式 ID 的重放由 WP2 的持久首条受理记录与唯一约束挡住（尚未实现）。正常进程重启（同一 `storeEpoch`）receipt 继续有效；整库重建使其失效；节点身份材料变化按 §9.4 进入 `identity_changed`，不得用新密钥静默重签成同一 `sessionId`。已正式受理的 `committed` 优先于运行期过代/租约/目录变化仍由服务端在验签与绑定之后判定。
- **脱敏**：`PreparationReceipt` 与 `PreparationClaims` 的 `Debug` 只输出固定脱敏标记，不打印 claims 值（目录路径、origin、设备/会话/节点标识、`storeEpoch`）或签名内容；receipt 不含私钥、认证材料、正文、附件或 Provider/MCP 凭据；`SecretBytes` 仍不实现 `Debug`/`Serialize`、`KeyHandle` 的 `Debug` 保持不透明。claims 含本机规范化目录，调用方 MUST NOT 把整个 receipt 原样写入日志/审计（路径禁入日志，§14.1）。`PreparationError` 的 `Display`/`Debug` 只带固定文案与字段类别，不带 claims 值、路径、签名内容或密钥材料；adapter MUST NOT 把本地 `PreparationBinding` 原样写入日志、错误 `message`/`details` 或审计。
- **验证顺序与撤销优先级（服务端，计划）**：服务端在任何凭据读取或临时资源访问前先确认当前连接认证与最新撤销状态；之后才验签与 claims 绑定，再查首条持久受理，最后核对运行期资源代/租约与目录/Agent 现状。已撤销设备立即中止其准备控制帧、附件准备上传与临时端点（附件连接按 `4410` 关闭）。
- **临时资源与孤儿清理（计划）**：准备目录、租约、资源代、临时端点与暂存传输只存在于运行期（[`CORE_PORTS_AND_STORAGE.md`](./CORE_PORTS_AND_STORAGE.md) §5.11）；不写带 `owned_session` 外键的行，不产生正式会话/命令/事件。临时资源计入 `sessions.max_active_sessions` 容量，租约 30 秒、可见页面每 10 秒续租（[`SYNC_PROTOCOL.md`](./SYNC_PROTOCOL.md) §14）。未封存或已封存待认领的暂存字节由启动回收与 24 小时闲置回收清理；活动传输与已受理消息引用的字节不得被误删。回收只删除无引用孤儿，且不写日志正文、路径或摘要之外的敏感字段。
- **不重传、不复制（计划）**：首次提交只认领已封存字节的元数据与引用（design D6.4）；转正不得再次全量上传、复制文件或重新计算整份摘要。必要的存在性/权限复核不在此限。

## 13. 持久化与本地数据保护

### 13.1 第一阶段静态数据决策

第一阶段不自行实现 SQLite 全库加密，也不引入自定义透明加密层：

- 依赖 OS 用户账户隔离、目录 ACL 和用户启用的 BitLocker/FileVault/LUKS 等磁盘保护。
- 文档和 CLI 应建议在存放敏感工程的电脑上启用全盘加密。
- Provider credential、Node private key 和 pairing secret 不进入普通 SQLite 字段。
- `[决定]`（2026-10-17，`agent-declaration-from-config`）**删除 Provider 凭据管理**：凭据由 Agent 自己持有；本系统不再保存、解析或注入 Provider 凭据，平台 keystore 只承载节点/设备身份密钥。
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
- 大型终端、diff、附件和 raw payload 使用明确配额及 `truncated/rawUnavailable` 标识；附件使用 32 KiB 分块与并发/速率保护，不设附件数量/大小产品额度，但不以分块绕过磁盘与会话容量上限。
- 删除是逻辑/文件系统删除，不宣称擦除 SSD、备份或快照。

### 13.4 客户端与 Access Node 数据最小化

- 第一阶段固定为 `no-content-cache`：Access Node 和受其服务的远程客户端默认不持久化会话正文，只允许有界内存转发。
- Access Node 可以把 imported 资源交付给它自己的 Sync 客户端（浏览器 PWA），但只允许按 `SYNC_PROTOCOL.md` §9.6–§9.7 的 `resource.remote-origin.v1`/`resource.stateless-origin.v1` 转发：不改写 ACP 语义与 `acp.rawJson`、不缓存正文、不扩张 capability、不改会话状态；快照只含元数据，正文历史一律在线回源 Owner，离线时返回 `resource.remote_unavailable` 并置 `origin.online=false`。运行期 ACK 只表示当前连接的流控与处理进度，不是可恢复索引。
- Access Node **不得持久化** Owner 的 import/来源引用、cursor/ACK、requestId、命令终态引用、event type/digest 或 local sequence 映射，也不得持久化任何可还原 prompt、回复、diff、终端、附件或 ACP raw 的内容。允许保留的只有连接与信任资料（节点/设备身份、公钥指纹、地址与协议配置、本机信任/撤销决定、连接重试配置）以及摘要不含远程内容的连接身份审计（`SECURITY_DESIGN.md` §14.2 的动作类别、时间、actor、目标节点 ID 与固定结果类别）。
向第三方客户端交付正文必须被视为显式数据披露
- 撤销设备或节点后 Owner 不再提供数据，Access 清除该来源的运行期模型与内存内容（不存在需要删除的持久索引）。
- 客户端收到自己被撤销或用户执行“清除此设备”时，应删除缓存和 device key；浏览器能力不足时明确说明残留风险。
- **准备材料的数据最小化（浏览器侧已实现；Owner 侧仍按计划）**：准备凭据只在浏览器内随准备草稿持有（窄例外与实现边界见 §11.4），不写日志、不进 URL、不进入普通 SQLite 字段；Owner 侧对准备阶段**不落任何行**（core/storage 已在 WP2 落地，服务端接线待 WP5）——准备目录/租约/资源代与暂存传输都是运行期状态，正式记录只由首次受理事务产生（[`CORE_PORTS_AND_STORAGE.md`](./CORE_PORTS_AND_STORAGE.md) §7.3）。Access Node 不接触准备凭据：它既不签发、不验证，也不缓存，准备帧只在 Sync 面由 Owner 处理。
- 未来启用加密离线正文缓存时，必须新增协商 feature、Owner 明示授权、容量/TTL/撤销提示和独立 ADR；Owner 仍不得声称能远程可靠擦除已有副本。
- 与 Daemon 同包发布的 PWA 静态资源属 §3 的 `public-artifact` 类别：根路径静态面只交付这些公开构建产物（入口文档与前端资源），不交付任何会话内容、凭据或用户数据，因此未认证的请求也能取到它。同包发布、构建版本可检查与发布构建缺产物即拒绝启动见 §16.3。

## 14. 日志、审计与崩溃信息

### 14.1 默认日志允许字段

- 稳定但非秘密的 correlation ID、event ID、request ID。
- 事件/命令类型、状态、耗时、大小和错误码。
- 截断并脱敏的 Agent executable 名称，不记录完整参数中的 secret。
- 命令级日志的固定字段（每个被接受的业务命令与它的终态各一条，**至少包含**）：主体标识（哪个设备或节点在控制，取设备标识或节点标识）、命令名、资源标识（如被操作的会话标识）、请求标识、终态类别（`completed`/`failed`/`uncertain`）与失败成因类别；受理那一条另带适配器侧的 turn 标识。主体标识是必需项——「当前由谁在控制」必须能从日志字段直接看出，不靠推断；成因类别取**封闭词表**，不得写入未登记的取值，也不得把路径、凭据或 prompt 正文塞进该字段。

默认禁止：

- 完整 prompt、回复、ACP `rawJson`、终端、diff 和文件内容。
- Node/Device private key、pairing secret、pairing code、扫码取回凭据（`qrToken`）、HMAC、签名输入、Provider token。
- 完整敏感路径、URL fragment、Authorization/Cookie 和 QR payload。

### 14.2 审计事件

至少记录不含内容的安全审计元数据：

```text
pairing.created / claimed / approved / rejected / expired
device.authenticated / auth_failed / revoked / scopes_changed   # scopes_changed 为历史取值，本版本不再产生（见下方 [决定]）
node.paired / node.authenticated / node.auth_failed / node.trust_revoked / node.identity_changed
（历史只读取值）export.created / revoked
（历史只读取值）import.added / removed
provider.configured
authorization.denied
rate_limit.triggered
storage.integrity_failed
```

`[决定]`（2026-10-16，`pairing-model` 变更）`device.scopes_changed` 标注为**只读的历史取值**：设备不再有授权集合（删除了设备权限层，设备通过认证即可执行命令），本版本**不再产生**该动作。保留它是因为升级前的库可能已有这类审计行——两张审计表的 `action` CHECK 仍接纳它，收缩取值会让历史行写不回去、读不出来（`CORE_PORTS_AND_STORAGE.md` §7.2 的 v8 段与 §11.6 第 1 条）。

`[决定]`（2026-09-26）新增 `node.authenticated`/`node.auth_failed` 两类：节点握手是安全动作，成功与失败都要留痕，而此前节点侧只有 `node.paired`/`node.trust_revoked`/`node.identity_changed`，握手的成败只能靠误用其它取值表达。首次认证把已批准配对推进到 `consumed` 时**不另设动作**——那次转移随该写集记录 `node.authenticated`（设备为 `device.authenticated`）；配对的认领方也会出现在审计里（`actor_kind = 'pairing_claimant'`），落库时两张审计表的 `actor_kind`/`action` CHECK 必须同步扩宽（12-step 表重建，见 `CORE_PORTS_AND_STORAGE.md` §7.2/§11.8）。

`[决定]`（2026-09-23）新增 `export.created`/`export.revoked`/`import.added`/`import.removed`/`provider.configured` 五类历史动作；`provider.configured` 在 v10 后不再产生但保留为只读历史取值。落库时 `owned_audit`/`imported_audit` 的 CHECK 要同步改，而 SQLite 不能修改现有 CHECK，必须走 12-step 表重建（见 `CORE_PORTS_AND_STORAGE.md` §11.8）。

审计日志不能成为第二份聊天记录。失败原因对本地日志可以比远程错误更详细，但仍不得包含 secret。

### 14.3 崩溃

- panic/error report 不包含敏感 DTO 的 `Debug` 全量输出。
- release 构建不得自动上传 crash dump 或 telemetry。
- OS crash dump 可能包含内存敏感数据，这是已知风险；文档应说明如何按平台禁用或保护 dump。

## 15. 资源滥用与失败安全

- 在分配大对象前应用 Sync Protocol 的认证前、单消息和数组/深度限制。
- 按 IP、device、command 和 pairing ID 限流；限流状态有界且会过期。Sync 新连接认证按来源 IP 固定为 100 次/分钟，详见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14；配对接口与 Node Link 独立计数。
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
- Node mismatch、Origin mismatch、错误 device/node、设备撤销与节点撤销。
- 节点循环、撤销售后与 Node Link 重放。
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
| 已记录目录的规范化路径对被配对设备可见；目录浏览构成有限的目录结构探测面 | 目录页与创建选择器需要路径才能做同名消歧与展示，属产品明确要求的展示能力；任一被配对设备本就可见本节点全部会话与事件 | 路径可见面**只有两处**且各自 feature 门控（快照 `workspaces` 条目的规范化路径始终随目录标识下发；`workspace.browse` 的 `command.result` 只在协商 `core.workspace-browse.v1` 时返回该层条目；未协商的一律不下发/拒绝且不触碰文件系统）+ 浏览只读且只返回一层、不返回文件与任何文件元数据、单层条目上限、复用命令族限流（120/分钟）、不产生审计动作（避免第二条"谁看了什么"的隐私面）+ 这两处之外的一切输出（事件、日志、审计记录、错误 `details`）仍不含任何路径；决策见 [ADR-0009](./adr/0009-authenticated-workspace-path-and-browse.md) |

## 20. 编码前仍需收口的实现选择

以下选择不能由普通实现补丁静默决定：

- Linux（没有可用的 D-Bus Secret Service，例如无桌面会话或容器）上是否提供经过审计的持久化 fallback，推迟到 Linux 平台开发阶段处理；当前优先交付 Windows，见 `INITIAL_DESIGN.md` §14。在决定前 Linux 正式模式仍失败关闭，不引入明文 fallback。该决定只影响 `identity-keystore`：`identity-auth` 的 keystore 端口必须允许非硬件保护的实现存在，但默认不启用（[ADR-0006](./adr/0006-identity-keystore-split.md) 决策 5）。
  - 2026-09-23 定案：**本阶段维持失败关闭**，不提供持久化 fallback；将来若要提供，必须新开 ADR 定 KDF、解锁来源与备份策略（§9.2），并同步 `CONFIG_REFERENCE.md` §8 与本节。
- 是否以及何时把密钥存储提升到**硬件保护的不可导出档位**（Windows CNG/TPM、Linux 除 Secret Service 以外的系统 keystore）：需要单独 ADR，因为它会改变已接受的「非硬件保护实现允许存在、默认不启用」边界（ADR-0006 决策 5）。当前档位见 §9.2（Windows 用 DPAPI 包裹 + 进程内签名）。注意 `unsafe_code = "forbid"` 下交付该档位需要自写或引入 wrappable 的 FFI 封装，成本与风险必须在 ADR 里评估。
- `identity-keystore` 与 `agent-host` 的平台包装 crate 选型（DPAPI wrapper、Windows Job Object wrapper 等）：候选必须按 `AGENTS.md` §8 核验维护状态、许可证、平台支持与安全风险，并确认不超出 `deny.toml` 的许可证 allow 列表与 `[graph] targets`；若候选的 **MSRV 高于 `Cargo.toml` 的 `rust-version`（1.87）**，必须先单独做一个「是否抬 MSRV」的决定（工具链版本 `rust-toolchain.toml` 是另一件事，不能代替 MSRV 决策）。选型结论必须写回 `MODULE_ARCHITECTURE.md` §4.5/§4.12。
- npm provenance、checksum 签名和 SBOM 的发布工作流与格式仍待发布阶段确定。普通 CI 已接入 GitHub Actions（`.github/workflows/ci.yml`），执行合同门禁、Rust 检查、提交规范校验、依赖许可证与来源判定、依赖安全公告、密钥扫描及文档引用检查；已有普通 CI 不代表发布 provenance、签名或 SBOM 已实现。发布 job 继续按 §16.2 与普通 CI 分离。
- release crash dump 的平台默认策略。
- 是否以及何时通过 ADR 引入 SQLCipher、字段加密或 Noise Transport Profile。

本地管理通道已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定（stdio + 平台本地 IPC，不实现 loopback HTTP 管理面）。

这些选择不阻塞领域模型和 Sync DTO 开发，但相关平台 adapter 或正式发布在选择落定前不能声称安全完成。

## 独立目录创建（ADR-0010）

独立建目录沿用已认证设备能力面，不新增设备授权维度；仅 Sync 协商 core.workspace-mkdir.v1 后可执行，不开放 Node Link 任意路径。终态、错误与审计不泄漏路径或名字；崩溃窗口不自动重做。安全决定见 [ADR-0010](adr/0010-immediate-workspace-directory-creation.md)，事务权威见 CORE_PORTS_AND_STORAGE §5.6。
