# Design

## Context

现有实现已包括 node-link-client、server::sync 和 PWA，不能采用 config.yaml 中“尚未实现”的旧画像。RemoteDeliveryStore 当前同时持有 imported 会话索引、收据、ACK 与请求引用，core 清单从它读取；node-link-client.delivery 在收据与 ACK 落盘后发送 resource.ack。ReadView 的本地重放与 Owner origin cursor 是不同序号域。Owner 的 owned_command 独立保存幂等记录。

用户确认和取舍见 [ADR-0013](../../../docs/adr/0013-stateless-access-owner-data.md)；本文与 wire 字段均为待实施设计，不宣称现有代码已经支持。

## Goals / Non-Goals

只保留 Access 连接与信任资料；在线回源、请求防重复执行和事件有序恢复仍有明确合同。保留 owned 会话的所有持久保证。无离线读取、无进度落盘、无命令离线队列、无跨节点正文复制，不创建新的通用 runtime。

## Decisions

### D1 — 严格区分连接资料与资源数据

允许持久化：对端 nodeId、公钥/指纹、连接地址/规范 origin、TLS 与协议配置、由本机用户作出的信任/撤销决定、连接重试配置及配对资料的原有安全存储。对端节点标识属于连接资料；对端 Agent、会话、目录、事件及请求标识不属于它。

禁止持久化：远程 catalog/会话摘要/标题/状态/版本/路径、附件/正文、attachment generation、Owner 游标、收据、ACK、local sequence 映射、远程 requestId/命令状态/结果引用及包含这些值的审计与诊断。节点握手和信任变更的本地安全审计仅含连接身份及固定结果类别，可以保留；运行期的资源诊断不得被日志订阅器写盘。身份私钥仍经现有 keystore。节点自有 owned 家族按资源归属保留，不按“此进程是 Access”一刀切。

采用封闭表/列白名单和写路径测试，而非仅靠字段命名。反例方案“删正文但保留摘要/进度”已被用户拒绝。

### D2 — 以 core 端口隔离运行期目录和远程读取

移除 imported 资源对 SQLite RemoteDeliveryStore 的依赖；其资源写入、local_replay、ack/load_ack 和 request 查找不保留伪存储实现。core 定义纯类型的运行期远程目录/事件来源/查询端口，node-link-client 提供有界内存实现，app 装配，server 只调用 core 用例。SessionBackendFactory 和 RemoteSessionReads 继续承载命令及回源读取；来源检查由受认证连接的当前目录提供，不可因删 imported 表而回退访问本机文件。

目录只在已认证连接期有效；断开、撤销、Owner epoch 改变时整代失效，取消 attachment/读写任务并清除结果。客户端仍持有来源游标可用于恢复，但不得继续渲染断开前业务资料。不同 Owner 的 generation 与任务所有者隔离。运行期资源上限沿用协议已有目录、帧、队列及超时限额，不新增无界缓存。

### D3 — 本地与远程同步分域，不创建临时假日志

拒绝给 imported 事件分配 Access 本地 globalSequence：即使放内存，重启后也不能兑现既有持久游标语义。既有 sync.subscribe/event/sync.ack 保持只针对 owned 日志，persistent serverEpoch 不改变。新增协商 feature `resource.stateless-origin.v1`（同时用于 Sync 与 Node Link），Sync 远程流以 `(ownerNodeId, originSessionId)` 单会话订阅，游标直接使用 `{ originEpoch, originSequence }`。

新消息使用现有协议信封及逐连接序号；以下 body 都是 closed object，不另造业务命令同义词：

| 消息 | 方向 | body 与语义 |
| --- | --- | --- |
| sync.remote_subscribe | 客户端→Access | subscriptionId(UUID)、sessionRef{ownerNodeId,sessionId}、cursor(OriginCursor 或 null)；绑定设备可访问的当前来源 |
| sync.remote_event | Access→客户端 | subscriptionId、event（保留 resource.event 的 originEventId、originEpoch、originSequence、sessionRef、eventType、payloadDigest、createdAt、payload）；不带 Access globalSequence |
| sync.remote_caught_up | Access→客户端 | subscriptionId、cursor；Owner 已完成来源重放屏障，之后才发实时事件 |
| sync.remote_ack | 客户端→Access | subscriptionId、cursor；仅本次连接连续处理进度，不落盘 |
| sync.remote_reset_required | Access→客户端 | subscriptionId、reason（沿用 epoch_mismatch/cursor_expired/cache_incompatible）、cursor；仅 Owner 已验证的有效 head 可给 cursor，否则 null |
| sync.remote_unsubscribe | 客户端→Access | subscriptionId；取消本连接订阅及所属观察者 |
| sync.remote_unavailable | Access→客户端 | subscriptionId、已有结构化错误；断线、撤销、不支持分别用现有错误码 |
| sync.remote_catalog_changed | Access→客户端 | ownerNodeId、revision(decimal string)；提示该来源清单失效，客户端重新查询，不携带旧资源副本 |
| sync.remote_source_state | Access→客户端 | ownerNodeId、state(online/offline/revoked)；来源连接事实，不写入本地 owned 事件流 |

OriginCursor 在各协议 crate 各自定义并按边界映射，两个协议 crate 不互相依赖。event 原始 ACP 文本逐字转发，不能通过通用数字 DTO 重编码。eventId 使用 Owner 的 originEventId，不创建替代 ID；客户端去重键绑定来源节点、来源会话、epoch 和事件标识。所有序号均为十进制字符串。新订阅和退订归入现有 observe 速率族，单连接最多 32 个远程订阅，超限 resource.rate_limited；单帧、发送队列和重放批次使用现有协议预算，客户端重建缓冲计入同一有界预算，超限明确 backpressure/重建而不落盘扩容。新常量在 WP1 同步两协议限额表和 fixture，不新增配置开关。

### D4 — 订阅、重放与读取的屏障

Owner resource.subscribe 的非空游标走既有来源日志增量重放；增加 feature 门控的 resource.caught_up（attachmentId、attachmentGeneration、cursor）作为明确来源屏障，不用定时器推测追平。null 订阅先取得 Owner 当前元数据 snapshot 的 barrier B；Access 验证完成后向客户端发送 remote_reset_required(cache_incompatible, cursor=B)，客户端回源 session.read 并重建明细，在客户端有界缓冲 B 后事件。此处初始重建属于未持有可兼容远程状态，不伪造持久化 cursor。

选用每页独立的原子读取边界，不引入跨页快照 token：同一 session.read 返回的 messages/history 在一次 Owner 只读事务中捕获来源位置 R，新协商结果增加 readCursor=R、该读取时刻的完整 sessionSummary，以及 messageCoverage 数组，每个已返回消息恰有 `{messageId, through: OriginCursor, lastDeltaIndex: decimalString|null}`；through 等于 R，lastDeltaIndex 为该消息在所返回完整内容中已覆盖的最后连续增量索引，无增量时为 null。数据和覆盖信息必须同次原子读取，不能读取结束后再采样 head。这是待新增合同，不是现有客户端本地读取保护可以证明的事实。

客户端以 `(来源, messageId)` 记录当前已应用覆盖位置，只有较新或相同覆盖页可替换消息；页面内容覆盖 through 及之前针对该消息的创建/增量事件，对应缓冲项不再拼接，through 之后按来源次序与 deltaIndex 应用。未返回的消息不受 R 覆盖，不能丢弃其缓冲事件；history 按来源稳定 eventId 去重。每个读取结果独立合并，不声明整个分页浏览是同一历史快照；下一页有自己的 R2，迟到旧页不能覆盖更新内容。messageCoverage 和 readCursor 只在该 feature 协商后出现，messages/history 原有页边界与 hasEarlier/historyHasEarlier 不改变。缓冲溢出则明确取消并重新构建，不能静默丢事件。原始 ACP 文档不参与覆盖推断、不被解析补造消息。

重连时无论客户端是否仍持有最后处理位置 C，只要断线策略已清除来源业务模型，就必须重新回源 session.read，并按每页覆盖信息重建，不能只重放 C 之后的事件。仍持有 C 时将它交给 Owner 请求来源重放并缓冲，同时读取最新权威页；有效 C 只是衔接与去重位置，不能证明页面仍拥有此前内容。没有 C 则走 null 订阅/B 后读取的首次重建路径。epoch 不匹配/过期先明确 reset，再走首次重建；Owner 不可达则明确 unavailable，不展示已清除内容。只有重建读取成功且来源屏障已追平后才能呈现可用视图。owned 事件的 snapshotDigest、cursor 校验与 ACK 保留原合同。

### D5 — ACK、命令和多客户端

新 feature 下 resource.ack 只用于当前 attachment 的流控，不再表示可恢复索引；Owner 验证所属会话、epoch、单调性与已发 head，不据此提前清理保留期内数据。Access 不保存 ACK；多个客户端订阅各自持有当前运行期进度，慢客户端超限时取消该订阅，不能阻塞 Owner。Node Link ACK 可按已验证并交给有界下游的进度发送，不作为任何客户端已阅读或持久恢复承诺。

命令沿用原 requestId，Owner 保存接受/终态及参数冲突判定。Access 只保留在线观察者，command.status 直接向原 Owner 查询；远程查询 payload 在该 feature 下增加必需 remoteTarget `{ ownerNodeId, sessionId: UUID|null }`，sessionId 为 null 仅用于尚未建成会话的 session.create，其他会话操作必须给原来源会话。Owner 以既有 `(ownerNodeId, accessNodeId, requestId)` 隔离和参数冲突合同校验，不能以 Access 本地记录判断。不同设备须继续生成全局随机稳定 requestId；若碰撞则按既有参数冲突规则拒绝，不静默换号。当前协议没有 localPrincipalRef，本变更不添加它、不修改 Owner 对最终用户身份的信任模型。所有副作用重试保持原标识，Access 不主动换号补发。

### D6 — 清单、可用性和兼容

Owner session.list 在线返回完整摘要，Access 运行期与 owned 清单组合；Node Link catalog 保持发现用途，不假装目前无 title 的 catalog 已能提供完整标题。Owner 的会话摘要版本变化（包括 Agent 标题、状态，后续包括自定义标题）必须推进 catalog revision 并通知已订阅连接；Access 合并突发更新为运行期 remote_catalog_changed，客户端重新发 session.list 替换该来源条目，旧查询结果以连接 generation/revision 拒绝。不能只订阅当前会话而让其他客户端列表永久显示旧标题。读取会话时用原子结果 sessionSummary 及 version 防止清单与来源屏障之间竞态回滚标题。超出限制遵循 existing result_too_large，不静默截断。来源状态通过 remote_source_state 及远程条目 origin.online 表达；Access 不将通知写 owned_event，只在连接期发送。

新 Sync/Node Link feature 与原 resource.remote-origin.v1、core.event-origin.v1、历史能力交集协商。未具备新 feature 的远程通道明确不支持，不通过旧落盘实现兼容；本地功能保持。旧客户端依旧可读 owned title；远程资源只有在端到端能力实际满足时开放，不虚报兼容。接收旧 imported 本地游标时返回既有 cache_incompatible reset，不翻译或钳制。

### D7 — 权威文档落地清单

实施时一起更新 AGENTS §3/§4/§5/§6/§7 的 Access 存储与恢复条款，INITIAL_DESIGN、MODULE_ARCHITECTURE、CORE_PORTS_AND_STORAGE、SYNC_PROTOCOL、NODE_LINK_PROTOCOL、SECURITY_DESIGN、FRONTEND_DESIGN 及相关 ADR；保留 Owner 先提交与命令去重不变量。新增消息/feature/结果字段及幂等键变化同步 compatibility、schemas、fixtures、Rust/TS codec 与共享 manifest；如触及签名字段，同批维护 transcript 登记及固定向量。配置没有新增开关，不提供偷偷恢复旧缓存的模式。

## Risks / Trade-offs

- 无 Access 本地恢复 → Owner 不可用时不能控制或读历史，客户端只可重建可保留数据，不承诺恢复丢失的请求进度。
- 重建与实时竞态 → 来源屏障和读取覆盖信息必须原子取得；测试消息半截读取、事件重放、重复 delta 和来源重置。
- 不同 Owner 的重复 UUID → 来源命名空间是 UI/运行期主键的一部分，sessionId 线上若冲突必须明确拒绝该资源且说明冲突，不静默合并。
- 一端尚未升级 → 关闭远程能力，不落盘兼容；先验证 Owner 支持新 feature，再启用新 Access。

## Migration Plan

1. 停止 Daemon，在单实例锁内迁移；基于实际最新 schema 追加版本，不复用旧 v13/v5 常量。明确删除 imported_session、imported_delivery_index、imported_command_ref 及来源恢复附属数据；旧版本同义表经实际 migration 链识别。连接身份与本地自有表保留，外键按依赖顺序处理。
2. 清查 owned_command、审计、缓存及历史日志是否含 imported 代理记录；按明确的资源来源谓词清理，不以 command 名称或时间猜测，不能连带删除 owned 资源事件。持久源无法可靠辨认时迁移失败并给本机诊断，不保留来源不明的数据冒充满足禁存。
3. 事务完成后清理可读的 SQLite WAL/回滚日志和自由页副本（停服 checkpoint/truncate + 数据库重写/既有安全清理机制），成功才开始监听；清理中断重开继续完成启动门禁。不承诺从硬盘或历史备份做取证级擦除；不自动改用户外部备份。
4. 上线后跨节点读写、重放和重启，检查数据库、临时目录、日志与浏览器持久存储无远程测试哨兵及动态资源标识。owned epoch/事件/幂等、节点身份与信任逐项不变。
5. 回滚不允许旧二进制打开新库，禁止恢复含已禁远程数据的旧备份直接运行。保留 owned/连接资料的脱敏导出用于人工恢复；迁移前完整备份必须标记旧敏感资料并离线保管，不能被新运行时加载。

## Open Questions

无产品语义阻塞项。具体 schema 版本号以实施基线为准；所有新增字段和错误码复用/登记需由 WP1 合同先行验证，不允许 coder 临时发明第二套协议。
