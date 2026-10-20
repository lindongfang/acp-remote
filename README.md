# ACP Remote

本地优先的 ACP（Agent Client Protocol）中转站：一个节点可以直接管理本地 Agent，也可以通过 Node Link 导入其他节点导出的 Agent；手机、电脑、Zed、PWA 与 CLI 都是客户端形态，不是固定权限角色。

## 仓库当前状态

**设计层完成，实现已开始。** 仓库当前包含权威文档、机器可读合同（JSON Schema）、跨语言固定向量（fixtures）、兼容性矩阵与校验脚本，以及一个随增量增长的 Rust workspace（`docs/MODULE_ARCHITECTURE.md` §3.1）：

| crate | 已实现范围 |
|---|---|
| `acpr-transcript` | 长度前缀 transcript codec 与表驱动校验（叶子 crate，无协议语义） |
| `acpr-wire` | 跨协议共用的 wire 值对象与字段校验机制（叶子 crate，无协议词表） |
| `core` | `core::model` 的值对象与不变量、`core::use_cases` 用例面、`core::ports` 端口签名、`core::broker`（每会话串行、active turn、幂等、交互仲裁、先提交后发布、失败关闭）；运行时依赖只有 `async-trait`/`thiserror`/`p256`/`sha2`（后两者用于 `PeerPublicKey` 的构造期点校验与指纹派生） |
| `storage-sqlite` | §7 的 `owned_*`/`imported_*` 表结构与 migration、保留窗口/容量清理（含 imported 家族度量）、附件内容寻址、崩溃恢复与只读失败关闭；实现 `SessionStore`/`ReadView`/`RemoteDeliveryStore`/`AttachmentStore`，以及 §7 管理表上的两个管理 store（`TrustStore`/`LocalConfigStore`：写集一事务提交、失败关闭、容量纳入） |
| `sync-protocol` | v1 的全部 18 个消息类型（信封与消息类型分派，`auth`/`sync`/`control`/`error`/`event`/`command` 六个家族 body）、33 个事件视图的类型化投影、13 条 Sync 命令的封闭词表（含 `session.create` 的 `{ agentId, directoryId 或 workspacePath（恰好一个）, createIfMissing? }` payload 与 `completed { sessionId, session }` 终态结果），以及配对 HTTPS 载荷（二维码 / claim / status / HTTP 错误体） |
| `node-link-protocol` | §9.3/§9.4 的 transcript domain/tag 表、v1 的全部 29 个消息类型（信封与分派，`handshake`/`catalog`/`resource`/`command`/`error` 五个家族 body）与配对 HTTPS 载荷 |
| `acp-protocol` | JSON-RPC 信封分类与方向/required 校验、ACP v1 wire DTO（`initialize`/`session/new`/`session/prompt`/`session/update` 的 11 种判别子/`session/request_permission`/elicitation 与 content block）、`RawDocument` 原文承载与逐字节回写、capability wire 形状、固定 v1 消息上限（1 MiB），以及由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的契约测试 |
| `identity-auth` | 身份、配对、签名与授权的**纯状态机**：设备/节点一次性配对（创建/认领/落定/过期/重启终结）、逐连接 challenge-response（挑战一次性、注入时钟 15 秒窗口、验签公钥只来自持久化信任、P1363 only）、无授权词表展开（已删除 `grant.*`；`local.*` 永不远程授予，设备与节点方向都只要求认证）与 10 个 transcript domain 的装配/验签/HMAC；不访问存储、不读系统时间、无平台 `cfg`，密钥与随机性经端口注入 |
| `identity-keystore` | 平台安全存储适配器：自研条目格式（`ACPK` 头 + 每条目盐 + 平台包裹的秘密值）、原子写、进程内 `p256` 签名（64 字节 P1363）；不再承载 Provider 凭据。Windows 走 DPAPI（当前用户 scope，附加熵绑定条目）；非 Windows 一律失败关闭（不建目录、不写文件、不退回进程内实现） |
| `agent-host` | 本机 ACP 子进程作为 `AgentCatalog`/`SessionBackendFactory`/`SessionEndpoint`：启动与监督、stdio 分帧、request id 与 ACP session id 映射、capability 协商与调用门控、权限/elicitation 转发、超时与取消、stderr 有界采集、Windows Job Object（Unix 进程组）进程树清理、profile 与凭据注入边界、wire→core mapper |
| `server` | 入站 adapter 的宿主（`docs/MODULE_ARCHITECTURE.md` §4.9），当前落地五条路径：`server::transport::local`（平台本地 IPC 的 endpoint 与访问控制——Windows Named Pipe 的 SDDL、Unix 的 `0700` 目录 + `0600` socket、连接后对端凭据校验，以及帧编解码、首帧 channel 绑定、未完成请求上限、`0x02` 在 facade 缺席期的失败方式）、`server::local_admin`（管理信封与 `local.*` 错误码的值对象/编解码、方法分发表与方法路由）、`server::transport::net`（共享 HTTP/WSS listener：`proxy` 默认 loopback、`direct` 默认全接口、TLS `proxy`/`direct` 两种终止方式、`Host` 边界，以及**三类端点**的 path 路由：ws 与 post 两类协议端点（`/sync/v1`、`/node-link/v1`、两类 `/pairing/*`），加上**根路径 `/` 的前端静态托管**——入口文档、单页应用回退、按扩展名的内容类型映射与安全响应头；静态面只是 router 的 fallback，因此协议端点天然优先、协议族保留前缀下的未注册子路径仍 404；嵌入资源表在 `transport::net::ui`，内容类型映射的唯一一份在 `transport::net::static_ui`）、`server::node_link`（配对 HTTP、节点握手与鉴权留痕、Export catalog、resource 快照/重放/ack、命令管线与终态推送、撤销传播）与 `server::sync`（`/sync/v1` 的 WSS 连接面：四消息握手与来源绑定、单设备替换、撤销传播、心跳与 close code 映射、有界出站账本；订阅与投递面：cursor 判定、增量重放与实时扇出的 barrier、快照与 `snapshotDigest`、累计 ACK；**业务命令面**：命令帧的解码分派、认证与撤销判定先于本机读取、六条查询命令、结果超限的显式失败、按设备与命令族的限流与连续超限关闭、七条 mutation 的接受路径、`(actor, requestId)` 幂等与冲突判定、`expectedVersion` 版本冲突、按连接存活的终态观察者，以及 `session.create` 的 feature 门控与 `uncertain` 收口；两条配对 HTTP 端点，由 `app` 组合根接线）。**`server::sync` 的命令面已接线**（`SYNC_PROTOCOL.md` §11）；仍未落地的是 `session.read` 的 `turns`/`pending_interactions`/`capabilities` 三类明细（本期回 `capability.unsupported_by_broker`）与 imported 事件的回源投递。**仍未落地**：`server::acp_facade`；Node Link 也**只有 Owner 侧入站面**，Access 侧的出站重连管理器属切片 6 的 `node-link-client` |
| `app` | 组合根与 `acp-remote` 可执行程序（`docs/MODULE_ARCHITECTURE.md` §4.10）：daemon 的启动/关闭序列（停接入层 → 取消周期任务 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 释放锁；详见 `SECURITY_DESIGN.md` §12.1）与单实例锁（`instanceId` + 锁记录）、配置加载与未接线段落的显式上报、周期任务装配、本节点身份与由节点公钥派生的 `nodeId`、本地管理通道客户端，以及 `LOCAL_ADMIN_PROTOCOL.md` §5.8 映射表的全部 CLI 子命令与 `doctor`/`acp-stdio`（`daemon start` 是前台进程；`acp-stdio` 只是 stdin/stdout ↔ channel `0x02` 的字节泵）；并按 `sync-ui-hosting` 注册根路径静态端点与嵌入资源表——发布构建缺前端产物时 `daemon start` 拒绝启动，`daemon.status` 的 `uiBuildVersion` 取自同一份嵌入资源 |

尚未开始：`server::acp_facade`（每落地一个才加入 workspace `members`）。`node-link-client` **已落地并登记为 workspace 成员**（切片 6）：WSS 传输与握手、单任务连接所有权/重连/撤销、catalog 无正文投影、`commit_receipt`→发布→`resource.ack`、远程 `SessionEndpoint`/`SessionBackendFactory`，以及独立附件连接上的有界内存转发。WP5d 已把 Access 侧出站装配接线：`node pair --mode access` 的 HTTPS claim/status（`app::access::AccessManager`，二维码固定公钥验签 `ownerProof`、批准后才写 Owner 信任）、启动时按持久化 Owner 信任建立 `OwnerLink`、`daemon.status.links[]`、远程 `SessionEndpoint`/附件转发入口，以及撤销后停止重连与路由失效；出站 TLS 信任锚 = 系统根 + 可选 `node_link.access.trust_roots`（空锚/坏 PEM 失败关闭）。仍未落地的是 `server::acp_facade`（本地 ACP 客户端的业务命令转发）与 Node Link 面的 `session.read` 正文投影。`server` 的 `/sync/v1/attachments` 与 `/node-link/v1/attachments` 也已随组合根注册（装配附件传输端口时同时宣告对应 feature）。`server::sync` 已落地到**认证、订阅、快照、事件投递与业务命令面**（`SYNC_PROTOCOL.md` §11 的查询命令、mutation 接受/幂等/版本冲突/终态观察者与 `session.create`，见该文件 §9 末与 §10.3 的实现状态注记）；仍未落地的是 `session.read` 的 `turns`/`pending_interactions`/`capabilities` 三类明细（本期回 `capability.unsupported_by_broker`，口径见 §9.4）与 Owner 可达性的真实出站数据源（今天的 `NodeLinkReachability` 读入站注册表，是近似值）。变更②（命令面）已完成实现与协议级端到端验收；变更③（UI 托管，`sync-ui-hosting`）已落地；变更④（前端接线 `frontend-sync-wiring`）的前端侧已落地（连接契约、配对链路、路由与片段三组缺口闭合，独立门禁与假主机 e2e 已绿），其真实 Daemon 的浏览器级端到端验收**已完成**（2026-10-16 逐条实测，记录见归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」）。

**2026-10-17（`retire-export-import-and-node-grants`）**：Export/Import 整族与 `grant.*` 授权词汇已删除；节点配对即完全信任（`local.*` 除外）。存储升级到 v11（owned）/v4（imported），升级不可逆，回滚只能靠升级前备份。Node Link catalog 改为 `agents`/`directories`/`sessions` 全量投影，Sync 的 remote origin 去掉 `exportId`。

`conversation-area-complete`（2026-10-20）已落地对话区完整内容链路：附件分块传输与清单封存、类型化媒体/思考/部分正文、有效待处理交互与真实表单、ACP 媒体映射与真实终端（含 Windows 进程树清理）、Sync/Node Link 新能力与独立认证附件连接、`node-link-client` 出站与 Access 无正文收据，以及 PWA 的附件/表单/流式/安全 Markdown/阅读锚点。根目录 `npm run verify` 与 `clients/app` 的 `npm run check` 均通过；真实 Codex/Oh My Pi（E7）与 Windows 终端/进程树（E8）已有证据。**仍未闭环**：Owner 的 Node Link 远程创建/命令/有效交互/附件消费路径（`server::acp_facade` 仍缺席）使 E5 双节点只能验证配对、imported 在线读取、Owner 离线降级、事件重放与 Access 无正文磁盘检查；E1 刷新后的媒体恢复与 E6 媒体加载位置/历史 UI 翻页仍缺证据。详见 [`conversation-area-complete` 单节点证据](docs/evidence/conversation-area-complete-single.md)、[双节点证据](docs/evidence/conversation-area-complete-dual.md) 与 [真实 Agent 证据](docs/evidence/conversation-area-complete-agents.md)。

`session-activity-three-states`（2026-10-20）的**三态会话合同已实现**（不再只是设计）：`core` 的会话主状态只剩 `idle | running | pending`，由「有效未解决交互优先、其次未结束任务、否则空闲」这一条归约规则统一计算，任务结果与命令状态词表保持不变；`storage-sqlite` 升级到文件格式 v13（owned 13 / imported 5），旧七态按迁移映射收敛、`closed_at` 删除，事件原文、序号与摘要不变；Sync 与 Node Link 都把 `core.session-activity.v1` 登记为**必需** feature，任一侧未选中即 `protocol.feature_required` / `nodelink.protocol.feature_required` 明确拒绝；`agent-host`/`server`/`app` 的终态、派发失败、放弃与启动恢复路径统一按「先确认停止、再推进 FIFO；`uncertain` 不自动重派」处理；PWA 在目录与对话页呈现三态，输入可用性独立于会话主状态（不再有「已关闭所以只读」）。跨层判据在 `crates/app/tests/session_activity_cross_layer.rs`：重启后已受理 prompt 结为 `uncertain` 且不重派、停止未确认前不提升排队 turn、缺三态 feature 的握手被拒与声明后选中、v12→v13 升级后旧 `session.updated` 原文/摘要可重放而当前摘要仍是三态，复现命令 `cargo test -p app --test session_activity_cross_layer`（由最终验收在主检出执行）。**仍未完成**：真实 Daemon + 可控 ACP Agent 的浏览器 E2E（状态、导航、多交互、失败续跑与重连）尚待执行，本文不声称该 E2E 已通过；本变更的最终组合验收与归档也未完成。

前端工程已落地在 `clients/app`（Expo/React Native 通用工程，当前 `expo ~57`；`app/` 路由与 `src/{domain,features,protocol,platform}` 分层，自带 `check` = typecheck + `typecheck:e2e` + vitest + `expo export`；`e2e/` 是**独立 tsconfig 的子工程**，由 `typecheck:e2e` 覆盖）。它是独立 npm 工程，不在根 `npm run check` 的链上——改动它时在 `clients/app` 下单独跑 `npm run check`。

仓库另含一份**前端设计原型**（`prototypes/`）：单文件 PWA 原型（`acp-remote-pwa.html` 与移动版 `acp-remote-pwa-mobile.html`，零外部资源、hash 路由，含配对/目录/目录详情/对话四页与控制台状态切换），并附 [`IMPLEMENTATION-GAPS.md`](prototypes/IMPLEMENTATION-GAPS.md) 按层列出「要实现还差什么」及源码行号证据。**它不是产品代码**：不参与构建、不被任何 crate 打包、没有测试，其中的结论也尚未同步到权威文档；与 `docs/` 冲突时以 `docs/` 为准。

Owner 侧的**会话恢复**（`session.resume` → ACP `session/resume`）已在 OpenSpec 变更 `session-resume` 中落地到组合根接线的真实 `SqliteStore` + `AgentHost`：`owned_session` 在文件格式 v5 追加可空的 `agent_session_id`/`workspace_cwd`（只追加、不推导），`acp-protocol` 有 `session/resume` 的类型化 DTO（`delivery = conditional_mvp`，端到端门控在 `agent-host`），`agent-host` 按持久化标识重新拉起进程并按 `sessionCapabilities.resume` 门控发送，`core` 有 `resume_session`/`settle_session_resume` 用例，`server::node_link` 路由 `session.resume`（授权先于本机读取、`payload` 非空即拒、终态与 `uncertain` 只用于崩溃窗口）。设计取舍见[会话延续设计](docs/SESSION_CONTINUITY_DESIGN.md)。仍需 Access 侧 `node-link-client` 才能从客户端发起。

切片 7 的 **Sync 合同**已包含 `session.create` 与本机 workspace/Agent 目录（OpenSpec 变更 `sync-workspaces-and-create`）：命令目录把 `session.create` 的 transport 扩为 `["sync","node_link"]` ，`schemas/sync/v1/command.schema.json` 与 `sync-protocol` 的 `CommandName`/payload/result 一致登记，feature registry 新增 `core.session-create.v1` 与 `core.local-catalog.v1`，`identity-auth` 的授权镜像（`grant.*` → 命令名集合）同步，`docs/SYNC_PROTOCOL.md` §5.2/§11.3/§11.5/§16.2、`docs/SECURITY_DESIGN.md` §9/§10.2/§11.2 与 `docs/FRONTEND_DESIGN.md` §2.2/§4.1/§7/§9 随之更新，固定向量见 `fixtures/sync/v1/`。**这里的 `session.create` 已是实绩**：变更② 已把它接线（feature 门控 `core.session-create.v1`、`accepted {result: null}` → `completed {sessionId, session}`、未登记别名回 `authorization.scope_denied`、无法确认 Agent 启动结果以 `uncertain` 终结）。**本变更 `pairing-model` 改写了这处授权口径**：设备授权包 `pack.*` 与配对预设 `preset.*` 删除（`commands.json` 去掉 `packs`/`presets` 与 `pack.create-session` 字段），设备记录、配对请求与 `auth.authenticated` 都不再携带授权集合；Sync 面的 `session.create` 以「该连接已通过设备认证」为唯一前提，载荷放开为「`agentId` + `workspaceAlias`（本机已登记别名）或 `workspacePath`（客户端提供的绝对目录）二者取一」，未登记别名与形状非法的目录改由目录校验承担（参数类错误 / `internal.unavailable`），Node Link 面仍只接受别名、请求不得携带任意 Owner 路径；设备配对与节点配对的短验证码一并删除，由两条设备侧密码学校验取代（`docs/SECURITY_DESIGN.md` §5.1/§10）。

`sync-server-connection`（OpenSpec 变更）的 **Daemon 侧同步面**已落地：设备配对 HTTP、四消息 WSS 认证与来源绑定、单设备替换与撤销传播、心跳与静默超时、cursor 判定与增量重放、快照组装与摘要、累计 ACK 与实时事件扇出，由 `app` 组合根 `crates/app/src/daemon.rs` 的 `register_sync_paths` 接线，`sync.*` 三个配置键按「只下调」消费。同步面各节的实现状态与仍未落地项写在 `docs/SYNC_PROTOCOL.md` §3.1/§7.4/§8.5/§9 的「实现状态」注记里（含「imported 事件回源投递仍是计划条款」与「`session.read` 三类明细本期降级」两条如实登记）。

`sync-server-commands`（OpenSpec 变更②）的**业务命令面**已落地：`SYNC_PROTOCOL.md` §11 的六条查询命令、七条 mutation 的「先持久接受、后经持久记录给终态」路径、`(actor, requestId)` 幂等与冲突判定、`expectedVersion` 版本冲突、`session.create` 的 feature 门控与 `uncertain` 收口，以及按连接存活的终态观察者；命令面与订阅面由 `server::sync::router` 的组合路由按 path 分派，终态观察者由组合根 spawn。协议级端到端用例在 `crates/app/tests/sync_commands_e2e.rs`（真实 loopback listener + 真实 SQLite + 脚本化后端：`accepted` → 领域事件 → 带 `terminalEventId` 的终态、未授权零副作用、幂等重发、`session.resume` 拒绝与限流）。

`session-file-browser`（OpenSpec 变更）的**原生只读适配器**已落地在 `crates/app/src/session_files/`（`core::ports::SessionFiles` 的实现）：会话根内分层枚举（默认显示隐藏/系统/忽略项，`.git` 目录与 worktree 的 `.git` 管理文件永不出现）、确定排序与有界分页、根外/循环链接与特殊文件的拒绝、bounded 内存内容快照（文本 256 KiB / 图片 4 MiB 预览、64 MiB 下载、32 KiB 分块、空闲 60 秒释放），以及受管理的 Git status/diff（固定参数、literal pathspec、`--no-optional-locks`、10 秒超时与整棵进程树清理、禁用 external diff/textconv/fsmonitor/pager/hooks/仓库 filter 命令）。**本机 `git` 是可选运行依赖**：缺失时文件浏览照常，Git 视图与忽略过滤显式返回 `gitState = unavailable`，不要求终端用户安装 Rust；原生适配器不改变 `workspace.browse` 合同。Sync 入站门控与组合根接线完成前，设备端尚不能调用新增的四条查询（feature 未宣告）。

`sync-ui-hosting`（OpenSpec 变更③）的**前端静态资源同源托管**已落地：Daemon 在**根路径**同源托管 PWA 构建产物——入口文档、单页应用回退、按扩展名的内容类型映射（未知扩展名回落 `application/octet-stream`），以及 `Content-Security-Policy`（取值等于 `docs/FRONTEND_DESIGN.md` §4.5 的基线，**由响应头下发**）与四个与配对端点同取值的安全响应头。协议端点行为不变：静态面只是 router 的 fallback，因此 `/sync/v1`、`/node-link/v1` 与两条配对端点天然优先，协议族保留前缀下的未注册子路径仍 404；静态面上的非读取方法返回 `405` + `Allow: GET, HEAD`。产物以 `rust-embed` **嵌入二进制**（资源表在 `server::transport::net::ui`；`crates/server/build.rs` 在编译期于「真实产物 ↔ 树内占位入口」之间二选一，产物目录不入库），**发布构建缺前端产物即拒绝启动**（错误指向先在 `clients/app` 跑 `npm run export:web`），构建版本可经 `daemon.status` 的 `uiBuildVersion` 查看（取自同一份嵌入资源；缺失即 `null`，不编造）。**如实边界**：该变更交付的是「浏览器能加载页面与资源、CSP 生效、协议端点行为不变」；当时尚未闭合的四处接线缺口（连接路径 `/sync` → `/sync/v1`、WS subprotocol、`approved` 响应落 `hostPublicKey`、配对页二维码输入入口）已由变更④ `frontend-sync-wiring` 接上（证据与缺口闭合见下段）；该变更在 `SECURITY_DESIGN.md` §11.1 登记的「产物内联 `<style>` 与 CSP 基线冲突」已由后续变更 `ui-csp-inline-styles` 在前端侧消除（内联形态移除、样式改由同源外部样式表交付，Rust 侧 CSP 未改动，见下段）。

`frontend-sync-wiring`（OpenSpec 变更④）的**前端接线**已落地：**连接契约对齐**（出站地址唯一由组合根构造、路径为权威端点 `/sync/v1`；socket 端口容纳「声明的子协议列表」，Web 实现把它作为 `new WebSocket(url, protocols)` 的第二个参数；子协议值经合同测试与仓库根 `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol` 逐字比对，前端不硬编码第二份；握手处比对服务端选中的 feature 是否覆盖客户端必需集合，未覆盖即阻断）、**配对链路闭合**（状态查询的已批准结果携带主机标识与主机公钥，落盘 `paired-host/<canonicalOrigin>` 记录后进入已配对并触发连接；配对页的入口（**已被后续变更取代**：`pairing-code-web-entry` 增加 8 位配对码输入，`device-pairing-qr-reference` 最终把设备入口收敛为**只用配对码**，见下段）原是打开带配对内容的地址（`#data=` 片段），读到片段后立即清除；`/pair` 的 sync 载荷自动开始认领，`/node-link/pair` 的 node-link 载荷只呈现对端信息、不发起 claim/status；页内没有输入框、粘贴入口、提交按钮或「重新扫码」，也不绘制页头/返回，全局顶栏仍不呈现；结构非法时明确失败且不发请求；配对中按载荷 `expiresAt` 显示真实剩余有效期，归零或打开即过期进入「地址已过期」并停止轮询，审批通过后约 1.6 秒自动进入目录（不等连接在线），失败页按九类固定文案呈现且不提供任何按钮；地址片段中的配对秘密**先读后清**，只活在私有会话字段，不进缓存摘要、可导出状态与出站 URL——这一配对页形态由后续变更 `pairing-page-open-url` 落地，见 [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md) §4.1/§9）与**路由/片段取舍消解**（路由走正常路径 `/`、`/pair`、`/hosts`、`/dir/<alias>`、`/session/<id>`，地址片段只承载配对内容）。`incompatible` 阻断态新增封闭成因词表 `subprotocol_not_selected` / `protocol_version_unsupported` / `required_features_missing`，与「网络断开」在事件层可分，且阻断态**结构性**停止自动重连。实测（主检出）：`cd clients/app && npm run check` = 59 files / 725 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = 51/51（既有 49 条 + PW-1、PW-2；真实 Chromium + 假主机）。**当时登记为未做的两项（现已闭合）**：① 真实 Daemon（同源托管）+ 真实浏览器的端到端验收（按 `docs/WORKFLOW.md` 的 E2E 口径不得以假主机用例代替）——**2026-10-16 已完成**，逐条实测见归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」（其中如实登记为待裁决的「内联样式与 CSP 冲突」已由 `ui-csp-inline-styles` 闭合）；② `SYNC_PROTOCOL.md` §7.1 的「当前 origin 与 secure context」校验——**已实现**（变更 `pairing-origin-secure-context`，主规格 `pwa-web-client` 的「配对入口校验来源与安全上下文」）。原登记的前端缺口「服务端为本机 node-link 生成的配对二维码指向 `/node-link/pair`、而前端没有该路由」已由变更 `node-link-pair-route` 关闭：该路由现已存在（与 `/pair` 共用同一入口主体），路由登记见 [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md) §4.1。**本变更 `pairing-model` 把配对的信任判定收口为两条设备侧密码学校验**：用配对载荷里**固定的**主机公钥验证认领响应的 `hostProof`（失败即停止轮询、不写任何主机记录），以及状态为已批准时校验响应里的主机公钥与载荷**逐字节相同**、主机标识一致（不一致即失败且不落盘；落盘的主机公钥以载荷为准，上一段的「已批准结果落 `hostPublicKey`」即在此基础上加逐字节比对并把取值来源改为载荷）。短验证码（SAS）就此删除——它在这条流程里没有独立职责，理由见 [docs/SECURITY_DESIGN.md](docs/SECURITY_DESIGN.md) §5.1。

`device-pairing-qr-reference`（OpenSpec 变更）的**设备短二维码 + 配对码入口**已实现（Native 真机未执行）：设备二维码改为 `https://<canonical-origin>/pair#q2=<payload>`（版本 `0x02` + 32 字节扫码取回凭据 + 32 字节主机公钥 SHA-256 指纹，共 65 字节负载 / 87 字符），`device.pair.begin` 只生成该短地址并保留四个返回键；新增 `POST /sync/v1/pairing/qr-lookup`（显式校验 `Origin`/`Host`/持久绑定，成功重复取回同一份七字段资料、不消耗网页码、不推进状态，统一失败 `401 pairing.qr_invalid`，独立 10 次/分钟限流）；claim 成功后关闭取回入口。PWA 的设备配对**只使用 8 位配对码**：`/pair` 无片段显示码输入，打开 `#q2=` 只清除片段并显示固定 App 指引，打开旧 `#data=` 清除并提示已停用，两者都不发 lookup/qr-lookup/claim/status；`/node-link/pair` 仍只读展示节点资料。原生消费合同（解码、32 字节完整公钥指纹校验、来源绑定、有限重试）在 `clients/app/src/features/native-qr-consumer.ts`，仅由测试调用，PWA 不接入。**C 链路证据**：`crates/app/tests/device_qr_reference_e2e.rs` 用模拟原生消费方对**真实 `acp-remote` Daemon**（真实 listener/SQLite/Authority）走完「短二维码 → qr-lookup → 65 字节公钥与 SHA-256 指纹 → claim → 本机确认 → status → 携带同一 Origin 的 WSS 四消息认证」，并覆盖响应丢失重试同材料、认领后取回 401、重复 claim 幂等、不同内容 claim 冲突、错误指纹/来源/绑定不发 claim，以及日志/进程输出不含完整 `qrToken`/`pairingSecret`；页面回归由 `node e2e/run-e2e.mjs`（假主机，81/81 通过）覆盖，**不替代真实浏览器对真实 Daemon 的验收**。发布/回滚口径：Daemon 与前端必须配套发布，旧前端缓存/已打开标签页在刷新后失效（静态资源 `no-store`、无 service worker，部署后需硬刷新），未完成配对在重启后失效并需重新生成，新旧二维码格式不混用、不保留双栈解码；已批准的设备身份与会话**不要求重新配对**。本机真实浏览器对真实 Daemon 的 Main E2E 已执行：桌面 1280×900 与手机 390×844 视口分别用配对码完成本机确认并进入目录（在线），打开 `q2=`/旧 `data=` 片段均先清片段、显示固定文案且零配对请求，`/node-link/pair` 只读展示且零请求，主题偏好刷新后保留；Android/iOS App、相机、keystore 与真机扫码不在本轮交付。

`ui-csp-inline-styles`（OpenSpec 变更）的**前端入口文档样式形态**已落地，闭合了上面 `sync-ui-hosting` 登记的「产物内联样式与 CSP 基线冲突」：入口文档不再含内联样式——原内联 `<style id="expo-reset">` 抽出为同源 `/entry-reset.css`，并在 bundle 的 `<script defer>` **之前**声明一份**必须保持为空**的同源 `/rnw-stylesheet.css`，`react-native-web` 于模块求值期复用该 `<link>` 的 `sheet` 写入全部运行时原子类（`insertRule` 不是 CSP 的检查点，因此这是消除冲突、**不是放宽基线**；Rust 侧的 CSP 与安全响应头取值一字未动）。取舍、失败形态与判据见归档的 `openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md`（D1/D3），回归判据取浏览器级的**计算样式**断言。

当前优先交付 Windows x64 的 Daemon/CLI 与 Node Link 闭环，Linux 延后开发；完整平台顺序见 [初始设计 §14](docs/INITIAL_DESIGN.md#14-npm-分发)。共享代码的 Linux CI 保留，不代表 Linux 产品已可运行。

逐切片的实施顺序与验收节点见 [开发计划](docs/DEVELOPMENT_PLAN.md)；产品与协议语义仍以各权威合同为准。

管理状态的端口签名、写集 DTO、值对象与 DDL 已并入 [核心与存储合同 §3/§5/§7](docs/CORE_PORTS_AND_STORAGE.md#11-管理状态持久化合同形状已并入-357)（该节 §11 保留设计理由与索引），并由 `scripts/check-contract-drift.mjs` 逐条断言；`identity-auth` 的内部状态机、握手入口、授权展开与 keystore 端口冻结在 [身份与认证合同](docs/IDENTITY_AND_AUTH_CONTRACT.md)；本地通道的 ACP 流会话语义与管理载荷的机器表达见 [本地管理通道](docs/LOCAL_ADMIN_PROTOCOL.md) §3.1 与 [`schemas/local-admin/v1/`](schemas/local-admin/v1/)。SQLite 侧的 `TrustStore`/`LocalConfigStore` **落盘实现已落地**（写集一事务提交、失败关闭与容量纳入）；`identity-auth`/`identity-keystore` 两个身份 crate **已落地**（契约、状态机、平台 keystore 与固定向量回归测试）；Daemon/CLI 接线与 `server` 的本地通道也随之**已落地**（`app` 的组合根、启动/关闭序列、单实例锁与全部 CLI 子命令；`server::transport::local` + `server::local_admin`），因此配对、撤销、本地配置、Export 管理与 Import 的列表/删除已可经本地通道端到端使用（两处按合同的例外：`import.add` 因为需要 Owner 的 catalog 快照、而提供它的 Access 侧 `node-link-client` 属切片 6，恒返回 `local.unavailable`，`node pair --mode access` 返回 `local.unsupported`）。Owner 侧的 Node Link 入站面（`server::transport::net` + `server::node_link`，含 `app` 的组合根接线）**已落地**：已配对的 Access 节点可经 Node Link 完成握手、拉取 Export catalog 与资源、下发命令，且撤销即时生效。客户端同步面（`server::sync`，含 `app` 的组合根接线）**也已落地到认证、订阅、快照、事件投递与业务命令面**这一段（边界见上面 `sync-server-connection` 与 `sync-server-commands` 两段）。仍未实现的是 `node-link-client` 与 `server::acp_facade`；前端工程已落地，其静态资源也已由 Daemon 同源托管，前端侧的连接接线也随变更④闭合（见上面 `sync-ui-hosting` 与 `frontend-sync-wiring` 两段），其对真实 Daemon 的浏览器级端到端验收也已完成（记录见归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」）。Access 侧 backend（`node-link-client`）与 `server::acp_facade` 未落地，因此仍不能声称 Zed 经 Access Node 的远程会话闭环或 ACP 流（`acp-stdio`）已经可用；`0x02` 连接在 facade 缺席期的行为见 [本地管理通道](docs/LOCAL_ADMIN_PROTOCOL.md) §3.1 的实现状态注记。

## 权威文档

修改代码或设计前先读 `AGENTS.md`，它列出各文档的职责边界与维护规则。主要入口：

| 文档 | 内容 |
|---|---|
| [docs/WORKFLOW.md](docs/WORKFLOW.md) | 变更执行流程：OpenSpec schema、工作包隔离与 patch 交付、独立检视、串行合入、分层 E2E 与门禁 |
| [docs/INITIAL_DESIGN.md](docs/INITIAL_DESIGN.md) | 产品目标、系统行为、阶段计划 |
| [docs/MODULE_ARCHITECTURE.md](docs/MODULE_ARCHITECTURE.md) | crate / 模块职责与依赖方向 |
| [docs/SYNC_PROTOCOL.md](docs/SYNC_PROTOCOL.md) | 客户端同步协议（认证、cursor、命令、事件） |
| [docs/NODE_LINK_PROTOCOL.md](docs/NODE_LINK_PROTOCOL.md) | 节点间资源导出/导入协议 |
| [docs/LOCAL_ADMIN_PROTOCOL.md](docs/LOCAL_ADMIN_PROTOCOL.md) | CLI 与 Daemon 之间的本地管理通道 |
| [docs/SECURITY_DESIGN.md](docs/SECURITY_DESIGN.md) | 威胁模型、信任边界与安全验收 |
| [docs/ACP_COMPATIBILITY_MATRIX.md](docs/ACP_COMPATIBILITY_MATRIX.md) | ACP v1 覆盖范围与各层策略 |
| [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md) | 前端阶段、状态模型与平台边界 |
| [docs/IDENTITY_AND_AUTH_CONTRACT.md](docs/IDENTITY_AND_AUTH_CONTRACT.md) | 身份与认证：配对状态机、握手入口、授权展开与 keystore 端口（已定型；`identity-auth`/`identity-keystore` 已按它落地） |
| [docs/CONFIG_REFERENCE.md](docs/CONFIG_REFERENCE.md) | Daemon 配置键、类型与默认值 |
| [config.example.toml](config.example.toml) | 可直接复制的配置模板（默认值可运行；`node_link.*` 暂不生效），由单元测试解析校验 |
| [docs/CORE_PORTS_AND_STORAGE.md](docs/CORE_PORTS_AND_STORAGE.md) | `core` 值对象、用例面、端口签名、broker 事务顺序与 `storage-sqlite` 表结构/保留/migration |
| [docs/adr/](docs/adr/) | 已接受的架构决策 |
| [GLOSSARY.md](GLOSSARY.md) | 术语的唯一用词表：只收「在本仓库里必须唯一」的词（命令五态、受理/拒绝、规范化路径等），不写实现细节 |

## 合同检查

合同门禁只依赖 Node ≥ 22.12（`commitlint` 21 的下限）与仓库内的 devDependencies，不参与运行时产物；
Rust 侧的编译器版本以 [`rust-toolchain.toml`](rust-toolchain.toml) 为唯一来源（本地 rustup 与 CI 读同一个文件，
因此不会有「本机绿、CI 红」的 clippy / rustfmt 版本假失败）：

```text
npm ci          # 首次或依赖变动后
npm run verify  # 合同门禁 + fmt / clippy / test，与 CI 的 checks job 同源
```

② 命令目录与本仓库封闭词表的一致性——`commands.json`、两个协议 schema、SYNC §11.5 与 SECURITY §10.2 的表格，以及本地管理的方法集与错误码（`LOCAL_ADMIN_PROTOCOL.md` 的

CI（[`.github/workflows/ci.yml`](.github/workflows/ci.yml)）在 push、PR 与每日定时任务上跑六个 job：`checks`（`npm run check` + `npm run check:rust`，与本地 `npm run verify` 同源）、`frontend`（`clients/app` 的 `npm ci` + `npm run check`：typecheck + `typecheck:e2e` + vitest + `expo export --platform web`；根目录的检查链不覆盖这个独立工程，见「分支保护」一节的必需检查说明）、`commits`（提交范围规范）、`deps`（`cargo-deny` 的 bans/licenses/sources 与 `scripts/check-dependency-advisories.mjs`）、`advisories`（`cargo-deny` 的 advisory 判定）、`secrets`（`gitleaks` 密钥扫描：push/PR 扫本次范围，每日定时任务扫全历史）。Linux runner 会真正执行 `#[cfg(unix)]` 的权限路径（`0700`/`0600`/模式位判定），这些在 Windows 开发机上不会跑到。

`deps` 的 npm 侧判定不直接跑 `npm audit`，而经 [`scripts/check-dependency-advisories.mjs`](scripts/check-dependency-advisories.mjs)：`npm audit` 没有 ignore 机制，而仓库有一条**上游无修复**的 advisory（`braces` 的 `GHSA-vfj7-8cjw-p6xm`，受影响范围是 `*`、3.x 线最新版即被通报版本，`fixAvailable` 为 false），直接跑会让该 job 永远红灯。该脚本**按 (advisory ID, 工程) 放行已登记项**，未登记项与任何 `critical` 一律失败；每条豁免的理由、适用范围与到期条件写在该脚本的 `ALLOWED` 数组里。判定范围是**两个** npm 工程——仓库根与独立工程 `clients/app`（自带 `package.json` 与 `package-lock.json`，根目录的 `npm ci` / `npm run check` 都不覆盖它），两个工程各跑一次 `npm audit` 后合并判定，失败输出会标明出问题的工程路径。已登记的 4 条豁免里，`braces`（`GHSA-vfj7-8cjw-p6xm`）、`node-forge`（`GHSA-86w9-cpqp-85rv`）、`uuid`（`GHSA-w5hq-g745-h8pq`）是**不可达**（构建期工具链，或漏洞前提在本仓库不成立）；`decode-uri-component` 的 `GHSA-vcc3-ghjq-m6fr` 则是**真实可达但延后**——它进了 web 产物，修复需 Expo SDK 58，到期条件是「SDK 58 进入 npm `latest` 时升级并删除该条」。与 `advisories`、`secrets` 同理，这层判定**本地没有等价物**，只在 CI 的 `deps` job 运行。

`deps` / `advisories` / `secrets` 需要网络或额外二进制，**没有包含在 `npm run verify` 里**（依赖判决见 [`deny.toml`](deny.toml)，密钥扫描规则见 [`.gitleaks.toml`](.gitleaks.toml)）；工具版本、许可证、向外发送的数据与已知残余风险见 [ADR-0008](docs/adr/0008-ci-supply-chain-tooling.md)。依赖更新由 [`.github/dependabot.yml`](.github/dependabot.yml) 提出：升版前有冷却期（避免第一时间采用刚发布的版本），minor/patch 分组、major 单独提交，且分组 PR 同样要过全部 job。

提交信息遵循 Conventional Commits（`<type>(<scope>)!?: <主题>`）：type 与 scope 词表以 [`commitlint.config.mjs`](commitlint.config.mjs) 为唯一机器定义，本地由 husky 的 `.husky/commit-msg` 钩子在 `npm install` 时装配，CI 的独立 `commits` job 会对本次推送/合并请求引入的提交范围再校验一次（`npm run lint:commits -- --from <base> --to <head>`，`--no-verify` 绕得过本地钩子但绕不过它）。词表与合并豁免规则以 [`commitlint.config.mjs`](commitlint.config.mjs) 为唯一机器定义。

上游 ACP 固定快照（`schemas/acp/v1/upstream/schema.json`，来源与 sha256 见 `compatibility/acp/v1/matrix.json` 的 `protocol` 块）由 `check:acp` 重算 digest 并校验 commit 与 major 版本目录；`fixtures/acp/v1` 也按同一快照做 ajv 校验。升级快照必须同时改固定值、vendored 文件与矩阵行，且先通过 `node scripts/check-acp-compatibility.mjs`。

## 分支保护

CI 的判定只有在分支保护要求它时才真的能拦住合并：[`.github/workflows/ci.yml`](.github/workflows/ci.yml) 定义检查，「合并前必须通过」却是 GitHub 的仓库设置，不属于版本控制内容。

**必需检查的 context 必须是 check-runs 上报的名字，不是 job id**——这一点实测过：填 job id（`checks`、`deps`……）时
`rulesets/<id>` 会原样存下，但没有任何 check 叫这个名字，规则就变成「永久等待」，非绕过 actor 的 PR 永远合不进去
（写错时自己有 bypass 感觉不到，属于安静的地雷）。正确名字从权威接口取：

```text
gh api repos/<owner>/<repo>/commits/<sha>/check-runs --jq '.check_runs[].name' | sort -u
```

本仓库当前必需的六个是：**合同门禁 + Rust 检查**、**提交信息规范**、**依赖许可证与来源**、**密钥扫描**、
**依赖安全公告**、**前端检查（clients/app）**。其中**依赖安全公告**是否必需按
[ADR-0008](docs/adr/0008-ci-supply-chain-tooling.md) 决策 4 判断：它的失败可能来自与本次改动无关的
上游 advisory。

**前端检查（clients/app）** 是 2026-10-11 新增的 job。`clients/app` 是独立 npm 工程，根目录的
`npm ci` 与 `npm run check` 都不覆盖它，所以这个 job 是前端唯一的机器判据——在它存在之前，
`react-native` 大版本升级这类**必然打断 `expo export`** 的改动在 CI 上仍是全绿（两次实盘：#68、#69）。
新增 required check 有一个前置条件，见下面「上线时踩到的两个坑」第 1 条。

设置入口是仓库 Settings → Rules/Branches（`gh api -X PUT repos/<owner>/<repo>/branches/main/protection` 也适用，但 payload 形状取决于要开哪几条，建议先用界面）。核实当前状态与可用性：

```text
gh auth status
gh api repos/lindongfang/acp-remote --jq .private     # 私有仓库的分支保护需要付费 plan
gh api user --jq .plan
gh api repos/lindongfang/acp-remote/branches/main/protection   # 404 = 未启用或不可用
```

分支保护在 public 仓库上随 GitHub Free 就有，私有仓库需要 GitHub Pro/Team/Enterprise。**本仓库是 public**
（2026-09-23 核实：`gh api repos/lindongfang/acp-remote --jq .visibility` 返回 `public`），所以这一项没有 plan 前提。

**已核实的现状（2026-09-23）**：ruleset **`main-protection`**（id `23858733`）——`target: branch`、
`enforcement: active`、条件 `ref_name: ["~DEFAULT_BRANCH"]`；规则为 `deletion` + `non_fast_forward` +
`required_status_checks`（`strict_required_status_checks_policy = true`、`do_not_enforce_on_create = false`，
五个必需检查就是上面那五个上报名，已逐个与 check-runs 对上）+ `pull_request`
（`required_approving_review_count = 0`；单人仓库必须是 0，否则自己的 PR 无人可批准）；
bypass list 保留 `RepositoryRole admin / always`。`branches/main/protection` 仍返回 404：
本仓库用 ruleset 而不是经典分支保护，两者不需要同时开。

这一档（**PR 必需 + 保留 admin 紧急出口**）的实际含义：默认路径是 PR，因此三个只能在 CI 运行的判定
（依赖许可证与来源、依赖安全公告、密钥扫描）是先于落地的门禁；bypass 让直推仍然可行，但那是紧急出口不是
日常路径（直推会跳过这三个判定）。变更落地流程是：分支 → PR → `gh pr checks --watch` → squash 合并
（`gh pr create --fill`；squash 合并用 PR 标题当提交信息，因此标题同样要合规）。本地合入到开 PR
之间的完整步骤（含六个必需检查与 up-to-date 要求）见 [`docs/WORKFLOW.md`](docs/WORKFLOW.md) §6.1–§6.2。

没有再上「移除 bypass」的理由：对单人仓库它不是对你的安全边界（你本来就能改 ruleset），只是减速带；
而它的代价是真实的一一某个 workflow 改动把必需检查弄红时，所有 PR 都进不来，你还得先去改设置。
多人协作时再考虑。

本地密钥扫描机制已装好：`.husky/pre-commit` 会调 gitleaks 扫暂存内容，规则与 CI 共用
`.gitleaks.toml`。自研 keystore 条目**明文**形态的规则（`acpr-keystore-plaintext-entry`）已随
`identity-auth-and-keystore` 落地；它的**首次真实执行仍在 CI 的 `secrets` job**（本机无 gitleaks），
且只覆盖「被 base64/base64url 编码后的明文条目」这一形态——原始二进制与 DPAPI 密文无法模式识别。详情见下方与
`docs/adr/0008-ci-supply-chain-tooling.md` 的残余风险 3。

已核实为**已开启的**（2026-09-23）：`secret_scanning`、`secret_scanning_push_protection`、
Dependabot 告警与安全更新。push protection 在推送前拦截已知 provider 模式的凭据——这个时序 CI 给不了；
但本项目自研格式的密钥要靠 CI 的 `gitleaks` 或通用模式检测。两层仓库级检测与运行时扫描的分工写在
`SECURITY_DESIGN.md` §18.1。

两个仍有待开启的，**只能走界面，REST API 写不进去**（实测：`PATCH /repos/{owner}/{repo}` 带
`security_and_analysis.secret_scanning_non_provider_patterns` / `secret_scanning_validity_checks`
既不报错也不生效，字段原样返回 `disabled`）：

两个曾计划开启的子特性，**已核实为在本仓库不可用**（2026-09-23），不要再去翻开关：

- REST API 不接受这两个字段：`PATCH /repos/{owner}/{repo}` 带
  `security_and_analysis.secret_scanning_non_provider_patterns` / `secret_scanning_validity_checks`
  既不报错也不生效，读回仍是 `disabled`；
- 仓库的 `security_and_analysis.advanced_security` 为 `null`（该仓库不适用 GitHub Advanced Security）；
- 界面侧：Settings → **Advanced Security**（个人账号仓库下是这个名称；组织账号的文档里叫
  “Code security” / “Code security and analysis”）页面里**没有 Secret scanning 区域**——public 仓库的
  secret scanning 与 push protection 由 GitHub 自动开启、不提供开关，而 non-provider patterns /
  validity checks 属于 GHAS 特性。

定性：这是**账号/仓库类型的平台限制**（能力缺口），不是配置遗漏，**不必为此升级付费 plan**。
（顺带修正一个概念：`validity checks` 只是把命中的凭据发给签发方校验是否仍有效，对没有签发方的自研格式
密钥本就没有意义；真正有用的是 non-provider patterns，而它不可用。）

由此得到一条必须记住的推论：**自研格式的凭据在推送前没有任何服务端防线**。push protection 只认 provider
模式，而本项目的 P-256 私钥与 base64url 配对密钥是自研格式；又因为仓库是 public，一旦进了历史就是公开的。

机制已经装好，采用「**一份规则、两个执行器**」：

- 规则写在 [`.gitleaks.toml`](.gitleaks.toml)，CI 的 `secrets` job 与本地钩子共用同一份（不维护两套）；
- 本地执行器是 `.husky/pre-commit` 调 `gitleaks git --pre-commit --redact --staged`（来自上游
  `.pre-commit-hooks.yaml` 的官方写法，不是自拟参数），扫的是**暂存内容**，命中内容由 `--redact` 不打印；
- 本机未安装 `gitleaks` 时钩子**只提示并跳过**（不让提交依赖一个仓库不随附的二进制），CI 仍会判定。

**自研格式规则已落地**（触发条件「`identity-auth`/`identity-keystore` 开始产生真实密钥」已发生）：
`.gitleaks.toml` 里的 `acpr-keystore-plaintext-entry` 只匹配**编码后**的明文条目形态，`keywords` 用编码前缀
`QUNQS`（不能写字面 `ACPK`——编码后的文本里不存在它，规则会永不评估）。诚实边界：原始二进制条目与
DPAPI 包裹的密文仍不可模式识别；规则首次真实执行在 CI 的 `secrets` job。

关于「Dependabot 安全更新」还有一个前置条件值得记下：它要求 **Dependabot 告警先开**，否则
`PUT .../automated-security-fixes` 直接返回 422「Vulnerability alerts must be enabled」。顺序是：

```text
gh api -X PUT repos/lindongfang/acp-remote/vulnerability-alerts      # Dependabot 告警（先决条件）
gh api -X PUT repos/lindongfang/acp-remote/automated-security-fixes  # 安全更新
```

三类设置的实际作用不同，**以 Rulesets 界面的原文为准**：

- **Require status checks to pass**：作用在 **ref 更新**上，界面原文是「Choose which status checks must pass
  before the ref is updated. When enabled, commits must first be pushed to another ref where the checks pass.」——
  直推一个未经过检查的新提交会被拒；正确流程是先把提交推到另一个 ref（分支 / PR 分支），让检查在那里通过，
  再让 main 更新到那些提交。它**不只拦「合并」**：即使不开「要求 PR」，直推也会被拦（前提是下面的绕过设置）。
  两个子选项：`Require branches to be up to date before merging` 只对 PR 生效；
  `Do not require status checks on creation` 豁免「创建 ref/分支」这类场景。
- **Bypass**：仓库 admin 默认绕过该 ruleset 的全部规则（bypass list 里会有 `Repository admin`；
  经典分支保护里的对应开关是「Do not allow bypassing the above settings」的反面）。所以对单人仓库来说，
  只要保留 admin 绕过，这条规则拦的是别人与自动化，不拦你自己；取消绕过才会真正约束你的直推。
- **Require a pull request before merging**：把 main 变成只能经由 PR 落地。
- **Block force pushes / Restrict deletions**：默认随规则生效，不因 actor 而异。

**上线时踩到的两个坑**（改这些设置时会再遇到，所以留在这里）：

1. 必需检查的候选列表只来自**最近跑过的检查**：新 job 从未执行过时 `Add checks` 的下拉是空的，无从选起；
2. 必需检查的 context 必须是 **check-runs 上报的名字**
   （`gh api repos/<owner>/<repo>/commits/<sha>/check-runs --jq '.check_runs[].name'`），不是 workflow 里的
   job id：填错会原样存下一个永远不会上报的名字，规则变成**永久等待**，而自己有 bypass 所以感觉不到。

这也是当初先上零摩擦档、等五个 job 都绿过再收紧的原因：在检查没绿之前就上「要求 PR」，会把「合并」与
「改配置」一起锁死。

**当前档位**：默认路径是 PR（规则要求 PR + 五个必需检查 + `strict_required_status_checks_policy` 的 up-to-date），
并保留 `Repository admin` 作为紧急出口。**只有多人协作时才需要考虑移除 bypass**：对单人仓库它不是对你的
安全边界（你本来就能改 ruleset），只是减速带；而它的代价是真实的——某个 workflow 改动把必需检查弄红时，
所有 PR 都进不来，你还得先去改设置。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
