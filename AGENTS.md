# AGENTS.md

本文件适用于整个仓库。它是后续使用 Codex、Oh My Pi 等编码 Agent 进行 vibe coding 时的长期执行约束。

## 1. 开始工作前

在修改代码或设计前，先阅读与任务相关的文档：

- [docs/INITIAL_DESIGN.md](docs/INITIAL_DESIGN.md)：产品目标、系统行为、安全与同步策略的权威来源。
- [docs/MODULE_ARCHITECTURE.md](docs/MODULE_ARCHITECTURE.md)：模块名称、职责、依赖方向和 crate 边界的唯一权威来源。
- [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md)：前端阶段、客户端行为、状态模型和平台适配边界的权威来源。
- [docs/SYNC_PROTOCOL.md](docs/SYNC_PROTOCOL.md)：客户端与 Daemon 之间认证、消息、游标、重放、幂等和 wire schema 的权威来源。
- [docs/NODE_LINK_PROTOCOL.md](docs/NODE_LINK_PROTOCOL.md)：ACP Remote 节点之间资源导出/导入、权威、认证、授权、重放和幂等边界的权威来源。
- [docs/LOCAL_ADMIN_PROTOCOL.md](docs/LOCAL_ADMIN_PROTOCOL.md)：CLI 与 Daemon 之间本地管理通道的请求/响应编码、framing 与方法集的唯一权威来源。
- [docs/SECURITY_DESIGN.md](docs/SECURITY_DESIGN.md)：系统威胁模型、信任边界、授权、数据保护、供应链和安全验收的权威来源。
- [docs/ACP_COMPATIBILITY_MATRIX.md](docs/ACP_COMPATIBILITY_MATRIX.md)：ACP v1 覆盖范围、各层处理策略和兼容性验收矩阵的权威来源；机器合同位于 `compatibility/acp/v1/matrix.json`。
- [docs/CONFIG_REFERENCE.md](docs/CONFIG_REFERENCE.md)：Daemon 配置键名、类型、默认值与可否调整的唯一权威来源；协议层限额仍以 Sync、Node Link 两份协议文档为准。
- [docs/CORE_PORTS_AND_STORAGE.md](docs/CORE_PORTS_AND_STORAGE.md)：`core::model` 值对象、`core::use_cases` 用例面、`core::ports` 端口签名、broker 事务顺序与 `storage-sqlite` v1 表结构/保留/migration 的唯一权威来源。
- [docs/adr/](docs/adr/)：已经接受的架构决策；相关 ADR 优先于仍保留的早期候选描述。

同时检查现有代码、测试和工作区状态。不要假设文档中的规划已经实现，也不要覆盖用户尚未提交的修改。

如果实现需要改变产品边界、协议语义、安全模型或模块依赖，先明确指出影响并请求确认。普通的模块内部实现细节可以自主决定。

## 2. 项目定位

ACP Remote 是运行在用户控制节点上的本地优先 ACP 中转站：

- 一个节点可以直接管理本地 Agent，也可以通过 Node Link 导入其他节点导出的 Agent；同一节点可以同时承担 Owner 和 Access 角色。
- 每个 Agent/会话的 Owner Node 是该资源的唯一权威；默认 `no-content-cache`，Access Node 只持久化来源引用、cursor、ACK、requestId、命令终态引用和 local sequence 映射，不持久化 prompt、回复、diff、终端或 ACP raw 正文。
- 手机、电脑、Zed、PWA 和 CLI 是客户端形态，不是固定权限角色；能力由 principal scope、Export Policy 和端到端 capability 决定。
- Node Link 首个纵向切片必须支持受 `remote-work`、已导出 Agent 和 workspace template 约束的远程 `session.create`，以支持 Zed 的正常 `session/new`；当前 PWA MVP 可以不展示创建入口。
- Zed 是可选客户端，项目不得与 Zed 强绑定。
- 对直接管理的 Agent，Owner Node Daemon 是其唯一 ACP Client，负责 Codex、Oh My Pi 等 Agent 的生命周期；Access Node 通过 Node Link 访问，不直接接管远程 Agent stdio。
- 第一阶段不提供应用级云服务器或持久云中继；Owner Node 离线时，远程控制不可用。
- LAN、Tailscale 或其他网络路径都是可替换的部署方式，核心不能依赖 Tailscale SDK、CLI 或身份体系。
- 核心、Daemon 和 CLI 使用 Rust；npm 只负责分发预编译二进制，最终用户不应被要求安装 Rust 工具链。
- Node Link 是首个产品纵向切片；前端首个交付仍只实现由 Daemon 本地托管的 Web/PWA，Android/iOS 原生客户端属于后续阶段。

不要在没有明确需求的情况下引入账号中心、云数据库、遥测平台、第三方消息中继或厂商锁定的网络能力。

## 3. 最高优先级不变量

### ACP 兼容性不变量

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

因此：

- ACP 标准能力保持原有语义。
- 未知字段、未来协议字段和 Agent 扩展 payload 必须能够往返保真；Sync 边界使用协议规定的 `acp.rawJson`，不得先经 JavaScript number 或通用 DTO 解析后再声称原文保真。
- Broker 只为业务决策创建公共领域视图，不能把该视图当作完整 ACP 消息。
- 权限请求、工具调用、终端、文件修改、diff 等结构化事件不能静默转换成普通文本。
- 能力协商必须如实反映端到端实际能力；不能虚报支持。
- 无法安全处理的能力必须明确返回不支持或协议错误，不能静默忽略。

### 会话与消息不变量

- 同一个会话同时最多有一个 active turn；不同会话可以并行。
- Owned Agent 事件必须由 Owner 的 `SessionStore` 先持久化成功，再向客户端广播；imported 事件已经由 Owner 持久化，Access 必须先提交无正文 `RemoteDeliveryStore` 收据再向本地客户端广播。
- `persist_deltas: false` 只允许 turn 完成后压缩或清理短期 delta，不允许未持久化就广播带 sequence 的事件。
- 事件采用至少一次投递，客户端按稳定 `eventId` 去重。
- 可重试命令必须携带稳定 `requestId`；客户端重试不得造成第二次接受或派发，外部 Agent 崩溃窗口无法确认时必须显式进入 `uncertain`。
- 断线恢复依赖持久化 sequence/cursor，不依赖进程内消息队列。
- 一个状态只能有一个权威写入者。

### 安全不变量

- 网络可达不等于应用授权，Tailscale 身份不能代替 ACP Remote 设备或节点身份。
- 首次扫码用于建立长期设备/节点信任；正常重连不要求重复扫码。
- 默认安全 Profile 是可信 HTTPS/WSS + ECDSA P-256 身份签名 challenge-response；TLS 终止点必须位于对应节点主机的可信边界内。
- 长期 Node/设备签名密钥和 TLS 单次连接临时密钥必须分离。
- Node/设备 wire signature 固定为 64-byte P1363 `r || s` + 无填充 base64url；签名/HMAC 不直接使用普通 JSON，必须使用对应协议定义的域分离长度前缀 transcript。
- 一个 PWA 设备身份只绑定一个 canonical origin；Origin 变化必须重新配对，禁止通过导出私钥实现跨 Origin 迁移。
- 每个新 WSS 连接完整执行 challenge-response，不引入长期 bearer session/refresh token。
- 每个业务命令都要根据已认证设备及其 scope 授权。
- 私钥和 token 不得写入日志、协议错误、测试快照或普通 SQLite 字段。
- 使用经过审查的密码学协议和实现，不自行设计加密算法。
- Noise 不进入第一阶段；只有经单独 ADR 接受后才能新增为可选 Transport Profile。

## 4. 模块与依赖规则

计划中的主要边界为：

```text
core
acp-protocol
sync-protocol
node-link-protocol
acpr-transcript
acpr-wire
agent-host
node-link-client
storage-sqlite
identity-auth
identity-keystore
server
app
```

物理 crate 采用 Pi 风格的粗粒度边界：`core` 内含 model/use_cases/ports/broker，`server` 内含 sync/node_link/acp_facade/local_admin，`app` 内含 daemon/CLI/组合根。协议因兼容周期独立而分别建 crate。只有需要阻止反向依赖、独立发布或拥有独立协议/平台实现时才继续拆 crate；平级模块共享的底层实现下沉为叶子 crate（`acpr-transcript`，见 `docs/adr/0005-shared-transcript-codec.md`；跨协议共用的 wire 值对象与校验机制见 `acpr-wire`，`docs/adr/0007-shared-wire-value-crate.md`），不通过横向依赖复用。平台实现同样单独成 crate：`identity-keystore` 只为隔离平台 keystore 依赖而存在（`docs/adr/0006-identity-keystore-split.md`）。

依赖必须指向更稳定的内层：

```text
app              -> server / backends / identity-auth / core
server           -> core + wire protocols
outbound adapters-> core + wire protocols
core use_cases   -> core ports + core model
```

必须遵守：

- `core` 不依赖任何 wire protocol、Tokio runtime、Axum、SQLite、WebSocket、子进程、ACP DTO 或具体 Agent。
- `acp-protocol`、`sync-protocol`、`node-link-protocol` 不依赖 `core`；wire/core mapper 属于对应 adapter。三个协议 crate 彼此不直接依赖：共享的 transcript codec 结构与表驱动校验来自叶子 crate `acpr-transcript`（协议 crate 只导出自己的 `DOMAINS` 表并正常依赖它，不复制宽度与校验逻辑），跨协议共用的 wire 值对象与字段校验机制来自叶子 crate `acpr-wire`（`docs/adr/0007-shared-wire-value-crate.md`）；协议专属的值对象与词表留在各自 crate。
- `server::sync`、`server::node_link`、`server::acp_facade`、`server::local_admin` 是平级入站适配器，只调用 `core::use_cases`，不能互相调用。
- `storage-sqlite`、`agent-host`、`node-link-client` 等出站适配器之间不能互相调用。
- `identity-auth` 是纯状态机，不得依赖任何平台 API 或 `cfg` 平台分支；平台 keystore 由 `identity-keystore` 实现 `identity-auth` 定义的端口，依赖方向只能是 `identity-keystore -> identity-auth`，且除 `app` 外没有 crate 依赖 `identity-keystore`。
- 需要其它模块的能力时通过端口或函数签名传入（组合根装配），不要 import 隔壁模块的实现。
- ACP DTO 只存在于 ACP 边界，Sync DTO 只存在于同步协议边界，数据库 record 只存在于 SQLite 适配器。
- 禁止定义巨型 `AgentRuntime`；本地和远程 backend 通过 `AgentCatalog`、`SessionBackendFactory` 和会话级 `SessionEndpoint` 实现。
- Owned session 使用 `SessionStore` 单次事务提交状态、事件和幂等；imported session 使用无正文 `RemoteDeliveryStore`，不得复用 owned content 写入路径。
- `app` 是组合根，可以装配具体实现，但不能承载业务规则。
- 禁止创建万能 `common`、`helpers` 或 `utils` 模块；代码应放到拥有其语义的模块中。
- 不要为了复用几行代码破坏依赖方向。

## 5. 协议实现规则

### ACP 边界

- 将 wire DTO、原始/扩展 payload 和业务公共视图分开。
- 实现、声明或改变 ACP 能力前，必须更新并满足 `compatibility/acp/v1/matrix.json`；不得只在代码或散落测试中维护一份隐式支持列表。
- 解码与重新编码必须测试未知字段保真。
- Agent 特有能力优先通过 capability/extension 表达，不在核心中堆积 Agent 名称判断。
- ACP 版本变化应集中在 `acp-protocol` 和适配器边界处理。

### 客户端同步协议

- 协议必须显式版本化，并通过 feature negotiation 演进。
- 命令、事件、ACK、快照和错误使用明确的结构化类型。
- 查询命令同步完成；mutation 只同步接受，最终状态使用持久化 command terminal event。不得让 UI 自行猜测命令是否完成。
- 所有输入都有长度、数量、频率和资源限制。
- 慢客户端不能阻塞 Broker 或 Agent；断开后依靠 cursor 重放。
- 协议变更必须说明向前/向后兼容策略，并增加 fixture 或契约测试。
- Sync wire 变更必须同步维护 `schemas/sync/v1/` 与 `fixtures/sync/v1/`；Rust 和 TypeScript 必须消费同一 manifest，不能各自复制一套测试样例。
- 错误码、feature ID、命令名等封闭词汇表只能有一处机器定义：错误码在 `compatibility/errors/v1/errors.json`，feature ID 在 `compatibility/features/v1/features.json`，命令与 grant/pack 在 `compatibility/commands/v1/commands.json`；新增或修改时必须同步更新对应 schema enum 或协议文档表格、`compatibility/transcripts/v1/transcripts.json`（涉及签名/HMAC 字段时）与相应 fixture，并让 `npm run check` 通过。
- transcript domain 与字段 tag 表由 `compatibility/transcripts/v1/transcripts.json` 机器登记；`fixtures/*/v1/transcripts/` 的固定向量必须能由该表从 `input` 重新编码得到，否则视为实现或表格错误。

### Node Link

- Node Link 与客户端 Sync Protocol 是不同边界，不能直接复用 wire DTO 或把 ACP stdio 透明隧道化。
- Owner Node 保存 origin event/sequence、命令终态和会话正文；Access Node 默认只保存无正文的交付索引，并保留来源身份、cursor、摘要和 local sequence 映射。
- 第一阶段采用节点级信任：Owner 认证并授权 Access Node，Access 对本地 Zed/PWA/CLI 负责；`localPrincipalRef` 只用于审计归因，不是 Owner 直接认证的最终用户身份。
- 有效权限是 Owner Export grant、Access 本地授权和实际 capability 的交集；节点配对不产生传递信任。
- 第一阶段 imported Agent 只能提供给本节点客户端，禁止再次通过 Node Link 导出。
- 第一阶段 Node Link 必须把 Zed `session/new` 映射为受限远程 `session.create`；请求不得携带任意 Owner 路径或 Provider/MCP 凭据。
- 命令名与 payload 字段名以 [`compatibility/commands/v1/commands.json`](compatibility/commands/v1/commands.json) 与 `docs/SYNC_PROTOCOL.md` §11.5 为唯一来源；Node Link 与 Sync 共用同一批命令名，不得在节点协议里另造同义命令。
- 跨节点可重试命令保持稳定 requestId；崩溃窗口仍必须进入 `uncertain`，不能在 Access Node 擅自重试副作用。

### 前端客户端

- 使用 TypeScript 与 Expo/React Native 通用工程；前端首个交付只构建 Web/PWA，并位于 Node Link 首个纵向切片之后。
- 页面不能直接操作 WebSocket、浏览器数据库、SecureStore、SQLite 或平台生命周期。
- 协议 DTO、客户端领域状态和 UI view model 必须分离。
- 连接、认证、重放和命令状态使用明确状态机，不使用可能产生非法组合的一组布尔值。
- PWA 私钥不得存入 `localStorage`；浏览器安全能力不足时只能进入显式开发模式，不能静默降低正式认证要求。
- PWA 不能声称具有 Keychain/Keystore、后台常驻连接或可靠系统通知能力。
- 后续原生客户端复用协议、Sync Client、状态机和 feature 层，通过平台 adapter 替换密钥、缓存、相机和生命周期能力。
- 前端暂不支持的 ACP 能力必须明确降级，不得隐藏、曲解或静默文本化。

## 6. 持久化规则

- Owner Node 的 SQLite 是其本地 Agent/会话的权威事件日志。默认 `no-content-cache` 下，Access Node SQLite 只能保存 import、来源引用、cursor/ACK、幂等记录、命令终态引用和无正文交付索引；正文通过 Owner 重放获取，不把内存广播队列当作事实来源。
- 需要原子性的状态变化和事件追加必须放在同一事务中。
- schema 变更必须提供可重复执行或版本化 migration，并测试旧数据库升级。
- Owner Node 长期保存最终消息、关键结构化事件、权限结果、状态变化和必要摘要；本条不得被解释为允许 Access Node 默认复制远程会话正文。
- 流式 delta、终端噪声和大型附件使用 TTL、大小上限或压缩策略。
- 清理原始噪声不能破坏仍在承诺期内的重放、审计和 ACP 语义保真。

## 7. Rust 实现约定

- 优先使用清晰的所有权和具体类型；不要用无边界的 `serde_json::Value` 代替领域建模，但 ACP 扩展容器除外。
- ID 使用 newtype；不要在核心中到处传裸 `String`。
- 库边界使用有语义的错误类型；只在应用组合层使用通用错误上下文。
- 正常运行路径不得使用 `unwrap()`、`expect()` 或无说明的 panic。
- 异步任务必须有所有者、取消路径和关闭顺序，不能遗留 detached task。
- 外部进程必须处理启动失败、超时、取消、异常退出、stderr 和完整进程树清理。
- Windows 必须验证 Job Object 或等价的子进程树清理机制。
- 日志使用结构化字段并进行敏感信息脱敏；不要记录完整 prompt、密钥或认证 payload。
- 新增依赖前检查其必要性、维护状态、许可证、平台支持和安全风险。
- 保持公开 API 最小；第一阶段默认 workspace-private。

## 8. 实现工作流

一次改动尽量形成小而完整的纵向切片：

1. 确认要实现的用户行为及所属模块。
2. 找出会受影响的不变量、协议和持久化边界。
3. 先定义领域类型/端口，再实现适配器和组合代码。
4. 添加最小必要测试，包含失败路径和重连/重试场景。
5. 运行与改动相关的格式化、静态检查和测试。
6. 如果行为、协议或架构发生变化，同步更新权威文档。

Rust workspace 建立后，完成改动通常应运行：

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

如果某个命令因平台、缺少外部 Agent 或环境限制无法执行，必须在交付说明中准确写明，不能声称已经通过。

不要顺手重构无关代码。优先修改根因，并保持补丁范围可审查。

## 9. 测试要求

根据改动选择对应测试：

- `core`：状态转换、值对象、owned/imported 分流、每会话串行、事务提交和不变量。
- `acp-protocol`：官方 fixture、未知字段往返、扩展 payload 和版本兼容。
- `agent-host`：fake ACP child、超时、取消、崩溃和乱序响应。
- `node-link-client`：fake Owner、attachment generation、显式重连、origin 去重和 uncertain。
- `storage-sqlite`：migration、owned 原子提交、imported 无正文约束、TTL、容量限制和崩溃恢复。
- `identity-auth`：设备/节点配对过期、重放、密钥变化、无传递信任、撤销和错误权限。
- `server::sync`：认证、限流、backpressure、ACK 和断线续传。
- `server::node_link`：Export 过滤、attachment、节点认证、ACK 和撤销。
- `server::acp_facade`：ACP contract、能力协商真实性、扩展透传和会话重放。

普通 CI 使用可控的 fake ACP Agent。真实 Codex/OMP 测试属于可选兼容性套件，不应成为普通单元测试的硬依赖。

修复 bug 时，除非客观上无法稳定复现，否则先添加或同时添加能够覆盖该问题的回归测试。

## 10. 文档维护

- 产品行为和产品级同步策略变化：更新 `docs/INITIAL_DESIGN.md`。
- 威胁模型、信任边界、授权默认、数据保护或供应链要求变化：更新 `docs/SECURITY_DESIGN.md`；改变已接受密码学/传输决策时同时新增或更新 ADR。
- Sync wire schema、消息状态或兼容语义变化：更新 `docs/SYNC_PROTOCOL.md`、`schemas/sync/v1/` 和 `fixtures/sync/v1/`。
- crate、模块职责或依赖方向变化：更新 `docs/MODULE_ARCHITECTURE.md`。
- 前端阶段、客户端行为或平台边界变化：更新 `docs/FRONTEND_DESIGN.md`。
- ACP 方法、通知、content、capability 或各层支持状态变化：更新 `docs/ACP_COMPATIBILITY_MATRIX.md`、机器矩阵和相应 fixture；运行 `npm run check`。
- 节点角色、Export/Import、跨节点身份、授权、cursor 或命令语义变化：更新 `docs/NODE_LINK_PROTOCOL.md`，并同步安全、模块与协议测试资产。
- Node Link wire/资产变更：更新 `docs/NODE_LINK_PROTOCOL.md` + `schemas/node-link/v1/` + `fixtures/node-link/v1/`，并运行 `npm run check`。
- Sync wire/资产变更：更新 `docs/SYNC_PROTOCOL.md` + `schemas/sync/v1/` + `fixtures/sync/v1/`，并运行 `npm run check`。
- feature ID 或 feature 词表变化：更新 `compatibility/features/v1/features.json`、`docs/SYNC_PROTOCOL.md` §5.2 与 `docs/NODE_LINK_PROTOCOL.md` §11.3 的表格、以及相应 fixture，并运行 `npm run check`。
- core 端口签名、值对象、broker 事务顺序或 storage-sqlite 表结构/保留策略变化：更新 `docs/CORE_PORTS_AND_STORAGE.md`，并同步 `docs/MODULE_ARCHITECTURE.md` §4.1/§4.7 的职责描述。
- 配置键名、默认值、部署开关变化：更新 `docs/CONFIG_REFERENCE.md`；协议层限额变化仍按 Sync/Node Link 各自的规则维护。
- CLI 与 Daemon 之间的管理方法、envelope 或 framing 变化：更新 `docs/LOCAL_ADMIN_PROTOCOL.md`。
- `npm run check` 是本仓库唯一的机器校验入口（Node ≥ 20），串行运行 `scripts/` 下七个脚本：fixture 用 ajv 校验（sync / node-link / acp 三个 asset root）、错误码 registry 与四处定义一致、命令目录与 core 的 `required_grant` 镜像一致（`commands.json` ↔ 两个协议 schema ↔ SYNC §11.5 与 SECURITY §10.2 表格 ↔ `core::broker::required_grant`）、feature ID 词表三方一致（registry / 协议文档表格 / fixture）、`rawJson` 与 transcript 密码学固定向量重算、ACP 矩阵及上游快照 sha256 比对，以及 **crate 依赖方向门禁**（以 `MODULE_ARCHITECTURE.md` §5 的依赖矩阵为唯一判据，用 `cargo metadata` 校验每个 crate 的实际依赖，并硬约束 `core` 不引入 runtime/DB/HTTP/子进程/wire protocol 依赖）。改动合同资产或 crate 依赖后必须让它全绿。CI 尚未接入，这一条目前靠人工执行。
- `core` 的普通依赖闭包必须等于 `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13 冻结的 allow-list（`cargo tree -p core --edges normal` 的可执行 crate 名集合），由 `check:boundaries` 断言；新增 core 依赖必须同时改 allow-list、合同 §9 判据 13 与本条。
- 两份文档出现重叠时，保留一个权威定义，另一处只写概要并链接过去。
- 不要手工修改生成型架构图来代替源规范修改。
- 代码尚未实现的设计必须继续使用“计划”“建议”或“待验证”等措辞，不能写成已经存在的能力。

## 11. 完成定义

只有同时满足以下条件，任务才算完成：

- 行为符合用户需求和上述不变量。
- 依赖方向没有被破坏。
- 成功路径、失败路径及关键边界有相应测试。
- 相关检查已执行，或明确记录未执行原因。
- 没有泄漏敏感信息，也没有引入未经授权的云依赖。
- 影响设计的变更已经同步到权威文档。
