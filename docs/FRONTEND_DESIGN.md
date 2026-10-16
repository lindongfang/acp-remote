# ACP Remote 前端设计

> 状态：编码前客户端约束  
> 版本：0.5
> 修订记录（2026-10-16，`node-link-pair-route`）：§4.1 登记新增路由 `/node-link/pair`（与 `/pair` 共用同一入口主体）与「两类配对入口互不混淆」的口径，并记下该路径与协议保留前缀的关系；§9 追加两条入口区分用例。  
> 修订记录（2026-10-16，`pairing-origin-secure-context`）：§4.5 把「来源与 secure context 校验尚未实现」改写成实绩——配对入口在读载荷之后、发任何请求之前依次校验安全上下文、来源一致与载荷类型，环境事实经组合根注入，失败码为 `auth.origin_mismatch` / 新增的 `auth.insecure_context`。  
> 修订记录（2026-10-16，`frontend-sync-wiring`）：§4.1 写回「主机连接与首次配对」的实绩与两处未闭合边界；§4.5 写回配对秘密的清理口径与尚未实现的来源校验；§5 补阻断态的封闭成因词表与「与普通断网可分、阻断后结构性停止自动重连」；§9 写回前端级验收证据，并如实登记尚未执行的真实 Daemon 浏览器级端到端验收。  
> 修订记录（2026-10-15，`sync-ui-hosting` WP4）：§4.3 把分发图示的托管位置由 `/ui` 改为**根目录**，写回「静态资源嵌入二进制」的裁定、理由与代价（含发布构建缺产物即拒绝启动）；§11 的「静态资源嵌入二进制还是作为旁路资源安装」待决项按该节的结项规则结项，其余条目不变。  
> 修订记录（2026-10-12）：§4.1 补对话页口径（时间线与结构化卡片、输入与提交、取消、授权与提问应答的可用边界、模型与模式、变更文件右栏边界、宽窄两形态）；§6 明确「呈现覆盖由客户端自持」——协议的事件类型词表只决定解析，客户端自己的覆盖集合决定呈现，未被覆盖的已知事件一律走降级卡片，没有第三种结果。  
> 修订记录（2026-10-02）：§2.2 与 §9 第 1 条把「PWA 不展示会话创建」改为受限的目录内创建入口（feature `core.session-create.v1`），并记录原型来源；§4.1 新增目录浏览与目录内创建会话；§7 把 workspace/Agent 目录引用纳入可缓存的最小数据（只存引用）。  
> 修订记录（2026-09-18）：明确 PWA 本地缓存为固定常量（摘要缓存 8 MiB、TTL 30 天、LRU），imported 正文不占用配额。  
> 日期：2026-10-16（2026-09-18 基线）
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
- 第一阶段 PWA 在本机目录内提供受限的会话创建入口：`session.create` 的选择器来自本机已登记的 workspace 目录与已配置的 Agent（feature `core.local-catalog.v1` 的快照资源供数，离线也可渲染），提交只带 `{ workspaceAlias, agentId }` 两个引用（feature `core.session-create.v1`）。客户端**不**展示也不构造规范化路径、目录追加、MCP 配置或凭据；Node Link 侧 `session.create` 额外要求 Owner 导出的 workspace template，PWA 不使用这条形状。
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
   │  ├─ protocol/                生成或显式定义的协议 DTO（Sync 全套 + node-link 的配对二维码载荷）
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

  `[实现现状]`（2026-10-16，`frontend-sync-wiring`）连接与首次配对已按本变更接线闭合，实测来自 `clients/app` 的独立门禁（`cd clients/app && npm run check` = 59 files / 725 passed + `expo export` 成功）与假主机 e2e（`node e2e/run-e2e.mjs` = 51/51，既有 49 条 + PW-1、PW-2）：

  - 出站地址由组合根 `syncSocketUrl` **唯一**构造，路径为权威端点 `/sync/v1`（`src/sync-client/endpoint-contract.test.ts` 用字面量断言 `new URL(...).pathname === "/sync/v1"`）；socket 端口签名容纳「声明的子协议列表」，Web 实现把它作为 `new WebSocket(url, protocols)` 的第二个参数（`src/sync-client/socket.web.ts`）。
  - 子协议标识取自机器登记的同一来源：合同测试读仓库根 `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol`，与前端常量逐字比对；前端不硬编码第二份字面量。
  - 握手处比对服务端**选中的** feature 是否覆盖客户端必需集合（`core.command-status.v1`、`core.event-ack.v1`）；未覆盖即阻断（成因见 §5）。
  - 配对链路闭合：状态查询的已批准结果携带主机标识与主机公钥（缺材料是可区分的失败，而非「已批准」）；已批准后写入 `paired-host/<canonicalOrigin>` 记录（公钥取自状态响应），随后进入已配对并触发连接；从阻断态重新配对也会解除阻断并按新记录重建装配。配对页提供二维码内容**粘贴**入口与结构校验（design D3：v1 用粘贴，扫码属可选增强），非法输入明确失败且不发请求。
  - 路由走**正常路径**（`/`、`/pair`、`/node-link/pair`、`/hosts`、`/dir/<alias>`、`/session/<id>`），地址片段只承载配对内容、不参与路由命中（`src/routing.test.ts`；核实方式与产物证据见该文件头）。
  - 两类配对入口**互不混淆**（2026-10-16，`node-link-pair-route`）：`/node-link/pair` 有了路由文件（`app/node-link/pair.tsx`），它复用 `/pair` 的入口主体（`app/pair.tsx` 的 `PairingRouteBody`，design D4 的「复用同一组件」），只把**期望的配对类型**声明不同（design D2）：`/pair` 声明 `sync`、`/node-link/pair` 声明 `node-link`。
  - 载荷类型由组合根在读二维码内容时按 `pairingProtocol` **标注**（`readQrPayload`，design D3），入口把它与路由声明的类型对上：不一致（把一类内容的二维码喂给另一类入口）即明确失败，且**在读载荷之后、发任何请求之前**就停住；失败文案说清是哪一类喂给了哪一类、不回显载荷内容。
  - node-link 载荷（`NODE_LINK_PROTOCOL.md` §13.1）在本客户端只「读到并呈现」——端点 host 派生的 https 来源与 owner 节点标识——因为 §13.2/§13.3 的 claim/status 由节点发起、浏览器不是它的参与方。因此打开 node-link 二维码落在**可用入口**（对端信息可读、输入可提交），既不是空路由，也不会误入 sync 流程；该行为与证伪方式见 `clients/app/e2e/page/checks/pairing-kinds.ts` 的 NL-1 / NL-2。
  - `/node-link/pair` 与协议保留前缀的关系：该路径与保留前缀 `/node-link/v1` 同段但不同版本段，两者不冲突；若将来协议要占用 `/node-link/*` 的其它段（例如把 `pair` 收进版本段之下），这条前端路由需要重新评估，不能默认继续有效。

  **未闭合的边界（如实登记，不写作已实现）**：① 真实 Daemon 的浏览器级端到端验收尚未执行（见 §9）；② 本客户端不参与 node-link 配对本身（claim/status 由节点发起），因此 `/node-link/pair` 的入口只呈现对端信息，不代替节点完成配对。

- 主机身份显示、连接状态和重新配对入口。
- 已有会话列表与会话详情。
- workspace 目录浏览：目录列表（以目录为主角）、目录内会话列表、目录搜索、空目录与"没有任何目录"两种空态；目录项只显示展示名与别名，不显示或不可编辑规范化路径。目录数据随快照下发（`core.local-catalog.v1`），因此离线时目录页仍可渲染。
- 无会话时的目录页：本机已登记目录、但这些目录下一条会话都没有时，目录页除「目录已登记，但还没有会话」空态外，仍需同时列出这些目录项——空态指引所说的「在下面选一个目录」必须真的存在，不能只画一个空态、让指引指向一个不存在的列表；本机一个目录都没有时只呈现「本机没有任何目录」空态，不呈现任何目录项。
- 每个目录项都可进入其详情页：目录项自身的标题即进入该目录详情页的入口，且与会话数无关——一条会话都没有的目录同样可进入；该目录有会话时，另保留既有的「查看全部 N 个」入口（N 仍为该目录的会话总数）。两个入口可以指向同一页面，但必须各自带有可区分的机器标识，消费方不得把两者混为一谈。
- 目录项的最近活动时间：目录项呈现该目录下会话的最近活动时间，取值由这些会话自身的时间推出（该目录下会话 `updatedAt` 的最大值），不是第一条、也不是当前时间；呈现文案为 `最近 YYYY-MM-DD HH:MM UTC`（与同步时刻同一套 UTC 固定切片，任何机器渲染出同一段文本），原始时间值另原样带进 DOM 属性供断言。该目录没有会话时不呈现活动时间，也不用当前时间或其它信号代替；该时间不得引入本机路径或可用于反推路径的信息。
- 目录页的状态筛选与同步时间标注：筛选段提供「全部/运行中/待处理」三个取值，与目录搜索叠加生效，只决定哪些目录项留下、不改变各目录自己的会话数与运行中/待处理数；内容来自上次成功同步时，在来源标注旁给出该次同步的时刻或距今时长，从未成功同步过时不显示任何同步时间。
- 目录内创建会话：目录详情页与目录项都提供在该目录内创建会话的入口，两者使用同一条 `session.create` 命令与同一份权限判定，提交的载荷仍只有 `{ workspaceAlias, agentId }` 两个引用，选择器来自本机已配置的 Agent profile；未持 `session.create` scope 时两个入口都必须不可用（目录项上的入口不呈现，目录详情页的入口呈现为不可用），且页面必须给出"这台设备没有创建会话的权限，需要电脑端授予"的说明：只把入口藏起来、不作任何说明是不允许的。提交期间显示创建中状态且不接受重复提交，收到终态后进入新会话或显示可区分的失败原因，结果不确定时显眼保留直到用户处理。
- 历史快照、cursor 补发和实时事件切换。
- 用户 prompt、Agent 流式回复和最终消息。
- 第一阶段只发送文本 prompt；图片、文件和 resource 输入必须明确显示不支持，不能静默删除后提交剩余内容。
- 结构化工具调用、权限请求、终端、文件修改与行级差异统计的基础展示。
- 取消当前 turn：未确认前呈现为「取消请求已提交」并等待终态，呈现以服务端终态为准，不得在未确认前把会话呈现为空闲。
- 模型与模式：config option（`category=model`）列表、模式可选项、当前值、切换中、下一轮生效和不支持状态；切换以服务端确认的结果为准，未提供时不得展示任何可点选项。
- WebSocket 断线重连、ACK、事件去重和命令幂等。
- 未识别 ACP 扩展事件的明确降级展示和诊断信息入口。
- 对话页时间线与结构化卡片：消息与结构化条目在同一条按时间排序的时间线上呈现；来自实时事件与来自按会话读取的历史条目使用同一套条目呈现，同一标识的内容不得因两条来源并存而重复呈现。思考、工具调用、终端输出、计划、权限与提问、文件改动与降级各自成卡，而「某个事件类型有没有专用卡片」由客户端自己的呈现覆盖决定（§6）——协议词表认识、而客户端没有专用视图的事件同样必须走降级卡片，不得静默丢弃。重新进入本机自有会话时，保留期内的结构化历史条目仍然呈现；来源为其它节点且拿不到结构化历史的会话，必须显式说明该部分不可用，不得让用户误以为该会话没有结构化内容。
- 对话页输入与提交：输入区只支持文本并显式说明附件不受支持（图片与文件不得被静默丢弃后提交剩余文本）；区分可输入、进行中与已关闭三种状态，已关闭的会话不得发出提交；提交期间呈现为待确认，未确认前不得呈现为已发送或已排队，结果不确定时显著保留直到用户处理。来源为其它节点的会话不可达时，其草稿只在内存保留并标注未提交，不得写入任何持久存储。
- 权限与 elicitation 应答：只对本设备连接期间收到过请求内容的待处理交互提供应答动作，可选项只来自该请求自身的下发内容；仅从历史读取得到的待处理项呈现为不可操作并说明原因，不得呈现无法成功的应答按钮；已被其它设备处理的呈现为已处理并说明不是本设备所为。
- 变更文件右栏：只呈现每处改动的结构化统计与相对化展示路径，并为每条改动给出「查看 ACP 原文」的诊断入口；不得呈现没有结构化来源的差异正文，也不得为了填补空白而解析 ACP 原文并据此渲染。
- 对话页宽窄两形态：宽屏呈现时间线与并列的变更文件视图，窄屏把变更文件降为可切换的次级视图；两种形态必须呈现同一信息集合，不得因窄屏静默丢弃信息或改变判定结果。
- 响应式手机布局，同时保证桌面浏览器可用于调试。

### 4.2 明确不包含

- 会话明细中的 `turns`、`pending_interactions`、`capabilities` 三类资源，以及 imported（来源为其它节点）会话的明细资源。本期服务端对它们的请求一律回 `capability.unsupported_by_broker`（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9.4），PWA 因此不展示这三类明细与 imported 明细。**这不影响其余三类明细**：`messages`、`config_options` 与 `history` 照常经 `session.read` 取得并呈现。客户端 MUST NOT 把该拒绝渲染成「该会话没有 turn / 没有待处理交互 / 没有能力信息」，也 MUST NOT 因此认为明细已到最早一条——它是「本期服务端不提供这一类」，必须按 §6 的降级规则呈现为不可用并说明原因。
- Android APK/AAB 和 iOS App/IPA。
- APNs、FCM 或 Web Push 服务。
- 后台常驻 WebSocket。
- 电脑离线时的 prompt 排队。
- 原生 Keychain/Keystore 安全等级承诺。
- 生物识别解锁、系统小组件、分享扩展和系统级后台任务。
- 单独的云端 Web 托管或服务端渲染服务。

### 4.3 分发方式

PWA 构建为静态资源，由 Daemon 同源托管在**根目录**（`/`）并随发行包分发：

```text
Expo Web export
      ↓
static assets
      ↓
ACP Remote Daemon /          （根目录同源托管；前端路由深链回退到入口文档）
      ↓
Browser + same-origin WebSocket /sync/v1
```

优先使用相对 URL 和同源 WebSocket，避免额外的 CORS、endpoint 注入和版本漂移。同源 WebSocket 的**路径以 wire 权威为准**（`SYNC_PROTOCOL.md` §3.1 冻结为同源 `/sync/v1`；本节的图示按该口径书写）。前端构建版本必须与 Daemon 可检查地关联，协议握手仍需独立执行版本与 feature negotiation。

托管与协议端点的**解析优先级**：已注册的协议 path（`/sync/v1`、`/node-link/v1` 及两者的配对端点）先命中；只有**未命中任何协议端点**的请求才由静态面接住。协议族**保留前缀**之下的未注册子路径仍按协议面语义返回 404，不被静态面接管（静态端点与 `daemon.listen` 的关系见 `CONFIG_REFERENCE.md` §1）。

PWA 不需要独立云服务器。`[决定]`（2026-10-15，`sync-ui-hosting`）静态资源**嵌入 Rust 二进制**，随二进制同包发布；不采用「作为 npm 平台包中的只读资源旁路安装」。理由：

- 产物随二进制发布，运行期零磁盘 IO、零路径解析；
- 资源表是编译期生成的**精确键集合**，表外的路径根本不存在，静态面因此**没有路径穿越面**，也就不需要做 `..` 归一化之类的路径处理；
- 代价是二进制体积增加（前端产物整体编入二进制）。将来要减小时，正确路径是优化前端产物，而不是改回磁盘旁路。

发布构建在构建时没有可用前端产物（`clients/app/dist` 不含入口文档）时**失败关闭**：`daemon start` 拒绝启动并指向先在 `clients/app` 运行 `npm run export:web`，不静默提供空页面（debug 构建与测试放行，否则没有前端产物的 CI Rust 作业无法派生真实 `daemon start`）。

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

`[实现现状]`（2026-10-16，`frontend-sync-wiring`）配对秘密的清理口径落在组合根与 store 上（`clients/app/src/composition.ts`、`src/features/pairing-model.ts`、`src/features/client-store.ts`）：秘密经二维码内容进入（配对页粘贴的整段内容，或经地址片段自动读入），组合根 `takePairingFragment` **先读后清**——只对 `data=` 形态调 `history.replaceState`，且新地址保留 pathname 与 search（其余形态的片段既不解析也不清除，避免清掉别人的语义），因此秘密不留在地址历史里；读到的载荷只活在 `ClientStore` 的**私有**会话字段里，已批准或进入终态时立即丢弃。`ClientState` 是可导出、可快照的公开状态，其中没有承载秘密的槽位，秘密因此不进缓存摘要、可导出状态与出站 URL。来源与 secure context 校验已一并落地（`pairing-origin-secure-context`）：配对入口（唯一入口 `ClientStore.#beginPairing`）在读完载荷之后、发任何请求之前，依次校验**安全上下文 → 来源一致 → 载荷类型**，任一条不过都明确失败且**一个 claim/status 都不发**。环境事实（是否安全上下文、当前来源）经组合根 `CompositionInputs` 注入（默认读浏览器全局 `isSecureContext` 与 `location.origin`，与 `fetch` 同属一次性能力的注入面，不动 `src/platform/**` 的冻结成员表）；来源比较复用 §3.2 的 canonical origin 口径（浏览器 `URL.origin`），不另造字符串比较。失败码取自封闭词表：来源不符复用既有的 `auth.origin_mismatch`，非安全上下文用新增的 `auth.insecure_context`（已按 §6/§9 的同步清单登记 `compatibility/errors/v1/errors.json`、`schemas/sync/v1/common.schema.json` 的 enum 与 `docs/SYNC_PROTOCOL.md` §12.2）。非安全上下文明确拒绝，不回退到可导出形式的密钥、也不静默降级。

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

四类阻断态（`revoked` / `incompatible` / `identity_changed` / `replaced`）各有独立的恢复路径。其中 `incompatible` 同时覆盖三种恢复路径不同的情形，其成因因此进一步细分为**封闭词表**：`subprotocol_not_selected`（服务端未接受约定的子协议）、`protocol_version_unsupported`（协议版本无法形成交集）、`required_features_missing`（服务端未选中客户端的必需 feature）。细化成因随事件一并交付，不由关闭码反推——未选中子协议与握手期的必需 feature 缺失落到同一个关闭码。阻断态必须与「普通网络断开」可区分：后者进入 `reconnecting` 并按退避重连，阻断态则**结构性**停止自动重连，直到用户显式恢复（例如重新配对）。

最低要求：

- 只有进入 `online` 后，客户端才能把命令显示为已被服务器接受。
- `replaying` 阶段只有本节点资源或未来显式允许正文缓存的资源可以展示持久缓存，并必须标记尚未追平；imported resource 默认等待 Owner 回源。
- 未完成 snapshot 写入独立暂存区；`no-content-cache` 资源使用内存暂存，其他允许缓存的资源只有数量和 digest 验证成功后才能原子替换缓存。断线时丢弃未完成 snapshot 并重新请求。
- 连接断开时不能把本地输入伪装成已经发送；第一阶段默认不离线排队 prompt。
- `identity_changed` 必须阻止自动信任新主机密钥。
- `revoked` 必须清理会话密钥和受保护缓存，并要求重新配对。
- imported 会话的 `origin.online=false` 时连接状态仍可以是 `online`，但该会话必须标记为离线：只展示元数据，正文请求由服务端返回 `resource.remote_unavailable`，本地输入不得标记为已发送或排队；服务端发来 `session.origin.online_changed` 时立即刷新该标记。

命令至少具有：

```text
draft -> submitting -> accepted -> completed | failed | rejected | uncertain
```

连接在确认前中断时，客户端先使用同一 `requestId` 重试或执行 `command.status`；只有服务端确认进入 `uncertain` 才显示该终态，不能仅根据网络断开自行断定，也不能生成新 ID 导致重复执行。

第一阶段同一 PWA 设备只允许一条 active WSS。多标签页应协调连接；服务端以新认证连接替换旧连接时，旧页面进入 `replaced` 并停止自动重连，避免标签页互相抢占。切换窗口中的事件仍按 `eventId` 去重。

`[实现现状]`（2026-10-16，`frontend-sync-wiring`）12 个状态的封闭清单与四类阻断态在 `clients/app/src/state/connection-machine.ts`（`CONNECTION_STATES` / `BLOCKING_STATES`），细化成因词表在 `src/sync-client/connection.ts` 的 `IncompatibleReason`。三条成因各有单测：未选中子协议时**一个出站帧都不发**、以 `4406` 终止并落 `blocked`（而非 `closed`）；协议版本不兼容与必需 feature 缺失各落对应的 `reason`；真断网（`1006`）落 `closed` 且无 `blocked`。门面级断言阻断后 `connect()` 抛错，即**结构性**停止自动重连（而非依赖某一层「忘了重连」）。

## 6. ACP 能力的前端呈现

前端同样遵守 ACP 兼容性不变量：

- 结构化 ACP 能力使用结构化 UI，不静默降格为文本。
- 客户端理解公共字段，但保留扩展数据供诊断或未来组件使用。
- 暂时没有专用组件时显示明确的“当前客户端未提供专用视图”，不能直接隐藏事件。
- ACP 原文按 `acp.rawJson` 作为不可信文本保存或诊断展示；不得为了渲染先解析再覆盖原值，尤其不能损坏超过 JavaScript safe integer 的未知数字。
- `unsupported_by_client`、`unsupported_by_broker` 和 `unsupported_by_agent` 必须可区分。
- `unsupported_by_broker` 的呈现必须说清是**服务端本期不提供**，而不是「内容为空」或「已到最早一条」：本期服务端对 `turns`、`pending_interactions`、`capabilities` 三类明细资源与 imported 会话的明细一律回该错误（§4.2 与 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9.4），客户端据此呈现「本期不可用」并说明原因，`hasEarlier` 之类的分页标记不得由该拒绝推断。其余三类明细（`messages`、`config_options`、`history`）不受影响。
- capability negotiation 的结果进入客户端 feature gate，不能只靠版本号猜测能力。
- 呈现覆盖由客户端自持：客户端呈现能力由客户端自己维护，不进入服务端快照；服务端不定义、不协商也不下发客户端呈现能力字段。客户端自己维护「哪些事件类型有专用视图」的覆盖判定，而协议的**事件类型词表只决定客户端能否解析一个事件**，不得以该词表代替覆盖判定。对任一被消费的事件，只有两种结果——按专用视图呈现，或呈现明确的降级卡片；「协议认识该类型、但客户端当前没有它的专用视图」的事件同样必须呈现降级卡片并保留原文与事件元数据的入口，不得存在既不呈现也不降级的第三种结果。覆盖判定未命中不得导致整个会话渲染失败。
- 未知事件不得导致整个会话渲染崩溃。

## 7. 本地数据边界

PWA 首个版本只持久化连接和恢复所需的最小数据：

- 已配对主机的非秘密元数据。
- 当前 PWA 身份绑定的 canonical origin。
- 最后确认的 global/session cursor。
- 客户端 UI 偏好。
- 待确认命令的 `requestId` 和最小恢复信息。
- workspace 目录与 Agent profile 的引用缓存（别名、展示名、默认标记），用于离线渲染目录页与创建选择器；只存引用，不存规范化路径（§2.2 与 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.5）。目录项随快照重建校正，清除后重新同步即可。

规则：

- imported resource 遵守 `no-content-cache`，不得把会话摘要、prompt、回复、工具内容、diff、终端、附件或 ACP raw 写入 IndexedDB、Cache Storage 或其他持久浏览器存储。
- 对当前节点本地拥有的资源，只有节点本地策略显式允许时才缓存精简历史；缓存不是权威数据，重连后以 Daemon 快照和事件日志校正。
- 敏感字段不得因方便调试而进入普通浏览器存储。
- 必须设置大小上限、版本和迁移策略：v1 的固定值为本节点自有资源摘要缓存上限 **8 MiB**、TTL **30 天**、超出时按 LRU 淘汰。这些是固定常量，不是可由用户放宽的配置项。
- imported 资源只允许持久化恢复所需的无正文元数据：owner/export/session 标识、origin/global/session cursor、ACK、稳定 `requestId`、命令终态引用、事件类型与 SHA-256 digest、local sequence 映射。这里的 digest 是密码学摘要，不是会话摘要、标题、消息预览或内容摘录；这些内容只能保留在内存。元数据也必须有容量与清理边界，不得以“不占正文配额”为由无限增长。
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

1. 可以查看和操作当前节点可见的已有会话；会话创建是**受限**入口而不是被禁止的能力：只能在已登记的 workspace 目录内创建，Agent 只能从本机已配置的 profile 中选择，提交的 payload 只有 `{ workspaceAlias, agentId }` 两个引用——客户端无法通过 UI 或构造普通命令提交规范化路径、目录追加、MCP 配置或凭据。未持 `session.create` scope 的设备该入口不可用，且服务端也必须拒绝；目录与 Agent 目录在离线状态下仍可渲染（数据来自快照）。该入口的交互口径以 [`prototypes/acp-remote-pwa.html`](../prototypes/acp-remote-pwa.html) 为原型来源：目录页（`#/dirs`）、目录详情（`#/dir/work-api`）与「在此目录新建会话」弹层——目录为主角的列表、按月分档的详情、只列出本机已配置 Agent 的选择器、「将在你的电脑上启动一个 Agent 进程」的副作用提示，以及无权限时的「这台设备没有创建会话的权限，需要电脑端授予」文案。
2. 能完成配对、认证、订阅、历史追平和实时切换。
3. 网络断开后自动重连，并从最后 ACK cursor 补发，不重复显示事件。
4. prompt 重试不会导致 Agent 重复执行。
5. 能正确显示流式消息、权限请求、工具调用、终端和配置项与模式变化（含 `category=model`）的最低结构化视图。
6. 未知 ACP 扩展不会丢失、崩溃或被伪装成普通文本。
7. 慢速渲染或后台标签页不会阻塞 Agent 和其他客户端。
8. 浏览器数据清理后不会继续冒充原设备，而是要求重新配对。
9. Owner 离线时 imported 会话只显示元数据与 `origin.online=false`；正文请求返回 `resource.remote_unavailable`；输入不得标记为已发送。
10. PWA 构建可以由 Daemon 本地托管，不需要应用级云服务器。

`[实现现状]`（2026-10-16，`frontend-sync-wiring`；同日 `node-link-pair-route` 在其后追加两条入口区分用例）上述条目里属于前端接线的部分（第 2 条的配对链路与连接握手，即 §4.1 记的那组实绩）已有可执行证据，但**只到前端门禁与假主机 e2e 这一层**：`cd clients/app && npm run check` = 59 files / 725 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = 51/51（既有 49 条 + PW-1、PW-2；真实 Chromium + 假主机，含出站 URL 路径、升级请求声明的子协议、已批准后落盘的主机公钥、配对入口存在与可用这四条可证伪断言）。`node-link-pair-route` 在同一处追加两条：打开 `/node-link/pair#data=…`（node-link 载荷）落在可用入口（对端信息可读、输入可提交、一个请求都不发），以及两类入口交叉喂错内容都明确失败。上面两组计数是 `frontend-sync-wiring` 的实测值；本变更追加用例后的计数以本轮门禁实测为准。**未执行**：第 2 条所依赖的「真实浏览器 + 真实 Daemon（同源托管）」端到端验收（完整配对、已认证连接、目录与会话快照、断开按游标恢复）**尚未执行**——按 [WORKFLOW.md](./WORKFLOW.md) 的 E2E 口径不得以假主机用例代替，它属尚未收口的验收项，因此第 2 条暂不声称已在真实 Daemon 上验过。

## 10. 测试边界

- 协议 fixture：版本协商、未知字段、错误和 feature negotiation。
- 状态机：连接、认证、重放、在线、断线、撤销、身份变化。
- 幂等：接受响应丢失、重试、重复事件和 cursor 回退。
- 组件：结构化 ACP 事件、未知事件和能力降级。
- 存储：同步元数据迁移、容量限制、清除和损坏恢复，并验证 imported resource 正文不会落盘；remote 事件的固定用例是 [`fixtures/sync/v1/valid/event-remote-origin.json`](../fixtures/sync/v1/valid/event-remote-origin.json)。
- 浏览器端到端：Daemon fake/fixture、真实 WebSocket、重连和慢客户端。
- 安全：敏感信息不进入日志、URL、普通存储或错误页面。

PWA 测试通过不代表原生客户端的安全存储、后台行为和系统权限已经验证。

## 11. 暂缓决策

- 前端状态库和表单库的最终选择。
- PWA WebCrypto P-256/IndexedDB 的实际浏览器支持矩阵和验收结果。
- 原生端是否需要一个最小 Rust `mobile-security` 库。
- Android/iOS 的最低系统版本。
- 是否以及何时引入系统推送服务。

暂缓决策不能被单个实现补丁静默确定；需要记录验证结果并更新本文。本节原列的「静态资源嵌入二进制还是作为旁路资源安装」已按这一条**结项**（2026-10-15，`sync-ui-hosting`）：裁定结果、理由与代价写回 §4.3，不是被实现补丁静默改掉的。
