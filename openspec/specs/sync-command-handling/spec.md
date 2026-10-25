# sync-command-handling Specification

## Purpose
定义 Sync 的命令面：查询命令的同步完成、mutation 的先接受后终态、按设备与请求标识的幂等判定、逐命令的 scope 授权与零副作用拒绝、乐观并发控制的版本冲突，以及命令级限流与超限关闭。它让浏览器客户端能在不复制服务端规则的前提下安全地驱动本地 Agent。

## Requirements

### Requirement: 查询命令同步完成

查询类命令（`session.list`、`session.read`、`command.status`、`session.mode.list`、`session.config.list`）SHALL 不进入异步队列，直接返回一次 `completed` 或 `rejected`。其结果形状 SHALL 与协议登记的一致：`session.list` 返回会话摘要集合，`session.read` 返回该会话的明细资源与分页标记，`command.status` 返回目标请求的当前记录，模式与配置清单各自返回其状态对象。结果超过单条消息上限时 MUST 返回 `resource.result_too_large`，MUST NOT 截断后伪装成完整结果。

#### Scenario: 查询命令一次返回终态

- **WHEN** 客户端派发一条 `session.list`
- **THEN** 服务端在一条 `command.result` 中返回 `completed` 与其结果，MUST NOT 先返回 `accepted` 再另行推送终态

#### Scenario: 结果超限时显式失败

- **WHEN** 某查询命令的结果超过单条消息上限
- **THEN** 服务端返回 `resource.result_too_large`，MUST NOT 返回被截断的结果让客户端误以为完整

### Requirement: mutation 先接受后终态

mutation 类命令 SHALL 首次提交只返回 `accepted`（此时 `result` 与 `terminalEventId` 均为 `null`）或 `rejected`；`accepted` MUST NOT 表示业务完成。已接受 mutation 的终态 MUST 只通过一个持久化的 `command.completed`、`command.failed` 或 `command.uncertain` 事件表达，且同一请求 MUST 最多产生一个终态事件。领域状态变化与终态事件 SHALL 在同一事务提交，序号上先写领域事件、再写终态事件。客户端 MUST 只在看到终态事件或 `command.status` 的终态结果后才把命令标为完成。

对会创建资源的 mutation（当前为 `session.create`），受理 MUST 只持久化该命令的幂等记录，MUST NOT 因此创建资源行；资源行与其终态 MUST 在同一次提交内落盘；后端失败时 MUST NOT 留下资源行，MUST NOT 停留在只有受理记录、既无终态又无资源的状态。

#### Scenario: 派发后先收到接受

- **WHEN** 客户端派发一条 `session.prompt` 且授权通过
- **THEN** 首次响应为 `accepted`，`result` 与 `terminalEventId` 为空，MUST NOT 在同一条响应里给出业务结果

#### Scenario: 终态经持久化事件表达

- **WHEN** 被接受的 prompt 对应的轮次正常结束
- **THEN** 该请求的终态以一条 `command.completed` 事件下发，`terminalEventId` 指向它，且该事件与领域状态变化在同一事务内提交

#### Scenario: 崩溃窗口进入结果不确定

- **WHEN** Daemon 恢复时发现某个已接受的 mutation 可能已到达外部 Agent，但无法证明其结果
- **THEN** 服务端先持久化 `command.uncertain` 再向客户端广播，MUST NOT 自动重新派发该命令

#### Scenario: 创建类命令受理时没有资源行

- **WHEN** 一条创建资源的 mutation 已被接受、但后端尚未返回结果
- **THEN** 本机只有该命令的幂等记录（资源标识为空），没有对应的资源行

#### Scenario: 创建类命令失败后不留资源行

- **WHEN** 创建资源的 mutation 因后端失败而终结
- **THEN** 本机没有该资源行，且该命令已有终态（不是只有受理记录）

### Requirement: 命令级结构化日志

自有资源的命令受理与终态 SHALL 保留原结构化日志，含主体、命令、资源标识及固定结果类别；日志 MUST NOT 含路径、凭据或正文。Access 转发远程操作 MUST NOT 将请求标识、资源标识、状态、结果引用或其摘要写入日志。连接信任与认证的安全日志 SHALL 仅含允许的节点连接身份及固定类别，不借诊断复制远程操作记录。

#### Scenario: 受理与终态各有一条
- **WHEN** 当前节点权威管理的资源命令被接受并终结
- **THEN** 原受理与终态日志分别保留，含主体与命令名；若仅代理远程命令则不持久记录其请求、资源和结果

#### Scenario: 失败成因可读且不含敏感内容
- **WHEN** 业务命令失败或进入不确定
- **THEN** 调用方得到结构化原因，自有权威日志不含敏感内容，Access 不将远程错误连同请求/资源/状态写入日志

### Requirement: 命令幂等与冲突判定

同一设备内 `requestId` SHALL 唯一，幂等键 SHALL 为 `(deviceId, requestId)`。重复提交同一请求 MUST 返回首次的已知接受或终态结果，MUST NOT 造成第二次派发或第二个业务副作用。重复请求的 command、sessionId、`expectedVersion` 与经明确 DTO 解码后的 payload 语义 MUST 与首次一致，否则 MUST 返回 `command.idempotency_conflict`；判定 MUST NOT 依赖 JSON 对象键顺序。响应丢失后的重试 MUST 由客户端复用同一 `requestId`，服务端 MUST 按同一键处理。

#### Scenario: 相同内容重发返回首次结果

- **WHEN** 客户端用一个已提交过的 `requestId` 重发内容完全相同的命令
- **THEN** 服务端返回首次的已知结果，MUST NOT 再次派发、MUST NOT 产生第二个副作用

#### Scenario: 内容不一致判为冲突

- **WHEN** 同一 `requestId` 重发时 command、sessionId、`expectedVersion` 或解码后的 payload 语义与首次不同
- **THEN** 服务端返回 `command.idempotency_conflict`，且不改变首次已产生的任何状态

#### Scenario: mutation 幂等记录不被提前清理

- **WHEN** 某 mutation 的幂等记录在设备仍可能重试期间被清理，客户端随后以同一 `requestId` 重发
- **THEN** 该情况 MUST 被避免：幂等记录 SHALL 至少保留到设备被撤销且目标会话被删除，MUST NOT 把旧请求当作新命令执行

### Requirement: 命令以已认证设备为前提

服务端 SHALL 以连接通过设备认证且未撤销为执行业务命令或查看控制的前提，设备方向不增加命令级 scope 授权。未认证连接 MUST NOT 启动 Agent 或产生会话副作用。`session.resume` SHALL 仅在协商本机生命周期能力后对 owned 会话开放；未协商时保持 `command.unsupported`。本变更 MUST NOT 扩展 Node Link 导入会话的恢复或保活行为。

#### Scenario: 未认证连接不执行命令
- **WHEN** 尚未完成设备认证的连接派发业务命令或查看控制
- **THEN** 以认证类失败拒绝，不启动 Agent、不写事件、不改变会话状态

#### Scenario: 拒绝先于本机读取
- **WHEN** 未认证或已撤销设备对不存在会话派发命令
- **THEN** 仍返回认证类失败，不通过错误类型泄露会话存在性

#### Scenario: Sync 面不接受仅属其它传输面的命令
- **WHEN** 未协商本机生命周期的客户端在 Sync 面派发 `session.resume`
- **THEN** 返回 `command.unsupported`，不在本机执行

#### Scenario: 撤销后连接不再可用
- **WHEN** 设备被撤销后又派发命令或延续查看状态
- **THEN** 操作被拒，查看保活失效，不因连接仍在而继续可用

### Requirement: 乐观并发控制的版本冲突

对要求乐观并发控制的 mutation（模式切换与配置设置），请求 MUST 携带 `expectedVersion`；服务端 SHALL 在版本与当前不一致时以 `state.version_conflict` 拒绝，并在 `details` 中给出 `currentVersion`（存在 `expectedVersion` 时一并回显）。冲突 MUST NOT 产生业务副作用。

#### Scenario: 版本一致时成功

- **WHEN** 客户端以当前会话版本派发模式切换
- **THEN** 命令被接受并最终以 `command.completed` 终结，会话版本随之推进

#### Scenario: 版本过期时拒绝并给出当前值

- **WHEN** 客户端以过期的 `expectedVersion` 派发配置设置
- **THEN** 服务端返回 `state.version_conflict`，`details.currentVersion` 为当前真实版本，且 MUST NOT 改变会话状态

### Requirement: 命令速率限流与超限关闭

服务端 SHALL 按设备标识与命令族分别统计命令速率，默认上限为每分钟 120 次。超限 MUST 返回 `resource.rate_limited`（可重试，存在时可给出 `details.retryAfterMs`），并 SHALL 在连续超限时以 `4429` 关闭连接。客户端 SHALL 遵守给出的退避提示，MUST NOT 立即重发。限流计数 MUST NOT 以网络来源身份替代设备授权，也 MUST NOT 因不可信来源的转发头而失真。

#### Scenario: 超限返回可重试错误

- **WHEN** 同一设备在统计窗口内超过命令速率上限
- **THEN** 服务端返回 `resource.rate_limited` 且标记为可重试，MUST NOT 执行被拒的命令

#### Scenario: 连续超限关闭连接

- **WHEN** 同一连接连续多次触发限流
- **THEN** 服务端以 `4429` 关闭该连接，被拒命令 MUST NOT 产生任何副作用

### Requirement: 终态观察者随连接生存

被接受 mutation 的终态 SHALL 推送给提交该命令的连接；该观察者 MUST 有明确所有者，并在连接断开时取消，MUST NOT 遗留 detached 任务。终态推送 MUST 与持久化终态事件同源，MUST NOT 由适配层重新推导业务结果。

#### Scenario: 提交方收到自己的终态

- **WHEN** 某连接提交的 mutation 完成后该连接仍然在线
- **THEN** 该连接收到对应终态事件或结果

#### Scenario: 连接断开后观察者被取消

- **WHEN** 提交 mutation 的连接在终态产生前断开
- **THEN** 对应的观察者被取消、不遗留任务，且客户端重连后可用 `command.status` 查询该请求的终态

### Requirement: 远程命令持久去重归属 Owner

本节点自有命令 SHALL 保留既有持久幂等和终态保证。远程命令的同一保证 SHALL 由 Owner 承担，Access 只转发当前在线请求及观察终态，MUST NOT 创建持久代理命令行。相同请求标识不得在重新连接后再次造成副作用；标识或参数冲突 SHALL 由 Owner 按原合同拒绝，不因取消 Access 记录而放宽。

#### Scenario: Access 重启后重复提交原请求
- **WHEN** 调用方携带原稳定标识向原 Owner 查询或重试
- **THEN** Owner 返回原结果或明确冲突，不产生第二次执行，Access 本地无请求记录也不推断未执行

### Requirement: 远程状态查询携带显式来源

协商无持久中转能力后，远程 command.status SHALL 携带原请求标识和显式目标来源；Access SHALL 校验当前连接与来源可访问性后回源，不从本地请求表定位。尚未创建成功的会话可只定位 Owner，其余操作 SHALL 校验对应来源会话。来源不可达、原请求不存在与结果不确定 MUST 区分，未协商的新字段不得发送给旧端。

#### Scenario: 创建会话结果未知
- **WHEN** 远程创建已提交但调用方尚未得到新会话标识
- **THEN** 可用原请求和 Owner 定位查询，不要求以本地命令索引补出会话

#### Scenario: 来源声明不匹配
- **WHEN** 调用方为已知请求给出错误 Owner 或不属于该请求的会话
- **THEN** 拒绝查询或明确不存在，不返回其他来源或主体的结果

### Requirement: 本机激活采用可追踪的恢复命令

协商后的本机激活 SHALL 使用已登记的恢复命令，复用原身份，遵守稳定请求标识、先受理后持久终态及结果不确定不重派的合同。重复请求 SHALL 返回原结果，不同设备的并发恢复 SHALL 合并同会话建立过程。恢复失败 SHALL 提供准确的不支持、资源不可用、容量已满或不确定类别，MUST NOT 将这些情况误报为请求 schema 错误。

#### Scenario: 已有资源的重复激活
- **WHEN** 会话已激活且用户再次进入
- **THEN** 复用原资源，不再次创建或恢复 Agent 会话，不重新执行旧输入

#### Scenario: 响应丢失后重连
- **WHEN** 已受理恢复的响应丢失而客户端重新连接
- **THEN** 客户端先查询原请求结果，不以新标识盲目重派；查看状态按新连接重新声明

### Requirement: 查看控制有界且绑定连接

查看控制 SHALL 仅在协商成功且认证有效的连接内维持，有有限租期、明确的选中会话替换与离开语义，并限制数量、大小及频率。普通网络心跳 MUST NOT 续租会话查看状态。断开或撤销 SHALL 立即释放连接持有的查看声明；过期声明及旧页面的迟到结果 MUST NOT 恢复已离开的查看状态。

#### Scenario: 只发送网络心跳
- **WHEN** 页面隐藏后连接仍发送网络层心跳，但不再发送会话查看续租
- **THEN** 查看声明到期，不以连接在线为由无限保活该会话

#### Scenario: 隐藏后旧恢复完成
- **WHEN** 客户端已声明离开，之前恢复命令随后完成
- **THEN** 恢复收口但不重新添加查看者，无其他保护条件时开始完整保留期

### Requirement: 协议扩展不污染旧 peer 或远程来源

新增命令可用性、查看控制与生命周期展示 SHALL 由同一显式能力协商，缺少能力时明确提示不支持并保持既有合同，不发送未知字段或猜测支持。新旧客户端共享历史事件时 SHALL 获取适合协商能力的视图，保留原 ACP 内容。对远程来源 SHALL 沿用既有语义，MUST NOT 因本机上限而终结远程任务。

#### Scenario: 新客户端连接旧服务端
- **WHEN** 服务端未协商本机生命周期能力
- **THEN** 客户端明确显示自动激活不可用，不发送恢复或查看控制，不声称已有会话已激活
