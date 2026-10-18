# ACP Remote 前端设计

> 状态：编码前客户端约束  
> 版本：0.6
> 修订记录（2026-10-18，`device-pairing-qr-reference`，**已实现**；Native 真机未执行）：PWA 设备配对**只保留配对码入口**。`/pair` 打开新设备 `q2=` 短二维码片段时只清除片段并显示固定 App 指引，打开旧设备 `data=` 片段时清除并提示已不支持；两者都不发 lookup/qr-lookup/claim/status。设备二维码取回、指纹校验与有限重试由**未来原生 App 的共享消费合同**承载；共享解码/指纹/来源绑定与重试策略已有测试调用方，但 Android/iOS App、相机、keystore 与真机扫码**未执行**，PWA 不接入该取回器。`/node-link/pair` 不变。协议与机器合同见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.1/§7.6，二维码字段不在本文重复。
> 修订记录（2026-10-18，`pairing-theme-toggle`）：配对页右上角增加复用共享主题偏好的明暗切换按钮（`PairingThemeToggle`）；页面仍不呈现全局顶栏、页头或返回入口，主题状态仍由 `themePreferenceStore` 统一持有。
> 修订记录（2026-10-18，`pairing-code-web-entry`）：`/pair` 增加第二种入口——无 `data=` 片段时显示 8 位配对码输入（对应原型 S0），有片段时行为不变；移动端仍只使用 `#data=` 二维码载荷，`/node-link/pair` 仍只接受片段、不提供配对码输入。配对码输入页在 lookup 之前 MUST NOT 显示数字倒计时，只显示静态提示“配对码 5 分钟内有效。”；lookup 成功后改用服务端响应里的 `expiresAt` 启动倒计时，服务端始终是过期事实的权威。lookup 的任何失败（含响应丢失）统一呈现固定文案「配对码不对或已过期，请重新生成配对码」，不回显后端 `message`、不自动重试。配对码 MUST NOT 进入 URL、Cookie、localStorage、sessionStorage、地址历史或日志，只活在页面内存并在终态/卸载时清除。上一轮 `pairing-page-open-url` 对 `/pair` 的「仅地址片段入口」表述由本条取代；`/node-link/pair` 的仅片段入口不变。
> 修订记录（2026-10-18，`restore-directory-identity-baseline`）：前端目录身份补齐当前 wire 合同——目录身份取服务端下发的 `directoryId` 与 `path`；展示名由 `path` 尾段派生、尾段相同时用更多路径段消歧；目录标识只作路由与命令引用的内部标识，MUST NOT 作为界面文本呈现；目录页搜索只匹配展示名与会话标题；目录详情路由写作 `/dir/<directoryId>`。本轮不改协议、不改 Rust 行为。
> 修订记录（2026-10-17，`pairing-page-open-url`）：配对页入口当时收敛为**仅地址片段入口**——只从地址片段（`/pair#data=…`、`/node-link/pair#data=…`）读取配对内容，删除页内任意粘贴输入框、提交按钮与「重新扫码」（`/pair` 的这一口径已于 2026-10-18 由 `pairing-code-web-entry` 扩展为「片段或受控的 8 位配对码输入」，见上条；`/node-link/pair` 仍只接受片段）；两类入口读到片段后都立即清除，`/pair` 的 sync 载荷自动开始认领，`/node-link/pair` 的 node-link 载荷只呈现对端信息、不发起 claim/status。同时删除页头与页内返回，全局顶栏仍不呈现（单目的状态页，恢复路径是电脑重新生成地址）。配对中按载荷 `expiresAt` 呈现真实剩余有效期 `m:ss`，归零或打开即过期进入「地址已过期」并停止轮询；审批通过后约 1.6 秒自动 `replace` 到目录页、不以连接已在线为前提；失败页九类固定文案且无按钮。配对页完整复用当前主题调色板，但页内不提供主题切换——移动原型配对页的「标题栏 + 主题按钮」是本次对原型的**一处有意偏离**。§4.1/§4.5/§9 同步改写；后端零行为变更，越过 `expiresAt` 后 claim → 410、status → `expired`、CLI 本地按 `expiresAt` 退出，证据是既有用例。
> 修订记录（2026-10-16，`pairing-model`）：**配对链路补齐两条密码学校验**——§4.1 写回「认领响应的 `hostProof` 用配对载荷里固定的主机公钥验签」与「已批准响应的主机公钥必须与载荷逐字节相同」，落盘的主机公钥取自配对载荷而不是响应自带；**创建入口不再按权限门控**——§2.2/§4.1/§9 删去「未持 `session.create` scope 则入口不可用」「需要电脑端授予」的降级态与其说明（设备方向没有授权维度），§4.1 与 §2.2 的 `session.create` 载荷放开为「`agentId` + 目录引用（已登记别名或用户输入的绝对目录）」；§1 的冲突顺序不变。设备认证、撤销与「配对确认必须由本机用户完成」未变；本轮只改文档口径，真实浏览器与真实 Daemon 的端到端验收仍以 §9 的登记为准。
> 修订记录（2026-10-16，`ui-csp-inline-styles`）：§4.5 登记入口文档的**样式形态**实绩——reset 外置为同源 `/entry-reset.css`、`react-native-web` 的运行时样式表写进**必须保持为空**的同源 `/rnw-stylesheet.css`，样式因此在既定 CSP 下由同源外部样式表交付（**不是**放宽 CSP）；取舍依据见归档的 `openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md`。  
> 修订记录（2026-10-16，`node-link-pair-route`）：§4.1 登记新增路由 `/node-link/pair`（与 `/pair` 共用同一入口主体）与「两类配对入口互不混淆」的口径，并记下该路径与协议保留前缀的关系；§9 追加两条入口区分用例。  
> 修订记录（2026-10-16，`pairing-origin-secure-context`）：§4.5 把「来源与 secure context 校验尚未实现」改写成实绩——配对入口在读载荷之后、发任何请求之前依次校验安全上下文、来源一致与载荷类型，环境事实经组合根注入，失败码为 `auth.origin_mismatch` / 新增的 `auth.insecure_context`。  
> 修订记录（2026-10-16，`frontend-sync-wiring`）：§4.1 写回「主机连接与首次配对」的实绩与未闭合边界；§4.5 写回配对秘密的清理口径与尚未实现的来源校验；§5 补阻断态的封闭成因词表与「与普通断网可分、阻断后结构性停止自动重连」；§9 写回前端级验收证据，并记入同日已执行完成、逐条给实测输出的真机端到端验收（真实 Daemon 同源托管 + 真实 Chromium，见归档 `plan.md` 的「真机验收记录」）。  
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

设备形态不定义权限。客户端行为由已认证的设备/节点身份、Owner Export Policy 与端到端 capability 的交集决定——**设备方向没有授权维度**：本节点自己的设备通过认证即可执行命令，配对不授予授权，客户端也不呈现任何权限门控：

- 可以访问当前节点本地拥有或从其他节点导入、且已授权的 Agent/会话。
- 可以查看历史和实时事件、继续对话、取消 turn、处理权限请求、选择模型和调用 Agent 暴露且被允许的能力。
- 第一阶段 PWA 提供在本机目录内创建会话的入口：Agent 只能从本机已配置的 profile 中选择（feature `core.local-catalog.v1` 的快照资源供数，离线也可渲染），目录引用取本机已记录的目录标识（`directoryId`）或用户输入的绝对目录（`workspacePath`）二者之一（feature `core.session-create.v1`）。入口**不按权限门控**（设备方向没有授权维度，服务端以「已认证」为唯一前提）。客户端**不**展示也不构造规范化路径、目录追加、MCP 配置或凭据；Node Link 侧 `session.create` 额外要求 Owner 导出的 workspace template 且只接受目录标识，PWA 不使用这条形状。
- 客户端不能提交任意 Owner 路径：Node Link 面一律禁止，Sync 面的创建命令也只允许一个绝对目录并经本机路径校验（不存在的目录、相对路径与含 `..` 的输入都必须明确失败，不回退到任何替代目录）。客户端不能配置 Provider 凭据、扩大沙箱权限或直接发送任意 ACP JSON-RPC。
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

页面启动读取本地身份与配对记录期间，统一显示普通的“正在加载…”状态，不挂载业务页面，也不显示红色初始化错误或未配对提示。初始化成功后进入正常页面；只有初始化失败才显示明确错误与刷新重试指引。

`session.create` 完成回复中的服务端会话摘要须补入客户端共享会话清单，使进入会话页后返回目录列表或目录详情时立即可见，无需刷新或等待额外的 `session.created` 事件。只在收到完成终态后补入；重复回复不得覆盖已有的后续实时更新。会话的目录归属仍以服务端摘要为准。

创建完成摘要引用了本地清单尚无的新目录时，客户端须在当前连接无游标重新订阅，经服务端 `reset_required` 和完整快照取得目录标识与路径；验证通过后原子替换清单，使新目录及其会话可见。刷新期间保留旧清单，不从用户提交路径构造目录条目，不重新认证；重复完成回复不得重复刷新，并发创建合并在途刷新且保留后续更新，断线后保留刷新意图。服务端已将目录与成功的会话同事务保存，页面刷新和进程重启后仍能恢复。仅浏览目录或独立新建空文件夹不使它进入会话目录页。

- 主机连接与首次配对。

  页面入口在本地身份与配对记录读取完成后统一判断：没有已配对主机记录时，访问业务页面自动替换路由到 `/pair`；`/pair` 与 `/node-link/pair` 保留各自入口，不重复跳转。读取尚未完成时不按未配对处理。已有记录时设备配对输入页显示「重新配对」；进入页面只读取记录，不覆盖既有配对信息。

  `[实现现状]`（2026-10-16，`frontend-sync-wiring`）连接与首次配对已按本变更接线闭合，实测来自 `clients/app` 的独立门禁（`cd clients/app && npm run check` = 59 files / 725 passed + `expo export` 成功）与假主机 e2e（`node e2e/run-e2e.mjs` = 51/51，既有 49 条 + PW-1、PW-2）：

  - 出站地址由组合根 `syncSocketUrl` **唯一**构造，路径为权威端点 `/sync/v1`（`src/sync-client/endpoint-contract.test.ts` 用字面量断言 `new URL(...).pathname === "/sync/v1"`）；socket 端口签名容纳「声明的子协议列表」，Web 实现把它作为 `new WebSocket(url, protocols)` 的第二个参数（`src/sync-client/socket.web.ts`）。
  - 子协议标识取自机器登记的同一来源：合同测试读仓库根 `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol`，与前端常量逐字比对；前端不硬编码第二份字面量。
  - 握手处比对服务端**选中的** feature 是否覆盖客户端必需集合（`core.command-status.v1`、`core.event-ack.v1`）；未覆盖即阻断（成因见 §5）。
  - 配对链路闭合：状态查询的已批准结果携带主机标识与主机公钥（缺材料是可区分的失败，而非「已批准」）；已批准后写入 `paired-host/<canonicalOrigin>` 记录（设备配对只经配对码入口，主机公钥绑定来自该次 lookup 响应），随后进入已配对并触发连接；从阻断态重新配对也会解除阻断并按新记录重建装配。配对页的设备入口（2026-10-18，`device-pairing-qr-reference`，**已实现**；取代 `pairing-code-web-entry` 的 `#data=` 自动认领）：`/pair` **只使用配对码**。无片段时显示 8 位配对码输入（S0：当前 canonical origin 与 HTTPS 标识、8 个单字符格；大小写不敏感、`I`/`L`→`1`、`O`→`0`、可粘贴 `XXXX-XXXX` 或连续 8 位），提交后先经同源 lookup 取回绑定与配对秘密，再复用同一套 claim/status 流程；打开新设备 `q2=` 短二维码片段时立即清除片段并显示固定文案「请在 ACP Remote App 内扫码，或返回网页使用配对码」，打开旧设备 `data=` 片段时清除并明确提示已停用；两种片段都 MUST NOT 触发 lookup、qr-lookup、claim 或 status。设备扫码取回与指纹校验属于后续原生 App，PWA MUST NOT 接入该取回器；阶段与原型 S0–S12 一一对应（S0 输入、S1 lookup、S2 claim、S3 等待本机确认、S4 已批准，S5–S12 为固定失败/终态）。`/node-link/pair` 仍只接受 `data=` 片段，无片段时呈现节点专用文案「这个地址里没有节点配对信息」，MUST NOT 显示配对码输入或发起 lookup；node-link 载荷只呈现对端信息、不发起 claim/status。结构非法的片段明确失败且不发请求。配对码 MUST NOT 写入 URL、Cookie、localStorage、sessionStorage、日志或可导出配置。
  - 配对页形态（2026-10-17，`pairing-page-open-url`；2026-10-18 由 `pairing-code-web-entry` 扩展）：单目的状态页——不绘制页头与页内返回，全局顶栏仍不呈现。进行中呈现居中转圈与按服务端 `expiresAt` 计算的真实剩余有效期 `m:ss`（配对码入口取自 lookup 响应；每秒刷新，不是从页面打开起固定 300 秒）；设备二维码在 PWA 中不进入配对流程，因此不显示倒计时或进行中状态（原生 App 的倒计时合同见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.6，由后续 App 交付）；配对码入口归零（含 lookup 之后的倒计时）时立即停止轮询并进入「配对码不对或已过期」（S6），lookup 响应本身已过期时也进入该呈现。审批通过后显示短暂过渡，并在约 1.6 秒后以替换当前历史条目的方式进入目录页（路由 `/`，不是 `/dirs`）；进入目录**不以连接已在线为前提**，连接装配失败也照常进入，由目录页的设备胶囊表达断开与自动重连。失败页按十类固定文案呈现标题与说明——被拒绝、已过期、已被使用、无法识别、来源不一致、非安全上下文、请求未能完成、载荷类型与入口不匹配、没有载荷、配对码不对或已过期——MUST NOT 渲染后端 `message`，也不提供任何按钮；恢复路径统一是「在电脑上重新生成地址或配对码，再用新的地址或新码进入」。`/node-link/pair` 无片段（或刷新后片段已清）与载荷类型不匹配各走专用文案，且都不发任何请求；`/pair` 无片段则进入配对码输入（S0），不显示数字倒计时，只显示静态提示“配对码 5 分钟内有效。”，lookup 成功后才按服务端 `expiresAt` 启动倒计时。lookup 的任何失败（含响应丢失）统一呈现「配对码不对或已过期，请重新生成配对码」，不显示配对是否存在、不自动重试、不提供页内重试、不落盘主机记录。配对码只保留在页面内存（`ClientStore` 私有字段），进入终态或页面卸载时清除。`/node-link/pair` 复用同一页面主体，只呈现对端信息，不跳转、不显示倒计时、不提供配对码输入。
  - 路由走**正常路径**（`/`、`/pair`、`/node-link/pair`、`/hosts`、`/dir/<directoryId>`、`/session/<id>`），地址片段只承载配对内容、不参与路由命中（`src/routing.test.ts`；核实方式与产物证据见该文件头）。
  - 两类配对入口**互不混淆**（2026-10-16，`node-link-pair-route`）：`/node-link/pair` 有了路由文件（`app/node-link/pair.tsx`），它复用 `/pair` 的入口主体（`app/pair.tsx` 的 `PairingRouteBody`，design D4 的「复用同一组件」），只把**期望的配对类型**声明不同（design D2）：`/pair` 声明 `sync`、`/node-link/pair` 声明 `node-link`。
  - 载荷类型由组合根在读二维码内容时按 `pairingProtocol` **标注**（`readQrPayload`，design D3），入口把它与路由声明的类型对上：不一致（把一类内容的二维码喂给另一类入口）即明确失败，且**在读载荷之后、发任何请求之前**就停住；失败文案是固定的「这不是本入口的配对地址」，不回显载荷内容、也不透露载荷类型细节。
  - `[实现现状]`（2026-10-18，`device-pairing-qr-reference`）设备短二维码不再进入 `readQrPayload` 的 JSON 解析：`/pair` 对 `q2=` 与旧 `data=` 片段都只做「读取判定 → 清除片段 → 固定文案」，不构造 sync 配对载荷、不调用 claim。node-link 分支的现有解析与只读展示保持不变，`/node-link/pair#data=…` 不受设备格式替换影响。假主机页面 runner 的 PW-1/PW-1b 两条用例断言两条误扫路径都零请求（`clients/app/e2e/page/checks/pairing-first-run.ts`）；它不替代真实 Daemon 证据。
  - node-link 载荷（`NODE_LINK_PROTOCOL.md` §13.1）在本客户端只「读到并呈现」——端点 host 派生的 https 来源与 owner 节点标识——因为 §13.2/§13.3 的 claim/status 由节点发起、浏览器不是它的参与方。因此打开 node-link 配对地址落在**可用入口**（对端信息可读、一个请求都不发、不跳转、不显示倒计时），既不是空路由，也不会误入 sync 流程；该行为与证伪方式见 `clients/app/e2e/page/checks/pairing-kinds.ts` 的 NL-1 / NL-2。
  - `/node-link/pair` 与协议保留前缀的关系：该路径与保留前缀 `/node-link/v1` 同段但不同版本段，两者不冲突；若将来协议要占用 `/node-link/*` 的其它段（例如把 `pair` 收进版本段之下），这条前端路由需要重新评估，不能默认继续有效。

  **未闭合的边界（如实登记，不写作已实现）**：本客户端不参与 node-link 配对本身（claim/status 由节点发起），因此 `/node-link/pair` 的入口只呈现对端信息，不代替节点完成配对。

  与上面那条边界相对，真实 Daemon 的浏览器级端到端验收**已执行完成**（2026-10-16，逐条实测见 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」小节，见 §9）——它是实绩，因此不登记在未闭合边界里。

- 配对的两条密码学校验（`pairing-model`；`pairing-code-web-entry` 扩展入口）：客户端 MUST 用**本次配对固定的**主机公钥验证认领响应的 `hostProof`，MUST NOT 使用响应自带的公钥或跳过校验；验签失败 MUST 明确失败、停止后续轮询，且 MUST NOT 写入任何已配对主机记录。状态为已批准时 MUST 校验响应里的主机公钥与该固定公钥**逐字节相同**，并校验主机标识一致（状态响应没有主机签名，只能靠比对）；任一项不符 MUST 明确失败、MUST NOT 落盘。二维码入口的固定公钥来自 fragment 配对载荷，配对码入口的固定公钥来自该次同源 lookup 的成功响应，之后不得被任何后续响应替换。已配对主机记录里的公钥以这个固定公钥为准，响应只是比对对象。**配对码入口**允许用户点击「继续配对」显式重新配对；仅在本机批准且上述校验通过后替换同源旧记录，进页、等待确认或失败 MUST NOT 覆盖旧记录；**二维码入口不受这条限制**——用户在显式重新配对时以 fragment 载荷的公钥为准写入新记录（与 `rememberPairedHost` 及「从阻断态重新配对」的行为一致）。配对不再派生或展示短验证码——设备侧的主机信任判定全部是密码学的，理由见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §5.1。

- 主机身份显示、连接状态和重新配对入口。
- 共享外壳与全局入口：全部路由共用**一份**外壳（装在根布局里，路由不各自拼导航）——顶栏含标识、设备胶囊、开发模式徽标、主题开关与帮助入口；页面不在安全上下文时（即无 HTTPS 托管，§4.4）顶部呈现开发模式横幅、顶栏同时呈现开发模式徽标，且不得把它呈现为安全连接；帮助抽屉 + 遮罩与全局提示在所有路由可用且行为一致。连接状态**只**由设备胶囊这一个控件表达：圆点分正常 / 需要注意 / 阻断三档，后两档各追加一个短词说明；离线（`disconnected`）按阻断档呈现红色，正在重连与配对中按需要注意档呈现黄色，阻断态可从胶囊进入主机与连接面板。配对入口不呈现全局顶栏，也不绘制页头或页内返回——它是单目的状态页，恢复路径是由电脑重新生成配对地址（审批通过后自动进入目录）。主题提供跟随系统 / 白天 / 黑夜三态，选择持久化为**客户端 UI 偏好**（§7）并在下次打开生效；跟随系统时随系统深浅色变化。该偏好不得与设备身份、连接凭据或会话正文写在同一个存储槽位，也不得影响任何数据判定。
- 已有会话列表与会话详情。
- workspace 目录浏览：目录列表（以目录为主角）、目录内会话列表、目录搜索、空目录与"没有任何目录"两种空态。目录身份取服务端下发的 `directoryId` 与 `path`，**展示名由 `path` 的尾段派生**；两个目录的尾段相同时，用**更多路径段**消歧，尾段唯一时不得呈现任何额外路径片段。路径必须**逐字**取自服务端下发的引用，客户端不得拼接、规范化、推导或反查本机路径，也不得让用户编辑它；目录标识（`directoryId`）只作为路由与命令引用的内部标识，MUST NOT 作为界面元素或其它界面文本呈现。目录页搜索只匹配由路径派生的展示名与名下会话标题，不匹配目录标识。目录数据随快照下发（`core.local-catalog.v1`），因此离线时目录页仍可渲染。
- 无会话时的目录页：本机已登记目录、但这些目录下一条会话都没有时，目录页除「目录已登记，但还没有会话」空态外，仍需同时列出这些目录项——空态指引所说的「在下面选一个目录」必须真的存在，不能只画一个空态、让指引指向一个不存在的列表；本机一个目录都没有时只呈现「从第一个会话开始」空态，不呈现任何目录项；工具条仍保留搜索、状态筛选与右上角创建入口，空态说明块内另给一个常规尺寸的创建入口。
- 每个目录项都可进入其详情页：目录项自身的标题即进入该目录详情页的入口，且与会话数无关——一条会话都没有的目录同样可进入；该目录有会话时，另保留「查看全部 ›」入口（文案不带会话数；内联只列出前若干条会话）；内联会话行超过展示上限时，卡体底部再给一个同样指向详情页的「查看全部 ›」。这些入口可以指向同一页面，但必须各自带有可区分的机器标识，消费方不得把它们混为一谈。
- 会话时间的相对口径：目录页、目录详情页与对话页左栏按**同一个**相对口径呈现会话时间——一小时内给「刚刚」或「N 分钟前」，超过一小时给该时刻的日期（UTC 固定切片）。该文案由客户端用会话自身的时刻与当前时间算出（时钟作为视图模型输入注入，组件仍是纯函数），不得由服务端下发显示字符串。目录项本身不再呈现「最近活动时间」。
- 目录详情按时间倒序分页：详情页把该目录的会话按时间倒序平铺，每页固定条数（10 条），呈现当前页码与总页数，并提供上一页 / 下一页与页码输入；非法或超出范围的页码在输入位置**就地纠正**、不因此重画列表；翻页控件**固定呈现**，只有一页时上一页 / 下一页为禁用态、页码显示 1 / 1；翻到新的一页后回到该页顶部。**月份分档与目录内搜索移除**——需要按标题找会话时使用目录页的搜索（它同时匹配展示名与名下会话标题）。
- 目录页的顺序与分页：目录项按该目录下最近一次会话的时间倒序排列（没有会话的目录排在最后，时间相同或无时间时保持快照顺序），每页固定 5 个目录。列表底部固定呈现翻页控件（上一页 / 页码输入 / 下一页）；只有一页时两个按钮为禁用态。搜索或筛选变化回到第一页；非法或越界页码在输入位置就地纠正。排序与分页不得改变目录项的会话数与运行中/待处理汇总。目录块超出可视高度时，工具条保持固定，工具条以下的目录块与翻页控件在**同一垂直滚动区**内上下滑动，不得被裁切。
- 目录页的状态筛选：筛选段提供「全部/运行中/待处理」三个取值，与目录搜索叠加生效；它们决定哪些目录项留下，并**同时过滤每个目录项内联的会话行**——目录展示名命中查询词时保留全部（按筛选后的）行，只命中部分会话标题时只保留命中的行，筛选值只保留对应状态的行。两者都 MUST NOT 改变各目录自己的会话数与运行中/待处理数。已有目录但全部没有会话时仍按目录展示名搜索；「运行中」「待处理」不保留零会话目录，无命中时显示「没有匹配的结果」。目录页不展示内容来源和同步时间；连接状态由顶栏设备胶囊表达。没有目录时显示「从第一个会话开始」，不显示创建说明长文。

- 创建会话的成功导航：目录页与目录详情页收到服务端确认成功的创建结果后，关闭创建弹层并进入结果所指的新会话。仅已受理、失败或结果不确定时不跳转。成功关闭后解除弹层与该结果的关联，保留命令记录，保证再次新建不会跳回旧会话。
- 文件夹选择器的显示排序：对收到的盘符、子目录按名称升序排列，忽略大小写，数字使用自然顺序（例如 `project2` 在 `Project10` 前）；同名按原始路径稳定定序，不修改名称、路径、登记标记或原始结果。协议的 Unicode 码点顺序及截断语义不变；Windows 隐藏/系统属性目录由默认后端在列举时过滤，见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5.5。
- 新建会话弹窗按桌面原型呈现：居中遮罩、540px 宽度上限、视口高度内滚动、分隔的页头与页脚、Agent 单选卡片、页脚「取消 / 创建会话」。文件夹选择器在创建弹窗上层打开，宽度上限 460px、内容区可滚动；键盘焦点保留在当前最上层弹窗，Escape 关闭该层，关闭后归还焦点。任意目录指运行 Daemon 的主机上的目录：顶部入口逐层浏览后选定，可新建文件夹再创建会话；不使用浏览器本机的文件选择来冒充主机目录。已有后端接收 `workspacePath`，`createIfMissing: true` 时创建缺失目录及父层，受主机文件权限约束；本次未改变协议或后端行为。
- 目录内创建会话：目录详情页与目录项都提供在该目录内创建会话的入口，两者使用同一条 `session.create` 命令，选择器来自本机已配置的 Agent profile；提交的载荷只允许**三类**字段——Agent 标识、目录引用（本机已记录的目录标识，或浏览主机文件系统后选定的一层，二者之一）与一个可选的「目录不存在则先创建」开关。出现其它键（含 MCP 配置与凭据字段）或把该开关与目录标识同时给出时，服务端以 schema 校验错误拒绝，客户端不得因此回显提交内容。指定目录有**两条**路径：选择本机已登记的目录，或从主机的根列表开始逐层浏览并选定（面包屑、上一级、已登记目录标注、并在浏览过程中新建一层）；服务端未提供浏览能力时，入口必须明确说明该能力不可用，并保留选择已登记目录这条路径，不得呈现一个必然失败的手工输入。入口**不因权限不可用**——设备方向没有授权维度，页面 MUST NOT 出现「没有创建会话的权限」「需要电脑端授予」一类说明，也不得把入口藏起来当作「权限不足」的处理。目录引用不可用时以可区分的失败原因呈现（不存在、不是目录、权限不足各自可区分，且不回退到任何替代目录）。入口旁 MUST 说明提交会在电脑上启动一个新的 Agent 进程。提交期间显示创建中状态且不接受重复提交，收到终态后进入新会话或显示可区分的失败原因，结果不确定时显眼保留直到用户处理。
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
- 变更文件右栏：只呈现每处改动的结构化统计与相对化展示路径，并为每条改动给出「查看 ACP 原文」的诊断入口。服务端随事件下发了**结构化差异行**时逐行呈现并区分新增、删除与上下文行（行内容按不可信文本处理）；未下发时明确说明原因（原文未同步或超出单条上限），不得渲染占位差异行假装完整，也不得为了填补空白而解析 ACP 原文并据此渲染。差异行与展示路径同源，都由变更 `sync-path-browse-and-diff-delivery` 下发（`file.changed` 视图的 `diffLines` / `diffUnavailable`）；客户端不得绕过 wire 自行构造或推导这两类取值。
- 对话页三栏与宽窄两形态：宽屏呈现**三栏**——左栏（当前会话所属目录的上下文与会话列表：目录展示名与路径、进入目录详情页的入口、只作用于左栏的搜索、会话行、已按批呈现数与继续加载、吸底的新建会话入口）、中栏（会话头、连接状态条、时间线、输入区）、右栏（变更文件）。窄屏把左栏与右栏降为**可切换的次级视图**（时间线 / 同目录会话 / 变更文件，四列内容始终在 DOM，未选中的列只是不占主体），两种形态必须呈现同一信息集合，不得因窄屏静默丢弃信息或改变判定结果。左栏的搜索不得改变中栏当前会话的呈现；当前会话没有目录归属时，左栏明确说明它没有目录归属，不得因此隐藏中栏。
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

托管与协议端点的**解析优先级**：已注册的协议 path（`/sync/v1`、`/node-link/v1` 及两者的配对端点）先命中；只有**未命中任何协议端点**的请求才由静态面接住。协议族**保留前缀**之下的未注册子路径仍按协议面语义返回 404，不被静态面接管（静态端点与 `daemon.port`/`daemon.bind` 的关系见 `CONFIG_REFERENCE.md` §1）。

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

`[实现现状]`（2026-10-18，`device-pairing-qr-reference`，取代下一段的历史实绩）设备配对秘密在 PWA 只经**配对码 lookup** 进入；设备 `q2=` 与旧 `data=` 片段在读取后立即用 `history.replaceState` 清除，但**不解析、不兑换**，也不把 `qrToken` 放进 store 或任何可导出状态。扫码取回凭据只属于未来原生 App 的内存会话；PWA 不发送 qr-lookup。刷新后裸 `/pair` 恢复配对码输入，旧片段不从历史或存储恢复。页面 runner 的 PW-1/PW-1b 用例同时断言载荷/秘密不回显、地址栏不留片段，且四个配对端点零请求。

`[实现现状]`（2026-10-16，`frontend-sync-wiring`；设备 fragment 入口已被上一条取代，以下仅记录当时的实绩）配对秘密的清理口径落在组合根与 store 上（`clients/app/src/composition.ts`、`src/features/pairing-model.ts`、`src/features/client-store.ts`）：秘密只经两条受控入口进入：地址片段或配对码 lookup。fragment 入口由组合根 `takePairingFragment` **先读后清**——只对 `data=` 形态调 `history.replaceState`，且新地址保留 pathname 与 search（其余形态的片段既不解析也不清除，避免清掉别人的语义），因此秘密不留在地址历史里；配对码入口用一次性 8 位码换回 secret，码与 secret 都不进入 URL、Cookie、localStorage 或 sessionStorage。读到的载荷只活在 `ClientStore` 的**私有**会话字段里，已批准或进入终态时立即丢弃。配对进行中由 store 按本次配对的 `expiresAt`（二维码入口来自载荷，配对码入口来自 lookup 响应）安排一次到期回调：到期即取消轮询、丢弃会话并落到对应终态（二维码入口为「地址已过期」，配对码入口为配对码无效/已过期呈现），页面只负责按同一截止时间显示倒计时。`ClientState` 是可导出、可快照的公开状态，其中没有承载秘密的槽位，秘密因此不进缓存摘要、可导出状态与出站 URL。来源与 secure context 校验已一并落地（`pairing-origin-secure-context`）：**地址片段入口**（`ClientStore.startPairingFromAddress`）在读完载荷之后、发任何请求之前，依次校验**安全上下文 → 来源一致 → 载荷类型**，任一条不过都明确失败且**一个 claim/status 都不发**。**配对码入口**在 lookup 之前只校验**安全上下文**与**码形状**（规范化后为 8 位 Crockford 字母表内的字符，不合法即明确失败、不发 lookup）：它没有载荷可读，因此没有来源一致与载荷类型这两步；来源/Host 由服务端在 lookup 上按登记的 canonical origin 校验。环境事实（是否安全上下文、当前来源）经组合根 `CompositionInputs` 注入（默认读浏览器全局 `isSecureContext` 与 `location.origin`，与 `fetch` 同属一次性能力的注入面，不动 `src/platform/**` 的冻结成员表）；来源比较复用 §3.2 的 canonical origin 口径（浏览器 `URL.origin`），不另造字符串比较。失败码取自封闭词表：来源不符复用既有的 `auth.origin_mismatch`，非安全上下文用新增的 `auth.insecure_context`（已按 §6/§9 的同步清单登记 `compatibility/errors/v1/errors.json`、`schemas/sync/v1/common.schema.json` 的 enum 与 `docs/SYNC_PROTOCOL.md` §12.2）。非安全上下文明确拒绝，不回退到可导出形式的密钥、也不静默降级。

完整决策和信任边界见 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)。Noise 不进入第一阶段，仅保留为未来不可信中继场景的可选 Transport Profile。

第一阶段浏览器验收覆盖 Android Chrome、iOS Safari 和桌面 Chrome/Edge。HTTPS Provider 必须同时通过 Tailscale Serve 和至少一种非 Tailscale 同机方案。

PWA 不加载第三方脚本、字体或 CDN 资源。CSP 最低基线为 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`，Agent 产生的 Markdown、终端、diff 和工具内容全部按不可信输入渲染。

`[实现现状]`（2026-10-16，`ui-csp-inline-styles`）入口文档的**样式形态**已按上述基线落地：`expo export` 生成的入口文档里原有的内联 `<style id="expo-reset">` 被抽出为同源 `/entry-reset.css`，并在 bundle 的 `<script defer>` **之前**声明一份**必须保持为空**的同源 `/rnw-stylesheet.css`——`react-native-web`（RNW）于模块求值期按 id 复用该 `<link>` 的 `sheet`，把它的初始 reset 与全部原子类 `insertRule` 进去，因此页面不再依赖任何内联样式形态。两个文件都由 Daemon 在同源根路径托管（`MIME_TYPES` 已含 `css → text/css; charset=utf-8`，托管层与响应头一字未动），样式由此在**既定 CSP** 的同源托管下完整交付（此前「同源托管下前端无样式」的缺口就此闭合）；**这不是放宽 CSP**（`insertRule` 不是 CSP 的检查点）。取舍、失败形态与判据见归档的 `openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md`（D1/R2/R3）。**硬边界**：`/rnw-stylesheet.css` 必须保持为空，写进内容会让 RNW 的 hydration 遍历到无 `[stylesheet-group]` 标记的规则并抛错（整页空白）；link 必须排在 bundle 之前（RNW 取 `link.sheet`，未加载完时为 `null` 且不重试）；回归判据是浏览器级的**计算样式**断言，而不是「DOM 里有 `<style>`」。

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
- workspace 目录与 Agent profile 的引用缓存（目录标识 `directoryId`、服务端下发的 `path`、Agent 标识与默认标记），用于离线渲染目录页与创建选择器；只存服务端下发的引用，不存客户端规范化后的路径（§2.2 与 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.5）。目录项随快照重建校正，清除后重新同步即可。

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

- Sync Protocol 类型与契约测试（含设备短二维码 `q2` 解码与共享固定向量）。
- 设备扫码取回合同：固定二维码外层 canonical origin；经 `qr-lookup` 取回后先匹配完整主机公钥指纹，再走 claim/status；HTTP/WSS 显式携带同一 Origin，重定向直接失败；暂时故障有限重试（最多 3 次、总预算 30 秒、429 按 `Retry-After`），格式/来源/指纹/认证失败立即停止。字段与限额以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.6 为唯一权威。
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

`[实现现状]`（2026-10-18，`device-pairing-qr-reference`）共享解码、指纹与重试合同的消费方已有测试调用方（`clients/app/src/features/native-qr-consumer.ts` 及其伪时钟/模拟传输测试），并以模拟原生消费方对**真实 Daemon**走完取回/指纹/claim/status/WSS（`crates/app/tests/device_qr_reference_e2e.rs`）。**原生真机未执行**：Android/iOS App、相机权限、深链、安装分发、原生 keystore 与真机扫码均不在本轮交付；PWA 不接入该消费方，仅保留配对码入口。

## 9. 第一阶段验收标准

PWA MVP 至少满足：

1. 可以查看和操作当前节点可见的已有会话；会话创建是**可用**入口：目录引用取本机已登记的目录，或**从主机的根列表逐层浏览后选定**的一层，Agent 只能从本机已配置的 profile 中选择，提交的 payload 只允许三类字段——`agentId`、目录引用（目录标识或客户端提供的目录，二者之一）与一个可选的「目录不存在则先创建」开关——客户端无法通过 UI 或构造普通命令提交目录追加、MCP 配置或凭据，本机规范化后的路径也不得被回显。入口**不按权限门控**（设备方向没有授权维度，服务端以「已认证」为唯一前提），页面 MUST NOT 出现「没有创建会话的权限」「需要电脑端授予」的说明；目录与 Agent 目录在离线状态下仍可渲染（数据来自快照）。该入口的交互口径以 [`prototypes/acp-remote-pwa.html`](../prototypes/acp-remote-pwa.html) 为原型来源：目录页（`#/dirs`）、目录详情（`#/dir/work-api`）与「在此目录新建会话」弹层——目录为主角的列表、按时间倒序分页的详情、只列出本机已配置 Agent 的选择器与「将在你的电脑上启动一个 Agent 进程」的副作用提示；原型里「这台设备没有创建会话的权限，需要电脑端授予」的降级文案**不再**属于本期行为（该文案随设备权限层一起删除）。
2. 设备配对在 PWA 中**只使用配对码**：`/pair` 无片段时显示 8 位配对码输入（S0：显示当前 canonical origin 与 HTTPS 标识、8 个单字符格，大小写不敏感、`I`/`L`→`1`、`O`→`0`、可粘贴 `XXXX-XXXX`），提交后经同源 lookup 取回绑定与配对秘密并复用同一套 claim/status；打开新设备 `q2=` 片段只清除片段并显示 App 指引，打开旧设备 `data=` 片段清除并提示已停用，两者**都不发** lookup/qr-lookup/claim/status。`/node-link/pair` 仍只接受 `data=` 片段并只呈现对端信息、不发起 claim/status；无片段时呈现节点专用文案「这个地址里没有节点配对信息」且不提供配对码输入。配对码输入页 MUST NOT 显示数字倒计时，只显示静态提示“配对码 5 分钟内有效。”；lookup 成功后按服务端响应的 `expiresAt` 启动倒计时。lookup 的任何失败（含响应丢失）统一呈现「配对码不对或已过期，请重新生成配对码」，不自动重试、不提供页内重试；配对码不进入 URL、Cookie、localStorage、sessionStorage、地址历史或日志，并只在页面内存中保留到终态或卸载。页内不绘制页头与页内返回，全局顶栏仍不呈现；配对中显示由服务端 `expiresAt` 计算的真实剩余有效期 `m:ss`；配对码入口归零（含 lookup 后的倒计时）时立即停止轮询并进入「配对码不对或已过期」，lookup 返回已过期时同样进入该呈现；审批通过后约 1.6 秒自动进入目录页（路由 `/`），不要求连接已在线；载荷类型与入口不匹配、`/node-link/pair` 无载荷各呈现专用固定文案且不发请求；失败页覆盖十类固定原因（被拒绝、已过期、已被使用、无法识别、来源不一致、非安全上下文、请求未能完成、载荷类型与入口不匹配、没有载荷、配对码不对或已过期），不提供任何按钮。在此前提下能完成配对、认证、订阅、历史追平和实时切换。
3. 网络断开后自动重连，并从最后 ACK cursor 补发，不重复显示事件。
4. prompt 重试不会导致 Agent 重复执行。
5. 能正确显示流式消息、权限请求、工具调用、终端和配置项与模式变化（含 `category=model`）的最低结构化视图。
6. 未知 ACP 扩展不会丢失、崩溃或被伪装成普通文本。
7. 慢速渲染或后台标签页不会阻塞 Agent 和其他客户端。
8. 浏览器数据清理后不会继续冒充原设备，而是要求重新配对。
9. Owner 离线时 imported 会话只显示元数据与 `origin.online=false`；正文请求返回 `resource.remote_unavailable`；输入不得标记为已发送。
10. PWA 构建可以由 Daemon 本地托管，不需要应用级云服务器。
11. 三页与共享外壳的呈现口径与原型一致（差异只剩 `pwa-prototype-parity` 登记的三处有意偏离）：全部路由**共用一份外壳**（顶栏 + 开发模式横幅 + 帮助抽屉 + 全局提示），连接状态只由设备胶囊表达；主题三态（跟随系统 / 白天 / 黑夜）可持久化、跟随系统时随系统变化；会话时间按相对口径（刚刚 / N 分钟前 / 日期）由客户端算出；目录详情按时间倒序、每页固定条数分页；对话页宽屏三栏、窄屏经次级视图切换，两形态的信息集合相同。**前置条件（已满足）**：展示路径与结构化差异行两类取值由变更 `sync-path-browse-and-diff-delivery` 下发（快照 `workspaces` 的路径、`workspace.browse` 的目录浏览，以及 `file.changed` 视图的 `diffLines` / `diffUnavailable`），本客户端不得绕过 wire 自行构造或推导；该变更**已合入并归档**（归档于 `openspec/changes/archive/2026-10-17-sync-path-browse-and-diff-delivery/`），因此这三处不再按「数据缺席」降级——路径行、逐层浏览与差异行右栏的客户端消费由 §9 记录的浏览器级用例覆盖（`R16-2` / `R16-5` / `CP-5`）。未协商相应能力的连接仍走既有降级路径（不显示路径、浏览入口说明不可用、差异区说明未下发）。

`[实现现状]`（2026-10-16，`frontend-sync-wiring`；同日 `node-link-pair-route` 在其后追加两条入口区分用例）上述条目里属于前端接线的部分（第 2 条的配对链路与连接握手，即 §4.1 记的那组实绩）已有可执行证据（前端门禁与假主机 e2e；真实 Daemon 上的浏览器级端到端验收见段末）：`cd clients/app && npm run check` = 59 files / 725 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = 51/51（既有 49 条 + PW-1、PW-2；真实 Chromium + 假主机，含出站 URL 路径、升级请求声明的子协议、已批准后落盘的主机公钥、配对入口存在与可用这四条可证伪断言）。`node-link-pair-route` 在同一处追加两条：打开 `/node-link/pair#data=…`（node-link 载荷）落在可用入口（对端信息可读、不跳转、一个请求都不发），以及两类入口交叉喂错内容都明确失败。上面两组计数是 `frontend-sync-wiring` 的实测值；本变更追加用例后的计数以本轮门禁实测为准。**已执行**：第 2 条所依赖的「真实浏览器 + 真实 Daemon（同源托管）」端到端验收（完整配对、已认证连接、目录与会话快照、断开按游标恢复）已于 2026-10-16 在本机 Windows x64 用真实 Chromium + 真实 Daemon（同源 https 托管）执行完成，7 项逐条实测输出（1–6 通过；第 7 项「内联样式与 CSP 冲突」如实登记为待裁决，其后由变更 `ui-csp-inline-styles` 闭合）与同批修掉的两个缺陷（WSS 扩展协商导致的连接拒绝、生产构建里共享策略符号 `undefined`）见 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录（2026-10-16，本机 Windows x64；验收已完成，逐条给实测输出）」小节；第 2 条据此已在真实 Daemon 上验过。**仍存的边界**：该验收**不在 CI 中运行**，它需要本地真实 Chromium 与真实 Daemon、同源 https 托管与一次真实配对（本地管理通道确认），因此是一次性的实测记录，而不是持续门禁。

`[实现现状]`（2026-10-17，`pwa-prototype-parity`）第 11 条的**呈现口径对齐**已落地：三页与共享外壳按原型重排——顶栏、开发模式横幅、主题三态、帮助抽屉与全局提示由共享外壳统一承载，连接状态只由设备胶囊表达；目录页新增「在此目录新建会话」与文件夹浏览入口，目录详情改为按时间倒序分页，对话页改为宽屏三栏/窄屏次级视图并接入右栏差异行；与原型的差异只剩该变更在归档 `proposal.md` 里登记的有意偏离。证据：本机跑 `cd clients/app && npm run check` = 69 files / 994 passed + `expo export` 成功，`node e2e/run-e2e.mjs` = **74/74**（含 `UI-SHELL`、`R15-*`、`R16-*`、`R18-*` 与 `CP-*` 检查集）。**仍存的边界**：该组浏览器用例与 §9 那次真机验收一样**不在 CI 中运行**（CI 无 Chromium），需要本地真实 Chromium 手工跑；该变更 `plan.md` 的 Main E2E 块没有记录实测输出（`reason`/`basis` 为空）、计划中的「与原型并排逐页核对」也没有留下差异清单，因此上面两行是本轮主检出的复测值，而不是该变更自留的证据；该变更登记的「刷新 `prototypes/README.md` 与 `prototypes/IMPLEMENTATION-GAPS.md` 的现状记录」尚未执行（它们不是权威文档）。

`[实现现状]`（2026-10-18，目录页与目录详情页按桌面原型补齐视觉与行为细节）：目录卡片始终单列堆叠（不再随宽屏并排），卡片头自成一栏（`panel-2` 底 + 下边框），折叠箭头不带底色块；`⊕ 新建` 改为淡主色底+主色字，`查看全部 ›` 改为弱化文字，顶部 `⊕ 新建` 改为实心主色底白字，搜索框带图标且最大宽度 420。卡内会话行按当前搜索/筛选收窄（展示名命中保留全部按筛选后的行，只命中标题时只留命中行），超过 5 行时卡体底部再给一个「查看全部 ›」；「一个目录都没有」的空态标题改为「还没有可用的目录」；工具条在所有状态都保留搜索、状态筛选与右上角创建入口，空态块内另给一个常规尺寸的同一入口。任意目录创建仍走「逐层浏览主机目录 + `createIfMissing`」那条链路。详情页页头改为描边次级「目录」按钮 + 21px 目录名 + 会话数胶囊 + 路径等宽值，会话行直接铺在页面上。主机与连接面板不再内联在目录页（原型只经设备胶囊打开），本实现由设备胶囊进入 `/hosts` 路由；宽屏内容区两侧的留白铺当前主题底色，不再露出路由容器的默认白底。证据：`cd clients/app && npm run check` = 74 files / 1063 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = **81/81**（含 `R15-*`、`R16-*`）。

`[实现现状]`（2026-10-17，`pairing-page-open-url`；2026-10-18 由 `pairing-code-web-entry` 扩展；其中设备 `#data=` 自动认领已被下一条 2026-10-18 `device-pairing-qr-reference` 的实绩取代）第 2 条的配对页形态已按上述口径落地：`/pair` 与 `/node-link/pair` 共用同一状态页；有 `data=` 片段时读到后立即清除，`/pair` 的 sync 载荷自动开始认领，`/node-link/pair` 的 node-link 载荷只呈现对端信息、不发起 claim/status。`/pair` 无片段时显示 8 位配对码输入（S0），lookup 之前不发 claim/status，也不显示数字倒计时（只显示静态提示“配对码 5 分钟内有效。”）；lookup 成功后改用响应里的 `expiresAt` 启动倒计时。`/node-link/pair` 仍只接受片段，无片段时走「没有载荷」文案，不提供配对码输入。页内不绘制页头与返回，全局顶栏仍不呈现。二维码入口的倒计时取自载荷 `expiresAt`，归零与打开即过期都停止轮询并进入「地址已过期」；配对码入口在服务端返回过期或 lookup 失败时进入固定失败呈现。审批通过后约 1.6 秒自动进入目录页（路由 `/`），连接装配失败仍进目录、由设备胶囊表达断开与重连。失败页十类各走固定文案（含 `/node-link/pair` 无载荷、载荷类型不匹配与配对码无效三种专用呈现），不渲染后端 `message`、无任何按钮，恢复路径是电脑重新生成地址或配对码。配对页颜色全部来自当前主题调色板；右上角提供复用共享偏好的明暗切换按钮（`PairingThemeToggle`），不建立第二份主题状态。`pairing-page-open-url` 本身后端零行为变更；本轮 `pairing-code-web-entry` 新增 lookup 端点、`pairing.code_invalid` 与 `device.pair.begin` 的 `pairingCode`，wire 细节以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.1/§7.5 与 §12.2 为准，本文不重复。

`[实现现状]`（2026-10-18，`device-pairing-qr-reference`，WP5）第 2 条的设备入口已按「只使用配对码」落地：`/pair` 无片段为 S0；`#q2=` 与旧 `#data=` 都在任何业务动作前清除片段，分别显示固定 App 指引与「旧格式已停用」，两者零 lookup/qr-lookup/claim/status。证据（假主机页面回归，**不替代真实 Daemon**）：`cd clients/app && node e2e/run-e2e.mjs` = **81/81 通过**，含 PW-1/PW-1b、NL-1/NL-2/NL-3、PW-2/PW-2b/PW-6..PW-10；真实 Daemon 的 C 链路由 `crates/app/tests/device_qr_reference_e2e.rs` 以模拟原生消费方覆盖（短二维码 → qr-lookup → 65 字节公钥与 SHA-256 指纹 → claim → 本机确认 → status → 携带同一 Origin 的 WSS 四消息认证），含丢包重试同材料、认领后取回 401、重复 claim 幂等、不同内容冲突、错误指纹/来源/绑定不发 claim，以及日志/进程输出不含完整 `qrToken`/`pairingSecret`。**真实浏览器对真实 Daemon 的 Main E2E 已执行**：桌面 1280×900 与手机 390×844 视口分别用配对码完成本机确认并在线进入目录；`q2=`/旧 `data=` 片段先清除再显示固定文案且零 lookup/qr-lookup/claim/status，`/node-link/pair` 只读展示且零请求，主题偏好刷新后保留。**原生 App 真机未执行**（相机、keystore、安装分发与真机扫码不在本轮）。

### 9.1 发布、缓存刷新与回滚

- **配套发布**：Daemon 与前端静态资源必须同版本发布。新 Daemon 只生成 `q2=` 短地址；旧前端即使在缓存里，也不会被新二维码触发自动认领——但**已经打开的旧标签页**在刷新前仍运行旧代码，部署后必须提示/强制刷新（硬刷新或清站点数据），不能只改服务端生成器就宣称安全。
- **缓存与 Service Worker**：前端静态资源由 Daemon 以 `cache-control: no-store` + `pragma: no-cache` 下发，入口文档通过内容哈希引用 bundle；本客户端**不注册 service worker**，因此没有可继续运行的旧离线版本。浏览器仍持有旧页面时使用硬刷新/清站点数据后重开 `/pair`。
- **未完成配对**：配对秘密与扫码取回凭据只驻留内存；Daemon 重启使未完成配对失效，必须在电脑上重新生成配对码/二维码，不能凭旧地址恢复。
- **已有身份**：已批准的设备信任、本地数据库与会话不迁移、不要求重新配对；本变更不新增数据库字段。
- **回滚**：整体回滚 Daemon 与前端到**配套**旧版本；不保留双栈解码、新旧二维码格式不混用；未完成配对在重启后失效并需重新生成；已批准信任不因回滚被撤销。旧二维码跨版本不再保证可用，恢复路径是重新生成。

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

## 独立目录创建（ADR-0010）

「新建」立即创建主机目录，保持选择器打开且不自动确认选择；成功后重新浏览父层，从真实条目取得路径并自动进入新目录，方便继续创建下一级，最后单独选择。刷新失败或截断须与创建成功分开显示，无法取得真实条目时不伪造路径跳转；旧服务端未协商时明确显示不支持独立新建。关闭弹层或取消会话不会删除目录。命令状态与兼容规则见 SYNC_PROTOCOL §11.5、[ADR-0010](adr/0010-immediate-workspace-directory-creation.md)。

新建会话表单的目录为必填，未确认目录时点击创建会话，将目录行的「未选择目录」标红并阻止提交；选定后清除错误状态。不显示额外的必填提示、常驻的未选目录操作说明或未登记目录归属提示。未手动选择 Agent 时选中主机声明的默认 Agent；用户改选后保留其选择，无默认 Agent 时由用户选择。
