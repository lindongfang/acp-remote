# Design

## Context

现有 ConversationStream 顶部为普通 Text；client-store.titleOf 对空值显示未命名。agent-host.mapper 只消费 session_info_update，core.derive.title_intent 区分缺席与显式 null，不存在 ACP Remote 自动命名。当前规范禁止客户端改名，需随本变更替换。完整审视决定见 [interview.md](interview.md)。

前置 [access-owner-data-stateless](../archive/2026-10-21-access-owner-data-stateless/proposal.md) 必须已实现并通过门禁；本设计使用其在线目录、Owner 查询和独立远程事件通道，不恢复 imported 索引/收据。后续 [scoped-offline-page](../archive/2026-10-21-scoped-offline-page/proposal.md) 独立提供离线状态与连接问题弹窗，本变更只保留明确的不可用状态。

## Goals / Non-Goals

实现顶部双击原地编辑及共享最高优先级标题。仅 Owner 持久保存标题，不增加账号、设备权限、Agent 端重命名或自动命名，不提供恢复/清空和额外编辑入口，不实现跨页草稿或离线队列。

## Decisions

### D1 — 分开保存来源，统一下发有效标题

Owner 保留 owned_session.title 为 Agent 标题，追加 nullable custom_title；有效值为 custom_title 优先，否则 title。对外沿用 SessionSummary.title 下发有效值，不新增 customTitle wire 字段，避免 closed schema 旧客户端拒绝。旧客户端在可用资源通道上自然显示自定义值；远程通道兼容性以已实施前置变更的能力要求为准，不承诺旧版 Access 的存储兼容。

Agent 通知仍只更新 Agent 列，显式清空不会清除 custom_title。原 ACP 与 session.info.changed 的 Agent 投影保持原意；客户端头部、列表、搜索统一由权威 SessionSummary/session.updated 的有效 title 驱动，不从 Agent 通知直接覆盖。会话摘要是标题、状态等概况记录，不是正文总结。

### D2 — Owner 元数据命令

新增 mutation `session.title.set`，payload 是 closed object `{ title: string }`，输入（trim 前）为 1–512 个 Unicode 标量值，保存值是 trim 后的文本；trim 后为空则拒绝，超长按输入计、不截断，不接受 null。feature 固定为 `core.session-title.v1`，在 Sync 与 Node Link 共享登记。Node Link 按当前“已配对且未撤销的节点可执行业务命令”接线，不重新引入已删除的 Export、grant 或设备权限模型；长度、频率、实际能力、local.* 禁止远程和撤销边界保持。

该命令不依赖 Agent 提供重命名能力，不启动 Agent、不调用 SessionEndpoint，不占用 active turn，可在 idle/running/pending 任一状态执行。expectedVersion 为 null，采用 Owner 提交顺序后写生效；不能偷偷增加版本冲突确认。命令接受沿用 requestId，Owner 的状态写入、版本推进、完整 session.updated 和 command terminal 在一次事务完成，Owner 先提交再广播。

尚无 custom_title 时，即使保存文字等于 Agent title，也必须建立覆盖值；已有相同覆盖值时不重复变更业务状态，命令终态仍可明确完成。Agent 更新与用户保存并发只写各自列，由 Owner 事务产生一致有效摘要。

### D3 — 纯在线 Access 与查询

Access 仅使用前置运行期来源目录将命令发往 Owner，不保存标题/命令状态/收据，不把代理命令写 owned_command。command.status 使用前置定义的来源定位与原 targetRequestId 直接回源，Owner 负责去重和归属校验。未知结果只查询原请求，不自动换号重发。

当前页面运行期间可内存保留原请求标识，页面刷新后只读取当前标题，不恢复上次保存进度。读取成功仅说明当前值，不可据此伪造已丢失请求的完成状态。Owner 离线、撤销、不支持用实际错误明确区分。

### D4 — 原地编辑状态机

状态为 viewing、editing、submitting、editing-error，按 sessionId 绑定；仅顶部双击进入，不加编辑按钮。输入框保持为空，当前有效标题只作为灰色占位（未命名显示未命名占位），不预填为真实输入；不输入直接确认时保留旧标题、不派发命令，只有输入非空文本才提交。Enter 与点击外部保存，Escape 取消；输入法 composition 期间 Enter 不保存。Esc 触发的 blur 被抑制，Enter/blur 共用一次提交意图及 requestId，不能产生两次保存。

输入非法时不发命令，保留输入框并显示原因；同值错误的重复 blur 不无限重试。提交中不再产生第二个保存，不在 accepted 时显示完成。收到 Owner 终态并取得对应权威摘要后退出编辑；其他设备更新不覆盖编辑中的文字。已发出的旧请求结果按请求/会话定位，迟到不覆盖新会话或较新提交。

点击另一会话时先为原会话尝试提交，再正常导航，不等待网络结果锁住页面。仍在当前页的失败保留输入并显示错误，已离页则放弃输入，只显示可归因原会话的错误提示；不保存跨页草稿、不强制跳回。离线禁止新提交，不能因 blur 产生离线队列。放弃输入不等于取消已抵达 Owner 的修改。

### D5 — 合同与文档同步

实施更新 INITIAL_DESIGN、FRONTEND_DESIGN、SYNC_PROTOCOL、NODE_LINK_PROTOCOL、CORE_PORTS_AND_STORAGE、MODULE_ARCHITECTURE、ACP_COMPATIBILITY_MATRIX，替换客户端禁止改名条款；同步命令/feature 登记、schema、Rust/TS fixture、golden 列清单与合同断言。不新增 ACP 方法，不把 Broker 元数据能力冒充 Agent capability。具体 module 组合仍遵守内层端口，不让 app 承载优先级规则。

## Risks / Trade-offs

- 只给双击入口 → 可发现性和触屏使用有限，这是用户明确要求，不擅自加按钮。
- 不提供恢复 → 自定义后只能再次改名，Agent 原值仍保真，但不暴露恢复接口。
- 点击外部提交与离页竞态 → 提交意图绑定原会话、同次触发复用 requestId，迟到结果不得改当前输入。
- Agent 自动命名不确定 → 无真实 Agent 证据时不宣称保证，只验证 fake Agent 通知投影和标题优先级。

## Migration Plan

先合入并验证 Access 改造及新恢复协议，再对实际最新 owned schema 追加 custom_title 可空列，旧行初始化 NULL；原 Agent 标题、owned 事件/epoch/幂等记录不变。旧 owned 库升级、新建库、重复打开列清单一致，事务失败整体回滚。部署 Owner 后开放编辑 feature，再更新 PWA；新旧客户端的 title 形状不变。回滚不允许旧二进制写升级库，不通过恢复旧 Access 库重新引入远程资料。无需为 Access 建 custom_title 列。

## Open Questions

无产品语义阻塞项。最终 migration 版本号读取实施时基线；协议 feature 和命令名称以本设计为候选，WP1 登记后成为唯一机器定义，不在代码复制词表。
