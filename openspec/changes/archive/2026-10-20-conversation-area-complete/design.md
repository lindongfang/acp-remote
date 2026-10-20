# Design

## Context

动机与范围见 proposal.md，逐项需求及修订见 `docs/conversation-area-discussion.md`。本文件描述拟实施方案，不表示当前能力已实现。

已核实的基础和缺口：

| 位置 | 当前事实 | 本次处理 |
|---|---|---|
| `clients/app/src/domain/timeline.ts` | 实时与结构化历史统一，工具/轮次按对象合并 | 扩展轮次组织、思考与媒体、流式恢复 |
| `clients/app/src/components/ConversationTimeline.tsx` | 提问仅显示 schema，明确不收集值 | 真实表单、两端校验、重新获取请求 |
| `clients/app/src/domain/conversation.ts` | 非文本消息折为结构化存在标记 | 类型化媒体/资源视图，保留原始载荷 |
| `crates/app/src/compose.rs` | HostConfig 默认不宣告表单或终端服务 | 完成服务后接线并真实宣告 |
| `crates/agent-host/src/mapper.rs` | 非文本增量有 block，工具详情主要为文本 | 类型化媒体来源与工具详情；不在前端解析 raw |
| `crates/core/src/broker.rs` | 回复有最终正文折叠，思考没有 completed 消息 | 保存思考和中断部分输出 |
| `crates/core/src/ports.rs` | AttachmentStore 的 put/get 接受完整字节/Vec | 增加运行时无关的分块接口，保留迁移期兼容 |
| `README.md` 当前状态 | Node Link 只有 Owner 入站，Access 出站尚未实现 | 本次明确包含既定 node-link-client 边界 |

现有权威约定排除 PWA 非文本输入、Node Link history 和历史请求应答；本次在协商新能力后扩展，旧路径不静默改变。`openspec/config.yaml` 画像仍称前端及 Sync 未落地，规划依据实际代码和 README，不据此误判为绿地项目；画像更新纳入 WP1。

## Goals / Non-Goals

**Goals:** 保持 ACP 原始语义与未知字段保真，Owner 单一正文权威；提供有界传输、可恢复输出与真实交互。工作包按合同、核心、适配器、客户端顺序交付，不以 UI 完成为整体完成。

**Non-Goals:** 见 proposal.md。浏览器不能保证关闭页提醒一定弹出或后台可靠上传；不引入浏览器持久草稿、云存储、永久 bearer 下载地址或通用插件运行器。

## Decisions

### D1. 领域与依赖保持既定边界

沿用 core 的 AgentCatalog、SessionBackendFactory、会话 SessionEndpoint 和现有端口；新增 node-link-client 作为出站适配器，由 app 装配。server::sync 与 server::node_link 只调用用例，不互调；storage 不依赖 Agent 或网络。附件文件后端复用 storage-sqlite 已有附件目录与内容索引归属，不额外引入万能资源 crate。core 不加入 Tokio、wire DTO 或新普通依赖。

替代方案“Sync 直接调用 Node Link 入站实现”会破坏依赖方向，也不具备出站重连所有权，因此不采用。

### D2. 用协商扩展承载新协议，不放宽旧 schema

拟登记附件传输、可恢复交互、媒体消息及远程历史 feature，名称和词表在 WP1 一次落机器登记，之后 Rust/TS 同源消费。新客户端与旧服务端保留文本能力，新功能显式不可用；未协商连接不出现新封闭字段或消息族。Node Link 复用同批业务命令名称，不另造同义名。

附件采用独立的已认证 WSS 传输连接，仍使用对应节点现有 challenge-response、Origin/Host、撤销与心跳规则，在握手后协商附件帧；不向业务消息连接发送巨量正文。传输帧表达 begin、chunk、seal、status、abort、read 等操作，属于传输状态，不冒充已受理业务 prompt。帧必须有类型化 schema、transferId、会话/主体绑定、偏移和确定的错误语义；它们不是长期 bearer。建立连接或读取媒体不依赖公开下载 URL。认证后新用途消息族的路由在 WP1 明确登记，旧 peer 不接收新帧。

单块原始数据 32 KiB，偏移/总长度用十进制字符串；每传输最多一块未确认，每设备同时 2 个活动传输、节点同时 8 个，其余等待；单连接发送预算和速率沿用协议限流框架独立计数。它们是运行保护，不是附件个数/大小产品额度。附件清单采用分页/分批登记及 seal，prompt 仅携带一份有归属的已封存提交清单引用，避免大量小文件塞爆单帧。清单引用只在 ACP Remote 内使用，不能直接当作 ACP 新内容类型发给 Agent。

替代方案“把任意附件作为 base64 塞进一条命令”受现有 1 MiB 帧约束且造成高内存和事件阻塞，拒绝采用。不为允许大附件而取消 ACP 单帧防护。

### D3. 上传状态和消息执行分离

上传状态为 selected → uploading → sealed，旁路 failed/cancelled/expired；消息命令另用 submitting → accepted → terminal（completed/failed/uncertain）。客户端整批附件完成后提交一次稳定 requestId 的 prompt；服务端在接受前再次检查清单完整性、身份、会话、路由和 Agent 能力，随后在同一业务事务绑定消息、附件引用及幂等记录。上传成功不等于业务命令受理，也不提前向 Agent 派发。

chunk 采用期望偏移，重复同块返回原确认，不同内容的重复位置拒绝；seal 校验长度与流式摘要。数据库事务与文件 rename 不具备跨资源原子性：先落临时文件并封存，再提交引用，缺失时拒绝引用；重启回收孤儿。清理先提交引用删除，再在事务外删除无有效引用文件。临时活动时间由服务端持久记录，最近活动不足 24 小时不清理；清理与 seal/提交有互斥或条件更新防竞态。

草稿仅页面内存。连接中断后允许本页面查询上传状态并重试确定的块；刷新后不恢复上传，24 小时清理失联临时内容。发生 uncertain 的 prompt 不自动重新提交新请求。

### D4. ACP 内容映射服从能力和实际消息预算

上传内容与工作目录路径引用分开。后者服务端只验证当前工作目录边界和链接逃逸，向 Agent 传受支持的路径引用，不复制、不固定版本或监控文件；历史只保留引用。路径格式在 ACP 出站边界转换，客户端不得构造 Owner 任意路径。

图片/音频在 Agent 支持对应 content block 且最终编码符合 ACP 消息预算时按原生类型提供；通用文件在其声明支持的资源/嵌入内容能力内映射。选择前可依据文件类型、长度与已协商能力预估，接受前做最终编码预算校验。不支持的格式或过大的原生块明确拒绝整条消息；不自动转文本、不擅自把上传附件改成 Agent 未宣告可读的路径。不设上传产品额度，不等于 Agent 没有输入约束。

Agent 输出的媒体和资源经 agent-host 类型化投影进入 core，再由 Sync 映射为媒体引用/可用性；完整 ACP raw 保持原文。嵌入字节可抽取为 Owner 附件；仅有外部 URI 的内容不自动下载，显示资源链接与未获取状态。客户端只消费专用视图，诊断 raw 入口不成为媒体解析数据源。

### D5. 正文折叠和增量重放

消息稳定标识包含 role/channel，回复与思考分别保存，Agent 未提供思考时不生成。每个带 sequence 增量先入 SessionStore；终态事务先写完整/部分正文、媒体引用和真实完成状态，再允许压缩短期增量。失败、停止与崩溃恢复都覆盖部分内容；无法确认完成时不伪造成功。消息正文只从 messages 读取，不塞 history 或清单 snapshot。

工具按 toolCallId、终端按 terminalId + chunkIndex 原位更新，终端输出不得直接套用每次“完整输出快照”重复追加。实时与重放按 eventId 去重，读取完整内容按版本/序号屏障替换，忽略已经被权威正文覆盖的早期增量；新到达的后续增量不被旧读取结果覆盖。Access 使用 RemoteDeliveryStore 收据和来源映射，不能走 owned 正文事务。保留期外显式 unavailable，不制造空正文。

前端允许按渲染帧合并多次视图刷新，但每个协议增量都先进入状态且不能丢弃。Markdown 暂缺闭合符号时按可读文本或未完成代码块显示，使用稳定节点键与阅读锚点，避免整个时间线反复卸载。长输出按块/可见范围渲染，不把完整会话重复解析于每次增量。

### D6. 有效待处理请求与表单

通过 session.read 的 pending_interactions 取得当前有效完整请求及版本；历史只提供定位信息。重新获取后仍须在 resolve 执行前校验身份、会话、请求版本、有效性及 Agent 连接存活。不能复活 Daemon 重启后已经丢失上游等待方的交互。首个有效答案在原子条件更新中胜出，失败校验不消费请求；重复同 requestId 幂等，不同设备的后到回答返回已处理事实。

表单仅按当前 ACP 快照支持的 schema 子集构建文本、整数/数值、布尔、枚举与字符串数组选择，支持必填及范围校验。前后端共用测试向量；精确整数不得先经 JS number 再声称保真。未知必需字段或不可表达类型使提交不可用并说明，保留 decline/cancel 等合法动作。submit/decline/cancel 映射既有 ACP accept/decline/cancel，空对象与 null 区别保持，不引入自动批准。

替代方案“任意历史内容直接恢复应答”会产生过期答案和竞争错误，故必须重新取得当前请求。

### D7. ACP 终端真实实现

agent-host 增加终端服务所有者，按 vendored ACP 的 terminal/create、output、wait_for_exit、kill、release 编解码和方法语义实现。kill 保留终端标识可供读取；release 杀死未退出任务并释放调用句柄，已嵌入工具的输出及历史仍保留，不能删掉对话证据。每个任务归属 Agent/会话，相关审批仍来自原 ACP 请求，不凭空追加每命令一次的自动审批策略。

沿用受监督子进程机制与 Windows Job Object，标准输出/错误流以实际可区分程度保留，UTF-8 跨块正确解码。输出遵循 Agent 请求的预算及既有服务输出限额，截断保留标记；等待退出有取消所有者，不以普通短请求超时杀死长任务。停止/自然退出/释放收敛单终态；关闭顺序为停止新请求、取消等待、终止任务树、排空可提交输出、关闭通道。能力只在完整服务接线后宣告；不提供交互 stdin/PTY 页面。

### D8. Access 真实链路纳入本变更

node-link-client 实现身份握手、连接所有权/取消、重连、catalog、import 注册、资源 attach、重放/ACK、远程 SessionEndpoint 与读取/附件端口。app 组合根装配，server 仅使用用例。现有 CLI import.add 和 node pair --mode access 的缺席路径需接线，方法、framing 或映射改变时同步 LOCAL_ADMIN 合同。

Owner 的 origin cursor 与 Access local sequence 映射持久化；重放正文在线回 Owner 取得。所有 imported 正文只在有界内存转发，不进入临时附件目录、SQLite、日志、浏览器缓存或 Service Worker。Owner 不可达时清理不再允许展示的远程正文，草稿仅内存标未提交；恢复后回源读取。撤销立即取消相关路由、读取与上传，不把网络可达当授权，也不允许再次导出 imported 资源。

### D9. 页面交互沿用已有边界

不重做三栏外壳或文件浏览器。消息、卡片、执行结果按真实轮次标识归组；无标识不猜组。工具默认随状态展开/收起，用户操作保留，思考默认收起，计划紧凑、最终回复展开，审批/提问/失败摘要始终可见。草稿在执行中可编辑但 submit 始终禁用；停止待确认时仍禁用。

媒体读取走平台端口和受认证分块连接，Blob URL 只用于呈现并及时撤销；允许安全栅格图片和音频所需 CSP blob 源，不开放外部源或内联脚本。HTML/SVG/可执行文件只下载或以不可信文本诊断，不活动渲染。上传源/草稿/媒体缓冲不落持久缓存。

## Risks / Trade-offs

- 无产品大小额度可能耗尽 Owner 磁盘 → 分块、并发/速率控制、预检可用空间和 ENOSPC 失败、24 小时临时清理；不通过隐含固定产品额度替代需求。
- 附件很大但 ACP 原生块不能容纳 → 上传前预估/提交前精确门控并明确说明，不更改 ACP 语义或静默转格式。
- Access 基础设施尚缺，交付规模较大 → 显式 WP5，不把本地完成或 fake host 通过当远程完成。
- Codex/Oh My Pi 实际能力可能不同 → 按初始化能力测试并记录支持/明确不支持，不伪造“所有 Agent 都能输出全部类型”。
- 时间线内容长、增量多 → 分块呈现与稳定阅读锚点；用实际长输出和双节点断线场景验收。
- 用户关闭页可能丢草稿 → 支持浏览器可用的离开提醒，但不承诺系统强杀恢复。

## Migration Plan

1. WP1 先冻结新能力、消息/错误形状、端口和存储合同；保留旧 feature 下原有文本行为。更新配置画像，实际实现状态仍写计划/待实施。
2. SQLite 以实施时最新版本追加迁移（当前文档节标题和版本注记存在历史差异，不硬编码猜版本）；增加消息通道/部分结果、媒体引用、临时传输、有效交互所需状态，保持 imported 表无正文。补旧库升级、失败回滚、重复启动与孤儿文件测试。
3. 先上线 Owner 存储/Agent 服务及协议，再启用 Access，最后客户端协商新功能；旧 peer 不收到新字段。所有已处理交互保留历史事实，不能因迁移重新待处理。
4. 回退优先关闭新 feature/入口；不自动降级数据库或丢弃新消息。回到不识别新 schema 的二进制须停机后恢复迁移前备份，备份须包含数据库及附件目录；禁止旧二进制直接写新库。
5. 逐包独立检视、串行合入后运行合同与实现门禁，再做真实单节点/双节点浏览器和 Codex/Oh My Pi 兼容验收，证据与未执行项分开。
