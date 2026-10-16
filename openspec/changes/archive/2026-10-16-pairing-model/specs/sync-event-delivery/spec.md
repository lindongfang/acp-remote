# Spec Delta

## ADDED Requirements

### Requirement: 增量重放与实时投递的衔接

cursor 有效且仍在保留窗口内时，服务端 SHALL 从该 cursor 之后读取事件，按全局序号升序投递 `event`，随后发送 `sync.caught_up` 携带屏障游标。重放期间到达的实时事件 MUST 排在屏障之后投递，MUST NOT 越过序号更早的已持久化事件。设备方向不再有按授权的可见性过滤，因此投递集合是「该 cursor 之后的全部已持久化事件」；仍可能出现的序号空洞只来自保留策略（已压缩或已清理的事件），客户端 MUST NOT 因此报错，并可对 `sync.caught_up` 的屏障游标发 ACK。

#### Scenario: 从游标续传

- **WHEN** 客户端以有效 cursor 订阅，且该 cursor 之后存在可读事件
- **THEN** 服务端按序号升序投递这些事件，随后发送 `sync.caught_up`，其后继续实时投递新事件

#### Scenario: 重放期间的实时事件不越过屏障

- **WHEN** 重放进行中，新的实时事件被提交并发布
- **THEN** 该事件在 `sync.caught_up` 之后才投递，MUST NOT 出现在序号早于它的重放事件之前

#### Scenario: 保留策略造成的空洞不报错

- **WHEN** 某段全局序号在保留窗口内但因保留策略已压缩或清理而不可读
- **THEN** 客户端不因这段空洞报错，按屏障游标继续，并可对其发 ACK

### Requirement: 会话清单范围

快照 `sessions` 资源 SHALL 同时覆盖本节点自有会话与已导入会话：已导入会话 SHALL 受 `resource.remote-origin.v1` 门控（未协商时 MUST NOT 出现在快照中，其事件也 MUST NOT 带非空来源信息）。每个会话条目的 `origin` 字段 SHALL 区分本节点自有与来自其它节点；来自其它节点的条目 SHALL 携带其来源节点标识、来源导出标识、来源事件库 epoch 与可达性标记。设备方向不再有按授权的可见性过滤：任何已认证设备看到本节点全部会话；进入结果的过滤只由 feature 协商决定，且 MUST 作为服务端判定执行，MUST NOT 依赖客户端的筛选。

#### Scenario: 两类来源在同一快照中共存

- **WHEN** 本节点既有一个自有会话、又通过 Import 持有另一个节点的会话，且设备协商了 `resource.remote-origin.v1`
- **THEN** 快照的 `sessions` chunk 同时含两条，自有条目的来源标记为本节点，另一条的来源标记为其它节点并携带来源标识与可达性

#### Scenario: 未协商来源 feature 时过滤外来会话

- **WHEN** 设备未协商 `resource.remote-origin.v1`，本节点存在已导入会话
- **THEN** 快照中不出现任何来源为其它节点的会话，MUST NOT 出现空的来源标记或占位条目

#### Scenario: 已认证设备看到全部自有会话

- **WHEN** 一台已认证设备协商了来源 feature，本节点存在多个自有会话
- **THEN** 这些会话全部出现在快照中，不因设备维度而被过滤

## REMOVED Requirements

### Requirement: 增量重放与实时投递的无缝衔接

**Reason**：设备配对不再授予授权，原文「投递 MUST 只包含该设备 scope 允许可见的事件；因权限过滤造成的序号空洞 SHALL 是合法的」这一前提消失；重放排序与屏障语义本身继续有效，已并入本 delta 的新增要求。

**Migration**：见新增要求「增量重放与实时投递的衔接」。

### Requirement: 设备可见的会话清单范围

**Reason**：原文「设备无权读取的会话与字段 MUST NOT 进入结果」的前提消失（设备不再有授权维度）；来源门控与 `origin` 形状继续有效，已并入本 delta 的新增要求。

**Migration**：见新增要求「会话清单范围」。
