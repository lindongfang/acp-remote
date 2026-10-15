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

#### Scenario: 派发后先收到接受

- **WHEN** 客户端派发一条 `session.prompt` 且授权通过
- **THEN** 首次响应为 `accepted`，`result` 与 `terminalEventId` 为空，MUST NOT 在同一条响应里给出业务结果

#### Scenario: 终态经持久化事件表达

- **WHEN** 被接受的 prompt 对应的轮次正常结束
- **THEN** 该请求的终态以一条 `command.completed` 事件下发，`terminalEventId` 指向它，且该事件与领域状态变化在同一事务内提交

#### Scenario: 崩溃窗口进入结果不确定

- **WHEN** Daemon 恢复时发现某个已接受的 mutation 可能已到达外部 Agent，但无法证明其结果
- **THEN** 服务端先持久化 `command.uncertain` 再向客户端广播，MUST NOT 自动重新派发该命令

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

### Requirement: 逐命令的 scope 授权与零副作用拒绝

服务端 SHALL 对每条业务命令在当前连接身份下重新按设备 scope 判定授权，MUST NOT 依赖客户端 UI 的可用性判定，也 MUST NOT 依赖配对时的资源快照。授权 MUST 发生在读取会话行、触碰文件系统或启动进程之前；未授权时 MUST 返回 `authorization.scope_denied` 且 MUST NOT 产生任何副作用，也 MUST NOT 因会话是否存在、是否为 imported 而改变判定结果或错误类型。`session.resume` 不属于 Sync 命令枚举，收到时 MUST 返回 `command.unsupported`。scope 缩减 SHALL 立即作用于后续命令。

#### Scenario: 无权限设备的 mutation 被拒且无副作用

- **WHEN** 一个未持 `session.prompt` scope 的设备派发 `session.prompt`
- **THEN** 服务端返回 `authorization.scope_denied`，MUST NOT 启动 Agent、MUST NOT 写入事件、MUST NOT 改变会话状态

#### Scenario: 授权先于本机读取

- **WHEN** 未授权设备对某个不存在的会话派发命令
- **THEN** 返回的仍是授权拒绝，MUST NOT 通过错误类型泄露该会话是否存在

#### Scenario: Sync 面不接受仅属其它传输面的命令

- **WHEN** 客户端在 Sync 面派发 `session.resume`
- **THEN** 服务端返回 `command.unsupported`，MUST NOT 在本机执行该操作

#### Scenario: 缩减 scope 立即生效

- **WHEN** 某设备的 scope 被缩减后，该设备用一条新请求派发已失去 scope 的命令
- **THEN** 该命令被拒，MUST NOT 因连接仍在而继续可用

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
