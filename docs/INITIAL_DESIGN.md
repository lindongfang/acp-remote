# ACP Remote 初始设计文档

> 目录选择器补充（2026-10-18）：Windows 默认浏览不展示隐藏或系统属性目录；PWA 按名称忽略大小写、数字自然升序显示收到的目录。这些规则仅影响浏览与显示，不新增创建路径授权限制。实现边界见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5.5，客户端行为见 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) §4.1。

> 修订记录（2026-10-21，custom-session-title（合同与实现））：§8 的会话标题口径改写为「两个来源 + 有效标题投影」——登记 Owner 元数据命令 `session.title.set` 与 feature `core.session-title.v1`，有效标题 = 自定义标题优先 → 否则 Agent 标题 → 否则未命名，由 Owner 投影成既有 `SessionSummary.title`；删除「标题只由 Agent 单向更新 / 不存在客户端重命名入口」的旧条款（原文见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.3 与 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §5），明确 Access 只转发命令、不持久化标题与命令状态，且该命令不新增 ACP 方法。Owner 的 `custom_title` 持久化与命令事务已接线。  
> 状态：编码前原始设计（Draft）  
> 版本：0.5  
> 修订记录（2026-10-17，`sync-path-browse-and-diff-delivery`）：把「workspace 实际路径不离开 Owner Node」收窄为带前提的边界——**已登记**目录的规范化路径只下发给**协商了 `core.workspace-path.v1`** 的**已认证设备**，且只出现在快照的 `workspaces` 资源条目上（协商 `core.workspace-browse.v1` 时只读浏览的 `command.result` 是第二处路径可见面，只含该层条目）；客户端自己提交的目录不得被回显（§2/§8）。本机自己的设备新增两项受限能力：`session.create` 可带一个「目录不存在则先创建」开关，`workspace.browse` 提供只读的逐层目录浏览（feature 门控、不登记、不产生审计动作）。口径以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.2/§12.3 与 [ADR-0009](./adr/0009-authenticated-workspace-path-and-browse.md) 为权威。
> 修订记录（2026-10-18，`device-pairing-qr-reference`）：§11.2 的设备二维码改为短引用格式 `#q2=`（版本 + 32 字节扫码取回凭据 + 32 字节主机公钥指纹），完整资料经同源 `qr-lookup` 取回；PWA 只使用配对码，旧设备 `#data=` 退出；原生 App 扫码仍为后续交付。字段与拒绝规则以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.1/§7.6 为唯一权威。
> 修订记录（2026-10-16，`pairing-model`）：删除两套配对短验证码（§11.2/§11.7 的展示与派生规则改为两条设备侧密码学校验：用二维码里固定的主机公钥验证认领响应的 `hostProof`，已批准响应的主机公钥必须与二维码逐字节相同）；删除设备权限层（§1/§3.4/§8/§9/§11.4/§17 的「按 Actor 的 scope 授权」「设备记录保存命令 scope」「设备是否拥有该命令 scope」改为「通过设备认证即可执行，设备方向没有授权维度」，§11.4 的设备记录示例去掉 `scopes` 并改名）；`session.create` 在 Sync 面允许客户端提交绝对目录（§8）。跨节点 `grant.*`/Export `scopes`/Import grants 与设备认证、撤销、速率上限不变。
> 修订记录（2026-09-18）：配对有效期改为固定 5 分钟；PWA 首版明确不展示会话创建入口；移除与 Sync/Node Link 已冻结取值冲突的旧叙述。  
> 日期：2026-09-18
> 用途：记录项目最初的产品边界、架构选择、安全模型和发布方式，作为后续设计与编码的基线。

## 1. 项目目标

ACP Remote 是一个运行在用户控制节点上的、有状态的 ACP（Agent Client Protocol）中转站。节点可以直接连接 Codex、Oh My Pi 等本地 Agent，也可以通过另一个 ACP Remote 节点访问远程 Agent。

项目不实现 Agent 推理，也不替代具体 Agent。手机、电脑、Zed 和 PWA 都只是不同形态的访问端；核心模型不再以设备形态区分能力，而以资源归属、节点身份与设备认证状态决定能力，跨节点能力再由 Export policy 与 `grant.*` 约束。

核心目标：

- 一个 ACP Remote 节点可以导出它直接管理的 Agent，也可以导入其他节点导出的 Agent。
- 本地或远程 Zed 可以连接就近 ACP Remote 的本地 ACP facade，再访问资源归属节点上的 Agent。
- PWA、手机、桌面 App 和 CLI 可以查看会话、继续对话、处理通知并控制被授权的能力。
- 会话创建不再与“电脑/手机”绑定；是否允许创建由 Owner Node 的 Export Policy（跨节点导入的资源）或该设备自身的认证状态（本机自己的设备）决定。
- Agent、workspace、凭据、会话和事件日志始终由资源归属节点负责。
- 项目不与 Zed 强绑定；Zed 只是可选的 ACP 客户端。
- 项目不使用应用级云服务器或持久化云中继。
- 网络连接方式可替换，不与 Tailscale 耦合。
- 客户端与节点、节点与节点首次配对后建立可撤销的长期信任，正常重连不重复扫码。
- 核心 Daemon 和 CLI 使用 Rust 实现，最终通过 npm 分发预编译二进制。

## 2. 非目标

第一阶段明确不包含：

- 不在云端运行 Agent。
- 不在云端保存聊天记录、命令或凭据。
- 不提供 Owner Node 离线时的可靠命令排队。
- 不提供 Owner Node 离线时的权威历史读取；默认 `no-content-cache` 下，Access Node/远程客户端离线时不提供会话正文。
- 不实现模型供应商账号系统。
- 除协商了 `core.workspace-path.v1` 的设备可按目录资源收到**已登记**目录的规范化路径、以及协商了 `core.workspace-browse.v1` 的设备可在只读浏览结果里收到所请求那一层的直接子目录绝对路径外，Provider API Key、OAuth 凭据、MCP 配置与 workspace 实际路径不离开 Owner Node。
- 跨节点的远程会话创建只能引用 Owner Node 预先导出的 workspace alias/template，不能提交任意绝对路径；本机自己的设备在 Sync 面可提交一个绝对目录，但受本机的绝对路径与存在性校验约束（§8）。
- 不重新实现 Codex、Oh My Pi 的 Agent Runtime。
- 不将 Tailscale、Zed 或某个具体 Agent 写入核心协议。
- 第一阶段不允许 imported Agent 再经 Node Link 导出，不实现多跳 federation。
- 第一阶段不追求完全复刻 Happy 的云端同步、推送和多设备服务。

## 3. 已确认的产品边界

### 3.1 Owner Node（资源归属节点）

直接连接 Agent 的 ACP Remote 节点负责：

- 安装、登录和启动具体 Agent，并作为该 Agent 的唯一 ACP Client。
- 管理 Agent、workspace、Provider/MCP 凭据、会话和权威事件日志。
- 创建 Export，选择允许访问的 Agent/会话、workspace template、capability ceiling 和 scopes。
- 配对、查看和撤销客户端设备或 Access Node。
- 对所有本地和远程命令执行最终授权、幂等和会话串行化。

### 3.2 Access Node（访问节点）

访问节点通过 Node Link 导入 Owner Node 的 Agent/会话：

- 向本地 Zed/CLI 暴露 `acp-remote acp-stdio`。
- 向本地 Sync 客户端暴露本节点 owned 资源。
- 经 Node Link 导入的资源通过 `acp_facade` 与 Sync 转发两条路径提供（转发规则见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9.6–§9.7）。
- 只保存连接与信任资料（节点/设备身份、公钥指纹、地址与协议配置、本机信任/撤销决定、连接重试配置）以及节点自有的 owned 资源；**不保存** Owner 的 import 索引、来源游标、ACK、`requestId`、命令状态或任何交付收据——远程数据只在已认证连接期有界存在。
- 将命令转发给 Owner Node，并保留 origin event/sequence。
- 不能修改远程 Agent 凭据、扩大 workspace 或突破 Export grant。
- 不是远程会话权威，Owner 离线时不能伪造 accepted/complete 状态。

同一 ACP Remote 实例可以同时是多个资源的 Owner Node 和 Access Node。

### 3.3 访问客户端

Zed、CLI、PWA、手机 App 和桌面 App 都是客户端形态，不是固定权限角色。它们可以按授权查看消息、继续对话、取消 turn、处理权限/elicitation、选择模型或模式。会话创建、删除与管理能力：本机自己的设备通过认证即可执行（设备方向没有授权维度）；跨节点能力由 Export Policy 与 `grant.*` 决定。Node Link 首个纵向切片支持供 Zed 使用的受限远程 `session.create`；本机设备另可只读地逐层浏览目录以选择工作目录（`workspace.browse`，feature `core.workspace-browse.v1` 门控，见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.2）；PWA 按 `FRONTEND_DESIGN.md` §2.2/§9 在目录内提供创建入口。

对话页的「文件」栏收敛为两个只读来源：「全部文件」逐层浏览**当前会话工作目录**内的文件、「Git 变更」查看该目录相对 HEAD 的未提交状态与差异。2026-10-19 已确认移除「会话变更」来源，客户端与桌面原型已按此调整；首次进入默认「全部文件」，旧的「会话变更」选择回退到「全部文件」，默认来源、回退与界面边界以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) §2 为准。仓库级验证（PV1–PV4）已通过；真实 Daemon 验收尚未执行，状态与未执行项以 [`docs/evidence/remove-session-file-changes.md`](./evidence/remove-session-file-changes.md) 为准。Git 变更可能包含人工操作及其他会话的修改，不代表当前会话独占的改动；对话中的工具调用、文件差异与既有历史记录继续保留。两个来源都只读、不编辑文件、不执行 Git 写操作，也不读取会话工作目录之外的内容；根外链接只展示名称、不可打开。查询范围限于本节点直接管理的 owned 会话：远程导入（imported）会话的两个来源均明确不受支持，不读取 Access 本机替代目录。文件内容、预览与下载只经已认证的 Sync 查询（feature `core.session-files.v1`，见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.5），不生成长期 bearer 链接；内容与 Git 差异不写入会话日志或持久正文缓存，并受分页、预览/下载与并发上限约束。

### 3.4 Zed

Zed 不是系统核心，也不是会话所有者。它是一个可选 ACP Client：

- 可以通过本地 `acp-remote acp-stdio` 接入 Daemon。
- 可以访问当前节点本地拥有或从其他节点导入的 Agent/会话。
- 可以继续已有会话。
- 是否允许 Zed 创建会话由本地授权（[`[[imports]].grants`](./CONFIG_REFERENCE.md) 描述的 Access 侧授权）与 Owner Export Policy 的交集决定。

### 3.5 前端交付策略

客户端的阶段、行为和平台边界以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为权威来源。

- 前端首个交付只实现 Web/PWA，不实现 Android/iOS 原生包；项目实施顺序先完成 Node Link 纵向切片。
- PWA 是真实的端到端验证客户端，与后续原生客户端共享协议、状态机和功能分层，不另建一次性 UI。
- 后续客户端采用 TypeScript 与 Expo/React Native 通用工程，通过平台 adapter 使用 Keychain、Keystore、SQLite、相机和生命周期能力。
- PWA 不能被视为原生安全存储、后台连接和系统通知的等价实现。

### 3.6 对话内容与输入（计划，`conversation-area-complete`，尚未实现）

对话区从“仅文本输入”扩为**按真实能力协商**的文本、图片、文件、已有音频与工作目录路径引用；能力、协议帧、端口与存储形状已在本次合同包冻结，实现仍按工作包推进，未实现前不得对外宣称已支持。

- 内容范围：用户消息、Agent 回复、Agent 明确提供的思考、工具调用及结果、审批、提问、计划、终端、文件改动、轮次结果、多媒体与资源、异常与不可用提示。
- 输入：点击发送后才上传附件，全部完成后整体提交；任一失败保留完整草稿、不触发执行。不支持的类型明确说明，MUST NOT 静默删除附件或只发文字。
- 附件：无数量/大小产品额度，但有 32 KiB 分块、并发、速率、内存与磁盘保护；已提交附件随消息保留，未提交上传闲置 24 小时清理，刷新不承诺续传。
- 交互与历史：刷新或换设备后可重新取得服务端确认的有效待处理请求再应答；首个有效答案胜出。历史与实时同形，超出保留期显式不可用。
- 远程：Owner 保存正文与附件；Access 只在内存转发，不把远程正文写入 SQLite、临时文件、日志或持久缓存。

需求追溯（Q1–Q23 来自 `docs/conversation-area-discussion.md` 的已确认范围；编号以该记录为准，Q22 为思考单独保存与历史恢复，Q23 为流式增量规则；本表给出**权威合同位置**，术语以各文档为准）：

| 需求 | 合同位置 |
|---|---|
| Q1 内容范围 | 本节 / [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.2–§10.4 / [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) §4 |
| Q2 图片、文件、已有音频输入 | 本节 / SYNC §11.5 `PromptContentBlock` / [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §5 |
| Q3 以 ACP 标准能力为基础 | ACP 矩阵 §2/§5 / [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.5 |
| Q4 实时、历史、失败与不支持 | SYNC §10.2–§10.4 / FRONTEND §4.2 |
| Q5 本地与远程会话 | 本节 §3.2 / [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.10 |
| Q6 选择/粘贴图片、文件与音频 | FRONTEND §4.2 / SYNC §11.5 |
| Q7 附件内容、路径引用与外部链接 | SYNC §11.5 `workspace_path_ref` / §15 |
| Q8 点发送才上传、整体提交 | SYNC §3.4 / §11.5 |
| Q9 附件保留与清理 | SYNC §3.4 / [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.5（计划 §7.6） |
| Q10 无产品额度、有界传输 | SYNC §3.4 预算表 / §14 |
| Q11 刷新/换设备重新取得有效请求 | SYNC §11.7 / §10.4 |
| Q12 按一次发送组织轮次 | FRONTEND §4.1 / SYNC §10.3 |
| Q13 折叠与默认展开 | FRONTEND §4.4 |
| Q14 表单类型、校验与首个有效答案 | SYNC §11.7 / §10.4 / FRONTEND §4.1 |
| Q15 执行中可编辑不可发送、无排队 | FRONTEND §4.1 |
| Q16 分块、内存、速率与磁盘保护 | SYNC §3.4/§14 / CORE §5.8 |
| Q17 路径引用不固定版本 | SYNC §11.5 |
| Q18 草稿仅在内存、24 小时清理 | FRONTEND §4.1 / SYNC §3.4 / CORE §7.6（计划） |
| Q19 上读保持锚点、回到底部 | FRONTEND §4.1 / SYNC §11.5 |
| Q20 Markdown、代码与媒体安全渲染 | FRONTEND §4.5 / [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §11.2 |
| Q21 真实 ACP 终端服务 | ACP 矩阵 terminal rows / MODULE §4.5 |
| Q22 思考单独保存与历史恢复 | [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.4 / [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.3、§7.6（计划，WP2） / 本节 §10.3 |
| Q23 增量原位更新、重放去重与部分结果 | SYNC §10.2–§10.4 / FRONTEND §4.2 / CORE §6 第 14 条 |

### 3.7 Web 新建会话的准备与首次提交（计划，`web-session-preparation`，尚未实现）

Web/PWA 的「新建会话」在产品上分成两段：**准备**（临时）与**首发**（正式）。本节的合同已冻结在 [`SYNC_PROTOCOL.md`](./SYNC_PROTOCOL.md) §11.9 与 [`CORE_PORTS_AND_STORAGE.md`](./CORE_PORTS_AND_STORAGE.md) §5.11/§12，wire 资产在 `schemas/sync/v1/` 与 `fixtures/sync/v1/`；实现按工作包推进，落地前不得对外宣称已支持。

- **临时准备**：Owner 先分配**最终** `sessionId` 并签发准备凭据（`acp-remote/session-preparation/v1`），允许为它在 Agent 侧建立临时端点以取得**真实的**模式与配置。准备阶段 ACP Remote MUST NOT 持久化会话记录、目录登记、owned 命令或事件：准备不进入目录/快照、不被其他设备看到，也不产生正式恢复承诺。临时资源只存在运行期，计入节点容量（[`CONFIG_REFERENCE.md`](./CONFIG_REFERENCE.md) §4）。
- **固定标识与地址**：`sessionId` 在准备时就确定，页面地址 `/session/<id>` 在整条流程中保持不变；首次提交成功后不换 ID、不跳转、不重建页面，快照随后让同一地址读到正式内容。
- **首次提交的原子保存与派发围栏**：首条消息走 Sync-only mutation `session.start`（`kind: initial`）。受理只在**同一事务**写完正式会话、目录归属、完整首条输入、已确认的模式/配置、稳定的消息/请求身份与附件正式引用之后发生（阶段 `saved_not_dispatched`）；`accepted` 只在该事务提交后发出。真正调用 Agent 之前必须先在**同一事务**写入派发围栏（`dispatch_started`），因此崩溃后只有两种确定结论：可证明未派发，或进入既有 `uncertain`（不自动重发）。显式继续（`kind: continue`）只允许对可证明未派发、且上一次尝试已有失败终态的首条提交执行。
- **不采用的替代**：不发送「隐藏首条消息」促使 Agent 写盘，不在准备阶段自动派发 prompt，也不把「已有恢复数据缺失」解释成可重新准备。旧 `session.create`（立即持久化）的语义与 wire 形状不变；未协商该 feature 的客户端**不得**回退到它来代替准备式新建。
- **正常重启与存储重建**：正常重启后，有效准备凭据可以重新取得/重建准备资源（已受理则返回 `committed` 关联）；存储被整体重建（`storeEpoch` 改变）时旧凭据统一失效并返回不可重试的 `preparation.store_rebuilt`，客户端丢弃旧材料、认证后回目录重新开始。准备阶段建立的空 Agent 会话**不**承诺跨进程恢复。

准备阶段的浏览器草稿、刷新与标签页归属见 [`FRONTEND_DESIGN.md`](./FRONTEND_DESIGN.md) §4.6；凭据不是登录凭证、草稿的窄例外与清理边界见 [`SECURITY_DESIGN.md`](./SECURITY_DESIGN.md) §12.8；决策背景见 [ADR-0014](./adr/0014-web-session-preparation-before-first-message.md)。

## 4. 总体架构

```text
远程 PWA/手机/桌面/CLI ── Sync ──> Owner Node ── ACP stdio ── Agent

远端/本地 Zed ─────── local ACP ──┐
                                  ├──> Access Node ── Node Link ──> Owner Node ── ACP stdio ── Agent
本地 PWA/手机/CLI ──── Sync ──────┘
```

Sync 客户端可以直连资源归属的 Owner Node，也可以连接 Access Node 并看到该节点从 Owner 导入的资源；Access Node 只转发，不成为正文权威，也不改写 ACP 语义。

每个 Agent/会话的 Owner Node 是该资源的唯一权威：

- Owner Node 是底层 ACP Agent 的唯一 ACP Client。
- Owner Node 管理 Agent 子进程、ACP request ID、会话状态和 origin event log。
- Access Node 是远程 Agent backend 的消费者，可以重排为本地 sequence，但必须保留来源身份与 cursor。
- 每一跳都执行认证和授权；最终授权在 Owner Node 强制执行。
- Node Link 的详细语义以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准。

客户端之间不直接通信，Node 之间也不共享 Agent 的 stdin/stdout；跨节点只使用版本化 Node Link。

## 5. 核心模块

本节描述 Daemon 的**运行时职责**，不代表 Rust crate 的拆分方式。一个职责可能由多个 crate 协作完成，也可能只是某个 crate 的内部模块。crate 名称、边界和依赖关系以 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) 为唯一权威来源。

```text
ACP Remote Daemon
├─ Agent Process Manager   Agent 子进程生命周期
├─ ACP Transport           JSON-RPC/stdin/stdout
├─ Agent Backend Router    本地 AgentHost / 远程 Node Link
├─ Session Registry        会话与 Agent 会话 ID 映射
├─ Session Actor           每会话串行命令队列
├─ Command Router          命令验证、授权和幂等
├─ Event Store             SQLite 持久化
├─ Event Dispatcher        实时广播与断线补发
├─ Identity Service        配对、认证、撤销
├─ Sync Server             HTTP/WebSocket
├─ Node Link Client/Server Export/Import 与节点通信
├─ Export Registry         导出策略与 capability ceiling
└─ Local ACP Facade        可选 Zed/IDE 接入
```

### 5.1 Agent Process Manager

- 启动 `codex-acp`、`omp acp` 等程序。
- stdout 只按 ACP JSON-RPC 解析。
- stderr 只做有界采集与结构化计数（内容不进日志，可按上限回取）。
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

会话主状态（2026-10-20 `session-activity-three-states`）：

- 会话只有三种活跃程度：`idle`、`running`、`pending`。判断顺序固定：至少一个有效未解决交互 → `pending`；否则仍有未结束 turn（`queued`/`running`/`waiting_*`，含已派发但停止尚未确认的执行）→ `running`；否则 `idle`。
- `failed`/`cancelled` 是一个 **turn** 的结果，不是会话终态：失败或取消只终结当前轮，会话按剩余排队/交互重新归约，后续 prompt 继续执行。会话没有 `closed` 状态，也没有 `closed_at`。
- `pending` 包含「已仲裁、等待 Agent 确认处理」的交互：first-writer-wins 只阻止其它设备重复应答，不冒充已解除；端点确认后该交互才退出有效集合。一个会话可同时有授权与表单请求，原因从各有效交互取得，不用单一互斥字段。
- 历史数据里的 `queued`/`waiting_*`/`failed`/`closed` 只是**旧版本的过去事实**（迁移暂存或显式历史读取），不能作为实时摘要解码，也不得覆盖重算后的三态摘要。任务失败、命令 `uncertain` 等结果词表不变。

本机会话生命周期（`session-activation-lifecycle`，**合同已冻结，实现待 WP2–WP5**；wire 见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9.8/§11.5/§11.8）：

- **进入即激活**：用户进入或返回可见的本机会话页面时立即请求激活。已可用的运行资源直接复用；已被回收时恢复同一 Agent 会话身份与工作目录，使后续交流承接原上下文。MUST NOT 用新建会话、重发历史或只展示旧正文冒充恢复成功；恢复失败时历史可读、草稿保留并明确说明原因。
- **只在可见且选中时保活**：只有已认证、页面可见且当前选中该会话的客户端计为查看者；阅读历史同样保活。每个连接最多一个当前查看会话，查看声明有 30 秒租期、可见页面每 10 秒续租，普通网络心跳不续租。后台、锁屏、离页、断开与设备撤销都不再保活；重新可见或重连后立即重新声明并激活。
- **多人共享一次激活**：同一会话的多个查看者共享一次激活与同一份运行资源；第一个离开不触发回收计时，最后一个离开后才开始计时。
- **保留期**：没有查看者、未结束/排队任务、有效待处理请求与停止未确认的执行时，才开始完整的 30 分钟保留期（`sessions.idle_timeout_ms`，可配置；显式 `0` 只关闭按时间回收）。任一保护条件重新出现即取消计时，全部解除后重新计时。页面阅读、心跳与配置查询不刷新它。
- **容量保护**：本机同时占用运行资源的会话数受 `sessions.max_active_sessions` 限制（正整数，候选默认 50，**待实测**；没有无限取值）。创建与恢复共用名额，待处理会话计入、历史会话不计入。满额时优先提前回收无人查看、无任务且无交互的最久未使用会话；所有会话都受保护时以 `session.capacity_exceeded` 拒绝新激活，保留历史可读，MUST NOT 中断受保护会话。50 不是已验证的安全上限，也不是内存硬限额。
- **展示**：协商该能力的客户端用「空闲 / 活跃 / 待处理」呈现本机活跃程度（有效交互优先待处理；否则未结束任务、已确认可用资源或回收未确认时为活跃；否则空闲），任务细节（正在执行/排队/失败原因）仍读任务事实。未协商的连接与远程导入会话沿用任务三态，不显示本机活跃字段。活跃不是拒绝发送的理由；发送只依据实际可用的运行资源与既有排队策略。
- **回收**：按会话释放运行资源，保留会话身份与全部历史，不影响同 Agent 的其他会话；只有全部会话都可回收时才允许停止共享进程。无法证明真实释放时明确报告「回收未完成」并保留占用，MUST NOT 只改展示或计数冒充成功。

### 5.3 ACP 兼容性不变量

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

ACP Remote 是 `ACP-aware broker`，不是把不同 Agent 压缩成最低公共能力集合的抽象层。实现必须遵守：

- ACP 标准能力保持原有语义；中转站只增加会话 ID 映射、事件序列和来源等必要元数据。
- Broker 必须理解的能力转换为统一领域模型；Broker 无需理解的合法能力和 Agent 扩展应保留原始 ACP payload 并尽可能透传。
- Sync Protocol 使用 `view + acp.rawJson` 双轨表示，防止浏览器解析未知字段时损坏大整数、键顺序或数字文本；公共视图不能替代原始 ACP document。
- 解码未知字段或新版扩展时不得静默丢弃；无法安全处理时必须返回明确的 `unsupported` 或协议错误。
- 权限请求、工具调用、终端、文件修改和 diff 等结构化事件不得降格为普通文本。
- `acp-facade` 必须如实进行 capability negotiation，不得声明实际无法提供的能力。
- 任一客户端或 Node Link 暂不支持某项能力时，应显示明确的降级或不支持状态，不能改变该能力的含义。
- 存储可以按保留策略压缩或清理无价值的流式噪声，但在事件有效期内必须保留完成重放所需的结构化语义和扩展数据。

具体 ACP v1 方法、通知、content block、capability gate、各层处理策略和验收测试以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 及其机器矩阵为准。任何能力不得只靠文档中的概括性描述宣称支持。

## 6. 网络与部署

### 6.1 Direct Path Only

第一阶段只支持客户端或 Access Node 通过用户提供的网络路径直接连接目标节点，不提供应用级云中继：

```text
Client -> HTTPS/WSS -> ACP Remote Node
Access Node -> Node Link/WSS -> Owner Node
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

正式模式要求 TLS 直接终止在 Daemon 或节点主机上的可信 Provider。局域网模式可以使用同机反向代理和受信本地 CA；Daemon 不应默认无认证监听 `0.0.0.0`。明文 HTTP/WS 只允许显式开发模式和 loopback。

### 6.3 Endpoint 与身份分离

原生客户端或 Access Node 保存一个目标 Node identity 和多个候选地址，并在每个 endpoint 上验证同一个身份。Sync v1 wire 中仍沿用 `hostId/hostPublicKey` 字段名，它们语义上表示当前服务节点：

```json
{
  "hostId": "host-uuid",
  "hostPublicKey": "base64...",
  "endpoints": [
    "wss://work-pc.example.ts.net/sync/v1",
    "wss://work-pc.home.arpa/sync/v1"
  ]
}
```

endpoint 的 path **以 wire 权威为准**（`SYNC_PROTOCOL.md` §3.1：WebSocket endpoint 为同源 `/sync/v1`；本节示例按该口径书写）。原生客户端或 Access Node 的 IP/endpoint 变化只需要重新发现地址，不需要重新配对。任何 endpoint 都必须证明持有同一个 Node 私钥。

PWA 是例外：浏览器密钥和 IndexedDB 按 Origin 隔离，一个 PWA 设备身份只绑定一个 `scheme + hostname + port` canonical origin。更换 Origin 必须作为新设备重新配对，第一阶段不支持 PWA 跨 Origin 自动切换；同一 Origin 内的路径变化不受影响。详见 [ADR-0001](./adr/0001-pwa-identity-and-secure-transport.md)。

### 6.4 无云模式限制

- Owner Node 关机、休眠、断网或 Daemon 停止时，远程客户端/Access Node 无法控制其 Agent。
- 默认 `no-content-cache` 下，Access Node 或客户端只能在 Owner 在线时读取会话正文；Owner 离线时只显示连接状态与显式的不可用说明，Access 不保留可展示的远程摘要/正文副本。
- Owner Node 离线时不能可靠排队 prompt，也不能伪装为已接受。
- 手机/PWA 后台 WebSocket 可能被操作系统挂起。
- 在不使用 APNs/FCM 等云推送服务时，后台通知不能保证实时到达。
- 客户端或 Access Node 恢复后，通过各自 cursor 自动补发遗漏事件。

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

事件信封、事件 body 字段、`origin` 块、`remoteOrigin` 块、`payload.view` 与 `payload.acp` 的完整形状以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §4 与 §10.1 为唯一权威来源，本文不重复定义。

产品级语义：

- `globalSequence`：一个 ACP Remote Node 本地事件日志范围内的增量同步游标，编码为无前导零十进制字符串。
- `sessionSequence`：单会话严格排序，非会话级事件为 `null`。
- `eventId`：客户端去重；imported 事件的 `eventId` 等于 Owner 的 `originEventId`，跨节点不重新编号。
- `origin.kind`：记录事件来自 Agent、设备、Daemon 或本地 CLI；远程来源不放这里，单独放在 `remoteOrigin`。

### 7.3 订阅与补发

订阅消息、cursor 二元组、快照与 ACK 的完整形状以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9 为唯一权威来源；客户端以持久化 cursor 订阅，而不是用裸 sequence 比较大小。

Daemon：

1. 从 SQLite 补发 cursor 之后的可见事件。
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

慢客户端或 Access Node 不能阻塞 Agent 或其他订阅者。

## 8. 客户端命令

UI 客户端和 Access Node 使用受约束的业务命令，而不是向 Owner Node 直接发送任意 ACP JSON-RPC。

命令名、类别、`scope`、`grant`、`transport` 与首阶段交付状态以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 为唯一来源，本文不重复维护命令清单；命令信封、`requestId`、`command.result` 与 payload 字段以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11 为准。

Daemon 立即返回接受、排队或拒绝状态；实际执行结果通过事件流发送。

命令名不等于权限：设备方向以「该连接已通过设备认证且设备未撤销」为执行前提（设备没有授权维度，配对不授予授权），跨节点方向另按 Owner Export Policy 与 `grant.*` 授权；逐命令的对照表见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.2。原始 workspace 路径选择、Agent/Provider 配置、设备与 Export 管理、Node key 轮换和审计导出只能由 Owner Node 本地管理入口执行，永不远程授予，清单见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.3。

跨节点的远程 `session.create` 只能选择已发布的 Agent 与 workspace template，禁止提交 `cwd`、`mcpServers`、绝对路径或任何 Provider/MCP 凭据；本机自己的设备在 Sync 面可以提交一个绝对目录，但必须先通过本机的绝对路径与存在性校验，客户端提交的目录不会被回显，路径的可见面只有两处：协商 `core.workspace-path.v1` 时随目录资源下发**已登记**目录的规范化路径，协商 `core.workspace-browse.v1` 时随只读浏览结果下发该层直接子目录的绝对路径（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §12.3）。目录浏览 `workspace.browse` 是只读查询（feature `core.workspace-browse.v1` 门控，不登记、不产生审计动作），创建会话时可带一个「目录不存在则先创建」开关；命令与载荷形状以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.5 为准。

会话标题有两个来源：Agent 经 ACP `session_info_update` 给出的标题，以及用户经 `session.title.set`（feature `core.session-title.v1`）设置的**自定义标题**。有效标题 = 自定义标题优先 → 否则 Agent 标题 → 否则未命名，由会话所属节点（Owner）投影成既有的 `SessionSummary.title` 下发，因此旧客户端不需要新字段也能显示。标题只由 Owner 持久保存并向获准客户端同步；Access 只转发命令，不持久化标题、命令状态或转发进度。该命令是 Owner 元数据能力，不是 ACP 能力：不新增 ACP 方法，不要求 Agent 具备重命名能力，也不改写 ACP 原文。系统不提供清除自定义标题或恢复 Agent 标题的命令；Agent 改名与显式清空只更新 Agent 标题，不覆盖已存在的自定义标题。

## 9. 模型与模式选择

模型与模式不引入 ACP 之外的抽象，两者都直接复用 ACP 原生机制：

- 模型选择复用 ACP `SessionConfigOption`（`category` 为 `model` 或 `model_config`）与 `session/set_config_option`；Sync 命令为 `session.config.list` / `session.config.set`，事件为 `session.config.changed`。
- 模式选择复用 ACP `session/set_mode`；Sync 命令为 `session.mode.list` / `session.mode.set`，事件为 `session.mode.changed`。
- 配置项与模式的公开视图、命令 payload 和事件形状以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §10.3 与 §11.5 为唯一权威来源；Agent 是否暴露某项配置以 ACP `agentCapabilities` 为准。

规则：

- 客户端只能选择 Owner Node 已配置且当前 Agent 实际暴露、Export 允许的配置项取值与模式。
- Provider 凭据不发送到 Access Node 或访问客户端。
- 切换携带 `expectedVersion`，避免并发覆盖。
- 当前 turn 运行中时，切换默认从下一 turn 生效。
- 如果 Agent 不支持会话内切换，必须明确返回“不支持”，不能静默丢失上下文。
- 变更通过事件同步给所有客户端。

## 10. 数据存储

Owner Node 需要保存足以支持客户端和 Access Node 历史查看、断线恢复的数据，但不永久保存完整 ACP 原始流。Access Node 采用 `no-content-cache` 且**不持久化任何 Owner 远程资源**：不保存 import 索引、资源来源、cursor/ACK、`requestId`、命令终态引用、事件摘要/digest 或 local sequence 映射，也不保存 prompt、回复、工具内容、diff、终端输出、附件或 ACP raw 正文。远程目录、来源事件与读取结果只在已认证连接期有界存在；Access 不先提交收据再广播，而是把 Owner 来源事件原样转发（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §9.7）。重放正文时按 origin cursor 回源 Owner。未来的加密离线正文缓存必须由独立 feature、Owner Export Policy 和 ADR 显式启用。

`no-content-cache` 约束 ACP Remote Access Node 和项目自带客户端。数据一旦按 Export 授权发送给 Zed 或其他第三方 ACP Client，ACP Remote 无法保证该客户端不保存历史、日志或崩溃转储；Owner 管理员必须把允许该客户端接收内容视为一次数据披露授权，并通过受管终端策略控制第三方留存。

### 10.1 长期保存

- 用户最终消息。
- Agent 合并后的最终回复。
- 工具调用摘要和结果状态。
- 权限请求与最终决策。
- 配置项与模式变化。
- Turn 生命周期。
- 必要的文件修改摘要。
- 会话与底层 Agent session ID 映射。
- 已提交附件的引用、封存清单与媒体引用（字节在附件目录，表只存引用/摘要）。
- 回复与 Agent 明确提供的思考的消息通道，以及停止/失败时的部分结果状态。
- 有效待处理交互的版本与终态事实（历史卡片不能直接应答）。

### 10.2 短期保存

- 流式文本 delta。
- 临时工具进度。
- 未完成的请求状态。
- 点击发送前不存在的上传；点击发送后才创建的临时传输与未封存清单（闲置 24 小时清理，不随刷新续传）。

所有带 sequence 并进入 Sync Protocol 的流式 delta 都必须先写入短期事件日志，再广播。`persist_deltas: false` 只表示 turn 完成并形成最终消息后可以按 ACK、TTL 和容量策略压缩或清理，不允许绕过“先持久化、后广播”；清理造成 cursor 超出保留窗口时必须要求客户端重建 snapshot。

### 10.3 不保存或严格限量

- 心跳、typing、presence：只放内存。
- Agent 未明确提供的思考：从不生成、不保存；Agent 明确发送的思考按会话正文的保留/清理规则处理，并支持历史恢复（与回复分别保存）。
- 终端完整输出：限制 head/tail 和总大小。
- 大型 diff：压缩并限制容量。
- 图片、音频与文件：已提交附件使用内容寻址存储，随所属消息保留与清理；不设附件数量/大小产品额度，但受分块、并发、速率、磁盘与库容量保护。未提交上传按 24 小时闲置清理；远程附件正文只由 Owner 保存，Access 不落盘。
- 准备阶段的临时资源：运行期准备目录、租约/资源代、准备凭据与临时端点都不写 SQLite，也不进快照、事件或命令历史（[`CORE_PORTS_AND_STORAGE.md`](./CORE_PORTS_AND_STORAGE.md) §5.11）。首次受理事务才写入正式记录（`owned_first_submission` 的计划 DDL 见该文档 §12）：完整首条输入、稳定消息身份与派发阶段随会话保留；即使首条尚未派发，它也是正式记录，不随未提交上传的 24 小时 TTL 清除。存储整体重建后旧准备凭据失效（`preparation.store_rebuilt`），客户端不得凭旧材料恢复。

### 10.4 保留策略默认值

保留期、容量上限、终端截断与流式落盘的具体键名、类型与默认值统一在 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) 维护，本节不重复。

产品级约束在这里：这些值必须可配置；`persist_deltas` 只影响 turn 完成后的压缩与清理，不允许绕过“先持久化、后广播”；清理造成 cursor 超出保留窗口时必须要求客户端重建 snapshot。实际取值通过使用数据调整。

## 11. 双方认证与长期配对

本节描述产品级身份行为；系统威胁模型、数据保护、授权分级、供应链和安全验收以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为权威来源，设备 wire contract 以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 为准，节点 wire 边界以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准。

### 11.1 安全原则

- 网络可达不等于应用授权。
- Tailscale 身份不能替代 ACP Remote 设备身份。
- 每个 ACP Remote Node 和客户端设备拥有独立长期密钥；Node key 与 Device key 用途分离。
- 首次扫码只用于建立信任。
- 后续连接自动执行双向设备或节点签名认证。
- TLS 为每次连接产生临时会话密钥，应用长期密钥只用于 Device/Node 身份签名。
- 每个业务命令仍需执行权限检查。

### 11.2 首次配对

服务节点生成长期身份。Sync v1 为兼容既有 wire 继续使用 `hostId/hostPublicKey` 字段名，其语义是服务该连接的 Node Identity：

```text
hostId
hostPrivateKey       ECDSA P-256，不可写入普通存储
hostPublicKey        P-256 public key
```

`acp-remote device pair` 创建一次性**短引用**二维码：

```text
https://work-pc.example.ts.net/pair#q2=<payload>
payload = base64url-no-pad(0x02 || 32 字节扫码取回凭据 || 32 字节主机公钥指纹)
```

设备二维码不再内嵌完整配对资料；原生 App 扫码后经同源 `POST /sync/v1/pairing/qr-lookup` 取回 `pairingId`/`hostId`/`hostPublicKey`/`canonicalOrigin`/`pairingSecret`/`expiresAt`。网页/桌面/手机 PWA 只输入 8 位配对码，不消费设备二维码。旧设备 `#data=` 完整资料格式明确停用，节点二维码不受影响。字段布局、拒绝规则、端点与限额以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §7.1/§7.6 为唯一权威来源；本轮只交付协议合同与共享向量，原生 App 与 `qr-lookup` 服务端仍待后续工作包实现。

规则：

- `pairingSecret` 与扫码取回凭据分别使用 CSPRNG 独立生成；两者不是彼此的派生值。
- 配对请求只允许成功一次。
- 二维码固定 5 分钟过期（与 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14 的固定 v1 常量一致，不作为配置项）。
- 新配对使旧的未完成配对失效。
- 客户端（PWA 输码或后续原生 App 扫码）认领后生成自己的长期设备密钥。
- 服务节点本地入口显示设备名称与指纹，并要求用户确认（节点方向另显示待授予的 `grant.*`）。
- 配对不派生也不显示短验证码：设备侧对「此后长期使用的主机公钥」的信任由两条密码学校验确立——原生 App 用扫码取回且**完整匹配二维码 32 字节指纹**的主机公钥验证认领响应的 `hostProof`，且已批准响应里的主机公钥必须与该绑定公钥**逐字节相同**；PWA 的绑定公钥来自同源配对码 lookup（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §5.1）。
- `pairingSecret` 只作为短期配对状态凭据：设备首次 WSS 认证成功时立即清除，未完成、拒绝或未连接的 pairing 最迟在原过期时间清除；不得延长或转成长期 bearer token。
- PWA 与后续原生 App 使用 `pairingSecret` 对规范 transcript 计算 HMAC-SHA256 proof（认领与状态两条 domain）；不派生、不展示短验证码。设备二维码里的扫码取回凭据只用于取回资料，不参与 proof。
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

默认安全 Profile 的 Node 与设备签名统一使用 ECDSA P-256 + SHA-256；wire signature 固定为 64-byte P1363 `r || s` 后进行无填充 base64url 编码。TLS/WSS 负责临时密钥、机密性和完整性。正式 TLS 必须直接终止在 Daemon 或节点主机上的可信 Provider。Noise 仅保留为未来不可信中继等场景的可选 Transport Profile，不进入第一阶段。

每个新 WSS 连接都使用新 nonce 完整执行 challenge-response，不签发长期 bearer session/refresh token；认证状态随连接关闭而失效。

### 11.4 设备记录与命令前提

服务节点保存：

```json
{
  "deviceId": "phone-uuid",
  "name": "User Phone",
  "clientKind": "pwa",
  "canonicalOrigin": "https://work-pc.example.ts.net",
  "keyAlgorithm": "ECDSA_P256_SHA256",
  "signatureEncoding": "P1363_BASE64URL",
  "publicKey": "<base64url-65-bytes>",
  "createdAt": "...",
  "revokedAt": null
}
```

设备记录与 wire 都**不保存任何授权集合**（设备配对不授予授权，设备方向没有授权维度）；`grant.*` 是唯一的授权管理输入形式，落到节点信任记录与 Export 前展开为它直接列出的命令名集合，配对请求中出现任何授权集合字段一律被拒绝。逐命令的对照表以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §10.2 为唯一权威来源，命令名集合以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 为准。

每条消息必须检查：

- 连接是否完成双向认证。
- `deviceId` 是否匹配当前连接身份。
- 设备是否已撤销（撤销判定以持久化记录为准，进程内连接状态不得让已撤销设备继续可用）。
- 目标的会话状态、workspace 边界与命令 schema 是否满足（设备方向不再有命令级 scope 判定）。
- `requestId` 是否已经处理。
- 连接消息计数器是否有效，防止重放。

### 11.5 密钥存储

ACP Remote Node：

- Windows：DPAPI 或 CNG/TPM；第一阶段实交付档位是 DPAPI（当前用户 scope）包裹私钥 + 进程内签名，CNG/TPM 不可导出档位需单独 ADR（`SECURITY_DESIGN.md` §9.2/§20）。
- macOS：Keychain。
- Linux：Secret Service 或经过单独评审的系统 keystore；不可用时正式模式失败关闭。持久化加密文件 fallback 必须先通过独立 ADR，不能临时自研。

原生客户端设备：

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

### 11.7 节点配对

Node pairing 与设备 pairing 使用相同的“一次性引导、长期公钥信任、每连接 challenge-response”原则，但必须使用不同 key purpose、record type 和 transcript domain。Owner Node 本地确认 Access Node 指纹、Export 和初始 `grant.*`（不显示短验证码，理由见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §5.1）；后续无需重复扫码。Node 配对不信任 Access Node 的下游节点，第一阶段禁止 imported Agent 再导出。

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
- 附件必须检查声明大小、实际大小和类型；单块、偏移、总长度与流式摘要都必须校验，重复块只在内容相同时返回原确认。
- 附件连接必须单独完成与业务连接相同的认证、Origin/Host、撤销与心跳校验；每一帧重新校验会话与主体绑定，读取不得依赖公开或长期 bearer URL。
- 外部链接与远程媒体不因展示而自动获取；不可用/过期内容显式说明，不得以空内容冒充。
- 不允许客户端或 Access Node 绕过允许的业务命令构造任意底层 ACP 方法。

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

acp-remote device pair
acp-remote device list
acp-remote device revoke

acp-remote node pair|list|revoke
acp-remote export create|list|revoke
acp-remote import add|list|remove

acp-remote acp-stdio
acp-remote doctor
```

- **首切片不提供 `session create`**：业务命令 `session.create` 在 `compatibility/commands/v1/commands.json` 里的 transport 只有 `node_link`（它是 Access 请求 Owner 远程建会话，受 `grant.remote-work` 约束），而本地通道不承载业务命令；本地会话的入口是 `acp-remote acp-stdio`（Zed → `server::acp_facade` → `core::use_cases::create_session`）。`session list` 同属 `post_mvp`，需先解决“是否为命令引入 `local` transport”。
- CLI 子命令到本地方法/业务命令的映射、配对的多步流程与退出码约定见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.8；本文不重复。

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

当前优先交付平台（2026-09-23 用户决定）：

```text
Windows x64
```

Windows 是当前 Daemon/CLI、节点配对、进程树管理和首个 Node Link 闭环的验收目标。Linux 延后开发，Secret Service 与无桌面环境 fallback 不作为 Windows 开工前置条件；后续支持 Linux 前仍须完成其平台安全设计与验收。macOS ARM64 保留为后续平台计划，不要求与 Windows 同批交付；其他架构和 musl 同样后续增加。上面的包名列表是长期目标，不代表全部已有产物。

共享 core、协议及存储层继续保持平台无关；已有 Unix 代码和 Linux CI 作为可移植性回归保留，通过这些检查不等于已经支持 Linux Daemon 产品。平台优先级不改变 PWA 可作为其他设备浏览器客户端的产品定位。

发布流程：

1. CI 在对应平台构建和测试二进制。
2. 生成校验和和发布产物。
3. 先发布相同版本的所有平台包。
4. 最后发布 npm 入口包。
5. 所有平台包与主包严格保持同一版本。

## 15. MVP 实施顺序

### 阶段一：Node Link 最小纵向切片

- Owner 与 Access 优先在 Windows 上交付和验收；其他节点平台按 §14 后续推进。
- Rust Workspace、ACP/Node Link 协议类型、最小 Broker 和 fake ACP Agent。
- Node Identity、节点配对、Export/Import 和撤销。
- 单 Owner、单 Access、单 Export 的 catalog、command/event、cursor、重放和端到端幂等。
- Access Node 的 remote Agent backend 与 `acp-remote acp-stdio`。
- 远程 Zed 经 Access Node 完成 initialize、session/new、session/prompt、session/update 和 cancel。
- `session/new` 必须经 Node Link `command.submit(session.create)` 映射为受 `grant.remote-work`、已导出 Agent 和 workspace template 约束的远程 `session.create`；请求禁止携带 `cwd`、`mcpServers`、绝对路径或凭据字段。
- 第一阶段单 hop；禁止 imported Agent 再导出。
- capability 交集、ACP raw 跨节点保真、默认 `no-content-cache` 和 Owner 离线正文不可用。
- 第一阶段采用节点级信任，Owner 以 Access Node 为授权 principal，`localPrincipalRef` 只作审计归因。

### 阶段二：本地 Web/PWA MVP

- 增加 Sync 协议类型、HTTP/WebSocket Sync Server、事件序列、ACK、补发和命令幂等。
- 增加设备身份、一次性配对、双向认证、基本权限和撤销。
- 交付 Web/PWA：查看已有会话、继续对话、结构化事件、权限处理、模型列表与切换；首版不展示会话创建入口。
- PWA 静态资源由 Daemon 本地托管；不实现 Android/iOS 原生包。
- PWA 遵守 Export 的 `no-content-cache`，会话正文默认只保留在内存。

### 阶段三：可靠性、安全与网络完善

- 浏览器/节点密钥方案与跨端加密互操作验证。
- 重连、慢客户端、Access 不落盘检查、进程崩溃和数据库迁移测试。
- 存储 TTL、大小限制、流式事件合并和敏感信息审计。
- Tailscale 部署文档和可选辅助工具。
- 多 endpoint 自动连接、局域网与可替换 HTTPS 暴露方案。
- Codex ACP 适配和能力差异测试。

### 阶段四：原生客户端

- Android Expo Development Build，使用 Keystore、原生 SQLite、扫码和生命周期 adapter。
- iOS 客户端，使用 Keychain 并验证本地网络权限和后台恢复。
- 复用 PWA 已验证的 Sync Client、状态机、协议类型和 feature 层。
- 是否引入系统推送服务作为独立产品与隐私决策处理。

### 阶段五：发布

- Windows 产品构建、平台安全与进程生命周期验收；保留现有 Linux 共享代码 CI。macOS/Linux 产品发布门禁随对应平台开发补齐。
- npm 平台包和 launcher。
- 数据库迁移策略。
- 升级、回滚和 Daemon 单实例处理。
- 安全与异常恢复测试。

## 16. 编码前必须验证的风险

1. 各 ACP Agent 是否都支持会话恢复、模型列表和运行中模型切换。
2. Codex ACP 与 OMP ACP 的事件、权限和终端能力差异。
3. Zed 是否能正确展示由其他客户端或 Access Node 发起的、非 Zed 本端发起的 turn。
   - 2026-09-18 **已验证（手工探针）**：用一个不接收 `session/prompt` 就主动推流的假 ACP agent 驱动 Zed，Zed 正常显示外部 turn 的流式 `agent_message_chunk`、`agent_thought_chunk`（Thinking 块）、`tool_call` 与 diff 内容块，并显示**可交互**的 `session/request_permission` UI（Allow once / Reject），由 Zed 自己提交 `{"outcome":{"outcome":"selected","optionId":"allow-once"}}`。也就是说外部 turn 的权限请求不需要 facade 代答。
   - 仍未确认（都不阻塞，属"呈现更完整"而非"能否显示"）：`plan` 与 `usage_update` 是否渲染、外部 turn 进行中是否提供 Stop/取消按钮。
4. Windows 上 Daemon、子进程树和休眠恢复行为。
   - 2026-09-18 **子进程树清理已验证（一次性探针）**。探针零依赖（直接 FFI kernel32），源码留在 `%TEMP%\acpr-job-probe`（仓库外，未提交）。它造出「父 → 孙」两层进程树、每 50ms 写心跳，三种场景的结果：
     - **普通 spawn**：杀掉父进程后孙进程**继续写**（探针退出后 `A.log` 仍在增长，实测 2 秒 +936 字节，只能靠 `taskkill` 收尸）——这正是「daemon 崩溃留下孤儿 Agent 进程」的风险；
     - **Job Object + `TerminateJobObject`**：孙进程立即停止，前后字节数完全一致（922 → 922）；
     - **Job Object + `KILL_ON_JOB_CLOSE` + 关句柄**（模拟 daemon 崩溃）：同样立即停止（900 → 900）。
   - 结论：`agent-host` **必须**用 Job Object 管理 Agent 进程树，并把 `KILL_ON_JOB_CLOSE` 设在守护进程持有的 Job 上；探针实测生效的 `ExtendedLimitInformation` 布局（144 字节）与 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000` 可直接复用到实现里。
   - 仍未验证：daemon 自身的休眠/唤醒行为（需要可运行的 daemon，属实现期）。
5. 手机后台 WebSocket 被系统挂起后的恢复体验。
6. WebCrypto P-256 密钥持久化及其与 Rust 的签名格式互操作性。
   - 2026-09-18 **Rust 侧已验证**。用 `p256 0.13.2`（`ecdsa 0.16.9`、`signature 2.2.0`）实现 transcript 编码、P1363 验签、HMAC-SHA256 与 SAS 派生，对 `fixtures/{sync,node-link}/v1/` 的固定向量逐项复算：12/12 重编码逐字节一致、6 个 P1363 签名验证通过、6 个 HMAC 重算一致、2 个 SAS 一致、20/20 畸形输入（transcript 结构错误与非法公钥）以声明的错误被拒。
   - 选型：`p256` + `sha2` + `hmac` + `base64`（无填充 base64url）。四者都是纯 Rust、无原生依赖、无 `cfg` 平台分支，与"`identity-auth` 保持纯状态机"的拆分一致。**四个原语的当前版本口径与变更规则见 `MODULE_ARCHITECTURE.md` §3.1**（本节只留这次实测记录与实现约束，不重复维护版本号）：2026-09-23 起 `sha2`/`base64` 已由 0.10/0.22 升到 0.11/0.23，算法语义不变，但那次一次性探针没有重跑，因此本节的实测向量仍是实现阶段必须固化成常驻测试的回归要求（见本条末句）。
   - 实测得到的实现约束（不写就会错）：`VerifyingKey::from_sec1_bytes` **接受 33 字节压缩点**，所以必须先断言 65 字节再解析，否则违反"SEC1 uncompressed"合同；`Signature::from_slice` 只接受 64 字节 P1363，70 字节 DER 被拒（不得在 wire 上使用 `from_der`）；合法 high-S 与 low-S 都必须被接受（实测确认）；`r`/`s` 为 0 必须被拒；带填充或非 base64url 字母表必须被拒。
   - 仍未验证的部分：PWA 侧不可导出 `CryptoKey` 的 IndexedDB 持久化，属浏览器行为，阶段二开工前用一个静态页验证即可。
7. 客户端与 Node endpoint 变化时的安全发现方案。
8. SQLite 写入频率、流式事件合并和磁盘上限策略。
9. npm optional dependency 在 npm、pnpm、yarn 不同配置下的安装行为。
10. ACP 协议升级时的版本协商和向后兼容方式。

按"什么时候必须解决"分类（2026-09-18，避免重复评估）：

- **开工前必须**：#6 —— 已完成（结果见上）。它决定密码学依赖选型，选错会导致 `identity-auth` 返工。
- **必须固化为常驻回归测试（不是一次性探针）**：#4 的 Windows Job Object 行为（父→孙进程、杀父后孙停止、关 Job 句柄后孙停止）与 #6 的密码学向量与畸形输入（transcript 重编码、P1363 验签、HMAC、33 字节压缩点与指纹不一致）。两者目前只有未提交的探针记录；实现变更必须把它们带进 CI 可跑的测试，否则视为未完成（约束落在 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md) §3 与 [MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.5）。
- **首切片验收前必须**：#3（Zed 对非本端发起的 turn 的展示；用假 ACP agent 即可预验，不依赖本仓库代码）、#1/#2（需要真实的 Codex/OMP，用于填写能力兼容报告；不阻塞编码，因为矩阵中这些能力本就是 `conditional_mvp` + `advertise_if_end_to_end`，代码只需如实协商）。
- **实现期验证**：#4、#5、#8、#9 —— 需要可运行的程序、真机或发布流程。
- **设计项，不是验证项**：#7 —— 首切片的 endpoint 由配置与 Import 记录给出，不需要发现机制；到阶段三"多 endpoint 自动连接"时才需要设计。
- **已由合同层覆盖**：#10 —— ACP 矩阵固定上游 commit + sha256 并由 `npm run check` 强制校验，升级流程见 `docs/ACP_COMPATIBILITY_MATRIX.md` §2（权威来源与版本固定，含 5 步升级流程）；未来真有新版本时执行该流程即可。

## 17. 核心设计原则

1. 每个 Agent/会话的 Owner Node 是该资源状态的唯一权威。
2. 底层 Agent 只保持一个受控 ACP 连接。
3. 先持久化、后广播。
4. 每会话串行执行，不允许并发 prompt 破坏状态。
5. 事件至少投递一次，客户端负责去重。
6. 命令可重试，但效果必须幂等。
7. 网络可达与应用认证分离。
8. 设备身份与 endpoint 分离。
9. 长期设备身份与临时连接密钥分离。
10. 客户端与 Access Node 权限由每一跳校验，并由 Owner Node 最终强制执行。
11. 不永久保存无价值的流式和终端噪声。
12. 网络层、IDE 和具体 Agent 都必须可替换。
13. npm 只负责分发，Rust 二进制负责实际运行。
14. 第一阶段保持无应用云服务器的本地优先设计。
15. 遵守 ACP 兼容性不变量：只增强传输与协调能力，不削弱、曲解或静默丢弃 ACP 原生能力。
16. 设备形态不决定权限；本机自己的设备通过认证即可执行命令（没有设备级授权维度），跨节点能力来自 Export Policy、`grant.*` 与实际端到端 capability 的交集。
17. 节点配对不产生传递信任；第一阶段 imported Agent 不得再次经 Node Link 导出。

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

## 独立目录创建（ADR-0010）

文件夹选择器支持立即在主机创建一层目录，用户可逐层新建后单独选择；取消会话不删除已建目录。产品行为与兼容取舍见 [ADR-0010](adr/0010-immediate-workspace-directory-creation.md)，wire 权威见 SYNC_PROTOCOL §11.5。
