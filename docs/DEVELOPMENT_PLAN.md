# ACP Remote 开发计划

> 状态：实施路线图；记录建议的交付顺序与验收节点，不新增产品、协议或安全合同。
> 基线：2026-09-23（2026-09-24 随 `identity-auth-and-keystore` 补充切片 3 的落地状态，2026-09-25 随 `daemon-cli-and-local-admin` 补充切片 4 的落地状态，2026-09-26 随 `node-link-owner` 补充切片 5 的落地状态）。实际能力以代码和测试为准；各项待实现内容以其权威文档为准。

## 1. 目标与边界

首个产品闭环是在 Windows x64 上运行 Owner 与 Access 两个节点，让 Zed 经 Access Node 使用 Owner 导出的 Agent，完成远程会话创建、对话、事件接收和取消。随后交付由 Daemon 本地托管的 Web/PWA，供已配对设备查看和继续已有会话。Linux 产品支持、原生移动客户端和 npm 分发按[初始设计 §14–§15](INITIAL_DESIGN.md)后续推进。

本计划只编排工作。产品行为以[初始设计](INITIAL_DESIGN.md)、模块边界以[模块架构](MODULE_ARCHITECTURE.md)、端口与持久化以[核心与存储合同](CORE_PORTS_AND_STORAGE.md)、认证以[身份与认证合同](IDENTITY_AND_AUTH_CONTRACT.md)及[安全设计](SECURITY_DESIGN.md)为准；各 wire 细节仍由相应协议文档、schema 和 fixture 决定。实施中若需要改变这些边界，先按 `AGENTS.md` 的规则处理设计确认与合同同步。

## 2. 当前基线

Rust workspace 已包含 `acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`node-link-protocol`，以及切片 2 在 `acp-boundary-and-agent-host` 变更中落地的 `acp-protocol` 与 `agent-host`。其中 core 的会话用例和 broker、SQLite 的 owned/imported 会话存储、Sync/Node Link v1 wire 类型，以及 `acp-protocol` 的 ACP v1 wire 与 `agent-host` 的本机 Agent backend 已有实现。管理状态持久化的 core 端口与 SQLite 存储两层已落地。切片 3 在 `identity-auth-and-keystore` 变更中落地了 `identity-auth`（纯状态机：配对、逐连接 challenge-response、授权展开、12 个 transcript domain）与 `identity-keystore`（Windows DPAPI 包裹 + 非 Windows 失败关闭）。切片 4 在 `daemon-cli-and-local-admin` 变更中落地了 `app`（组合根、Daemon 生命周期与单实例锁、全部 CLI 子命令）与 `server` 的本地通道（`server::transport::local` + `server::local_admin`）。切片 5 在 `node-link-owner` 变更中落地了 `server::transport::net`（默认 loopback 的共享 HTTP/WSS listener、TLS `proxy`/`direct`、`Host` 边界，以及 `/sync/v1`、`/node-link/v1`、两类 `/pairing/*` 的 path 路由）与 `server::node_link`（Owner 侧配对 HTTP、节点握手、Export catalog、resource、command 与撤销传播），并由 `app` 组合根接线；该切片的四条验收（只有已授权 Access 能访问导出的资源、先持久化再发布、相同 `requestId` 不重复派发、撤销后旧连接不能再取资源或发命令）已由受控路径的端到端用例覆盖，原始输出在该变更的 `reports/wp7-integration.log` 与 `reports/pv5-windows-nodelink.log`。`server::sync`、`server::acp_facade` 与 `node-link-client` 仍待后续切片，前端工程尚未开始。详细范围以 [README 的仓库当前状态](../README.md#仓库当前状态)为准；切片 2 的验收与合入状态以该变更的记录为准，不以本文的列举为完成声明。

计划中的每一项都是待交付能力，不因为合同门禁通过就视为已经实现。特别是[核心与存储合同 §11](CORE_PORTS_AND_STORAGE.md#11-管理状态持久化合同形状已并入-357)的形状（值对象、写集端口、管理表 DDL 与 v1 → v2 迁移）已并入该合同 §3/§5/§7，`storage-sqlite` 的管理 store 落盘实现与切片 4 的 Daemon/CLI 接线也已落地；本切片剩余的是端到端验收。

## 3. 实施切片

切片按依赖关系排序。每项可以拆成多个小 PR，但在宣称该切片完成前，必须满足相应的闭环验收。共享合同已有实现只补缺口，不重复建设。

### 1. 管理状态与配置持久化

将[核心与存储合同](CORE_PORTS_AND_STORAGE.md#11-管理状态持久化合同形状已并入-357)的管理状态形状落入 core 端口与 `storage-sqlite`：身份与信任记录、配对、Export/Import、审计、本地配置和 Agent profile。端口签名、写集 DTO 与 DDL 已并入该合同 §5/§7，`storage-sqlite` 侧的 store 落盘实现已落地；切片 4 已完成 Daemon/CLI 接线与本地管理通道（`server` 的本地适配器与 `app`），本切片剩余的是**端到端验收**（切片 5 已落地其中的 Owner 侧 Node Link；`server::sync`/`server::acp_facade` 与 `node-link-client` 属后续切片）。

验收：旧数据库升级、事务原子性、重启恢复、撤销及损坏记录失败关闭均有测试；imported 表和端口仍不能写入远程会话正文。

### 2. ACP 边界与本地 Agent

实现 `acp-protocol` 的 wire DTO、原始消息保真和能力映射，再由 `agent-host` 接管本地 ACP 子进程、stdio JSON-RPC、会话映射、超时、取消与退出处理。Agent profile 从本地配置端口读取；凭据按[模块架构 §4.5](MODULE_ARCHITECTURE.md#45-agent-host)的边界解析和注入。

验收：fake ACP Agent 能驱动一个完整 turn；未知字段与扩展 payload 往返保真，结构化事件不被文本化；启动失败、乱序响应、异常退出和取消有明确结果。Windows Job Object 的父子孙进程清理及关闭句柄清理成为常驻回归测试。实现或声明的能力与[ACP 兼容性矩阵](ACP_COMPATIBILITY_MATRIX.md)一致。

### 3. 身份、配对与授权

实现纯状态机 `identity-auth`、平台适配 `identity-keystore`，覆盖 Node/Device 长期身份、一次性配对、每条新 WSS 连接的 challenge-response、scope 展开、撤销和 nonce 重放防护。密钥职责与端口签名以[身份与认证合同](IDENTITY_AND_AUTH_CONTRACT.md)为准。

验收：现有 transcript 固定向量、P1363 签名、HMAC、SAS 及畸形输入成为常驻测试；节点和设备身份不可混用；错误权限、重复 claim、过期及密钥变化均失败关闭；私钥不进入普通 SQLite 字段、日志或协议错误。

### 4. Daemon、CLI 与本地管理通道

实现 `app` 组合根、Daemon 单实例与关闭顺序、平台本地 IPC、`server::local_admin` 和 CLI 管理命令。`acp-stdio` 只做 stdio 与 Daemon 本地通道的流转接；管理方法及 CLI 映射遵循[本地管理协议](LOCAL_ADMIN_PROTOCOL.md)。

验收：在本机可启动和停止 Daemon，配置 Agent profile，完成节点配对和 Export/Import 管理；Daemon 未运行时 `acp-stdio` 明确退出；本地通道的访问控制、framing 和两类载荷均有测试。

### 5. Owner 侧 Node Link

实现 `server::node_link` 的节点认证、Export 过滤、catalog、resource、command、ACK 和重放。先用单 Owner、单 Access、单 Export 跑通受控路径，再覆盖撤销、连接 generation 和慢连接。[Node Link 协议](NODE_LINK_PROTOCOL.md)定义 wire 和顺序语义。

验收：只有已授权 Access 能访问导出的资源；Owner 先持久化事件和命令终态再发布；相同 `requestId` 的重试不会重复派发；撤销后旧连接不能继续取资源或发命令。

### 6. Access 侧远程 backend 与 Zed 闭环

实现 `node-link-client` 的 Import、catalog、attachment、origin cursor、无正文交付索引与显式重连；再实现 `server::acp_facade`，让 Zed 经 `acp-stdio` 使用本地或导入的 Agent。远程 `session/new` 必须映射到受 Export grant 和 workspace template 约束的 `session.create`，不允许任意 Owner 路径或凭据输入。

验收：两台 Windows 节点上，Zed 完成 `initialize`、`session/new`、`session/prompt`、`session/update` 和 `cancel`；事件断线后按 cursor 补发且去重；Access 先提交无正文交付收据再向本地发布。Owner 离线时正文不可用，Access 不自动重试结果不确定的副作用命令；imported Agent 不可再次导出。

### 7. Sync Server 与本地 Web/PWA

在 Node Link 闭环后实现 `server::sync` 的设备认证、snapshot、事件、ACK、补发和命令终态；建立 TypeScript + Expo/React Native 通用工程，首个交付只构建 Daemon 本地托管的 Web/PWA。客户端状态、平台 adapter 与功能范围以[前端设计](FRONTEND_DESIGN.md)为准。

验收：PWA 能配对、查看和继续已有会话，展示流式及关键结构化事件、处理权限请求、取消 turn、查看与切换模型；重连从已 ACK cursor 恢复且不重复显示或执行命令。首版不提供会话创建入口；Owner 离线时只显示可用元数据，不把输入误报为已发送。

### 8. 稳定性、兼容性与发布

围绕已经跑通的产品路径补齐崩溃恢复、慢客户端、休眠唤醒、容量与 TTL、迁移、备份恢复、日志脱敏及真实 Codex/OMP 兼容报告。按[初始设计 §15](INITIAL_DESIGN.md#15-mvp-实施顺序)逐步处理多 endpoint 和可替换网络部署；最后完成 Windows 构建、安装、升级与回滚验收，再推进 npm 平台包。

验收：Windows 产品路径有可重复的端到端验收记录；ACP 能力声明与真实 Agent 测试结果一致；发布构建和进程生命周期满足[安全设计](SECURITY_DESIGN.md)；Linux 共享代码 CI 不被误写为 Linux 产品已交付。

## 4. 切片交付规则

- 每个实施切片按仓库的 OpenSpec agentic 流程建立变更，明确目标、任务与证据；优先提交小而完整、可审查的纵向 PR。
- 修改 wire、封闭词表、端口、存储或能力支持状态时，在同一变更中维护对应权威文档、schema、fixture、兼容性矩阵和合同门禁。crate 依赖遵循[模块架构 §5](MODULE_ARCHITECTURE.md#5-依赖矩阵)。
- 本地运行 `npm run verify`；PR 以仓库规定的 CI 检查验收。真实 Codex/OMP 属兼容性套件，不替代普通 CI 中的 fake ACP Agent 测试。
- 某切片的必要端到端路径出现后，应以真实可运行路径验收；此前的替代验证和 agentic E2E 判定按该变更的计划与仓库流程记录，不把合同检查通过等同于产品闭环。

**建议的第一项实施变更**：管理状态与配置持久化。它是身份、CLI、Agent profile 和 Node Link 的共同前置依赖；合同形状已并入 §3/§5/§7，core 端口与 SQLite 落盘两层已实现，Daemon/CLI 接线也已随切片 4 落地，剩余工作是端到端验收。
